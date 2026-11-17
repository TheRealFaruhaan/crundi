#!/usr/bin/env node
// The desktop area end to end (AREAS.md): the browser drawer (tabs per
// project, the address bar, devices, networks, annotations sent to a chat,
// history and clearing), the native host's browser protocol on drawer tabs
// and hidden browsers, the Browsers tab, the Inbox, notifications.
//
//   native-gpui/tools/test-server.sh 4396 --bare
//   native-gpui/tools/build.sh; BIN=$HOME/.cache/crundi-native-target/debug/crundi-gpui
//   NATIVE_BIN=$BIN xvfb-run -a -s "-screen 0 1440x900x24" node native-gpui/test/desktop.mjs
//
// (CRUNDI_TEST_SERVER=http://localhost:<port> CRUNDI_TEST_PORT=<port> for
// other ports.) ONLY=drawer,host,inbox,cru,cruscreen,kid,wake,passwords,bubble,tray,node,unload,hostproc runs some parts.
// (unload: the one rule of every web view, not visible: hidden at once, freed
// after the unload grace, loaded again when visible; then the host's idle stop.)
// (hostproc: the browser host process: lazy start, relaunch, crash, idle
// stop, stale host, the app killed; it ends the run, killing the app.)

import http from 'node:http';
import { readFileSync, writeFileSync, existsSync, rmSync, readdirSync, symlinkSync, lstatSync } from 'node:fs';
import { spawn } from 'node:child_process';
import * as t from './lib.mjs';

const ONLY = (process.env.ONLY || '').split(',').filter(Boolean);
const part = (p) => !ONLY.length || ONLY.includes(p);

// A page to browse: a button that counts clicks, a link that opens a window,
// a login form; its title says which path it is.
const page = http.createServer((req, res) => {
  res.setHeader('Content-Type', 'text/html; charset=utf-8');
  // A sign-in form (saved passwords): it posts to /welcome, which has none.
  if (req.url.startsWith('/login')) return res.end(`<!doctype html><title>login</title><body style="font:16px sans-serif;margin:20px">
<form action="/welcome" method="post"><input id=u name=u type=text placeholder=Email><input id=p name=p type=password placeholder=Password><button id=go type=submit>Sign in</button></form>`);
  if (req.url.startsWith('/welcome')) return res.end('<!doctype html><title>welcome</title><body style="font:16px sans-serif;margin:20px">Welcome back.');
  // A page that never stops drawing (a counter on every animation frame): a
  // view that is shown sends frames all the time, a hidden one none.
  if (req.url.startsWith('/anim')) return res.end(`<!doctype html><title>anim</title><body style="font:40px sans-serif;margin:20px"><div id=n>0</div>
<script>let n=0;(function f(){document.getElementById('n').textContent=++n;requestAnimationFrame(f)})()</script>`);
  res.end(`<!doctype html><meta name=viewport content="width=device-width"><title>page ${req.url}</title>
<body style="font:16px sans-serif;margin:20px">
<button id=b style="position:absolute;left:40px;top:80px;width:160px;height:50px" onclick="this.textContent='clicked '+(++window.n)">Click me</button>
<a id=pop href="/popped" target="_blank" style="position:absolute;left:40px;top:160px">open a window</a>
<p style="margin-top:220px">viewport <span id=vp></span></p>
<script>window.n=0;console.log('hello from '+location.pathname);function vp(){document.getElementById('vp').textContent=innerWidth+'x'+innerHeight+' dpr '+devicePixelRatio+' touch '+('ontouchstart' in window)}vp();addEventListener('resize',vp)</script>`);
});
await new Promise((r) => page.listen(0, '127.0.0.1', r));
const PAGE = `http://127.0.0.1:${page.address().port}`;

// Saved passwords under the test driver: no desktop keyring here, so the
// app takes its key from CRUNDI_PW_TEST_KEY (honoured only with the driver on).
process.env.CRUNDI_PW_TEST_KEY ||= Array.from({ length: 32 }, () => Math.floor(Math.random() * 256).toString(16).padStart(2, '0')).join('');
// "Hey Cru": the models already on this machine when there (else the app downloads them).
if (existsSync('/tmp/vw-install/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01')) process.env.CRUNDI_WAKE_MODELS ||= '/tmp/vw-install';
const T = await t.begin('desktop');
const D = (p) => p.areas.desktop;
const dr = (p) => D(p).drawer;
const panelOf = (p, id) => dr(p).tabs.find((x) => x.id === id)?.panel;
/** Run code in a drawer tab's page: its answer ({ok, result} or {ok:false, error}). */
async function evalTab(tab, code) {
  const r = await T.act('eval', tab, code);
  if (!r.ok) return r;
  for (let i = 0; i < 60; i++) {
    const v = D(await T.props()).evals[r.token];
    if (v) return v;
    await t.sleep(250);
  }
  return { ok: false, error: 'no answer' };
}

if (part('drawer')) {
  // ─── The drawer: open, a first tab, the address bar ───
  await T.act('browser');
  let p = await T.until('the Browser button opens the drawer with a first tab', (p) => p.drawerOpen && dr(p).open && dr(p).tabs.length === 1 && dr(p).tabs[0].network === 'client');
  const first = dr(p).tabs[0].id;
  await T.act('go', `${PAGE.replace('http://', '')}/a`);
  p = await T.until('the address field opens a host typed without a scheme', (p) => dr(p).tabs[0].url === `${PAGE}/a` && /page \/a/.test(dr(p).tabs[0].title));
  // (The page's next frame can come a moment after its title: let it paint
  // before the picture, which the GPUI client takes from the X server.)
  await t.sleep(800);
  await T.snap('drawer-tab');

  // Tabs: a second, switching, the history.
  const r2 = await T.act('new-tab', `${PAGE}/b`);
  t.check('a second tab', r2.ok, r2);
  p = await T.until('the second tab is active and loads', (p) => dr(p).active === r2.id && /page \/b/.test(dr(p).tabs[1].title));
  t.check('each tab has its own page', panelOf(p, first) !== panelOf(p, r2.id), dr(p).tabs);
  await T.act('activate', first);
  await T.until('a click on a tab shows it', (p) => dr(p).active === first && D(p).drawerActive === panelOf(p, first));
  await T.until('the history keeps the visits', (p) => dr(p).history >= 2);
  await T.act('url-typed', 'page');
  await T.act('panel', 'history');
  await T.until('the History panel lists them', (p) => dr(p).panel === 'history');
  await T.snap('drawer-history');
  await T.act('panel-close');

  // A link that opens a window: a new tab in the project.
  const before = (await T.props()).areas.desktop.drawer.tabs.length;
  await T.act('click', first, '60', '170');
  await T.until('a window a page opens becomes a new tab', (p) => dr(p).tabs.length === before + 1 && dr(p).tabs.some((x) => x.url.endsWith('/popped')));

  // Devices: the popover, a phone.
  await T.act('activate', first);
  await T.act('pop', 'dev');
  await T.until('the device popover', (p) => dr(p).pop === 'dev');
  await T.snap('drawer-devices');
  await T.act('pick', '3');
  p = await T.until('iPhone 15 Pro on the tab', (p) => dr(p).tabs.find((x) => x.id === first)?.device === 3);
  await t.sleep(2500);
  const vp = await evalTab(first, "document.getElementById('vp').textContent");
  t.check('the page is a phone: 393x852 at 3x, touch', /^393x852 dpr 3 touch true/.test(vp.result || ''), vp);
  await T.snap('drawer-phone');
  await T.act('pop', 'dev');
  await T.act('pick', '0');
  await T.until('back to Responsive', (p) => dr(p).tabs.find((x) => x.id === first)?.device === 0);

  // Networks: the popover's labels; the server's network for a tab.
  await T.act('pop', 'net');
  p = await T.until('the network popover', (p) => dr(p).pop === 'net');
  await T.snap('drawer-network');
  await T.act('pick', 'server');
  p = await T.until('the tab moves to the server\'s network (a page of its own)', (p) => dr(p).tabs.find((x) => x.id === first)?.network === 'server');
  await T.until('…and loads there again', (p) => /page \/a/.test(dr(p).tabs.find((x) => x.id === first)?.title || ''));
  await T.act('pop', 'net');
  await T.act('pick', 'client');
  await T.until('back on this PC', (p) => dr(p).tabs.find((x) => x.id === first)?.network === 'client');
}

