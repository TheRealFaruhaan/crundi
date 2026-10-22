#!/usr/bin/env node
/**
 * test-factory-webhook.mjs — a factory started by a public webhook (an alert
 * from Grafana, a CI call): secured, read into the change request, never
 * processed twice, and handed to the running Manager when it arrives mid-run.
 *
 * Covers: key in URL / header / Bearer; HMAC-SHA256 signatures (hex, sha256=,
 * base64, Grafana's "<timestamp>:<body>" with a freshness check); payload
 * placeholders with arrays and aliases; missing ones left empty and listed, a
 * title that comes out empty replaced, never a failure; the full request kept
 * with credentials taken out; idempotency keys (repeat ignored, a burst lets
 * one through, a failed start lets the retry through); mid-run delivery to
 * the Manager of the same factory, queueing for another factory or an
 * unreachable Manager; the change request's "How this started".
 *
 * Run: node scripts/test-factory-webhook.mjs
 */
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHmac } from 'node:crypto';
import * as T from '../src/factory-triggers.js';
import { checkCr, crMarkdown } from '../src/factory-cr.js';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const dir = mkdtempSync(join(tmpdir(), 'fx-webhook-'));
const mac = (secret, s, enc = 'hex') => createHmac('sha256', secret).update(s).digest(enc);

// ─── Verifying ───
const kt = T.normalizeTrigger({ kind: 'webhook' });
ok(T.verifyWebhook(kt, { key: kt.webhook.key }).ok, 'key: in the URL');
ok(T.verifyWebhook(kt, { headers: { 'x-crundi-key': kt.webhook.key } }).ok, 'key: X-Crundi-Key header');
ok(T.verifyWebhook(kt, { headers: { authorization: 'Bearer ' + kt.webhook.key } }).ok, 'key: Authorization Bearer');
ok(T.verifyWebhook(kt, { key: 'nope' }).status === 403 && T.verifyWebhook(kt, {}).status === 403, 'key: wrong or missing is refused');

const st = T.normalizeTrigger({ kind: 'webhook', webhook: { auth: 'signature' } });
const raw = '{"messageId":"m1","message":"CPU high"}';
const H = st.webhook.signatureHeader;
ok(T.verifyWebhook(st, { raw, headers: { [H]: mac(st.webhook.secret, raw) } }).ok, 'signature: hex');
ok(T.verifyWebhook(st, { raw, headers: { [H]: 'sha256=' + mac(st.webhook.secret, raw) } }).ok, 'signature: sha256=<hex> (GitHub form)');
ok(T.verifyWebhook(st, { raw, headers: { [H]: mac(st.webhook.secret, raw, 'base64') } }).ok, 'signature: base64');
ok(T.verifyWebhook(st, { raw: raw + ' ', headers: { [H]: mac(st.webhook.secret, raw) } }).status === 401, 'signature: a changed body is refused');
ok(T.verifyWebhook(st, { raw, key: st.webhook.key }).status === 401, 'signature: the URL key alone is not enough');
ok(T.verifyWebhook(st, { raw, headers: { [H]: mac('other', raw) } }).status === 401, 'signature: another secret is refused');

const gt = T.normalizeTrigger({ kind: 'webhook', webhook: { auth: 'signature', signatureHeader: 'X-Grafana-Alerting-Signature', timestampHeader: 'X-Grafana-Alerting-Signature-Timestamp' } });
const now = Date.now(); const ts = String(Math.floor(now / 1000));
const gh = (t, sig) => ({ 'x-grafana-alerting-signature': sig, 'x-grafana-alerting-signature-timestamp': t });
ok(T.verifyWebhook(gt, { raw, headers: gh(ts, mac(gt.webhook.secret, ts + ':' + raw)) }, now).ok, 'grafana: signs "<timestamp>:<body>"');
const old = String(Math.floor(now / 1000) - 3600);
ok(T.verifyWebhook(gt, { raw, headers: gh(old, mac(gt.webhook.secret, old + ':' + raw)) }, now).status === 401, 'grafana: an old timestamp is refused (replay)');
ok(T.verifyWebhook(gt, { raw, headers: gh('', mac(gt.webhook.secret, raw)) }, now).status === 401, 'grafana: a missing timestamp is refused');

