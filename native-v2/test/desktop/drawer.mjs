// The drawer: open, a first tab, the address bar, tabs, history, a window a
// page opens, devices, networks (native-gpui/test/desktop.mjs "The drawer…").
// v2's own: the drawer is a layout view whose bars are cached leaves beside
// the page: a page's frames re-render the page alone, never a bar, and an
// open drawer on a still page draws nothing.
//
// Also the shared helpers of the drawer's parts (host, passwords, browsers,
// unload, hostproc): the page server, a tab's eval, the server's tools.

import http from 'node:http';
import { readFileSync, createWriteStream, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { FrameMeter } from '../../tools/frames.mjs';

let PAGE = null;
/** A page server (ephemeral port), started once: a button that counts
 *  clicks, a link that opens a window, a sign-in form, an animating page. */
export async function pageServer() {
  if (PAGE) return PAGE;
  const srv = http.createServer((req, res) => {
    res.setHeader('Content-Type', 'text/html; charset=utf-8');
    // A sign-in form (saved passwords): it posts to /welcome, which has none.
    if (req.url.startsWith('/login')) return res.end(`<!doctype html><title>login</title><body style="font:16px sans-serif;margin:20px">
<form action="/welcome" method="post"><input id=u name=u type=text placeholder=Email><input id=p name=p type=password placeholder=Password><button id=go type=submit>Sign in</button></form>`);
    if (req.url.startsWith('/welcome')) return res.end('<!doctype html><title>welcome</title><body style="font:16px sans-serif;margin:20px">Welcome back.');
    // A page that never stops drawing (a counter on every animation frame).
    if (req.url.startsWith('/anim')) return res.end(`<!doctype html><title>anim</title><body style="font:40px sans-serif;margin:20px"><div id=n>0</div>
<script>let n=0;(function f(){document.getElementById('n').textContent=++n;requestAnimationFrame(f)})()</script>`);
    res.end(`<!doctype html><meta name=viewport content="width=device-width"><title>page ${req.url}</title>
<body style="font:16px sans-serif;margin:20px">
<button id=b style="position:absolute;left:40px;top:80px;width:160px;height:50px" onclick="this.textContent='clicked '+(++window.n)">Click me</button>
<a id=pop href="/popped" target="_blank" style="position:absolute;left:40px;top:160px">open a window</a>
<p style="margin-top:220px">viewport <span id=vp></span></p>
<script>window.n=0;console.log('hello from '+location.pathname);function vp(){document.getElementById('vp').textContent=innerWidth+'x'+innerHeight+' dpr '+devicePixelRatio+' touch '+('ontouchstart' in window)}vp();addEventListener('resize',vp)</script>`);
  });
  await new Promise((r) => srv.listen(0, '127.0.0.1', r));
  srv.unref();
  PAGE = `http://127.0.0.1:${srv.address().port}`;
  return PAGE;
}

export const dr = (T, p) => T.D(p).drawer;
export const panelOf = (T, p, id) => dr(T, p).tabs.find((x) => x.id === id)?.panel;

/** Run code in a drawer tab's page: its answer ({ok, result} or {ok:false, error}). */
export async function evalTab(T, t, tab, code) {
  const r = await T.act('eval', tab, code);
  if (!r.ok) return r;
  for (let i = 0; i < 60; i++) {
    const v = T.D(await T.state()).evals?.[r.token];
    if (v) return v;
    await t.sleep(250);
  }
  return { ok: false, error: 'no answer' };
}

/** The server's tools, as a chat calls them (/api/mcp/call with the internal key). */
export function mcpOf(T) {
  const KEY = readFileSync(`/tmp/crundi-devtest-${new URL(T.SERVER).port}/data/.api-key`, 'utf8').trim();
  // A tool that answers with MCP content (text + an image) is read back as
  // one object: the text's JSON, with the image's data as `image`.
  const unwrap = (r) => {
    if (!r || !Array.isArray(r.content)) return r;
    const txt = r.content.find((c) => c.type === 'text');
    let o = {};
    try { o = JSON.parse(txt?.text || '{}'); } catch { o = { text: txt?.text }; }
    const img = r.content.find((c) => c.type === 'image');
    if (img && !o.image) o.image = img.data;
    return { ok: r.ok, ...o };
  };
  return (tool, args = {}) => fetch(T.SERVER + '/api/mcp/call', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Api-Key': KEY }, body: JSON.stringify({ tool, args: { alias: 'demo', ...args } }) })
    .then((r) => r.json()).then(unwrap).catch((e) => ({ ok: false, error: String(e) }));
}

/** Renders of each counted view between two states. */
export const rendersSince = (a, b) => Object.fromEntries(Object.keys(b.renders || {}).map((k) => [k, (b.renders[k] || 0) - ((a.renders || {})[k] || 0)]));
/** The drawer's bars (its cached leaves other than the page). */
export const BARS = ['drawer-tabs', 'drawer-address', 'drawer-pw', 'drawer-ann', 'drawer-body', 'drawer-note', 'drawer-suggest'];

/**
 * Stop the app and start it again with the same config (and CEF profile),
 * plus `env`; it signs in from its saved login and the project is opened
 * again. (native-gpui's T.relaunch; test/lib.mjs has none yet: this swaps
 * the harness's app, driver and frame meter under T.)
 */
export async function relaunch(T, t, env = {}, signal = 'SIGTERM') {
  const old = T.app;
  try { T.d.close(); } catch { /* gone */ }
  if (old.exitCode === null && old.signalCode === null) { const gone = new Promise((r) => old.once('exit', r)); old.kill(signal); await gone; }
  const log = createWriteStream(join(T.OUT, `native-${T.name}.log`), { flags: 'a' });
  const e = { ...process.env, CRUNDI_TEST_PORT: process.env.CRUNDI_TEST_PORT, CRUNDI_CONFIG_DIR: T.configDir, CRUNDI_OPEN_URL_FILE: join(T.configDir, 'open-url.txt'), ZED_MEASUREMENTS: '1', ...env };
  delete e.WAYLAND_DISPLAY;
  const app = spawn(process.env.NATIVE_BIN, [], { env: e, stdio: ['ignore', 'pipe', 'pipe'] });
  let exited = false;
  app.on('exit', () => { exited = true; });
  app.stdout.pipe(log);
  const frames = new FrameMeter(app.stderr, (l) => log.write(l));
  const d = await t.driver(Number(process.env.CRUNDI_TEST_PORT), () => !exited);
  const state = async () => (await d.send({ cmd: 'state' })).state || {};
  const end0 = T.__end0 || T.end;
  Object.defineProperty(T, 'exited', { get: () => exited, configurable: true });
  Object.assign(T, {
    app, d, frames, state, props: state, __end0: end0,
    ui: (n, ...args) => d.send({ cmd: 'ui', name: n, args }),
    key: (text, mods = []) => d.send({ cmd: 'key', text: text === 'Enter' ? 'Return' : text, mods }),
    mouse: (kind, x, y, extra = {}) => d.send({ cmd: 'mouse', kind, x, y, ...extra }),
    async snap(area, file) {
      const dir = join(process.env.SCREENS_DIR || fileURLToPath(new URL('../../screens/', import.meta.url)), area);
      mkdirSync(dir, { recursive: true });
      // (F107/F98: no toast of the step before over it, and its state drawn.)
      if (!/toast/i.test(file)) await d.send({ cmd: 'ui', name: 'toasts-clear', args: [] });
      await new Promise((r) => setTimeout(r, 350));
      const r = await d.send({ cmd: 'snapshot', path: join(dir, `${file}.png`) });
      if (!r?.ok) console.log(`(no snapshot ${area}/${file}: ${r?.error})`);
      return r;
    },
    async until(what, test, ms = 20000) {
      const stop = Date.now() + ms;
      let last;
      while (Date.now() < stop) {
        last = await state();
        try { if (test(last)) { t.check(what, true); return last; } } catch { /* not yet */ }
        await t.sleep(200);
      }
      t.check(what, false, last);
      return null;
    },
    async end() {
      try { d.close(); } catch { /* gone */ }
      if (!exited) { const gone = new Promise((r) => app.once('exit', r)); app.kill(); await Promise.race([gone, t.sleep(3000)]); }
      return end0();
    },
  });
  await T.until('signed in again from the saved login', (p) => p.signedIn, 30000);
  if ((await state()).project !== 'demo') await T.ui('select-project', 'demo');
  await T.until('the project is open again', (p) => p.project === 'demo' && p.online, 30000);
}

/** Open a bar's popover with a click on its button (its place from the
 *  drawer's `spots`): the menu the person sees, drawn over the page. */
export async function openPop(T, t, spot, which) {
  const p0 = await T.state();
  const b = dr(T, p0).spots?.[spot];
  t.check(`the ${spot} button has a place`, !!b, dr(T, p0).spots);
  if (!b) return null;
  await T.mouse('click', b[0] + b[2] / 2, b[1] + b[3] / 2);
  return T.until(`a click on ${spot} opens its popover`, (p) => dr(T, p).pop === which, 5000);
}

/** Esc closes the open popover (the menu has the keys). */
export async function closePop(T, t, which) {
  await T.key('Escape');
  await T.until(`Esc closes the ${which} popover`, (p) => dr(T, p).pop !== which, 5000);
}

export async function run(T, t) {
  const PAGE = await pageServer();
  const D = (p) => dr(T, p);

  // ─── The drawer: open, a first tab, the address bar ───
  await T.act('browser');
  let p = await T.until('the Browser button opens the drawer with a first tab', (p) => D(p).open && D(p).tabs.length === 1 && D(p).tabs[0].network === 'client');
  const first = D(p).tabs[0].id;
  await T.until('the drawer is drawn: its bars and its page', (p) => ['drawer', 'drawer-tabs', 'drawer-address', 'page'].every((k) => (p.renders || {})[k] > 0), 5000);
  await T.act('go', `${PAGE.replace('http://', '')}/a`);
  // (Long: under dbus-run-session Chromium's first look for a keyring waits
  // for D-Bus's activation timeout, about 25 s, before the first page loads.)
  p = await T.until('the address field opens a host typed without a scheme', (p) => D(p).tabs[0].url === `${PAGE}/a` && /page \/a/.test(D(p).tabs[0].title), 60000);
  await t.sleep(800);
  await T.snap('desktop', 'drawer-tab');

  // Tabs: a second, switching, the history.
  const r2 = await T.act('new-tab', `${PAGE}/b`);
  t.check('a second tab', r2.ok, r2);
  p = await T.until('the second tab is active and loads', (p) => D(p).active === r2.id && /page \/b/.test(D(p).tabs[1].title));
  t.check('each tab has its own page', panelOf(T, p, first) !== panelOf(T, p, r2.id), D(p).tabs);
  await T.act('activate', first);
  await T.until('a click on a tab shows it', (p) => D(p).active === first && T.D(p).drawerActive === panelOf(T, p, first));
  await T.until('the history keeps the visits', (p) => D(p).history >= 2);
  await T.act('url-typed', 'page');
  await T.act('panel', 'history');
  await T.until('the History panel lists them', (p) => D(p).panel === 'history');
  await T.until('…drawn in the body leaf', (p) => (p.renders || {})['drawer-body'] > 0, 5000);
  await T.snap('desktop', 'drawer-history');
  await T.act('panel-close');
  await T.until('Close leaves the panel', (p) => D(p).panel === '');
  // Clear browsing data: Every site / Only this site, what to clear, Cancel and Clear.
  await T.act('panel', 'clear');
  await T.until('the Clear browsing data panel', (p) => D(p).panel === 'clear');
  await T.snap('desktop', 'drawer-clear');
  await T.act('panel-close');
  await T.until('…closed', (p) => D(p).panel === '');

  // A link that opens a window: a new tab in the project.
  const before = D(await T.state()).tabs.length;
  await T.act('click', first, '60', '170');
  await T.until('a window a page opens becomes a new tab', (p) => D(p).tabs.length === before + 1 && D(p).tabs.some((x) => x.url.endsWith('/popped')));

  // Devices: the popover (opened by a click on its button, as a person
  // does: F104), a phone.
  await T.act('activate', first);
  // F221: with the shared context menu open, the press elsewhere closes it and goes through
  // (the web's capturing pointerdown): one click on the device button opens its popover.
  // F227: a real pointer's click on a context-menu item picks it (no double lease on the menu's layer).
  await T.act('test-menu', '40', '400');
  await T.until('a context menu open', (p) => !!T.D(p).menu, 5000);
  await t.sleep(300);
  await T.mouse('click', 70, 418);
  {
    const p = await T.until('a real click on a menu item picks it and closes the menu', (p) => !T.D(p).menu && T.D(p).menuPicked === 'left-open', 5000);
    t.check('…the client lives on (the item\'s pick ran once)', T.D(p).menuPicked === 'left-open', T.D(p).menuPicked);
  }
  // F236: Escape closes it (the web's document keydown, WH:12187), wherever the keys are.
  await T.act('test-menu', '40', '400');
  await T.until('a context menu open', (p) => !!T.D(p).menu, 5000);
  await T.key('Escape');
  await T.until('Escape closes the context menu, nothing picked', (p) => !T.D(p).menu, 5000);
  await T.act('test-menu', '40', '400');
  await T.until('a context menu open', (p) => !!T.D(p).menu, 5000);
  await openPop(T, t, 'device', 'dev');
  t.check('…one click closed the menu and opened the popover (the press went through)', !T.D(await T.state()).menu, T.D(await T.state()).menu);
  await T.snap('desktop', 'drawer-devices');
  await closePop(T, t, 'dev');
  await T.act('pop', 'dev');
  await T.act('pick', '3');
  p = await T.until('iPhone 15 Pro on the tab', (p) => D(p).tabs.find((x) => x.id === first)?.device === 3);
  await t.sleep(2500);
  const vp = await evalTab(T, t, first, "document.getElementById('vp').textContent");
  t.check('the page is a phone: 393x852 at 3x, touch', /^393x852 dpr 3 touch true/.test(vp.result || ''), vp);
  await T.snap('desktop', 'drawer-phone');
  await T.act('pop', 'dev');
  await T.act('pick', '0');
  await T.until('back to Responsive', (p) => D(p).tabs.find((x) => x.id === first)?.device === 0);

  // Networks: the popover's labels; the server's network for a tab.
  p = await openPop(T, t, 'net', 'net');
  await T.snap('desktop', 'drawer-network');
  await closePop(T, t, 'net');
  await T.act('pop', 'net');
  await T.act('pick', 'server');
  p = await T.until('the tab moves to the server\'s network (a page of its own)', (p) => D(p).tabs.find((x) => x.id === first)?.network === 'server');
  await T.until('…and loads there again', (p) => /page \/a/.test(D(p).tabs.find((x) => x.id === first)?.title || ''), 30000);
  await T.act('pop', 'net');
  await T.act('pick', 'client');
  await T.until('back on this PC', (p) => D(p).tabs.find((x) => x.id === first)?.network === 'client');

  // Homepool (F94): the menu offers the PC browser key's networks; its panel makes the key.
  {
    const H = (p) => T.D(p).hp;
    const net = await T.api.http('POST', '/api/homepool', { action: 'network-add', name: 'Home line' });
    const hid = net?.body?.id;
    await T.act('pop', 'net');
    p = await T.until('the Network menu asks the server: homepool is there, no key yet', (p) => H(p).available && !H(p).key && H(p).nets.some((n) => n.name === 'Home line'), 10000);
    await T.act('pick', 'hp-settings');
    p = await T.until('"Homepool: set up…" opens "Homepool for this browser", every network checked', (p) => D(p).panel === 'homepool' && H(p).nets.find((n) => n.name === 'Home line')?.on);
    await T.snap('desktop', 'drawer-homepool-setup');
    for (const n of H(p).nets.filter((n) => n.on)) await T.act('hp-net', n.id);
    await T.act('hp-save');
    await T.until('…none checked: "Choose at least one network"', (p) => /at least one network/.test(p.toast || ''), 5000);
    await T.act('hp-net', hid);
    await T.act('hp-save');
    p = await T.until('Make the PC browser key: the key is made, its network offered', (p) => H(p).key && H(p).offered.some((n) => n.id === hid), 10000);
    await T.snap('desktop', 'drawer-homepool');
    await T.act('panel-close');
    await T.act('pop', 'net');
    await T.act('pick', `hp:${hid}`);
    p = await T.until('a tab on the homepool network: no exit up, its badge says so', (p) => D(p).tabs.find((x) => x.id === first)?.network === `hp:${hid}` && H(p).polling && !H(p).offered.find((n) => n.id === hid)?.exit, 10000);
    await T.act('pop', 'net');
    await T.act('pick', 'client');
    await T.until('…back on this PC', (p) => D(p).tabs.find((x) => x.id === first)?.network === 'client');
  }

  // The width: dragged on the split, kept for the next launch (VD st.width).
  {
    const p0 = await T.state();
    const [x0] = D(p0).root || [0, 0];
    const w0 = D(p0).width;
    await T.mouse('drag', x0 - 3, 500, { toX: x0 - 83, toY: 500 });
    const p1 = await T.until('dragging the split widens the drawer', (p) => Math.abs(D(p).width - (w0 + 80)) < 4, 5000);
    await t.sleep(1200);
    let kept = null;
    try { kept = JSON.parse(readFileSync(join(T.configDir, 'desktop-drawer.json'), 'utf8')).width; } catch { /* not yet */ }
    t.check('…and the width is kept for the next launch', p1 && Math.abs(kept - D(p1).width) < 1, { kept, width: p1 && D(p1).width });
    const [x1] = D(await T.state()).root || [0, 0];
    await T.mouse('drag', x1 - 3, 500, { toX: x0 - 3, toY: 500 });
    await T.until('…dragged back', (p) => Math.abs(D(p).width - w0) < 8, 5000);
  }

  // Annotating: the annotation bar, a leaf of its own.
  await T.act('annotate', 'true');
  p = await T.until('Annotate shows the annotation bar', (p) => D(p).annotating && (p.renders || {})['drawer-ann'] > 0);
  await T.act('tool', 'box');
  await T.until('…a tool picked', (p) => D(p).tool === 'box');
  await T.act('tool', 'pick');
  await T.act('annotate', 'false');
  await T.until('Done hides it', (p) => !D(p).annotating);

  // ─── v2: a page's frames re-render the page alone ───
  const anim = (await T.act('new-tab', `${PAGE}/anim`)).id;
  p = await T.until('an animating page in a tab, sending frames', (p) => (D(p).tabs.find((x) => x.id === anim)?.frames || 0) > 5, 30000);
  await t.sleep(500);
  {
    const a = await T.state();
    const fa = D(a).tabs.find((x) => x.id === anim).frames;
    await t.sleep(1500);
    const b = await T.state();
    const fb = D(b).tabs.find((x) => x.id === anim).frames;
    const d = rendersSince(a, b);
    t.check('…the page re-renders with its frames', d.page > 5 && fb > fa + 5, { d, fa, fb });
    t.check('…and no bar of the drawer re-renders', BARS.every((k) => !d[k]), d);
  }
  await T.act('close-tab', anim);
  await T.until('…its tab closed', (p) => !D(p).tabs.some((x) => x.id === anim));

  // ─── v2: idle with the drawer open on a still page draws nothing ───
  // (Another tab than the workbench, whose empty "New terminal" launcher animates by design.)
  await T.act('activate', first);
  await T.ui('tab', 'browsers');
  await T.until('the drawer open beside another tab, on a still page', (p) => p.tab === 'browsers' && D(p).open && T.D(p).drawerActive === panelOf(T, p, first));
  await t.sleep(2500);
  {
    const a = await T.state();
    const m = T.frames.mark();
    await t.sleep(3000);
    const b = await T.state();
    const f = T.frames.since(m);
    const d = rendersSince(a, b);
    t.check('idle, the drawer open on a still page: no frames', f.frames === 0, { frames: f, renders: d });
    t.check('…no view re-renders', Object.values(d).every((n) => n === 0), d);
    t.check('…and the area\'s pump sleeps', T.D(b).pumps - T.D(a).pumps <= 1, { before: T.D(a).pumps, after: T.D(b).pumps, why: (T.D(b).pumpWhy || []).slice(-Math.max(1, T.D(b).pumps - T.D(a).pumps)) });
  }
  await T.ui('tab', 'workbench');
}
