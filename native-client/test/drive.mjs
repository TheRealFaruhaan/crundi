#!/usr/bin/env node
// The native prototype, driven by a Crundi server. This process is the server
// side: the real src/browser.js (what the chats' browser_* tools call) behind a
// /ws endpoint, exactly as the server attaches a desktop app as its native host.
// Start the prototype with CRUNDI_SERVER=ws://127.0.0.1:<port>/ws?token=test and
// this script waits for it to register, then drives both panels.
//
//   node native-client/test/drive.mjs [port]     (default 7611; screenshots to /tmp)

import http from 'node:http';
import { writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import * as browser from '../../src/browser.js';

const require = createRequire(import.meta.url);
const { WebSocketServer } = require('ws');
const port = Number(process.argv[2] || 7611);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : '  ' + JSON.stringify(extra).slice(0, 300)}`); if (!ok) failed++; };

// A test page: a button that counts clicks, an input, the viewport size.
const page = http.createServer((req, res) => {
  res.setHeader('Content-Type', 'text/html; charset=utf-8');
  res.end(`<!doctype html><meta name=viewport content="width=device-width"><title>drive ${req.url}</title>
<body style="font:16px sans-serif;margin:20px">
<button id=b style="position:absolute;left:40px;top:80px;width:160px;height:50px" onclick="this.textContent='clicked '+(++window.n)">Click me</button>
<p style="margin-top:150px">viewport <span id=vp></span></p>
<script>window.n=0;function vp(){document.getElementById('vp').textContent=innerWidth+'x'+innerHeight+' dpr '+devicePixelRatio+' touch '+('ontouchstart' in window)}vp();addEventListener('resize',vp)</script>`);
});

// Raw requests the server has no wrapper for yet (prototype-only).
let hostWs = null, rawId = 0;
const rawWaiters = new Map();
function raw(type, fields) {
  const requestId = 'raw-' + (++rawId);
  return new Promise((resolve) => {
    rawWaiters.set(requestId, resolve);
    hostWs.send(JSON.stringify({ type: 'native-host', payload: { type, requestId, ...fields } }));
    setTimeout(() => { if (rawWaiters.delete(requestId)) resolve({ ok: false, error: 'timeout' }); }, 15000);
  });
}

const server = http.createServer();
const wss = new WebSocketServer({ server, path: '/ws' });
let registered;
const ready = new Promise((r) => { registered = r; });
wss.on('connection', (ws) => {
  ws.on('message', (data) => {
    const msg = JSON.parse(String(data));
    if (msg.type === 'register-native-host') {
      hostWs = ws;
      browser.setRemoteHost({ send: (m) => ws.send(JSON.stringify({ type: 'native-host', payload: m })), capabilities: msg.capabilities || [], deviceId: msg.deviceId || '' });
      ws.send(JSON.stringify({ type: 'native-host-ready', features: [] }));
      registered(msg);
      return;
    }
    if (msg.type === 'native-host-result') {
      const p = msg.payload || {};
      if (rawWaiters.has(p.requestId)) { const r = rawWaiters.get(p.requestId); rawWaiters.delete(p.requestId); r(p); return; }
      browser.handleHostMessage(p);
    }
  });
  ws.on('close', () => { if (ws === hostWs) browser.clearRemoteHost(); });
});

await new Promise((r) => page.listen(0, '127.0.0.1', r));
await new Promise((r) => server.listen(port, '127.0.0.1', r));
const pageUrl = `http://127.0.0.1:${page.address().port}`;
console.log(`waiting for the prototype on ws://127.0.0.1:${port}/ws?token=test`);
const reg = await Promise.race([ready, sleep(60000).then(() => null)]);
check('the prototype registers as native host', !!reg, 'no registration in 60s');
if (!reg) process.exit(1);
console.log('  capabilities', reg.capabilities, 'device', reg.deviceId);

const A = await browser.openBrowser({ alias: 'proto', name: 'a', url: pageUrl + '/a' });
check('browser_open fills panel A', A.ok, A);
const B = await browser.openBrowser({ alias: 'proto', name: 'b', url: pageUrl + '/b' });
check('browser_open fills panel B', B.ok, B);
const C = await browser.openBrowser({ alias: 'proto', name: 'c', url: pageUrl + '/c' });
check('a third browser is refused (two panels)', !C.ok, C);

let r = await browser.evalBrowser(A.key, 'document.title');
check('browser_eval on A', r.ok && r.result === 'drive /a', r);
r = await browser.readBrowserPage(B.key);
check('browser_read_page on B', r.ok && /Click me/.test(r.text || ''), r);

r = await browser.clickBrowser(A.key, '#b');
r = await browser.evalBrowser(A.key, 'document.getElementById("b").textContent');
check('browser_click by selector', r.result === 'clicked 1', r);

r = await browser.mouseBrowser(B.key, 120, 105, 'click');
await sleep(300);
r = await browser.evalBrowser(B.key, 'document.getElementById("b").textContent');
check('browser_mouse click (real input events)', r.result === 'clicked 1', r);

r = await browser.navigateBrowser(B.key, pageUrl + '/b2');
const t = await browser.evalBrowser(B.key, 'document.title');
check('browser_navigate waits for the load', r.ok && t.result === 'drive /b2', [r, t]);

r = await raw('browserDevice', { key: A.key, device: 'iPhone 15 Pro' });
await sleep(2500);
const vp = await browser.evalBrowser(A.key, 'document.getElementById("vp").textContent');
check('device emulation: iPhone 15 Pro on A', r.ok && /^393x852 dpr 3 touch true/.test(vp.result || ''), [r, vp]);
const ua = await browser.evalBrowser(A.key, 'navigator.userAgent');
check('  with the iPhone user agent', /iPhone/.test(ua.result || ''), ua);

r = await browser.resizeBrowser(B.key, 800, 600);
await sleep(1000);
const vp2 = await browser.evalBrowser(B.key, 'innerWidth+"x"+innerHeight');
check('browser_resize: B is 800x600', vp2.result === '800x600', vp2);

r = await raw('browserPoint', { key: A.key, x: 120, y: 105, label: 'This button' });
check('browserPoint drops a pin', r.ok, r);
await sleep(500);

let shot = await browser.screenshotBrowser(A.key);
check('browser_screenshot A', shot.ok && shot.data && shot.data.length > 1000, shot.error);
if (shot.ok) writeFileSync('/tmp/native-drive-a.png', Buffer.from(shot.data, 'base64'));
shot = await browser.screenshotBrowser(B.key);
if (shot.ok) writeFileSync('/tmp/native-drive-b.png', Buffer.from(shot.data, 'base64'));

r = await raw('browserMarks', { key: A.key });
check('browserMarks reads the annotations', r.ok && r.marks && r.marks.pin && r.marks.pin.label === 'This button', r);
console.log('  marks', JSON.stringify(r.marks).slice(0, 200));

if (process.env.KEEP) { console.log('KEEP set: leaving the panels open'); await sleep(Number(process.env.KEEP)); }
r = await browser.closeBrowser(A.key);
check('browser_close frees panel A', r.ok, r);
const D = await browser.openBrowser({ alias: 'proto', name: 'd', url: pageUrl + '/d' });
check('the freed panel takes a new browser', D.ok, D);

console.log(failed ? `${failed} failed` : 'all passed');
process.exit(failed ? 1 : 0);
