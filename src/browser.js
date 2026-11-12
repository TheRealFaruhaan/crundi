/**
 * browser.js — Background browser instance manager.
 *
 * Manages browser instances (rendered as <webview> in the Electron UI) via IPC.
 * Claude uses MCP tools (in mcp-server.js) → this module → process.send() →
 * app/main.js (creates webview in renderer, controls via webContents API) → result.
 *
 * When running headless (no Electron), all operations return a graceful error.
 */

// ─── Constants ───

import { findScript, pointScript, selectScript, pickerScript } from './browser-find.js';
import * as headless from './browser-headless.js';
import { planTouch, probeInstallScript, probeCollectScript, summarizeProbe } from './browser-touch.js';
import { AsyncLocalStorage } from 'node:async_hooks';

const MAX_WIDTH = 1920;
const MAX_HEIGHT = 1080;
const MAX_PER_PROJECT = 5;
const MAX_TOTAL = 20;
const IDLE_TIMEOUT_MS = 10 * 60 * 1000; // 10 minutes
const MAX_CONSOLE_LOGS = 200;
const IPC_TIMEOUT_MS = 40_000; // backstop for a dead IPC channel; the main
                               // process applies its own (shorter) per-action
                               // timeout and normally replies well before this.

// ─── State ───

// key → BrowserInstance
const browsers = new Map();

// requestId → { resolve, reject, timer }
const pendingRequests = new Map();
let nextRequestId = 1;

/**
 * @typedef {Object} BrowserInstance
 * @property {string} key           — `${alias}:${name}`
 * @property {string} alias         — project alias
 * @property {string} name          — instance name
 * @property {string} url           — current URL
 * @property {number} width
 * @property {number} height
 * @property {Array<{time: number, level: string, text: string}>} consoleLogs
 * @property {'loading'|'ready'|'closed'} status
 * @property {number} createdAt
 * @property {number} lastActivityAt
 * @property {NodeJS.Timeout|null} idleTimer
 */

// ─── Native host bridge ───
//
// A browser panel has to be rendered by something with a GUI. Originally that
// was always our own parent: Electron spawned the server as a child process and
// we drove it over the Node IPC channel. A server in a container has no such
// parent, so the same requests now go to whichever NATIVE HOST is attached —
// either that parent, or a desktop app connected over the network that has
// registered itself as one.
//
// WHERE a browser runs is the caller's choice, per instance (openBrowser's
// `on`), because the two answers are different machines on different networks:
//
//   'server'   the machine this server runs on — our Electron parent in an
//              all-in-one install, else a headless browser here. What a dev
//              server on this machine's localhost needs.
//   'desktop'  the Crundi desktop app the person is using, attached over the
//              network (or our own parent, which is the same machine). Where
//              they can see it, on their network.
//   (omitted)  as before: a desktop app when one is attached, else headless.
//
// The choice is made once, at open, and every later call on that instance goes
// to the same place: the page only exists where it was opened.

let remoteHost = null;   // { send(msg) } registered by a connected desktop app
const keyBackend = new Map();   // instance key -> 'parent' | 'remote' | 'headless'

function isElectron() {
  return process.env.ELECTRON_RUN === '1';
}

function hasParentIpc() {
  return isElectron() && typeof process.send === 'function';
}

/** Attach a desktop app as this server's native host. Returns a detach fn. */
export function setRemoteHost(host) {
  remoteHost = host && typeof host.send === 'function' ? host : null;
  return () => { if (remoteHost === host) clearRemoteHost(); };
}

export function clearRemoteHost() {
  remoteHost = null;
  // Pages opened on that app went with it.
  let dropped = false;
  for (const [key, where] of keyBackend) {
    if (where !== 'remote') continue;
    keyBackend.delete(key);
    const inst = instFor(key);
    if (inst) { if (inst.idleTimer) clearTimeout(inst.idleTimer); browsers.delete(key); dropped = true; }
  }
  if (dropped) emitBrowserUpdate();
  // Anything still in flight can never be answered now — fail it rather than
  // leaving callers hanging until the 40s backstop.
  for (const [id, entry] of pendingRequests) {
    clearTimeout(entry.timer);
    entry.reject(new Error('The desktop app disconnected'));
    pendingRequests.delete(id);
  }
}

/** The device the drawer's app runs on (its homepool node's deviceId), if known. */
export function drawerDeviceId() {
  if (hasParentIpc()) return process.env.CRUNDI_DEVICE_ID || '';
  return (remoteHost && remoteHost.deviceId) || '';
}

// The bundled server's end of the drawer's tunnel: with an Electron parent the
// frames come over IPC (base64), not a websocket. Set by webapp.js.
let parentTunnel = null;
export function setParentTunnel(endpoint) { parentTunnel = endpoint; }
export function sendParentTunnel(buf) { if (hasParentIpc()) process.send({ type: 'netTunnel', b64: buf.toString('base64') }); }

export function hasNativeHost() {
  return hasParentIpc() || !!remoteHost;
}

