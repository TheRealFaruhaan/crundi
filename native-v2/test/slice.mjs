// The vertical slice (ARCHITECTURE.md; PLAN.md "F"): the shell, the
// workbench and a chat pane with a live chat streaming over the real
// WebSocket, against a fresh test server, with no forced redraws:
//
//  1. sign in, the project opens, the link is online;
//  2. a new chat gets its pane; a message to it streams Claude's reply into
//     the log with no other action (real server events), in batches;
//  3. the frames while it streams and while the log scrolls (p50, p95,
//     share at most 7 ms; the 7 ms budget is checked with CRUNDI_FRAME_BUDGET=1,
//     on the owner's PC with a release build);
//  4. idle, nothing animating: no frames at all;
//  5. a silently dead connection (the proxy stops forwarding without
//     closing) is dropped and reconnected within 45 s, and the chat's next
//     reply shows again with no other action.
//
// How to run it: test/lib.mjs (on the server, under Xvfb, never on the owner's PC).

import * as t from './lib.mjs';
import { join } from 'node:path';

const T = await t.begin('slice', { proxy: true });
const budget = process.env.CRUNDI_FRAME_BUDGET === '1';

// 1b. A fresh project's workbench: the web's "New terminal" launcher pane
// (F18), not an empty leaf.
const sl = await T.until('a fresh project shows the New terminal launcher', (s) => s.panes?.length === 1 && s.panes[0].startsWith('pend:'), 10000);
await T.snap('workbench', 'launcher');

// 2. A new chat, its pane, a streamed reply.
await T.ui('new-chat');
const s0 = await T.until('the new chat has its pane, focused', (s) => s.chat && s.panes?.includes(`live:${s.chat}`), 30000);
const chat = s0?.chat;
t.check('the new chat took the launcher\'s place', s0 && s0.panes.length === 1, s0?.panes);
await T.snap('chat', 'new-chat-pane');
const b0 = s0?.batches ?? 0;
const streamFrom = T.frames.mark();
await T.ui('send', 'Reply with a markdown list of the numbers one to forty, one per line, written as words. Nothing else.');
const s1 = await T.until('the user message shows', (s) => s.entries?.some((e) => e.kind === 'user'), 15000);
await T.snap('chat', 'user-message-sent');
const rows1 = s1?.rows ?? 0, spliced1 = s1?.spliced ?? 0, batches1 = s1?.batches ?? 0;
// The reply arrives by itself, as Claude streams it. Watched closely: a
// reply seen part-way (some of it, not all yet) streamed live.
let partial = false;
const s2 = await T.until("Claude's reply streams in", (s) => {
  const a = s.entries?.find((e) => e.kind === 'assistant');
  if (a && !/forty/i.test(a.text)) partial = true;
  return a && /forty/i.test(a.text);
}, 180000);
await T.snap('chat', 'reply-arrived');
// (Under Xvfb at a high load a frame takes long, and a fast reply can land
// in one batch: seen part-way, or two batches or more, shows it streamed.)
t.check('the reply came live (seen part-way, or in several batches)', partial || (s2?.batches ?? 0) - b0 >= 2, { partial, b0, b: s2?.batches });
// The reply is one log row per markdown block (F10): forty list items.
t.check('the reply is a row per markdown block', (s2?.rows ?? 0) >= (s2?.entries?.length ?? 0) + 30, s2 && { rows: s2.rows, entries: s2.entries.length });
// F17: what a batch splices is the rows it added and the last few before
// them (the store's own test: store.rs), never the whole log again.
const added = (s2?.rows ?? 0) - rows1, batches = (s2?.batches ?? 0) - batches1;
t.check('rows were spliced, not rebuilt per batch', (s2?.spliced ?? 0) - spliced1 <= added + 3 * Math.max(1, batches), s2 && { spliced: s2.spliced - spliced1, added, batches });
await T.until('the chat is idle again', (s) => s.chatState === 'idle', 120000);
// The head follows: out of WORKING, and the server's title (its list's, live
// from the event stream, no refetch) once the server has named the chat.
await T.until('the head leaves WORKING when the reply ends', (s) => s.head && s.head.state !== 'working', 15000);
await T.until("the head shows the server's title, live", (s) => {
  const listed = (s.chats || []).find((c) => c.id === chat);
  return listed && listed.title && listed.title !== 'Chat' && s.head?.title === listed.title;
}, 60000);
const streaming = T.frames.since(streamFrom);
await T.snap('chat', 'reply-done-idle');
console.log('frames while streaming:', JSON.stringify(streaming));
t.check('frames were drawn while it streamed', streaming.frames > 0, streaming);

