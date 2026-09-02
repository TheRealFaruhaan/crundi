/**
 * claude-update.js — check and update the Claude Code CLI.
 *
 * Claude Code updates itself, but only when it can write to where it is
 * installed. On this server it lives at /usr/lib/node_modules as root while
 * Crundi runs as an unprivileged user, so every attempt fails with
 * "Auto-update failed: no write permission" and the CLI silently falls further
 * behind. This surfaces that in Settings and offers to do it properly.
 *
 * ─── The tested version ───
 *
 * Claude Code is the engine every session runs on. A regression there does not
 * break one feature, it breaks every chat and terminal at once, and the cause
 * is not obvious from inside Crundi. So package.json pins the version this
 * build was actually exercised against, and updating PAST it is a separate,
 * explicit decision rather than something a button does on your behalf.
 *
 * That pin is a statement about what was tested, not a claim that later
 * versions are broken. It is almost always fine. It just should not be
 * discovered to be otherwise at 2am with no idea what changed.
 */

import { execFile } from 'child_process';
import { readFileSync, accessSync, constants } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { isNewer } from './server-update.js';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const PKG = '@anthropic-ai/claude-code';
const CHECK_TTL_MS = 60 * 60 * 1000;   // an hour; npm is not going anywhere

let cache = { latest: '', checkedAt: 0, error: '' };
let applying = false;
let lastResult = '';

function run(cmd, args, timeoutMs = 300000) {
  return new Promise((resolve) => {
    execFile(cmd, args, { timeout: timeoutMs, maxBuffer: 8 * 1024 * 1024, windowsHide: true },
      (err, stdout, stderr) => resolve({
        ok: !err,
        stdout: String(stdout || ''),
        stderr: String(stderr || ''),
        error: err ? (err.code === 'ENOENT' ? 'not found' : String(err.message || err)) : '',
      }));
  });
}

/** The version this Crundi build was exercised against. */
export function testedVersion() {
  try {
    return JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf-8')).claudeTestedVersion || '';
  } catch {
    return '';
  }
}

/** The installed CLI's version, or '' if it is not on the PATH. */
export async function installedVersion() {
  const r = await run('claude', ['--version'], 20000);
  if (!r.ok) return '';
  // "2.1.250 (Claude Code)"
  return (/(\d+\.\d+\.\d+)/.exec(r.stdout) || [])[1] || '';
}

/** Where npm would install a global package. */
async function npmPrefix() {
  const r = await run('npm', ['config', 'get', 'prefix'], 20000);
  return r.ok ? r.stdout.trim() : '';
}

/**
 * Can this process update the CLI, and does it need root to do it?
 *
 * Checked rather than assumed: a user-local install needs no privilege at all,
 * and offering a sudo button there would be asking for a password to do
 * something that did not require one.
 */
export async function updateCapability() {
  const prefix = await npmPrefix();
  if (!prefix) return { can: false, needsSudo: false, reason: 'npm is not available' };
  const dir = join(prefix, 'lib', 'node_modules');
  try {
    accessSync(dir, constants.W_OK);
    return { can: true, needsSudo: false, prefix };
  } catch { /* needs elevation */ }
  const sudo = await run('sudo', ['-n', 'true'], 10000);
  if (sudo.ok) return { can: true, needsSudo: true, prefix };
  return {
    can: false, needsSudo: true, prefix,
    reason: `Claude Code is installed at ${dir}, which needs root to change, and passwordless sudo is not available.`,
  };
}

/** Ask npm for the latest published version. Cached for an hour. */
export async function check({ force = false } = {}) {
  const fresh = Date.now() - cache.checkedAt < CHECK_TTL_MS;
  if (!force && fresh && cache.latest) return cache;
  const r = await run('npm', ['view', PKG, 'version'], 45000);
  if (!r.ok) {
    cache = { ...cache, checkedAt: Date.now(), error: r.stderr.trim().split('\n')[0] || r.error || 'npm view failed' };
    return cache;
  }
  cache = { latest: r.stdout.trim(), checkedAt: Date.now(), error: '' };
  return cache;
}

/** Everything Settings needs to draw the row. */
export async function status({ force = false } = {}) {
  const [installed, cap] = await Promise.all([installedVersion(), updateCapability()]);
  const tested = testedVersion();
  const c = await check({ force });
  const latest = c.latest || '';
  const available = !!(latest && installed && isNewer(latest, installed));
  return {
    installed,
    latest,
    tested,
    available,
    // True when the newest release is past what this build was tested with.
    beyondTested: !!(tested && latest && isNewer(latest, tested)),
    // True when the version already installed is past it — worth saying so
    // plainly rather than pretending the pin still holds.
    runningBeyondTested: !!(tested && installed && isNewer(installed, tested)),
    canUpdate: cap.can,
    needsSudo: !!cap.needsSudo,
    blocker: cap.reason || '',
    applying,
    lastResult,
    checkedAt: c.checkedAt || 0,
    error: c.error || '',
  };
}

/**
 * Install a version of the CLI.
 *
 * Refuses to go past the tested version unless the caller says so explicitly,
 * so "Update" and "update to something nobody here has run" stay two different
 * decisions.
 */
export async function apply({ version = '', allowBeyondTested = false } = {}) {
  if (applying) return { ok: false, error: 'An update is already running' };

  const cap = await updateCapability();
  if (!cap.can) return { ok: false, error: cap.reason || 'Cannot update Claude Code here' };

  const c = await check({ force: true });
  const target = version || c.latest;
  if (!target) return { ok: false, error: c.error || 'Could not work out which version to install' };
  if (!/^\d+\.\d+\.\d+(?:-[\w.]+)?$/.test(target)) {
    return { ok: false, error: `"${target}" is not a version number` };
  }

  const tested = testedVersion();
  if (tested && isNewer(target, tested) && !allowBeyondTested) {
    return {
      ok: false,
      needsConfirm: true,
      error: `${target} is newer than ${tested}, the version this Crundi build was tested against. Confirm to install it anyway.`,
    };
  }

  applying = true;
  lastResult = '';
  try {
    const args = ['install', '-g', `${PKG}@${target}`, '--no-audit', '--no-fund'];
    const r = cap.needsSudo
      ? await run('sudo', ['-n', 'npm', ...args], 600000)
      : await run('npm', args, 600000);
    if (!r.ok) {
      lastResult = r.stderr.trim().split('\n').slice(-2).join(' ') || r.error || 'npm install failed';
      return { ok: false, error: lastResult };
    }
    // Trust the CLI over npm's exit code: what matters is what `claude
    // --version` now reports, not that a command exited zero.
    const now = await installedVersion();
    if (now !== target) {
      lastResult = `npm reported success but claude --version says ${now || 'nothing'}`;
      return { ok: false, error: lastResult, installed: now };
    }
    lastResult = `Updated to ${now}`;
    return { ok: true, installed: now };
  } finally {
    applying = false;
  }
}
