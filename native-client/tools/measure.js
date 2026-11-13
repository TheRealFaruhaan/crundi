// Paste into browser_eval on the web UI page (or run in its devtools): the
// computed style and box of each element you name, its text's box, and its
// ::before / ::after when they draw something. Edit SEL for the screen.
// The numbers are the spec: copy them, do not eyeball a screenshot.
(() => {
  const SEL = {
    // name: 'css selector'   (fill in for the screen being matched)
    badge: '#conn-badge', topbar: '#topbar', tab: '.tab-btn.active', pane: '.term-cell.focused',
  };
  const KEEP = ['padding','margin','gap','fontFamily','fontSize','fontWeight','letterSpacing','lineHeight','color',
    'backgroundColor','backgroundImage','borderTop','borderBottom','borderLeft','borderRight','borderRadius','boxShadow',
    'width','height','textTransform','opacity'];
  const style = (el, p) => { const cs = getComputedStyle(el, p); const o = {};
    for (const k of KEEP) { const v = cs[k]; if (v && !['none','normal','auto','0px','rgba(0, 0, 0, 0)'].includes(v) && !(k.startsWith('border') && v.startsWith('0px'))) o[k] = k === 'fontFamily' ? v.split(',')[0] : v; }
    return o; };
  const box = (el) => { const r = el.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map((n) => Math.round(n * 10) / 10); };
  const text = (el) => { const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT); let n; while ((n = w.nextNode()) && !n.textContent.trim()); if (!n) return null;
    const r = document.createRange(); r.selectNodeContents(n); return box(r); };
  const out = {};
  for (const [k, s] of Object.entries(SEL)) {
    const el = document.querySelector(s); if (!el) { out[k] = 'MISSING'; continue; }
    out[k] = { box: box(el), text: text(el), style: style(el) };
    for (const p of ['::before', '::after']) { const st = style(el, p); if (st.backgroundColor || st.backgroundImage || st.borderTop) out[k][p] = st; }
  }
  return JSON.stringify(out);
})()