/**
 * Does the attached app support a given feature?
 *
 * An older client is attached and answering browser calls quite happily, but
 * silently drops anything it does not know - so "a host is attached" is not the
 * same question as "this will arrive". A remote client announces what it can do
 * when it registers; the parent process in an all-in-one install is the same
 * build as this server, so it can be taken at its word.
 */
export function hostSupports(feature) {
  if (hasParentIpc()) return true;
  return !!(remoteHost && Array.isArray(remoteHost.capabilities)
    && remoteHost.capabilities.indexOf(feature) !== -1);
}

/**
 * Fire-and-forget message to whichever desktop app is attached: the parent
 * process in an all-in-one install, or a remote client over the websocket.
 *
 * Not a request - there is no requestId and no reply. Returns false when no
 * app is attached, so a caller can report that rather than pretending it sent.
 */
export function notifyHost(payload) {
  return postToHost({ type: 'notify', ...payload });
}

/** Deliver a reply from a remote host. Shares the parent-IPC handler. */
export function handleHostMessage(msg) {
  handleIpcMessage(msg);
}

/**
 * Is anything able to drive a browser — a desktop host, or a local headless
 * one? This replaced a bare isElectron() check on every single operation, which
 * is what made the tools refuse to work on a server even though nothing they do
 * requires a display.
 */
function browserBackendReady() {
  return hasNativeHost() || headless.isSupported();
}

function backendUnavailableReason() {
  return 'No browser available: connect the Crundi desktop app, or re-run the installer to provision a headless browser.';
}

function postToHost(msg) {
  if (hasParentIpc()) { process.send(msg); return true; }
  if (remoteHost) { remoteHost.send(msg); return true; }
  return false;
}

/** Which places a browser can run right now, for the UI and browser_list. */
export function browserPlaces() {
  return {
    server: hasParentIpc() || headless.isSupported(),
    desktop: !!remoteHost || hasParentIpc(),
  };
}

/**
 * Turn a caller's `on` into a backend, or explain why it cannot be had.
 * Returns { backend } or { error }.
 */
function chooseBackend(on) {
  const where = String(on || '').toLowerCase();
  if (where === 'server') {
    if (hasParentIpc()) return { backend: 'parent' };
    if (headless.isSupported()) return { backend: 'headless' };
    return { error: 'This server has no browser of its own. Re-run the installer to provision a headless one, or use on: "desktop".' };
  }
  if (where === 'desktop') {
    if (remoteHost) return { backend: 'remote' };
    if (hasParentIpc()) return { backend: 'parent' };
    return { error: 'No Crundi desktop app is connected to this server. Open the desktop app, or use on: "server".' };
  }
  if (where) return { error: 'on must be "server", "desktop" or "drawer"' };
  if (hasParentIpc()) return { backend: 'parent' };
  if (remoteHost) return { backend: 'remote' };
  if (headless.isSupported()) return { backend: 'headless' };
  return { error: backendUnavailableReason() };
}

function sendIpc(type, payload, timeoutMs = IPC_TIMEOUT_MS, backendOverride = null) {
  // Each instance answers from where it was opened (see the note at the top).
  // An instance with no record (opened before this choice existed) falls back
  // to the old order: a desktop app when one is attached, else headless.
  let backend = backendOverride || (payload && payload.key ? keyBackend.get(payload.key) : null);
  if (payload && isDrawerKey(payload.key)) {
    // A drawer tab lives in a desktop app's window and nowhere else, and it
    // belongs to one project: say which project is asking, so the app can
    // refuse another project's chat.
    const ctx = callerContext.getStore() || {};
    if (ctx.noDrawer) return Promise.reject(new Error('The browser drawer is not available on this project.'));
    // hpAllowed: may this caller use a tab on a homepool network (the owner
    // approved this chat, or it is Cru)? The app refuses such a tab otherwise.
    payload = { ...payload, alias: ctx.alias || payload.alias, hpAllowed: !!(ctx.hpAllowed || ctx.cru) };
    if (!backend) backend = drawerBackend();
    if (!backend) return Promise.reject(new Error(drawerUnavailableReason()));
  }
  if (!backend) backend = hasNativeHost() ? null : 'headless';
  if (backend === 'headless') {
    if (!headless.supports(type) || !headless.isSupported()) {
      return Promise.reject(new Error('This server has no headless browser for that.'));
    }
    return headless.handle({ type, ...payload });
  }

  return new Promise((resolve, reject) => {
    if (backend === 'remote' && !remoteHost) return reject(new Error('The desktop app this browser ran on has disconnected.'));
    if (backend === 'parent' && !hasParentIpc()) return reject(new Error('The desktop app this browser ran on has gone.'));
    if (!backend && !hasNativeHost()) {
      return reject(new Error('Browser tools need the Crundi desktop app — run one, or connect it to this server.'));
    }

    const requestId = nextRequestId++;
    const timer = setTimeout(() => {
      pendingRequests.delete(requestId);
      reject(new Error('Browser request timed out'));
    }, timeoutMs);

    pendingRequests.set(requestId, { resolve, reject, timer });
    try {
      const msg = { type, requestId, ...payload };
      if (backend === 'remote') remoteHost.send(msg);
      else if (backend === 'parent') process.send(msg);
      else postToHost(msg);
    } catch (err) {
      pendingRequests.delete(requestId);
      clearTimeout(timer);
      reject(err);
    }
  });
}

