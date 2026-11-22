// The shell's own parts against the web's (F78, F79): the first-run
// sign-in (set a password, the restore's layer, the authenticator step),
// the top bar ("/ No project", the connection badge's reconnecting states
// and line), the sidebar (remove, drag to reorder), the sign-in page
// (its prompt, the collaborator's way in), and the start-up screen for a
// saved login. Unlike lib.mjs's begin() it signs the app in through its
// own first-run form, so it starts the app itself.
//
// Run as test/lib.mjs's header says (a FRESH stub server each run):
//   CRUNDI_TEST_SERVER=http://localhost:4881 CRUNDI_TEST_PORT=4886 NATIVE_BIN=… \
//     xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/shell.mjs

import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, createWriteStream } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { totpNow } from '../../src/auth-password.js';
import { check, client, driver, sleep } from './lib.mjs';

const SERVER = process.env.CRUNDI_TEST_SERVER, PORT = Number(process.env.CRUNDI_TEST_PORT || 0), BIN = process.env.NATIVE_BIN;
if (!SERVER || !PORT || !BIN) { console.error('Set CRUNDI_TEST_SERVER, CRUNDI_TEST_PORT and NATIVE_BIN'); process.exit(2); }
if (!process.env.DISPLAY || process.env.DISPLAY === ':0') { console.error('Run under xvfb-run.'); process.exit(2); }
const SCREENS = process.env.SCREENS_DIR || fileURLToPath(new URL('../screens/', import.meta.url));
const OUT = join(tmpdir(), 'crundi-v2-shell-out');
mkdirSync(OUT, { recursive: true });
const api = client(SERVER);
const m0 = await api.http('GET', '/api/auth/methods');
if (!m0.body?.setupRequired) { console.error(`${SERVER} needs to be fresh`); process.exit(2); }

let passed = 0, failed = 0;
const ok = (name, cond, extra) => { if (check(name, !!cond, extra)) passed++; else failed++; return cond; };
const configDir = mkdtempSync(join(tmpdir(), 'crundi-v2-config-shell-'));
process.on('exit', () => { try { rmSync(configDir, { recursive: true, force: true }); } catch { /* gone */ } });
const log = createWriteStream(join(OUT, 'native-shell.log'), { flags: 'a' });

let app = null, d = null, exited = false;
async function start() {
  const env = { ...process.env, CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: configDir, CRUNDI_OPEN_URL_FILE: join(configDir, 'open-url.txt') };
  delete env.WAYLAND_DISPLAY;
  exited = false;
  app = spawn(BIN, [], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  app.stdout.pipe(log); app.stderr.pipe(log);
  app.on('exit', () => { exited = true; });
  d = await driver(PORT, () => !exited);
}
async function stop() {
  try { app.kill(); } catch { /* gone */ }
  d?.close();
  await new Promise((r) => { if (exited) r(); else { app.once('exit', r); setTimeout(r, 3000); } });
}
const state = async () => (await d.send({ cmd: 'state' })).state || {};
const ui = (n, ...args) => d.send({ cmd: 'ui', name: n, args });
const mouse = (kind, x, y, extra = {}) => d.send({ cmd: 'mouse', kind, x, y, ...extra });
async function snap(file) {
  mkdirSync(join(SCREENS, 'shell'), { recursive: true });
  // (F107/F98: no toast of the step before over it, and its state drawn.)
  await ui('toasts-clear');
  await new Promise((r) => setTimeout(r, 350));
  const r = await d.send({ cmd: 'snapshot', path: join(SCREENS, 'shell', `${file}.png`) });
  if (!r?.ok) console.log(`(no snapshot shell/${file}: ${r?.error})`);
}
async function until(what, test, ms = 20000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) { last = await state(); try { if (test(last)) { ok(what, true); return last; } } catch { /* not yet */ } await sleep(150); }
  ok(what, false, last);
  return null;
}

// ─── First run: set a password ───
await start();
await ui('check-server', SERVER);
let s = await until('a fresh server: the first-run form', (s) => s.signinStep === 'Login' && s.signin?.setupRequired);
ok('its prompt is the web\'s', s?.signin?.prompt === 'Set up a way to sign in', s?.signin);
await sleep(300);
await snap('signin-setup');
// The restore from a backup (Operations' layer over the sign-in page).
const r0 = await ui('login-restore');
ok('the restore opens from the sign-in page', r0.ok, r0);
await sleep(400);
await snap('signin-restore');
await ui('ops:skills-act', 'lr-back', '');
await sleep(200);
const password = 'native-v2-shell-' + Math.random().toString(36).slice(2, 10);
await ui('set-up-password', password);
s = await until('the authenticator step', (s) => s.signin?.enrol && s.signin?.enrolSecret);
const secret = s?.signin?.enrolSecret;
await sleep(300);
await snap('signin-enrol');
await ui('finish-enrol');
s = await until('signed in after Continue, online', (s) => s.signedIn && s.online, 30000);

