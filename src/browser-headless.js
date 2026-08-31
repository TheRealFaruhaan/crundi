/**
 * Headless browser backend for the MCP browser tools.
 *
 * Why this exists: every browser operation used to be gated on Electron, so on
 * a server with no desktop app the tools returned "Browser tools require the
 * Electron desktop app" — even though nothing about fetching a page, clicking a
 * selector or taking a screenshot needs a visible window.
 *
 * This is a FALLBACK, not a replacement. When a desktop app is attached it
 * still handles everything, exactly as before, including the device emulation
 * and visible windows that genuinely do need it. This only takes over when
 * there is no host to ask.
 *
 * It speaks CDP directly over a WebSocket rather than pulling in Puppeteer:
 * `ws` is already a dependency, and a browser download is something the install
 * script provisions once rather than something npm drags in on every update.
 *
 * The operation contract is deliberately identical to app/main.js's
 * browserIpcHandlers, down to reusing its page snippets verbatim, so both
 * backends answer the same question the same way.
 */

import { spawn } from 'child_process';
import { existsSync, readFileSync, mkdtempSync, rmSync, readdirSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import os from 'os';
import { WebSocket } from 'ws';
import { config } from './config.js';

const LAUNCH_TIMEOUT_MS = 20000;
const CMD_TIMEOUT_MS = 30000;
const LOAD_GRACE_MS = 15000;
const MAX_CONSOLE = 500;
const MAX_NETWORK = 1000;

// ─── Finding a browser ───

// Where the platform's Chrome-for-Testing build keeps its executable.
const CHROME_LEAVES = process.platform === 'win32'
  ? [join('chrome-win64', 'chrome.exe'), join('chrome-headless-shell-win64', 'chrome-headless-shell.exe')]
  : process.platform === 'darwin'
    ? [join('chrome-mac-x64', 'Google Chrome for Testing.app', 'Contents', 'MacOS', 'Google Chrome for Testing')]
    : [join('chrome-linux64', 'chrome'), join('chrome-headless-shell-linux64', 'chrome-headless-shell')];

// The app's own root, so a bundled browser is found when the server is run
// straight out of the release archive. The Windows server ships as a zip you
// extract and start — there is no installer to copy anything into place.
const appRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Newest versioned subdirectory of a Chrome-for-Testing style cache.
 * Deliberately prefix-agnostic: the directories are named per platform
 * (linux-<ver>, win-<ver>), and filtering for one of them is how this would
 * silently find nothing on the other.
 */
function newestIn(dir, leaf) {
  try {
    const names = readdirSync(dir).sort();
    for (let i = names.length - 1; i >= 0; i--) {
      const p = join(dir, names[i], leaf);
      if (existsSync(p)) return p;
    }
  } catch { /* not there */ }
  return '';
}

/**
 * Where to find a browser, best first.
 *
 * Crundi's own copy wins over everything: the caches below belong to other
 * projects, and the Info tab now offers a button that deletes them. Depending
 * on someone else's cache would mean shipping a feature that another feature
 * can break.
 */
export function findChrome() {
  const fromEnv = process.env.CRUNDI_CHROME || '';
  if (fromEnv && existsSync(fromEnv)) return fromEnv;

  // Installed copy first, then one sitting in the release archive.
  for (const base of [join(config.dataDir, 'chrome'), join(appRoot, 'chrome')]) {
    for (const leaf of CHROME_LEAVES) {
      const found = newestIn(base, leaf);
      if (found) return found;
    }
    for (const name of ['chrome.exe', 'chrome']) {
      const p = join(base, name);
      if (existsSync(p)) return p;
    }
  }

  const system = process.platform === 'win32'
    ? [
      'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
      'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
    ]
    : process.platform === 'darwin'
      ? ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
        '/Applications/Chromium.app/Contents/MacOS/Chromium']
      : ['/usr/bin/google-chrome', '/usr/bin/google-chrome-stable',
        '/usr/bin/chromium', '/usr/bin/chromium-browser', '/snap/bin/chromium'];
  for (const p of system) if (existsSync(p)) return p;

  // Last resort: a cache another tool downloaded.
  const home = os.homedir();
  for (const [dir, leaf] of [
    [join(home, '.cache/puppeteer/chrome'), CHROME_LEAVES[0]],
    [join(home, '.cache/puppeteer/chrome-headless-shell'), CHROME_LEAVES[1] || CHROME_LEAVES[0]],
  ]) {
    const found = newestIn(dir, leaf);
    if (found) return found;
  }
  return '';
}

