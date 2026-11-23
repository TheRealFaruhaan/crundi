// The streaming tail (crates/chat/src/log/tail.rs; ARCHITECTURE.md, the
// streaming path): while a reply streams and the log follows its end, the
// reply's last block is drawn by its own view, so a delta that keeps the
// block's height redraws that view alone, not the log. A deterministic
// stream is played into a chat (the log's `chat-event`, as the live link
// brings it), and the suite checks: deltas redraw the tail only; a delta
// that wraps the block redraws the log; the tail's text sits where the list
// puts that block once the stream ends (the gap to the block above, its
// left edge, its width); scrolling up gives the block back to the list;
// the stream's end does too; a markdown list streams with no blank row
// before the item being written (F3), in played events and from the
// server's stub; and nothing draws when idle.
// Pictures go to native-v2/screens/chat-tail/ (SCREENS_DIR elsewhere).
//
//   native-v2/tools/stub-server.sh 4884 &
//   CRUNDI_TEST_SERVER=http://localhost:4884 CRUNDI_TEST_PORT=4889 \
//     xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/chat-tail.mjs
//   kill $(cat /tmp/crundi-devtest-4884/server.pid)

import * as t from './lib.mjs';
import { execFileSync } from 'node:child_process';

const T = await t.begin('chat-tail');
const snap = (f) => T.snap('chat-tail', f);
const ev = (e) => T.ui('chat-event', JSON.stringify(e));
const renders = async () => (await T.state()).renders || {};
const drew = (a, b) => Object.fromEntries(Object.keys(b).map((k) => [k, b[k] - (a[k] || 0)]).filter(([, v]) => v));
const texts = async () => (await T.ui('log-texts')).texts || [];

await T.ui('new-chat');
await T.until('a chat pane', (st) => st.chat && st.panes?.length > 0, 30000);
await T.until('the chat is idle', (st) => st.chatState === 'idle', 30000);

