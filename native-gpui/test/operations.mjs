#!/usr/bin/env node
// The ops area (phase 5), end to end: every screen of the area against a
// real test server, each section in test/ops/<screen>.mjs.
//
//   native-gpui/tools/test-server.sh 4394 --bare
//   native-gpui/tools/build.sh; BIN=$HOME/.cache/crundi-native-target/debug/crundi-gpui
//   NATIVE_BIN=$BIN xvfb-run -a -s "-screen 0 1440x900x24" node native-gpui/test/operations.mjs
//   kill $(cat /tmp/crundi-devtest-4394/server.pid)
//
// One section alone: ONLY=services (comma-separated names below).

import * as t from './lib.mjs';

const SECTIONS = ['services', 'docker', 'homepool', 'vault', 'extras', 'info', 'settings', 'skills', 'panels'];
const only = (process.env.ONLY || '').split(',').filter(Boolean);

const T = await t.begin('ops');
for (const name of SECTIONS) {
  if (only.length && !only.includes(name)) continue;
  console.log(`\n── ${name} ──`);
  const mod = await import(`./ops/${name}.mjs`);
  try {
    await mod.run(T, t);
  } catch (e) {
    t.check(`${name}: ran to the end`, false, String(e?.stack || e));
  }
  // Leave the window as each section found it: no dialog, the workbench.
  await T.dialog(false);
  await T.ui('tab', 'workbench');
}
await T.end();
