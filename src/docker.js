/**
 * Docker containers — list, read logs, stop.
 *
 * Security note, because this takes an id from the browser: a container is
 * never addressed by the string the client sent. Every request resolves that
 * string against the live `docker ps -a` listing and then uses the resolved
 * container's own hex ID. An id that is not currently on this machine is not a
 * container, so nothing runs. That closes two doors at once — arbitrary values
 * reaching the command line, and a name like "--help" or "-f" being read by
 * docker as a flag instead of a target.
 *
 * Everything uses execFile with an argument array, so there is no shell to
 * quote against in the first place.
 */

import { execFile } from 'child_process';

const MAX_LOG_BYTES = 512 * 1024;

function run(args, timeoutMs = 20000) {
  return new Promise((resolve) => {
    execFile('docker', args, { timeout: timeoutMs, maxBuffer: 16 * 1024 * 1024, windowsHide: true },
      (err, stdout, stderr) => {
        resolve({
          ok: !err,
          stdout: String(stdout || ''),
          stderr: String(stderr || ''),
          error: err ? (err.code === 'ENOENT' ? 'Docker is not installed' : String(err.message || err)) : '',
        });
      });
  });
}

let availableCache = null;
let availableAt = 0;

/** Is there a docker daemon we can talk to? Cached for a minute. */
export async function isAvailable() {
  const now = Date.now();
  if (availableCache !== null && now - availableAt < 60000) return availableCache;
  const r = await run(['version', '--format', '{{.Server.Version}}'], 8000);
  availableCache = r.ok;
  availableAt = now;
  return availableCache;
}

/** Pull one label out of docker's flat "k=v,k=v" Labels string. */
function label(labels, key) {
  const s = String(labels || '');
  const at = s.indexOf(key + '=');
  if (at < 0) return '';
  const rest = s.slice(at + key.length + 1);
  const end = rest.indexOf(',');
  return (end < 0 ? rest : rest.slice(0, end)).trim();
}

/**
 * Every container on the machine, running or not.
 * The raw rows carry a very large Labels blob; only the few fields the UI
 * actually draws are passed on.
 */
export async function listContainers() {
  if (!await isAvailable()) return { ok: false, error: 'Docker is not available', containers: [] };
  const r = await run(['ps', '-a', '--format', 'json']);
  if (!r.ok) return { ok: false, error: r.stderr.trim() || r.error, containers: [] };
  const out = [];
  for (const line of r.stdout.split('\n')) {
    const t = line.trim();
    if (!t.startsWith('{')) continue;
    let row;
    try { row = JSON.parse(t); } catch { continue; }
    out.push({
      id: String(row.ID || ''),
      ips: [],
      name: String(row.Names || '').split(',')[0],
      image: String(row.Image || ''),
      state: String(row.State || ''),
      status: String(row.Status || ''),
      health: row.HealthStatus && row.HealthStatus !== 'none' ? String(row.HealthStatus) : '',
      // Which compose stack it belongs to — on a shared box this is how you
      // tell your own container from somebody else's before stopping it.
      project: label(row.Labels, 'com.docker.compose.project'),
      running: String(row.State || '') === 'running',
    });
  }
  await attachAddresses(out);
  out.sort((a, b) => (b.running - a.running) || a.name.localeCompare(b.name));
  return { ok: true, containers: out };
}

/**
 * Fill in each container's network addresses.
 *
 * One `docker inspect` for the whole set rather than one per container: on a
 * machine running ten of them that is the difference between a listing that
 * appears at once and one that visibly crawls. `docker ps` does not carry the
 * addresses, only the network names, so this second call is unavoidable.
 */
async function attachAddresses(list) {
  if (!list.length) return;
  const fmt = '{{.Id}}|{{range $k, $v := .NetworkSettings.Networks}}{{$k}}={{$v.IPAddress}} {{end}}';
  const r = await run(['inspect', '--format', fmt, ...list.map((c) => c.id)], 25000);
  if (!r.ok) return;   // addresses are a nicety; the listing still stands
  const byPrefix = new Map();
  for (const line of r.stdout.split('\n')) {
    const at = line.indexOf('|');
    if (at < 0) continue;
    const full = line.slice(0, at).trim();
    const pairs = [];
    for (const part of line.slice(at + 1).trim().split(/\s+/)) {
      if (!part) continue;
      const eq = part.lastIndexOf('=');
      if (eq < 0) continue;
      const net = part.slice(0, eq);
      const ip = part.slice(eq + 1);
      // A stopped container keeps its network membership but has no address.
      if (ip) pairs.push({ network: net, ip });
    }
    byPrefix.set(full, pairs);
  }
  for (const c of list) {
    for (const [full, pairs] of byPrefix) {
      if (full.startsWith(c.id)) { c.ips = pairs; break; }
    }
  }
}

/** Resolve a client-supplied string to a real container, or null. */
async function resolve(idOrName) {
  const want = String(idOrName || '');
  if (!want) return null;
  const { containers } = await listContainers();
  return containers.find((c) => c.id === want || c.name === want)
    || containers.find((c) => c.id.startsWith(want) && want.length >= 8)
    || null;
}

/** Recent log lines for one container, newest last. */
export async function containerLogs(idOrName, tail = 200) {
  const c = await resolve(idOrName);
  if (!c) return { ok: false, error: 'No such container' };
  const n = Math.max(1, Math.min(2000, parseInt(tail, 10) || 200));
  // Docker writes container stdout to stdout and stderr to stderr; a log view
  // that dropped one of them would hide exactly the half you came to read.
  const r = await run(['logs', '--tail', String(n), '--timestamps', c.id], 30000);
  if (!r.ok && !r.stdout && !r.stderr) return { ok: false, error: r.error || 'Could not read logs' };
  let text = (r.stdout || '') + (r.stderr || '');
  if (text.length > MAX_LOG_BYTES) text = text.slice(text.length - MAX_LOG_BYTES);
  return { ok: true, name: c.name, text };
}

/** Stop a container. It keeps its data and can be started again. */
export async function stopContainer(idOrName) {
  const c = await resolve(idOrName);
  if (!c) return { ok: false, error: 'No such container' };
  if (!c.running) return { ok: false, error: 'That container is already stopped' };
  const r = await run(['stop', c.id], 60000);
  if (!r.ok) return { ok: false, error: r.stderr.trim().split('\n')[0] || r.error };
  return { ok: true, name: c.name };
}

/** Start a container that is currently stopped. */
export async function startContainer(idOrName) {
  const c = await resolve(idOrName);
  if (!c) return { ok: false, error: 'No such container' };
  if (c.running) return { ok: false, error: 'That container is already running' };
  const r = await run(['start', c.id], 60000);
  if (!r.ok) return { ok: false, error: r.stderr.trim().split('\n')[0] || r.error };
  return { ok: true, name: c.name };
}
