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
    else t.check('the card\'s subtask row is on show', false, { todo, controls: K(s).boxes.controls.filter((c) => c.id === cardId) });
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
    const r = M(s).nodes.find((x) => x.id === idea.id);
    await dropOnTerm('a Mindmap idea', r.box.x + Math.min(30, r.box.w / 2), r.box.y + r.box.h / 2, `[Mindmap Idea ID: ${idea.id}]`);
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
  }

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
