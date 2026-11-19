#!/usr/bin/env node
// The frame budget on the owner's PC (ARCHITECTURE.md, "The frame budget"):
// the release client, run with the owner's own config folder as it is (their
// sign-in, their server, their open project), one window, closed by its PID.
//
//   CRUNDI_TEST_PORT=4881 node native-v2/tools/measure-pc.mjs [binary] [out dir]
//   MEASURE_STREAM=0  skip the streaming scenario (it creates a chat in the
//                     open project and sends it one message)
//
// Scenarios (frames from ZED_MEASUREMENTS=1, tools/frames.mjs):
//   idle-open     nothing changes after the window settles: 0 frames
//   scroll-panes  a person's mouse wheel swept over every pane (a grid over
//                 the window, one wheel event per 7 ms, up and down)
//   stream        a new chat (the fifth pane) streaming a long reply
//   stream+scroll the wheel sweep while that chat streams
//   idle-end      after it all: 0 frames
// Each: frames, p50, p95, max, ≤7 ms share, frames over 7 and over 16 ms.
//   MEASURE_STREAM=reuse  as 0 (no new chat)
//   PROFILE=<gdb script>  then a stack sampler on the UI thread while the wheel
//                         sweeps (perf needs perf_event_paranoid ≤ 2; where it
//                         is not, gdb samples: start the binary through a
//                         wrapper that allows the tracer, PR_SET_PTRACER, as
//                         ptrace_scope=1 forbids attaching otherwise). See
//                         native-v2/perf/README.md.
import { spawn } from 'node:child_process';
import { createWriteStream, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { driver, sleep } from '../test/lib.mjs';
import { FrameMeter } from './frames.mjs';

const BIN = process.argv[2] || join(homedir(), '.cache/crundi-native-target/release/crundi-v2');
const OUT = process.argv[3] || '/tmp/crundi-v2-measure';
const PORT = Number(process.env.CRUNDI_TEST_PORT || 4881);
mkdirSync(OUT, { recursive: true });
const log = createWriteStream(join(OUT, 'client.log'));
const t0 = Date.now();
const env = { ...process.env, CRUNDI_TEST_PORT: String(PORT), ZED_MEASUREMENTS: '1' };
delete env.CRUNDI_CONFIG_DIR; // the owner's own config, as it is
const app = spawn(BIN, [], { env, stdio: ['ignore', 'pipe', 'pipe'] });
console.log(`client pid ${app.pid}`);
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
  for (let i = 0; i < 120; i++) { s = await state(); if (s.signedIn && s.online) break; await sleep(250); }
  if (!(s.signedIn && s.online)) throw new Error('not signed in / online: ' + JSON.stringify({ signedIn: s.signedIn, online: s.online, err: s.signinError }));
  await d.send({ cmd: 'resize', w: W, h: H });
  await sleep(4000); // the chats' history loads, the window settles
  s = await state();
  report.setup = { project: s.project, panes: s.panes, chats: (s.chats || []).length, window: `${W}x${H}` };
  console.log('setup', JSON.stringify(report.setup));

  let m = fm.mark(); await sleep(8000); stats('idle-open', m);
  m = fm.mark(); const n1 = await sweep(d, 8000); await sleep(300); stats('scroll-panes', m); report['scroll-panes'].wheelEvents = n1;
  // The focused chat's log scrolled (as the slice suite), up then down, 7 ms apart.
  const scrollChat = async (ms) => { const end = Date.now() + ms; let i = 0; while (Date.now() < end) { await d.send({ cmd: 'ui', name: 'log-scroll', args: [(i % 60) < 30 ? '-50' : '50'] }); i++; await sleep(7); } return i; };
  report.setup.focused = s.chat; report.setup.focusedRows = (s.entries || []).length;
  m = fm.mark(); await scrollChat(8000); await sleep(300); stats('scroll-chat', m);
  if (process.env.PROFILE) {
    // A stack sampler on the UI thread while the chat scrolls (tools: gdb; the
    // binary built with -C force-frame-pointers=yes; see measure-pc's notes).
    const { spawn: sp } = await import('node:child_process');
    const g = sp('gdb', ['-q', '-p', String(app.pid), '-batch', '-x', process.env.PROFILE], { env: { ...process.env, SAMPLE_OUT: join(OUT, 'samples.txt') }, stdio: ['ignore', 'ignore', 'inherit'] });
    let gone = false; g.on('exit', () => { gone = true; });
    const stop = Date.now() + 240000;
    while (!gone && Date.now() < stop) await sweep(d, 2000); // until the sampler is done
  }

  if (!['0', 'reuse'].includes(process.env.MEASURE_STREAM)) {
    await d.send({ cmd: 'ui', name: 'new-chat' });
    let c = null;
    for (let i = 0; i < 100; i++) { s = await state(); if (s.chat && s.panes?.includes(`live:${s.chat}`) && !(report.setup.panes || []).includes(`live:${s.chat}`)) { c = s.chat; break; } await sleep(300); }
    if (!c) throw new Error('no new chat pane');
    report.setup.streamChat = c; report.setup.panesWhileStreaming = s.panes.length;
    await sleep(1500);
    const b0 = s.batches || 0;
    await d.send({ cmd: 'ui', name: 'send', args: ['Write the numbers from one to three hundred as words, one per line, as a markdown list. Nothing else.'] });
    // Wait for the reply to start streaming.
    for (let i = 0; i < 600; i++) { s = await state(); if (s.entries?.some((e) => e.kind === 'assistant' && e.text?.length > 20)) break; await sleep(200); }
    const st = fm.mark();
    const n2 = await sweep(d, 8000); stats('stream+scroll', st); report['stream+scroll'].wheelEvents = n2;
    const so = fm.mark();
    for (let i = 0; i < 900; i++) { s = await state(); if (s.chatState === 'idle') break; await sleep(200); }
    stats('stream', so);
    report.setup.batches = (s.batches || 0) - b0; report.setup.spliced = s.spliced;
    report.setup.replyChars = (s.entries || []).filter((e) => e.kind === 'assistant').map((e) => e.text.length).reduce((a, b) => a + b, 0);
    await sleep(2000);
    m = fm.mark(); await sleep(8000); stats('idle-end', m);
  }
  d.close();
} catch (e) { console.log('error', String(e)); report.error = String(e); }
app.kill(); // by its PID
await sleep(500);
report.ranSeconds = Math.round((Date.now() - t0) / 1000);
report.allFrames = fm.frames.length;
writeFileSync(join(OUT, 'report.json'), JSON.stringify(report, null, 1));
console.log(`window open ${report.ranSeconds} s (pid ${app.pid}, exited: ${exited})`);