// ─── Reading the payload ───
const alert = { messageId: 'm1', status: 'firing', alerts: [{ labels: { alertname: 'HighCPU', instance: 'web-1' }, annotations: { summary: 'CPU 97%' } }] };
const v = T.varsFor('webhook', { body: alert, headers: { 'User-Agent': 'Grafana', authorization: 'Bearer x', 'x-grafana-alerting-signature': 'abc' }, query: { key: 'k', env: 'prod' } });
ok(T.fill('{{payload.alerts.0.labels.alertname}} / {{payload.alerts[0].labels.instance}} / {{body.status}}', v) === 'HighCPU / web-1 / firing', 'fill: payload paths, arrays by .0 and [0], body alias');
ok(T.fill('{{headers.user-agent}} {{query.env}} {{query.key}} {{headers.authorization}}', v) === 'Grafana prod  ', 'fill: headers and query, credentials not readable');
ok(T.fill('{{payload.nope.deeper}}|{{payload.alerts.9.x}}|{{payload.messageId.length}}', v) === '||', 'fill: anything missing is empty, never an error');
ok(T.fill('{{payload.constructor}}{{payload.__proto__}}', v) === '', 'fill: no prototype properties');
const trig = T.normalizeTrigger({ name: 'Grafana', kind: 'webhook', cr: { title: '{{payload.title}}', sections: { goal: 'Look at {{payload.alerts.0.labels.alertname}} on {{payload.alerts.0.labels.host}}' } } });
const c = T.crFor(trig, v);
ok(c.title.startsWith('Grafana: webhook at') && c.sections.goal === 'Look at HighCPU on', 'crFor: an empty title becomes the trigger name and time; sections show what they can');
ok(c.missing.join() === 'payload.title,payload.alerts.0.labels.host', 'crFor: lists what the call did not have');
const rec = T.requestRecord({ method: 'POST', headers: { authorization: 'Bearer x', cookie: 'c', 'x-crundi-key': 'k', 'x-hub-signature-256': 's', 'content-type': 'application/json' }, query: { key: 'k', a: '1' }, body: alert });
ok(!rec.headers.authorization && !rec.headers.cookie && !rec.headers['x-crundi-key'] && !rec.headers['x-hub-signature-256'] && rec.headers['content-type'] && !rec.query.key && rec.query.a === '1' && rec.body.alerts.length === 1, 'request: kept in full, credentials taken out');
const textRec = T.requestRecord({ body: null, raw: 'plain text alert' });
ok(textRec.body === 'plain text alert', 'request: a non-JSON body is kept as text');

// ─── Dispatching ───
const store = T.createTriggerStore(dir);
const started = []; const told = []; let active = null; let tellFails = false;
const d = T.createTriggerDispatcher({
  store,
  startRun: async (a) => { started.push(a); if (a.cr.title.includes('FAIL')) return { ok: false, error: 'nope' }; return { ok: true, run: { id: 'run_' + started.length } }; },
  activeRunFor: () => active,
  tellRun: async (runId, x) => { if (tellFails) return { ok: false, error: 'not running' }; told.push({ runId, ...x }); return { ok: true }; },
});
const w = store.upsert({ name: 'Alerts', project: 'p', factoryId: 'f1', kind: 'webhook', webhook: { idempotency: '{{payload.messageId}}' }, cr: { title: 'Alert: {{payload.alerts.0.labels.alertname}}', sections: { goal: '{{payload.message}}' } } }).trigger;
ok(w.ifBusy === 'manager', 'webhook triggers default to telling the running Manager');
const call = (body, extra = {}) => d.onWebhook(w.id, { key: w.webhook.key, body, raw: JSON.stringify(body), headers: { 'content-type': 'application/json' }, query: {}, ...extra });

