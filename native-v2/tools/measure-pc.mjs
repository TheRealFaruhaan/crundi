#!/usr/bin/env node
// The frame budget on the owner's PC (ARCHITECTURE.md, "The frame budget"):
// the release client, in one window on the PC's own display, with its OWN
// fresh config folder and a FRESH test server's own account. Never the
// owner's sign-in or their server: a used or rotated refresh token can get
// their login revoked (the owner's rule; F13).
//
//   On the Crundi server: native-gpui/tools/test-server.sh 4884 --bare
//   (reached from the PC through a forward, e.g. localhost:4884 on the PC)
//   On the PC:
//     export CRUNDI_CONFIG_DIR=$(mktemp -d /tmp/crundi-v2-measure-config-XXXXXX)
//     MEASURE_SERVER=http://localhost:4884 CRUNDI_TEST_PORT=4881 \
//       node native-v2/tools/measure-pc.mjs [binary] [out dir]
//
// It refuses to run without CRUNDI_CONFIG_DIR, with the owner's own config
// folder, with a folder that already holds anything, and with a server that
// is not fresh (already set up, or with a Telegram bot). It sets the server
// up (a password, a project "demo"), seeds the chats the measurement needs
// (four, each with a long reply: a 300-line list and mixed markdown, so with
// the fifth that streams there are five panes), signs the client in through
// its test driver, and removes the config folder when it is done.
//
//   MEASURE_SERVER       the fresh test server, as the PC reaches it (required)
//   MEASURE_PROJECT_DIR  the project's folder on that server (default
//                        /tmp/crundi-devtest-<MEASURE_SERVER_PORT>/proj, the
//                        port defaulting to MEASURE_SERVER's)
//   MEASURE_STREAM=0     skip the streaming scenario (no fifth chat)
//   PROFILE=<gdb script> then a stack sampler on the UI thread while the
//                        wheel sweeps (see native-v2/perf/README.md)
//
// Scenarios (frames from ZED_MEASUREMENTS=1, tools/frames.mjs):
//   idle-open     nothing changes after the window settles: 0 frames
//   scroll-panes  a mouse wheel swept over every pane (a grid over the
//                 window, one wheel event per 7 ms, up and down)
//   scroll-chat   the focused chat's log scrolled, 7 ms apart
//   stream+scroll the wheel sweep while a fifth chat streams a long reply
//   stream        the rest of that stream
//   idle-end      after it all: 0 frames
// Each: frames, p50, p95, max, ≤7 ms share, frames over 7 and over 16 ms.
import { spawn } from 'node:child_process';
import { createWriteStream, mkdirSync, writeFileSync, existsSync, readdirSync, rmSync, realpathSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { homedir } from 'node:os';
import { driver, client, sleep } from '../test/lib.mjs';
import { totpNow } from '../../src/auth-password.js';
import { FrameMeter } from './frames.mjs';

const die = (m) => { console.error(m); process.exit(2); };
const BIN = process.argv[2] || join(homedir(), '.cache/crundi-native-target/release/crundi-v2');
const OUT = process.argv[3] || '/tmp/crundi-v2-measure';
const PORT = Number(process.env.CRUNDI_TEST_PORT || 4881);
const SERVER = process.env.MEASURE_SERVER;

// Its own config folder, fresh, never the owner's.
const CONFIG = process.env.CRUNDI_CONFIG_DIR;
if (!CONFIG) die('Set CRUNDI_CONFIG_DIR to a fresh folder of its own (mktemp -d): this harness never runs with the owner\'s config.');
const owners = [join(homedir(), '.config/crundi-native'), join(homedir(), '.config/crundi'), join(homedir(), '.config')].map((p) => resolve(p));
if (owners.includes(resolve(CONFIG)) || (existsSync(CONFIG) && owners.includes(realpathSync(CONFIG)))) die(`Refusing CRUNDI_CONFIG_DIR=${CONFIG}: that is the owner's config.`);
if (existsSync(CONFIG) && readdirSync(CONFIG).length) die(`Refusing CRUNDI_CONFIG_DIR=${CONFIG}: it is not empty (a fresh folder only).`);
if (!SERVER) die('Set MEASURE_SERVER to a fresh test server (native-gpui/tools/test-server.sh <port> --bare on the Crundi server, through a forward).');
mkdirSync(CONFIG, { recursive: true });
mkdirSync(OUT, { recursive: true });
const cleanup = () => { try { rmSync(CONFIG, { recursive: true, force: true }); } catch { /* gone */ } };

// The server: fresh, no bot; its own account and project, the chats seeded.
const api = client(SERVER);
const methods = await api.http('GET', '/api/auth/methods').catch((e) => ({ body: null, text: String(e) }));
if (!methods.body) { cleanup(); die(`No server at ${SERVER}: ${methods.text}`); }
if (methods.body.telegram || methods.body.telegramBotId) { cleanup(); die(`Refusing ${SERVER}: it has a Telegram bot configured.`); }
if (!methods.body.setupRequired) { cleanup(); die(`${SERVER} must be fresh: native-gpui/tools/test-server.sh <port> --bare`); }
const password = 'native-v2-measure-' + Math.random().toString(36).slice(2, 10);
const setup = await api.http('POST', '/api/auth/setup', { method: 'password', password });
if (!setup.body?.token) { cleanup(); die('setup failed: ' + setup.text); }
api.setToken(setup.body.token);
const secret = setup.body.totpSecret;
const sport = process.env.MEASURE_SERVER_PORT || new URL(SERVER).port;
const projDir = process.env.MEASURE_PROJECT_DIR || `/tmp/crundi-devtest-${sport}/proj`;
const p = await api.http('POST', '/api/projects', { alias: 'demo', name: 'Demo', path: projDir, create: true });
if (p.status >= 300) { cleanup(); die('no project: ' + p.text); }
const SEEDS = [
  'Write the numbers from one to three hundred as words, one per line, as a markdown list. Nothing else.',
  'Explain how a hash map works, in about 1500 words of markdown: headings, paragraphs, a bulleted list, a numbered list, a table and two code blocks in Rust. Do not use any tools.',
  'Write a markdown list of the 120 largest cities in the world with one sentence about each. Do not use any tools.',
  'Write a long markdown article (about 2000 words) on the history of the printing press, with sections, quotes and lists. Do not use any tools.',
];
const seeded = [];
for (const [i, text] of SEEDS.entries()) {
  const c = await api.http('POST', '/api/ui-sessions/create', { project: 'demo', title: `Measure ${i + 1}` });
  if (!c.body?.id) { cleanup(); die('no chat: ' + c.text); }
  await api.http('POST', `/api/ui-sessions/${c.body.id}/send`, { text });
  seeded.push(c.body.id);
}
console.log('seeded', seeded.join(' '), '- waiting for the replies');
for (let i = 0; i < 900; i++) {
  const l = await api.http('GET', '/api/ui-sessions');
  const mine = (l.body?.sessions || []).filter((s) => seeded.includes(s.id));
  if (mine.length === seeded.length && mine.every((s) => s.state === 'idle')) break;
  await sleep(2000);
}

const log = createWriteStream(join(OUT, 'client.log'));
const t0 = Date.now();
const env = { ...process.env, CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: CONFIG, CRUNDI_OPEN_URL_FILE: join(CONFIG, 'open-url.txt'), ZED_MEASUREMENTS: '1' };
const app = spawn(BIN, [], { env, stdio: ['ignore', 'pipe', 'pipe'] });
console.log(`client pid ${app.pid}, config ${CONFIG}`);
app.stdout.pipe(log);
const fm = new FrameMeter(app.stderr, (l) => log.write(l));
let exited = false; app.on('exit', () => { exited = true; });

const report = {};
const stats = (name, from, until = Date.now()) => {
  const s = fm.since(from, until);
  const r = { ...s, seconds: Math.round((until - from) / 100) / 10 };
  report[name] = r;
  console.log(name.padEnd(14), JSON.stringify(r));
  return r;
};
const W = 1920, H = 1040;
// The wheel over a grid of points across the workbench (every pane is under some).
async function sweep(d, ms) {
  const xs = [320, 620, 920, 1220, 1520, 1800], ys = [260, 520, 800];
  const end = Date.now() + ms;
  let i = 0;
  while (Date.now() < end) {
    const x = xs[Math.floor(i / 40) % xs.length], y = ys[Math.floor(i / 240) % ys.length];
    await d.send({ cmd: 'mouse', kind: 'wheel', x, y, dy: (i % 40) < 20 ? -60 : 60 });
    i++;
    await sleep(7);
  }
  return i;
}

try {
  const d = await driver(PORT, () => !exited);
  const state = async () => (await d.send({ cmd: 'state' })).state || {};
  let s;
  // Signed in as a person would: the server step, then password and code.
  await d.send({ cmd: 'ui', name: 'check-server', args: [SERVER] });
  for (let i = 0; i < 60; i++) { s = await state(); if (s.signinStep === 'Login') break; await sleep(250); }
  await d.send({ cmd: 'ui', name: 'sign-in-password', args: [SERVER, password, totpNow(secret)] });
  for (let i = 0; i < 120; i++) { s = await state(); if (s.signedIn && s.online && s.project === 'demo') break; await sleep(250); }
  if (!(s.signedIn && s.online)) throw new Error('not signed in / online: ' + JSON.stringify({ signedIn: s.signedIn, online: s.online, err: s.signinError }));
  await d.send({ cmd: 'resize', w: W, h: H });
  await sleep(4000); // the chats' history loads, the window settles
  s = await state();
  report.setup = { server: SERVER, project: s.project, panes: s.panes, chats: (s.chats || []).length, window: `${W}x${H}` };
  console.log('setup', JSON.stringify(report.setup));

  let m = fm.mark(); await sleep(8000); stats('idle-open', m);
  let r0 = s.renders || {};
  m = fm.mark(); const n1 = await sweep(d, 8000); await sleep(300); stats('scroll-panes', m); report['scroll-panes'].wheelEvents = n1;
  s = await state(); report['scroll-panes'].renders = Object.fromEntries(Object.entries(s.renders || {}).map(([k, v]) => [k, v - (r0[k] || 0)]));
  // The focused chat's log scrolled (as the slice suite), up then down, 7 ms apart.
  const scrollChat = async (ms) => { const end = Date.now() + ms; let i = 0; while (Date.now() < end) { await d.send({ cmd: 'ui', name: 'log-scroll', args: [(i % 60) < 30 ? '-50' : '50'] }); i++; await sleep(7); } return i; };
  report.setup.focused = s.chat; report.setup.focusedRows = s.rows;
  m = fm.mark(); await scrollChat(8000); await sleep(300); stats('scroll-chat', m);
  if (process.env.PROFILE) {
    // A stack sampler on the UI thread while the wheel sweeps (gdb; the
    // binary built with -C force-frame-pointers=yes; see perf/README.md).
    const g = spawn('gdb', ['-q', '-p', String(app.pid), '-batch', '-x', process.env.PROFILE], { env: { ...process.env, SAMPLE_OUT: join(OUT, 'samples.txt') }, stdio: ['ignore', 'ignore', 'inherit'] });
    let gone = false; g.on('exit', () => { gone = true; });
    const stop = Date.now() + 240000;
    while (!gone && Date.now() < stop) await sweep(d, 2000); // until the sampler is done
  }

  if (process.env.MEASURE_STREAM !== '0') {
    const before = s.panes || [];
    await d.send({ cmd: 'ui', name: 'new-chat' });
    let c = null;
    for (let i = 0; i < 100; i++) { s = await state(); if (s.chat && s.panes?.includes(`live:${s.chat}`) && !before.includes(`live:${s.chat}`)) { c = s.chat; break; } await sleep(300); }
    if (!c) throw new Error('no new chat pane');
    report.setup.streamChat = c; report.setup.panesWhileStreaming = s.panes.length;
    await sleep(1500);
    const b0 = s.batches || 0;
    await d.send({ cmd: 'ui', name: 'send', args: [SEEDS[0]] });
    // Wait for the reply to start streaming.
    for (let i = 0; i < 600; i++) { s = await state(); if (s.entries?.some((e) => e.kind === 'assistant' && e.text?.length > 20)) break; await sleep(200); }
    r0 = s.renders || {};
    const st = fm.mark();
    const n2 = await sweep(d, 8000); stats('stream+scroll', st); report['stream+scroll'].wheelEvents = n2;
    const so = fm.mark();
    for (let i = 0; i < 900; i++) { s = await state(); if (s.chatState === 'idle') break; await sleep(200); }
    stats('stream', so);
    report.stream.renders = Object.fromEntries(Object.entries(s.renders || {}).map(([k, v]) => [k, v - (r0[k] || 0)]));
    report.setup.batches = (s.batches || 0) - b0; report.setup.spliced = s.spliced; report.setup.rows = s.rows;
    report.setup.replyChars = (s.entries || []).filter((e) => e.kind === 'assistant').map((e) => e.text.length).reduce((a, b) => a + b, 0);
    await sleep(2000);
    m = fm.mark(); await sleep(8000); stats('idle-end', m);
  }
  d.close();
} catch (e) { console.log('error', String(e)); report.error = String(e); }
app.kill(); // by its PID
await sleep(500);
cleanup();
report.ranSeconds = Math.round((Date.now() - t0) / 1000);
report.allFrames = fm.frames.length;
writeFileSync(join(OUT, 'report.json'), JSON.stringify(report, null, 1));
console.log(`window open ${report.ranSeconds} s (pid ${app.pid}, exited: ${exited}); config folder removed`);
