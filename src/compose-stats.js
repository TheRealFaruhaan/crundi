/**
 * compose-stats.js — real usage numbers for services that run docker compose.
 *
 * ─── Why this exists ───
 *
 * A service registered as `docker compose up instagram-scraper` was reporting
 * 73 MB and roughly no CPU while the container it started was using 902 MB and
 * 266% of a core. Nothing was broken: the Services tab walks the process tree
 * descending from the pid Crundi spawned, and for a compose service that tree
 * is
 *
 *     sh -> sg -> docker -> docker-compose
 *
 * which is a client attached to the daemon, relaying log lines. It really is
 * idle. The workload lives under containerd-shim, reparented to init, in a
 * different tree entirely — so the number was an honest measurement of the
 * wrong thing, which is the worst kind of wrong: it looks fine.
 *
 * So compose-backed services are measured by their CONTAINERS instead. A
 * service that starts three of them (`up warp db ocr-server`) reports the sum,
 * because that is what the service costs the machine.
 *
 * ─── Where the numbers come from ───
 *
 * `docker stats --no-stream` is the obvious source and it is too slow to put on
 * a 3s tick: it takes ~3.2s here, because the CLI collects its own sample
 * window before printing. So on cgroup v2 (Linux) the counters are read
 * straight out of /sys/fs/cgroup, which costs a few file reads and lets the
 * existing cadence stand. Memory matches docker's own figure because it is
 * computed the same way — memory.current minus inactive_file, the page cache
 * the kernel would drop under pressure rather than OOM.
 *
 * Everywhere else — Windows, cgroup v1, unusual layouts — falls back to
 * `docker stats`, refreshed on its own slow timer and never awaited by a tick.
 * Until the first one lands the value is null, which draws as a dash. That is
 * deliberate: a number that is 15 seconds stale is fine, and a number invented
 * to avoid an empty cell is not.
 */

import { execFile } from 'child_process';
import { readFile, access, realpath } from 'fs/promises';
import { constants } from 'fs';

const isLinux = process.platform === 'linux';

/** How long the container listing (id -> compose labels) is trusted. */
const MAP_TTL_MS = 20000;
/** How long a `docker stats` sample is trusted on the fallback path. */
const STATS_TTL_MS = 15000;

// ─── Reading the command ───

/**
 * Split a command line into tokens, honouring quotes.
 *
 * A quoted run stays one token, so a path with a space in it survives. The
 * nested case that matters here — `sg docker -c "docker compose up warp db"` —
 * is handled by parseComposeCommand recursing into that token instead, which
 * keeps quoting correct rather than trading it away.
 */
export function tokenize(command) {
  const out = [];
  let cur = '';
  let quote = '';
  let had = false;
  for (const ch of String(command || '')) {
    if (quote) {
      if (ch === quote) quote = '';
      else cur += ch;
      continue;
    }
    if (ch === '"' || ch === "'") { quote = ch; had = true; continue; }
    if (/\s/.test(ch)) {
      if (cur || had) out.push(cur);
      cur = '';
      had = false;
      continue;
    }
    cur += ch;
  }
  if (cur || had) out.push(cur);
  return out;
}

// Flags that swallow the next token. Getting this wrong would read a flag's
// value as a service name — `-f docker-compose.yml up api` would look like a
// service called "docker-compose.yml".
const GLOBAL_VALUE_FLAGS = new Set([
  '-f', '--file', '-p', '--project-name', '--project-directory',
  '--profile', '--env-file', '--parallel', '--progress', '--ansi',
]);
const UP_VALUE_FLAGS = new Set([
  '--scale', '--exit-code-from', '-t', '--timeout', '--wait-timeout',
  '--attach', '--no-attach', '--pull', '--remove-orphans-timeout',
]);

/**
 * Does this command start a compose stack, and which services of it?
 *
 * @returns {{services: string[], projectName: string}|null}
 *          null when it is not a compose `up`/`start` at all. An empty
 *          `services` means the command named none, which in compose means all
 *          of them — a real answer, not a failure to find any.
 */
