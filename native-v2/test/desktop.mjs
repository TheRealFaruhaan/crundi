#!/usr/bin/env node
// The desktop area (AREAS.md) end to end against a fresh test server, ported
// from native-gpui/test/desktop.mjs: the browser drawer (tabs per project,
// the address bar, devices, networks, annotations sent to a chat, history
// and clearing, saved passwords), the native host's browser protocol on
// drawer tabs and hidden browsers, the Browsers tab, the Inbox,
// notifications, the tray, the homepool node, Cru (its
// screen, live screen, kid mode, "Hey Cru"), the unload rule and the
// browser host process. Each part's checks are in test/desktop/<part>.mjs
// (run(T, t)); ONLY=drawer,host,… runs some.
//
// The browser host must sit beside the app with CEF's files
// (native-v2/tools/stage-browser.sh). The tray and notifications speak
// D-Bus: run in a session bus of the run's own (dbus-run-session), never
// the desktop's, so nothing reaches the person's own tray or notifications.
//
//   native-gpui/tools/test-server.sh 4880 --bare
//   native-v2/tools/build.sh && native-v2/tools/stage-browser.sh
//   VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json CRUNDI_TEST_SERVER=http://localhost:4880 CRUNDI_TEST_PORT=4881 \
//     NATIVE_BIN=$HOME/.cache/crundi-native-target/debug/crundi-v2 \
//     dbus-run-session -- xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/desktop.mjs
//   kill $(cat /tmp/crundi-devtest-4880/server.pid)

import * as t from './lib.mjs';
import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// In this order: the drawer first (it opens the browser host), the host
// process last (it kills the app).
const PARTS = ['drawer', 'host', 'passwords', 'browsers', 'inbox', 'tray', 'lock', 'node', 'cru', 'cruscreen', 'kid', 'wake', 'unload', 'hostproc'];
const ONLY = (process.env.ONLY || '').split(',').filter(Boolean);

const bus = process.env.DBUS_SESSION_BUS_ADDRESS || '';
if (/^unix:path=\/run\/user\/\d+\/bus$/.test(bus)) {
  console.error('Run under dbus-run-session: this is the desktop\'s own session bus (the tray and notifications would reach it).');
  process.exit(2);
}
// The autostart entry is never the person's: a config home of the run's own.
// (test/lib.mjs makes it: XDG_CONFIG_HOME, APPDATA.)

// "Hey Cru"'s models: downloaded once into a folder kept between runs (else
// the wake part checks the off state only, never downloading each run).
if (existsSync('/tmp/vw-install/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01')) process.env.CRUNDI_WAKE_MODELS ||= '/tmp/vw-install';
// CEF's profile of the run's own (a shared one makes CEF refuse to start:
// "Opening in existing browser session"), and saved passwords' key given
// (honoured only with the driver on), so no keyring is asked.
const cef = process.env.CRUNDI_CEF_CACHE ? null : mkdtempSync(join(tmpdir(), 'crundi-v2-desktop-cef-'));
if (cef) process.env.CRUNDI_CEF_CACHE = cef;
// The run's own folders go however it ends (T.end() exits the process, F71).
const dropFolders = () => { try { if (cef) rmSync(cef, { recursive: true, force: true }); } catch { /* gone */ } };
process.on('exit', dropFolders);
process.env.CRUNDI_PW_TEST_KEY ||= Array.from({ length: 32 }, () => Math.floor(Math.random() * 256).toString(16).padStart(2, '0')).join('');
const T = await t.begin('desktop');
// native-gpui's suites' names for the driver: act (a desktop action), props (the state).
T.act = (n, ...args) => T.ui(`desktop:${n}`, ...args.map(String));
T.props = T.state;
T.D = (s) => s.areas?.desktop || {};
for (const p of PARTS) {
  if (ONLY.length && !ONLY.includes(p)) continue;
  console.log(`\n── ${p} ──`);
  const m = await import(`./desktop/${p}.mjs`);
  await m.run(T, t);
}
await T.end();
