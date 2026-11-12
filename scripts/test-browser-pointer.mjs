#!/usr/bin/env node
// app/browser-pointer.js: browser_mouse as real input (CDP mouse events), with
// the chat's cursor drawn before each step in a drawer tab. A fake CDP and a
// fake cursor record what would happen. The real thing, in Electron with a
// cross-site iframe, is in scripts/test-browser-drawer.mjs.
//
// Run: node scripts/test-browser-pointer.mjs

import { runMouse, mouseError, glideMs, MOUSE_ACTIONS } from '../app/browser-pointer.js';
import { headlessMouse } from '../src/browser-headless.js';

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${ok ? '' : '  ' + String(extra).slice(0, 400)}`); if (!ok) failed++; };

function rig({ cursor = true } = {}) {
  const log = [];           // everything, in order: cdp events and cursor draws
  const state = {};
  const send = async (m, p) => { log.push({ cdp: m, ...p }); };
  const show = cursor ? async (p) => { log.push({ cursor: true, ...p }); } : null;
  const run = (msg) => runMouse({ send, state, show, msg });
  const events = () => log.filter((e) => e.cdp);
  const draws = () => log.filter((e) => e.cursor);
  return { log, state, run, events, draws };
}
const types = (evs) => evs.map((e) => e.type).join(',');

// ── Validation ──
check('every action is known', MOUSE_ACTIONS.join() === 'move,click,double_click,down,up,drag');
check('an unknown action is refused', /action must be one of/.test(mouseError({ action: 'hover', x: 1, y: 1 })));
check('click needs x and y', /needs x and y/.test(mouseError({ action: 'click', x: 1 })));
check('drag needs toX and toY', /toX and toY/.test(mouseError({ action: 'drag', x: 1, y: 1 })));
check('up may have no point', mouseError({ action: 'up' }) === null);
check('up with half a point is refused', /both x and y/.test(mouseError({ action: 'up', x: 3 })));
check('a bad button is refused', /button/.test(mouseError({ action: 'click', x: 1, y: 1, button: 'side' })));
check('numbers given as strings are fine', mouseError({ action: 'click', x: '10', y: '20' }) === null);
{
  const r = rig();
  let threw = null; try { await r.run({ action: 'up' }); } catch (e) { threw = e.message; }
  check('up before the pointer has been anywhere says so', /has not been anywhere/.test(threw || ''), threw);
}

// ── Glide time ──
check('no glide the first time (nowhere to come from)', glideMs(null, { x: 10, y: 10 }) === 0);
check('no glide for no distance', glideMs({ x: 5, y: 5 }, { x: 5, y: 5 }) === 0);
check('a short hop is quick, a long one slower but capped', glideMs({ x: 0, y: 0 }, { x: 20, y: 0 }) < glideMs({ x: 0, y: 0 }, { x: 400, y: 0 }) && glideMs({ x: 0, y: 0 }, { x: 5000, y: 0 }) === 480);

// ── click ──
{
  const r = rig();
  const out = await r.run({ action: 'click', x: 100, y: 50 });
  const ev = r.events();
  check('click: move, press, release, in that order', types(ev) === 'mouseMoved,mousePressed,mouseReleased', types(ev));
  check('click: at the point, left button, one click', ev.every((e) => e.x === 100 && e.y === 50) && ev[1].button === 'left' && ev[1].clickCount === 1 && ev[1].buttons === 1 && ev[2].buttons === 0);
  check('click: through CDP Input.dispatchMouseEvent', ev.every((e) => e.cdp === 'Input.dispatchMouseEvent'));
  check('click: returns where the pointer is, nothing held', out.x === 100 && out.y === 50 && out.held === null, JSON.stringify(out));
  const firstDraw = r.log.findIndex((e) => e.cursor), firstEv = r.log.findIndex((e) => e.cdp);
  check('the cursor is drawn before the page gets the event', firstDraw >= 0 && firstDraw < firstEv);
  check('the press shows a ripple, the release lifts the ring', r.draws().some((d) => d.click && d.pressed) && r.draws().at(-1).pressed === false);
  await r.run({ action: 'click', x: 400, y: 50 });
  const glide = r.draws().find((d) => d.x === 400 && d.ms > 0);
  check('the next click glides there from where the pointer was', !!glide && glide.ms === glideMs({ x: 100, y: 50 }, { x: 400, y: 50 }), JSON.stringify(r.draws()));
}
{
  const r = rig();
  await r.run({ action: 'click', x: 10, y: 10, button: 'right' });
  const ev = r.events();
  check('right click: button right, buttons 2', ev[1].button === 'right' && ev[1].buttons === 2 && ev[2].button === 'right');
}
{
  const r = rig();
  await r.run({ action: 'double_click', x: 30, y: 40 });
  const ev = r.events().filter((e) => e.type !== 'mouseMoved');
  check('double_click: two presses, the second with clickCount 2', types(ev) === 'mousePressed,mouseReleased,mousePressed,mouseReleased' && ev[2].clickCount === 2 && ev[3].clickCount === 2, JSON.stringify(ev));
}

// ── move (hover) ──
{
  const r = rig();
  await r.run({ action: 'move', x: 5, y: 6 });
  const ev = r.events();
  check('move: one mouseMoved with no button', types(ev) === 'mouseMoved' && ev[0].button === 'none' && ev[0].buttons === 0);
  check('move: nothing held afterwards', r.state.down == null && r.state.x === 5 && r.state.y === 6);
}

// ── down, move while held, up (separate calls) ──
{
  const r = rig();
  const d = await r.run({ action: 'down', x: 100, y: 100 });
  check('down: pressed and still held', d.held === 'left' && types(r.events()) === 'mouseMoved,mousePressed', JSON.stringify(d));
  check('down: the cursor shows the press', r.draws().at(-1).pressed === true);
  const before = r.events().length;
  await r.run({ action: 'move', x: 200, y: 100, duration: 160 });
  const path = r.events().slice(before);
  check('a move while held sends the whole path, with the button down', path.length >= 2 && path.every((e) => e.type === 'mouseMoved' && e.buttons === 1 && e.button === 'left'), types(path));
  check('the path goes step by step to the end', path.at(-1).x === 200 && path[0].x > 100 && path[0].x < 200, JSON.stringify(path.map((e) => e.x)));
  check('the cursor travels linearly while held, in one glide', r.draws().some((d2) => d2.linear && d2.pressed && d2.x === 200 && d2.ms === 160));
  const u = await r.run({ action: 'up' });
  const last = r.events().at(-1);
  check('up with no point releases where the pointer is', last.type === 'mouseReleased' && last.x === 200 && last.y === 100 && u.held === null);
}
{
  const r = rig();
  await r.run({ action: 'down', x: 10, y: 10 });
  await r.run({ action: 'click', x: 50, y: 50 });
  const ev = r.events().filter((e) => e.type !== 'mouseMoved');
  check('a click while a button is held releases it first', types(ev) === 'mousePressed,mouseReleased,mousePressed,mouseReleased' && ev[1].x === 10, types(ev));
}

// ── drag ──
{
  const r = rig();
  const out = await r.run({ action: 'drag', x: 10, y: 20, toX: 110, toY: 20, duration: 200 });
  const ev = r.events();
  const iP = ev.findIndex((e) => e.type === 'mousePressed'), iR = ev.findIndex((e) => e.type === 'mouseReleased');
  const between = ev.slice(iP + 1, iR);
  check('drag: press at the start, release at the end', ev[iP].x === 10 && ev[iR].x === 110 && ev[iR].y === 20, JSON.stringify(ev));
  check('drag: held moves in between, enough of them', between.length >= 10 && between.every((e) => e.type === 'mouseMoved' && e.buttons === 1), between.length);
  check('drag: nothing held afterwards', out.held === null);
}
{
  const r = rig();
  const t0 = Date.now();
  await r.run({ action: 'drag', x: 0, y: 0, toX: 50, toY: 0 });
  const took = Date.now() - t0;
  check('drag takes about its default 500 ms, so the page sees a real movement', took >= 400 && took < 1500, took);
}

// ── Without a cursor (hidden browsers), and a cursor that fails ──
{
  const r = rig({ cursor: false });
  await r.run({ action: 'click', x: 1, y: 2 });
  check('a hidden browser clicks the same way, with nothing drawn', types(r.events()) === 'mouseMoved,mousePressed,mouseReleased');
}
{
  const state = {}; const ev = [];
  await runMouse({ send: async (m, p) => { ev.push(p); }, state, show: async () => { throw new Error('page gone'); }, msg: { action: 'click', x: 3, y: 4 } });
  check('a cursor that fails never stops the click', ev.length === 3);
}

// ── The headless server browser: same actions ──
{
  const ev = []; const state = {};
  const send = async (m, p) => { ev.push(p); };
  await headlessMouse(send, state, { action: 'drag', x: 0, y: 0, toX: 40, toY: 0, duration: 32 });
  const iP = ev.findIndex((e) => e.type === 'mousePressed'), iR = ev.findIndex((e) => e.type === 'mouseReleased');
  check('headless: drag presses, moves held, releases at the end', iP >= 0 && iR > iP + 1 && ev[iR].x === 40 && ev.slice(iP + 1, iR).every((e) => e.buttons === 1), JSON.stringify(ev));
  ev.length = 0;
  await headlessMouse(send, state, { action: 'down', x: 5, y: 5 });
  await headlessMouse(send, state, { action: 'up', x: 9, y: 9 });
  const held = ev.slice(2, -1);
  check('headless: down then up at another point (held moves, then the release there)', types(ev.slice(0, 2)) === 'mouseMoved,mousePressed' && held.length >= 1 && held.every((e) => e.type === 'mouseMoved' && e.buttons === 1) && ev.at(-1).type === 'mouseReleased' && ev.at(-1).x === 9, types(ev));
  let threw = null; try { await headlessMouse(send, {}, { action: 'drag', x: 1, y: 1 }); } catch (e) { threw = e.message; }
  check('headless: drag without an end is refused', /toX and toY/.test(threw || ''), threw);
}

console.log(failed ? `\n${failed} failure(s).` : '\nAll passed.');
process.exit(failed ? 1 : 0);
