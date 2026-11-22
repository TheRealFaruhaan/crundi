// The native host: the server's browser tools reach this client's drawer
// tabs and hidden browsers (native-gpui/test/desktop.mjs "The native host…").
// v2's own: a chat's tools on a drawer tab redraw the page, not the bars
// (only the tab's title or address moving redraws the tabs row and the
// address bar).

import { pageServer, dr, panelOf, mcpOf, rendersSince, BARS } from './drawer.mjs';

export async function run(T, t) {
  const PAGE = await pageServer();
  const D = (p) => dr(T, p);
  const mcp = mcpOf(T);

  // ─── The native host: the server's browser tools reach this client ───
  await T.until('registered as the server\'s native host', (p) => Array.isArray(T.D(p).host?.features) && T.D(p).host.features.includes('browser-drawer'), 30000);
  let r = await mcp('browser_list');
  t.check('browser_list: the drawer is available', r.ok && r.drawer === 'available', r);
  r = await mcp('browser_drawer_open', { url: `${PAGE}/host` });
  t.check('browser_drawer_open opens a drawer tab', r.ok && /^drawer:/.test(r.key || ''), r);
  const key = r.key || 'drawer:none';
  const tab = key.slice('drawer:'.length);
  await T.until('…a tab in the project\'s drawer', (p) => D(p).tabs.some((x) => x.id === tab));
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
  await T.act('activate', tab);
  await T.until('the tab shown', (p) => T.D(p).drawerActive === panelOf(T, p, tab));
  await t.sleep(500);
  {
    // A chat's pin: drawn on the page alone (the bars keep their caches).
    const a = await T.state();
    r = await mcp('browser_point', { key, selector: '#b', label: 'This one' });
    t.check('browser_point drops a pin on the element', r.ok, r);
    await t.sleep(600);
    const b = await T.state();
    const d = rendersSince(a, b);
    t.check('…the pin redraws the page, no bar', d.page > 0 && ['drawer-tabs', 'drawer-address', 'drawer-pw', 'drawer-body'].every((k) => !d[k]), d);
  }
  await T.snap('desktop', 'drawer-claude-pin');
  r = await mcp('browser_annotations', { key, image: true });
  t.check('browser_annotations reads the marks back, with the picture', r.ok && (r.annotations || []).some((a) => a.by === 'claude' && a.kind === 'pin' && a.text === 'This one') && (r.image || '').length > 1000, { ...r, image: (r.image || '').length });
  r = await mcp('browser_resize', { key, width: 800, height: 600 });
  await t.sleep(1000);
  r = await mcp('browser_eval', { key, code: 'innerWidth+"x"+innerHeight' });
  t.check('browser_resize: the tab is 800x600 (a custom size)', r.result === '800x600', r);
  await T.until('…the Device button says so', (p) => JSON.stringify(D(p).tabs.find((x) => x.id === tab)?.custom) === '[800,600]');
  {
    // A chat's mouse on the page: its cursor, drawn on the page only.
    const a = await T.state();
    r = await mcp('browser_mouse', { key, x: 60, y: 60, action: 'move' });
    await t.sleep(400);
    const b = await T.state();
    const d = rendersSince(a, b);
    t.check('a chat\'s pointer moves on the page: no bar redraws', BARS.every((k) => !d[k]), d);
  }
  r = await mcp('browser_navigate', { key, url: `${PAGE}/next` });
  t.check('browser_navigate answers once loaded', r.ok, r);
  r = await mcp('browser_eval', { key, code: 'location.pathname' });
  t.check('…on the new page', r.result === '/next', r);
  await T.until('…the tab follows (its title)', (p) => /page \/next/.test(D(p).tabs.find((x) => x.id === tab)?.title || ''));
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
  await T.until('a hidden browser, not a drawer tab', (p) => T.D(p).hidden.some((h) => h.key === hk));
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
  await T.until('the Browsers tab lists the hidden browser', (p) => T.D(p).browsers.list.some((b) => b.key === hk));
  await T.act('browsers-shot', hk);
  await T.until('…and takes its screenshot', (p) => T.D(p).browsers.shots[hk] === 'ok');
  await T.snap('desktop', 'browsers-tab');
  await T.act('browsers-close', hk);
  await T.until('Close closes it', (p) => !T.D(p).hidden.some((h) => h.key === hk), 10000);
  await T.ui('tab', 'workbench');
  await T.act('close-tab', tab);
  await T.until('(its drawer tab closed)', (p) => !D(p).tabs.some((x) => x.id === tab));
}
