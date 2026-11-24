// ─── The remaining parity lines (PARITY-ELECTRON.md), end to end ───
// The native host says who it is and when it is brought to the front;
// browser_touch lists the listeners under the finger; a drawer tab's
// download is saved where the Save dialog says; a notification's click
// raises the window (held on top for 300 ms on Linux, GNOME's workaround);
// the homepool node reports this PC's public IP; the kid lock's watchdog
// puts the window back (un-minimised, full screen, in front) while real
// keys cannot leave it; this OS's power and lock are read; the DisconnectRDP
// status is Windows-only.

import http from 'node:http';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir, hostname } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mcpOf } from '../desktop/drawer.mjs';

const FAKE_IP = '203.0.113.7';
let PAGE = '';

/** Pages for the checks, and the stand-in for api.ipify.org (on loopback). */
function pages() {
  const srv = http.createServer((req, res) => {
    if (req.url.startsWith('/ip')) { res.setHeader('Content-Type', 'text/plain'); return res.end(FAKE_IP + '\n'); }
    if (req.url.startsWith('/file.txt')) {
      res.setHeader('Content-Type', 'application/octet-stream');
      res.setHeader('Content-Disposition', 'attachment; filename="report.txt"');
      return res.end('downloaded by the drawer\n');
    }
    res.setHeader('Content-Type', 'text/html; charset=utf-8');
    if (req.url.startsWith('/touch')) return res.end(`<!doctype html><meta name=viewport content="width=device-width"><title>touch</title>
<body style="margin:0"><div id=pad style="position:absolute;left:40px;top:40px;width:200px;height:120px;background:#9cf">pad</div>
<script>document.getElementById('pad').addEventListener('touchstart', function padTouched(e) { document.title = 'touched'; }, { passive: true });
document.body.addEventListener('pointerdown', function bodyDown() {}, true);</script>`);
    if (req.url.startsWith('/dl')) return res.end(`<!doctype html><title>dl</title><body style="font:16px sans-serif;margin:20px"><a id=dl href="/file.txt">Get the report</a>`);
    res.end('<!doctype html><title>blank</title>');
  });
  return new Promise((r) => srv.listen(0, '127.0.0.1', () => { srv.unref(); r(`http://127.0.0.1:${srv.address().port}`); }));
}

/** Before the app starts: the public IP's stand-in (read once, at the node's start). */
export async function before() {
  PAGE = await pages();
  process.env.CRUNDI_PUBLIC_IP_URL = `${PAGE}/ip`;
}

// As a window manager would answer (none on Xvfb); xprop cannot set two atoms.
const NOTIFYD = fileURLToPath(new URL('../tools/notifyd.py', import.meta.url));
const XWMSTATE = fileURLToPath(new URL('../tools/xwmstate.py', import.meta.url));
const xprop = (id, ...states) => execFileSync('python3', ['-I', XWMSTATE, String(id), ...states]);

