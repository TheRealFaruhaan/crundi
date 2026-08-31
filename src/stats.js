/**
 * Machine and process statistics.
 *
 * Two jobs, deliberately kept in one place because they share a sampler and a
 * ring buffer: what the whole box is doing (Info tab), and what each service is
 * costing (Services tab).
 *
 * The per-process side used to live in services.js and was Windows-only — it
 * shelled out to `wmic`, so on Linux every sample threw, was swallowed, and
 * every service reported `memoryBytes: null` forever. Linux now reads /proc
 * directly (no subprocess per sample), and the Windows path is kept intact.
 *
 * CPU is a RATE, so it only exists relative to a previous sample. Everything
 * here is delta-based against the last tick and reports null until a second
 * sample exists, rather than inventing a number for the first paint.
 */

import os from 'os';
import { exec } from 'child_process';
import { promisify } from 'util';
import { readFile, readdir, statfs } from 'fs/promises';

const execP = promisify(exec);
const isLinux = process.platform === 'linux';
const isWindows = process.platform === 'win32';

// Linux reports process CPU in clock ticks. 100/s on every platform Node runs
// on, but read it rather than assume it.
let CLK_TCK = 100;

/** Samples kept for the sparklines: 90 x 2s ≈ 3 minutes of history. */
export const HISTORY_MAX = 90;

// ─── Small helpers ───

const num = (v) => (Number.isFinite(v) ? v : 0);

/** Push onto a ring buffer, dropping from the front once it is full. */
export function pushHistory(arr, value) {
  arr.push(value);
  if (arr.length > HISTORY_MAX) arr.splice(0, arr.length - HISTORY_MAX);
  return arr;
}

// ─── Per-process sampling ───

/**
 * One entry per process on the box: { pid, ppid, rss, ticks }.
 * `ticks` is cumulative CPU time in clock ticks (Linux) or in the same unit
 * consistently on Windows — only differences between samples are ever used.
 */
async function readAllProcesses() {
  if (isLinux) return readProcfs();
  if (isWindows) return readWindowsProcesses();
  return null;
}

async function readProcfs() {
  let names;
  try { names = await readdir('/proc'); } catch { return null; }
  const out = [];
  await Promise.all(names.map(async (name) => {
    // /proc holds far more than pids (self, net, meminfo, ...); only all-digit
    // names are processes.
    if (name.charCodeAt(0) < 48 || name.charCodeAt(0) > 57) return;
    const pid = Number(name);
    if (!Number.isInteger(pid)) return;
    let raw;
    // A process can exit between readdir and read; that is normal, not an error.
    try { raw = await readFile('/proc/' + name + '/stat', 'utf8'); } catch { return; }
    // comm is parenthesised and may itself contain spaces or ')', so the only
    // safe split point is the LAST ')'.
    const close = raw.lastIndexOf(')');
    if (close < 0) return;
    const f = raw.slice(close + 2).split(' ');
    // f[0] is field 3 (state), so field N is at f[N - 3].
    const ppid = parseInt(f[1], 10);
    const utime = parseInt(f[11], 10);
    const stime = parseInt(f[12], 10);
    const rssPages = parseInt(f[21], 10);
    if (!Number.isFinite(rssPages)) return;
    out.push({
      pid,
      ppid: Number.isFinite(ppid) ? ppid : 0,
      rss: rssPages * 4096,
      ticks: num(utime) + num(stime),
    });
  }));
  return out;
}

