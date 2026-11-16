// operations.mjs's "extras" section: run(T, t) with T from t.begin('ops') (test/lib.mjs).
//
// Add project (the sidebar's "+ Add project": the dialog, the required
// fields, a missing path asks to create the folder, the new project opens,
// Esc closes), the approvals inbox (the top bar's button and count, the
// panel, "Open secrets", Dismiss, Clear all with its confirm), What's new
// (shown once, the steps, the recap, Esc skips and marks it seen; Settings'
// "all" opens it again).
import { mcpAsk } from './vault.mjs';

export async function run(T, t) {
  const { check } = t;
  const http = T.api.http;
  const X = (p) => p.areas.ops.extras;
  const until = (what, f, ms = 10000) => T.until(what, (p) => f(X(p), p), ms);
  const act = (...a) => T.act(...a);
  const port = new URL(T.SERVER).port;

  // ── Add project ──
  const cfg = (await http('GET', '/api/projects/config')).body || {};
  await act('extras-add-open');
  let p = await until('the dialog opens', (x) => x.add !== null, 5000);
  if (p) check('the mode is the server\'s', X(p).add.single === (cfg.mode === 'single'), { add: X(p).add, cfg });
  check('a covering dialog: the area is modal', p && X(p).modal === true);
  await T.snap('extras-add');
  await act('extras-add-submit');
  await T.until('nothing typed: the required fields', (q) => q.toast === (cfg.mode === 'single' ? 'Project name is required' : 'Alias and path are required'), 5000);
  if (cfg.mode === 'single') {
    await act('extras-add-set', 'name', 'Second Proj');
    await act('extras-add-submit');
    p = await until('created and opened', (x, q) => x.add === null && q.toast === 'Project created' && q.currentProject === 'second-proj', 15000);
  } else {
    await act('extras-add-set', 'alias', 'Second Proj');
    await act('extras-add-set', 'path', `/tmp/crundi-devtest-${port}/proj2`);
    await act('extras-add-set', 'dname', 'Second');
    await act('extras-add-submit');
    p = await T.until('a missing path asks to create it', (q) => q.dialog?.message === 'Path does not exist. Create empty folder?', 5000);
    await T.dialog(true);
    p = await until('added and opened', (x, q) => x.add === null && q.currentProject === 'secondproj', 15000);
  }
  const projects = (await http('GET', '/api/projects')).body?.projects || [];
  check('the server lists it', projects.some((x) => x.alias === 'second-proj' || x.alias === 'secondproj'), projects.map((x) => x.alias));
  await T.ui('select-project', 'demo');
  await T.until('back on demo', (q) => q.currentProject === 'demo');
  await act('extras-add-open');
  await until('open again', (x) => x.add !== null, 5000);
  check('the fields start empty', (await T.mine()).extras.add.alias === '' && (await T.mine()).extras.add.name === '');
  await T.key('Escape');
  await until('Esc closes it', (x) => x.add === null && !x.modal, 5000);

  // ── Approvals ──
  p = await until('nothing waiting: no button', (x) => x.appr.count === 0, 5000);
  await http('POST', '/api/secrets', { action: 'add', name: 'E2E_APPR', description: '', value: 'v-appr', pin: 'open-sesame' });
  const ask1 = mcpAsk(T, t, 'E2E_APPR');
  p = await until('a secret request: the button counts 1', (x) => x.appr.count === 1, 30000);
  if (p) {
    const it = X(p).appr.items[0];
    check('a secret item: its source, its buttons', it.source === 'secret' && JSON.stringify(it.acts) === JSON.stringify(['Open secrets', 'Dismiss']), it);
    await act('extras-appr-toggle');
    await until('the panel opens', (x) => x.appr.open, 5000);
    await T.snap('extras-appr-open');
    await act('extras-appr-act', it.id, 'secrets');
    await until('Open secrets: the tab, the panel closed', (x, q) => !x.appr.open && q.currentTab === 'secrets', 5000);
    await T.ui('tab', 'workbench');
    await act('extras-appr-toggle');
    await until('open again', (x) => x.appr.open, 5000);
    await act('extras-appr-act', it.id, 'dismiss');
    await until('Dismiss: gone, the button hidden, the panel closed', (x) => x.appr.count === 0 && !x.appr.open, 10000);
    const req = (await http('GET', '/api/secrets')).body?.requests || [];
    check('dismissing a secret request declines it', req.length === 0, req);
    await Promise.race([ask1, t.sleep(5000)]);
    // Clear all, with two waiting.
    const a2 = mcpAsk(T, t, 'E2E_APPR');
    const a3 = mcpAsk(T, t, 'E2E_APPR');
    await until('two waiting', (x) => x.appr.count === 2, 30000);
    await act('extras-appr-clear');
    p = await T.until('Clear all asks first', (q) => q.dialog?.title === 'Clear all 2?' && q.dialog?.message === '2 of them will be declined. Chat cards are only hidden and stay answerable in their chats.', 5000);
    await T.dialog(true);
    await until('cleared', (x) => x.appr.count === 0, 10000);
    await Promise.race([Promise.all([a2, a3]), t.sleep(5000)]);
  } else check('an approval reached the app (skipped the rest)', false);

  // ── What's new ──
  const before = (await http('GET', '/api/whatsnew')).body;
  if (before?.steps?.length) {
    await act('extras-wn-auto');
    p = await until('shown once after an install', (x) => x.wn !== null, 10000);
    if (p) check('the first step', X(p).wn.at === 0 && X(p).wn.n === before.steps.length + 1 && X(p).wn.title === before.steps[0].title, X(p).wn);
    await T.snap('extras-whatsnew');
    await T.key('Right');
    await until('Right: the next step', (x) => x.wn?.at === 1, 5000);
    await T.key('Left');
    await until('Left: back', (x) => x.wn?.at === 0, 5000);
    await act('extras-wn-go', String(before.steps.length));
    p = await until('the recap', (x) => x.wn?.last === true && x.wn.title === 'You are up to date', 5000);
    await T.key('Escape');
    await until('Esc skips', (x) => x.wn === null && !x.modal, 5000);
    let after = null;
    for (let i = 0; i < 20; i++) { after = (await http('GET', '/api/whatsnew')).body; if (!after?.steps?.length) break; await t.sleep(250); }
    check('skipped: marked seen', after && after.steps.length === 0, after);
    await act('extras-wn-auto');
    await t.sleep(1500);
    check('not shown again', (await T.mine()).extras.wn === null);
  } else check('the server has a What\'s new edition to show', false, before);
  await act('extras-wn-open', 'true');
  p = await until('Settings\' "What\'s new" opens it again', (x) => x.wn !== null && x.wn.all === true, 10000);
  await T.key('Return');
  await until('Enter: the next step', (x) => x.wn?.at === 1, 5000);
  await act('extras-wn-close', 'done');
  await until('closed', (x) => x.wn === null, 5000);
}