export function isSupported() { return !!findChrome(); }

// ─── CDP plumbing ───

let browserProc = null;
let browserWs = null;
let profileDir = '';
let launching = null;
let nextId = 1;
const pending = new Map();          // command id -> {resolve, reject, timer}
const sessionHandlers = new Map();  // sessionId -> (method, params) => void
let sandboxDisabled = false;

/** Whether the sandbox had to be turned off to start. Surfaced, never hidden. */
export function sandboxStatus() {
  return { running: !!browserProc, sandboxDisabled };
}

function cdpSend(method, params = {}, sessionId) {
  return new Promise((resolve, reject) => {
    if (!browserWs || browserWs.readyState !== 1) return reject(new Error('The browser is not running'));
    const id = nextId++;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(method + ' timed out'));
    }, CMD_TIMEOUT_MS);
    pending.set(id, { resolve, reject, timer });
    const msg = { id, method, params };
    if (sessionId) msg.sessionId = sessionId;
    try { browserWs.send(JSON.stringify(msg)); }
    catch (err) { pending.delete(id); clearTimeout(timer); reject(err); }
  });
}

function onCdpMessage(raw) {
  let msg;
  try { msg = JSON.parse(raw); } catch { return; }
  if (msg.id && pending.has(msg.id)) {
    const entry = pending.get(msg.id);
    pending.delete(msg.id);
    clearTimeout(entry.timer);
    if (msg.error) entry.reject(new Error(msg.error.message || 'CDP error'));
    else entry.resolve(msg.result || {});
    return;
  }
  if (msg.method && msg.sessionId) {
    const handler = sessionHandlers.get(msg.sessionId);
    if (handler) handler(msg.method, msg.params || {});
  }
}

/** Read the port and browser websocket path Chrome writes on startup. */
function readDevToolsEndpoint(dir) {
  const f = join(dir, 'DevToolsActivePort');
  if (!existsSync(f)) return null;
  const lines = readFileSync(f, 'utf8').split('\n');
  const port = parseInt(lines[0], 10);
  const path = (lines[1] || '').trim();
  if (!Number.isFinite(port) || !path) return null;
  return 'ws://127.0.0.1:' + port + path;
}

function spawnChrome(bin, dir, noSandbox) {
  const args = [
    '--headless=new',
    '--disable-gpu',
    '--remote-debugging-port=0',
    '--user-data-dir=' + dir,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-dev-shm-usage',
    '--disable-background-networking',
    '--disable-extensions',
    '--mute-audio',
    'about:blank',
  ];
  if (noSandbox) args.unshift('--no-sandbox');
  return spawn(bin, args, { stdio: ['ignore', 'ignore', 'pipe'], detached: false });
}

async function waitForEndpoint(dir, proc, ms) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (proc.exitCode !== null) return null;   // it died; stop waiting
    const ep = readDevToolsEndpoint(dir);
    if (ep) return ep;
    await new Promise((r) => setTimeout(r, 120));
  }
  return null;
}

/**
 * Start the browser, once, shared by every open page.
 *
 * The sandbox is kept on wherever possible. Ubuntu 24.04 restricts the
 * unprivileged user namespaces it depends on, which the install script fixes
 * with an AppArmor profile; if that has not been done, this falls back to
 * --no-sandbox rather than refusing to work, and says so through
 * sandboxStatus() instead of degrading quietly.
 */
async function ensureBrowser() {
  if (browserWs && browserWs.readyState === 1) return;
  if (launching) return launching;

  launching = (async () => {
    const bin = findChrome();
    if (!bin) throw new Error('No Chrome found. Run the Crundi installer to provision one, or set CRUNDI_CHROME.');

    profileDir = mkdtempSync(join(os.tmpdir(), 'crundi-chrome-'));
    let proc = spawnChrome(bin, profileDir, false);
    let endpoint = await waitForEndpoint(profileDir, proc, LAUNCH_TIMEOUT_MS);

    if (!endpoint) {
      try { proc.kill('SIGKILL'); } catch { /* already gone */ }
      proc = spawnChrome(bin, profileDir, true);
      endpoint = await waitForEndpoint(profileDir, proc, LAUNCH_TIMEOUT_MS);
      if (!endpoint) {
        try { proc.kill('SIGKILL'); } catch { /* already gone */ }
        throw new Error('Chrome would not start. Check its shared libraries are installed.');
      }
      sandboxDisabled = true;
    } else {
      sandboxDisabled = false;
    }

    const ws = new WebSocket(endpoint, { maxPayload: 256 * 1024 * 1024 });
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('Timed out connecting to the browser')), LAUNCH_TIMEOUT_MS);
      ws.once('open', () => { clearTimeout(timer); resolve(); });
      ws.once('error', (e) => { clearTimeout(timer); reject(e); });
    });
    ws.on('message', (d) => onCdpMessage(d.toString()));
    ws.on('close', () => { browserWs = null; });

    browserProc = proc;
    browserWs = ws;
    proc.once('exit', () => {
      browserProc = null;
      browserWs = null;
      for (const [, p] of pending) { clearTimeout(p.timer); p.reject(new Error('The browser exited')); }
      pending.clear();
    });
  })().finally(() => { launching = null; });

  return launching;
}