export async function run(T, t) {
  const D = T.D;
  const mcp = mcpOf(T);

  // ─── The native host: name, clientKind and version; native-host-active on focus ───
  await T.until('registered as the server\'s native host', (p) => (D(p).host?.features || []).includes('browser-drawer'), 30000);
  const hosts = async () => (await T.api.http('GET', '/api/browsers')).body?.places?.hosts || [];
  let h = (await hosts())[0];
  t.check('the server knows this PC by its name', h?.name === hostname(), h);
  t.check('…as a native client, with its version', h?.kind === 'native' && /^\d+\.\d+\.\d+/.test(h?.version || ''), h);
  const used0 = Date.parse(h?.lastUsed || 0);
  const sent0 = D(await T.state()).host.activeSent;
  await t.sleep(20);
  // The platform's activation (a window manager's focus; none on Xvfb) as the area hears it.
  await T.act('window-activated');
  await T.until('brought to the front: native-host-active sent', (p) => D(p).host.activeSent > sent0, 5000);
  await t.sleep(300);
  h = (await hosts())[0];
  t.check('…the server marks this PC used last (its new pages come here)', Date.parse(h?.lastUsed || 0) > used0, { before: used0, h });

  // ─── browser_touch: the listeners under the finger (CDP DOMDebugger) ───
  let r = await mcp('browser_drawer_open', { url: `${PAGE}/touch` });
  t.check('a drawer tab for the touch page', r.ok && /^drawer:/.test(r.key || ''), r);
  const key = r.key || 'drawer:none';
  await t.sleep(1500);
  r = await mcp('browser_touch', { key, x: 120, y: 90 });
  const ls = r.report?.listeners || [];
  const pad = ls.find((x) => (x.listeners || []).some((l) => l.type === 'touchstart'));
  t.check('browser_touch reports the listeners at the point (listenersAt)', r.ok && Array.isArray(r.report?.listeners) && !!pad, r);
  t.check('…the pad\'s touchstart, passive, with its handler', !!pad && pad.listeners.some((l) => l.type === 'touchstart' && l.passive === true && /padTouched/.test(l.handler)), pad);
  t.check('…and the body\'s pointerdown up the chain, in capture', ls.some((x) => (x.listeners || []).some((l) => l.type === 'pointerdown' && l.capture === true)), ls);
  r = await mcp('browser_eval', { key, code: 'document.title' });
  t.check('…and the tap reached the page', r.result === 'touched', r);

  // ─── A drawer tab's download: one Save dialog, the file where it says ───
  const dir = mkdtempSync(join(tmpdir(), 'crundi-v2-dl-'));
  const dest = join(dir, 'report.txt');
  r = await mcp('browser_drawer_open', { url: `${PAGE}/dl` });
  const dkey = r.key || 'drawer:none';
  await t.sleep(1200);
  await T.ui('workspace:pick-next', dest);
  r = await mcp('browser_click', { key: dkey, selector: '#dl' });
  const dl = await T.until('a download link in a drawer tab: saved where the dialog said', (p) => (D(p).downloads || []).some((x) => x.state === 'done'), 20000);
  const row = (D(dl || {}).downloads || []).find((x) => x.state === 'done') || {};
  t.check('…named as the page named it (the dialog\'s default)', row.name === 'report.txt', row);
  t.check('…the file\'s bytes', existsSync(dest) && readFileSync(dest, 'utf8') === 'downloaded by the drawer\n', row);
  rmSync(dir, { recursive: true, force: true });

  // ─── The drawer's DevTools and Clear browsing data for one site, with real clicks ───
  const at = async (words) => { const b = await T.ui('find', words); return b.ok ? { x: b.x + b.w / 2, y: b.y + b.h / 2 } : null; };
  const click = async (words) => { const c = await at(words); t.check(`(${words} on screen)`, !!c, words); if (c) await T.mouse('click', c.x, c.y); };
  const LOCAL = PAGE.replace('127.0.0.1', 'localhost');
  r = await mcp('browser_drawer_open', { url: `${LOCAL}/blank?b` });
  const bkey = r.key || 'drawer:none';
  await t.sleep(1200);
  r = await mcp('browser_eval', { key: bkey, code: 'document.cookie = "k=b; max-age=3600"; document.cookie' });
  r = await mcp('browser_drawer_open', { url: `${PAGE}/blank?a` });
  const akey = r.key || 'drawer:none';
  await t.sleep(1200);
  r = await mcp('browser_eval', { key: akey, code: 'document.cookie = "k=a; max-age=3600"; document.cookie' });
  t.check('a cookie on each site (127.0.0.1 and localhost)', r.result === 'k=a' && (await mcp('browser_eval', { key: bkey, code: 'document.cookie' })).result === 'k=b', r);
  const DR = (p) => D(p).drawer || {};
  if (!DR(await T.state()).open) await T.act('browser');
  {
    const ta = (DR(await T.state()).tabs || []).find((x) => x.url.includes('127.0.0.1') && x.url.includes('blank?a'));
    if (ta) await T.act('activate', ta.id);
  }
  await T.until('the drawer shown, on the 127.0.0.1 tab', (p) => DR(p).open && (DR(p).tabs || []).find((x) => x.id === DR(p).active)?.url?.includes('127.0.0.1'), 8000);
  await t.sleep(600);
  // The DevTools button, clicked: the host is asked for the page's DevTools (Cmd::DevTools).
  // (Xvfb opens no DevTools window: the window itself is the owner's check on a real desktop.)
  {
    const n0 = DR(await T.state()).devtoolsSent;
    await click('DevTools');
    await T.until('a click on DevTools asks the browser host for the page\'s DevTools (Cmd::DevTools)', (p) => DR(p).devtoolsSent === n0 + 1, 3000);
  }
  await T.act('panel', 'clear');
  await T.until('the Clear browsing data panel, for this tab\'s site', (p) => DR(p).panel === 'clear', 5000);
  await t.sleep(400);
  await click('Only this site');
  await click('Cookies and site data');
  await t.sleep(300);
  await click('Clear');
  await T.until('Clear: "Cleared for 127.0.0.1: history, cookies and site data, cached files"', (p) => /Cleared for 127\.0\.0\.1: history, cookies and site data, cached files/.test(p.toast || ''), 8000);
  await t.sleep(1500);
  r = await mcp('browser_eval', { key: akey, code: 'document.cookie' });
  t.check('…127.0.0.1\'s cookie gone', r.ok !== false && r.result === '', r);
  r = await mcp('browser_eval', { key: bkey, code: 'document.cookie' });
  t.check('…localhost\'s kept', r.result === 'k=b', r);
  {
    let hist = [];
    for (let i = 0; i < 20; i++) { try { hist = JSON.parse(readFileSync(join(T.configDir, 'drawer-history.json'), 'utf8')); } catch { hist = []; } if (!hist.some((h) => h.url.includes('127.0.0.1'))) break; await t.sleep(300); }
    t.check('…its history gone, localhost\'s kept (drawer-history.json)', !hist.some((h) => h.url.includes('127.0.0.1')) && hist.some((h) => h.url.includes('localhost')), hist.map((h) => h.url));
  }
  if (DR(await T.state()).open) await T.act('browser');

  // ─── A notification's click: raised, held on top for 300 ms (GNOME), its chat ───
  const raises0 = D(await T.state()).win.raises;
  await T.act('notify-click', '');
  await T.until('a notification\'s click raises the window, held above', (p) => D(p).win.raises > raises0 && D(p).win.raisedAbove === true, 3000);
  await T.until('…and lets go of "above" after 300 ms', (p) => D(p).win.raisedAbove === false, 3000);

  // ─── FINDINGS T9: a server notice through the session bus's notification service ───
  {
    const d = spawn('python3', ['-I', NOTIFYD], { stdio: ['pipe', 'pipe', 'inherit'] });
    const lines = [];
    let buf = '';
    d.stdout.on('data', (b) => { buf += b; let i; while ((i = buf.indexOf('\n')) >= 0) { try { lines.push(JSON.parse(buf.slice(0, i))); } catch { /* not ours */ } buf = buf.slice(i + 1); } });
    for (let i = 0; i < 50 && !lines.some((l) => l.ready); i++) await t.sleep(100);
    t.check('T9: a notification service owns org.freedesktop.Notifications', lines.some((l) => l.ready));
    await T.ui('tab', 'kanban');
    await T.until('T9: away from the chats (Kanban)', (p) => p.tab === 'kanban');
    const raises1 = D(await T.state()).win.raises;
    await T.act('notify-show', 'Done -1', "--help 'quoted' {}", '/chat/t9chat');
    for (let i = 0; i < 50 && !lines.some((l) => l.id); i++) await t.sleep(100);
    const n = lines.find((l) => l.id) || {};
    t.check('T9: Notify called with the spec\'s arguments (app, 0, icon, summary, body, the default action, desktop-entry, -1)',
      n.app === 'Crundi Client' && n.replaces === 0 && n.icon === 'crundi-client' && n.summary === 'Done -1' && n.body === "--help 'quoted' {}"
      && JSON.stringify(n.actions) === '["default","Open"]' && n.hints?.['desktop-entry'] === 'crundi-client' && n.expire === -1, n);
    d.stdin.write(`click ${n.id}\n`);
    await T.until('T9: its click (ActionInvoked) raises the window and opens the chats', (p) => D(p).win.raises > raises1 && p.tab === 'workbench', 8000);
    d.stdin.end();
    d.kill();
  }

  // ─── The homepool node: this PC's public IP (api.ipify.org every 10 min) ───
  const s0 = await T.act('homepool-status');
  const made = await T.api.http('POST', '/api/homepool', { action: 'self-node', deviceId: s0.status.deviceId, name: 'This PC' });
  const node = made.body?.node || {};
  const nodeKey = node.key || (node.code && /key=([^&]+)/.exec(node.code)?.[1]) || '';
  t.check('a node key for this PC', /^hpn_/.test(nodeKey), made.body);
  r = await T.act('homepool-share', T.SERVER, nodeKey);
  t.check('…shared', r.ok, r);
  const st = await T.until('the node connects and knows its public IP', (p) => D(p).homepool.state === 'connected' && D(p).homepool.publicIp === FAKE_IP, 20000);
  t.check('…logged as Electron logs it', (D(st || {}).homepool?.log || []).some((l) => l.includes(`public IP ${FAKE_IP}`)), D(st || {}).homepool?.log);
  {
    let seen = '';
    for (let i = 0; i < 30 && seen !== FAKE_IP; i++) {
      const o = await T.api.http('GET', '/api/homepool');
      seen = (JSON.stringify(o.body).match(/"publicIp":"([^"]*)"/g) || []).map((x) => x.split('"')[3]).find((x) => x) || '';
      if (seen !== FAKE_IP) await t.sleep(500);
    }
    t.check('…and the server has it (the node\'s info)', seen === FAKE_IP, seen);
  }
  await T.act('homepool-forget');
  await T.until('…forgotten: off again', (p) => D(p).homepool.state === 'off', 5000);

  // ─── The kid lock's watchdog (700 ms): back in front, full screen, un-minimised ───
  await T.api.http('POST', '/api/voice/kid/pin', { pin: '1357' });
  await T.act('kid-start', 'tray');
  const k = await T.until('kid mode locks the window', (p) => D(p).kid.on && D(p).windowLocked, 15000);
  const xid = D(k || {}).win?.x11;
  t.check('the X11 window\'s id', xid > 0, D(k || {}).win);
  // The window manager's answer, as one would set it (no WM on Xvfb): full screen, focused.
  xprop(xid, '_NET_WM_STATE_FULLSCREEN', '_NET_WM_STATE_FOCUSED');
  await T.until('…full screen and in front, as the window manager says', (p) => D(p).win.wmState?.fullscreen === true && D(p).win.wmState?.focused === true, 5000);
  await t.sleep(800);
  const w0 = D(await T.state()).win.watch;
  await t.sleep(1600);
  const w1 = D(await T.state()).win.watch;
  t.check('the watchdog looks every 700 ms', w1.ticks - w0.ticks >= 2, { w0, w1 });
  t.check('…and leaves a window that is as it should be alone', w1.fullscreen === w0.fullscreen && w1.front === w0.front && w1.restored === w0.restored, { w0, w1 });
  // Real keys a child would press: Escape, Alt+Tab. Neither leaves.
  await T.key('Escape');
  await T.key('Tab', ['alt']);
  await t.sleep(400);
  t.check('Escape and Alt+Tab do not leave kid mode', D(await T.state()).kid.on);
  // Minimised, out of full screen and not in front (what got past the keys).
  xprop(xid, '_NET_WM_STATE_HIDDEN');
  const w2 = await T.until('the watchdog puts it right within a look or two', (p) => {
    const w = D(p).win.watch;
    return w.restored > w1.restored && w.fullscreen > w1.fullscreen && w.front > w1.front;
  }, 3000);
  t.check('…each of the three put right', !!w2, w2 && D(w2).win.watch);
  // The window manager would answer so: shown, full screen, focused again.
  xprop(xid, '_NET_WM_STATE_FULLSCREEN', '_NET_WM_STATE_FOCUSED');
  await T.until('…and stays so', (p) => D(p).win.wmState?.fullscreen === true && D(p).win.wmState?.hidden === false, 5000);
  await T.act('kid-pin');
  await T.until('the PIN pad', (p) => D(p).kid.pin);
  for (const d of ['1', '3', '5', '7', 'OK']) await T.act('kid-key', d);
  await T.until('the PIN leaves kid mode', (p) => !D(p).kid.on && !D(p).windowLocked, 10000);
  xprop(xid, '_NET_WM_STATE_FOCUSED');
  {
    const a = D(await T.state()).win.watch;
    await t.sleep(1500);
    const b = D(await T.state()).win.watch;
    t.check('…and the watchdog stops with it', a.ticks === b.ticks, { a, b });
  }

  // ─── Power and the screen's lock as this OS reads them; RDP (Windows only) ───
  r = await T.act('power');
  t.check('power and lock are read (booleans)', r.ok && typeof r.onBattery === 'boolean' && typeof r.locked === 'boolean', r);
  r = await T.act('rdp-status');
  if (process.platform === 'win32') t.check('rdp:status on Windows', r.ok && r.supported === true && typeof r.installed === 'boolean', r);
  else t.check('rdp:status elsewhere: nothing to set up', r.ok && r.supported === false && r.installed === false && r.isRdp === false, r);
  r = await T.act('rdp-setup');
  if (process.platform !== 'win32') t.check('rdp:setup elsewhere: Windows only', r.ok === false && r.error === 'Windows only', r);
}