// UserModeTime/KernelModeTime are cumulative 100ns units — a different scale
// from Linux ticks, which is fine because only deltas are used, normalised
// by the same CLK_TCK below so the rate maths is identical on both.
//
// Columns are located BY NAME from the header row, never by position:
// `wmic /format:csv` returns them in alphabetical order rather than the order
// asked for, so adding the two time columns to the old query silently shifts
// ProcessId out from under a fixed index. PowerShell keeps the requested
// order, which is a second reason not to hardcode either layout.
export function parseWindowsProcessCsv(stdout) {
  const lines = stdout.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
  const cells = (line) => line.split(',').map((s) => s.replace(/^"|"$/g, '').trim());
  let col = null;
  const out = [];
  for (const line of lines) {
    const p = cells(line);
    if (!col) {
      const idx = {};
      p.forEach((name, i) => { idx[name.toLowerCase()] = i; });
      if (idx.processid == null) continue;   // preamble/blank line, keep looking
      col = {
        pid: idx.processid,
        ppid: idx.parentprocessid,
        rss: idx.workingsetsize,
        kt: idx.kernelmodetime,
        ut: idx.usermodetime,
      };
      continue;
    }
    const pid = parseInt(p[col.pid], 10);
    if (!Number.isFinite(pid)) continue;
    const ppid = parseInt(p[col.ppid], 10);
    const rss = parseInt(p[col.rss], 10);
    const kt = parseInt(p[col.kt], 10);
    const ut = parseInt(p[col.ut], 10);
    out.push({
      pid,
      ppid: Number.isFinite(ppid) ? ppid : 0,
      rss: num(rss),
      // 100ns units -> the same "ticks per second" scale used for Linux.
      ticks: (num(kt) + num(ut)) / 1e7 * CLK_TCK,
    });
  }
  return out.length ? out : null;
}

async function readWindowsProcesses() {
  // wmic is gone in recent Windows 11 builds, so the PowerShell path is a real
  // fallback rather than a formality — and it runs whenever wmic yields nothing
  // usable, not only when it fails outright.
  try {
    const { stdout } = await execP(
      'wmic process get ParentProcessId,ProcessId,WorkingSetSize,KernelModeTime,UserModeTime /format:csv',
      { windowsHide: true, maxBuffer: 8 * 1024 * 1024 },
    );
    const rows = parseWindowsProcessCsv(stdout);
    if (rows) return rows;
  } catch { /* fall through */ }
  try {
    const { stdout } = await execP(
      'powershell -NoProfile -Command "Get-CimInstance Win32_Process | Select-Object ParentProcessId,ProcessId,WorkingSetSize,KernelModeTime,UserModeTime | ConvertTo-Csv -NoTypeInformation"',
      { windowsHide: true, maxBuffer: 8 * 1024 * 1024 },
    );
    return parseWindowsProcessCsv(stdout);
  } catch { return null; }
}

// rootPid -> { ticks, at } from the previous sample, for the CPU rate.
const lastTree = new Map();

/**
 * Total RSS and CPU% for each root pid, summed over its whole process tree.
 *
 * A service is usually a shell that spawns the real server, so charging it only
 * the root process would under-report it to the point of being useless.
 *
 * @param {number[]} rootPids
 * @returns {Promise<Map<number, {rssBytes: number, cpuPct: number|null}>>}
 */
export async function sampleProcessTrees(rootPids) {
  const result = new Map();
  if (!rootPids.length) { lastTree.clear(); return result; }

  const procs = await readAllProcesses();
  if (!procs) return result;

  const children = new Map();
  const byPid = new Map();
  for (const p of procs) {
    byPid.set(p.pid, p);
    let kids = children.get(p.ppid);
    if (!kids) { kids = []; children.set(p.ppid, kids); }
    kids.push(p.pid);
  }

  const now = Date.now();
  const live = new Set(rootPids);
  for (const [pid] of lastTree) if (!live.has(pid)) lastTree.delete(pid);

  for (const rootPid of rootPids) {
    let rss = 0;
    let ticks = 0;
    let found = false;
    const stack = [rootPid];
    const seen = new Set();
    while (stack.length) {
      const pid = stack.pop();
      if (seen.has(pid)) continue;      // a pid cycle would otherwise hang this
      seen.add(pid);
      const p = byPid.get(pid);
      if (p) { rss += p.rss; ticks += p.ticks; found = true; }
      const kids = children.get(pid);
      if (kids) for (const k of kids) stack.push(k);
    }
    if (!found) { lastTree.delete(rootPid); continue; }

    const prev = lastTree.get(rootPid);
    let cpuPct = null;
    if (prev) {
      const secs = (now - prev.at) / 1000;
      // Children exiting take their accumulated ticks out of the tree total, so
      // the difference can go negative. That is bookkeeping, not idleness, but
      // there is no honest rate to report for it — clamp to 0.
      if (secs > 0.05) cpuPct = Math.max(0, (ticks - prev.ticks) / CLK_TCK / secs * 100);
    }
    lastTree.set(rootPid, { ticks, at: now });
    result.set(rootPid, { rssBytes: rss, cpuPct });
  }
  return result;
}

