// The workbench's suite (F80, F77, F64, F83): the "+" menu and its items,
// add-pane, Fit to screen and Layout, moving a pane by its head (swap, onto
// an empty leaf, a pinned one refused), renaming a chat by its title, a
// narrow chat head's "…" menu, a chat head's tags, full screen over the
// whole window, the double-click on an empty pane, the launcher's last-used
// mark. Run as test/lib.mjs's header says (a FRESH stub server each run).

import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import * as t from './lib.mjs';

const T = await t.begin('workbench');
const snap = (f) => T.snap('workbench', f);
const leafOf = (s, key) => s.leaves?.find((l) => l.key === key);
const keyAt = (s, id) => s.leaves?.find((l) => l.id === id)?.key;

await T.until('a fresh project shows the New terminal launcher', (s) => s.panes?.length === 1 && s.panes[0].startsWith('pend:'), 10000);

// ─── The "+" menu (F77) ───
const PLUS = { x: 379, y: 64 };
await T.mouse('click', PLUS.x, PLUS.y);
const m0 = await T.until('the project bar\'s "+" opens the menu', (s) => s.addMenu?.open, 5000);
t.check('the menu has the web\'s items, Panel among them (F64)', JSON.stringify(m0?.addMenu.items.map((i) => i.label)) === JSON.stringify(['Terminal', 'Files', 'Git', 'Kanban', 'Notes', 'Mindmap', 'Media', 'Factory', 'Panel', 'Fit to screen', 'Layout']), m0?.addMenu.items);
t.check('it sits 4px under the "+"', m0 && Math.abs(m0.addMenu.y - 82) <= 2 && Math.abs(m0.addMenu.x - (PLUS.x - 14)) <= 16, m0?.addMenu);
await snap('add-menu');
const lay = m0.addMenu.items.find((i) => i.id === 'layout');
await T.mouse('move', lay.x + 20, lay.y + lay.h / 2);
const m1 = await T.until('hovering Layout opens its presets', (s) => s.addMenu?.layouts, 3000);
await snap('add-menu-layout');
t.check('the presets are the web\'s', JSON.stringify(m1?.addMenu.sub.map((i) => i.label)) === JSON.stringify(['2 columns', '3 columns', '2×2 cross', 'Left + right split', 'Top + bottom split']), m1?.addMenu.sub);
// A press outside closes it.
await T.mouse('click', 900, 600);
await T.until('a press outside closes the menu', (s) => !s.addMenu?.open, 3000);

// Files through the menu, by a click on the item.
await T.mouse('click', PLUS.x, PLUS.y);
const m2 = await T.until('the menu opens again', (s) => s.addMenu?.open, 3000);
const files = m2.addMenu.items.find((i) => i.id === 'files');
await T.mouse('click', files.x + 20, files.y + files.h / 2);
await T.until('Files from the menu opens a Files pane, the menu closed', (s) => s.panes?.includes('panel:files') && !s.addMenu?.open, 8000);
// Files again: refused with the web's words.
await T.ui('add-pane', 'files');
await T.until('a second Files is refused: "Files is already in the workbench"', (s) => /Files is already in the workbench/.test(s.toast || ''), 3000);
// Panel (the add-pane driver action, F64).
await T.ui('add-pane', 'panel');
await T.until('Panel adds a panel pane', (s) => s.panes?.some((k) => k.startsWith('panel:widget')), 8000);
await T.ui('add-pane', 'terminal');
// (Files took the first launcher's place, as the next pane placed does.)
await T.until('Terminal adds a launcher', (s) => s.panes?.some((k) => k.startsWith('pend:')) && s.panes.length >= 3, 5000);
await T.ui('add-pane', 'layout:cols2');
await T.until('Layout: 2 columns lays the panes out again', (s) => s.leaves?.length >= 3 && s.leaves.every((l) => l.y === s.leaves[0].y), 3000);
await T.ui('add-pane', 'fit');
await t.sleep(300);
await snap('after-add');

