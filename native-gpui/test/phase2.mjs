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
//   2.4 live state: real Claude turns make a chat working, needs input
//       (a question), waiting (a background task), idle and exited; each
//       pane's dot and badge are checked in the app's own picture, with the
//       project dot (needs input over working; running; none, active or
//       not) and the folded rail's initials and avatar rings; titles set by
//       Claude (rename_chat, the long form in a wide header) and renames in
//       another client arrive live.
//   2.20 a factory's result card, on a real run (a git project, a factory,
//       its finish handed off through the MCP bridge): the Manager's pane and
//       its Factory tag, the card's parts, files and the full diff,
//       follow-ups, Discard's arming, a refused choice, Request changes (an
//       empty note, a note), a second finish asking again in place, Merge
//       and Undo (dev's commits); a widget check's screenshots row.
//   2.16f live frames: a stored click test in its own view (painted, the
//       box less its edge, a fixed height kept), a click through the window
//       (its title, the page green), typing in it (the composer gets none,
//       Esc is its), a click outside, a new-window link to the opener,
//       released when scrolled off and fresh when back, the cap of 6, the
//       chat closed.
//   2.24 voice input, against tools/deepgram-stub.mjs (skipped without it):
//       the mic shown, voice mode (frames at the stub, words in the box, an
//       utterance sent, scratch that, send now, off with the draft back),
//       push to talk (sent; slid onto × and dropped), a hold pausing another
//       chat's voice mode, voice mode moving, Esc, the server's error,
//       replies said (once, and only when they should be).
//   2.22 the resume picker, on seeded earlier conversations: the list, the
//       keyboard and its scroll, Esc, the heavy choice and Back, compact and
//       Skip Permissions resumes (their create calls, the launcher's leaf),
//       loading, the list failing, an empty list, a refused create, a 404.
//   2.23 parked panes: the card in the same leaf, its stored conversation,
//       Peek / Esc / Parked view, a choice running and refused, the heavy
//       warning, the short and very short forms, a send from Peek resuming
//       in place, Compact and resume, × discarding.
//
// Needs a FRESH test server with no sign-in set up (this sets a password on
// it), started through tools/test-server.sh <port> --bare, never by hand; it
// refuses a server with a Telegram bot configured:
//
//   native-gpui/tools/test-server.sh 4811 --bare
//   CRUNDI_TEST_SERVER=http://localhost:4811 NATIVE_BIN=… DISPLAY=… node native-gpui/test/phase2.mjs
//
// Pictures go to DRIVE_OUT (default: the temp dir).

import net from 'node:net';
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, openSync, readFileSync, readdirSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { totpNow } from '../../src/auth-password.js';
import { readPng, count } from './png.mjs';

