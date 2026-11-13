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
//   2.2 the narrow header: three (then four) narrow panes; "…" toggles its
//       menu and focuses the pane, one menu at a time; pin, reset and a live
//       rename or chat-secrets count leave it open; Esc, the lock, full
//       screen, × and a press outside (which still acts) close it; it goes
//       with its pane; it closes when the pane goes wide; and the fold edge
//       (pane ≤ 567) checked by dragging a gutter across it.
//   2.3 moving panes: every Layout preset with four chats (and, in a second
//       project, with two: the spare slots empty), Fit (even shares; the
//       error toast when it can't fit), a gutter drag, a preset from the
//       menu by the mouse; the hover controls split down and right without
//       taking the focus; drag a pane by its header onto an empty pane
//       (moves) or a chat (swaps), Esc cancels, a pinned pane is refused;
//       an empty pane's Terminal chip puts a launcher in that pane and the
//       chat it starts takes the same leaf; × removes an empty pane; each
//       project keeps its own layout, and the sizes survive a restart.
//
// Needs a FRESH test server with no sign-in set up (this sets a password on
// it), started through tools/test-server.sh <port> --bare, never by hand; it
// refuses a server with a Telegram bot configured:
//
//   native-client/tools/test-server.sh 4811 --bare
//   CRUNDI_TEST_SERVER=http://localhost:4811 NATIVE_BIN=… DISPLAY=… node native-client/test/phase2.mjs
//
// Pictures go to DRIVE_OUT (default: the temp dir).

import net from 'node:net';
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, openSync, readFileSync, readdirSync } from 'node:fs';
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
// Never against a server running the LIVE Telegram bot (checked again below,
// once signed in): a test server comes from tools/test-server.sh <port> --bare.
if (methods.body?.telegram || methods.body?.telegramBotId) {
  console.error(`Refusing ${SERVER}: it has a Telegram bot configured. Start a test server with native-client/tools/test-server.sh <port> --bare.`);
  process.exit(2);
}
if (!methods.body?.setupRequired) { console.error('This needs a fresh dev server (no sign-in set up yet)'); process.exit(2); }
const setup = await http('POST', '/api/auth/setup', { method: 'password', password: PASSWORD });
check('the server takes a password', setup.body?.ok && setup.body.totpSecret, setup.body);
const SECRET = setup.body.totpSecret;
const token = setup.body.token;

// No bot behind the sign-in either (an empty .env, no TELEGRAM_BOT_TOKEN).
const status = await http('GET', '/api/status', null, token);
if (status.body?.botConfigured !== false) {
  console.error(`Refusing ${SERVER}: it has a Telegram bot configured (botConfigured=${status.body?.botConfigured}). Start a test server with native-client/tools/test-server.sh <port> --bare.`);
  process.exit(2);
}
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
await ui('add-pane', 'layout:top-bottom2');
p = await until('the rows preset keeps the pin', (q) => pane(q, chatA)?.pinned && pane(q, chatB) && pane(q, chatA).y < pane(q, chatB).y);
if (p) check('a layout preset keeps the pin', true);
await ui('add-pane', 'layout:cols2');
p = await until('back to two columns, still pinned', (q) => pane(q, chatA)?.pinned && pane(q, chatB) && Math.abs(pane(q, chatA).y - pane(q, chatB).y) < 1 && q.panes.length === 2);
if (p) check('another preset keeps it too', Math.round(pane(p, chatA).w) === pinW, p.panes);
a = pane(p, chatA);   // a preset makes new leaves (new ids)

