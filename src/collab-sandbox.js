/**
 * collab-sandbox.js — the OS sandbox that confines a collaborator's shell.
 *
 * Permission rules alone do not keep Bash inside a worktree. Verified on this
 * machine: with Bash allowed, a collaborator's Claude wrote, copied and read
 * files in another folder; only paths on the deny list were stopped. Anything
 * that runs code (node, python, npm scripts) has the same reach. So the shell
 * runs inside Claude Code's own sandbox, which on Linux is bubblewrap:
 * writes are confined to the worktree, reads of other projects are refused,
 * and the network is limited to an allowlist.
 *
 * Two things on this box stand between bubblewrap and working, both found by
 * running it, not by reading about it:
 *
 *   1. Crundi runs with the ambient capability to bind low ports, and every
 *      child inherits it. bwrap refuses to run with capabilities it did not
 *      get from setuid: "Unexpected capabilities but not setuid". So a
 *      collaborator's Claude is launched through `setpriv` with inheritable
 *      and ambient capabilities cleared.
 *   2. Ubuntu 24.04 restricts unprivileged user namespaces through AppArmor
 *      (kernel.apparmor_restrict_unprivileged_userns=1): "setting up uid map:
 *      Permission denied". A profile for /usr/bin/bwrap alone lifts that.
 *
 * Collaborator chats refuse to start unless the sandbox actually works, and
 * the settings also carry failIfUnavailable, so the CLI exits rather than run
 * the shell unsandboxed if something changes underneath.
 */
import { spawn, spawnSync } from 'child_process';
import { existsSync, readFileSync } from 'fs';
import { join, resolve as resolvePath, isAbsolute } from 'path';

export const APPARMOR_PROFILE_PATH = '/etc/apparmor.d/bwrap';
export const APPARMOR_PROFILE = [
  '# Added by Crundi for collaborator sandboxing (Claude Code sandbox uses bubblewrap).',
  '# Ubuntu restricts unprivileged user namespaces; this lets bwrap alone create them.',
  'abi <abi/4.0>,',
  'include <tunables/global>',
  '',
  'profile bwrap /usr/bin/bwrap flags=(unconfined) {',
  '  userns,',
  '  include if exists <local/bwrap>',
  '}',
  '',
].join('\n');

const CAP_DROP = ['--inh-caps=-all', '--ambient-caps=-all'];
const STATUS_TTL_MS = 60_000;
let cached = null;

function which(bin) {
  const r = spawnSync('sh', ['-c', `command -v ${bin}`], { encoding: 'utf8', timeout: 5000 });
  return r.status === 0 ? String(r.stdout || '').trim() : '';
}

function canSudo() {
  const r = spawnSync('sudo', ['-n', 'true'], { timeout: 5000 });
  return r.status === 0;
}

/**
 * How to launch a collaborator's Claude with capabilities dropped.
 * @returns {{bin:string, prefix:string[]}|null} null when not needed / not possible
 */
export function capDropLauncher() {
  if (process.platform !== 'linux') return null;
  const setpriv = which('setpriv');
  return setpriv ? { bin: setpriv, prefix: [...CAP_DROP] } : null;
}

