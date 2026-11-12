/**
 * browser-touch.js — touch gestures for browser_touch, and the probe that says
 * what took them.
 *
 * A gesture is planned HERE and sent to the backend as a flat list of CDP
 * Input.dispatchTouchEvent steps, each with how long to wait after it. The
 * backends only replay steps, so a new gesture (or a tweak to an old one)
 * needs no desktop-app update: every app that can replay steps can do it.
 *
 * Touch emulation is switched on by the backend before the first step, so the
 * page sees a touch device (navigator.maxTouchPoints, 'ontouchstart' in
 * window). A page that sniffed for touch at load needs a reload after that.
 *
 * The probe exists because "the swipe did nothing" is the question that
 * matters, and a screenshot cannot answer it. Around every gesture it records
 * the element stack under the finger (an invisible overlay is a classic),
 * each element's touch-action and scrollability, every preventDefault /
 * stopPropagation / setPointerCapture made during the gesture together with
 * the script line that made it, the listeners attached along the path, and
 * what actually scrolled, moved or zoomed afterwards.
 */

const TOUCH_MAX_MS = 10_000;
const TOUCH_FRAME_MS = 16;
const TOUCH_ACTIONS = ['tap', 'double_tap', 'long_press', 'swipe', 'pinch'];

export function finite(n) { return typeof n === 'number' && Number.isFinite(n); }

/**
 * Plan a gesture. Returns { steps } or { error }.
 * A step is { type, points: [{ x, y, id }], wait } — points are every finger
 * still down (CDP wants all active points on a move, none on an end).
 */
export function planTouch({ action, x, y, toX, toY, direction, distance, duration, scale } = {}) {
  action = action || 'tap';
  if (!TOUCH_ACTIONS.includes(action)) return { error: `Unknown touch action "${action}". Use one of: ${TOUCH_ACTIONS.join(', ')}.` };
  if (!finite(x) || !finite(y)) return { error: 'x and y (viewport CSS pixels) are required.' };
  const ms = (v, d) => Math.min(Math.max(finite(v) ? v : d, 0), TOUCH_MAX_MS);
  const p = (px, py, id = 0) => ({ x: Math.round(px), y: Math.round(py), id });
  const steps = [];
  const tap = (wait) => {
    steps.push({ type: 'touchStart', points: [p(x, y)], wait: 50 });
    steps.push({ type: 'touchEnd', points: [], wait });
  };

  if (action === 'tap') tap(0);
  else if (action === 'double_tap') { tap(100); tap(0); }
  else if (action === 'long_press') {
    steps.push({ type: 'touchStart', points: [p(x, y)], wait: ms(duration, 800) });
    steps.push({ type: 'touchEnd', points: [], wait: 0 });
  } else if (action === 'swipe') {
    let ex = toX, ey = toY;
    if (!finite(ex) || !finite(ey)) {
      // direction is where the FINGER travels: "up" scrolls the content down.
      const d = finite(distance) ? distance : 300;
      const v = { up: [0, -d], down: [0, d], left: [-d, 0], right: [d, 0] }[direction];
      if (!v) return { error: 'swipe needs toX/toY, or direction ("up", "down", "left", "right") with an optional distance.' };
      ex = x + v[0]; ey = y + v[1];
    }
    const total = ms(duration, 300);
    const n = Math.max(2, Math.round(total / TOUCH_FRAME_MS));
    steps.push({ type: 'touchStart', points: [p(x, y)], wait: TOUCH_FRAME_MS });
    for (let i = 1; i <= n; i++) {
      const t = i / n;
      steps.push({ type: 'touchMove', points: [p(x + (ex - x) * t, y + (ey - y) * t)], wait: TOUCH_FRAME_MS });
    }
    // Lifted straight off the last move, so a quick swipe flings like a thumb
    // and a slow one (longer duration) ends as a drag.
    steps.push({ type: 'touchEnd', points: [], wait: 0 });
  } else if (action === 'pinch') {
    if (!finite(scale) || scale <= 0) return { error: 'pinch needs scale > 0 (2 zooms in to double, 0.5 zooms out to half).' };
    // Two fingers either side of (x, y), horizontally, from 100px apart to 100*scale.
    const start = scale >= 1 ? 50 : 50 / scale;
    const end = start * scale;
    const total = ms(duration, 400);
    const n = Math.max(2, Math.round(total / TOUCH_FRAME_MS));
    const pair = (half) => [p(x - half, y, 0), p(x + half, y, 1)];
    steps.push({ type: 'touchStart', points: pair(start), wait: TOUCH_FRAME_MS });
    for (let i = 1; i <= n; i++) {
      steps.push({ type: 'touchMove', points: pair(start + (end - start) * (i / n)), wait: TOUCH_FRAME_MS });
    }
    steps.push({ type: 'touchEnd', points: [], wait: 0 });
  }
  return { steps };
}


