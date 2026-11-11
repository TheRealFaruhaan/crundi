/**
 * browser-drawer.js — the main-process side of the PC client's browser drawer.
 *
 * The drawer (app/vendor/crundi-browser-drawer.js, in the web UI) holds one
 * Electron <webview> per tab, inside the page layout, so overlays draw on top
 * of it and it narrows and moves with the page. A <webview> reloads if it is
 * ever moved in the DOM, so the drawer never moves one; hidden tabs and a
 * hidden drawer stay laid out at full size with opacity 0, which keeps them
 * rendering and capturable (display:none shrinks them to 0x0; visibility
 * hidden, off-screen or clipped makes capturePage fail).
 *
 * This module:
 *  - hardens every <webview> the page creates (will-attach-webview): no
 *    preload, no Node, sandboxed, and only the drawer's two partitions;
 *  - keeps tabId → the tab's webContents, registered by the page after the
 *    guest attaches, and checked to be a guest of the window that asked;
 *  - serves the page (navigation, device emulation over the in-process
 *    debugger, capture, DevTools) and relays page state back to it;
 *  - serves the server: the browserDrawer* handlers, and a drawer tab as an
 *    entry the generic browser handlers in main.js can drive, so every
 *    browser_* tool works on "drawer:<tabId>". Those requests reach us over
 *    the same channel as the hidden browsers: our own server's IPC, or the
 *    native-host websocket of a server on another machine. No CDP port is
 *    ever opened.
 *
 * Each tab has a network. "client" uses this machine's network; "server"
 * uses a separate partition whose proxy is the loopback tunnel to the server
 * (net-tunnel-client.js), or no proxy when the server is this machine.
 */

import { webContents as WC, session as Session } from 'electron';
import { readFileSync, writeFileSync, renameSync } from 'node:fs';

export const PARTITIONS = { client: 'persist:crundi-browser', server: 'persist:crundi-browser-srv' };
const PREFIX = 'drawer:';
const MOBILE_UA = 'Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36';
const REQUEST_TIMEOUT_MS = 20000;
const MAX_CONSOLE = 500;

export function isDrawerKey(key) { return typeof key === 'string' && key.startsWith(PREFIX); }
function tabIdOf(key) { return String(key).slice(PREFIX.length); }
const TAB_ID = /^[A-Za-z0-9_-]{4,64}$/;
const okUrl = (u) => /^(https?:\/\/|about:blank$)/i.test(String(u || ''));
const lc = (s) => String(s || '').toLowerCase();

/**
 * deps:
 *   ipcMain, getWindow() → the main BrowserWindow (or null)
 *   log(line)
 *   loadUrl(wc, url) → Promise, resolves when loaded enough (main.js loadUrlWithGrace)
 *   serverNet: { info() → { mode: 'local'|'remote', status: 'direct'|'tunnel'|'offline'|'unsupported' },
 *                ensure() → Promise (the server partition's proxy is set) }
 */
