/**
 * server-update.js — is there a newer Crundi, and can this machine take it?
 *
 * The desktop app updates itself through electron-updater. A server has none
 * of that: it is a tarball someone unpacked, usually on a box they only visit
 * when something is wrong. So it asks GitHub what the latest release is, says
 * so in Settings, and can pull it down on request.
 *
 * Deliberately NOT automatic. Updating restarts the server, which kills every
 * running Claude session and terminal on it. That is a decision for whoever is
 * using it, not for a timer — so this reports, and applies only when asked.
 */

import { readFileSync, existsSync, writeFileSync, mkdirSync, statSync, unlinkSync } from 'fs';
import { join, dirname } from 'path';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { tmpdir } from 'os';
import { config } from './config.js';

const __dirname = dirname(fileURLToPath(import.meta.url));

/** The install prefix: the directory holding src/, app/ and node_modules/. */
const ROOT = join(__dirname, '..');

const REPO = process.env.CRUNDI_REPO || 'TheRealFaruhaan/crundi';
const API = `https://api.github.com/repos/${REPO}/releases/latest`;

// GitHub allows 60 unauthenticated calls an hour per IP. Six hours is far
// inside that even with several servers behind one address, and a release is
// not something you need to hear about within the minute.
const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
const MIN_RECHECK_MS = 5 * 1000;           // anti-burst floor for a forced check
// How soon to look again when a release is found mid-publish.
const RETRY_WHILE_PUBLISHING_MS = 3 * 60 * 1000;

let state = {
  checkedAt: 0,
  latest: null,          // { version, tag, url, asset }
  error: '',
  applying: false,
  log: '',
};
let timer = null;

/** This build's version, read from the package.json beside the code. */
export function currentVersion() {
  try {
    return JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf-8')).version || '0.0.0';
  } catch {
    return '0.0.0';
  }
}

/**
 * Compare two semver-ish strings. Only the numeric core matters here; a
 * prerelease suffix (1.4.0-dev.3) sorts BELOW the same release (1.4.0), which
 * is what you want when running a dev build against a published one.
 */
export function isNewer(a, b) {
  const parse = (v) => {
    const m = String(v).match(/^v?(\d+)\.(\d+)\.(\d+)(?:-(.+))?$/);
    if (!m) return null;
    return { nums: [+m[1], +m[2], +m[3]], pre: m[4] || '' };
  };
  const x = parse(a), y = parse(b);
  if (!x || !y) return false;
  for (let i = 0; i < 3; i++) {
    if (x.nums[i] !== y.nums[i]) return x.nums[i] > y.nums[i];
  }
  // Same numbers: a release beats a prerelease, and neither beats itself.
  if (!x.pre && y.pre) return true;
  if (x.pre && !y.pre) return false;
  return false;
}

/** The release asset this platform could actually install. */
function assetFor(assets) {
  if (process.platform !== 'linux') return null;
  const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
  const re = new RegExp('^crundi-server-linux-' + arch + '-.*[.]tar[.]gz$');
  return (assets || []).find(a => re.test(a.name)) || null;
}

/**
 * Why this install cannot update itself, or '' when it can.
 *
 * Being honest here matters more than being permissive: an update that half
 * applies on an unsupported layout leaves a server that will not boot, on a
 * machine nobody is sitting at.
 */
export function updateBlocker(latest) {
  if (process.versions.electron) return 'The desktop app updates itself — this is only for servers.';
  if (process.platform !== 'linux') {
    return 'Self-update is only wired up for the Linux tarball. Re-run the installer to upgrade.';
  }
  if (!existsSync(join(ROOT, 'scripts', 'install.sh'))) {
    return 'This install has no scripts/install.sh, so it was not installed from a release tarball.';
  }
  try {
    // Refuse rather than fail halfway through: an update that cannot write is
    // better caught before it has replaced anything.
    const probe = join(ROOT, '.write-probe');
    writeFileSync(probe, 'x');
    try { unlinkSync(probe); } catch { /* leaving it is harmless */ }
  } catch {
    return `${ROOT} is not writable by this process.`;
  }
  if (latest && !latest.asset) {
    // Almost always a race rather than a broken release: the main workflow
    // creates the release with the Windows installer, and the one that attaches
    // the Linux tarball finishes minutes later. A check landing in that gap used
    // to cache the gap and report it as permanent.
    return `The Linux server package for ${latest.version} has not been attached yet — `
      + 'the release is still publishing. It will clear on the next check.';
  }
  return '';
}