// Handle IPC replies from Electron main process
function handleIpcMessage(msg) {
  if (!msg) return;
  if (msg.type === 'netTunnel') { if (parentTunnel && typeof msg.b64 === 'string') parentTunnel.handle(Buffer.from(msg.b64, 'base64')); return; }
  // Renderer-initiated close — main process tells us to clean up local state
  if (msg.type === 'browserClosed' && msg.key) {
    const inst = browsers.get(msg.key);
    if (inst) {
      if (inst.idleTimer) clearTimeout(inst.idleTimer);
      browsers.delete(msg.key);
    }
    keyBackend.delete(msg.key);
    return;
  }
  if (msg.type !== 'browserResult') return;
  const entry = pendingRequests.get(msg.requestId);
  if (!entry) return;
  pendingRequests.delete(msg.requestId);
  clearTimeout(entry.timer);
  entry.resolve(msg);
}

// Register message handler — called once at module load
if (typeof process.on === 'function') {
  process.on('message', handleIpcMessage);
}

// ─── Idle Timer ───

function resetIdleTimer(key) {
  const inst = instFor(key);
  if (!inst) return;
  inst.lastActivityAt = Date.now();
  if (inst.idleTimer) clearTimeout(inst.idleTimer);
  inst.idleTimer = setTimeout(() => {
    console.log(`[browser] Auto-closing idle browser: ${key}`);
    closeBrowser(key).catch(() => {});
  }, IDLE_TIMEOUT_MS);
}

// ─── Helpers ───

function makeKey(alias, name) {
  return `${alias}:${name || 'default'}`;
}

function clampDimensions(width, height) {
  return {
    width: Math.min(Math.max(width || 1280, 200), MAX_WIDTH),
    height: Math.min(Math.max(height || 720, 200), MAX_HEIGHT),
  };
}

// ─── Public API ───

export async function openBrowser({ alias, name, url, width, height, on, network, show }) {
  if (String(on || '').toLowerCase() === 'drawer') return openDrawerTab({ alias, url, network, show });
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const chosen = chooseBackend(on);
  if (chosen.error) return { ok: false, error: chosen.error };

  const key = makeKey(alias, name || 'default');
  const dims = clampDimensions(width, height);

  // Check limits
  if (browsers.has(key)) {
    return { ok: false, error: `Browser '${name || 'default'}' already open for project '${alias}'. Close it first or use a different name.` };
  }
  const projectCount = [...browsers.values()].filter(b => b.alias === alias).length;
  if (projectCount >= MAX_PER_PROJECT) {
    return { ok: false, error: `Max ${MAX_PER_PROJECT} browsers per project reached. Close one first.` };
  }
  if (browsers.size >= MAX_TOTAL) {
    return { ok: false, error: `Max ${MAX_TOTAL} total browsers reached. Close one first.` };
  }

  keyBackend.set(key, chosen.backend);
  try {
    const result = await sendIpc('browserOpen', {
      key, url, width: dims.width, height: dims.height,
    });
    if (!result.ok) { keyBackend.delete(key); return { ok: false, error: result.error }; }

    // Track locally
    browsers.set(key, {
      key,
      alias,
      name: name || 'default',
      url,
      width: dims.width,
      height: dims.height,
      consoleLogs: [],
      status: 'ready',
      createdAt: Date.now(),
      lastActivityAt: Date.now(),
      idleTimer: null,
    });
    resetIdleTimer(key);
    emitBrowserUpdate();
    return { ok: true, key, on: placeOf(chosen.backend) };
  } catch (err) {
    keyBackend.delete(key);
    return { ok: false, error: err.message };
  }
}

export async function closeBrowser(key) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };

  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };

  try {
    await sendIpc('browserClose', { key });
  } catch { /* ignore — window may already be gone */ }

  if (inst.idleTimer) clearTimeout(inst.idleTimer);
  browsers.delete(key);
  keyBackend.delete(key);
  emitBrowserUpdate();
  return { ok: true };
}

/** 'server' or 'desktop', as a caller would name it. */
function placeOf(backend) {
  if (backend === 'remote') return 'desktop';
  if (backend === 'headless') return 'server';
  return hasParentIpc() ? 'server' : 'desktop';   // all-in-one: one machine
}

