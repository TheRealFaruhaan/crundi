import { join } from 'node:path';
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
    const opened = await T.until('a click on a card at rest opens its menu', (p) => K(p).columns.flatMap((c) => c.cards).find((c) => c.id === id)?.menuOpen);
    if (!opened) { const q = K(await T.state()); console.log('  (menu click:', JSON.stringify({ menuBtn, held: q.held, drag: q.drag, live: q.live, hot: q.hot, cards: q.columns.flatMap((c) => c.cards).map((c) => [c.title, c.menuOpen]), controls: q.boxes.controls.filter((c) => c.id === id) }), ')'); }
    await T.ui('kanban-act', 'card-menu', id);
  } else t.check('the card at rest reports its controls', false);
  await T.mouse('move', 60, 600);
  await t.sleep(400);
  const tick = (K(await T.state())?.boxes?.controls || []).find((c) => c.id === id && c.control.startsWith('tick:') && c.control.endsWith(':false'));
  if (tick) {
    await T.mouse('down', tick.x + tick.w / 2, tick.y + tick.h / 2); await T.mouse('up', tick.x + tick.w / 2, tick.y + tick.h / 2);
    await T.until('a click on its tick ticks the subtask', (p) => K(p).columns.flatMap((c) => c.cards).find((c) => c.id === id)?.done === 2);
  }
  // F215: a card's footer menu hovered keeps its 1 px edge (the web's #3c3c58), and open,
  // its accent edge while hovered (the open rule outranks the hover one).
  {
    await T.mouse('move', 60, 600);
    await t.sleep(400);
    const m = await ctl('menu');
    const edgeAt = async (file) => {
      if (!m) return null;
      const s = await T.snap('planning', file);
      // The left edge's middle (the tip covers the top one) and a pixel inside, as [r, g, b];
      // with no snapshot (no ImageMagick), the virtual screen read by PIL, the window's
      // place on it found where the black around it ends.
      const x = Math.round(m.x), y = Math.round(m.y + m.h / 2);
      const py = [
        'import sys,os',
        'from PIL import Image,ImageGrab',
        'f,x,y=sys.argv[1],int(sys.argv[2]),int(sys.argv[3])',
        'i=(Image.open(f) if f else ImageGrab.grab(xdisplay=os.environ.get("DISPLAY"))).convert("RGB")',
        'ox=oy=0',
        'if not f:',
        ' i.save(sys.argv[4]);W,H=i.size',
        ' ox=next((k for k in range(W//2) if i.getpixel((k,H//2))!=(0,0,0)),0)',
        ' oy=next((k for k in range(H//2) if i.getpixel((W//2,k))!=(0,0,0)),0)',
        'print(*i.getpixel((x+ox,y+oy)),*i.getpixel((x+ox+3,y+oy)))',
      ].join('\n');
      try { const v = execFileSync('python3', ['-I', '-c', py, s?.ok ? s.path : '', String(x), String(y), join(T.OUT, `${file}.grab.png`)], { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim().split(/\s+/).map(Number); return { edge: v.slice(0, 3), inside: v.slice(3), at: [x, y] }; } catch { return null; }
    };
    const near = (a, b, tol = 14) => a && a.every((v, i) => Math.abs(v - b[i]) <= tol);
    if (m) {
      await T.mouse('move', m.x + m.w / 2, m.y + m.h / 2);
      await t.sleep(500);
      const h = await edgeAt('kanban-look-menu-hover');
      if (h) t.check('a hovered footer menu keeps its 1 px edge (F215)', near(h.edge, [0x3c, 0x3c, 0x58]) && near(h.inside, [0x1a, 0x1a, 0x28]), h);
      else console.log('(no snapshot reading: skipped the hover edge check)');
      await T.ui('kanban-act', 'card-menu', id);
      await T.until('…its menu opens', (p) => K(p).columns.flatMap((c) => c.cards).find((c) => c.id === id)?.menuOpen);
      await T.mouse('move', m.x + m.w / 2 + 1, m.y + m.h / 2);
      await t.sleep(500);
      const o = await edgeAt('kanban-look-menu-open-hover');
      if (o) t.check('…open and hovered, it keeps its accent edge and tint (F215)', near(o.edge, [0x63, 0x66, 0xf1]) && !near(o.inside, [0x1a, 0x1a, 0x28], 4), o);
      await T.ui('kanban-act', 'card-menu', id);
      await T.until('…and shuts', (p) => !K(p).columns.flatMap((c) => c.cards).find((c) => c.id === id)?.menuOpen);
    } else t.check('the card at rest reports its menu', false);
    await T.mouse('move', 60, 600);
    await t.sleep(400);
  }
  // F202: Cru's highlight finds the words on a card at rest (its title, drawn by card_el)
  // and on a live one, and rings them, not the whole card.
  {
    const D = (p) => p.areas?.desktop || {};
    const inCard = (p, title) => {
      const r = D(p).cruSpot?.ring?.at, cd = K(p).columns.flatMap((c) => c.cards).find((c) => c.title === title);
      const cb = cd && K(p).boxes.cards.find((x) => x.id === cd.id);
      return { ring: r, card: cb, ok: !!(r && cb && r.x >= cb.x - 1 && r.y >= cb.y - 1 && r.x + r.w <= cb.x + cb.w + 1 && r.y + r.h <= cb.y + cb.h + 1 && r.h < cb.h - 8) };
    };
    for (const [n, live] of [['kl1', false], ['kl2', true]]) {
      await T.ui('kanban-all-live', live ? 'true' : 'false');
      await t.sleep(500);
      await T.act('cru-ui-go', JSON.stringify({ id: n, tab: 'kanban', highlight: 'Look: a compact card', note: 'This card' }));
      const q = await T.until(`Cru's highlight finds a card ${live ? 'live' : 'at rest'} by its title (F202)`, (p) => D(p).cru?.lastAck?.id === n && D(p).cru.lastAck.found === true, 8000);
      if (q) { const v = inCard(q, 'Look: a compact card'); t.check('…the ring is on the title, inside the card', v.ok, v); }
      // The next press takes the ring away (once it is armed, 300 ms; on the sidebar's
      // empty foot: nothing under it).
      await t.sleep(500);
      await T.mouse('click', 60, 600);
      await T.until('…a press: the ring goes', (p) => !D(p).cruSpot?.ring, 4000);
    }
    await T.ui('kanban-all-live', 'false');
    const st = (await T.state());
    const sub = K(st).columns.flatMap((c) => c.cards).find((c) => c.id === id)?.todos?.[0]?.text;
    if (sub) {
      await T.act('cru-ui-go', JSON.stringify({ id: 'kl3', tab: 'kanban', highlight: sub, note: 'This step' }));
      await T.until('…and a subtask\'s words on a card at rest (F202)', (p) => D(p).cru?.lastAck?.id === 'kl3' && D(p).cru.lastAck.found === true, 8000);
      await t.sleep(500);
      await T.mouse('click', 60, 600);
      await T.until('…the ring goes', (p) => !D(p).cruSpot?.ring, 4000);
    }
  }
  // Leave the board and the map as they were (the next suites start empty).
  if (idea) await T.api.http('POST', '/api/mindmap', { action: 'deleteNode', id: idea });
  const left = (await T.api.http('GET', '/api/kanban?project=demo')).body?.board?.tasks || [];
  for (const x of left.filter((x) => x.title.startsWith('Look:'))) await kpost({ action: 'deleteTask', taskId: x.id });
}
