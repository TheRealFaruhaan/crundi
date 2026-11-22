// Drops (run by test/planning.mjs; F100, F121): a Kanban card and a subtask,
// a Mindmap idea and a Notes block dragged with the pointer, as a person
// does, from their workbench pane onto a terminal pane. Each lands as the
// web's labelled reference ("[Kanban Task ID: …]") or, for a Notes block,
// its Markdown, typed into the program ("Sent to terminal").

const PA = (p) => p.areas?.planning ?? p.planningArea ?? {};
const K = (p) => PA(p).kanban;
const M = (p) => PA(p).mindmap;
const N = (p) => PA(p).notes;
const W = (p) => p.workspace;

export async function run(T, t) {
  const sleep = t.sleep;
  const kpost = (body) => T.api.http('POST', '/api/kanban', { project: 'demo', ...body });
  const card = (await kpost({ action: 'addTask', title: 'Drop me', status: 'todo' })).body;
  const cardId = card?.task?.id ?? card?.id;
  await kpost({ action: 'addTodo', taskId: cardId, text: 'A step to drop' });
  const idea = (await T.api.http('POST', '/api/mindmap', { action: 'addNode', text: 'An idea to drop' })).body?.node;
  const page = (await T.api.http('POST', '/api/notes', { project: 'demo', clientId: 'test-other', action: 'create', title: 'Dropped', blocks: [{ type: 'p', text: 'A <b>dropped</b> line' }] })).body.page;
  t.check('a card, a subtask, an idea and a page to drag', !!cardId && !!idea?.id && !!page?.id, { card, idea });

  // A terminal (an Empty Shell) that swallows what it is given.
  await T.ui('tab', 'workbench');
  // The suites before leave panes open: an empty workbench to start.
  for (const k of (await T.props()).panes || []) if (!k.startsWith('pend:')) await T.ui('workbench:pane-close', k);
  await T.until('the workbench has its New terminal launcher', (p) => p.panes?.some((k) => k.startsWith('pend:')));
  await T.ui('workspace:launch', '', 'shell');
  let s = await T.until('an Empty Shell in a pane', (p) => p.panes?.some((k) => k.startsWith('term:')), 30000);
  const key = s?.panes.find((k) => k.startsWith('term:'));
  const TS = (p) => W(p)?.terms.find((x) => x.key === key);
  await T.until('the shell prints its prompt', (p) => TS(p)?.screen.some((l) => /\$\s*$/.test(l) || /\$ /.test(l)), 20000);
  await T.ui('workspace:term-send', key, 'cat > /dev/null');
  await sleep(400);
  const screen = (p) => (TS(p)?.screen || []).join('');
  const leaf = (p, k) => p.leaves?.find((l) => l.key === k);

  // Drag from (x, y) onto the terminal's screen; the text it should get.
  // ready: what must hold with the pointer at rest on (x, y) before the
  // press (F136: a Notes block's grip shows only while its block is hovered;
  // a press before the grip is drawn there presses the margin instead).
  const dropOnTerm = async (what, x, y, want, ready = null) => {
    await T.ui('toasts-clear');
    const p = await T.props();
    const tl = leaf(p, key);
    if (!tl) { t.check(`${what}: the terminal is on screen`, false, p.leaves); return; }
    // The pointer comes to rest on it first (a hovered idea is the one that drags).
    await T.mouse('move', x, y);
    await sleep(250);
    if (ready) { if (!(await T.until(`${what}: ready to drag`, ready))) return; await sleep(120); }
    await T.mouse('drag', x, y, { toX: tl.x + tl.w / 2, toY: tl.y + tl.h / 3, steps: 14 });
    const got = await T.until(`${what} dropped on the terminal is typed there, "Sent to terminal"`, (q) => q.toast === 'Sent to terminal' && screen(q).includes(want), 10000);
    if (!got) { const z = await T.props(); console.log('   drag from', x, y, 'to', tl, 'toast', z.toast, 'screen', TS(z)?.screen.slice(-4)); }
  };

  // ── A Kanban card and a subtask ──
  await T.act('open-pane', 'kanban');
  await T.ui('tab', 'workbench');
  const inLeaf = (l, b) => l && b && b.w > 0 && b.x >= l.x && b.x + 30 < l.x + l.w && b.y > l.y + 70 && b.y + 30 < l.y + l.h;
  const cardShown = (p) => inLeaf(leaf(p, 'panel:kanban'), K(p)?.boxes?.cards?.find((b) => b.id === cardId));
  s = await T.until('a Kanban pane beside the terminal', (p) => leaf(p, 'panel:kanban') && leaf(p, key) && K(p)?.boxes?.cards?.length);
  // (A board with many cards: scrolled to the card, as a person would.)
  for (let i = 0; s && i < 60 && !cardShown(await T.props()); i++) {
    const kl = leaf(await T.props(), 'panel:kanban');
    await T.mouse('wheel', kl.x + kl.w / 2, kl.y + kl.h / 2, { dy: -120 });
    await sleep(60);
  }
  s = await T.until('the card on show in the pane', cardShown);
  if (s) {
    const b = K(s).boxes.cards.find((x) => x.id === cardId);
    await dropOnTerm('a Kanban card', b.x + 30, b.y + 14, `[Kanban Task ID: ${cardId}]`);
    s = await T.props();
    const todo = K(s).columns.flatMap((c) => c.cards).find((c) => c.id === cardId)?.todos?.[0];
    const moreOf = (p) => todo && K(p).boxes.controls.find((c) => c.id === cardId && c.control === `more:${todo.id}`);
    // (Well inside the pane: its row whole, clear of the pane's bottom.)
    const roomy = (p) => { const l = leaf(p, 'panel:kanban'); const m = moreOf(p); return inLeaf(l, m) && m.y < l.y + l.h - 160; };
    for (let i = 0; i < 12 && todo && !roomy(s); i++) {
      const kl = leaf(s, 'panel:kanban');
      await T.mouse('wheel', kl.x + kl.w / 2, kl.y + kl.h / 2, { dy: -60 });
      await sleep(80);
      s = await T.props();
    }
    const more = moreOf(s);
    const cb = K(s).boxes.cards.find((x) => x.id === cardId);
    if (more && cb) await dropOnTerm('a Kanban subtask', cb.x + 50, more.y + more.h / 2, `[Kanban Subtask ID: ${todo.id}]`);
    else t.check('the card\'s subtask row is on show', false, { todo, controls: K(s).boxes.controls.filter((c) => c.id === cardId), card: cb, leaf: leaf(s, 'panel:kanban'), drag: K(s).drag, held: K(s).held, cards: K(s).boxes.cards.length });
  }
  await T.ui('workbench:pane-close', 'panel:kanban');
  await T.until('the Kanban pane closes', (p) => !p.panes?.includes('panel:kanban'));

  // ── A Mindmap idea ──
  await T.act('open-pane', 'mindmap');
  await T.ui('tab', 'workbench');
  const ideaShown = (p) => inLeaf(leaf(p, 'panel:mindmap'), M(p)?.nodes?.find((r) => r.id === idea.id)?.box);
  s = await T.until('a Mindmap pane beside the terminal', (p) => leaf(p, 'panel:mindmap') && M(p)?.state === 'ok' && M(p).nodes?.some((r) => r.id === idea.id && r.box));
  if (s && !ideaShown(s)) {
    // (A big map: panned to the idea, as a person would.)
    const ml = leaf(s, 'panel:mindmap');
    const bx = M(s).nodes.find((r) => r.id === idea.id).box;
    await T.act('mindmap-pan', String(bx.x - (ml.x + ml.w / 3)), String(bx.y - (ml.y + ml.h / 2)));
    if (!ideaShown(await T.props())) await T.act('mindmap-pan', String(-2 * (bx.x - (ml.x + ml.w / 3))), String(-2 * (bx.y - (ml.y + ml.h / 2))));
  }
  s = await T.until('the idea on show in the pane', ideaShown);
  if (s) {
    // The idea in hand follows the pointer over the window, out of the map
    // (its clone is drawn over every pane, as the web's fixed .drag-clone).
    // (F176: the map has drawn the idea where its box says, and the pointer
    // rests there, before the press: else it starts no drag.)
    await T.settle(400, 4000);
    s = await T.props();
    const r0 = M(s).nodes.find((x) => x.id === idea.id);
    const tl = leaf(s, key);
    const [gx, gy] = [r0.box.x + Math.min(30, r0.box.w / 2), r0.box.y + r0.box.h / 2];
    await T.mouse('move', gx, gy);
    await sleep(250);
    await T.mouse('down', gx, gy);
    const [tx, ty] = [tl.x + tl.w / 2, tl.y + tl.h / 2];
    for (let i = 1; i <= 12; i++) await T.mouse('move', gx + (tx - gx) * i / 12, gy + (ty - gy) * i / 12, { buttons: ['left'] });
    const dg = await T.until('over the terminal, the idea\'s clone is there with the pointer', (p) => M(p).drag?.id === idea.id
      && M(p).drag.x > tl.x - 200 && M(p).drag.x < tx && M(p).drag.y > tl.y - 60 && M(p).drag.y < ty);
    if (!dg) { const z = await T.props(); console.log('   clone at', M(z).drag, 'terminal', tl, 'pressed at', gx, gy, 'idea box', r0.box, 'map', leaf(z, 'panel:mindmap'), 'canvas', M(z).canvas, 'view', M(z).view, 'scroll', M(z).scroll, 'drawn', M(z).drawn?.includes(idea.id), 'last press', M(z).lastPress, 'hit there', await T.ui('mindmap-hit', String(gx), String(gy)), 'edit', M(z).edit, 'hover', M(z).hover, 'draft', M(z).draft, 'sel', M(z).sel); }
    await T.mouse('move', gx, gy, { buttons: ['left'] });
    await T.mouse('up', gx, gy);
    await T.until('let go where it was taken, nothing moves', (p) => !M(p).drag);
    await sleep(400);
    const r = M(await T.props()).nodes.find((x) => x.id === idea.id);
    await dropOnTerm('a Mindmap idea', r.box.x + Math.min(30, r.box.w / 2), r.box.y + r.box.h / 2, `[Mindmap Idea ID: ${idea.id}]`);
    await T.until('…and the idea in hand is gone from over the terminal', (p) => !M(p).drag, 3000);

    // Onto a chat's box (the owner, 22 Nov): the reference goes in at the
    // caret and the idea in hand goes with the release (the chat takes the
    // drop and stops the release there; it stayed floating over the box).
    await T.ui('new-chat');
    s = await T.until('a chat pane beside the map', (p) => p.chat && leaf(p, `live:${p.chat}`) && leaf(p, 'panel:mindmap'), 30000);
    if (s) {
      await T.ui('composer-text', '');
      await T.settle(400, 4000);
      s = await T.props();
      const cl = leaf(s, `live:${s.chat}`);
      const ri = M(s).nodes.find((x) => x.id === idea.id);
      const [gx, gy] = [ri.box.x + Math.min(30, ri.box.w / 2), ri.box.y + ri.box.h / 2];
      const [tx, ty] = [cl.x + cl.w / 2, cl.y + cl.h - 40];
      await T.mouse('move', gx, gy);
      await sleep(250);
      await T.mouse('down', gx, gy);
      for (let i = 1; i <= 14; i++) await T.mouse('move', gx + (tx - gx) * i / 14, gy + (ty - gy) * i / 14, { buttons: ['left'] });
      await T.until('over the chat\'s box, the idea\'s clone is there with the pointer', (p) => M(p).drag?.id === idea.id, 4000);
      await T.mouse('up', tx, ty);
      await T.until('dropped on the chat\'s box: "[Mindmap Idea ID: …]" goes in', (p) => (p.composer?.text || '').includes(`[Mindmap Idea ID: ${idea.id}]`), 6000);
      // (The map draws the idea in hand exactly while its drag is set.)
      if (await T.until('…and no idea is left in hand over the box', (p) => !M(p).drag, 3000)) await T.snap('planning', 'mindmap-drop-chat');
      await T.ui('composer-text', '');
      // (The chat's × arms "Sure?" first; the second closes it.)
      const ck = `live:${s.chat}`;
      await T.ui('workbench:pane-close', ck);
      await T.until('the chat\'s × arms "Sure?"', (p) => leaf(p, ck)?.armed, 4000);
      await T.ui('workbench:pane-close', ck);
      await T.until('…the second closes it', (p) => !p.panes?.includes(ck), 10000);
    }
  }
  await T.ui('workbench:pane-close', 'panel:mindmap');
  await T.until('the Mindmap pane closes', (p) => !p.panes?.includes('panel:mindmap'));

  // ── A Notes block: its grip, dragged ──
  await T.ui('add-pane', 'notes');
  s = await T.until('a Notes pane', (p) => p.panes?.some((k) => k.startsWith('panel:notes:')));
  const nk = s?.panes.find((k) => k.startsWith('panel:notes:'));
  await T.act('notes-pick', nk, page.id);
  const at = (p) => N(p)?.places?.find((x) => x.key === nk);
  s = await T.until('the page opens in the pane', (p) => at(p)?.mode === 'editor' && at(p).blocks[0]?.src === 'A **dropped** line' && at(p).boxes?.length);
  if (s) {
    const bid = at(s).blocks[0].id;
    const bx = at(s).boxes.find((x) => x.id === bid);
    await T.mouse('move', at(s).pageBox.x + at(s).pageBox.w / 2, bx.y + bx.h / 2);
    const sh = await T.until('the pointer on the block shows its grip', (p) => at(p).plus.some((x) => x.id === bid));
    const pl = sh && at(sh).plus.find((x) => x.id === bid);
    // The grip sits right of the "+" (each 20 px).
    // With the pointer on the grip, the block is still the hovered one (its
    // "+" and grip shown), and a frame has drawn them there.
    if (pl) await dropOnTerm('a Notes block', pl.x + 21, pl.y, 'A **dropped** line', (p) => at(p).plus.some((x) => x.id === bid));
    // A press on the grip with no frame drawn since the pointer came onto
    // the block, then moves at once (F258): the grip takes the press while
    // its gutter isn't shown yet (the web's .nt-gutter, opacity 0), and the
    // drag starts on the first move. The pointer comes from the terminal,
    // and the driver's "down" moves it there and presses in one go.
    const tl = leaf(await T.props(), key);
    if (pl && tl) {
      await T.ui('toasts-clear');
      await T.mouse('move', tl.x + tl.w / 2, tl.y + tl.h / 3);
      await sleep(250);
      const [gx, gy] = [pl.x + 21, pl.y];
      await T.mouse('down', gx, gy);
      for (const dx of [6, 12, 20]) await T.mouse('move', gx + dx, gy, { buttons: ['left'] });
      t.check('a press on a Notes block\'s grip, moved at once, starts its drag (F258)', N(await T.props())?.drag === true, N(await T.props())?.drag);
      const [tx, ty] = [tl.x + tl.w / 2, tl.y + tl.h / 3];
      for (let i = 1; i <= 8; i++) await T.mouse('move', gx + 20 + (tx - gx - 20) * i / 8, gy + (ty - gy) * i / 8, { buttons: ['left'] });
      await T.mouse('up', tx, ty);
      await T.until('…dropped on the terminal, it is typed there again, "Sent to terminal"', (q) => q.toast === 'Sent to terminal' && screen(q).includes('A **dropped** line'), 10000);
    }
  }

  // ── Dropped on a page's words, a reference is typed in there (F162: the
  // web's dropText: the caret follows the pointer, a space either side) ──
  await T.act('open-pane', 'kanban');
  await T.ui('tab', 'workbench');
  await T.until('a Kanban pane again', (p) => leaf(p, 'panel:kanban') && K(p)?.boxes?.cards?.length);
  for (let i = 0; i < 60 && !cardShown(await T.props()); i++) {
    const kl = leaf(await T.props(), 'panel:kanban');
    await T.mouse('wheel', kl.x + kl.w / 2, kl.y + kl.h / 2, { dy: -120 });
    await sleep(60);
  }
  s = await T.until('a Kanban pane beside the Notes pane', (p) => leaf(p, 'panel:kanban') && leaf(p, nk) && cardShown(p) && at(p)?.boxes?.length);
  if (s) {
    const bid = at(s).blocks[0].id;
    const n0 = at(s).blocks.length;
    const bx = at(s).boxes.find((x) => x.id === bid);
    await T.mouse('move', at(s).pageBox.x + at(s).pageBox.w / 2, bx.y + bx.h / 2);
    const sh = await T.until('the pointer on the page\'s block shows its "+"', (p) => at(p).plus.some((x) => x.id === bid));
    const pl = sh && at(sh).plus.find((x) => x.id === bid);
    const cb = K(sh).boxes.cards.find((x) => x.id === cardId);
    if (pl && cb) {
      const [fx, fy, tx, ty] = [cb.x + 30, cb.y + 14, pl.x + 40 + 2, bx.y + bx.h / 2];
      await T.ui('toasts-clear');
      await T.mouse('move', fx, fy);
      await sleep(250);
      await T.mouse('down', fx, fy);
      for (let i = 1; i <= 14; i++) await T.mouse('move', fx + (tx - fx) * i / 14, fy + (ty - fy) * i / 14, { buttons: ['left'] });
      await T.until('over the words, the drop has its spot in the block (the caret shows there)', (p) => N(p).dropSpot?.id === bid);
      await T.mouse('up', tx, ty);
      const want = `[Kanban Task ID: ${cardId}]`;
      const got = await T.until('dropped, it is typed into that block at the pointer, "Added to the note"', (p) => at(p).blocks.length === n0
        && at(p).blocks[0].src.includes(want) && !at(p).blocks[0].src.endsWith('line' + want) && p.toast === 'Added to the note' && !N(p).dropSpot);
      if (!got) { for (let i = 0; i < 6; i++) { const z = await T.props(); console.log('   page now', at(z).blocks.map((b) => b.src), 'toast', JSON.stringify(z.toast), N(z).dropSpot); await sleep(300); } }
    } else t.check('the card and the block to drop on are on show', false, { pl, cb });
  }

  // ── The workbench scrolls at its edges while a card is held (wbAutoScroll) ──
  for (const k of ['files', 'git', 'media', 'mindmap']) {
    const q = await T.props();
    const ww = q.window?.w ?? 1280;
    if ((q.leaves || []).some((l) => l.x + l.w > ww + 4)) break;
    await T.ui('add-pane', k);
    await sleep(300);
  }
  s = await T.props();
  const ww = s.window?.w ?? 1280;
  if ((s.leaves || []).some((l) => l.x + l.w > ww + 4) && leaf(s, 'panel:kanban')) {
    const kl = leaf(s, 'panel:kanban');
    // (The Kanban pane on screen, the grid at its start.)
    const cb = K(s).boxes.cards.find((x) => x.id === cardId);
    if (cb && cb.x > 0 && cb.x < ww - 80) {
      const x0 = kl.x;
      const [fx, fy] = [cb.x + 30, cb.y + 14];
      await T.mouse('move', fx, fy);
      await sleep(250);
      await T.mouse('down', fx, fy);
      const ex = s.window ? s.window.w - 12 : ww - 12;
      for (let i = 1; i <= 12; i++) await T.mouse('move', fx + (ex - fx) * i / 12, fy, { buttons: ['left'] });
      await sleep(900);
      const q = await T.props();
      t.check('a card held at the workbench\'s right edge scrolls the grid on', leaf(q, 'panel:kanban') && leaf(q, 'panel:kanban').x < x0 - 40, { x0, now: leaf(q, 'panel:kanban') });
      await T.mouse('move', fx, fy, { buttons: ['left'] });
      await T.mouse('up', fx, fy);
    } else t.check('the card is on show to hold', false, { cb, ww });
  } else console.log('   (the grid fits the window: no edge scroll to check)');
  for (const k of (await T.props()).panes || []) if (k !== key && k !== nk && !k.startsWith('pend:')) await T.ui('workbench:pane-close', k);

  // ── Edge auto-scroll: a block held near the page's bottom edge scrolls it ──
  const long = (await T.api.http('POST', '/api/notes', { project: 'demo', clientId: 'test-other', action: 'create', title: 'Long drop', blocks: Array.from({ length: 80 }, (_, i) => ({ type: 'p', text: `Row ${i + 1}` })) })).body.page;
  await T.act('notes-pages', nk);
  await T.until('the pane\'s picker again', (p) => at(p)?.mode === 'picker' && at(p).items.some((i) => i.id === long.id));
  await T.act('notes-pick', nk, long.id);
  s = await T.until('the long page in the pane', (p) => at(p)?.mode === 'editor' && at(p).blocks.length === 80 && at(p).boxes?.length > 3);
  if (s) {
    const b0 = at(s).blocks[1].id;
    const bx = at(s).boxes.find((x) => x.id === b0);
    await T.mouse('move', at(s).pageBox.x + at(s).pageBox.w / 2, bx.y + bx.h / 2);
    const sh = await T.until('the pointer on the second block shows its grip', (p) => at(p).plus.some((x) => x.id === b0));
    const pl = sh && at(sh).plus.find((x) => x.id === b0);
    if (pl) {
      const pb = at(sh).pageBox;
      const top0 = Math.min(...at(sh).boxes.map((x) => x.y));
      const shown0 = at(sh).boxes.map((x) => x.id).join();
      await T.mouse('down', pl.x + 21, pl.y);
      for (let i = 1; i <= 10; i++) await T.mouse('move', pl.x + 21, pl.y + (pb.y + pb.h - 20 - pl.y) * i / 10, { buttons: ['left'] });
      await sleep(900);
      const q = await T.props();
      t.check('held near the page\'s bottom edge, the page scrolls on by itself', at(q).boxes.map((x) => x.id).join() !== shown0 || Math.min(...at(q).boxes.map((x) => x.y)) < top0 - 40,
        { top0, now: at(q).boxes.slice(0, 3) });
      await T.mouse('up', pl.x + 21, pb.y + pb.h - 20);
    }
  }
  await T.ui('workbench:pane-close', nk);
  await T.ui('workspace:term-tool', key, 'ctrl+c');
}