// ─── Whole-machine sampling ───

let prevCpu = null;      // per-core cumulative times from os.cpus()
let prevNet = null;      // { rx, tx, at }
const history = { cpu: [], mem: [], at: [] };
let latest = null;
let timer = null;

/** Per-core busy percentage since the previous tick. */
function sampleCpu() {
  const cpus = os.cpus();
  const cur = cpus.map((c) => {
    const t = c.times;
    const idle = t.idle;
    const total = t.user + t.nice + t.sys + t.irq + t.idle;
    return { idle, total };
  });
  let cores = cur.map(() => null);
  let overall = null;
  if (prevCpu && prevCpu.length === cur.length) {
    let dIdleAll = 0;
    let dTotalAll = 0;
    cores = cur.map((c, i) => {
      const dIdle = c.idle - prevCpu[i].idle;
      const dTotal = c.total - prevCpu[i].total;
      dIdleAll += dIdle;
      dTotalAll += dTotal;
      if (dTotal <= 0) return null;
      return Math.min(100, Math.max(0, (1 - dIdle / dTotal) * 100));
    });
    if (dTotalAll > 0) overall = Math.min(100, Math.max(0, (1 - dIdleAll / dTotalAll) * 100));
  }
  prevCpu = cur;
  return { overall, cores };
}

/**
 * Memory. On Linux os.freemem() counts the page cache as used, which makes a
 * healthy box look nearly full; MemAvailable is what the kernel will actually
 * hand out, so it is the honest number to draw.
 */
async function sampleMemory() {
  const total = os.totalmem();
  let available = os.freemem();
  let swapTotal = 0;
  let swapUsed = 0;
  let cached = 0;
  if (isLinux) {
    try {
      const raw = await readFile('/proc/meminfo', 'utf8');
      const kb = {};
      for (const line of raw.split('\n')) {
        const m = /^(\w+):\s+(\d+)/.exec(line);
        if (m) kb[m[1]] = Number(m[2]) * 1024;
      }
      if (kb.MemAvailable != null) available = kb.MemAvailable;
      if (kb.Cached != null) cached = kb.Cached + num(kb.Buffers);
      if (kb.SwapTotal != null) {
        swapTotal = kb.SwapTotal;
        swapUsed = kb.SwapTotal - num(kb.SwapFree);
      }
    } catch { /* os.freemem() still gives a usable answer */ }
  }
  return { total, available, used: total - available, cached, swapTotal, swapUsed };
}

async function sampleDisk() {
  const path = isWindows ? process.cwd().slice(0, 3) : '/';
  try {
    const s = await statfs(path);
    const total = s.blocks * s.bsize;
    // bavail excludes the root-reserved blocks, so it is what a normal process
    // can really use. Report "used" against the same yardstick.
    const free = s.bavail * s.bsize;
    return { path, total, free, used: total - free };
  } catch {
    return { path, total: 0, free: 0, used: 0 };
  }
}

