#!/usr/bin/env node
// The operations area (AREAS.md), end to end against a fresh test server,
// ported from native-gpui/test/operations.mjs: every screen of the area,
// each section in test/operations/<screen>.mjs (run(T, t)). ONLY=services,vault
// runs some.
//
//   native-gpui/tools/test-server.sh 4880 --bare
//   native-v2/tools/build.sh
//   VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json CRUNDI_TEST_SERVER=http://localhost:4880 CRUNDI_TEST_PORT=4881 \
//     NATIVE_BIN=$HOME/.cache/crundi-native-target/debug/crundi-v2 \
//     xvfb-run -a -s "-screen 0 1440x900x24" node native-v2/test/operations.mjs
//   kill $(cat /tmp/crundi-devtest-4880/server.pid)
//
// Each section ends with an idle check: nothing animating, no frames.

import * as t from './lib.mjs';

const SECTIONS = ['services', 'docker', 'homepool', 'vault', 'extras', 'info', 'settings', 'skills', 'panels'];
const ONLY = (process.env.ONLY || '').split(',').filter(Boolean);

const T = await t.begin('operations');
// native-gpui's suites' names for the driver: act ("ops:<name>"), props and
// mine (the state; the area's own under areas.ops), snap (a picture in
// screens/operations), dialog and menuPick (the ask layer).
const withNames = (s) => ({ ...s, currentTab: s.tab, currentProject: s.project });
T.act = (n, ...args) => T.ui(`ops:${n}`, ...args.map(String));
T.props = async () => withNames(await T.state());
T.mine = async () => (await T.state()).areas?.ops;
const until = T.until;
T.until = (what, test, ms) => until(what, (s) => test(withNames(s)), ms);
const snap = T.snap;
T.snap = (file) => snap('operations', file);
T.dialog = (ok, value = '') => T.ui('dialog-answer', ok ? 'true' : 'false', value);
T.menuPick = (id) => T.ui('menu-pick', id);

/** No frames for a while with nothing animating (rule 6): the toasts
 *  cleared first (a toast fades out), then 3 s with no frame and no view
 *  of the window rendered. */
async function idle(name, live = null) {
  await T.ui('toasts-clear');
  await t.sleep(1500);
  const r0 = (await T.state()).renders || {};
  const m = T.frames.mark();
  await t.sleep(3000);
  const f = T.frames.since(m);
  const r1 = (await T.state()).renders || {};
  const drawn = Object.keys(r1).filter((k) => r1[k] !== (r0[k] ?? 0));
  // The shell's bars redraw on the server's own pushes (the top bar's usage):
  // such a frame is not this area's (the tab's layout view re-runs with the
  // shell). Anything of the area drawing, or more frames than those, fails.
  const SHELL = ['top-bar', 'sidebar', 'project-bar'];
  const shellDrew = SHELL.reduce((n, k) => n + ((r1[k] ?? 0) - (r0[k] ?? 0)), 0);
  const mine = drawn.filter((k) => !SHELL.includes(k) && k !== 'ops-tab');
  if (!live) return t.check(`${name}: idle draws no frames`, mine.length === 0 && f.frames <= shellDrew, { f, drawn });
  // A screen of live figures (the web polls them too): it redraws only when
  // a poll brings new ones, at most once per poll, and nothing else draws.
  t.check(`${name}: still, it redraws only for new figures (at most once per ${live.every / 1000} s poll)`,
    f.frames <= Math.ceil(3000 / live.every) + shellDrew && mine.every((k) => k === live.view), { f, drawn });
}

for (const name of SECTIONS) {
  if (ONLY.length && !ONLY.includes(name)) continue;
  console.log(`\n── ${name} ──`);
  const mod = await import(`./operations/${name}.mjs`);
  try {
    await mod.run(T, t);
  } catch (e) {
    t.check(`${name}: ran to the end`, false, String(e?.stack || e));
  }
  // Leave the window as each section found it: no dialog, the workbench
  // (whose New terminal launcher animates by the owner's call: no idle check there).
  await T.dialog(false);
  await T.ui('tab', 'workbench');
}

// Every operations tab on show, still: no frames (a poll that finds nothing
// new draws nothing; nothing animates).
if (!ONLY.length || ONLY.includes('idle')) {
  console.log('\n── idle tabs ──');
  for (const tab of ['panels', 'services', 'docker', 'homepool', 'info', 'secrets', 'settings']) {
    await T.ui('tab', tab);
    await t.sleep(1500);
    // Info's machine figures (2 s) and Docker's container list (3 s) are live.
    const live = { info: { every: 2000, view: 'info' }, docker: { every: 3000, view: 'docker' } }[tab] || null;
    await idle(`the ${tab} tab`, live);
  }
  await T.ui('tab', 'workbench');
}
await T.end();
