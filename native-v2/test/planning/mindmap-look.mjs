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
  // F202: Cru's highlight finds an idea's words drawn at rest (idea_el) and its chip's,
  // and rings them inside the idea.
  {
    const D = (p) => p.areas?.desktop || {};
    await T.mouse('move', 60, 600);
    // (A big map, left by the suites before: panned to the idea first.)
    const view = (p) => { const [cx, cy] = M(p).canvas, [vw, vh] = M(p).view; return { x: cx, y: cy, w: vw, h: vh }; };
    const shown = (p, text) => { const v = view(p), b = M(p).nodes.find((x) => x.text === text)?.box; return b && b.x >= v.x && b.y >= v.y && b.x + b.w <= v.x + v.w && b.y + b.h <= v.y + v.h; };
    const showIdea = async (text) => {
      const q0 = await T.state();
      if (shown(q0, text)) return;
      const v = view(q0), b = M(q0).nodes.find((x) => x.text === text).box;
      const [dx, dy] = [b.x - (v.x + v.w / 3), b.y - (v.y + v.h / 2)];
      await T.act('mindmap-pan', String(dx), String(dy));
      if (!shown(await T.state(), text)) await T.act('mindmap-pan', String(-2 * dx), String(-2 * dy));
      await T.until(`"${text}" on show to find`, (p) => shown(p, text), 4000);
    };
    await showIdea('Scoped to the project');
    await t.sleep(400);
    await T.act('cru-ui-go', JSON.stringify({ id: 'ml1', tab: 'mindmap', highlight: 'Scoped to the project', note: 'This idea' }));
    const q = await T.until('Cru\'s highlight finds an idea at rest by its words (F202)', (p) => D(p).cru?.lastAck?.id === 'ml1' && D(p).cru.lastAck.found === true, 8000);
    if (q) {
      const r = D(q).cruSpot?.ring?.at, n = M(q).nodes.find((x) => x.text === 'Scoped to the project')?.box;
      t.check('…the ring is on its words, inside the idea', !!(r && n && r.x >= n.x - 1 && r.y >= n.y - 1 && r.x + r.w <= n.x + n.w + 1 && r.h < n.h), { r, n });
    }
    await t.sleep(500);
    await T.mouse('click', 60, 600);
    await T.until('…a press: the ring goes', (p) => !D(p).cruSpot?.ring, 4000);
    await showIdea('With notes');
    await T.mouse('move', 60, 600);
    await t.sleep(400);
    await T.act('cru-ui-go', JSON.stringify({ id: 'ml2', tab: 'mindmap', highlight: '2 notes', note: 'Its notes' }));
    await T.until('…and a chip\'s words (F202)', (p) => D(p).cru?.lastAck?.id === 'ml2' && D(p).cru.lastAck.found === true, 8000);
    await t.sleep(500);
    await T.mouse('click', 60, 600);
    await T.until('…the ring goes', (p) => !D(p).cruSpot?.ring, 4000);
  }
}
