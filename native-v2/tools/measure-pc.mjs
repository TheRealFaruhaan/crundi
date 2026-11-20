#!/usr/bin/env node
// The frame budget on the owner's PC (ARCHITECTURE.md, "The frame budget"):
// the release client, in one window on the PC's own display, with its OWN
// fresh config folder and a FRESH test server's own account. Never the
// owner's sign-in or their server: a used or rotated refresh token can get
// their login revoked (the owner's rule; F13).
//
//   A test server whose chats never run the real Claude: native-v2/tools/stub-server.sh 4884
//   (on the PC itself, or on the Crundi server through a forward). Or a bare
//   one there: native-gpui/tools/test-server.sh 4884 --bare (its chats run Claude)
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
//   MEASURE_TIMEOUT=<s>  the whole run's limit (default 2400): past it, it
//                        fails, stops the client and cleans up
//
// However it ends (done, a failure, Ctrl+C or a signal, the client dying,
// the time limit), it stops the client by its PID and removes the config
// folder (which holds the test server's refresh token); a dead client ends
// the run at once instead of driving a port nobody answers (F16).
//   PROFILE_AT=planning  the sampler runs in planning+stream+scroll instead
//                        (after its numbers are taken)
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
// Then the planning area's (MEASURE_PLANNING=0 skips them; the board and the
// map are seeded before the client starts: 4 columns of 20 cards, some with
// subtasks; a map of 133 ideas):
//   scroll-kanban the Kanban tab: the wheel over each column, 7 ms apart
//   pan-mindmap   the Mindmap tab: the wheel over the map, down and sideways
//   idle-planning the Mindmap tab after it: 0 frames
//   planning+stream+scroll  the workbench with a Kanban and a Mindmap pane
//                 beside the chats (seven panes, laid out so every one is in
//                 view: planning's "measure-layout"), the wheel sweep while
//                 the fifth chat streams another long reply (its renders show
//                 the Kanban's and the map's views redrawn under the wheel)
// Then the workspace's (MEASURE_TERMINAL=0 skips them): an Empty Shell pane
// beside the chats (a real shell on the test server):
//   term-stream   the shell prints 3000 lines, one every 2 ms or so (about
//                 8 s of steady output; batched: one feed and one screen
//                 redraw a frame at most)
//   term-stream+scroll  the wheel sweep while it prints them again
//   idle-terminal after it (the terminal not focused: no blink): 0 frames
// Last, the change request's scenario (MEASURE_CR=0 skips it): five panes
// (a chat streaming, a chat, Kanban, Mindmap, the shell):
//   cr-sweep      the wheel sweep over the window while the chat streams
//   cr-scroll     the wheel over the chat, the Kanban and the map in turn
// Each: frames, p50, p95, max, ≤7 ms share, frames over 7 and over 16 ms.
import { spawn } from 'node:child_process';
import { createWriteStream, mkdirSync, writeFileSync, existsSync, readdirSync, rmSync, realpathSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { homedir } from 'node:os';
import { driver, client, sleep } from '../test/lib.mjs';
import { totpNow } from '../../src/auth-password.js';
import { FrameMeter } from './frames.mjs';
import { sweep } from './sweep.mjs';

const die = (m) => { console.error(m); process.exit(2); };
const BIN = process.argv[2] || join(homedir(), '.cache/crundi-native-target/release/crundi-v2');
const OUT = process.argv[3] || '/tmp/crundi-v2-measure';
const PORT = Number(process.env.CRUNDI_TEST_PORT || 4881);
const SERVER = process.env.MEASURE_SERVER;

// Its own config folder, fresh, never the owner's.
const CONFIG = process.env.CRUNDI_CONFIG_DIR;
if (!CONFIG) die('Set CRUNDI_CONFIG_DIR to a fresh folder of its own (mktemp -d): this harness never runs with the owner\'s config.');
// The owner's config folders on each system (crates/core/src/store.rs), and
// the folders that hold them (F22).
const appData = process.env.APPDATA || join(homedir(), 'AppData', 'Roaming');
const owners = [
  join(process.env.XDG_CONFIG_HOME || join(homedir(), '.config'), 'crundi-native'), join(homedir(), '.config/crundi-native'), join(homedir(), '.config/crundi'), join(homedir(), '.config'),
  join(appData, 'Crundi Native'), appData,
  join(homedir(), 'Library/Application Support/Crundi Native'), join(homedir(), 'Library/Application Support'),
  homedir(),
].map((p) => resolve(p));
if (owners.includes(resolve(CONFIG)) || (existsSync(CONFIG) && owners.includes(realpathSync(CONFIG)))) die(`Refusing CRUNDI_CONFIG_DIR=${CONFIG}: that is the owner's config.`);
if (existsSync(CONFIG) && readdirSync(CONFIG).length) die(`Refusing CRUNDI_CONFIG_DIR=${CONFIG}: it is not empty (a fresh folder only).`);
if (!SERVER) die('Set MEASURE_SERVER to a fresh test server (native-gpui/tools/test-server.sh <port> --bare on the Crundi server, through a forward).');
mkdirSync(CONFIG, { recursive: true });
mkdirSync(OUT, { recursive: true });
let app = null;
let exited = false;
const report = {};
const t0 = Date.now();
const cleanup = () => {
  if (app && !exited) { try { process.kill(app.pid); } catch { /* gone */ } }
  try { rmSync(CONFIG, { recursive: true, force: true }); } catch { /* gone */ }
};
/** The one way out: the client stopped, the folder removed, the report written. */
const finish = (code, why) => {
  if (why) { console.log('error', why); report.error = why; }
  cleanup();
  report.ranSeconds = Math.round((Date.now() - t0) / 1000);
  try { writeFileSync(join(OUT, 'report.json'), JSON.stringify(report, null, 1)); } catch { /* no report */ }
  console.log(`done in ${report.ranSeconds} s (client ${app ? 'pid ' + app.pid : 'never started'}); config folder removed`);
  process.exit(code);
};
process.on('exit', cleanup);
for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(sig, () => finish(130, `stopped by ${sig}`));
process.on('uncaughtException', (e) => finish(1, 'uncaught: ' + (e?.stack || e)));
process.on('unhandledRejection', (e) => finish(1, 'unhandled: ' + (e?.stack || e)));
const LIMIT = Number(process.env.MEASURE_TIMEOUT || 2400) * 1000;
setTimeout(() => finish(1, `over the time limit (${LIMIT / 1000} s)`), LIMIT).unref();

// The server: fresh, no bot; its own account and project, the chats seeded.
const api = client(SERVER);
const methods = await api.http('GET', '/api/auth/methods').catch((e) => ({ body: null, text: String(e) }));
if (!methods.body) finish(2, `No server at ${SERVER}: ${methods.text}`);
if (methods.body.telegram || methods.body.telegramBotId) finish(2, `Refusing ${SERVER}: it has a Telegram bot configured.`);
if (!methods.body.setupRequired) finish(2, `${SERVER} must be fresh: native-gpui/tools/test-server.sh <port> --bare`);
const password = 'native-v2-measure-' + Math.random().toString(36).slice(2, 10);
const setup = await api.http('POST', '/api/auth/setup', { method: 'password', password });
if (!setup.body?.token) finish(2, 'setup failed: ' + setup.text);
api.setToken(setup.body.token);
const secret = setup.body.totpSecret;
const sport = process.env.MEASURE_SERVER_PORT || new URL(SERVER).port;
const projDir = process.env.MEASURE_PROJECT_DIR || `/tmp/crundi-devtest-${sport}/proj`;
const p = await api.http('POST', '/api/projects', { alias: 'demo', name: 'Demo', path: projDir, create: true });
if (p.status >= 300) finish(2, 'no project: ' + p.text);
// Each chat a new conversation (the project's earlier ones are not continued).
const SEEDS = [
  'Write the numbers from one to three hundred as words, one per line, as a markdown list. Nothing else.',
  'Explain how a hash map works, in about 1500 words of markdown: headings, paragraphs, a bulleted list, a numbered list, a table and two code blocks in Rust. Do not use any tools.',
  'Write a markdown list of the 120 largest cities in the world with one sentence about each. Do not use any tools.',
  'Write a long markdown article (about 2000 words) on the history of the printing press, with sections, quotes and lists. Do not use any tools.',
];
// The planning area's data (a board and a map big enough to scroll).
if (process.env.MEASURE_PLANNING !== '0') {
  const STS = ['backlog', 'todo', 'in_progress', 'done'];
  for (let i = 0; i < 80; i++) {
    const r = await api.http('POST', '/api/kanban', { project: 'demo', action: 'addTask', title: `Task ${i + 1}: ${['Write the plan', 'Fix the build', 'Review the layout', 'Ship the release'][i % 4]}`, status: STS[i % 4] });
    const tid = r.body?.task?.id || r.body?.id;
    if (tid && i % 3 === 0) for (let j = 0; j < 3; j++) await api.http('POST', '/api/kanban', { project: 'demo', action: 'addTodo', taskId: tid, text: `Step ${j + 1}` });
  }
  const root = (await api.http('POST', '/api/mindmap', { action: 'addNode', text: 'The plan', project: 'demo' })).body?.node?.id;
  for (let i = 0; i < 12; i++) {
    const b = (await api.http('POST', '/api/mindmap', { action: 'addNode', text: `Branch ${i + 1}`, parentId: root })).body?.node?.id;
    for (let j = 0; j < 10; j++) await api.http('POST', '/api/mindmap', { action: 'addNode', text: `Idea ${i + 1}.${j + 1}: a few words that wrap onto a second line`, parentId: b });
  }
  console.log('seeded the board (80 cards) and the map (133 ideas)');
}
const seeded = [];
for (const [i, text] of SEEDS.entries()) {
  const c = await api.http('POST', '/api/ui-sessions/create', { project: 'demo', title: `Measure ${i + 1}`, sessionMode: 'new' });
  if (!c.body?.id) finish(2, 'no chat: ' + c.text);
  await api.http('POST', `/api/ui-sessions/${c.body.id}/send`, { text });
  seeded.push(c.body.id);
}
console.log('seeded', seeded.join(' '), '- waiting for the replies');
// Done when each is idle again (the list's agentState; status says only
// running or exited). An exited one will not answer: the run fails.
const sessionsOf = (b) => (Array.isArray(b) ? b : b?.sessions || []);
for (let i = 0; ; i++) {
  const l = await api.http('GET', '/api/ui-sessions');
  const mine = sessionsOf(l.body).filter((s) => seeded.includes(s.id));
  if (mine.some((s) => s.status === 'exited')) finish(1, 'a seeded chat exited before its reply: ' + JSON.stringify(mine.map((s) => [s.id, s.status, s.agentState])));
  if (mine.length === seeded.length && mine.every((s) => s.agentState === 'idle')) break;
  if (i % 15 === 0) console.log('  replies:', mine.map((s) => s.agentState).join(' '));
  await sleep(2000);
}

const log = createWriteStream(join(OUT, 'client.log'));
const env = { ...process.env, CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: CONFIG, CRUNDI_OPEN_URL_FILE: join(CONFIG, 'open-url.txt'), ZED_MEASUREMENTS: '1' };
app = spawn(BIN, [], { env, stdio: ['ignore', 'pipe', 'pipe'] });
console.log(`client pid ${app.pid}, config ${CONFIG}`);
app.stdout.pipe(log);
const fm = new FrameMeter(app.stderr, (l) => log.write(l));
let done = false;
// The client died: nothing more to drive.
app.on('exit', (code, sig) => { exited = true; if (!done) finish(1, `the client exited (${sig || code}) before the run was done`); });

// MEASURE_PPROF=<scenario> (a frame-journal build, CRUNDI_PPROF_DIR set):
// the in-process sampler runs through that scenario only.
const PPROF = process.env.MEASURE_PPROF, PDIR = process.env.CRUNDI_PPROF_DIR;
const ppStart = (name) => { if (PPROF === name && PDIR) writeFileSync(join(PDIR, 'start'), ''); };
const stats = (name, from, until = Date.now()) => {
  if (PPROF === name && PDIR) writeFileSync(join(PDIR, 'stop'), '');
  const s = fm.since(from, until);
  const r = { ...s, seconds: Math.round((until - from) / 100) / 10 };
  report[name] = r;
  console.log(name.padEnd(14), JSON.stringify(r));
  return r;
};
const W = 1920, H = 1040;
// The wheel over a grid of points across the workbench: tools/sweep.mjs.

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
  const drew = (r0, s) => Object.fromEntries(Object.entries(s.renders || {}).map(([k, v]) => [k, v - (r0[k] || 0)]).filter(([, v]) => v));
  let r0 = s.renders || {};
  ppStart('scroll-panes'); m = fm.mark(); const n1 = await sweep(d, 8000); await sleep(300); stats('scroll-panes', m); report['scroll-panes'].wheelEvents = n1;
  s = await state(); report['scroll-panes'].renders = Object.fromEntries(Object.entries(s.renders || {}).map(([k, v]) => [k, v - (r0[k] || 0)]));
  // The focused chat's log scrolled (as the slice suite), up then down, 7 ms apart.
  const scrollChat = async (ms) => { const end = Date.now() + ms; let i = 0; while (Date.now() < end) { await d.send({ cmd: 'ui', name: 'log-scroll', args: [(i % 60) < 30 ? '-50' : '50'] }); i++; await sleep(7); } return i; };
  report.setup.focused = s.chat; report.setup.focusedRows = s.rows;
  r0 = (await state()).renders || {};
  m = fm.mark(); await scrollChat(8000); await sleep(300); stats('scroll-chat', m);
  s = await state(); report['scroll-chat'].renders = drew(r0, s);
  if (process.env.PROFILE && process.env.PROFILE_AT !== 'planning') {
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
    // F30: a frame here should be a real change; which views drew, and
    // whether the chat's events brought anything (its batches, its head).
    const e0 = await state();
    m = fm.mark(); await sleep(8000); stats('idle-end', m);
    s = await state();
    report['idle-end'].renders = drew(e0.renders || {}, s);
    report['idle-end'].batches = (s.batches || 0) - (e0.batches || 0);
    report['idle-end'].head = { before: e0.head, after: s.head };
  }

  if (process.env.MEASURE_TERMINAL !== '0') {
    // An Empty Shell in a new pane (the empty pane's Terminal chip, then the
    // launcher's Empty Shell), beside the chats.
    const ws = (name, ...args) => d.send({ cmd: 'ui', name: `workspace:${name}`, args: args.map(String) });
    const TS = (s, key) => (s.workspace?.terms || []).find((x) => x.key === key);
    await d.send({ cmd: 'ui', name: 'tab', args: ['workbench'] });
    await ws('chip', 'terminal');
    for (let i = 0; i < 40; i++) { s = await state(); if (s.panes?.some((k) => k.startsWith('pend:'))) break; await sleep(250); }
    const pend = s.panes?.find((k) => k.startsWith('pend:'));
    if (!pend) throw new Error('no launcher for the terminal');
    await ws('launch', pend, 'shell');
    let key = null;
    for (let i = 0; i < 120; i++) { s = await state(); key = s.panes?.find((k) => k.startsWith('term:')); if (key && TS(s, key)?.sized && TS(s, key).screen.some((l) => /\$\s*$/.test(l) || /\$ /.test(l))) break; await sleep(250); }
    if (!key) throw new Error('no terminal pane');
    report.setup.terminal = { key, panes: s.panes.length, size: [TS(s, key)?.cols, TS(s, key)?.rows] };
    await sleep(1500);
    const streamTerm = async (name, scroll) => {
      const n = scroll ? 3001 : 3000;
      await ws('term-send', key, `sleep 1; for ((i=1;i<=${n};i++)); do printf 'line %d ..........................................................\\n' $i; sleep 0.002; done`);
      await sleep(600);
      const s0 = await state();
      const r0 = s0.renders || {}, b0 = TS(s0, key)?.batches || 0;
      ppStart(name);
      const from = fm.mark();
      let wheel = 0;
      const done = (s) => TS(s, key)?.screen.some((l) => l.startsWith(`line ${n} `));
      if (scroll) { while (!done(await state()) && Date.now() - from < 60000) wheel += await sweep(d, 1000); }
      else { for (let i = 0; i < 900; i++) { if (done(await state())) break; await sleep(100); } }
      stats(name, from);
      const s1 = await state();
      report[name].renders = drew(r0, s1);
      report[name].batches = (TS(s1, key)?.batches || 0) - b0;
      // The screen's own time on the UI thread (render + paint), per paint.
      const us = (TS(s1, key)?.screenUs || 0) - (TS(s0, key)?.screenUs || 0), np = (TS(s1, key)?.screenPaints || 0) - (TS(s0, key)?.screenPaints || 0);
      report[name].screen = { paints: np, usPerPaint: np ? Math.round(us / np) : 0 };
      console.log(`  ${name} screen: ${np} paints, ${np ? Math.round(us / np) : 0} µs each (render + paint)`);
      if (scroll) report[name].wheelEvents = wheel;
    };
    await streamTerm('term-stream', false);
    await streamTerm('term-stream+scroll', true);
    if (process.env.PROFILE && process.env.PROFILE_AT === 'term') {
      // PROFILE_AT=term: the sampler on the UI thread while the shell prints
      // and the wheel sweeps (after the numbers above are taken).
      await ws('term-send', key, `for ((i=1;i<=40000;i++)); do printf 'line %d ..........................................................\\n' $i; sleep 0.002; done`);
      await sleep(600);
      const g = spawn('gdb', ['-q', '-p', String(app.pid), '-batch', '-x', process.env.PROFILE], { env: { ...process.env, SAMPLE_OUT: join(OUT, 'samples.txt') }, stdio: ['ignore', 'ignore', 'inherit'] });
      let gone = false; g.on('exit', () => { gone = true; });
      const stop = Date.now() + 240000;
      while (!gone && Date.now() < stop) await sweep(d, 2000);
      await ws('term-tool', key, 'ctrl+c');
    }
    await sleep(2000);
    const i0 = (await state()).renders || {};
    m = fm.mark(); await sleep(6000); stats('idle-terminal', m);
    report['idle-terminal'].renders = drew(i0, await state());
  }

  if (process.env.MEASURE_PLANNING !== '0') {
    const ui = (name, ...args) => d.send({ cmd: 'ui', name, args: args.map(String) });
    const K = (s) => s.planningArea?.kanban;
    const MM = (s) => s.planningArea?.mindmap;
    // The Kanban tab: the wheel over each column in turn, 20 up then 20 down.
    await ui('tab', 'kanban');
    for (let i = 0; i < 60; i++) { s = await state(); if (K(s)?.state === 'ok' && K(s).boxes?.cols?.length >= 4) break; await sleep(250); }
    await sleep(1500);
    s = await state();
    const cols = K(s)?.boxes?.cols || [];
    report.setup.kanban = { columns: cols.length, cards: (K(s)?.columns || []).reduce((a, c) => a + c.cards.length, 0) };
    r0 = s.renders || {};
    m = fm.mark();
    { const end = Date.now() + 8000; let i = 0; while (Date.now() < end && cols.length) { const c = cols[Math.floor(i / 40) % cols.length]; await d.send({ cmd: 'mouse', kind: 'wheel', x: c.x + c.w / 2, y: c.y + Math.min(c.h / 2, 400), dy: (i % 40) < 20 ? -60 : 60 }); i++; await sleep(7); } }
    await sleep(300); stats('scroll-kanban', m);
    s = await state(); report['scroll-kanban'].renders = drew(r0, s);
    // The Mindmap tab: the wheel down and up, then sideways (Shift), over the map.
    await ui('tab', 'mindmap');
    for (let i = 0; i < 60; i++) { s = await state(); if (MM(s)?.state === 'ok' && MM(s).view?.[1] > 0) break; await sleep(250); }
    await ui('mindmap-zoom', '1');
    await sleep(1500);
    s = await state();
    const [cx0, cy0] = MM(s)?.canvas || [400, 200];
    const [vw, vh] = MM(s)?.view || [800, 600];
    report.setup.mindmap = { ideas: (MM(s)?.nodes || []).filter((n) => n.kind === 'node').length, drawn: (MM(s)?.drawn || []).length, view: [vw, vh] };
    r0 = s.renders || {};
    m = fm.mark();
    { const end = Date.now() + 8000; let i = 0; while (Date.now() < end) { const side = Math.floor(i / 80) % 2 === 1; await d.send({ cmd: 'mouse', kind: 'wheel', x: cx0 + vw - 30, y: cy0 + vh / 2, dy: (i % 40) < 20 ? -60 : 60, mods: side ? ['shift'] : [] }); i++; await sleep(7); } }
    await sleep(300); stats('pan-mindmap', m);
    s = await state(); report['pan-mindmap'].renders = drew(r0, s); report['pan-mindmap'].drawnAtEnd = (MM(s)?.drawn || []).length; report['pan-mindmap'].relayouts = MM(s)?.relayouts;
    m = fm.mark(); await sleep(6000); stats('idle-planning', m);
    // The workbench with both panels beside the chats, a chat streaming.
    await ui('tab', 'workbench');
    await ui('open-pane', 'kanban');
    await ui('open-pane', 'mindmap');
    await sleep(1000);
    s = await state();
    // Every pane in view (seven do not fit in one row): the Kanban and the
    // Mindmap each over a chat, the chat that streams over the others.
    const sc = report.setup.streamChat || s.chat;
    if (sc) await ui('measure-layout', `live:${sc}`);
    await sleep(2500);
    s = await state();
    report.setup.planningPanes = s.panes;
    report.setup.planningInView = { focused: s.chat, kanbanCols: (K(s)?.boxes?.cols || []).length, mindmapCanvas: MM(s)?.canvas, mindmapView: MM(s)?.view };
    if (process.env.MEASURE_STREAM !== '0' && s.chat) {
      await d.send({ cmd: 'ui', name: 'send', args: [SEEDS[2]] });
      const n0 = (s.entries || []).length;
      for (let i = 0; i < 600; i++) { s = await state(); if ((s.entries || []).length > n0 + 1 && s.chatState === 'working') break; await sleep(200); }
      report.setup.planningStream = { chat: s.chat, state: s.chatState };
    }
    r0 = s.renders || {};
    ppStart('planning+stream+scroll'); m = fm.mark(); const n3 = await sweep(d, 8000); stats('planning+stream+scroll', m); report['planning+stream+scroll'].wheelEvents = n3;
    s = await state(); report['planning+stream+scroll'].renders = drew(r0, s);
    // The panes the sweep really had (an open terminal comes back into the
    // layout after measure-layout: the workspace's sync places it again).
    report['planning+stream+scroll'].panes = s.panes;
    if (process.env.PROFILE && process.env.PROFILE_AT === 'planning') {
      // The sampler on the UI thread while this scenario's sweep goes on (the
      // seven panes, the chat still streaming): where its time goes.
      const g = spawn('gdb', ['-q', '-p', String(app.pid), '-batch', '-x', process.env.PROFILE], { env: { ...process.env, SAMPLE_OUT: join(OUT, 'samples.txt') }, stdio: ['ignore', 'ignore', 'inherit'] });
      let gone = false; g.on('exit', () => { gone = true; });
      const stop = Date.now() + 240000;
      while (!gone && Date.now() < stop) await sweep(d, 2000);
    }
    // The same, one region at a time (2 s each, while the chat still
    // streams): which pane's frames cost what.
    if (process.env.MEASURE_REGIONS !== '0') {
      s = await state();
      const kb = (K(s)?.boxes?.cols || [])[0];
      const [mx, my] = MM(s)?.canvas || [0, 0];
      const [mw, mh] = MM(s)?.view || [0, 0];
      const regions = { kanban: kb && [kb.x + kb.w / 2, kb.y + Math.min(kb.h / 2, 300)], mindmap: mw > 0 && [mx + mw - 20, my + mh / 2] };
      // MEASURE_TERM_CLEAR=1: the Kanban's region again with the open shell's
      // screen cleared (what an idle terminal's drawn text costs the others).
      const tk = report.setup.terminal?.key;
      if (process.env.MEASURE_TERM_CLEAR === '1' && tk && regions.kanban) { regions['kanban-termclear'] = regions.kanban; regions['kanban-termfull'] = regions.kanban; }
      for (const [name, at] of Object.entries(regions)) {
        if (!at) continue;
        if (name === 'kanban-termclear') { await d.send({ cmd: 'ui', name: 'workspace:term-send', args: [tk, 'clear'] }); await sleep(1500); }
        if (name === 'kanban-termfull') { await d.send({ cmd: 'ui', name: 'workspace:term-send', args: [tk, "for ((i=1;i<=300;i++)); do printf 'line %d ..........................................................\\n' $i; done"] }); await sleep(2500); }
        const r1 = (await state()).renders || {};
        const mm0 = fm.mark();
        const end = Date.now() + 2000; let i = 0;
        while (Date.now() < end) { await d.send({ cmd: 'mouse', kind: 'wheel', x: at[0], y: at[1], dy: (i % 40) < 20 ? -60 : 60 }); i++; await sleep(7); }
        stats(`region:${name}`, mm0);
        report[`region:${name}`].renders = drew(r1, await state());
      }
    }
    for (let i = 0; i < 900; i++) { s = await state(); if (s.chatState !== 'working') break; await sleep(200); }
  }

  // The change request's own scenario (MEASURE_CR=0 skips it; it needs the
  // terminal, the planning panes and the stream): five panes, the Kanban
  // over a chat, the Mindmap over the shell, the streaming chat in the third
  // column. The other seeded chats are closed on the server first (every
  // running chat has a pane). Then, while the chat streams: the wheel sweep
  // over the window (cr-sweep) and the wheel over the chat, the Kanban and
  // the map in turn (cr-scroll).
  const tk = report.setup.terminal?.key, sc = report.setup.streamChat;
  if (process.env.MEASURE_CR !== '0' && tk && sc && process.env.MEASURE_PLANNING !== '0' && process.env.MEASURE_STREAM !== '0') {
    const ui = (name, ...args) => d.send({ cmd: 'ui', name, args: args.map(String) });
    const K = (s) => s.planningArea?.kanban;
    const MM = (s) => s.planningArea?.mindmap;
    const other = seeded[0];
    for (const id of seeded.slice(1)) await api.http('POST', `/api/ui-sessions/${id}/close`, {});
    for (let i = 0; i < 60; i++) { s = await state(); if (!(s.panes || []).some((k) => seeded.slice(1).some((id) => k === `live:${id}`))) break; await sleep(250); }
    await ui('measure-layout', `live:${sc}`, 'cr', `live:${other}`, tk);
    await sleep(2500);
    s = await state();
    report.setup.cr = { panes: s.panes, kanbanCols: (K(s)?.boxes?.cols || []).length, mindmapCanvas: MM(s)?.canvas, mindmapView: MM(s)?.view };
    console.log('cr layout', JSON.stringify(report.setup.cr));
    if ((s.panes || []).length !== 5) throw new Error('the CR layout has ' + (s.panes || []).length + ' panes: ' + JSON.stringify(s.panes));
    // The longest reply (the cities: about 19 s with the stub), sent anew
    // for each of the two so the chat streams through both.
    const stream = async () => {
      for (let i = 0; i < 900; i++) { s = await state(); if (s.chatState !== 'working') break; await sleep(200); }
      const n0 = (s.entries || []).length;
      await d.send({ cmd: 'ui', name: 'send', args: [SEEDS[2]] });
      for (let i = 0; i < 600; i++) { s = await state(); if ((s.entries || []).length > n0 + 1 && s.chatState === 'working') break; await sleep(200); }
    };
    await stream();
    // The chat pane's place: the leaf under the Kanban (its column's lower half).
    const kb = (K(s)?.boxes?.cols || [])[0];
    const [mx, my] = MM(s)?.canvas || [0, 0];
    const [mw, mh] = MM(s)?.view || [0, 0];
    const pts = [[kb ? kb.x + kb.w / 2 : 320, 800], kb && [kb.x + kb.w / 2, kb.y + Math.min(kb.h / 2, 200)], mw > 0 && [mx + mw - 20, my + mh / 2]].filter(Boolean);
    report.setup.cr.points = pts;
    r0 = s.renders || {};
    ppStart('cr-sweep'); m = fm.mark(); const n4 = await sweep(d, 8000); stats('cr-sweep', m); report['cr-sweep'].wheelEvents = n4;
    s = await state(); report['cr-sweep'].renders = drew(r0, s); report['cr-sweep'].streaming = s.chatState;
    await stream();
    r0 = s.renders || {};
    ppStart('cr-scroll'); m = fm.mark();
    { const end = Date.now() + 9000; let i = 0; while (Date.now() < end) { const [x, y] = pts[Math.floor(i / 40) % pts.length]; await d.send({ cmd: 'mouse', kind: 'wheel', x, y, dy: (i % 40) < 20 ? -60 : 60 }); i++; await sleep(7); } report.crWheel = i; }
    stats('cr-scroll', m);
    s = await state(); report['cr-scroll'].renders = drew(r0, s); report['cr-scroll'].streaming = s.chatState;
    for (let i = 0; i < 900; i++) { s = await state(); if (s.chatState !== 'working') break; await sleep(200); }
  }
  d.close();
} catch (e) { done = true; report.allFrames = fm.frames.length; finish(1, String(e)); }
done = true;
report.allFrames = fm.frames.length;
finish(0);
