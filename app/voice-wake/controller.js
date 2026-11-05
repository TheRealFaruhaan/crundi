/**
 * "Hey Crundi" on the PC (D40), the main-process side.
 *
 * When the setting is on and the PC is on charge, a hidden window holds the
 * microphone (listener.html) and streams 16 kHz PCM over IPC to a utility
 * process running the keyword spotter (worker.js). Nothing goes to the
 * network: the hidden window's session refuses every web request and grants
 * the microphone to that one window only.
 *
 * It listens only when all of these hold:
 *   - the setting is on (off by default, kept in client.json);
 *   - the PC is on AC power (powerMonitor; checked again every minute too,
 *     since not every platform sends on-ac / on-battery);
 *   - the voice screen is not open in the main window (one microphone owner
 *     at a time; the page is asked every second whether it is open);
 *   - the screen is not locked and the PC is not asleep.
 *
 * On "Hey Crundi" the main window is shown and brought to the front, and the
 * voice screen is opened in the page.
 */

import { join } from 'path';
import { pathToFileURL } from 'url';
import { ensureInstalled, isInstalled } from './install.js';

const POLL_MS = 1000;
const POWER_RECHECK_MS = 60 * 1000;
const RESUME_DELAY_MS = 800;
const RETRY_MS = 15 * 1000;

// Asks the page to open the voice screen. Prefers a page function made for
// this, then the top-bar mic button (its click opens the screen and keeps it
// open). Says what it did.
const OPEN_SCRIPT = `(function () {
  var V = window.CrundiVoice;
  if (V && V.isOpen) return 'open';
  if (typeof window.crundiOpenVoice === 'function') { window.crundiOpenVoice({ how: 'wake' }); return 'called'; }
  var b = document.getElementById('voice-btn');
  if (b) { b.click(); return 'clicked'; }
  return 'none';
})()`;
const IS_OPEN_SCRIPT = '!!(window.CrundiVoice && window.CrundiVoice.isOpen)';

/**
 * @param {object} d
 * @param {typeof import('electron')} d.electron
 * @param {string} d.moduleDir     folder of this file (unpacked when packaged)
 * @param {string} d.dataDir       where the engine and model go (<userData>/voice-wake)
 * @param {() => boolean} d.getEnabled
 * @param {(on: boolean) => boolean} d.saveEnabled
 * @param {() => Electron.BrowserWindow|null} d.getMainWindow
 * @param {() => void} d.bringToFront
 * @param {(s: string) => void} d.log
 * @param {(status: object) => void} [d.onStatus]
 */
