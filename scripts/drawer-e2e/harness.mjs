// Electron side of scripts/test-browser-drawer.mjs: a desktop client in
// REMOTE mode, i.e. on another machine than its server. It loads the
// server's page with the real app/preload.js, registers on the server's
// websocket as its native host, and answers browser requests with the real
// app/browser-drawer.js and app/net-tunnel-client.js. The few generic
// browser handlers it needs are the same code as app/main.js's.
//
// Run by the test, not by hand: electron scripts/drawer-e2e/harness.mjs
// (SERVER_URL set).

import { app, BrowserWindow, ipcMain, session } from 'electron';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createBrowserDrawer, isDrawerKey, partitionOf } from '../../app/browser-drawer.js';
import { createDrawerNet } from '../../app/drawer-net.js';
import { runMouse } from '../../app/browser-pointer.js';
import { createPasswordStore } from '../../app/drawer-passwords.js';

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const WS = require('ws');
const SERVER = process.env.SERVER_URL;
const out = (m) => process.stdout.write('[harness] ' + m + '\n');

let win = null, ws = null, features = [];
const up = () => !!(ws && ws.readyState === 1);
// The real network plumbing of app/main.js (app/drawer-net.js), in remote mode.
const serverNet = createDrawerNet({
  isRemote: () => true, remoteUp: up, remoteFeatures: () => features,
  sendRemote: (b) => ws.send(b, { binary: true }), localUp: () => false, sendLocal: () => { throw new Error('no local server'); },
  sessionFor: (p) => session.fromPartition(p), partitionOf, log: out,
});
app.on('login', (event, _wc, _d, authInfo, cb) => { const c = serverNet.login(authInfo); if (c) { event.preventDefault(); cb(c.user, c.pass); } });

// Same as app/main.js's loadUrlWithGrace.
function loadUrlWithGrace(wc, url, graceMs = 15000) {
  return new Promise((resolve, reject) => {
    let done = false;
    const finish = (fn) => { if (done) return; done = true; cleanup(); fn(); };
    const onStop = () => finish(resolve);
    const onFail = (_e, code, desc, _u, isMain) => { if (isMain && code !== -3) finish(() => reject(new Error(`Load failed (${code}): ${desc}`))); };
    const timer = setTimeout(() => finish(resolve), graceMs);
    function cleanup() { clearTimeout(timer); wc.off('did-stop-loading', onStop); wc.off('did-fail-load', onFail); }
    wc.on('did-stop-loading', onStop); wc.on('did-fail-load', onFail);
    wc.loadURL(url).catch(() => {});
  });
}

const drawer = createBrowserDrawer({ ipcMain, getWindow: () => win, log: out, loadUrl: loadUrlWithGrace, serverNet, freezeAfterMs: Number(process.env.DRAWER_SLEEP_MS) || undefined, discardAfterMs: Number(process.env.DRAWER_UNLOAD_MS) || undefined, historyFile: join(app.getPath('userData'), 'drawer-history.json'),
  // Test only: a reversible stand-in for safeStorage (a display under xvfb has no keyring).
  passwords: createPasswordStore({ file: join(app.getPath('userData'), 'drawer-passwords.json'),
    encrypt: (s) => Buffer.from('test:' + Buffer.from(s, 'utf8').toString('base64')), decrypt: (b) => Buffer.from(String(b).slice(5), 'base64').toString('utf8'), backend: () => ({ ok: true }) }) });

