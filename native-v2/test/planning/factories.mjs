import { execSync } from 'node:child_process';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Factories (run by test/planning.mjs), ported from
// native-gpui/test/planning/factories.mjs. The Factories tab (Library with
// its templates, the editor, Runs, Cleanup, the start form) and the live
// graph (tab and panel), the panel's earlier runs and triggers. A run is
// started for real: start the test server with tools/stub-server.sh (its
// `claude` is the stand-in, so the Manager's chat opens and no real Claude
// is spent). Without it the run's checks are skipped.

let T;
const F = (p) => p.planningArea?.factories;
const V = (p) => F(p)?.viewer;
const E = (p) => F(p)?.editor;
const D = (p) => F(p)?.dialogs;
const fx = (...a) => T.act('factories-act', ...a);
const fe = (...a) => T.act('factories-edit', ...a);
// The open confirm or prompt (the widgets' ask layer): the driver's "dialog".
const DLG = (p) => p.dialog || F(p)?.dialog;
const answer = (ok, v = '') => T.ui('dialog-answer', ok ? 'true' : 'false', v);

export async function run(T_, t) {
  T = T_;
  const projDir = `/tmp/crundi-devtest-${new URL(T.SERVER).port}/proj`;
  // ── The Library: no factories of our own yet, the templates ──
  await T.ui('tab', 'factories');
  const lib = await T.until('the library loads with the templates', (p) => F(p)?.view === 'library' && !F(p).library.loading && F(p).library.empty && F(p).library.templates.length >= 3);
  t.check('the templates name their team', F(lib)?.library.templates.every((x) => x.team.length > 0), F(lib)?.library.templates);
  await sleep(300); await T.snap('planning', 'factories-library-empty');
  const still = T.frames.mark();
  await sleep(1500);
  t.check('the idle library draws no frames', T.frames.since(still).frames === 0, T.frames.since(still));

  // Use a template: it is copied and opens in the editor.
  await fx('use-template', 'tpl_pair');
  const ed = await T.until('the template opens in the editor', (p) => F(p)?.sub === 'editor' && E(p)?.state === 'ok' && E(p).nodes.length >= 5 && E(p).pane?.mode === 'settings', 30000);
  const mgr = E(ed).nodes.find((n) => n.kind === 'manager');
  t.check('the Manager step has Finish among its outputs', mgr && mgr.outs.some((o) => o.label === 'Finish'), mgr);
  await sleep(300); await T.snap('planning', 'factories-editor');

  // Select a step and rename it: Unsaved, then Saved; undo.
  const emp = E(ed).nodes.find((n) => n.kind === 'employee');
  await T.act('factories-node-click', 'e', emp.id);
  await T.until('the step inspector', (p) => E(p).pane?.mode === 'node' && E(p).pane.kindLabel === 'Employee' && E(p).sel === 'node:' + emp.id);
  await fe('ed', 'label', 'Build it');
  await T.until('the label shows and it is unsaved', (p) => E(p).nodes.find((n) => n.id === emp.id)?.title === 'Build it' && E(p).saveLabel === 'Unsaved' && E(p).canUndo);
  await T.until('saved after a moment', (p) => E(p).saveLabel === 'Saved', 8000);
  await fx('e-undo');
  await T.until('undo puts the name back', (p) => E(p).nodes.find((n) => n.id === emp.id)?.title !== 'Build it' && E(p).canRedo);
  await fx('e-redo');
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
  await sleep(300); await T.snap('planning', 'factories-editor-team');
  await fx('e-tab', 'issues');
  await T.until('the Issues pane says it can run', (p) => E(p).pane?.mode === 'issues' && E(p).pane.issuesTitle === 'Ready to run');
  await fx('e-back');
  await T.until('Back saves and shows the library', (p) => F(p).sub === '' && F(p).library.factories.length === 1);

  // New factory: a name, the skeleton (Task, Manager, Result); add a person and wire them.
  await fx('new');
  await T.until('New factory asks for a name', (p) => DLG(p)?.value === 'New factory');
  await sleep(300); await T.snap('planning', 'factories-new-prompt');
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
  // The Dry run walks the flow without Claude (the engine, here): the Manager has the work, hands it on.
  await fx('e-tab', 'dry');
  await fx('e-dry-start');
  const dr = await T.until('the dry run starts with someone at work', (p) => E(p).dry?.on && E(p).dry.tokens.length >= 1);
  await fx('e-dry-handoff', E(dr).dry.tokens[0].id, '');
  await T.until('a handoff moves it on and is logged', (p) => E(p).dry.log.some((l) => /handed off/.test(l)) && E(p).dry.handoffs >= 1);
  await fx('e-dry-end');
  await T.until('End leaves the dry run', (p) => E(p).dry && !E(p).dry.on);
  // The '/' skills menu in the description (when this machine has skills).
  await fx('e-settings');
  await T.until('the factory settings', (p) => E(p).pane?.mode === 'settings');
  await T.act('factories-slash', 'description', 'Use /');
  const sl = await T.props();
  if (E(sl).slash?.items?.length) {
    const sk = E(sl).slash.items[0];
    await T.act('factories-slash-pick', sk);
    await T.until('picking a skill writes its name', (p) => !E(p).slash && E(p).pane?.mode === 'settings');
  }
  await sleep(300); await T.snap('planning', 'factories-editor-new');
  await fx('e-back');
  const two = await T.until('two factories in the library', (p) => F(p).sub === '' && F(p).library.factories.length === 2);
  await sleep(300); await T.snap('planning', 'factories-library');

  // Runs: none yet.
  await fx('view', 'runs');
  await T.until('No runs yet', (p) => F(p).view === 'runs' && !F(p).runs.loading && F(p).runs.items.length === 0);

  // ── Start a factory: the change request ──
  try { execSync('git init -q -b main && git -c user.email=t@t -c user.name=t commit -q --allow-empty -m init', { cwd: projDir }); } catch { /* already a repo */ }
  const pair = F(two).library.factories.find((f) => f.name !== 'Two step');
  await fx('view', 'library');
  await fx('start-one', pair.id);
  const st = await T.until('the start form: project, factory, the change request, the precheck', (p) => D(p)?.start?.open && !D(p).start.loading && D(p).start.sections.length >= 3
    && D(p).start.info.some((x) => /own worktree/.test(x.text)) && F(p).pointerModal, 20000);
  t.check('it starts from the factory picked', D(st)?.start.factories[D(st).start.factory] === pair.name, D(st)?.start);
  // A picture pasted in the change request attaches, named as the web names
  // it (pasted-<time>-<n>.png); its x takes it off again.
  await T.act('factories-clip-image', 'start|title');
  await T.key('v', ['control']);
  await T.until('a pasted picture attaches as pasted-….png', (p) => D(p).start.files.length === 1 && /^pasted-\d+-\d+\.png$/.test(D(p).start.files[0]) && D(p).start.title === '');
  await fx('st-unattach', '0');
  await T.until('its x takes it off', (p) => D(p).start.files.length === 0);
  await fx('st-go');
  await T.until('Start without a title asks for one', (p) => D(p).start.titleBad);
  await fe('start', 'title', 'Add a health check');
  for (const s of D(st).start.sections) await fe('start', 'sec:' + s.id, s.id === 'done' ? 'GET /health answers 200' : 'Something about ' + s.label);
  await sleep(300); await T.snap('planning', 'factories-start');
  await fx('st-go');
  // "/health" in the text reads as a skill the project does not have: said, with Start anyway.
  await T.until('a missing skill is named, with Start anyway', (p) => D(p).start.info.some((x) => /names skill the project does not have: \/health/.test(x.text)) && D(p).start.info.some((x) => x.kind === 'anyway'), 20000);
  await fx('st-go', 'anyway');
  const started = await T.until('the run starts and its live graph opens', (p) => !D(p).start.open && F(p).sub === 'viewer' && V(p)?.state === 'ok' && V(p).nodes.length >= 5, 40000);
  if (!started) { const p = await T.props(); console.log('start form:', JSON.stringify(D(p)?.start), 'sub:', F(p)?.sub, 'viewer:', JSON.stringify(V(p))?.slice(0, 400), 'toast:', JSON.stringify(p.toast)); t.check('(the run could not start: is the server started with tools/stub-server.sh?)', false); return; }
  t.check('the graph has the Manager at work (or starting)', V(started).nodes.some((n) => n.kind === 'manager'), V(started).nodes.map((n) => n.kind));
  await T.until('the timeline has the start', (p) => V(p).timeline.some((x) => x.main === 'Started'), 20000);
  await T.until('the run pane lists the task and the branch', (p) => V(p).kv.some((x) => x[0] === 'Task' && x[1].includes('Add a health check')) && V(p).kv.some((x) => x[0] === 'Branch'));
  await sleep(300); await T.snap('planning', 'factories-run');
  // A step's pane, the timeline tab.
  await T.act('factories-node-click', 'v', V(started).nodes.find((n) => n.kind === 'manager').id);
  await T.until('the step pane', (p) => V(p).sel && /working|not reached|done|waiting|deciding/.test(V(p).now));
  await fx('v-tab', 'timeline');
  await T.until('the timeline tab', (p) => V(p).tab === 'timeline');
  // The replay slider: to the first event, then back to live.
  const ev = V(await T.props()).events;
  if (ev > 1) {
    await T.act('factories-slider', '1');
    const r1 = await T.until('the slider replays the first event', (p) => !V(p).live && V(p).at === 1);
    // The run as it was then (the journal folded): the token still at Start, none invented.
    const act1 = V(r1).nodes.filter((n) => n.active);
    t.check('at the first event the one token is at Start', act1.length === 1 && act1[0].kind === 'start', V(r1).nodes.map((n) => [n.kind, n.active, n.token]));
    const begin = V(r1).timeline.find((x) => x.main === 'The Manager got the task');
    if (begin) {
      await T.act('factories-slider', String(begin.i));
      const r2 = await T.until('replayed to the Manager getting the task', (p) => V(p).at === begin.i);
      t.check('then the Manager has it, and Start does not', V(r2).nodes.find((n) => n.kind === 'manager')?.active && !V(r2).nodes.find((n) => n.kind === 'start')?.active, V(r2).nodes.map((n) => [n.kind, n.active, n.token]));
    }
    // (Its end now: the run may have logged more since.)
    await T.act('factories-slider', String(V(await T.props()).events));
    await T.until('at its end it is live again', (p) => V(p).live);
  }
  await fx('v-tab', 'run');
  await fx('v-unselect');

  // A person's transcript is their live chat (the web's chat page in a
  // frame): the conversation from the server's replay on subscribe, live as
  // it changes, and its message box.
  const rid = V(await T.props()).runId;
  const got = await T.api.http('GET', `/api/factory-runs/get?id=${encodeURIComponent(rid)}`);
  const mchat = got.body?.run?.managerChatId;
  t.check('the run has the Manager\'s chat', !!mchat, got.body);
  if (mchat) {
    const TR = (p) => V(p).transcript;
    await T.act('factories-transcript', mchat, 'Manager', 'sheet');
    const s1 = await T.until('the transcript shows the chat, its history', (p) => TR(p).open && TR(p).stores?.[0]?.id === mchat && TR(p).stores[0].rows > 0, 20000);
    if (s1) {
      const rows0 = TR(s1).stores[0].rows;
      await T.act('factories-transcript-send', 'Hello from the graph');
      await T.until('a message from its box, and the reply, show live', (p) => TR(p).stores[0].rows >= rows0 + 2 && TR(p).stores[0].state !== 'working', 30000);
      await sleep(300); await T.snap('planning', 'factories-transcript');
      // The page's header (#cp-head, F188): the chat's mark (its initials on
      // the project's colours, or the project's icon), ⋯ with Rename chat,
      // Stop Claude (only while it works) and Open in Crundi; renaming in the head.
      const H = (p) => TR(p).stores[0].head;
      const h0 = H(await T.props());
      t.check('the header has the chat\'s mark', h0 && (h0.icon || /^[A-Z0-9]{1,2}$/.test(h0.mark)), h0);
      await T.act('factories-transcript-head', 'menu');
      const mn = await T.until('⋯ opens the page\'s menu', (p) => p.planningArea?.menu?.items?.length >= 3);
      const ids = (mn?.planningArea.menu.items || []).map((i) => i.id);
      t.check('…with Rename chat, Stop Claude and Open in Crundi', ['rename', 'stop', 'open'].every((x) => ids.includes(x)), mn?.planningArea.menu);
      t.check('…Stop Claude off while Claude is not working', mn?.planningArea.menu.items.find((i) => i.id === 'stop')?.enabled === (H(mn).state === 'working'), mn?.planningArea.menu);
      await T.ui('menu-pick', 'none');
      await T.act('factories-transcript-head', 'rename', 'Renamed in the head');
      await T.until('a rename in the head renames the chat', (p) => H(p).title === 'Renamed in the head', 15000);
      await T.act('factories-transcript-head', 'rename-escape', 'Not this');
      await sleep(800);
      t.check('…Escape keeps the name', H(await T.props()).title === 'Renamed in the head');
      t.check('no secret requests: no approval cards', (H(await T.props()).asks?.cards || []).length === 0, H(await T.props()).asks);
      // F210: each card starts with an empty box, its word from the lock; a
      // PIN typed for one request never stays for the next.
      {
        const rq = (id, lock, name) => ({ id, lock, secretName: name, secretId: 's-' + id, kind: 'read', reason: 'test', project: '', sessionId: '' });
        const A = (p) => H(p).asks || {};
        await T.act('factories-transcript-head', 'ask-set', JSON.stringify([rq('rqa', 'pin', 'Alpha')]));
        const a1 = await T.until('a secret request: its card, armed, the box saying PIN', (p) => A(p).cards?.[0]?.armed && A(p).placeholder === 'PIN', 6000);
        t.check('…the box empty', A(a1).pin === '', A(a1));
        // F216: the cards are drawn, with their height (not 0 px).
        await T.until('…the card drawn with its height', (p) => A(p).h > 40, 6000);
        // F224: the whole card shows (its PIN box, Approve, Reject), not cut off.
        await T.until('…the whole card shows, not cut off', (p) => A(p).clipped === false && A(p).h > 100, 6000);
        await sleep(300); await T.snap('planning', 'factories-transcript-ask-1');
        // F222: the PIN box takes 6 characters at most.
        await T.act('factories-transcript-head', 'ask-type', '1234567');
        await sleep(300);
        t.check('…the PIN box takes no more than 6 characters', A(await T.props()).pin === '', A(await T.props()));
        await T.act('factories-transcript-head', 'ask-type', '123456');
        await T.until('…the PIN typed', (p) => A(p).pin === '123456');
        // F217: a wrong PIN says why and empties the box for the next try.
        await T.act('factories-transcript-head', 'ask-approve');
        const aw = await T.until('a wrong PIN: the card says why', (p) => A(p).error && A(p).cards?.[0]?.armed, 8000);
        t.check('…and the box is empty', A(aw).pin === '', A(aw));
        // Two requests: both cards drawn, taller than one.
        const h1 = A(aw).h;
        await T.act('factories-transcript-head', 'ask-set', JSON.stringify([rq('rqa', 'pin', 'Alpha'), rq('rqc', 'pin', 'Gamma')]));
        await T.until('two requests: both cards drawn, taller than one', (p) => A(p).cards?.length === 2 && A(p).h > h1 + 40, 6000);
        await T.until('…both whole, the second not cut off', (p) => A(p).cards?.length === 2 && A(p).clipped === false, 6000);
        await sleep(300); await T.snap('planning', 'factories-transcript-ask-2');
        await T.act('factories-transcript-head', 'ask-type', '123456');
        await T.until('…the PIN typed', (p) => A(p).pin === '123456');
        await T.act('factories-transcript-head', 'ask-set', JSON.stringify([rq('rqb', 'passphrase', 'Beta')]));
        const a2 = await T.until('the next request armed by itself', (p) => A(p).cards?.[0]?.id === 'rqb' && A(p).cards[0].armed, 6000);
        t.check('…its box empty: the PIN typed before is gone', A(a2).pin === '', A(a2));
        t.check('…saying Passphrase for a passphrase lock', A(a2).placeholder === 'Passphrase', A(a2));
        await T.act('factories-transcript-head', 'ask-type', 'a passphrase');
        await T.act('factories-transcript-head', 'ask-reject', 'rqb');
        const a3 = await T.until('Reject empties the box', (p) => A(p).pin === '', 6000);
        t.check('…the box empty after Reject', A(a3).pin === '', A(a3));
        // F229: after an approve the next card is armed, whichever comes
        // first, the server's push or the POST's answer.
        await T.act('factories-transcript-head', 'ask-set', JSON.stringify([rq('rqd', 'pin', 'Delta'), rq('rqe', 'pin', 'Epsilon')]));
        await T.until('two requests, the first armed', (p) => A(p).cards?.length === 2 && A(p).cards[0].id === 'rqd' && A(p).cards[0].armed, 6000);
        await T.act('factories-transcript-head', 'ask-set', JSON.stringify([rq('rqe', 'pin', 'Epsilon')]));
        await T.until('the push first: the next card armed', (p) => A(p).cards?.[0]?.id === 'rqe' && A(p).cards[0].armed, 6000);
        await T.act('factories-transcript-head', 'ask-approved', 'rqd');
        await sleep(300);
        t.check('…the approve\'s answer after it leaves the next card armed', A(await T.props()).cards?.[0]?.armed === true, A(await T.props()));
        await T.act('factories-transcript-head', 'ask-set', JSON.stringify([rq('rqd', 'pin', 'Delta'), rq('rqe', 'pin', 'Epsilon')]));
        await T.until('again two requests, the first armed', (p) => A(p).cards?.length === 2 && A(p).cards.some((c) => c.armed), 6000);
        await T.act('factories-transcript-head', 'ask-approved', 'rqd');
        await T.act('factories-transcript-head', 'ask-set', JSON.stringify([rq('rqe', 'pin', 'Epsilon')]));
        const a5 = await T.until('the answer first, then the push: the next card armed', (p) => A(p).cards?.length === 1 && A(p).cards[0].id === 'rqe' && A(p).cards[0].armed, 6000);
        t.check('…its box empty, saying PIN', A(a5).pin === '' && A(a5).placeholder === 'PIN', A(a5));
        await T.act('factories-transcript-head', 'ask-set', '[]');
        await T.until('…no requests: no cards', (p) => (A(p).cards || []).length === 0);
      }
    }
    await fx('chat-close');
    await T.until('closed, the transcript lets go of the chat', (p) => !TR(p).open && TR(p).views.length === 0);
  }

  // The project's Factory panel: the live graph, Follow and Details.
  await T.act('open-pane', 'factory');
  await T.ui('tab', 'workbench');
  const pl = await T.until('the panel is the run\'s live graph', (p) => F(p).panel.state === 'live' && V(p)?.panel && V(p).state === 'ok' && V(p).follow, 20000);
  t.check('the panel has no Back', pl && V(pl).panel, pl && V(pl));
  await fx('v-drawer');
  await T.until('Details opens the drawer', (p) => V(p).drawer);
  await T.until('the graph is fitted to the panel', (p) => V(p).zoom > 0 && V(p).nodes.length >= 5);
  await sleep(300); await T.snap('planning', 'factories-panel-live');
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
  await sleep(300); await T.snap('planning', 'factories-trigger');
  await fx('tr-save');
  await T.until('saved: the panel lists it, Triggers open', (p) => !D(p).trigger.open && F(p).panel.triggersOpen && F(p).panel.triggers.some((x) => x.name === 'Cards to the team' && x.task === 'When a card enters To do'), 20000);
  await sleep(300); await T.snap('planning', 'factories-panel');

  // Back in the tab: Runs lists the stopped run; Cleanup its worktree.
  await T.ui('tab', 'factories');
  await fx('v-back');
  await fx('view', 'runs');
  await T.until('Runs lists the run, stopped', (p) => F(p).runs.items.length === 1 && F(p).runs.items[0].chip === 'Stopped' && /handoffs/.test(F(p).runs.items[0].meta));
  await fx('view', 'cleanup');
  const cl = await T.until('Cleanup lists the stopped run\'s worktree', (p) => F(p).view === 'cleanup' && !F(p).cleanup.loading && F(p).cleanup.items.length >= 1, 20000);
  t.check('it says what it is', /Worktree of a stopped run|Kept branch/.test(F(cl)?.cleanup.items[0].title || ''), F(cl)?.cleanup);
  await sleep(300); await T.snap('planning', 'factories-cleanup');
  await fx('cleanup-delete');
  await T.until('Delete selected without a pick says so', (p) => /Choose what to delete first/.test(JSON.stringify(p.toast || '')));
  await fx('view', 'library');
}