export async function navigateBrowser(key, url) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };

  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };

  try {
    const result = await sendIpc('browserNavigate', { key, url });
    if (!result.ok) return { ok: false, error: result.error };
    inst.url = url;
    resetIdleTimer(key);
    emitBrowserUpdate();
    return { ok: true };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export async function screenshotBrowser(key) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };

  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };

  try {
    const result = await sendIpc('browserScreenshot', { key });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, data: result.data };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export async function readBrowserPage(key) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };

  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };

  try {
    const result = await sendIpc('browserReadPage', { key });
    if (!result.ok) return { ok: false, error: result.error };
    // Merge console logs from main process
    if (result.consoleLogs) {
      inst.consoleLogs = result.consoleLogs.slice(-MAX_CONSOLE_LOGS);
    }
    resetIdleTimer(key);
    return {
      ok: true,
      // The handler returns the cleaned page text as `text`; this read
      // `result.html`, which no handler has ever set. So browser_read_page has
      // returned a title and a URL and silently dropped the page content since
      // the first public release — on Electron too, not just headless.
      text: result.text,
      consoleLogs: inst.consoleLogs,
      url: result.url,
      title: result.title,
    };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

/**
 * browser_click: by CSS selector (as ever), by what a person sees (text, with
 * role and within to narrow it; an open dialog is searched first), or at x,y
 * (real mouse input, like browser_mouse). Every way reports what it clicked
 * (tag, role, text, where), and anything covering it, so a wrong hit shows.
 * Built from browser_eval and browser_mouse, so every desktop app has it.
 */
export async function clickBrowser(key, selector, opts = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };

  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  const { text, role, within, exact, nth, x, y } = opts || {};
  const parse = (r) => { try { return JSON.parse(r && r.result); } catch { return null; } };

  try {
    if (x != null || y != null) {
      if (!Number.isFinite(Number(x)) || !Number.isFinite(Number(y))) return { ok: false, error: 'x and y must both be numbers' };
      const at = parse(await evalBrowser(key, pointScript(Number(x), Number(y))));
      const m = await mouseBrowser(key, Number(x), Number(y), 'click', { button: opts.button });
      if (!m.ok) return m;
      return { ok: true, url: m.url, clicked: at && at.ok ? strip(at) : null };
    }
    if (!selector && !text && !role) return { ok: false, error: 'Say what to click: selector, text (with role or within), or x and y' };
    const r = await evalBrowser(key, findScript({ selector, text, role, within, exact, nth, click: true }));
    if (!r.ok) return { ok: false, error: r.error };
    const f = parse(r);
    if (!f) return { ok: false, error: 'The page did not answer' };
    if (!f.ok) {
      // A selector that matches only something hidden: click it as before (el.click()).
      if (selector && !text && !role) {
        const old = await sendIpc('browserClick', { key, selector });
        if (!old.ok) return { ok: false, error: old.error };
        resetIdleTimer(key);
        return { ok: true, url: old.url, clicked: null, note: 'Matched only a hidden element; clicked it anyway (el.click()).' };
      }
      return { ok: false, error: f.error, ...(f.near && f.near.length ? { onPage: f.near } : {}) };
    }
    resetIdleTimer(key);
    const out = { ok: true, clicked: strip(f), matches: f.matches, searched: f.searched };
    if (f.matches > 1) out.note = `${f.matches} elements matched; clicked number ${(Number(nth) || 0) + 1} (nth picks another).`;
    if (f.tag === 'select') out.note = `A native dropdown: a click by selector or text does not open its list. To show its options for the person to choose, click it at its position (browser_click x: ${f.x}, y: ${f.y}): Crundi draws the list in the page. To choose yourself, browser_select { selector: ${JSON.stringify(f.selector)}, value: "<an option's text or value>" }. Its options are in clicked.options.`;
    if (f.covered) out.warning = `Something else covers it at that point: ${f.covered.role} "${f.covered.text}". A person's click would land there; el.click() reached the element anyway.`;
    return out;
  } catch (err) {
    return { ok: false, error: err.message };
  }
}
function strip(d) { const { ok, matches, searched, covered, clicked, ...rest } = d || {}; return rest; }

