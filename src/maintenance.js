/**
 * Disk maintenance — reclaim space without surprises.
 *
 * Every task is a fixed entry in TASKS below. Nothing from the request ever
 * reaches a command line: the client sends an id, the id is looked up in this
 * map, and the command it names is run with execFile and an argument array. An
 * unknown id is simply "no such task".
 *
 * Each task measures itself BEFORE it runs, so a button never asks to be
 * pressed on faith, and measures itself again after, so what it reports having
 * freed is observed rather than claimed.
 *
 * `docker system prune` is kept apart from the rest and marked danger: it
 * removes every stopped container, and on a shared box those belong to other
 * people's projects — a stack someone intends to `docker start` again tomorrow
 * is not garbage. So that one names the containers it is about to destroy and
 * asks again before doing it, while the everyday tasks stay one press.
 */

import { execFile } from 'child_process';
import os from 'os';
import { join } from 'path';
import { existsSync } from 'fs';

const HOME = os.homedir();

const isWindows = process.platform === 'win32';
const isLinux = process.platform === 'linux';

/** execFile as a promise that resolves rather than throws. */
function run(cmd, args, timeoutMs = 300000) {
  return new Promise((resolve) => {
    execFile(cmd, args, { timeout: timeoutMs, maxBuffer: 8 * 1024 * 1024, windowsHide: true },
      (err, stdout, stderr) => {
        resolve({
          ok: !err,
          stdout: String(stdout || ''),
          stderr: String(stderr || ''),
          error: err ? (err.code === 'ENOENT' ? 'not installed' : String(err.message || err)) : '',
        });
      });
  });
}

/**
 * Docker prints sizes as human strings ("8.469GB", "522.6MB (6%)", "0B").
 * SI and binary suffixes both appear across versions, so both are handled.
 */
export function parseSize(text) {
  const m = /([\d.]+)\s*([KMGTP]?i?)B/i.exec(String(text || ''));
  if (!m) return 0;
  const n = parseFloat(m[1]);
  if (!Number.isFinite(n)) return 0;
  const unit = m[2].toUpperCase();
  const binary = unit.endsWith('I');
  const base = binary ? 1024 : 1000;
  const exp = { '': 0, K: 1, KI: 1, M: 2, MI: 2, G: 3, GI: 3, T: 4, TI: 4, P: 5, PI: 5 }[unit] ?? 0;
  return Math.round(n * Math.pow(base, exp));
}

/** Reclaimable bytes per `docker system df` row type. */
async function dockerReclaimable(type) {
  const r = await run('docker', ['system', 'df', '--format', 'json'], 20000);
  if (!r.ok) return null;
  for (const line of r.stdout.split('\n')) {
    const t = line.trim();
    if (!t.startsWith('{')) continue;
    try {
      const row = JSON.parse(t);
      if (row.Type === type) return parseSize(row.Reclaimable);
    } catch { /* a malformed row is not a reason to fail the whole probe */ }
  }
  return null;
}

/** Total bytes of a directory. Returns null when it cannot be measured. */
async function dirSize(path, sudo) {
  if (isWindows) {
    const ps = 'if (Test-Path -LiteralPath "' + path + '") { (Get-ChildItem -LiteralPath "' + path
      + '" -Recurse -File -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum } else { 0 }';
    const r = await run('powershell', ['-NoProfile', '-Command', ps], 60000);
    if (!r.ok) return null;
    const n = parseInt(String(r.stdout).trim(), 10);
    return Number.isFinite(n) ? n : 0;
  }
  const args = sudo ? ['-n', 'du', '-sb', path] : ['-sb', path];
  const r = await run(sudo ? 'sudo' : 'du', args, 60000);
  if (!r.ok) return null;
  const n = parseInt(String(r.stdout).split(/\s+/)[0], 10);
  return Number.isFinite(n) ? n : null;
}

const npmCacheDir = join(os.homedir(), isWindows ? 'AppData/Local/npm-cache' : '.npm');

/**
 * The stopped containers a system prune would remove, by name.
 *
 * "This deletes stopped containers" is not something anyone can act on. The
 * names are, because that is how you recognise someone else's database sitting
 * there waiting to be started again.
 */
async function stoppedContainers() {
  const r = await run('docker', ['ps', '-a', '--filter', 'status=exited', '--filter', 'status=created',
    '--format', '{{.Names}}'], 20000);
  if (!r.ok) return [];
  return r.stdout.split('\n').map((x) => x.trim()).filter(Boolean);
}

/**
 * Run the first candidate that exists. A tool can be installed but absent from
 * the server's PATH — go lives in /usr/local/go/bin, which systemd's PATH does
 * not include — so a bare name alone would report "not installed" on a machine
 * that plainly has it.
 */
