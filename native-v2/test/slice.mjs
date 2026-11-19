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

// 2. A new chat, its pane, a streamed reply.
await T.ui('new-chat');
const s0 = await T.until('the new chat has its pane, focused', (s) => s.chat && s.panes?.includes(`live:${s.chat}`), 30000);
const chat = s0?.chat;
await T.snap('chat', 'new-chat-pane');
const b0 = s0?.batches ?? 0;
const streamFrom = T.frames.mark();
await T.ui('send', 'Reply with a markdown list of the numbers one to forty, one per line, written as words. Nothing else.');
const s1 = await T.until('the user message shows', (s) => s.entries?.some((e) => e.kind === 'user'), 15000);
await T.snap('chat', 'user-message-sent');
// The reply arrives by itself, as Claude streams it.
const s2 = await T.until("Claude's reply streams in", (s) => s.entries?.some((e) => e.kind === 'assistant' && /forty/i.test(e.text)), 180000);
await T.snap('chat', 'reply-arrived');
t.check('the reply came in several live batches', (s2?.batches ?? 0) - b0 >= 3, { b0, b: s2?.batches });
t.check('rows were spliced, not rebuilt per event', (s2?.spliced ?? 0) < 10 * (s2?.batches ?? 1), s2 && { spliced: s2.spliced, batches: s2.batches });
await T.until('the chat is idle again', (s) => s.chatState === 'idle', 120000);
const streaming = T.frames.since(streamFrom);
await T.snap('chat', 'reply-done-idle');
console.log('frames while streaming:', JSON.stringify(streaming));
t.check('frames were drawn while it streamed', streaming.frames > 0, streaming);

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
const wheelFrom = T.frames.mark();
for (let i = 0; i < 30; i++) { await T.mouse('wheel', 800, 400, { dy: 40 }); await t.sleep(16); }
await t.sleep(400);
const wheel = T.frames.since(wheelFrom);
console.log('frames while wheel-scrolling:', JSON.stringify(wheel));
await T.d.send({ cmd: 'snapshot', path: after3 });
await T.snap('chat', 'log-wheel-scrolled');
t.check('a mouse wheel over the log draws frames by itself', wheel.frames > 0, wheel);
t.check('a mouse wheel over the log moves its content', T.diff(before3, after3, '1100x600+260+120') > 500, { px: T.diff(before3, after3, '1100x600+260+120') });

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

await T.snap('chat', 'after-reconnect-reply');
await T.end();
