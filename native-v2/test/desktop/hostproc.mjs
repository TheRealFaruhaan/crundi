// The browser host: Chromium in a process of its own (webview's client.rs,
// crates/browser-host): lazy start, relaunch, frames only for what is on
// screen, crash and Reload, idle stop, a stale host reaped, the app killed
// (native-gpui/test/desktop.mjs "The browser host…"). It ends the run: the
// app is killed last.

import { readFileSync, writeFileSync, rmSync, readdirSync, symlinkSync, lstatSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { pageServer, dr, relaunch } from './drawer.mjs';

export async function run(T, t) {
  const PAGE = await pageServer();
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
  const BH = (p) => T.D(p).browserHost;
  const tab = (p, id) => dr(T, p).tabs.find((x) => x.id === id);

  // Tabs to remember: two in this project, the first active.
  let p = await T.state();
  if (dr(T, p).tabs.length < 2) {
    await T.act('new-tab', `${PAGE}/one`);
    await T.act('new-tab', `${PAGE}/two`);
  }
  p = await T.until('two tabs to remember', (p) => dr(T, p).tabs.length >= 2);
  const [ta, tb] = [dr(T, p).tabs[0].id, dr(T, p).tabs[1].id];
  await T.act('activate', ta);
  await T.act('hide');
  await T.until('the drawer closed, the first tab active', (p) => !dr(T, p).open && dr(T, p).active === ta);

  // A relaunch: no host, the tabs listed but not loaded.
  await relaunch(T, t, { CRUNDI_BROWSER_IDLE_SECS: '4', CRUNDI_BROWSER_UNLOAD_SECS: '8' });
  p = await T.state();
  t.check('no browser host process at launch', BH(p).state === 'off' && hostsUnder(T.app.pid).length === 0, { host: BH(p), under: tree(T.app.pid).map(cmd) });
  // Only the browser host links CEF: the app never maps libcef.
  const libcef = (pid) => { try { return readFileSync(`/proc/${pid}/maps`, 'utf8').split('\n').some((l) => /libcef/.test(l)); } catch { return null; } };
  t.check('the idle app has no libcef mapped (/proc/<pid>/maps)', libcef(T.app.pid) === false && BH(p).libcef === false, { maps: libcef(T.app.pid), props: BH(p).libcef });
  t.check('after a relaunch the tabs are listed, none loaded, the drawer closed', !dr(T, p).open && dr(T, p).tabs.length >= 2 && dr(T, p).tabs.every((x) => x.panel == null), dr(T, p).tabs);
  await t.sleep(1500);
  t.check('…and the host still does not start by itself', hostsUnder(T.app.pid).length === 0 && BH(await T.state()).state === 'off');

  // The drawer opens: the host starts, only the active tab loads.
  await T.act('drawer', 'true');
  p = await T.until('the drawer opens: the browser host starts', (p) => BH(p).state === 'up' && BH(p).pid > 0, 30000);
  const host1 = BH(p).pid;
  t.check('…as a child process of the app', hostsUnder(T.app.pid).includes(host1), { host1, under: hostsUnder(T.app.pid) });
  // (Long: the first page after a start waits on Chromium's keyring look-up under dbus-run-session.)
  p = await T.until('…and the active tab loads', (p) => tab(p, ta)?.panel != null && /page \//.test(tab(p, ta)?.title || ''), 60000);
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
    const a = fr(await T.state());
    await t.sleep(1500);
    const b = await T.state();
    t.check('…shown, it sends frames all the time', fr(b) > a + 5 && !tab(b, anim)?.paused, { before: a, after: fr(b) });
  }
  await T.act('activate', ta);
  p = await T.until('…behind another tab it is hidden in the host', (p) => tab(p, anim)?.paused, 5000);
  await t.sleep(600);
  let f0 = fr(await T.state());
  const sent0 = BH(await T.state()).frames;
  await t.sleep(2500);
  p = await T.state();
  t.check('…and sends no frames while hidden', fr(p) === f0, { before: f0, after: fr(p) });
  t.check('…(the active tab, a still page, sends none either: nothing at all comes)', BH(p).frames === sent0, { before: sent0, after: BH(p).frames });
  await T.act('activate', anim);
  p = await T.until('…shown again, it draws again at once', (p) => !tab(p, anim)?.paused && fr(p) > f0 + 3, 10000);
  p = await T.until('…the host reports what it sent (frames, bytes copied into segments)', (p) => BH(p).host?.sent > 0 && BH(p).host?.copied > 0, 6000);
  await T.act('drawer', 'false');
  p = await T.until('the drawer closed: its pages are hidden', (p) => !dr(T, p).open && tab(p, anim)?.paused, 5000);
  await t.sleep(300);
  f0 = fr(await T.state());
  await t.sleep(2000);
  p = await T.state();
  // (Within the idle grace, 4 s here: after it the pages are let go, which is no frames as well.)
  t.check('…and send no frames', fr(p) === f0 || tab(p, anim)?.panel == null, { before: f0, after: fr(p), tab: tab(p, anim) });
  await T.act('drawer', 'true');
  p = await T.until('the drawer again: the animating tab draws again', (p) => dr(T, p).open && !tab(p, anim)?.paused && fr(p) > 3, 30000);
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
  await T.snap('desktop', 'drawer-crashed');
  await T.act('reload', ta);
  p = await T.until('Reload starts a new host and loads the page again', (p) => BH(p).state === 'up' && BH(p).pid !== host1 && !tab(p, ta)?.crashed && /page \//.test(tab(p, ta)?.title || ''), 30000);
  t.check('…only the old host is gone', !live(host1));

  // The drawer closed: its pages go after the grace, then the host stops.
  await T.act('drawer', 'false');
  const host2 = BH(p)?.pid;
  p = await T.until('the drawer closed: its pages go after the unload grace', (p) => dr(T, p).tabs.every((x) => x.panel == null), 20000);
  p = await T.until('…and the host stops after the idle grace with no views', (p) => BH(p).state === 'off' && BH(p).views === 0, 20000);
  await T.until('…its process gone (and its helpers)', () => !live(host2) && hostsUnder(T.app.pid).length === 0 && tree(T.app.pid).every((c) => !cmd(c).some((a) => a.startsWith('--type='))), 10000);
  t.check('…the tabs still listed', dr(T, p).tabs.length >= 2);

  // A stale host from a crashed run, and a stale CEF lock: reaped at start.
  const stale = spawn(process.env.NATIVE_BIN, ['--browser-host'], { stdio: ['pipe', 'ignore', 'ignore'], detached: true });
  let staleGone = false;
  stale.on('exit', () => { staleGone = true; });
  await t.sleep(800);
  writeFileSync(`${T.configDir}/browser-host.pid`, `${stale.pid}\n`);
  // (Only in this run's own CEF profile, CRUNDI_CEF_CACHE: never a shared one.)
  const cefDir = process.env.CRUNDI_CEF_CACHE;
  const lock = cefDir ? `${cefDir}/SingletonLock` : null;
  if (lock) { try { rmSync(lock, { force: true }); } catch { /* none */ } symlinkSync('no-such-host-4194300', lock); }
  t.check('a stale host is running before the relaunch', !staleGone && live(stale.pid));
  await relaunch(T, t, { CRUNDI_BROWSER_IDLE_SECS: '4', CRUNDI_BROWSER_UNLOAD_SECS: '8' });
  p = await T.state();
  for (let i = 0; i < 40 && !staleGone; i++) await t.sleep(100);
  t.check('a stale host from a previous run is reaped at start', staleGone && BH(p).reaped === stale.pid, { gone: staleGone, host: BH(p) });
  if (lock) {
    let lockLeft = true;
    try { lstatSync(lock); } catch { lockLeft = false; }
    t.check('…and its stale CEF lock cleared', !lockLeft);
  } else console.log('(the stale CEF lock: set CRUNDI_CEF_CACHE to a profile of the run\'s own)');

  // kill -9 the app: nothing of the browser remains.
  await T.act('drawer', 'true');
  p = await T.until('the drawer again: a host with its helpers', (p) => BH(p).state === 'up' && /page \//.test(tab(p, ta)?.title || ''), 60000);
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
