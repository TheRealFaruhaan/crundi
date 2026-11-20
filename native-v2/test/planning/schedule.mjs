import { createHmac } from 'node:crypto';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Schedules and webhooks (run by test/planning.mjs), ported from
// native-gpui/test/planning/schedule.mjs.

let T;
const S = (p) => p.planningArea?.schedule;
const D = (p) => S(p)?.dialog;
const modal = (p) => !!(p.planningArea?.pointerModal || S(p)?.pointerModal);
const DLG = (p) => p.dialog ?? S(p)?.ask;
const answer = (ok) => T.ui('dialog-answer', ok ? 'true' : 'false');
const item = (p, name) => S(p)?.items?.find((x) => x.name === name);
const spost = (body) => T.api.http('POST', '/api/schedules', body);
// A call to a webhook from outside: no session, only the key or the signature.
const hook = async (url, body, headers = {}) => {
  const raw = JSON.stringify(body);
  const r = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json', ...headers }, body: raw });
  let json = null; try { json = JSON.parse(await r.text()); } catch { /* not JSON */ }
  return { status: r.status, body: json };
};
const sign = (secret, raw) => createHmac('sha256', secret).update(raw, 'utf8').digest('hex');

export async function run(T_, t) {
  T = T_;
  // Seed: a kanban task with a subtask (for a kanban condition) and a service.
  const kt = await T.api.http('POST', '/api/kanban', { project: 'demo', action: 'addTask', title: 'Ship it', status: 'todo' });
  const taskId = kt.body?.task?.id || kt.body?.id;
  if (taskId) await T.api.http('POST', '/api/kanban', { project: 'demo', action: 'addTodo', taskId, text: 'Tests pass' });
  await T.api.http('POST', '/api/services', { name: 'web', command: 'sleep 1000', alias: 'demo' });

  // ── The empty list ──
  await T.ui('tab', 'schedule');
  await T.until('the list loads empty, filtered to the project', (p) => S(p)?.state === 'ok' && S(p).items.length === 0
    && S(p).empty === 'No scheduled tasks for this project' && S(p).hasProject && S(p).filter);
  await sleep(300); await T.snap('planning', 'schedule-empty');

  // ── + Add schedule: the dialog, an Agent run every day, with conditions ──
  await T.act('schedule-act', 'add');
  await T.until('New schedule opens: recurring, every day, Agent', (p) => D(p)?.title === 'New schedule' && D(p).when === 'recurring'
    && D(p).days.every(Boolean) && D(p).time === '09:00' && D(p).kind === 'agent' && D(p).projects[0] === 'Demo' && modal(p));
  await T.act('schedule-save');
  await T.until('a name first', (p) => /Give the schedule a name/.test(p.toast) && D(p));
  await T.act('schedule-type', 'name', 'Morning review');
  await T.act('schedule-save');
  await T.until('then a prompt', (p) => /Agent needs a prompt/.test(p.toast));
  await T.act('schedule-type', 'prompt', 'Review the open pull requests');
  await T.act('schedule-pick', 'model', '1');
  await T.act('schedule-seg', 'mode', 'skip');
  await T.act('schedule-day', '0');
  await T.act('schedule-day', '6');
  await T.until('picks and days kept (typed text survives a redraw)', (p) => D(p).model === 1 && D(p).mode === 'skip'
    && !D(p).days[0] && !D(p).days[6] && D(p).name === 'Morning review' && D(p).prompt === 'Review the open pull requests');
  await T.act('schedule-add-cond', 'usage');
  await T.act('schedule-add-cond', 'terminals');
  await T.act('schedule-add-cond', 'kanban');
  await T.until('three condition rows', (p) => D(p).conds.map((c) => c.title).join() === '5h usage,Open terminals,Kanban'
    && D(p).conds[2].statuses.join() === 'Backlog,To Do,In Progress,Done' && D(p).tasks.includes('Ship it'));
  await T.act('schedule-cond-pick', '0', 'op', '1');
  await T.act('schedule-cond-value', '0', '80');
  // "Ship it" by name: with the other screens' tests run first, the board
  // has their tasks too, so its place in the list isn't always 1.
  const ship = D(await T.props()).tasks.indexOf('Ship it');
  await T.act('schedule-cond-pick', '2', 'task', String(ship));
  await T.until('a task gives its subtasks', (p) => D(p).conds[2].task === ship && D(p).conds[2].todos.join() === 'whole task,Tests pass');
  await T.act('schedule-cond-pick', '2', 'todo', '1');
  await T.until('a subtask: done or pending', (p) => D(p).conds[2].statuses.join() === 'done,pending' && D(p).conds[2].status === 0);
  await sleep(300); await T.snap('planning', 'schedule-dialog-agent');
  await T.act('schedule-remove-cond', '1');
  await T.until('× removes a row', (p) => D(p).conds.map((c) => c.title).join() === '5h usage,Kanban');
  await T.act('schedule-save');
  await T.until('saved and closed; the card shows it', (p) => !D(p) && !modal(p) && item(p, 'Morning review')?.summary === 'Agent · Claude · skip-perms · opus · new session'
    && item(p, 'Morning review').when === 'Mo,Tu,We,Th,Fr 09:00' && item(p, 'Morning review').conds.join() === '5h usage above 80%,kanban subtask = done');

  // ── One-time, a CLI command ──
  await T.act('schedule-act', 'add');
  await T.until('the dialog again', (p) => D(p)?.title === 'New schedule' && D(p).name === '');
  await T.act('schedule-type', 'name', 'Release build');
  await T.act('schedule-seg', 'whenmode', 'once');
  await T.act('schedule-seg', 'kind', 'command');
  await T.act('schedule-type', 'time', '7:5');
  await T.act('schedule-save');
  await T.until('a date for a one-time run', (p) => /Pick a date for the one-time run/.test(p.toast) && D(p)?.when === 'once');
  await T.act('schedule-type', 'date', '2026-12-01');
  await T.act('schedule-save');
  await T.until('a command', (p) => /Enter a command/.test(p.toast));
  await T.act('schedule-type', 'command', 'npm run build');
  await T.act('schedule-add-cond', 'usage-week');
  await T.act('schedule-add-cond', 'kanban');
  await T.act('schedule-cond-pick', '1', 'task', '1');
  await T.until('a one-time command with its conditions', (p) => D(p).command === 'npm run build' && D(p).date === '2026-12-01'
    && D(p).conds.map((c) => c.title).join() === 'Weekly usage,Kanban' && D(p).conds[1].statuses[D(p).conds[1].status] === 'Backlog');
  await sleep(300); await T.snap('planning', 'schedule-dialog-conditions');
  await T.act('schedule-save');
  await T.until('saved: once, the time tidied', (p) => !D(p) && item(p, 'Release build')?.when === '2026-12-01 07:05' && item(p, 'Release build').summary === 'Run: npm run build'
    && item(p, 'Release build').conds.join() === 'weekly usage below 50%,kanban task = backlog');

  // ── Service ──
  await T.act('schedule-act', 'add');
  await T.until('the dialog', (p) => !!D(p));
  await T.act('schedule-seg', 'kind', 'service');
  await T.until('the project\'s services, and the operations', (p) => D(p).kind === 'service' && D(p).hasServices && D(p).services[0] === 'web' && D(p).op === 'start');
  await T.act('schedule-seg', 'op', 'toggle');
  await T.until('Toggle picked', (p) => D(p).op === 'toggle');
  await T.act('schedule-close');
  await T.until('Cancel closes it', (p) => !D(p) && !modal(p));

  // ── 4.6 Webhooks: the dialog's "When called" mode ──
  await webhooks(t);

  // ── Enable switch, filter, live, edit, delete ──
  const mr = item(await T.props(), 'Morning review');
  await T.act('schedule-act', 'enable', mr.id);
  await T.until('switched off', (p) => item(p, 'Morning review')?.enabled === false);
  await spost({ action: 'add', schedule: { name: 'From elsewhere', project: 'demo', action: { kind: 'command', command: 'date' }, when: { mode: 'recurring', time: '18:00', days: [1] } } });
  await T.until('a schedule made elsewhere arrives live', (p) => item(p, 'From elsewhere')?.when === 'Mo 18:00');
  await sleep(300); await T.snap('planning', 'schedule-list');
  await T.act('schedule-act', 'filter');
  await T.until('All projects', (p) => S(p).filter === false && S(p).items.length === 5);
  await T.act('schedule-act', 'filter');
  await T.until('This project again', (p) => S(p).filter === true);

  await T.act('schedule-act', 'edit', mr.id);
  await T.until('Edit schedule, filled in', (p) => D(p)?.title === 'Edit schedule' && D(p).name === 'Morning review' && D(p).conds.length === 2 && D(p).model === 1);
  await T.act('schedule-type', 'name', 'Morning review (edited)');
  await T.act('schedule-save');
  await T.until('updated', (p) => !D(p) && item(p, 'Morning review (edited)'));

  const fe = item(await T.props(), 'From elsewhere');
  await T.act('schedule-act', 'delete', fe.id);
  await T.until('Delete asks', (p) => DLG(p)?.title === 'Delete schedule "From elsewhere"?');
  await answer(true);
  await T.until('deleted', (p) => !item(p, 'From elsewhere') && S(p).items.length === 4);
}