/** Stop the shared browser and clean up its profile. */
export async function shutdown() {
  for (const key of [...pages.keys()]) { try { await closePage(key); } catch { /* going away anyway */ } }
  try { browserWs?.close(); } catch { /* ignore */ }
  try { browserProc?.kill(); } catch { /* ignore */ }
  browserWs = null;
  browserProc = null;
  if (profileDir) {
    try { rmSync(profileDir, { recursive: true, force: true }); } catch { /* tmp will get it */ }
    profileDir = '';
  }
}

// ─── Pages ───

const pages = new Map();   // key -> { targetId, sessionId, contextId, consoleLogs, networkEntries, recording }

function page(key) { return pages.get(key) || null; }

/** Console text from a Runtime.consoleAPICalled argument list. */
function argsToText(args) {
  return (args || []).map((a) => {
    if (a.value !== undefined) return typeof a.value === 'string' ? a.value : JSON.stringify(a.value);
    if (a.description) return a.description;
    return a.type || '';
  }).join(' ');
}

function attachEvents(entry) {
  sessionHandlers.set(entry.sessionId, (method, p) => {
    if (method === 'Runtime.consoleAPICalled') {
      entry.consoleLogs.push({ time: Date.now(), text: argsToText(p.args) });
      if (entry.consoleLogs.length > MAX_CONSOLE) entry.consoleLogs.shift();
    } else if (method === 'Log.entryAdded') {
      entry.consoleLogs.push({ time: Date.now(), text: String(p.entry?.text || '') });
      if (entry.consoleLogs.length > MAX_CONSOLE) entry.consoleLogs.shift();
    } else if (entry.recording && method === 'Network.requestWillBeSent') {
      // Same field names the Electron backend uses, so callers cannot tell
      // which one answered.
      entry.networkEntries.push({
        id: p.requestId,
        url: p.request?.url || '',
        method: p.request?.method || '',
        resourceType: (p.type || '').toLowerCase(),
        requestHeaders: p.request?.headers || {},
        timestamp: Date.now(),
      });
      if (entry.networkEntries.length > MAX_NETWORK) entry.networkEntries.shift();
    } else if (entry.recording && method === 'Network.responseReceived') {
      const e = entry.networkEntries.find((x) => x.id === p.requestId);
      if (e) {
        e.statusCode = p.response?.status;
        e.statusLine = p.response?.statusText || '';
        e.responseHeaders = p.response?.headers || {};
        e.fromCache = !!p.response?.fromDiskCache;
      }
    } else if (entry.recording && (method === 'Network.loadingFinished' || method === 'Network.loadingFailed')) {
      const e = entry.networkEntries.find((x) => x.id === p.requestId);
      if (e) {
        e.completedAt = Date.now();
        if (method === 'Network.loadingFailed') e.error = p.errorText || 'failed';
      }
    }
  });
}

/**
 * Navigate and wait for the page to settle.
 *
 * Resolves on load OR after a grace period rather than hanging: a page that
 * streams forever, or one whose load event never fires, is still a page you
 * want to read, and the desktop backend is equally forgiving here.
 */
async function navigate(entry, url) {
  const target = url || 'about:blank';
  let settled = false;
  const loaded = new Promise((resolve) => {
    const prev = sessionHandlers.get(entry.sessionId);
    sessionHandlers.set(entry.sessionId, (method, p) => {
      prev?.(method, p);
      if (!settled && (method === 'Page.loadEventFired' || method === 'Page.frameStoppedLoading')) {
        settled = true;
        sessionHandlers.set(entry.sessionId, prev);
        resolve();
      }
    });
  });
  const res = await cdpSend('Page.navigate', { url: target }, entry.sessionId);
  if (res.errorText) throw new Error(res.errorText + ' (' + target + ')');
  await Promise.race([loaded, new Promise((r) => setTimeout(r, LOAD_GRACE_MS))]);
  if (!settled) settled = true;
}