async function runFirst(candidates, args, timeoutMs) {
  let last = { ok: false, error: 'not installed' };
  for (const bin of candidates) {
    if (bin.includes('/') && !existsSync(bin)) continue;
    const r = await run(bin, args, timeoutMs);
    if (r.ok || r.error !== 'not installed') return r;
    last = r;
  }
  return last;
}

/**
 * Delete a cache directory outright, for tools with no clean command.
 *
 * Guarded to paths under the home directory: these are fixed strings from the
 * catalogue below and never come from a request, but a bad edit to this file
 * should not be able to point `rm -rf` at the root of the machine.
 */
async function removeDir(dir) {
  if (!dir || !dir.startsWith(HOME + '/') || dir.length <= HOME.length + 1) {
    return { ok: false, error: 'Refusing to remove a path outside the home directory' };
  }
  if (!existsSync(dir)) return { ok: true, stdout: '', stderr: '', error: '' };
  if (isWindows) return run('powershell', ['-NoProfile', '-Command', 'Remove-Item -LiteralPath "' + dir + '" -Recurse -Force -ErrorAction SilentlyContinue'], 300000);
  // Go's module cache is deliberately read-only; rm alone fails on it.
  await run('chmod', ['-R', 'u+w', dir], 120000);
  return run('rm', ['-rf', dir], 300000);
}

/**
 * A package-manager cache: measured as a directory, cleared with the tool's own
 * command when it has one, and by deleting the directory when it does not.
 * Falling back matters because a cache often outlives the tool that made it.
 */
function cacheTask(spec) {
  return {
    id: spec.id,
    label: spec.label,
    description: spec.description,
    windows: true,
    probe: () => (existsSync(spec.dir) ? dirSize(spec.dir, false) : null),
    exec: async () => {
      if (spec.cli) {
        const r = await runFirst(spec.cli.bins, spec.cli.args, 300000);
        if (r.ok) return r;
      }
      return removeDir(spec.dir);
    },
  };
}

/**
 * The whole catalogue. `probe` reports reclaimable bytes (null = unavailable,
 * which is how a task hides itself on a machine that lacks it).
 */
