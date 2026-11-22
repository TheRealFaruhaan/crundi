// ─── The first browser start does not freeze the window ───
// The browser host (CEF) starts lazily, on the first page, off the main
// thread: while it starts (slowed here to 3 s, as a cold disk or a virus
// scan would) the window answers real keys within 200 ms, and the drawer
// tab shows its page loading (the Reload arrow turns, .brd-loading).
import { mcpOf } from '../desktop/drawer.mjs';

const DELAY = 3000;

export function setup() { process.env.CRUNDI_TEST_HOST_START_DELAY_MS = String(DELAY); }

export async function run(T, t) {
  const D = T.D;
  const mcp = mcpOf(T);
  const host = async () => (await T.state()).areas?.desktop?.browserHost || {};
  t.check('no browser host yet', (await host()).state === 'off', await host());
  const opened = mcp('browser_drawer_open', { url: 'about:blank' });
  await T.until('the first page: the browser host is starting', (p) => D(p).browserHost?.state === 'starting', 5000);
  // Real key presses while it starts: each answered by the window in time.
  const lat = [];
  for (let i = 0; i < 6; i++) {
    const a = Date.now();
    await T.key('Tab');
    lat.push(Date.now() - a);
    await t.sleep(150);
  }
  const mid = D(await T.state());
  t.check('…still starting while the keys were pressed', mid.browserHost?.state === 'starting', mid.browserHost);
  t.check('…and the window answered every key within 200 ms', lat.every((l) => l < 200), lat);
  const tab = (mid.drawer?.tabs || []).at(-1);
  t.check('…the drawer tab shows its page loading meanwhile', !!tab && tab.loading === true, tab);
  await T.until('the host comes up', (p) => D(p).browserHost?.state === 'up' && D(p).browserHost?.pid > 0, 20000);
  const r = await opened;
  t.check('…and the tab opens', r.ok && /^drawer:/.test(r.key || ''), r);
  await T.until('…its page loaded: no longer loading', (p) => (D(p).drawer?.tabs || []).at(-1)?.loading === false, 10000);
}
