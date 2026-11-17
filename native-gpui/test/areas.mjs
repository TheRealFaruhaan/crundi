#!/usr/bin/env node
// The foundation (AREAS.md), end to end: every tab reaches its area through
// the tab host, a workbench panel embeds its area's view, each area answers
// the driver, and the shared confirm, prompt and context menu work.
//
//   native-gpui/tools/test-server.sh 4398 --bare
//   native-gpui/tools/build.sh; BIN=$HOME/.cache/crundi-native-target/debug/crundi-gpui
//   NATIVE_BIN=$BIN xvfb-run -a -s "-screen 0 1440x900x24" node native-gpui/test/areas.mjs

import * as t from './lib.mjs';

const T = await t.begin('lead');

// Every tab: its area (ui/areas.slint), shown and heard by Rust.
const TABS = { git: 'workspace', files: 'workspace', media: 'workspace', kanban: 'planning', notes: 'planning', mindmap: 'planning', factories: 'planning', schedule: 'planning', panels: 'ops', services: 'ops', docker: 'ops', homepool: 'ops', info: 'ops', secrets: 'ops', settings: 'ops', browsers: 'desktop' };
for (const tab of Object.keys(TABS)) {
  await T.ui('tab', tab);
  await T.until(`the ${tab} tab shows`, (p) => p.currentTab === tab, 5000);
}
await T.snap('tab-last');
await T.ui('tab', 'workbench');

// Each area answers its own driver actions and has its props.
for (const a of ['workspace', 'planning', 'ops', 'desktop']) {
  const r = await T.send({ cmd: 'ui', name: `${a}:ping`, args: [] });
  t.check(`${a} answers the driver`, r.ok && r.area === a, r);
  const p = await T.send({ cmd: 'area', area: a });
  t.check(`${a} has props`, p.ok && p.props && typeof p.props === 'object', p);
}
const bad = await T.send({ cmd: 'ui', name: 'nowhere:ping', args: [] });
t.check('an unknown area is refused', bad.ok === false, bad);

// A panel in a workbench pane: its area's view, embedded.
await T.ui('add-pane', 'kanban');
await T.until('a Kanban panel in the workbench', (p) => p.panes.some((x) => x.kind === 'panel' && x.key === 'panel:kanban'));
await T.snap('panel-in-pane');

// The shared confirm: Cancel, then OK.
await T.ui('kit-confirm', 'Delete the card?', 'It goes for good.');
await T.until('the confirm opens', (p) => p.dialog?.title === 'Delete the card?' && p.dialog.danger && p.pointerModal);
await T.snap('confirm');
await T.dialog(false);
await T.until('Cancel answers no', (p) => !p.dialog && p.kitLast?.confirm === false);
await T.ui('kit-confirm', 'Again?', '');
await T.until('the confirm opens again', (p) => p.dialog?.title === 'Again?');
await T.key('Return');
await T.until('Enter answers yes', (p) => !p.dialog && p.kitLast?.confirm === true);

// The shared prompt: typed text comes back.
await T.ui('kit-prompt', 'Rename', 'Old name');
await T.until('the prompt opens with its value', (p) => p.dialog?.prompt && p.dialog.value === 'Old name');
await T.dialog(true, 'New name');
await T.until('OK gives the text', (p) => p.kitLast?.prompt === 'New name');
await T.ui('kit-prompt', 'Rename', 'x');
await T.until('the prompt opens', (p) => p.dialog?.prompt);
await T.key('Escape');
await T.until('Esc cancels the prompt', (p) => !p.dialog && p.kitLast?.prompt === null);

// The shared context menu.
await T.ui('kit-menu', '300', '200');
await T.until('the menu opens', (p) => p.menu?.items?.length === 2 && p.menu.items[1].danger);
await T.snap('menu');
await T.menuPick('delete');
await T.until('a pick answers with its id', (p) => !p.menu && p.kitLast?.menu === 'delete');
await T.ui('kit-menu', '300', '200');
await T.until('the menu opens again', (p) => !!p.menu);
await T.key('Escape');
await T.until('Esc closes the menu', (p) => !p.menu);

// The kit's gallery: every shared part in its states.
await T.ui('kit-gallery', 'true');
await T.snap('kit-gallery');
await T.ui('kit-gallery', 'false');

await T.end();