// The pixels too (the texts' rects are the layout's, the misplaced tail was
// not): the screen grabbed (ffmpeg's x11grab, gray), the text lines in the
// list's column found by their bright pixels, down to the list's end; the
// line being written starts one line (22 px) under the one above, not two.
const W = 1440, H = 900;
let ffmpeg = true; try { execFileSync('ffmpeg', ['-version'], { stdio: 'ignore' }); } catch { ffmpeg = false; }
const lines = (x0, y0, y1) => {
  const g = execFileSync('ffmpeg', ['-loglevel', 'error', '-f', 'x11grab', '-video_size', `${W}x${H}`, '-i', process.env.DISPLAY, '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'gray', '-'], { maxBuffer: W * H * 2 });
  const lit = (y) => { for (let x = Math.max(0, x0 - 24); x < Math.min(W, x0 + 160); x++) if (g[y * W + x] > 160) return true; return false; };
  const out = []; let on = false;
  for (let y = Math.max(0, y0); y < Math.min(H, y1); y++) { const l = lit(y); if (l && !on) out.push(y); on = l; }
  return out;
};
// A markdown list played into the empty chat (F3, N5), one item a step: the
// log fills, reaches its foot, and the tail and the "Working…" row take
// their own views there. At each step the item being written sits one line
// under the one before, on screen: no blank row above it (the tail was
// placed at the last layout's room, 0 while "Working…" was in the list,
// and drawn a row low until the next delta).
await ev({ type: 'state', state: 'working' });
let list = 'Counting:\n\n- item 1';
await ev({ type: 'entry', entry: { id: 'l1', kind: 'assistant-text', text: list, streaming: true } });
await t.sleep(300);
const steps = []; let liveSteps = 0;
for (let k = 2; k <= 44; k++) {
  const piece = k % 3 ? `\n- item ${k}` : `\n- ite`;
  list += piece; await ev({ type: 'delta', id: 'l1', text: piece });
  if (k % 3 === 0) { await t.sleep(60); list += `m ${k}`; await ev({ type: 'delta', id: 'l1', text: `m ${k}` }); }
  await t.sleep(250);
  const st = await T.state();
  const ts = await texts();
  const its = ts.filter((x) => /^item \d+$/.test(x.text));
  if (!its.length) continue;
  if (st.log?.tail?.live) liveSteps++;
  if (!ffmpeg || !process.env.DISPLAY) continue;
  const x0 = Math.min(...its.map((x) => x.x));
  const top = Math.max(...its.map((x) => x.y)) - 50;
  const end = st.log?.tail ? st.log.tail.seen[0] - st.log.tail.seen[2] : H;
  const ls = lines(x0, Math.round(top), Math.round(end));
  const step = ls.length >= 2 ? ls[ls.length - 1] - ls[ls.length - 2] : 0;
  steps.push(step);
  if (k === 31) await snap('tail-list-streaming');
  if (step > 40) { console.log('a blank row at item', k, JSON.stringify({ ls, tail: st.log?.tail })); }
}
console.log('played list: steps', JSON.stringify(steps), 'tail live in', liveSteps);
t.check('the played list reached the foot, its tail drawn by its own view', liveSteps >= 5, liveSteps);
if (ffmpeg && process.env.DISPLAY) t.check('…at every step, the item being written one line under the one above, on screen (no blank row)', steps.length >= 30 && steps.every((x) => x > 0 && x <= 40), steps);
else console.log('(no ffmpeg or display: the on-screen check skipped)');
await ev({ type: 'entry', entry: { id: 'l1', kind: 'assistant-text', text: list } });
await ev({ type: 'state', state: 'idle' });
await t.sleep(400);

// A list through the server (F3): the stub's 300 numbers in words, a
// list sent in 24-character pieces as Claude streams, sampled mid-stream.
// The item being written sits under the one before at the list's gap,
// never under a blank row (the tail placed at the last layout's room while
// the "Working…" row moved out of the list: a row low). A lone "-" (a piece
// ending at the next item's dash) is the web's own paragraph after the list
// (claude-chat.js md()), its 8 px.
await T.ui('composer-text', 'Reply with three hundred numbers in words.');
await T.ui('composer-send');
await T.until('the reply streams (working)', (st) => st.composer?.state === 'working', 30000);
const tailRows = (ts) => {
  const rows = [...new Set(ts.map((x) => x.row))].sort((a, b) => a - b).slice(-4);
  return rows.map((r) => { const L = ts.filter((x) => x.row === r); return { top: Math.min(...L.map((x) => x.y)), bot: Math.max(...L.map((x) => x.y + x.h)), text: L.map((x) => x.text).join('') }; });
};
const low = []; let grabs = 0;
const wrong = []; let samples = 0, shot = false;
for (let k = 0; k < 150; k++) {
  const st = await T.state();
  if (st.composer?.state !== 'working') break;
  const bx = tailRows(await texts());
  if (st.log?.tail?.live && bx.length === 4) {
    samples++;
    bx.slice(1).forEach((b, i) => { const g = Math.round(b.top - bx[i].bot); if (g !== 2 && !(b.text === '-' && g === 8)) wrong.push({ above: bx[i].text, row: b.text, gap: g, tail: st.log.tail }); });
    if (ffmpeg && process.env.DISPLAY && k % 2 === 0) {
      const x0 = Math.min(...(await texts()).map((x) => x.x));
      const ls = lines(x0, Math.round(bx[0].top) - 4, Math.round(st.log.tail.seen[0] - st.log.tail.seen[2]));
      grabs++;
      // (The caret's bar is the accent, not lit; the last line is the one being written.)
      const step = ls.length >= 2 ? ls[ls.length - 1] - ls[ls.length - 2] : 0;
      // (A blank row puts it 44 px under; a lone "-", the web's paragraph
      // after the list, is lit mid-line, 35 px under: allowed.)
      if (ls.length >= 2 && step > 40) low.push({ lines: ls, step, last: bx[3].text, tail: st.log.tail });
    }
    if (!shot && samples > 20) { shot = true; await snap('tail-list-stream-server'); }
  }
  await t.sleep(20);
}
console.log('streamed list: samples', samples, 'wrong', JSON.stringify(wrong.slice(0, 3)));
t.check('the streamed list, sampled while the tail draws it', samples >= 20, samples);
t.check('…no blank row before the item being written', wrong.length === 0, wrong.slice(0, 3));
console.log('streamed list, on screen: grabs', grabs, 'low', JSON.stringify(low.slice(0, 2)));
if (ffmpeg && process.env.DISPLAY) t.check('…nor on screen: the line being written one line under the one above, in every grab', grabs >= 10 && low.length === 0, { grabs, low: low.slice(0, 3) });
else console.log('(no ffmpeg or display: the on-screen check skipped)');
await T.until('…and the reply ends', (st) => st.composer?.state === 'idle', 60000);

// A history taller than the log, then a reply that streams.
const paras = Array.from({ length: 40 }, (_, i) => `Paragraph ${i + 1} of an earlier reply, long enough to fill a line or so of the pane.`).join('\n\n');
await ev({ type: 'entry', entry: { id: 'h1', kind: 'assistant-text', text: paras } });
await ev({ type: 'state', state: 'working' });
let text = 'The reply so far: its first paragraph, done.\n\nAnd the block that streams';
await ev({ type: 'entry', entry: { id: 'a1', kind: 'assistant-text', text, streaming: true } });
await t.sleep(600);
// One more delta: the tail takes the block (the list's layout found it at the foot).
text += ' on'; await ev({ type: 'delta', id: 'a1', text: ' on' });
await t.sleep(400);
let s = await T.state();
t.check('the log follows the end', s.logFollowing, { following: s.logFollowing });

// Deltas that keep the block on its line: the tail redraws, the log does not.
let r0 = await renders();
for (const w of [' and', ' on', ' a', ' bit']) { text += w; await ev({ type: 'delta', id: 'a1', text: w }); await t.sleep(120); }
await t.sleep(300);
let d = drew(r0, await renders());
console.log('views drawn by four short deltas:', JSON.stringify(d));
t.check('short deltas redraw the tail', (d['log-tail'] || 0) >= 3, d);
t.check('…and not the log', (d.log || 0) <= 1, d);
// The turn's activity row (F159): "Working…" under the reply, its own view at the foot.
s = await T.state();
t.check('while working: "Working…" pinned at the log\'s end, drawn by its own view', s.log?.activity?.label === 'Working…' && s.log.activity.own === true, s.log?.activity);
t.check('…and its spinner redraws it, not the log', (d['log-activity'] || 0) >= 1, d);
await snap('tail-streaming');

// The tail's text where the list will put it: the block above and the tail.
const pair = (ts) => {
  const rows = [...new Set(ts.map((x) => x.row))].sort((a, b) => a - b);
  const last = rows[rows.length - 1], prev = rows[rows.length - 2];
  const L = ts.filter((x) => x.row === last), P = ts.filter((x) => x.row === prev);
  if (!L.length || !P.length) return null;
  const top = Math.min(...L.map((x) => x.y)), x = Math.min(...L.map((x) => x.x)), w = Math.max(...L.map((x) => x.w));
  const pb = Math.max(...P.map((x) => x.y + x.h));
  return { last, prev, gap: Math.round((top - pb) * 100) / 100, x, w, text: L.map((x) => x.text).join('') };
};
const during = pair(await texts());
console.log('tail state:', JSON.stringify((await T.state()).log?.tail));
console.log('while streaming:', JSON.stringify(during));
t.check('the tail reports its text (selection and links reach it)', during && /a bit$/.test(during.text), during);

// A long delta wraps the block: its height changes, the log redraws.
r0 = await renders();
const long = ' and then a long run of words that will surely not fit on the one line the block had so far, so it wraps onto a second line of the pane, which is wide: well over a thousand pixels of text at the chat size, so this goes on for quite a while more, and more, and more.';
text += long; await ev({ type: 'delta', id: 'a1', text: long });
await t.sleep(400);
d = drew(r0, await renders());
console.log('views drawn by a wrapping delta:', JSON.stringify(d));
t.check('a delta that wraps the block redraws the log', (d.log || 0) >= 1, d);
const wrapped = pair(await texts());
t.check('the wrapped tail keeps its place under the block above', wrapped && during && Math.abs(wrapped.gap - during.gap) < 0.6, { during, wrapped });
await snap('tail-wrapped');

// Scrolled up: the list draws the block again (the tail steps aside).
await T.ui('log-scroll', '-300');
await t.sleep(300);
s = (await T.state()).log || {};
t.check('scrolled up: "Go to the newest message" shows', s.jump?.shown, s.jump);
r0 = await renders();
text += ' More.'; await ev({ type: 'delta', id: 'a1', text: ' More.' });
await t.sleep(300);
d = drew(r0, await renders());
// (The block is below what the list made, "Go to the newest message"
// shows: nothing on screen changed, so nothing redraws.)
t.check('scrolled up, a delta below the screen redraws neither the tail nor the log', !(d['log-tail'] > 0) && !(d.log > 0), d);

await T.ui('log-jump');
await T.until('back at the end, following', (st) => st.logFollowing, 5000);
await t.sleep(300);

// The stream ends: the list draws the block, where the tail had it.
await ev({ type: 'entry', entry: { id: 'a1', kind: 'assistant-text', text } });
await ev({ type: 'state', state: 'idle' });
await t.sleep(600);
const after = pair(await texts());
console.log('after the stream:', JSON.stringify(after));
t.check('the block is the same text once the list draws it', after && during && after.text === wrapped.text + ' More.', { wrapped, after });
t.check('…at the same gap under the block above', after && wrapped && Math.abs(after.gap - wrapped.gap) < 0.6, { wrapped, after });
t.check('…at the same left edge and width', after && wrapped && Math.abs(after.x - wrapped.x) < 0.6 && Math.abs(after.w - wrapped.w) < 1, { wrapped, after });
await snap('tail-done');
t.check('idle: no activity row', !(await T.state()).log?.activity, (await T.state()).log?.activity);

// Idle: nothing draws (the caret's blink stopped with the stream).
await t.sleep(1500);
const m = T.frames.mark();
await t.sleep(3000);
const idle = T.frames.since(m);
t.check('idle: no frames for 3 s', idle.frames === 0, idle);

await T.end();