function computeStatus() {
  if (process.platform !== 'linux') {
    return {
      ok: false, supported: false, works: false,
      problems: [`Collaborator sandboxing needs Linux (bubblewrap). This machine runs ${process.platform}.`],
      canAutoFix: false,
    };
  }
  const bwrap = which('bwrap');
  const socat = which('socat');
  const setpriv = which('setpriv');
  const problems = [];
  if (!bwrap) problems.push('bubblewrap is not installed');
  if (!socat) problems.push('socat is not installed');
  if (!setpriv) problems.push('setpriv (util-linux) is not installed');

  let apparmorRestricted = false;
  try {
    apparmorRestricted = readFileSync('/proc/sys/kernel/apparmor_restrict_unprivileged_userns', 'utf8').trim() === '1';
  } catch { /* not an AppArmor kernel */ }
  const profileInstalled = existsSync(APPARMOR_PROFILE_PATH);

  // The only test that means anything is starting one, exactly the way a
  // collaborator's chat will: capabilities dropped, every namespace unshared.
  let works = false;
  let testError = '';
  if (bwrap && setpriv) {
    const r = spawnSync(setpriv, [...CAP_DROP, bwrap, '--ro-bind', '/', '/', '--dev', '/dev', '--proc', '/proc', '--unshare-all', 'true'],
      { encoding: 'utf8', timeout: 10_000 });
    works = r.status === 0;
    if (!works) {
      testError = String(r.stderr || (r.error && r.error.message) || `exit ${r.status}`).trim().slice(0, 300);
      if (apparmorRestricted && !profileInstalled) {
        problems.push('Ubuntu blocks the user namespaces bubblewrap needs (no AppArmor profile for bwrap)');
      } else {
        problems.push(`bubblewrap could not start a sandbox: ${testError}`);
      }
    }
  }
  const ok = works && problems.length === 0;
  return {
    ok, supported: true, works, bwrap: !!bwrap, socat: !!socat, setpriv: !!setpriv,
    apparmorRestricted, profileInstalled, problems, testError,
    canAutoFix: !ok && canSudo(),
  };
}

/** Is the collaborator sandbox usable on this machine? Cached for a minute. */
export function sandboxStatus({ force = false } = {}) {
  if (!force && cached && Date.now() - cached.at < STATUS_TTL_MS) return cached.value;
  const value = computeStatus();
  cached = { at: Date.now(), value };
  return value;
}

/** Run a command without blocking the event loop — apt can take minutes. */
function runAsync(cmd, args, { input = null, timeoutMs = 60_000 } = {}) {
  return new Promise((resolveRun) => {
    let out = '';
    let err = '';
    let child;
    try { child = spawn(cmd, args, { stdio: ['pipe', 'pipe', 'pipe'] }); }
    catch (e) { resolveRun({ status: -1, stdout: '', stderr: e.message }); return; }
    const timer = setTimeout(() => { try { child.kill(); } catch { /* gone */ } }, timeoutMs);
    child.stdout.on('data', (d) => { out += d; });
    child.stderr.on('data', (d) => { err += d; });
    child.on('error', (e) => { err += e.message; });
    child.on('close', (code) => { clearTimeout(timer); resolveRun({ status: code, stdout: out, stderr: err }); });
    if (input !== null) child.stdin.end(input); else child.stdin.end();
  });
}

let setupRunning = null;

/**
 * Install what the sandbox needs. Uses sudo, so only ever run on the owner's
 * explicit request from the Info tab. Concurrent calls share one run.
 */
export function setupSandbox() {
  if (!setupRunning) setupRunning = doSetup().finally(() => { setupRunning = null; });
  return setupRunning;
}