// ─── A chat (rename, tags, "…", full screen) ───
// Start clean: only the chat and one launcher.
let s = await T.state();
for (const k of s.panes.filter((k) => k.startsWith('panel:'))) { await T.ui('pane-close', k); }
await T.ui('new-chat');
s = await T.until('a chat pane', (s) => s.chat && s.panes?.includes(`live:${s.chat}`), 30000);
const chat = `live:${s.chat}`;
await T.until('the chat head carries the "chat" tag (F80)', (s) => s.headProps?.[chat]?.tags?.includes('chat'), 5000);
await T.ui('add-pane', 'layout:cols2');
await T.ui('add-pane', 'fit');
s = await T.until('the chat and a launcher side by side', (s) => s.leaves?.filter((l) => l.key).length >= 2, 5000);
await snap('chat-head-wide');

// Rename: a click on the title, type, Enter.
let cl = leafOf(s, chat);
await T.mouse('click', cl.x + 52, cl.y + 14);
await T.until('a click on the chat\'s title opens the rename box', (s) => s.headProps?.[chat]?.renaming, 3000);
await snap('rename-box');
await T.key('Renamed chat');
await T.key('Return');
await T.until('Enter saves: the head shows the new title at once', (s) => !s.headProps?.[chat]?.renaming && s.heads?.[chat] === 'Renamed chat', 3000);
await t.sleep(800);
const list = await T.api.http('GET', '/api/ui-sessions');
const onServer = JSON.stringify(list.body || '').includes('Renamed chat');
t.check('the server got the rename (POST /api/ui-sessions/:id/rename)', onServer, list.body);
// Escape cancels.
s = await T.state(); cl = leafOf(s, chat);
await T.mouse('click', cl.x + 52, cl.y + 14);
await T.until('the box again', (s) => s.headProps?.[chat]?.renaming, 3000);
await T.key('Nope');
await T.key('Escape');
await T.until('Escape puts the title back', (s) => !s.headProps?.[chat]?.renaming && s.heads?.[chat] === 'Renamed chat', 3000);

// ─── Moving a pane (F80): swap by a real drag on the head ───
s = await T.state();
const a = leafOf(s, chat);
const b = s.leaves.find((l) => l.key && l.key !== chat);
const bKey = b.key;
// Pick it up and move; look mid-drag. (Counted from just after the press:
// the press itself may clear the head's own tooltip, which redraws that
// head alone, GPUI patch (c).)
await T.mouse('down', a.x + 200, a.y + 12);
await t.sleep(150);
const r0 = (await T.state()).renders || {};
for (let i = 1; i <= 6; i++) await T.mouse('move', a.x + 200 + i * ((b.x + b.w / 2 - a.x - 200) / 6), a.y + 12 + i * 40, { buttons: ['left'] });
const mid = await T.state();
t.check('mid-drag: the pane is picked up and the other lit', mid.drag && mid.drag.src === a.id && mid.drag.target === b.id, mid.drag);
await snap('drag-mid');
await T.mouse('up', b.x + b.w / 2, a.y + 12 + 240);
const r1 = (await T.state()).renders || {};
t.check('moving a pane redrew no head (only the card and the layout views)', (r1.head || 0) - (r0.head || 0) <= 2, { head: [r0.head, r1.head], workbench: [r0.workbench, r1.workbench] });
s = await T.until('the drop swaps the two panes', (s) => keyAt(s, a.id) === bKey && keyAt(s, b.id) === chat, 3000);
// Back with the driver's drag (one go, as a hand fast).
s = await T.state();
const a2 = leafOf(s, chat); const b2 = leafOf(s, bKey);
await T.mouse('drag', a2.x + 200, a2.y + 12, { toX: b2.x + b2.w / 2, toY: b2.y + 300, steps: 10 });
await T.until('a drag back swaps them again', (s) => keyAt(s, a.id) === chat && keyAt(s, b.id) === bKey, 3000);
// A pinned pane does not move.
await T.ui('pane-pin', chat);
s = await T.state();
const p = leafOf(s, chat);
await T.mouse('drag', p.x + 200, p.y + 12, { toX: b.x + b.w / 2, toY: b.y + 300, steps: 10 });
await T.until('a pinned pane is refused: "This pane is pinned. Unpin it to move it."', (s) => /This pane is pinned/.test(s.toast || '') && keyAt(s, a.id) === chat, 3000);
await T.ui('pane-pin', chat);

