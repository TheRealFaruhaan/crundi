#!/usr/bin/env node
// The planning area (AREAS.md, phase 4), end to end against a real server.
// Each screen's checks are in test/planning/<screen>.mjs (run(T, t));
// ONLY=mindmap,notes runs some of them.
//
//   native-gpui/tools/test-server.sh 4392 --bare
//   native-gpui/tools/build.sh; BIN=$HOME/.cache/crundi-native-target/debug/crundi-gpui
//   NATIVE_BIN=$BIN xvfb-run -a -s "-screen 0 1440x900x24" node native-gpui/test/planning.mjs
// (Another port pair: CRUNDI_TEST_SERVER=http://localhost:<port> CRUNDI_TEST_PORT=<driver>,
// and TMPDIR=<a dir of its own> so two clients don't share a CEF profile.)

import * as t from './lib.mjs';

const SCREENS = ['kanban', 'mindmap', 'notes', 'schedule', 'factories', 'wbscroll'];
const ONLY = (process.env.ONLY || '').split(',').filter(Boolean);

const T = await t.begin('planning');
for (const s of SCREENS) {
  if (ONLY.length && !ONLY.includes(s)) continue;
  console.log(`\n── ${s} ──`);
  const m = await import(`./planning/${s}.mjs`);
  await m.run(T, t);
}
await T.end();