async function sampleNetwork() {
  if (!isLinux) return { rxPerSec: null, txPerSec: null };
  let raw;
  try { raw = await readFile('/proc/net/dev', 'utf8'); } catch { return { rxPerSec: null, txPerSec: null }; }
  let rx = 0;
  let tx = 0;
  for (const line of raw.split('\n')) {
    const idx = line.indexOf(':');
    if (idx < 0) continue;
    const iface = line.slice(0, idx).trim();
    if (iface === 'lo' || iface.startsWith('veth') || iface.startsWith('docker')) continue;
    const f = line.slice(idx + 1).trim().split(/\s+/);
    rx += num(parseInt(f[0], 10));
    tx += num(parseInt(f[8], 10));
  }
  const now = Date.now();
  let rxPerSec = null;
  let txPerSec = null;
  if (prevNet) {
    const secs = (now - prevNet.at) / 1000;
    // Counters are 64-bit but can reset when an interface goes away; a negative
    // delta means the baseline moved, not that traffic flowed backwards.
    if (secs > 0.05) {
      rxPerSec = Math.max(0, (rx - prevNet.rx) / secs);
      txPerSec = Math.max(0, (tx - prevNet.tx) / secs);
    }
  }
  prevNet = { rx, tx, at: now };
  return { rxPerSec, txPerSec };
}

/** Facts that cannot change while the process is alive — read once. */
let hostInfo = null;
function getHostInfo() {
  if (hostInfo) return hostInfo;
  const cpus = os.cpus();
  hostInfo = {
    hostname: os.hostname(),
    platform: process.platform,
    release: os.release(),
    arch: process.arch,
    cpuModel: (cpus[0]?.model || '').replace(/\s+/g, ' ').trim(),
    cores: cpus.length,
    nodeVersion: process.version,
  };
  return hostInfo;
}

async function tick() {
  const cpu = sampleCpu();
  const [mem, disk, net] = await Promise.all([sampleMemory(), sampleDisk(), sampleNetwork()]);
  // Windows has no load average; os.loadavg() returns [0,0,0] there, which
  // would render as a perfectly convincing "0.00 0.00 0.00". Report null and
  // let the UI omit the row entirely.
  const load = os.loadavg();
  const hasLoad = !isWindows;
  latest = {
    at: Date.now(),
    cpu,
    mem,
    disk,
    net,
    load: hasLoad ? { one: load[0], five: load[1], fifteen: load[2] } : null,
    uptimeSec: os.uptime(),
    processUptimeSec: process.uptime(),
    processRssBytes: process.memoryUsage().rss,
    host: getHostInfo(),
  };
  // Only real rates go into the history, so the sparkline never opens with a
  // fabricated zero from the very first tick.
  if (cpu.overall != null) {
    pushHistory(history.cpu, Math.round(cpu.overall * 10) / 10);
    pushHistory(history.mem, Math.round((mem.used / mem.total) * 1000) / 10);
    pushHistory(history.at, latest.at);
  }
  return latest;
}

export function startStatsSampler(intervalMs = 2000) {
  if (timer) return;
  if (isLinux) {
    // Read the real value rather than trusting the 100 that it always is.
    execP('getconf CLK_TCK').then(({ stdout }) => {
      const v = parseInt(String(stdout).trim(), 10);
      if (Number.isFinite(v) && v > 0) CLK_TCK = v;
    }).catch(() => { /* 100 is right on every platform Node supports */ });
  }
  tick().catch(() => {});
  timer = setInterval(() => { tick().catch(() => {}); }, intervalMs);
  timer.unref?.();
}

export function stopStatsSampler() {
  if (timer) { clearInterval(timer); timer = null; }
  history.cpu.length = 0;
  history.mem.length = 0;
  history.at.length = 0;
  latest = null;
  prevCpu = null;
  prevNet = null;
}

/** Latest snapshot plus the rolling history the sparklines draw. */
export async function getSystemStats() {
  const snap = latest || await tick();
  return { ...snap, history: { cpu: [...history.cpu], mem: [...history.mem] } };
}
