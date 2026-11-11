#!/usr/bin/env node
// End to end: the PC client's browser drawer, driven by a server's chats, with
// the client on ANOTHER machine (remote mode). Everything crosses the client's
// native-host websocket; no CDP port is involved anywhere.
//
//   this process   the server: src/browser.js (what the MCP browser_* tools
//                  call) and src/net-tunnel.js on a websocket, plus the page
//   Electron       the client: scripts/drawer-e2e/harness.mjs, with the real
//                  app/preload.js, app/browser-drawer.js, app/net-tunnel-client.js
//                  and app/vendor/crundi-browser-drawer.js in its window
//
// Needs Electron (ELECTRON_BIN, node_modules/electron, or Electron's download
// cache) and a display (xvfb-run on Linux). Skips, passing, when it has none.
//
// Run: node scripts/test-browser-drawer.mjs

import http from 'node:http';
import net from 'node:net';
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, mkdtempSync, rmSync, chmodSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import * as browser from '../src/browser.js';
import { createTunnelEndpoint } from '../src/net-tunnel.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
const { WebSocketServer } = require('ws');

let failed = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : '  ' + String(extra).slice(0, 300)}`); if (!ok) failed++; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ─── Electron ───
function findElectron() {
  if (process.env.ELECTRON_BIN && existsSync(process.env.ELECTRON_BIN)) return { bin: process.env.ELECTRON_BIN };
  const local = join(root, 'node_modules', 'electron', 'dist', process.platform === 'win32' ? 'electron.exe' : 'electron');
  if (existsSync(local)) return { bin: local };
  if (process.platform !== 'linux') return null;
  // Electron's own download cache: unpack the newest linux-x64 zip.
  const cache = join(homedir(), '.cache', 'electron');
  if (!existsSync(cache)) return null;
  const zips = readdirSync(cache).filter((f) => /^electron-v[\d.]+-linux-x64\.zip$/.test(f)).sort();
  if (!zips.length) return null;
  const dir = mkdtempSync(join(tmpdir(), 'crundi-electron-'));
  try {
    execFileSync('python3', ['-I', '-c', 'import zipfile,sys;zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])', join(cache, zips.at(-1)), dir]);
    chmodSync(join(dir, 'electron'), 0o755);
    try { chmodSync(join(dir, 'chrome_crashpad_handler'), 0o755); } catch { /* optional */ }
  } catch { rmSync(dir, { recursive: true, force: true }); return null; }
  return { bin: join(dir, 'electron'), cleanup: dir };
}
const electron = findElectron();
const needXvfb = process.platform === 'linux' && !process.env.DISPLAY;
const hasXvfb = !needXvfb || (() => { try { execFileSync('which', ['xvfb-run'], { stdio: 'ignore' }); return true; } catch { return false; } })();
if (!electron || !hasXvfb) {
  console.log(`skip: ${!electron ? 'no Electron binary found' : 'no display and no xvfb-run'}`);
  process.exit(0);
}

// ─── Targets ───
let pageLoads = 0;
const target = http.createServer((req, res) => {
  res.setHeader('Content-Type', 'text/html; charset=utf-8');
  // Refuses framing every way an iframe would be refused: a <webview> is not an iframe.
  res.setHeader('X-Frame-Options', 'DENY');
  res.setHeader('Content-Security-Policy', "frame-ancestors 'none'");
  if (req.url.startsWith('/whoami')) { res.end('<title>who</title><body>via-server ' + req.headers.host + '</body>'); return; }
  if (req.url.startsWith('/page2')) { res.end('<title>Second</title><body style="margin:0;background:#fff">second</body>'); return; }
  pageLoads++;
  res.end(`<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><title>Target page</title>
    <body style="margin:0;min-height:100vh;font-family:system-ui,sans-serif;background:#f6f7fb;color:#1f2330">
    <header style="height:48px;display:flex;align-items:center;gap:16px;padding:0 18px;background:#fff;border-bottom:1px solid #e3e5ee"><b style="color:#4f46e5">Acme</b><span style="color:#667">Shop</span><span style="color:#667">About</span></header>
    <main style="padding:22px 18px 90px"><h2 style="margin:0 0 8px">Spring collection</h2><p style="margin:0 0 16px;color:#556;max-width:420px">Light jackets, linen shirts and canvas shoes, made to last.</p>
    <button id="btn" style="width:130px;height:40px;border:none;border-radius:8px;background:#4f46e5;color:#fff;font-weight:600">Add to cart</button>
    <input id="inp" placeholder="Email for updates" style="height:36px;margin-left:10px;padding:0 10px;border:1px solid #ccd;border-radius:8px">
    <p><a id="pop" href="/page2" target="_blank">Size guide (new window)</a></p>
    <div style="display:flex;gap:12px;margin-top:14px"><div style="width:140px;height:110px;border-radius:10px;background:#dfe3f3"></div><div style="width:140px;height:110px;border-radius:10px;background:#e8dff3"></div></div></main>
    <footer style="position:fixed;left:0;right:0;bottom:0;height:56px;background:#00e000"></footer>
    <script>window.clicks=0;document.getElementById('btn').onclick=function(){window.clicks++;};</script></body>`);
});
await new Promise((r) => target.listen(0, '127.0.0.1', r));
const T = target.address().port;
// Only the server resolves this name, so reaching it proves a tab went through the server.
let tunnelOpens = 0;
const serverConnect = (o) => { tunnelOpens++; return net.connect({ host: o.host === 'internal.test' ? '127.0.0.1' : o.host, port: o.port }); };

// ─── The server ───
// The app's own icon set, so the drawer looks as it does in Crundi.
const ICONS = {};
{
  const src = readFileSync(join(root, 'src', 'webapp-html.js'), 'utf8');
  const block = src.slice(src.indexOf('const ICON_PATHS = {'), src.indexOf('function ic(name)'));
  for (const m of block.matchAll(/^\s*'?([a-z-]+)'?: '([^']*)',?$/gm)) ICONS[m[1]] = m[2];
}
const PAGE = `<!doctype html><html><head><meta charset="utf-8"><style>
  :root{--bg-primary:#0a0a0f;--bg-secondary:#12121a;--bg-tertiary:#1a1a28;--border:#2a2a3d;--text-primary:#e8e8f0;--text-secondary:#8888a8;--text-muted:#5a5a78;--accent:#6366f1;}
  body{margin:0;height:100vh;display:flex;flex-direction:column;font-family:sans-serif} .top{height:40px;background:#333;flex:0 0 auto}
  .main-layout{display:flex;flex:1;overflow:hidden} #area{flex:1;background:#0a0a0f;color:#fff;min-width:0} .top{background:#12121a;border-bottom:1px solid #2a2a3d}</style></head>
<body><div class="top"></div><div class="main-layout"><div id="area"><div style="height:38px;display:flex;gap:18px;align-items:center;padding:0 14px;border-bottom:1px solid #2a2a3d;background:#12121a;color:#8888a8;font-size:13px;white-space:nowrap;overflow:hidden"><span style="color:#6366f1">Workbench</span><span>Git</span><span>Files</span><span>Kanban</span><span>Notes</span><span>Mindmap</span><span>Media</span></div><div style="display:flex;gap:8px;padding:8px;height:calc(100% - 54px)"><div style="flex:1;border:1px solid #2a2a3d;border-radius:8px;background:#0a0a0f;padding:10px;color:#8888a8;font-size:13px">Chat one</div><div style="flex:1;border:1px solid #2a2a3d;border-radius:8px;background:#0a0a0f"></div></div></div>
<div class="brd-split brd-closed" id="brd-split"></div><aside class="brd brd-closed" id="brd"></aside></div>
<script src="/vendor/crundi-browser-drawer.js"></script>
<script>var ICONS=${JSON.stringify(ICONS)};window.sent=[];
window.drawer=CrundiBrowserDrawer.init({root:document.getElementById('brd'),split:document.getElementById('brd-split'),api:window.api.wbrowser.drawer,
  ic:function(n){var p=ICONS[n];return p?'<svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'+p+'</svg>':''},toast:function(m){console.log('toast '+m)},listChats:function(){return [{id:'c1',title:'Chat one',focused:true}]},
  sendToChat:function(id,text,png){window.sent.push({id:id,text:text,png:png?png.length:0});return Promise.resolve();}});
drawer.setProject('proj');</script></body></html>`;
const web = http.createServer((req, res) => {
  if (req.url === '/') { res.setHeader('Content-Type', 'text/html'); res.end(PAGE); return; }
  if (req.url === '/vendor/crundi-browser-drawer.js') { res.setHeader('Content-Type', 'application/javascript'); res.end(readFileSync(join(root, 'app', 'vendor', 'crundi-browser-drawer.js'))); return; }
  res.statusCode = 404; res.end();
});
await new Promise((r) => web.listen(0, '127.0.0.1', r));
const wss = new WebSocketServer({ server: web, path: '/ws' });
let hostWs = null; const testWaits = new Map(); let testSeq = 1;
wss.on('connection', (ws) => {
  let tunnel = null;
  ws.on('message', (raw, isBinary) => {
    if (isBinary) { if (tunnel) tunnel.handle(raw); return; }
    const msg = JSON.parse(raw.toString());
    if (msg.type === 'register-native-host') {
      browser.setRemoteHost({ send: (m) => ws.send(JSON.stringify({ type: 'native-host', payload: m })), capabilities: msg.capabilities || [] });
      tunnel = createTunnelEndpoint({ send: (b) => ws.send(b, { binary: true }), buffered: () => ws.bufferedAmount, connect: serverConnect });
      hostWs = ws;
      ws.send(JSON.stringify({ type: 'native-host-ready', features: ['net-tunnel', 'browser-drawer'] }));
      return;
    }
    if (msg.type === 'native-host-result') { browser.handleHostMessage(msg.payload); return; }
    if (msg.type === 'test-result') { const w = testWaits.get(msg.id); if (w) { testWaits.delete(msg.id); w(msg); } }
  });
  ws.on('close', () => { if (tunnel) tunnel.closeAll(); browser.clearRemoteHost(); });
});
function client(op, extra) {
  const id = testSeq++;
  return new Promise((resolve, reject) => {
    const t = setTimeout(() => { testWaits.delete(id); reject(new Error('test call timed out')); }, 15000);
    testWaits.set(id, (m) => { clearTimeout(t); resolve(m); });
    hostWs.send(JSON.stringify({ type: 'test', id, op, ...extra }));
  });
}
const page = async (code) => (await client('exec', { code })).result;
const SHOTS = process.env.DRAWER_SHOTS_DIR || '';
const shot = async (name) => { if (!SHOTS) return; await sleep(400); const sh = await client('shot', {}); (await import('node:fs')).writeFileSync(join(SHOTS, name + '.png'), Buffer.from(sh.data, 'base64')); };
const as = (alias, fn) => browser.callerContext.run({ alias }, fn);
const proj = (fn) => as('proj', fn);

// ─── Launch the client ───
const userData = mkdtempSync(join(tmpdir(), 'crundi-drawer-ud-'));
const args = [join(root, 'scripts', 'drawer-e2e', 'harness.mjs'), '--no-sandbox', `--user-data-dir=${userData}`];
const env = { ...process.env, SERVER_URL: `http://127.0.0.1:${web.address().port}`, ELECTRON_DISABLE_SECURITY_WARNINGS: '1', EXIT_ON_CLOSE: '1', DRAWER_SLEEP_MS: '3000', DRAWER_UNLOAD_MS: '4000' };
// Its own process group, so the whole tree (xvfb-run, Xvfb, Electron) can be
// stopped together: killing xvfb-run alone leaves Electron and Xvfb running.
const child = needXvfb
  ? spawn('xvfb-run', ['-a', '-s', '-screen 0 1400x900x24', electron.bin, ...args], { env, stdio: ['ignore', 'pipe', 'pipe'], detached: true })
  : spawn(electron.bin, args, { env, stdio: ['ignore', 'pipe', 'pipe'], detached: true });
const exited = new Promise((r) => child.once('exit', r));
let clientLog = '';
child.stdout.on('data', (d) => { clientLog += d; });
child.stderr.on('data', (d) => { clientLog += d; });
const finish = async (code) => {
  // Ask Electron to quit (closing the socket does too), then stop the group.
  try { if (hostWs) await Promise.race([client('quit', {}), sleep(2000)]); } catch { /* gone */ }
  try { wss.close(); for (const c of wss.clients) c.terminate(); } catch { /* ignore */ }
  await Promise.race([exited, sleep(4000)]);
  try { process.kill(-child.pid, 'SIGTERM'); } catch { /* already gone */ }
  await sleep(300);
  try { process.kill(-child.pid, 'SIGKILL'); } catch { /* already gone */ }
  rmSync(userData, { recursive: true, force: true });
  if (electron.cleanup) rmSync(electron.cleanup, { recursive: true, force: true });
  web.close(); target.close();
  process.exit(code);
};
const watchdog = setTimeout(() => { console.log('FAIL timed out\n' + clientLog.split('\n').filter((l) => /harness|error/i.test(l)).slice(-30).join('\n')); finish(1); }, 180000);

try {
  // Up: registered, and the drawer page is ready.
  for (let i = 0; i < 100 && !hostWs; i++) await sleep(200);
  check('client registered on the server websocket as native host', !!hostWs, clientLog.slice(-500));
  for (let i = 0; i < 100; i++) { try { if (await page('typeof window.drawer')) break; } catch { /* page loading */ } await sleep(200); }
  await sleep(500);

  // ── A chat opens a tab and uses it ──
  const list0 = await proj(() => browser.listAllBrowsers('proj'));
  check('browser_list: the drawer is available, no tabs yet', list0.drawer === 'available' && !list0.browsers.length, JSON.stringify(list0));
  const o = await proj(() => browser.openDrawerTab({ alias: 'proj', url: `http://127.0.0.1:${T}/page`, show: true }));
  check('browser_open on: "drawer" opens a tab (page that refuses framing loads)', o.ok && /^drawer:/.test(o.key) && o.network === 'client' && o.networkLabel === 'This PC', JSON.stringify(o));
  const key = o.key;
  const title = await proj(() => browser.evalBrowser(key, 'document.title'));
  check('browser_eval reads the page', title.ok && title.result === 'Target page', JSON.stringify(title));
  check('the drawer is open on that tab', await page('drawer.isOpen() && document.querySelectorAll(".brd-wv.on").length === 1'));
  await proj(() => browser.clickBrowser(key, '#btn'));
  const clicks = await proj(() => browser.evalBrowser(key, 'window.clicks'));
  check('browser_click', clicks.result === '1', JSON.stringify(clicks));
  await proj(() => browser.typeBrowser(key, 'hello', '#inp'));
  const typed = await proj(() => browser.evalBrowser(key, 'document.getElementById("inp").value'));
  check('browser_type', typed.result === 'hello', JSON.stringify(typed));
  const png = await proj(() => browser.screenshotBrowser(key));
  check('browser_screenshot returns a PNG', png.ok && Buffer.from(png.data, 'base64').subarray(1, 4).toString() === 'PNG', png.error);
  await proj(() => browser.openDrawerTab({ alias: 'proj', url: `http://127.0.0.1:${T}/page2` }));
  await shot('1-drawer');

  // ── Scoped to the project ──
  const other = await as('other', () => browser.evalBrowser(key, '1'));
  check('another project\'s chat is refused', !other.ok && /another project/.test(other.error), JSON.stringify(other));
  const otherList = await as('other', () => browser.listAllBrowsers('other'));
  check('another project lists none of these tabs', !otherList.browsers.length);
  const collab = await browser.callerContext.run({ alias: 'proj', noDrawer: true }, () => browser.evalBrowser(key, '1'));
  check('a collaborator\'s chat is refused', !collab.ok && /not available/.test(collab.error), JSON.stringify(collab));

  // ── Point and annotate, where the person sees it ──
  const pt = await proj(() => browser.pointDrawer(key, { selector: '#btn', label: 'this button' }));
  check('browser_point on a selector', pt.ok && pt.drawn === 1, JSON.stringify(pt));
  const bb0 = JSON.parse((await proj(() => browser.evalBrowser(key, 'JSON.stringify(document.getElementById("inp").getBoundingClientRect())'))).result);
  const an = await proj(() => browser.annotateDrawer(key, { shapes: [{ kind: 'box', x: 200, y: 20, width: 120, height: 60, text: 'empty area' }, { kind: 'arrow', x: 300, y: 200, toX: 100, toY: 60 }] }));
  check('browser_annotate draws shapes', an.ok && an.total === 3, JSON.stringify(an));
  const bad = await proj(() => browser.annotateDrawer(key, { shapes: [{ kind: 'box', x: 1, y: 1 }] }));
  check('a box without a size is refused', !bad.ok && /width and height/.test(bad.error));
  const lbl = await page('(function(){var l=document.querySelector(".brd-lbl.claude");var r=l.getBoundingClientRect();return {x:r.left+r.width-6,y:r.top+r.height/2};})()');
  const px = await client('pixel', lbl);
  check('the marks draw over the live page (overlay pixel is the mark\'s colour)', px.rgb && Math.abs(px.rgb[0] - 99) < 30 && Math.abs(px.rgb[2] - 241) < 30, JSON.stringify(px.rgb));
  const pagePx = await client('pixel', await page('(function(){var r=document.querySelector(".brd-wv.on").getBoundingClientRect();return {x:r.right-20,y:r.bottom-20};})()'));
  check('and the page shows around them', pagePx.rgb && pagePx.rgb[1] > 200 && pagePx.rgb[0] < 60, JSON.stringify(pagePx.rgb));
  await page('document.querySelector(\'[data-brd="annotate"]\').click()');
  await page(`(function(){var a=document.querySelector('.brd-ann'),b=a.getBoundingClientRect();a.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,clientX:b.left+${Math.round(bb0.x+20)},clientY:b.top+${Math.round(bb0.y+15)},pointerId:1}));})()`);
  await sleep(500);
  const hov = await page('(function(){var h=document.querySelector(".brd-hov");return h?h.textContent:("none:"+document.querySelector(".brd-ann").className);})()');
  check('the element picker outlines what is under the pointer, with its selector', hov === '#inp', JSON.stringify(hov));
  await shot('3-pick-hover');
  await page('document.querySelector(\'[data-ann="done"]\').click()');

  // ── The person marks the page and sends it to a chat ──
  const ptr = (tool, pts, note) => page(`(async function(){
    var bar=document.querySelector('.brd-annbar'); if (bar.hidden) document.querySelector('[data-brd="annotate"]').click();
    document.querySelector('[data-tool="${tool}"]').click();
    var a=document.querySelector('.brd-ann'); var b=a.getBoundingClientRect();
    function ev(t,x,y){ a.dispatchEvent(new PointerEvent(t,{bubbles:true,clientX:b.left+x,clientY:b.top+y,button:0,pointerId:1})); }
    var p=${JSON.stringify(pts)}; ev('pointerdown',p[0][0],p[0][1]); for (var i=1;i<p.length;i++) ev('pointermove',p[i][0],p[i][1]); ev('pointerup',p[p.length-1][0],p[p.length-1][1]);
    for (var j=0;j<40 && !document.querySelector('.brd-note input');j++) await new Promise(function(r){setTimeout(r,50);});
    var inp=document.querySelector('.brd-note input'); inp.value=${JSON.stringify(note)};
    inp.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));
    return true; })()`);
  const btnBox = await proj(() => browser.evalBrowser(key, 'JSON.stringify(document.getElementById("btn").getBoundingClientRect())'));
  const bb = JSON.parse(btnBox.result);
  await ptr('pick', [[bb.x + 10, bb.y + 10]], 'make this red');
  const cssOn = await page('!document.querySelector(\'[data-ann="send-css"]\').disabled');
  check('select an element: "CSS selectors" can be sent', cssOn === true);
  await page('document.querySelector(\'[data-ann="send-css"]\').click()');
  await sleep(800);
  let sent = await page('window.sent');
  check('"CSS selectors" sends the note, selector, tag and HTML, and no picture', sent.length === 1 && sent[0].png === 0 && /make this red/.test(sent[0].text) && /selector: #btn/.test(sent[0].text) && /<button>/.test(sent[0].text) && /html: <button id="btn"/.test(sent[0].text), JSON.stringify(sent).slice(0, 500));
  await ptr('box', [[30, 120], [90, 160], [160, 200]], 'too much space');
  const cssOff = await page('document.querySelector(\'[data-ann="send-css"]\').disabled && !document.querySelector(\'[data-ann="send-shot"]\').disabled');
  check('once something is drawn, only "Full screenshot" can be sent', cssOff === true);
  await page('document.querySelector(\'[data-ann="send-shot"]\').click()');
  await sleep(1500);
  sent = await page('window.sent');
  const last = sent[sent.length - 1] || {};
  check('"Full screenshot" attaches your marked-up page; the element keeps its selector, the drawing has none (Claude\'s own marks are not sent back)', sent.length === 2 && last.png > 1000 && /selector: #btn/.test(last.text) && /box at .*see the screenshot.*too much space/.test(last.text) && (last.text.match(/selector:/g) || []).length === 1 && !/Claude/.test(last.text), JSON.stringify(last).slice(0, 600));
  const read = await proj(() => browser.readDrawerAnnotations(key));
  const info = read.ok ? JSON.parse(read.content[0].text) : {};
  const mineEl = (info.annotations || []).find((a) => a.by === 'person' && a.kind === 'element');
  const mineBox = (info.annotations || []).find((a) => a.by === 'person' && a.kind === 'box');
  check('browser_annotations: the selected element with its note and selector', !!mineEl && mineEl.text === 'make this red' && mineEl.element && mineEl.element.selector === '#btn', JSON.stringify(info).slice(0, 400));
  check('and the drawing, with its note and no selector', !!mineBox && mineBox.text === 'too much space' && !mineBox.element, JSON.stringify(mineBox));
  const pin = (info.annotations || []).find((a) => a.by === 'claude' && a.kind === 'pin');
  check('a chat\'s pin made from a selector keeps it', pin && pin.element && pin.element.selector === '#btn', JSON.stringify(pin));
  check('browser_annotations includes the annotated screenshot', read.content[1] && read.content[1].type === 'image' && read.content[1].data.length > 1000);
  await shot('2-annotate');
  await page('document.querySelector(\'[data-ann="done"]\').click()');

  // ── Hidden, and other projects: pages keep running, nothing reloads ──
  await proj(() => browser.evalBrowser(key, 'window.__mark = 42'));
  const loadsBefore = pageLoads;
  await page('drawer.close()');
  check('closed drawer: the project area takes the width back', await page('document.getElementById("brd").classList.contains("brd-closed") && document.getElementById("area").getBoundingClientRect().width > 1200'));
  const hidEval = await proj(() => browser.evalBrowser(key, 'window.__mark + ":" + innerWidth'));
  const hidShot = await proj(() => browser.screenshotBrowser(key));
  check('a chat can still use the tab while the drawer is hidden (same page, full size)', /^42:\d{3,}$/.test(hidEval.result || '') && hidShot.ok && hidShot.data.length > 1000, JSON.stringify(hidEval));
  await page('drawer.open()');
  await page('drawer.setProject("other")');
  const otherProjEval = await proj(() => browser.evalBrowser(key, 'window.__mark'));
  check('the tab keeps running while another project is shown', otherProjEval.result === '42');
  check('the other project shows none of them', await page('document.querySelectorAll(".brd-wv.on").length === 0 && document.querySelectorAll(".brd-tab").length === 0'));
  await page('drawer.setProject("proj")');
  const back = await proj(() => browser.evalBrowser(key, 'window.__mark'));
  check('hiding, showing and switching projects never reloaded the page', back.result === '42' && pageLoads === loadsBefore, `${back.result} loads ${loadsBefore}->${pageLoads}`);
  check('open drawer: the project area narrowed', await page('document.getElementById("area").getBoundingClientRect().width < 900'));

  // ── Device emulation ──
  await proj(() => browser.resizeBrowser(key, 390, 844));
  await sleep(2500);
  const dev = await proj(() => browser.evalBrowser(key, 'innerWidth + "x" + innerHeight'));
  check('browser_resize emulates a device size', dev.result === '390x844', JSON.stringify(dev));
  await shot('4-device');
  const box = await page('(function(){var w=document.querySelector(".brd-wv.on").getBoundingClientRect(),s=document.querySelector(".brd-stage").getBoundingClientRect();return [Math.round(w.width),Math.round(w.height),Math.round(s.height)];})()');
  check('the device is scaled to fit the drawer, in proportion', Math.abs(box[0] / box[1] - 390 / 844) < 0.02 && box[1] <= box[2], JSON.stringify(box));
  await page('(function(){var s=document.querySelector(".brd-dev");s.value="0";s.dispatchEvent(new Event("change"));})()');
  await sleep(2500);
  const resp = await proj(() => browser.evalBrowser(key, 'innerWidth'));
  check('back to responsive fills the drawer', Number(resp.result) > 400, JSON.stringify(resp));

  // ── Networks ──
  const opensBefore = tunnelOpens;
  const s1 = await proj(() => browser.openDrawerTab({ alias: 'proj', url: `http://internal.test:${T}/whoami`, network: 'server' }));
  check('a tab on the server network opens', s1.ok && s1.network === 'server' && s1.networkLabel === 'Server', JSON.stringify(s1));
  const who = await proj(() => browser.evalBrowser(s1.key, 'document.body.innerText'));
  check('it reaches a host only the server can resolve, through the client\'s websocket', /via-server internal\.test/.test(who.result || '') && tunnelOpens > opensBefore, JSON.stringify(who));
  const lh = await proj(() => browser.navigateBrowser(s1.key, `http://localhost:${T}/whoami`));
  const lhText = await proj(() => browser.evalBrowser(s1.key, 'document.body.innerText'));
  check('localhost on the server network is the server\'s localhost (goes through the tunnel)', lh.ok && /via-server localhost/.test(lhText.result || '') && tunnelOpens > opensBefore + 1, JSON.stringify(lhText));
  const opensMid = tunnelOpens;
  await proj(() => browser.evalBrowser(key, 'location.reload()'));
  await sleep(800);
  check('a tab on the client network does not use the tunnel', tunnelOpens === opensMid, `${opensMid} -> ${tunnelOpens}`);
  const sw = await proj(() => browser.setDrawerNetwork(key, 'server'));
  check('browser_tab_network switches a tab to the server network', sw.ok && sw.network === 'server', JSON.stringify(sw));
  const nav = await proj(() => browser.navigateBrowser(key, `http://internal.test:${T}/whoami`));
  check('and it then reaches the server-only host', nav.ok, JSON.stringify(nav));
  await proj(() => browser.setDrawerNetwork(key, 'client'));
  const nav2 = await proj(() => browser.navigateBrowser(key, `http://internal.test:${T}/whoami`));
  check('back on the client network it cannot (proves the switch)', !nav2.ok && /Load failed/.test(nav2.error || ''), JSON.stringify(nav2));
  check('the drawer shows which network each tab is on', await page('document.querySelectorAll(".brd-tab .brd-net-dot.server").length === 1'));
  await page(`(function(){var t=[].find.call(document.querySelectorAll(".brd-tab"),function(x){return x.querySelector(".brd-net-dot")});t.click();})()`);
  check('and the toolbar names it', await page('document.querySelector(".brd-netsel").value === "server" && /Server/.test(document.querySelector(".brd-netsel").selectedOptions[0].textContent)'));
  await shot('5-server-network');

  // ── One browser per client: logins and storage shared by every tab, in every project ──
  await proj(() => browser.navigateBrowser(key, `http://127.0.0.1:${T}/page`));
  await proj(() => browser.evalBrowser(key, 'document.cookie = "login=alice; max-age=3600"; localStorage.setItem("cart", "3"); 1'));
  const oth = await as('other', () => browser.openDrawerTab({ alias: 'other', url: `http://127.0.0.1:${T}/page` }));
  const seen = await as('other', () => browser.evalBrowser(oth.key, 'document.cookie + "|" + localStorage.getItem("cart")'));
  check('a login made in one project\'s tab is there in another project\'s tab (one browser)', /login=alice/.test(seen.result || '') && /\|3$/.test(seen.result || ''), JSON.stringify(seen));
  const hist = await page('window.api.wbrowser.drawer.history("Target page", 5)');
  check('history: visits are kept, and found by title or address', hist.items && hist.items.some((h) => /\/page$/.test(h.url) && h.title === 'Target page' && h.visits >= 2), JSON.stringify(hist).slice(0, 300));
  await page('document.querySelector(\'[data-brd="menu"]\').click()');
  await page('document.querySelector(\'[data-pan="history"]\').click()');
  await sleep(500);
  check('the History view lists them', await page('document.querySelectorAll(".brd-hrow").length >= 2'));
  await shot('6-history');
  await page('document.querySelector(\'.brd-ph [data-pan="clear"]\').click()');
  await page('(function(){var c=document.querySelector(\'[data-clr="cookies"]\');c.checked=true;})()');
  await shot('7-clear');
  await page('document.querySelector(\'[data-pan="do-clear"]\').click()');
  await sleep(1000);
  const after = await as('other', () => browser.evalBrowser(oth.key, 'document.cookie + "|" + localStorage.getItem("cart")'));
  const hist2 = await page('window.api.wbrowser.drawer.history("", 50)');
  check('"Clear browsing data" clears cookies, site storage and history', after.result === '|null' && hist2.items.length === 0, JSON.stringify(after) + ' ' + JSON.stringify(hist2).slice(0, 200));
  check('and the open tabs stay open', (await as('other', () => browser.evalBrowser(oth.key, 'document.title'))).result === 'Target page');
  await as('other', () => browser.closeBrowser(oth.key));

  // ── Sleep: a tab nobody looks at or uses is frozen, and wakes as it was ──
  await proj(() => browser.navigateBrowser(key, `http://127.0.0.1:${T}/page`));
  await page(`(function(){var t=[].find.call(document.querySelectorAll(".brd-tab"),function(x){return !x.querySelector(".brd-net-dot")});t.click();})()`);
  await proj(() => browser.evalBrowser(key, 'window.__z = 42; window.__n = 0; setInterval(function(){ window.__n++; }, 100); 1'));
  await page('drawer.close()');
  const t0 = Date.now();
  let slept = false;
  for (let i = 0; i < 40 && !slept; i++) { await sleep(250); slept = (await proj(() => browser.listAllBrowsers('proj'))).browsers.some((b) => b.key === key && b.sleeping); }
  const sleptAfter = Date.now() - t0;
  check('a hidden tab falls asleep after its idle time (3 s here, 5 min by default)', slept && sleptAfter >= 2500, `slept=${slept} after ${sleptAfter} ms`);
  await sleep(3000);
  const woke = await proj(() => browser.evalBrowser(key, 'window.__z + ":" + window.__n'));
  const [z, n] = String(woke.result || '').split(':').map(Number);
  check('while asleep its scripts stopped; a chat using it wakes it first, with nothing reloaded', z === 42 && n > 10 && n < (Date.now() - t0) / 100 - 15, JSON.stringify(woke) + ` over ${Date.now() - t0} ms`);
  const awake = (await proj(() => browser.listAllBrowsers('proj'))).browsers.find((b) => b.key === key);
  check('and it is listed awake again', awake && !awake.sleeping, JSON.stringify(awake));

  // ── Unload: asleep for its unload time (4 s here, 1 h by default), the page is closed ──
  const loadsBeforeUnload = pageLoads;
  let unloaded = null;
  for (let i = 0; i < 80 && !unloaded; i++) { await sleep(250); unloaded = (await proj(() => browser.listAllBrowsers('proj'))).browsers.find((b) => b.key === key && b.unloaded); }
  check('a tab asleep for its unload time is unloaded (listed, page closed)', !!unloaded && /\/page$/.test(unloaded.url || ''), JSON.stringify(unloaded));
  check('the tab strip still has it', await page('[].some.call(document.querySelectorAll(".brd-tab.sleeping"), function(x){ return /Unloaded/.test(x.title); })'));
  const again = await proj(() => browser.evalBrowser(key, 'document.title + ":" + typeof window.__z'));
  check('a chat using it loads it again at the same address (the one reload)', again.result === 'Target page:undefined' && pageLoads === loadsBeforeUnload + 1, JSON.stringify(again) + ` loads ${loadsBeforeUnload}->${pageLoads}`);
  await page('drawer.open()');

  // ── Popups become tabs; closing ──
  await proj(() => browser.navigateBrowser(key, `http://127.0.0.1:${T}/page`));
  const nBefore = (await proj(() => browser.listAllBrowsers('proj'))).browsers.length;
  await proj(() => browser.clickBrowser(key, '#pop'));
  await sleep(2500);
  const afterPop = (await proj(() => browser.listAllBrowsers('proj'))).browsers;
  check('a link that opens a window becomes a new drawer tab', afterPop.length === nBefore + 1 && afterPop.some((b) => /\/page2$/.test(b.url || '')), JSON.stringify(afterPop.map((b) => b.url)));
  const cl = await proj(() => browser.closeBrowser(s1.key));
  const afterClose = (await proj(() => browser.listAllBrowsers('proj'))).browsers;
  check('browser_close closes a drawer tab', cl.ok && !afterClose.some((b) => b.key === s1.key), JSON.stringify(cl));

  // ── Hardening ──
  const evil = await page(`new Promise(function(res){ var w=document.createElement('webview'); w.setAttribute('partition','persist:evil'); w.setAttribute('src','about:blank');
    document.body.appendChild(w); setTimeout(function(){ var ok=false; try{ w.getWebContentsId(); ok=true; }catch(e){} res(ok); }, 2500); })`);
  check('the page cannot create a webview outside the drawer\'s partitions', evil === false);
  const nodeInGuest = await proj(() => browser.evalBrowser(key, 'typeof require + "," + typeof process'));
  check('drawer pages have no Node', nodeInGuest.result === 'undefined,undefined', JSON.stringify(nodeInGuest));
} catch (err) {
  check('no exceptions', false, err.stack);
}
clearTimeout(watchdog);
if (failed) console.log('\nclient log tail:\n' + clientLog.split('\n').filter((l) => /harness/.test(l)).slice(-25).join('\n'));
console.log(failed ? `\n${failed} failed` : '\nall passed');
finish(failed ? 1 : 0);
