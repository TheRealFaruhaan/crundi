// Not visible: hidden at once, freed after the unload grace, loaded again
// when visible; then the browser host's idle stop (native-gpui/test/
// desktop.mjs "Not visible…" and "A background drawer tab…"). Short graces:
// CRUNDI_BROWSER_UNLOAD_SECS 3, CRUNDI_BROWSER_IDLE_SECS 4.
//
// (native-gpui's first half, a chat's embed scrolled off the log, waits for
// v2's chat embeds; the drawer's half is here.) v2's own: the rule runs at
// its deadlines, never on a timer: nothing pumps while nothing falls due.

import { pageServer, dr, relaunch } from './drawer.mjs';

export async function run(T, t) {
  const PAGE = await pageServer();
  const D = (p) => dr(T, p);
  const BH = (p) => T.D(p).browserHost;
  await T.act('drawer', 'false');
  await relaunch(T, t, { CRUNDI_BROWSER_IDLE_SECS: '4', CRUNDI_BROWSER_UNLOAD_SECS: '3' });

  // ─── A background drawer tab: the same rule ───
  await T.act('drawer', 'true');
  const t1 = (await T.act('new-tab', `${PAGE}/anim`)).id;
  const tabOf = (p, id) => D(p).tabs.find((x) => x.id === id);
  // (Long: the first page after a start waits on Chromium's keyring look-up under dbus-run-session.)
  let p = await T.until('a drawer tab, shown, painting', (p) => tabOf(p, t1)?.panel != null && tabOf(p, t1).frames > 3 && !tabOf(p, t1).paused, 60000);
  const t2 = (await T.act('new-tab', `${PAGE}/two`)).id;
  const hid = Date.now();
  p = await T.until('behind another tab: hidden at once (the page kept)', (p) => tabOf(p, t1)?.paused && tabOf(p, t1).panel != null, 2000);
  p = await T.until('…freed after the unload grace, still listed', (p) => tabOf(p, t1) && tabOf(p, t1).panel == null && tabOf(p, t1).unloaded, 10000);
  t.check('…not before it', Date.now() - hid >= 2500, { ms: Date.now() - hid });
  await T.act('activate', t1);
  p = await T.until('…clicked: it loads again', (p) => tabOf(p, t1)?.panel != null && tabOf(p, t1).frames > 0 && !tabOf(p, t1).paused, 30000);
  t.check('…and the tab that went behind is hidden now', tabOf(p, t2)?.paused === true || tabOf(p, t2)?.panel == null, p && tabOf(p, t2));
  await T.act('drawer', 'false');
  p = await T.until('the drawer closed: every tab freed after the grace, listed', (p) => D(p).tabs.every((x) => x.panel == null) && tabOf(p, t1) && tabOf(p, t2), 12000);
  p = await T.until('…then the host stops after the idle grace', (p) => BH(p).state === 'off' && BH(p).views === 0, 15000);
  {
    // Nothing left to fall due: the area's pump sleeps (no per-second tick).
    await t.sleep(500);
    const a = await T.state();
    await t.sleep(3000);
    const b = await T.state();
    t.check('…and nothing ticks: no pump in 3 s', T.D(b).pumps === T.D(a).pumps, { before: T.D(a).pumps, after: T.D(b).pumps });
  }
  await T.act('drawer', 'true');
  p = await T.until('the drawer opened again: only the active tab loads', (p) => tabOf(p, t1)?.panel != null && tabOf(p, t2)?.panel == null && BH(p).state === 'up', 30000);
  await T.act('close-tab', t1);
  await T.act('close-tab', t2);
  await T.act('drawer', 'false');
}