export async function typeBrowser(key, text, selector) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };

  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };

  try {
    const result = await sendIpc('browserType', { key, text, selector });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, url: result.url };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export async function resizeBrowser(key, width, height) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };

  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };

  const dims = clampDimensions(width, height);

  try {
    const result = await sendIpc('browserResize', { key, width: dims.width, height: dims.height });
    if (!result.ok) return { ok: false, error: result.error };
    inst.width = dims.width;
    inst.height = dims.height;
    resetIdleTimer(key);
    emitBrowserUpdate();
    return { ok: true };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

// ─── Mouse ───
//
// move, click, double_click, down (press and hold), up, drag. The pointer
// remembers where it is and which button is held, per browser, so a press,
// moves and a release can be separate calls. Desktop app browsers send the
// events through their debugger (app/browser-pointer.js), and a drawer tab
// shows the chat's cursor while it moves.

const MOUSE_CAPABILITY = 'browser-mouse';
const MOUSE_ACTIONS = ['move', 'click', 'double_click', 'down', 'up', 'drag'];
const OLD_MOUSE_ACTIONS = ['move', 'click'];   // what a desktop app from before browser-mouse understood

function mouseSupported(key, action) {
  if (OLD_MOUSE_ACTIONS.includes(action)) return true;
  let backend = keyBackend.get(key);
  if (!backend) backend = isDrawerKey(key) ? drawerBackend() : (hasNativeHost() ? null : 'headless');
  if (backend === 'headless' || backend === 'parent') return true;
  return hostSupports(MOUSE_CAPABILITY);   // a remote app announces it
}

export async function mouseBrowser(key, x, y, action, opts = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };

  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  action = action || 'click';
  if (!MOUSE_ACTIONS.includes(action)) return { ok: false, error: `action must be one of ${MOUSE_ACTIONS.join(', ')}` };
  if (!mouseSupported(key, action)) {
    return { ok: false, error: `The mouse action "${action}" is not supported by the desktop app this browser runs on. Update the Crundi app, or use "click" and "move".` };
  }
  const { button, toX, toY, duration } = opts;
  const msg = { key, x, y, action };
  if (button != null) msg.button = button;
  if (toX != null) msg.toX = toX;
  if (toY != null) msg.toY = toY;
  if (duration != null) msg.duration = duration;
  const ms = Math.min(10000, Math.max(0, Number(duration) || 0)) + (action === 'drag' ? 1000 : 0);

  // A press on a native <select>: its own list would close at once (the
  // drawer gives the keyboard back to the person) and no screenshot shows it.
  // So the cursor goes there and Crundi draws the options in the page, under
  // the dropdown (pickerScript), where the person and the chat both see them;
  // either can click one, or the chat chooses with browser_select.
  if ((action === 'click' || action === 'double_click' || action === 'down') && x != null && y != null && (opts.button || 'left') === 'left') {
    try {
      const at = await evalBrowser(key, pointScript(Number(x), Number(y)));
      let d = null; try { d = JSON.parse(at && at.result); } catch { /* no answer */ }
      if (d && d.ok && d.tag === 'select') {
        await sendIpc('browserMouse', { key, x, y, action: 'move' }).catch(() => {});
        const pk = await evalBrowser(key, pickerScript(Number(x), Number(y)));
        let shown = null; try { shown = JSON.parse(pk && pk.result); } catch { /* no answer */ }
        resetIdleTimer(key);
        const { ok, ...dropdown } = d;
        return { ok: true, x, y, held: null, dropdown, listShown: !!(shown && shown.ok), note: `A dropdown: its options are shown in the page under it, where the person sees them and your screenshot does. To let the person choose, leave the list open and say so. To choose yourself, click an option in it (browser_click text: "<option>", role: "option") or browser_select { selector: ${JSON.stringify(d.selector)}, value: "<option text or value>" }.` };
      }
    } catch { /* just click */ }
  }
  try {
    const result = await sendIpc('browserMouse', msg, ms + IPC_TIMEOUT_MS);
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    const out = { ok: true, url: result.url };
    if (result.x != null) { out.x = result.x; out.y = result.y; out.held = result.held || null; }
    return out;
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

// ─── Touch gestures ───
//
// planTouch (browser-touch.js) turns a gesture into CDP touch steps, which the
// backend replays; the probe scripts around it report what took the gesture.

const TOUCH_CAPABILITY = 'browser-touch';
const TOUCH_SETTLE_MS = 700;

/** Can the backend this instance lives on replay touch steps? */
function touchSupported(key) {
  let backend = keyBackend.get(key);
  if (!backend) backend = hasNativeHost() ? null : 'headless';
  if (backend === 'headless' || backend === 'parent') return true;
  return hostSupports(TOUCH_CAPABILITY);   // a remote app announces it
}

export async function touchBrowser(key, opts = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  if (!touchSupported(key)) {
    return { ok: false, error: 'Touch is not supported by the desktop app this browser runs on. Update the Crundi app, or open the browser with on: "server".' };
  }
  const plan = planTouch(opts);
  if (plan.error) return { ok: false, error: plan.error };
  const report = opts.report !== false;
  const totalMs = plan.steps.reduce((s, st) => s + st.wait, 0);
  try {
    // Probe first, so the report sees the page as the finger found it.
    let install = null;
    if (report) {
      const r = await sendIpc('browserEval', { key, code: probeInstallScript(opts.x, opts.y) });
      install = r.ok ? parseJson(r.result) : null;
    }
    const result = await sendIpc('browserTouch', {
      key, steps: plan.steps, ...(report ? { listenersAt: { x: opts.x, y: opts.y } } : {}),
    }, totalMs + IPC_TIMEOUT_MS);
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    const out = { ok: true, action: opts.action || 'tap' };
    if (report) {
      // A fling, a scroll-snap or a carousel's own animation finishes after
      // the finger lifts; judge what moved once it has.
      await new Promise((r) => setTimeout(r, TOUCH_SETTLE_MS));
      const r = await sendIpc('browserEval', { key, code: probeCollectScript });
      const collect = r.ok ? parseJson(r.result) : null;
      if (collect && !collect.error) out.report = summarizeProbe(install, collect, result.listeners);
      else out.report = { summary: [collect?.error || 'Could not read the page after the gesture.'], listeners: result.listeners };
    }
    return out;
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

function parseJson(v) {
  if (v && typeof v === 'object') return v;
  try { return JSON.parse(v); } catch { return null; }
}

export async function evalBrowser(key, code) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserEval', { key, code });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, result: result.result };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export async function getBrowserSource(key) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserSource', { key });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, html: result.html, url: result.url, title: result.title };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export async function getBrowserConsole(key, { clear = false, start, end, countOnly } = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserConsole', { key, clear, start, end, countOnly });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    if (countOnly) return { ok: true, total: result.total };
    return { ok: true, logs: result.logs || [], total: result.total };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

/**
 * Network capture control. action: 'start' | 'stop' | 'log' | 'clear'.
 * 'start' begins recording requests (and clears prior ones); 'log' returns them;
 * 'clear' empties the buffer so the next view shows only fresh calls.
 */
export async function browserNetwork(key, action, { start, end, countOnly } = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserNetwork', { key, action, start, end, countOnly });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    if (countOnly) return { ok: true, recording: result.recording, total: result.total };
    return { ok: true, requests: result.requests || [], recording: result.recording, total: result.total };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export function listBrowsers(alias) {
  const all = [...browsers.values()];
  const filtered = alias ? all.filter(b => b.alias === alias) : all;
  return filtered.map(b => ({
    key: b.key,
    alias: b.alias,
    name: b.name,
    url: b.url,
    width: b.width,
    height: b.height,
    status: b.status,
    createdAt: b.createdAt,
    lastActivityAt: b.lastActivityAt,
    consoleLogCount: b.consoleLogs.length,
    on: placeOf(keyBackend.get(b.key) || (hasNativeHost() ? (hasParentIpc() ? 'parent' : 'remote') : 'headless')),
  }));
}

export async function goBackBrowser(key) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserGoBack', { key });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, url: result.url };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function goForwardBrowser(key) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserGoForward', { key });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, url: result.url };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function scrollBrowser(key, { x, y, selector } = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserScroll', { key, x, y, selector });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function waitBrowser(key, { selector, timeout } = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    // The page-side wait runs up to `timeout` ms; give the IPC channel extra
    // headroom so the main process returns a clean result before we time out.
    const waitMs = timeout || 30000;
    const result = await sendIpc('browserWait', { key, selector, timeout: waitMs }, waitMs + 10000);
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, found: result.found };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function cookiesBrowser(key, { action, cookie, filter, url } = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserCookies', { key, action: action || 'get', cookie, filter, url });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, cookies: result.cookies };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function snapshotBrowser(key, { maxDepth, maxLength } = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserSnapshot', { key, maxDepth, maxLength });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, tree: result.tree, url: result.url, title: result.title, length: result.length };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function elementsBrowser(key, { start, end, countOnly, selector } = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserElements', { key, start, end, countOnly, selector });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    if (countOnly) return { ok: true, total: result.total, url: result.url, title: result.title };
    return { ok: true, elements: result.elements, total: result.total, url: result.url, title: result.title };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function fillBrowser(key, selector, value) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserFill', { key, selector, value });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, url: result.url };
  } catch (err) { return { ok: false, error: err.message }; }
}