// ─── Probe (runs in the page) ───
//
// Written as real functions and shipped with .toString(), so they are linted
// and readable like any other code. They must stay self-contained: no closure
// over anything in this module.

function pageProbeInstall(x, y) {
  var old = window.__crundiTouch;
  if (old && old.restore) old.restore();
  var TYPES = /^(touch|pointer|mouse|click|dblclick|contextmenu|wheel|gesture|lostpointer|gotpointer)/;
  var S = window.__crundiTouch = { x: x, y: y, calls: {}, events: {}, restore: null };

  function label(el) {
    if (!el) return null;
    if (el === window) return 'window';
    if (el === document) return 'document';
    if (!el.tagName) return String(el.nodeName || '?');
    var s = el.tagName.toLowerCase();
    if (el.id) s += '#' + el.id;
    var c = typeof el.className === 'string' ? el.className.trim() : '';
    if (c) s += '.' + c.split(/\s+/).slice(0, 3).join('.');
    return s;
  }
  // The page frame that made the call: skip Error, this function and the patch.
  function caller() {
    return String(new Error().stack || '').split('\n').slice(3)
      .map(function (l) { return l.trim().replace(/^at /, ''); })
      .filter(function (l) { return l && l.indexOf('crundiTouch') === -1; })
      .slice(0, 2).map(function (l) { return l.length > 200 ? l.slice(0, 140) + '…' + l.slice(-50) : l; });
  }
  function note(what, e, extra) {
    if (!e || !TYPES.test(e.type)) return;
    var from = caller();
    var k = what + '|' + e.type + '|' + label(e.currentTarget) + '|' + (from[0] || '');
    var c = S.calls[k];
    if (!c) {
      c = S.calls[k] = { what: what, event: e.type, on: label(e.currentTarget), target: label(e.target), from: from, count: 0 };
      if (extra) for (var p in extra) c[p] = extra[p];
    }
    c.count++;
    return c;
  }

  var EP = Event.prototype, ElP = Element.prototype;
  var orig = { pd: EP.preventDefault, sp: EP.stopPropagation, sip: EP.stopImmediatePropagation, spc: ElP.setPointerCapture };
  EP.preventDefault = function crundiTouchPD() {
    var was = this.defaultPrevented;
    var r = orig.pd.apply(this, arguments);
    var c = note('preventDefault', this);
    // Called, but the browser ignored it: a passive listener, or a touchmove the
    // browser had already turned into a scroll (cancelable false).
    if (c && !was && !this.defaultPrevented) c.ignored = this.cancelable ? 'passive listener' : 'not cancelable (scroll already started)';
    return r;
  };
  EP.stopPropagation = function crundiTouchSP() { note('stopPropagation', this); return orig.sp.apply(this, arguments); };
  EP.stopImmediatePropagation = function crundiTouchSIP() { note('stopImmediatePropagation', this); return orig.sip.apply(this, arguments); };
  ElP.setPointerCapture = function crundiTouchSPC(id) {
    var from = caller();
    var k = 'setPointerCapture|' + label(this) + '|' + (from[0] || '');
    if (!S.calls[k]) S.calls[k] = { what: 'setPointerCapture', event: 'pointer', on: label(this), from: from, count: 0 };
    S.calls[k].count++;
    return orig.spc.apply(this, arguments);
  };

  var names = ['touchstart', 'touchmove', 'touchend', 'touchcancel', 'pointerdown', 'pointermove', 'pointerup',
    'pointercancel', 'gotpointercapture', 'click', 'dblclick', 'contextmenu'];
  function ev(t) { return S.events[t] || (S.events[t] = { fired: 0, reachedWindow: 0, prevented: 0, target: null }); }
  var cap = function crundiTouchCap(e) { var r = ev(e.type); r.fired++; if (!r.target) r.target = label(e.target); };
  var bub = function crundiTouchBub(e) { var r = ev(e.type); r.reachedWindow++; if (e.defaultPrevented) r.prevented++; };
  names.forEach(function (t) { window.addEventListener(t, cap, true); window.addEventListener(t, bub, false); });

  S.restore = function () {
    EP.preventDefault = orig.pd; EP.stopPropagation = orig.sp; EP.stopImmediatePropagation = orig.sip; ElP.setPointerCapture = orig.spc;
    names.forEach(function (t) { window.removeEventListener(t, cap, true); window.removeEventListener(t, bub, false); });
  };

  // The element stack under the finger, top first, and the ancestor path of
  // the top one — which is where the gesture is delivered and bubbles.
  var stack = (document.elementsFromPoint(x, y) || []).slice(0, 6);
  var path = [];
  for (var el = stack[0]; el; el = el.parentElement || (el.getRootNode && el.getRootNode().host) || null) path.push(el);
  S.path = path;
  function snap(el) {
    var cs = getComputedStyle(el), r = el.getBoundingClientRect();
    var sx = /(auto|scroll)/.test(cs.overflowX) && el.scrollWidth > el.clientWidth;
    var sy = /(auto|scroll)/.test(cs.overflowY) && el.scrollHeight > el.clientHeight;
    return { el: label(el), touchAction: cs.touchAction, pointerEvents: cs.pointerEvents,
      scrolls: (sx ? 'x' : '') + (sy ? 'y' : ''), scrollLeft: el.scrollLeft, scrollTop: el.scrollTop,
      rect: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)], transform: cs.transform,
      opacity: cs.opacity };
  }
  S.before = { path: path.map(snap), scrollX: window.scrollX, scrollY: window.scrollY,
    zoom: window.visualViewport ? window.visualViewport.scale : 1 };
  S.snap = snap;
  return JSON.stringify({
    stack: stack.map(function (e) { var s = snap(e); return { el: s.el, pointerEvents: s.pointerEvents, opacity: s.opacity, rect: s.rect }; }),
  });
}

