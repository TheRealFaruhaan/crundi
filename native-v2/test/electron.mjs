#!/usr/bin/env node
// What the Electron app did that the native client now does itself
// (PARITY-ELECTRON.md), end to end against a fresh test server. Each part's
// checks are in test/electron/<part>.mjs: `setup(ctx)` before the app starts
// (its environment), `run(T, t, ctx)` after it signed in. ONLY=updates,… runs some.
//
//   updates    the updater: channels, the check 8 s after launch, the
//              prompts, the download, the tray item
//   install    (needs updates) the Linux install: sudo -n refused, then
//              pkexec and its terminal offer, then sudo -n: the relaunch
//   lifecycle  launch at startup (Electron's names), crundi.log, a second
//              launch showing the first, the system-wide lock keys (real
//              key presses through XTEST, another window holding the focus)
//
// Run under Xvfb and a session bus of the run's own, with a fresh test server:
//
//   native-gpui/tools/test-server.sh 4820 --bare
//   native-v2/tools/build.sh
//   VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json CRUNDI_TEST_SERVER=http://localhost:4820 CRUNDI_TEST_PORT=4821 \
//     dbus-run-session -- xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/electron.mjs
//   kill $(cat /tmp/crundi-devtest-4820/server.pid)

import * as t from './lib.mjs';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// The order they run in: the updater first (its prompt comes 8 s after
// launch), its install last (it quits the app and starts it again).
const PARTS = ['updates', 'lifecycle', 'install'];
const ONLY = (process.env.ONLY || '').split(',').filter(Boolean);
const parts = PARTS.filter((p) => !ONLY.length || ONLY.includes(p) || (p === 'install' && ONLY.includes('updates')));

const bus = process.env.DBUS_SESSION_BUS_ADDRESS || '';
if (/^unix:path=\/run\/user\/\d+\/bus$/.test(bus)) {
  console.error('Run under dbus-run-session: this is the desktop\'s own session bus.');
  process.exit(2);
}
// The autostart entry is never the person's: a config home of the run's own.
const xdg = mkdtempSync(join(tmpdir(), 'crundi-v2-electron-xdg-'));
process.env.XDG_CONFIG_HOME = xdg;
const ctx = { drop: [xdg] };
process.on('exit', () => { for (const d of ctx.drop) { try { rmSync(d, { recursive: true, force: true }); } catch { /* gone */ } } });

const mods = {};
for (const p of parts) { mods[p] = await import(`./electron/${p}.mjs`); if (mods[p].setup) await mods[p].setup(ctx, t); }
const T = await t.begin('electron');
T.act = (n, ...args) => T.ui(`desktop:${n}`, ...args.map(String));
T.ops = (n, ...args) => T.ui(`ops:${n}`, ...args.map(String));
T.D = (s) => s.areas?.desktop || {};
for (const p of parts) {
  console.log(`\n── ${p} ──`);
  try { await mods[p].run(T, t, ctx); } catch (e) { t.check(`${p}: ran to the end`, false, String(e?.stack || e)); }
}
await T.end();
