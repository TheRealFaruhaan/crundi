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

// ─── IPC Bridge ───

function isElectron() {
  return process.env.ELECTRON_RUN === '1';
}

function sendIpc(type, payload, timeoutMs = IPC_TIMEOUT_MS) {
  return new Promise((resolve, reject) => {
    if (!isElectron() || typeof process.send !== 'function') {
      return reject(new Error('Browser tools require the Electron desktop app.'));
    }

    const requestId = nextRequestId++;
    const timer = setTimeout(() => {
      pendingRequests.delete(requestId);
      reject(new Error('Browser IPC request timed out'));
    }, timeoutMs);

    pendingRequests.set(requestId, { resolve, reject, timer });
    try {
      process.send({ type, requestId, ...payload });
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
  // Renderer-initiated close — main process tells us to clean up local state
  if (msg.type === 'browserClosed' && msg.key) {
    const inst = browsers.get(msg.key);
    if (inst) {
      if (inst.idleTimer) clearTimeout(inst.idleTimer);
      browsers.delete(msg.key);
    }
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
  const inst = browsers.get(key);
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

export async function openBrowser({ alias, name, url, width, height }) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };

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

  try {
    const result = await sendIpc('browserOpen', {
      key, url, width: dims.width, height: dims.height,
    });
    if (!result.ok) return { ok: false, error: result.error };

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
    return { ok: true, key };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export async function closeBrowser(key) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };

  const inst = browsers.get(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };

  try {
    await sendIpc('browserClose', { key });
  } catch { /* ignore — window may already be gone */ }

  if (inst.idleTimer) clearTimeout(inst.idleTimer);
  browsers.delete(key);
  emitBrowserUpdate();
  return { ok: true };
}

export async function navigateBrowser(key, url) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };

  const inst = browsers.get(key);
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
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };

  const inst = browsers.get(key);
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
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };

  const inst = browsers.get(key);
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
      html: result.html,
      consoleLogs: inst.consoleLogs,
      url: result.url,
      title: result.title,
    };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export async function clickBrowser(key, selector) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };

  const inst = browsers.get(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };

  try {
    const result = await sendIpc('browserClick', { key, selector });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, url: result.url };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export async function typeBrowser(key, text, selector) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };

  const inst = browsers.get(key);
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
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };

  const inst = browsers.get(key);
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

export async function mouseBrowser(key, x, y, action) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };

  const inst = browsers.get(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };

  try {
    const result = await sendIpc('browserMouse', { key, x, y, action });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, url: result.url };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

export async function evalBrowser(key, code) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
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
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
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
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
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
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
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
  }));
}

export async function goBackBrowser(key) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserGoBack', { key });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, url: result.url };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function goForwardBrowser(key) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserGoForward', { key });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, url: result.url };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function scrollBrowser(key, { x, y, selector } = {}) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserScroll', { key, x, y, selector });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function waitBrowser(key, { selector, timeout } = {}) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
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
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserCookies', { key, action: action || 'get', cookie, filter, url });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, cookies: result.cookies };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function snapshotBrowser(key, { maxDepth, maxLength } = {}) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserSnapshot', { key, maxDepth, maxLength });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, tree: result.tree, url: result.url, title: result.title, length: result.length };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function elementsBrowser(key, { start, end, countOnly, selector } = {}) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
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
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserFill', { key, selector, value });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, url: result.url };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function selectBrowser(key, selector, value) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
  if (!inst) return { ok: false, error: `No browser found with key '${key}'.` };
  try {
    const result = await sendIpc('browserSelect', { key, selector, value });
    if (!result.ok) return { ok: false, error: result.error };
    resetIdleTimer(key);
    return { ok: true, url: result.url };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function pdfBrowser(key, { landscape } = {}) {
  if (!isElectron()) return { ok: false, error: 'Browser tools require the Electron desktop app.' };
  const inst = browsers.get(key);
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
  if (typeof process.send !== 'function') return;
  try {
    process.send({ type: 'browsers', data: listBrowsers() });
  } catch { /* channel closed */ }
}
