// The Mind map's view as a screen's checks found it (zoom, scroll), and
// putting it back when they end (FINDINGS-ELECTRON 4): a suite that zooms or
// pans to its ideas leaves the next one the map it was given, with no idea
// hovered or in hand.
const M = (p) => (p.areas?.planning ?? p.planningArea ?? {}).mindmap;

export async function saveView(T) {
  const m = M(await T.state());
  return m ? { zoom: m.zoom, scroll: [...m.scroll] } : null;
}

export async function restoreView(T, t, v, label) {
  if (!v) return;
  await T.mouse('move', 4, 600);
  if (Math.abs(M(await T.state()).zoom - v.zoom) > 0.001) await T.ui('mindmap-zoom', String(v.zoom));
  // A pan of (dx, dy) moves the scroll by that much one way or the other: the
  // way that brings it nearer, a few times (it is clamped at the map's edges).
  for (let i = 0; i < 4; i++) {
    const m = M(await T.state());
    const [dx, dy] = [m.scroll[0] - v.scroll[0], m.scroll[1] - v.scroll[1]];
    if (Math.abs(dx) < 1 && Math.abs(dy) < 1) break;
    await T.ui('mindmap-pan', String(dx), String(dy));
    const n = M(await T.state());
    if (Math.hypot(n.scroll[0] - v.scroll[0], n.scroll[1] - v.scroll[1]) > Math.hypot(dx, dy)) await T.ui('mindmap-pan', String(-2 * dx), String(-2 * dy));
  }
  const q = await T.until(`${label}: the map is left as it was found (zoom, scroll, nothing hovered or held)`, (p) => {
    const m = M(p);
    return m && Math.abs(m.zoom - v.zoom) < 0.001 && Math.abs(m.scroll[0] - v.scroll[0]) < 2 && Math.abs(m.scroll[1] - v.scroll[1]) < 2 && !m.hover && !m.drag;
  }, 4000);
  if (!q) { const m = M(await T.state()); console.log('   view now', { zoom: m.zoom, scroll: m.scroll, hover: m.hover, drag: m.drag }, 'was', v); }
  return q;
}