// 2b. One selection across a reply's blocks and across messages (F15): a
// drag from one text to another selects everything between, Ctrl+C copies it.
{
  // From the top: the user's message and the reply's first blocks on screen.
  await T.ui('log-scroll', '-100000');
  await t.sleep(400);
  const lt = await T.ui('log-texts');
  const texts = (lt.texts || []).filter((x) => x.w > 4 && x.h > 4 && x.y > 120 && x.y + x.h < 820).sort((a, b) => a.row - b.row || a.part - b.part);
  if (t.check('the log reports the texts it drew', texts.length >= 3, lt)) {
    const a = texts[0], z = texts[texts.length - 1], mid = texts[Math.floor(texts.length / 2)];
    await T.mouse('move', a.x + 1, a.y + a.h / 2); await t.sleep(100);
    await T.mouse('drag', a.x + 1, a.y + a.h / 2, { toX: z.x + z.w - 1, toY: z.y + z.h / 2, steps: 12 });
    await t.sleep(300);
    const sel = await T.ui('log-texts');
    t.check('a drag selects across blocks (a middle block whole, line breaks between)', sel.selection.includes(mid.text) && sel.selection.includes('\n') && sel.selection.length >= mid.text.length + 4, { sel: sel.selection.slice(0, 200), mid: mid.text });
    await T.snap('chat', 'selection-across-blocks');
    await T.key('c', ['ctrl']);
    const cp = await T.ui('log-texts');
    t.check('Ctrl+C copies the selection', cp.clipboard === sel.selection && cp.clipboard.length > 0, { clip: cp.clipboard.slice(0, 120) });
    // Across messages: from the user's bubble into Claude's reply.
    const user = texts.find((x) => x.text.startsWith('Reply with a markdown list'));
    if (user) {
      const into = texts.find((x) => x.row > user.row + 1);
      await T.mouse('drag', user.x + 1, user.y + user.h / 2, { toX: into.x + into.w - 1, toY: into.y + into.h / 2 });
      await t.sleep(300);
      const s5 = await T.ui('log-texts');
      t.check('a drag selects from a message into the next', s5.selection.startsWith('Reply with') && s5.selection.includes(into.text), { sel: s5.selection.slice(0, 200) });
    }
    // A double-click selects a word; a click elsewhere clears the selection.
    await T.mouse('dblclick', mid.x + 2, mid.y + mid.h / 2);
    await t.sleep(200);
    const w = await T.ui('log-texts');
    t.check('a double-click selects a word', w.selection.length > 0 && !/\s/.test(w.selection), { sel: w.selection });
    await T.mouse('click', mid.x + 2, mid.y + mid.h / 2);
    await t.sleep(200);
    t.check('a click clears the selection', (await T.ui('log-texts')).selection === '');
  }
}

// 3. Scrolling the log.
const scrollFrom = T.frames.mark();
for (let i = 0; i < 60; i++) { await T.ui('log-scroll', i % 20 < 10 ? '-40' : '40'); await t.sleep(16); }
await t.sleep(300);
const scroll = T.frames.since(scrollFrom);
await T.snap('chat', 'log-scrolled');
console.log('frames while scrolling:', JSON.stringify(scroll));
t.check('the log scrolls (frames drawn)', scroll.frames > 0, scroll);
// 3b. A person's mouse wheel over the log: it scrolls and redraws by itself
// (no driver redraw), and the picture changes.
const before3 = join(T.OUT, 'wheel-before.png'), after3 = join(T.OUT, 'wheel-after.png');
await T.d.send({ cmd: 'snapshot', path: before3 });
await T.mouse('move', 800, 400); await t.sleep(300);
const r0 = (await T.state()).renders || {};
const wheelFrom = T.frames.mark();
for (let i = 0; i < 30; i++) { await T.mouse('wheel', 800, 400, { dy: 40 }); await t.sleep(16); }
await t.sleep(400);
const wheel = T.frames.since(wheelFrom);
// Only the log redraws (and its layout parents): never the heads, the
// composers, the top bar or the sidebar (F11).
const r1 = (await T.state()).renders || {};
const drew = Object.fromEntries(Object.keys(r1).map((k) => [k, r1[k] - (r0[k] || 0)]));
console.log('views rendered while wheel-scrolling:', JSON.stringify(drew));
t.check('a wheel over the log redraws the log', drew.log > 0, drew);
t.check('a wheel over the log redraws no head, composer, top bar or sidebar', !drew.head && (drew.composer || 0) <= 1 && !drew['top-bar'] && !drew.sidebar, drew);
console.log('frames while wheel-scrolling:', JSON.stringify(wheel));
await T.d.send({ cmd: 'snapshot', path: after3 });
await T.snap('chat', 'log-wheel-scrolled');
t.check('a mouse wheel over the log draws frames by itself', wheel.frames > 0, wheel);
t.check('a mouse wheel over the log moves its content', T.diff(before3, after3, '1100x600+260+120') > 500, { px: T.diff(before3, after3, '1100x600+260+120') });
// Back down to the end: the log follows it again (as the web's stickBottom).
for (let i = 0; i < 60; i++) { await T.mouse('wheel', 800, 400, { dy: -80 }); await t.sleep(16); }
await T.until('back at the end, the log follows it again', (s) => s.logFollowing, 5000);

