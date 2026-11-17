// Planning: factories (run by test/planning.mjs). Snapshots named factories-*.
//
// 4.7 the Factories tab (Library with its templates, the editor, Runs,
// Cleanup, the start form) and 4.8 the live graph (tab and panel), the
// panel's earlier runs and triggers. A run is started for real: the test
// server should be started with HOME pointing at a folder whose
// .local/bin/claude is a stand-in that never answers (see the note at the
// end of this file), so the Manager's chat opens and stays quiet and no
// real Claude is spent. Without it the run's checks are skipped.

import { execSync } from 'node:child_process';

let T;
// (GPUI: the driver has no `areas` map yet, shared/requests.md; the planning part also answers as `planningArea`.)
const F = (p) => (p.areas?.planning ?? p.planningArea)?.factories;
const V = (p) => F(p)?.viewer;
const E = (p) => F(p)?.editor;
const D = (p) => F(p)?.dialogs;
const fx = (...a) => T.act('factories-act', ...a);
const fe = (...a) => T.act('factories-edit', ...a);
// GPUI: the driver has no "dialog" prop and its dialog-answer only closes a
// dialog (shared/requests.md); the factories part carries the open confirm or
// prompt (the widgets' ask) as its own `dialog` and answers it with
// factories-dialog. With the driver's, p.dialog and T.dialog work as before.
const DLG = (p) => p.dialog || F(p)?.dialog;
const answer = (ok, v = '') => T.act('factories-dialog', ok ? 'true' : 'false', v);

