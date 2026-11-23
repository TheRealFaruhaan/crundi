// Replacing Electron (PARITY-ELECTRON.md, package/CONTRACT.md): local mode
// end to end. The client is the all-in-one here (CRUNDI_SERVER_DIR points at
// this repo's server, CRUNDI_NODE at this Node) with a config home of the
// run's own, holding an Electron install's settings to take over:
//   1. first run: Electron's settings imported once (device id, drawer
//      history, update settings, client.json), the setup wizard (seven
//      steps, the alert, Skip, Back; keys typed and pressed), "Restore from a
//      backup" writing setup:save's .env and starting the server;
//   2. the next start: no sign-in method yet, the server's setup page (as
//      Electron's page shows), a password set; the bundled server started as startBot does (its
//      environment: DATA_DIR, DOTENV_PATH, CRUNDI_DEVICE_ID, CRUNDI_DEV, no
//      NODE_OPTIONS, the IPC channel, the PATH), signed in with its key, the
//      tray's status, Start and Stop; Stop (shutdown, the tree gone), Start
//      from the status screen, a crash (Error, "Process crashed"), Restart,
//      the server screen's local / remote choice, this PC as a worker
//      (join-self, kind native); Quit leaves no process behind.
// The server runs in dev mode on a port of the run's (CRUNDI_DEV, DEV_WEB_PORT),
// with no homepool, an empty bot token and its data under the run's folder.
//
//   native-v2/tools/build.sh
//   LOCAL_PORT=4802 CRUNDI_TEST_PORT=4803 NATIVE_BIN=$HOME/.cache/crundi-native-target/debug/crundi-v2 \
//     dbus-run-session -- xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/electron/local.mjs   (or ONLY=local node native-v2/test/electron.mjs)

import net from 'node:net';
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync, readdirSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { check, counts, driver, sleep } from '../lib.mjs';
import { totpNow } from '../../../src/auth-password.js';

const REPO = fileURLToPath(new URL('../../../', import.meta.url));
const OWN_BIN = fileURLToPath(new URL('../../.bin/debug/crundi-v2', import.meta.url));
// PACKAGE_DIR=<an install folder> (package/stage.mjs's layout, e.g. ~/.cache/crundi-pack/stage-full):
// its own client, which finds server/, node/ and crundi-worker beside itself
// (no overrides), and a stub Claude for its chats (tools/claude-stub.mjs).
const PKG = process.env.PACKAGE_DIR || '';
const BIN = PKG ? join(PKG, 'crundi') : process.env.NATIVE_BIN || (existsSync(OWN_BIN) ? OWN_BIN : '');
const SERVER_DIR = PKG ? join(PKG, 'server') : REPO;
const SERVER_PORT = Number(process.env.LOCAL_PORT || 4802);
const DRIVER_PORT = Number(process.env.CRUNDI_TEST_PORT || 4803);
if (!BIN) { console.error('Set NATIVE_BIN (or build: native-v2/tools/build.sh)'); process.exit(2); }
if (!process.env.DISPLAY || process.env.DISPLAY === ':0') { console.error('Run under xvfb-run (DISPLAY is not a virtual display).'); process.exit(2); }
if (/^unix:path=\/run\/user\/\d+\/bus$/.test(process.env.DBUS_SESSION_BUS_ADDRESS || '')) { console.error('Run under dbus-run-session (the tray would reach the desktop\'s own bus).'); process.exit(2); }
for (const p of [SERVER_PORT, DRIVER_PORT]) {
  if ([22, 53, 80, 443, 8888].includes(p) || (p >= 7480 && p <= 7599)) { console.error(`port ${p} is not the run's`); process.exit(2); }
}
const free = (port) => new Promise((r) => { const s = net.createServer().once('error', () => r(false)).once('listening', () => s.close(() => r(true))).listen(port, '127.0.0.1'); });
for (const p of [SERVER_PORT, DRIVER_PORT]) if (!(await free(p))) { console.error(`port ${p} is taken: pick another (LOCAL_PORT, CRUNDI_TEST_PORT)`); process.exit(2); }
if (!PKG && !existsSync(join(REPO, 'node_modules'))) { console.error('The server needs its node_modules (npm ci --omit=dev in the repo).'); process.exit(2); }