// The server's tools, as a chat calls them (/api/mcp/call with the internal key).
const KEY = readFileSync(`/tmp/crundi-devtest-${new URL(T.SERVER).port}/data/.api-key`, 'utf8').trim();
// A tool that answers with MCP content (text + an image) is read back as one
// object: the text's JSON, with the image's data as `image`.
const unwrap = (r) => {
  if (!r || !Array.isArray(r.content)) return r;
  const txt = r.content.find((c) => c.type === 'text');
  let o = {};
  try { o = JSON.parse(txt?.text || '{}'); } catch { o = { text: txt?.text }; }
  const img = r.content.find((c) => c.type === 'image');
  if (img && !o.image) o.image = img.data;
  return { ok: r.ok, ...o };
};
const mcp = (tool, args = {}) => fetch(T.SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY }, body: JSON.stringify({ tool, args: { alias: 'demo', ...args } }) }).then((r) => r.json()).then(unwrap).catch((e) => ({ ok: false, error: String(e) }));

if (part('host')) {
  // ─── The native host: the server's browser tools reach this client ───
  await T.until('registered as the server\'s native host', (p) => Array.isArray(D(p).host.features) && D(p).host.features.includes('browser-drawer'), 30000);
  let r = await mcp('browser_list');
  t.check('browser_list: the drawer is available', r.ok && r.drawer === 'available', r);
  r = await mcp('browser_drawer_open', { url: `${PAGE}/host` });
  t.check('browser_drawer_open opens a drawer tab', r.ok && /^drawer:/.test(r.key || ''), r);
  const key = r.key;
  const tab = key.slice('drawer:'.length);
  await T.until('…a tab in the project\'s drawer', (p) => dr(p).tabs.some((x) => x.id === tab));
  r = await mcp('browser_eval', { key, code: 'document.title' });
  t.check('browser_eval on the drawer tab', r.ok && r.result === 'page /host', r);
  r = await mcp('browser_read_page', { key });
  t.check('browser_read_page: text, title, console', r.ok && /Click me/.test(r.text || '') && r.title === 'page /host' && (r.consoleLogs || []).some((l) => /hello from \/host/.test(l.text)), r);
  r = await mcp('browser_click', { key, selector: '#b' });
  r = await mcp('browser_eval', { key, code: 'document.getElementById("b").textContent' });
  t.check('browser_click by selector', r.result === 'clicked 1', r);
  r = await mcp('browser_mouse', { key, x: 120, y: 105, action: 'click' });
  t.check('browser_mouse click answers where the pointer is', r.ok && r.x === 120 && r.y === 105, r);
  r = await mcp('browser_eval', { key, code: 'document.getElementById("b").textContent' });
  t.check('…a real click on the page', r.result === 'clicked 2', r);
  await T.until('…with the chat\'s cursor drawn', (p) => true);
  r = await mcp('browser_fill', { key, selector: '#b', value: 'x' });
  t.check('browser_fill answers', r.ok !== undefined, r);
  r = await mcp('browser_view_source', { key });
  t.check('browser_view_source', r.ok && /<button id="b"/.test(r.html || ''), r);
  r = await mcp('browser_elements', { key });
  t.check('browser_elements lists the button and the link', r.ok && (r.elements || []).some((e) => e.tag === 'button') && (r.elements || []).some((e) => e.tag === 'a'), r);
  r = await mcp('browser_snapshot', { key });
  t.check('browser_snapshot', r.ok && /button/.test(JSON.stringify(r)), r);
  r = await mcp('browser_console', { key });
  t.check('browser_console', r.ok && (r.logs || []).some((l) => /hello/.test(l.text)), r);
  r = await mcp('browser_wait', { key, selector: '#b', timeout: 2000 });
  t.check('browser_wait finds what is there', r.ok, r);
  r = await mcp('browser_wait', { key, selector: '#nothing', timeout: 500 });
  t.check('browser_wait times out on what is not', !r.ok && /Timeout waiting for #nothing/.test(r.error || ''), r);
  r = await mcp('browser_screenshot', { key });
  t.check('browser_screenshot', r.ok && (r.data || '').length > 1000, r.error);
  r = await mcp('browser_point', { key, selector: '#b', label: 'This one' });
  t.check('browser_point drops a pin on the element', r.ok, r);
  await T.act('activate', tab);
  await T.until('the pin shows on the tab', (p) => D(p).drawerActive === panelOf(p, tab));
  await T.snap('drawer-claude-pin');
  r = await mcp('browser_annotations', { key, image: true });
  t.check('browser_annotations reads the marks back, with the picture', r.ok && (r.annotations || []).some((a) => a.by === 'claude' && a.kind === 'pin' && a.text === 'This one') && (r.image || '').length > 1000, { ...r, image: (r.image || '').length });
  r = await mcp('browser_resize', { key, width: 800, height: 600 });
  await t.sleep(1000);
  r = await mcp('browser_eval', { key, code: 'innerWidth+"x"+innerHeight' });
  t.check('browser_resize: the tab is 800x600 (a custom size)', r.result === '800x600', r);
  r = await mcp('browser_navigate', { key, url: `${PAGE}/next` });
  t.check('browser_navigate answers once loaded', r.ok, r);
  r = await mcp('browser_eval', { key, code: 'location.pathname' });
  t.check('…on the new page', r.result === '/next', r);
  r = await mcp('browser_go_back', { key });
  r = await mcp('browser_eval', { key, code: 'location.pathname' });
  t.check('browser_go_back', r.result === '/host', r);
  r = await mcp('browser_tab_network', { key, network: 'server' });
  t.check('browser_tab_network moves it to the server\'s network', r.ok && r.network === 'server', r);
  r = await mcp('browser_eval', { key, code: 'location.pathname' });
  t.check('…and it still answers there', r.ok && r.result === '/host', r);
  r = await mcp('browser_eval', { key: 'drawer:nosuchtab', code: '1' });
  t.check('an unknown drawer tab is refused', !r.ok && /No such drawer tab/.test(r.error || ''), r);

  // A hidden browser (on: "desktop": this client runs it, out of sight).
  r = await mcp('browser_open', { name: 'hid', url: `${PAGE}/hidden`, on: 'desktop', width: 1000, height: 700 });
  t.check('browser_open on the desktop app', r.ok, r);
  const hk = r.key || 'demo:hid';
  await T.until('a hidden browser, not a drawer tab', (p) => D(p).hidden.some((h) => h.key === hk));
  r = await mcp('browser_eval', { key: hk, code: 'document.title + " " + innerWidth' });
  t.check('it runs the page at its size', r.ok && r.result === 'page /hidden 1000', r);
  r = await mcp('browser_network', { key: hk, action: 'start' });
  r = await mcp('browser_navigate', { key: hk, url: `${PAGE}/net` });
  r = await mcp('browser_network', { key: hk, action: 'log' });
  t.check('browser_network records the requests', r.ok && (r.requests || []).some((q) => /\/net$/.test(q.url) && q.statusCode === 200), r);
  r = await mcp('browser_cookies', { key: hk, action: 'set', cookie: { name: 'c1', value: 'v1', url: `${PAGE}/` } });
  r = await mcp('browser_cookies', { key: hk, action: 'get' });
  t.check('browser_cookies set and get', r.ok && (r.cookies || []).some((c) => c.name === 'c1' && c.value === 'v1'), r);
  r = await mcp('browser_pdf', { key: hk });
  t.check('browser_pdf', r.ok && (r.data || '').length > 500, r.error);
  // The Browsers tab lists it.
  await T.ui('tab', 'browsers');
  await T.until('the Browsers tab lists the hidden browser', (p) => D(p).browsers.list.some((b) => b.key === hk));
  await T.act('browsers-shot', hk);
  await T.until('…and takes its screenshot', (p) => D(p).browsers.shots[hk] === 'ok');
  await T.snap('browsers-tab');
  await T.act('browsers-close', hk);
  await T.until('Close closes it', (p) => !D(p).hidden.some((h) => h.key === hk), 10000);
  await T.ui('tab', 'workbench');
}

if (part('inbox')) {
  // ─── The Inbox and notifications ───
  if ((await T.props()).areas.desktop.drawer.open) {
    await T.act('browser');
    await T.until('the Browser button closes the drawer again', (p) => !dr(p).open);
  }
  await T.until('the inbox loads', (p) => D(p).inbox.state === 'ok');
  let r = await mcp('send_message_to_user', { message: 'The build finished: 12 tests passed.' });
  t.check('a message to the owner', r.ok !== false, r);
  if (process.env.DEBUG) console.log('send_message_to_user:', JSON.stringify(r));
  await T.until('the sidebar counts it', (p) => D(p).inbox.unread >= 1);
  await T.until('…and this PC shows a notification', (p) => D(p).notifications.some((n) => /build finished/.test(n.body)), 10000);
  await T.act('inbox');
  let p = await T.until('the sidebar\'s Inbox opens the Inbox page with its thread', (p) => p.currentTab === 'inbox' && D(p).inbox.threads.length >= 1);
  await T.snap('inbox-list');
  const th = D(p).inbox.threads[0].key;
  await T.act('inbox-open', th);
  await T.until('opening a thread shows its messages and reads them', (p) => D(p).inbox.open === th && D(p).inbox.items.length >= 1 && D(p).inbox.unread === 0);
  await T.snap('inbox-thread');
  r = await mcp('send_message_to_user', { message: 'A second one.' });
  await T.until('a new message lands in the open thread live', (p) => D(p).inbox.items.length >= 2);
  const id = D(await T.props()).inbox.items[0].id;
  await T.act('inbox-delete', id);
  await T.until('a message\'s × deletes it', (p) => !D(p).inbox.items.some((x) => x.id === id));
  await T.act('inbox-delete-thread');
  await T.until('Delete thread asks first', (p) => /Delete this whole conversation/.test(p.dialog?.title || ''));
  await T.dialog(true);
  await T.until('…and deletes it', (p) => !D(p).inbox.threads.some((x) => x.key === th));
  await T.snap('inbox-empty');
  await T.ui('tab', 'workbench');
}

if (part('cru')) {
  // ─── Cru: the PC client's screen (app/vendor/crundi-voice.js), part by part ───
  const C = (p) => D(p).cru;
  const U = (p) => D(p).cru.ui || {};
  // Projects for the field's cells.
  for (const a of ['shop', 'blog']) await T.api.http('POST', '/api/projects', { alias: a, name: a, path: `${T.projDir}-${a}`, create: true });
  await t.sleep(1500);
  // A panel that held the keys closes (the drawer, its address field focused): the window's
  // shortcuts still work (the focus goes back to the window's root, not to nothing).
  if (!dr(await T.props()).open) await T.act('browser');
  await T.until('the drawer is open', (p) => dr(p).open && dr(p).tabs.length > 0);
  const fr = await T.act('drawer-focus');
  t.check('…its address field has the keys', fr.ok, fr);
  await T.act('drawer', 'false');
  await T.until('…the drawer closes', (p) => !dr(p).open);
  await t.sleep(500);
  await T.send({ cmd: 'key', text: 'Space', mods: ['ctrl'], phase: 'down' });
  await T.until('after it, Ctrl+Space still opens Cru\'s push to talk', (p) => D(p).cru.open && D(p).cru.ptt);
  await T.send({ cmd: 'key', text: 'Space', mods: ['ctrl'], phase: 'up' });
  await T.until('…let go', (p) => !D(p).cru.ptt);
  await T.key('Escape');
  await T.until('…and Esc closes it', (p) => !D(p).cru.open);
  await T.act('cru');
  let p = await T.until('Cru\'s screen opens from the top bar\'s mark', (p) => C(p).open && U(p).w > 0 && C(p).unavailable !== undefined);
  p = await T.until('…its settings read: a server with no Claude key says so (Unavailable, in red)', (p) => /no Claude API key/.test(C(p).unavailable) && U(p).word === 'Unavailable' && U(p).tone === 'problem');
  t.check('…400 by 600', U(p).w === 400 && U(p).h === 600, U(p));
  const an = U(p).anchor;
  t.check('…under the mark, at its right', an && Math.abs(U(p).top - (an.bottom + 8)) < 1 && Math.abs((U(p).viewportW - U(p).right) - (an.right + 8)) < 1, U(p));
  t.check('the header: the wordmark, ⋯ and close', U(p).wordmark && U(p).more && U(p).close, U(p));
  p = await T.until('the usage line: Claude and Deepgram spent of their budgets', (p) => C(p).legend.some((x) => /^Claude \$\d+\.\d\d \/ \$\d/.test(x)) && C(p).legend.some((x) => /^Deepgram \$\d+\.\d\d \/ \$\d/.test(x)));
  t.check('the chat view is the default, empty: "No messages yet."', U(p).view === 'chat' && C(p).chatView && U(p).empty && U(p).bubbles === 0, U(p));
  t.check('the hint', U(p).hint === 'Hold Ctrl+Space to talk · click the mic to keep this open', U(p).hint);
  t.check('the liquid mic in the chat view (64)', U(p).mic && U(p).micBox === 64 && !U(p).typingBox, U(p));
  t.check('it moves at the display rate while open', U(p).animating, U(p));
  await t.sleep(600);
  await T.snap('cru-chat-empty');

  // The ⋯ menu.
  await T.act('cru-menu-open', 'true');
  p = await T.until('the ⋯ menu: Voice view, Mute Cru\'s voice, Compact the conversation', (p) => JSON.stringify(U(p).menuItems) === JSON.stringify(['Voice view', 'Mute Cru\'s voice', 'Compact the conversation']));
  await T.snap('cru-menu');
  await T.act('cru-menu-open', 'false');
  await T.act('cru-menu', 'compact');
  await T.until('Compact asks first', (p) => C(p).confirm);
  await T.act('cru-menu', 'compact-no');
  await T.until('…Cancel', (p) => !C(p).confirm);
  await T.act('cru-menu', 'voice-off');
  await T.until('Mute Cru\'s voice', (p) => C(p).voiceOff);
  await T.act('cru-menu', 'voice-off');
  await T.until('…and back', (p) => !C(p).voiceOff);

  // The conversation as bubbles (Cru's history on the server).
  {
    const { DatabaseSync } = await import('node:sqlite');
    const db = new DatabaseSync(`/tmp/crundi-devtest-${new URL(T.SERVER).port}/data/voice-memory/index.db`);
    const ins = db.prepare("INSERT INTO entries (kind, at, ref, text) VALUES ('turn', ?, '', ?)");
    const now = Date.now();
    [['What is running in Demo?', 'Demo has one chat open and nothing else running. The last run listed an empty folder.'],
      ['Any services down?', 'No, Sir. No services are registered yet, so nothing is down.'],
      ['Remind me what changed today.', 'Three new projects were added: shop, blog and api-server. Nothing has run in them yet.']]
      .forEach(([s, c], i) => ins.run(now - (3 - i) * 60000, `Owner: ${s}\nCru: ${c}`));
    db.close();
  }
  await T.act('cru-history');
  p = await T.until('the chat view shows the conversation as bubbles', (p) => U(p).bubbles === 6 && !U(p).empty);
  await t.sleep(600);
  await T.snap('cru-chat');

  // The voice view: the field with the projects as cells, the server's message in amber.
  await T.act('cru-menu', 'chat-view');
  p = await T.until('the voice view: the field (the head, the contours, the cells)', (p) => U(p).view === 'voice' && U(p).field && U(p).micBox === 112);
  t.check('…the server\'s message in amber under it', /no Claude API key/.test(U(p).note) && U(p).noteColor === 'amber', U(p));
  await t.sleep(800);
  await T.snap('cru-voice');
  await T.snap('cru-amber');

  // The mic's states (MIC_COLORS) and the words at the right.
  const words = { listen: '', think: '', speak: '', waking: 'Waking', ended: 'Asleep', offline: 'Offline', problem: 'Unavailable' };
  const tones = { listen: 'listen', think: 'think', speak: 'speak', waking: 'start', ended: 'ended', offline: 'offline', problem: 'problem' };
  for (const [ph, wd] of Object.entries(words)) {
    await T.act('cru-state', ph);
    await T.until(`the mic ${ph}: ${tones[ph]}${wd ? `, "${wd}"` : ', no word'}`, (p) => U(p).tone === tones[ph] && U(p).word === wd);
  }

  // The × by the words being heard (canCancel): they go.
  await T.act('cru-state', 'listen');
  await T.act('cru-hear', 'turn on the lights');
  await T.until('words being heard: the × to cancel them by them', (p) => U(p).cancelX && C(p).canCancel);
  await T.act('cru-cancel');
  await T.until('…the × takes them back', (p) => !C(p).interim && !U(p).cancelX);
  // Said while Cru thinks: queued, "Queued, answered next", taken back with the turn.
  await T.act('cru-state', 'think');
  await T.act('cru-utter', 'and the blog?');
  p = await T.until('said while Cru thinks: queued, "Queued, answered next" under the words', (p) => C(p).queued === 'and the blog?' && U(p).queuedLine === 'and the blog?' && U(p).cancelX);
  await T.act('cru-cancel');
  await T.until('…the × cancels the turn with what was queued ("Cancelled.")', (p) => !C(p).pending && C(p).phase !== 'think' && /^Cancelled/.test(C(p).notice));

  // Esc closes; Ctrl+Space held opens it to talk, let go sends what was heard.
  await T.key('Escape');
  await T.until('Esc closes it', (p) => !C(p).open);
  await T.send({ cmd: 'key', text: 'Space', mods: ['ctrl'], phase: 'down' });
  p = await T.until('Ctrl+Space held: Cru opens to talk (push to talk)', (p) => C(p).open && C(p).ptt && C(p).how === 'ptt');
  await t.sleep(1500);
  await T.act('cru-state', 'listen');
  await T.act('cru-hear', 'what is running in demo');
  const before = C(await T.props()).turns;
  await t.sleep(300);
  t.check('…held, nothing goes yet', C(await T.props()).turns === before);
  await T.send({ cmd: 'key', text: 'Space', mods: ['ctrl'], phase: 'up' });
  await T.until('…let go: what was heard goes to Cru', (p) => !C(p).ptt && C(p).turns === before + 1 && C(p).heard === 'what is running in demo', 10000);
  await T.key('Escape');
  await T.until('closed', (p) => !C(p).open);
  // The shortcut from Cru's settings (per device).
  writeFileSync(`${T.configDir}/ptt.json`, JSON.stringify({ ctrl: true, alt: true, shift: false, meta: false, key: 'k' }));
  await T.send({ cmd: 'key', text: 'Space', mods: ['ctrl'] });
  await t.sleep(800);
  t.check('…another device shortcut: Ctrl+Space no longer opens it', !C(await T.props()).open);
  await T.send({ cmd: 'key', text: 'k', mods: ['ctrl', 'alt'], phase: 'down' });
  p = await T.until('…Ctrl+Alt+K held opens it, the hint says so', (p) => C(p).open && C(p).ptt && U(p).hint === 'Hold Ctrl+Alt+K to talk · click the mic to keep this open');
  await T.send({ cmd: 'key', text: 'k', mods: ['ctrl', 'alt'], phase: 'up' });
  await T.until('…let go', (p) => !C(p).ptt);
  rmSync(`${T.configDir}/ptt.json`, { force: true });

  // The keyboard: the typing box only while it is on (the chat view, as the web's picture).
  await T.act('cru-menu', 'chat-view');
  await T.until('back to the chat view', (p) => U(p).view === 'chat' && U(p).bubbles > 0);
  // The chat view's faded bubbles: queued, and being heard, each with its ×.
  await T.act('cru-state', 'think');
  await T.act('cru-utter', 'what about shop');
  await T.until('queued while Cru thinks: a faded bubble, "Queued: Cru answers it next"', (p) => U(p).queuedBubble && C(p).queued === 'what about shop');
  await T.act('cru-drop', 'queued');
  await T.until('…its × drops it: never asked', (p) => !U(p).queuedBubble && !C(p).pending);
  await T.act('cru-state', 'listen');
  await T.act('cru-hear', 'hello there');
  await T.until('words being heard: a faded bubble', (p) => U(p).hearingBubble);
  await T.act('cru-drop', 'hearing');
  await T.until('…its × drops them', (p) => !U(p).hearingBubble && !C(p).interim);
  await T.act('cru-keyboard', 'true');
  p = await T.until('the keyboard on: the typing box and send take the mic\'s place, no hint', (p) => C(p).keyboard && U(p).typingBox && !U(p).mic && U(p).hint === '');
  await t.sleep(600);
  await T.snap('cru-keyboard');
  await T.act('cru-keyboard', 'false');
  await T.until('…off needs a Deepgram key here: it says so and stays', (p) => C(p).keyboard && /Deepgram key/.test(C(p).notice));
  await T.key('Escape');
  await T.until('Esc closes it', (p) => !C(p).open);
}

if (part('passwords')) {
  // ─── Saved passwords: offered after a sign-in that worked, saved behind the PIN, filled, shown with the PIN ───
  const PW = (p) => D(p).passwords;
  await T.api.http('POST', '/api/voice/kid/pin', { pin: '2468' });
  await T.until('the passwords\' key is ready', (p) => PW(p).ready && PW(p).available, 15000);
  if (!dr(await T.props()).open) await T.act('browser');
  const nt = await T.act('new-tab', `${PAGE}/login`);
  const tab = nt.id;
  await T.until('a sign-in page in a tab', (p) => dr(p).tabs.some((x) => x.id === tab && /login/.test(x.title)));
  await t.sleep(800);
  await evalTab(tab, "document.getElementById('u').value='me@example.com';document.getElementById('p').value='s3cret-pw!';document.getElementById('go').click();true");
  let p = await T.until('a sign-in that worked offers to save', (p) => PW(p).bars[tab]?.kind === 'save' && PW(p).bars[tab].username === 'me@example.com', 30000);
  await T.snap('pw-offer');
  await T.act('pw-save', 'me@example.com');
  p = await T.until('Save keeps it (the PIN is set)', (p) => PW(p).logins.length === 1 && !PW(p).bars[tab]);
  const id = PW(p).logins[0].id;
  let r = await mcp('browser_eval', { key: `drawer:${tab}`, code: "'the password is s3cret-pw!'" });
  t.check('a chat never reads it back from the tab', r.ok && r.result === 'the password is ••••••••', r);
  await T.act('go', `${PAGE}/login`);
  p = await T.until('back on the sign-in page: the fill bar', (p) => PW(p).bars[tab]?.kind === 'fill' && PW(p).bars[tab].logins[0].id === id, 15000);
  await T.snap('pw-fill');
  await T.act('pw-fill', id);
  let v;
  for (let i = 0; i < 20; i++) { v = await evalTab(tab, "document.getElementById('u').value+'|'+document.getElementById('p').value"); if (v.result === 'me@example.com|s3cret-pw!') break; await t.sleep(300); }
  t.check('Fill puts the login in the page', v.result === 'me@example.com|s3cret-pw!', v);
  await T.act('panel', 'passwords');
  await T.act('pw-show', id);
  await T.until('Show asks for the PIN first', (p) => PW(p).pinFor === id);
  await T.act('pw-unlock', '0000');
  await t.sleep(800);
  t.check('…a wrong PIN shows nothing', !PW(await T.props()).revealed.includes(id));
  await T.act('pw-unlock', '2468');
  await T.until('…the PIN shows it', (p) => PW(p).revealed.includes(id));
  await T.until('(the wrong PIN\'s toast has gone)', (p) => !p.toast, 10000);
  await T.snap('pw-panel');
  await T.act('pw-remove', id);
  await T.until('Delete asks first', (p) => /Delete this saved password/.test(p.dialog?.title || ''));
  await T.dialog(true);
  await T.until('…and deletes it', (p) => PW(p).logins.length === 0);
  await T.act('panel-close');
}

if (part('cruscreen')) {
  // ─── Cru's live screen: blocks, a page in a frame (talking both ways), the full view ───
  const CS = (p) => D(p).cruScreen;
  await T.act('cru-keyboard', 'true');
  await T.act('cru');
  await T.until('Cru opens, with its events socket', (p) => D(p).cru.open && CS(p).socket);
  const blocks = [
    { type: 'heading', text: 'Your day' }, { type: 'text', text: 'Three things, one done.' },
    { type: 'checklist', items: [{ text: 'Walk', done: true }, { text: 'Write the report' }, { text: 'Call Mum' }] },
    { type: 'table', headers: ['When', 'What'], rows: [['9:00', 'Stand-up'], ['14:00', 'Dentist']] },
    { type: 'progress', label: 'Report', value: 40 },
    { type: 'chips', items: [{ text: 'on time', tone: 'ok' }, { text: 'busy', tone: 'warn' }, { text: 'late', tone: 'bad' }] },
  ];
  await T.act('cru-event', JSON.stringify({ type: 'screen', screen: { title: 'Today', blocks } }));
  await T.until('blocks on the live screen', (p) => CS(p).screen?.title === 'Today' && !CS(p).frame);
  await T.snap('cru-screen-blocks');
  const doc = `<!doctype html><html><body style="margin:0;font:28px sans-serif;background:#fde68a;display:flex;align-items:center;justify-content:center;height:100vh"><div>A game for you</div>
<script>addEventListener('message',function(e){if(e.data&&e.data.cru==='volume'){document.title='volume '+e.data.value;parent.postMessage({cru:'event',kind:'start',detail:'loaded'},'*');}});</script></body></html>`;
  await T.act('cru-event', JSON.stringify({ type: 'screen', screen: { title: 'A game', doc } }));
  await T.until('a page in its own frame, painted', (p) => CS(p).frame && CS(p).painted, 30000);
  await T.until('…Cru sends it the volume on load', (p) => CS(p).title === 'volume 1', 15000);
  await T.until('…and hears it back (the message bridge)', (p) => CS(p).lastMessage?.cru === 'event' && CS(p).lastMessage?.detail === 'loaded', 15000);
  await t.sleep(500);
  await T.snap('cru-screen-frame');
  await T.act('cru-screen-full');
  await T.until('Full view: Cru\'s whole view', (p) => CS(p).full);
  await t.sleep(500);
  await T.snap('cru-full');
  await T.act('cru-screen-normal');
  await T.until('Normal view: the panel again', (p) => !CS(p).full && CS(p).screen);
  await T.act('cru-screen-close');
  await T.until('× takes it down', (p) => !CS(p).screen && !CS(p).frame);
  await T.act('cru');
  await T.until('Cru closes', (p) => !D(p).cru.open);
}

if (part('kid')) {
  // ─── Kid mode: started from the tray, left only with the PIN ───
  const K = (p) => D(p).kid;
  await T.api.http('POST', '/api/voice/kid/pin', { pin: '2468' });
  await T.act('kid-start', 'tray');
  await T.until('the kid screen opens over everything', (p) => K(p).on && D(p).cru.open, 15000);
  await t.sleep(500);
  await T.snap('cru-kid');
  await T.key('Escape');
  await t.sleep(300);
  t.check('Esc does not leave it', K(await T.props()).on);
  await T.act('kid-pin');
  for (const k of ['1', '1', '1', '1', 'OK']) await T.act('kid-key', k);
  await T.until('a wrong PIN: "That is not the PIN"', (p) => /not the PIN/.test(K(p).pinError) && K(p).on, 10000);
  await T.snap('cru-kid-pin');
  for (const k of ['2', '4', '6', '8', 'OK']) await T.act('kid-key', k);
  await T.until('the right PIN leaves kid mode', (p) => !K(p).on && !D(p).cru.open, 10000);
}

if (part('wake')) {
  // ─── "Hey Cru": trained from five recordings, listening only when it may, the owner's voice opens Cru ───
  const W = (p) => D(p).wake;
  const fx = (n) => new URL(`./fixtures/heycru/${n}.wav`, import.meta.url).pathname;
  // A second of silence, as a recording.
  const silent = '/tmp/crundi-wake-silent.wav';
  { const n = 16000, b = Buffer.alloc(44 + n * 2); b.write('RIFF', 0); b.writeUInt32LE(36 + n * 2, 4); b.write('WAVEfmt ', 8); b.writeUInt32LE(16, 16); b.writeUInt16LE(1, 20); b.writeUInt16LE(1, 22); b.writeUInt32LE(16000, 24); b.writeUInt32LE(32000, 28); b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34); b.write('data', 36); b.writeUInt32LE(n * 2, 40); writeFileSync(silent, b); }
  let p = await T.props();
  t.check('off and untrained at first', !W(p).enabled && !W(p).trained, W(p));
  await T.ui('tab', 'settings');
  await T.ui('ops:settings-open', 'voice');
  await t.sleep(800);
  // Down to "On this device", at the voice section's end.
  const toDevice = async () => { await T.ui('ops:settings-open', 'voice'); await t.sleep(600); for (let i = 0; i < 5; i++) await T.mouse('wheel', 800, 500, { dy: -300 }); await t.sleep(600); };
  await toDevice();
  await T.snap('settings-voice-device');
  await T.act('wake-train-start');
  await T.until('Train: five recordings, the first', (p) => W(p).training && W(p).trainPos === 0);
  await T.act('wake-take', silent);
  await T.until('a silent take is refused, as the web says', (p) => W(p).trainBad && /Nothing was heard\. Speak a little closer\. Record it again\./.test(W(p).trainMsg) && W(p).trainPos === 0);
  for (let i = 1; i <= 2; i++) await T.act('wake-take', fx(`draco-train${i}`));
  await T.until('…two kept', (p) => W(p).trainPos === 2 && W(p).trainMsg === 'Got it.');
  await toDevice();
  await T.snap('wake-training');
  for (let i = 3; i <= 5; i++) await T.act('wake-take', fx(`draco-train${i}`));
  p = await T.until('…the fifth trains it', (p) => W(p).trained && !W(p).training, 120000);
  t.check('…with the owner\'s voice check', W(p).voiceCheck === 'on' || !process.env.CRUNDI_WAKE_MODELS, W(p));
  await toDevice();
  await T.snap('wake-trained');
  await T.act('wake-cond', 'false', 'false');
  await T.act('wake-set', 'true');
  await T.until('on: listening (on AC, unlocked, Cru closed)', (p) => W(p).state === 'listening', 60000);
  await T.act('wake-cond', 'true', 'false');
  await T.until('on battery: paused', (p) => W(p).state === 'on-battery' && W(p).label === 'Paused: on battery');
  await T.act('wake-cond', 'false', 'true');
  await T.until('screen locked: paused', (p) => W(p).state === 'paused' && W(p).label === 'Paused: screen locked');
  await T.act('wake-cond', 'false', 'false');
  await T.until('…listening again', (p) => W(p).state === 'listening', 30000);
  await T.ui('tab', 'workbench');
  await T.act('wake-hear', fx('draco-neg-heyyou'));
  await t.sleep(4000);
  t.check('a decoy ("hey you") does not open Cru', !D(await T.props()).cru.open);
  if (process.env.CRUNDI_WAKE_MODELS) {
    await T.act('wake-hear', fx('arcas-pos-heycru'));
    await t.sleep(4000);
    t.check('another voice\'s "Hey Cru" does not open Cru', !D(await T.props()).cru.open, W(await T.props()).log);
  }
  await T.act('wake-hear', fx('draco-pos-heycru'));
  await T.until('the owner\'s "Hey Cru" opens Cru', (p) => D(p).cru.open && W(p).lastHeard?.keyword === 'HEY_CRU', 15000);
  await T.until('…and it pauses while Cru\'s screen is open', (p) => W(p).state === 'voice-open');
  await T.act('cru');
  await T.until('Cru closed: listening again', (p) => !D(p).cru.open && W(p).state === 'listening', 30000);
  await T.act('wake-set', 'false');
  await T.act('wake-forget');
  await T.until('Forget asks first', (p) => /Forget your/.test(p.dialog?.title || ''));
  await T.dialog(true);
  await T.until('…and forgets: off, untrained', (p) => !W(p).trained && !W(p).enabled);
  await T.act('wake-cond', '', '');
}