export function parseComposeCommand(command, depth = 0) {
  const t = tokenize(command);
  let i = -1;
  for (let n = 0; n < t.length; n++) {
    if (t[n] === 'docker-compose') { i = n + 1; break; }
    if (t[n] === 'docker' && t[n + 1] === 'compose') { i = n + 2; break; }
  }
  if (i < 0) {
    // Not at this level, but services are routinely wrapped: `sg docker -c`,
    // `bash -c`, `ssh host`. Look inside any token that still holds a whole
    // command line. Bounded, because a crafted command should not be able to
    // make this recurse forever.
    if (depth >= 3) return null;
    for (const tok of t) {
      if (!/\s/.test(tok)) continue;
      if (!tok.includes('docker compose') && !tok.includes('docker-compose')) continue;
      const inner = parseComposeCommand(tok, depth + 1);
      if (inner) return inner;
    }
    return null;
  }

  let projectName = '';
  // Global flags sit between `compose` and the subcommand.
  while (i < t.length && t[i].startsWith('-')) {
    const tok = t[i];
    const eq = tok.indexOf('=');
    if (eq > 0) {
      const name = tok.slice(0, eq);
      if (name === '-p' || name === '--project-name') projectName = tok.slice(eq + 1);
      i++;
      continue;
    }
    if (GLOBAL_VALUE_FLAGS.has(tok)) {
      if (tok === '-p' || tok === '--project-name') projectName = t[i + 1] || '';
      i += 2;
      continue;
    }
    i++;
  }

  const sub = t[i];
  // Only the subcommands that leave containers running are worth tracking.
  if (sub !== 'up' && sub !== 'start') return null;
  i++;

  const services = [];
  while (i < t.length) {
    const tok = t[i];
    if (tok.startsWith('-')) {
      if (tok.indexOf('=') > 0) { i++; continue; }
      i += UP_VALUE_FLAGS.has(tok) ? 2 : 1;
      continue;
    }
    services.push(tok);
    i++;
  }
  return { services, projectName };
}

// ─── Finding the containers ───

function run(args, timeoutMs = 20000) {
  return new Promise((resolve) => {
    execFile('docker', args, { timeout: timeoutMs, maxBuffer: 16 * 1024 * 1024, windowsHide: true },
      (err, stdout) => resolve({ ok: !err, stdout: String(stdout || '') }));
  });
}

let mapCache = { at: 0, containers: [] };

/**
 * Running containers that belong to a compose project, with the labels needed
 * to tie them back to a service. `--no-trunc` because cgroup directories are
 * named with the full 64-character id, not the short one `docker ps` prints.
 */
export async function listComposeContainers({ force = false } = {}) {
  if (!force && Date.now() - mapCache.at < MAP_TTL_MS) return mapCache.containers;
  const fmt = '{{.ID}}\t{{.Names}}\t{{.Label "com.docker.compose.project"}}'
    + '\t{{.Label "com.docker.compose.service"}}'
    + '\t{{.Label "com.docker.compose.project.working_dir"}}';
  const r = await run(['ps', '--no-trunc', '--format', fmt]);
  if (!r.ok) {
    // Keep whatever was known rather than blanking every service because one
    // docker call failed. It goes stale, but staleness is bounded by the TTL.
    mapCache = { at: Date.now(), containers: mapCache.containers };
    return mapCache.containers;
  }
  const containers = [];
  for (const line of r.stdout.split('\n')) {
    const f = line.split('\t');
    if (f.length < 5 || !f[0].trim()) continue;
    const service = f[3].trim();
    if (!service) continue;      // not a compose container
    containers.push({
      id: f[0].trim(),
      name: f[1].trim().split(',')[0],
      project: f[2].trim(),
      service,
      workingDir: f[4].trim(),
    });
  }
  mapCache = { at: Date.now(), containers };
  return containers;
}

const realCache = new Map();

/** Resolve a path for comparison, tolerating one that no longer exists. */
async function canonical(p) {
  const s = String(p || '');
  if (!s) return '';
  if (realCache.has(s)) return realCache.get(s);
  let out = s;
  try { out = await realpath(s); } catch { /* keep the literal path */ }
  out = out.replace(/[\\/]+$/, '');
  realCache.set(s, out);
  return out;
}

