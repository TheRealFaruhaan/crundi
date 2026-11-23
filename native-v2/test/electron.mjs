// What the Electron app did that the native client now does itself
// (PARITY-ELECTRON.md, package/CONTRACT.md), end to end against a fresh test
// server. Each part's checks are in test/electron/<part>.mjs: `setup(ctx, t)`
// / `before(t)` before the app starts (its environment), `run(T, t, ctx)`
// after it signed in. ONLY=updates,… runs some.
//
//   local      local mode (electron/local.mjs, its own launches of the
//              all-in-one and its own server: no test server needed, a run
//              of its own first): Electron's settings imported, the setup
//              wizard, startBot's server, the tray's Start and Stop, a crash,
//              Restart, the server screen, the worker, Quit (LOCAL_PORT)
//   updates    the updater: channels, the check 8 s after launch, the
//              prompts, the download, the tray item
//   lifecycle  launch at startup (Electron's names), crundi.log, a second
//              launch showing the first, the system-wide lock keys (real
//              key presses through XTEST, another window holding the focus)
//   window     the main window (least size, frameless, icon), the top
//              bar's buttons clicked (a spy on the root window), × to the
//              tray, the tray icon's click and tooltip
//   gaps       the remaining parity lines: the native host's registration
//              and focus, browser_touch's listeners, drawer downloads,
//              notification clicks and the raise, the homepool public IP,
//              the kid lock's watchdog, power and lock, the DisconnectRDP status
//   trial      the owner's trial findings: T5 the kid lock clicked, the
//              PIN keyed; T12 a project clicked leaves the Inbox
//   install    (needs updates) the Linux install: sudo -n refused, then
//              pkexec and its terminal offer, then sudo -n: the relaunch
//
// The browser host must sit beside the app (native-v2/tools/stage-browser.sh).
// Run under Xvfb and a session bus of the run's own, with a fresh test server:
//
//   native-gpui/tools/test-server.sh 4820 --bare
//   native-v2/tools/build.sh && native-v2/tools/stage-browser.sh
//   VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json CRUNDI_TEST_SERVER=http://localhost:4820 CRUNDI_TEST_PORT=4821 \
//     dbus-run-session -- xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/electron.mjs
//   kill $(cat /tmp/crundi-devtest-4820/server.pid)

import * as t from './lib.mjs';
import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// The order they run in: the updater first (its prompt comes 8 s after
// launch), its install last (it quits the app and starts it again).
const PARTS = ['local', 'updates', 'lifecycle', 'browserstart', 'window', 'gaps', 'trial', 'install'];
const ONLY = (process.env.ONLY || '').split(',').filter(Boolean);
const parts = PARTS.filter((p) => !ONLY.length || ONLY.includes(p) || (p === 'install' && ONLY.includes('updates')));

const bus = process.env.DBUS_SESSION_BUS_ADDRESS || '';
if (/^unix:path=\/run\/user\/\d+\/bus$/.test(bus)) {
  console.error('Run under dbus-run-session: this is the desktop\'s own session bus (the tray and notifications would reach it).');
  process.exit(2);
}
// Never the person's autostart entries or CEF profile: the config home
// test/lib.mjs gives every suite (XDG_CONFIG_HOME, APPDATA).
const ctx = { drop: [] };
if (!process.env.CRUNDI_CEF_CACHE) { const cef = mkdtempSync(join(tmpdir(), 'crundi-v2-electron-cef-')); process.env.CRUNDI_CEF_CACHE = cef; ctx.drop.push(cef); }
process.on('exit', () => { for (const d of ctx.drop) { try { rmSync(d, { recursive: true, force: true }); } catch { /* gone */ } } });
process.env.CRUNDI_PW_TEST_KEY ||= Array.from({ length: 32 }, () => Math.floor(Math.random() * 256).toString(16).padStart(2, '0')).join('');
if (!existsSync(fileURLToPath(new URL('../.bin/debug/crundi-browser-host', import.meta.url))) && !process.env.NATIVE_BIN) console.log('(no browser host in .bin/debug: run native-v2/tools/stage-browser.sh)');

// Local mode first, as a run of its own (it starts the all-in-one twice,
// with its own server, and quits it: nothing left for the parts below).
if (parts.includes('local')) {
  console.log('\n── local ──');
  const r = spawnSync(process.execPath, [fileURLToPath(new URL('./electron/local.mjs', import.meta.url))], { stdio: 'inherit', env: process.env });
  t.check('local: its run passed (electron/local.mjs)', r.status === 0, `exit ${r.status}`);
}
const rest = parts.filter((p) => p !== 'local');
if (!rest.length) { const c = t.counts(); process.exit(c.failed ? 1 : 0); }

const mods = {};
for (const p of rest) { mods[p] = await import(`./electron/${p}.mjs`); await mods[p].setup?.(ctx, t); await mods[p].before?.(t); }
const T = await t.begin('electron');
T.act = (n, ...args) => T.ui(`desktop:${n}`, ...args.map(String));
T.ops = (n, ...args) => T.ui(`ops:${n}`, ...args.map(String));
T.D = (s) => s.areas?.desktop || {};
for (const p of rest) {
  console.log(`\n── ${p} ──`);
  try { await mods[p].run(T, t, ctx); } catch (e) { t.check(`${p}: ran to the end`, false, String(e?.stack || e)); }
}
await T.end();