if (part('bubble')) {
  // ─── The chat bubble: a chat in a floating window of its own (6.3) ───
  const B = (p) => D(p).bubble;
  const shotDir = process.env.SCREENS_DIR || '/tmp';
  const id = (await T.api.http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Bubble chat' })).body?.id;
  t.check('a chat to float', !!id);
  await T.until('…with its pane', (p) => (p.panes || []).some((q) => q.key === `live:${id}`), 30000);
  await T.act('bubble-open', id);
  let p = await T.until('the bubble opens on it, expanded', (p) => B(p).open && B(p).expanded && B(p).chat === id && B(p).title === 'Bubble chat');
  const n0 = B(p).entries;
  await T.act('bubble-send', 'Hello from the bubble');
  p = await T.until('a message sent from the bubble goes into the chat', (p) => B(p).entries > n0, 30000);
  await t.sleep(1500);
  await T.act('bubble-snap', `${shotDir}/bubble.png`);
  await T.act('bubble-toggle');
  p = await T.until('the head collapses it', (p) => B(p).open && !B(p).expanded && !B(p).unread);
  const n1 = B(p).entries;
  await T.ui('send', 'A message from the main window');
  await T.until('…a new message while collapsed: the unread dot', (p) => B(p).entries > n1 && B(p).unread, 30000);
  await t.sleep(500);
  await T.act('bubble-snap', `${shotDir}/bubble-head.png`);
  await T.act('bubble-toggle');
  await T.until('expanded again, read', (p) => B(p).expanded && !B(p).unread);
  await T.act('bubble-close');
  await T.until('× closes it', (p) => !B(p).open);
}

if (part('tray')) {
  // ─── The tray: its menu in the PC client's order and words; Server… opens the server's settings ───
  const tr = (p) => D(p).tray;
  const p = await T.until('the tray says the connection', (p) => tr(p).tooltip === 'Crundi — Running');
  t.check('its menu', JSON.stringify(tr(p).menu.map((m) => m.label)) === JSON.stringify(['Crundi', '', 'Running', '', 'Open', 'Kid mode', 'Server…', '', 'Quit']), tr(p).menu);
  // With a session bus (dbus-run-session), the item is on it.
  if (process.env.DBUS_SESSION_BUS_ADDRESS) {
    t.check('the tray is up', tr(p).shown, tr(p));
    const names = (await import('node:child_process')).execSync('gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.ListNames').toString();
    t.check('…a StatusNotifierItem on the session bus', /org\.kde\.StatusNotifierItem-\d+-\d+/.test(names), names.slice(0, 300));
  } else console.log('  no session bus: the tray is not checked (run under dbus-run-session)');
  await T.act('tray-pick', 'server');
  await T.until('Server… shows the server\'s settings', (p) => p.currentTab === 'settings');
  await T.ui('tab', 'workbench');
}

if (part('node')) {
  // ─── The homepool node: off until shared ───
  const r = await T.act('homepool-status');
  t.check('the node is off, with this PC\'s id', r.ok && r.status.state === 'off' && /^pc-/.test(r.status.deviceId), r);
}

if (part('unload')) {
  // ─── Not visible: hidden at once, freed after the unload grace, loaded again when visible ───
  // (Short graces: CRUNDI_BROWSER_UNLOAD_SECS 3, CRUNDI_BROWSER_IDLE_SECS 4.)
  await T.act('drawer', 'false');
  await T.relaunch({ CRUNDI_BROWSER_IDLE_SECS: '4', CRUNDI_BROWSER_UNLOAD_SECS: '3' });
  const BH = (p) => D(p).browserHost;
  // A chat embed: an animating page on the log.
  const fc = (await T.api.http('POST', '/api/ui-sessions/create', { project: 'demo', title: 'Unload' })).body?.id;
  const paneOf = (p) => (p.panes || []).find((x) => x.key === 'live:' + fc);
  let p = await T.until('a chat pane for the embed', (p) => paneOf(p), 30000);
  await T.ui('focus-pane', paneOf(p).id);
  const src = `${PAGE}/anim`;
  const fkey = `${fc}|${src}`;
  const fr = (p) => (p.frames || []).find((x) => x.key === fkey);
  await T.ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: 'un1', kind: 'tool', name: 'mcp__crundi__show_embed', toolUseId: 'tun1', input: {}, status: 'done', isError: false, result: JSON.stringify({ ok: true, items: [{ kind: 'embed', src, title: 'Anim' }] }) } }));
  p = await T.until('the embed on the log: a live view, painting', (p) => fr(p)?.live && fr(p)?.painted && fr(p)?.frames > 3 && !fr(p)?.paused, 30000);
  // One reply at a time, until the embed leaves the view: the time it did.
  for (let i = 0; i < 20 && fr(await T.props())?.onScreen; i++) await T.ui('chat-event', JSON.stringify({ type: 'entry', entry: { id: `away${i}`, kind: 'assistant-text', text: Array(12).fill('A long reply line to push the frame off the log.').join('\n\n') } }));
  const off = Date.now();
  p = await T.until('scrolled out of the log\'s view: hidden at once (the view kept)', (p) => fr(p) && !fr(p).onScreen && fr(p).live && fr(p).paused, 2000);
  t.check('…within a second', Date.now() - off < 1500 || !p, { ms: Date.now() - off });
  if (p) {
    const f0 = fr(p).frames;
    await t.sleep(1000);
    const q = await T.props();
    t.check('…and no frames while hidden', fr(q).frames === f0 || !fr(q).live, { before: f0, after: fr(q).frames });
  }
  p = await T.until('…freed after the unload grace', (p) => fr(p) && !fr(p).live, 10000);
  t.check('…not before it', Date.now() - off >= 2500, { ms: Date.now() - off });
  t.check('…the host still up (the drawer and other views aside)', BH(p).state === 'up' || BH(p).state === 'stopping');
  // Back into view (the wheel over the log): made again, with the loading state, then painting.
  const pn = paneOf(await T.props());
  let back = false;
  for (let i = 0; i < 120 && !back; i++) {
    await T.mouse('wheel', pn.x + pn.w / 2, pn.y + pn.h / 2, { dy: 300 });
    await t.sleep(60);
    back = !!fr(await T.props())?.onScreen;
  }
  t.check('scrolled back into view', back);
  p = await T.until('…a new view, loaded again (painting)', (p) => fr(p)?.live && fr(p)?.painted && !fr(p)?.paused, 20000);
  // Nothing visible anywhere: the embed scrolled off again and freed; then the host stops after the idle grace.
  // (Scrolled up by hand, the log no longer follows the bottom: the wheel takes it down again.)
  for (let i = 0; i < 120 && fr(await T.props())?.onScreen; i++) { await T.mouse('wheel', pn.x + pn.w / 2, pn.y + pn.h / 2, { dy: -300 }); await t.sleep(60); }
  await T.until('…out of view again: freed after the grace', (p) => fr(p) && !fr(p).live, 12000);
  p = await T.until('nothing visible anywhere: the host stops after the idle grace', (p) => BH(p).state === 'off' && BH(p).views === 0, 15000);
  await T.api.http('POST', `/api/ui-sessions/${fc}/close`, {});

  // ─── A background drawer tab: the same rule ───
  await T.act('drawer', 'true');
  const t1 = (await T.act('new-tab', `${PAGE}/anim`)).id;
  const tabOf = (p, id) => dr(p).tabs.find((x) => x.id === id);
  p = await T.until('a drawer tab, shown, painting', (p) => tabOf(p, t1)?.panel != null && tabOf(p, t1).frames > 3 && !tabOf(p, t1).paused, 30000);
  const t2 = (await T.act('new-tab', `${PAGE}/two`)).id;
  const hid = Date.now();
  p = await T.until('behind another tab: hidden at once (the page kept)', (p) => tabOf(p, t1)?.paused && tabOf(p, t1).panel != null, 2000);
  p = await T.until('…freed after the unload grace, still listed', (p) => tabOf(p, t1) && tabOf(p, t1).panel == null && tabOf(p, t1).unloaded, 10000);
  t.check('…not before it', Date.now() - hid >= 2500, { ms: Date.now() - hid });
  await T.act('activate', t1);
  p = await T.until('…clicked: it loads again', (p) => tabOf(p, t1)?.panel != null && tabOf(p, t1).frames > 0 && !tabOf(p, t1).paused, 30000);
  t.check('…and the tab that went behind is hidden now', tabOf(p, t2)?.paused === true || tabOf(p, t2)?.panel == null, tabOf(p, t2));
  await T.act('drawer', 'false');
  p = await T.until('the drawer closed: every tab freed after the grace, listed', (p) => dr(p).tabs.every((x) => x.panel == null) && tabOf(p, t1) && tabOf(p, t2), 12000);
  p = await T.until('…then the host stops after the idle grace', (p) => BH(p).state === 'off' && BH(p).views === 0, 15000);
  await T.act('drawer', 'true');
  p = await T.until('the drawer opened again: only the active tab loads', (p) => tabOf(p, t1)?.panel != null && tabOf(p, t2)?.panel == null && BH(p).state === 'up', 30000);
  await T.act('close-tab', t1);
  await T.act('close-tab', t2);
  await T.act('drawer', 'false');
}