let r = await call({ ...alert, message: 'CPU 97% on web-1' });
const s0 = started.at(-1);
ok(r.ok && s0.cr.title === 'Alert: HighCPU' && s0.cr.sections.goal === 'CPU 97% on web-1' && s0.trigger.key === 'm1', 'start: filled from the payload, with its key');
ok(s0.files.length === 1 && s0.files[0].name === 'webhook-request.json' && JSON.parse(s0.files[0].content).body.messageId === 'm1', 'start: the full request goes with it as a file');
ok(/Idempotency key: m1/.test(s0.cr.context) && /"message": "CPU 97% on web-1"/.test(s0.cr.context), 'start: "How this started" names the key and shows the body');
const checked = checkCr(s0.cr, null, { lenient: true });
ok(checked.ok && checked.cr.context && /## How this started/.test(crMarkdown(checked.cr, null)), 'cr: the context survives checking and reaches the Manager\'s first message');

r = await call({ ...alert, message: 'again' });
ok(r.ok && r.duplicate && started.length === 1 && store.get(w.id).history[0].outcome === 'duplicate', 'idempotency: a repeat is answered and not processed');

const burst = await Promise.all([1, 2, 3].map(() => call({ ...alert, messageId: 'm2' })));
ok(burst.filter(x => x.duplicate).length === 2 && started.length === 2, 'idempotency: a burst of the same call lets one through');

r = await call({ message: 'no id here', alerts: [] });
ok(r.ok && started.length === 3 && /No idempotency key was found/.test(started.at(-1).cr.context), 'idempotency: a call without a key is still processed, and the Manager is told');

// Mid-run, same factory: the Manager hears about it.
active = { id: 'run_live', factoryId: 'f1' };
r = await call({ ...alert, messageId: 'm3', message: 'still firing' });
ok(r.ok && r.told && r.runId === 'run_live' && told.length === 1 && started.length === 3, 'mid-run: handed to the running Manager, not a new run');
ok(told[0].cr.sections.goal === 'still firing' && told[0].idem === 'm3' && told[0].request.body.messageId === 'm3', 'mid-run: with the filled request, its key and the full request');
ok(store.get(w.id).history[0].outcome === 'told the Manager', 'mid-run: recorded');
r = await call({ ...alert, messageId: 'm3' });
ok(r.duplicate && told.length === 1, 'mid-run: a repeat of it is still ignored');

// Mid-run, another factory: queued for later.
active = { id: 'run_other', factoryId: 'f9' };
r = await call({ ...alert, messageId: 'm4' });
ok(r.ok && r.queued && told.length === 1 && store.get(w.id).queue.length === 1, 'another factory is running: queued, not sent to a stranger');
// The Manager cannot be reached: queued too.
active = { id: 'run_live', factoryId: 'f1' }; tellFails = true;
r = await call({ ...alert, messageId: 'm5' });
ok(r.ok && r.queued && store.get(w.id).queue.length === 2 && /Could not reach/.test(store.get(w.id).history[0].note), 'Manager unreachable: queued, and says why');
tellFails = false;
// Free again: the queued ones start, with their requests.
active = null;
await d.tick(new Date(2026, 0, 1, 3, 3));
ok(started.length === 4 && started.at(-1).trigger.key === 'm4' && started.at(-1).files.length === 1, 'queue: started when free, with the request and key');

// A start that fails lets the sender retry.
const fw = store.upsert({ name: 'Broken', project: 'z', factoryId: 'f1', kind: 'webhook', webhook: { idempotency: '{{payload.id}}' }, cr: { title: 'FAIL {{payload.id}}' } }).trigger;
const fcall = () => d.onWebhook(fw.id, { key: fw.webhook.key, body: { id: 'x1' }, raw: '{"id":"x1"}' });
r = await fcall();
ok(!r.ok && (await fcall()).ok === false && started.filter(s => s.cr.title === 'FAIL x1').length === 2, 'failed start: the key is released, so a retry is tried again');

// Never crash on odd bodies.
r = await d.onWebhook(w.id, { key: w.webhook.key, body: { text: 'not json' }, raw: 'not json' });
ok(r.ok, 'odd body: still handled');
r = await d.onWebhook(w.id, { key: w.webhook.key, body: null, raw: '' });
ok(r.ok, 'empty body: still handled');

// Preview, for the editor.
const p = d.preview({ kind: 'webhook', webhook: { idempotency: '{{payload.groupKey}}' }, cr: { title: '{{payload.title}}' } }, { body: { title: 'T' } });
ok(p.title === 'T' && p.idem === '' && p.idemMissing.join() === 'payload.groupKey', 'preview: shows the key and what is missing');
ok(!('seen' in T.publicTrigger(store.get(w.id))) && T.publicTrigger(store.get(w.id)).seenCount >= 4, 'public: remembered keys are not sent to the browser');
const before = { ...store.get(w.id).webhook };
const sig = store.rotateKey(w.id, 'secret');
ok(sig.ok && sig.trigger.webhook.secret !== before.secret && sig.trigger.webhook.key === before.key, 'secret: can be replaced on its own');

rmSync(dir, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