async function webhooks(t) {
  // A background chat called by Grafana: the dialog before the first save.
  await T.act('schedule-act', 'add');
  await T.until('the dialog', (p) => !!D(p));
  await T.act('schedule-type', 'name', 'Grafana alert');
  await T.act('schedule-seg', 'whenmode', 'webhook');
  await T.until('When called: key in the URL, no URL before saving', (p) => D(p).when === 'webhook' && D(p).hookAuth === 'key' && D(p).hookUrl === '');
  await T.act('schedule-save');
  await T.until('a webhook runs a chat or a service only', (p) => /A webhook can run a background chat or a service action/.test(p.toast));
  await T.act('schedule-seg', 'kind', 'chat');
  await T.act('schedule-seg', 'hookauth', 'signature');
  await T.act('schedule-type', 'ts-header', 'X-Grafana-Alerting-Signature-Timestamp');
  await T.until('signed: the signature and timestamp headers', (p) => D(p).hookAuth === 'signature' && D(p).sigHeader === 'x-crundi-signature'
    && D(p).tsHeader === 'X-Grafana-Alerting-Signature-Timestamp' && D(p).cwdHint.endsWith('/proj'));
  await T.act('schedule-save');
  await T.until('the chat needs a prompt', (p) => /The chat needs a prompt/.test(p.toast));
  await T.act('schedule-type', 'prompt', 'Look into this alert: {{payload.message}}');
  await sleep(300); await T.snap('planning', 'schedule-webhook-new');
  await T.act('schedule-save');
  await T.until('a new webhook stays open: its URL and secret', (p) => D(p)?.title === 'Edit schedule' && /\/api\/schedule-hook\/[0-9a-f]{16}$/.test(D(p).hookUrl)
    && D(p).hookSecret.length > 10 && /Saved\. Its URL is below\./.test(p.toast));
  await sleep(300); await T.snap('planning', 'schedule-webhook-signed');
  await T.act('schedule-seg', 'session', 'resume');
  await T.until('One ongoing chat: none yet, Create it now', (p) => D(p).session === 'resume' && D(p).sessionId === '');
  await T.act('schedule-close');
  await T.until('the card says when it runs', (p) => !D(p) && item(p, 'Grafana alert')?.when === 'When its webhook is called');

  // A service action, called from here over HTTP (a chat would start Claude).
  await T.act('schedule-act', 'add');
  await T.until('the dialog', (p) => !!D(p));
  await T.act('schedule-type', 'name', 'Deploy hook');
  await T.act('schedule-seg', 'whenmode', 'webhook');
  await T.act('schedule-seg', 'kind', 'service');
  await T.act('schedule-type', 'idem', '{{payload.id}}');
  await T.act('schedule-save');
  let p = await T.until('saved: the URL carries the key', (p) => /\/api\/schedule-hook\/[0-9a-f]{16}\?key=.+/.test(D(p)?.hookUrl || ''));
  await sleep(300); await T.snap('planning', 'schedule-webhook-key');
  const keyUrl = D(p).hookUrl;
  let r = await hook(keyUrl, { id: 'call-1' });
  t.check('a call with the key starts it', r.status === 200 && r.body?.ok && r.body.started, r);
  r = await hook(keyUrl, { id: 'call-1' });
  t.check('the same idempotency key again is not processed', r.status === 200 && r.body?.duplicate, r);
  r = await hook(keyUrl.replace(/key=.*/, 'key=wrong'), { id: 'call-2' });
  t.check('a wrong key is refused', r.status === 403, r);

  // New key: the old URL stops working, the new one works.
  await T.act('schedule-rotate');
  await T.until('New key asks first', (p) => /The old URL stops working\. Make a new key\?/.test(DLG(p)?.title || ''));
  await answer(true);
  p = await T.until('a new key in the URL', (p) => D(p)?.hookUrl && D(p).hookUrl !== keyUrl);
  r = await hook(keyUrl, { id: 'call-3' });
  t.check('the old key no longer works', r.status === 403, r);
  r = await hook(D(p).hookUrl, { id: 'call-3' });
  t.check('the new key does', r.status === 200 && r.body?.started, r);

  // Signed with a secret: the URL loses its key; the secret signs the body.
  await T.act('schedule-seg', 'hookauth', 'signature');
  p = await T.until('signed: the URL and the secret', (p) => D(p).hookAuth === 'signature' && !/\?key=/.test(D(p).hookUrl) && D(p).hookSecret.length > 10);
  await T.act('schedule-save');
  await T.until('saved (an edit closes)', (p) => !D(p));
  const url = D(p).hookUrl, secret = D(p).hookSecret;
  const raw = JSON.stringify({ id: 'call-4' });
  r = await hook(url, { id: 'call-4' }, { 'x-crundi-signature': 'sha256=' + sign(secret, raw) });
  t.check('a signed call is accepted', r.status === 200 && r.body?.started, r);
  r = await hook(url, { id: 'call-5' }, { 'x-crundi-signature': 'sha256=' + sign('not-the-secret', JSON.stringify({ id: 'call-5' })) });
  t.check('a bad signature is refused', r.status === 401, r);

  // New secret.
  await T.act('schedule-act', 'edit', item(await T.props(), 'Deploy hook').id);
  await T.until('reopened, signed', (p) => D(p)?.hookAuth === 'signature' && D(p).hookSecret === secret);
  await T.act('schedule-rotate');
  await T.until('New secret asks first', (p) => /Calls signed with the old secret stop working/.test(DLG(p)?.title || ''));
  await answer(true);
  p = await T.until('a new secret', (p) => D(p)?.hookSecret && D(p).hookSecret !== secret);
  const raw6 = JSON.stringify({ id: 'call-6' });
  r = await hook(url, { id: 'call-6' }, { 'x-crundi-signature': sign(secret, raw6) });
  t.check('the old secret no longer signs', r.status === 401, r);
  r = await hook(url, { id: 'call-6' }, { 'x-crundi-signature': sign(D(p).hookSecret, raw6) });
  t.check('the new one does', r.status === 200 && r.body?.started, r);
  await T.act('schedule-close');
  await T.act('schedule-reload');
  await T.until('its card shows the last run', (p) => /^last run /.test(item(p, 'Deploy hook')?.last || ''));
}