const ROOT = mkdtempSync(join(tmpdir(), 'crundi-v2-electron-'));
const XDG = join(ROOT, 'xdg');
const CFG = join(ROOT, 'native');
const ELECTRON = join(XDG, 'crundi-desktop');
const OUT = process.env.DRIVE_OUT || join(tmpdir(), 'crundi-v2-electron-out');
mkdirSync(OUT, { recursive: true });
// The worker program the package bundles (worker/, built by package/check.sh), if built here:
// it then runs for real (its lock and config under the run's XDG folder, its work folder the run's).
const PACKED = join(homedir(), '.cache', 'crundi-pack', 'worker-target', 'release', 'crundi-worker');
const WORKER = PKG ? join(PKG, 'crundi-worker') : process.env.WORKER_BIN || (existsSync(PACKED) ? PACKED : '');
const DEVICE = 'pc-electrontest' + Math.random().toString(36).slice(2, 8);

// An Electron all-in-one's userData, as v1.21 leaves it (no .env yet: a first run).
mkdirSync(join(ELECTRON, 'data'), { recursive: true });
writeFileSync(join(ELECTRON, 'homepool-node.json'), JSON.stringify({ deviceId: DEVICE, enabled: false }));
writeFileSync(join(ELECTRON, 'drawer-history.json'), JSON.stringify([{ url: 'https://example.com/', title: 'Example', at: Date.now(), visits: 3 }]));
writeFileSync(join(ELECTRON, 'update-config.json'), JSON.stringify({ autoUpdate: true, channel: 'production' }));
writeFileSync(join(ELECTRON, 'data', 'client.json'), JSON.stringify({ mode: 'local', serverUrl: '', voiceWake: false }));

// Only what the client needs from this shell: nothing of the caller's Crundi or Claude session.
const keep = ['PATH', 'HOME', 'USER', 'LOGNAME', 'SHELL', 'LANG', 'DISPLAY', 'XAUTHORITY', 'DBUS_SESSION_BUS_ADDRESS', 'VK_ICD_FILENAMES', 'TMPDIR'];
const baseEnv = Object.fromEntries(keep.filter((k) => process.env[k]).map((k) => [k, process.env[k]]));
const env = {
  ...baseEnv,
  CRUNDI_TEST_PORT: String(DRIVER_PORT), CRUNDI_CONFIG_DIR: CFG, XDG_CONFIG_HOME: XDG,
  CRUNDI_SERVER_DIR: REPO, CRUNDI_NODE: process.execPath, CRUNDI_WORKER: WORKER || join(ROOT, 'no-worker'),
  CRUNDI_DEV: '1', DEV_WEB_PORT: String(SERVER_PORT), HOMEPOOL_DISABLE: '1', FACTORY_WORKTREES_DIR: join(ROOT, 'worktrees'),
  CRUNDI_OPEN_URL_FILE: join(ROOT, 'open-url.txt'),
  // Removed for the server, as Electron's startBot does.
  NODE_OPTIONS: '--max-old-space-size=3071',
};
if (PKG) {
  delete env.CRUNDI_SERVER_DIR; delete env.CRUNDI_NODE; delete env.CRUNDI_WORKER;
  // A home of the run's own whose ~/.local/bin/claude is the stub (as tools/stub-server.sh).
  const H = join(ROOT, 'home');
  mkdirSync(join(H, '.local', 'bin'), { recursive: true });
  writeFileSync(join(H, '.local', 'bin', 'claude'), `#!/bin/sh\nexec node ${fileURLToPath(new URL('../../tools/claude-stub.mjs', import.meta.url))} "$@"\n`, { mode: 0o755 });
  env.HOME = H;
  env.PATH = [join(H, '.local', 'bin'), ...(process.env.PATH || '').split(':').filter((x) => x && !x.startsWith(join(homedir(), '.local', 'bin')) && !x.includes('/.claude'))].join(':');
}

let app = null, d = null, exited = false;
const pidsSeen = new Set();
function cleanup() {
  try { app?.kill('SIGKILL'); } catch { /* gone */ }
  for (const p of pidsSeen) { try { process.kill(-p, 'SIGKILL'); } catch { /* gone */ } try { process.kill(p, 'SIGKILL'); } catch { /* gone */ } }
  try { rmSync(ROOT, { recursive: true, force: true }); } catch { /* gone */ }
}
process.on('exit', cleanup);
for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(sig, () => { cleanup(); process.exit(130); });