/**
 * browser_select: choose in a native <select>. The dropdown by selector or by
 * its label; the option by value or by its visible text (or index). Fires
 * input and change and reports what was chosen; lists the options when none
 * matches. (A <select>'s open list is drawn outside the page, where neither a
 * screenshot nor a click reaches it, so this is how a chat chooses.)
 */
export async function selectBrowser(key, selector, value, opts = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  const { label, index } = opts || {};
  if (!selector && !label) return { ok: false, error: 'Say which dropdown: selector, or label (its label text)' };
  if ((value === undefined || value === null || value === '') && index == null) return { ok: false, error: 'Say which option: value (its value or its visible text), or index' };
  try {
    const r = await evalBrowser(key, selectScript({ selector, label, value, index }));
    if (!r.ok) return { ok: false, error: r.error };
    let f = null; try { f = JSON.parse(r.result); } catch { /* no answer */ }
    if (!f) return { ok: false, error: 'The page did not answer' };
    resetIdleTimer(key);
    if (!f.ok) { const { ok, ...rest } = f; return { ok: false, ...rest }; }
    return f;
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function pdfBrowser(key, { landscape } = {}) {
  if (!browserBackendReady()) return { ok: false, error: backendUnavailableReason() };
  const inst = instFor(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserPdf', { key, landscape });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, data: result.data };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function closeAllBrowsers() {
  const keys = [...browsers.keys()];
  for (const key of keys) {
    try { await closeBrowser(key); } catch { /* ignore */ }
  }
}

export function emitBrowserUpdate() {
  try { postToHost({ type: 'browsers', data: listBrowsers() }); }
  catch { /* channel closed */ }
}

// ─── The browser drawer: the desktop app's own tabs, shared with the chats ───
//
// The PC client has a browser drawer beside the project area. Its tabs belong
// to a project, and that project's chats can use them with every browser_*
// tool, by key "drawer:<tab id>" (browser_list shows them, browser_open with
// on: "drawer" makes one). Unlike the hidden browsers above they are not ours
// to track or idle-close: the person owns them, and the desktop app is the
// only record of what is open. So nothing is kept here but which app holds
// which tab, learned from browser_list / browser_open and re-learned on demand.
//
// Each tab also has a network: "client" (the desktop app's own, the default)
// or "server" (this machine's, carried over the app's websocket; see
// net-tunnel.js). The app does the routing; here it is just a field.

/** Which project the current tool call is for, set by the MCP dispatcher. */
export const callerContext = new AsyncLocalStorage();

const DRAWER_CAPABILITY = 'browser-drawer';
const DRAWER_PREFIX = 'drawer:';

export function isDrawerKey(key) {
  return typeof key === 'string' && key.startsWith(DRAWER_PREFIX);
}

/** A stand-in record for a drawer tab, so the browser_* functions above accept its key. */
function instFor(key) {
  const inst = browsers.get(key);
  if (inst || !isDrawerKey(key)) return inst;
  return { key, alias: (callerContext.getStore() || {}).alias || '', consoleLogs: [], drawer: true };
}

/** The desktop apps that can hold drawer tabs right now. */
function drawerHosts() {
  const out = [];
  if (hasParentIpc()) out.push('parent');
  if (remoteHost && hostSupports(DRAWER_CAPABILITY)) out.push('remote');
  return out;
}

function drawerBackend() {
  return drawerHosts()[0] || null;
}

function drawerUnavailableReason() {
  if (remoteHost && !hostSupports(DRAWER_CAPABILITY)) return 'The browser drawer is not supported by the Crundi desktop app connected to this server. Update the app.';
  return 'The browser drawer is in the Crundi desktop app, and none is connected to this server.';
}

const NETWORKS = ['client', 'server'];
/** "client" | "server" | "homepool:<network>" (as chats say it) → the app's name: client, server, hp:<network>. */
function normNetwork(n) {
  const v = String(n || '').toLowerCase();
  if (!v) return null;
  if (NETWORKS.includes(v)) return v;
  const m = /^(?:homepool|hp):([a-z0-9][a-z0-9_-]{0,39})$/.exec(v);
  return m ? 'hp:' + m[1] : undefined;
}
const NETWORK_HINT = 'network must be "client", "server" or "homepool:<network>"';
/** The app's name back to the chats' ("hp:home" → "homepool:home"). */
const outNetwork = (n) => (typeof n === 'string' && n.startsWith('hp:') ? 'homepool:' + n.slice(3) : n);

/** Can a drawer tab be had right now? { ok } or { ok: false, error } in words. */
export function drawerAvailability() {
  const ctx = callerContext.getStore() || {};
  if (ctx.noDrawer) return { ok: false, error: 'The browser drawer is not available on this project.' };
  return drawerHosts().length ? { ok: true } : { ok: false, error: drawerUnavailableReason() };
}

/** The drawer tabs of a project (every connected app), with where each runs. */
export async function listDrawerTabs(alias) {
  const ctx = callerContext.getStore() || {};
  if (ctx.noDrawer) return [];
  const out = [];
  for (const backend of drawerHosts()) {
    try {
      // Cru sees every project's tabs; a chat, its own project's.
      const r = await sendIpc('browserDrawerList', { alias, all: !!ctx.cru && !alias }, 15000, backend);
      if (!r || !r.ok || !Array.isArray(r.tabs)) continue;
      for (const t of r.tabs) {
        const key = DRAWER_PREFIX + t.tabId;
        keyBackend.set(key, backend);
        out.push({
          key, alias: t.project, name: t.title || t.url || 'tab', url: t.url, on: 'drawer',
          network: outNetwork(t.network), networkLabel: t.networkLabel, exit: t.exit || undefined, active: !!t.active, shown: !!t.shown, sleeping: !!t.sleeping, unloaded: !t.loaded,
          device: t.device || null,
        });
      }
    } catch (err) { if (out.problems) out.problems.push(err.message); else Object.defineProperty(out, 'problems', { value: [err.message] }); }
  }
  return out;
}

/** browser_list: the hidden browsers plus this project's drawer tabs. */
export async function listAllBrowsers(alias) {
  const hidden = listBrowsers(alias);
  let drawer = [];
  try { drawer = await listDrawerTabs(alias); } catch { /* none */ }
  // Why there are no drawer tabs, when that is not simply "none open".
  const why = !drawerHosts().length ? drawerUnavailableReason() : (drawer.problems && drawer.problems.length && !drawer.length ? drawer.problems[0] : '');
  const out = { browsers: [...hidden, ...drawer], drawer: why || 'available' };
  // Say how to open one, so "available" with no tabs is not a dead end.
  if (!why) out.openTab = 'browser_drawer_open { url, network: "server" (this machine, its localhost) or "client" (the person\'s own network), show: true }';
  return out;
}

export async function openDrawerTab({ alias, url, network, show }) {
  const backend = drawerBackend();
  if (!backend) return { ok: false, error: drawerUnavailableReason() };
  const net = normNetwork(network);
  if (net === undefined) return { ok: false, error: NETWORK_HINT };
  const ctx = callerContext.getStore() || {};
  if (ctx.noDrawer) return { ok: false, error: 'The browser drawer is not available on this project.' };
  try {
    const r = await sendIpc('browserDrawerOpen', { alias, url: url || 'about:blank', network: net || 'client', show: !!show, hpAllowed: !!(ctx.hpAllowed || ctx.cru) }, IPC_TIMEOUT_MS, backend);
    if (!r.ok) return { ok: false, error: r.error };
    const key = DRAWER_PREFIX + r.tabId;
    keyBackend.set(key, backend);
    return { ok: true, key, on: 'drawer', network: outNetwork(r.network), networkLabel: r.networkLabel, url: r.url, ...(r.warning ? { warning: r.warning } : {}) };
  } catch (err) { return { ok: false, error: err.message }; }
}

function needDrawer(key) {
  if (!isDrawerKey(key)) return { ok: false, error: `Only drawer tabs can do that (keys starting "${DRAWER_PREFIX}", see browser_list). Open one with browser_drawer_open.` };
  return null;
}

/** Change a tab's network. The page reloads on the new one; cookies are kept per network. */
export async function setDrawerNetwork(key, network) {
  const bad = needDrawer(key); if (bad) return bad;
  const net = normNetwork(network);
  if (!net) return { ok: false, error: NETWORK_HINT };
  try {
    const r = await sendIpc('browserDrawerSetNetwork', { key, network: net });
    if (!r.ok) return { ok: false, error: r.error };
    return { ok: true, network: outNetwork(r.network), networkLabel: r.networkLabel, url: r.url };
  } catch (err) { return { ok: false, error: err.message }; }
}

const SHAPES = ['pin', 'box', 'arrow', 'label'];

/** Validate the shapes a chat asks to draw; returns { shapes } or { error }. */
export function cleanShapes(list) {
  if (!Array.isArray(list)) return { error: 'shapes must be a list' };
  if (list.length > 50) return { error: 'at most 50 shapes at a time' };
  const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? Math.round(v * 10) / 10 : undefined);
  const out = [];
  for (const raw of list) {
    const sh = raw || {};
    const kind = String(sh.kind || sh.type || 'pin').toLowerCase();
    if (!SHAPES.includes(kind)) return { error: `shape kind must be one of ${SHAPES.join(', ')}` };
    const c = { kind, text: typeof sh.text === 'string' ? sh.text.slice(0, 500) : '' };
    if (typeof sh.selector === 'string' && sh.selector) c.selector = sh.selector.slice(0, 500);
    for (const k of ['x', 'y', 'width', 'height', 'toX', 'toY']) { const v = num(sh[k]); if (v !== undefined) c[k] = v; }
    if (!c.selector && (c.x === undefined || c.y === undefined)) return { error: `each ${kind} needs x and y (page CSS pixels in the viewport) or a selector` };
    if (kind === 'box' && !c.selector && (!c.width || !c.height)) return { error: 'a box needs width and height (or a selector)' };
    if (kind === 'arrow' && (c.toX === undefined || c.toY === undefined)) return { error: 'an arrow needs toX and toY' };
    out.push(c);
  }
  return { shapes: out };
}

/** Draw on the overlay over a drawer tab, where the person sees it. */
export async function annotateDrawer(key, { shapes, clear, show } = {}) {
  const bad = needDrawer(key); if (bad) return bad;
  const c = cleanShapes(shapes || []);
  if (c.error) return { ok: false, error: c.error };
  if (!c.shapes.length && !clear) return { ok: false, error: 'Give shapes to draw, or clear: true' };
  try {
    const r = await sendIpc('browserDrawerAnnotate', { key, shapes: c.shapes, clear: !!clear, show: show !== false });
    if (!r.ok) return { ok: false, error: r.error };
    return { ok: true, drawn: r.drawn, total: r.total };
  } catch (err) { return { ok: false, error: err.message }; }
}

/** Point at one spot (a pin with an optional label). */
export function pointDrawer(key, { x, y, selector, label, show } = {}) {
  return annotateDrawer(key, { shapes: [{ kind: 'pin', x, y, selector, text: label || '' }], show });
}

/**
 * What is drawn on a tab: the person's marks and the chats', each with the
 * element under it, and (image: true, the default) a screenshot with the
 * marks drawn in. Answers as content blocks, text and image.
 */
export async function readDrawerAnnotations(key, { image = true } = {}) {
  const bad = needDrawer(key); if (bad) return bad;
  try {
    const r = await sendIpc('browserDrawerAnnotations', { key, image: image !== false });
    if (!r.ok) return { ok: false, error: r.error };
    const info = { ok: true, url: r.url, title: r.title, viewport: r.viewport, annotations: r.annotations || [] };
    const content = [{ type: 'text', text: JSON.stringify(info, null, 2) }];
    if (r.image) content.push({ type: 'image', data: r.image, mimeType: 'image/png' });
    return { ok: true, content };
  } catch (err) { return { ok: false, error: err.message }; }
}
