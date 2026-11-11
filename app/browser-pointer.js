/**
 * browser-pointer.js — browser_mouse for the desktop app's browsers (the
 * hidden windows and the drawer's tabs), as real mouse input.
 *
 * Events go through the tab's debugger (CDP Input.dispatchMouseEvent), not
 * webContents.sendInputEvent: CDP input is hit-tested by the browser like a
 * real mouse, so a click lands in a cross-site <iframe> (CodePen's preview, a
 * payment form) where sendInputEvent's never arrived, and its coordinates are
 * the page's CSS pixels at any device emulation scale.
 *
 * The pointer remembers where it is and which button is held, per tab, so a
 * chat can press, move while holding, and release in separate calls, or do it
 * in one with "drag".
 *
 * show(p), when given (a drawer tab), draws the chat's cursor over the page
 * before each step: { x, y, pressed, ms, linear, click }. It resolves when the
 * cursor has arrived (ms later), so what the person sees and what the page
 * gets stay in step. It must never throw: a web UI without the cursor simply
 * resolves at once.
 *
 * Pure apart from the two callbacks: no Electron import, so the drawer's e2e
 * harness and app/main.js run the same code.
 */

export const MOUSE_ACTIONS = ['move', 'click', 'double_click', 'down', 'up', 'drag'];
const BUTTONS = { left: 1, right: 2, middle: 4 };
const MAX_MS = 10000;

const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : (typeof v === 'string' && v.trim() !== '' && Number.isFinite(Number(v)) ? Number(v) : null));
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const sleep = (ms) => (ms > 0 ? new Promise((r) => setTimeout(r, ms)) : Promise.resolve());

/** How long the visible cursor takes to travel from one point to another. */
export function glideMs(from, to) {
  if (!from || from.x == null || from.y == null) return 0;
  const d = Math.hypot(to.x - from.x, to.y - from.y);
  if (d < 1) return 0;
  return Math.round(clamp(140 + d * 0.45, 140, 480));
}

/** Check a browser_mouse request; returns an error message, or null. */
export function mouseError(msg) {
  const action = msg.action || 'click';
  if (!MOUSE_ACTIONS.includes(action)) return `action must be one of ${MOUSE_ACTIONS.join(', ')}`;
  if (msg.button != null && !BUTTONS[msg.button]) return 'button must be "left", "right" or "middle"';
  if (action !== 'up' && (num(msg.x) == null || num(msg.y) == null)) return `${action} needs x and y`;
  if (action === 'up' && (num(msg.x) == null) !== (num(msg.y) == null)) return 'up takes both x and y, or neither';
  if (action === 'drag' && (num(msg.toX) == null || num(msg.toY) == null)) return 'drag needs toX and toY (where to release)';
  if (msg.duration != null && (num(msg.duration) == null || num(msg.duration) < 0)) return 'duration must be a number of milliseconds';
  return null;
}

/**
 * Run one browser_mouse request.
 *   send(method, params) → Promise   the tab's CDP
 *   state                            per tab, kept by the caller: { x, y, down }
 *   show(p) → Promise | null         the visible cursor (drawer tabs only)
 * Returns { x, y, held } — where the pointer is now and the button still
 * held, if any.
 */
export async function runMouse({ send, state, show = null, msg }) {
  const err = mouseError(msg);
  if (err) throw new Error(err);
  const action = msg.action || 'click';
  const button = msg.button || 'left';
  const draw = async (p) => { if (show) { try { await show(p); } catch { /* the cursor is only a picture */ } } };
  const ev = (type, x, y, extra) => send('Input.dispatchMouseEvent', { type, x, y, ...extra });
  const held = () => (state.down ? { button: state.down, buttons: BUTTONS[state.down] } : { button: 'none', buttons: 0 });

  // Travel to (tx, ty). With a button held, the page gets the whole path
  // (a drag needs the movement, not just the end), in step with the cursor.
  async function travel(tx, ty, ms) {
    const from = state.x == null ? { x: tx, y: ty } : { x: state.x, y: state.y };
    if (ms == null) ms = glideMs(state.x == null ? null : from, { x: tx, y: ty });
    ms = clamp(Math.round(ms), 0, MAX_MS);
    if (!state.down) {
      await draw({ x: tx, y: ty, pressed: false, ms });
      await ev('mouseMoved', tx, ty, held());
    } else {
      const n = clamp(Math.round(ms / 16), 2, 120);
      const cursor = draw({ x: tx, y: ty, pressed: true, ms, linear: true });
      for (let i = 1; i <= n; i++) {
        const px = from.x + ((tx - from.x) * i) / n, py = from.y + ((ty - from.y) * i) / n;
        await ev('mouseMoved', px, py, held());
        await sleep(ms / n);
      }
      await cursor;
    }
    state.x = tx; state.y = ty;
  }
  async function press(clickCount = 1) {
    state.down = button;
    await draw({ x: state.x, y: state.y, pressed: true, ms: 0, click: true });
    await ev('mousePressed', state.x, state.y, { button, buttons: BUTTONS[button], clickCount });
  }
  async function release(clickCount = 1) {
    const b = state.down || button;
    state.down = null;
    await ev('mouseReleased', state.x, state.y, { button: b, buttons: 0, clickCount });
    await draw({ x: state.x, y: state.y, pressed: false, ms: 0 });
  }

  const x = num(msg.x), y = num(msg.y);
  const dur = num(msg.duration);
  switch (action) {
    case 'move':
      await travel(x, y, dur);
      break;
    case 'click':
    case 'double_click':
      if (state.down) await release();
      await travel(x, y, dur);
      await press(1); await release(1);
      if (action === 'double_click') { await sleep(60); await press(2); await release(2); }
      break;
    case 'down':
      if (state.down) await release();
      await travel(x, y, dur);
      await press(1);
      break;
    case 'up':
      if (x != null) await travel(x, y, dur);
      if (state.x == null) throw new Error('up needs x and y: the pointer has not been anywhere yet');
      if (state.down) await release();
      break;
    case 'drag': {
      if (state.down) await release();
      await travel(x, y, null);
      await press(1);
      await travel(num(msg.toX), num(msg.toY), dur == null ? 500 : dur);
      await release(1);
      break;
    }
  }
  return { x: state.x, y: state.y, held: state.down || null };
}