async function launch(tag) {
  exited = false;
  app = spawn(BIN, [], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  const log = (await import('node:fs')).createWriteStream(join(OUT, `native-electron-${tag}.log`));
  app.stdout.pipe(log); app.stderr.pipe(log);
  app.on('exit', () => { exited = true; });
  d = await driver(DRIVER_PORT, () => !exited);
}
const state = async () => (await d.send({ cmd: 'state' })).state || {};
const ui = (n, ...args) => d.send({ cmd: 'ui', name: n, args });
const key = (text, mods = []) => d.send({ cmd: 'key', text, mods });
/** Text typed into the focused box, one character at a time (never a key's name). */
async function type(text) { for (const ch of text) await key(ch); }
// Pictures for the UI reviewer: native-v2/screens/electron/ (the latest only),
// and the tray's menu (drawn by the desktop shell, so as JSON) in each state.
const SCREENS = fileURLToPath(new URL('../../screens/electron/', import.meta.url));
mkdirSync(SCREENS, { recursive: true });
async function snap(file) {
  if (!/alert/.test(file)) await ui('toasts-clear');
  await sleep(400);
  const r = await d.send({ cmd: 'snapshot', path: join(SCREENS, `${file}.png`) });
  if (!r?.ok) console.log(`(no snapshot electron/${file}: ${r?.error})`);
}
const trayJson = (file, s) => writeFileSync(join(SCREENS, `${file}.json`), JSON.stringify({ tooltip: tray(s).tooltip, menu: (tray(s).menu || []).map(({ id, label, enabled, checked, kind }) => ({ id, label, enabled, checked, kind })) }, null, 2) + '\n');
const local = (s) => s.areas?.desktop?.local || {};
const tray = (s) => s.areas?.desktop?.tray || {};
const item = (s, id) => (tray(s).menu || []).find((m) => m.id === id) || {};
async function until(what, test, ms = 30000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    try { last = await state(); if (test(last)) { check(what, true); return last; } } catch { /* not yet */ }
    await sleep(200);
  }
  check(what, false, { toast: last?.toast, worker: local(last || {}).worker, screen: last?.localScreen, dialog: last?.dialog, signedIn: last?.signedIn, focus: last?.focus, local: local(last || {}) });
  return last;
}
const alive = (pid) => { try { const s = readFileSync(`/proc/${pid}/stat`, 'utf8'); return s.slice(s.lastIndexOf(')') + 2, s.lastIndexOf(')') + 3) !== 'Z'; } catch { return false; } };
function tree(pid) {
  const kids = [];
  for (const e of readdirSync('/proc')) {
    if (!/^\d+$/.test(e)) continue;
    try { const s = readFileSync(`/proc/${e}/stat`, 'utf8'); const pp = Number(s.slice(s.lastIndexOf(')') + 2).split(' ')[1]); kids.push([Number(e), pp]); } catch { /* gone */ }
  }
  const out = []; const todo = [pid];
  while (todo.length) { const p = todo.pop(); for (const [c, pp] of kids) if (pp === p && !out.includes(c)) { out.push(c); todo.push(c); } }
  return out;
}
/** The command lines of crundi-worker processes under the client (by its PID's tree). */
function pgrepWorker() {
  if (!app?.pid) return [];
  return tree(app.pid).map((p) => { try { return readFileSync(`/proc/${p}/cmdline`, 'utf8'); } catch { return ''; } }).filter((c) => c.split('\0')[0].endsWith('crundi-worker'));
}
const environ = (pid) => Object.fromEntries(readFileSync(`/proc/${pid}/environ`, 'utf8').split('\0').filter(Boolean).map((l) => [l.slice(0, l.indexOf('=')), l.slice(l.indexOf('=') + 1)]));
async function gone(pids, ms = 8000) { const end = Date.now() + ms; while (Date.now() < end) { if (pids.every((p) => !alive(p))) return true; await sleep(100); } return pids.every((p) => !alive(p)); }