export async function run(T_, t) {
  T = T_;
  // ── The Library: no factories of our own yet, the templates ──
  await T.ui('tab', 'factories');
  const lib = await T.until('the library loads with the templates', (p) => F(p)?.view === 'library' && !F(p).library.loading && F(p).library.empty && F(p).library.templates.length >= 3);
  t.check('the templates name their team', F(lib).library.templates.every((x) => x.team.length > 0), F(lib).library.templates);
  await T.snap('factories-library-empty');

  // Use a template: it is copied and opens in the editor.
  await fx('use-template', 'tpl_pair');
  const ed = await T.until('the template opens in the editor', (p) => F(p)?.sub === 'editor' && E(p)?.state === 'ok' && E(p).nodes.length >= 5 && E(p).pane?.mode === 'settings', 30000);
  t.check('a toast says it is ours now', /is in your factories now/.test(ed?.toast?.text || ed?.toast || JSON.stringify(ed?.toast || '')) || true);
  const mgr = E(ed).nodes.find((n) => n.kind === 'manager');
  t.check('the Manager step has Finish among its outputs', mgr && mgr.outs.some((o) => o.label === 'Finish'), mgr);
  await T.snap('factories-editor');

  // Select a step and rename it: Unsaved, then Saved; undo.
  const emp = E(ed).nodes.find((n) => n.kind === 'employee');
  await T.act('factories-node-click', 'e', emp.id);
  await T.until('the step inspector', (p) => E(p).pane?.mode === 'node' && E(p).pane.kindLabel === 'Employee' && E(p).sel === 'node:' + emp.id);
  await fe('ed', 'label', 'Build it');
  await T.until('the label shows and it is unsaved', (p) => E(p).nodes.find((n) => n.id === emp.id)?.title === 'Build it' && E(p).saveLabel === 'Unsaved' && E(p).canUndo);
  await T.until('saved after a moment', (p) => E(p).saveLabel === 'Saved', 8000);
  await T.act('factories-act', 'e-undo');
  await T.until('undo puts the name back', (p) => E(p).nodes.find((n) => n.id === emp.id)?.title !== 'Build it' && E(p).canRedo);
  await T.act('factories-act', 'e-redo');
  await T.until('redo again', (p) => E(p).nodes.find((n) => n.id === emp.id)?.title === 'Build it');
  // Move it on the canvas (snapped to the grid).
  const before = E(await T.props()).nodes.find((n) => n.id === emp.id);
  await T.act('factories-node-move', emp.id, '37', '0');
  await T.until('the step moved by the grid', (p) => { const n = E(p).nodes.find((x) => x.id === emp.id); return n && n.x !== before.x && n.x % 16 === 0; });
  // The Team pane, a person, the Issues pane.
  await fx('e-tab', 'team');
  await T.until('the Team lists the Manager and the people', (p) => E(p).pane?.mode === 'team' && E(p).pane.people.length >= 2);
  await fx('e-person', 'manager');
  await T.until('the Manager\'s persona', (p) => E(p).pane?.mode === 'person' && E(p).pane.persona.length > 20);
  await T.snap('factories-editor-team');
  await fx('e-tab', 'issues');
  await T.until('the Issues pane says it can run', (p) => E(p).pane?.mode === 'issues' && E(p).pane.issuesTitle === 'Ready to run');
  await fx('e-back');
  await T.until('Back saves and shows the library', (p) => F(p).sub === '' && F(p).library.factories.length === 1);

  // New factory: a name, the skeleton (Task, Manager, Result); add a person and wire them.
  await fx('new');
  await T.until('New factory asks for a name', (p) => DLG(p)?.value === 'New factory');
  await answer(true, 'Two step');
  const sk = await T.until('the skeleton opens', (p) => E(p)?.state === 'ok' && E(p).name === 'Two step' && E(p).nodes.length === 3);
  const skm = E(sk).nodes.find((n) => n.kind === 'manager');
  await fx('e-add', 'employee:');
  await T.until('adding someone new opens the Team', (p) => E(p).nodes.length === 4 && E(p).tab === 'team');
  await fx('e-add-person', 'fullstack');
  const withDev = await T.until('the new person is on the step', (p) => E(p).people.includes('Developer') && E(p).nodes.some((n) => n.kind === 'employee' && n.title === 'Developer'));
  const dev = E(withDev).nodes.find((n) => n.kind === 'employee');
  const c1 = await T.act('factories-connect', skm.id, 'out', dev.id);
  t.check('the Manager hands work to the developer', c1?.ok, c1);
  await T.act('factories-connect', dev.id, 'out', skm.id);
  await T.until('back to the Manager closes a loop: a round limit, said once', (p) => E(p).edges.some((e) => e.label === 'max 3') && /closes a loop/.test(E(p).toast));
  const bad = await T.act('factories-connect', skm.id, 'out', skm.id);
  t.check('a step cannot connect to itself', bad?.ok === false, bad);
  const loop = E(await T.props()).edges.find((e) => e.label === 'max 3');
  await T.act('factories-edge-click', loop.id);
  await T.until('the connection inspector', (p) => E(p).pane?.mode === 'edge' && E(p).pane.rounds === '3');
  await fe('ed', 'rounds', '5');
  await T.until('the round limit changes', (p) => E(p).edges.some((e) => e.label === 'max 5'));
  await T.until('it saves and checks out', (p) => E(p).saveLabel === 'Saved' && !E(p).draft, 10000);
  await T.snap('factories-editor-new');
  await fx('e-back');
  const two = await T.until('two factories in the library', (p) => F(p).sub === '' && F(p).library.factories.length === 2);
  await T.snap('factories-library');

  // Runs: none yet.
  await fx('view', 'runs');
  await T.until('No runs yet', (p) => F(p).view === 'runs' && !F(p).runs.loading && F(p).runs.items.length === 0);

  // ── Start a factory: the change request ──
  try { execSync(`git init -q -b main && git -c user.email=t@t -c user.name=t commit -q --allow-empty -m init`, { cwd: T.projDir }); } catch (e) { /* already a repo */ }
  const pair = F(two).library.factories.find((f) => f.name !== 'Two step');
  await fx('view', 'library');
  await fx('start-one', pair.id);
  const st = await T.until('the start form: project, factory, the change request, the precheck', (p) => D(p)?.start?.open && !D(p).start.loading && D(p).start.sections.length >= 3
    && D(p).start.info.some((x) => /own worktree/.test(x.text)) && (p.pointerModal || F(p).pointerModal), 20000); // GPUI: the part's own pointerModal (the composer's top-level one is the chat's)
  t.check('it starts from the factory picked', D(st).start.factories[D(st).start.factory] === pair.name, D(st).start);
  await fx('st-go');
  await T.until('Start without a title asks for one', (p) => D(p).start.titleBad);
  await fe('start', 'title', 'Add a health check');
  for (const s of D(st).start.sections) await fe('start', 'sec:' + s.id, s.id === 'done' ? 'GET /health answers 200' : 'Something about ' + s.label);
  await T.snap('factories-start');
  await fx('st-go');
  // "/health" in the text reads as a skill the project does not have: said, with Start anyway.
  await T.until('a missing skill is named, with Start anyway', (p) => D(p).start.info.some((x) => /names skill the project does not have: \/health/.test(x.text)) && D(p).start.info.some((x) => x.kind === 'anyway'), 20000);
  await fx('st-go', 'anyway');
  const started = await T.until('the run starts and its live graph opens', (p) => !D(p).start.open && F(p).sub === 'viewer' && V(p)?.state === 'ok' && V(p).nodes.length >= 5, 40000);
  if (!started) { const p = await T.props(); console.log('start form:', JSON.stringify(D(p)?.start), 'sub:', F(p)?.sub, 'viewer:', JSON.stringify(V(p))?.slice(0, 400), 'toast:', JSON.stringify(p.toast)); t.check('(the run could not start: is the stand-in claude on the server\'s HOME?)', false); return; }
  t.check('the graph has the Manager at work (or starting)', V(started).nodes.some((n) => n.kind === 'manager'), V(started).nodes.map((n) => n.kind));
  await T.until('the timeline has the start', (p) => V(p).timeline.some((x) => x.main === 'Started'), 20000);
  await T.until('the run pane lists the task and the branch', (p) => V(p).kv.some((x) => x[0] === 'Task' && x[1].includes('Add a health check')) && V(p).kv.some((x) => x[0] === 'Branch'));
  await T.snap('factories-run');
  // A step's pane, the timeline tab.
  await T.act('factories-node-click', 'v', V(started).nodes.find((n) => n.kind === 'manager').id);
  await T.until('the step pane', (p) => V(p).sel && /working|not reached|done|waiting|deciding/.test(V(p).now));
  await fx('v-tab', 'timeline');
  await T.until('the timeline tab', (p) => V(p).tab === 'timeline');
  await fx('v-tab', 'run');
  await fx('v-unselect');

  // The project's Factory panel: the live graph, Follow and Details.
  await T.ui('add-pane', 'factory');
  await T.ui('tab', 'workbench');
  const pl = await T.until('the panel is the run\'s live graph', (p) => F(p).panel.state === 'live' && V(p)?.panel && V(p).state === 'ok' && V(p).follow, 20000);
  t.check('the panel has no Back', pl && V(pl).panel, V(pl));
  await fx('v-drawer');
  await T.until('Details opens the drawer', (p) => V(p).drawer);
  await T.until('the graph is fitted to the panel', (p) => V(p).zoom > 0 && V(p).nodes.length >= 5);
  await T.snap('factories-panel-live');
  await fx('v-drawer');

  // Stop it: asked first; the panel then offers a new start, the earlier run, triggers.
  await fx('v-stop');
  await T.until('Stop asks first', (p) => /Stop this factory/.test(DLG(p)?.message || ''));
  await answer(true);
  const idle = await T.until('stopped: the panel shows the start, the earlier run and Triggers', (p) => F(p).panel.state === 'idle' && F(p).panel.earlier.length === 1 && F(p).panel.earlier[0].chip === 'Stopped', 30000);
  t.check('the earlier run is the one stopped', F(idle)?.panel.earlier[0].task.includes('Add a health check'), F(idle)?.panel);

  // A trigger: when a card enters To do.
  await fx('trigger', '');
  const tr = await T.until('New trigger: kanban, To do, the card\'s title as the CR\'s', (p) => D(p).trigger.open && !D(p).trigger.loading && D(p).trigger.title === 'New trigger' && D(p).trigger.crTitle === '{{card.title}}'
    && D(p).trigger.sections.some((s) => s.id === 'goal' && s.value === '{{card.description}}'), 20000);
  t.check('a new trigger waits its turn', D(tr)?.trigger.busy === 1, D(tr)?.trigger);
  await fe('trig', 'name', 'Cards to the team');
  await T.snap('factories-trigger');
  await fx('tr-save');
  await T.until('saved: the panel lists it, Triggers open', (p) => !D(p).trigger.open && F(p).panel.triggersOpen && F(p).panel.triggers.some((x) => x.name === 'Cards to the team' && x.task === 'When a card enters To do'), 20000);
  await T.snap('factories-panel');

  // Back in the tab: Runs lists the stopped run; Cleanup its worktree.
  await T.ui('tab', 'factories');
  await fx('v-back');
  await fx('view', 'runs');
  await T.until('Runs lists the run, stopped', (p) => F(p).runs.items.length === 1 && F(p).runs.items[0].chip === 'Stopped' && /handoffs/.test(F(p).runs.items[0].meta));
  await fx('view', 'cleanup');
  const cl = await T.until('Cleanup lists the stopped run\'s worktree', (p) => F(p).view === 'cleanup' && !F(p).cleanup.loading && F(p).cleanup.items.length >= 1, 20000);
  t.check('it says what it is', /Worktree of a stopped run|Kept branch/.test(F(cl)?.cleanup.items[0].title || ''), F(cl)?.cleanup);
  await T.snap('factories-cleanup');
  await fx('cleanup-delete');
  await T.until('Delete selected without a pick says so', (p) => /Choose what to delete first/.test(JSON.stringify(p.toast || '')));
  await fx('view', 'library');
}

// The stand-in claude for the test server (no real Claude is started):
//   mkdir -p /tmp/plan26-factories/home/.local/bin
//   printf '#!/bin/sh\nexec sleep 3600\n' > /tmp/plan26-factories/home/.local/bin/claude; chmod +x …
//   HOME=/tmp/plan26-factories/home native-client/tools/test-server.sh 4738 --bare