async function evaluate(entry, expression, awaitPromise = false) {
  const res = await cdpSend('Runtime.evaluate', {
    expression, returnByValue: true, awaitPromise, userGesture: true,
  }, entry.sessionId);
  if (res.exceptionDetails) {
    const d = res.exceptionDetails;
    throw new Error(d.exception?.description || d.text || 'Script error');
  }
  return res.result?.value;
}

async function currentUrl(entry) {
  try { return await evaluate(entry, 'location.href'); } catch { return ''; }
}

async function closePage(key) {
  const entry = pages.get(key);
  if (!entry) return;
  sessionHandlers.delete(entry.sessionId);
  pages.delete(key);
  try { await cdpSend('Target.closeTarget', { targetId: entry.targetId }); } catch { /* already gone */ }
  try { await cdpSend('Target.disposeBrowserContext', { browserContextId: entry.contextId }); } catch { /* ditto */ }
}

// ─── Operations ───
//
// One entry per message type in app/main.js's browserIpcHandlers, returning the
// same shapes. The page snippets are copied from there verbatim on purpose:
// two implementations of "read this page" that differ subtly is worse than one
// duplicated string.

const ops = {
  async browserOpen(msg) {
    const { key, url, width, height } = msg;
    if (pages.has(key)) return { ok: false, error: 'Browser "' + key + '" already open' };
    if (pages.size >= 20) return { ok: false, error: 'Too many browser instances (max 20)' };

    // A context per page, so cookies and storage do not bleed between them —
    // the headless equivalent of Electron's per-instance session partition.
    const ctx = await cdpSend('Target.createBrowserContext', { disposeOnDetach: false });
    const target = await cdpSend('Target.createTarget', {
      url: 'about:blank', browserContextId: ctx.browserContextId,
      width: width || 1280, height: height || 720,
    });
    const att = await cdpSend('Target.attachToTarget', { targetId: target.targetId, flatten: true });

    const entry = {
      key,
      targetId: target.targetId,
      sessionId: att.sessionId,
      contextId: ctx.browserContextId,
      consoleLogs: [],
      networkEntries: [],
      recording: false,
    };
    attachEvents(entry);
    pages.set(key, entry);

    try {
      await cdpSend('Page.enable', {}, entry.sessionId);
      await cdpSend('Runtime.enable', {}, entry.sessionId);
      await cdpSend('Log.enable', {}, entry.sessionId);
      await cdpSend('Network.enable', {}, entry.sessionId);
      await cdpSend('Emulation.setDeviceMetricsOverride', {
        width: width || 1280, height: height || 720, deviceScaleFactor: 1, mobile: false,
      }, entry.sessionId);
      // Headless has no one to answer a dialog, so an alert() would block the
      // page forever. Same reasoning as the desktop backend, different lever.
      await cdpSend('Page.setInterceptFileChooserDialog', { enabled: true }, entry.sessionId).catch(() => {});
      await navigate(entry, url);
      return { ok: true };
    } catch (err) {
      await closePage(key);
      return { ok: false, error: err.message };
    }
  },

  async browserClose(msg) {
    if (!pages.has(msg.key)) return { ok: false, error: 'Browser not found' };
    await closePage(msg.key);
    return { ok: true };
  },

  async browserNavigate(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await navigate(entry, msg.url);
    return { ok: true, url: await currentUrl(entry) };
  },

  async browserScreenshot(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const res = await cdpSend('Page.captureScreenshot', { format: 'png' }, entry.sessionId);
    return { ok: true, data: res.data };
  },

  async browserReadPage(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const text = await evaluate(entry, `
      (function() {
        // innerText on the LIVE body, not a clone. A cloned node is detached,
        // so it has no layout, so innerText silently degrades to textContent
        // and every line break is lost ("HeadingFirst para.Second para.").
        // Live innerText also already omits script/style/noscript, because
        // they are not rendered — the clone-and-remove dance was doing damage
        // to solve a problem that did not exist.
        return (document.body.innerText || '').substring(0, 50000);
      })()
    `);
    return {
      ok: true,
      title: await evaluate(entry, 'document.title'),
      url: await currentUrl(entry),
      text,
      consoleLogs: entry.consoleLogs.slice(-20),
    };
  },

  async browserSource(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    return { ok: true, html: await evaluate(entry, 'document.documentElement.outerHTML') };
  },

  async browserClick(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await evaluate(entry, `document.querySelector(${JSON.stringify(msg.selector)})?.click()`);
    return { ok: true };
  },

  async browserType(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    if (msg.selector) {
      await evaluate(entry, `document.querySelector(${JSON.stringify(msg.selector)})?.focus()`);
    }
    await cdpSend('Input.insertText', { text: String(msg.text ?? '') }, entry.sessionId);
    return { ok: true };
  },

  async browserFill(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await evaluate(entry, `
      (function() {
        const el = document.querySelector(${JSON.stringify(msg.selector)});
        if (!el) throw new Error('Element not found');
        el.focus();
        el.value = ${JSON.stringify(msg.value)};
        el.dispatchEvent(new Event('input', {bubbles:true}));
        el.dispatchEvent(new Event('change', {bubbles:true}));
      })()
    `);
    return { ok: true };
  },

  async browserSelect(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await evaluate(entry, `
      (function() {
        const el = document.querySelector(${JSON.stringify(msg.selector)});
        if (!el) throw new Error('Element not found');
        el.value = ${JSON.stringify(msg.value)};
        el.dispatchEvent(new Event('change', {bubbles:true}));
      })()
    `);
    return { ok: true };
  },

  async browserEval(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const result = await evaluate(entry, msg.code, true);
    return { ok: true, result: typeof result === 'string' ? result : JSON.stringify(result) };
  },

  async browserMouse(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const base = { x: msg.x, y: msg.y, button: 'left', clickCount: 1 };
    if (msg.action === 'click') {
      await cdpSend('Input.dispatchMouseEvent', { ...base, type: 'mousePressed' }, entry.sessionId);
      await cdpSend('Input.dispatchMouseEvent', { ...base, type: 'mouseReleased' }, entry.sessionId);
    } else {
      await cdpSend('Input.dispatchMouseEvent', { type: 'mouseMoved', x: msg.x, y: msg.y }, entry.sessionId);
    }
    return { ok: true };
  },

  async browserResize(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await cdpSend('Emulation.setDeviceMetricsOverride', {
      width: msg.width, height: msg.height, deviceScaleFactor: 1, mobile: false,
    }, entry.sessionId);
    return { ok: true };
  },

  async browserScroll(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await evaluate(entry, `
      (function() {
        const el = ${msg.selector ? 'document.querySelector(' + JSON.stringify(msg.selector) + ')' : 'window'};
        if (el) el.scrollBy(${msg.x || 0}, ${msg.y || 0});
      })()
    `);
    return { ok: true };
  },

  async browserWait(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const timeout = msg.timeout || 30000;
    await evaluate(entry, `
      new Promise((resolve, reject) => {
        const el = document.querySelector(${JSON.stringify(msg.selector)});
        if (el) return resolve(true);
        const obs = new MutationObserver(() => {
          if (document.querySelector(${JSON.stringify(msg.selector)})) { obs.disconnect(); resolve(true); }
        });
        obs.observe(document.body, { childList: true, subtree: true });
        setTimeout(() => { obs.disconnect(); reject(new Error('Timeout waiting for ' + ${JSON.stringify(msg.selector)})); }, ${timeout});
      })
    `, true);
    return { ok: true };
  },

  async browserGoBack(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const h = await cdpSend('Page.getNavigationHistory', {}, entry.sessionId);
    const prev = h.entries?.[h.currentIndex - 1];
    if (prev) await cdpSend('Page.navigateToHistoryEntry', { entryId: prev.id }, entry.sessionId);
    return { ok: true };
  },

  async browserGoForward(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const h = await cdpSend('Page.getNavigationHistory', {}, entry.sessionId);
    const next = h.entries?.[h.currentIndex + 1];
    if (next) await cdpSend('Page.navigateToHistoryEntry', { entryId: next.id }, entry.sessionId);
    return { ok: true };
  },

  async browserConsole(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    let logs = entry.consoleLogs;
    if (msg.countOnly) return { ok: true, count: logs.length };
    const start = msg.start || 0;
    const end = msg.end || logs.length;
    logs = logs.slice(start, end);
    if (msg.clear) entry.consoleLogs = [];
    return { ok: true, logs };
  },

  async browserNetwork(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    if (msg.action === 'start') { entry.recording = true; entry.networkEntries = []; return { ok: true, recording: true }; }
    if (msg.action === 'stop') { entry.recording = false; return { ok: true, recording: false }; }
    if (msg.action === 'clear') { entry.networkEntries = []; return { ok: true, recording: entry.recording }; }
    if (msg.action === 'log') {
      const total = entry.networkEntries.length;
      if (msg.countOnly) return { ok: true, recording: entry.recording, total };
      const requests = entry.networkEntries.slice(msg.start || 0, msg.end || undefined);
      return { ok: true, requests, recording: entry.recording, total };
    }
    return { ok: false, error: 'Unknown network action' };
  },

  async browserCookies(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    if (!msg.action || msg.action === 'get') {
      const res = await cdpSend('Network.getCookies', {}, entry.sessionId);
      let cookies = res.cookies || [];
      const f = msg.filter || {};
      // Electron filters server-side; CDP does not, so the same filter keys are
      // applied here rather than handing back everything and calling it a match.
      if (f.name) cookies = cookies.filter((c) => c.name === f.name);
      if (f.domain) cookies = cookies.filter((c) => (c.domain || '').includes(f.domain));
      if (f.path) cookies = cookies.filter((c) => c.path === f.path);
      if (f.secure !== undefined) cookies = cookies.filter((c) => !!c.secure === !!f.secure);
      if (f.session !== undefined) cookies = cookies.filter((c) => (c.expires === -1) === !!f.session);
      return { ok: true, cookies };
    }
    if (msg.action === 'set' && msg.cookie) {
      const c = msg.cookie;
      const params = { name: c.name, value: c.value ?? '' };
      if (c.url) params.url = c.url; else params.url = await currentUrl(entry);
      if (c.domain) params.domain = c.domain;
      if (c.path) params.path = c.path;
      if (c.secure !== undefined) params.secure = !!c.secure;
      if (c.httpOnly !== undefined) params.httpOnly = !!c.httpOnly;
      if (c.expirationDate) params.expires = c.expirationDate;
      const res = await cdpSend('Network.setCookie', params, entry.sessionId);
      if (res.success === false) return { ok: false, error: 'The browser rejected that cookie' };
      return { ok: true };
    }
    if (msg.action === 'delete') {
      const url = msg.url || await currentUrl(entry);
      await cdpSend('Network.deleteCookies', { name: msg.cookie?.name || '', url }, entry.sessionId);
      return { ok: true };
    }
    return { ok: false, error: 'Unknown cookie action' };
  },

  async browserSnapshot(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const tree = await evaluate(entry, `
      (function walk(el, depth) {
        if (depth > ${msg.maxDepth || 10}) return null;
        const role = el.getAttribute?.('role') || el.tagName?.toLowerCase() || '';
        const label = el.getAttribute?.('aria-label') || el.textContent?.substring(0, 80) || '';
        const children = [];
        for (const c of (el.children || [])) { const r = walk(c, depth+1); if(r) children.push(r); }
        return { role, label: label.trim(), children: children.length ? children : undefined };
      })(document.body, 0)
    `);
    const text = JSON.stringify(tree, null, 2);
    return { ok: true, snapshot: text.substring(0, msg.maxLength || 50000) };
  },

  async browserElements(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const sel = msg.selector || 'a,button,input,select,textarea,[role="button"],[onclick]';
    const elements = await evaluate(entry, `
      Array.from(document.querySelectorAll(${JSON.stringify(sel)})).map(el => ({
        tag: el.tagName.toLowerCase(),
        id: el.id || undefined,
        class: el.className || undefined,
        text: (el.textContent || '').trim().substring(0, 80),
        type: el.type || undefined,
        href: el.href || undefined,
        value: el.value || undefined,
      }))
    `);
    if (msg.countOnly) return { ok: true, count: elements.length };
    return { ok: true, elements: elements.slice(msg.start || 0, msg.end || elements.length) };
  },

  async browserPdf(msg) {
    const entry = page(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const res = await cdpSend('Page.printToPDF', {
      landscape: msg.landscape || false, printBackground: true,
    }, entry.sessionId);
    return { ok: true, data: res.data };
  },
};

export function supports(type) { return Object.prototype.hasOwnProperty.call(ops, type); }

/**
 * Run one browser operation. Mirrors the reply shape the desktop host sends
 * back over IPC, so browser.js cannot tell which backend answered.
 */
export async function handle(msg) {
  const op = ops[msg?.type];
  if (!op) return { ok: false, error: 'Unsupported browser operation: ' + (msg?.type || '?') };
  try {
    await ensureBrowser();
    return await op(msg);
  } catch (err) {
    return { ok: false, error: err.message || String(err) };
  }
}