// Double-click the header's background: 92 / 8, again: even. (Unpinned
// first: a pinned neighbour keeps its size.) 50px right of the centre: the
// hover split controls (2.3) cover the header's middle, as in the web.
await ui('pane-pin', a.id);
p = await until('unpin', (q) => !pane(q, chatA).pinned && q.lockedGutters === 0);
const b0 = pane(p, chatB);
await mouse('dblclick', WB.x + b0.x + b0.w / 2 + 50, headY(b0));
p = await until('double-click on the header maximises in the split', (q) => pane(q, chatB).w > pane(q, chatA).w * 1.5);
await mouse('dblclick', WB.x + pane(p, chatB).x + pane(p, chatB).w / 2 + 50, headY(b0));
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
const PIN = 'phase-two-pass';
const ids = [];
if (caps.includes('chat-secret-preauth')) {
  check('the lock shows when the server can', pane(await props(), chatA).secrets);
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

// ─── 2.2 The pane header, narrow ───
// Three chats side by side: each pane is 378 wide, so each header folds.
if (pane(await props(), chatA).pinned) await ui('pane-pin', pane(await props(), chatA).id);
await http('POST', `/api/ui-sessions/${chatA}/rename`, { title: 'Fruit list and directory' }, token);
const chatC = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Second chat' }, token)).body?.id;
const chatD = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Third chat' }, token)).body?.id;
p = await until('three panes, each narrow', (q) => [chatA, chatC, chatD].every((id) => pane(q, id)?.narrow) && pane(q, chatA).title === 'Fruit list and directory');
if (p) check('three 378px panes fold their headers', true);
a = pane(p, chatA);
check('a narrow header shows the short title with no 160 cap', a.titleMax > 1000 && a.title === a.shortTitle, a);
await snap('2.2-rest');
// Where things are in a narrow header (26px) and its menu.
const nHeadY = (q) => WB.y + q.y + 1 + 12.5;
const moreX = (q) => WB.x + q.x + q.w - 1 - 4 - 27.7 - 4 - 12.5;
const menuRight = (q) => WB.x + q.x + q.w - 1 - 4;
const menuY = (q) => WB.y + q.y + 1 + 27 + 19.5;          // the middle of its 21px row
const pinX = (q) => menuRight(q) - 1 - 8 - 13.5;
const resetX = (q) => pinX(q) - 13.5 - 6 - 30.8 - 6 - 12.5;
// "…" opens this header's menu, and focuses the pane.
await mouse('click', moreX(a), nHeadY(a));
p = await until('"…" opens the menu', (q) => pane(q, chatA).menu && pane(q, chatA).focused);
if (p) check('"…" opens the menu and focuses its pane', true);
await snap('2.2-menu-open');
// Opening another pane's closes this one.
const c = pane(p, chatC);
await mouse('click', moreX(c), nHeadY(c));
p = await until('one menu at a time', (q) => pane(q, chatC).menu && !pane(q, chatA).menu);
if (p) check('opening one menu closes the other', true);
await mouse('click', moreX(c), nHeadY(c));
await until('"…" again closes it', (q) => !pane(q, chatC).menu) && check('"…" toggles its menu', true);
// Items that act and leave it open: pin, reset (after A+).
await mouse('click', moreX(a), nHeadY(a));
await until('menu open', (q) => pane(q, chatA).menu);
await mouse('click', pinX(a), menuY(a));
p = await until('pin in the menu pins', (q) => pane(q, chatA).pinned);
check('pin acts and the menu stays open', p && pane(p, chatA).menu, p?.panes);
await snap('2.2-menu-pinned');
await mouse('click', pinX(a), menuY(a));
await until('unpin from the menu', (q) => !pane(q, chatA).pinned);
await ui('pane-font', a.id, 1);
await mouse('click', resetX(a), menuY(a));
p = await until('reset in the menu', (q) => Math.abs(pane(q, chatA).fs - 1) < 1e-3);
check('reset acts and the menu stays open', p && pane(p, chatA).menu, p?.panes);
// A live update leaves it open.
await http('POST', `/api/ui-sessions/${chatA}/rename`, { title: 'Fruit list renamed' }, token);
p = await until('a live rename shows', (q) => pane(q, chatA).title === 'Fruit list renamed');
check('the menu stays open through a live update', p && pane(p, chatA).menu);
// A chat-secrets event changes the count inside the open menu, and leaves it open.
if (ids.length === 2 && ids.every(Boolean)) {
  await http('POST', '/api/secrets/chat', { action: 'grant', sessionId: chatA, secrets: [{ id: ids[0], pin: PIN }], untilChatEnds: true }, token);
  p = await until('a grant counts 1 in the menu', (q) => pane(q, chatA).secretsN === 1);
  check('the count changes live with the menu open', p && pane(p, chatA).menu, p?.panes);
  await http('POST', '/api/secrets/chat', { action: 'grant', sessionId: chatA, secrets: ids.map((id) => ({ id, pin: PIN })), untilChatEnds: true }, token);
  p = await until('two grants count 2 in the menu', (q) => pane(q, chatA).secretsN === 2);
  check('the menu stays open through chat-secrets updates', p && pane(p, chatA).menu, p?.panes);
  await snap('2.2-menu-secrets2-live');
}
// Esc closes the menu, and only the menu.
await key('Escape');
p = await until('Esc closes the menu', (q) => !pane(q, chatA).menu);
check('Esc closes the menu only', p && p.full === null);
// The lock closes it (and opens the secrets dialog, 2.12).
if (pane(await props(), chatA).secrets) {
  await mouse('click', moreX(a), nHeadY(a));
  await until('menu open', (q) => pane(q, chatA).menu);
  const aMinusLeft = resetX(a) - 12.5 - 6 - 25.3;
  await mouse('click', aMinusLeft - 6 - 30, menuY(a));
  await until('the lock closes the menu', (q) => !pane(q, chatA).menu) && check('the lock closes the menu', true);
}
// A press outside closes it and still acts: here it opens another pane's rename.
await mouse('click', moreX(a), nHeadY(a));
await until('menu open again', (q) => pane(q, chatA).menu);
const d0 = pane(await props(), chatD);
await mouse('click', WB.x + d0.x + 1 + 43 + 20, nHeadY(d0));
p = await until('a press outside closes the menu', (q) => !pane(q, chatA).menu);
await sleep(300);
await key('Picked');
await key('Return');
await until('…and the press still acts (the rename opened)', (q) => pane(q, chatD).title === 'Picked') && check('a press outside closes the menu and still acts', true);
check('a press on a pane\'s title focuses that pane', pane(await props(), chatD).focused);
// Any press in a pane focuses it, its header buttons included.
await ui('focus-pane', pane(await props(), chatC).id);
await until('pane C focused', (q) => pane(q, chatC).focused);
await mouse('move', 800, 700);
await mouse('click', moreX(a) - 25 - 4, nHeadY(a));   // A's full screen
p = await until('a press on a header button focuses its pane', (q) => pane(q, chatA).focused || q.full?.key === 'live:' + chatA);
if (p) check('a press on a header button focuses its pane', pane(p, chatA).focused);
if (p?.full) { await key('Escape'); await until('left full screen', (q) => q.full === null); }
// Full screen closes it (and the full-screen header is wide).
await mouse('click', moreX(a), nHeadY(a));
await until('menu open', (q) => pane(q, chatA).menu);
await mouse('click', moreX(a) - 25 - 4, nHeadY(a));
p = await until('full screen closes the menu', (q) => q.full?.key === 'live:' + chatA && !pane(q, chatA).menu);
if (p) check('full screen closes the menu', true);
await key('Escape');
await until('Esc leaves full screen', (q) => q.full === null);
// × closes it and arms.
await mouse('click', moreX(a), nHeadY(a));
await until('menu open', (q) => pane(q, chatA).menu);
await mouse('click', moreX(a) + 12.5 + 4 + 13.85, nHeadY(a));
p = await until('× closes the menu and arms', (q) => !pane(q, chatA).menu && pane(q, chatA).armed);
if (p) check('× closes the menu and arms', true);
await sleep(3300);
// The menu goes with its pane: with a fourth chat the panes are 340 (the
// fourth is past the window's edge, as on the web), the third pane's menu
// open, that chat closed on the server. No other pane picks the menu up, and
// the three left stay narrow.
const chatE = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Fourth chat' }, token)).body?.id;
p = await until('four narrow panes', (q) => [chatA, chatC, chatD, chatE].every((id) => pane(q, id)?.narrow));
if (p) {
  const c3 = pane(p, chatC);
  await mouse('click', moreX(c3), nHeadY(c3));
  p = await until('the 340px pane\'s menu opens', (q) => pane(q, chatC).menu);
  if (p) check('a menu opens in a 340px pane', true);
  await snap('2.2-menu-340');
  await http('POST', `/api/ui-sessions/${chatC}/close`, {}, token);
  p = await until('its pane goes', (q) => !pane(q, chatC) && [chatA, chatD, chatE].every((id) => pane(q, id)?.narrow));
  check('the menu goes with its pane, and no other pane opens one', p && p.panes.every((x) => !x.menu), p?.panes);
  const back = pane(p, chatD);
  await mouse('click', moreX(back), nHeadY(back));
  p = await until('a menu opens again after', (q) => pane(q, chatD).menu);
  if (p) check('menus still open after a pane went', true);
  await key('Escape');
  await until('closed', (q) => !pane(q, chatD).menu);
  a = pane(await props(), chatA);
}
// The menu closes when its pane goes wide: a chat closing elsewhere leaves
// two 571px panes.
await mouse('click', moreX(a), nHeadY(a));
await until('menu open', (q) => pane(q, chatA).menu);
await http('POST', `/api/ui-sessions/${chatD}/close`, {}, token);
p = await until('the pane goes wide', (q) => !pane(q, chatD) && !pane(q, chatA).narrow);
check('the menu closes when the pane goes wide', p && !pane(p, chatA).menu, p?.panes);
check('a wide header again shows the long form or the 160 cap', p && pane(p, chatA).titleMax <= 440, p?.panes);

// The edge, with a real gutter drag: the header folds while its content box
// (pane − 8) is at most 559px, the web's @container (max-width: 559px), so a
// pane of 567 is narrow and anything over it (567.3, 568) is wide. Drag A's
// right gutter in steps across it.
{
  let q = await props();
  const L = pane(q, chatA), G = q.gutterViews.find((g) => g.row && Math.abs(g.x - (L.x + L.w)) < 1);
  if (check('a gutter right of the pane to drag', L && G, q.gutterViews)) {
    const gy = WB.y + G.y + G.h / 2;
    const target = async (w) => {
      // Drag from the gutter's current place by (w − width).
      const now = await props();
      const cur = pane(now, chatA);
      const g = now.gutterViews.find((v) => v.row && Math.abs(v.x - (cur.x + cur.w)) < 1);
      const gx = WB.x + g.x + g.w / 2;
      await mouse('move', gx, gy);
      await mouse('down', gx, gy);
      const dx = w - cur.w;
      await mouse('move', gx + dx / 2, gy);
      await mouse('move', gx + dx, gy);
      await mouse('up', gx + dx, gy);
      await sleep(300);
      return pane(await props(), chatA);
    };
    const seen = [];
    for (const w of [575, 569, 568.4, 567.6, 566, 560, 566.8, 567.4, 568.6, 571]) {
      const got = await target(w);
      seen.push({ want: w, w: got.w, narrow: got.narrow, menu: got.menu });
      if (!check(`at pane width ${got.w.toFixed(2)} the header is ${got.w <= 567 ? 'narrow' : 'wide'}`, got.narrow === (got.w <= 567), got)) break;
    }
    check('the drag reached both sides of the edge', seen.some((s) => s.w <= 567 && s.w >= 565) && seen.some((s) => s.w > 567 && s.w < 570), seen);
    // Open the menu just under the edge, then drag over it: the menu closes
    // and does not come back when the pane narrows again.
    const n = await target(563);
    if (check('dragged narrow to open a menu', n.narrow, n)) {
      await mouse('click', moreX(n), nHeadY(n));
      await until('menu open at 566', (q) => pane(q, chatA).menu);
      let g = await target(570);
      check('dragging the pane wide closes its menu', !g.narrow && !g.menu, g);
      g = await target(563);
      check('…and narrowing it again leaves the menu shut', g.narrow && !g.menu, g);
    }
  }
}

// ─── 2.3 Moving panes ───
const chatG = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Fifth chat' }, token)).body?.id;
const chatH = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Sixth chat' }, token)).body?.id;
const four = [chatA, chatE, chatG, chatH];
p = await until('four chats, four panes', (q) => four.every((id) => pane(q, id)));
const savedTree = () => saved().layouts?.demo?.root;
const shapeOf = (n) => (n.t === 'leaf' ? (n.key ? four.indexOf(n.key.slice(5)) : '∅') : `${n.dir}[${n.kids.map(shapeOf).join(', ')}]`);
const presetCase = async (name, want, boxes) => {
  await ui('add-pane', 'layout:' + name);
  const q = await until(`preset ${name}`, (x) => shapeOf(savedTree()) === want);
  if (!q) return;
  const ok = boxes.every(([i, x, y, w, h]) => { const b = pane(q, four[i]); return Math.abs(b.x - (x - WB.x)) < 1 && Math.abs(b.y - (y - WB.y)) < 1 && Math.abs(b.w - w) < 1 && Math.abs(b.h - h) < 1; });
  check(`preset ${name} with four chats: ${want}`, ok, q.panes.map((b) => [b.title, b.x + WB.x, b.y + WB.y, b.w, b.h]));
};
// Display order is the tree's: cols2 first puts the chats in order 0..3.
await ui('add-pane', 'layout:cols2');
await until('cols2', (q) => savedTree()?.t === 'split');
const order = JSON.stringify(saved().layouts.demo).match(/live:[0-9a-f]+/g).map((k) => k.slice(5));
four.splice(0, 4, ...order);
await presetCase('cross4', 'row[col[0, 2], col[1, 3]]', [[0, 246, 90, 570.5, 378.5], [2, 246, 475.5, 570.5, 378.5], [1, 823.5, 90, 570.5, 378.5], [3, 823.5, 475.5, 570.5, 378.5]]);
await presetCase('left-right2', 'row[0, col[1, 2], 3]', [[0, 246, 90, 378, 764], [1, 631, 90, 378, 378.5], [2, 631, 475.5, 378, 378.5], [3, 1016, 90, 378, 764]]);
await presetCase('top-bottom2', 'col[0, row[1, 2], 3]', [[0, 246, 90, 1148, 250], [1, 246, 347, 570.5, 250], [2, 823.5, 347, 570.5, 250], [3, 246, 604, 1148, 250]]);
// Four in a row: 340 each, the grid scrolls (panes 758 tall over its scrollbar).
await presetCase('cols3', 'row[0, 1, 2, 3]', [[0, 246, 90, 340, 758], [1, 593, 90, 340, 758], [2, 940, 90, 340, 758], [3, 1287, 90, 340, 758]]);
await snap('2.3-four-row');
// Fit that can't fit: the error toast.
await ui('add-pane', 'fit');
p = await until('Fit says it does not fit', (q) => q.toast.startsWith('Does not fit: too many panes'));
if (p) check('Fit that cannot fit shows the error toast', true);
// Back to 2 + 2: cross4. Fit after a gutter drag brings the shares back even.
await ui('add-pane', 'layout:cross4');
p = await until('cross4 again', (q) => shapeOf(savedTree()) === 'row[col[0, 2], col[1, 3]]');
const g0 = p.gutterViews.find((g) => g.row);
// Ten moves of 10px: the gutter follows every one (not only the first).
const w0 = pane(p, four[0]).w;
await mouse('move', WB.x + g0.x + 3.5, WB.y + g0.y + 200);
await mouse('down', WB.x + g0.x + 3.5, WB.y + g0.y + 200);
for (let i = 1; i <= 10; i++) await mouse('move', WB.x + g0.x + 3.5 + i * 10, WB.y + g0.y + 200);
await mouse('up', WB.x + g0.x + 103.5, WB.y + g0.y + 200);
p = await until('a gutter drag resizes', (q) => Math.abs(pane(q, four[0]).w - (w0 + 100)) < 2);
check('a gutter drag follows the pointer all the way', !!p, p?.panes?.map((x) => x.w));
await ui('add-pane', 'fit');
p = await until('Fit evens the shares', (q) => Math.abs(pane(q, four[0]).w - 570.5) < 1);
if (p) check('Fit brings the shares back even', true);
// A preset picked from the "+" menu with the mouse: the press on the menu
// focuses no pane (even the one that lands under the pointer).
await ui('focus-pane', pane(p, four[1]).id);
await until('pane 1 focused', (q) => pane(q, four[1]).focused);
await ui('add-menu', 'true');
await mouse('move', 450, 438);          // over "Layout": the submenu opens
await sleep(300);
await mouse('move', 600, 440);
await mouse('move', 620, 580);          // "Top + bottom split"
await sleep(200);
await mouse('click', 620, 580);
p = await until('the preset from the menu', (q) => shapeOf(savedTree()) === 'col[0, row[1, 2], 3]');
await sleep(400);
p = await props();
check('picking a preset with the mouse leaves the focus alone', p && pane(p, four[1]).focused, p?.panes?.map((x) => [x.title, x.focused]));
await ui('add-pane', 'layout:cross4');
p = await until('cross4 once more', (q) => shapeOf(savedTree()) === 'row[col[0, 2], col[1, 3]]');
// The hover split controls: split down on pane 0 (by the mouse). The click
// doesn't reach the pane: the focus stays where it was (pane 1).
await ui('focus-pane', pane(p, four[1]).id);
p = await until('pane 1 focused', (q) => pane(q, four[1]).focused);
let b = pane(p, four[0]);
await mouse('move', WB.x + b.x + 100, WB.y + b.y + 200);
await sleep(300);
await mouse('click', WB.x + b.x + b.w / 2 + 12, WB.y + b.y + 3 + 9);
p = await until('split down from the hover controls', (q) => q.panes.some((x) => x.kind === 'empty'));
check('a click on the split controls leaves the focus alone', p && pane(p, four[1]).focused && !pane(p, four[0]).focused);
check('split down makes an empty pane under it', p && shapeOf(savedTree()) === 'row[col[col[0, ∅], 2], col[1, 3]]' || shapeOf(savedTree()) === 'row[col[0, ∅, 2], col[1, 3]]', shapeOf(savedTree()));
await snap('2.3-split-result');
// Drag pane 3 into the empty pane: it moves, its old leaf collapses.
let empty = p.panes.find((x) => x.kind === 'empty');
b = pane(p, four[3]);
await mouse('down', WB.x + b.x + 230, WB.y + b.y + 13);
await mouse('move', WB.x + b.x + 240, WB.y + b.y + 20);
await mouse('move', WB.x + empty.x + empty.w / 2, WB.y + empty.y + empty.h / 2);
p = await until('dragging: the empty pane lights up', (q) => q.dragOn && q.panes.find((x) => x.id === empty.id)?.drop);
if (p) check('a dragged pane lights up the pane under the pointer', true);
await snap('2.3-drag-over-empty');
await mouse('up', WB.x + empty.x + empty.w / 2, WB.y + empty.y + empty.h / 2);
p = await until('the drop moves the pane', (q) => !q.dragOn && !q.panes.some((x) => x.kind === 'empty') && pane(q, four[3]).x === empty.x && pane(q, four[3]).y === empty.y);
check('a drop on an empty pane moves the pane there', !!p, shapeOf(savedTree()));
check('the dragged pane keeps the focus', p && pane(p, four[3]).focused);
// Drag pane 0 onto pane 1: they swap.
const before0 = pane(p, four[0]), before1 = pane(p, four[1]);
await mouse('down', WB.x + before0.x + 230, WB.y + before0.y + 13);
await mouse('move', WB.x + before0.x + 240, WB.y + before0.y + 20);
await mouse('move', WB.x + before1.x + 100, WB.y + before1.y + 100);
await snap('2.3-drag-over-chat');
await mouse('up', WB.x + before1.x + 100, WB.y + before1.y + 100);
p = await until('the drop swaps the panes', (q) => pane(q, four[0]).x === before1.x && pane(q, four[0]).y === before1.y && pane(q, four[1]).x === before0.x && pane(q, four[1]).y === before0.y);
if (p) check('a drop on another pane swaps the two', true);
await sleep(500);   // the swap's motion (220ms) settles before the next press
// Esc cancels a drag.
b = pane(p, four[0]);
await mouse('down', WB.x + b.x + 230, WB.y + b.y + 13);
await mouse('move', WB.x + b.x + 240, WB.y + b.y + 20);
await mouse('move', WB.x + before0.x + 100, WB.y + before0.y + 100);
await until('dragging', (q) => q.dragOn);
await key('Escape');
await mouse('up', WB.x + before0.x + 100, WB.y + before0.y + 100);
p = await until('Esc puts it back', (q) => !q.dragOn && pane(q, four[0]).x === b.x && pane(q, four[0]).y === b.y);
if (p) check('Esc cancels a drag with no change', true);
// A pinned pane can be dragged but is refused on release.
await ui('pane-pin', pane(p, four[0]).id);
p = await until('pinned', (q) => pane(q, four[0]).pinned);
check('a pinned pane locks its dividers', p && p.lockedGutters > 0);
b = pane(p, four[0]);
const other = pane(p, four[2]);
await mouse('down', WB.x + b.x + 230, WB.y + b.y + 13);
await mouse('move', WB.x + b.x + 240, WB.y + b.y + 20);
await mouse('move', WB.x + other.x + 100, WB.y + other.y + 100);
await mouse('up', WB.x + other.x + 100, WB.y + other.y + 100);
p = await until('a pinned pane is refused', (q) => q.toast === 'This pane is pinned. Unpin it to move it.');
check('dropping a pinned pane is refused with the toast', p && pane(p, four[0]).x === b.x && pane(p, four[0]).y === b.y);
await ui('pane-pin', b.id);
await until('unpinned', (q) => !pane(q, four[0]).pinned);
// The empty pane: split right, then its Terminal chip puts a launcher in that pane; × removes a pane.
b = pane(await props(), four[1]);
await ui('split', b.id, 'false');
p = await until('an empty pane', (q) => q.panes.some((x) => x.kind === 'empty'));
empty = p.panes.find((x) => x.kind === 'empty');
await snap('2.3-empty');
await mouse('click', WB.x + empty.x + empty.w / 2, WB.y + empty.y + empty.h / 2 - 6.5);   // the chip: the column is centred
p = await until('the chip starts a launcher in that pane', (q) => q.panes.find((x) => x.id === empty.id)?.kind === 'launcher');
if (p) check('the Terminal chip puts a launcher in the clicked pane', true);
await ui('close-pane', empty.id);
p = await until('the launcher closes', (q) => !q.panes.some((x) => x.kind === 'launcher'));
await ui('split', b.id, 'true');
p = await until('another empty pane', (q) => q.panes.some((x) => x.kind === 'empty'));
empty = p.panes.find((x) => x.kind === 'empty');
await mouse('click', WB.x + empty.x + empty.w / 2 + 78, WB.y + empty.y + empty.h / 2 + 31.5);   // its ×
p = await until('× removes the empty pane', (q) => !q.panes.some((x) => x.kind === 'empty'));
if (p) check('the empty pane\'s × removes it', true);
// The hover split controls again: split right on pane 2 (by the mouse). A new
// empty pane right of it, same top; the focus stays where it was.
const shapeBeforeSplit = shapeOf(savedTree());
await ui('focus-pane', pane(await props(), four[1]).id);
p = await until('pane 1 focused (split right)', (q) => pane(q, four[1]).focused);
b = pane(p, four[2]);
await mouse('move', WB.x + b.x + 100, WB.y + b.y + 200);
await sleep(300);
await mouse('click', WB.x + b.x + b.w / 2 - 12, WB.y + b.y + 3 + 9);   // the left button: Split right
p = await until('split right from the hover controls', (q) => q.panes.some((x) => x.kind === 'empty'));
empty = p?.panes.find((x) => x.kind === 'empty');
check('split right puts an empty pane right of it, same top', p && Math.abs(empty.y - pane(p, four[2]).y) < 1 && empty.x > pane(p, four[2]).x && Math.abs(empty.x - (pane(p, four[2]).x + pane(p, four[2]).w + 7)) < 1, p && [empty, pane(p, four[2])]);
check('a split right shares that row evenly (340 + 7 + 340 at least)', p && Math.abs(empty.w - pane(p, four[2]).w) < 1 && empty.w >= 340, p && [empty.w, pane(p, four[2]).w]);
check('a click on Split right leaves the focus alone', p && pane(p, four[1]).focused && !pane(p, four[2]).focused);
// The empty pane's Terminal chip (by the mouse) puts a launcher in that very
// pane, and the chat the launcher starts (UI Mode) takes the same leaf.
await mouse('click', WB.x + empty.x + empty.w / 2, WB.y + empty.y + empty.h / 2 - 6.5);
p = await until('the chip: a launcher in the clicked pane', (q) => q.panes.find((x) => x.id === empty.id)?.kind === 'launcher');
if (p) check('the Terminal chip starts a launcher in that pane', true);
const sessionsBefore = (await http('GET', '/api/ui-sessions', null, token)).body?.sessions?.map((s) => s.id) || [];
await ui('launch-chat', empty.id);
let started;
p = await until('UI Mode starts a chat in the launcher\'s leaf', async (q) => {
  const x = q.panes.find((y) => y.id === empty.id);
  if (x?.kind !== 'chat') return false;
  started = x.key.slice(5);
  return !q.panes.some((y) => y.kind === 'launcher' || y.kind === 'empty');
});
check('the chat started from the chip takes the launcher\'s leaf (same place, same size)', p && Math.abs(pane(p, started).x - empty.x) < 1 && Math.abs(pane(p, started).y - empty.y) < 1 && Math.abs(pane(p, started).w - empty.w) < 1, p && pane(p, started));
check('…and it is a new chat on the server, in this project', started && !sessionsBefore.includes(started) && (await serverChat(started))?.project === 'demo', started);
check('the chat started from the chip has the focus', p && pane(p, started)?.focused);
// Closed on the server, its leaf collapses: the tree is as before the split.
await http('POST', `/api/ui-sessions/${started}/close`, {}, token);
p = await until('the started chat goes, its leaf collapses', (q) => !pane(q, started) && !q.panes.some((x) => x.kind === 'empty') && shapeOf(savedTree()) === shapeBeforeSplit);
if (p) check('a chat closed on the server collapses its leaf', true);

// Each preset with fewer chats than slots, in a second project with two
// chats: the spare slots are empty panes. Each project keeps its own layout.
const proj2 = await http('POST', '/api/projects', { alias: 'other', name: 'Other', path: join(tmpdir(), 'crundi-native-other2-' + Date.now()), create: true }, token);
check('a second project', proj2.status < 300, proj2.body);
const chatP = (await http('POST', '/api/ui-sessions/create', { project: 'other', title: 'Other one' }, token)).body?.id;
const chatQ = (await http('POST', '/api/ui-sessions/create', { project: 'other', title: 'Other two' }, token)).body?.id;
const demoGeo = (q) => four.map((id) => { const x = pane(q, id); return x && [Math.round(x.x), Math.round(x.y), Math.round(x.w), Math.round(x.h)]; });
const demoTreeBefore = JSON.stringify(savedTree());
const demoGeoBefore = demoGeo(await props());
await ui('select-project', 'other');
p = await until('the other project: its two chats', (q) => q.currentProject === 'other' && pane(q, chatP) && pane(q, chatQ) && !pane(q, four[0]));
const two = [chatP, chatQ];
const otherTree = () => saved().layouts?.other?.root;
const shape2 = (n) => (n.t === 'leaf' ? (n.key ? two.indexOf(n.key.slice(5)) : '∅') : `${n.dir}[${n.kids.map(shape2).join(', ')}]`);
await ui('add-pane', 'layout:cols2');
await until('other: cols2', (q) => otherTree()?.t === 'split');
two.splice(0, 2, ...JSON.stringify(saved().layouts.other).match(/live:[0-9a-f]+/g).map((k) => k.slice(5)));
const preset2 = async (name, want, more) => {
  await ui('add-pane', 'layout:' + name);
  const q = await until(`other: preset ${name}`, () => shape2(otherTree()) === want);
  if (!q) return;
  const qq = await props();
  const empties = qq.panes.filter((x) => x.kind === 'empty');
  check(`preset ${name} with two chats: ${want}`, more(qq, empties), qq.panes.map((x) => [x.kind, x.title, x.x + WB.x, x.y + WB.y, x.w, x.h]));
};
await preset2('cols3', 'row[0, 1, ∅]', (q, e) => e.length === 1 && [pane(q, two[0]), pane(q, two[1]), e[0]].every((x, i) => Math.abs(x.x + WB.x - [246, 631, 1016][i]) < 1 && Math.abs(x.w - 378) < 1 && Math.abs(x.h - 764) < 1));
await preset2('cross4', 'row[col[0, ∅], col[1, ∅]]', (q, e) => e.length === 2 && Math.abs(pane(q, two[1]).x + WB.x - 823.5) < 1 && Math.abs(pane(q, two[0]).h - 378.5) < 1);
await preset2('left-right2', 'row[0, col[1, ∅]]', (q, e) => e.length === 1 && Math.abs(pane(q, two[0]).w - 570.5) < 1 && Math.abs(pane(q, two[0]).h - 764) < 1 && Math.abs(e[0].y + WB.y - 475.5) < 1);
await preset2('top-bottom2', 'col[0, row[1, ∅]]', (q, e) => e.length === 1 && Math.abs(pane(q, two[0]).w - 1148) < 1 && Math.abs(pane(q, two[0]).h - 378.5) < 1 && Math.abs(e[0].x + WB.x - 823.5) < 1);
await preset2('cols2', 'row[0, 1]', (q, e) => e.length === 0 && Math.abs(pane(q, two[0]).w - 570.5) < 1 && Math.abs(pane(q, two[1]).x + WB.x - 823.5) < 1);
await snap('2.3-other-cols2');
// Sizes by a gutter drag (left 120px wider), saved for this project.
p = await props();
const go = p.gutterViews.find((g) => g.row);
const wo = pane(p, two[0]).w;
await mouse('move', WB.x + go.x + 3.5, WB.y + go.y + 300);
await mouse('down', WB.x + go.x + 3.5, WB.y + go.y + 300);
for (let i = 1; i <= 6; i++) await mouse('move', WB.x + go.x + 3.5 + i * 20, WB.y + go.y + 300);
await mouse('up', WB.x + go.x + 123.5, WB.y + go.y + 300);
p = await until('other: the gutter drag resizes', (q) => Math.abs(pane(q, two[0]).w - (wo + 120)) < 2);
const otherSizes = JSON.stringify(otherTree()?.sizes);
check('the end of a gutter drag saves the sizes for that project', p && otherTree()?.sizes && Math.abs(otherTree().sizes[0] - otherTree().sizes[1]) > 5, otherTree());
const otherGeo = (q) => two.map((id) => { const x = pane(q, id); return x && [Math.round(x.x), Math.round(x.w)]; });
const otherGeoBefore = otherGeo(await props());
// Back to Demo: its own layout, untouched.
await ui('select-project', 'demo');
p = await until('back on Demo: its four panes as they were', (q) => q.currentProject === 'demo' && JSON.stringify(demoGeo(q)) === JSON.stringify(demoGeoBefore));
check('each project keeps its own layout', p && JSON.stringify(savedTree()) === demoTreeBefore && JSON.stringify(otherTree()?.sizes) === otherSizes);
// Uneven sizes on Demo too (its column gutter 80px left), to see them come back after the restart.
p = await props();
const gd = p.gutterViews.find((g) => g.row);
const leftOf = four.find((id) => { const x = pane(p, id); return Math.abs(x.x + x.w - gd.x) < 1 && x.y >= gd.y - 1 && x.y < gd.y + gd.h; });
const wd = pane(p, leftOf).w;
await mouse('move', WB.x + gd.x + 3.5, WB.y + gd.y + 200);
await mouse('down', WB.x + gd.x + 3.5, WB.y + gd.y + 200);
for (let i = 1; i <= 4; i++) await mouse('move', WB.x + gd.x + 3.5 - i * 20, WB.y + gd.y + 200);
await mouse('up', WB.x + gd.x - 76.5, WB.y + gd.y + 200);
p = await until('Demo: the gutter drag resizes', (q) => Math.abs(pane(q, leftOf).w - (wd - 80)) < 2);
if (p) check('Demo: uneven sizes by a gutter drag', true);
// The layout, sizes included, survives a restart, per project.
const kept = JSON.stringify(savedTree());
const geo = (q) => four.map((id) => { const x = pane(q, id); return [Math.round(x.x), Math.round(x.y), Math.round(x.w), Math.round(x.h)]; });
const geoBefore = geo(await props());
d.close();
app.kill();
await sleep(1500);
app = launch('run3');
d = await driver();
p = await until('after a restart: the same layout', (q) => q.signedIn && four.every((id) => pane(q, id)) && JSON.stringify(geo(q)) === JSON.stringify(geoBefore), 30000);
check('the layout survives a restart', p && JSON.stringify(savedTree()) === kept);
check('…with the sizes a gutter drag left (not even)', p && Math.abs(pane(p, leftOf).w - (wd - 80)) < 2, p && geo(p));
await ui('select-project', 'other');
p = await until('after a restart: the other project\'s sizes', (q) => q.currentProject === 'other' && JSON.stringify(otherGeo(q)) === JSON.stringify(otherGeoBefore));
check('the other project\'s sizes survive a restart too', p && JSON.stringify(otherTree()?.sizes) === otherSizes, p && otherGeo(p));

// ─── 2.4 Live state ───
// Real Claude turns on the test server; each wait is long enough for one.
const send = (id, text) => http('POST', `/api/ui-sessions/${id}/send`, { text }, token);
const make = async (project, title, skip) => (await http('POST', '/api/ui-sessions/create', { project, title, skipPermissions: !!skip }, token)).body?.id;
const dot = (q, alias) => q.projectDots?.find((x) => x.alias === alias)?.state;
// A chat's claude process: the one whose environment names the chat.
const chatProc = (id) => {
  for (const pid of readdirSync('/proc').filter((x) => /^\d+$/.test(x))) {
    try {
      const env = readFileSync(`/proc/${pid}/environ`, 'utf8').split('\0');
      if (env.includes('CRUNDI_CHAT_ID=' + id) && readFileSync(`/proc/${pid}/cmdline`, 'utf8').includes('claude')) {
        return { pid: Number(pid), key: (env.find((e) => e.startsWith('CRUNDI_API_KEY=')) || '').slice(15) };
      }
    } catch { /* gone, or not ours */ }
  }
  return null;
};
await http('POST', '/api/projects', { alias: 'other', name: 'Other', path: join(tmpdir(), 'crundi-native-other2'), create: true }, token);
// Demo on screen (an earlier part may have left another project open).
await ui('select-project', 'demo');
await until('Demo is open', (q) => q.currentProject === 'demo');
// Working: a foreground wait (a plain sleep is refused by this machine's shell hook).
const liveW = await make('demo', 'Long job', true);
await send(liveW, 'Run exactly this shell command in the foreground with timeout 600000 and wait for it to finish: timeout 90 tail -f /dev/null. It is a deliberate wait for a UI test; do not run it in the background. Then say done.');
p = await until('a working chat', (q) => pane(q, liveW)?.state === 'working' && dot(q, 'demo') === 'working', 120000);
if (p) check('a working chat: its pane and the project dot say working', true);
await snap('2.4-working');
// Needs input: a question. Needs input wins over working on the project's dot.
const liveQ = await make('demo', 'Question', false);
await send(liveQ, 'Use the AskUserQuestion tool to ask me one question: "Which colour do you prefer?" with the options Red, Green and Blue. Do nothing else.');
p = await until('a question: needs input', (q) => pane(q, liveQ)?.state === 'needs-input' && dot(q, 'demo') === 'needs-input', 120000);
if (p) check('a question makes the pane and the project needs input (over working)', true);
await snap('2.4-needs-input');
// Waiting, in another (inactive) project: a background task armed, the turn over.
const liveO = await make('other', 'Background', true);
await send(liveO, 'Run the shell command `timeout 120 tail -f /dev/null` with run_in_background set to true. Do not wait for it; just reply "started" and end your turn.');
p = await until('the other project waits', (q) => dot(q, 'other') === 'waiting', 120000);
if (p) check('an inactive project shows waiting', dot(p, 'demo') === 'needs-input');
await snap('2.4-sidebar-mixed');
// The answer is not drawn ahead of the server: the project goes back to working (W still runs).
const qEntry = (await d.send({ cmd: 'state' })).state;
const qe = await (async () => { for (let i = 0; i < 20; i++) { const st = (await http('GET', '/api/ui-sessions', null, token)).body?.sessions?.find((x) => x.id === liveQ); const pend = st?.pending?.[0]; if (pend) return pend; await sleep(500); } return null; })();
if (check('the question is pending on the server', !!qe, qEntry?.chat)) {
  const input = { ...(qe.input || {}), answers: { 'Which colour do you prefer?': 'Green' } };
  await http('POST', `/api/ui-sessions/${liveQ}/respond`, { requestId: qe.requestId, behavior: 'allow', updatedInput: input }, token);
  p = await until('answered: needs input ends', (q) => pane(q, liveQ)?.state !== 'needs-input' && dot(q, 'demo') === 'working', 60000);
  if (p) check('after the answer the project is working again (the other chat still runs)', true);
}
// Exited: its claude process ends; the pane goes grey even though its stream last said idle.
const procLW = chatProc(liveW);
if (check('the working chat has a claude process', !!procLW)) {
  process.kill(procLW.pid);
  p = await until('the killed chat is exited', (q) => pane(q, liveW)?.state === 'exited', 30000);
  if (p) check('a chat whose process ended is exited on its pane', true);
  await snap('2.4-exited');
  p = await until('…and no longer counts on the project dot', (q) => dot(q, 'demo') !== 'working', 30000);
  if (p) check('an exited chat no longer counts on the project dot', true);
}
// Claude names its chat (rename_chat): the short title live, the long one in a wide header.
const procLQ = chatProc(liveQ) || chatProc(liveO);
if (check('a running chat to take the internal key from', !!procLQ?.key)) {
  const mcp = async (args) => (await fetch(SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'x-api-key': procLQ.key }, body: JSON.stringify({ tool: 'rename_chat', args }) })).json();
  const r = await mcp({ sessionId: liveQ, title: 'Named by Claude', long: 'A title Claude gave this chat itself, long form' });
  check('rename_chat answers ok', r?.ok, r);
  p = await until('the server-set title shows live', (q) => pane(q, liveQ)?.shortTitle === 'Named by Claude', 15000);
  if (p) check('a title set by the server shows live', true);
  await ui('pane-full', pane(p, liveQ).id);
  p = await until('full screen shows the long form', (q) => q.full?.title === 'A title Claude gave this chat itself, long form', 10000);
  if (p) check('a wide header shows the long form', true);
  await key('Escape');
  await until('left full screen', (q) => q.full === null);
  // Renamed in another client: the typed title shows, and a later automatic one is refused.
  await http('POST', `/api/ui-sessions/${liveQ}/rename`, { title: 'Renamed in another client' }, token);
  p = await until('a rename from another client shows live', (q) => pane(q, liveQ)?.title === 'Renamed in another client', 15000);
  if (p) check('a rename in another client shows live', true);
  const r2 = await mcp({ sessionId: liveQ, title: 'Claude tries again', long: 'x' });
  await sleep(1500);
  check('after a typed rename, the automatic one is refused and the title stays', r2?.ok === false && pane(await props(), liveQ).title === 'Renamed in another client', r2);
  await snap('2.4-titles-live');
}
for (const id of [liveW, liveQ, liveO]) await http('POST', `/api/ui-sessions/${id}/close`, {}, token);

d.close();
app.kill();
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