export function createBrowserDrawer({ ipcMain, getWindow, log = () => {}, loadUrl, serverNet, freezeAfterMs = 5 * 60 * 1000, discardAfterMs = 60 * 60 * 1000, historyFile = null }) {
  const tabs = new Map();       // tabId → entry
  const pending = new Map();    // reqId → { resolve, reject, timer }
  let nextReq = 1;
  let readyId = null;           // webContents id of the page whose drawer is ready
  const netHooked = new Set();  // sessions with the network-capture listeners

  function networkLabel(network) {
    if (network !== 'server') return 'This PC';
    const i = serverNet.info();
    if (i.mode === 'local') return 'Server (this PC)';
    if (i.status === 'tunnel') return 'Server';
    if (i.status === 'unsupported') return 'Server (needs a newer server)';
    return 'Server (not connected)';
  }

  function win() { const w = getWindow(); return w && !w.isDestroyed() ? w : null; }

  // ─── History ───
  // The drawer is one browser per client: every tab in every project shares
  // its profile (cookies, logins, storage), and this history. Electron keeps
  // none of its own beyond each tab's back/forward list, so it is kept here,
  // in a file in the app's data folder: one entry per address, newest first.
  const HISTORY_MAX = 5000;
  let history = [];
  try { if (historyFile) { const j = JSON.parse(readFileSync(historyFile, 'utf8')); if (Array.isArray(j)) history = j.filter((h) => h && typeof h.url === 'string'); } } catch { /* none yet */ }
  let historyTimer = null;
  function saveHistory() {
    if (!historyFile || historyTimer) return;
    historyTimer = setTimeout(() => {
      historyTimer = null;
      try { writeFileSync(historyFile + '.tmp', JSON.stringify(history)); renameSync(historyFile + '.tmp', historyFile); } catch (err) { log(`[drawer] history not saved: ${err.message}`); }
    }, 1000);
    historyTimer.unref?.();
  }
  function recordVisit(url, title) {
    if (!/^https?:\/\//i.test(url)) return;
    const i = history.findIndex((h) => h.url === url);
    const old = i >= 0 ? history.splice(i, 1)[0] : null;
    history.unshift({ url, title: title || (old && old.title) || '', at: Date.now(), visits: ((old && old.visits) || 0) + 1 });
    if (history.length > HISTORY_MAX) history.length = HISTORY_MAX;
    saveHistory();
  }
  function retitle(url, title) {
    const h = history.find((x) => x.url === url);
    if (h && title && h.title !== title) { h.title = title; saveHistory(); }
  }
  function queryHistory(q, limit) {
    const n = Math.max(1, Math.min(200, Number(limit) || 8));
    const words = String(q || '').toLowerCase().split(/\s+/).filter(Boolean);
    const out = [];
    for (const h of history) {
      const hay = (h.url + ' ' + h.title).toLowerCase();
      if (words.every((w) => hay.includes(w))) out.push(h);
      if (out.length >= n) break;
    }
    return out;
  }

  // ─── Requests to the page (main → renderer) ───
  function request(cmd, args, timeoutMs = REQUEST_TIMEOUT_MS) {
    const w = win();
    if (!w || readyId !== w.webContents.id) {
      return Promise.reject(new Error('The browser drawer is not available: the Crundi window is not showing a web UI that has it (open the app, or update the server).'));
    }
    return new Promise((resolve, reject) => {
      const reqId = nextReq++;
      const timer = setTimeout(() => { pending.delete(reqId); reject(new Error(`The browser drawer did not answer (${cmd})`)); }, timeoutMs);
      pending.set(reqId, { resolve, reject, timer });
      try { w.webContents.send('drawer:request', { reqId, cmd, args }); }
      catch (err) { clearTimeout(timer); pending.delete(reqId); reject(err); }
    });
  }
  ipcMain.on('drawer:reply', (e, reqId, result) => {
    const p = pending.get(reqId); if (!p) return;
    pending.delete(reqId); clearTimeout(p.timer);
    const w = win();
    if (!w || e.sender.id !== w.webContents.id) { p.reject(new Error('reply from another page')); return; }
    if (result && result.ok === false) p.reject(new Error(result.error || 'failed'));
    else p.resolve(result || {});
  });
  ipcMain.on('drawer:ready', (e) => { const w = win(); if (w && e.sender.id === w.webContents.id) { readyId = e.sender.id; log('[drawer] page ready'); } });

  function emit(channel, payload) { const w = win(); if (w) { try { w.webContents.send(channel, payload); } catch { /* page gone */ } } }

  function canGo(wc, dir) {
    try { const h = wc.navigationHistory; if (h) return dir === 'back' ? h.canGoBack() : h.canGoForward(); return dir === 'back' ? wc.canGoBack() : wc.canGoForward(); } catch { return false; }
  }
  function emitState(entry, extra) {
    const wc = entry.wc; if (!wc || wc.isDestroyed()) return;
    let url = '', title = '', loading = false;
    try { url = wc.getURL(); title = wc.getTitle(); loading = wc.isLoading(); } catch { /* ignore */ }
    emit('drawer:state', Object.assign({ tabId: entry.tabId, url, title, loading, canGoBack: canGo(wc, 'back'), canGoForward: canGo(wc, 'forward') }, extra || {}));
  }

  // ─── Network capture for browser_network, per tab ───
  function hookNetwork(ses) {
    if (netHooked.has(ses)) return;
    netHooked.add(ses);
    const byWc = (id) => { for (const e of tabs.values()) if (e.wc && !e.wc.isDestroyed() && e.wc.id === id) return e; return null; };
    ses.webRequest.onBeforeSendHeaders((d, cb) => {
      const e = byWc(d.webContentsId);
      if (e && e.recording) e.networkEntries.push({ id: d.id, url: d.url, method: d.method, resourceType: d.resourceType, requestHeaders: d.requestHeaders, timestamp: d.timestamp || Date.now() });
      cb({ cancel: false, requestHeaders: d.requestHeaders });
    });
    ses.webRequest.onCompleted((d) => {
      const e = byWc(d.webContentsId); if (!e || !e.recording) return;
      const x = e.networkEntries.find((n) => n.id === d.id);
      if (x) Object.assign(x, { statusCode: d.statusCode, statusLine: d.statusLine, responseHeaders: d.responseHeaders, fromCache: d.fromCache, completedAt: Date.now() });
    });
    ses.webRequest.onErrorOccurred((d) => {
      const e = byWc(d.webContentsId); if (!e || !e.recording) return;
      const x = e.networkEntries.find((n) => n.id === d.id);
      if (x) Object.assign(x, { error: d.error, completedAt: Date.now() });
    });
  }

  function newEntry(tabId, project, network) {
    const entry = {
      tabId, project, network, wc: null,
      // The generic handlers in main.js read entry.win.webContents.
      win: { webContents: null, setContentSize() { /* drawer tabs resize through browserResize */ } },
      ses: null, consoleLogs: [], networkEntries: [], recording: false,
      emu: undefined, waiters: [], drawer: true,
      // Sleep: a tab nobody is looking at or using is frozen (see sweep).
      visible: false, lastUse: Date.now(), frozen: false,
    };
    tabs.set(tabId, entry);
    return entry;
  }

  function wire(entry, wc) {
    const tabId = entry.tabId;
    const st = () => { if (entry.wc === wc) emitState(entry); };
    ['did-navigate', 'did-navigate-in-page', 'page-title-updated', 'did-stop-loading', 'did-start-loading'].forEach((ev) => wc.on(ev, st));
    wc.on('did-navigate', (_e, url, code) => { if (!code || code < 400) recordVisit(url, wc.getTitle()); });
    wc.on('page-title-updated', (_e, title) => { try { retitle(wc.getURL(), title); } catch { /* gone */ } });
    wc.on('did-fail-load', (_e, code, desc, url, isMain) => {
      if (isMain && code !== -3 && entry.wc === wc) { log(`[drawer] fail tab=${tabId} ${code} ${desc} ${url}`); emitState(entry, { diag: `Load failed (${code}): ${desc}` }); }
    });
    wc.on('console-message', (...a) => {
      // Electron 41 passes one event object; older ones (event, level, message).
      const ev = a[0] || {};
      const text = typeof a[2] === 'string' ? a[2] : (ev.message || '');
      if (entry.wc !== wc) return;
      entry.consoleLogs.push({ time: Date.now(), text });
      if (entry.consoleLogs.length > MAX_CONSOLE) entry.consoleLogs.splice(0, entry.consoleLogs.length - MAX_CONSOLE);
    });
    wc.on('render-process-gone', (_e, d) => { log(`[drawer] page process gone tab=${tabId} ${d && d.reason}`); if (entry.wc === wc) emitState(entry, { diag: 'The page crashed. Reload to try again.', crashed: true }); });
    wc.on('will-prevent-unload', (e) => e.preventDefault());
    // A link that opens a window becomes a new tab in the same project, on the same network.
    wc.setWindowOpenHandler(({ url }) => {
      // It becomes the project's active tab, but does not pop the drawer open:
      // a chat may have clicked it while the person has the drawer hidden.
      if (okUrl(url)) openTab({ project: entry.project, url, network: entry.network, show: false, activate: true }).catch((err) => log(`[drawer] popup tab failed: ${err.message}`));
      return { action: 'deny' };
    });
    wc.once('destroyed', () => { if (entry.wc === wc) { entry.wc = null; entry.win.webContents = null; } });
  }

  // ─── The window: harden every <webview> its page creates ───
  function hardenWindow(w) {
    const host = w.webContents;
    host.on('will-attach-webview', (event, prefs, params) => {
      const part = params.partition;
      if (!Object.values(PARTITIONS).includes(part)) { log(`[drawer] refused a webview in partition ${part}`); event.preventDefault(); return; }
      if (params.src && !okUrl(params.src)) { log(`[drawer] refused a webview at ${String(params.src).slice(0, 80)}`); event.preventDefault(); return; }
      delete prefs.preload; delete prefs.preloadURL;
      prefs.nodeIntegration = false;
      prefs.nodeIntegrationInSubFrames = false;
      prefs.nodeIntegrationInWorker = false;
      prefs.contextIsolation = true;
      prefs.sandbox = true;
      prefs.webSecurity = true;
      prefs.allowRunningInsecureContent = false;
      prefs.experimentalFeatures = false;
      prefs.enableBlinkFeatures = '';
      prefs.webviewTag = false;
      prefs.partition = part;
      // allowpopups stays: without it window.open and target=_blank links do
      // nothing at all. Every one still goes to setWindowOpenHandler (wire),
      // which refuses the window and opens a drawer tab instead.
    });
    // A reload or a new page: every guest went with the old one.
    host.on('did-start-navigation', (d) => { if (d && d.isMainFrame && !d.isSameDocument) { if (readyId === host.id) readyId = null; } });
  }

  // ─── The page's own calls ───
  const getTab = (tabId) => { const e = tabs.get(tabId); return e && e.wc && !e.wc.isDestroyed() ? e : null; };

  ipcMain.handle('drawer:attach', async (e, a) => {
    const { tabId, project, network, wcId } = a || {};
    if (!TAB_ID.test(String(tabId || ''))) return { ok: false, error: 'bad tab id' };
    if (!PARTITIONS[network]) return { ok: false, error: 'bad network' };
    const wc = WC.fromId(Number(wcId));
    if (!wc || !wc.hostWebContents || wc.hostWebContents.id !== e.sender.id) return { ok: false, error: 'not a guest of this window' };
    if (wc.session !== Session.fromPartition(PARTITIONS[network])) return { ok: false, error: 'the tab is not in that network\'s partition' };
    if (network === 'server') { try { await serverNet.ensure(); } catch (err) { log(`[drawer] server network setup: ${err.message}`); } }
    let entry = tabs.get(tabId);
    if (!entry) entry = newEntry(tabId, lc(project), network);
    entry.project = lc(project); entry.network = network;
    entry.wc = wc; entry.win.webContents = wc; entry.ses = wc.session; entry.emu = undefined;
    entry.frozen = false; entry.lastUse = Date.now();
    delete entry.touch; delete entry.castHooked;
    hookNetwork(wc.session);
    wire(entry, wc);
    log(`[drawer] attach tab=${tabId} project=${entry.project} network=${network}`);
    const w = entry.waiters.splice(0); w.forEach((fn) => fn());
    return { ok: true, networkLabel: networkLabel(network) };
  });
  ipcMain.handle('drawer:detach', (_e, tabId) => { const en = tabs.get(tabId); if (en) { tabs.delete(tabId); log(`[drawer] close tab=${tabId}`); } return { ok: true }; });
  ipcMain.handle('drawer:navigate', (_e, tabId, url) => {
    const en = getTab(tabId); if (!en) return { ok: false, error: 'no such tab' };
    if (!okUrl(url)) return { ok: false, error: 'Only http and https addresses open here.' };
    en.wc.loadURL(url).catch(() => { /* reported through did-fail-load */ });
    return { ok: true };
  });
  ipcMain.handle('drawer:nav', (_e, tabId, dir) => {
    const en = getTab(tabId); if (!en) return { ok: false };
    const wc = en.wc; const h = wc.navigationHistory;
    try {
      if (dir === 'reload') wc.reload();
      else if (dir === 'stop') wc.stop();
      else if (dir === 'back' && canGo(wc, 'back')) { h ? h.goBack() : wc.goBack(); }
      else if (dir === 'forward' && canGo(wc, 'forward')) { h ? h.goForward() : wc.goForward(); }
      return { ok: true };
    } catch (err) { return { ok: false, error: err.message }; }
  });
  ipcMain.handle('drawer:emulate', async (_e, tabId, config, scale) => {
    const en = getTab(tabId); if (!en) return { ok: false };
    try { await emulate(en, config || null, scale || 1); return { ok: true }; }
    catch (err) { log(`[drawer] emulate ${err.message}`); return { ok: false, error: err.message }; }
  });
  ipcMain.handle('drawer:devtools', (_e, tabId) => {
    const en = getTab(tabId); if (!en) return { ok: false };
    try { en.wc.isDevToolsOpened() ? en.wc.closeDevTools() : en.wc.openDevTools({ mode: 'detach' }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; }
  });
  ipcMain.handle('drawer:capture', async (_e, tabId) => {
    const en = getTab(tabId); if (!en) return { ok: false, error: 'no such tab' };
    try { const img = await en.wc.capturePage(); return { ok: true, data: img.toPNG().toString('base64'), size: img.getSize() }; }
    catch (err) { return { ok: false, error: err.message }; }
  });
  ipcMain.handle('drawer:describe', async (_e, tabId, points) => {
    const en = getTab(tabId); if (!en) return { ok: false, error: 'no such tab' };
    try { return { ok: true, elements: await describe(en.wc, points) }; } catch (err) { return { ok: false, error: err.message }; }
  });
  // The page says which tab is on screen (at most one: the open drawer's
  // shown tab). Showing a tab wakes it.
  ipcMain.handle('drawer:visible', async (_e, tabId, visible) => {
    const en = tabs.get(tabId); if (!en) return { ok: false };
    en.visible = !!visible; en.lastUse = Date.now();
    if (visible && en.wc && !en.wc.isDestroyed()) { try { await thaw(en); } catch (err) { log(`[drawer] wake ${err.message}`); } }
    return { ok: true };
  });

  ipcMain.handle('drawer:history', (_e, q, limit) => ({ ok: true, items: queryHistory(q, limit) }));

  // Clear browsing data, for this client's browser (both its profiles: the
  // one for this PC's network and the one for the server's).
  //   history  the drawer's history, and every tab's back/forward list
  //   cookies  cookies and site data: localStorage, IndexedDB, service
  //            workers, cache storage (this signs you out of sites)
  //   cache    cached images and files
  ipcMain.handle('drawer:clear', async (_e, what) => {
    const w = what || {};
    const done = [];
    if (w.history) {
      history = []; saveHistory();
      for (const en of tabs.values()) { try { if (en.wc && !en.wc.isDestroyed()) en.wc.navigationHistory.clear(); } catch { /* old Electron */ } emitState(en); }
      done.push('history');
    }
    for (const part of Object.values(PARTITIONS)) {
      const ses = Session.fromPartition(part);
      if (w.cookies) { await ses.clearStorageData(); await ses.clearAuthCache(); }
      if (w.cache) { await ses.clearCache(); try { await ses.clearCodeCaches({}); } catch { /* not there */ } }
    }
    if (w.cookies) done.push('cookies and site data');
    if (w.cache) done.push('cached files');
    log(`[drawer] cleared ${done.join(', ') || 'nothing'}`);
    return { ok: true, cleared: done };
  });

  ipcMain.handle('drawer:info', () => ({ ok: true, ...serverNet.info(), labels: { client: networkLabel('client'), server: networkLabel('server') } }));

  // ─── Sleep ───
  // Hidden tabs stay laid out and rendering (so a chat can use them), which
  // costs CPU. One that has been neither on screen nor used for a while is
  // frozen through the debugger (Page.setWebLifecycleState): its scripts,
  // timers and painting stop, and the page stays in memory as it was, so
  // nothing reloads. It wakes when shown, or before any server call on it.
  // A tab playing sound, or with DevTools open, is left awake.
  async function lifecycle(en, state) {
    const dbg = en.wc.debugger;
    try { dbg.attach('1.3'); } catch (err) { if (!/already attached/i.test(err.message)) throw err; }
    await dbg.sendCommand('Page.setWebLifecycleState', { state });
  }
  async function freeze(en) {
    if (en.frozen || !en.wc || en.wc.isDestroyed()) return;
    await lifecycle(en, 'frozen');
    en.frozen = true; en.frozenAt = Date.now();
    log(`[drawer] asleep tab=${en.tabId}`);
    emitState(en, { sleeping: true });
  }
  async function thaw(en) {
    en.lastUse = Date.now();
    if (!en.frozen || !en.wc || en.wc.isDestroyed()) return;
    await lifecycle(en, 'active');
    en.frozen = false;
    log(`[drawer] awake tab=${en.tabId}`);
    emitState(en, { sleeping: false });
  }
  // Asleep for an hour (discardAfterMs): the page process is closed to give
  // its memory back. The tab keeps its address, title, device and network,
  // and loads again when it is shown or a chat uses it. That is the one
  // time a drawer tab reloads.
  function discard(en) {
    if (en.discarding) return;
    en.discarding = true;
    request('discard', { tabId: en.tabId })
      .then(() => { log(`[drawer] unloaded tab=${en.tabId} (asleep ${Math.round((Date.now() - en.frozenAt) / 60000)} min)`); en.frozen = false; })
      .catch((err) => log(`[drawer] unload ${err.message}`))
      .finally(() => { en.discarding = false; });
  }
  let windowHiddenSince = 0;
  function sweep() {
    const w = win();
    const shown = !!(w && w.isVisible() && !w.isMinimized());
    const now = Date.now();
    if (shown) windowHiddenSince = 0; else if (!windowHiddenSince) windowHiddenSince = now;
    for (const en of tabs.values()) {
      if (!en.wc || en.wc.isDestroyed()) continue;
      if (en.frozen) { if (now - en.frozenAt >= discardAfterMs) discard(en); continue; }
      // On screen counts only while the window is; a hidden window's tab
      // starts its idle time when the window went away.
      const since = en.visible ? (shown ? now : Math.max(en.lastUse, windowHiddenSince)) : en.lastUse;
      if (now - since < freezeAfterMs) continue;
      let busy = false;
      try { busy = en.wc.isCurrentlyAudible() || en.wc.isDevToolsOpened(); } catch { busy = true; }
      if (busy) continue;
      freeze(en).catch((err) => log(`[drawer] sleep ${err.message}`));
    }
  }
  const sweepTimer = setInterval(sweep, Math.max(250, Math.min(30000, Math.floor(freezeAfterMs / 4))));
  sweepTimer.unref?.();

  /** Tell the page that what "server network" means has changed (connected, dropped). */
  function notifyInfo() { emit('drawer:info', { ...serverNet.info(), labels: { client: networkLabel('client'), server: networkLabel('server') } }); }

  // Device emulation through the in-process debugger: the page renders at the
  // device's true size and `scale` shrinks the picture to the box the drawer
  // gave the <webview>. Reload only when the device itself changes.
  async function emulate(en, config, scale) {
    const wc = en.wc; const dbg = wc.debugger;
    const prev = en.emu === undefined ? null : en.emu;
    const first = en.emu === undefined;
    en.emu = config;
    const changed = (!!config !== !!prev) || (config && prev && (config.width !== prev.width || config.height !== prev.height || config.mobile !== prev.mobile || config.ua !== prev.ua));
    if (config) {
      try { dbg.attach('1.3'); } catch (err) { if (!/already attached/i.test(err.message)) throw err; }
      await dbg.sendCommand('Emulation.setDeviceMetricsOverride', {
        width: config.width, height: config.height, deviceScaleFactor: config.deviceScaleFactor || 0, mobile: !!config.mobile,
        scale: scale || 1, screenWidth: config.width, screenHeight: config.height,
      });
      // maxTouchPoints must be 1-16 even when switching touch off.
      await dbg.sendCommand('Emulation.setTouchEmulationEnabled', config.mobile ? { enabled: true, maxTouchPoints: 5 } : { enabled: false });
      await dbg.sendCommand('Emulation.setEmitTouchEventsForMouse', { enabled: !!config.mobile, configuration: 'mobile' });
      if (changed) {
        await dbg.sendCommand('Emulation.setUserAgentOverride', { userAgent: config.ua || (config.mobile ? MOBILE_UA : '') });
        const url = wc.getURL();
        if (url && url !== 'about:blank') { log(`[drawer] device tab=${en.tabId} ${config.width}x${config.height} reload`); wc.reload(); }
      }
    } else if (!first) {
      if (dbg.isAttached()) {
        for (const [m, p] of [['Emulation.setEmitTouchEventsForMouse', { enabled: false }], ['Emulation.setTouchEmulationEnabled', { enabled: false }], ['Emulation.setUserAgentOverride', { userAgent: '' }], ['Emulation.clearDeviceMetricsOverride', undefined]]) {
          try { await dbg.sendCommand(m, p); } catch { /* ignore */ }
        }
      }
      if (changed) { const url = wc.getURL(); if (url && url !== 'about:blank') wc.reload(); }
    }
  }

  // What is under each point (page CSS pixels, viewport), for marks a chat reads.
  async function describe(wc, points) {
    const pts = (Array.isArray(points) ? points : []).slice(0, 60).map((p) => [Number(p[0]) || 0, Number(p[1]) || 0]);
    if (!pts.length) return [];
    return wc.executeJavaScript(`(function(pts){
      function sel(e){ var parts=[]; while(e&&e.nodeType===1&&parts.length<5){ if(e.id){parts.unshift('#'+CSS.escape(e.id));break;}
        var s=e.tagName.toLowerCase(); var c=(typeof e.className==='string'&&e.className.trim())?e.className.trim().split(/\\s+/).slice(0,2).map(function(x){return '.'+CSS.escape(x);}).join(''):'';
        var par=e.parentElement; if(par){var same=[].filter.call(par.children,function(k){return k.tagName===e.tagName;}); if(same.length>1) s+=':nth-of-type('+(same.indexOf(e)+1)+')';}
        parts.unshift(s+c); e=par; } return parts.join(' > '); }
      return pts.map(function(p){ var el=document.elementFromPoint(p[0],p[1]); if(!el) return null; var r=el.getBoundingClientRect();
        var open=el.outerHTML; var gt=open.indexOf('>'); open=(gt>0?open.slice(0,gt+1):open).slice(0,300);
        return { selector: sel(el), tag: el.tagName.toLowerCase(), text: String(el.innerText||el.value||el.alt||el.getAttribute('aria-label')||'').trim().replace(/\\s+/g,' ').slice(0,160),
          html: open, rect: { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) } }; });
    })(${JSON.stringify(pts)})`, true);
  }

  async function rectOf(wc, selector) {
    return wc.executeJavaScript(`(function(s){ var e; try { e=document.querySelector(s); } catch(_) { return { error: 'bad selector' }; } if(!e) return null;
      var r=e.getBoundingClientRect(); if (r.bottom<0||r.top>innerHeight||r.right<0||r.left>innerWidth) { e.scrollIntoView({block:'center',inline:'center'}); r=e.getBoundingClientRect(); }
      return { x: r.x, y: r.y, width: r.width, height: r.height }; })(${JSON.stringify(selector)})`, true);
  }

  // ─── For the server ───

  /** Before any server call on a drawer key: the tab exists (made if needed) and belongs to the caller's project. */
  async function prepare(msg) {
    const tabId = tabIdOf(msg.key);
    if (!TAB_ID.test(tabId)) throw new Error('No such drawer tab');
    let en = getTab(tabId);
    if (!en) {
      // Known to the page but not loaded yet (a tab restored at startup, or
      // the window reloaded): the page makes its <webview> and we wait.
      const r = await request('ensure', { tabId, project: msg.alias || '' });
      en = getTab(tabId);
      if (!en) await new Promise((resolve, reject) => {
        const e2 = tabs.get(tabId) || newEntry(tabId, lc(r.project), r.network || 'client');
        const t = setTimeout(() => reject(new Error('The drawer tab did not load')), REQUEST_TIMEOUT_MS);
        e2.waiters.push(() => { clearTimeout(t); resolve(); });
      });
      en = getTab(tabId);
    }
    if (!en) throw new Error('No such drawer tab');
    if (msg.alias && lc(msg.alias) !== en.project) throw new Error('That drawer tab belongs to another project.');
    // A chat is using it: wake it first (and restart its idle time).
    try { await thaw(en); } catch (err) { log(`[drawer] wake ${err.message}`); }
    return en;
  }

  function getEntry(key) { return isDrawerKey(key) ? getTab(tabIdOf(key)) : null; }

  async function openTab({ project, url, network, show, activate }) {
    const r = await request('open', { project: lc(project), network: PARTITIONS[network] ? network : 'client', show: !!show, activate: activate !== false });
    let en = getTab(r.tabId);
    if (!en) throw new Error('The drawer tab did not load');
    if (url && url !== 'about:blank') {
      try { await loadUrl(en.wc, url); } catch (err) { return { tabId: r.tabId, entry: en, warning: err.message }; }
    }
    return { tabId: r.tabId, entry: en };
  }

  const handlers = {
    async browserDrawerList(msg) {
      const r = await request('list', { project: lc(msg.alias) });
      const list = (r.tabs || []).map((t) => { const en = tabs.get(t.tabId); return { ...t, networkLabel: networkLabel(t.network), sleeping: !!(en && en.frozen && en.wc) }; });
      return { ok: true, tabs: list };
    },
    async browserDrawerOpen(msg) {
      if (msg.url && !okUrl(msg.url)) return { ok: false, error: 'Only http and https addresses open in the drawer.' };
      if (!msg.alias) return { ok: false, error: 'Which project? (no alias)' };
      const o = await openTab({ project: msg.alias, url: msg.url, network: msg.network, show: msg.show, activate: !!msg.show });
      return { ok: true, tabId: o.tabId, network: o.entry.network, networkLabel: networkLabel(o.entry.network), url: o.entry.wc.getURL(), ...(o.warning ? { warning: o.warning } : {}) };
    },
    async browserDrawerSetNetwork(msg) {
      const en = await prepare(msg);
      if (!PARTITIONS[msg.network]) return { ok: false, error: 'network must be "client" or "server"' };
      await request('set-network', { tabId: en.tabId, network: msg.network });
      const now = getTab(en.tabId);
      return { ok: true, network: msg.network, networkLabel: networkLabel(msg.network), url: now ? now.wc.getURL() : '' };
    },
    async browserDrawerAnnotate(msg) {
      const en = await prepare(msg);
      const shapes = [];
      for (const s of msg.shapes || []) {
        const c = { ...s };
        if (c.selector) {
          const r = await rectOf(en.wc, c.selector);
          if (!r) return { ok: false, error: `No element matches ${c.selector}` };
          if (r.error) return { ok: false, error: `${r.error}: ${c.selector}` };
          if (c.kind === 'box') Object.assign(c, { x: r.x, y: r.y, width: r.width, height: r.height });
          else if (c.kind === 'arrow') Object.assign(c, { toX: r.x + r.width / 2, toY: r.y + r.height / 2, x: c.x ?? r.x - 60, y: c.y ?? r.y - 60 });
          else Object.assign(c, { x: r.x + r.width / 2, y: r.y + r.height / 2 });
        }
        shapes.push(c);
      }
      return request('annotate', { tabId: en.tabId, shapes, clear: !!msg.clear, show: msg.show !== false });
    },
    async browserDrawerAnnotations(msg) {
      const en = await prepare(msg);
      const r = await request('annotations', { tabId: en.tabId, image: msg.image !== false });
      // A selected element (or a chat's mark made from a selector) carries its
      // selector; a drawing does not: it is in the screenshot.
      const list = r.annotations || [];
      list.forEach((a) => { a.element = a.picked || null; delete a.picked; delete a.anchor; });
      return { ok: true, url: en.wc.getURL(), title: en.wc.getTitle(), viewport: r.viewport, annotations: list, image: r.image || null };
    },
  };

  async function close(msg) { const en = await prepare(msg); await request('close', { tabId: en.tabId }); return { ok: true }; }
  async function resize(msg) { const en = await prepare(msg); await request('resize', { tabId: en.tabId, width: msg.width, height: msg.height }); return { ok: true }; }

  return { hardenWindow, handlers, prepare, getEntry, close, resize, notifyInfo, isDrawerKey, networkLabel, _tabs: tabs };
}
