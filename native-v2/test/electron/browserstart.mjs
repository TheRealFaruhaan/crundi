// ─── The first browser start does not freeze the window ───
// The browser host (CEF) starts lazily, on the first page, off the main
// thread: while it starts (slowed here to 3 s, as a cold disk or a virus
// scan would) the window answers real keys within 200 ms, and the drawer
// tab shows its page loading (the Reload arrow turns, .brd-loading). The
// whole drawer shows the "browser booting" overlay meanwhile ("Starting the
// browser…", after 10 s "This can take a moment the first time."), gone the
// moment the host is up.
// A frame made before the host is up (What's new's step page, FINDINGS 2)
// waits in the link and loads once the host connects; so does one made
// while the host is still stopping (it is started afresh).
import { mcpOf } from '../desktop/drawer.mjs';

const DELAY = 3000;

export function setup() { process.env.CRUNDI_TEST_HOST_START_DELAY_MS ||= String(DELAY); }

export async function run(T, t) {
  const D = T.D;
  const mcp = mcpOf(T);
  const host = async () => (await T.state()).areas?.desktop?.browserHost || {};
  t.check('no browser host yet', (await host()).state === 'off', await host());
  // This first start slowed past 10 s (the overlay's second line).
  await T.act('browser-host-delay', '12500');
  // The drawer first: the run's own fresh CEF profile (electron.mjs), so this
  // is the host's cold first start, pictured while it starts and once shown.
  const opened = mcp('browser_drawer_open', { url: T.SERVER });
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
  await T.act('drawer', 'true');
  await T.until('…the drawer shown while the host starts', (p) => D(p).drawer?.open && D(p).browserHost?.state === 'starting', 3000);
  await T.until('…the "browser booting" overlay over the whole drawer', (p) => D(p).drawer?.boot === 'starting', 3000);
  t.check('…not yet its second line', D(await T.state()).browserHost?.boot === 'starting');
  await T.snap('electron', 'browser-first-start-loading');
  {
    const a = Date.now();
    await T.key('Tab');
    t.check('…the window still answers a key under the overlay', Date.now() - a < 200, Date.now() - a);
  }
  await T.until('…after 10 s: "This can take a moment the first time."', (p) => D(p).drawer?.boot === 'slow' && D(p).browserHost?.state === 'starting', 12000);
  await T.snap('electron', 'browser-first-start-slow');
  await T.until('the host comes up', (p) => D(p).browserHost?.state === 'up' && D(p).browserHost?.pid > 0, 20000);
  await T.until('…the overlay gone the moment it is up', (p) => D(p).drawer?.boot === '' && D(p).browserHost?.boot === null, 2000);
  const r = await opened;
  t.check('…and the tab opens', r.ok && /^drawer:/.test(r.key || ''), r);
  await T.until('…its page loaded: no longer loading', (p) => (D(p).drawer?.tabs || []).at(-1)?.loading === false, 10000);
  await t.sleep(1000);
  await T.snap('electron', 'browser-first-start-loaded');
  await T.act('drawer', 'false');
  await T.act('browser-host-stop');
  await T.until('the host stopped (What\'s new below starts it)', (p) => D(p).browserHost?.state === 'off', 10000);
  await whatsNew(T, t, host);
}

/** What's new opened with the host off, then with it stopping: its step's
 *  page is painted either way, from the server. */
async function whatsNew(T, t, host) {
  const W = (p) => p.areas?.ops?.extras?.wn;
  const base = T.SERVER.replace(/\/$/, '');
  const page = (p) => W(p)?.page;
  const loaded = (p) => !!page(p)?.painted && !page(p).failed && page(p).url === base + W(p).pagePath;
  await T.ops('extras-wn-open', 'true');
  await T.until('What\'s new opens with the host off: the host starts for its page', (p) => !!W(p) && D0(p).browserHost?.state === 'starting', 10000);
  const early = await T.state();
  t.check('…its page made before the host is up, not painted yet', !!page(early) && !page(early).painted, page(early));
  const why = async () => { const s = await T.state(); console.log('     ', JSON.stringify({ wn: W(s), views: s.areas?.ops?.extras?.wnViews, host: D0(s).browserHost })); };
  if (!(await T.until('…and the page loads once the host connects', loaded, 20000))) await why();
  await T.ops('extras-wn-close');
  await T.until('…closed', (p) => !W(p), 5000);
  // Stopped, and opened again at once, while the old host is still leaving.
  await T.act('browser-host-stop');
  const st = (await host()).state;
  await T.ops('extras-wn-open', 'true');
  t.check('What\'s new opened again while the host was stopping', st === 'stopping', st);
  if (!(await T.until('…a fresh host, and the page loads', (p) => loaded(p) && D0(p).browserHost?.state === 'up', 20000))) await why();
  await T.ops('extras-wn-close');
  await T.until('…closed', (p) => !W(p), 5000);
  await T.act('browser-host-stop');
  await T.until('the host stopped again (the drawer below starts it)', (p) => D0(p).browserHost?.state === 'off', 10000);
}
const D0 = (s) => s.areas?.desktop || {};
