/**
 * docker.js — the Docker tab's view of the engine on this machine.
 *
 * Containers, Compose stacks, images, volumes, networks and the engine itself:
 * listing, the per-container detail (overview, logs, stats, inspect, a shell),
 * and the actions on each. Owner-only: the daemon is shared by every project on
 * the machine, and access to it is as good as root.
 *
 * Two ways in, each for what it is good at:
 *   - The Engine API over its socket, for everything that is a request and a
 *     reply. Structured JSON, no output to parse, and much faster than forking
 *     the CLI once per container.
 *   - The `docker` CLI for Compose (there is no Compose API) and for `exec` into
 *     a shell, which wants a real PTY that node-pty can give it.
 *
 * Security, because every target here arrives from the browser: nothing is
 * ever addressed by the string the client sent. Each request resolves that
 * string against the live listing (containers, images, volumes, networks) and
 * then uses the engine's own id. A value that is not currently on this machine
 * names nothing, so nothing runs; a value like "--help" is never read as a
 * flag; and a value with a "/" or "?" never reaches an API path. CLI calls use
 * execFile with argument arrays, so there is no shell to quote against either.
 */

import http from 'http';
import { execFile, spawn } from 'child_process';
import { existsSync } from 'fs';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);
let pty = null;
try { pty = require('node-pty'); } catch { /* the shell tab says so */ }

const MAX_LOG_BYTES = 768 * 1024;
const MAX_JOB_BYTES = 512 * 1024;
const IS_WIN = process.platform === 'win32';

// ─── Talking to the engine ───