export function createVoiceWake(d) {
  const { BrowserWindow, utilityProcess, powerMonitor, session, net, ipcMain } = d.electron;
  const log = (s) => { try { d.log('[voice-wake] ' + s); } catch { /* ignore */ } };

  let enabled = !!d.getEnabled();
  let onBattery = false;
  let locked = false;
  let asleep = false;
  let voiceOpen = false;
  let installing = null;        // Promise while downloading
  let installError = '';
  let micState = 'stopped';     // from the window: loaded | listening | stopped | error
  let micError = '';
  let lastHeard = null;         // { keyword, at }
  let win = null;               // the hidden window
  let child = null;             // the utility process
  let childReady = false;
  let pollTimer = null;
  let polling = false;
  let powerTimer = null;
  let retryTimer = null;
  let resumeAt = 0;

  const listenerHtml = join(d.moduleDir, 'listener.html');
  const listenerUrl = pathToFileURL(listenerHtml).href;
  const ses = session.fromPartition('crundi-voice-wake');   // in memory, never on disk

  // ─── the hidden window's session: microphone for that window, no network ───
  ses.setPermissionRequestHandler((wc, permission, cb, details) => {
    const types = (details && details.mediaTypes) || [];
    const ok = !!win && !win.isDestroyed() && wc === win.webContents && permission === 'media'
      && types.length > 0 && types.every((t) => t === 'audio')
      && String((details && details.requestingUrl) || '').startsWith(listenerUrl);
    if (!ok) log('refused ' + permission + ' to ' + ((details && details.requestingUrl) || '?'));
    cb(ok);
  });
  ses.setPermissionCheckHandler((wc, permission, _origin, details) =>
    !!win && !win.isDestroyed() && wc === win.webContents && permission === 'media' && (!details || !details.mediaType || details.mediaType === 'audio'));
  ses.webRequest.onBeforeRequest({ urls: ['http://*/*', 'https://*/*', 'ws://*/*', 'wss://*/*'] }, (_det, cb) => cb({ cancel: true }));

  function readPower() {
    try { return !!powerMonitor.isOnBatteryPower(); } catch { return false; }
  }

  function status() {
    let state;
    if (!enabled) state = 'off';
    else if (installing) state = 'installing';
    else if (installError) state = 'error';
    else if (onBattery) state = 'on-battery';
    else if (asleep || locked) state = 'paused';
    else if (voiceOpen) state = 'voice-open';
    else if (micState === 'listening' && childReady) state = 'listening';
    else if (micState === 'error') state = 'error';
    else state = 'starting';
    return {
      enabled,
      state,
      onBattery,
      installed: safeInstalled(),
      error: installError || (micState === 'error' ? micError : ''),
      lastHeard,
    };
  }
  function safeInstalled() { try { return isInstalled(d.dataDir); } catch { return false; } }

  let lastSent = '';
  function emit() {
    const s = status();
    const key = JSON.stringify(s);
    if (key === lastSent) return;
    lastSent = key;
    log('status ' + s.state + (s.error ? ' (' + s.error + ')' : ''));
    try { d.onStatus && d.onStatus(s); } catch { /* ignore */ }
  }

  const wanted = () => enabled && !onBattery && !locked && !asleep && !voiceOpen && !installing && !installError;

  // ─── the spotter's process ───
  function startChild() {
    if (child) return;
    childReady = false;
    child = utilityProcess.fork(join(d.moduleDir, 'worker.js'), [], { serviceName: 'Crundi Hey Crundi', stdio: 'pipe' });
    const me = child;
    // The engine reports a bad model or keyword on stderr before it exits.
    const tell = (buf) => String(buf).split('\n').filter(Boolean).forEach((l) => log('spotter: ' + l.slice(0, 300)));
    try { me.stdout && me.stdout.on('data', tell); me.stderr && me.stderr.on('data', tell); } catch { /* no pipes */ }
    me.on('message', (m) => {
      if (me !== child || !m) return;
      if (m.type === 'ready') { childReady = true; emit(); }
      else if (m.type === 'error') { log('spotter: ' + m.message); }
      else if (m.type === 'detected') heard(m.keyword);
    });
    me.on('exit', (code) => {
      if (me !== child) return;
      child = null; childReady = false;
      log('spotter process ended (' + code + ')');
      if (wanted()) scheduleRetry();
      emit();
    });
    me.postMessage({ type: 'init', dir: d.dataDir });
  }
  function stopChild() {
    const c = child; child = null; childReady = false;
    if (c) { try { c.kill(); } catch { /* gone */ } }
  }

  // ─── the microphone window ───
  function startWindow() {
    if (win && !win.isDestroyed()) { send('start'); return; }
    micState = 'stopped'; micError = '';
    win = new BrowserWindow({
      show: false, width: 200, height: 100, skipTaskbar: true, focusable: false,
      title: 'Hey Crundi',
      webPreferences: {
        preload: join(d.moduleDir, 'listener-preload.cjs'),
        partition: 'crundi-voice-wake',
        sandbox: true, contextIsolation: true, nodeIntegration: false,
        backgroundThrottling: false, spellcheck: false, webgl: false,
      },
    });
    const w = win;
    w.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
    w.webContents.on('will-navigate', (e) => e.preventDefault());
    w.webContents.on('render-process-gone', (_e, det) => {
      if (w !== win) return;
      log('listener window gone: ' + (det && det.reason));
      try { w.destroy(); } catch { /* gone */ }
      win = null; micState = 'stopped';
      if (wanted()) scheduleRetry();
      emit();
    });
    w.on('closed', () => { if (w === win) { win = null; micState = 'stopped'; } });
    w.loadFile(listenerHtml).catch((err) => log('listener load failed: ' + err.message));
  }
  function send(cmd) {
    if (win && !win.isDestroyed()) { try { win.webContents.send('voice-wake:command', cmd); } catch { /* gone */ } }
  }
  function stopWindow() {
    const w = win; win = null; micState = 'stopped';
    if (w && !w.isDestroyed()) { try { w.destroy(); } catch { /* gone */ } }
  }

  ipcMain.on('voice-wake:state', (e, state, detail) => {
    if (!win || win.isDestroyed() || e.sender !== win.webContents) return;
    if (state === 'loaded') { if (wanted()) send('start'); return; }
    micState = state; micError = state === 'error' ? String(detail || '') : '';
    if (state === 'error') { log('microphone: ' + micError); stopWindow(); if (wanted()) scheduleRetry(); }
    emit();
  });
  ipcMain.on('voice-wake:pcm', (e, samples) => {
    if (!win || e.sender !== win.webContents || !child || !childReady || !wanted()) return;
    try { child.postMessage({ type: 'pcm', samples }); } catch { /* the exit handler tidies up */ }
  });

  function scheduleRetry() {
    if (retryTimer) return;
    retryTimer = setTimeout(() => { retryTimer = null; reconcile(); }, RETRY_MS);
  }

  // ─── the voice screen in the page ───
  function mainWc() {
    const w = d.getMainWindow();
    return w && !w.isDestroyed() ? w.webContents : null;
  }
  function startPolling() {
    if (pollTimer) return;
    pollTimer = setInterval(pollVoice, POLL_MS);
  }
  function stopPolling() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
  }
  function pollVoice() {
    const wc = mainWc();
    if (!wc) { setVoiceOpen(false); return; }
    if (polling || wc.isLoading()) return;
    polling = true;
    const t = setTimeout(() => { polling = false; }, 3000);
    wc.executeJavaScript(IS_OPEN_SCRIPT, false).then((open) => {
      clearTimeout(t); polling = false;
      setVoiceOpen(!!open);
    }, () => { clearTimeout(t); polling = false; setVoiceOpen(false); });
  }
  function setVoiceOpen(open) {
    if (open === voiceOpen) {
      if (!open && resumeAt && Date.now() >= resumeAt) { resumeAt = 0; reconcile(); }
      return;
    }
    voiceOpen = open;
    // Give the voice screen a moment to let go of the microphone.
    if (!open) resumeAt = Date.now() + RESUME_DELAY_MS;
    else reconcile();
    emit();
  }

  function heard(keyword) {
    lastHeard = { keyword: String(keyword || ''), at: Date.now() };
    log('heard ' + lastHeard.keyword);
    // Hand the microphone over before the voice screen asks for it.
    voiceOpen = true; resumeAt = 0;
    reconcile();
    try { d.bringToFront(); } catch (err) { log('could not show the window: ' + err.message); }
    const wc = mainWc();
    if (!wc) { voiceOpen = false; resumeAt = Date.now() + RESUME_DELAY_MS; emit(); return; }
    const run = () => wc.executeJavaScript(OPEN_SCRIPT, true).then((how) => {
      log('voice screen: ' + how);
      // Nothing to open (the setup page, an older server): listen again.
      if (how === 'none') { voiceOpen = false; resumeAt = Date.now() + RESUME_DELAY_MS; }
      emit();
    }, (err) => { log('could not open the voice screen: ' + err.message); voiceOpen = false; resumeAt = Date.now() + RESUME_DELAY_MS; emit(); });
    if (wc.isLoading()) wc.once('did-finish-load', run); else run();
  }

  // ─── putting it together ───
  function install() {
    if (installing || safeInstalled()) return;
    installError = '';
    const fetchImpl = net && net.fetch ? (u, o) => net.fetch(u, o) : globalThis.fetch;
    installing = ensureInstalled(d.dataDir, { fetch: fetchImpl, log: (m) => { try { d.log(m); } catch { /* ignore */ } } })
      .catch((err) => { installError = String(err && err.message || err); log('install failed: ' + installError); })
      .finally(() => { installing = null; reconcile(); emit(); });
    emit();
  }

  function reconcile() {
    if (enabled && !safeInstalled() && !installing && !installError) { install(); return; }
    // Watching for the voice screen matters whenever we might be listening.
    if (enabled && !onBattery) startPolling(); else stopPolling();
    if (wanted()) {
      startChild();
      startWindow();
    } else {
      stopWindow();
      // The spotter's process stays warm through a voice screen or a lock;
      // off or on battery, it goes too.
      if (!enabled || onBattery || installError) stopChild();
    }
    emit();
  }

  powerMonitor.on('on-ac', () => { onBattery = false; reconcile(); });
  powerMonitor.on('on-battery', () => { onBattery = true; reconcile(); });
  powerMonitor.on('lock-screen', () => { locked = true; reconcile(); });
  powerMonitor.on('unlock-screen', () => { locked = false; reconcile(); });
  powerMonitor.on('suspend', () => { asleep = true; reconcile(); });
  powerMonitor.on('resume', () => {
    asleep = false;
    // The microphone a sleep interrupted does not come back by itself.
    stopWindow();
    onBattery = readPower();
    reconcile();
  });

  onBattery = readPower();
  powerTimer = setInterval(() => {
    const b = readPower();
    if (b !== onBattery) { onBattery = b; reconcile(); }
  }, POWER_RECHECK_MS);
  reconcile();

  return {
    status,
    get: () => enabled,
    /** Turn the option on or off; saves it. Returns the status. */
    set(on) {
      on = !!on;
      if (on !== enabled) {
        if (!d.saveEnabled(on)) return { ...status(), error: 'Could not save the setting' };
        enabled = on;
        if (on) installError = '';   // turning it on again retries a failed download
        log(on ? 'turned on' : 'turned off');
      }
      reconcile();
      return status();
    },
    stop() {
      stopPolling();
      if (powerTimer) clearInterval(powerTimer);
      if (retryTimer) clearTimeout(retryTimer);
      stopWindow();
      stopChild();
    },
  };
}
