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


d.close();
app.kill();
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
