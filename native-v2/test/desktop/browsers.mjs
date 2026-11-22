// The Browsers tab (src/webapp-html.js loadBrowsers, renderBrowsers): the
// browsers chats opened with browser_open (GET /api/browsers), opened,
// screenshotted, driven and closed from here (browsers-open, -shot, -go,
// -close: its + Open, a card's screenshot box, Go and Close). Not in
// native-gpui's suite: these are new. v2's own: the page is a cached leaf
// ("browsers") under the tab's layout view, drawn from its copy: it is not
// redrawn by a drawer page's frames beside it.

import { pageServer, dr, mcpOf, rendersSince } from './drawer.mjs';

export async function run(T, t) {
  const PAGE = await pageServer();
  const B = (p) => T.D(p).browsers;
  const mcp = mcpOf(T);
  await T.until('registered as the server\'s native host', (p) => (T.D(p).host?.features || []).includes('browser-drawer'), 30000);

  await T.ui('tab', 'browsers');
  let p = await T.until('the tab shown: the list loaded', (p) => p.tab === 'browsers' && B(p).loaded && (p.renders || {}).browsers > 0);
  const before = (B(p)?.list || []).map((b) => b.key);

  // + Open: a browser at the URL, then the list loads again.
  await T.act('browsers-open', `${PAGE}/opened`);
  p = await T.until('+ Open opens one: the list shows it', (p) => B(p).list.some((b) => !before.includes(b.key) && /\/opened$/.test(b.url || '')), 60000);
  const key = (B(p)?.list || []).find((b) => !before.includes(b.key))?.key || 'none';
  await T.until('…with the toast', (p) => /Browser opened/.test(p.toast || ''), 5000);
  let r = await mcp('browser_eval', { key, code: 'document.title' });
  t.check('…it runs the page', r.ok && r.result === 'page /opened', r);

  // The screenshot box: Loading..., then the picture.
  await T.act('browsers-shot', key);
  await T.until('Screenshot takes its picture', (p) => B(p).shots[key] === 'ok', 30000);
  await T.snap('desktop', 'browsers-card');

  // Go: the card's address.
  await T.act('browsers-go', key, `${PAGE}/went`);
  let path = '';
  for (let i = 0; i < 30 && path !== '/went'; i++) { await t.sleep(300); r = await mcp('browser_eval', { key, code: 'location.pathname' }); path = r.result; }
  t.check('Go takes it to the address', path === '/went', r);

  // v2: a drawer page's frames beside the tab redraw neither this page nor a bar.
  await T.act('drawer', 'true');
  const anim = (await T.act('new-tab', `${PAGE}/anim`)).id;
  await T.until('an animating drawer page beside the tab', (p) => (dr(T, p).tabs.find((x) => x.id === anim)?.frames || 0) > 5, 60000);
  {
    const a = await T.state();
    await t.sleep(1500);
    const b = await T.state();
    const d = rendersSince(a, b);
    t.check('…the page redraws; the Browsers page does not', d.page > 5 && !d.browsers, d);
  }
  await T.act('close-tab', anim);
  await T.act('drawer', 'false');

  // Close: gone from the list.
  await T.act('browsers-close', key);
  await T.until('Close closes it: the list loads without it', (p) => !B(p).list.some((b) => b.key === key), 15000);
  await T.until('…with the toast', (p) => /Browser closed/.test(p.toast || ''), 5000);
  await T.ui('tab', 'workbench');
}
