// operations.mjs's "extras" section: run(T, t) with T from t.begin('ops') (test/lib.mjs).
//
// Add project (the sidebar's "+ Add project": the dialog, the required
// fields, a missing path asks to create the folder, the new project opens,
// Esc closes), the approvals inbox (the top bar's button and count, the
// panel, "Open secrets", Dismiss, Clear all with its confirm), What's new
// (shown once, the steps, each step's page live in the frame from the server,
// the dots, the fallback, the recap, Esc skips and marks it seen, Skip frees
// the view; Settings' "all" opens it again).
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
  // Each step's picture is the step's own page (its `page` on this server),
  // live in an off-screen view inside the frame: painted, from the server,
  // swapped with the step, freed when the tour closes.
  const base = T.SERVER.replace(/\/$/, '');
  const pageOk = (x, st) => x.wn?.page && x.wn.page.url === base + st.page && x.wn.page.painted && !x.wn.page.failed && x.wn.page.title === "What's new" && x.wnViews === 1;
  const dotsOk = (x) => x.wn && x.wn.dots.length === x.wn.n && x.wn.dots.every((d, i) => d === (i === x.wn.at ? 'on' : i < x.wn.at ? 'done' : ''));
  const before = (await http('GET', '/api/whatsnew')).body;
  if (before?.steps?.length) {
    check('every step names its page on the server', before.steps.every((st) => /^\/whatsnew\/[^/]+\/[^/]+\.html$/.test(st.page || '')), before.steps.map((st) => st.page));
    const pg = await fetch(base + before.steps[0].page).then(async (r) => ({ status: r.status, type: r.headers.get('content-type') || '', body: await r.text() })).catch((e) => ({ status: 0, body: String(e) }));
    check('the server serves the step\'s page: its fragment on the stage', pg.status === 200 && /text\/html/.test(pg.type) && pg.body.includes(before.steps[0].art), { status: pg.status, type: pg.type });
    await act('extras-wn-auto');
    p = await until('shown once after an install', (x) => x.wn !== null, 10000);
    if (p) check('the first step', X(p).wn.at === 0 && X(p).wn.n === before.steps.length + 1 && X(p).wn.title === before.steps[0].title, X(p).wn);
    p = await until('step 1: the frame holds the step\'s page, live, from the server', (x) => pageOk(x, before.steps[0]), 20000);
    if (p) check('the dots: the first on, the rest to come', dotsOk(X(p)), X(p).wn?.dots);
    const key0 = p && X(p).wn.page.key;
    // Its own animation running: the view keeps painting.
    if (p) {
      const g0 = (await T.mine()).extras.wn?.page;
      await t.sleep(1200);
      check('the page is live (painted, its first load done)', g0 && g0.painted && g0.loads >= 1, g0);
    }
    await t.sleep(1500);
    await T.snap('whatsnew-step1');
    await T.key('Right');
    p = await until('Right: the next step, its own page', (x) => x.wn?.at === 1 && pageOk(x, before.steps[1]) && x.wn.page.key !== key0, 20000);
    if (p) check('the dots follow', dotsOk(X(p)) && X(p).wn.dots[0] === 'done' && X(p).wn.dots[1] === 'on', X(p).wn.dots);
    // A middle step for the review.
    const mid = Math.max(1, Math.floor(before.steps.length / 2));
    await act('extras-wn-go', String(mid));
    await until('a middle step, its page', (x) => x.wn?.at === mid && pageOk(x, before.steps[mid]), 20000);
    await t.sleep(1500);
    await T.snap('whatsnew-middle');
    await act('extras-wn-go', '1');
    await until('back to step 2', (x) => x.wn?.at === 1 && pageOk(x, before.steps[1]), 20000);
    await T.key('Left');
    p = await until('Left: back, step 1\'s page again (one view held)', (x) => x.wn?.at === 0 && pageOk(x, before.steps[0]), 20000);
    if (p) check('the dots follow back', dotsOk(X(p)), X(p).wn.dots);
    // A page that can't load: the step's title instead.
    await act('extras-wn-page', '/whatsnew/0.0.0/missing.html');
    p = await until('a page that fails: the plain fallback (the failed load seen)', (x) => x.wn?.page?.failed === true, 20000);
    await act('extras-wn-go', String(before.steps.length));
    p = await until('the recap: no page held', (x) => x.wn?.last === true && x.wn.title === 'You are up to date' && x.wn.page === null && x.wnViews === 0, 5000);
    if (p) check('the recap\'s dots: every step done', dotsOk(X(p)) && X(p).wn.dots.at(-1) === 'on', X(p).wn.dots);
    await T.snap('whatsnew-recap');
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
  await until('its page, live', (x) => x.wn?.page?.painted === true && x.wnViews === 1, 20000);
  await T.key('Return');
  await until('Enter: the next step', (x) => x.wn?.at === 1, 5000);
  // Skip (its button's action): the tour closes and its view is freed.
  await until('the next step\'s page', (x) => x.wn?.page?.painted === true && x.wnViews === 1, 20000);
  await act('extras-wn-close');
  await until('Skip: closed, the page\'s view freed', (x) => x.wn === null && x.wnViews === 0, 5000);
}
