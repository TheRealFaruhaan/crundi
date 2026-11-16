// Planning 4.3 the Mind map (run by test/planning.mjs). Snapshots named mindmap-*.

let T;
const M = (p) => p.areas?.planning?.mindmap;
const node = (p, text) => M(p)?.nodes?.find((n) => n.kind === 'node' && n.text === text);
const mm = async () => (await T.api.http('GET', '/api/mindmap')).body?.mindmap?.nodes || [];
const mpost = async (body) => (await T.api.http('POST', '/api/mindmap', body)).body;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Wait until the server's map passes test(nodes). */
async function server(t, what, test, ms = 15000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    last = await mm();
    try { if (test(last)) { t.check(what, true); return last; } } catch { /* not yet */ }
    await sleep(250);
  }
  t.check(what, false, last);
  return null;
}

const centre = (n) => [n.box.x + n.box.w / 2, n.box.y + n.box.h / 2];

export async function run(T_, t) {
  T = T_;
  // ── The empty map ──
  await T.ui('tab', 'mindmap');
  await T.until('an empty map says how to start', (p) => M(p)?.state === 'empty');
  await T.snap('mindmap-empty');

  // ── Typing ideas in place: Tab nests, Ctrl+Enter is the next at its level, Enter saves and stops ──
  await T.act('mindmap-act', 'add-root');
  await T.until('+ Add idea opens a box on the canvas', (p) => M(p)?.state === 'ok' && M(p).draft?.on && !M(p).draft.parent);
  await T.key('Trip');
  await T.until('the words go in the box', (p) => M(p).draft?.text === 'Trip');
  await T.key('Tab');
  await T.until('Tab keeps it and opens one nested under it', (p) => node(p, 'Trip') && M(p).draft?.parent === node(p, 'Trip').id && M(p).draft.text === '');
  await T.key('Flights');
  await T.key('Return', ['Control']);
  await T.until('Ctrl+Enter keeps it and opens the next at its level', (p) => node(p, 'Flights') && M(p).draft?.after === node(p, 'Flights').id);
  await T.key('Hotel');
  await T.key('Return');
  const typed = await T.until('Enter keeps it and stops; the ring is on it', (p) => !M(p).draft && node(p, 'Hotel') && M(p).sel === node(p, 'Hotel').id && M(p).ring && M(p).saving === 0 && !node(p, 'Hotel').pending);
  await server(t, 'the server has the three, in order', (ns) => {
    const trip = ns.find((n) => n.text === 'Trip');
    const kids = ns.filter((n) => n.parentId === trip.id).map((n) => n.text);
    return kids.join() === 'Flights,Hotel';
  });
  if (typed) {
    const p = typed;
    t.check('columns by depth', node(p, 'Flights').x > node(p, 'Trip').x && node(p, 'Flights').x === node(p, 'Hotel').x, M(p).nodes);
    t.check('a parent sits between its children', node(p, 'Trip').y > node(p, 'Flights').y && node(p, 'Trip').y < node(p, 'Hotel').y);
    t.check('"add idea" boxes under each idea', M(p).nodes.filter((n) => n.kind === 'ghost').length === 3);
    t.check('links drawn', (M(p).edges.match(/C/g) || []).length === 2, M(p).edges);
  }

  // ── The keyboard: walk, fold, edit ──
  await T.key('Up', ['Control', 'Shift']);
  await T.until('Ctrl+Shift+Up: the idea above', (p) => M(p).sel === node(p, 'Flights')?.id);
  await T.key('Left', ['Control', 'Shift']);
  await T.until('Ctrl+Shift+Left: its parent', (p) => M(p).sel === node(p, 'Trip')?.id);
  await T.key('-');
  await T.until('− folds what is under it', (p) => !node(p, 'Flights') && node(p, 'Trip').collapsed);
  await T.key('+');
  await T.until('+ unfolds it', (p) => node(p, 'Flights') && !node(p, 'Trip').collapsed);
  await T.key('Return');
  await T.until('Enter edits it in place', (p) => M(p).edit?.on && M(p).edit.text === 'Trip');
  await T.key(' 2026');
  await T.key('Return');
  await server(t, 'Enter saves the edit', (ns) => ns.some((n) => n.text === 'Trip 2026'));
  await T.until('the ring stays on it', (p) => !M(p).edit && M(p).sel === node(p, 'Trip 2026')?.id);
  await T.key('Tab');
  await T.until('Tab on the ring: a new idea under it', (p) => M(p).draft?.parent === node(p, 'Trip 2026')?.id);
  await T.key('Visa');
  await T.key('Escape');
  await T.until('Escape closes it unsaved', (p) => !M(p).draft && !node(p, 'Visa'));

  // ── Projects, links, notes (from the server: live) ──
  const task = (await T.api.http('POST', '/api/kanban', { project: 'demo', action: 'addTask', title: 'Book flights', status: 'todo' })).body;
  const taskId = task?.task?.id || task?.id;
  const pack = await mpost({ action: 'addNode', text: 'Packing list', project: 'demo', taskId });
  await mpost({ action: 'addNode', text: 'Socks', parentId: pack?.node?.id });
  const hotel = (await mm()).find((n) => n.text === 'Hotel');
  await mpost({ action: 'updateNode', id: hotel.id, notes: ['Near the beach'] });
  const live = await T.until('changes from elsewhere arrive live', (p) => node(p, 'Packing list')?.linked && node(p, 'Socks')?.chip === 'inherited' && node(p, 'Hotel')?.notes === 1);
  if (live) {
    t.check('a linked idea shows its task', node(live, 'Packing list').chip === 'link' && node(live, 'Packing list').chipText === 'Book flights', node(live, 'Packing list'));
    t.check('the project bar: Demo, then General', M(live).bar && M(live).chips.map((c) => c.label).join() === 'Demo,General', M(live).chips);
  }
  await T.act('mindmap-select', 'Hotel');
  await T.snap('mindmap-map');

  // The filter lights up matches, dims the rest, hides the "add idea" boxes.
  await T.act('mindmap-search', 'beach');
  await T.until('Filter ideas: a note matches', (p) => node(p, 'Hotel').hit && node(p, 'Flights').dim && M(p).nodes.filter((n) => n.kind === 'ghost').every((n) => n.hidden));
  await T.act('mindmap-search', '');
  await T.until('cleared, all as before', (p) => !node(p, 'Flights').dim && !node(p, 'Hotel').hit);

  // A project chip off hides its ideas.
  await T.act('mindmap-act', 'chip', '');
  await T.until('General off: its ideas hide, the bar says so', (p) => !node(p, 'Hotel') && node(p, 'Packing list') && M(p).hiddenCount === 1 && M(p).chips.find((c) => c.key === '').off);
  await T.act('mindmap-act', 'chip-all');
  await T.until('Show all', (p) => node(p, 'Hotel') && M(p).hiddenCount === 0);

  // ── The idea dialog: a double click ──
  await T.act('mindmap-measure');
  let mp = await T.until('the canvas is measured', (p) => M(p).canvas[1] > 0 && node(p, 'Hotel')?.box);
  const [hx, hy] = centre(node(mp, 'Hotel'));
  await T.mouse('dblclick', hx, hy);
  await T.until('a double click opens its details', (p) => M(p).detail?.title === 'Hotel' && M(p).detail.notes.join() === 'Near the beach' && M(p).detail.linkLabel === 'Link / scope' && p.pointerModal);
  await T.snap('mindmap-detail');
  await T.act('mindmap-d-act', 'edit');
  await T.until('Edit: the words and the notes as boxes', (p) => M(p).detail?.editing && M(p).detail.text === 'Hotel' && M(p).detail.editNotes.join() === 'Near the beach');
  await T.act('mindmap-d-act', 'add-note');
  await T.act('mindmap-d-note', '1', 'Two towels');
  await T.act('mindmap-d-act', 'save');
  await server(t, 'Save keeps the notes', (ns) => ns.find((n) => n.text === 'Hotel')?.notes?.join('|') === 'Near the beach|Two towels');
  await T.until('and closes', (p) => !M(p).detail && node(p, 'Hotel').notes === 2 && !p.pointerModal);

  // Link / scope: the picker, scope to the project.
  await T.act('mindmap-detail', 'Hotel');
  await T.act('mindmap-d-act', 'link');
  await T.until('Link / scope opens the picker with the board', (p) => M(p).link && !M(p).link.loading && M(p).link.names.includes('Demo') && M(p).link.rows.some((r) => r.text === 'Book flights' && r.status === 'todo') && M(p).link.scope === 'Scope to “Demo” (no task)');
  await T.act('mindmap-l-search', 'zzz');
  await T.until('the search finds nothing', (p) => M(p).link.rows.length === 0);
  await T.act('mindmap-l-act', 'scope');
  await T.until('scoped to the project', (p) => !M(p).link && node(p, 'Hotel')?.scoped && node(p, 'Hotel').chip === 'scope');
  // A child keeps its parent's project: the picker is locked to it.
  await T.act('mindmap-detail', 'Socks');
  await T.act('mindmap-d-act', 'link');
  await T.until('a child is locked to its branch\'s project', (p) => M(p).link?.locked && M(p).link.lockedText.startsWith('Scoped to “Demo” (inherited from parent)'));
  await T.act('mindmap-l-act', 'pick', taskId, '');
  await T.until('linking a task', (p) => !M(p).link && node(p, 'Socks')?.linked);
  await T.act('mindmap-detail', 'Socks');
  await T.until('the dialog shows the link and offers Unlink', (p) => M(p).detail?.linkKind === 'link' && M(p).detail.linkMain === 'Kanban: demo · Book flights' && M(p).detail.linkLabel === 'Unlink');
  await T.act('mindmap-d-act', 'link');
  await T.until('Unlink', (p) => !node(p, 'Socks')?.linked && node(p, 'Socks')?.chip === 'inherited');

  // ── Drag to rearrange: Flights onto the middle of Hotel ──
  await T.act('mindmap-measure');
  mp = await T.until('measured again', (p) => node(p, 'Flights')?.box && node(p, 'Hotel')?.box);
  const [fx, fy] = centre(node(mp, 'Flights'));
  const [tx, ty] = centre(node(mp, 'Hotel'));
  await T.mouse('move', fx, fy);
  await T.mouse('down', fx, fy);
  // Out to the gap between the columns, down, then into the middle of
  // Hotel from the side (crossing its top edge would open a "drop before" gap).
  const gx = node(mp, 'Hotel').box.x - 25;
  const path = [[gx, fy], [gx, (fy + ty) / 2], [gx, ty], [gx + 30, ty], [tx, ty]];
  for (const [x, y] of path) { await T.mouse('move', x, y); await sleep(60); }
  const dg = await T.until('dragging over the middle of an idea outlines it', (p) => M(p).drag && node(p, 'Hotel')?.target && node(p, 'Flights')?.lifted);
  if (!dg) { const p = await T.props(); console.log('drag debug', JSON.stringify({ from: [fx, fy], to: [tx, ty], drag: M(p).drag, f: node(p, 'Flights'), h: node(p, 'Hotel'), canvas: M(p).canvas, scroll: M(p).scroll, zoom: M(p).zoom, hover: M(p).hover })); }
  await T.snap('mindmap-drag');
  await T.mouse('up', tx, ty);
  await server(t, 'dropped: it is under that idea now', (ns) => ns.find((n) => n.text === 'Flights')?.parentId === ns.find((n) => n.text === 'Hotel')?.id);

  // ── Delete, twice ──
  await T.act('mindmap-select', 'Flights');
  await T.until('the ring on it', (p) => M(p).sel === node(p, 'Flights')?.id);
  await T.key('\u007f');
  await T.until('the first Delete warns', (p) => M(p).ring?.warn && M(p).ring.msg === 'Press Delete again to delete');
  await T.key('\u007f');
  await server(t, 'the second deletes it', (ns) => !ns.some((n) => n.text === 'Flights'));

  // ── Zoom: Ctrl and +, Ctrl and the wheel ──
  const z0 = M(await T.props()).zoom;
  await T.key('=', ['Control']);
  await T.until('Ctrl and + zooms in', (p) => Math.abs(M(p).zoom - z0 * 1.2) < 0.01);
  await T.act('mindmap-wheel', '-120', '200', '200');
  await T.until('Ctrl and the wheel zoom out', (p) => M(p).zoom < z0 * 1.2 - 0.01);
  await T.act('mindmap-zoom', '1');

  // ── The shortcut sheet ──
  await T.act('mindmap-act', 'help');
  const hp = await T.until('Shortcuts opens the sheet', (p) => M(p).help?.rows.length > 30 && M(p).help.rows[0].head === 'ADDING IDEAS' && p.pointerModal);
  if (hp) {
    const i = M(hp).help.rows.findIndex((r) => r.dia);
    await T.act('mindmap-h-toggle', String(i));
    await T.until('a row opens to its picture', (p) => M(p).help.rows[i].open);
  }
  await T.snap('mindmap-help');
  await T.act('mindmap-h-act', 'all');
  await T.until('Show all pictures', (p) => M(p).help.all && M(p).help.rows.filter((r) => r.dia).every((r) => r.open));
  await T.act('mindmap-h-act', 'close');
  await T.until('closed', (p) => !M(p).help && !p.pointerModal);

  // ── A failed load says so ──
  await T.act('mindmap-load-path', '/api/mindmap-nope');
  await T.until('the error line', (p) => M(p).state === 'error' && M(p).error.startsWith('Failed to load mindmap: '));
  await T.act('mindmap-reload');
  await T.until('and back', (p) => M(p).state === 'ok' && node(p, 'Hotel'));

  // ── The panel in a workbench pane ──
  await T.ui('add-pane', 'mindmap');
  await T.until('a Mindmap pane', (p) => p.panes.some((x) => x.key === 'panel:mindmap'));
  await T.ui('tab', 'workbench');
  await T.until('the workbench shows it', (p) => p.currentTab === 'workbench' && M(p).state === 'ok');
  await T.snap('mindmap-panel');
  await T.ui('tab', 'mindmap');
}