// ─── The top bar ───
s = await until('no project: "/ No project"', (s) => s.projectName === 'No project');
await until('the badge says connected', (s) => s.conn?.badge === 'connected');
await sleep(500);
await snap('topbar-no-project');
// The API, signed in as the owner.
const login = await api.http('POST', '/api/auth/password', { password, code: totpNow(secret) });
ok('the test signs in to the API', login.body?.token, login.body);
api.setToken(login.body?.token);
const dir = (n) => `/tmp/crundi-devtest-${new URL(SERVER).port}/${n}`;
for (const [alias, name] of [['demo', 'Demo'], ['other', 'Other'], ['third', 'Third']]) {
  const p = await api.http('POST', '/api/projects', { alias, name, path: dir(alias), create: true });
  ok(`project ${alias} added`, p.status < 300, p.body);
}
s = await until('the sidebar lists them', (s) => ['demo', 'other', 'third'].every((a) => s.projects?.includes(a)));
await ui('select-project', 'demo');
await until('the project in the top bar', (s) => s.projectName === 'Demo');
await until('multi mode: projects can be removed', (s) => s.mode === 'multi');

// The connection badge: a drop that comes straight back shows nothing…
const r1 = await state();
await ui('conn-link', 'false'); await ui('conn-link', 'true');
await sleep(1600);
s = await state();
ok('a blink of a drop shows nothing', s.conn?.badge === 'connected' && s.conn?.line === 'Off', s.conn);
// …a real one: still "connected" for 1.2 s, then reconnecting with its line.
await ui('conn-link', 'false');
await sleep(600);
s = await state();
ok('within 1.2 s it still says connected', s.conn?.badge === 'connected', s.conn);
s = await until('then reconnecting, its tooltip, the line', (s) => s.conn?.badge === 'reconnecting' && s.conn?.tip === 'Reconnecting' && s.conn?.line === 'On', 3000);
const rr = (s?.renders || {});
await sleep(300);
await snap('topbar-reconnecting');
const before = await state();
await sleep(1000);
const after = await state();
const d1 = (k) => (after.renders?.[k] ?? 0) - (before.renders?.[k] ?? 0);
ok('the blink redraws the badge and the line, not the bar or the sidebar', d1('top-bar') === 0 && d1('sidebar') === 0 && d1('top-right') === 0 && (d1('conn-badge') > 0 || d1('reconn-line') > 0), after.renders);
s = await until('after 9 s more the tooltip says more', (s) => s.conn?.tip === 'Still trying. Crundi may be restarting, or the network may be down.', 12000);
await ui('conn-link', 'true');
s = await until('back: connected, a moment of green', (s) => s.conn?.badge === 'connected' && s.conn?.line === 'Back', 3000);
await snap('topbar-back');
await until('then no line', (s) => s.conn?.line === 'Off', 4000);
void r1; void rr;

// Idle: nothing redraws.
await sleep(800);
const i0 = await state(); await sleep(2000); const i1 = await state();
// (The shell's views; another area's screen is that area's suite's.)
const SHELL_VIEWS = ['usage-rows', 'top-bar', 'usage-pill', 'top-right', 'conn-badge', 'reconn-line', 'sidebar', 'project-bar', 'sign-in'];
const changed = SHELL_VIEWS.filter((k) => (i1.renders?.[k] ?? 0) !== (i0.renders?.[k] ?? 0));
ok('idle: no view of the shell redraws', changed.length === 0, changed);

// ─── The sidebar ───
// Hover a row: its remove button shows.
await mouse('move', 120, 150);
await sleep(300);
await snap('sidebar-hover');
// Drag "third" (the third row) above "demo".
await mouse('drag', 100, 184, { toX: 100, toY: 104, steps: 12 });
s = await until('dragged to the top: the new order', (s) => s.projects?.[0] === 'third', 5000);
const order = await api.http('GET', '/api/projects');
const list = (order.body?.projects || order.body || []).map((p) => p.alias);
ok('the server keeps the order', list[0] === 'third', list);
// Remove "other": asked first, then gone, with the web's toast.
await ui('remove-project', 'other');
s = await until('the confirm asks', (s) => s.dialog && JSON.stringify(s.dialog).includes('Remove \\"Other\\" from Crundi?'), 3000);
await snap('sidebar-remove-confirm');
await ui('dialog-answer', 'true');
s = await until('removed', (s) => !s.projects?.includes('other'), 8000);
ok('its toast', s && /Removed "Other"/.test(s.toast), s?.toast);

// ─── The sign-in page ───
await ui('sign-out');
s = await until('signed out: the login step', (s) => !s.signedIn && s.signinStep === 'Login');
// (The server is asked again what it offers: a password, by now.)
s = await until('the prompt for a password server', (s) => s.signin?.prompt === 'Sign in with your password and authenticator code', 5000);
await sleep(400);
await snap('signin-login');
await ui('signin-collab', 'true');
await until('the collaborator\'s way in', (s) => s.signin?.collab);
await sleep(200);
await snap('signin-collab');
await ui('sign-in-collab', 'nobody', 'wrong-passcode');
await until('a wrong passcode says so', (s) => /do not match/.test(s.signinError || ''), 8000);
await ui('signin-collab', 'false');

// ─── A saved login: the start-up screen, never the form ───
await ui('sign-in-password', SERVER, password, totpNow(secret));
await until('signed in again', (s) => s.signedIn, 30000);
await stop();
await start();
let sawForm = false, sawBoot = false;
const end = Date.now() + 20000;
while (Date.now() < end) {
  const st = await state();
  if (st.signinStep === 'Boot') sawBoot = true;
  if (!st.signedIn && st.signinStep === 'Login') sawForm = true;
  if (st.signedIn) break;
  await sleep(20);
}
ok('a saved login never shows the sign-in form', !sawForm);
ok('it signs in by itself', (await state()).signedIn);
console.log(sawBoot ? '(saw the start-up screen)' : '(the resume was faster than the poll)');
await stop();
console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