const SERVER = process.env.CRUNDI_TEST_SERVER || 'http://localhost:8889';
const BIN = process.env.NATIVE_BIN;
const PORT = Number(process.env.CRUNDI_TEST_PORT || 7612);
const OUT = process.env.DRIVE_OUT || tmpdir();
mkdirSync(OUT, { recursive: true });
if (!BIN) { console.error('Set NATIVE_BIN'); process.exit(2); }

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : '  ' + JSON.stringify(extra).slice(0, 400)}`); if (!ok) failed++; return ok; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// The whole run outlasts the server's 15-minute access token (the GPUI
// client gets through every section): on a 401 for the test's own token,
// renew it once with the refresh token from the setup and try again.
let refreshTok = '';
async function http(method, path, body, tok) {
  const go = (t) => fetch(SERVER + path, {
    method,
    headers: { 'Content-Type': 'application/json', ...(t ? { Authorization: 'Bearer ' + t } : {}) },
    body: body ? JSON.stringify(body) : undefined,
  });
  let res = await go(tok);
  if (res.status === 401 && tok && tok === token && refreshTok) {
    const r = await fetch(SERVER + '/api/auth/refresh', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ refreshToken: refreshTok }) });
    const j = await r.json().catch(() => null);
    if (j?.ok && j.token) { token = j.token; refreshTok = j.refreshToken; res = await go(token); }
  }
  return { status: res.status, body: await res.json().catch(() => null) };
}

// ─── The server: a password, a project, two chats ───
const PASSWORD = 'native-client-test-' + Math.random().toString(36).slice(2, 10);
const methods = await http('GET', '/api/auth/methods');
// Never against a server running the LIVE Telegram bot (checked again below,
// once signed in): a test server comes from tools/test-server.sh <port> --bare.
if (methods.body?.telegram || methods.body?.telegramBotId) {
  console.error(`Refusing ${SERVER}: it has a Telegram bot configured. Start a test server with native-gpui/tools/test-server.sh <port> --bare.`);
  process.exit(2);
}
if (!methods.body?.setupRequired) { console.error('This needs a fresh dev server (no sign-in set up yet)'); process.exit(2); }
const setup = await http('POST', '/api/auth/setup', { method: 'password', password: PASSWORD });
check('the server takes a password', setup.body?.ok && setup.body.totpSecret, setup.body);
const SECRET = setup.body.totpSecret;
let token = setup.body.token;
refreshTok = setup.body.refreshToken || '';

// No bot behind the sign-in either (an empty .env, no TELEGRAM_BOT_TOKEN).
const status = await http('GET', '/api/status', null, token);
if (status.body?.botConfigured !== false) {
  console.error(`Refusing ${SERVER}: it has a Telegram bot configured (botConfigured=${status.body?.botConfigured}). Start a test server with native-gpui/tools/test-server.sh <port> --bare.`);
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
// The microphone (2.24): a WAV the app reads instead of a device, a soft
// 180 Hz tone, 3 s at 16 kHz.
const MIC_WAV = join(OUT, 'mic.wav');
{
  const n = 16000 * 3, b = Buffer.alloc(44 + n * 2);
  b.write('RIFF', 0); b.writeUInt32LE(36 + n * 2, 4); b.write('WAVEfmt ', 8); b.writeUInt32LE(16, 16); b.writeUInt16LE(1, 20); b.writeUInt16LE(1, 22);
  b.writeUInt32LE(16000, 24); b.writeUInt32LE(32000, 28); b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34); b.write('data', 36); b.writeUInt32LE(n * 2, 40);
  for (let i = 0; i < n; i++) b.writeInt16LE(Math.round(Math.sin((i / 16000) * 2 * Math.PI * 180) * 6000), 44 + i * 2);
  writeFileSync(MIC_WAV, b);
}
function launch(tag) {
  const log = openSync(join(OUT, `native2-${tag}.log`), 'w');
  return spawn(BIN, [], { env: { ...process.env, CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: configDir, CRUNDI_URL_A: 'about:blank', CRUNDI_URL_B: 'about:blank', CRUNDI_TEST_MIC_WAV: MIC_WAV, CRUNDI_TEST_UPLOAD_KBPS: '256', CRUNDI_BROWSER_UNLOAD_SECS: process.env.CRUNDI_BROWSER_UNLOAD_SECS || '3' }, stdio: ['ignore', log, log] });
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

// The real X pointer parked outside the window (it is moved over it only
// for the real wheel check, 2.16): a pointer resting on the window gives it
// hover and enter events the driven checks do not expect.
const parkPointer = async () => {
  try { (await import('node:child_process')).execFileSync('xdotool', ['mousemove', '1439', '899'], { env: { ...process.env, DISPLAY: process.env.DISPLAY || ':99' } }); } catch { /* no xdotool: nothing to park */ }
};
await parkPointer();
let app = launch('run1');
// Never leave the app behind (it would hold the driver's port for the next run).
process.on('exit', () => { try { app.kill(); } catch {} });
// Every temporary folder the run makes goes when it ends, failed or not
// (the attachments' files piled up to gigabytes in /tmp otherwise). A
// timeout's SIGTERM or a Ctrl+C ends through process.exit, so they go too.
// A run killed from outside (timeout, SIGKILL) skips the exit hook below:
// folders of earlier runs older than an hour go first (F54).
for (const f of readdirSync(tmpdir()).filter((x) => /^crundi-attach-/.test(x))) {
  try { const p = join(tmpdir(), f); const st = (await import('node:fs')).statSync(p); if (Date.now() - st.mtimeMs > 3600e3) rmSync(p, { recursive: true, force: true }); } catch { /* not ours, or gone */ }
}
const tmpDirs = [];
const tmpDir = (prefix) => { const t = mkdtempSync(join(tmpdir(), prefix)); tmpDirs.push(t); return t; };
process.on('exit', () => { for (const t of tmpDirs) { try { rmSync(t, { recursive: true, force: true }); } catch {} } });
for (const sig of ['SIGTERM', 'SIGINT']) process.on(sig, () => process.exit(1));
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
  // …and opens the secrets dialog (2.12): closed again before going on.
  await until('the secrets dialog opens', (q) => q.secretsDialog, 5000) && check('the lock opens the secrets dialog', true);
  await key('Escape');
  await until('the dialog closed', (q) => q.secretsDialog === null, 5000);
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
// The empty pane's centred column (3.1): the message, the chip rows (24 high,
// 6 apart; a chip is 1+10+icon 14+6+label+10+1), then Split ▸ / Split ▾ / ×.
// One row of chips puts the first chip 6.5 above the centre and × 31.5 below
// it; each more row moves the chips up 15 and × down 15. The Terminal chip
// leads the first row: its icon is 18 from the row's left.
const chipRows = (e) => (e.chips && e.chips.length) ? e.chips : [[{ id: 'terminal', label: 'Terminal' }]];
const chipLabelW = (l) => l.length * 6.6;
const terminalChipAt = (e) => {
  const rows = chipRows(e), first = rows[0];
  const rowW = first.reduce((a, c) => a + 42 + chipLabelW(c.label), 0) + 6 * (first.length - 1);
  return [WB.x + e.x + e.w / 2 - rowW / 2 + 18, WB.y + e.y + e.h / 2 - 6.5 - 15 * (rows.length - 1)];
};
const emptyCloseAt = (e) => [WB.x + e.x + e.w / 2 + 78, WB.y + e.y + e.h / 2 + 31.5 + 15 * (chipRows(e).length - 1)];
// The empty pane: split right, then its Terminal chip puts a launcher in that pane; × removes a pane.
b = pane(await props(), four[1]);
await ui('split', b.id, 'false');
p = await until('an empty pane', (q) => q.panes.some((x) => x.kind === 'empty'));
empty = p.panes.find((x) => x.kind === 'empty');
await snap('2.3-empty');
await mouse('click', ...terminalChipAt(empty));   // the Terminal chip
p = await until('the chip starts a launcher in that pane', (q) => q.panes.find((x) => x.id === empty.id)?.kind === 'launcher');
if (p) check('the Terminal chip puts a launcher in the clicked pane', true);
await ui('close-pane', empty.id);
p = await until('the launcher closes', (q) => !q.panes.some((x) => x.kind === 'launcher'));
await ui('split', b.id, 'true');
p = await until('another empty pane', (q) => q.panes.some((x) => x.kind === 'empty'));
empty = p.panes.find((x) => x.kind === 'empty');
await mouse('click', ...emptyCloseAt(empty));   // its ×
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
await mouse('click', ...terminalChipAt(empty));
p = await until('the chip: a launcher in the clicked pane', (q) => q.panes.find((x) => x.id === empty.id)?.kind === 'launcher');
if (p) check('the Terminal chip starts a launcher in that pane', true);
const sessionsBefore = (await http('GET', '/api/ui-sessions', null, token)).body?.sessions?.map((s) => s.id) || [];
await ui('launch-chat', empty.id);
// (Earlier conversations in this project: the resume picker first, 2.22.)
if (await until('the launch goes on or asks which conversation', (q) => q.resume || q.panes.find((y) => y.id === empty.id)?.kind === 'chat', 10000).then((q) => q?.resume)) await ui('resume-new');
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
// What is drawn, from the app's snapshot: the state colours (theme.slint).
const C = { green: '#10b981', yellow: '#f59e0b', accent: '#6366f1', hover: '#818cf8', sky: '#38bdf8', muted: '#5a5a78' };
const shot = async () => readPng(Buffer.from((await d.send({ cmd: 'snapshot' })).data, 'base64'));
const tally = (img, box) => Object.fromEntries(Object.entries(C).map(([k, c]) => [k, count(img, ...box, c)]));
// A pane's header: the dot (8px, 25-45 from the pane's left) and what follows
// it (the title and the badge; the header's buttons are further right).
const headDot = (img, q) => tally(img, [WB.x + q.x + 25, headY(q) - 7, WB.x + q.x + 45, headY(q) + 7]);
const headRest = (img, q) => tally(img, [WB.x + q.x + 46, headY(q) - 8, WB.x + q.x + Math.min(q.w - 100, 245), headY(q) + 8]);
// The sidebar's rows (34 apart from y116; the dot at x25), and the folded rail's avatars (50 apart from y124, x30).
const rowDot = (img, q, alias) => tally(img, [14, 116 + 34 * q.projectDots.findIndex((x) => x.alias === alias) - 7, 36, 116 + 34 * q.projectDots.findIndex((x) => x.alias === alias) + 7]);
const avatar = (img, q, alias) => { const y = 124 + 50 * q.projectDots.findIndex((x) => x.alias === alias); return tally(img, [6, y - 24, 54, y + 24]); };
const only = (t, k) => t[k] >= 20 && Object.entries(t).every(([n, v]) => n === k || n === 'muted' || v === 0);
const FOLD = [211, 71], UNFOLD = [30, 71];
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
{
  const img = await shot(), q = pane(p || await props(), liveW);
  const dt = headDot(img, q), rest = headRest(img, q), row = rowDot(img, p || await props(), 'demo');
  check('working: the pane dot is yellow, with a yellow WORKING badge', only(dt, 'yellow') && rest.yellow >= 20, { dt, rest });
  check('working: the project dot is yellow', only(row, 'yellow'), row);
}
// Needs input: a question. Needs input wins over working on the project's dot.
const liveQ = await make('demo', 'Question', false);
await send(liveQ, 'Use the AskUserQuestion tool to ask me one question: "Which colour do you prefer?" with the options Red, Green and Blue. Do nothing else.');
p = await until('a question: needs input', (q) => pane(q, liveQ)?.state === 'needs-input' && dot(q, 'demo') === 'needs-input', 120000);
if (p) check('a question makes the pane and the project needs input (over working)', true);
await snap('2.4-needs-input');
{
  const q0 = p || await props(), img = await shot(), q = pane(q0, liveQ);
  const row = rowDot(img, q0, 'demo'), w = headDot(img, pane(q0, liveW));
  // The fourth pane may run past the window's edge: then look at it full screen.
  if (WB.x + q.x + 250 <= 1400) {
    const dt = headDot(img, q), rest = headRest(img, q);
    check('needs input: the pane dot is accent, with a NEEDS INPUT badge (accent-hover text)', only(dt, 'accent') && rest.hover >= 50, { dt, rest });
  } else {
    await ui('pane-full', q.id);
    await until('full screen', (x) => x.full?.key === 'live:' + liveQ, 5000);
    await sleep(400);
    const fi = await shot(), dt = tally(fi, [0, 0, 60, 30]), rest = tally(fi, [61, 0, 700, 30]);
    check('needs input (full screen): an accent dot, and a NEEDS INPUT badge (accent-hover text)', dt.accent >= 20 && rest.hover >= 50, { dt, rest });
    await key('Escape');
    await until('left full screen', (x) => x.full === null);
  }
  check('…the working pane beside it stays yellow', only(w, 'yellow'), w);
  check('needs input wins on the project dot (accent)', only(row, 'accent'), row);
  // The folded rail: two-letter initials, Demo's avatar ringed in accent.
  await mouse('click', ...FOLD);
  await sleep(700);
  const f = await props(), fi = await shot(), av = avatar(fi, f, 'demo');
  check('folded: the rail shows the projects\' initials', f.projectDots.find((x) => x.alias === 'demo')?.initials === 'DE' && f.projectDots.find((x) => x.alias === 'other')?.initials === 'OT', f.projectDots);
  check('folded: a needs-input project\'s avatar has an accent ring', av.accent >= 20 && av.yellow === 0 && av.green === 0, av);
  await snap('2.4-folded-needs-input');
  await mouse('click', ...UNFOLD);
  await sleep(700);
}
// Waiting, in another (inactive) project: a background task armed, the turn over.
const liveO = await make('other', 'Background', true);
await send(liveO, 'Run the shell command `timeout 120 tail -f /dev/null` with run_in_background set to true. Do not wait for it; just reply "started" and end your turn.');
p = await until('the other project waits', (q) => dot(q, 'other') === 'waiting', 120000);
if (p) check('an inactive project shows waiting', dot(p, 'demo') === 'needs-input');
await snap('2.4-sidebar-mixed');
{
  const img = await shot(), q0 = await props(), row = rowDot(img, q0, 'other');
  check('an inactive waiting project: a sky dot', only(row, 'sky'), row);
  // Its pane: open Other, then back to Demo.
  await ui('select-project', 'other');
  const qo = await until('Other open, its chat waiting', (q) => q.currentProject === 'other' && pane(q, liveO)?.state === 'waiting', 30000);
  if (qo) {
    const im = await shot(), dt = headDot(im, pane(qo, liveO)), rest = headRest(im, pane(qo, liveO)), r2 = rowDot(im, qo, 'other'), r1 = rowDot(im, qo, 'demo');
    check('waiting: the pane dot is sky, with a sky WAITING badge', only(dt, 'sky') && rest.sky >= 20, { dt, rest });
    check('…the active row keeps its waiting (sky) dot, and Demo stays accent', only(r2, 'sky') && only(r1, 'accent'), { r2, r1 });
    await snap('2.4-waiting');
  }
  await ui('select-project', 'demo');
  await until('back on Demo', (q) => q.currentProject === 'demo' && pane(q, liveQ) && pane(q, liveW));
}
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
// Idle and exited, drawn: the answered chat ends its turn (green, no badge),
// the killed one is grey; Demo's dot is running (green: an idle chat whose
// process lives), and folded, its avatar has a green ring.
p = await until('the answered chat goes idle, Demo running', (q) => pane(q, liveQ)?.state === 'idle' && dot(q, 'demo') === 'running', 120000);
if (p) {
  const img = await shot(), qi = pane(p, liveQ), qx = pane(p, liveW);
  const di = headDot(img, qi), ri = headRest(img, qi), dx = headDot(img, qx), rx = headRest(img, qx), row = rowDot(img, p, 'demo');
  check('idle: a green dot and no badge', only(di, 'green') && ri.yellow + ri.accent + ri.hover + ri.sky === 0, { di, ri });
  check('exited: a grey dot and no badge', dx.muted >= 20 && dx.green + dx.yellow + dx.accent + dx.sky === 0 && rx.yellow + rx.accent + rx.hover + rx.sky === 0, { dx, rx });
  check('running: the project dot is green', only(row, 'green'), row);
  await snap('2.4-idle-exited');
  await mouse('click', ...FOLD);
  await sleep(700);
  const f = await props(), fi = await shot(), av = avatar(fi, f, 'demo'), ao = avatar(fi, f, 'other');
  check('folded: a running project\'s avatar has a green ring', av.green >= 20 && av.yellow === 0, av);
  // Other's background task (120 s) may have ended by now: then it is running.
  if (dot(f, 'other') === 'waiting') check('folded: a waiting project looks like one with nothing running (no sky, no ring colour)', ao.sky + ao.green + ao.yellow + ao.accent === 0, ao);
  else check('folded: an inactive running project has a green ring', dot(f, 'other') === 'running' && ao.green >= 20, { other: dot(f, 'other'), ao });
  await snap('2.4-folded-running');
  await mouse('click', ...UNFOLD);
  await sleep(700);
}
// A project with nothing in it: a grey dot; active, green; folded, "FI".
await http('POST', '/api/projects', { alias: 'fifth', name: 'Fifth', path: join(tmpdir(), 'crundi-native-fifth2'), create: true }, token);
p = await until('Fifth in the sidebar', (q) => q.projectDots.some((x) => x.alias === 'fifth'));
if (p) {
  const img = await shot(), row = rowDot(img, p, 'fifth');
  check('none, inactive: a grey project dot', dot(p, 'fifth') === '' && row.muted >= 20 && row.green + row.yellow + row.accent + row.sky === 0, row);
  check('a new project gets its initials live', p.projectDots.find((x) => x.alias === 'fifth').initials === 'FI', p.projectDots);
  await ui('select-project', 'fifth');
  const q5 = await until('Fifth open', (q) => q.currentProject === 'fifth');
  if (q5) { const r5 = rowDot(await shot(), q5, 'fifth'); check('none, active: a green project dot', only(r5, 'green'), r5); }
  await ui('select-project', 'demo');
  await until('back on Demo', (q) => q.currentProject === 'demo' && pane(q, liveQ));
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

// ─── 2.5 Composer and Stop ───
const liveC = await make('demo', 'Composer', true);
p = await until('the composer chat has a pane', (q) => pane(q, liveC), 30000);
{ const c = pane(p, liveC); check('a fresh chat: no session id, model or cost yet', c.session === '' && c.sessionFull === '' && c.model === '' && c.cost === '' && c.stopArmed === false, c); }
await ui('focus-pane', pane(p, liveC).id);
await ui('composer-focus');
const keys = (text, mods) => d.send({ cmd: 'key', text, mods });
const clearBox = async () => { await keys('a', ['Control']); await keys('Backspace'); };
const cpane = async () => pane(await props(), liveC);
check('Enter sends by default', (await props()).chatEnter === 'enter');
// Typing fills the draft; Shift+Enter makes a newline, not a send.
await keys('one'); await keys('Return', ['Shift']); await keys('two');
p = await until('Shift+Enter makes a newline', async (q) => pane(q, liveC)?.draft === 'one\ntwo', 5000);
if (p) check('typing fills the box; Shift+Enter is a newline', pane(p, liveC).state === 'idle');
// A blank box sends nothing (Send is never disabled; nothing happens).
await clearBox(); await keys('   '); await keys('Return');
await sleep(800);
{ const c = await cpane(); check('a blank box: Enter does nothing', c.draft === '   ' && c.state === 'idle', c); }
// Ctrl+Enter mode: one setting, saved; Enter makes a newline, Ctrl+Enter sends.
await ui('chat-enter', 'newline');
p = await until('the Enter mode switched', (q) => q.chatEnter === 'newline', 5000);
check('the Enter mode is saved for every chat', saved().chat_enter === 'newline', saved().chat_enter);
await clearBox();
await keys('Write the numbers from 1 to 150 as words, one per line, with no other text.');
await keys('Return');
await sleep(500);
{ const c = await cpane(); check('switched: Enter makes a newline and sends nothing', c.draft.endsWith('\n') && c.state === 'idle', c); }
await keys('Backspace');
await keys('Return', ['Control']);
{ const c = await cpane(); check('Ctrl+Enter sends: the box empties and the chat is working at once', c.draft === '' && c.state === 'working', { state: c.state, draft: c.draft, qSent: c.qSent, qHeld: c.qHeld }); }
await snap('2.5-working');
// Stop: armed by one click, disarmed after 3 s; two clicks interrupt.
p = await until('the turn is under way', (q) => pane(q, liveC)?.state === 'working', 30000);
await ui('stop');
{ const c = await cpane(); check('Stop, one click: armed ("Sure?")', c.stopArmed === true && c.state === 'working', c); }
await snap('2.5-stop-armed');
await sleep(3500);
{ const c = await cpane(); check('…and disarmed by itself after 3 s', c.stopArmed === false, c); }
if ((await cpane()).state === 'working') {
  await ui('stop'); await ui('stop');
  p = await until('two clicks on Stop interrupt the turn', (q) => pane(q, liveC)?.state === 'idle', 30000);
  if (p) check('Stop twice: the turn is interrupted and the chat idle', !pane(p, liveC).stopArmed);
}
// The session id and the model arrive live (no reload).
p = await until('the session id and model shown', (q) => pane(q, liveC)?.session?.length === 8 && pane(q, liveC)?.model !== '', 30000);
if (p) check('the meta row: session id (8 characters) and model, live', pane(p, liveC).sessionFull.startsWith(pane(p, liveC).session), pane(p, liveC));
if (p) check('the cost reads "$x.xxxx" (or nothing when 0)', /^(\$\d+\.\d{4})?$/.test(pane(p, liveC).cost), pane(p, liveC).cost);
await ui('chat-enter', 'enter');
await until('Enter sends again', (q) => q.chatEnter === 'enter', 5000);
check('…saved back', saved().chat_enter === '', saved().chat_enter);
// Exited, then a refused send: the error in the log, the text back in the box.
const procC = chatProc(liveC);
if (check('the composer chat has a claude process', !!procC)) {
  process.kill(procC.pid);
  p = await until('the chat exits', (q) => pane(q, liveC)?.state === 'exited', 30000);
  if (p) check('exited: the composer says so', true);
  await snap('2.5-exited');
  await ui('send', 'after the end');
  const st = await until('a refused send', async (q) => {
    const c = pane(q, liveC);
    const e = (await d.send({ cmd: 'state' })).state?.entries || [];
    return c?.draft === 'after the end' && e.some((x) => x.kind === 'error' && /not running/i.test(x.text));
  }, 15000);
  if (st) check('a refused send: the server\'s error in the log, the text back in the box, the chat as it was', pane(st, liveC).state === 'exited');
  await snap('2.5-send-refused');
}
await http('POST', `/api/ui-sessions/${liveC}/close`, {}, token);

// ─── 2.6 Permission-mode picker ───
const liveM = await make('demo', 'Modes', true);
const liveN = await make('demo', 'Modes plain', false);
p = await until('the mode chats have panes', (q) => pane(q, liveM) && pane(q, liveN), 30000);
await send(liveM, 'Run exactly this shell command in the foreground with timeout 600000 and wait for it to finish: timeout 120 tail -f /dev/null. It is a deliberate wait for a UI test; do not run it in the background. Then say done.');
p = await until('the skip-permissions chat is working', (q) => pane(q, liveM)?.state === 'working', 120000);
await ui('focus-pane', pane(await props(), liveM).id);
await ui('mode-open');
p = await until('the picker opens', (q) => q.modePicker, 5000);
if (p) {
  check('a skip-permissions chat: six modes, Bypass all first and selected', p.modePicker.modes.map((m) => m.key).join() === 'bypassPermissions,default,acceptEdits,auto,plan,dontAsk' && p.modePicker.current === 'bypassPermissions' && p.modePicker.start === 0, p.modePicker);
  check('the names and descriptions are the web\'s', p.modePicker.modes[1].name === 'Ask permissions' && p.modePicker.modes[5].desc === 'Never asks. Anything not already allowed is refused.', p.modePicker.modes);
}
await snap('2.6-open-bypass');
await ui('mode-choose', 'acceptEdits');
{ const q = await props(); check('choosing closes the picker and the button changes at once', q.modePicker === null && pane(q, liveM).mode === 'Accept edits' && pane(q, liveM).modeKey === 'acceptEdits', pane(q, liveM)); }
// The turn carries on: the pane still works, unless the server says Claude
// itself ended the turn meanwhile (a real turn's timing; F85/F92). A turn the
// switch cut short would read working on neither side.
if (await until('the server has the new mode', async () => (await serverChat(liveM))?.permissionMode === 'acceptEdits', 10000)) {
  const sc = await serverChat(liveM), st = pane(await props(), liveM).state;
  // (Under acceptEdits Claude's next command may ask: needs-input is the turn going on too.)
  check('…and the server has it; the turn carries on', st === 'working' || st === 'needs-input' || (sc?.agentState !== 'working' && sc?.status === 'running'), { pane: st, server: sc?.agentState });
}
await ui('mode-open');
await until('the picker opens again', (q) => q.modePicker?.current === 'acceptEdits', 5000);
await ui('mode-choose', 'plan');
p = await until('Plan mode: the header shows PLANNING', (q) => pane(q, liveM)?.planning === true, 10000);
if (p) check('Plan mode puts PLANNING in the header', pane(p, liveM).mode === 'Plan mode');
await snap('2.6-planning');
// Changed elsewhere (the API, as another client): the button and the tag follow.
await http('POST', `/api/ui-sessions/${liveM}/permission-mode`, { mode: 'default' }, token);
p = await until('a mode set elsewhere shows live', (q) => pane(q, liveM)?.modeKey === 'default' && !pane(q, liveM).planning, 10000);
if (p) check('a mode changed by another client shows with no click, and PLANNING goes', true);
// Esc, ×/close and a click on the scrim each close it, and change nothing.
await ui('mode-open'); await until('open', (q) => q.modePicker, 5000);
await key('Escape');
await until('Esc closes the picker', (q) => q.modePicker === null, 5000) && check('Esc closes the picker', true);
await ui('mode-open'); await until('open', (q) => q.modePicker, 5000);
await mouse('click', 20, 840);
await until('a click on the scrim closes the picker', (q) => q.modePicker === null, 5000) && check('a click on the scrim closes the picker', true);
await ui('mode-open'); await until('open', (q) => q.modePicker, 5000);
await ui('mode-close');
await until('× closes the picker', (q) => q.modePicker === null, 5000);
check('closing changes nothing', (await serverChat(liveM))?.permissionMode === 'default' && pane(await props(), liveM).modeKey === 'default');
await http('POST', `/api/ui-sessions/${liveM}/interrupt`, {}, token);
// A plain chat: five modes; the keyboard chooses (Tab, Enter).
await ui('focus-pane', pane(await props(), liveN).id);
await ui('mode-open');
p = await until('the plain chat\'s picker', (q) => q.modePicker, 5000);
if (p) check('a plain chat: five modes, Ask permissions selected, no Bypass all', p.modePicker.modes.length === 5 && p.modePicker.current === 'default' && !p.modePicker.modes.some((m) => m.key === 'bypassPermissions'), p.modePicker);
await snap('2.6-open-default');
await key('Tab'); await key('Return');
p = await until('Tab then Enter chooses the next mode', (q) => q.modePicker === null && pane(q, liveN)?.modeKey === 'acceptEdits', 5000);
if (p) check('the keyboard: Tab moves, Enter chooses', true);
// Under a modal's scrim nothing follows the pointer: hovering a pane shows
// none of its hover controls (F50). First, without the picker, they do.
{
  // The leftmost pane (the grid may run past the window's right edge).
  const q = (await props()).panes.filter((x) => x.kind === 'chat').sort((m, n) => m.x - n.x || m.y - n.y)[0];
  const cx = Math.round(WB.x + q.x + q.w / 2), cy = Math.round(WB.y + q.y + 12);
  const box = async () => { const img = await shot(), px = []; for (let y = cy - 14; y <= cy + 14; y++) for (let x = cx - 60; x <= cx + 60; x++) px.push(img.at(x, y)); return px; };
  const differ = (a1, a2) => a1.filter((v, i) => v.some((c, j) => Math.abs(c - a2[i][j]) > 6)).length;
  await mouse('move', 20, 840); await sleep(400); const off0 = await box();
  await mouse('move', cx, cy + 40); await sleep(400); const on0 = await box();
  check('without a modal, hovering a pane shows its hover controls', differ(off0, on0) > 0, { q: [q.x, q.y, q.w], cx, cy });
  await ui('mode-open'); await until('the picker opens', (x) => x.modePicker, 5000);
  check('a modal open: the pointer is its scrim\'s', (await props()).pointerModal === true);
  await mouse('move', 20, 840); await sleep(400); const off1 = await box();
  await mouse('move', cx, cy + 40); await sleep(400); const on1 = await box();
  check('under the scrim, hovering a pane shows none of its hover controls', differ(off1, on1) === 0, differ(off1, on1));
  await key('Escape');
  await until('closed', (x) => x.modePicker === null && x.pointerModal === false, 5000);
}
// A short window: the dialog scrolls, and the keyboard's focus is kept in
// view, the chat's mode too when it opens below the fold (F51).
await http('POST', `/api/ui-sessions/${liveN}/permission-mode`, { mode: 'dontAsk' }, token);
await until('Don\'t ask set', (q) => pane(q, liveN)?.modeKey === 'dontAsk', 10000);
await d.send({ cmd: 'resize', w: 1400, h: 420 });
await sleep(800);
await ui('mode-open');
p = await until('the picker opens on Don\'t ask', (q) => q.modePicker?.focus === 4, 5000);
if (p) {
  const m = p.modePicker;
  check('a short window: the dialog scrolls to show the chat\'s mode at open', m.scroll > 0 && m.focusBottom - m.scroll <= m.viewH + 1 && m.focusTop - m.scroll >= -1, m);
}
await snap('2.6-short-window');
await key('Tab');
p = await until('Tab to ×', (q) => q.modePicker?.focus === -1, 5000);
if (p) check('Tab to × scrolls back to the header', p.modePicker.scroll === 0, p.modePicker);
await key('Backtab');
p = await until('Shift+Tab to the last mode', (q) => q.modePicker?.focus === 4, 5000);
if (p) check('Shift+Tab to the last mode scrolls it into view', p.modePicker.scroll > 0 && p.modePicker.focusBottom - p.modePicker.scroll <= p.modePicker.viewH + 1, p.modePicker);
await key('Escape');
await until('closed', (q) => q.modePicker === null, 5000);
await d.send({ cmd: 'resize', w: 1400, h: 860 });
await sleep(800);
await http('POST', `/api/ui-sessions/${liveN}/permission-mode`, { mode: 'acceptEdits' }, token);
await until('Accept edits back', (q) => pane(q, liveN)?.modeKey === 'acceptEdits', 10000);
// Refused (the chat has exited): the button goes back, a toast says why.
const procN = chatProc(liveN);
if (check('the plain chat has a claude process', !!procN)) {
  process.kill(procN.pid);
  await until('the plain chat exits', (q) => pane(q, liveN)?.state === 'exited', 30000);
  await ui('mode-open'); await until('open', (q) => q.modePicker, 5000);
  await ui('mode-choose', 'dontAsk');
  p = await until('a refused change goes back, with a toast', (q) => /^Could not switch permission mode: No session/.test(q.toast) && pane(q, liveN)?.modeKey === 'acceptEdits', 10000);
  if (p) check('a refused change: the button goes back and the toast says why', true);
  await snap('2.6-refused');
}
for (const id of [liveM, liveN]) await http('POST', `/api/ui-sessions/${id}/close`, {}, token);

// ─── 2.7 Attachments ───
const demoPath = process.env.CRUNDI_TEST_PROJECT || join(tmpdir(), 'crundi-native-demo2');
const attDir = join(demoPath, 'crundi_attachments');
const attCount = () => (existsSync(attDir) ? readdirSync(attDir).length : 0);
const tmpA = tmpDir('crundi-attach-');
const notes = join(tmpA, 'notes.txt'); writeFileSync(notes, 'hello notes\n');
const otherF = join(tmpA, 'other file.md'); writeFileSync(otherF, '# other\n');
const emptyF = join(tmpA, 'empty.txt'); writeFileSync(emptyF, '');
// Big enough that the upload is still under way when the test looks (the bar, attach busy).
// Small (it leaves no mark: F54); the app uploads at 256 KB/s under the test
// (CRUNDI_TEST_UPLOAD_KBPS, given at launch), so it is under way for ~10 s (F64).
const big = join(tmpA, 'big.bin'); writeFileSync(big, Buffer.alloc(2 * 1024 * 1024, 7));
const pic = join(tmpA, 'shot.png'); writeFileSync(pic, Buffer.from((await d.send({ cmd: 'snapshot' })).data, 'base64'));
const liveA = await make('demo', 'Attach', false);
const liveB2 = await make('demo', 'Attach two', false);
p = await until('the attach chats have panes', (q) => pane(q, liveA) && pane(q, liveB2), 30000);
const apane = async () => pane(await props(), liveA);
const focusA = async () => { await ui('focus-pane', (await apane()).id); await ui('composer-focus'); };
await focusA();
await clearBox();
// Attach: the path goes in at the caret, spaced; the file is on the server.
await keys('Please read andsummarise');
await ui('caret', '15');
// Through the attach button: its picker (answered by the driver) and upload.
await ui('pick-next', notes);
await ui('attach-button');
p = await until('an attached file\'s path goes in at the caret', (q) => /^Please read and \/\S+\/crundi_attachments\/\d+-notes\.txt summarise$/.test(pane(q, liveA)?.draft || ''), 20000);
if (p) {
  const path = pane(p, liveA).draft.split(' ')[3];
  check('the upload reached the server with the same bytes', existsSync(path) && readFileSync(path, 'utf8') === 'hello notes\n', path);
  p = await until('the toast names the stored file', (q) => /^Attached: \d+-notes\.txt$/.test(q.toast), 5000);
  if (p) check('toast "Attached: <stored name>"', true);
  p = await until('the bar goes after the upload, attach free again', (q) => !pane(q, liveA).upOn && !pane(q, liveA).upBusy, 5000);
  if (p) check('the upload bar goes and attach is free again', true);
}
// A big file: the bar shows (name, a number), attach busy, then all gone.
await clearBox();
await ui('pick-next', big);
await ui('attach-button');
p = await until('uploading: the bar and attach busy', (q) => { const c = pane(q, liveA); return c.upOn && c.upBusy && c.upName === 'big.bin' && /^(\d+%|saving)$/.test(c.upLabel); }, 10000);
if (p) check('while uploading: the bar (name, number) and attach busy', true, pane(p, liveA));
await snap('2.7-uploading');
// Busy: the attach button opens no picker until the upload is done.
await ui('pick-next', notes);
await ui('attach-button');
await sleep(400);
{ const q = await props(); check('attach while uploading: no picker, nothing else uploads', q.pickWaiting === true && !/notes\.txt/.test(pane(q, liveA).draft), { pick: q.pickWaiting, draft: pane(q, liveA).draft }); }
await ui('pick-next', '');
p = await until('the big upload finishes', (q) => /-big\.bin $/.test(pane(q, liveA)?.draft || '') && !pane(q, liveA).upBusy, 60000);
if (p) check('a big file uploads and its path goes in', true);
// A long name elides; name and track share the bar 1:2 (F53): the track
// keeps about two thirds of the bar's room.
{
  const longF = join(tmpA, 'quarterly-report-final-version-with-all-the-appendices-and-the-notes-included.bin');
  writeFileSync(longF, Buffer.alloc(2 * 1024 * 1024, 7));   // 2 MB like big.bin: at the test's 256 KB/s upload it finishes in ~10 s (F76)
  await clearBox();
  await ui('attach', longF);
  p = await until('the long-named upload shows its bar', (q) => pane(q, liveA)?.upOn && pane(q, liveA)?.upBusy, 10000);
  if (p) {
    const q = pane(p, liveA), img = await shot();
    const near = (c, h, t) => Math.abs(c[0] - parseInt(h.slice(1, 3), 16)) <= t && Math.abs(c[1] - parseInt(h.slice(3, 5), 16)) <= t && Math.abs(c[2] - parseInt(h.slice(5, 7), 16)) <= t;
    let best = 0;
    for (let y = Math.round(WB.y + q.y + q.h - 140); y < Math.min(img.h, WB.y + q.y + q.h); y++) {
      let run = 0;
      for (let x = Math.round(WB.x + q.x); x < Math.min(img.w, WB.x + q.x + q.w); x++) {
        const c = img.at(x, y);
        if (near(c, '#1a1a28', 3) || near(c, '#6366f1', 12)) { run++; best = Math.max(best, run); } else run = 0;
      }
    }
    check('a long file name elides and the track keeps its two thirds of the bar', best > q.w * 0.5, { best, w: q.w });
    await snap('2.7-long-name');
  }
  await until('the long upload finishes', (q) => !pane(q, liveA)?.upBusy, 60000);
}
// Shift+Insert pastes the clipboard's text, as Ctrl+V does (F57).
await clearBox();
await keys('ab');
await ui('caret', '1');
await ui('clipboard-text', 'XY');
await keys('Insert', ['Shift']);
p = await until('Shift+Insert pastes text at the caret', (q) => pane(q, liveA)?.draft === 'aXYb', 5000);
if (p) check('Shift+Insert pastes the clipboard\'s text at the caret', true);
// Dropped on a chat from this machine: the local paths, no upload.
await clearBox();
const before = attCount();
await ui('drop', notes, otherF);
p = await until('dropped files: their local paths', (q) => pane(q, liveA)?.draft === `${notes} ${otherF} ` && q.toast === '2 files added', 5000);
if (p) check('a drop on the same machine: local paths, space-separated, "2 files added", nothing uploaded', attCount() === before, attCount());
// Dropped as the system delivers it (one event per file): on the chat the
// drag is over, not the focused one; with no chat under it, the focused chat.
{
  await clearBox();
  const b = pane(await props(), liveB2);
  await ui('file-hover', 'true');
  await d.send({ cmd: 'mouse', kind: 'move', x: WB.x + b.x + b.w / 2, y: WB.y + b.y + b.h / 2 });
  await sleep(200);
  await ui('os-drop', notes, otherF);
  p = await until('an OS drop lands on the chat under the pointer', (q) => pane(q, liveB2)?.draft === `${notes} ${otherF} ` && q.toast === '2 files added', 5000);
  if (p) check('a system drop of two files: one insertion on the chat under the pointer, the focused chat untouched', pane(p, liveA).draft === '', { a: pane(p, liveA).draft, b: pane(p, liveB2).draft });
  await d.send({ cmd: 'mouse', kind: 'leave', x: 0, y: 0 });
  await ui('os-drop', notes);
  p = await until('an OS drop with no chat under it goes to the focused chat', (q) => pane(q, liveA)?.draft === `${notes} ` && q.toast === 'File added', 5000);
  if (p) check('a system drop with no target: the focused chat, "File added"', pane(p, liveB2).draft === `${notes} ${otherF} `);
  // Empty the second chat's box again (2.8 types into it).
  await ui('focus-pane', pane(await props(), liveB2).id); await ui('composer-focus'); await clearBox();
  await focusA();
}
// Dropped on a chat from elsewhere (the driver's remote): each uploads.
await clearBox();
await ui('drop', 'upload', notes);
p = await until('a remote drop uploads', (q) => /\/crundi_attachments\/\d+-notes\.txt $/.test(pane(q, liveA)?.draft || ''), 20000);
if (p) check('a drop on a remote server uploads; the server\'s path goes in', true);
// A pasted picture uploads as image.png.
await clearBox();
await ui('paste-image', pic);
p = await until('a pasted picture uploads', (q) => /\/crundi_attachments\/\d+-image\.png $/.test(pane(q, liveA)?.draft || ''), 20000);
if (p) check('a pasted picture uploads as image.png', true);
// Refused: an empty file ("Missing name or data"): nothing goes in.
await clearBox();
await keys('kept');
await ui('attach', emptyF);
p = await until('a refused upload toasts', (q) => q.toast === 'Upload failed: Missing name or data', 10000);
if (p) check('a refused upload: the red toast, nothing inserted, attach free', pane(p, liveA).draft === 'kept' && !pane(p, liveA).upBusy, pane(p, liveA));
await snap('2.7-refused');

// ─── 2.8 Drafts ───
await clearBox();
await keys('a draft to keep');
await ui('focus-pane', pane(await props(), liveB2).id); await ui('composer-focus');
await keys('the second one');
{
  // (Written at most every 350 ms: polled, not a fixed wait, so a busy
  // machine's late write is not a failure.)
  const ok = (sd) => sd['chat:' + liveA]?.v === 'a draft to keep' && sd['chat:' + liveB2]?.v === 'the second one';
  let sd = {};
  for (let i = 0; i < 25; i++) { await sleep(200); sd = saved().drafts?.[SERVER] || {}; if (ok(sd)) break; }
  check('drafts are saved per server and chat', ok(sd), sd);
}
// Switching away and back brings each back.
await ui('select-project', 'other');
await until('Other open', (q) => q.currentProject === 'other', 10000);
await ui('select-project', 'demo');
p = await until('back on Demo: both drafts back', (q) => pane(q, liveA)?.draft === 'a draft to keep' && pane(q, liveB2)?.draft === 'the second one', 15000);
if (p) check('switching projects keeps each chat\'s draft', true);
// A restart: the drafts come back; a 31-day-old draft is pruned, a 29-day-old one stays.
d.close();
app.kill();
await sleep(1500);
{
  const f = join(configDir, 'session.json'), j = JSON.parse(readFileSync(f, 'utf8')), day = 24 * 3600 * 1000;
  j.drafts[SERVER]['chat:old-one'] = { v: 'too old', t: Date.now() - 31 * day };
  j.drafts[SERVER]['chat:recent-one'] = { v: 'recent', t: Date.now() - 29 * day };
  writeFileSync(f, JSON.stringify(j));
}
app = launch('run4');
d = await driver();
p = await until('after a restart: the drafts back', (q) => q.signedIn && pane(q, liveA)?.draft === 'a draft to keep' && pane(q, liveB2)?.draft === 'the second one', 30000);
if (p) {
  check('a client restart brings the drafts back', true);
  check('a draft older than 30 days is pruned; a newer one stays', !p.savedDrafts['chat:old-one'] && p.savedDrafts['chat:recent-one']?.v === 'recent', Object.keys(p.savedDrafts));
}
// Adopted: the chat owning the project's copy closes; a new chat starts with it.
// (The other chat empties its box first: it lets go of the copy.)
await ui('focus-pane', pane(await props(), liveB2).id); await ui('composer-focus');
await clearBox();
await focusA();
await clearBox(); await keys('a draft to keep!');
await sleep(500);
await ui('pane-close', pane(await props(), liveA).id);
await ui('pane-close', pane(await props(), liveA).id);
await until('the attach chat closed', (q) => !pane(q, liveA), 15000);
check('closing a chat keeps its draft', saved().drafts?.[SERVER]?.['project:demo']?.v === 'a draft to keep!', saved().drafts?.[SERVER]);
const liveN3 = await make('demo', 'Adopter', false);
p = await until('a new chat adopts the project\'s draft', (q) => pane(q, liveN3)?.draft === 'a draft to keep!', 30000);
if (p) check('a new chat in the project starts with the closed chat\'s draft', true);
// Sending clears it, and the project's copy with it.
await ui('focus-pane', pane(await props(), liveN3).id); await ui('composer-focus');
await clearBox(); await keys('Reply with just the word ok'); await keys('Return');
p = await until('sent: the draft is empty', (q) => pane(q, liveN3)?.draft === '', 10000);
await sleep(800);
{
  const sd = saved().drafts?.[SERVER] || {};
  check('sending clears the chat\'s draft and the project\'s copy', !sd['chat:' + liveN3] && !sd['project:demo'], sd);
}
for (const id of [liveB2, liveN3]) await http('POST', `/api/ui-sessions/${id}/close`, {}, token);

// ─── 2.9 Queued messages ───
const hist = async (id) => (await http('GET', `/api/ui-sessions/${id}/history`, null, token)).body?.session || {};
const pendingTexts = async (id) => ((await hist(id)).pendingInjections || []).map((x) => x.text);
const userTexts = async (id) => ((await hist(id)).messages || []).filter((m) => m.kind === 'user').map((m) => m.text);
const errorRows = async () => ((await d.send({ cmd: 'state' })).state?.entries || []).filter((e) => e.kind === 'error').map((e) => e.text);
const qp = async (id) => pane(await props(), id);
const focusChat = async (id) => { await ui('focus-pane', (await qp(id)).id); await ui('composer-focus'); };
const enter = async (text) => { await keys(text); await keys('Return'); };
const BASH_WAIT = 'Run the shell command `timeout 300 tail -f /dev/null` in the foreground and wait for it, then say done.';
const liveQW = await make('demo', 'Queue working', true);
const liveQN = await make('demo', 'Queue held', false);
const liveQX = await make('demo', 'Queue exit', false);
await send(liveQW, 'Run exactly this shell command in the foreground with timeout 600000 and wait for it to finish: timeout 300 tail -f /dev/null. It is a deliberate wait for a UI test; do not run it in the background. Then say done.');
await send(liveQN, BASH_WAIT);
await send(liveQX, BASH_WAIT);
p = await until('one chat working, two on a permission prompt', (q) => pane(q, liveQW)?.state === 'working' && pane(q, liveQN)?.state === 'needs-input' && pane(q, liveQX)?.state === 'needs-input', 180000);
await sleep(3000);
// Working: handed over at once; take it back (cancel-queued): nothing is sent.
await focusChat(liveQW);
await clearBox();
await enter('do not send me');
p = await until('handed over: the drawer, recallable', (q) => pane(q, liveQW)?.qSent === 'do not send me' && pane(q, liveQW).qSentKind === 'recallable' && pane(q, liveQW).draft === '', 20000);
if (p) check('a message sent while working is handed over at once (the server holds it)', (await pendingTexts(liveQW)).includes('do not send me'));
await snap('2.9-handed');
await ui('take-back');
p = await until('taken back: the drawer goes, the text in the box', (q) => pane(q, liveQW)?.qSent === '' && pane(q, liveQW).draft === 'do not send me', 15000);
if (p) check('a take-back cancels it on the server and puts it back in the box', !(await pendingTexts(liveQW)).includes('do not send me'));
await clearBox();
// The same by a real click on the drawer (found by its fill: slate-haze
// for handed over, accent-haze for held), down the pane's right side.
const drawerAt = async (id, rgb) => {
  const q = await qp(id), img = await shot();
  // (Clear of the window's resize border at its right edge.)
  const x = Math.min(img.w - 30, Math.round(WB.x + q.x + q.w - 24));
  // The lowest run of that fill at least 20 px tall: the drawers sit at the
  // pane's bottom, above the box. (Not the tallest: a user bubble higher up
  // has nearly the same fill, and in a narrow pane its wrapped text makes it
  // the taller one, as the GPUI client's 378 px panes do.)
  let best = [0, 0], run = [0, 0];
  for (let y = Math.round(WB.y + q.y); y < Math.min(img.h, WB.y + q.y + q.h); y++) {
    const c = img.at(x, y);
    if (rgb.every((v, i) => Math.abs(c[i] - v) <= 4)) { run = run[1] ? [run[0], run[1] + 1] : [y, 1]; if (run[1] >= 20) best = run; } else run = [0, 0];
  }
  return best[1] >= 20 ? { x, y: best[0] + best[1] / 2 } : null;
};
await enter('click me back');
await until('handed over again', (q) => pane(q, liveQW)?.qSent === 'click me back' && pane(q, liveQW).qSentKind === 'recallable', 20000);
{
  const at = await drawerAt(liveQW, [29, 30, 39]);
  if (check('the handed-over drawer is on screen', !!at)) {
    await mouse('click', at.x, at.y);
    p = await until('a click on the handed-over drawer takes it back', (q) => pane(q, liveQW)?.qSent === '' && pane(q, liveQW).draft === 'click me back', 15000);
    if (p) check('a click on the handed-over drawer cancels it on the server and puts it in the box', !(await pendingTexts(liveQW)).includes('click me back'));
  }
}
await clearBox();
// Too late: the server says it couldn't cancel (no error; "Claude is reading it").
await enter('late one');
await until('handed over', (q) => pane(q, liveQW)?.qSentKind === 'recallable', 20000);
const errs0 = (await errorRows()).length;
await ui('stub', 'cancel-queued', JSON.stringify({ ok: true, cancelled: false }));
await ui('take-back');
p = await until('too late: Claude is reading it', (q) => pane(q, liveQW)?.qSentKind === 'started', 10000);
if (p) check('too late to take back: "Claude is reading it", no error row, the box unchanged', (await errorRows()).length === errs0 && pane(p, liveQW).draft === '');
// A take-back that fails: one error row, the drawer stays.
await enter('e1 one');
await until('handed over', (q) => pane(q, liveQW)?.qSentKind === 'recallable', 20000);
await ui('stub', 'cancel-queued', JSON.stringify({ ok: false, error: 'E1' }));
await ui('take-back');
p = await until('a failed take-back', async (q) => (await errorRows()).includes('E1'), 10000);
if (p) check('a failed take-back: an error row with the server\'s words, the drawer stays', /e1 one/.test(pane(p, liveQW).qSent));
// A refused send gives the text back, with an error row.
await ui('stub', 'send', JSON.stringify({ ok: false, error: 'E2' }));
await enter('refused one');
p = await until('a refused send gives the text back', async (q) => pane(q, liveQW)?.draft === 'refused one' && (await errorRows()).includes('E2'), 10000);
if (p) check('a refused send: its text back in the box, an error row, no slot', !/refused one/.test(pane(p, liveQW).qSent));
// The give-back leaves the caret at the end of the text (F62): typing goes after it.
await keys('Z');
p = await until('typing after a give-back goes at the end', (q) => pane(q, liveQW)?.draft === 'refused oneZ', 5000);
if (p) check('after a give-back the caret is at the end of the text', true);
await clearBox();
await enter('queued one');
await until('queued one handed over', (q) => /queued one/.test(pane(q, liveQW)?.qSent || ''), 20000);
// Needs input: held here, two sends; ↑ takes them back in front of the box's text.
await focusChat(liveQN);
await clearBox();
await enter('a'); await enter('b');
p = await until('held while needs input', (q) => pane(q, liveQN)?.qHeld === 'a\nb' && pane(q, liveQN).qHeldN === 2, 5000);
if (p) check('needs input: the lines are held here (two sends), nothing reaches the server', (await pendingTexts(liveQN)).length === 0);
await snap('2.9-held');
await keys('y');
await ui('caret', '0');
await keys('Up');
p = await until('↑ takes back', (q) => pane(q, liveQN)?.qHeld === '' && pane(q, liveQN).draft === 'a\nb\ny', 5000);
if (p) check('↑ on the first row: every held line back, then the box\'s own text', true);
await clearBox();
// A click on the held drawer takes it back too.
await enter('c');
await until('c held', (q) => pane(q, liveQN)?.qHeld === 'c', 5000);
{
  const at = await drawerAt(liveQN, [26, 26, 47]);
  if (check('the held drawer is on screen', !!at)) {
    // Its tooltip at the window's right edge stays inside (F63); and one at
    // the bottom (the pane's mode button) flips above the pointer.
    await mouse('move', at.x, at.y); await sleep(1500);
    await snap('2.9-tip-right-edge');
    {
      // (Pane x is the grid's, before its sideways scroll: find the focused
      // pane's left edge on screen by its accent border, from the right.)
      const q = await qp(liveQN), img = await shot(), yy = Math.round(WB.y + q.y + q.h / 2);
      let lx = -1;
      for (let x = img.w - 1; x > WB.x && lx < 0; x--) { const c = img.at(x, yy); if (Math.abs(c[0] - 99) < 14 && Math.abs(c[1] - 102) < 14 && c[2] > 220) lx = x; }
      await mouse('move', lx + 115, WB.y + q.y + q.h - 22); await sleep(1500);
      await snap('2.9-tip-bottom');
      // The mode button's tooltip, flipped above the pointer, inside the window.
      // (Counted against the shot before the hover: the Queue button's text is white too.)
      const t = await shot(), bright = (im) => { let n = 0; for (let y = Math.round(WB.y + q.y + q.h - 80); y < Math.round(WB.y + q.y + q.h - 26); y++) for (let x = lx + 60; x < im.w; x++) { const c = im.at(x, y); if (c[0] > 200 && c[1] > 200 && c[2] > 200) n++; } return n; };
      const tip = bright(t) - bright(img);
      check('a tooltip at the window\'s bottom opens above the pointer', lx > 0 && tip > 50, { lx, tip });
    }
    await mouse('move', at.x, at.y); await sleep(300);
    await mouse('click', at.x, at.y);
    p = await until('a click on the held drawer takes it back', (q) => pane(q, liveQN)?.qHeld === '' && pane(q, liveQN).draft === 'c', 5000);
    if (p) check('a click on the held drawer puts the held line back in the box, nothing sent', (await pendingTexts(liveQN)).length === 0);
  }
}
await focusChat(liveQN);
await clearBox();
// A long held batch is clamped at four rows (F61): the box stays in the
// pane, seen (Queue drawn), focusable and typed into.
{
  for (let i = 1; i <= 12; i++) await enter(`held line ${i} of a long batch that keeps going`);
  await until('a long batch held', (q) => pane(q, liveQN)?.qHeldN === 12, 5000);
  await snap('2.9-long-batch');
  const q = await qp(liveQN), img = await shot();
  let queue = 0;
  for (let y = Math.round(WB.y + q.y); y < Math.min(img.h, WB.y + q.y + q.h); y++) {
    for (let x = Math.round(WB.x + q.x); x < Math.min(img.w, WB.x + q.x + q.w); x++) {
      const c = img.at(x, y);
      if (c[2] > 220 && c[0] > 85 && c[0] < 140 && c[1] > 90 && c[1] < 150) queue++;
    }
  }
  await ui('composer-focus');
  await keys('k');
  p = await until('the box takes typing under a long batch', (x) => pane(x, liveQN)?.draft === 'k', 5000);
  check('a long held batch: the drawer is clamped, the box still shows (Queue) and takes typing', !!p && queue > 600, { queue, h: q.h });
  await clearBox();
  await ui('take-back');
  await until('taken back', (x) => pane(x, liveQN)?.qHeld === '', 5000);
  await clearBox();
}
// Both drawers at once (F64): a held line, and a handed-over one (played in
// as the server's history would bring it: a chat can't be working and on a
// prompt at once from a test). Handed over on top, held under it; one
// take-back brings both, handed over first.
{
  await focusChat(liveQN);
  await clearBox();
  await enter('k');
  await until('k held', (q) => pane(q, liveQN)?.qHeld === 'k', 5000);
  const h = await hist(liveQN);
  await ui('chat-event', JSON.stringify({ type: 'history', session: { ...h, pendingInjections: [{ text: 'h', uuid: 'u-both', state: 'queued' }] } }));
  p = await until('both drawers', (q) => pane(q, liveQN)?.qSent === 'h' && pane(q, liveQN).qHeld === 'k', 5000);
  if (p) {
    const slate = await drawerAt(liveQN, [29, 30, 39]), dashed = await drawerAt(liveQN, [26, 26, 47]);
    check('both drawers: handed over on top, held under it', !!slate && !!dashed && slate.y < dashed.y, { slate, dashed });
    await snap('2.9-both-drawers');
    await ui('stub', 'cancel-queued', JSON.stringify({ ok: true, cancelled: true }));
    await ui('take-back');
    p = await until('one take-back brings both', (q) => pane(q, liveQN)?.draft === 'h\nk' && pane(q, liveQN).qSent === '' && pane(q, liveQN).qHeld === '', 10000);
    if (p) check('one take-back: the handed-over line, then the held one, in the box', true);
  }
  await clearBox();
}
await enter('a'); await enter('b');
await until('held again', (q) => pane(q, liveQN)?.qHeld === 'a\nb', 5000);
// Held lines stay through a project switch (native).
await ui('select-project', 'other');
await until('Other open', (q) => q.currentProject === 'other', 10000);
await ui('select-project', 'demo');
p = await until('back on Demo, still held', (q) => q.currentProject === 'demo' && pane(q, liveQN)?.qHeld === 'a\nb', 15000);
if (p) check('held lines survive a project switch', true);
// A held line in the third chat, then a restart: handed-over messages come
// back from the server, held ones in the box (the draft).
await focusChat(liveQX);
await clearBox();
await enter('held over quit');
await until('held', (q) => pane(q, liveQX)?.qHeld === 'held over quit', 5000);
// (Past the drafts' write throttle: the test stops the app with a kill.)
await sleep(800);
d.close();
app.kill();
await sleep(2500);
app = launch('run5');
d = await driver();
p = await until('after a restart: handed over back', (q) => q.signedIn && /queued one/.test(pane(q, liveQW)?.qSent || ''), 40000);
if (p) check('a restart: handed-over messages come back (history)', true);
p = await until('after a restart: held in the box', (q) => pane(q, liveQX)?.draft === 'held over quit', 20000);
if (p) check('a restart: held lines are in the box', pane(p, liveQX).qHeld === '');
else console.log('  QX', JSON.stringify(pane(await props(), liveQX)).slice(0, 300), 'QW', JSON.stringify(pane(await props(), liveQW)?.qSent));
// The prompt answered: the held lines go out as one message.
// (held lines went into the draft at the restart's quit too)
await focusChat(liveQN);
await clearBox();
await enter('a'); await enter('b');
await until('held once more', (q) => pane(q, liveQN)?.qHeld === 'a\nb', 5000);
const permN = (await serverChat(liveQN))?.pending?.[0];
if (check('the permission prompt is pending', !!permN)) {
  await http('POST', `/api/ui-sessions/${liveQN}/respond`, { requestId: permN.requestId, behavior: 'allow' }, token);
  p = await until('the held lines go out as one', async (q) => pane(q, liveQN)?.qHeld === '' && ((await pendingTexts(liveQN)).includes('a\nb') || (await userTexts(liveQN)).includes('a\nb')), 30000);
  if (p) check('the prompt answered: the held lines go out as ONE message', true);
}
// Exit: held lines are kept (in the box), handed-over cleared.
await focusChat(liveQX);
await clearBox();
await enter('z');
await until('z held', (q) => pane(q, liveQX)?.qHeld === 'z', 5000);
const procX = chatProc(liveQX);
if (check('the exit chat has a claude process', !!procX)) {
  process.kill(procX.pid);
  p = await until('exited, the held line kept', (q) => pane(q, liveQX)?.state === 'exited' && /(^|\n)z($|\n)/.test(pane(q, liveQX).draft) && pane(q, liveQX).qHeld === '' && pane(q, liveQX).qSent === '', 30000);
  if (p) check('exit: the held line is kept in the box (the web loses it)', true);
}
// The turn ends: what was handed over is read, and lands in the log.
await http('POST', `/api/ui-sessions/${liveQW}/interrupt`, {}, token);
p = await until('turn over: the drawer empties', (q) => pane(q, liveQW)?.qSent === '', 30000);
if (p) {
  const u = await userTexts(liveQW);
  check('turn end: the handed-over message is in the log, the taken-back ones are not', u.some((t) => /queued one/.test(t)) && !u.some((t) => /do not send me|click me back/.test(t)), u);
}
for (const id of [liveQW, liveQN, liveQX]) await http('POST', `/api/ui-sessions/${id}/close`, {}, token);

// ─── 2.10 Scheduled messages ───
const sch = async (id) => (await qp(id))?.sched;
const pad = (n) => String(n).padStart(2, '0');
const fieldOf = (d) => `${pad(d.getMonth() + 1)}/${pad(d.getDate())}/${d.getFullYear()}, ${pad(((d.getHours() + 11) % 12) + 1)}:${pad(d.getMinutes())} ${d.getHours() < 12 ? 'AM' : 'PM'}`;
const liveS = await make('demo', 'Schedule', true);
const liveS2 = await make('demo', 'Schedule other', true);
p = await until('the schedule chats have panes', (q) => pane(q, liveS) && pane(q, liveS2), 30000);
await http('POST', '/api/settings', { limitWarmup: false }, token);
await focusChat(liveS);
await clearBox();
await keys('carried text');
// Open: the composer's text carried in, At a time, now + 30 minutes.
await ui('sched-open');
p = await until('the panel opens', async (q) => pane(q, liveS)?.sched?.open && pane(q, liveS).sched.loaded, 10000);
if (p) {
  const sc = pane(p, liveS).sched, want = new Date(Date.now() + 30 * 60000), w2 = new Date(Date.now() + 29 * 60000);
  check('open: the composer\'s text carried in, At a time, now + 30 minutes', sc.text === 'carried text' && sc.trigger === 'time' && (sc.at === fieldOf(want) || sc.at === fieldOf(w2)), sc);
  check('open: nothing scheduled yet; the warmer off disables Limit resets', sc.list === 'Nothing scheduled for this project.' && sc.limitOff === true, sc);
}
await snap('2.10-open');
// Esc and a click elsewhere don't close it; the clock and × do.
await key('Escape');
await sleep(300);
check('Esc leaves the panel open', (await sch(liveS)).open === true);
await ui('sched-open');
await until('the clock closes it', async (q) => !pane(q, liveS)?.sched?.open, 5000) && check('the clock toggles the panel', true);
await ui('sched-open');
await until('open again', (q) => pane(q, liveS)?.sched?.open, 5000);
await ui('sched-close');
await until('× closes it', (q) => !pane(q, liveS)?.sched?.open, 5000) && check('× closes the panel', true);
await ui('sched-open');
await until('open', (q) => pane(q, liveS)?.sched?.open && pane(q, liveS).sched.loaded, 10000);
// Triggers: the notes; Limit resets refused while the warmer is off.
await ui('sched-trigger', 'turn-end');
p = await until('turn ends', (q) => pane(q, liveS)?.sched?.trigger === 'turn-end', 5000);
if (p) check('Turn ends: its note', pane(p, liveS).sched.note === 'Sends as soon as the current turn finishes. If nothing is running, it waits for the next turn to end.');
await ui('sched-trigger', 'limit-reset');
await sleep(300);
check('Limit resets can\'t be chosen with the warmer off', (await sch(liveS)).trigger === 'turn-end');
await ui('sched-trigger', 'time');
await until('at a time', (q) => pane(q, liveS)?.sched?.trigger === 'time' && pane(q, liveS).sched.note === '', 5000);
// Validation: empty, no time, a past time.
await ui('sched-text', '  ');
await ui('sched-save');
await until('empty message', (q) => pane(q, liveS)?.sched?.note === 'Write a message first.', 5000) && check('an empty message: "Write a message first."', true);
await ui('sched-text', 'later');
await ui('sched-at', '');
await ui('sched-save');
await until('no time', (q) => pane(q, liveS)?.sched?.note === 'Pick a time.', 5000) && check('no time: "Pick a time."', true);
await ui('sched-at', fieldOf(new Date(Date.now() - 3600000)));
await ui('sched-save');
await until('a past time', (q) => pane(q, liveS)?.sched?.note === 'That time has already passed', 10000) && check('a past time: the server\'s "That time has already passed"', true);
// Schedule tomorrow 09:00: the form clears (trigger and time stay), the
// item is listed, the composer holding the same text empties.
const tomorrow = new Date(); tomorrow.setDate(tomorrow.getDate() + 1); tomorrow.setHours(9, 0, 0, 0);
await ui('sched-close');
await focusChat(liveS); await clearBox(); await keys('e2e later');
await ui('sched-open');
await until('open with the text', (q) => pane(q, liveS)?.sched?.text === 'e2e later', 10000);
await ui('sched-at', fieldOf(tomorrow));
await ui('sched-save');
p = await until('scheduled', (q) => { const sc = pane(q, liveS)?.sched; return sc && sc.items.some((i) => i.text === 'e2e later'); }, 10000);
if (p) {
  const sc = pane(p, liveS).sched, it = sc.items.find((i) => i.text === 'e2e later');
  check('scheduled: the box and note clear, trigger and time stay', sc.text === '' && sc.note === '' && sc.trigger === 'time' && sc.at === fieldOf(tomorrow), sc);
  check('the item lists its time', it.when === `${tomorrow.getMonth() + 1}/${tomorrow.getDate()}/${tomorrow.getFullYear()}, 9:00:00 AM`, it);
  check('the composer, holding the same text, is emptied', pane(p, liveS).draft === '', pane(p, liveS).draft);
}
// Other text in the composer: left alone.
await ui('sched-close');
await focusChat(liveS); await clearBox(); await keys('keep me');
await ui('sched-open');
await until('open', (q) => pane(q, liveS)?.sched?.open && pane(q, liveS).sched.loaded, 10000);
await ui('sched-text', 'e2e two');
await ui('sched-save');
p = await until('the second item', (q) => pane(q, liveS)?.sched?.items.some((i) => i.text === 'e2e two'), 10000);
if (p) check('a composer holding other text is left alone', pane(p, liveS).draft === 'keep me', pane(p, liveS).draft);
// A turn-end item lists first, as next.
await ui('sched-trigger', 'turn-end');
await ui('sched-text', 'Reply with just the word ok');
await ui('sched-save');
p = await until('the turn-end item first', (q) => pane(q, liveS)?.sched?.items[0]?.when === 'when the turn ends', 10000);
if (p) check('a turn-end item lists first, as next', pane(p, liveS).sched.items[0].next && !pane(p, liveS).sched.items[1]?.next);
await snap('2.10-with-items');
// Delete: gone from the list and the server.
{
  const it = (await sch(liveS)).items.find((i) => i.text === 'e2e two');
  await ui('sched-delete', it.id);
  p = await until('deleted', (q) => !pane(q, liveS)?.sched?.items.some((i) => i.text === 'e2e two'), 10000);
  const srv = (await http('GET', `/api/chat-schedule?project=demo&chat=${liveS}`, null, token)).body?.items || [];
  if (p) check('✕ deletes it (no confirm): gone from the list and the server', !srv.some((i) => i.text === 'e2e two'));
}
// Per chat: another chat's item isn't listed here.
await http('POST', '/api/chat-schedule', { project: 'demo', text: 'for the other chat', trigger: { type: 'time', at: tomorrow.toISOString() }, chat: liveS2 }, token);
await ui('sched-close'); await ui('sched-open');
p = await until('reloaded', (q) => pane(q, liveS)?.sched?.open && pane(q, liveS).sched.loaded && pane(q, liveS).sched.items.length > 0, 10000);
if (p) check('another chat\'s item is not in this chat\'s list', !pane(p, liveS).sched.items.some((i) => i.text === 'for the other chat'));
// Fire: the current minute; it goes into the chat as a user message (and
// the turn-end item after its turn); the open panel doesn't update.
await ui('sched-trigger', 'time');
await ui('sched-at', fieldOf(new Date()));
await ui('sched-text', 'Reply with just the word pong');
await ui('sched-save');
await until('listed', (q) => pane(q, liveS)?.sched?.items.some((i) => i.text === 'Reply with just the word pong'), 10000);
p = await until('the scheduled message fires into the chat', async () => (await userTexts(liveS)).includes('Reply with just the word pong'), 60000);
if (p) {
  check('a scheduled message fires as a user message in the chat', true);
  check('the open panel does not update live (as the web)', (await sch(liveS)).items.some((i) => i.text === 'Reply with just the word pong'));
  await ui('sched-close'); await ui('sched-open');
  p = await until('reopened: the fired one gone', (q) => pane(q, liveS)?.sched?.open && pane(q, liveS).sched.loaded && !pane(q, liveS).sched.items.some((i) => i.text === 'Reply with just the word pong'), 10000);
  if (p) check('reopened, the fired message is gone from the list', true);
  p = await until('the turn-end item fires after the turn', async () => (await userTexts(liveS)).includes('Reply with just the word ok'), 90000);
  if (p) check('a turn-end item fires when the turn ends', true);
}
// The warmer on: Limit resets can be chosen.
await http('POST', '/api/settings', { limitWarmup: true }, token);
await ui('sched-close'); await ui('sched-open');
p = await until('warmer on', (q) => pane(q, liveS)?.sched?.open && pane(q, liveS).sched.loaded && !pane(q, liveS).sched.limitOff, 10000);
if (p) {
  await ui('sched-trigger', 'limit-reset');
  p = await until('limit resets', (q) => pane(q, liveS)?.sched?.trigger === 'limit-reset', 5000);
  if (p) check('with the warmer on, Limit resets is chosen, with its note', pane(p, liveS).sched.note === 'Sends when the rolling 5-hour usage window rolls over.');
}
await http('POST', '/api/settings', { limitWarmup: false }, token);
// Errors: a load that fails, a save that fails, an older server.
await ui('sched-close');
await ui('stub', 'sched-list', JSON.stringify({ net: 'down' }));
await ui('sched-open');
await until('a failed load', (q) => pane(q, liveS)?.sched?.list === 'Could not load scheduled messages.', 10000) && check('a failed load: "Could not load scheduled messages."', true);
await ui('sched-text', 'x');
await ui('stub', 'sched-save', JSON.stringify({ net: 'down' }));
await ui('sched-save');
await until('a failed save', (q) => /^Could not schedule that: /.test(pane(q, liveS)?.sched?.note || ''), 10000) && check('a failed save: "Could not schedule that: …"', true);
await ui('sched-close');
await ui('stub', 'sched-list', JSON.stringify({ status: 404, error: 'Not found' }));
await ui('sched-open');
p = await until('an older server', (q) => pane(q, liveS)?.sched?.unsupported, 10000);
if (p) check('an older server: "not supported by this server"', pane(p, liveS).sched.list === 'Scheduled messages are not supported by this server.');
await ui('sched-close');
for (const id of [liveS, liveS2]) await http('POST', `/api/ui-sessions/${id}/close`, {}, token);

// ─── 2.11 Slash menu ───
const liveSl = await make('demo', 'Slash', true);
p = await until('the slash chat has commands', (q) => pane(q, liveSl), 30000);
await focusChat(liveSl);
await clearBox();
await keys('/');
p = await until('the slash menu opens', (q) => pane(q, liveSl)?.slash?.items?.length > 0, 20000);
if (p) {
  const sl = pane(p, liveSl).slash;
  check('"/" opens the menu: the CLI\'s commands, the first selected, /btw last when the CLI has none', sl.sel === 0 && sl.items.length <= 40 && (sl.items.includes('/btw') || sl.items.length === 40), sl.items.slice(-3));
}
await snap('2.11-open');
const first = (await qp(liveSl)).slash?.items?.[0] || '/x';
const pre = first.slice(0, 3);
await keys(pre.slice(1));
p = await until('filtered', (q) => (pane(q, liveSl)?.slash?.items || []).length > 0 && pane(q, liveSl).slash.items.every((n) => n.toLowerCase().startsWith(pre.toLowerCase())), 5000);
if (p) check('typing filters by prefix (case-insensitive)', true);
const userBefore = (await userTexts(liveSl)).length;
const slItems = (await qp(liveSl)).slash.items;
await keys('Down');
await keys('Return');
const want = slItems.length > 1 ? slItems[1] + ' ' : slItems[0] + ' ';
p = await until('a choice fills the box', (q) => pane(q, liveSl)?.draft === want && !pane(q, liveSl).slash, 5000);
if (p) {
  await sleep(1500);
  check('↓ then Enter: the box reads "/name " and nothing is sent; the list closes', (await userTexts(liveSl)).length === userBefore, { draft: pane(p, liveSl).draft });
  await sleep(500);
  check('the draft after a choice is the chosen text (saved)', saved().drafts?.[SERVER]?.['chat:' + liveSl]?.v === want, saved().drafts?.[SERVER]?.['chat:' + liveSl]);
}
await clearBox();
await keys('/zzzzqq');
await sleep(400);
check('no match: no list (no empty state)', !(await qp(liveSl)).slash);
await clearBox();
await keys('/');
await until('open again', (q) => pane(q, liveSl)?.slash, 5000);
await key('Escape');
await sleep(300);
{ const c = await qp(liveSl); check('Esc closes the list and keeps the text', !c.slash && c.draft === '/', { slash: c.slash, draft: c.draft }); }
await keys('b');
p = await until('/b', (q) => pane(q, liveSl)?.slash, 5000);
if (p) check('the next edit opens it again; /btw is listed for /b', pane(p, liveSl).slash.items.includes('/btw'), pane(p, liveSl).slash.items);
// By mouse: a press on a row chooses it. The selected row is found by its
// fill (accent-dim over bg-secondary); the third row is two row-heights below.
await clearBox();
await keys('/');
p = await until('open for the mouse', (q) => pane(q, liveSl)?.slash?.items?.length >= 3 && pane(q, liveSl).slash.sel === 0, 5000);
if (p) {
  await sleep(300);
  const q = pane(p, liveSl), img = await shot(), sel = [30, 30, 58];
  let best = null;
  for (let x = Math.round(WB.x + q.x + 6); x < Math.min(img.w - 30, WB.x + q.x + q.w - 6); x += 7) {
    let run = [0, 0];
    for (let y = Math.round(WB.y + q.y); y < Math.min(img.h, WB.y + q.y + q.h); y++) {
      const c = img.at(x, y);
      if (sel.every((v, i) => Math.abs(c[i] - v) <= 4)) { run = run[1] ? [run[0], run[1] + 1] : [y, 1]; if (run[1] >= 14 && run[1] <= 40 && (!best || run[1] > best.h)) best = { x, top: run[0], h: run[1] }; } else run = [0, 0];
    }
    if (best) break;
  }
  if (check('the selected row is drawn (accent-dim)', !!best, best)) {
    const want3 = q.slash.items[2] + ' ', before = (await userTexts(liveSl)).length;
    await mouse('click', best.x, best.top + best.h * 2.5);
    p = await until('the mouse choice fills the box', (r) => pane(r, liveSl)?.draft === want3 && !pane(r, liveSl).slash, 5000);
    if (check('a click on the third row: the box reads its "/name ", the list closes', !!p, { want: want3, draft: pane(await props(), liveSl)?.draft })) {
      await sleep(1000);
      check('a mouse choice sends nothing', (await userTexts(liveSl)).length === before);
      check('the box keeps the focus after a mouse choice', pane(await props(), liveSl)?.focused === true);
    }
  }
}
await clearBox();
await http('POST', `/api/ui-sessions/${liveSl}/close`, {}, token);

// ─── 2.12 Chat secrets dialog ───
const liveSec = await make('demo', 'Secrets', true);
p = await until('the secrets chat has a pane', (q) => pane(q, liveSec), 30000);
const added = await http('POST', '/api/secrets', { action: 'add', name: 'E2E_TOKEN', description: 'e2e', value: 'v', pin: 'open-sesame' }, token);
check('a secret is seeded on the server', added.body?.ok === true, added.body);
const grantsOf = async () => (await http('GET', `/api/secrets/chat?sessionId=${liveSec}`, null, token)).body?.grants || [];
await ui('sec-open', (await qp(liveSec)).id);
p = await until('the dialog loads', (q) => q.secretsDialog && !q.secretsDialog.loading, 10000);
if (p) check('the lock opens the dialog: E2E_TOKEN listed, nothing pre-authorised, 1 hour', p.secretsDialog.secrets.includes('E2E_TOKEN') && p.secretsDialog.grants.length === 0 && p.secretsDialog.duration === '1h', p.secretsDialog);
check('the dialog is modal (the pointer is its scrim\'s)', (await props()).pointerModal === true);
await ui('sec-grant');
await until('nothing ticked', (q) => q.secretsDialog?.err === 'Choose at least one secret.', 5000) && check('nothing ticked: "Choose at least one secret."', true);
await ui('sec-tick', 'E2E_TOKEN');
await ui('sec-grant');
await until('no passphrase', (q) => q.secretsDialog?.err === 'Enter the PIN or passphrase of each secret chosen.', 5000) && check('a ticked secret with no passphrase: its field red, the error line', (await props()).secretsDialog.bad !== '');
await keys('wrong-pass');
await keys('Return');
p = await until('a wrong passphrase', (q) => q.secretsDialog?.err === 'E2E_TOKEN: Incorrect passphrase', 10000);
if (p) check('a wrong passphrase: "E2E_TOKEN: Incorrect passphrase", nothing granted', (await grantsOf()).length === 0);
await snap('2.12-wrong');
await keys('open-sesame');
await keys('Return');
p = await until('granted', (q) => q.secretsDialog?.grants?.length === 1 && pane(q, liveSec)?.secretsN === 1, 10000);
if (p) {
  const g = await grantsOf(), d = p.secretsDialog;
  check('granted: on the server for about an hour', g.length === 1 && g[0].name === 'E2E_TOKEN' && Math.abs(Date.parse(g[0].expiresAt) - Date.now() - 3600000) < 120000, g);
  check('the lock counts it, with its tooltip', pane(p, liveSec).secretsTip === '1 secret pre-authorised for this chat');
  check('the dialog lists it: "until HH:MM (1 h left)" (or 59 min, as the web\'s describe)', /^until \d\d:\d\d \(((59|60) min|1 h) left\)$/.test(d.grants[0].when), d.grants[0]);
  check('the toast, and the ticks cleared', p.toast === 'Pre-authorised for this chat' && d.chosen.length === 0, { toast: p.toast, chosen: d.chosen });
}
await snap('2.12-granted');
await ui('sec-revoke', 'E2E_TOKEN');
// (The toast comes with the revoke's answer, which may land after the
// server's grants event: wait for both; F92.)
p = await until('revoked', (q) => q.secretsDialog?.grants?.length === 0 && pane(q, liveSec)?.secretsN === 0 && q.toast === 'Revoked E2E_TOKEN', 10000);
if (p) check('Revoke: gone from the server and the lock; the toast', (await grantsOf()).length === 0 && p.toast === 'Revoked E2E_TOKEN', p.toast);
// F74: Tab and Shift+Tab move between the ticked secrets' fields, and what
// is typed lands in the field Tab went to.
const added2 = await http('POST', '/api/secrets', { action: 'add', name: 'E2E_TWO', description: 'e2e', value: 'v', pin: 'two-pass' }, token);
check('a second secret is seeded', added2.body?.ok === true, added2.body);
await key('Escape');
await until('closed to reload', (q) => q.secretsDialog === null, 5000);
await ui('sec-open', (await qp(liveSec)).id);
await until('the dialog lists both', (q) => q.secretsDialog && !q.secretsDialog.loading && q.secretsDialog.secrets.includes('E2E_TWO'), 10000);
await ui('sec-tick', 'E2E_TOKEN');
await ui('sec-tick', 'E2E_TWO');
await new Promise((r) => setTimeout(r, 300));
await keys('ab');
p = await until('typed in the last ticked field', (q) => q.secretsDialog?.typed?.E2E_TWO === 2, 5000);
if (p) check('the field of the secret ticked last has the focus', p.secretsDialog.typed.E2E_TOKEN === undefined || p.secretsDialog.typed.E2E_TOKEN === 0, p.secretsDialog.typed);
await key('Tab');
await new Promise((r) => setTimeout(r, 200));
await keys('xyz');
p = await until('Tab moves on', (q) => q.secretsDialog?.typed?.E2E_TOKEN === 3, 5000);
if (p) check('Tab moves to the other ticked field and typing lands there', p.secretsDialog.typed.E2E_TWO === 2, p.secretsDialog.typed);
await key('Backtab');
await new Promise((r) => setTimeout(r, 200));
await keys('c');
p = await until('Shift+Tab moves back', (q) => q.secretsDialog?.typed?.E2E_TWO === 3, 5000);
if (p) check('Shift+Tab moves back, staying among the fields', p.secretsDialog.typed.E2E_TOKEN === 3, p.secretsDialog.typed);
check('the dialog is still open', (await props()).secretsDialog !== null);
// F75: in a short window the pick list gives up its height first; the
// durations, Close and Pre-authorise stay in view.
await d.send({ cmd: 'resize', w: 860, h: 480 });
p = await until('short window', (q) => q.secProbe && q.secProbe.actionsShown === true, 5000);
check('860x480: Close and Pre-authorise in view', !!p, (await props()).secProbe);
await snap('2.12-short');
await d.send({ cmd: 'resize', w: 1400, h: 860 });
await until('back to size', (q) => q.secProbe?.actionsShown === true, 5000);
await key('Escape');
await until('Esc closes the dialog', (q) => q.secretsDialog === null && q.pointerModal === false, 5000) && check('Esc closes the dialog', true);

// ─── 2.12b Secret requests (the dock) ───
// Claude's MCP bridge: POST /api/mcp/call with the chat's key (the test
// server's dev-mcp file for the chat). The call waits for the owner, so it
// is started and not awaited.
{
  const DATA = process.env.CRUNDI_TEST_DATA || `/tmp/crundi-devtest-${new URL(SERVER).port}/data`;
  const mcpFile = join(DATA, 'dev-mcp', `${liveSec}.json`);
  await until('the chat\'s MCP config', () => existsSync(mcpFile), 20000);
  if (!check('the secrets chat has its MCP config (dev-mcp)', existsSync(mcpFile), mcpFile)) {
    console.log('skipping 2.12b');
  } else {
    const KEY = JSON.parse(readFileSync(mcpFile, 'utf8')).mcpServers.crundi.env.CRUNDI_API_KEY;
    const mcp = (tool, args) => fetch(SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY }, body: JSON.stringify({ tool, args }) }).then((r) => r.json()).catch((e) => ({ ok: false, error: String(e) }));
    const run = () => mcp('secret_run', { sessionId: liveSec, name: 'E2E_RUN', command: 'echo len=${#SECRET}', reason: 'e2e' });
    const seeded = await http('POST', '/api/secrets', { action: 'add', name: 'E2E_RUN', description: 'e2e', value: 'v-e2e', pin: 'open-sesame' }, token);
    check('E2E_RUN is seeded', seeded.body?.ok === true, seeded.body);
    // Reject.
    let call = run();
    p = await until('a run request: one card', (q) => q.sreq?.cards?.length === 1, 10000);
    if (p) {
      const c = p.sreq.cards[0];
      check('the card: a run of E2E_RUN with its command, armed', c.name === 'E2E_RUN' && c.run && c.command === 'echo len=${#SECRET}' && c.armed, c);
      check('armed without the focus (the composer keeps the keyboard)', p.sreqFocused === false);
      check('the dock is not modal', p.pointerModal === false);
    }
    await snap('2.12b-one-run');
    // F79: a press on the dock is the dock's: the focused pane stays.
    {
      const q0 = await props();
      const focused0 = q0.panes.find((x) => x.focused)?.key;
      check('the dock tells the pointer where its card is', q0.dock.w > 0 && q0.dock.h > 0, q0.dock);
      await mouse('click', q0.dock.x + q0.dock.w / 2, q0.dock.y + 12);
      await sleep(300);
      const q1 = await props();
      check('a press on the card focuses no pane beneath it', q1.panes.find((x) => x.focused)?.key === focused0, { before: focused0, after: q1.panes.find((x) => x.focused)?.key });
    }
    await ui('sreq-reject', 'E2E_RUN');
    let ans = await call;
    check('Reject: the call is denied', ans.ok === false && ans.error === 'The user denied this secret access request.', ans);
    p = await until('the dock goes', (q) => q.sreq === null && q.toast === 'Rejected', 10000);
    if (p) check('the dock goes; the toast "Rejected"', true);
    // Wrong, then right.
    call = run();
    await until('a second request', (q) => q.sreq?.cards?.length === 1, 10000);
    await ui('sreq-focus');
    await until('the field has the keyboard', (q) => q.sreqFocused === true, 5000);
    await keys('nope-nope');
    await keys('Return');
    p = await until('a wrong passphrase', (q) => q.sreq?.err === 'Incorrect passphrase', 10000);
    if (p) {
      const pending = (await http('GET', '/api/secrets', null, token)).body?.requests || [];
      check('wrong: "Incorrect passphrase", the card and the request still there, the text kept', p.sreq.cards.length === 1 && pending.length === 1 && p.sreq.typed === 9, { sreq: p.sreq, pending: pending.length });
    }
    await snap('2.12b-wrong');
    // Esc: the error goes, the text stays, the field lets go.
    await key('Escape');
    p = await until('Esc', (q) => q.sreq?.err === '' && q.sreqFocused === false, 5000);
    if (p) check('Esc in the field: the error cleared, the text kept, the focus gone', p.sreq.typed === 9, p.sreq);
    await ui('sreq-focus');
    await until('focused again', (q) => q.sreqFocused === true, 5000);
    // The text is selected: typing replaces it.
    await keys('open-sesame');
    await keys('Return');
    ans = await call;
    check('right: the command ran with the secret bound', ans.ok === true && ans.stdout === 'len=5\n', ans);
    p = await until('approved', (q) => q.sreq === null && q.toast === 'Approved', 10000);
    if (p) check('the toast "Approved"; no dock', true);
    // Allow for this chat.
    call = run();
    await until('a third request', (q) => q.sreq?.cards?.length === 1, 10000);
    await ui('sreq-allow');
    await ui('sreq-duration', '1h');
    await until('ticked', (q) => q.sreq?.allowOn === true, 5000);
    await snap('2.12b-allow');
    await ui('sreq-focus');
    await until('focused', (q) => q.sreqFocused === true, 5000);
    await keys('open-sesame');
    await keys('Return');
    ans = await call;
    check('allowed: it ran', ans.ok === true, ans);
    p = await until('allowed for this chat', (q) => q.sreq === null && q.toast === 'Approved, and allowed for this chat' && pane(q, liveSec)?.secretsN === 1, 10000);
    if (p) {
      const g = (await http('GET', `/api/secrets/chat?sessionId=${liveSec}`, null, token)).body?.grants || [];
      check('the toast "Approved, and allowed for this chat"; the lock counts 1; the grant about an hour', g.length === 1 && g[0].name === 'E2E_RUN' && Math.abs(Date.parse(g[0].expiresAt) - Date.now() - 3600000) < 120000, g);
    }
    ans = await run();
    check('the next run from the chat asks nothing', ans.ok === true && ans.stdout === 'len=5\n', ans);
    check('and no card appeared', (await props()).sreq === null);
    // Several: a get and a run; Approve on the second moves the field there.
    const callGet = mcp('secret_get', { sessionId: liveSec, name: 'E2E_TOKEN', reason: '' });
    await until('the get', (q) => q.sreq?.cards?.length === 1, 10000);
    const callRun2 = mcp('secret_run', { sessionId: liveSec, name: 'E2E_TWO', command: 'true', reason: '' });
    p = await until('two cards', (q) => q.sreq?.cards?.length === 2, 10000);
    if (p) check('two cards, oldest first, the first armed', p.sreq.cards[0].name === 'E2E_TOKEN' && !p.sreq.cards[0].run && p.sreq.cards[0].armed && !p.sreq.cards[1].armed, p.sreq.cards);
    await snap('2.12b-several');
    await keys('typed-in-first');
    await ui('sreq-approve', 'E2E_TWO');
    p = await until('the second armed', (q) => q.sreq?.cards?.[1]?.armed === true && q.sreqFocused === true, 5000);
    if (p) check('Approve on the second: its field, empty and focused', p.sreq.typed === 0 && !p.sreq.cards[0].armed, p.sreq);
    await ui('sreq-reject', 'E2E_TWO');
    await until('one left', (q) => q.sreq?.cards?.length === 1, 10000);
    p = await props();
    check('rejected: the first card armed again', p.sreq?.cards?.[0]?.armed === true, p.sreq);
    await ui('sreq-reject', 'E2E_TOKEN');
    p = await until('none left', (q) => q.sreq === null, 10000);
    const [g1, r2] = await Promise.all([callGet, callRun2]);
    check('both calls denied', g1.ok === false && r2.ok === false, { g1, r2 });
  }
}
await http('POST', `/api/ui-sessions/${liveSec}/close`, {}, token);

// ─── 2.13 Questions ───
// Three chats ask at once (real AskUserQuestion calls): several questions
// (a note, ticks, the draft across a restart, Other and Enter), one to
// dismiss, one orphaned by its process going.
{
  const ASK = (input) => 'Call the AskUserQuestion tool exactly once with exactly this input JSON (verbatim, no changes), then after the answer reply with one short sentence repeating what I chose and stop. Input: ' + JSON.stringify(input);
  const QL = 'Which language should the service use?', QT = 'Which tests should run?', QD = 'Where should it deploy?';
  const many = { questions: [
    { header: 'Language', question: QL, multiSelect: false, options: [{ label: 'Rust', description: 'Fast and safe' }, { label: 'Go', description: 'Simple concurrency' }] },
    { header: 'Tests', question: QT, multiSelect: true, options: [{ label: 'Unit', description: 'Small and quick' }, { label: 'Integration', description: 'Across modules' }, { label: 'E2E', description: 'The whole thing' }] },
    { header: 'Deploy', question: QD, multiSelect: false, options: [{ label: 'This server', description: 'Where Crundi runs', preview: 'systemctl restart app\njournalctl -u app -f' }, { label: 'A container', description: 'Docker on the same box' }] },
  ] };
  const one = (q) => ({ questions: [{ header: 'Database', question: q, multiSelect: false, options: [{ label: 'SQLite', description: 'A file' }, { label: 'MySQL', description: 'A server' }] }] });
  const hist = async (id) => { const b = (await http('GET', `/api/ui-sessions/${id}/history`, null, token)).body; return (b?.session?.messages || b?.messages || []).filter((e) => e.kind === 'question').pop(); };
  const qMany = await make('demo', 'Several questions', false);
  const qDis = await make('demo', 'Dismiss me', false);
  const qOrph = await make('demo', 'Orphaned', false);
  await send(qMany, ASK(many));
  await send(qDis, ASK(one('Which database for the cache?')));
  await send(qOrph, ASK(one('Which database for the queue?')));
  const qOf = (q, id) => pane(q, id)?.question;
  p = await until('three questions pending', (q) => [qMany, qDis, qOrph].every((id) => qOf(q, id)?.status === 'pending'), 240000);
  if (check('three chats ask: question cards, pending', !!p, p && [qMany, qDis, qOrph].map((id) => qOf(p, id)))) {
    // Several questions: Submit waits for every one.
    await ui('focus-pane', pane(p, qMany).id);
    check('the card holds three questions; Submit disabled', qOf(p, qMany).questions === 3 && qOf(p, qMany).can === false, qOf(p, qMany));
    await ui('q-note', 0, 1);
    await sleep(300);
    await keys('keep it small');
    p = await until('a note on Go', (q) => qOf(q, qMany)?.picks?.[0]?.note === 'keep it small', 5000);
    if (p) check('the note button picks the choice and opens its note, which takes the keyboard', JSON.stringify(qOf(p, qMany).picks[0].chosen) === '["Go"]' && qOf(p, qMany).picks[0].noteOpt === 'Go', qOf(p, qMany).picks[0]);
    await ui('q-pick', 0, 0);
    p = await until('Rust', (q) => qOf(q, qMany)?.picks?.[0]?.chosen?.[0] === 'Rust', 5000);
    if (p) check('the note follows the pick (Go to Rust), its text kept', qOf(p, qMany).picks[0].noteOpt === 'Rust' && qOf(p, qMany).picks[0].note === 'keep it small', qOf(p, qMany).picks[0]);
    await ui('q-pick', 1, 1);
    await ui('q-pick', 1, 0);
    p = await until('two ticks', (q) => qOf(q, qMany)?.picks?.[1]?.chosen?.length === 2, 5000);
    if (p) check('multi-select: ticks in the options\' order; Submit still disabled (Deploy unanswered)', JSON.stringify(qOf(p, qMany).picks[1].chosen) === '["Unit","Integration"]' && qOf(p, qMany).can === false, qOf(p, qMany));
    await snap('2.13-several');
    // The answer draft survives a restart.
    const before = JSON.stringify(qOf(await props(), qMany).picks);
    check('the answer draft is kept (drafts store)', JSON.stringify(qOf(await props(), qMany).draft) === before);
    d.close();
    app.kill();
    await sleep(1500);
    app = launch('run6');
    d = await driver();
    p = await until('after a restart: the card back with its picks', (q) => q.signedIn && JSON.stringify(qOf(q, qMany)?.picks) === before, 40000);
    check('after a client restart, the half-answered card comes back as it was', !!p, p && qOf(p, qMany));
    // Other, then Enter: submits (the web does nothing on Enter: a named difference).
    p = await props();
    await ui('focus-pane', pane(p, qMany).id);
    await ui('q-other-focus', 2);
    await sleep(300);
    await keys('Somewhere else');
    p = await until('Other typed', (q) => qOf(q, qMany)?.picks?.[2]?.other === 'Somewhere else' && qOf(q, qMany)?.can === true, 5000);
    if (p) check('Other typed: the question answered, Submit enabled', true);
    await keys('Return');
    p = await until('answered', (q) => qOf(q, qMany)?.status === 'allowed', 10000);
    if (p) check('Enter in Other submits; the card shows its tags in question order with the note', JSON.stringify(qOf(p, qMany).tags) === '["Rust","Unit, Integration","Somewhere else"]' && JSON.stringify(qOf(p, qMany).notes) === '["Note: keep it small"]' && qOf(p, qMany).draft === null, qOf(p, qMany));
    let h = await hist(qMany);
    check('the server got the answers and the note', h?.status === 'allowed' && h.answeredInput?.answers?.[QL] === 'Rust' && h.answeredInput.answers[QT] === 'Unit, Integration' && h.answeredInput.answers[QD] === 'Somewhere else' && h.answeredInput.annotations?.[QL]?.notes === 'keep it small', h && h.answeredInput);
    await snap('2.13-answered');
    // Dismiss.
    p = await props();
    await ui('focus-pane', pane(p, qDis).id);
    // Before dismissing: Other against a pick, the note's ✕, the arrows.
    {
      const pk = async (what, f) => { const r = await until(what, (q) => { const x = qOf(q, qDis)?.picks?.[0]; return x && f(x); }, 5000); return r && qOf(r, qDis).picks[0]; };
      await ui('q-pick', 0, 0);
      await pk('SQLite picked', (x) => x.chosen?.[0] === 'SQLite');
      await ui('q-other', 0, 'Redis');
      let x = await pk('Other typed', (x) => x.other === 'Redis');
      if (x) check('typing in Other un-picks the choices; Submit still enabled (Other answers)', x.chosen.length === 0 && qOf(await props(), qDis).can === true, x);
      await ui('q-pick', 0, 1);
      x = await pk('MySQL picked', (x) => x.chosen?.[0] === 'MySQL');
      if (x) check('a pick clears Other', x.other === '', x);
      await ui('q-note', 0, 0);
      x = await pk('note on SQLite', (x) => x.noteOpt === 'SQLite');
      if (x) check('the note button on SQLite picks it (a single choice: MySQL goes)', JSON.stringify(x.chosen) === '["SQLite"]', x);
      await keys('cache size');
      await pk('note typed', (x) => x.note === 'cache size');
      await ui('q-note-remove', 0);
      x = await pk('note removed', (x) => x.note === '' && x.noteOpt === '');
      if (x) check('the note ✕ clears the note and closes it; the pick stays', JSON.stringify(x.chosen) === '["SQLite"]', x);
      // The arrows in a radio group (real keys to the focused radio).
      await ui('q-move', 0, 0, 1);
      x = await pk('moved to MySQL', (x) => x.chosen?.[0] === 'MySQL');
      if (x) check('an arrow moves to the next choice and picks it', true);
      await key('Down');
      x = await pk('Down wraps to SQLite', (x) => x.chosen?.[0] === 'SQLite');
      if (x) check('↓ on the last radio (a real key) wraps to the first and picks it', true);
      await key('Up');
      x = await pk('Up to MySQL', (x) => x.chosen?.[0] === 'MySQL');
      if (x) check('↑ (a real key) moves back and picks', true);
      await key('Escape');
      await sleep(300);
      check('Esc does nothing to the card', qOf(await props(), qDis)?.status === 'pending' && qOf(await props(), qDis)?.picks?.[0]?.chosen?.[0] === 'MySQL', qOf(await props(), qDis));
    }
    await ui('q-dismiss');
    p = await until('dismissed', (q) => qOf(q, qDis)?.dismissed === true, 10000);
    if (p) check('Dismiss: the card shows "dismissed" at once', true);
    h = await hist(qDis);
    check('the server got deny', h?.status === 'denied' && !h.answeredInput?.answers, h);
    // Orphaned: the asking process goes; the card stays answerable.
    const proc = chatProc(qOrph);
    if (check('the orphaned chat has a claude process', !!proc)) {
      process.kill(proc.pid, 'SIGKILL');
      p = await until('orphaned', (q) => qOf(q, qOrph)?.orphaned === true, 30000);
      if (p) check('the process gone: the card is orphaned (cancelled), still open', qOf(p, qOrph).open === true, qOf(p, qOrph));
      await ui('focus-pane', pane(await props(), qOrph).id);
      await ui('q-pick', 0, 0);
      await until('picked', (q) => qOf(q, qOrph)?.can === true, 5000);
      await ui('q-submit');
      p = await until('answered late', (q) => qOf(q, qOrph)?.status === 'answered-late', 10000);
      if (p) check('an orphaned answer shows its tag at once', JSON.stringify(qOf(p, qOrph).tags) === '["SQLite"]', qOf(p, qOrph));
      // (The server records it a moment after the card shows it.)
      for (let i = 0; i < 20; i++) { h = await hist(qOrph); if (h?.status === 'answered-late') break; await sleep(300); }
      check('the server marked it answered-late', h?.status === 'answered-late' && h.answeredInput?.answers?.['Which database for the queue?'] === 'SQLite', h);
    }
  }
  for (const id of [qMany, qDis, qOrph]) await http('POST', `/api/ui-sessions/${id}/close`, {}, token);
}

// ─── 2.14 Tool cards ───
// A real turn: Bash (with a description), Read, Edit and a failing command.
// Then, as the server's own frames: a running call patched to done (an open
// card stays open), a TodoWrite (this CLI has none) and an agent row.
{
  writeFileSync(join(demoPath, 'app.js'), 'const port = 3000;\nconsole.log("listening on", port);\n');
  const tc = await make('demo', 'Tool calls', true);
  await send(tc, 'This is a UI test. Make exactly these tool calls, one per message, in this order: 1. Bash `ls -la` with description "List the project files"; 2. Read app.js; 3. Edit app.js, replace `const port = 3000;` with `const port = 4000;`; 4. Bash `cat missing-file.txt` (it will fail). Then reply "Done."');
  const cards = (rows) => (rows || []).flatMap((r) => r.card ? [r.card] : r.group ? r.members : []);
  p = await until('the tool turn ends', (q) => pane(q, tc)?.state === 'idle' && cards(pane(q, tc)?.tools).length >= 4, 240000);
  if (check('a turn with four tool calls', !!p, p && pane(p, tc)?.tools)) {
    await ui('focus-pane', pane(p, tc).id);
    // Open every group, then every card.
    for (const r of pane(p, tc).tools) if (r.group) await ui('tool-toggle', r.group);
    p = await props();
    for (const c of cards(pane(p, tc).tools)) await ui('tool-toggle', c.id);
    p = await until('all open', (q) => cards(pane(q, tc).tools).every((c) => c.open) && pane(q, tc).tools.every((r) => !r.group || r.open), 5000);
    const all = cards(pane(p, tc).tools);
    const bash = all.find((c) => c.name === 'Bash' && c.summary === 'ls -la');
    check('Bash: green dot, the raw name, summary "ls -la"; open: the command, its description, the listing', bash && bash.status !== 'running' && !bash.error && bash.body === 'bash' && bash.args === 'ls -la' && bash.note === 'List the project files' && /app\.js/.test(bash.result), bash);
    const read = all.find((c) => c.name === 'Read');
    check('Read: summary …/<dir>/app.js; open: the JSON input and the file', read && /^…\/[^/]+\/app\.js$/.test(read.summary) && read.body === 'json' && read.args.includes('"file_path"') && /const port/.test(read.result), read);
    const edit = all.find((c) => c.name === 'Edit');
    check('Edit: the diff, "- const port = 3000;" removed and "+ const port = 4000;" added', edit && /app\.js$/.test(edit.summary) && edit.lines.some((l) => l.text === '- const port = 3000;' && !l.add) && edit.lines.some((l) => l.text === '+ const port = 4000;' && l.add), edit);
    const bad = all.find((c) => c.name === 'Bash' && /cat missing-file\.txt/.test(c.summary));
    check('the failing Bash: red, its result in red with "No such file"', bad && bad.error && /No such file/.test(bad.result), bad);
    const groups = pane(p, tc).tools.filter((r) => r.group);
    if (groups.length) {
      check('group heads count their calls ("N tools")', groups.every((g) => g.count === g.members.length && g.count >= 1), groups.map((g) => [g.summary, g.count]));
      const gBad = groups.find((g) => g.members.some((m) => m.error));
      check('the group with the failed call has a red dot', !gBad || gBad.status === 'failed', gBad);
    }
    await snap('2.14-open');
    // Live: a running call (spinner), opened, then its result: it stays open.
    const ev = (e) => ui('chat-event', JSON.stringify(e));
    await ev({ type: 'entry', entry: { id: 'e2e-run', kind: 'tool', name: 'Bash', toolUseId: 'tu-run', input: { command: 'sleep 30' }, status: 'running', result: null } });
    await ev({ type: 'entry', entry: { id: 'e2e-todo', kind: 'tool', name: 'TodoWrite', toolUseId: 'tu-todo', input: { todos: [{ content: 'Read the code', status: 'completed' }, { content: 'Change the port', activeForm: 'Changing the port', status: 'in_progress' }, { content: 'Run the tests', status: 'pending' }] }, status: 'done', result: 'ok' } });
    await ev({ type: 'entry', entry: { id: 'e2e-agent', kind: 'tool', name: 'Task', toolUseId: 'tu-agent', input: { description: 'Count lines' }, status: 'running' } });
    await ev({ type: 'agent', agent: { toolUseId: 'tu-agent', kind: 'agent', subagentType: 'general-purpose', description: 'Count lines in app.js', status: 'running' } });
    await ui('tool-toggle', 'e2e-run');
    p = await until('the running card', (q) => pane(q, tc).tools.some((r) => r.card?.id === 'e2e-run' && r.card.open && r.card.status === 'running'), 5000);
    if (p) {
      const todo = pane(p, tc).tools.find((r) => r.card?.id === 'e2e-todo')?.card;
      check('TodoWrite: "3 items"; done, the active one with its activeForm, pending', todo && todo.summary === '3 items' && JSON.stringify(todo.todos) === JSON.stringify([{ text: 'Read the code', state: 'completed' }, { text: 'Changing the port', state: 'in_progress' }, { text: 'Run the tests', state: 'pending' }]), todo);
      const ag = pane(p, tc).tools.find((r) => r.agent === 'e2e-agent');
      check('an agent record turns its call into an agent row', ag && ag.type === 'general-purpose' && ag.status === 'running', ag);
    }
    await snap('2.14-live');
    await ev({ type: 'patch', id: 'e2e-run', patch: { status: 'done', result: 'slept', isError: false } });
    p = await until('patched', (q) => pane(q, tc).tools.some((r) => r.card?.id === 'e2e-run' && r.card.status === 'done'), 5000);
    if (p) check('its result arrives: the open card stays open (the web closes it: a named difference)', pane(p, tc).tools.find((r) => r.card?.id === 'e2e-run').card.open === true && pane(p, tc).tools.find((r) => r.card?.id === 'e2e-run').card.result === 'slept');
    // More kinds: Write (its content), Grep, an MCP tool (key=value summary).
    await ev({ type: 'entry', entry: { id: 'e2e-write', kind: 'tool', name: 'Write', toolUseId: 'tu-write', input: { file_path: join(demoPath, 'CHANGELOG.md'), content: '# Changelog\n- Port 4000.' }, status: 'done', result: 'ok' } });
    await ev({ type: 'entry', entry: { id: 'e2e-grep', kind: 'tool', name: 'Grep', toolUseId: 'tu-grep', input: { pattern: 'port', glob: '*.js' }, status: 'done', result: 'app.js' } });
    await ev({ type: 'entry', entry: { id: 'e2e-mcp', kind: 'tool', name: 'mcp__crundi__list_services', toolUseId: 'tu-mcp', input: { all: true }, status: 'done', isError: true, result: 'boom' } });
    p = await until('three more cards', (q) => ['e2e-write', 'e2e-grep', 'e2e-mcp'].every((id) => cards(pane(q, tc).tools).some((c) => c.id === id)), 5000);
    if (p) {
      const c = (id) => cards(pane(p, tc).tools).find((x) => x.id === id);
      check('Write: summary …/<dir>/CHANGELOG.md; body: its content', /^…\/[^/]+\/CHANGELOG\.md$/.test(c('e2e-write').summary) && c('e2e-write').body === 'write' && c('e2e-write').args === '# Changelog\n- Port 4000.', c('e2e-write'));
      check('Grep: summary "port (*.js)"', c('e2e-grep').summary === 'port (*.js)', c('e2e-grep'));
      check('an MCP tool: its raw name, summary "all=true", failed', c('e2e-mcp').name === 'mcp__crundi__list_services' && c('e2e-mcp').summary === 'all=true' && c('e2e-mcp').error === true && c('e2e-mcp').body === 'json', c('e2e-mcp'));
    }
    // A head clicked again closes it.
    await ui('tool-toggle', 'e2e-run');
    p = await until('closed again', (q) => cards(pane(q, tc).tools).some((c) => c.id === 'e2e-run' && !c.open), 5000);
    if (p) check('a second press on an open card\'s head closes it', true);
    const g0 = pane(await props(), tc).tools.find((r) => r.group && r.open);
    if (g0) {
      await ui('tool-toggle', g0.group);
      p = await until('group closed', (q) => pane(q, tc).tools.some((r) => r.group === g0.group && !r.open), 5000);
      if (p) check('a second press on an open group\'s head closes it', true);
    }
  }
  await http('POST', `/api/ui-sessions/${tc}/close`, {}, token);
}

// ─── 2.15 Thinking and 2.16 the rest of the log ───
// Frames as the server's own (this CLI returns no thinking text).
{
  const lg = await make('demo', 'Log', true);
  p = await until('the log chat has a pane', (q) => pane(q, lg), 30000);
  await ui('focus-pane', pane(p, lg).id);
  const ev = (e) => ui('chat-event', JSON.stringify(e));
  const rowsOf = async () => pane(await props(), lg)?.tools || [];
  const th = async (id) => (await rowsOf()).find((r) => r.think === id);
  // 1-3: a block that starts streaming opens; a newer one folds it.
  await ev({ type: 'entry', entry: { id: 'th1', kind: 'thinking', text: 'Looking at the port setting.', streaming: true, tokens: 0 } });
  let t1 = await th('th1');
  check('a thinking block that starts streaming is open, live', t1?.open === true && t1?.streaming === true, t1);
  for (let i = 0; i < 30; i++) await ev({ type: 'delta', id: 'th1', text: `\nline ${i} of the reasoning` });
  await ev({ type: 'entry', entry: { id: 'th2', kind: 'thinking', text: 'A second thought.', streaming: true, tokens: 0 } });
  t1 = await th('th1'); let t2 = await th('th2');
  check('a newer block folds the older at once (still streaming), and opens', t1?.open === false && t1?.streaming === true && t2?.open === true, { t1, t2 });
  await snap('2.15-newer-folds');
  // 4: finished: open, then folded 5 s later.
  await ev({ type: 'patch', id: 'th2', patch: { streaming: false } });
  t2 = await th('th2');
  check('just finished: still open', t2?.open === true && t2?.streaming === false, t2);
  p = await until('folded after the linger', async () => (await th('th2'))?.open === false, 8000);
  check('it folds 5 s after it finished', !!p);
  // 5: the reader's click wins.
  await ui('tool-toggle', 'th2');
  await sleep(6000);
  check('opened by a click, it stays open', (await th('th2'))?.open === true);
  await ui('tool-toggle', 'th2');
  check('closed by a click', (await th('th2'))?.open === false);
  // 6: no text: the "Thought" line.
  await ev({ type: 'entry', entry: { id: 'th3', kind: 'thinking', text: '', tokens: 1234 } });
  await ev({ type: 'entry', entry: { id: 'th4', kind: 'thinking', text: '' } });
  check('"Thought for ~1.2k tokens" and "Thought"', (await th('th3'))?.detail === 'Thought for ~1.2k tokens' && (await th('th4'))?.detail === 'Thought');
  await ev({ type: 'entry', entry: { id: 'th5', kind: 'thinking', text: 'From history: not streaming.' } });
  check('a block first seen not streaming is folded', (await th('th5'))?.open === false);
  // 2.16: a reply with markdown.
  await ev({ type: 'entry', entry: { id: 'r1', kind: 'assistant-text', text: '# Title\n## The fix\nFirst line\nsecond line, see [docs](https://example.com/a) and https://example.com/b, and /tmp/proj/client.js:12 and `src/server.js`.\n\n```js\nconst a = 1;\nconst long = "' + 'x'.repeat(300) + '";\n```\n\n1) one\n2) two\n\n> a quote\n> another\n\n---\n![pic](https://x/p.png)' } });
  const rows = await rowsOf();
  const prose = rows.filter((r) => r.prose);
  check('# is the heading, ## bold body size', prose.some((r) => r.prose === 'h' && /Title/.test(r.text)) && prose.some((r) => r.prose === 'h4' && /The fix/.test(r.text)), prose);
  check('a paragraph keeps its line breaks; links and bare URLs are links', prose.some((r) => r.prose === 'p' && r.text.includes('First line  \nsecond line') && r.text.includes('[docs](https://example.com/a)') && r.text.includes('<https://example.com/b>,')), prose);
  check('numbered items count from 1; each > line is its own quote; a rule', prose.filter((r) => r.prose === 'li' || r.prose === 'lie').map((r) => r.marker).join() === '1.,2.' && prose.filter((r) => r.prose === 'quote').length === 2 && prose.some((r) => r.prose === 'hr'), prose);
  check('paths are path links (the look; the click waits for phase 3)', prose.some((r) => r.prose === 'p' && r.text.includes('[/tmp/proj/client.js:12](path:/tmp/proj/client.js:12)') && r.text.includes('[`src/server.js`](path:src/server.js)')), prose);
  check('a list\'s last item carries the list\'s gap below it', prose.some((r) => r.prose === 'lie' && r.marker === '2.'), prose.map((r) => r.prose));
  check('a markdown image is not drawn: "!" and the link, as the web', prose.some((r) => r.text.includes('\\![pic](https://x/p.png)')), prose);
  const code = rows.find((r) => r.code !== undefined);
  check('the code block: its text, the fence\'s trailing newline dropped', code && code.code.startsWith('const a = 1;') && !code.code.endsWith('\n'), code);
  await ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'n1', kind: 'notice', text: 'Context compacted.' } }));
  await ev({ type: 'entry', entry: { id: 'e1', kind: 'error', text: 'Process error: boom' } });
  await ev({ type: 'entry', entry: { id: 'res1', kind: 'result', subtype: 'error_max_turns', durationMs: 1000 } });
  await ev({ type: 'entry', entry: { id: 'res2', kind: 'result', subtype: 'success', durationMs: 12300, costUsd: 0.0421 } });
  const es = (await rowsOf()).filter((r) => r.entry);
  check('a failed turn shows its subtype as an error; the end of a turn its time and cost', es.some((r) => r.entry === 'result' && r.error && r.text === 'error_max_turns') && es.some((r) => r.entry === 'result' && !r.error && r.detail === '12.3s · $0.0421'), es);
  await snap('2.16-reply');
  // Copy and links (the handlers; the clipboard as the system holds it).
  await ui('tool-copy', 'const a = 1;');
  p = await until('copied', (q) => q.clipboard === 'const a = 1;' && q.toast === 'Copied', 5000);
  check('the copy button puts the block in the clipboard: "Copied"', !!p, p && { clipboard: (await props()).clipboard });
  await ui('open-link', 'https://example.com/a');
  check('a link opens in the system browser (recorded under test)', (await props()).lastOpened === 'https://example.com/a');
  // F87: wheel steps up from the bottom stay; the jump button.
  for (let i = 0; i < 40; i++) await ev({ type: 'entry', entry: { id: 'fill' + i, kind: 'notice', text: 'Filler line ' + i } });
  await ui('log-watch', 'live:' + lg);
  p = await until('at the bottom', (q) => q.log.key === 'live:' + lg && q.log.dist < 2 && q.log.stick, 5000);
  check('the log follows the bottom', !!p, (await props()).log);
  const lp = pane(await props(), lg);
  const wx = WB.x + lp.x + lp.w / 2, wy = WB.y + lp.y + lp.h / 3;
  const ds = [];
  for (let i = 0; i < 3; i++) { await d.send({ cmd: 'mouse', kind: 'wheel', x: wx, y: wy, dy: 100 }); await sleep(300); ds.push((await props()).log.dist); }
  check('three wheel steps up: further each time, never back at the bottom', ds[0] > 2 && ds[1] > ds[0] && ds[2] > ds[1], ds);
  await ev({ type: 'entry', entry: { id: 'late', kind: 'notice', text: 'Arrived while reading up' } });
  await sleep(300);
  p = await props();
  check('an arrival does not pull the log down; the jump button shows its dot', p.log.dist >= ds[2] - 2 && !p.log.stick && p.log.jump && p.log.jumpNew, p.log);
  await snap('2.16-jump');
  // F91: the jump button glides to the end and lets go: a notch up moves the log again.
  p = await props();
  await mouse('click', p.log.jumpX, p.log.jumpY);
  p = await until('jumped', (q) => q.log.dist < 2 && q.log.stick && !q.log.jump, 5000);
  check('the jump button takes the log to the end and hides', !!p, (await props()).log);
  await d.send({ cmd: 'mouse', kind: 'wheel', x: wx, y: wy, dy: 15 });
  await sleep(500);
  p = await props();
  check('after the jump, one notch up moves the log (and stays)', p.log.dist > 2 && !p.log.stick, p.log);
  // F87: one notch up, then an arrival: it stays up.
  await ev({ type: 'entry', entry: { id: 'late2', kind: 'notice', text: 'Another arrival' } });
  await sleep(400);
  p = await props();
  check('one notch up stays up after an arrival', p.log.dist > 2 && !p.log.stick, p.log);
  // Back to the bottom (a notch down at a time).
  for (let i = 0; i < 6 && !(await props()).log.stick; i++) { await d.send({ cmd: 'mouse', kind: 'wheel', x: wx, y: wy, dy: -100 }); await sleep(200); }
  p = await until('following again', (q) => q.log.stick && q.log.dist < 2, 5000);
  check('scrolling back to the bottom follows again', !!p, (await props()).log);
  // F90a: a block folding while following leaves no blank band.
  await ev({ type: 'entry', entry: { id: 'thF', kind: 'thinking', text: Array.from({ length: 12 }, (_, k) => 'reasoning line ' + k).join('\n'), streaming: true } });
  await sleep(400);
  await ev({ type: 'entry', entry: { id: 'thG', kind: 'thinking', text: 'Newer.', streaming: true } });
  await sleep(700);
  p = await props();
  check('a block folding while following: still at the end, no blank band', p.log.dist < 2 && p.log.over < 2 && p.log.stick, p.log);
  // F90b: opening a block at the bottom keeps the end in view.
  await ui('tool-toggle', 'thF');
  await sleep(700);
  p = await props();
  check('a block opened at the bottom while following stays at the end', p.log.dist < 2 && p.log.over < 2, p.log);
  // F90c: a narrower window while following: still at the end.
  await d.send({ cmd: 'resize', w: 900, h: 860 });
  await sleep(800);
  p = await props();
  check('a narrower window while following stays at the end', p.log.dist < 2 && p.log.over < 2 && p.log.stick, p.log);
  await d.send({ cmd: 'resize', w: 1400, h: 860 });
  await sleep(500);
  // F96: a REAL wheel notch (an X event through xdotool, not the test
  // control) from the bottom leaves it and stays there after an arrival.
  {
    const { execFileSync } = await import('node:child_process');
    let xd = null;
    try { xd = execFileSync('xdotool', ['search', '--pid', String(app.pid)], { env: { ...process.env, DISPLAY: process.env.DISPLAY || ':99' } }).toString().trim().split('\n').filter(Boolean); } catch { xd = null; }
    if (!xd?.length) { check('xdotool finds the client window (for the real wheel check)', false, xd); } else {
      await ev({ type: 'entry', entry: { id: 'rw0', kind: 'notice', text: 'Before the real wheel' } });
      p = await until('at the end', (q) => q.log.stick && q.log.dist < 2, 5000);
      const lq = pane(await props(), lg);
      const rx = Math.round(WB.x + lq.x + lq.w / 2), ry = Math.round(WB.y + lq.y + lq.h / 3);
      const env = { ...process.env, DISPLAY: process.env.DISPLAY || ':99' };
      // The window that is the client's (the largest one of its windows).
      const win = xd.map((id) => { try { const g = execFileSync('xdotool', ['getwindowgeometry', id], { env }).toString(); const m = g.match(/Geometry: (\d+)x(\d+)/); return { id, area: m ? +m[1] * +m[2] : 0 }; } catch { return { id, area: 0 }; } }).sort((x, y) => y.area - x.area)[0].id;
      execFileSync('xdotool', ['mousemove', '--window', win, String(rx), String(ry)], { env });
      await sleep(200);
      // (XTEST: a real button event at the pointer; a synthetic one sent to
      // the window is ignored.)
      execFileSync('xdotool', ['click', '4'], { env });
      await sleep(800);
      p = await props();
      check('a real wheel notch up from the bottom leaves it (following off)', p.log.dist > 2 && !p.log.stick, p.log);
      const d1 = p.log.dist;
      await ev({ type: 'entry', entry: { id: 'rw1', kind: 'notice', text: 'After the real wheel' } });
      await sleep(600);
      p = await props();
      check('…and an arrival does not pull it back', p.log.dist >= d1 - 2 && !p.log.stick, p.log);
      // Real notches down to the end follow again.
      for (let i = 0; i < 8 && !(await props()).log.stick; i++) { execFileSync('xdotool', ['click', '5'], { env }); await sleep(400); }
      p = await until('following', (q) => q.log.stick && q.log.dist < 2, 4000);
      check('real notches down to the end follow again', !!p, (await props()).log);
      await parkPointer();
    }
  }
  await http('POST', `/api/ui-sessions/${lg}/close`, {}, token);
}

// ─── 2.16m Media under show_* calls ───
// Real files stored through /api/mcp/call with the chat's key, each result
// injected as the call's done tool entry (as a real turn would produce it).
{
  const { execFileSync } = await import('node:child_process');
  const med = tmpDir('crundi-media-');
  const have = (bin) => { try { execFileSync('which', [bin]); return true; } catch { return false; } };
  if (!have('convert')) { console.log('skipping 2.16m: no ImageMagick'); } else {
    execFileSync('convert', ['-size', '1200x700', 'gradient:steelblue-navy', join(med, 'a.png')]);
    execFileSync('convert', ['-size', '600x900', 'gradient:orange-red', join(med, 'b.jpg')]);
    execFileSync('convert', ['-size', '500x500', 'xc:seagreen', join(med, 'c.webp')]);
    execFileSync('convert', ['-size', '400x300', 'xc:purple', join(med, 'd.gif')]);
    writeFileSync(join(med, 'export.csv'), 'a,b\n1,22222\n');
    const mc = await make('demo', 'Media', true);
    p = await until('the media chat has a pane', (q) => pane(q, mc), 30000);
    await ui('focus-pane', pane(p, mc).id);
    const DATA = process.env.CRUNDI_TEST_DATA || `/tmp/crundi-devtest-${new URL(SERVER).port}/data`;
    const mf = join(DATA, 'dev-mcp', `${mc}.json`);
    await until('its MCP config', () => existsSync(mf), 20000);
    const KEY = JSON.parse(readFileSync(mf, 'utf8')).mcpServers.crundi.env.CRUNDI_API_KEY;
    const show = async (tool, args) => (await fetch(SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY }, body: JSON.stringify({ tool, args: { sessionId: mc, ...args } }) })).json();
    let n = 0;
    const put = (tool, res) => ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'med' + (++n), kind: 'tool', name: 'mcp__crundi__' + tool, toolUseId: 'tm' + n, input: {}, status: 'done', isError: false, result: JSON.stringify(res) } }));
    const card = async (k) => pane(await props(), mc).tools.filter((r) => r.card)[k];
    // The four formats in a row.
    const r4 = await show('show_image', { paths: ['a.png', 'b.jpg', 'c.webp', 'd.gif'].map((f) => join(med, f)), caption: 'Four formats' });
    check('show_image stores four pictures', r4.ok && r4.items?.length === 4, r4);
    await put('show_image', r4);
    p = await until('decoded', async (q) => { const c = await card(0); return c?.media?.items?.every((i) => i.state === 'ready'); }, 15000);
    const c4 = await card(0);
    check('four boxes 220 tall (377 / 147 / 220 / 293), the call\'s caption', c4 && c4.media.caption === 'Four formats' && JSON.stringify(c4.media.items.map((i) => [i.w, i.h])) === JSON.stringify([[377, 220], [147, 220], [220, 220], [293, 220]]), c4?.media);
    const sizes = (await props()).media;
    check('PNG, JPEG, WebP and GIF all decode, at their pixel sizes', !!p && ['png', 'jpg', 'webp', 'gif'].every((x) => Object.entries(sizes).some(([k, v]) => k.endsWith('.' + x) && v.state === 'ready' && v.size)) && Object.values(sizes).some((v) => JSON.stringify(v.size) === '[600,900]'), sizes);
    await snap('2.16m-formats');
    // Gone.
    await put('show_image', { ok: true, items: [{ kind: 'image', name: '0123456789abcdef.png', w: 300, h: 200, caption: 'Gone' }] });
    p = await until('gone', async () => (await card(1))?.media?.items?.[0]?.state === 'gone', 10000);
    check('a picture no longer stored: "gone"', !!p, (await card(1))?.media);
    // The viewer.
    await ui('view', `${mc}|${r4.items[0].name}`);
    p = await until('the viewer', (q) => q.viewer?.ready, 5000);
    check('a click on a picture: the viewer, "1 / 4" (the chat\'s pictures that loaded: not the gone one)', p && p.viewer.at === 0 && p.viewer.count === 4, p?.viewer);
    await key('Right');
    await until('2 / 4', (q) => q.viewer?.at === 1, 3000) && check('→ shows the next picture', true);
    await snap('2.16m-viewer');
    await key('Escape');
    await until('closed', (q) => q.viewer === null, 3000) && check('Esc closes the viewer', true);
    // A file: Download to a path the test gives (instead of the dialog).
    const rf = await show('show_file', { path: join(med, 'export.csv'), caption: 'The export' });
    await put('show_file', rf);
    const cf = await until('the file card', async () => (await card(2))?.media?.items?.[0]?.kind === 'file', 5000);
    check('the file card: its name and "12 B · The export"', cf && (await card(2)).media.items[0].line === '12 B · The export', (await card(2))?.media);
    const out = join(med, 'saved.csv');
    await ui('pick-next', out);
    await ui('download', rf.items[0].id, 'export.csv');
    p = await until('saved', (q) => q.downloads?.some((d) => d.state === 'done'), 10000);
    check('Download: "Saved · 12 B", the bytes as stored', p && p.downloads.find((d) => d.state === 'done').line === 'Saved · 12 B' && readFileSync(out, 'utf8') === 'a,b\n1,22222\n', p?.downloads);
    writeFileSync(join(med, 'export.csv'), '');
    execFileSync('rm', [join(med, 'export.csv')]);
    await ui('pick-next', join(med, 'again.csv'));
    await ui('download', rf.items[0].id, 'export.csv');
    p = await until('410', (q) => q.downloads?.some((d) => d.state === 'failed'), 10000);
    check('a file no longer where it was: the row says so, failed', p && /no longer where it was/.test(p.downloads.find((d) => d.state === 'failed').line), p?.downloads);
    await snap('2.16m-download');
    // Frames: captions and Open.
    const rh = await show('show_embed', { html: '<h1>Counter</h1>', title: 'Counter', height: 160 });
    const ru = await show('show_embed', { url: 'https://example.com/', title: 'A plain page' });
    await put('show_embed', rh);
    await put('show_embed', ru);
    await sleep(500);
    const fh = (await card(3))?.media?.items?.[0], fu = (await card(4))?.media?.items?.[0];
    check('a stored HTML frame: 640 × its height, its title, no Open', fh && fh.w === 640 && fh.h === 160 && fh.caption === 'Counter' && fh.link === '', fh);
    check('an https frame: "… · Open on example.com"', fu && fu.caption === 'A plain page' && fu.link === 'Open on example.com', fu);
    await ui('open-frame', '', 'https://example.com/');
    check('Open: the system browser with its url (recorded)', (await props()).lastOpened === 'https://example.com/');
    // A video: the system player with the file's path, never a URL.
    if (have('ffmpeg')) {
      execFileSync('ffmpeg', ['-loglevel', 'error', '-f', 'lavfi', '-i', 'testsrc=duration=6:size=320x240:rate=10', '-pix_fmt', 'yuv420p', join(med, 'v.mp4')]);
      const rv = await show('show_video', { path: join(med, 'v.mp4'), caption: 'The test pattern' });
      await put('show_video', rv);
      p = await until('the video', async () => (await card(5))?.media?.items?.[0]?.kind === 'video', 5000);
      const vi = (await card(5))?.media?.items?.[0];
      check('a video: its box and "The test pattern · 0:06"', vi && vi.caption === 'The test pattern · 0:06', vi);
      await ui('play', rv.items[0].name, 'v.mp4');
      p = await until('played', (q) => q.lastOpened.endsWith(rv.items[0].name), 15000);
      check('play: the file in the temp folder, opened by path (not a URL)', p && existsSync(p.lastOpened) && !/^https?:/.test(p.lastOpened) && readFileSync(p.lastOpened).length === readFileSync(join(med, 'v.mp4')).length, p?.lastOpened);
      await snap('2.16m-video');
    }
    await http('POST', `/api/ui-sessions/${mc}/close`, {}, token);
  }
}

// ─── 2.17 Suggestions and recaps; 2.21 Side questions ───
// This CLI sends no prompt_suggestion: both come as the server's own frames.
// Side questions' answers are stubbed (a real one needs a long turn).
{
  const sg = await make('demo', 'Suggest', true);
  p = await until('the chat idle', (q) => pane(q, sg)?.state === 'idle', 60000);
  await ui('focus-pane', pane(p, sg).id);
  const ev = (e) => ui('chat-event', JSON.stringify(e));
  const dk = async () => pane(await props(), sg)?.dock;
  await ev({ type: 'suggestion', suggestion: { text: 'Add due dates to the tasks', uuid: 'u1' } });
  await ev({ type: 'recap', recap: { text: 'You are testing the dock; next, the side questions.', why: 'resumed', at: Date.now() } });
  p = await until('the dock', async () => { const k = await dk(); return k?.suggestion && k?.recap; }, 5000);
  check('a suggestion and a recap show in the dock', !!p, await dk());
  await snap('2.17-dock');
  // The suggestion sent: it and the recap go; the box keeps what is typed.
  await ui('composer-focus');
  await keys('my own words');
  await ui('stub', 'send', JSON.stringify({ ok: true, injected: false }));
  await ui('sug-send');
  p = await until('sent', async () => { const k = await dk(); return k && k.suggestion === '' && k.recap === null; }, 5000);
  check('a click on the suggestion sends it; it and the recap go; the box keeps its text', !!p && pane(await props(), sg).draft === 'my own words', { dock: await dk(), draft: pane(await props(), sg).draft });
  await clearBox();
  // Dismissing.
  await ev({ type: 'suggestion', suggestion: { text: 'Another', uuid: 'u2' } });
  await ui('sug-dismiss');
  check('× dismisses the suggestion', (await dk())?.suggestion === '');
  await ev({ type: 'recap', recap: { text: 'Away.', why: 'idle' } });
  await ui('recap-dismiss');
  check('× dismisses the recap', (await dk())?.recap === null);
  // A side question: answered (markdown), the box emptied, nothing queued, the recap dismissed.
  await ev({ type: 'recap', recap: { text: 'Before asking.', why: 'idle' } });
  await ui('stub', 'aside', JSON.stringify({ status: 200, body: { ok: true, answer: 'Writing **design.md**.' } }));
  await keys('/btw what file are you writing?');
  await keys('Return');
  p = await until('answered', async () => (await dk())?.asides?.[0]?.state === 'done', 5000);
  const q1 = pane(await props(), sg);
  check('/btw: a card answered in place; the box empty, nothing queued, the recap dismissed', !!p && q1.dock.asides[0].question === 'what file are you writing?' && /design\.md/.test(q1.dock.asides[0].text) && q1.draft === '' && q1.qSent === '' && q1.dock.recap === null, q1.dock);
  // An error.
  await ui('stub', 'aside', JSON.stringify({ status: 200, body: { ok: false, error: 'E3' } }));
  await keys('/btw and then?');
  await keys('Return');
  p = await until('error', async () => (await dk())?.asides?.[1]?.state === 'error', 5000);
  check('a refused side question: the red box with its error', !!p && (await dk()).asides[1].text === 'E3', await dk());
  await snap('2.21-cards');
  // A server without side questions (404): no card; sent as typed.
  await ui('stub', 'aside', JSON.stringify({ status: 404, body: null }));
  await ui('stub', 'send', JSON.stringify({ ok: true, injected: false }));
  await keys('/btw old server');
  await keys('Return');
  await sleep(800);
  check('404: no card, the text sent as typed', (await dk()).asides.length === 2);
  // × closes one card; the other stays.
  await ui('aside-close', String((await dk()).asides[0].id));
  check('× closes one card', (await dk()).asides.length === 1 && (await dk()).asides[0].state === 'error');
  // "/btw" alone is not a side question.
  await ui('stub', 'send', JSON.stringify({ ok: true, injected: false }));
  await keys('/btw');
  // (The slash menu offers /btw: Esc closes it, then Enter sends.)
  await key('Escape');
  await keys('Return');
  await sleep(500);
  check('"/btw" alone is sent as a message, not asked', (await dk()).asides.length === 1 && pane(await props(), sg).draft === '', { asides: (await dk()).asides.length, draft: pane(await props(), sg).draft });
  // ─── 2.18 The agent dock (records as the server's frames) ───
  await ev({ type: 'agent', agent: { toolUseId: 'ta1', kind: 'agent', subagentType: 'general-purpose', description: 'Count lines in app.js', status: 'running', prompt: 'Count the lines.' } });
  await ev({ type: 'agent', agent: { toolUseId: 'tt1', kind: 'task', description: 'npm test', status: 'completed', endedAt: Date.now(), usage: { total_tokens: 812 } } });
  const ad = async () => pane(await props(), sg)?.agentDock;
  p = await until('badges', async () => (await ad())?.agents === 1 && (await ad())?.commands === 1, 5000);
  check('two badges: an agent running, a command finished, with their tooltips', !!p && (await ad()).agentsRunning === true && (await ad()).agTip === '1 agent, 1 running — click to see it' && (await ad()).cmdTip === '1 background command, all finished — click to see it', await ad());
  await snap('2.18-badges');
  await ui('agent-badge', 'agents');
  let a1 = await ad();
  check('the agents badge opens its list: one row, "1 running"', a1.sheet === 'agents' && a1.rows.length === 1 && a1.rows[0].title === 'Count lines in app.js' && a1.sub === '1 running', a1);
  await ui('agent-open', 'ta1', 'list');
  a1 = await ad();
  check('a row opens its panel with the back link, the prompt, the empty line', a1.sheet === 'panel' && a1.back === '‹ Agents' && a1.prompt === 'Count the lines.' && a1.empty === 'Waiting for the agent’s first step…', a1);
  await ev({ type: 'agent-entry', toolUseId: 'ta1', entry: { id: 'x1', kind: 'assistant-text', text: 'Looking at app.js.' } });
  a1 = await until('entry', async () => (await ad())?.entries === 1, 5000) ? await ad() : await ad();
  check('a live agent entry joins the open panel', a1.entries === 1 && a1.empty === '', a1);
  // F113: the panel follows its end as entries arrive.
  for (let i = 2; i < 32; i++) await ev({ type: 'agent-entry', toolUseId: 'ta1', entry: { id: 'x' + i, kind: 'assistant-text', text: 'Step ' + i + ' of the agent, a line of its transcript.' } });
  p = await until('followed', async () => (await ad())?.entries === 31 && (await props()).panelDist < 2, 5000);
  check('a long live transcript: the panel stays at its end', !!p, { entries: (await ad())?.entries, dist: (await props()).panelDist });
  await snap('2.18-panel');
  await ui('agent-back');
  check('back: the list again', (await ad()).sheet === 'agents');
  await ui('agent-dismiss', 'ta1');
  a1 = await ad();
  check('dismissing the last one: gone from the badge, the list closes', a1.agents === 0 && a1.sheet === '', a1);
  await ui('agent-badge', 'commands');
  await ui('agent-dismiss-all', 'commands');
  a1 = await ad();
  check('Dismiss all: the commands go, the list closes', a1.commands === 0 && a1.sheet === '', a1);
  // ─── 2.19 The goal and the plan ───
  const gl = async () => pane(await props(), sg)?.goal;
  await ev({ type: 'goal', goal: { condition: 'All tests pass', verdicts: 0, looping: true } });
  p = await until('goal', async () => (await gl())?.on, 5000);
  check('the goal bar: its condition, "running" while looping', !!p && (await gl()).condition === 'All tests pass' && (await gl()).count === 'running' && (await gl()).looping, await gl());
  await ev({ type: 'entry', entry: { id: 'gv1', kind: 'goal-verdict', text: 'Two tests still fail.', n: 1 } });
  await ev({ type: 'goal', goal: { condition: 'All tests pass', verdicts: 1, looping: false } });
  p = await until('stopped', async () => (await gl())?.stopped, 5000);
  check('a check and the loop stopped: "1 check", the stopped line', !!p && (await gl()).count === '1 check', await gl());
  await snap('2.19-goal');
  await ev({ type: 'meta', plan: { text: '### Dark-mode toggle\n\n1. Add the setting\n2. Wire it', status: 'proposed' } });
  p = await until('plan', async () => (await gl())?.plan === 'proposed', 5000);
  check('a proposed plan: the PLAN pill', !!p);
  await ui('plan-toggle');
  p = await until('panel', async () => (await gl())?.planOpen && (await gl())?.planBlocks >= 3, 5000);
  check('the pill opens the plan panel: "waiting for your approval", the plan\'s blocks', !!p && (await gl()).planState === 'waiting for your approval', await gl());
  await snap('2.19-plan');
  await ev({ type: 'meta', plan: null });
  p = await until('plan gone', async () => (await gl())?.plan === '' && !(await gl())?.planOpen, 5000);
  check('the plan going closes the panel', !!p, await gl());
  await ev({ type: 'goal', goal: null });
  check('goal null: the bar goes', (await gl()).on === false);
  await http('POST', `/api/ui-sessions/${sg}/close`, {}, token);
}


// ─── 2.22 The resume picker; 2.23 Parked panes ───
// A project of its own, with earlier conversations seeded where Claude Code
// keeps them (~/.claude/projects/<the path, encoded>/<session>.jsonl):
// a heavy one (138.2k tokens), a light one, and twenty more to scroll.
{
  const { homedir } = await import('node:os');
  const { rmSync, utimesSync } = await import('node:fs');
  const rpath = join(tmpdir(), 'crundi-native-resume-' + Date.now());
  const rp = await http('POST', '/api/projects', { alias: 'resume', name: 'Resume', path: rpath, create: true }, token);
  check('a project for the resume picker', rp.status < 300, rp.body);
  const tdir = join(homedir(), '.claude', 'projects', rpath.replace(/[^a-zA-Z0-9]/g, '-'));
  mkdirSync(tdir, { recursive: true });
  const H = 'bbbb2222-3333-4444-8555-666677778888';
  const L = 'aaaa1111-2222-4333-8444-555566667777';
  const transcript = (sid, title, tokens) => [
    // (The token estimate skips the first line: it may be cut.)
    JSON.stringify({ type: 'summary', summary: '' }),
    JSON.stringify({ type: 'ai-title', aiTitle: title, sessionId: sid }),
    JSON.stringify({ type: 'user', sessionId: sid, uuid: sid.slice(0, 9) + 'u001', parentUuid: null, timestamp: new Date().toISOString(), cwd: rpath, message: { role: 'user', content: title } }),
    JSON.stringify({ type: 'assistant', sessionId: sid, uuid: sid.slice(0, 9) + 'a001', parentUuid: sid.slice(0, 9) + 'u001', timestamp: new Date().toISOString(), cwd: rpath, message: { id: 'msg_' + sid.slice(0, 8), role: 'assistant', model: 'claude-test', content: [{ type: 'text', text: 'Done: ' + title }], usage: { input_tokens: 100, cache_read_input_tokens: tokens - 100, cache_creation_input_tokens: 0, output_tokens: 10 } } }),
    '',
  ].join('\n');
  const nowS = Date.now() / 1000;
  const seed = (sid, title, tokens, ageS) => { const f = join(tdir, sid + '.jsonl'); writeFileSync(f, transcript(sid, title, tokens)); utimesSync(f, nowS - ageS, nowS - ageS); };
  seed(H, 'Refactor the billing exports into one module', 138200, 300);
  seed(L, 'A light conversation about the docs', 38100, 7300);
  for (let i = 0; i < 20; i++) seed(`cccc${String(i).padStart(4, '0')}-0000-4000-8000-000000000000`, `Earlier conversation number ${i + 1}, about a long list of things to scroll through`, 20000 + i * 100, 86400 * 2 + i * 60);
  try {
    await ui('select-project', 'resume');
    await ui('add-pane', 'terminal');
    let q0 = await until('a launcher in the project', (q) => q.panes.some((x) => x.kind === 'launcher'));
    let launcher = q0?.panes.find((x) => x.kind === 'launcher');
    const lpane = async () => (await props()).panes.find((x) => x.id === launcher.id);

    // The list: New session first, then the rows, newest first.
    await ui('launch-chat', launcher.id);
    p = await until('UI Mode opens the picker with the earlier conversations', (q) => q.resume?.rows?.length === 22);
    let r = p?.resume;
    check('heading "Open UI Mode"; New session has the keyboard; the rows newest first', r && r.heading === 'Open UI Mode' && r.focus === 0 && r.error === '' && r.rows[0].title === 'Refactor the billing exports into one module' && r.rows[1].title === 'A light conversation about the docs', r);
    check('a row\'s meta: id · age · tokens; heavy marked', r && r.rows[0].meta === 'bbbb2222 · 5m ago · 138.2k tokens' && r.rows[0].heavy && !r.rows[1].heavy && r.rows[1].meta === 'aaaa1111 · 2h ago · 38.1k tokens' && r.rows[2].meta.endsWith('· 2d ago · 20.0k tokens'), r?.rows?.slice(0, 3));
    await snap('2.22-list');
    // The keyboard: ↓ walks the rows and the list follows; Tab reaches Cancel and wraps.
    for (let i = 0; i < 22; i++) await key('Down');
    r = (await props()).resume;
    check('↓ to the last row: it has the keyboard, the list scrolled to show it (heading and Cancel stay)', r.focus === 22 && r.contentH > r.viewH && r.scroll > 0 && r.focusBottom - r.scroll <= r.viewH + 1, r && { focus: r.focus, scroll: r.scroll, viewH: r.viewH, contentH: r.contentH, fb: r.focusBottom });
    await snap('2.22-list-end');
    await key('Down');
    check('↓ stops at the last row', (await props()).resume.focus === 22);
    await key('Tab');
    check('Tab after the last row: Cancel', (await props()).resume.focus === 23);
    await key('Tab');
    check('…then round to New session', (await props()).resume.focus === 0);
    await key('Escape');
    p = await until('Esc cancels the picker', (q) => !q.resume);
    check('…nothing launched: the launcher as it was', p && (await lpane())?.kind === 'launcher' && (await lpane()).launchBusy === 0);

    // A heavy conversation: compact first, or load it all, or back.
    await ui('launch-chat', launcher.id);
    await until('the picker again', (q) => q.resume?.rows?.length === 22);
    await ui('resume-pick', 0);
    p = await until('a heavy conversation offers compacting first', (q) => q.resume?.heavy);
    r = p?.resume;
    check('the heavy choice: the title, its cost, and the three choices (the list gone)', r && r.heavyTitle === 'Refactor the billing exports into one module' && r.heavyIntro === '138.2k tokens — continuing loads all of it into every turn. Compacting loads it once, then summarises it so later turns stay small.' && r.rows.length === 0 && r.focus === 0, r);
    await snap('2.22-heavy');
    await ui('resume-back');
    p = await until('← Back to the list: the list again', (q) => q.resume && !q.resume.heavy && q.resume.rows.length === 22);
    if (p) check('← Back to the list: the list again', true);
    await ui('resume-pick', 0);
    await until('the heavy choice again', (q) => q.resume?.heavy);
    await ui('resume-compact');
    p = await until('Compact first: the chat lands in the launcher\'s leaf', async (q) => !q.resume && (await lpane())?.kind === 'chat');
    check('the create call: compact, the conversation picked', JSON.stringify(p?.lastCreate) === JSON.stringify({ project: 'resume', sessionMode: 'compact', resumeId: H, title: 'Chat (compacted)' }), p?.lastCreate);
    const heavyChat = p && (await lpane()).key.slice(5);
    check('…it has the focus', p && (await lpane()).focused);

    // Skip Permissions; a light conversation resumes at once.
    await ui('add-pane', 'terminal');
    q0 = await until('another launcher', (q) => q.panes.some((x) => x.kind === 'launcher'));
    launcher = q0?.panes.find((x) => x.kind === 'launcher');
    await ui('launch-chat', launcher.id, 'skip');
    p = await until('Skip Permissions opens it too', (q) => q.resume?.rows?.length === 22);
    check('heading "Open UI Mode — Skip Permissions"', p?.resume.heading === 'Open UI Mode — Skip Permissions', p?.resume.heading);
    await ui('resume-pick', 1);
    p = await until('a light conversation resumes in the launcher\'s leaf', async (q) => !q.resume && (await lpane())?.kind === 'chat');
    check('the create call: resume, Skip Permissions', JSON.stringify(p?.lastCreate) === JSON.stringify({ project: 'resume', skipPermissions: true, title: 'Chat (skip perms)', sessionMode: 'resume', resumeId: L }), p?.lastCreate);
    const lightChat = p && (await lpane()).key.slice(5);
    check('…a chat of this project on the server', lightChat && (await serverChat(lightChat))?.project === 'resume', lightChat);

    // Loading shows nothing and a second click is ignored; the list failing:
    // New session and the error.
    await ui('add-pane', 'terminal');
    q0 = await until('a third launcher', (q) => q.panes.some((x) => x.kind === 'launcher'));
    launcher = q0?.panes.find((x) => x.kind === 'launcher');
    await ui('stub', 'resumable', JSON.stringify({ delayMs: 1500, net: 'Failed to fetch' }));
    await ui('launch-chat', launcher.id);
    await sleep(300);
    p = await props();
    check('loading: no dialog, the launcher as it was', !p.resume && (await lpane()).launchBusy === 0);
    await ui('launch-chat', launcher.id);
    p = await until('the list failed: the picker with its error', (q) => q.resume?.error);
    check('…New session and "Could not load earlier conversations: Failed to fetch" (no rows)', p && p.resume.rows.length === 0 && p.resume.error === 'Failed to fetch', p?.resume);
    await snap('2.22-error');
    await ui('resume-cancel');
    await sleep(500);
    check('Cancel closes it; the click made while loading did nothing', !(await props()).resume && (await lpane()).kind === 'launcher');

    // No earlier conversation: no dialog, a new session at once (busy); the
    // create call refused: a toast, the launcher usable again.
    await ui('stub', 'resumable', JSON.stringify({ ok: true, sessions: [] }));
    await ui('stub', 'create', JSON.stringify({ delayMs: 1500, ok: false, error: 'Project "resume" no longer exists' }));
    await ui('launch-chat', launcher.id);
    p = await until('an empty list: no dialog, UI Mode spinning', async (q) => (await lpane())?.launchBusy === 1, 3000);
    check('…a new session asked for', p && !p.resume && JSON.stringify(p.lastCreate) === JSON.stringify({ project: 'resume', sessionMode: 'new', title: 'Chat (new)' }), p?.lastCreate);
    await snap('2.22-launching');
    p = await until('the create call refused: a toast, the launcher usable', async (q) => /no longer exists/.test(q.toast || '') && (await lpane())?.launchBusy === 0);
    if (p) check('the create call refused: a toast, the launcher usable', true);
    // An old server without the list (404): a new session.
    await ui('stub', 'resumable', JSON.stringify({ status: 404, error: 'Not found' }));
    await ui('launch-chat', launcher.id);
    p = await until('no list on this server (404): a new session at once', async (q) => (await lpane())?.kind === 'chat');
    check('…its create call a new session', p && p.lastCreate.sessionMode === 'new', p?.lastCreate);
    const newChat = p && (await lpane()).key.slice(5);

    // ─── 2.23 Parked panes ───
    const leafOf = (q, id) => pane(q, id)?.id;
    const lLeaf = leafOf(await props(), lightChat);
    let pk = await http('POST', `/api/panes/${lightChat}/park`, {}, token);
    check('the server parks a chat', pk.body?.ok, pk.body);
    p = await until('a parked chat keeps its leaf and shows the card', (q) => pane(q, lightChat)?.pk && leafOf(q, lightChat) === lLeaf);
    let c = p && pane(p, lightChat);
    check('the card: "Closed · just now", what Resume does, Peek (it has a conversation), not heavy', c?.pk.why === 'Closed · just now' && c.pk.what === 'Resume picks up this conversation.' && c.pk.session && !c.pk.heavy, c?.pk);
    check('its header: exited, the title, no chat tools', c?.state === 'exited' && !c.narrow && !c.menu, c && { state: c.state, title: c.title });
    p = await until('its stored conversation behind the card', (q) => pane(q, lightChat)?.entries > 0, 8000);
    // (A clean picture: the last step's toast gone first; F73.)
    for (let i = 0; i < 20 && (await props()).toast; i++) await sleep(250);
    await sleep(2500);
    await snap('2.23-parked');
    // Peek, then Esc (native) and Parked view back.
    await ui('peek', 'live:' + lightChat, 'on');
    p = await until('Peek shows the conversation', (q) => pane(q, lightChat)?.pk?.peek);
    await snap('2.23-peek');
    await key('Escape');
    p = await until('Esc leaves Peek', (q) => pane(q, lightChat)?.pk && !pane(q, lightChat).pk.peek);
    if (p) check('Esc leaves Peek', true);
    await ui('peek', 'live:' + lightChat, 'on');
    await ui('peek', 'live:' + lightChat, 'off');
    check('Parked view: the card again', pane(await props(), lightChat)?.pk?.peek === false);
    // A choice while it runs: its label, the card dimmed; refused: a toast, back.
    await ui('stub', 'park-resume', JSON.stringify({ delayMs: 1500, ok: false, error: 'This pane is not parked' }));
    await ui('park-resume', 'live:' + lightChat, 'new');
    p = await until('New session pressed: Starting…', (q) => pane(q, lightChat)?.pk?.busy === 'new', 3000);
    check('the quiet New session reads "Starting…" while it runs (F118)', p && pane(p, lightChat).pk.busyLabel === 'Starting…', p && pane(p, lightChat).pk);
    await snap('2.23-starting');
    p = await until('refused: a toast, the card as it was', (q) => q.toast === 'This pane is not parked' && pane(q, lightChat)?.pk?.busy === '');
    if (p) check('refused: a toast, the card as it was', true);

    // The heavy one: the warning, Compact and resume, New session.
    const hLeaf = leafOf(await props(), heavyChat);
    pk = await http('POST', `/api/panes/${heavyChat}/park`, {}, token);
    p = await until('a heavy parked chat', (q) => pane(q, heavyChat)?.pk?.heavy);
    check('…its warning\'s tokens: 138.2k tokens', p && pane(p, heavyChat).pk.tokens === '138.2k tokens' && pane(p, heavyChat).pk.session, p && pane(p, heavyChat).pk);
    // (A clean picture: the last step's toast gone first; F73.)
    for (let i = 0; i < 20 && (await props()).toast; i++) await sleep(250);
    await sleep(2500);
    await snap('2.23-heavy');
    // Short panes: the card gets tighter (no What, the warning a pill), then
    // one row (no Why, a smaller Resume, the tokens only).
    for (const [h, name] of [[440, 'short'], [330, 'very-short']]) {
      await d.send({ cmd: 'resize', w: 1400, h });
      p = await until('the window at ' + h, (q) => pane(q, heavyChat) && pane(q, heavyChat).h < h - 84, 5000);
      await sleep(400);
      await snap('2.23-' + name);
    }
    await d.send({ cmd: 'resize', w: 1400, h: 860 });
    await until('the window back', (q) => pane(q, heavyChat)?.h > 700, 5000);

    // A send from Peek resumes it first, then goes; the pane swaps in place.
    await ui('peek', 'live:' + lightChat, 'on');
    await ui('focus-pane', lLeaf);
    await ui('send', 'Hello after the park');
    p = await until('a send from Peek resumes the chat in place', (q) => pane(q, lightChat) && !pane(q, lightChat).pk && leafOf(q, lightChat) === lLeaf, 20000);
    if (p) check('a send from Peek resumes the chat in place (same id, same leaf)', true);
    const sent = async () => { const h = (await http('GET', `/api/ui-sessions/${lightChat}/history`, null, token)).body?.session || {}; return [...(h.messages || []).filter((m) => m.kind === 'user').map((m) => m.text), ...(h.pendingInjections || []).map((x) => x.text)]; };
    let got = false;
    for (let i = 0; i < 30 && !got; i++) { got = (await sent()).includes('Hello after the park'); if (!got) await sleep(500); }
    check('…and then the message went', got, await sent());

    // Park, Peek, leave Peek, then Resume on the card: the same chat, live,
    // in the same leaf, on the same conversation.
    const lSess = (await serverChat(lightChat))?.sessionId;
    pk = await http('POST', `/api/panes/${lightChat}/park`, {}, token);
    p = await until('parked again for Resume', (q) => pane(q, lightChat)?.pk && !pane(q, lightChat).pk.peek);
    await ui('peek', 'live:' + lightChat, 'on');
    p = await until('…Peek', (q) => pane(q, lightChat)?.pk?.peek);
    await ui('peek', 'live:' + lightChat, 'off');
    p = await until('…Parked view', (q) => pane(q, lightChat)?.pk && !pane(q, lightChat).pk.peek);
    await ui('park-resume', 'live:' + lightChat, 'resume');
    p = await until('Resume: live again in the same leaf', (q) => pane(q, lightChat) && !pane(q, lightChat).pk && leafOf(q, lightChat) === lLeaf, 20000);
    if (p) check('Resume: live again in the same leaf', true);
    let lSess2 = '';
    for (let i = 0; i < 20 && lSess && lSess2 !== lSess; i++) { lSess2 = (await serverChat(lightChat))?.sessionId || ''; if (lSess2 !== lSess) await sleep(500); }
    check('…on the same conversation', !lSess || lSess2 === lSess, { before: lSess, after: lSess2 });

    // Resume on the card: the same id, live, in the same leaf.
    await ui('park-resume', 'live:' + heavyChat, 'compact');
    p = await until('Compact and resume: live again in the same leaf', (q) => pane(q, heavyChat) && !pane(q, heavyChat).pk && leafOf(q, heavyChat) === hLeaf, 20000);
    if (p) check('Compact and resume: live again in the same leaf', true);

    // × on a parked pane discards it.
    pk = await http('POST', `/api/panes/${newChat}/park`, {}, token);
    p = await until('parked again', (q) => pane(q, newChat)?.pk);
    const nLeaf = leafOf(p, newChat);
    await ui('close-pane', nLeaf);
    await ui('close-pane', nLeaf);
    p = await until('× (twice) discards a parked chat: its pane goes', (q) => !pane(q, newChat));
    const st = await http('GET', `/api/panes/${newChat}/preview`, null, token);
    check('…and the server forgot it', st.body?.ok === false, st.body);

    for (const id of [lightChat, heavyChat]) if (id) await http('POST', `/api/ui-sessions/${id}/close`, {}, token);
  } finally {
    // (After the closed chats' processes have written their last lines.)
    await sleep(1500);
    rmSync(tdir, { recursive: true, force: true });
  }
}


// ─── 2.20 Cards: a factory's result card ───
// A real run on the test server: a git project, a factory (Manager and a
// Developer), its run started, files written into its worktree, and the
// Manager's finish handed off through Claude's MCP bridge (an ordinary
// chat's key): the server posts the card into the Manager's chat.
{
  const { execFileSync } = await import('node:child_process');
  const { unlinkSync } = await import('node:fs');
  const git = (cwd, ...a) => execFileSync('git', a, { cwd, encoding: 'utf8', env: { ...process.env, GIT_AUTHOR_NAME: 'Test', GIT_AUTHOR_EMAIL: 't@e.st', GIT_COMMITTER_NAME: 'Test', GIT_COMMITTER_EMAIL: 't@e.st' } }).trim();
  const fpath = join(tmpdir(), 'crundi-native-fr-' + Date.now());
  mkdirSync(fpath, { recursive: true });
  git(fpath, 'init', '-q', '-b', 'dev');
  writeFileSync(join(fpath, 'index.html'), '<h1>Hello</h1>\n');
  writeFileSync(join(fpath, 'style.css'), 'body { color: black; }\n');
  writeFileSync(join(fpath, 'old.txt'), 'gone soon\n');
  git(fpath, 'add', '.');
  git(fpath, 'commit', '-q', '-m', 'First');
  const first = git(fpath, 'rev-parse', 'dev');
  const fp = await http('POST', '/api/projects', { alias: 'fr', name: 'Factory', path: fpath, create: false }, token);
  check('a git project for a factory run', fp.status < 300, fp.body);
  const quiet = 'You are a test fixture. Never call any tool. Answer every message with the one word: waiting.';
  const def = {
    settings: {},
    roster: [{ id: 'manager', name: 'Manager', persona: quiet }, { id: 'dev', name: 'Developer', persona: quiet }],
    nodes: [{ id: 'start', type: 'start' }, { id: 'mgr', type: 'manager' }, { id: 'end', type: 'end' }, { id: 'dev', type: 'employee', employeeId: 'dev', label: 'Developer' }],
    edges: [{ source: 'start', target: 'mgr' }, { source: 'mgr', sourceHandle: 'finish', target: 'end' }, { source: 'mgr', target: 'dev' }, { source: 'dev', target: 'mgr', maxRounds: 5 }],
  };
  const fc = await http('POST', '/api/factories', { action: 'create', name: 'Site builder', definition: def }, token);
  check('a factory', fc.body?.ok, fc.body);
  const st = await http('POST', '/api/factory-runs', { action: 'start', project: 'fr', factoryId: fc.body?.factory?.id, task: 'Wait. Call no tools.', mergeMode: 'ask' }, token);
  check('its run starts', st.body?.ok && st.body.run?.managerChatId && st.body.run?.worktree, st.body);
  const run = st.body?.run || {};
  const mgr = run.managerChatId;
  if (run.worktree) {
    writeFileSync(join(run.worktree, 'index.html'), '<h1>Hello, site</h1>\n<p>Built by the factory.</p>\n');
    mkdirSync(join(run.worktree, 'assets', 'images', 'icons', 'navigation'), { recursive: true });
    writeFileSync(join(run.worktree, 'assets', 'images', 'icons', 'navigation', 'a-very-long-file-name-for-the-menu-icon.svg'), '<svg/>\n');
    unlinkSync(join(run.worktree, 'old.txt'));
  }
  // An ordinary chat of the project: its MCP key hands the Manager's finish over.
  const helper = (await http('POST', '/api/ui-sessions/create', { project: 'fr', title: 'Helper' }, token)).body?.id;
  const DATA = process.env.CRUNDI_TEST_DATA || `/tmp/crundi-devtest-${new URL(SERVER).port}/data`;
  const keyFile = join(DATA, 'dev-mcp', `${helper}.json`);
  await until('the helper chat\'s MCP config', () => existsSync(keyFile), 20000);
  const KEY = existsSync(keyFile) ? JSON.parse(readFileSync(keyFile, 'utf8')).mcpServers.crundi.env.CRUNDI_API_KEY : '';
  const mcp = (tool, args) => fetch(SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY }, body: JSON.stringify({ tool, args }) }).then((r) => r.json()).catch((e) => ({ ok: false, error: String(e) }));
  const finish = (summary, again) => mcp('factory_handoff', again ? { sessionId: mgr, summary, finish: true } : { sessionId: mgr, summary, finish: true, findings: [
    { text: 'Tidy the header markup', where: 'index.html', blocking: false },
    { text: 'Name the icon by what it shows', blocking: false },
    { text: 'The page has no title', where: 'index.html', blocking: true },
  ] });
  const h1 = await finish('## Done\n\nThe site has a **new heading** and an icon.\n\n- `index.html` reworded\n- an icon added');
  check('the Manager hands its finish over', !(h1?.ok === false), h1);

  try {
    await ui('select-project', 'fr');
    const card = async () => pane(await props(), mgr)?.fr?.cards?.[0];
    p = await until('the Manager\'s chat has a pane, with the card', async (q) => pane(q, mgr) && (await card())?.state === 'ask', 30000);
    check('…its header has the Factory tag', p && pane(p, mgr).factory === true, p && pane(p, mgr)?.title);
    let c = await card();
    const id = c?.id;
    check('the card: title, summary, files line, message, usage, follow-ups, four buttons', c && c.title === 'Factory result · Site builder' && c.blocks >= 3
      && /^\+\d+ −\d+ in 3 files on factory\//.test(c.line) && c.msg === 'Look it over, then choose what happens to it.'
      && c.fuLine === '3 follow-ups from review' && JSON.stringify(c.buttons) === JSON.stringify(['Merge into dev', 'Request changes', 'Keep branch', 'Discard']), c);
    await snap('2.20-ask');
    // The files, and the full diff.
    await ui('fr-toggle', mgr, id, 'files');
    c = await card();
    check('the files line opens the files: name and counts', c.filesOpen && c.files.length === 3 && c.files.some((f) => /a-very-long-file-name-for-the-menu-icon\.svg \+1 −0$/.test(f)) && c.files.some((f) => /^old\.txt \+0 −1$/.test(f)), c.files);
    await ui('fr-diff', mgr, id);
    p = await until('Show the full diff: the diff in its place', async () => (await card())?.diffOn, 10000);
    c = await card();
    check('…plain lines, from "diff --git"', c.diffLines > 5 && c.diffHead.startsWith('diff --git') && c.diffErr === '', c && { lines: c.diffLines, head: c.diffHead });
    await ui('fr-toggle', mgr, id, 'fu');
    c = await card();
    check('follow-ups: F1-F3, F3 blocking, where in brackets', c.fuOpen && c.fus.length === 3 && c.fus[2].id === 'F3' && c.fus[2].blocking && c.fus[0].where === 'index.html', c.fus);
    await snap('2.20-open');
    // Discard arms for 3 s.
    await ui('fr-press', mgr, id, 'discard');
    check('Discard once: "Sure? Discard"', (await card()).buttons[3] === 'Sure? Discard');
    p = await until('…3 s later, Discard again', async () => (await card())?.buttons[3] === 'Discard', 6000);
    if (p) check('…3 s later, Discard again', true);
    // A refused choice: the error, the buttons back.
    await ui('stub', 'fr-finish', JSON.stringify({ delayMs: 1200, ok: false, error: 'The run is running; there is no result waiting for a decision.' }));
    await ui('fr-press', mgr, id, 'keep');
    p = await until('a choice being sent: the buttons wait', async () => (await card())?.busy, 3000);
    await snap('2.20-busy');
    p = await until('refused: the server\'s error, the buttons back', async () => { const x = await card(); return x && !x.busy && x.actErr === 'The run is running; there is no result waiting for a decision.'; }, 6000);
    if (p) check('refused: the server\'s error, the buttons back', true);
    // Request changes: an empty note only takes the keyboard; a note goes.
    await ui('fr-press', mgr, id, 'changes');
    c = await card();
    check('Request changes opens the note (kept open lists)', c.noteOpen && c.filesOpen && c.fuOpen, c);
    const lastBefore = JSON.stringify((await pane(await props(), mgr)).fr.last);
    await ui('fr-send', mgr, id);
    check('Send with an empty note posts nothing', JSON.stringify((await pane(await props(), mgr)).fr.last) === lastBefore);
    await ui('fr-note', mgr, id, 'Give the page a title.');
    await snap('2.20-changes-form');
    await ui('fr-send', mgr, id);
    p = await until('the note sent: the card says changes, no buttons', async () => { const x = await card(); return x?.state === 'changes' && x.buttons.length === 0; }, 15000);
    check('…its body: changes with the note', JSON.stringify(pane(await props(), mgr).fr.last) === JSON.stringify({ action: 'finish', id: run.id, choice: 'changes', note: 'Give the page a title.' }), pane(await props(), mgr).fr.last);
    c = await card();
    check('…in place (the same entry), its message, the summary gone', c && c.id === id && c.msg.startsWith('Changes requested.') && c.blocks === 0, c);
    check('…the sent note gone and the form closed (F126); the earlier refusal\'s error gone with the patch (F124)', c && !c.noteOpen && c.note === '' && c.actErr === '', c && { noteOpen: c.noteOpen, note: c.note, actErr: c.actErr });
    await snap('2.20-changes');
    // The Manager finishes again: the same card asks again.
    const h2 = await finish('Gave the page a title.', true);
    check('a second finish', !(h2?.ok === false), h2);
    p = await until('the same card asks again', async () => { const x = await card(); return x?.state === 'ask' && x.id === id; }, 20000);
    if (p) check('the same card asks again', true);
    c = await card();
    check('…with no note form and no old error (F124, F126)', c && !c.noteOpen && c.note === '' && c.actErr === '', c && { noteOpen: c.noteOpen, note: c.note, actErr: c.actErr });
    // A merge refused (the dev checkout has an uncommitted edit): the
    // card's own error, once (F125).
    writeFileSync(join(fpath, 'index.html'), '<h1>Edited in the checkout</h1>\n');
    await ui('fr-press', mgr, id, 'merge');
    p = await until('a refused merge: the card\'s error', async () => { const x = await card(); return x && !x.busy && x.error !== ''; }, 20000);
    c = await card();
    check('…shown once: above the buttons, no second line under them (F125)', c && c.state === 'ask' && c.error !== '' && c.actErr === '', c && { state: c.state, error: c.error, actErr: c.actErr });
    git(fpath, 'checkout', '--', 'index.html');
    // Merge, then Undo.
    await ui('fr-press', mgr, id, 'merge');
    p = await until('merged: one commit, Undo', async () => { const x = await card(); return x?.state === 'merged' && JSON.stringify(x.buttons) === '["Undo"]'; }, 20000);
    c = await card();
    check('"Merged into **dev** as one commit `…`."', c && /^Merged into \*\*dev\*\* as one commit `[0-9a-f]{7}`\.$/.test(c.msg), c?.msg);
    check('…dev has the commit', git(fpath, 'rev-parse', 'dev') !== first);
    await snap('2.20-merged');
    await ui('fr-press', mgr, id, 'undo');
    p = await until('undone', async () => (await card())?.state === 'undone', 20000);
    c = await card();
    check('Undone: its message, no buttons; dev back where it was', c && c.msg.startsWith('Undone. dev is back where it was') && c.buttons.length === 0 && git(fpath, 'rev-parse', 'dev') === first, c && { msg: c.msg, b: c.buttons });
    // A widget check's screenshots, under its widget_render call.
    await ui('focus-pane', pane(await props(), helper).id);
    await ui('stub', 'renders', JSON.stringify({ ok: true, renders: { at: 1, shots: [{ frame: 'inline', state: 'live', width: 600, height: 300 }, { frame: 'mobile', state: 'dark', width: 390, height: 844 }] } }));
    await ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'wr1', kind: 'tool', name: 'mcp__crundi__widget_render', input: { id: 'w-check' }, status: 'done', toolUseId: 'tu-wr1', result: '{"ok":true}' } }));
    p = await until('a widget check: its screenshots row', (q) => (pane(q, helper)?.tools || []).some((t) => t.shots?.length === 2), 8000);
    const sh = p && pane(p, helper).tools.find((t) => t.shots?.length === 2).shots;
    check('…one per shot, captioned "inline", "mobile · dark"', sh && sh[0].caption === 'inline' && sh[1].caption === 'mobile · dark' && sh[0].key.includes('render-image'), sh);
    // F122: a long user message's bubble is as tall as its text (the
    // selectable copy lines up with what is drawn).
    await ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'u-long', kind: 'user', text: Array.from({ length: 22 }, (_, i) => `Line ${i + 1} of a long step message: {"summary":"…"} and /tmp/a/very/long/path/to/a/worktree/run_mwgymxh95f6eac/main`).join('\n') } }));
    await sleep(800);
    const bb = (await props()).bubble;
    check('a long user bubble is as tall as its drawn text (F122)', bb && Math.abs(bb.box - bb.drawn - 16) < 1 && Math.abs(bb.copy - bb.drawn) < 3, bb);

    // ─── 2.20 gallery: the web screenshots' own data (specs/cards/web-*),
    // replayed into the Manager's pane state by state, for the review's
    // web-over-native crops (native-<state>.png and each pane's rectangle).
    {
      const WEB = { runId: 'run_mwgxunww405aa5', factory: 'Site builder', branch: 'factory/site-builder-405aa5', parentBranch: 'dev' };
      const files8 = [['README.md', 3, 1], ['docs/old.md', 0, 30], ['src/greet.js', 3, 2], ['src/pages/greeting/components/really/deeply/nested/a-very-long-component-name-for-the-greeting.html', 120, 0],
        ['src/pages/greeting/index.html', 1, 0], ['src/pages/greeting/part1.js', 1, 0], ['src/pages/greeting/part2.js', 1, 0], ['src/pages/greeting/part3.js', 1, 0]];
      const tests = Array.from({ length: 45 }, (_, i) => `test/case${i + 1}.test.js`).sort();
      const files53 = [...files8.map((f) => f), ...tests.map((t) => [t, 1, 0])].sort((a, b) => (a[0] < b[0] ? -1 : 1));
      const asFiles = (l) => l.slice(0, 40).map(([file, added, removed]) => ({ file, added, removed }));
      const base = {
        id: 'web-card', kind: 'factory-result', ...WEB, state: 'ask', error: '', commit: '', auto: false, task: 'Build a greeting page.',
        summary: 'Added a **greeting page** under `src/pages/greeting` and taught `greet()` three languages (English, French, Spanish).\n\n- The page says hello in the chosen language.\n- The old docs page is gone; the README says how to start it.',
        diff: { added: 130, removed: 33, count: 8, files: asFiles(files8) },
        followUps: [{ id: 'F1', text: 'The greeting page has no test for an unknown language.', where: 'src/greet.js:2', blocking: false },
          { id: 'F2', text: 'README should mention the lang parameter.', where: 'README.md', blocking: false },
          { id: 'F3', text: 'Spanish greeting misses the opening ¡ mark, which a native speaker would expect at the start of the sentence before Hola.', where: '', blocking: true }],
        usage: { costUsd: 0.18, turns: 1, handoffs: 1, chats: 1, activeMs: 25000 },
      };
      const LONG = 'Made the changes you asked for.\n\n## What changed\n\n- **Spanish** now opens with ¡ before Hola, as a native speaker expects.\n- 45 new tests cover every language, an unknown language (falls back to English), an empty name, a very long name of several hundred characters that has to wrap nicely across the card, and names with accents such as Zoë and Ñandú.\n- The README explains the `lang` parameter with an example: `greet("Ana", "es")` returns `¡Hola, Ana!`.\n\n## What to check\n\n1. Open the greeting page and switch languages.\n2. Run `npm test`: all 45 pass.\n\nThe reviewer left two nits; they are below as follow-ups.';
      const DIFF = 'diff --git a/README.md b/README.md\nindex ffc7886..6e8db8c 100644\n--- a/README.md\n+++ b/README.md\n@@ -1,3 +1,5 @@\n # Demo\n \n-A demo project.\n+A demo project with a greeting page.\n+\n+Run `npm start`.\ndiff --git a/docs/old.md b/docs/old.md\ndeleted file mode 100644\nindex 316a8f0..0000000\n--- a/docs/old.md\n+++ /dev/null\n@@ -1,30 +0,0 @@\n' + Array(30).fill('-line').join('\n') + '\ndiff --git a/src/greet.js b/src/greet.js\nindex 1f2e3d4..5a6b7c8 100644\n--- a/src/greet.js\n+++ b/src/greet.js\n@@ -1,3 +1,4 @@\n-export const greet = (name) => `Hello, ${name}!`;\n-export default greet;\n+const words = { en: \'Hello\', fr: \'Bonjour\', es: \'Hola\' };\n+export const greet = (name, lang = \'en\') => `${words[lang] || words.en}, ${name}!`;\n+export default greet;\n';
      const ev = (e) => ui('chat-event', JSON.stringify(e));
      const patch = (p) => ev({ type: 'patch', id: 'web-card', patch: p });
      const wc = async () => (pane(await props(), mgr)?.fr?.cards || []).find((x) => x.id === 'web-card');
      const rects = {};
      const shoot = async (name) => {
        await sleep(500);
        const q = await props();
        const m = pane(q, mgr);
        rects[name] = m && { x: WB.x + m.x, y: WB.y + m.y, w: m.w, h: m.h };
        await d.send({ cmd: 'snapshot', path: join(OUT, `cards-native-${name}.png`) });
      };
      await ui('focus-pane', pane(await props(), mgr).id);
      await ev({ type: 'entry', entry: base });
      p = await until('the web\'s card in the Manager\'s pane', async () => (await wc())?.state === 'ask', 8000);
      let c = await wc();
      check('the web\'s data: its usage line is drawn ($0.18 · 1 turn · 1 handoff · 1 chat)', c && c.usage === '$0.18 · 1 turn · 1 handoff · 1 chat', c?.usage);
      await shoot('ask-two-panes');
      // Hover: the pointer over Discard, then Request changes (their places
      // in this layout: the buttons' row 121 above the pane's bottom).
      {
        const r = rects['ask-two-panes'];
        await mouse('move', r.x + 462, r.y + r.h - 121);
        await shoot('discard-hover');
        await mouse('move', r.x + 230, r.y + r.h - 121);
        await shoot('button-hover');
        await mouse('leave', 0, 0);
      }
      // Narrow (a 1000-wide window) and the text sizes, on the first card.
      await d.send({ cmd: 'resize', w: 1000, h: 860 });
      await until('narrow', (q) => pane(q, mgr)?.w < 400, 5000);
      await shoot('narrow');
      await d.send({ cmd: 'resize', w: 1400, h: 860 });
      await until('wide again', (q) => pane(q, mgr)?.w > 500, 5000);
      const leaf = pane(await props(), mgr).id;
      await ui('pane-font', leaf, 1); await ui('pane-font', leaf, 1);
      await shoot('text-large');
      await ui('pane-font', leaf, 0); await ui('pane-font', leaf, -1); await ui('pane-font', leaf, -1);
      await shoot('text-small');
      await ui('pane-font', leaf, 0);
      await ui('fr-toggle', mgr, 'web-card', 'files');
      await shoot('files-open');
      await ui('stub', 'fr-diff', JSON.stringify({ ok: true, diff: DIFF, truncated: false }));
      await ui('fr-diff', mgr, 'web-card');
      await until('the diff', async () => (await wc())?.diffOn, 5000);
      await shoot('full-diff');
      await ui('fr-toggle', mgr, 'web-card', 'files');
      await ui('fr-toggle', mgr, 'web-card', 'fu');
      await shoot('followups-open');
      await ui('fr-toggle', mgr, 'web-card', 'fu');
      await ui('fr-press', mgr, 'web-card', 'changes');
      await shoot('changes-form');
      await ui('fr-press', mgr, 'web-card', 'changes');
      await ui('fr-press', mgr, 'web-card', 'discard');
      await shoot('discard-armed');
      await sleep(3200);
      await ui('stub', 'fr-finish', JSON.stringify({ delayMs: 2500, ok: false, error: 'The run is running; there is no result waiting for a decision.' }));
      await ui('fr-press', mgr, 'web-card', 'keep');
      await shoot('busy');
      await until('the error', async () => (await wc())?.actErr !== '', 6000);
      await shoot('action-error');
      await patch({ state: 'changes', summary: '' });
      await until('changes', async () => (await wc())?.state === 'changes', 5000);
      await shoot('changes');
      await patch({ state: 'ask', summary: LONG, diff: { added: 175, removed: 33, count: 53, files: asFiles(files53) }, usage: { costUsd: 0.2, turns: 2, handoffs: 2, chats: 1, activeMs: 60000 } });
      await until('ask again', async () => (await wc())?.state === 'ask', 5000);
      await shoot('long-summary');
      await ui('fr-toggle', mgr, 'web-card', 'files');
      c = await wc();
      check('…53 files: 40 rows and "and 13 more"', c && c.files.length === 40 && c.more === 'and 13 more', c && { n: c.files.length, more: c.more });
      await shoot('long-files-end');
      await ui('fr-toggle', mgr, 'web-card', 'files');
      await patch({ state: 'merged', commit: '6b00fea3c1d2', error: '' });
      await until('merged', async () => (await wc())?.state === 'merged', 5000);
      await shoot('merged');
      await patch({ state: 'undone', diff: { added: 0, removed: 0, count: 0, files: [] } });
      await until('undone', async () => (await wc())?.state === 'undone', 5000);
      await shoot('undone');
      // The states the web's screenshots patched in, with the same patches.
      for (const [name, pt, open] of [
        ['ask-empty', { state: 'ask', summary: 'Added a greeting page.', diff: { added: 0, removed: 0, count: 0, files: [] }, followUps: [], error: '' }],
        ['kept-files-binary', { state: 'kept', summary: 'Added a greeting page.', diff: { added: 12, removed: 3, count: 2, files: [{ file: 'src/greet.js', added: 3, removed: 2 }, { file: 'logo.png', added: null, removed: null }] } }, true],
        ['discarded', { state: 'discarded' }],
        ['conflict', { state: 'conflict', error: 'Merging conflicts in src/greet.js, README.md.', usage: { costUsd: 4.37, turns: 58, handoffs: 14, chats: 5, activeMs: (2 * 60 + 25) * 60000 } }],
        ['merged-auto', { state: 'merged', auto: true, commit: 'a1b2c3d4e5f6', error: '' }],
        ['ask-merge-error', { state: 'ask', auto: false, commit: '', error: 'Your checkout of dev has uncommitted changes to src/greet.js that the merge would overwrite. Commit or stash them, then press Merge again.', usage: { costUsd: 0, turns: 3, handoffs: 2, chats: 1, activeMs: 20000 } }],
      ]) {
        await patch(pt);
        await until(name, async () => (await wc())?.state === pt.state, 5000);
        if (open) await ui('fr-toggle', mgr, 'web-card', 'files');
        await shoot(name);
        if (open) await ui('fr-toggle', mgr, 'web-card', 'files');
      }
      c = await wc();
      check('the merge error\'s usage: "3 turns · 2 handoffs · 1 chat"', c && c.usage === '3 turns · 2 handoffs · 1 chat' && c.error.startsWith('Your checkout of dev'), c && { u: c.usage, e: c.error });
      // The widget cards: the inline widget is phase 3's (nothing); the check's screenshots.
      await ui('stub', 'renders', JSON.stringify({ ok: true, renders: { at: 1, shots: [{ frame: 'inline', state: 'live', width: 560, height: 220 }, { frame: 'mobile', state: 'live', width: 390, height: 844 }] } }));
      await ev({ type: 'entry', entry: { id: 'wgo1', kind: 'tool', name: 'mcp__crundi__widget_open', toolUseId: 'tu_wgo1', input: { id: 'build', title: 'Build status', slot: 'inline' }, status: 'done', result: '{"ok":true}' } });
      await ev({ type: 'entry', entry: { id: 'wgr1', kind: 'tool', name: 'mcp__crundi__widget_render', toolUseId: 'tu_wgr1', input: { id: 'build' }, status: 'done', result: 'No errors or warnings.' } });
      await until('the screenshots row', (q) => (pane(q, mgr)?.tools || []).some((t) => t.shots?.length === 2), 5000);
      await shoot('widget-cards');
      writeFileSync(join(OUT, 'cards-rects.json'), JSON.stringify(rects));
    }
  } finally {
    if (helper) await http('POST', `/api/ui-sessions/${helper}/close`, {}, token);
  }
}


// ─── 2.24 Voice input ───
// Against tools/deepgram-stub.mjs: the test server started with
// DEEPGRAM_HOST=<the stub> and NODE_TLS_REJECT_UNAUTHORIZED=0, and
// CRUNDI_TEST_DEEPGRAM=<the stub> given here. The app's microphone is a WAV
// (CRUNDI_TEST_MIC_WAV).
if (!process.env.CRUNDI_TEST_DEEPGRAM) {
  console.log('skipping 2.24: no CRUNDI_TEST_DEEPGRAM (tools/deepgram-stub.mjs)');
} else {
  const https = await import('node:https');
  const dg = (path, body) => new Promise((resolve) => {
    const [h, port] = process.env.CRUNDI_TEST_DEEPGRAM.split(':');
    const req = https.request({ host: h, port: Number(port), path, method: body ? 'POST' : 'GET', rejectUnauthorized: false, headers: { 'Content-Type': 'application/json' } }, (res) => {
      let t = ''; res.on('data', (c) => (t += c)); res.on('end', () => { try { resolve(JSON.parse(t)); } catch { resolve(null); } });
    });
    req.on('error', () => resolve(null));
    if (body) req.write(JSON.stringify(body));
    req.end();
  });
  const set = await http('POST', '/api/voice/settings', { deepgramKey: 'stub', ttsVoice: 'aura-2-draco-en' }, token);
  check('a voice key set on the server (the stub)', set.status < 300, set.body);
  const vA = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Voice A' }, token)).body?.id;
  const vB = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Voice B' }, token)).body?.id;
  await ui('select-project', 'demo');
  await until('both voice chats have panes', (q) => pane(q, vA) && pane(q, vB), 20000);
  await ui('voice-recheck');
  p = await until('the mic shows (the server can listen)', (q) => pane(q, vA)?.mic === true, 10000);
  if (p) check('the mic shows (a key, and chat-voice)', p.voice.cap === true && p.voice.ok === true);
  const kA = 'live:' + vA, kB = 'live:' + vB;
  const V = async () => (await props()).voice;
  const UI = async () => (await props()).voiceUi;
  const frames = async () => ((await dg('/ctl/stats'))?.conns || []).reduce((n, c) => n + c.frames, 0);
  const sentTo = async (id) => { const h = (await http('GET', `/api/ui-sessions/${id}/history`, null, token)).body?.session || {}; return [...(h.messages || []).filter((m) => m.kind === 'user').map((m) => m.text), ...(h.pendingInjections || []).map((x) => x.text)]; };
  await ui('focus-pane', pane(await props(), vA).id);
  // Voice mode: a tap.
  const draftBefore = pane(await props(), vA).draft;
  await ui('mic-tap', kA);
  p = await until('voice mode on: recording, the strip', async () => (await UI())?.key === kA && (await V())?.rec?.hold === false, 5000);
  if (p) check('a tap turns voice mode on: the strip says Listening', (await UI()).status === 'Listening' && (await UI()).text === '');
  p = await until('audio reaches the server and Deepgram\'s side', async () => (await frames()) > 3, 10000);
  if (p) check('16 kHz frames reach Deepgram through the server', true);
  const stats = await dg('/ctl/stats');
  check('…as linear16 at 16000', (stats?.conns || []).some((c) => /encoding=linear16/.test(c.url) && /sample_rate=16000/.test(c.url)), stats?.conns?.map((c) => c.url));
  p = await until('the listen tone (ready)', async () => (await V())?.tones >= 1, 5000);
  if (p) check('ready: the listen tone', true);
  await snap('2.24-listening');
  await dg('/ctl/say', { interim: 'Could you add a short README' });
  p = await until('words arriving: in the box', async () => (await UI())?.text === 'Could you add a short README', 5000);
  if (p) check('interim words show in the box', true);
  await snap('2.24-interim');
  await dg('/ctl/say', { final: 'What day of the week is it today?', end: true });
  p = await until('an utterance: sent', async () => (await V())?.lastSent === 'voice input: What day of the week is it today?', 8000);
  if (p) check('an utterance goes as "voice input: …"', true);
  let got = false;
  for (let i = 0; i < 20 && !got; i++) { got = (await sentTo(vA)).some((t) => t === 'voice input: What day of the week is it today?'); if (!got) await sleep(400); }
  check('…through the normal send (the chat has it)', got, await sentTo(vA));
  check('…the sent tone, voice mode carries on, the box empty', (await V()).tones >= 2 && (await V()).rec?.hold === false && (await UI()).text === '', await V());
  check('the draft is untouched under the words', pane(await props(), vA).draft === draftBefore);
  // The cancel words: dropped, nothing sent.
  await dg('/ctl/say', { interim: 'Make it blue' });
  await until('words', async () => (await UI())?.text === 'Make it blue', 5000);
  await dg('/ctl/say', { final: 'Make it blue actually scratch that', end: true });
  p = await until('"scratch that": the words go, nothing sent', async () => (await UI())?.text === '', 8000);
  check('…nothing sent', (await V()).lastSent === 'voice input: What day of the week is it today?', (await V()).lastSent);
  // Send now: a tap with words in progress.
  await dg('/ctl/say', { interim: 'Add a footer' });
  await until('words', async () => (await UI())?.text === 'Add a footer', 5000);
  await ui('mic-tap', kA);
  p = await until('a tap with words: sent now', async () => (await V())?.lastSent === 'voice input: Add a footer', 5000);
  if (p) check('a tap with words in progress sends them now', (await V()).rec?.key === kA);
  // Off: a tap with nothing heard; the draft is back.
  await ui('mic-tap', kA);
  p = await until('voice mode off', async () => !(await V())?.rec && (await UI())?.key === '', 5000);
  if (p) check('a tap with nothing heard turns voice mode off; the draft as it was', pane(await props(), vA).draft === draftBefore);
  // Push to talk: held, words, let go: sent as one.
  await ui('mic-press', kA);
  p = await until('held: push to talk', async () => (await V())?.rec?.hold === true, 3000);
  if (p) check('a hold (350 ms) is push to talk', true);
  await sleep(600);
  await dg('/ctl/say', { final: 'Make the table header bold', end: true });
  await until('heard', async () => (await V())?.rec?.heard?.length === 1, 5000);
  await dg('/ctl/say', { interim: 'and sort it' });
  await until('words', async () => (await UI())?.text === 'Make the table header bold and sort it', 5000);
  await snap('2.24-ptt-held');
  await ui('mic-release', kA);
  p = await until('let go: Sending, then sent', async () => (await V())?.lastSent === 'voice input: Make the table header bold', 6000);
  if (p) check('let go: what was heard goes as one voice input', !(await V()).rec);
  // Held, slid onto ×, let go: nothing sent.
  await ui('mic-press', kA);
  await until('held again', async () => (await V())?.rec?.hold === true, 3000);
  await sleep(500);
  await dg('/ctl/say', { interim: 'Delete everything' });
  await ui('mic-near', kA, 'on');
  p = await until('over ×: cancelling', async () => (await UI())?.cancelling === true, 3000);
  check('over ×: the strip still says Listening (in red; F128)', (await UI()).status === 'Listening', await UI());
  await snap('2.24-ptt-cancel');
  await ui('mic-release', kA, 'over');
  await sleep(800);
  check('let go over ×: dropped, nothing sent', !(await V()).rec && (await V()).lastSent === 'voice input: Make the table header bold', await V());
  // Two chats: voice mode in A, a hold in B pauses A, which carries on after.
  await ui('mic-tap', kA);
  await until('A in voice mode', async () => (await V())?.rec?.key === kA, 5000);
  await ui('mic-press', kB);
  p = await until('a hold in B pauses A', async () => (await V())?.rec?.key === kB && (await V())?.away === kA, 3000);
  if (p) check('…A says "Paused: listening in another chat"', (await V()).lines[kA]?.text === 'Paused: listening in another chat');
  await snap('2.24-paused-other');
  await ui('mic-release', kB);
  p = await until('the hold done: A listens again', async () => (await V())?.rec?.key === kA && !(await V())?.away, 6000);
  if (p) check('the hold done: A carries on in voice mode', true);
  // Voice mode moved: on in B turns A off, saying so.
  await ui('mic-tap', kB);
  p = await until('voice mode moved', async () => (await V())?.rec?.key === kB, 5000);
  if (p) check('voice mode on in B: A off with "Voice mode moved to another chat."', (await V()).lines[kA]?.text === 'Voice mode moved to another chat.');
  // Esc in the recording chat ends voice mode.
  await ui('focus-pane', pane(await props(), vB).id);
  await key('Escape');
  p = await until('Esc ends voice mode', async () => !(await V())?.rec, 3000);
  if (p) check('Esc ends voice mode', true);
  // A wide composer while recording: the box keeps the row, the strip is
  // as wide as its contents (F127).
  {
    const leaf = pane(await props(), vA).id;
    await ui('pane-full', leaf);
    await until('full screen', (q) => q.full, 3000);
    await ui('mic-tap', kA);
    p = await until('recording, wide', async () => (await UI())?.key === kA && (await UI())?.stripW > 0 && (await UI())?.boxW > 0, 5000);
    const u = await UI();
    check('wide: the box keeps the row, the strip its contents (F127)', u.boxW > u.stripW * 2 && u.stripW < 260, { box: u.boxW, strip: u.stripW });
    await snap('2.24-wide-recording');
    await ui('mic-tap', kA);
    await until('off', async () => !(await V())?.rec, 3000);
    await ui('pane-full', leaf);
    await until('not full', (q) => !q.full, 3000);
  }
  // The server's error: Deepgram refusing the key.
  await dg('/ctl/mode', { mode: 'refuse' });
  await ui('mic-tap', kA);
  p = await until('refused: the error on the voice line', async () => /Deepgram refused the key/.test((await V())?.lines?.[kA]?.text || '') && !(await V())?.rec, 10000);
  if (p) check('the server\'s error on the voice line (red), recording ends', (await V()).lines[kA].kind === 'error');
  await snap('2.24-err-refused');
  await dg('/ctl/mode', { mode: 'ok' });
  // A reply said: voice mode on, Claude's speak (its voice-say).
  await ui('mic-tap', kA);
  await until('voice mode', async () => (await V())?.rec?.key === kA, 5000);
  const saidBefore = (await V()).said;
  await ui('chat-event', JSON.stringify({ type: 'voice-say', id: 'vs1', text: 'It is Sunday.' }));
  // (Played to the speaker when there is one; without a device it is
  // done at once.)
  p = await until('a voice-say in voice mode is said', async () => (await V())?.said > saidBefore, 15000);
  if (p) check('a voice-say in voice mode is said', true);
  await ui('chat-event', JSON.stringify({ type: 'voice-say', id: 'vs1', text: 'It is Sunday.' }));
  await sleep(600);
  check('…the same id is not said twice', (await V()).said === saidBefore + 1 && (await V()).queue === 0, await V());
  await ui('mic-tap', kA);
  await until('off', async () => !(await V())?.rec, 3000);
  await ui('chat-event', JSON.stringify({ type: 'voice-say', id: 'vs2', text: 'Said after a hold.' }));
  p = await until('voice mode off, a hold here a minute ago: still said (10 min)', async () => (await V())?.said === saidBefore + 2, 15000);
  if (p) check('voice mode off, but a hold here within 10 minutes: the reply is said', true);
  // No Deepgram voice: the device's own voice says it (F129). (This
  // machine has none: it fails gracefully, and the reply is done.)
  await http('POST', '/api/voice/settings', { ttsVoice: '' }, token);
  await ui('voice-recheck');
  await sleep(800);
  await ui('mic-tap', kA);
  await until('voice mode', async () => (await V())?.rec?.key === kA, 5000);
  const dev0 = (await V()).deviceSays, said0 = (await V()).said;
  await ui('chat-event', JSON.stringify({ type: 'voice-say', id: 'vs3', text: 'It is Sunday. Rest well.' }));
  p = await until('the device voice takes the reply', async () => (await V())?.deviceSays === dev0 + 1 && (await V())?.said === said0 + 1, 20000);
  if (p) check('no Deepgram voice: the device voice takes the reply (and is done)', true);
  await ui('mic-tap', kA);
  await until('off', async () => !(await V())?.rec, 3000);
  // The chat ends: a tap says so.
  await http('POST', `/api/ui-sessions/${vB}/close`, {}, token);
  for (const id of [vA]) await http('POST', `/api/ui-sessions/${id}/close`, {}, token);
}


// ─── 2.16f Live frames in the log ───
// A stored click test (show_embed html) injected as a done tool entry: its
// own off-screen view in the box, input through the window, a popup to the
// opener, release and a fresh return, the cap, pane close.
{
  const fc = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Frames' }, token)).body?.id;
  await ui('select-project', 'demo');
  p = await until('the frames chat has a pane', (q) => pane(q, fc), 30000);
  await ui('focus-pane', pane(p, fc).id);
  const DATA = process.env.CRUNDI_TEST_DATA || `/tmp/crundi-devtest-${new URL(SERVER).port}/data`;
  const mf = join(DATA, 'dev-mcp', `${fc}.json`);
  await until('its MCP config', () => existsSync(mf), 20000);
  const KEY = JSON.parse(readFileSync(mf, 'utf8')).mcpServers.crundi.env.CRUNDI_API_KEY;
  const show = async (args) => (await fetch(SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY }, body: JSON.stringify({ tool: 'show_embed', args: { sessionId: fc, ...args } }) })).json();
  let n = 0;
  const put = (res) => ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'fr' + (++n), kind: 'tool', name: 'mcp__crundi__show_embed', toolUseId: 'tf' + n, input: {}, status: 'done', isError: false, result: JSON.stringify(res) } }));
  const html = '<!doctype html><html><head><title>Click test</title><style>html,body{margin:0;height:100%;background:#1e1e2e;color:#fff;font-family:sans-serif}'
    + 'button{display:block;width:100%;height:60px;font-size:20px;margin:0}input{display:block;width:90%;height:60px;margin:0;box-sizing:border-box}a{display:block;height:40px;color:#fff}p{margin:0;height:36px;user-select:none}</style></head><body>'
    + '<button onclick="window.n=(window.n||0)+1;document.title=\'clicked \'+window.n;document.body.style.background=\'#16a34a\'">Click me</button>'
    + '<input placeholder="type here"><a target="_blank" href="https://example.com/">open example</a><p ondblclick="document.title=\'double \'+event.detail">double click here</p></body></html>';
  const r = await show({ html, title: 'Click test', height: 200 });
  check('show_embed stores the click test', r.ok && r.items?.[0]?.name, r);
  const fkey = `${fc}|stored:${r.items?.[0]?.name}`;
  await put(r);
  const fr = async () => ((await props()).frames || []).find((f) => f.key === fkey);
  p = await until('its own view paints in the box', async () => (await fr())?.painted, 20000);
  let f = await fr();
  const fp = pane(await props(), fc);
  check('a live frame: the box less its edge (the pane\'s width, 198 high), painted', f && f.live && f.h === 198 && f.w > 200 && f.w <= 638, { f, pane: fp && fp.w });
  await snap('2.16f-live');
  // A click through the window: the page counts and turns green.
  await sleep(600);
  f = await fr();
  await mouse('click', f.x + 100, f.y + 30);
  p = await until('clicked', async () => (await fr())?.title === 'clicked 1', 8000);
  if (p) check('a click on the page\'s button: its title "clicked 1"', true);
  await sleep(500);
  const img = readPng(Buffer.from((await d.send({ cmd: 'snapshot' })).data, 'base64'));
  const px = img.at(Math.round(f.x + f.w - 10), Math.round(f.y + 180));
  check('…and the page turned green (#16a34a)', Math.abs(px[0] - 0x16) < 30 && Math.abs(px[1] - 0xa3) < 30 && Math.abs(px[2] - 0x4a) < 30, px);
  await snap('2.16f-clicked');
  // Typing in the page: the frame has the keyboard, the composer none.
  f = await fr();
  await mouse('click', f.x + 100, f.y + 90);
  await until('focused', async (q) => (await props()).frameFocused === fkey, 3000);
  await key('abc');
  check('typing in the page: the frame has the keyboard, the composer stays empty', (await props()).frameFocused === fkey && pane(await props(), fc).draft === '', { focused: (await props()).frameFocused, draft: pane(await props(), fc).draft });
  await key('Escape');
  check('Esc is the page\'s (the frame keeps the keyboard)', (await props()).frameFocused === fkey);
  f = await fr();
  await mouse('click', f.x + 700, f.y - 30);
  p = await until('a click elsewhere takes the keyboard back', async () => (await props()).frameFocused === null, 3000);
  if (p) check('a click outside the frame takes the keyboard back', true);
  // A target=_blank link: no window; the system browser (recorded).
  await sleep(400);
  f = await fr();
  await mouse('click', f.x + 40, f.y + 140);
  p = await until('opened', async () => (await props()).frameOpened === 'https://example.com/', 6000);
  if (p) check('a new-window link goes to the system browser', (await fr())?.live === true);
  // A double click reaches the page (its click count, F133).
  f = await fr();
  await mouse('dblclick', f.x + 60, f.y + 175);
  p = await until('double', async () => /^double 2$/.test((await fr())?.title || ''), 5000);
  if (p) check('a double click reaches the page (dblclick, detail 2)', true);
  // Scrolled away (replies under it): hidden at once, released after the
  // unload grace (CRUNDI_BROWSER_UNLOAD_SECS, 3 s here; once a 4500 px
  // keep-alive); back: a fresh page.
  for (let i = 0; i < 14; i++) await ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'long' + i, kind: 'assistant-text', text: Array(12).fill('A long reply line to push the frame off the log.').join('\n\n') } }));
  p = await until('hidden', async () => { const x = await fr(); return x && !x.onScreen && (x.paused || !x.live); }, 3000);
  if (p) check('the frame scrolled off the log: hidden at once', true);
  p = await until('released', async () => !(await fr())?.live, 10000);
  if (p) check('…its view released after the unload grace', (await props()).framesLive === 0);
  // (Real X wheel notches over the log, as 2.16's F96 check: xdotool.)
  {
    const { execFileSync } = await import('node:child_process');
    const env = { ...process.env, DISPLAY: process.env.DISPLAY || ':99' };
    let ids = [];
    try { ids = execFileSync('xdotool', ['search', '--pid', String(app.pid)], { env }).toString().trim().split('\n').filter(Boolean); } catch { ids = []; }
    const win = ids.map((id) => { try { const g = execFileSync('xdotool', ['getwindowgeometry', id], { env }).toString(); const m = g.match(/Geometry: (\d+)x(\d+)/); return { id, area: m ? +m[1] * +m[2] : 0 }; } catch { return { id, area: 0 }; } }).sort((x, y) => y.area - x.area)[0]?.id;
    const q = pane(await props(), fc);
    if (win) {
      execFileSync('xdotool', ['mousemove', '--window', win, String(Math.round(WB.x + q.x + q.w / 2)), String(Math.round(WB.y + q.y + q.h / 2))], { env });
      for (let i = 0; i < 200 && !(await fr())?.live; i++) { execFileSync('xdotool', ['click', '4'], { env }); await sleep(60); }
      // (Then on until the whole box is in the log's view.)
      for (let i = 0; i < 30; i++) { const x = await fr(); if (x && x.y > WB.y + q.y + 40) break; execFileSync('xdotool', ['click', '4'], { env }); await sleep(80); }
    }
    await parkPointer();
  }
  p = await until('back', async () => (await fr())?.painted, 15000);
  check('scrolled back: a new view, the page fresh (its own title again)', p && (await fr()).title === 'Click test', await fr());
  // The wheel over a frame (a chat of its own, the frames last on the log,
  // which follows the bottom): a page that can still scroll takes it; one
  // that cannot (or at its end) passes it on to the log (F134).
  {
    const { execFileSync } = await import('node:child_process');
    const env = { ...process.env, DISPLAY: process.env.DISPLAY || ':99' };
    let ids = [];
    try { ids = execFileSync('xdotool', ['search', '--pid', String(app.pid)], { env }).toString().trim().split('\n').filter(Boolean); } catch { ids = []; }
    const win = ids.map((id) => { try { const g = execFileSync('xdotool', ['getwindowgeometry', id], { env }).toString(); const m = g.match(/Geometry: (\d+)x(\d+)/); return { id, area: m ? +m[1] * +m[2] : 0 }; } catch { return { id, area: 0 }; } }).sort((x, y) => y.area - x.area)[0]?.id;
    const fc4 = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Frames 4' }, token)).body?.id;
    p = await until('its pane', (q) => pane(q, fc4), 30000);
    await ui('focus-pane', pane(p, fc4).id);
    const mf4 = join(DATA, 'dev-mcp', `${fc4}.json`);
    await until('its MCP config', () => existsSync(mf4), 20000);
    const KEY4 = JSON.parse(readFileSync(mf4, 'utf8')).mcpServers.crundi.env.CRUNDI_API_KEY;
    const store = async (html) => (await fetch(SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY4 }, body: JSON.stringify({ tool: 'show_embed', args: { sessionId: fc4, html, title: 'Wheel', height: 200 } }) })).json();
    for (let i = 0; i < 6; i++) await ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'w-long' + i, kind: 'assistant-text', text: Array(10).fill('Text above the frames, for the log to scroll.').join('\n\n') } }));
    const tall = await store('<title>tall</title><body style="margin:0;background:#225"><div style="height:2000px">tall page</div></body>');
    await ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'w1', kind: 'tool', name: 'mcp__crundi__show_embed', toolUseId: 'tw1', input: {}, status: 'done', isError: false, result: JSON.stringify(tall) } }));
    const flat = await store('<title>flat</title><body style="margin:0;background:#522">flat page</body>');
    await ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'w2', kind: 'tool', name: 'mcp__crundi__show_embed', toolUseId: 'tw2', input: {}, status: 'done', isError: false, result: JSON.stringify(flat) } }));
    const fw = async (r) => ((await props()).frames || []).find((x) => x.key === `${fc4}|stored:${r.items[0].name}`);
    await until('both frames painted, their scroll read', async () => (await fw(tall))?.scroll && (await fw(flat))?.scroll, 20000);
    await ui('log-watch', 'live:' + fc4);
    await sleep(1200);
    const over = async (r) => { const x = await fw(r); execFileSync('xdotool', ['mousemove', '--window', win, String(Math.round(x.x + 60)), String(Math.round(x.y + 100))], { env }); };
    if (win) {
      // The tall page takes the wheel down while it can (the log stays).
      await over(tall);
      const l0 = (await props()).log.dist;
      for (let i = 0; i < 3; i++) { execFileSync('xdotool', ['click', '5'], { env }); await sleep(200); }
      await sleep(800);
      const t = await fw(tall);
      check('a page that can scroll takes the wheel (it scrolls, the log does not)', t.scroll[0] > 0 && Math.abs((await props()).log.dist - l0) < 2, { scroll: t.scroll, l0, l1: (await props()).log.dist });
      // The flat page cannot: the wheel up over it scrolls the log.
      await over(flat);
      const d0 = (await props()).log.dist;
      for (let i = 0; i < 3; i++) { execFileSync('xdotool', ['click', '4'], { env }); await sleep(200); }
      await sleep(800);
      const d1 = (await props()).log.dist;
      check('a wheel over a page that does not scroll passes on to the log (F134)', d1 > d0 + 50, { d0, d1 });
    }
    await parkPointer();
    await http('POST', `/api/ui-sessions/${fc4}/close`, {}, token);
  }
  // Eight frames at once (a chat of their own): never more than 6 live views.
  {
    const fc3 = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Frames 3' }, token)).body?.id;
    p = await until('its pane', (q) => pane(q, fc3), 30000);
    await ui('focus-pane', pane(p, fc3).id);
    const mf3 = join(DATA, 'dev-mcp', `${fc3}.json`);
    await until('its MCP config', () => existsSync(mf3), 20000);
    const KEY3 = JSON.parse(readFileSync(mf3, 'utf8')).mcpServers.crundi.env.CRUNDI_API_KEY;
    for (let i = 0; i < 8; i++) {
      const r3 = await (await fetch(SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY3 }, body: JSON.stringify({ tool: 'show_embed', args: { sessionId: fc3, html: `<title>f${i}</title><body style="margin:0;background:#334">${i}</body>`, title: 'Small ' + i, height: 40 } }) })).json();
      await ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'sm' + i, kind: 'tool', name: 'mcp__crundi__show_embed', toolUseId: 'tsm' + i, input: {}, status: 'done', isError: false, result: JSON.stringify(r3) } }));
    }
    let most = 0;
    for (let i = 0; i < 24; i++) { most = Math.max(most, (await props()).framesLive); await sleep(250); }
    check('eight frames on the log: at most 6 live views (the cap)', most === 6, most);
    await snap('2.16f-cap');
    await http('POST', `/api/ui-sessions/${fc3}/close`, {}, token);
  }
  // A page from an address (not stored), in a chat of its own: it loads
  // (F132). The test server's own page, over http on this machine: the same
  // path as an https page.
  {
    const fc2 = (await http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Frames 2' }, token)).body?.id;
    p = await until('its pane', (q) => pane(q, fc2), 30000);
    await ui('focus-pane', pane(p, fc2).id);
    await ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'pg1', kind: 'tool', name: 'mcp__crundi__show_embed', toolUseId: 'tpg1', input: {}, status: 'done', isError: false, result: JSON.stringify({ ok: true, items: [{ kind: 'embed', src: SERVER + '/', title: 'The server' }] }) } }));
    const pk = `${fc2}|${SERVER}/`;
    const pf = async () => ((await props()).frames || []).find((x) => x.key === pk);
    p = await until('the page loads', async () => { const x = await pf(); return x?.painted && x.title && x.title !== 'about:blank'; }, 20000);
    check('a frame from an address loads its page (painted, its title)', !!p, await pf());
    await snap('2.16f-address');
    await http('POST', `/api/ui-sessions/${fc2}/close`, {}, token);
  }
  // The chat closed: no view left.
  await http('POST', `/api/ui-sessions/${fc}/close`, {}, token);
  p = await until('no views', async () => (await props()).framesLive === 0, 10000);
  if (p) check('the chat closed: no live frame views', true);
}

d.close();
app.kill();
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