/** Where the engine listens: DOCKER_HOST when set, else the platform default. */
function endpoint() {
  const host = String(process.env.DOCKER_HOST || '').trim();
  if (host.startsWith('unix://')) return { socketPath: host.slice(7) };
  if (host.startsWith('npipe://')) return { socketPath: host.slice(8).replace(/\//g, '\\') };
  if (host.startsWith('tcp://')) {
    try {
      const u = new URL('http://' + host.slice(6));
      return { host: u.hostname, port: Number(u.port) || 2375 };
    } catch { /* fall through to the default */ }
  }
  return IS_WIN ? { socketPath: '\\\\.\\pipe\\docker_engine' } : { socketPath: '/var/run/docker.sock' };
}

class EngineError extends Error {
  constructor(status, message) { super(message); this.status = status; }
}

function friendly(err) {
  const code = err && err.code;
  if (code === 'EACCES') return 'Permission denied on the Docker socket. Add the user Crundi runs as to the docker group, then restart Crundi.';
  if (code === 'ENOENT' || code === 'ECONNREFUSED') return 'Cannot reach Docker. Is the Docker service running?';
  return 'Cannot reach Docker: ' + (err && err.message || err);
}

function qs(query) {
  if (!query) return '';
  const parts = [];
  for (const [k, v] of Object.entries(query)) {
    if (v === undefined || v === null) continue;
    const val = typeof v === 'object' ? JSON.stringify(v) : String(v);
    parts.push(encodeURIComponent(k) + '=' + encodeURIComponent(val));
  }
  return parts.length ? '?' + parts.join('&') : '';
}

/** Open a request; resolves with the live response (status already checked). */
function open(method, path, { query, body, timeout = 30000 } = {}) {
  return new Promise((resolve, reject) => {
    const payload = body === undefined ? null : Buffer.from(JSON.stringify(body));
    const req = http.request({
      ...endpoint(),
      method,
      path: path + qs(query),
      headers: payload
        ? { 'Content-Type': 'application/json', 'Content-Length': payload.length, Host: 'docker' }
        : { Host: 'docker' },
      timeout: timeout || 0,
    }, (res) => {
      if (res.statusCode >= 400) {
        const chunks = [];
        res.on('data', (c) => chunks.push(c));
        res.on('end', () => {
          const raw = Buffer.concat(chunks).toString('utf8');
          let msg = raw;
          try { msg = JSON.parse(raw).message || raw; } catch { /* plain text */ }
          reject(new EngineError(res.statusCode, String(msg).trim() || ('HTTP ' + res.statusCode)));
        });
        return;
      }
      resolve({ res, req });
    });
    req.on('timeout', () => req.destroy(new Error('Docker took too long to answer')));
    req.on('error', (err) => reject(err instanceof EngineError ? err : new EngineError(0, friendly(err))));
    if (payload) req.write(payload);
    req.end();
  });
}

/** One request, whole reply. JSON when it is JSON, else the raw Buffer. */
async function call(method, path, opts = {}) {
  const { res } = await open(method, path, opts);
  const chunks = [];
  for await (const c of res) chunks.push(c);
  const buf = Buffer.concat(chunks);
  if (!buf.length) return null;
  if (String(res.headers['content-type'] || '').includes('json')) {
    try { return JSON.parse(buf.toString('utf8')); } catch { /* fall through */ }
  }
  return buf;
}

/** Run an engine call, turning its failure into the usual { ok:false } shape. */
async function guard(fn) {
  try { return await fn(); }
  catch (err) { return { ok: false, error: err instanceof EngineError ? err.message : String(err && err.message || err), status: err.status || 0 }; }
}

function runCli(args, timeoutMs = 30000) {
  return new Promise((resolve) => {
    execFile('docker', args, { timeout: timeoutMs, maxBuffer: 16 * 1024 * 1024, windowsHide: true },
      (err, stdout, stderr) => resolve({
        ok: !err, stdout: String(stdout || ''), stderr: String(stderr || ''),
        error: err ? (err.code === 'ENOENT' ? 'The docker command is not installed' : String(err.message || err)) : '',
      }));
  });
}

// ─── Is it here at all? ───

let statusCache = null;
let statusAt = 0;

/**
 * Whether Docker is installed, and whether we can talk to it.
 *
 * "Installed" is what decides if the tab shows at all; "reachable" is what
 * decides whether it shows the engine or the reason it cannot. Cached briefly:
 * the tab asks on every open.
 */
export async function status({ force = false } = {}) {
  if (!force && statusCache && Date.now() - statusAt < 15000) return statusCache;
  const ep = endpoint();
  let installed = ep.socketPath ? existsSync(ep.socketPath) : true;
  if (!installed) {
    const r = await runCli(['--version'], 5000);
    installed = r.ok;
  }
  let out = { ok: true, installed, reachable: false, error: '' };
  if (installed) {
    try {
      const v = await call('GET', '/version', { timeout: 6000 });
      out = { ...out, reachable: true, version: v.Version, apiVersion: v.ApiVersion, os: v.Os, arch: v.Arch };
    } catch (err) {
      out.error = err.message;
    }
  }
  statusCache = out;
  statusAt = Date.now();
  return out;
}

// ─── Shaping what the engine says ───

const short = (id) => String(id || '').replace(/^sha256:/, '').slice(0, 12);
const nameOf = (c) => String((c.Names && c.Names[0]) || c.Name || '').replace(/^\//, '');

function healthFrom(statusText) {
  const m = /\((healthy|unhealthy|health: starting)\)/.exec(String(statusText || ''));
  if (!m) return '';
  return m[1] === 'health: starting' ? 'starting' : m[1];
}

function portsOf(c) {
  const seen = new Set();
  const out = [];
  for (const p of c.Ports || []) {
    const key = (p.PublicPort || '') + '/' + p.PrivatePort + '/' + p.Type;
    if (seen.has(key)) continue;          // the same mapping on 0.0.0.0 and ::
    seen.add(key);
    out.push({ ip: p.IP || '', public: p.PublicPort || 0, private: p.PrivatePort, type: p.Type });
  }
  return out.sort((a, b) => (b.public ? 1 : 0) - (a.public ? 1 : 0) || a.private - b.private);
}

function shapeContainer(c) {
  const labels = c.Labels || {};
  const nets = (c.NetworkSettings && c.NetworkSettings.Networks) || {};
  return {
    id: short(c.Id),
    fullId: c.Id,
    name: nameOf(c),
    image: c.Image,
    imageId: c.ImageID,
    command: c.Command || '',
    created: c.Created,
    state: c.State,
    status: c.Status,
    health: healthFrom(c.Status),
    running: c.State === 'running',
    project: labels['com.docker.compose.project'] || '',
    service: labels['com.docker.compose.service'] || '',
    ports: portsOf(c),
    networks: Object.entries(nets).map(([name, n]) => ({ name, ip: n.IPAddress || '' })),
    mounts: (c.Mounts || []).map((m) => ({ type: m.Type, name: m.Name || '', source: m.Source || '', destination: m.Destination })),
  };
}

async function rawContainers() {
  return (await call('GET', '/containers/json', { query: { all: 'true' } })) || [];
}

// ─── Live numbers ───
//
// A one-shot stats sample carries no "previous" reading, so CPU % cannot be
// read off a single one. The last sample per container is kept here and the
// next call measures against it; the first call for a container reports no
// CPU figure rather than a made-up zero.

const lastSample = new Map();   // full id -> { total, system, at }

function digestStats(fullId, raw) {
  if (!raw || typeof raw !== 'object') return null;
  const cpu = raw.cpu_stats || {};
  const total = (cpu.cpu_usage && cpu.cpu_usage.total_usage) || 0;
  const system = cpu.system_cpu_usage || 0;
  const cpus = cpu.online_cpus || (cpu.cpu_usage && cpu.cpu_usage.percpu_usage || []).length || 1;
  let cpuPct = null;
  const prev = lastSample.get(fullId);
  if (prev && system > prev.system) {
    cpuPct = Math.max(0, ((total - prev.total) / (system - prev.system)) * cpus * 100);
  } else if (IS_WIN && prev && raw.read) {
    // Windows reports 100ns ticks and no system counter.
    const dt = (Date.parse(raw.read) - prev.at) * 10000;
    if (dt > 0) cpuPct = Math.max(0, ((total - prev.total) / dt) * 100);
  }
  lastSample.set(fullId, { total, system, at: Date.parse(raw.read) || Date.now() });

  const mem = raw.memory_stats || {};
  const ms = mem.stats || {};
  const cache = ms.inactive_file != null ? ms.inactive_file : (ms.total_inactive_file || 0);
  const memory = Math.max(0, (mem.usage || mem.privateworkingset || 0) - cache);
  let rx = 0, tx = 0;
  for (const n of Object.values(raw.networks || {})) { rx += n.rx_bytes || 0; tx += n.tx_bytes || 0; }
  let blkRead = 0, blkWrite = 0;
  for (const b of (raw.blkio_stats && raw.blkio_stats.io_service_bytes_recursive) || []) {
    const op = String(b.op || '').toLowerCase();
    if (op === 'read') blkRead += b.value || 0;
    else if (op === 'write') blkWrite += b.value || 0;
  }
  if (raw.storage_stats && raw.storage_stats.read_size_bytes != null) {
    blkRead = raw.storage_stats.read_size_bytes; blkWrite = raw.storage_stats.write_size_bytes || 0;
  }
  return {
    at: Date.now(),
    cpuPct,
    cpus,
    memory,
    memoryLimit: mem.limit || 0,
    pids: (raw.pids_stats && raw.pids_stats.current) || raw.num_procs || 0,
    netRx: rx, netTx: tx, blkRead, blkWrite,
  };
}

async function sampleOne(fullId) {
  try {
    const raw = await call('GET', '/containers/' + fullId + '/stats',
      { query: { stream: 'false', 'one-shot': 'true' }, timeout: 8000 });
    return digestStats(fullId, raw);
  } catch { return null; }
}

/** Run fn over items with at most `limit` in flight. */
async function pool(items, limit, fn) {
  const out = new Array(items.length);
  let i = 0;
  const worker = async () => { while (i < items.length) { const k = i++; out[k] = await fn(items[k]); } };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
  return out;
}

// ─── Containers ───

/** Every container, running or not; with CPU and memory when `stats` is set. */
export async function listContainers({ stats = false } = {}) {
  return guard(async () => {
    const list = (await rawContainers()).map(shapeContainer);
    if (stats) {
      const running = list.filter((c) => c.running);
      const samples = await pool(running, 8, (c) => sampleOne(c.fullId));
      running.forEach((c, k) => { c.stats = samples[k]; });
    }
    list.sort((a, b) => (b.running - a.running) || a.name.localeCompare(b.name));
    return { ok: true, containers: list };
  });
}

/** Resolve a client-supplied id or name to a real container, or null. */
async function resolveContainer(idOrName) {
  const want = String(idOrName || '').trim();
  if (!want) return null;
  const list = await rawContainers();
  const hit = list.find((c) => c.Id === want || nameOf(c) === want)
    || (want.length >= 6 ? list.find((c) => c.Id.startsWith(want)) : null);
  return hit ? shapeContainer(hit) : null;
}

/** Look a container up or say why not, in one step. */
async function withContainer(idOrName, fn) {
  return guard(async () => {
    const c = await resolveContainer(idOrName);
    if (!c) return { ok: false, error: 'No such container', status: 404 };
    return fn(c);
  });
}

const SECRET_KEY = /PASS|SECRET|TOKEN|KEY|AUTH|CREDENTIAL|PRIVATE/i;

/**
 * One container in full: a tidy overview plus the raw inspect JSON.
 *
 * The environment in the overview hides values whose names look like secrets;
 * the raw inspect shows everything, since that is what it is for and the tab is
 * owner-only.
 */
export async function containerDetail(idOrName) {
  return withContainer(idOrName, async (c) => {
    const ins = await call('GET', '/containers/' + c.fullId + '/json');
    const cfg = ins.Config || {};
    const st = ins.State || {};
    const host = ins.HostConfig || {};
    const env = (cfg.Env || []).map((e) => {
      const at = e.indexOf('=');
      const key = at < 0 ? e : e.slice(0, at);
      const value = at < 0 ? '' : e.slice(at + 1);
      const secret = SECRET_KEY.test(key) && value !== '';
      return { key, value: secret ? '' : value, hidden: secret };
    });
    return {
      ok: true,
      container: c,
      overview: {
        id: ins.Id,
        name: c.name,
        image: cfg.Image || c.image,
        imageId: ins.Image,
        created: ins.Created,
        startedAt: st.StartedAt,
        finishedAt: st.FinishedAt,
        state: st.Status,
        exitCode: st.ExitCode,
        error: st.Error || '',
        oomKilled: !!st.OOMKilled,
        health: st.Health ? st.Health.Status : '',
        restartCount: ins.RestartCount || 0,
        restartPolicy: host.RestartPolicy ? host.RestartPolicy.Name : '',
        tty: !!cfg.Tty,
        entrypoint: [].concat(cfg.Entrypoint || []).join(' '),
        cmd: [].concat(cfg.Cmd || []).join(' '),
        workingDir: cfg.WorkingDir || '',
        user: cfg.User || '',
        hostname: cfg.Hostname || '',
        ports: c.ports,
        networks: Object.entries((ins.NetworkSettings && ins.NetworkSettings.Networks) || {})
          .map(([name, n]) => ({ name, ip: n.IPAddress || '', gateway: n.Gateway || '', mac: n.MacAddress || '' })),
        mounts: (ins.Mounts || []).map((m) => ({
          type: m.Type, name: m.Name || '', source: m.Source || '', destination: m.Destination, rw: m.RW !== false,
        })),
        env,
        labels: cfg.Labels || {},
        project: c.project,
        service: c.service,
      },
      inspect: ins,
    };
  });
}

const VERBS = new Set(['start', 'stop', 'restart', 'pause', 'unpause', 'kill']);

/** start, stop, restart, pause, unpause or kill. */
export async function containerAction(idOrName, verb) {
  if (!VERBS.has(verb)) return { ok: false, error: 'Unknown action' };
  return withContainer(idOrName, async (c) => {
    await call('POST', '/containers/' + c.fullId + '/' + verb, { timeout: 120000 });
    lastSample.delete(c.fullId);
    return { ok: true, name: c.name };
  });
}

/** Container names Docker itself would accept. */
const NAME_RE = /^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,127}$/;

export async function renameContainer(idOrName, newName) {
  const name = String(newName || '').trim().replace(/^\//, '');
  if (!NAME_RE.test(name)) return { ok: false, error: 'Names use letters, digits, _ . and - and start with a letter or digit' };
  return withContainer(idOrName, async (c) => {
    await call('POST', '/containers/' + c.fullId + '/rename', { query: { name } });
    return { ok: true, name };
  });
}

export async function removeContainer(idOrName, { force = false, volumes = false } = {}) {
  return withContainer(idOrName, async (c) => {
    await call('DELETE', '/containers/' + c.fullId, { query: { force: !!force, v: !!volumes }, timeout: 120000 });
    lastSample.delete(c.fullId);
    return { ok: true, name: c.name };
  });
}

/** One stats sample for one container (the client keeps the history). */
export async function containerStats(idOrName) {
  return withContainer(idOrName, async (c) => {
    if (!c.running) return { ok: true, running: false, stats: null };
    return { ok: true, running: true, stats: await sampleOne(c.fullId) };
  });
}

/**
 * Split Docker's multiplexed log stream into { stream, text } frames.
 *
 * A container without a TTY sends 8-byte headers (stream, 0, 0, 0, size) in
 * front of every chunk; one with a TTY sends the bytes as they are. A chunk
 * can end mid-frame, so leftovers are carried to the next call.
 */
export function makeDemuxer(tty) {
  let rest = Buffer.alloc(0);
  return (chunk) => {
    if (tty) return [{ stream: 'stdout', text: chunk.toString('utf8') }];
    rest = rest.length ? Buffer.concat([rest, chunk]) : chunk;
    const out = [];
    while (rest.length >= 8) {
      const kind = rest[0];
      const size = rest.readUInt32BE(4);
      if (rest.length < 8 + size) break;
      out.push({ stream: kind === 2 ? 'stderr' : 'stdout', text: rest.subarray(8, 8 + size).toString('utf8') });
      rest = rest.subarray(8 + size);
    }
    return out;
  };
}

/** The last `tail` lines of a container's log, both streams, in order. */
export async function containerLogs(idOrName, tail = 500) {
  return withContainer(idOrName, async (c) => {
    const ins = await call('GET', '/containers/' + c.fullId + '/json');
    const n = Math.max(1, Math.min(5000, parseInt(tail, 10) || 500));
    const raw = await call('GET', '/containers/' + c.fullId + '/logs',
      { query: { stdout: 'true', stderr: 'true', timestamps: 'true', tail: n }, timeout: 30000 });
    const buf = Buffer.isBuffer(raw) ? raw : Buffer.from(raw ? String(raw) : '');
    const frames = makeDemuxer(!!(ins.Config && ins.Config.Tty))(buf);
    let text = frames.map((f) => f.text).join('');
    if (text.length > MAX_LOG_BYTES) text = text.slice(text.length - MAX_LOG_BYTES);
    return { ok: true, name: c.name, text };
  });
}

/**
 * Follow a container's log as it is written. `onFrames` gets arrays of
 * { stream, text }; `onEnd` fires once when the stream stops for any reason.
 * Returns a function that stops following.
 */
export async function followLogs(idOrName, { tail = 500 } = {}, onFrames, onEnd) {
  const c = await resolveContainer(idOrName).catch(() => null);
  if (!c) return { ok: false, error: 'No such container' };
  let req = null;
  let stopped = false;
  try {
    const ins = await call('GET', '/containers/' + c.fullId + '/json');
    const demux = makeDemuxer(!!(ins.Config && ins.Config.Tty));
    const n = Math.max(0, Math.min(5000, parseInt(tail, 10) || 0));
    const opened = await open('GET', '/containers/' + c.fullId + '/logs', {
      query: { stdout: 'true', stderr: 'true', timestamps: 'true', follow: 'true', tail: n },
      timeout: 0,
    });
    req = opened.req;
    opened.res.on('data', (chunk) => { if (!stopped) { const f = demux(chunk); if (f.length) onFrames(f); } });
    const end = () => { if (!stopped) { stopped = true; onEnd && onEnd(); } };
    opened.res.on('end', end);
    opened.res.on('error', end);
    opened.res.on('close', end);
  } catch (err) {
    return { ok: false, error: err.message };
  }
  return {
    ok: true,
    name: c.name,
    stop: () => { stopped = true; try { req && req.destroy(); } catch { /* gone */ } },
  };
}

/**
 * A shell inside a running container, on a PTY.
 *
 * Prefers bash and falls back to sh, inside the container, so the image picks.
 * Returns the node-pty process; the caller wires it to a socket.
 */
export async function spawnShell(idOrName, { cols = 100, rows = 30 } = {}) {
  if (!pty) return { ok: false, error: 'The shell needs node-pty, which is not available on this install.' };
  const c = await resolveContainer(idOrName).catch(() => null);
  if (!c) return { ok: false, error: 'No such container' };
  if (!c.running) return { ok: false, error: 'Start the container to open a shell in it.' };
  const shell = 'command -v bash >/dev/null 2>&1 && exec bash || exec sh';
  try {
    const proc = pty.spawn('docker', ['exec', '-it', '-e', 'TERM=xterm-256color', c.fullId, '/bin/sh', '-c', shell], {
      name: 'xterm-256color',
      cols: Math.max(20, Math.min(400, cols | 0)),
      rows: Math.max(5, Math.min(200, rows | 0)),
      env: process.env,
    });
    return { ok: true, name: c.name, proc };
  } catch (err) {
    return { ok: false, error: 'Could not open a shell: ' + err.message };
  }
}

// ─── Compose stacks ───

const STACK_COMMANDS = {
  up: ['up', '-d', '--remove-orphans'],
  pull: ['pull'],
  restart: ['restart'],
  stop: ['stop'],
  start: ['start'],
  down: ['down'],
};
/** These need the compose file itself; the rest work from the project name. */
const NEEDS_FILE = new Set(['up', 'pull']);

function groupStacks(raw) {
  const byName = new Map();
  for (const c of raw) {
    const labels = c.Labels || {};
    const name = labels['com.docker.compose.project'];
    if (!name) continue;
    if (!byName.has(name)) byName.set(name, []);
    byName.get(name).push(c);
  }
  const stacks = [];
  for (const [name, list] of byName) {
    const labels = list[0].Labels || {};
    const files = String(labels['com.docker.compose.project.config_files'] || '').split(',').filter(Boolean);
    const directory = labels['com.docker.compose.project.working_dir'] || '';
    const containers = list.map(shapeContainer)
      .sort((a, b) => a.service.localeCompare(b.service) || a.name.localeCompare(b.name));
    stacks.push({
      name,
      directory,
      files,
      filesPresent: files.length > 0 && files.every((f) => existsSync(f)),
      live: containers.filter((c) => ['running', 'paused', 'restarting'].includes(c.state)).length,
      total: containers.length,
      containers,
    });
  }
  return stacks.sort((a, b) => a.name.localeCompare(b.name));
}

export async function listStacks() {
  return guard(async () => ({ ok: true, stacks: groupStacks(await rawContainers()) }));
}

export async function stackAction(name, action) {
  if (!STACK_COMMANDS[action]) return { ok: false, error: 'Unknown action' };
  return guard(async () => {
    const stack = groupStacks(await rawContainers()).find((s) => s.name === String(name || ''));
    if (!stack) return { ok: false, error: 'No such stack', status: 404 };
    if (NEEDS_FILE.has(action) && !stack.filesPresent) {
      return { ok: false, error: 'This needs the compose file, which was not found on this machine' };
    }
    const argv = ['compose', '--ansi', 'never', '--progress', 'plain', '-p', stack.name];
    if (stack.filesPresent) {
      for (const f of stack.files) argv.push('-f', f);
      if (stack.directory && existsSync(stack.directory)) argv.push('--project-directory', stack.directory);
    }
    argv.push(...STACK_COMMANDS[action]);
    const job = startJob(stack.name + ': compose ' + action, 'docker', argv,
      stack.directory && existsSync(stack.directory) ? stack.directory : undefined);
    return { ok: true, job: job.id, command: 'docker ' + argv.join(' ') };
  });
}

// ─── Jobs: commands whose output is worth watching ───
//
// Compose actions and image pulls take a while and say a lot while they do.
// Each runs as a job the client polls with an offset, so a slow phone
// connection only ever re-fetches what it has not seen yet.

const jobs = new Map();
let jobSeq = 0;

function trimJobs() {
  const cutoff = Date.now() - 15 * 60 * 1000;
  for (const [id, j] of jobs) if (j.done && j.endedAt < cutoff) jobs.delete(id);
  while (jobs.size > 30) jobs.delete(jobs.keys().next().value);
}

function newJob(title) {
  trimJobs();
  const id = 'j' + (++jobSeq) + '-' + Date.now().toString(36);
  const job = { id, title, out: '', dropped: 0, done: false, code: null, startedAt: Date.now(), endedAt: 0 };
  jobs.set(id, job);
  return job;
}

function jobWrite(job, text) {
  job.out += text;
  if (job.out.length > MAX_JOB_BYTES) {
    const cut = job.out.length - MAX_JOB_BYTES;
    job.out = job.out.slice(cut);
    job.dropped += cut;
  }
}

function jobFinish(job, code) {
  if (job.done) return;
  job.done = true;
  job.code = code;
  job.endedAt = Date.now();
}

function startJob(title, cmd, argv, cwd) {
  const job = newJob(title);
  jobWrite(job, '$ ' + cmd + ' ' + argv.join(' ') + '\n');
  let child;
  try {
    child = spawn(cmd, argv, { cwd, windowsHide: true, env: process.env });
  } catch (err) {
    jobWrite(job, String(err.message || err) + '\n');
    jobFinish(job, -1);
    return job;
  }
  child.stdout.on('data', (d) => jobWrite(job, d.toString('utf8')));
  child.stderr.on('data', (d) => jobWrite(job, d.toString('utf8')));
  child.on('error', (err) => { jobWrite(job, String(err.message || err) + '\n'); jobFinish(job, -1); });
  child.on('close', (code) => jobFinish(job, code == null ? -1 : code));
  return job;
}

/** Output from `from` onward (an absolute offset into everything written). */
export function jobOutput(id, from = 0) {
  const job = jobs.get(String(id || ''));
  if (!job) return { ok: false, error: 'No such job' };
  const start = Math.max(0, (parseInt(from, 10) || 0) - job.dropped);
  return {
    ok: true,
    title: job.title,
    text: job.out.slice(start),
    next: job.dropped + job.out.length,
    done: job.done,
    code: job.code,
  };
}

// ─── Images ───

export async function listImages() {
  return guard(async () => {
    const [imgs, ctrs] = await Promise.all([
      call('GET', '/images/json'),
      rawContainers(),
    ]);
    const users = new Map();
    for (const c of ctrs) {
      if (!users.has(c.ImageID)) users.set(c.ImageID, []);
      users.get(c.ImageID).push(nameOf(c));
    }
    const images = (imgs || []).map((i) => {
      const tags = (i.RepoTags || []).filter((t) => t && t !== '<none>:<none>');
      const digests = (i.RepoDigests || []).filter((d) => d && !d.startsWith('<none>'));
      return {
        id: short(i.Id),
        fullId: i.Id,
        tags,
        name: tags[0] || (digests[0] ? digests[0].split('@')[0] + ' (untagged)' : '<none>'),
        size: i.Size || 0,
        created: i.Created,
        dangling: tags.length === 0,
        usedBy: users.get(i.Id) || [],
      };
    });
    images.sort((a, b) => (b.usedBy.length > 0) - (a.usedBy.length > 0) || a.name.localeCompare(b.name));
    return { ok: true, images };
  });
}

export async function removeImage(id, { force = false } = {}) {
  return guard(async () => {
    const want = String(id || '');
    const imgs = (await call('GET', '/images/json')) || [];
    const img = imgs.find((i) => i.Id === want || short(i.Id) === want || (i.RepoTags || []).includes(want));
    if (!img) return { ok: false, error: 'No such image', status: 404 };
    // Removing by tag untags just that name; removing by id with several tags
    // needs force. Use the tag when the client named one.
    const target = (img.RepoTags || []).includes(want) ? want : img.Id;
    await call('DELETE', '/images/' + encodeURIComponent(target), { query: { force: !!force }, timeout: 120000 });
    return { ok: true, name: (img.RepoTags || [])[0] || short(img.Id) };
  });
}

/** An image reference as Docker reads it: no spaces, no flags. */
const REF_RE = /^[a-zA-Z0-9][a-zA-Z0-9._\-/:@]{0,254}$/;

export function pullImage(reference) {
  const ref = String(reference || '').trim();
  if (!REF_RE.test(ref)) return { ok: false, error: 'That does not look like an image name' };
  const job = startJob('Pull ' + ref, 'docker', ['pull', ref]);
  return { ok: true, job: job.id };
}

// ─── Volumes ───

export async function listVolumes() {
  return guard(async () => {
    const [vols, ctrs] = await Promise.all([call('GET', '/volumes'), rawContainers()]);
    const users = new Map();
    for (const c of ctrs) {
      for (const m of c.Mounts || []) {
        if (m.Type !== 'volume' || !m.Name) continue;
        if (!users.has(m.Name)) users.set(m.Name, []);
        users.get(m.Name).push(nameOf(c));
      }
    }
    const volumes = ((vols && vols.Volumes) || []).map((v) => ({
      name: v.Name,
      driver: v.Driver,
      mountpoint: v.Mountpoint,
      created: v.CreatedAt || '',
      project: (v.Labels || {})['com.docker.compose.project'] || '',
      anonymous: /^[0-9a-f]{64}$/.test(v.Name),
      usedBy: users.get(v.Name) || [],
    }));
    volumes.sort((a, b) => (b.usedBy.length > 0) - (a.usedBy.length > 0) || a.anonymous - b.anonymous || a.name.localeCompare(b.name));
    return { ok: true, volumes };
  });
}

export async function removeVolume(name) {
  return guard(async () => {
    const vols = ((await call('GET', '/volumes')) || {}).Volumes || [];
    const v = vols.find((x) => x.Name === String(name || ''));
    if (!v) return { ok: false, error: 'No such volume', status: 404 };
    await call('DELETE', '/volumes/' + encodeURIComponent(v.Name), { timeout: 60000 });
    return { ok: true, name: v.Name };
  });
}

// ─── Networks ───

const BUILTIN_NETWORKS = new Set(['bridge', 'host', 'none', 'nat']);

export async function listNetworks() {
  return guard(async () => {
    const [nets, ctrs] = await Promise.all([call('GET', '/networks'), rawContainers()]);
    const users = new Map();
    for (const c of ctrs) {
      for (const [, n] of Object.entries((c.NetworkSettings && c.NetworkSettings.Networks) || {})) {
        if (!n.NetworkID) continue;
        if (!users.has(n.NetworkID)) users.set(n.NetworkID, []);
        users.get(n.NetworkID).push(nameOf(c));
      }
    }
    const networks = (nets || []).map((n) => {
      const cfg = (n.IPAM && n.IPAM.Config && n.IPAM.Config[0]) || {};
      return {
        id: short(n.Id),
        fullId: n.Id,
        name: n.Name,
        driver: n.Driver,
        scope: n.Scope,
        subnet: cfg.Subnet || '',
        gateway: cfg.Gateway || '',
        internal: !!n.Internal,
        builtin: BUILTIN_NETWORKS.has(n.Name),
        project: (n.Labels || {})['com.docker.compose.project'] || '',
        usedBy: users.get(n.Id) || [],
      };
    });
    networks.sort((a, b) => a.builtin - b.builtin || (b.usedBy.length > 0) - (a.usedBy.length > 0) || a.name.localeCompare(b.name));
    return { ok: true, networks };
  });
}

export async function removeNetwork(id) {
  return guard(async () => {
    const nets = (await call('GET', '/networks')) || [];
    const want = String(id || '');
    const n = nets.find((x) => x.Id === want || short(x.Id) === want || x.Name === want);
    if (!n) return { ok: false, error: 'No such network', status: 404 };
    if (BUILTIN_NETWORKS.has(n.Name)) return { ok: false, error: 'Docker\'s own networks cannot be removed' };
    await call('DELETE', '/networks/' + n.Id, { timeout: 60000 });
    return { ok: true, name: n.Name };
  });
}

// ─── The engine ───

/**
 * Disk use per category, with what a clean-up would free. Separate from
 * systemInfo because the engine walks every layer to answer it: seconds on a
 * machine with a large build cache, where the engine facts are instant.
 */
export async function diskUsage() {
  return guard(async () => {
    const df = await call('GET', '/system/df', { timeout: 120000 });
    const disk = {};
    if (df) {
      const imgs = df.Images || [];
      disk.images = {
        count: imgs.length,
        size: imgs.reduce((s, i) => s + (i.Size || 0), 0),
        reclaimable: imgs.filter((i) => !i.Containers).reduce((s, i) => s + (i.Size || 0), 0),
      };
      const ctrs = df.Containers || [];
      disk.containers = {
        count: ctrs.length,
        size: ctrs.reduce((s, c) => s + (c.SizeRw || 0), 0),
        reclaimable: ctrs.filter((c) => c.State !== 'running').reduce((s, c) => s + (c.SizeRw || 0), 0),
      };
      const vols = df.Volumes || [];
      disk.volumes = {
        count: vols.length,
        size: vols.reduce((s, v) => s + Math.max(0, (v.UsageData && v.UsageData.Size) || 0), 0),
        reclaimable: vols.filter((v) => !(v.UsageData && v.UsageData.RefCount))
          .reduce((s, v) => s + Math.max(0, (v.UsageData && v.UsageData.Size) || 0), 0),
      };
      const bc = df.BuildCache || [];
      disk.build = {
        count: bc.length,
        size: bc.reduce((s, b) => s + (b.Size || 0), 0),
        reclaimable: bc.filter((b) => !b.InUse).reduce((s, b) => s + (b.Size || 0), 0),
      };
    }
    return { ok: true, disk };
  });
}

export async function systemInfo() {
  return guard(async () => {
    const [info, version] = await Promise.all([call('GET', '/info'), call('GET', '/version')]);
    return {
      ok: true,
      engine: {
        version: version.Version,
        apiVersion: version.ApiVersion,
        compose: await composeVersion(),
        os: info.OperatingSystem,
        kernel: info.KernelVersion,
        arch: info.Architecture,
        cpus: info.NCPU,
        memory: info.MemTotal,
        storageDriver: info.Driver,
        cgroup: info.CgroupDriver ? info.CgroupDriver + (info.CgroupVersion ? ' v' + info.CgroupVersion : '') : '',
        rootDir: info.DockerRootDir,
        name: info.Name,
        containers: info.Containers, running: info.ContainersRunning,
        paused: info.ContainersPaused, stopped: info.ContainersStopped,
        images: info.Images,
        warnings: info.Warnings || [],
      },
    };
  });
}

let composeCache = null;
async function composeVersion() {
  if (composeCache !== null) return composeCache;
  const r = await runCli(['compose', 'version', '--short'], 8000);
  composeCache = r.ok ? r.stdout.trim() : '';
  return composeCache;
}

/**
 * Clean up one category. `images` removes only dangling ones; `images-all`
 * every image no container uses; `volumes` every unused volume (named ones
 * too: the engine's default since 23.0 spares those, which hides the space).
 */
export async function prune(kind) {
  const routes = {
    containers: ['/containers/prune', {}],
    images: ['/images/prune', { filters: { dangling: ['true'] } }],
    'images-all': ['/images/prune', { filters: { dangling: ['false'] } }],
    volumes: ['/volumes/prune', { filters: { all: ['true'] } }],
    networks: ['/networks/prune', {}],
    build: ['/build/prune', { all: 'true' }],
  };
  const r = routes[kind];
  if (!r) return { ok: false, error: 'Unknown clean-up' };
  return guard(async () => {
    const out = await call('POST', r[0], { query: r[1], timeout: 600000 }) || {};
    const removed = (out.ContainersDeleted || out.VolumesDeleted || out.NetworksDeleted
      || out.ImagesDeleted || out.CachesDeleted || []).length;
    return { ok: true, removed, reclaimed: out.SpaceReclaimed || 0 };
  });
}

/** Forget cached state. For tests. */
export function reset() {
  statusCache = null;
  lastSample.clear();
  jobs.clear();
}
