#!/usr/bin/env node
// The desktop area end to end (AREAS.md): the browser drawer (tabs per
// project, the address bar, devices, networks, annotations sent to a chat,
// history and clearing), the native host's browser protocol on drawer tabs
// and hidden browsers, the Browsers tab, the Inbox, notifications.
//
//   native-client/tools/test-server.sh 4396 --bare
//   BIN=$(native-client/tools/area-build.sh desktop)
//   NATIVE_BIN=$BIN xvfb-run -a -s "-screen 0 1440x900x24" node native-client/test/desktop.mjs
//
// (CRUNDI_TEST_SERVER=http://localhost:<port> CRUNDI_TEST_PORT=<port> for
// other ports.) ONLY=drawer,host,inbox,cru,cruscreen,kid,wake,passwords,bubble,tray,node runs some parts.

import http from 'node:http';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
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
  // ─── Cru: the top bar's mark opens its screen ───
  await T.act('cru-keyboard', 'true');
  await T.act('cru');
  const p = await T.until('Cru\'s screen opens (keyboard mode, its settings read)', (p) => D(p).cru.open && D(p).cru.keyboard);
  t.check('a server with no Claude key says so', /no Claude API key/.test(D(p).cru.unavailable || '') || D(p).cru.unavailable === '', D(p).cru);
  await T.snap('cru-open');
  await T.key('Escape');
  await T.until('Esc closes it', (p) => !D(p).cru.open);
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

await T.end();
