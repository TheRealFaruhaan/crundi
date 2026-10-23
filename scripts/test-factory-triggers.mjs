#!/usr/bin/env node
/**
 * test-factory-triggers.mjs — what starts a factory run by itself.
 *
 * Covers: triggers cleaned and stored (ids and webhook keys kept across
 * edits); placeholders filled from the date, a kanban card or a webhook body;
 * a schedule due once at its minute on its days; a busy project skipping or
 * queueing, and the queue started when the project is free; kanban triggers
 * firing only for their column, carrying the card; webhooks refused with a
 * wrong key or when turned off; Run now; every outcome in the history.
 *
 * Run: node scripts/test-factory-triggers.mjs
 */
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const T = await import('../src/factory-triggers.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const dir = mkdtempSync(join(tmpdir(), 'factory-trg-'));

// ─── Cleaning and placeholders ───
const n = T.normalizeTrigger({ kind: 'nope', schedule: { at: '25:00', days: [1, 1, 9, 3] }, cr: { title: 'x', sections: { 'bad key!': 'y', goal: 'g' } }, ifBusy: 'whatever' });
ok(n.kind === 'schedule' && n.schedule.at === '09:00' && n.schedule.days.join() === '1,3' && !('bad key!' in n.cr.sections) && n.ifBusy === 'queue' && n.webhook.key.length > 20, 'normalize: kinds, times, days, section keys, defaults');
const vars = T.varsFor('kanban', { now: new Date(2026, 9, 21, 8, 5), card: { id: 'c1', title: 'Dark mode', description: 'Users ask for it', todos: [{ text: 'Toggle' }, { text: 'Gone', deleted: true }, { text: 'System' }] } });
ok(T.fill('{{card.title}} on {{date}} at {{time}}: {{card.todos}} {{nope}}', vars) === 'Dark mode on 2026-10-21 at 08:05: Toggle\nSystem ', 'fill: card fields, date, time, todos one per line, unknown left empty');
const wv = T.varsFor('webhook', { body: { issue: { title: 'Crash on save', labels: ['bug'] } } });
ok(T.fill('{{body.issue.title}} {{body.issue.labels}}', wv) === 'Crash on save ["bug"]', 'fill: webhook body paths, objects as JSON');
const cr = T.crFor(T.normalizeTrigger({ cr: { title: '  Fix:   {{body.issue.title}} ', sections: { goal: 'Stop the crash: {{body.issue.title}}' } } }), wv);
ok(cr.title === 'Fix: Crash on save' && cr.sections.goal === 'Stop the crash: Crash on save', 'crFor: the change request a firing produces');

// ─── Schedules ───
const sch = T.normalizeTrigger({ kind: 'schedule', schedule: { at: '07:30', days: [1, 2, 3, 4, 5] } });
ok(T.isDue(sch, new Date(2026, 9, 21, 7, 30, 10)), 'schedule: due at its minute on a weekday (a Wednesday)');
ok(!T.isDue(sch, new Date(2026, 9, 21, 7, 31)) && !T.isDue(sch, new Date(2026, 9, 25, 7, 30)), 'schedule: not a minute later, not on a Sunday');
sch.lastFired = new Date(2026, 9, 21, 7, 30, 5).toISOString();
ok(!T.isDue(sch, new Date(2026, 9, 21, 7, 30, 40)), 'schedule: once per minute, however often it is checked');
ok(!T.isDue({ ...sch, enabled: false, lastFired: '' }, new Date(2026, 9, 21, 7, 30)), 'schedule: not when turned off');

// ─── Store ───
const store = T.createTriggerStore(dir);
ok(!store.upsert({ project: 'p', factoryId: 'f', cr: { title: '' } }).ok, 'store: a CR title is required');
const s1 = store.upsert({ name: 'Nightly', project: 'P', factoryId: 'f1', kind: 'schedule', schedule: { at: '02:00' }, cr: { title: 'Nightly tidy {{date}}', sections: { goal: 'Keep it clean' } } });
ok(s1.ok && s1.trigger.project === 'p', 'store: saved, project lower-cased');
const key = s1.trigger.webhook.key;
const s2 = store.upsert({ ...s1.trigger, name: 'Nightly tidy' });
ok(s2.trigger.id === s1.trigger.id && s2.trigger.webhook.key === key && store.list().length === 1, 'store: an edit keeps the id and the key');
ok(T.createTriggerStore(dir).get(s1.trigger.id)?.name === 'Nightly tidy', 'store: on disk');
ok(store.rotateKey(s1.trigger.id).trigger.webhook.key !== key, 'store: a key can be replaced');

// ─── Dispatcher ───
const started = [];
let busy = new Set();
const d = T.createTriggerDispatcher({
  store,
  startRun: async (a) => { started.push(a); busy.add(a.project); return a.cr.title.includes('FAIL') ? { ok: false, error: 'nope' } : { ok: true, run: { id: 'run_' + started.length } }; },
  activeRunFor: (p) => busy.has(p),
});
await d.tick(new Date(2026, 9, 21, 2, 0, 3));
ok(started.length === 1 && started[0].cr.title === 'Nightly tidy 2026-10-21' && started[0].factoryId === 'f1' && started[0].trigger.source === 'schedule', 'tick: a due schedule starts its factory with the filled CR');
await d.tick(new Date(2026, 9, 21, 2, 0, 30));
ok(started.length === 1, 'tick: not twice in the same minute');
ok(store.get(s1.trigger.id).history[0].outcome === 'started' && store.get(s1.trigger.id).history[0].runId === 'run_1', 'history: started, with the run');

// Kanban, while the project is busy: queued, then started when free.
const k = store.upsert({ name: 'From cards', project: 'p', factoryId: 'f2', kind: 'kanban', kanban: { status: 'todo' }, cr: { title: '{{card.title}}', sections: { goal: '{{card.description}}', done: '{{card.todos}}' } } }).trigger;
const card = { id: 'c9', title: 'Dark mode', description: 'People ask', todos: [{ text: 'Toggle' }] };
let r = await d.onKanban('p', card, 'backlog');
ok(r.length === 0, 'kanban: another column does nothing');
r = await d.onKanban('p', card, 'todo');
ok(r[0].queued && store.get(k.id).queue.length === 1 && store.get(k.id).history[0].outcome === 'queued', 'kanban: the project is busy, so it waits its turn');
await d.tick(new Date(2026, 9, 21, 3, 0));
ok(started.length === 1, 'queue: still busy, still waiting');
busy = new Set();
await d.tick(new Date(2026, 9, 21, 3, 1));
const last = started.at(-1);
ok(started.length === 2 && last.cr.title === 'Dark mode' && last.cr.sections.done === 'Toggle' && last.trigger.cardId === 'c9' && /queued/.test(last.trigger.source), 'queue: started once the project is free, with the card');
ok(store.get(k.id).queue.length === 0, 'queue: emptied');

// Skip when busy.
store.upsert({ ...store.get(k.id), ifBusy: 'skip' });
r = await d.onKanban('p', card, 'todo');
ok(!r[0].ok && r[0].skipped && store.get(k.id).history[0].outcome === 'skipped', 'kanban: set to skip, a busy project skips it, and says so');

// Webhooks.
busy = new Set();
const w = store.upsert({ name: 'Issues', project: 'q', factoryId: 'f3', kind: 'webhook', cr: { title: 'Fix: {{body.title}}', sections: {} } }).trigger;
ok((await d.onWebhook(w.id, { key: 'wrong', body: {} })).status === 403, 'webhook: a wrong key is refused');
ok((await d.onWebhook('trg_000000000000', { key: w.webhook.key, body: {} })).status === 404, 'webhook: an unknown trigger');
const wr = await d.onWebhook(w.id, { key: w.webhook.key, body: { title: 'Crash on save' } });
ok(wr.ok && started.at(-1).cr.title === 'Fix: Crash on save' && started.at(-1).project === 'q', 'webhook: the right key starts a run from the body');
store.upsert({ ...store.get(w.id), enabled: false });
ok((await d.onWebhook(w.id, { key: w.webhook.key, body: {} })).status === 409, 'webhook: not when turned off');

// Failures are recorded; Run now.
busy = new Set();
const f = store.upsert({ name: 'Broken', project: 'z', factoryId: 'f4', kind: 'schedule', cr: { title: 'FAIL {{date}}' } }).trigger;
const fr = await d.runNow(f.id);
ok(!fr.ok && store.get(f.id).history[0].outcome === 'failed' && store.get(f.id).history[0].note === 'nope', 'run now: a run that cannot start is recorded with why');
ok(d.preview({ kind: 'kanban', cr: { title: '{{card.title}}!' } }, { card: { title: 'X' } }).title === 'X!', 'preview: the CR as it would come out');

rmSync(dir, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
