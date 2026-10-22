#!/usr/bin/env node
/**
 * test-schedule-hooks.mjs — scheduled chats started by a webhook.
 *
 * Covers: the store keeping the generated key and secret (never taken from
 * input, replaced only by rotate); the clock never firing a webhook schedule;
 * key and signature checks; the prompt filled from the payload with missing
 * fields empty and named and the full request always included; idempotency;
 * a call arriving while the chat runs going into it marked Mid-run (also
 * while the chat is still being opened); a new run once it closed itself;
 * a chat left open still getting calls; unmet conditions not claiming the
 * key; command and agent actions refused.
 *
 * Run: node scripts/test-schedule-hooks.mjs
 */
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHmac } from 'node:crypto';

const dir = mkdtempSync(join(tmpdir(), 'sched-hooks-'));
process.env.DATA_DIR = dir;
const store = await import('../src/schedule-store.js');
const { isDue } = await import('../src/scheduler.js');
const { createScheduleHooks } = await import('../src/schedule-hooks.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const tick = () => new Promise(r => setTimeout(r, 20));

// ─── Store ───
const add = store.addSchedule({ name: 'Alerts', project: 'Demo', when: { mode: 'webhook' }, action: { kind: 'chat', prompt: 'Look into {{payload.alerts.0.labels.alertname}} on {{payload.alerts.0.labels.host}}: {{payload.message}}', session: 'resume', sessionId: 'conv-1' }, conditions: [{ type: 'usage', metric: '5h', op: 'below', value: 90 }], webhook: { idempotency: '{{payload.groupKey}}', key: 'mine', secret: 'mine' } });
const sch = add.schedule;
ok(add.ok && sch.when.mode === 'webhook' && sch.webhook.key.length > 20 && sch.webhook.key !== 'mine' && sch.webhook.secret !== 'mine', 'store: webhook mode, key and secret generated, never taken from input');
const up = store.updateSchedule(sch.id, { webhook: { auth: 'signature', key: 'x' } });
ok(up.schedule.webhook.key === sch.webhook.key && up.schedule.webhook.auth === 'signature' && up.schedule.webhook.idempotency === '{{payload.groupKey}}' && up.schedule.name === 'Alerts', 'store: an update keeps key, secret, other settings and the name');
store.updateSchedule(sch.id, { webhook: { auth: 'key' } });
const rot = store.rotateHookKey(sch.id, 'key');
ok(rot.ok && rot.schedule.webhook.key !== sch.webhook.key && rot.schedule.webhook.secret === sch.webhook.secret, 'store: the key is replaced on its own');
ok(!isDue({ ...rot.schedule, when: { mode: 'webhook', time: new Date().toTimeString().slice(0, 5), days: [0, 1, 2, 3, 4, 5, 6] } }), 'scheduler: the clock never fires a webhook schedule');

// ─── Hooks ───
const sessions = [];
const sent = [];
const runs = [];
let finishRun = null;
const claudeUi = {
  list: () => sessions,
  sendMessage: (id, text) => { sent.push({ id, text }); return { ok: true }; },
};
let met = true;
const hooks = createScheduleHooks({
  dataDir: dir,
  getSchedule: (id) => store.getSchedule(id),
  runChat: (s, { prompt, onStarted }) => new Promise((resolve) => {
    runs.push({ prompt });
    const id = 'chat' + runs.length;
    setTimeout(() => { sessions.push({ id, status: 'running' }); onStarted(id); }, 30);
    finishRun = (outcome) => { if (outcome === 'finished') sessions.splice(sessions.findIndex(x => x.id === id), 1); resolve({ outcome, sessionId: id }); };
  }),
  conditionMet: () => met,
  claudeUi,
});
const key = () => store.getSchedule(sch.id).webhook.key;
const call = (body, extra = {}) => hooks.onWebhook(sch.id, { key: key(), body, raw: JSON.stringify(body), headers: { 'content-type': 'application/json', authorization: 'Bearer nope' }, query: {}, ...extra });
const alert = (g, msg) => ({ groupKey: g, message: msg, alerts: [{ labels: { alertname: 'HighCPU' } }] });

ok((await hooks.onWebhook(sch.id, { key: 'wrong', body: {} })).status === 403, 'auth: a wrong key is refused');
ok((await hooks.onWebhook('0000000000000000', { key: key(), body: {} })).status === 404, 'auth: an unknown schedule');

met = false;
let r = await call(alert('g1', 'CPU 97%'));
ok(r.ok && r.skipped && runs.length === 0, 'conditions unmet: answered, not run');
met = true;
r = await call(alert('g1', 'CPU 97%'));
ok(r.ok && r.started && runs.length === 1, 'started: the same key works once the conditions are met (not claimed while skipped)');
const p0 = runs[0].prompt;
ok(p0.startsWith('Look into HighCPU on : CPU 97%'), 'prompt: filled from the payload, a missing field empty');
ok(/Not in this call.*payload\.alerts\.0\.labels\.host/.test(p0) && /Idempotency key: g1/.test(p0), 'prompt: names what was missing and the key');
ok(/"groupKey": "g1"/.test(p0) && !/Bearer nope/.test(p0), 'prompt: the full request is included, credentials removed');

// While the chat is still being opened: held, then delivered.
r = await call(alert('g2', 'CPU 99%'));
ok(r.ok && r.midRun && runs.length === 1, 'mid-run (chat still opening): no second chat');
await new Promise(res => setTimeout(res, 60));
ok(sent.length === 1 && sent[0].id === 'chat1' && /Mid-run/.test(sent[0].text) && /CPU 99%/.test(sent[0].text), 'mid-run: delivered into the chat once it exists');
r = await call(alert('g3', 'Disk 95%'));
ok(r.midRun && sent.length === 2 && /Disk 95%/.test(sent[1].text), 'mid-run: straight into the running chat');
r = await call(alert('g3', 'Disk 95%'));
ok(r.duplicate && sent.length === 2, 'idempotency: a repeat is ignored');

// Left open (needs you): later calls still go to it.
finishRun('needs-input'); await tick();
r = await call(alert('g4', 'Memory'));
ok(r.midRun && sent.at(-1).id === 'chat1', 'left open: calls still go to that chat');
// It closes itself after a run: the next call starts a new one.
r = await call(alert('g5', 'x'));
finishRun('finished'); await tick();
sessions.length = 0;
r = await call(alert('g6', 'New day'));
ok(r.started && runs.length === 2, 'closed: the next call starts a new run');
finishRun('finished'); await tick();

// No body, odd body: still handled.
r = await hooks.onWebhook(sch.id, { key: key(), body: null, raw: '' });
ok(r.ok && runs.length === 3 && /No idempotency key was found/.test(runs[2].prompt), 'no body: still runs, says repeats cannot be told apart');
finishRun('finished'); await tick();

// Signature mode.
store.updateSchedule(sch.id, { webhook: { auth: 'signature' } });
const secret = store.getSchedule(sch.id).webhook.secret;
const raw = JSON.stringify(alert('g7', 'signed'));
const sig = createHmac('sha256', secret).update(raw).digest('hex');
r = await hooks.onWebhook(sch.id, { raw, body: JSON.parse(raw), headers: { 'x-crundi-signature': sig } });
ok(r.ok && r.started, 'signature: a signed call runs');
r = await hooks.onWebhook(sch.id, { raw, body: JSON.parse(raw), key: key() });
ok(r.status === 401, 'signature: the key alone is not enough');
finishRun('finished'); await tick();

// Only chat and service actions.
const cmd = store.addSchedule({ name: 'Cmd', project: 'demo', when: { mode: 'webhook' }, action: { kind: 'command', command: 'echo {{payload.x}}' } }).schedule;
ok((await hooks.onWebhook(cmd.id, { key: cmd.webhook.key, body: {} })).status === 409, 'a command action is refused (caller text in a shell)');

rmSync(dir, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