async function doSetup() {
  const log = [];
  let st = sandboxStatus({ force: true });
  if (st.ok) return { ok: true, status: st, log: ['Already set up.'] };
  if (!st.supported) return { ok: false, status: st, log, error: st.problems.join('; ') };
  if (!canSudo()) {
    return {
      ok: false, status: st, log,
      error: 'Setting this up needs passwordless sudo. On the server run: sudo apt-get install -y bubblewrap socat',
    };
  }
  if (!st.bwrap || !st.socat) {
    log.push('Installed bubblewrap and socat.');
    const r = await runAsync('sudo', ['-n', 'env', 'DEBIAN_FRONTEND=noninteractive', 'apt-get', 'install', '-y', 'bubblewrap', 'socat'],
      { timeoutMs: 300_000 });
    if (r.status !== 0) {
      const err = String(r.stderr || '').trim().split('\n').slice(-3).join(' ');
      return { ok: false, status: sandboxStatus({ force: true }), log: [], error: `apt-get failed: ${err}` };
    }
    st = sandboxStatus({ force: true });
  }
  if (!st.works && st.apparmorRestricted) {
    const w = await runAsync('sudo', ['-n', 'tee', APPARMOR_PROFILE_PATH], { input: APPARMOR_PROFILE, timeoutMs: 10_000 });
    if (w.status !== 0) return { ok: false, status: st, log, error: `Could not write the AppArmor profile: ${String(w.stderr || '').trim()}` };
    const l = await runAsync('sudo', ['-n', 'apparmor_parser', '-r', APPARMOR_PROFILE_PATH], { timeoutMs: 30_000 });
    if (l.status !== 0) return { ok: false, status: st, log, error: `Could not load the AppArmor profile: ${String(l.stderr || '').trim()}` };
    log.push(`Added an AppArmor profile for bwrap at ${APPARMOR_PROFILE_PATH}.`);
    st = sandboxStatus({ force: true });
  }
  return { ok: st.ok, status: st, log, error: st.ok ? '' : st.problems.join('; ') };
}

/**
 * A worktree keeps its git data in the main repository: `<root>/.git` is a
 * file pointing at `<repo>/.git/worktrees/<name>`, which names the shared
 * common dir. The sandbox has to let git reach exactly those, and no more.
 *
 * @returns {{gitDir:string, commonDir:string}|null}
 */
export function worktreeGitPaths(root) {
  try {
    const dotGit = join(root, '.git');
    const raw = readFileSync(dotGit, 'utf8');
    const m = raw.match(/^gitdir:\s*(.+)\s*$/m);
    if (!m) return null;
    const gitDir = isAbsolute(m[1]) ? m[1] : resolvePath(root, m[1]);
    let commonDir = resolvePath(gitDir, '..', '..');
    try {
      const c = readFileSync(join(gitDir, 'commondir'), 'utf8').trim();
      if (c) commonDir = isAbsolute(c) ? c : resolvePath(gitDir, c);
    } catch { /* older git: two levels up */ }
    return { gitDir, commonDir };
  } catch { return null; }
}

/**
 * Launch arguments for a service a collaborator registered.
 *
 * A service runs as the server user, outside their chat's sandbox — and
 * approving `npm run dev` really approves whatever their package.json and
 * dependencies run. So it gets a sandbox of its own:
 *
 *   - everything read-only; their worktree and package cache writable; a
 *     private /tmp
 *   - the owner's projects, every worktree and cache, Crundi's data and the
 *     usual credential folders replaced by empty directories
 *   - its own pid namespace, so it cannot read other processes' environments
 *     (Crundi's included) through /proc
 *   - the NETWORK IS SHARED on purpose: a dev server must bind a local port
 *     that forwards and tunnels can reach
 *
 * @returns {{bin:string, args:string[]}|null} null when bwrap/setpriv are missing
 */
export function serviceSandboxArgs({ worktree, cacheDir = '', hide = [], cwd, command }) {
  const setpriv = which('setpriv');
  const bwrap = which('bwrap');
  if (!setpriv || !bwrap || !worktree || !cwd) return null;
  const args = [...CAP_DROP, bwrap,
    '--ro-bind', '/', '/',
    '--dev', '/dev',
    '--unshare-pid', '--unshare-ipc', '--unshare-uts',
    '--proc', '/proc',
    '--tmpfs', '/tmp',
    '--die-with-parent',
    '--new-session',
  ];
  for (const d of hide) if (d && existsSync(d)) args.push('--tmpfs', d);
  // Their own folders come back, writable, after anything above them was hidden.
  args.push('--bind', worktree, worktree);
  if (cacheDir && existsSync(cacheDir)) args.push('--bind', cacheDir, cacheDir);
  args.push('--chdir', cwd, 'sh', '-c', command);
  return { bin: setpriv, args };
}

/** For tests. */
export function _resetSandboxStatusCache() { cached = null; }