// The generic handlers app/main.js runs on a drawer entry (copied from there).
const generic = {
  async browserNavigate(msg) { const e = drawer.getEntry(msg.key); await loadUrlWithGrace(e.win.webContents, msg.url); return { ok: true, url: e.win.webContents.getURL() }; },
  async browserScreenshot(msg) { const e = drawer.getEntry(msg.key); const img = await e.win.webContents.capturePage(); return { ok: true, data: img.toPNG().toString('base64') }; },
  async browserReadPage(msg) {
    const e = drawer.getEntry(msg.key); const wc = e.win.webContents;
    const text = await wc.executeJavaScript('(document.body.innerText || "").substring(0, 50000)');
    return { ok: true, title: wc.getTitle(), url: wc.getURL(), text, consoleLogs: e.consoleLogs.slice(-20) };
  },
  async browserScroll(msg) { const e = drawer.getEntry(msg.key); await e.win.webContents.executeJavaScript(`window.scrollBy(0, ${Number(msg.y) || 0})`); return { ok: true }; },
  async browserClick(msg) { const e = drawer.getEntry(msg.key); await e.win.webContents.executeJavaScript(`document.querySelector(${JSON.stringify(msg.selector)})?.click()`); return { ok: true }; },
  async browserType(msg) {
    const e = drawer.getEntry(msg.key);
    if (msg.selector) await e.win.webContents.executeJavaScript(`document.querySelector(${JSON.stringify(msg.selector)})?.focus()`);
    e.win.webContents.insertText(msg.text); return { ok: true };
  },
  async browserMouse(msg) {
    const e = drawer.getEntry(msg.key); const wc = e.win.webContents;
    try { wc.debugger.attach('1.3'); } catch (err) { if (!/already attached/i.test(err.message)) throw err; }
    e.pointer = e.pointer || {};
    try {
      const r = await runMouse({ send: (m, p) => wc.debugger.sendCommand(m, p), state: e.pointer, show: (p) => drawer.cursor(msg.key, p), msg });
      return { ok: true, url: wc.getURL(), ...r };
    } catch (err) { return { ok: false, error: err.message }; }
  },
  async browserFill(msg) {
    const e = drawer.getEntry(msg.key);
    await e.win.webContents.executeJavaScript(`(function(){ const el = document.querySelector(${JSON.stringify(msg.selector)}); if (!el) throw new Error('Element not found'); el.focus(); el.value = ${JSON.stringify(msg.value)}; el.dispatchEvent(new Event('input', {bubbles:true})); el.dispatchEvent(new Event('change', {bubbles:true})); })()`);
    return { ok: true };
  },
  async browserSource(msg) { const e = drawer.getEntry(msg.key); const wc = e.win.webContents; const html = await wc.executeJavaScript('document.documentElement.outerHTML'); return { ok: true, html, url: wc.getURL(), title: wc.getTitle() }; },
  async browserEval(msg) { const e = drawer.getEntry(msg.key); const r = await e.win.webContents.executeJavaScript(msg.code); return { ok: true, result: typeof r === 'string' ? r : JSON.stringify(r) }; },
  async browserClose(msg) { return drawer.close(msg); },
  async browserResize(msg) { return drawer.resize(msg); },
  ...drawer.handlers,
};

async function handle(msg) {
  const h = generic[msg.type];
  if (!h) return { ok: false, error: 'unsupported ' + msg.type };
  if (isDrawerKey(msg.key) && msg.type !== 'browserClose' && msg.type !== 'browserResize') await drawer.prepare(msg);
  const r = await h(msg);
  return isDrawerKey(msg.key) ? drawer.redact(msg.key, r) : r;   // as app/main.js does
}

// Test-only: run code in the drawer's page, or read a pixel of the window.
async function testCall(m) {
  if (m.op === 'exec') return { ok: true, result: await win.webContents.executeJavaScript(m.code) };
  if (m.op === 'pixel') {
    const img = await win.webContents.capturePage();
    const sz = img.getSize(); const bmp = img.toBitmap();
    const sf = sz.width / win.getContentBounds().width;
    const i = (Math.round(m.y * sf) * sz.width + Math.round(m.x * sf)) * 4;
    return { ok: true, rgb: [bmp[i + 2], bmp[i + 1], bmp[i]] };
  }
  if (m.op === 'shot') { const img = await win.webContents.capturePage(); return { ok: true, data: img.toPNG().toString('base64') }; }
  if (m.op === 'quit') { setTimeout(() => app.exit(0), 50); return { ok: true }; }
  return { ok: false };
}

function connect() {
  ws = new WS(SERVER.replace(/^http/, 'ws') + '/ws');
  ws.on('open', () => ws.send(JSON.stringify({ type: 'register-native-host', capabilities: ['notify', 'browser-touch', 'browser-drawer', 'browser-mouse'], deviceId: process.env.DRAWER_DEVICE || '' })));
  ws.on('message', (raw, isBinary) => {
    if (isBinary) { serverNet.handleFrame(raw); return; }
    const m = JSON.parse(raw.toString());
    if (m.type === 'native-host-ready') { features = m.features || []; drawer.notifyInfo(); out('registered ' + features.join(',')); return; }
    if (m.type === 'test') { testCall(m).then((r) => ws.send(JSON.stringify({ type: 'test-result', id: m.id, ...r })), (err) => ws.send(JSON.stringify({ type: 'test-result', id: m.id, ok: false, error: err.message }))); return; }
    if (m.type !== 'native-host') return;
    const msg = m.payload;
    handle(msg).then((r) => ws.send(JSON.stringify({ type: 'native-host-result', payload: { type: 'browserResult', requestId: msg.requestId, ...r } })),
      (err) => ws.send(JSON.stringify({ type: 'native-host-result', payload: { type: 'browserResult', requestId: msg.requestId, ok: false, error: err.message } })));
  });
  ws.on('close', () => { features = []; serverNet.dropped(); drawer.notifyInfo(); if (process.env.EXIT_ON_CLOSE) app.exit(0); });
}

app.whenReady().then(() => {
  win = new BrowserWindow({ width: 1300, height: 800, show: true, webPreferences: { preload: join(here, '..', '..', 'app', 'preload.js'), contextIsolation: true, nodeIntegration: false, webviewTag: true } });
  drawer.hardenWindow(win);
  win.webContents.on('console-message', (...a) => { const ev = a[0] || {}; const t = typeof a[2] === 'string' ? a[2] : ev.message; if (t) out('page: ' + t); });
  win.loadURL(SERVER + '/');
  connect();
});