// ─── 1. The first run ───
console.log(`client: ${BIN}\nconfig: ${ROOT}`);
await launch('first');
let s = await until('the all-in-one: the setup wizard on a first run', (s) => local(s).bundled && local(s).active && s.localScreen?.wizard && s.localScreen?.setup?.step === 1);
check('the server folder and Electron\'s userData', local(s).serverDir?.replace(/\/$/, '') === SERVER_DIR.replace(/\/$/, '') && local(s).userData === ELECTRON, local(s));
check('Electron\'s device id taken over', readFileSync(join(CFG, 'device-id'), 'utf8').trim() === DEVICE);
check('Electron\'s drawer history taken over', JSON.parse(readFileSync(join(CFG, 'drawer-history.json'), 'utf8'))[0]?.url === 'https://example.com/');
check('Electron\'s update settings taken over', JSON.parse(readFileSync(join(CFG, 'update-config.json'), 'utf8')).channel === 'production');
check('Electron\'s client.json taken over (local)', JSON.parse(readFileSync(join(CFG, 'client.json'), 'utf8')).mode === 'local');
check('the homepool node keeps Electron\'s device', JSON.parse(readFileSync(join(CFG, 'homepool-node.json'), 'utf8')).deviceId === DEVICE);
trayJson('tray-menu-stopped', s);
await snap('setup-wizard-1');
check('the tray: Stopped, Start on, Stop off', tray(s).tooltip === 'Crundi — Stopped' && item(s, 'start').enabled === true && item(s, 'stop').enabled === false, tray(s));

// Step 1: Next with nothing typed is refused (the alert), Escape closes it.
await ui('setup-next');
s = await until('step 1 refused empty: the alert', (s) => s.dialog?.message === 'Paste your Telegram bot token to continue.' && s.localScreen.setup.step === 1);
await snap('setup-wizard-1-alert');
await key('Escape');
s = await until('Escape closes the alert', (s) => !s.dialog);
// The token box has the keys (showSetupStep focuses it): typed, one corrected with Backspace.
await type('123456:ABCx');
await key('Backspace');
s = await until('the token typed and corrected', (s) => s.localScreen.setup.token === '123456:ABC');
await snap('setup-wizard-1-bot-token');
await ui('setup-next');
s = await until('step 2', (s) => s.localScreen.setup.step === 2);
await type('@owner');
await until('the username typed', (s) => s.localScreen.setup.username === '@owner', 5000);
await snap('setup-wizard-2-username');
// Next twice at once (N2): step 3's box focus, still waiting for its render,
// must not take the keys from step 3's alert; Escape closes it every time.
await ui('setup-next');
await ui('setup-next');
s = await until('step 3 refused without a folder (Next pressed as the step showed)', (s) => s.localScreen.setup.step === 3 && s.localScreen.setup.username === '@owner' && s.dialog?.message === 'Choose your projects directory (or switch to Multiple folders).');
await snap('setup-wizard-3-alert');
await key('Escape');
await until('Escape closes the alert', (s) => !s.dialog, 5000);
await type('p');
s = await until('…and the keys are back in step 3\'s box', (s) => s.localScreen.setup.projects === 'p', 5000);
await key('Backspace');
await until('Backspace empties it', (s) => s.localScreen.setup.projects === '', 5000);
await ui('setup-next');
await until('the alert again', (s) => !!s.dialog);
await key('Escape');
await until('the alert closed again by Escape', (s) => !s.dialog, 5000);
await ui('setup-mode', 'multi');
s = await until('multiple folders: no folder needed', (s) => s.localScreen.setup.single === false);
await ui('setup-mode', 'single');
await ui('setup-focus');
await type(join(ROOT, 'projects'));
await until('the folder typed', (s) => s.localScreen.setup.projects === join(ROOT, 'projects'), 5000);
await snap('setup-wizard-3-projects');
await ui('setup-next');
s = await until('step 4 (optional)', (s) => s.localScreen.setup.step === 4);
await type('eyJtoken');
await until('the tunnel token typed', (s) => s.localScreen.setup.tunnelToken === 'eyJtoken', 5000);
await snap('setup-wizard-4-tunnel');
await ui('setup-skip');
s = await until('Skip drops the tunnel and goes on to step 5', (s) => s.localScreen.setup.step === 5 && s.localScreen.setup.tunnelToken === '');
await snap('setup-wizard-5');
await ui('setup-next'); await ui('setup-next');
s = await until('step 7: updates and startup, both on', (s) => s.localScreen.setup.step === 7 && s.localScreen.setup.autoUpdate && s.localScreen.setup.autoLaunch);
await snap('setup-wizard-7-updates-startup');
await ui('setup-back');
s = await until('Back goes to step 6', (s) => s.localScreen.setup.step === 6);
await snap('setup-wizard-6');
await ui('setup-next');
await until('step 7 again', (s) => s.localScreen.setup.step === 7);
// "Restore from a backup" instead: setup:save with nothing set up; the server starts.
await ui('setup-restore');
s = await until('restore: the server starts, then the sign-in page offers the restore', (s) => local(s).status === 'running' && !s.localScreen?.active, 90000);
const envText = existsSync(join(ELECTRON, '.env')) ? readFileSync(join(ELECTRON, '.env'), 'utf8') : '';
check('setup:save\'s .env, in Electron\'s folder', envText.split('\n').join('|') === '# ─── Crundi Configuration ───|TELEGRAM_BOT_TOKEN=|ALLOWED_USERNAME=|PROJECTS_DIR=|WEB_PORT=0|CLOUDFLARE_TUNNEL_TOKEN=|CLOUDFLARE_TUNNEL_URL=|DATA_DIR=', envText);
check('first-run consent: update checks on', JSON.parse(readFileSync(join(CFG, 'update-config.json'), 'utf8')).autoUpdate === true);
check('first-run consent: launch at startup (Electron\'s entry, crundi.desktop)', existsSync(join(XDG, 'autostart', 'crundi.desktop')));
let pid = local(s).pid;
pidsSeen.add(pid);
await sleep(500);
let t0 = [pid, ...tree(pid)];
await d.send({ cmd: 'ui', name: 'desktop:tray-pick', args: ['quit'] });
let end = Date.now() + 15000; while (!exited && Date.now() < end) await sleep(100);
check('Quit: the client exits', exited);
check('Quit: the server and everything under it are gone', await gone(t0), t0.filter(alive));