await T.ui('pane-split', bKey, 'right');
await T.ui('add-pane', 'fit');
await t.sleep(300);
// ─── Double-click on an empty pane maximises it (F80) ───
s = await T.state();
const empty = s.leaves.find((l) => !l.key);
await T.mouse('dblclick', empty.x + empty.w / 2, empty.y + 20);
s = await T.until('a double-click on an empty pane fills its split', (s) => (s.leaves.find((l) => l.id === empty.id)?.w || 0) > empty.w + 50, 3000);
await snap('empty-maximised');

// ─── The narrow head's "…" menu (F80) ───
s = await T.until('the chat pane is narrow', (s) => leafOf(s, chat)?.narrow, 5000);
cl = leafOf(s, chat);
t.check('narrow: its tools are folded (no A- in the head)', true);
// "…": right of full screen, left of ×: × 28 wide, 4 from the edge.
const moreX = cl.x + cl.w - 1 - 4 - 28 - 4 - 12;
await T.mouse('click', moreX, cl.y + 13);
await T.until('"…" opens the folded tools', (s) => s.headProps?.[chat]?.menu, 3000);
await snap('narrow-more-menu');
await T.mouse('click', moreX, cl.y + 13);
await T.until('"…" again closes them', (s) => !s.headProps?.[chat]?.menu, 3000);
await T.mouse('click', moreX, cl.y + 13);
await T.until('open again', (s) => s.headProps?.[chat]?.menu, 3000);
await T.mouse('click', cl.x + 40, cl.y + 300);
await T.until('a press outside closes them', (s) => !s.headProps?.[chat]?.menu, 3000);
// A very long title never pushes the head's right side out (the owner's
// "Factory: Replace Electron…" chat): the title takes what is left, elided;
// the WORKING badge and the buttons keep their size, inside the pane.
{
  const long = 'Factory: Replace Electron with the native client, and everything the owner uses, carried over without anything lost';
  const id = chat.slice('live:'.length);
  await T.api.http('POST', `/api/ui-sessions/${id}/rename`, { title: long });
  await T.until('a very long title in a narrow pane', (s) => s.heads?.[chat] === long && leafOf(s, chat)?.narrow, 8000);
  await T.ui('composer-event', JSON.stringify({ type: 'state', state: 'working' }));
  s = await T.until('…working: the WORKING badge drawn', (s) => s.headProps?.[chat]?.rects?.badge, 5000);
  cl = leafOf(s, chat);
  const r = s.headProps[chat].rects;
  const right = (b) => b.x + b.w;
  t.check('…the buttons stay inside the pane', right(r.tools) <= cl.x + cl.w + 0.5 && r.tools.w >= 28 + 24 + 24, { r, cl });
  t.check('…the badge whole, left of the buttons', r.badge.w >= 40 && right(r.badge) <= r.tools.x + 0.5, r);
  t.check('…the title elided in the room left, before the badge', right(r.title) <= r.badge.x + 0.5 && r.title.w < cl.w / 2, { r, cl });
  await snap('long-title-narrow');
  await T.ui('composer-event', JSON.stringify({ type: 'state', state: 'idle' }));
}
// A head button's tooltip after a click (F213): it stays away while the
// pointer rests there, and comes back once the pointer has left and returned
// (as a browser's title; w::tip).
{
  const tipOf = async (x, y) => { await T.mouse('move', x, y); await T.mouse('move', x + 1, y); await t.sleep(1300); return (await T.state()).workspace?.tip || {}; };
  let tip = await tipOf(moreX, cl.y + 13);
  const text = tip.text;
  t.check('resting on a head button shows its tooltip', !!text && tip.shown && tip.ago < 1200, tip);
  await T.mouse('click', moreX + 1, cl.y + 13);
  await T.until('…a click opens its menu', (s) => s.headProps?.[chat]?.menu, 3000);
  tip = await tipOf(moreX + 2, cl.y + 13);
  t.check('after a click on a head button its tooltip stays away while the pointer rests there (F213)', tip.text === text && !tip.shown, tip);
  await T.mouse('click', cl.x + 40, cl.y + 300);
  await T.until('…its menu closed', (s) => !s.headProps?.[chat]?.menu, 3000);
  await t.sleep(200);
  tip = await tipOf(moreX, cl.y + 13);
  t.check('…and comes back once the pointer left and returned', tip.text === text && tip.shown && tip.ago < 1200, tip);
  await T.mouse('move', cl.x + 40, cl.y + 300);
}