// 3c. Hover follows the pointer: over "+ Add project", then away.
const away = join(T.OUT, 'hover-away.png'), over = join(T.OUT, 'hover-over.png');
await T.mouse('move', 800, 600); await t.sleep(300);
await T.d.send({ cmd: 'snapshot', path: away });
const hoverFrom = T.frames.mark();
await T.mouse('move', 120, 790); await t.sleep(300);
const hov = T.frames.since(hoverFrom);
await T.d.send({ cmd: 'snapshot', path: over });
await T.snap('shell', 'hover-add-project');
const hdiff = T.diff(away, over, '232x40+4+770');
t.check('hover: moving onto "+ Add project" redraws by itself', hov.frames > 0, hov);
t.check('hover: "+ Add project" shows its hover state', hdiff > 50, { px: hdiff });
await T.mouse('move', 800, 600); await t.sleep(300);
const back = join(T.OUT, 'hover-back.png');
await T.d.send({ cmd: 'snapshot', path: back });
t.check('hover: leaving it clears the hover state', T.diff(away, back, '232x40+4+770') < 20, { px: T.diff(away, back, '232x40+4+770') });

if (budget) {
  t.check('budget: 95% of frames ≤ 7 ms while streaming', streaming.share7 >= 0.95, streaming);
  t.check('budget: 95% of frames ≤ 7 ms while scrolling', scroll.share7 >= 0.95, scroll);
}

// 4. Idle: nothing changes, nothing animates (the box not focused).
await t.sleep(2000);
const idleFrom = T.frames.mark();
await t.sleep(5000);
const idle = T.frames.since(idleFrom);
t.check('idle: no frames at all for 5 s', idle.frames === 0, idle);

// 5. A dead connection that never closed.
const before = (await T.state()).live || {};
const frozen = T.proxy.freeze();
t.check('the connections went silent', frozen >= 2, { frozen });
const deadAt = Date.now();
const s3 = await T.until('the silent link reconnects within 45 s', (s) => (s.live?.silentDrops ?? 0) > (before.silentDrops ?? 0) && (s.live?.connects ?? 0) > (before.connects ?? 0) && s.online, 45000);
if (s3) console.log(`reconnected after ${Math.round((Date.now() - deadAt) / 1000)} s`);
t.check('a new event stream with it', (s3?.live?.streams ?? 0) > (before.streams ?? 0), s3?.live);
// The chat lives on: a message sent from elsewhere (the server's API) shows by itself.
const sent = await T.api.http('POST', `/api/ui-sessions/${chat}/send`, { text: 'Reply with the single word: zebra' });
t.check('a message sent from elsewhere is taken', sent.status < 300, sent.body);
await T.until('its reply shows after the reconnect, with no other action', (s) => s.entries?.some((e) => e.kind === 'assistant' && /zebra/i.test(e.text)), 180000);
await T.until('the chat is idle after that reply', (s) => s.chatState === 'idle' && s.head && s.head.state !== 'working', 120000);
const s4 = await T.state();
t.check('after the reconnect the log still follows the end (not back at its top)', s4.logFollowing === true, { logFollowing: s4.logFollowing });

await T.snap('chat', 'after-reconnect-reply');
await T.end();
