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
//   native-client/tools/test-server.sh 4811 --bare
//   CRUNDI_TEST_SERVER=http://localhost:4811 NATIVE_BIN=… DISPLAY=… node native-client/test/phase2.mjs
//
// Pictures go to DRIVE_OUT (default: the temp dir).

import net from 'node:net';
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, openSync, readFileSync, readdirSync, writeFileSync, existsSync } from 'node:fs';
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
  return spawn(BIN, [], { env: { ...process.env, CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: configDir, CRUNDI_URL_A: 'about:blank', CRUNDI_URL_B: 'about:blank', CRUNDI_TEST_MIC_WAV: MIC_WAV }, stdio: ['ignore', log, log] });
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
    + 'button{display:block;width:100%;height:60px;font-size:20px;margin:0}input{display:block;width:90%;height:60px;margin:0;box-sizing:border-box}a{display:block;height:76px;color:#fff}</style></head><body>'
    + '<button onclick="window.n=(window.n||0)+1;document.title=\'clicked \'+window.n;document.body.style.background=\'#16a34a\'">Click me</button>'
    + '<input placeholder="type here"><a target="_blank" href="https://example.com/">open example</a></body></html>';
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
  // Scrolled away (replies under it): released; back: a fresh page.
  for (let i = 0; i < 14; i++) await ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'long' + i, kind: 'assistant-text', text: Array(12).fill('A long reply line to push the frame off the log.').join('\n\n') } }));
  p = await until('released', async () => !(await fr())?.live, 10000);
  if (p) check('the frame scrolled off the log: its view released', (await props()).framesLive === 0);
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
    }
    await parkPointer();
  }
  p = await until('back', async () => (await fr())?.painted, 15000);
  check('scrolled back: a new view, the page fresh (its own title again)', p && (await fr()).title === 'Click test', await fr());
  // Eight frames at once: never more than 6 live views.
  const small = [];
  for (let i = 0; i < 8; i++) small.push(await show({ html: `<title>f${i}</title><body style="margin:0;background:#334">${i}</body>`, title: 'Small ' + i, height: 40 }));
  for (const s of small) await put(s);
  let most = 0;
  for (let i = 0; i < 20; i++) { most = Math.max(most, (await props()).framesLive); await sleep(250); }
  check('eight frames on the log: at most 6 live views (the cap)', most > 0 && most <= 6, most);
  await snap('2.16f-cap');
  // The chat closed: no view left.
  await http('POST', `/api/ui-sessions/${fc}/close`, {}, token);
  p = await until('no views', async () => (await props()).framesLive === 0, 10000);
  if (p) check('the chat closed: no live frame views', true);
}


d.close();
app.kill();
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
