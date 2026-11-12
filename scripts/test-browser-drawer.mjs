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
import { createHomepoolService } from '../src/homepool-service.js';
import { startNode } from '../app/homepool-node.js';
import { chatToken } from '../src/chat-token.js';
import { gateBrowserCall } from '../src/browser-gate.js';
import { dispatchBrowserTool } from '../src/browser-dispatch.js';
import { makeCruBrowserTools } from '../src/voice/browser-tools.js';

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
  // browser_mouse: a page whose iframe is another site (localhost inside
  // 127.0.0.1, so a separate frame process), and a pad that records a drag.
  // Saved passwords: a sign-in page that works unless the password is "bad".
  if (req.url.startsWith('/login')) {
    res.end(`<!doctype html><title>Sign in</title><body style="margin:20px;font-family:sans-serif">
      <form id="f"><input id="u" name="username" type="text" placeholder="Username"><input id="p" name="password" type="password" placeholder="Password"><button id="go" type="submit">Sign in</button></form><div id="msg"></div>
      <script>window.lastPw='';document.getElementById('f').addEventListener('submit',function(e){e.preventDefault();var p=document.getElementById('p').value;window.lastPw=p;
      if(p==='bad'){document.getElementById('msg').textContent='Wrong password';document.getElementById('p').value='';return;}
      document.getElementById('f').remove();document.getElementById('msg').textContent='Welcome';});</script></body>`);
    return;
  }
  if (req.url.startsWith('/inner')) {
    res.removeHeader('X-Frame-Options'); res.removeHeader('Content-Security-Policy');
    res.end('<body style="margin:0"><button id="b" style="width:300px;height:200px">inner</button><script>document.getElementById("b").onclick=function(){parent.postMessage("inner-click","*")}</script></body>');
    return;
  }
  if (req.url.startsWith('/framed')) {
    res.end(`<!doctype html><title>Framed</title><body style="margin:0">
      <iframe id="f" src="http://localhost:${T}/inner" style="position:absolute;left:0;top:0;width:300px;height:200px;border:0"></iframe>
      <div id="pad" style="position:absolute;left:310px;top:0;width:200px;height:250px;background:#eef"></div>
      <script>window.innerClicks=0;addEventListener('message',function(e){if(e.data==='inner-click')innerClicks++;});
      window.drag={moves:0};var pad=document.getElementById('pad');
      pad.addEventListener('mousedown',function(e){drag={down:[e.clientX,e.clientY],moves:0};});
      addEventListener('mousemove',function(e){if(e.buttons===1)drag.moves++;});
      addEventListener('mouseup',function(e){drag.up=[e.clientX,e.clientY];});
      window.ctx=0;pad.addEventListener('contextmenu',function(e){e.preventDefault();ctx++;});
      window.dbl=0;pad.addEventListener('dblclick',function(){dbl++;});</script></body>`);
    return;
  }
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

// ─── Homepool: a "Home" network with a node, and a network this PC is a node of ───
const hpDir = mkdtempSync(join(tmpdir(), 'crundi-drawer-hp-'));
const told = [];
const homepool = createHomepoolService({ dataDir: hpDir, basePort: 17650, log: () => {}, notifyOwner: (m) => told.push(m), chatInfo: () => ({ title: 'Test chat', project: 'proj' }) });
const HOME = (homepool.store.networks().find((n) => n.id === 'home') || {}).id || homepool.store.addNetwork('Home');
const ALIS = homepool.store.addNetwork("Ali's place");
const phone = homepool.store.addNode({ name: 'phone', network: HOME });
homepool.store.addNode({ name: 'this PC', network: ALIS, kind: 'crundi-app', deviceId: 'pc-test' });
const DEVICE = 'pc-test';
let phoneNode = null;
// A chat as Crundi starts it: an id, its project, and the token that proves it.
const chat = (id, project = 'proj') => ({ sessionId: id, alias: project, chatToken: chatToken(id, project) });
// A chat's MCP call, the way /api/mcp/call runs it: gate, then dispatch.
async function chatCall(tool, args, who) {
  const a = { ...args, ...who };
  const refused = await gateBrowserCall({ tool, args: a, homepool, browser });
  if (refused) return refused;
  return dispatchBrowserTool(tool, a);
}
const cru = makeCruBrowserTools({ call: (tool, args) => dispatchBrowserTool(tool, args) });
const cruTool = (name) => [...cru.read, ...cru.actions].find((t) => t.name === name);

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
  sendToChat:function(id,text,png){window.sent.push({id:id,text:text,png:png?png.length:0});return Promise.resolve();},
  apiFetch:function(p,i){return fetch(p,i);},deviceId:function(){return 'pc-test';}});