/**
 * The containers one service is responsible for.
 *
 * Matched on the compose project's working directory rather than its project
 * name, because the name can be overridden three different ways (-p,
 * COMPOSE_PROJECT_NAME, `name:` in the file) while the working directory is
 * simply where the service runs. When the command does pin a project name
 * explicitly, that has to agree too — two stacks sharing a directory is
 * unusual, but silently summing both would be wrong rather than merely
 * imprecise.
 */
export async function matchContainers(spec, containers) {
  const cwd = await canonical(spec.cwd);
  const want = new Set(spec.services || []);
  const out = [];
  for (const c of containers) {
    if (!c.workingDir) continue;
    if (await canonical(c.workingDir) !== cwd) continue;
    if (spec.projectName && c.project !== spec.projectName) continue;
    if (want.size && !want.has(c.service)) continue;
    out.push(c);
  }
  return out;
}

// ─── Reading the counters ───

const cgroupPath = new Map();   // full id -> directory, or null once ruled out

async function findCgroup(id) {
  if (cgroupPath.has(id)) return cgroupPath.get(id);
  const candidates = [
    `/sys/fs/cgroup/system.slice/docker-${id}.scope`,
    `/sys/fs/cgroup/docker/${id}`,
    `/sys/fs/cgroup/system.slice/moby-${id}.scope`,
  ];
  let found = null;
  for (const dir of candidates) {
    try {
      // memory.current only exists on cgroup v2; its absence is how a v1 box
      // (or a layout not covered here) routes itself to the docker fallback.
      await access(`${dir}/memory.current`, constants.R_OK);
      found = dir;
      break;
    } catch { /* try the next */ }
  }
  cgroupPath.set(id, found);
  return found;
}

/** Memory and cumulative CPU for one container, straight from cgroup v2. */
async function readCgroup(dir) {
  try {
    const [cur, stat, cpu] = await Promise.all([
      readFile(`${dir}/memory.current`, 'utf8'),
      readFile(`${dir}/memory.stat`, 'utf8'),
      readFile(`${dir}/cpu.stat`, 'utf8'),
    ]);
    const total = parseInt(cur.trim(), 10);
    if (!Number.isFinite(total)) return null;
    // Docker subtracts inactive file cache for the figure it prints, and it is
    // right to: that is reclaimable page cache, not memory the container needs.
    const m = /^inactive_file (\d+)$/m.exec(stat);
    const inactive = m ? parseInt(m[1], 10) : 0;
    const u = /^usage_usec (\d+)$/m.exec(cpu);
    return {
      rssBytes: Math.max(0, total - (Number.isFinite(inactive) ? inactive : 0)),
      usageUsec: u ? parseInt(u[1], 10) : null,
    };
  } catch {
    return null;
  }
}

// Previous CPU counter per container, so a rate can be derived. Same rule as
// the process sampler: no previous sample, no rate — report null rather than
// invent a first-paint number.
const lastCpu = new Map();

let statsCache = { at: 0, byId: new Map() };
let statsRunning = false;

/**
 * Refresh the `docker stats` fallback in the background.
 *
 * Never awaited by a caller on the sampling path: the command takes about
 * three seconds, and blocking a 3s tick on it would make every other service's
 * numbers late to fix these ones.
 */
function refreshDockerStats(ids) {
  if (statsRunning || !ids.length) return;
  if (Date.now() - statsCache.at < STATS_TTL_MS) return;
  statsRunning = true;
  run(['stats', '--no-stream', '--format', 'json', ...ids], 30000)
    .then((r) => {
      if (!r.ok) return;
      const byId = new Map();
      for (const line of r.stdout.split('\n')) {
        const t = line.trim();
        if (!t.startsWith('{')) continue;
        let row;
        try { row = JSON.parse(t); } catch { continue; }
        const full = String(row.Container || row.ID || '');
        const rssBytes = parseDockerSize(String(row.MemUsage || '').split('/')[0]);
        const cpuPct = parseFloat(String(row.CPUPerc || '').replace('%', ''));
        if (!full) continue;
        byId.set(full, {
          rssBytes: rssBytes == null ? null : rssBytes,
          cpuPct: Number.isFinite(cpuPct) ? cpuPct : null,
        });
      }
      statsCache = { at: Date.now(), byId };
    })
    .catch(() => {})
    .finally(() => { statsRunning = false; });
}