/** Ask GitHub. Cached; `force` bypasses the interval but not the floor. */
export async function check({ force = false } = {}) {
  const age = Date.now() - state.checkedAt;
  if (!force && age < CHECK_INTERVAL_MS && state.latest) return state.latest;
  // A forced check still gets an anti-burst floor, but a few seconds - not a
  // minute. Someone pressing "check" wants an answer, and someone about to
  // install wants the real latest.
  if (force && age < MIN_RECHECK_MS && state.latest) return state.latest;

  try {
    const res = await fetch(API, {
      headers: { 'Accept': 'application/vnd.github+json', 'User-Agent': 'crundi' },
      signal: AbortSignal.timeout(15000),
    });
    if (!res.ok) throw new Error(`GitHub returned ${res.status}`);
    const body = await res.json();
    const version = String(body.tag_name || '').replace(/^v/, '');
    state.latest = {
      version,
      tag: body.tag_name,
      url: body.html_url,
      notes: String(body.body || '').slice(0, 4000),
      asset: assetFor(body.assets),
    };
    state.error = '';
  } catch (err) {
    state.error = err.message;
  }
  state.checkedAt = Date.now();
  // A release whose platform asset is not attached yet is a passing state, so
  // do not sit on it for the full six hours. Age the timestamp so the next
  // poll retries within minutes and the answer corrects itself.
  if (state.latest && !state.latest.asset && isNewer(state.latest.version, currentVersion())) {
    state.checkedAt = Date.now() - (CHECK_INTERVAL_MS - RETRY_WHILE_PUBLISHING_MS);
  }
  return state.latest;
}

/** Everything Settings needs to draw the section. */
export function status() {
  const current = currentVersion();
  const latest = state.latest;
  const available = !!(latest && isNewer(latest.version, current));
  return {
    current,
    latest: latest ? latest.version : '',
    notesUrl: latest ? latest.url : '',
    available,
    checkedAt: state.checkedAt,
    error: state.error,
    applying: state.applying,
    log: state.log.slice(-4000),
    blocker: available ? updateBlocker(latest) : '',
  };
}

/**
 * Download the release and hand it to the installer, detached.
 *
 * Detached on purpose: the last thing the script does is restart the service,
 * which kills this process. A child in our own process group would die with it
 * halfway through, leaving a half-written install.
 */
export async function apply() {
  // ALWAYS re-check before downloading. state.latest can be hours old - the
  // interval is six hours - and installing whatever was newest last time we
  // looked means "update" can fetch a version that is no longer the latest, or
  // one already installed. Ask GitHub now, then install what it says.
  await check({ force: true }).catch(() => { /* fall back to what we have */ });

  const latest = state.latest;
  if (!latest) return { ok: false, error: 'No release information yet — check for updates first.' };
  if (!isNewer(latest.version, currentVersion())) return { ok: false, error: 'Already up to date.' };
  const blocker = updateBlocker(latest);
  if (blocker) return { ok: false, error: blocker };
  if (state.applying) return { ok: false, error: 'An update is already running.' };

  const work = join(tmpdir(), `crundi-update-${latest.version}`);
  const logFile = join(config.dataDir, 'update.log');
  const script = `#!/bin/sh
# Written by Crundi to update itself. Safe to delete.
set -e
exec >>"${logFile}" 2>&1
echo "=== $(date -Iseconds) updating to ${latest.version} ==="
rm -rf "${work}"
mkdir -p "${work}"
cd "${work}"
echo "downloading ${latest.asset.name}"
curl -fsSL -o pkg.tar.gz "${latest.asset.browser_download_url}"
tar xzf pkg.tar.gz
cd crundi
echo "installing into ${ROOT}"
# bash, NOT sh: install.sh uses 'set -o pipefail', which dash rejects
# outright - and /bin/sh is dash on Debian and Ubuntu.
bash scripts/install.sh --no-service --prefix "${ROOT}"
echo "restarting"
# The unit is called crundi either way; which manager owns it depends on how
# it was installed, so try the system one and fall back to the user one.
systemctl restart crundi 2>/dev/null || systemctl --user restart crundi 2>/dev/null || echo "could not restart automatically - restart Crundi yourself"
echo "=== done ==="
`;

  try {
    mkdirSync(config.dataDir, { recursive: true });
    const path = join(config.dataDir, 'update.sh');
    writeFileSync(path, script, { mode: 0o700 });
    const child = spawn('/bin/sh', [path], { detached: true, stdio: 'ignore' });
    child.unref();
    state.applying = true;
    state.log = `Update to ${latest.version} started. The server will restart when it finishes.`;
    console.log(`[update] Applying ${latest.version} — log: ${logFile}`);
    return { ok: true, version: latest.version, logFile };
  } catch (err) {
    return { ok: false, error: `Could not start the update: ${err.message}` };
  }
}

/** Read back what the detached updater has written, for the UI. */
export function readLog() {
  try {
    const f = join(config.dataDir, 'update.log');
    if (!existsSync(f)) return '';
    const size = statSync(f).size;
    const buf = readFileSync(f, 'utf-8');
    return size > 8000 ? buf.slice(-8000) : buf;
  } catch {
    return '';
  }
}

/** Check now, then on an interval. Failures are logged once and retried later. */
export function start() {
  if (timer) return;
  check().catch(() => { /* recorded in state.error */ });
  timer = setInterval(() => check().catch(() => {}), CHECK_INTERVAL_MS);
  if (timer.unref) timer.unref();
}

export function stop() {
  if (timer) { clearInterval(timer); timer = null; }
}
