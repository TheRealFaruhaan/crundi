#!/usr/bin/env node
// The planning area (AREAS.md), end to end against a fresh test server,
// ported from native-gpui/test/planning.mjs. Each screen's checks are in
// test/planning/<screen>.mjs (run(T, t)); ONLY=kanban,mindmap runs some.
//
//   native-gpui/tools/test-server.sh 4880 --bare
//   native-v2/tools/build.sh
//   VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json CRUNDI_TEST_SERVER=http://localhost:4880 CRUNDI_TEST_PORT=4881 \
//     NATIVE_BIN=$HOME/.cache/crundi-native-target/debug/crundi-v2 \
//     xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/planning.mjs
//   kill $(cat /tmp/crundi-devtest-4880/server.pid)

import * as t from './lib.mjs';

const SCREENS = ['kanban', 'kanban-look', 'mindmap', 'mindmap-look', 'schedule', 'notes', 'drops', 'factories'];
const ONLY = (process.env.ONLY || '').split(',').filter(Boolean);

const T = await t.begin('planning');
// native-gpui's suites' names for the driver: act (a ui action), props (the state).
T.act = (n, ...args) => T.ui(n, ...args.map(String));
T.props = T.state;
for (const s of SCREENS) {
  if (ONLY.length && !ONLY.includes(s)) continue;
  console.log(`\n── ${s} ──`);
  const m = await import(`./planning/${s}.mjs`);
  await m.run(T, t);
}
await T.end();
