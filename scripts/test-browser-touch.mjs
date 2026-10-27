#!/usr/bin/env node
/**
 * test-browser-touch.mjs — browser_touch's gesture planner and its report.
 *
 * The backends only replay the steps planned here, so a wrong step is a wrong
 * gesture on every backend at once. And the summary is the part a person reads
 * to learn why their swipe did nothing, so the cases it was built for are
 * pinned: a parent cancelling touchmove, an overlay, touch-action beyond a
 * scroller (which does NOT apply), a pointer-event carousel losing the gesture.
 *
 * Run: node scripts/test-browser-touch.mjs
 */

import { planTouch, summarizeProbe, probeInstallScript, probeCollectScript } from '../src/browser-touch.js';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const has = (r, re) => r.summary.some((l) => re.test(l));

// ─── Planner ───
const tap = planTouch({ x: 10, y: 20 }).steps;
ok(tap.length === 2 && tap[0].type === 'touchStart' && tap[1].type === 'touchEnd', 'tap is start + end');
ok(tap[1].points.length === 0, 'touchEnd carries no points (CDP rule)');

const sw = planTouch({ action: 'swipe', x: 100, y: 500, direction: 'up', distance: 300 }).steps;
const last = sw[sw.length - 2];
ok(last.type === 'touchMove' && last.points[0].x === 100 && last.points[0].y === 200, 'swipe up moves the finger up by distance');
ok(sw.filter((s) => s.type === 'touchMove').length >= 2, 'swipe has intermediate moves');
const sw2 = planTouch({ action: 'swipe', x: 0, y: 0, toX: 50, toY: 60 }).steps;
ok(sw2[sw2.length - 2].points[0].x === 50 && sw2[sw2.length - 2].points[0].y === 60, 'swipe toX/toY ends there');

const pin = planTouch({ action: 'pinch', x: 200, y: 200, scale: 2 }).steps;
const span = (s) => s.points[1].x - s.points[0].x;
ok(pin[0].points.length === 2 && span(pin[pin.length - 2]) === 2 * span(pin[0]), 'pinch 2 doubles the finger span');
ok(planTouch({ action: 'long_press', x: 1, y: 1, duration: 99999 }).steps[0].wait === 10000, 'durations are capped');
ok(!!planTouch({ action: 'swipe', x: 1, y: 1 }).error, 'swipe without a target is an error');
ok(!!planTouch({ action: 'pinch', x: 1, y: 1 }).error, 'pinch without scale is an error');
ok(!!planTouch({ action: 'fling', x: 1, y: 1 }).error, 'unknown action is an error');
ok(!!planTouch({ action: 'tap' }).error, 'missing coordinates are an error');

// ─── Probe scripts are valid JS ───
for (const [n, src] of [['install', probeInstallScript(1, 2)], ['collect', probeCollectScript]]) {
  let good = true; try { new Function(src); } catch { good = false; }
  ok(good, `probe ${n} script parses`);
}

// ─── Summary ───
const el = (name, extra = {}) => ({ el: name, touchAction: 'auto', pointerEvents: 'auto', scrolls: '', scrollLeft: 0, scrollTop: 0, rect: [0, 0, 390, 200], transform: 'none', opacity: '1', ...extra });
const page = (path, afterPath, more = {}) => ({
  before: { path, scrollX: 0, scrollY: 0, zoom: 1 },
  after: { path: afterPath || path, scrollX: 0, scrollY: 0, zoom: 1 },
  calls: [], events: { touchstart: { fired: 1 } }, ...more,
});

let r = summarizeProbe({ stack: [{ el: 'div.card' }] },
  page([el('div.card'), el('div#drawer')], null, { calls: [{ what: 'preventDefault', event: 'touchmove', on: 'div#drawer', from: ['onMove (app.js:10:5)'], count: 9 }] }), []);
ok(has(r, /div#drawer cancelled touchmove .*app\.js:10:5/), 'names the element and line that cancelled touchmove');
ok(has(r, /nothing scrolled/), 'reports that nothing moved');

r = summarizeProbe({ stack: [{ el: 'div.toast', opacity: '0' }, { el: 'div.card' }] }, page([el('div.toast'), el('body')]), []);
ok(has(r, /div\.toast, which sits on top of div\.card .*transparent/), 'spots a transparent overlay');

r = summarizeProbe({ stack: [{ el: 'div.card' }] },
  page([el('div.card'), el('div#cards', { scrolls: 'x' }), el('div#app', { touchAction: 'pan-y' })],
    [el('div.card', { rect: [-390, 0, 390, 200] }), el('div#cards', { scrolls: 'x', scrollLeft: 390 }), el('div#app', { touchAction: 'pan-y' })]), []);
ok(!has(r, /touch-action/), 'touch-action beyond the nearest scroller is not blamed');
ok(has(r, /div#cards scrolled by 390,0/) && !has(r, /div\.card shifted/), 'credits the scroller, not the child riding along');

r = summarizeProbe({ stack: [{ el: 'div.slide' }] },
  page([el('div.slide'), el('div#deck')], null, { events: { touchstart: { fired: 1 }, pointercancel: { fired: 1 } } }),
  [{ el: 'div#deck', listeners: [{ type: 'pointermove' }] }]);
ok(has(r, /pointer handlers on div#deck stopped/), 'explains a pointer-event carousel losing the gesture');

r = summarizeProbe({ stack: [{ el: 'div' }] }, page([el('div')], null, { events: { touchstart: { fired: 1 }, pointercancel: { fired: 1 } } }), []);
ok(!has(r, /pointercancel/), 'pointercancel alone (no pointer listeners) is not reported');

console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