const TASKS = [
  {
    id: 'docker-build-cache',
    label: 'Docker build cache',
    // Say what it costs, not just what it does — the honest trade is that the
    // next build is slower, and nothing else changes.
    description: 'Layers kept to make rebuilds fast. Removing them frees the most space; the next build is slower.',
    probe: () => dockerReclaimable('Build Cache'),
    exec: () => run('docker', ['builder', 'prune', '-f']),
  },
  {
    id: 'docker-images',
    label: 'Unused Docker images',
    description: 'Image layers no longer referenced by a tag or a container. Running containers are untouched.',
    probe: () => dockerReclaimable('Images'),
    exec: () => run('docker', ['image', 'prune', '-f']),
  },
  {
    id: 'docker-system-prune',
    label: 'Stopped containers and networks',
    description: 'A full docker system prune. Removes every stopped container on this machine, other projects\' included, plus unused networks. Running containers are untouched.',
    danger: true,
    // Counted as containers only, deliberately. A system prune also sweeps
    // images and build cache, but those are the two rows above — adding them
    // here would bill the same bytes twice and promise a total that does not
    // exist. What this row uniquely does is remove containers, and the honest
    // answer is that it frees almost no space; the reason to press it is
    // tidiness, which is why it lists the containers rather than a big number.
    probe: () => dockerReclaimable('Containers'),
    details: stoppedContainers,
    exec: () => run('docker', ['system', 'prune', '-f']),
  },
  {
    id: 'npm-cache',
    label: 'npm cache',
    description: 'Packages npm keeps so reinstalls skip the download. It refills itself as needed.',
    probe: () => dirSize(npmCacheDir, false),
    exec: () => run('npm', ['cache', 'clean', '--force'], 120000),
    windows: true,
  },
  cacheTask({
    id: 'go-build', label: 'Go build cache', dir: join(HOME, '.cache/go-build'),
    description: 'Compiled package objects Go keeps to make rebuilds fast. Rebuilt automatically.',
    cli: { bins: ['go', '/usr/local/go/bin/go', join(HOME, 'go/bin/go')], args: ['clean', '-cache'] },
  }),
  cacheTask({
    id: 'go-mod', label: 'Go module cache', dir: join(HOME, 'go/pkg/mod'),
    description: 'Downloaded Go modules. Re-fetched on the next build, so this needs the network back.',
    cli: { bins: ['go', '/usr/local/go/bin/go', join(HOME, 'go/bin/go')], args: ['clean', '-modcache'] },
  }),
  cacheTask({
    id: 'pip-cache', label: 'pip cache', dir: join(HOME, '.cache/pip'),
    description: 'Python wheels pip keeps so reinstalls skip the download.',
    cli: { bins: ['pip3', 'pip'], args: ['cache', 'purge'] },
  }),
  cacheTask({
    id: 'uv-cache', label: 'uv cache', dir: join(HOME, '.cache/uv'),
    description: 'Python packages cached by uv.',
    cli: { bins: ['uv'], args: ['cache', 'clean'] },
  }),
  cacheTask({
    id: 'cargo-cache', label: 'Cargo registry', dir: join(HOME, '.cargo/registry'),
    description: 'Rust crates downloaded for builds. Re-fetched when next needed.',
  }),
  cacheTask({
    id: 'yarn-cache', label: 'Yarn cache', dir: join(HOME, '.cache/yarn'),
    description: 'Packages Yarn keeps for faster installs.',
    cli: { bins: ['yarn'], args: ['cache', 'clean'] },
  }),
  cacheTask({
    id: 'maven-cache', label: 'Maven repository', dir: join(HOME, '.m2/repository'),
    description: 'Java artifacts Maven has downloaded. Re-fetched on the next build.',
  }),
  cacheTask({
    id: 'gradle-cache', label: 'Gradle caches', dir: join(HOME, '.gradle/caches'),
    description: 'Dependencies and build output Gradle keeps between builds.',
  }),
  cacheTask({
    id: 'composer-cache', label: 'Composer cache', dir: join(HOME, '.cache/composer'),
    description: 'PHP packages Composer keeps for faster installs.',
    cli: { bins: ['composer'], args: ['clear-cache'] },
  }),
  cacheTask({
    id: 'playwright-browsers', label: 'Playwright browsers', dir: join(HOME, '.cache/ms-playwright'),
    description: 'Browser builds Playwright downloaded. Anything using Playwright must download them again before it will run.',
  }),
  cacheTask({
    id: 'puppeteer-browsers', label: 'Puppeteer browsers', dir: join(HOME, '.cache/puppeteer'),
    description: 'Chrome builds Puppeteer downloaded. Anything using Puppeteer must download them again before it will run.',
  }),
  cacheTask({
    id: 'huggingface-cache', label: 'Hugging Face models', dir: join(HOME, '.cache/huggingface'),
    description: 'Downloaded model weights. These are large and slow to fetch again.',
  }),
  {
    id: 'apt-cache',
    label: 'Package archives',
    description: 'Installer files apt already unpacked and no longer needs.',
    linuxOnly: true,
    elevated: true,
    probe: () => dirSize('/var/cache/apt/archives', true),
    exec: () => run('sudo', ['-n', 'apt-get', 'clean'], 120000),
  },
];

function applicable(t) {
  if (t.linuxOnly && !isLinux) return false;
  if (isWindows && !t.windows && !t.id.startsWith('docker')) return false;
  return true;
}

/**
 * Every task with its current reclaimable size.
 * A task whose probe fails (docker absent, no sudo) comes back available:false
 * with a reason, rather than as a button that fails when pressed.
 */
export async function listTasks() {
  const out = [];
  await Promise.all(TASKS.filter(applicable).map(async (t) => {
    // A probe of null means "not on this machine" — no docker, no Go, no pip.
    // Those are dropped below rather than shown as a column of dead buttons.
    let bytes = null;
    try { bytes = await t.probe(); } catch { bytes = null; }
    let details = [];
    if (t.details && bytes != null) {
      try { details = await t.details(); } catch { details = []; }
    }
    out.push({
      id: t.id,
      label: t.label,
      description: t.description,
      elevated: !!t.elevated,
      danger: !!t.danger,
      details,
      available: bytes != null,
      bytes: bytes == null ? 0 : bytes,
      order: TASKS.indexOf(t),
    });
  }));
  out.sort((a, b) => a.order - b.order);
  return out.filter((t) => t.available).map(({ order, ...rest }) => rest);
}

/**
 * Run one task and report what it actually freed, by measuring again rather
 * than trusting the tool's own summary line.
 */
export async function runTask(id) {
  const task = TASKS.find((t) => t.id === id && applicable(t));
  if (!task) return { ok: false, error: 'No such task' };

  let before = null;
  try { before = await task.probe(); } catch { /* still worth running */ }

  const res = await task.exec();
  if (!res.ok) {
    return { ok: false, error: res.error || res.stderr.trim().split('\n')[0] || 'The command failed' };
  }

  let after = null;
  try { after = await task.probe(); } catch { /* freed stays unknown */ }

  const freed = (before != null && after != null) ? Math.max(0, before - after) : null;
  return { ok: true, freed, bytes: after == null ? 0 : after };
}
