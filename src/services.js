/**
 * Service process manager.
 *
 * Services are manually registered via service-registry.js — no auto-discovery.
 * This module owns the live-process side: spawning, stopping, log buffering,
 * and memory sampling. The registry is the source of truth for what exists.
 */

import { spawn, exec } from 'child_process';
import { promisify } from 'util';
import { IS_WIN } from './platform.js';
import {
  listAllRegistered,
  getRegistered,
  deleteRegistered,
} from './service-registry.js';
import {
  startTunnel,
  stopTunnel,
  getTunnelInfo,
  getAllTunnelInfo,
  stopAllTunnels,
} from './tunnel.js';

const execP = promisify(exec);

// key -> ServiceEntry (live process state). Registry holds persistent config.
const running = new Map();

// pid -> bytes, populated by sampleAllMemory() roughly every 3s while services run
const pidMemoryBytes = new Map();
let memorySamplerTimer = null;

/**
 * @typedef {Object} ServiceEntry
 * @property {string} key
 * @property {string} alias
 * @property {string} name
 * @property {string} projectPath         — working directory
 * @property {string} command             — raw command string
 * @property {string} stopCommand         — optional custom stop command (empty string = use taskkill)
 * @property {import('child_process').ChildProcess} proc
 * @property {string[]} logs              — last 200 lines
 * @property {Date} startedAt
 * @property {'running'|'stopped'|'crashed'} status
 * @property {number|null} exitCode
 */

function scrubbedEnv() {
  const e = { ...process.env };
  // Strip bot-only vars so the child service uses its own .env
  const strip = [
    'BOT_TOKEN', 'TELEGRAM_BOT_TOKEN', 'ALLOWED_USERNAME',
    'ADMIN_USER_ID', 'FORUM_CHAT_ID',
    'ANTHROPIC_API_KEY', 'CLAUDE_MODEL', 'DEFAULT_MODE',
    'CLAUDE_TIMEOUT', 'PROJECTS_DIR', 'DATA_DIR',
    'CLAUDE_PROJECTS_DIR', 'DOTENV_PATH',
  ];
  for (const k of strip) delete e[k];
  return e;
}

/**
 * Start a registered service by key.
 * @param {string} key — the registry key, e.g. "myproj:web"
 */
export function startService(key) {
  const reg = getRegistered(key);
  if (!reg) return { ok: false, error: `No service registered for "${key}"` };

  const existing = running.get(key);
  if (existing && existing.status === 'running') {
    return { ok: false, error: `${reg.name} is already running` };
  }
  running.delete(key);

  const logs = [];
  let proc;
  try {
    proc = spawn(reg.command, [], {
      cwd: reg.cwd,
      env: scrubbedEnv(),
      windowsHide: true,
      shell: true,
      // On POSIX, detach so the shell leads its own process group; that lets us
      // reap the whole tree with a negative-PID signal (the taskkill /T equivalent).
      detached: !IS_WIN,
    });
  } catch (err) {
    return { ok: false, error: `Failed to spawn: ${err.message}` };
  }

  /** @type {ServiceEntry} */
  const entry = {
    key,
    alias: reg.alias,
    name: reg.name,
    projectPath: reg.cwd,
    command: reg.command,
    stopCommand: reg.stopCommand || '',
    proc,
    logs,
    startedAt: new Date(),
    status: 'running',
    exitCode: null,
  };

  const appendLog = (line) => {
    logs.push(line);
    if (logs.length > 200) logs.shift();
  };

  proc.stdout?.on('data', (d) => String(d).split('\n').filter(Boolean).forEach(appendLog));
  proc.stderr?.on('data', (d) => String(d).split('\n').filter(Boolean).forEach(l => appendLog(`[err] ${l}`)));

  proc.on('exit', (code) => {
    entry.status = code === 0 ? 'stopped' : 'crashed';
    entry.exitCode = code;
    appendLog(`--- process exited with code ${code} ---`);
    if (entry.proc?.pid != null) pidMemoryBytes.delete(entry.proc.pid);
    stopMemorySamplerIfIdle();
    emitServiceUpdate();
  });

  proc.on('error', (err) => {
    entry.status = 'crashed';
    appendLog(`--- spawn error: ${err.message} ---`);
    emitServiceUpdate();
  });

  running.set(key, entry);
  startMemorySampler();
  emitServiceUpdate();

  // Auto-start tunnel only if it's enabled AND a port is set. tunnelEnabled is
  // separate from the port; legacy registrations (no flag) default to on.
  const tunnelOn = reg.tunnelEnabled !== undefined ? reg.tunnelEnabled : (reg.tunnelPort > 0);
  if (tunnelOn && reg.tunnelPort > 0) {
    startTunnel(key, reg.tunnelPort);
  }

  return { ok: true, key, name: reg.name, command: reg.command };
}