// ─── 2. The next start ───
await launch('second');
// No sign-in method on the server yet (N1): Electron's page shows the server's
// first-run setup (#login-setup), never the app with its errors.
s = await until('the next start: Running, no sign-in method yet: the setup page, not the app', (s) => local(s).status === 'running' && !s.signedIn && s.signinStep === 'Login' && s.signin?.setupRequired && !s.localScreen?.active, 90000);
check('…the web\'s words: "Set up a way to sign in"', s.signin?.prompt === 'Set up a way to sign in', s.signin);
await snap('settings-before-signin-setup');
await sleep(1500);
check('…and still not in the app', !(await state()).signedIn);
const password = 'native-v2-local-' + Math.random().toString(36).slice(2, 10);
await ui('set-up-password', password);
s = await until('the password set: the authenticator step', (s) => s.signin?.enrol && s.signin?.enrolSecret, 20000);
const secret = s.signin.enrolSecret;
await ui('finish-enrol');
s = await until('Continue: in the app, online', (s) => s.signedIn && s.online, 30000);
if (PKG) check('the package\'s own Node runs the server (node/bin/node)', readFileSync(`/proc/${local(s).pid}/cmdline`, 'utf8').split('\0')[0] === join(PKG, 'node', 'bin', 'node'), readFileSync(`/proc/${local(s).pid}/cmdline`, 'utf8').split('\0')[0]);
const su = await fetch(`http://localhost:${SERVER_PORT}/api/auth/password`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ password, code: totpNow(secret) }) }).then((r) => r.json()).catch((e) => ({ error: String(e) }));
check('the test signs in to the server\'s API', !!su.token, su);
if (PKG) {
  // A chat on the bundled server streams (its Claude the stub).
  const proj = join(ROOT, 'projects', 'demo');
  const pr = await fetch(`http://localhost:${SERVER_PORT}/api/projects`, { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${su.token}` }, body: JSON.stringify({ alias: 'demo', name: 'Demo', path: proj, create: true }) }).then((r) => r.status).catch(() => 0);
  check('a project on the bundled server', pr > 0 && pr < 300, pr);
  await until('…listed in the app', (s) => (s.projects || []).includes('demo'), 15000);
  await ui('select-project', 'demo');
  await until('…open', (s) => s.project === 'demo', 10000);
  await ui('tab', 'workbench');
  await ui('new-chat');
  await until('a chat pane, idle', (s) => (s.panes || []).length > 0 && s.chatState === 'idle', 30000);
  await ui('composer-text', 'Reply with three hundred numbers in words.');
  await ui('composer-send');
  await until('the chat streams its reply (working)', (s) => s.composer?.state === 'working', 30000);
  await snap('local-chat-streaming');
  await until('…and ends (idle, the model claude-stub)', (s) => s.composer?.state === 'idle' && s.composer?.model === 'claude-stub', 60000);
}
pid = local(s).pid; pidsSeen.add(pid);
const e = environ(pid);
check('startBot\'s environment: DATA_DIR and DOTENV_PATH in Electron\'s folder', e.DATA_DIR === join(ELECTRON, 'data') && e.DOTENV_PATH === join(ELECTRON, '.env'), { DATA_DIR: e.DATA_DIR, DOTENV_PATH: e.DOTENV_PATH });
check('CRUNDI_DEVICE_ID is Electron\'s device', e.CRUNDI_DEVICE_ID === DEVICE, e.CRUNDI_DEVICE_ID);
check('CRUNDI_DEV kept, NODE_OPTIONS removed', e.CRUNDI_DEV === '1' && !('NODE_OPTIONS' in e));
check('CRUNDI_NATIVE_RUN, not ELECTRON_RUN: the server\'s notify and browser calls go over the native-host socket, as a remote server\'s (src/browser.js hasParentIpc)', e.CRUNDI_NATIVE_RUN === '1' && !('ELECTRON_RUN' in e), { CRUNDI_NATIVE_RUN: e.CRUNDI_NATIVE_RUN, ELECTRON_RUN: e.ELECTRON_RUN });
check('the IPC channel (Node\'s, at fd 3)', e.NODE_CHANNEL_FD === '3' && e.NODE_CHANNEL_SERIALIZATION_MODE === 'json');
check('the PATH as serverPath builds it', e.PATH.includes(join(env.HOME, '.local', 'bin')) && e.PATH.split(':').includes('/usr/local/bin'), e.PATH);
check('the server\'s command: node --no-deprecation src/index.js', readFileSync(`/proc/${pid}/cmdline`, 'utf8').split('\0').slice(1, 3).join(' ') === `--no-deprecation ${join(SERVER_DIR, 'src', 'index.js')}`);
trayJson('tray-menu-running', s);
check('the tray: Running, Start off, Stop on', tray(s).tooltip === 'Crundi — Running' && item(s, 'status').label === 'Running' && !item(s, 'start').enabled && item(s, 'stop').enabled, tray(s));

// Tray › Stop: the shutdown message, the server and its tree gone, the status screen.
t0 = [pid, ...tree(pid)];
await ui('desktop:tray-pick', 'stop');
s = await until('Stop: Stopped, out of the app, on the status screen', (s) => local(s).status === 'stopped' && !s.signedIn && s.localScreen?.status === 'stopped', 15000);
check('Stop: the server went (its shutdown) and its tree too', await gone(t0), t0.filter(alive));
check('Stop: the server ended itself (exit 0)', /Process exited: code=0/.test(readFileSync(join(OUT, 'native-electron-second.log'), 'utf8')));
check('the tray after Stop: Start on, Stop off', item(s, 'start').enabled && !item(s, 'stop').enabled);
await snap('local-status-stopped');
// The status screen's Start.
await ui('local-start');
s = await until('Start (the status screen): running and signed in again', (s) => local(s).status === 'running' && s.signedIn, 90000);
pid = local(s).pid; pidsSeen.add(pid);
// A crash: Error, the words, Start again from the tray.
process.kill(pid, 'SIGKILL');
s = await until('a crash: Error and "Process crashed"', (s) => local(s).status === 'error' && /Process crashed/.test(s.localScreen?.lastLog || ''), 15000);
check('the tray after a crash: Error, Start on', tray(s).tooltip === 'Crundi — Error' && item(s, 'start').enabled);
trayJson('tray-menu-error', s);
await snap('local-status-crashed');
await d.send({ cmd: 'ui', name: 'desktop:tray-pick', args: ['start'] });
s = await until('Tray › Start after the crash: running and signed in', (s) => local(s).status === 'running' && s.signedIn, 90000);
pid = local(s).pid; pidsSeen.add(pid);
// Restart (the status screen's, bot:restart): a new server.
const starts = local(s).starts;
await ui('local-restart');
s = await until('Restart: a new server, running and signed in', (s) => local(s).status === 'running' && s.signedIn && local(s).starts > starts && local(s).pid !== pid, 90000);
pid = local(s).pid; pidsSeen.add(pid);
// The server's restart-server message (restartSelf, as a restored backup asks):
// the client stops it and starts a new one, as Electron's startBot does.
const tok = await fetch(`http://localhost:${SERVER_PORT}/api/auth/password`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ password, code: totpNow(secret) }) }).then((r) => r.json()).catch((e) => ({ error: String(e) }));
const rs = await fetch(`http://localhost:${SERVER_PORT}/api/backup/restart`, { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${tok.token}` }, body: '{}' }).then((r) => r.json()).catch((e) => ({ error: String(e) }));
check('the server can restart itself here (its IPC channel: restartMode "desktop")', rs.ok === true, rs);
const starts2 = local(s).starts;
s = await until('restart-server: a new server, running and signed in', (s) => local(s).status === 'running' && s.signedIn && local(s).starts > starts2 && local(s).pid !== pid, 90000);
check('…"Restarting the server as it asked" (startBot\'s restart)', /Restarting the server as it asked/.test(readFileSync(join(OUT, 'native-electron-second.log'), 'utf8')));
pid = local(s).pid; pidsSeen.add(pid);

// The server screen (Tray › Server…): this machine or another server.
await ui('desktop:tray-pick', 'server');
s = await until('Server…: the server screen over the app, "Run a server on this machine" chosen', (s) => s.signin?.serverOver && s.localScreen?.serverRemote === false);
await snap('server-screen-local');
await ui('server-mode', 'remote');
s = await until('"Connect to a server" chosen', (s) => s.localScreen?.serverRemote === true);
// The owner's finding: the options' words wrap inside the card, the address box
// stays in its padding: at the normal width and at Electron's narrowest (800).
const PAD = 24;
async function inside(tag) {
  const st = await until(`the server screen drawn (${tag})`, (s) => s.signin?.rects?.card && s.signin?.rects?.['srv-remote'] && s.signin?.rects?.input);
  const r = st.signin?.rects || {};
  const c = r.card || {};
  const right = c.x + c.w - PAD + 0.5;
  const bad = ['srv-local', 'srv-remote', 'input'].filter((k) => !r[k] || r[k].x < c.x + PAD - 0.5 || r[k].x + r[k].w > right);
  check(`${tag}: the options' words and the address box end inside the card's padding`, !bad.length, { card: c, ...Object.fromEntries(Object.entries(r)) });
  check(`${tag}: "Connect to a server"'s words wrap (two lines, not one past the card)`, (r['srv-remote']?.h || 0) > (r['srv-local']?.h || 0), r);
  if (tag !== 'normal') check(`${tag}: the card inside the window`, c.x >= 0 && c.x + c.w <= 800.5, c);
}
await d.send({ cmd: 'resize', w: 1100, h: 750 });
await sleep(400);
await inside('normal');
await snap('server-screen-remote');
await d.send({ cmd: 'resize', w: 800, h: 500 });
await sleep(400);
await inside('narrow');
await snap('server-screen-remote-narrow');
await ui('server-mode', 'local');
await until('this machine chosen (narrow)', (s) => s.localScreen?.serverRemote === false, 5000);
await snap('server-screen-local-narrow');
await d.send({ cmd: 'resize', w: 1100, h: 750 });
await sleep(300);
await ui('server-screen', 'false');
await until('Cancel: back to the app', (s) => !s.signin?.serverOver && s.signedIn);
// Settings on the bundled server, now that it has a sign-in method.
const so = (s) => s.areas?.ops?.settings || {};
await ui('tab', 'settings');
await until('settings: the tab loads', (s) => so(s).state === 'data');
await snap('settings-folded');
await ui('ops:settings-open', 'desktop');
await until('settings: Desktop App open', (s) => so(s).open?.desktop === true, 5000);
await snap('settings-desktop-app');
await ui('ops:settings-open', 'desktop', 'false');
await ui('ops:settings-open', 'server');
await until('settings: Server open, its update section loaded', (s) => so(s).open?.server === true && so(s).server?.body === 'data', 15000);
await snap('settings-server');
await ui('ops:settings-open', 'server', 'false');
await ui('tab', 'workbench');

// This PC as a worker: the switch joins it (join-self, kind native) and keeps the key.
// Its work folder the run's (never the person's ~/crundi-work).
writeFileSync(join(CFG, 'worker.json'), JSON.stringify({ workDir: join(ROOT, 'work') }));
await ui('desktop:worker-toggle', 'on');
if (WORKER) {
  s = await until('the worker: joined, crundi-worker running and online', (s) => local(s).worker?.configured && local(s).worker?.state === 'online', 30000);
  // Signed in again: the server was started anew since the first sign-in.
  const t2 = await fetch(`http://localhost:${SERVER_PORT}/api/auth/password`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ password, code: totpNow(secret) }) }).then((r) => r.json()).catch(() => ({}));
  const list = await fetch(`http://localhost:${SERVER_PORT}/api/workers`, { headers: { Authorization: `Bearer ${t2.token}` } }).then((r) => r.json()).catch((e) => ({ error: String(e) }));
  const me = (list.workers || []).find((w) => w.deviceId === DEVICE) || (list.workers || [])[0] || {};
  check('the server has this PC as a worker of kind "native" (join-self and the worker\'s hello)', me.kind === 'native' && (list.workers || []).every((w) => w.kind === 'native'), list.workers || list);
  const wp = pgrepWorker();
  check('crundi-worker started with --kind native and the run\'s work folder', wp.length === 1 && wp[0].includes('--kind\0native') && wp[0].includes(join(ROOT, 'work')), wp.map((c) => c.split('\0').join(' ')));
} else {
  s = await until('the worker: joined, its key kept, no worker program in this build', (s) => local(s).worker?.configured && local(s).worker?.state === 'missing', 20000);
  console.log('  (no crundi-worker built: WORKER_BIN, or package/check.sh builds one; its run is not checked)');
}
const wj = existsSync(join(CFG, 'worker.json')) ? JSON.parse(readFileSync(join(CFG, 'worker.json'), 'utf8')) : {};
check('worker.json: the server, a cw_w key, enabled', /^cw_w[0-9a-f]{10}\./.test(wj.key || '') && wj.enabled === true && wj.server === `http://localhost:${SERVER_PORT}`, { ...wj, key: (wj.key || '').slice(0, 6) });
await ui('desktop:worker-toggle', 'off');
await until('the worker switched off', (s) => local(s).worker?.enabled === false);
if (WORKER) { const end2 = Date.now() + 8000; while (pgrepWorker().length && Date.now() < end2) await sleep(100); check('…its crundi-worker gone', !pgrepWorker().length); }

// Quit: nothing left running: the client, the server and every process under either (by PID).
t0 = [app.pid, ...tree(app.pid), pid, ...tree(pid)];
console.log(`  processes before Quit: ${t0.length} (${t0.map((p) => { try { return `${p}:${readFileSync(`/proc/${p}/comm`, 'utf8').trim()}`; } catch { return p; } }).join(' ')})`);
await d.send({ cmd: 'ui', name: 'desktop:tray-pick', args: ['quit'] });
end = Date.now() + 15000; while (!exited && Date.now() < end) await sleep(100);
check('Quit: the client exits', exited);
check('Quit: no process left behind (the server, its tree)', await gone(t0), t0.filter(alive));

// ─── 3. Remote mode with no server (startBot's remote branch): the server step, no local server ───
writeFileSync(join(CFG, 'client.json'), JSON.stringify({ mode: 'remote', serverUrl: '' }));
{ const sp = join(CFG, 'session.json'); const v = existsSync(sp) ? JSON.parse(readFileSync(sp, 'utf8')) : {}; v.server = ''; v.refresh_token = ''; writeFileSync(sp, JSON.stringify(v)); }
d.close();
await launch('third');
s = await until('remote, no address: the server step (showServerScreen), not the status screen', (s) => s.signinStep === 'Server' && !s.signedIn && !s.localScreen?.active, 30000);
await sleep(1500);
s = await state();
check('…and no local server started', !local(s).pid && local(s).status !== 'running' && local(s).status !== 'starting' && !tree(app.pid).some((p) => { try { return readFileSync(`/proc/${p}/cmdline`, 'utf8').includes('src/index.js'); } catch { return false; } }), local(s));
await d.send({ cmd: 'ui', name: 'desktop:tray-pick', args: ['quit'] });
end = Date.now() + 15000; while (!exited && Date.now() < end) await sleep(100);
check('Quit (remote): the client exits', exited);

d.close();
cleanup();
const { passed, failed } = counts();
console.log(`\nlocal: ${passed} passed, ${failed} failed`);
process.exit(failed ? 1 : process.exitCode || 0);
