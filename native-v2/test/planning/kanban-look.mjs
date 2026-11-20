import { execFileSync } from 'node:child_process';
// A card at rest (its controls drawn as they look, without their handlers)
// is the same picture as a live card (run by test/planning.mjs; needs the
// snapshots: Xvfb with xwd and ImageMagick).
let T;
const K = (p) => p.planningArea?.kanban;
const kpost = (body) => T.api.http('POST', '/api/kanban', { project: 'demo', ...body });

export async function run(T_, t) {
  T = T_;
  // Cards with every control: subtasks (one done), media counts aside, a description.
  const a = (await kpost({ action: 'addTask', title: 'Look: a card with subtasks', status: 'todo' })).body;
  const id = a?.task?.id || a?.id;
  for (const x of ['First step', 'Second step that wraps onto another line']) await kpost({ action: 'addTodo', taskId: id, text: x });
  await kpost({ action: 'addTask', title: 'Look: a compact card', status: 'todo', description: 'With a description' });
  // A card with a Mindmap count (Brainstorm's link) and every subtask done.
  const c = (await kpost({ action: 'addTask', title: 'Look: done, in the Mindmap', status: 'todo' })).body;
  const cid = c?.task?.id || c?.id;
  const td = (await kpost({ action: 'addTodo', taskId: cid, text: 'Only step' })).body;
  const idea = (await T.api.http('POST', '/api/mindmap', { action: 'addNode', text: 'Look idea', project: 'demo', taskId: cid })).body?.node?.id;
  const board = (await T.api.http('GET', '/api/kanban?project=demo')).body?.board;
  const only = board?.tasks?.find((x) => x.id === cid)?.todos?.[0]?.id;
  if (only) await kpost({ action: 'updateTodo', taskId: cid, todoId: only, done: true });
  void td;
  await T.ui('tab', 'kanban');
  await T.until('the cards show', (p) => K(p)?.columns?.find((c) => c.status === 'todo')?.cards.length >= 3 && K(p).columns.flatMap((x) => x.cards).some((x) => x.mm > 0 && x.done === x.total && x.total > 0));
  const p = await T.state();
  const b = K(p).boxes.cards.find((x) => x.id === id);
  const done = K(p).columns.flatMap((c) => c.cards).find((c) => c.id === id);
  if (done) await T.ui('kanban-toggle', id, done.todos[0]?.id ?? '', 'true');
  await t.sleep(800);
  // The pointer far from the board (on the sidebar).
  await T.mouse('move', 60, 600);
  // (A redraw with the pointer away: no card keeps a hover from before.)
  await T.ui('kanban-all-live', 'true');
  await t.sleep(300);
  await T.ui('kanban-all-live', 'false');
  await t.sleep(600);
  const rest = await T.snap('planning', 'kanban-look-rest');
  await T.ui('kanban-all-live', 'true');
  await t.sleep(600);
  const live = await T.snap('planning', 'kanban-look-live');
  await T.ui('kanban-all-live', 'false');
  await t.sleep(400); // the cards at rest drawn again (their controls reported) before the clicks below
  if (rest?.ok && live?.ok) {
    const col = K(await T.state()).boxes.cols.find((c) => c.status === 'todo');
    const crop = `${Math.round(col.w)}x${Math.round(col.h)}+${Math.round(col.x)}+${Math.round(col.y)}`;
    let n = T.diff(rest.path, live.path, crop);
    // (No ImageMagick: Python's PIL, if there.)
    if (n < 0) {
      const py = 'import sys;from PIL import Image,ImageChops;w,h,x,y=map(int,sys.argv[3].replace("+","x").split("x"));a=Image.open(sys.argv[1]).convert("RGB").crop((x,y,x+w,y+h));b=Image.open(sys.argv[2]).convert("RGB").crop((x,y,x+w,y+h));d=ImageChops.difference(a,b).convert("L");print(sum(1 for v in d.getdata() if v))';
      try { n = Number(execFileSync('python3', ['-I', '-c', py, rest.path, live.path, crop]).toString().trim()); } catch { /* none */ }
    }
    t.check('a card at rest looks as a live one (pixels differing in the column)', n === 0, { n, crop, card: b });
  } else console.log('(no snapshots: skipped the picture check)');
  // A click on a card at rest's control (no move first: the pointer's
  // press and release, as right after a scroll): it does what the live one does.
  const ctl = async (name) => (K(await T.state())?.boxes?.controls || []).find((c) => c.id === id && c.control.startsWith(name));
  const menuBtn = await ctl('menu');
  if (menuBtn) {
    const [mx, my] = [menuBtn.x + menuBtn.w / 2, menuBtn.y + menuBtn.h / 2];
    await T.mouse('down', mx, my); await T.mouse('up', mx, my);
    await T.until('a click on a card at rest opens its menu', (p) => K(p).columns.flatMap((c) => c.cards).find((c) => c.id === id)?.menuOpen);
    await T.ui('kanban-act', 'card-menu', id);
  } else t.check('the card at rest reports its controls', false);
  await T.mouse('move', 60, 600);
  await t.sleep(400);
  const tick = (K(await T.state())?.boxes?.controls || []).find((c) => c.id === id && c.control.startsWith('tick:') && c.control.endsWith(':false'));
  if (tick) {
    await T.mouse('down', tick.x + tick.w / 2, tick.y + tick.h / 2); await T.mouse('up', tick.x + tick.w / 2, tick.y + tick.h / 2);
    await T.until('a click on its tick ticks the subtask', (p) => K(p).columns.flatMap((c) => c.cards).find((c) => c.id === id)?.done === 2);
  }
  // Leave the board and the map as they were (the next suites start empty).
  if (idea) await T.api.http('POST', '/api/mindmap', { action: 'deleteNode', id: idea });
  const left = (await T.api.http('GET', '/api/kanban?project=demo')).body?.board?.tasks || [];
  for (const x of left.filter((x) => x.title.startsWith('Look:'))) await kpost({ action: 'deleteTask', taskId: x.id });
}