if (part('hostproc')) {
  // ─── The browser host: Chromium in a process of its own (webview's host/, client.rs) ───
  // The processes under a PID (Linux /proc), live ones only (not zombies).
  const procs = () => {
    const out = new Map();
    for (const n of readdirSync('/proc')) {
      if (!/^\d+$/.test(n)) continue;
      try {
        const st = readFileSync(`/proc/${n}/stat`, 'utf8');
        const rest = st.slice(st.lastIndexOf(')') + 2).split(' ');
        out.set(Number(n), { ppid: Number(rest[1]), state: rest[0] });
      } catch { /* gone */ }
    }
    return out;
  };
  const tree = (pid) => {
    const all = procs();
    const found = [];
    const todo = [pid];
    while (todo.length) { const p = todo.pop(); for (const [c, i] of all) if (i.ppid === p && !found.includes(c)) { found.push(c); todo.push(c); } }
    return found.filter((c) => all.get(c).state !== 'Z');
  };
  const live = (pid) => { try { const st = readFileSync(`/proc/${pid}/stat`, 'utf8'); return st.slice(st.lastIndexOf(')') + 2)[0] !== 'Z'; } catch { return false; } };
  const cmd = (pid) => { try { return readFileSync(`/proc/${pid}/cmdline`, 'utf8').split('\0'); } catch { return []; } };
  const hostsUnder = (pid) => tree(pid).filter((c) => cmd(c).includes('--browser-host'));
  const BH = (p) => D(p).browserHost;
  const tab = (p, id) => dr(p).tabs.find((x) => x.id === id);

  // Tabs to remember: two in this project, the first active.
  let p = await T.props();
  if (dr(p).tabs.length < 2) {
    await T.act('new-tab', `${PAGE}/one`);
    await T.act('new-tab', `${PAGE}/two`);
  }
  p = await T.until('two tabs to remember', (p) => dr(p).tabs.length >= 2);
  const [ta, tb] = [dr(p).tabs[0].id, dr(p).tabs[1].id];
  await T.act('activate', ta);
  await T.act('hide');
  await T.until('the drawer closed, the first tab active', (p) => !dr(p).open && dr(p).active === ta);

  // A relaunch: no host, the tabs listed but not loaded.
  await T.relaunch({ CRUNDI_BROWSER_IDLE_SECS: '4', CRUNDI_BROWSER_UNLOAD_SECS: '8' });
  p = await T.props();
  t.check('no browser host process at launch', BH(p).state === 'off' && hostsUnder(T.app.pid).length === 0, { host: BH(p), under: tree(T.app.pid).map(cmd) });
  // Only the browser host links CEF: the app never maps libcef.
  const libcef = (pid) => { try { return readFileSync(`/proc/${pid}/maps`, 'utf8').split('\n').some((l) => /libcef/.test(l)); } catch { return null; } };
  t.check('the idle app has no libcef mapped (/proc/<pid>/maps)', libcef(T.app.pid) === false && BH(p).libcef === false, { maps: libcef(T.app.pid), props: BH(p).libcef });
  t.check('after a relaunch the tabs are listed, none loaded, the drawer closed', !dr(p).open && dr(p).tabs.length >= 2 && dr(p).tabs.every((x) => x.panel == null), dr(p).tabs);
  await t.sleep(1500);
  t.check('…and the host still does not start by itself', hostsUnder(T.app.pid).length === 0 && BH(await T.props()).state === 'off');

  // The drawer opens: the host starts, only the active tab loads.
  await T.act('drawer', 'true');
  p = await T.until('the drawer opens: the browser host starts', (p) => BH(p).state === 'up' && BH(p).pid > 0, 30000);
  const host1 = BH(p).pid;
  t.check('…as a child process of the app', hostsUnder(T.app.pid).includes(host1), { host1, under: hostsUnder(T.app.pid) });
  p = await T.until('…and the active tab loads', (p) => tab(p, ta)?.panel != null && /page \//.test(tab(p, ta)?.title || ''), 30000);
  t.check('…the other tab does not', tab(p, tb)?.panel == null, tab(p, tb));
  await T.act('activate', tb);
  await T.until('a click on the other tab loads it', (p) => tab(p, tb)?.panel != null && /page \//.test(tab(p, tb)?.title || ''), 30000);
  await T.act('activate', ta);
  t.check('…the host has libcef, the app still does not', libcef(host1) === true && libcef(T.app.pid) === false, { host: libcef(host1), app: libcef(T.app.pid) });

  // No frames for what is not on screen: a tab behind another, the drawer closed.
  const anim = (await T.act('new-tab', `${PAGE}/anim`)).id;
  const fr = (p) => tab(p, anim)?.frames || 0;
  await T.until('an animating tab loads', (p) => tab(p, anim)?.panel != null && fr(p) > 0, 30000);
  {
    const a = fr(await T.props());
    await t.sleep(1500);
    const b = await T.props();
    t.check('…shown, it sends frames all the time', fr(b) > a + 5 && !tab(b, anim)?.paused, { before: a, after: fr(b) });
  }
  await T.act('activate', ta);
  p = await T.until('…behind another tab it is hidden in the host', (p) => tab(p, anim)?.paused, 5000);
  await t.sleep(600);
  let f0 = fr(await T.props());
  const sent0 = BH(await T.props()).frames;
  await t.sleep(2500);
  p = await T.props();
  t.check('…and sends no frames while hidden', fr(p) === f0, { before: f0, after: fr(p) });
  t.check('…(the active tab, a still page, sends none either: nothing at all comes)', BH(p).frames === sent0, { before: sent0, after: BH(p).frames });
  await T.act('activate', anim);
  p = await T.until('…shown again, it draws again at once', (p) => !tab(p, anim)?.paused && fr(p) > f0 + 3, 10000);
  p = await T.until('…the host reports what it sent (frames, bytes copied into segments)', (p) => BH(p).host?.sent > 0 && BH(p).host?.copied > 0, 6000);
  await T.act('drawer', 'false');
  p = await T.until('the drawer closed: its pages are hidden', (p) => !dr(p).open && tab(p, anim)?.paused, 5000);
  await t.sleep(300);
  f0 = fr(await T.props());
  await t.sleep(2000);
  p = await T.props();
  // (Within the idle grace, 4 s here: after it the pages are let go, which is no frames as well.)
  t.check('…and send no frames', fr(p) === f0 || tab(p, anim)?.panel == null, { before: f0, after: fr(p), tab: tab(p, anim) });
  await T.act('drawer', 'true');
  p = await T.until('the drawer again: the animating tab draws again', (p) => dr(p).open && !tab(p, anim)?.paused && fr(p) > 3, 30000);
  await T.act('close-tab', anim);
  await T.until('…its tab closed', (p) => !tab(p, anim));
  // Both tabs loaded again (the drawer closed past the grace let them go).
  await T.act('activate', tb);
  await T.until('…the second tab loaded again', (p) => tab(p, tb)?.panel != null && /page \//.test(tab(p, tb)?.title || ''), 30000);
  await T.act('activate', ta);
  await T.until('…and the first', (p) => tab(p, ta)?.panel != null && /page \//.test(tab(p, ta)?.title || ''), 30000);

  // kill -9 the host: the pages show the crash, the app stays, Reload restarts it.
  process.kill(host1, 'SIGKILL');
  p = await T.until('the host killed: its pages show "This page crashed"', (p) => tab(p, ta)?.crashed && tab(p, tb)?.crashed && BH(p).state === 'off' && BH(p).crashes >= 1, 20000);
  t.check('…and the app is still up', !T.exited && live(T.app.pid));
  await t.sleep(500);
  await T.snap('drawer-crashed');
  await T.act('reload', ta);
  p = await T.until('Reload starts a new host and loads the page again', (p) => BH(p).state === 'up' && BH(p).pid !== host1 && !tab(p, ta)?.crashed && /page \//.test(tab(p, ta)?.title || ''), 30000);
  t.check('…only the old host is gone', !live(host1));

  // The drawer closed: its pages go after the grace, then the host stops.
  await T.act('drawer', 'false');
  const host2 = BH(p)?.pid;
  p = await T.until('the drawer closed: its pages go after the unload grace', (p) => dr(p).tabs.every((x) => x.panel == null), 20000);
  p = await T.until('…and the host stops after the idle grace with no views', (p) => BH(p).state === 'off' && BH(p).views === 0, 20000);
  await T.until('…its process gone (and its helpers)', () => !live(host2) && hostsUnder(T.app.pid).length === 0 && tree(T.app.pid).every((c) => !cmd(c).some((a) => a.startsWith('--type='))), 10000);
  t.check('…the tabs still listed', dr(p).tabs.length >= 2);

  // A stale host from a crashed run, and a stale CEF lock: reaped at start.
  const stale = spawn(process.env.NATIVE_BIN, ['--browser-host'], { stdio: ['pipe', 'ignore', 'ignore'], detached: true });
  let staleGone = false;
  stale.on('exit', () => { staleGone = true; });
  await t.sleep(800);
  writeFileSync(`${T.configDir}/browser-host.pid`, `${stale.pid}\n`);
  const lock = `${T.cefDir}/SingletonLock`;
  try { rmSync(lock, { force: true }); } catch { /* none */ }
  symlinkSync('no-such-host-4194300', lock);
  t.check('a stale host is running before the relaunch', !staleGone && live(stale.pid));
  await T.relaunch({ CRUNDI_BROWSER_IDLE_SECS: '4', CRUNDI_BROWSER_UNLOAD_SECS: '8' });
  p = await T.props();
  for (let i = 0; i < 40 && !staleGone; i++) await t.sleep(100);
  t.check('a stale host from a previous run is reaped at start', staleGone && BH(p).reaped === stale.pid, { gone: staleGone, host: BH(p) });
  let lockLeft = true;
  try { lstatSync(lock); } catch { lockLeft = false; }
  t.check('…and its stale CEF lock cleared', !lockLeft);

  // kill -9 the app: nothing of the browser remains.
  await T.act('drawer', 'true');
  p = await T.until('the drawer again: a host with its helpers', (p) => BH(p).state === 'up' && /page \//.test(tab(p, ta)?.title || ''), 30000);
  await t.sleep(1500);
  const shmBefore = new Set(readdirSync('/dev/shm'));
  const under = tree(T.app.pid);
  t.check('…the host and CEF\'s helpers are under the app', under.some((c) => cmd(c).includes('--browser-host')) && under.some((c) => cmd(c).some((a) => a.startsWith('--type='))), under.map((c) => cmd(c).slice(0, 2)));
  const ourShm = () => readdirSync('/dev/shm').filter((n) => /crundi/i.test(n) || (/chromium/i.test(n) && !shmBefore.has(n)));
  const appGone = new Promise((r) => T.app.once('exit', r));
  process.kill(T.app.pid, 'SIGKILL');
  await appGone;
  let left = under;
  for (let i = 0; i < 100 && left.length; i++) { await t.sleep(100); left = under.filter(live); }
  t.check('kill -9 the app: no host or helper process remains', left.length === 0, left.map((c) => cmd(c).slice(0, 2)));
  t.check('…and no shared-memory segment of ours in /dev/shm', ourShm().length === 0, ourShm());
}

await T.end();
