#!/usr/bin/env node
/**
 * test-voice-wakeups.mjs — Cru's tickers and triggers (src/voice/wakeups.js):
 * the store, time windows, schedules, cooldowns, the hourly cap, the spend
 * limit, condition evaluation from fake samples and events, busy retries,
 * silence, and the tools (safe: no confirmation, even locked).
 * Pure node, no network. Run: node scripts/test-voice-wakeups.mjs
 */
import { mkdtempSync, existsSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
process.env.DATA_DIR = mkdtempSync(join(tmpdir(), 'cru-wake-'));
const W = await import('../src/voice/wakeups.js');
const { makeActionTools, createPending } = await import('../src/voice/actions.js');
const { reportPrompt, SYSTEM_PROMPT } = await import('../src/voice/agent.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const throws = (fn) => { try { fn(); return false; } catch { return true; } };
const at = (y, mo, d, h, mi) => new Date(y, mo - 1, d, h, mi).getTime();
const MIN = 60 * 1000;

// ─── time windows ───
ok(W.inWindow(null, new Date(at(2026, 11, 9, 3, 0))), 'no window: always');
ok(W.inWindow({ from: '07:00', to: '12:00' }, new Date(at(2026, 11, 9, 8, 0))), 'inside a morning window');
ok(!W.inWindow({ from: '07:00', to: '12:00' }, new Date(at(2026, 11, 9, 12, 0))), 'the end is outside');
ok(W.inWindow({ from: '22:00', to: '06:00' }, new Date(at(2026, 11, 9, 23, 30))), 'a night window, before midnight');
ok(W.inWindow({ from: '22:00', to: '06:00' }, new Date(at(2026, 11, 10, 2, 0))), 'a night window, after midnight');
ok(!W.inWindow({ from: '22:00', to: '06:00' }, new Date(at(2026, 11, 10, 12, 0))), 'a night window, at noon: outside');
// 9 Nov 2026 is a Monday.
ok(W.inWindow({ days: [1] }, new Date(at(2026, 11, 9, 12, 0))) && !W.inWindow({ days: [2] }, new Date(at(2026, 11, 9, 12, 0))), 'days');
ok(W.inWindow({ from: '22:00', to: '06:00', days: [1] }, new Date(at(2026, 11, 10, 2, 0))), "a night belongs to the day it began (Monday night, Tuesday 02:00)");
ok(W.parseHm('7:30') === 450 && W.parseHm('25:00') === null, 'parseHm');
ok(W.parseAt('2026-11-10T07:30') === at(2026, 11, 10, 7, 30), 'parseAt: local time');

// ─── the store ───
let clock = at(2026, 11, 9, 7, 0);
const now = () => clock;
const file = join(process.env.DATA_DIR, 'voice-wakeups.json');
let changes = 0;
const store = W.createWakeupStore({ file, now, onChange: () => { changes++; } });
const morning = store.addTicker({ name: 'morning', purpose: 'Say good morning and give a briefing', at: '07:30', days: ['mon', 'tue', 'wed', 'thu', 'fri'] }, 'owner');
ok(morning.enabled === false, 'a ticker is off by default');
ok(morning.createdBy === 'owner' && /daily at 07:30 on weekdays/.test(morning.summary), 'its summary: ' + morning.summary);
ok(throws(() => store.addTicker({ name: 'fast', purpose: 'x', every: 2 })), `every under ${W.MIN_EVERY_MIN} minutes is refused`);
ok(throws(() => store.addTicker({ name: 'Morning', purpose: 'x', every: 10 })), 'names are unique (any case)');
ok(throws(() => store.addTicker({ name: 'nopurpose', every: 10 })), 'a purpose is required');
ok(throws(() => store.addTrigger({ name: 'shell', purpose: 'x', condition: { kind: 'shell', command: 'rm -rf /' } })), 'an unknown condition kind (no arbitrary commands) is refused');
ok(throws(() => store.addTrigger({ name: 'cpu', purpose: 'x', condition: { kind: 'cpu_above', percent: 150 } })), 'a percent out of range is refused');
ok(existsSync(file) && JSON.parse(readFileSync(file, 'utf-8')).tickers.length === 1 && changes >= 1, 'saved to the data dir, and the change announced');
const again = W.createWakeupStore({ file, now });
ok(again.find('MORNING') && again.find(morning.id).name === 'morning', 'found again by id or name after a reload');

// ─── tickers: when they are due ───
ok(!W.tickerDue(store.find('morning'), new Date(at(2026, 11, 9, 7, 30))), 'off: never due');
clock = at(2026, 11, 9, 7, 0);
store.setEnabled('morning', true);
ok(!W.tickerDue(store.find('morning'), new Date(at(2026, 11, 9, 7, 29))), 'not before its time');
ok(W.tickerDue(store.find('morning'), new Date(at(2026, 11, 9, 7, 31))), 'due at its time');
ok(!W.tickerDue(store.find('morning'), new Date(at(2026, 11, 9, 9, 0))), 'a time missed by over 30 minutes waits for the next day');
ok(!W.tickerDue(store.find('morning'), new Date(at(2026, 11, 14, 7, 31))), 'not on a Saturday');
clock = at(2026, 11, 9, 7, 45);
store.setEnabled('morning', false); store.setEnabled('morning', true);
ok(!W.tickerDue(store.find('morning'), new Date(at(2026, 11, 9, 7, 50))), 'switched on after its time today: not until tomorrow');
const every = store.addTicker({ name: 'pulse', purpose: 'Check the build', every: 15, from: '09:00', to: '17:00', enabled: true });
ok(every.enabled && /every 15 minutes, between 09:00 and 17:00/.test(every.summary), 'an every-N ticker, enabled by Cru at once: ' + every.summary);
ok(!W.tickerDue(store.find('pulse'), new Date(clock + 20 * MIN)), 'every-N: outside its window (08:05)');
store.remove('pulse');

// ─── the engine: firing, silence, cap, cooldown, limit ───
const reports = [];
let answer = { ok: true, quiet: true };
let over = false;
const engine = W.createWakeupEngine({ store, now, overLimit: () => over, maxPerHour: 4, report: async (note, item) => { reports.push({ note, item }); return typeof answer === 'function' ? answer() : answer; } });
store.setEnabled('morning', false);
clock = at(2026, 11, 10, 6, 0);
store.setEnabled('morning', true);
clock = at(2026, 11, 10, 7, 31);
await Promise.all(engine.scan());
ok(reports.length === 1 && /\[Ticker "morning"/.test(reports[0].note) && /purpose: Say good morning/.test(reports[0].note), 'a due ticker wakes Cru with its name and purpose');
ok(store.find('morning').fires === 1 && store.find('morning').lastFiredAt === clock, 'it is stamped');
await Promise.all(engine.scan());
ok(reports.length === 1, 'and not again the same morning');

const cpu = store.addTrigger({ name: 'cpu', purpose: 'Warn if the server is struggling', condition: { kind: 'cpu_above', percent: 90, forMinutes: 5 }, enabled: true });
ok(cpu.cooldown === W.DEFAULT_COOLDOWN_MIN && /CPU above 90% for 5 min/.test(cpu.summary), 'a trigger: default cooldown, summary ' + cpu.summary);
reports.length = 0;
clock = at(2026, 11, 10, 10, 0);
await Promise.all(engine.sample({ cpuPct: 95 }));
clock += 3 * MIN; await Promise.all(engine.sample({ cpuPct: 96 }));
ok(reports.length === 0, 'CPU high for 3 of 5 minutes: nothing yet');
clock += 2 * MIN; await Promise.all(engine.sample({ cpuPct: 93 }));
ok(reports.length === 1 && /CPU is at 93%, above 90% for 5 min/.test(reports[0].note), 'high for 5 minutes: fired, saying what happened');
clock += 1 * MIN; await Promise.all(engine.sample({ cpuPct: 97 }));
ok(reports.length === 1, 'still high: once per crossing');
clock += 1 * MIN; await Promise.all(engine.sample({ cpuPct: 20 }));
clock += 1 * MIN; await Promise.all(engine.sample({ cpuPct: 99 }));
clock += 6 * MIN; await Promise.all(engine.sample({ cpuPct: 99 }));
ok(reports.length === 1, 'down and up again within the cooldown: not fired');
clock += 30 * MIN; await Promise.all(engine.sample({ cpuPct: 10 })); await Promise.all(engine.sample({ cpuPct: 99 }));
clock += 6 * MIN; await Promise.all(engine.sample({ cpuPct: 99 }));
ok(reports.length === 2, 'after the cooldown, a new crossing fires again');

// The global cap (4 an hour here), shared by every trigger; a fresh engine.
store.remove(cpu.id);
const chatTrig = store.addTrigger({ name: 'done', purpose: 'Say when the API chat finishes', condition: { kind: 'chat_finished', project: 'api' }, cooldown: 5, enabled: true });
const other = store.addTrigger({ name: 'any-done', purpose: 'Any chat finishing', condition: { kind: 'chat_finished' }, cooldown: 5, enabled: true });
const capped = [];
const capEng = W.createWakeupEngine({ store, now, overLimit: () => over, maxPerHour: 4, report: async (note) => { capped.push(note); return { ok: true }; } });
const fin = (project = 'api') => capEng.event('chat_finished', { chat: { id: 'c1', title: 'Tests', project } });
clock = at(2026, 11, 10, 12, 0);
for (let k = 0; k < 4; k++) { clock += 6 * MIN; await Promise.all(fin()); }
ok(capped.length === 4 && capEng.limits().firedLastHour === 4, 'the hourly cap holds across triggers (2 match each time, 4 in all)');
clock += 61 * MIN;
await Promise.all(fin());
ok(capped.length === 6, 'an hour later there is room again');
clock += 6 * MIN;
await Promise.all(fin('web'));
ok(capped.length === 7, 'a chat in another project matches only the unfiltered trigger');
over = true; clock += 10 * MIN;
await Promise.all(fin());
ok(capped.length === 7, 'none once the spend limit is reached');
over = false;
store.remove(other.id);

// Busy: given back and tried again, not counted.
store.remove(chatTrig.id);
const waiting = store.addTrigger({ name: 'waits', purpose: 'Tell me when any chat needs me', condition: { kind: 'chat_waiting' }, enabled: true });
clock += 61 * MIN;
reports.length = 0;
answer = { ok: false, busy: true };
await Promise.all(engine.event('chat_waiting', { chat: { id: 'c3', title: 'Deploy', project: 'ops' }, what: 'a question' }));
ok(reports.length === 1 && engine.limits().firedLastHour === 0 && store.find(waiting.id).fires === 0, 'Cru busy: not counted, not stamped');
answer = { ok: true, said: 'screen' };
await Promise.all(engine.scan());
ok(reports.length === 2 && /waiting on Sir: a question/.test(reports[1].note) && store.find(waiting.id).fires === 1, 'retried on the next scan, with what happened');
await Promise.all(engine.scan());
ok(reports.length === 2, 'and only once');
// A collaborator's chat is not the owner's: only collaborator_action sees it.
await Promise.all(engine.event('chat_waiting', { chat: { id: 'c9', title: 'X', project: 'ops', collaborator: { id: 'k', name: 'Ali' } } }));
ok(reports.length === 2, "a collaborator's chat does not fire a chat trigger");
store.remove(waiting.id);

// Sample-based transitions: chats, services, containers, processes, collaborators.
const mk = (kind, extra = {}) => store.addTrigger({ name: kind, purpose: 'p ' + kind, condition: { kind, ...extra }, cooldown: 5, enabled: true });
mk('chat_opened', { match: 'deploy' }); mk('chat_closed'); mk('service_stopped', { service: 'api' }); mk('container_unhealthy');
mk('process_started', { name: 'ffmpeg' }); mk('process_stopped', { name: 'ffmpeg' }); mk('collaborator_action', { collaborator: 'ali' });
ok(engine.kindsOn().has('container_unhealthy') && engine.kindsOn().has('process_started'), 'kindsOn says what to read');
const eng2Reports = [];
const eng2 = W.createWakeupEngine({ store, now, maxPerHour: 100, report: async (note) => { eng2Reports.push(note); return { ok: true }; } });
clock += 2 * 60 * MIN;
const base = {
  chats: [{ id: 'a', title: 'Fix login', project: 'web' }],
  services: [{ key: 'web/api', name: 'api', status: 'running' }, { key: 'web/db', name: 'db', status: 'running' }],
  containers: [{ name: 'pg', health: 'healthy' }],
  processes: new Set(['node', 'bash']),
};
await Promise.all(eng2.sample(base));
ok(eng2Reports.length === 0, 'the first reading only sets the baseline');
clock += 6 * MIN;
await Promise.all(eng2.sample({
  chats: [{ id: 'b', title: 'Deploy prod', project: 'ops' }, { id: 'k1', title: 'Help', project: 'web', collaborator: { id: 'c-ali', name: 'Ali' } }],
  services: [{ key: 'web/api', name: 'api', status: 'crashed' }, { key: 'web/db', name: 'db', status: 'running' }],
  containers: [{ name: 'pg', health: 'unhealthy' }],
  processes: new Set(['node', 'bash', 'ffmpeg']),
}));
const has = (re) => eng2Reports.some(n => re.test(n));
ok(has(/"Deploy prod" chat in ops was opened/), 'chat_opened (title match)');
ok(has(/"Fix login" chat in web was closed/), 'chat_closed');
ok(has(/service "api" crashed/), 'service_stopped');
ok(has(/container "pg" is unhealthy/), 'container_unhealthy');
ok(has(/process called "ffmpeg" started/), 'process_started');
ok(has(/Ali opened a chat in web/), 'collaborator_action');
ok(!has(/ffmpeg" is no longer/), 'process_stopped not yet');
ok(eng2Reports.length === 6, 'one wake-up per trigger: ' + eng2Reports.length);
clock += 6 * MIN;
await Promise.all(eng2.sample({ chats: [], services: [], containers: [], processes: new Set(['node']) }));
ok(has(/"ffmpeg" is no longer running/), 'process_stopped');
// A field left out is forgotten, so it never compares against a stale reading.
const before = eng2Reports.length;
await Promise.all(eng2.sample({ chats: [] }));
clock += 6 * MIN;
await Promise.all(eng2.sample({ chats: [], processes: new Set(['ffmpeg']) }));
ok(eng2Reports.length === before, 'a reading after a gap sets a new baseline, no false start');
for (const x of store.list().triggers) store.remove(x.id);

// A one-off time: fires once and switches off; a long-missed one is dropped.
const once = store.addTrigger({ name: 'call', purpose: 'Remind Sir about the call', condition: { kind: 'at', datetime: '2026-11-12T15:00' }, enabled: true });
ok(once.once === true && /once at 2026-11-12 15:00/.test(once.summary), "'at' is a one-off");
reports.length = 0; answer = { ok: true };
clock = at(2026, 11, 12, 14, 59); await Promise.all(engine.scan());
ok(reports.length === 0, 'not before its time');
clock = at(2026, 11, 12, 15, 0); await Promise.all(engine.scan());
ok(reports.length === 1 && /has come/.test(reports[0].note) && !store.find(once.id).enabled, 'at its time: fired, then off');
store.addTrigger({ name: 'old', purpose: 'x', condition: { kind: 'at', datetime: '2026-11-01T09:00' }, enabled: true });
await Promise.all(engine.scan());
ok(reports.length === 1 && !store.find('old').enabled, 'missed by days: dropped, not said late');

// A time window on a trigger.
const windowed = store.addTrigger({ name: 'morning-chats', purpose: 'x', condition: { kind: 'chat_finished' }, from: '06:00', to: '12:00', enabled: true });
clock = at(2026, 11, 12, 18, 0);
await Promise.all(engine.event('chat_finished', { chat: { id: 'z', title: 'Z', project: 'p' } }));
ok(reports.length === 1, 'outside its window: not fired');
clock = at(2026, 11, 13, 8, 0);
await Promise.all(engine.event('chat_finished', { chat: { id: 'z', title: 'Z', project: 'p' } }));
ok(reports.length === 2 && /between 06:00 and 12:00/.test(store.find(windowed.id).summary || W.describe(store.find(windowed.id))), 'inside it: fired');

// One at a time: two wake-ups at once run one after the other.
let running = 0, most = 0;
const serial = W.createWakeupEngine({ store, now, report: async () => { running++; most = Math.max(most, running); await new Promise(r => setTimeout(r, 20)); running--; return { ok: true }; } });
store.addTrigger({ name: 'a1', purpose: 'x', condition: { kind: 'chat_waiting' }, enabled: true });
store.addTrigger({ name: 'a2', purpose: 'y', condition: { kind: 'chat_waiting' }, enabled: true });
await Promise.all(serial.event('chat_waiting', { chat: { id: 'q', title: 'Q', project: 'p' } }));
ok(most === 1, 'wake-ups are handed to Cru one at a time');

// ─── the tools: through actions.js, safe (no yes, even locked) ───
const alerts = [];
const tools = makeActionTools({
  listProjects: () => [], chats: () => [],
  wakeupDefs: W.wakeupToolDefs({ store, engine, alert: async (a) => { alerts.push(a); return { ok: true, delivered: 'phone' }; } }),
}, { pending: createPending(), locked: () => true });
const tool = (n) => tools.find(t => t.name === n);
ok(W.WAKEUP_TOOL_NAMES.every(n => tool(n)), 'every wake-up tool is offered: ' + W.WAKEUP_TOOL_NAMES.join(', '));
let r = await tool('add_ticker').run({ name: 'standup', purpose: 'Remind Sir of the standup', at: '09:55', days: ['mon'] });
ok(r.ok && r.ticker.enabled === false && !r.needsConfirmation && !r.needsUnlock, 'add_ticker: runs locked, off by default');
r = await tool('set_ticker_enabled').run({ ticker: 'standup', enabled: true });
ok(r.ok && r.ticker.enabled === true && store.find('standup').enabled, 'set_ticker_enabled: Cru switches it on itself, no confirmation');
r = await tool('add_trigger').run({ name: 'proc', purpose: 'Say when the render finishes', kind: 'process_stopped', process: 'ffmpeg', enabled: true });
ok(r.ok && r.trigger.enabled && store.find('proc').condition.name === 'ffmpeg', 'add_trigger with typed params');
r = await tool('add_trigger').run({ name: 'bad', purpose: 'x', kind: 'process_stopped' });
ok(r.ok === false && /name/.test(r.error), 'a bad trigger is refused with a reason');
r = await tool('list_wakeups').run({});
ok(r.ok && r.tickers.some(x => x.name === 'standup') && r.triggers.some(x => x.name === 'proc') && r.limits.perHour === 4, 'list_wakeups');
r = await tool('set_trigger_enabled').run({ trigger: 'proc', enabled: false });
ok(r.ok && !store.find('proc').enabled, 'set_trigger_enabled off');
r = await tool('update_ticker').run({ ticker: 'standup', every: 30 });
ok(r.ok && store.find('standup').every === 30 && !store.find('standup').at, 'update_ticker changes the schedule');
r = await tool('delete_trigger').run({ trigger: 'proc' });
ok(r.ok && !store.find('proc'), 'delete_trigger');
r = await tool('delete_ticker').run({ ticker: 'standup' });
ok(r.ok && !store.find('standup'), 'delete_ticker');
r = await tool('alert_owner').run({ title: 'API is down', urgent: true });
ok(r.ok && alerts[0].urgent === true && alerts[0].title === 'API is down', 'alert_owner goes to the alert path');

// ─── what Cru is told ───
const p = reportPrompt('[Ticker "morning"]', { wake: true });
ok(/reply with nothing at all/.test(p) && /alert_owner/.test(p) && /switch it off/.test(p), 'a wake-up lets Cru stay quiet, alert, or switch it off');
ok(/Tell the owner now/.test(reportPrompt('x')), 'a chat report is still always told');
ok(/Tickers and triggers/.test(SYSTEM_PROMPT) && /switch it off/.test(SYSTEM_PROMPT), "Cru's instructions cover them, and switching them off");

console.log(failed ? `\n${failed} FAILED` : '\nAll wake-up tests pass.');
process.exit(failed ? 1 : 0);
