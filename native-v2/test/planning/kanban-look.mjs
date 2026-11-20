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
  await T.ui('tab', 'kanban');
  await T.until('the cards show', (p) => K(p)?.columns?.find((c) => c.status === 'todo')?.cards.length >= 2);
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
}
