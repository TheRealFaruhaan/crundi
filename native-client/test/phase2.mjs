#!/usr/bin/env node
// Phase 2 (chats), end to end: the native client against a real Crundi
// server, driven through its test driver (src/testctl.rs). One section per
// screen of PLAN.md's phase 2; each drives the screen as a person would
// (the pointer and keys where it matters) and checks the server and the app.
//
//   2.1 the pane header: rename (click, type, Enter; Esc; a server rename
//       arriving live), text size (clamped, saved, the new-chat default,
//       kept across a restart), pin (locked dividers, kept across a
//       restart), full screen (Esc leaves), the two-click × (disarms after
//       3 s and on a layout change; closes the chat on the server),
//       double-click to maximise (nothing on a lone pane), presets keeping a
//       pin, one full-screen pane at a time, leaving a rename input saves,
//       and the secrets lock's count, live.
//
// Needs a FRESH dev server with no sign-in set up (this sets a password on it):
//
//   CRUNDI_TEST_SERVER=http://localhost:4811 NATIVE_BIN=… DISPLAY=… node native-client/test/phase2.mjs
//
// Pictures go to DRIVE_OUT (default: the temp dir).

import net from 'node:net';
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, openSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { totpNow } from '../../src/auth-password.js';

const SERVER = process.env.CRUNDI_TEST_SERVER || 'http://localhost:8889';
const BIN = process.env.NATIVE_BIN;
const PORT = Number(process.env.CRUNDI_TEST_PORT || 7612);
const OUT = process.env.DRIVE_OUT || tmpdir();
mkdirSync(OUT, { recursive: true });
if (!BIN) { console.error('Set NATIVE_BIN'); process.exit(2); }

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : '  ' + JSON.stringify(extra).slice(0, 400)}`); if (!ok) failed++; return ok; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function http(method, path, body, token) {
  const res = await fetch(SERVER + path, {
    method,
    headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: 'Bearer ' + token } : {}) },
    body: body ? JSON.stringify(body) : undefined,
  });
  return { status: res.status, body: await res.json().catch(() => null) };
}

// ─── The server: a password, a project, two chats ───
const PASSWORD = 'native-client-test-' + Math.random().toString(36).slice(2, 10);
const methods = await http('GET', '/api/auth/methods');
if (!methods.body?.setupRequired) { console.error('This needs a fresh dev server (no sign-in set up yet)'); process.exit(2); }
const setup = await http('POST', '/api/auth/setup', { method: 'password', password: PASSWORD });
check('the server takes a password', setup.body?.ok && setup.body.totpSecret, setup.body);
const SECRET = setup.body.totpSecret;
const token = setup.body.token;
const proj = await http('POST', '/api/projects', { alias: 'demo', name: 'Demo', path: process.env.CRUNDI_TEST_PROJECT || join(tmpdir(), 'crundi-native-demo2'), create: true }, token);
check('a project to work in', proj.status < 300, proj.body);
const chatA = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'First chat' }, token)).body?.id;
const chatB = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Second chat' }, token)).body?.id;
check('two chats on the server', chatA && chatB);
const serverChat = async (id) => (await http('GET', '/api/ui-sessions', null, token)).body?.sessions?.find((s) => s.id === id);

// ─── The app and its driver ───
const configDir = mkdtempSync(join(tmpdir(), 'crundi-native-config-'));
const saved = () => JSON.parse(readFileSync(join(configDir, 'session.json'), 'utf8'));
function launch(tag) {
  const log = openSync(join(OUT, `native2-${tag}.log`), 'w');
  return spawn(BIN, [], { env: { ...process.env, CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: configDir, CRUNDI_URL_A: 'about:blank', CRUNDI_URL_B: 'about:blank' }, stdio: ['ignore', log, log] });
}

function driver() {
  return new Promise((resolve, reject) => {
    const tryConnect = (left) => {
      const sock = net.connect(PORT, '127.0.0.1');
      sock.once('connect', () => {
        let buf = '';
        const waiting = [];
        sock.on('data', (d) => {
          buf += d;
          let i;
          while ((i = buf.indexOf('\n')) >= 0) { const line = buf.slice(0, i); buf = buf.slice(i + 1); waiting.shift()?.(JSON.parse(line)); }
        });
        resolve({ send: (cmd) => new Promise((r) => { waiting.push(r); sock.write(JSON.stringify(cmd) + '\n'); }), close: () => sock.end() });
      });
      sock.once('error', () => (left > 0 ? setTimeout(() => tryConnect(left - 1), 500) : reject(new Error('no test driver'))));
    };
    tryConnect(120);
  });
}

let d;
const props = () => d.send({ cmd: 'props' });
async function until(what, test, ms = 20000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    last = await props();
    if (await test(last)) return last;
    await sleep(300);
  }
  check(what, false, last && { panes: last.panes, full: last.full, toast: last.toast });
  return null;
}
const ui = (name, ...args) => d.send({ cmd: 'ui', name, args: args.map(String) });
const snap = (name) => d.send({ cmd: 'snapshot', path: join(OUT, `phase2-${name}.png`) });
const mouse = (kind, x, y) => d.send({ cmd: 'mouse', kind, x, y });
const key = (text) => d.send({ cmd: 'key', text });
const pane = (p, id) => p.panes.find((x) => x.key === 'live:' + id);
// Window coordinates: the workbench starts under the top bar and tab bar,
// right of the sidebar (240, 84); the header is 28px, its row centred at 14.
const WB = { x: 240, y: 84 };
const headY = (q) => WB.y + q.y + 14;
const closeX = (q) => WB.x + q.x + q.w - 1 - 4 - 13.85;   // the ×: 27.7 wide, 4px from the edge
const titleX = (q) => WB.x + q.x + 1 + 43 + 20;            // inside the title (it starts 43px in)

let app = launch('run1');
// Never leave the app behind (it would hold the driver's port for the next run).
process.on('exit', () => { try { app.kill(); } catch {} });
process.on('uncaughtException', (e) => { console.error(e); process.exit(1); });
d = await driver();

// Sign in, open the project.
await d.send({ cmd: 'ui', name: 'check-server', args: [SERVER] });
await until('the server offers a password', (p) => p.offerPassword && p.signinChecked);
await ui('sign-in-password', SERVER, PASSWORD, totpNow(SECRET));
await until('signed in', (p) => p.signedIn);
await ui('select-project', 'demo');
let p = await until('both chats have panes', (q) => pane(q, chatA) && pane(q, chatB));
await snap('2.1-rest');

// ─── 2.1 The pane header ───

// Rename: click the title, type, Enter. The input starts all selected, so typing replaces it.
let a = pane(p, chatA);
check('the header shows the short title', a.title === 'First chat' && a.shortTitle === 'First chat', a);
await mouse('click', titleX(a), headY(a));
await sleep(300);
await snap('2.1-renaming');
await key('Renamed here');
await key('Return');
p = await until('the title changes at once', (q) => pane(q, chatA).title === 'Renamed here');
await until('the server has the new title', async () => (await serverChat(chatA))?.title === 'Renamed here') && check('rename: the server has the new title', true);
// Esc puts the title back.
await mouse('click', titleX(a), headY(a));
await sleep(300);
await key('Throw away');
await key('Escape');
await sleep(800);
check('Esc cancels a rename', pane(await props(), chatA).title === 'Renamed here' && (await serverChat(chatA)).title === 'Renamed here');
// Empty becomes "Terminal".
await ui('pane-rename', a.id, '   ');
await until('an empty title becomes "Terminal"', (q) => pane(q, chatA).title === 'Terminal') && check('an empty title becomes "Terminal"', true);
// A rename on the server arrives live; it doesn't clobber an open rename.
await http('POST', `/api/ui-sessions/${chatA}/rename`, { title: 'From the server' }, token);
await until('a server rename shows live', (q) => pane(q, chatA).title === 'From the server') && check('a server rename shows live', true);
await mouse('click', titleX(a), headY(a));
await sleep(300);
await key('Mine');
await http('POST', `/api/ui-sessions/${chatA}/rename`, { title: 'Meanwhile' }, token);
await sleep(1500);
await key('Return');
await until('an open rename survives a server push', async (q) => pane(q, chatA).title === 'Mine' && (await serverChat(chatA))?.title === 'Mine') && check('an open rename survives a server push', true);
// Leaving the input (a click in the chat's log) saves too.
await mouse('click', titleX(a), headY(a));
await sleep(300);
await key('Blurred');
await mouse('click', WB.x + a.x + a.w / 2, WB.y + a.y + a.h / 2);
await sleep(500);
await snap('2.1-rename-blur');
await until('a click in the chat log leaves the input and saves the rename', async (q) => pane(q, chatA).title === 'Blurred' && (await serverChat(chatA))?.title === 'Blurred', 5000) && check('a click in the chat log saves the rename', true);
// Focus moving to the composer saves it too.
await mouse('click', WB.x + a.x + 100, WB.y + a.y + a.h - 56);
await until('focus moving to the composer saves the rename', async (q) => pane(q, chatA).title === 'Blurred' && (await serverChat(chatA))?.title === 'Blurred') && check('focus moving to the composer saves the rename', true);

// Text size: A+ / A- step, clamp at 9…24, reset to 13; saved per chat and as the new-chat default.
await ui('pane-font', a.id, 1);
p = await until('A+ makes the text larger', (q) => Math.abs(pane(q, chatA).fs - 14 / 13) < 1e-3);
check('the size is saved per chat and as the default', saved().font?.[chatA] === 14 && saved().font_default_chat === 14, saved());
for (let i = 0; i < 15; i++) await ui('pane-font', a.id, 1);
await until('A+ stops at 24', (q) => Math.abs(pane(q, chatA).fs - 24 / 13) < 1e-3) && check('A+ stops at 24', true);
await snap('2.1-font-max');
for (let i = 0; i < 20; i++) await ui('pane-font', a.id, -1);
await until('A- stops at 9', (q) => Math.abs(pane(q, chatA).fs - 9 / 13) < 1e-3) && check('A- stops at 9', true);
await snap('2.1-font-min');
check('the other chat keeps its size', Math.abs(pane(await props(), chatB).fs - 1) < 1e-3);
await ui('pane-font', a.id, 0);
await until('reset goes back to 13', (q) => Math.abs(pane(q, chatA).fs - 1) < 1e-3) && check('reset goes back to 13', true);
await ui('pane-font', a.id, 1);
await ui('pane-font', a.id, 1);   // 15: kept across the restart below

// Pin: the pane keeps its size, and the divider by it is locked.
p = await props();
a = pane(p, chatA);
await ui('pane-pin', a.id);
p = await until('pin', (q) => pane(q, chatA).pinned);
check('the divider by a pinned pane is locked', p.lockedGutters === 1, p);
const pinW = Math.round(pane(p, chatA).w);
const leaves = (n) => (n.t === 'leaf' ? [n] : n.kids.flatMap(leaves));
const savedLeaf = leaves(saved().layouts?.demo?.root || { t: 'split', kids: [] }).find((l) => l.key === 'live:' + chatA);
check('the pin is saved with the layout', savedLeaf?.pin?.w === pinW && savedLeaf.pin.h > 0, saved().layouts);
await snap('2.1-pinned');
// Layout presets keep the pin.
await ui('add-pane', 'layout:rows');
p = await until('the rows preset keeps the pin', (q) => pane(q, chatA)?.pinned && pane(q, chatB) && pane(q, chatA).y < pane(q, chatB).y);
if (p) check('a layout preset keeps the pin', true);
await ui('add-pane', 'layout:cols');
p = await until('back to two columns, still pinned', (q) => pane(q, chatA)?.pinned && pane(q, chatB) && Math.abs(pane(q, chatA).y - pane(q, chatB).y) < 1 && q.panes.length === 2);
if (p) check('another preset keeps it too', Math.round(pane(p, chatA).w) === pinW, p.panes);
a = pane(p, chatA);   // a preset makes new leaves (new ids)

// Double-click the header's background: 92 / 8, again: even. (Unpinned
// first: a pinned neighbour keeps its size.)
await ui('pane-pin', a.id);
p = await until('unpin', (q) => !pane(q, chatA).pinned && q.lockedGutters === 0);
const b0 = pane(p, chatB);
await mouse('dblclick', WB.x + b0.x + b0.w / 2, headY(b0));
p = await until('double-click on the header maximises in the split', (q) => pane(q, chatB).w > pane(q, chatA).w * 1.5);
await mouse('dblclick', WB.x + pane(p, chatB).x + pane(p, chatB).w / 2, headY(b0));
await until('again: an even split', (q) => Math.abs(pane(q, chatA).w - pane(q, chatB).w) < 2) && check('double-click: 92/8, then even', true);
// Not on a button: a double-click on A+ only steps the size twice.
p = await props();
const fsBefore = pane(p, chatB).fs;
const bb = pane(p, chatB);
await mouse('dblclick', closeX(bb) - 13.85 - 4 - 25 - 4 - 27 - 4 - 15.4, headY(bb));
await sleep(500);
p = await props();
check('a double-click on a button does not maximise', Math.abs(pane(p, chatA).w - pane(p, chatB).w) < 2 && pane(p, chatB).fs > fsBefore, p.panes);
await ui('pane-font', bb.id, 0);
await ui('pane-pin', a.id);
await until('pinned again', (q) => pane(q, chatA).pinned);

// Full screen covers the window; Esc leaves.
await ui('pane-full', a.id);
p = await until('full screen', (q) => q.full?.key === 'live:' + chatA);
check('full screen covers the window', p.full && p.full.w >= 1000 && p.full.h >= 600, p.full);
await snap('2.1-full');
await key('Escape');
await until('Esc leaves full screen', (q) => q.full === null) && check('Esc leaves full screen', true);
// Only one pane is full at a time, and the button leaves it.
await ui('pane-full', a.id);
await until('full screen again', (q) => q.full?.key === 'live:' + chatA);
await ui('pane-full', pane(await props(), chatB).id);
await until('another pane full replaces the first', (q) => q.full?.key === 'live:' + chatB) && check('only one pane is full at a time', true);
await ui('pane-full', pane(await props(), chatB).id);
await until('the button leaves full screen', (q) => q.full === null) && check('the full button leaves full screen', true);

// The secrets lock: its count follows the server live.
const caps = (await http('GET', '/api/handshake')).body?.server?.capabilities || [];
if (caps.includes('chat-secret-preauth')) {
  check('the lock shows when the server can', pane(await props(), chatA).secrets);
  const PIN = 'phase-two-pass';
  const ids = [];
  for (const name of ['P2_ONE', 'P2_TWO']) ids.push((await http('POST', '/api/secrets', { action: 'add', name, value: 'v-' + name, pin: PIN }, token)).body?.id);
  await http('POST', '/api/secrets/chat', { action: 'grant', sessionId: chatA, secrets: [{ id: ids[0], pin: PIN }], untilChatEnds: true }, token);
  await until('one grant: the lock counts 1', (q) => pane(q, chatA).secretsN === 1 && pane(q, chatA).secretsTip === '1 secret pre-authorised for this chat') && check('one grant shows live', true);
  await http('POST', '/api/secrets/chat', { action: 'grant', sessionId: chatA, secrets: ids.map((id) => ({ id, pin: PIN })), untilChatEnds: true }, token);
  await until('two grants: the lock counts 2', (q) => pane(q, chatA).secretsN === 2) && check('two grants show live', true);
  await snap('2.1-secrets-2');
  check('the other chat has none', pane(await props(), chatB).secretsN === 0);
  await http('POST', '/api/secrets/chat', { action: 'revokeAll', sessionId: chatA }, token);
  await until('revoked: no count', (q) => pane(q, chatA).secretsN === 0) && check('revoking shows live', true);
} else {
  check('no lock without the capability', !pane(await props(), chatA).secrets);
}

// A restart keeps the text size and the pin.
d.close();
app.kill();
await sleep(1500);
app = launch('run2');
d = await driver();
p = await until('signed in again after a restart', (q) => q.signedIn && pane(q, chatA));
p = await until('after a restart: text size and pin kept', (q) => pane(q, chatA) && Math.abs(pane(q, chatA).fs - 15 / 13) < 1e-3 && pane(q, chatA).pinned);
if (p) check('a restart keeps the text size and the pin', true);

// Close: the first click arms ("Sure?"), it disarms after 3 s; two clicks close the chat.
let bq = pane(await props(), chatB);
await mouse('click', closeX(bq), headY(bq));
await until('the first click arms the ×', (q) => pane(q, chatB).armed);
await snap('2.1-close-armed');
await sleep(3300);
check('it disarms after 3 s', !pane(await props(), chatB).armed);
// Arming one pane disarms the other.
await ui('pane-close', pane(await props(), chatA).id);
await ui('pane-close', bq.id);
p = await props();
check('only one pane is armed', pane(p, chatB).armed && !pane(p, chatA).armed, p.panes);
await sleep(3300);
// A layout change disarms it, so a click after a redraw never closes unwarned.
await ui('pane-close', bq.id);
await until('armed again', (q) => pane(q, chatB).armed);
await ui('pane-pin', bq.id);
p = await until('a layout change disarms the ×', (q) => !pane(q, chatB).armed && pane(q, chatB).pinned);
if (p) check('a layout change disarms the ×', true);
await ui('pane-pin', bq.id);
await until('unpinned again', (q) => !pane(q, chatB).pinned);
bq = pane(await props(), chatB);
await mouse('click', closeX(bq), headY(bq));
await sleep(200);
await mouse('click', WB.x + bq.x + bq.w - 1 - 4 - 22.4, headY(bq));   // "Sure?" is wider: its middle
await until('two clicks close the pane', (q) => !pane(q, chatB));
await until('the chat is closed on the server', async () => { const c = await serverChat(chatB); return !c || c.status === 'exited'; }) && check('two clicks close the chat', true);
check('a closed chat forgets its text size', !(chatB in (saved().font || {})), saved().font);
// A lone pane: a double-click on its header changes nothing.
p = await props();
const lone = pane(p, chatA);
await mouse('dblclick', WB.x + lone.x + lone.w / 2 + 60, headY(lone));
await sleep(600);
p = await props();
check('a double-click on a lone pane changes nothing', p.panes.length === 1 && Math.abs(pane(p, chatA).w - lone.w) < 1 && Math.abs(pane(p, chatA).h - lone.h) < 1 && p.full === null, p.panes);

d.close();
app.kill();
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
