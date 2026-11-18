// A wheel or touchpad scroll inside a workbench pane stays in the pane: the
// workbench's own sideways scroll moved with the mind map, a log, a list
// (the owner, 19 Nov). Over a pane's head, the workbench still scrolls.
// (One pane of each kind: the mind map, kanban, notes, git, files.)

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Poll the window's props (the workbench's are at the top) until test(p). */
async function until(T, t, what, test, ms) {
  const end = Date.now() + ms;
  let p;
  while (Date.now() < end) { p = await T.props(); try { if (test(p)) { t.check(what, true); return p; } } catch { /* not yet */ } await sleep(200); }
  t.check(what, false, { scrollX: p?.scrollX, gridW: p?.gridW, areaW: p?.areaW, panes: p?.panes?.map((x) => x.key) });
  return null;
}

export async function run(T, t) {
  await T.ui('tab', 'workbench');
  for (const k of ['mindmap', 'kanban', 'notes', 'git', 'files']) { await T.ui('add-pane', k); await sleep(300); }
  let p = await until(T, t, 'the panes make the workbench wider than the room', (p) => p.gridW > p.areaW + 50 && p.panes.some((x) => x.key.startsWith('panel:mindmap')), 15000);
  if (!p) return;
  const pane = p.panes.find((x) => x.key.startsWith('panel:mindmap'));
  const [ox, oy] = p.origin;
  const x = ox + pane.x - p.scrollX + pane.w / 2;
  const before = p.scrollX;
  // A touchpad's sideways swipe in the pane's body.
  for (let i = 0; i < 5; i++) { await T.mouse('wheel', x, oy + pane.y + pane.h / 2, { dx: -60, dy: 4 }); await sleep(60); }
  await sleep(300);
  p = await T.props();
  t.check('a sideways swipe in a pane leaves the workbench where it was', Math.abs(p.scrollX - before) < 1, { before, after: p.scrollX });
  // The same swipe over the pane's head scrolls the workbench.
  for (let i = 0; i < 5; i++) { await T.mouse('wheel', x, oy + pane.y + 12, { dx: -60 }); await sleep(60); }
  await until(T, t, 'over the pane head, the swipe scrolls the workbench', (p) => p.scrollX > before + 20, 5000);
  for (const x of (await T.props()).panes.filter((x) => x.key.startsWith('panel:'))) await T.ui('pane-close', String(x.id));
}
