// An idea at rest (drawn by the map's one element, idea_el.rs) is the same
// picture as its div (run by test/planning.mjs; needs the snapshots).
import { execFileSync } from 'node:child_process';

let T;
const M = (p) => p.planningArea?.mindmap;
const mpost = async (body) => (await T.api.http('POST', '/api/mindmap', body)).body;

export async function run(T_, t) {
  T = T_;
  const task = (await T.api.http('POST', '/api/kanban', { project: 'demo', action: 'addTask', title: 'Look task', status: 'todo' })).body;
  const taskId = task?.task?.id || task?.id;
  const root = (await mpost({ action: 'addNode', text: 'Look: a root idea' }))?.node?.id;
  await mpost({ action: 'addNode', text: 'A long idea whose words go on well past two lines of the card, so they are cut with an ellipsis', parentId: root });
  const scoped = (await mpost({ action: 'addNode', text: 'Scoped to the project' }))?.node?.id;
  await mpost({ action: 'scopeProject', id: scoped, project: 'demo' });
  await mpost({ action: 'addNode', text: 'Inherits the project', parentId: scoped });
  await mpost({ action: 'addNode', text: 'Linked to a task', project: 'demo', taskId });
  const noted = (await mpost({ action: 'addNode', text: 'With notes', parentId: root }))?.node?.id;
  await mpost({ action: 'updateNode', id: noted, notes: ['one', 'two'] });
  const folded = (await mpost({ action: 'addNode', text: 'Folded', parentId: root }))?.node?.id;
  await mpost({ action: 'addNode', text: 'Hidden under it', parentId: folded });
  await T.ui('tab', 'mindmap');
  await T.until('the look ideas show', (p) => M(p)?.state === 'ok' && M(p).nodes.some((n) => n.text === 'Inherits the project' && n.chip === 'inherited'));
  await T.ui('mindmap-zoom', '1');
  await T.ui('mindmap-act', 'toggle', folded);
  await t.sleep(500);
  await T.mouse('move', 60, 600);
  await T.ui('mindmap-all-div', 'true');
  await t.sleep(300);
  await T.ui('mindmap-all-div', 'false');
  await t.sleep(600);
  const rest = await T.snap('planning', 'mindmap-look-rest');
  await T.ui('mindmap-all-div', 'true');
  await t.sleep(600);
  const divs = await T.snap('planning', 'mindmap-look-div');
  await T.ui('mindmap-all-div', 'false');
  const p = await T.state();
  t.check('the map draws its ideas at rest as one element (none left as divs)', M(p).drawn.length > 5, M(p).drawn.length);
  if (rest?.ok && divs?.ok) {
    const [cx, cy] = M(p).canvas, [vw, vh] = M(p).view;
    const crop = `${Math.round(vw)}x${Math.round(vh)}+${Math.round(cx)}+${Math.round(cy)}`;
    let n = T.diff(rest.path, divs.path, crop);
    if (n < 0) {
      const py = 'import sys;from PIL import Image,ImageChops;w,h,x,y=map(int,sys.argv[3].replace("+","x").split("x"));a=Image.open(sys.argv[1]).convert("RGB").crop((x,y,x+w,y+h));b=Image.open(sys.argv[2]).convert("RGB").crop((x,y,x+w,y+h));d=ImageChops.difference(a,b).convert("L");print(sum(1 for v in d.getdata() if v))';
      try { n = Number(execFileSync('python3', ['-I', '-c', py, rest.path, divs.path, crop], { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim()); } catch { /* none */ }
    }
    t.check('an idea at rest looks as its div (pixels differing on the map)', n === 0, { n, crop });
  } else console.log('(no snapshots: skipped the picture check)');
}
