// A press on an idea after a pan (F260; run by test/planning.mjs): the
// Mind map in a workbench pane beside a terminal, as drops.mjs has it,
// panned 50 times by different amounts (the driver's pan, or a sweep of the
// wheel: the content built inline), pressed at once or 50, 150 or 300 ms
// after, then moved a little: each press must start its idea's drag. On a
// miss, the idea's box is logged against the map's own hit test there, the
// press the map took, its canvas and view.

import { saveView, restoreView } from './map-view.mjs';

const PA = (p) => p.areas?.planning ?? p.planningArea ?? {};
const M = (p) => PA(p).mindmap;
const W = (p) => p.workspace;
const ROUNDS = 50;

export async function run(T, t) {
  const sleep = t.sleep;
  // Polls without a check of its own (the loop's one check is at the end).
  const wait = async (test, ms) => { const end = Date.now() + ms; while (Date.now() < end) { const p = await T.props(); try { if (test(p)) return p; } catch { /* not yet */ } await sleep(30); } return null; };
  const mpost = (body) => T.api.http('POST', '/api/mindmap', body);
  // Enough ideas that a pan moves them a long way (a full run has more).
  const have = ((await T.api.http('GET', '/api/mindmap')).body?.mindmap?.nodes || []).length;
  if (have < 60) {
    const root = (await mpost({ action: 'addNode', text: 'Press root' })).body?.node?.id;
    for (let i = 0; i < 6; i++) {
      const k = (await mpost({ action: 'addNode', text: `Press branch ${i}`, parentId: root })).body?.node?.id;
      for (let j = 0; j < 9; j++) await mpost({ action: 'addNode', text: `Press leaf ${i}.${j}`, parentId: k });
    }
  }

  // A terminal pane, the map's pane beside it (drops.mjs's layout).
  await T.ui('tab', 'workbench');
  // (A terminal left open by the suite before, its program running, is kept.)
  for (const k of (await T.props()).panes || []) if (!k.startsWith('pend:') && !k.startsWith('term:')) await T.ui('workbench:pane-close', k);
  let s = await T.props();
  if (!s.panes?.some((k) => k.startsWith('term:'))) {
    await T.until('the workbench has its New terminal launcher', (p) => p.panes?.some((k) => k.startsWith('pend:')));
    await T.ui('workspace:launch', '', 'shell');
    s = await T.until('an Empty Shell in a pane', (p) => p.panes?.some((k) => k.startsWith('term:')), 30000);
  }
  const key = s?.panes.find((k) => k.startsWith('term:'));
  await T.act('open-pane', 'mindmap');
  await T.ui('tab', 'workbench');
  const leaf = (p, k) => p.leaves?.find((l) => l.key === k);
  s = await T.until('a Mindmap pane beside the terminal, with the ideas added while it was closed', (p) => leaf(p, 'panel:mindmap') && leaf(p, key) && M(p)?.state === 'ok' && M(p).nodes?.filter((n) => n.kind === 'node').length >= 50);
  if (!s) { const z = await T.props(); console.log('   panes', z.panes, 'leaves', JSON.stringify(z.leaves), 'map', M(z)?.state, M(z)?.nodes?.length, 'term', key); return; }
  await T.settle(400, 4000);
  // The map as a fresh one opens in this pane (zoomed out to 0.25, at its
  // start), whatever the screens before left it zoomed or scrolled to; given
  // back as it was found at the end.
  const view0 = await saveView(T);
  const START = { zoom: 0.25, scroll: [0, 0] };
  await restoreView(T, t, START, 'mindmap-press starts from a fresh map\'s view');
  await T.settle(200, 2000);

  // An idea whole on show, well inside the map's view and its pane.
  const pick = (p, i) => {
    const m = M(p), l = leaf(p, 'panel:mindmap');
    const [cx, cy, vw, vh] = [m.canvas[0], m.canvas[1], m.view[0], m.view[1]];
    const ok = m.nodes.filter((n) => n.kind === 'node' && n.box && m.drawn.includes(n.id) && n.box.w > 40
      && n.box.x > cx + 10 && n.box.x + n.box.w < cx + vw - 30 && n.box.y > cy + 10 && n.box.y + n.box.h < cy + vh - 30
      && n.box.x > l.x && n.box.x + n.box.w < l.x + l.w && n.box.y > l.y + 40 && n.box.y + n.box.h < l.y + l.h);
    return ok.length ? ok[i % ok.length] : null;
  };
  if (process.env.PRESS_DEBUG) {
    const z = await T.props(), m = M(z), l = leaf(z, 'panel:mindmap');
    const ns = m.nodes.filter((n) => n.kind === 'node');
    console.log('   DEBUG map', JSON.stringify({ nodes: ns.length, boxed: ns.filter((n) => n.box).length, drawn: m.drawn.length, canvas: m.canvas, view: m.view, scroll: m.scroll, zoom: m.zoom, leaf: l, sel: m.sel, hover: m.hover, edit: m.edit, collapsed: m.collapsed, sample: ns.filter((n) => n.box).slice(0, 5).map((n) => [n.text, n.box, m.drawn.includes(n.id)]) }));
  }
  const delays = [0, 50, 150, 300];
  const sizes = [30, 90, 160, 260, 420];
  let hits = 0, tries = 0;
  const misses = [];
  for (let i = 0; i < ROUNDS * 2 && tries < ROUNDS; i++) {
    const p0 = await T.props();
    const m0 = M(p0), ml = leaf(p0, 'panel:mindmap');
    // The pan: back toward the map's start when it has gone far, else
    // either way by one of the sizes; every third one a sweep of the wheel.
    const n = sizes[i % sizes.length], sx = m0.scroll[0], sy = m0.scroll[1];
    const dx = (sx > 300 ? -1 : sx < -100 ? 1 : (i % 2 ? 1 : -1)) * n * 0.6;
    const dy = (sy > 400 ? -1 : sy < -100 ? 1 : (i % 4 < 2 ? 1 : -1)) * n;
    const wheel = i % 3 === 2;
    if (wheel) {
      const [wx, wy] = [ml.x + ml.w - 30, ml.y + ml.h - 40];
      for (let k = 0; k < 6; k++) { await T.mouse('wheel', wx, wy, { dy: -Math.sign(dy) * Math.max(20, n / 6) }); await sleep(8); }
    } else await T.act('mindmap-pan', String(dx), String(dy));
    const d = delays[i % delays.length];
    const p1 = await T.props();
    const r = pick(p1, i);
    if (!r) continue;
    tries++;
    if (d) await sleep(d);
    // (Its box as the map places it now: a pan may still be landing.)
    const box = (M(await T.props()).nodes.find((x) => x.id === r.id) || r).box;
    const [x0, y0] = [box.x + Math.min(30, box.w / 2), box.y + box.h / 2];
    await T.mouse('move', x0, y0);
    await T.mouse('down', x0, y0);
    for (const ddx of [8, 14, 22]) await T.mouse('move', x0 + ddx, y0, { buttons: ['left'] });
    const ok = await wait((p) => M(p).drag?.id === r.id, 1500);
    if (ok) hits++;
    else {
      const z = await T.props(), m = M(z);
      const hit = await T.ui('mindmap-hit', String(x0), String(y0));
      misses.push({ press: tries, wheel, n, d, at: [x0, y0], box, hitNow: hit?.hit ?? hit, lastPress: m.lastPress, drag: m.drag, hover: m.hover, canvas: m.canvas, view: m.view, scroll: m.scroll, leaf: leaf(z, 'panel:mindmap'), drawn: m.drawn.includes(r.id) });
      console.log('   miss', JSON.stringify(misses[misses.length - 1]));
    }
    // Let go where it was taken: nothing moves.
    await T.mouse('move', x0, y0, { buttons: ['left'] });
    await T.mouse('up', x0, y0);
    await wait((p) => !M(p).drag, 3000);
    await sleep(60);
  }
  t.check(`a press on an idea after a pan starts its drag, ${ROUNDS} of ${ROUNDS} (F260)`, tries === ROUNDS && hits === ROUNDS, { tries, hits, misses });

  // A press on the idea under the pointer (drawn hovered: it carries the
  // shared drag layer's drag, for a chat or a terminal) and a flick off the
  // map in one go, no frame between (F260): the map's drag starts too (its
  // clone follows the pointer), and the release over the terminal is no
  // click on the idea (it opens no box to edit it).
  {
    await restoreView(T, t, START, 'the flicks start from a fresh map\'s view');
    await T.settle(200, 2000);
    let ok = 0, n = 0;
    const out = [];
    for (let i = 0; i < 10; i++) {
      const p0 = await T.props();
      const r = pick(p0, i), tl = leaf(p0, key);
      if (!r || !tl) continue;
      n++;
      const [x0, y0] = [r.box.x + Math.min(30, r.box.w / 2), r.box.y + r.box.h / 2];
      await T.mouse('move', x0, y0);
      await sleep(300);
      const d0 = M(await T.props()).drags;
      await T.mouse('drag', x0, y0, { toX: tl.x + tl.w / 2, toY: tl.y + tl.h / 3, steps: 12 });
      await sleep(400);
      const z = M(await T.props());
      if (z.drags > d0 && !z.drag && !z.edit) ok++;
      else out.push({ i, d0, drags: z.drags, drag: z.drag, edit: z.edit, lastPress: z.lastPress });
    }
    t.check('a press on an idea and a flick off the map at once start its drag, 10 of 10, and open no edit (F260)', n === 10 && ok === 10, { n, ok, out });
  }
  await restoreView(T, t, view0, 'mindmap-press');
  await T.ui('workbench:pane-close', 'panel:mindmap');
  await T.until('the Mindmap pane closes', (p) => !p.panes?.includes('panel:mindmap'));
}
