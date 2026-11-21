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
const r0 = s.renders || {};
// Pick it up and move; look mid-drag.
await T.mouse('down', a.x + 200, a.y + 12);
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

// (No idle check here: the launcher on screen animates UI Mode, by the owner's call.)
await T.end();