/**
 * Force-kill a process and its children. On Windows this is `taskkill /T`; on
 * POSIX the service shell was spawned detached (its own group), so a negative-PID
 * SIGKILL reaps the whole tree. Falls back to a plain kill if the group send fails.
 */
function killTree(pid) {
  if (pid == null) return;
  if (IS_WIN) {
    spawn('taskkill', ['/F', '/T', '/PID', String(pid)], { shell: true, windowsHide: true });
  } else {
    try { process.kill(-pid, 'SIGKILL'); }
    catch { try { process.kill(pid, 'SIGKILL'); } catch { /* already gone */ } }
  }
}

/**
 * Stop a running service. Uses stopCommand if set, otherwise kills the process tree.
 */
export function stopService(key) {
  const entry = running.get(key);
  if (!entry) return { ok: false, error: `Not running` };
  if (entry.status !== 'running') return { ok: false, error: `Not running (${entry.status})` };

  // Kill tunnel first (if any)
  stopTunnel(key);

  try {
    if (entry.stopCommand) {
      spawn(entry.stopCommand, [], {
        cwd: entry.projectPath,
        shell: true,
        windowsHide: true,
      });
    } else {
      killTree(entry.proc?.pid);
    }
    entry.status = 'stopped';
    entry.exitCode = null;
    emitServiceUpdate();
    return { ok: true };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export function restartService(key) {
  const reg = getRegistered(key);
  if (!reg) return { ok: false, error: `Not registered` };
  const entry = running.get(key);
  if (entry && entry.status === 'running') stopService(key);
  running.delete(key);
  return startService(key);
}

/**
 * Delete a registered service. Refuses while still running.
 */
export function deleteService(key) {
  const entry = running.get(key);
  if (entry && entry.status === 'running') {
    return { ok: false, error: 'Service is running — stop it before deleting' };
  }
  running.delete(key);
  const res = deleteRegistered(key);
  if (res.ok) emitServiceUpdate();
  return res;
}

/**
 * Drop a stopped/crashed entry from the live-state map (doesn't touch registry).
 */
export function removeService(key) {
  const entry = running.get(key);
  if (entry && entry.status === 'running') return false;
  running.delete(key);
  emitServiceUpdate();
  return true;
}

export function listServices() {
  return Array.from(running.values());
}

export function getService(key) {
  return running.get(key) || null;
}

export function getServicesForProject(alias) {
  const a = String(alias).toLowerCase();
  const out = [];
  for (const [key, entry] of running) {
    if (entry.alias === a) out.push({ key, ...entry });
  }
  return out;
}

export function getServiceLogs(key, lines = 30) {
  const entry = running.get(key);
  if (!entry) return null;
  return entry.logs.slice(-lines);
}

/**
 * Kill all running services (called on bot shutdown).
 * Awaits each stop command so `docker compose down` etc. actually complete
 * before the bot process exits.
 */
export async function killAllServices() {
  const jobs = [];
  for (const [, entry] of running) {
    if (entry.status !== 'running') continue;

    // Default stop (no custom command) on POSIX: a negative-PID SIGKILL is
    // synchronous, so reap the tree inline rather than awaiting a child process.
    if (!entry.stopCommand && !IS_WIN) { killTree(entry.proc?.pid); continue; }

    const spec = entry.stopCommand
      ? { cmd: entry.stopCommand, args: [], cwd: entry.projectPath, timeoutMs: 20000 }
      : { cmd: 'taskkill', args: ['/F', '/T', '/PID', String(entry.proc.pid)], cwd: undefined, timeoutMs: 5000 };

    jobs.push(new Promise((resolve) => {
      let child;
      try {
        child = spawn(spec.cmd, spec.args, {
          cwd: spec.cwd,
          shell: true,
          windowsHide: true,
          stdio: 'ignore',
        });
      } catch { resolve(); return; }

      const timer = setTimeout(() => {
        try { child.kill(); } catch { /* ignore */ }
        resolve();
      }, spec.timeoutMs);
      child.once('exit', () => { clearTimeout(timer); resolve(); });
      child.once('error', () => { clearTimeout(timer); resolve(); });
    }));
  }

  if (jobs.length) await Promise.all(jobs);
  stopAllTunnels();
  running.clear();
  if (memorySamplerTimer) { clearInterval(memorySamplerTimer); memorySamplerTimer = null; }
  pidMemoryBytes.clear();
  emitServiceUpdate();
}

/**
 * Serializable status for every registered service, merged with live state.
 * Registered-but-idle services appear with status='stopped' so the UI and bot
 * can always show the full list regardless of whether anything's running.
 */
export function getAllServiceStatus() {
  const seen = new Set();
  const out = [];
  const tunnelMap = getAllTunnelInfo();

  for (const reg of listAllRegistered()) {
    seen.add(reg.key);
    const entry = running.get(reg.key);
    const tunnel = tunnelMap[reg.key] || null;
    const tunnelEnabled = reg.tunnelEnabled !== undefined ? reg.tunnelEnabled : (reg.tunnelPort > 0);
    if (entry) {
      out.push({ ...toStatus(entry, true), tunnelPort: reg.tunnelPort || 0, tunnelEnabled, tunnel });
    } else {
      out.push({
        key: reg.key,
        alias: reg.alias,
        name: reg.name,
        command: reg.command,
        projectPath: reg.cwd,
        stopCommand: reg.stopCommand || '',
        status: 'stopped',
        exitCode: null,
        startedAt: null,
        logCount: 0,
        pid: null,
        memoryBytes: null,
        registered: true,
        tunnelPort: reg.tunnelPort || 0,
        tunnelEnabled,
        tunnel,
      });
    }
  }

  // Live entries whose registration has been deleted — still show them so the
  // user can stop them; they'll disappear once stopped and removed.
  for (const [key, entry] of running) {
    if (!seen.has(key)) {
      const tunnel = tunnelMap[key] || null;
      out.push({ ...toStatus(entry, false), tunnelPort: 0, tunnelEnabled: false, tunnel });
    }
  }
  return out;
}

function toStatus(entry, registered) {
  return {
    key: entry.key,
    alias: entry.alias,
    name: entry.name,
    command: entry.command,
    projectPath: entry.projectPath,
    stopCommand: entry.stopCommand || '',
    status: entry.status,
    exitCode: entry.exitCode,
    startedAt: entry.startedAt?.toISOString() || null,
    logCount: entry.logs.length,
    pid: entry.proc?.pid ?? null,
    memoryBytes: entry.proc?.pid != null ? (pidMemoryBytes.get(entry.proc.pid) ?? null) : null,
    registered,
  };
}

// ─── Memory sampler ───

async function sampleAllMemory() {
  const pids = [];
  for (const entry of running.values()) {
    if (entry.status === 'running' && entry.proc?.pid != null) pids.push(entry.proc.pid);
  }
  if (pids.length === 0) { pidMemoryBytes.clear(); return; }

  let procs;
  if (IS_WIN) {
    try {
      const { stdout } = await execP(
        'wmic process get ProcessId,ParentProcessId,WorkingSetSize /format:csv',
        { windowsHide: true, maxBuffer: 8 * 1024 * 1024 },
      );
      procs = parseWmicCsv(stdout);
    } catch {
      try {
        const { stdout } = await execP(
          'powershell -NoProfile -Command "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,WorkingSetSize | ConvertTo-Csv -NoTypeInformation"',
          { windowsHide: true, maxBuffer: 8 * 1024 * 1024 },
        );
        procs = parsePsCsv(stdout);
      } catch {
        return;
      }
    }
  } else {
    // POSIX: `ps` reports RSS in kilobytes; convert to bytes for parity with wmic.
    try {
      const { stdout } = await execP('ps -Ao pid=,ppid=,rss=', { maxBuffer: 8 * 1024 * 1024 });
      procs = parsePsUnix(stdout);
    } catch {
      return;
    }
  }

  const children = new Map();
  for (const p of procs) {
    if (!children.has(p.ppid)) children.set(p.ppid, []);
    children.get(p.ppid).push(p.pid);
  }
  const memByPid = new Map(procs.map(p => [p.pid, p.mem]));

  const freshMem = new Map();
  for (const rootPid of pids) {
    let total = 0;
    const stack = [rootPid];
    const visited = new Set();
    while (stack.length) {
      const pid = stack.pop();
      if (visited.has(pid)) continue;
      visited.add(pid);
      total += memByPid.get(pid) || 0;
      const kids = children.get(pid);
      if (kids) stack.push(...kids);
    }
    freshMem.set(rootPid, total);
  }
  pidMemoryBytes.clear();
  for (const [k, v] of freshMem) pidMemoryBytes.set(k, v);
  emitServiceUpdate();
}

function parseWmicCsv(stdout) {
  const lines = stdout.split(/\r?\n/).map(l => l.trim()).filter(Boolean);
  const out = [];
  for (const line of lines) {
    if (line.startsWith('Node,')) continue;
    const parts = line.split(',');
    if (parts.length < 4) continue;
    const ppid = parseInt(parts[1], 10);
    const pid = parseInt(parts[2], 10);
    const mem = parseInt(parts[3], 10);
    if (!Number.isFinite(pid)) continue;
    out.push({ pid, ppid: Number.isFinite(ppid) ? ppid : 0, mem: Number.isFinite(mem) ? mem : 0 });
  }
  return out;
}

// POSIX `ps -Ao pid=,ppid=,rss=` → whitespace-separated pid/ppid/rss(KB).
function parsePsUnix(stdout) {
  const lines = stdout.split(/\r?\n/).map(l => l.trim()).filter(Boolean);
  const out = [];
  for (const line of lines) {
    const parts = line.split(/\s+/);
    if (parts.length < 3) continue;
    const pid = parseInt(parts[0], 10);
    const ppid = parseInt(parts[1], 10);
    const rssKb = parseInt(parts[2], 10);
    if (!Number.isFinite(pid)) continue;
    out.push({ pid, ppid: Number.isFinite(ppid) ? ppid : 0, mem: Number.isFinite(rssKb) ? rssKb * 1024 : 0 });
  }
  return out;
}

function parsePsCsv(stdout) {
  const lines = stdout.split(/\r?\n/).map(l => l.trim()).filter(Boolean);
  const out = [];
  let headerSeen = false;
  for (const line of lines) {
    if (!headerSeen) { headerSeen = true; continue; }
    const parts = line.split(',').map(s => s.replace(/^"|"$/g, ''));
    if (parts.length < 3) continue;
    const pid = parseInt(parts[0], 10);
    const ppid = parseInt(parts[1], 10);
    const mem = parseInt(parts[2], 10);
    if (!Number.isFinite(pid)) continue;
    out.push({ pid, ppid: Number.isFinite(ppid) ? ppid : 0, mem: Number.isFinite(mem) ? mem : 0 });
  }
  return out;
}

function startMemorySampler() {
  if (memorySamplerTimer) return;
  sampleAllMemory().catch(() => {});
  memorySamplerTimer = setInterval(() => { sampleAllMemory().catch(() => {}); }, 3000);
}

function stopMemorySamplerIfIdle() {
  const anyRunning = Array.from(running.values()).some(e => e.status === 'running');
  if (!anyRunning && memorySamplerTimer) {
    clearInterval(memorySamplerTimer);
    memorySamplerTimer = null;
    pidMemoryBytes.clear();
  }
}

// Exported so callers that mutate service state *outside* of this module
// (e.g. the MCP register_service tool calling service-registry directly)
// can trigger a parent-process refresh. Internal callers in this file also
// use it; keep it in one place.
export function emitServiceUpdate() {
  if (typeof process.send !== 'function') return;
  try {
    process.send({ type: 'services', data: getAllServiceStatus() });
  } catch { /* channel closed */ }
}