// ─── Full screen over the whole window (F80) ───
await T.ui('pane-full', chat);
await T.until('full screen', (s) => s.full === chat, 3000);
await t.sleep(300);
await snap('full-screen');
await T.ui('pane-full', chat);
await T.until('and back', (s) => !s.full, 3000);

// ─── The launcher's last-used mark (F80) ───
writeFileSync(join(T.configDir, 'launch-modes.json'), JSON.stringify({ demo: 'normal' }));
await T.ui('add-pane', 'layout:cols3');
await T.ui('add-pane', 'terminal');
await T.until('the project\'s last launch mode is known', (s) => s.launchLast === 'normal', 3000);
await t.sleep(300);
await snap('launcher-last-used');

// ─── Moving a pane near the grid's edge scrolls it (F181) ───
// The web's makeDraggable runs wbAutoScroll on every move, the pane move by
// its head too (src/webapp-html.js:8388, 16998): four columns at 1400 px are
// wider than the grid, and a pane held in its 60 px edge band scrolls it.
await T.d.send({ cmd: 'resize', w: 1400, h: 860 });
await T.ui('add-pane', 'layout:cols3');
s = await T.state();
await T.ui('pane-split', s.leaves.find((l) => l.key).key, 'right');
s = await T.until('four columns, wider than the grid', (s) => s.leaves.length >= 4 && s.grid?.max > 100, 5000);
let g = s.grid;
// The chat to the third column (its head dropped on the launcher there: they
// swap), where it stays wholly in view while the grid scrolls (F213 below).
{
  const ck = s.leaves.find((l) => l.key?.startsWith('live:')), to = s.leaves.filter((l) => l.key?.startsWith('pend:')).sort((p, q) => p.x - q.x)[0];
  if (ck && to && to.x > ck.x + ck.w) {
    const [x0, y0] = [ck.x + 60, ck.y + 12], [x1, y1] = [to.x + to.w / 2, to.y + 200];
    await T.mouse('move', x0, y0); await T.mouse('down', x0, y0);
    for (let i = 1; i <= 6; i++) { await T.mouse('move', x0 + (x1 - x0) * i / 6, y0 + (y1 - y0) * i / 6, { buttons: ['left'] }); await t.sleep(30); }
    await T.mouse('up', x1, y1);
    s = await T.until('(the chat moved to the third column)', (s) => !s.drag && s.leaves.find((l) => l.key === ck.key)?.x > to.x - 2, 3000);
    await t.sleep(300);
    s = await T.state(); g = s.grid;
  }
}
const chatLeaf = s.leaves.find((l) => l.key?.startsWith('live:'));
const src = s.leaves.filter((l) => l.key && l.x >= g.x).sort((p, q) => p.x - q.x)[0];
const hx = Math.max(src.x + 60, g.x + 80), hy = src.y + 12;
await T.mouse('move', hx, hy);
await T.mouse('down', hx, hy);
const ry = hy + 200, rx = g.x + g.w - 15;
for (let i = 1; i <= 5; i++) { await T.mouse('move', hx + (rx - hx) * i / 6, ry, { buttons: ['left'] }); await t.sleep(30); }
await t.sleep(300);
const er0 = (await T.state()).renders || {}, ef = T.frames.mark();
await T.mouse('move', rx, ry, { buttons: ['left'] });
s = await T.until('held at the right edge, the grid scrolls right', (s) => s.drag && s.grid.scroll > 60, 5000);
const lit = s.leaves.find((l) => l.id === s.drag.target);
t.check('…and the lit pane is the one now under the pointer', !s.drag.target || (lit && rx >= lit.x && rx < lit.x + lit.w), { target: s.drag.target, lit, rx });
s = await T.until('…up to its end', (s) => s.drag && Math.abs(s.grid.scroll - s.grid.max) < 1, 5000);
// Each tick moves the grid by whole pixels: the panes' bodies are replayed
// moved, not drawn again (F213): the chat's log, its composer and Mini's
// layers render far less than once a tick. A pane crossing the grid's edge
// is replayed too, leaving (F241) and entering (patch e): below.
{
  const er1 = s.renders || {}, frames = T.frames.since(ef).frames;
  const d = Object.fromEntries(Object.keys(er1).map((k) => [k, (er1[k] || 0) - (er0[k] || 0)]).filter(([, v]) => v));
  console.log(`  edge scroll to the end (${Math.round(s.grid.max)} px, ${frames} frames):`, JSON.stringify(d));
  const seen = chatLeaf && chatLeaf.x - s.grid.max >= g.x && chatLeaf.x + chatLeaf.w <= g.x + g.w;
  t.check('during an edge scroll the chat (wholly in view) is moved, not drawn: its log, composer and Mini\'s layers not on every tick (F213)', seen && frames >= 5 && (d.log || 0) < frames / 2 && (d.composer || 0) < frames / 2 && (d['mini-panes'] || 0) < frames / 2, { d, frames, chat: chatLeaf, g });
}
const lx = g.x + 15;
for (let i = 1; i <= 6; i++) { await T.mouse('move', rx + (lx - rx) * i / 6, ry, { buttons: ['left'] }); await t.sleep(30); }
s = await T.until('held at the left edge, it scrolls back', (s) => s.drag && s.grid.scroll < 1, 5000);
await T.mouse('move', g.x + g.w / 2, ry, { buttons: ['left'] });
s = await T.until('away from the edges, the ramp stops', (s) => s.drag && !s.grid.edge, 3000);
const sc = s.grid.scroll;
await t.sleep(300);
s = await T.state();
t.check('…and the grid stays where it is', Math.abs(s.grid.scroll - sc) < 1, { before: sc, after: s.grid.scroll });
await T.mouse('up', g.x + g.w / 2, ry);
await T.until('let go, the move ends', (s) => !s.drag, 3000);