drawer.setProject('proj');</script></body></html>`;
const web = http.createServer((req, res) => {
  if (req.url === '/') { res.setHeader('Content-Type', 'text/html'); res.end(PAGE); return; }
  if (req.url === '/vendor/crundi-browser-drawer.js') { res.setHeader('Content-Type', 'application/javascript'); res.end(readFileSync(join(root, 'app', 'vendor', 'crundi-browser-drawer.js'))); return; }
  // The owner's PIN (src/webapp.js /api/lock, /api/lock/check), here 1234.
  if (req.url === '/api/lock') { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify({ ok: true, on: false, pinSet: true })); return; }
  if (req.url === '/api/lock/check' && req.method === 'POST') {
    let b = ''; req.on('data', (d) => { b += d; });
    req.on('end', () => { let pin = ''; try { pin = JSON.parse(b).pin; } catch { /* bad body */ } res.statusCode = pin === '1234' ? 200 : 403; res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify(pin === '1234' ? { ok: true } : { ok: false, error: 'That is not the PIN' })); });
    return;
  }
  if (req.url.startsWith('/api/homepool')) {
    const path = new URL(req.url, 'http://x').pathname;
    const readBody = () => new Promise((r) => { let b = ''; req.on('data', (d) => { b += d; }); req.on('end', () => r(b)); });
    const json = (r2, obj, code = 200) => { r2.statusCode = code; r2.setHeader('Content-Type', 'application/json'); r2.end(JSON.stringify(obj)); };
    homepool.handleApi(req, res, path, { principal: {}, readBody, json, isOwner: () => true });
    return;
  }
  res.statusCode = 404; res.end();
});
await new Promise((r) => web.listen(0, '127.0.0.1', r));
const wss = new WebSocketServer({ noServer: true });
web.on('upgrade', (req, socket, head) => {
  if (homepool.handleUpgrade(req, socket, head)) return;   // a homepool node
  if (req.url.startsWith('/ws')) wss.handleUpgrade(req, socket, head, (ws) => wss.emit('connection', ws, req));
  else socket.destroy();
});
let hostWs = null; const testWaits = new Map(); let testSeq = 1;
wss.on('connection', (ws) => {
  let tunnel = null;
  ws.on('message', (raw, isBinary) => {
    if (isBinary) { if (tunnel) tunnel.handle(raw); return; }
    const msg = JSON.parse(raw.toString());
    if (msg.type === 'register-native-host') {
      browser.setRemoteHost({ send: (m) => ws.send(JSON.stringify({ type: 'native-host', payload: m })), capabilities: msg.capabilities || [], deviceId: msg.deviceId || '' });
      tunnel = createTunnelEndpoint({ send: (b) => ws.send(b, { binary: true }), buffered: () => ws.bufferedAmount, connect: serverConnect,
        connectVia: (spec) => homepool.browserConnect({ ...spec, deviceId: msg.deviceId || '' }) });
      hostWs = ws;
      ws.send(JSON.stringify({ type: 'native-host-ready', features: ['net-tunnel', 'browser-drawer', 'browser-homepool'] }));
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
const env = { ...process.env, SERVER_URL: `http://127.0.0.1:${web.address().port}`, ELECTRON_DISABLE_SECURITY_WARNINGS: '1', EXIT_ON_CLOSE: '1', DRAWER_SLEEP_MS: '3000', DRAWER_UNLOAD_MS: '4000', DRAWER_DEVICE: DEVICE };
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
  try { phoneNode && phoneNode.stop(); } catch { /* gone */ }
  rmSync(hpDir, { recursive: true, force: true });
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

  // ── Homepool: the PC browser key, set up from the drawer ──
  phoneNode = startNode({ server: `http://127.0.0.1:${web.address().port}`, key: phone.key, allowPrivate: true, info: () => ({ platform: 'test', network: 'Wi-Fi', publicIp: '203.0.113.7' }) });
  for (let i = 0; i < 50 && !homepool.engine.links.has(phone.id); i++) await sleep(100);
  check('a homepool node of Home is up', homepool.engine.links.has(phone.id));
  await page(`document.querySelector('.brd-tab[data-tab="${key.slice(7)}"]').click()`);
  check('network and device size are icon buttons, leaving the room to the address', await page(`getComputedStyle(document.querySelector('.brd-netsel')).display === 'none' && getComputedStyle(document.querySelector('.brd-dev')).display === 'none' && !!document.querySelector('.brd-netbtn') && !!document.querySelector('.brd-devbtn')`));
  await page(`document.querySelector('[data-brd="netmenu"]').click()`); await sleep(600);
  const menu1 = await page(`[].map.call(document.querySelectorAll('.brd-pop .brd-pi'), function(b){ return b.dataset.v; }).join(',')`);
  check('with no PC browser key, the network menu offers to set homepool up', /^client,server/.test(menu1) && /hp-panel/.test(menu1) && !/hp:/.test(menu1), menu1);
  await page(`document.querySelector('.brd-pop [data-v="hp-panel"]').click()`); await sleep(800);
  const setup = await page(`(function(){ var h = document.querySelector('[data-hpnet="${HOME}"]'), a = document.querySelector('[data-hpnet="${ALIS}"]'); return h && a ? [h.checked, h.disabled, a.checked, a.disabled].join(',') : 'none'; })()`);
  check('set-up lists the networks; the one this PC is a node of is not offered', setup === 'true,false,false,true', setup);
  await shot('8-homepool-setup');
  await page(`document.querySelector('[data-pan="hp-create"]').click()`); await sleep(1000);
  const bk = homepool.store.browserKey();
  check('"Make the PC browser key" makes it, with the networks ticked (not the one this PC is a node of)', bk && bk.kind === 'browser' && bk.networks.includes(HOME) && !bk.networks.includes(ALIS), JSON.stringify(bk && bk.networks));
  await page(`document.querySelector('[data-pan="close"]').click()`);
  await page(`document.querySelector('[data-brd="netmenu"]').click()`); await sleep(600);
  const menu2 = await page(`[].map.call(document.querySelectorAll('.brd-pop .brd-pi'), function(b){ return b.dataset.v; }).join(',')`);
  check('the menu now offers Home, and not the network this PC is a node of', menu2.includes('hp:' + HOME) && !menu2.includes('hp:' + ALIS), menu2);
  await page(`document.querySelector('.brd-pop [data-v="hp:${HOME}"]').click()`); await sleep(1500);
  const viaHome = await dispatchBrowserTool('browser_navigate', { key, url: `http://127.0.0.1:${T}/whoami`, _cru: true });
  const homeText = await dispatchBrowserTool('browser_eval', { key, code: 'document.body.innerText', _cru: true });
  check('a tab on Home loads through the home node, over the client\'s websocket', viaHome.ok && /via-server 127\.0\.0\.1/.test(homeText.result || '') && (homepool.store.usage().nodes[phone.id] || {}).conns > 0, JSON.stringify(homeText) + JSON.stringify(homepool.store.usage().nodes));
  await page(`document.querySelector('[data-brd="netmenu"]').click()`); await sleep(700);
  const exitShown = await page(`(document.querySelector('.brd-pop .brd-pn') || {}).textContent + ' | ' + document.querySelector('.brd-netbtn').title + ' | ' + document.querySelector('.brd-netbtn').className + ' | ' + !!document.querySelector('.brd-tab .brd-net-dot.hp:not(.bad)')`);
  check('it shows the exit: the node and its IP, tab marked HP', /via phone/.test(exitShown) && /203\.0\.113\.7/.test(exitShown) && /hp/.test(exitShown) && / \| true$/.test(exitShown), exitShown);
  await shot('9-homepool-tab');
  await page(`document.querySelector('[data-brd="netmenu"]').click()`);

  // ── Chats ask first ──
  const denied = await chatCall('browser_eval', { key, code: '1' }, chat('chat-a'));
  check('a chat not approved cannot use a homepool tab', !denied.ok && /homepool/.test(denied.error || ''), JSON.stringify(denied));
  const t2 = await proj(() => browser.openDrawerTab({ alias: 'proj', url: `http://127.0.0.1:${T}/page2` }));
  const self = await chatCall('browser_tab_network', { key: t2.key, network: 'homepool:' + ALIS }, chat('chat-a'));
  check('a network this PC is a node of is refused to chats', !self.ok && /itself a node/.test(self.error || ''), JSON.stringify(self));
  const asking = chatCall('browser_tab_network', { key: t2.key, network: 'homepool:' + HOME, reason: 'check the shop from home' }, chat('chat-a'));
  let req = null;
  for (let i = 0; i < 40 && !req; i++) { await sleep(100); req = homepool.store.requests().find((r) => r.status === 'pending' && r.kind === 'browser'); }
  check('switching onto homepool asks the owner (a request, and he is told)', !!req && told.some((m) => /browse through it/.test(m.title)) && req.reason === 'check the shop from home', JSON.stringify(req));
  const dec = await (await fetch(`http://127.0.0.1:${web.address().port}/api/homepool`, { method: 'POST', body: JSON.stringify({ action: 'request-decide', id: req.id, approve: true }) })).json();
  const asked = await asking;
  check('once he approves, the switch goes ahead', dec.ok && asked.ok && asked.network === 'homepool:' + HOME, JSON.stringify(asked));
  const usesIt = await chatCall('browser_eval', { key: t2.key, code: 'document.title' }, chat('chat-a'));
  check('and that chat may use homepool tabs', usesIt.ok, JSON.stringify(usesIt));
  const other2 = await chatCall('browser_eval', { key: t2.key, code: '1' }, chat('chat-b'));
  check('another chat still may not', !other2.ok, JSON.stringify(other2));
  const forged = await chatCall('browser_eval', { key: t2.key, code: '1' }, { sessionId: 'chat-a', alias: 'proj', chatToken: 'forged' });
  check('a chat cannot pass for an approved one', !forged.ok, JSON.stringify(forged));
  await page(`document.querySelector('[data-brd="netmenu"]').click()`); await sleep(600);
  await page(`document.querySelector('.brd-pop [data-v="hp-panel"]').click()`); await sleep(800);
  check('the drawer lists the approved chat', await page(`!!document.querySelector('[data-hprm-chat="chat-a"]')`));
  await page(`document.querySelector('[data-hprm-chat="chat-a"]').click()`); await sleep(800);
  const revoked = await chatCall('browser_eval', { key: t2.key, code: '1' }, chat('chat-a'));
  check('removing it there ends its access', !revoked.ok && !homepool.store.browserAllows('chat-a', 'proj'), JSON.stringify(revoked));
  await page(`document.querySelector('[data-pan="close"]').click()`);

  // ── Cru: every project's tabs ──
  const cruOther = await cruTool('browser_go').run({ url: `http://127.0.0.1:${T}/page2`, project: 'other', show: false });
  const cruTabs = await cruTool('browser_tabs').run({});
  check('Cru sees the drawer tabs of every project', cruTabs.ok && cruTabs.tabs.some((t) => t.alias === 'proj') && cruTabs.tabs.some((t) => t.alias === 'other'), JSON.stringify(cruTabs).slice(0, 300));
  const cruLook = await cruTool('browser_look').run({ key });
  check('Cru can look at a homepool tab (its text and a picture)', cruLook.content && /via-server/.test(cruLook.content[0].text) && cruLook.content[1] && cruLook.content[1].type === 'image', JSON.stringify(cruLook).slice(0, 200));
  const cruMark = await cruTool('browser_mark').run({ key: cruOther.key, selector: 'body', label: 'Cru was here' });
  check('Cru can open a tab in any project and point at it', cruOther.ok && cruMark.ok, JSON.stringify(cruMark));

  // ── No exit up ──
  phoneNode.stop(); phoneNode = null;
  for (let i = 0; i < 50 && homepool.engine.links.has(phone.id); i++) await sleep(100);
  await dispatchBrowserTool('browser_navigate', { key, url: `http://127.0.0.1:${T}/whoami?again`, _cru: true });
  const noExit = await dispatchBrowserTool('browser_eval', { key, code: 'document.body.innerText', _cru: true });
  check('with no home node up, the page says no exit is up (nothing leaves another way)', /No exit is up/.test(noExit.result || ''), JSON.stringify(noExit));
  await page(`document.querySelector('[data-brd="netmenu"]').click()`); await sleep(700);
  check('and the network icon and the tab badge turn red', await page(`document.querySelector('.brd-netbtn').classList.contains('bad') && !!document.querySelector('.brd-tab .brd-net-dot.hp.bad')`));
  await shot('10-no-exit');
  await page(`document.querySelector('[data-brd="netmenu"]').click()`);

  // ── Clear one site ──
  await dispatchBrowserTool('browser_eval', { key: cruOther.key, code: 'document.cookie = "a=1; max-age=3600"; 1', _cru: true });
  const lhTab = await as('other', () => browser.openDrawerTab({ alias: 'other', url: `http://localhost:${T}/page2` }));
  await dispatchBrowserTool('browser_eval', { key: lhTab.key, code: 'document.cookie = "b=2; max-age=3600"; 1', _cru: true });
  await page(`document.querySelector('[data-brd="menu"]').click()`);
  await page(`document.querySelector('[data-pan="clear"]').click()`);
  const prefilled = await page(`document.querySelector('.brd-site').value`);
  await page(`(function(){ document.querySelector('input[name="brd-scope"][value="site"]').checked = true; document.querySelector('[data-clr="cookies"]').checked = true; })()`);
  await shot('11-clear-site');
  await page(`document.querySelector('[data-pan="do-clear"]').click()`); await sleep(1200);
  const c1 = await dispatchBrowserTool('browser_eval', { key: cruOther.key, code: 'document.cookie', _cru: true });
  const c2 = await dispatchBrowserTool('browser_eval', { key: lhTab.key, code: 'document.cookie', _cru: true });
  const hist3 = await page('window.api.wbrowser.drawer.history("", 50)');
  check('"Only this site" (filled in from the tab) clears that site\'s cookies and history, and nothing else', prefilled === '127.0.0.1' && c1.result === '' && /b=2/.test(c2.result || '') && hist3.items.every((h) => !/127\.0\.0\.1/.test(h.url)) && hist3.items.some((h) => /localhost/.test(h.url)), `${prefilled} ${JSON.stringify(c1)} ${JSON.stringify(c2)} ${JSON.stringify(hist3.items.map((h) => h.url))}`);

  // ── browser_mouse: real input, into a cross-site frame, with the chat's cursor ──
  const fr = await proj(() => browser.openDrawerTab({ alias: 'proj', url: `http://127.0.0.1:${T}/framed`, show: true }));
  const fk = fr.key;
  for (let i = 0; i < 20; i++) { const w = await proj(() => browser.evalBrowser(fk, 'innerWidth')); if (w.ok && Number(w.result) >= 520) break; await sleep(200); }
  await sleep(600);
  const vw = await proj(() => browser.evalBrowser(fk, 'innerWidth + "x" + innerHeight'));
  check('setup: a drawer tab whose page has a cross-site iframe', fr.ok && vw.ok, JSON.stringify(fr) + ' ' + JSON.stringify(vw));
  const curOn = () => page('document.querySelector(".brd-cur").classList.contains("on")');
  const m1 = await proj(() => browser.mouseBrowser(fk, 150, 100, 'click'));
  await sleep(250);
  const ic1 = await proj(() => browser.evalBrowser(fk, 'window.innerClicks'));
  check('browser_mouse click lands inside a cross-site iframe', m1.ok && ic1.result === '1', JSON.stringify(m1) + ' ' + JSON.stringify(ic1));
  check('the click answers where the pointer is', m1.x === 150 && m1.y === 100 && m1.held === null, JSON.stringify(m1));
  check('the chat\'s cursor shows in the drawer while it uses the mouse', await curOn() === true);
  const tr = await page('document.querySelector(".brd-cur").style.transform');
  check('...at the point it clicked', /translate\(150px, ?100px\)/.test(tr || ''), tr);
  await sleep(2400);
  check('...and hides by itself once the chat stops', await curOn() === false);
  const t0m = Date.now();
  const m2 = await proj(() => browser.mouseBrowser(fk, 290, 100, 'move'));
  const glided = Date.now() - t0m;
  check('a move glides the cursor there first (the call waits for it)', m2.ok && glided >= 140, `${glided} ms ${JSON.stringify(m2)}`);
  const d1 = await proj(() => dispatchBrowserTool('browser_mouse', { key: fk, action: 'drag', x: 320, y: 30, toX: 480, toY: 200, duration: 300 }));
  const dr1 = JSON.parse((await proj(() => browser.evalBrowser(fk, 'JSON.stringify(window.drag)'))).result || '{}');
  check('drag (through the MCP tool): pressed at the start, moved holding the button, released at the end', d1.ok && String(dr1.down) === '320,30' && String(dr1.up) === '480,200' && dr1.moves >= 5, JSON.stringify(d1) + ' ' + JSON.stringify(dr1));
  const dn = await proj(() => browser.mouseBrowser(fk, 330, 40, 'down'));
  check('down holds the button, and the cursor shows it held', dn.ok && dn.held === 'left' && await page('document.querySelector(".brd-cur").classList.contains("down")'), JSON.stringify(dn));
  await shot('12-mouse-held');
  await sleep(2400);
  check('a held cursor stays on screen', await curOn() === true);
  await proj(() => browser.mouseBrowser(fk, 450, 120, 'move', { duration: 200 }));
  const up1 = await proj(() => browser.mouseBrowser(fk, undefined, undefined, 'up'));
  const dr2 = JSON.parse((await proj(() => browser.evalBrowser(fk, 'JSON.stringify(window.drag)'))).result || '{}');
  check('down, move and up as separate calls make a drag', up1.ok && up1.held === null && String(dr2.down) === '330,40' && String(dr2.up) === '450,120' && dr2.moves >= 2, JSON.stringify(up1) + ' ' + JSON.stringify(dr2));
  await proj(() => browser.mouseBrowser(fk, 400, 220, 'click', { button: 'right' }));
  await proj(() => browser.mouseBrowser(fk, 400, 220, 'double_click'));
  const cd = await proj(() => browser.evalBrowser(fk, 'ctx + "," + dbl'));
  check('right click and double_click', cd.result === '1,1', JSON.stringify(cd));
  const bad1 = await proj(() => browser.mouseBrowser(fk, 1, 1, 'drag'));
  const bad2 = await proj(() => browser.mouseBrowser(fk, 1, 1, 'wiggle'));
  check('a drag without an end, or an unknown action, is refused with a reason', !bad1.ok && /toX and toY/.test(bad1.error) && !bad2.ok && /action must be one of/.test(bad2.error), JSON.stringify([bad1, bad2]));
  const cm = await cruTool('browser_do').run({ key: fk, op: 'mouse', mouse: 'click', x: 150, y: 100 });
  await sleep(250);
  const ic2 = await proj(() => browser.evalBrowser(fk, 'window.innerClicks'));
  check('Cru clicks with the mouse too (browser_do op mouse), into the same iframe', cm.ok !== false && ic2.result === '2', JSON.stringify(cm) + ' ' + JSON.stringify(ic2));
  await proj(() => browser.closeBrowser(fk));

  // ── Saved passwords: save after a sign-in, offer to fill, update, show with the PIN ──
  const lg = await proj(() => browser.openDrawerTab({ alias: 'proj', url: `http://127.0.0.1:${T}/login`, show: true }));
  const lk = lg.key;
  await sleep(800);
  const pwApi = 'window.api.wbrowser.drawer.pw';
  const bar = () => page('(function(){var b=document.querySelector(".brd-pwbar");return b&&!b.hidden?b.textContent:"";})()');
  const waitBar = async (re, ms = 6000) => { for (let i = 0; i < ms / 200; i++) { const t = await bar(); if (re.test(t)) return t; await sleep(200); } return await bar(); };
  const signIn = async (user, pw) => {
    await proj(() => browser.fillBrowser(lk, '#u', user));
    await proj(() => browser.fillBrowser(lk, '#p', pw));
    const r = JSON.parse((await proj(() => browser.evalBrowser(lk, 'JSON.stringify(document.getElementById("go").getBoundingClientRect())'))).result);
    return proj(() => browser.mouseBrowser(lk, Math.round(r.x + r.width / 2), Math.round(r.y + r.height / 2), 'click'));
  };
  await signIn('ali', 'hunter2-secret');
  const pt1 = await waitBar(/Save the password/);
  await shot('14-pw-offer');
  check('after a sign-in that worked, the drawer offers to save the password', /Save the password for 127\.0\.0\.1/.test(pt1), pt1);
  check('...with the username filled in', await page('document.querySelector(".brd-pwuser").value') === 'ali');
  await page('document.querySelector(\'[data-pw="save"]\').click()'); await sleep(600);
  const pl1 = await page(`${pwApi}.list()`);
  check('Save keeps it (on this computer, encrypted)', pl1.ok && pl1.logins.length === 1 && pl1.logins[0].username === 'ali' && pl1.logins[0].origin === `http://127.0.0.1:${T}`, JSON.stringify(pl1));
  const stored = await page(`${pwApi}.list().then(function(){return 1})`);
  const leak = await proj(() => browser.evalBrowser(lk, 'window.lastPw + " | " + document.title'));
  check('a chat reading the page never gets the password back', leak.ok && !/hunter2-secret/.test(leak.result) && /\u2022{8}/.test(leak.result), JSON.stringify(leak));
  check('the lock button shows on a site with a saved login', await page('!document.querySelector(\'[data-brd="pw"]\').hidden') === true && stored === 1);
  await proj(() => browser.navigateBrowser(lk, `http://127.0.0.1:${T}/login?again`));
  const pt2 = await waitBar(/Fill your saved login/);
  check('on the sign-in page again it offers to fill the saved login', /Fill your saved login for 127\.0\.0\.1/.test(pt2) && /ali/.test(pt2), pt2);
  await page('document.querySelector(\'[data-pw="fill"]\').click()'); await sleep(500);
  const filled = await proj(() => browser.evalBrowser(lk, 'document.getElementById("u").value + "|" + document.getElementById("p").value.split("").reverse().join("")'));
  check('Fill puts the username and password in', filled.result === 'ali|' + 'hunter2-secret'.split('').reverse().join(''), JSON.stringify(filled));
  check('...and the prompt goes', !(await bar()));
  // A changed password: asked to update, once the sign-in worked.
  await proj(() => browser.navigateBrowser(lk, `http://127.0.0.1:${T}/login?bad`)); await sleep(500);
  await page('(function(){var x=document.querySelector(\'[data-pw="nofill"]\');if(x)x.click();})()');
  await signIn('ali', 'bad');
  await sleep(9000);
  check('a refused sign-in (the form stays) offers nothing', !/Save|Update/.test(await bar()), await bar());
  await signIn('ali', 'hunter3-new');
  const pt3 = await waitBar(/Update the saved password/);
  check('a new password for a saved username asks to update it', /Update the saved password for ali/.test(pt3), pt3);
  await page('document.querySelector(\'[data-pw="save"]\').click()'); await sleep(600);
  const pid1 = pl1.logins[0].id;
  const prv = await page(`${pwApi}.reveal(${JSON.stringify(pid1)})`);
  check('Update saves the new password over the old', prv.ok && prv.password === 'hunter3-new', JSON.stringify(prv));
  // The list: showing a password asks for the PIN.
  await page('document.querySelector(\'[data-brd="menu"]\').click()'); await sleep(200);
  await page('document.querySelector(\'[data-pan="passwords"]\').click()'); await sleep(500);
  await page('document.querySelector(\'[data-pwact="show"]\').click()'); await sleep(200);
  check('Passwords, Show asks for the PIN first', await page('!document.querySelector(".brd-pinrow").hidden && /\u2022/.test(document.querySelector(".brd-pwval").textContent)') === true);
  await page('document.querySelector(".brd-pin").value="9999"; document.querySelector(\'[data-pwact="pin"]\').click()'); await sleep(500);
  check('...a wrong PIN shows nothing', await page('/\u2022/.test(document.querySelector(".brd-pwval").textContent)') === true);
  await page('document.querySelector(".brd-pin").value="1234"; document.querySelector(\'[data-pwact="pin"]\').click()'); await sleep(600);
  check('...the PIN shows it', await page('document.querySelector(".brd-pwval").textContent') === 'hunter3-new');
  check('...and the PIN box goes away', await page('getComputedStyle(document.querySelector(".brd-pinrow")).display') === 'none');
  await shot('13-passwords');
  await page('document.querySelector(\'[data-pan="close"]\').click()');
  const noChat = await proj(() => dispatchBrowserTool('browser_eval', { key: lk, code: 'typeof window.api' }));
  check('the page in the tab cannot reach the password API', noChat.result === 'undefined', JSON.stringify(noChat));
  await proj(() => browser.closeBrowser(lk));

  // ── Hardening ──
  const evil = await page(`new Promise(function(res){ var w=document.createElement('webview'); w.setAttribute('partition','persist:evil'); w.setAttribute('src','about:blank');
    document.body.appendChild(w); setTimeout(function(){ var ok=false; try{ w.getWebContentsId(); ok=true; }catch(e){} res(ok); }, 2500); })`);
  check('the page cannot create a webview outside the drawer\'s partitions', evil === false);
  const nodeInGuest = await dispatchBrowserTool('browser_eval', { key, code: 'typeof require + "," + typeof process', _cru: true });
  check('drawer pages have no Node', nodeInGuest.result === 'undefined,undefined', JSON.stringify(nodeInGuest));

  // ── No desktop app connected: every caller is told at once ──
  await Promise.race([client('quit', {}), sleep(1500)]).catch(() => {});
  for (let i = 0; i < 50 && browser.hasNativeHost(); i++) await sleep(100);
  const t0g = Date.now();
  const g1 = await proj(() => browser.listAllBrowsers('proj'));
  const g2 = await proj(() => browser.openDrawerTab({ alias: 'proj', url: 'https://example.com' }));
  const g3 = await cruTool('browser_tabs').run({});
  const g4 = await chatCall('browser_open', { url: 'https://example.com', on: 'drawer', network: 'homepool:' + HOME }, chat('chat-c'));
  const g5 = await chatCall('browser_eval', { key, code: '1' }, chat('chat-c'));
  const quick = Date.now() - t0g < 3000;
  check('with no desktop app connected, browser_list says why', /none is connected/.test(g1.drawer || ''), JSON.stringify(g1));
  check('opening a tab says so', !g2.ok && /none is connected/.test(g2.error || ''), JSON.stringify(g2));
  check('Cru is told the same', g3.ok && /none is connected/.test(g3.drawer || ''), JSON.stringify(g3));
  check('a chat asking for homepool is told, not left waiting on an approval', !g4.ok && /none is connected/.test(g4.error || '') && !homepool.store.requests().some((r) => r.chatId === 'chat-c'), JSON.stringify(g4));
  check('a call on a tab is refused with a reason', !g5.ok && !!g5.error, JSON.stringify(g5));
  check('all of it at once, nothing hangs', quick, `${Date.now() - t0g} ms`);
} catch (err) {
  check('no exceptions', false, err.stack);
}
clearTimeout(watchdog);
if (failed) console.log('\nclient log tail:\n' + clientLog.split('\n').filter((l) => /harness/.test(l)).slice(-25).join('\n'));
console.log(failed ? `\n${failed} failed` : '\nall passed');
finish(failed ? 1 : 0);