/** "902.4MiB" -> bytes. Docker prints decimal and binary units side by side. */
export function parseDockerSize(text) {
  const m = /^\s*([\d.]+)\s*([A-Za-z]*)\s*$/.exec(String(text || ''));
  if (!m) return null;
  const n = parseFloat(m[1]);
  if (!Number.isFinite(n)) return null;
  const units = {
    b: 1, kb: 1000, mb: 1000 ** 2, gb: 1000 ** 3, tb: 1000 ** 4,
    kib: 1024, mib: 1024 ** 2, gib: 1024 ** 3, tib: 1024 ** 4,
  };
  const mult = units[m[2].toLowerCase()] ?? 1;
  return Math.round(n * mult);
}

// ─── The sampler ───

/**
 * Usage for each compose-backed service.
 *
 * @param specs [{ key, cwd, services, projectName }]
 * @returns Map key -> { rssBytes, cpuPct, containers, source }
 *
 * A service whose containers are not up yet resolves to nulls rather than to
 * the footprint of the compose client. Showing 73 MB of idle CLI for a stack
 * that is still building is the exact lie this module was written to remove.
 */
export async function sampleComposeServices(specs) {
  const result = new Map();
  if (!specs.length) return result;

  const containers = await listComposeContainers();
  const now = Date.now();
  const fallbackIds = [];

  // Only containers still present keep their CPU baseline; a restarted
  // container resets its counter, and differencing across that would produce a
  // large negative or a meaningless spike.
  const liveIds = new Set(containers.map((c) => c.id));
  for (const id of [...lastCpu.keys()]) if (!liveIds.has(id)) lastCpu.delete(id);
  for (const id of [...cgroupPath.keys()]) if (!liveIds.has(id)) cgroupPath.delete(id);

  for (const spec of specs) {
    const mine = await matchContainers(spec, containers);
    const rows = [];
    let rss = 0;
    let cpu = 0;
    let haveRss = false;
    let haveCpu = false;
    let source = 'cgroup';

    for (const c of mine) {
      const dir = isLinux ? await findCgroup(c.id) : null;
      let rowRss = null;
      let rowCpu = null;

      if (dir) {
        const s = await readCgroup(dir);
        if (s) {
          rowRss = s.rssBytes;
          const prev = lastCpu.get(c.id);
          if (s.usageUsec != null) {
            if (prev) {
              const secs = (now - prev.at) / 1000;
              // Same clamp as the process sampler: a counter that went
              // backwards is bookkeeping, not idleness, and there is no honest
              // rate to report for it.
              if (secs > 0.05) rowCpu = Math.max(0, (s.usageUsec - prev.usageUsec) / 1e6 / secs * 100);
            }
            lastCpu.set(c.id, { usageUsec: s.usageUsec, at: now });
          }
        }
      }

      if (rowRss == null) {
        // No cgroup here — Windows, cgroup v1, or a layout not recognised.
        source = 'docker-stats';
        fallbackIds.push(c.id);
        const cached = statsCache.byId.get(c.id);
        if (cached) {
          rowRss = cached.rssBytes;
          rowCpu = cached.cpuPct;
        }
      }

      if (rowRss != null) { rss += rowRss; haveRss = true; }
      if (rowCpu != null) { cpu += rowCpu; haveCpu = true; }
      rows.push({ name: c.name, service: c.service, rssBytes: rowRss, cpuPct: rowCpu });
    }

    result.set(spec.key, {
      rssBytes: haveRss ? rss : null,
      cpuPct: haveCpu ? cpu : null,
      containers: rows,
      matched: mine.length,
      source: mine.length ? source : 'none',
    });
  }

  if (fallbackIds.length) refreshDockerStats([...new Set(fallbackIds)]);
  return result;
}

/** Drop every cache. For tests, and for when docker has plainly changed. */
export function reset() {
  mapCache = { at: 0, containers: [] };
  statsCache = { at: 0, byId: new Map() };
  lastCpu.clear();
  cgroupPath.clear();
  realCache.clear();
}