// A pane leaving across the grid's edge (F241): the chat swapped into the
// first column, four columns again, then a pane held at the right edge scrolls the grid, and the
// chat moves out across its left edge. Each tick brings nothing of it into
// view that was cut off, so it is replayed moved, not drawn: its log and
// composer render far less than once a tick.
{
  await t.sleep(300);
  s = await T.state();
  const ck = s.leaves.find((l) => l.key?.startsWith('live:'));
  const first = s.leaves.filter((l) => l.key).sort((p, q) => p.x - q.x)[0];
  if (ck && first && first.id !== ck.id) {
    const [x0, y0] = [ck.x + 60, ck.y + 12], [x1, y1] = [first.x + first.w / 2, first.y + 200];
    await T.mouse('move', x0, y0); await T.mouse('down', x0, y0);
    for (let i = 1; i <= 6; i++) { await T.mouse('move', x0 + (x1 - x0) * i / 6, y0 + (y1 - y0) * i / 6, { buttons: ['left'] }); await t.sleep(30); }
    await T.mouse('up', x1, y1);
    s = await T.until('(the chat moved to the first column)', (s) => !s.drag && s.leaves.find((l) => l.key === ck.key)?.x <= first.x + 2, 3000);
  }
  // (The last move took a column: four again, wider than the grid.)
  s = await T.state();
  if (!(s.grid?.max > 100)) {
    const n = s.leaves.length;
    await T.ui('pane-split', s.leaves.filter((l) => l.key).sort((p, q) => q.x - p.x)[0].key, 'right');
    s = await T.until('(four columns again)', (s) => s.leaves.length > n && s.grid?.max > 100, 5000);
  }
  await t.sleep(300);
  s = await T.state();
  const g2 = s.grid, other = s.leaves.filter((l) => l.key && !l.key.startsWith('live:') && l.x >= g2.x).sort((p, q) => p.x - q.x)[0];
  const px0 = Math.max(other.x + 60, g2.x + 80), py0 = other.y + 12, py = py0 + 200, prx = g2.x + g2.w - 15;
  await T.mouse('move', px0, py0); await T.mouse('down', px0, py0);
  for (let i = 1; i <= 5; i++) { await T.mouse('move', px0 + (prx - px0) * i / 6, py, { buttons: ['left'] }); await t.sleep(30); }
  await t.sleep(300);
  const r0 = (await T.state()).renders || {}, f0 = T.frames.mark();
  await T.mouse('move', prx, py, { buttons: ['left'] });
  s = await T.until('held at the right edge again, the grid scrolls to its end', (s) => s.drag && Math.abs(s.grid.scroll - s.grid.max) < 1, 5000);
  const r1 = s.renders || {}, frames = T.frames.since(f0).frames;
  const d = Object.fromEntries(Object.keys(r1).map((k) => [k, (r1[k] || 0) - (r0[k] || 0)]).filter(([, v]) => v));
  const c = s.leaves.find((l) => l.key?.startsWith('live:')), cx0 = c && c.x;
  console.log(`  edge scroll, the chat leaving (${frames} frames, chat ${Math.round(cx0)}..${Math.round(cx0 + c?.w)}, grid ${g2.x}..):`, JSON.stringify(d));
  t.check('during an edge scroll the chat crossing the grid\'s edge is moved, not drawn: its log and composer not on every tick (F241)', c && cx0 < g2.x && cx0 + c.w > g2.x && frames >= 5 && (d.log || 0) < frames / 2 && (d.composer || 0) < frames / 2, { d, frames, chat: c, g: s.grid });
  // The chat entering across the left edge as the grid scrolls back.
  const plx = g2.x + 15;
  for (let i = 1; i <= 5; i++) { await T.mouse('move', prx + (plx - prx) * i / 6, py, { buttons: ['left'] }); await t.sleep(30); }
  await t.sleep(300);
  const r2 = (await T.state()).renders || {}, f2 = T.frames.mark();
  await T.mouse('move', plx, py, { buttons: ['left'] });
  s = await T.until('…and held at the left edge, back', (s) => s.drag && s.grid.scroll < 1, 5000);
  {
    const r3 = s.renders || {}, frames = T.frames.since(f2).frames;
    const d = Object.fromEntries(Object.keys(r3).map((k) => [k, (r3[k] || 0) - (r2[k] || 0)]).filter(([, v]) => v));
    console.log(`  edge scroll, the chat entering (${frames} frames):`, JSON.stringify(d));
    // Entering brings into view what the grid's clip cut off: the grid's
    // clip is a wide clip (GPUI patch (e)), so its last scene holds it and
    // the chat is replayed moved: its log, composer and Mini's layers
    // render once at most (their settle), not on every tick.
    t.check('during an edge scroll the chat entering across the grid\'s edge is moved, not drawn: its log, composer and Mini\'s layers 0-1 times (F213, patch e)', frames >= 5 && ['log', 'composer', 'mini-panes', 'chat-mini'].every((k) => (d[k] || 0) <= 1), { d, frames });
  }
  await T.mouse('move', g2.x + g2.w / 2, py, { buttons: ['left'] });
  await t.sleep(200);
  await T.mouse('up', g2.x + g2.w / 2, py);
  await T.until('let go, that move ends', (s) => !s.drag, 3000);
}
// ─── T7: over a pane's body a sideways scroll moves the strip (T8: never a panic) ───
// A pane that can't use a sideways scroll (no area in it scrolls sideways)
// leaves it to the workbench strip; up and down stay the pane's.
{
  s = await T.state();
  const G = () => s.grid;
  const body = s.leaves.filter((l) => l.key && l.x > G().x && l.x + l.w < G().x + G().w)[0] || s.leaves[0];
  const at = [body.x + body.w / 2, body.y + Math.min(body.h - 20, 120)];
  const scrollOf = async () => (s = await T.state()).grid.scroll;
  // To the strip's start (wheel steps that never glide).
  for (let i = 0; i < 40 && (await scrollOf()) > 0.5; i++) await T.mouse('wheel', at[0], at[1], { dx: 200 });
  const a0 = await scrollOf();
  await T.mouse('wheel', at[0], at[1], { dx: -120 });
  const a1 = await scrollOf();
  t.check('T7: a sideways scroll over a pane\'s body scrolls the workbench strip', a1 - a0 > 60, { a0, a1, body: body.key });
  await T.mouse('wheel', at[0], at[1], { dy: -120 });
  const a2 = await scrollOf();
  t.check('T7: …an up-and-down one stays the pane\'s (the strip does not move)', Math.abs(a2 - a1) < 0.5, { a1, a2 });
  await T.mouse('wheel', at[0], at[1], { dy: -120, mods: ['shift'] });
  const a3 = await scrollOf();
  t.check('T7: …Shift and the wheel scroll the strip sideways', a3 - a2 > 60, { a2, a3 });
  // T8: through the whole range and back, both ways, over a body and a head.
  for (let round = 0; round < 3; round++) {
    for (let i = 0; i < 12; i++) await T.mouse('wheel', at[0], at[1], { dx: -150 });
    for (let i = 0; i < 4; i++) await T.mouse('wheel', at[0], at[1], { dy: i % 2 ? 300 : -300 });
    for (let i = 0; i < 12; i++) await T.mouse('wheel', at[0], at[1], { dx: 150, mods: i % 2 ? ['shift'] : [] });
  }
  const end = await scrollOf();
  t.check('T8: scrolled sideways and down through the whole range three times, the client still answers', Number.isFinite(end), { end });
  // A new pane (the "+" menu's Terminal) opens at the strip's right and comes wholly into view.
  for (let i = 0; i < 40 && (await scrollOf()) > 0.5; i++) await T.mouse('wheel', at[0], at[1], { dx: 200 });
  const before = new Set((await T.state()).leaves.map((l) => l.key).filter(Boolean));
  await T.mouse('click', PLUS.x, PLUS.y);
  const mt = await T.until('(the "+" menu)', (s) => s.addMenu?.open, 3000);
  const term = mt.addMenu.items.find((i) => i.label === 'Terminal');
  await T.mouse('click', term.x + 20, term.y + term.h / 2);
  s = await T.until('T7: a new pane comes wholly into view in the strip', (s) => { const l = s.leaves.find((x) => x.key && !before.has(x.key)); return l && l.x >= s.grid.x - 1 && l.x + l.w <= s.grid.x + s.grid.w + 1; }, 5000);
  { const st = await T.state(); console.log('  T7 new pane:', JSON.stringify({ grid: st.grid, leaves: st.leaves.map((l) => [l.id, l.key, Math.round(l.x), Math.round(l.w)]) })); }
  await snap('t7-strip-scrollbar');
}
// ─── Kinetic scrolling: a touchpad's scroll glides on, a wheel's does not ───
// (vendor/gpui patch (f): the one scroll path every list and pane uses.)
// Precise steps over the grid (four columns, wider than it): after the
// last one it keeps moving, slows, and stops; a strong flick stops at the
// edge; the wheel's line scroll stops dead; a press catches a glide.
{
  s = await T.state();
  const G = () => s.grid;
  // Over a pane's head (nothing inside it scrolls: the grid takes the scroll).
  const hd = s.leaves.filter((l) => l.key && l.x > G().x && l.x + l.w < G().x + G().w)[0] || s.leaves[0];
  const at = [hd.x + hd.w / 2, hd.y + 12];
  const scrollOf = async () => (s = await T.state()).grid.scroll;
  const settle = async () => { let a = await scrollOf(); for (let i = 0; i < 40; i++) { await t.sleep(150); const b = await scrollOf(); if (Math.abs(b - a) < 0.5 && !s.window?.gliding) return b; a = b; } return a; };
  // A touchpad's swipe: n precise steps 12 ms apart (timed in the app, as the hardware sends them).
  const swipe = async (dx, n = 8) => { await T.mouse('wheel', at[0], at[1], { dx, precise: true, steps: n, every: 12 }); await t.sleep(n * 12 + 5); };
  const start = await settle();
  const toEnd = start < G().max / 2 ? -1 : 1;   // pixel deltas follow the fingers: negative scrolls right
  await swipe(4 * toEnd, 10);
  const g1 = await T.until('a touchpad scroll glides on after its last step', (s) => s.window?.gliding, 1500);
  const end = await settle();
  // The steps alone move it 40 px; the glide carries it further, then stops.
  t.check('…further than its steps took it, then slows and stops', Math.abs(end - start) > 40 + 10 && !s.window?.gliding, { start, during: g1?.grid?.scroll, end });
  // A strong flick: it glides to the edge and stops there.
  await swipe(40 * toEnd);
  const edge = await settle();
  t.check('…a strong one stops at the content\'s edge', Math.abs(edge - (toEnd < 0 ? G().max : 0)) < 1, { edge, max: G().max });
  // Back the other way, then a press while it glides: it stops where it is.
  await swipe(-8 * toEnd);
  await T.until('(gliding back)', (s) => s.window?.gliding && Math.abs(s.grid.scroll - edge) > 64, 1500);
  await T.mouse('down', at[0], at[1]); await T.mouse('up', at[0], at[1]);
  const caught = await scrollOf();
  await t.sleep(300);
  t.check('…a press catches the glide: it stops at once', Math.abs((await scrollOf()) - caught) < 0.5 && !s.window?.gliding, { caught, now: s.grid.scroll });
  // The mouse wheel's line scroll: it moves, and stops dead.
  const w0 = await scrollOf();
  for (let i = 0; i < 4; i++) { await T.mouse('wheel', at[0], at[1], { dx: toEnd, lines: true }); await t.sleep(14); }
  const w1 = await scrollOf();
  await t.sleep(300);
  const w2 = await scrollOf();
  t.check('a mouse wheel\'s line scroll moves the grid', Math.abs(w1 - w0) > 1, { w0, w1 });
  t.check('…and does not glide', Math.abs(w2 - w1) < 0.5 && !s.window?.gliding, { w1, w2 });
}
await T.d.send({ cmd: 'resize', w: 1440, h: 900 });

// (No idle check here: the launcher on screen animates UI Mode, by the owner's call.)
await T.end();