function pageProbeCollect() {
  var S = window.__crundiTouch;
  if (!S) return JSON.stringify({ error: 'probe lost (the page navigated during the gesture)' });
  S.restore();
  var after = { path: S.path.map(function (el) { return el.isConnected ? S.snap(el) : null; }),
    scrollX: window.scrollX, scrollY: window.scrollY, zoom: window.visualViewport ? window.visualViewport.scale : 1 };
  var calls = Object.keys(S.calls).map(function (k) { return S.calls[k]; });
  var out = { before: S.before, after: after, calls: calls, events: S.events };
  delete window.__crundiTouch;
  return JSON.stringify(out);
}

export function probeInstallScript(x, y) {
  return `(${pageProbeInstall})(${Math.round(x)}, ${Math.round(y)})`;
}
export const probeCollectScript = `(${pageProbeCollect})()`;

// ─── Summary ───

const GESTURE_EVENTS = /^(touchstart|touchmove|pointerdown|pointermove)$/;

/**
 * Turn the raw probe into the few lines that answer "what took it?", most
 * telling first, keeping the raw detail alongside for when they do not.
 */
export function summarizeProbe(install, collect, listeners) {
  const lines = [];
  const stack = install?.stack || [];
  const before = collect.before || { path: [] };
  const after = collect.after || { path: [] };
  const path = before.path || [];

  if (stack.length) {
    const top = stack[0];
    let l = `Finger landed on ${top.el}`;
    if (stack[1] && path[1] && stack[1].el !== path[1].el) l += `, which sits on top of ${stack[1].el} (not its parent: an overlay?)`;
    if (top.opacity === '0') l += ' — it is fully transparent';
    lines.push(l + '.');
  } else lines.push('Nothing under that point (off the page?).');

  for (const c of collect.calls || []) {
    const at = c.from?.[0] ? ` — ${c.from[0]}` : '';
    const n = c.count > 1 ? ` (x${c.count})` : '';
    if (c.what === 'preventDefault' && c.ignored) {
      lines.push(`${c.on} called preventDefault on ${c.event}${n} but it was ignored: ${c.ignored}${at}`);
    } else if (c.what === 'preventDefault' && GESTURE_EVENTS.test(c.event)) {
      lines.push(`${c.on} cancelled ${c.event}${n} (preventDefault), so the browser will not scroll or zoom for it${at}`);
    } else if (c.what === 'setPointerCapture') {
      lines.push(`${c.on} captured the pointer (setPointerCapture): every later move goes to it${at}`);
    } else if (c.what !== 'preventDefault') {
      lines.push(`${c.on} called ${c.what} on ${c.event}${n}: listeners above it never saw it${at}`);
    }
  }

  // touch-action counts from the target up to the nearest scroll container
  // (spec), so one on an ancestor beyond a scroller does not apply.
  const scroller = path.findIndex((p) => p.scrolls);
  const inScope = scroller === -1 ? path : path.slice(0, scroller + 1);
  const blocked = inScope.filter((p) => p.touchAction && p.touchAction !== 'auto' && p.touchAction !== 'manipulation');
  for (const p of blocked.slice(0, 3)) lines.push(`${p.el} has touch-action: ${p.touchAction} — the browser only pans/zooms as that allows${scroller !== -1 ? ` (inside scroller ${path[scroller].el})` : ''}.`);
  const none = path.find((p) => p.pointerEvents === 'none');
  if (none) lines.push(`${none.el} has pointer-events: none (touches pass through it).`);

  const ev = collect.events || {};
  // pointercancel is normal when the browser scrolls; it only matters to
  // someone listening for pointer moves, which is a JS swipe handler.
  const pointerFolk = (listeners || []).filter((x) => (x.listeners || []).some((l) => /^pointer(move|down)$/.test(l.type)))
    .map((x) => x.el);
  if (ev.pointercancel?.fired && pointerFolk.length) {
    lines.push(`The browser took the gesture for native scrolling (pointercancel), so the pointer handlers on ${pointerFolk.slice(0, 3).join(', ')} stopped getting moves. touch-action: none (or pan-y for a sideways swipe) on the swipeable element keeps it for them.`);
  }
  if (ev.touchcancel?.fired) lines.push('The touch was cancelled (touchcancel).');
  if (!ev.touchstart?.fired && stack.length) lines.push('The page never received touchstart.');

  // What moved. Scrolls and transform changes are the movers; a plain change
  // of position is usually just a child riding along, so only the outermost
  // one is reported, and only when nothing else explains it.
  const moved = [];
  let shifted = null;
  path.forEach((b, i) => {
    const a = after.path?.[i];
    if (!a) return;
    const dsx = a.scrollLeft - b.scrollLeft, dsy = a.scrollTop - b.scrollTop;
    if (dsx || dsy) moved.push(`${b.el} scrolled by ${dsx},${dsy}`);
    if (a.transform !== b.transform) moved.push(`${b.el} moved by its transform (${b.transform} -> ${a.transform})`);
    else if (a.rect[0] !== b.rect[0] || a.rect[1] !== b.rect[1]) shifted = `${b.el} shifted by ${a.rect[0] - b.rect[0]},${a.rect[1] - b.rect[1]}`;
  });
  const wx = after.scrollX - before.scrollX, wy = after.scrollY - before.scrollY;
  if (wx || wy) {
    // html/body scrolling IS the page scrolling: say it once.
    for (let i = moved.length - 1; i >= 0; i--) if (/^(html|body)\b[^ ]* scrolled/.test(moved[i])) moved.splice(i, 1);
    moved.push(`the page scrolled by ${wx},${wy}`);
  }
  if (Math.abs((after.zoom || 1) - (before.zoom || 1)) > 0.01) moved.push(`zoom went ${round2(before.zoom)} -> ${round2(after.zoom)}`);
  if (!moved.length && shifted) moved.push(shifted);
  lines.push(moved.length ? `Result: ${moved.slice(0, 3).join('; ')}.` : 'Result: nothing scrolled, moved or zoomed.');

  return {
    summary: lines,
    stack,
    path: path.map((p) => ({ el: p.el, touchAction: p.touchAction, scrolls: p.scrolls || undefined })).slice(0, 12),
    calls: collect.calls,
    events: collect.events,
    listeners,
  };
}

function round2(n) { return Math.round((n || 1) * 100) / 100; }
