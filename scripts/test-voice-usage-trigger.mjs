#!/usr/bin/env node
/**
 * test-voice-usage-trigger.mjs — Cru's usage_above trigger
 * (src/voice/wakeups.js) and the readings that feed it (src/usage.js):
 * Claude Code's own rate_limit_event parsed into a reading, listeners told
 * of each one, the trigger firing once per crossing, re-armed only clearly
 * below, not again in the same window after a restart, retried after a
 * cooldown, and the add_trigger tool.
 * Pure node, no network. Run: node scripts/test-voice-usage-trigger.mjs
 */
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
process.env.DATA_DIR = mkdtempSync(join(tmpdir(), 'cru-usage-'));
const W = await import('../src/voice/wakeups.js');
const U = await import('../src/usage.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const throws = (fn) => { try { fn(); return false; } catch { return true; } };
const MIN = 60 * 1000, HOUR = 60 * MIN;

// ─── Claude Code's rate_limit_event (the real shape, from claude 2.1) ───
const ev = (five, week, extra = {}) => ({
  type: 'rate_limit_event',
  rate_limit_info: { status: 'allowed', rateLimitType: 'seven_day', utilization: week, resetsAt: 1794430800, unifiedWindows: { five_hour: { utilization: five, resetsAt: 1794369600, status: 'allowed' }, seven_day: { utilization: week, resetsAt: 1794430800, status: 'allowed_warning', surpassedThreshold: 0.75 } }, ...extra },
});
let r = U.parseRateLimitEvent(ev(0.09, 0.85));
ok(r && r.fiveHour.utilization === 9 && r.week.utilization === 85, 'fractions become percents');
ok(r.fiveHour.resetsAt === new Date(1794369600 * 1000).toISOString(), 'resetsAt in seconds becomes an ISO time');
r = U.parseRateLimitEvent({ type: 'rate_limit_event', rate_limit_info: { rateLimitType: 'five_hour', utilization: 0.91, resetsAt: 1794369600 } });
ok(r && r.fiveHour.utilization === 91 && !r.week, 'an older CLI: only the window it names');
ok(U.parseRateLimitEvent({ type: 'rate_limit_event', rate_limit_info: { status: 'allowed' } }) === null, 'no numbers: no reading');
ok(U.parseRateLimitEvent({ type: 'assistant' }) === null, 'other lines are ignored');

const heard = [];
const off = U.onUsageReading((x) => heard.push(x));
U.noteRateLimitEvent(ev(0.5, 0.6));
ok(heard.length === 1 && heard[0].source === 'claude' && heard[0].fiveHour.utilization === 50, 'a listener hears each reading, with its source');
const latest = U.getLatestReading();
ok(latest.week.utilization === 60 && latest.source === 'claude' && latest.at > 0, 'getLatestReading keeps the newest');
U.noteRateLimitEvent({ type: 'rate_limit_event', rate_limit_info: { rateLimitType: 'five_hour', utilization: 0.7 } });
ok(U.getLatestReading().fiveHour.utilization === 70 && U.getLatestReading().week.utilization === 60, 'a one-window reading keeps the other window');
off();
U.noteRateLimitEvent(ev(0.1, 0.1));
ok(heard.length === 2, 'unsubscribed: no longer told');

// ─── the store ───
let clock = new Date(2026, 10, 11, 9, 0).getTime();
const now = () => clock;
const file = join(process.env.DATA_DIR, 'voice-wakeups.json');
const store = W.createWakeupStore({ file, now });
ok(throws(() => store.addTrigger({ name: 'u', purpose: 'x', condition: { kind: 'usage_above', usageWindow: 'month', percent: 90 } })), 'an unknown window is refused');
ok(throws(() => store.addTrigger({ name: 'u', purpose: 'x', condition: { kind: 'usage_above', usageWindow: 'week', percent: 0 } })), 'a percent out of range is refused');
const wrap = store.addTrigger({ name: 'wrap up', purpose: 'Tell job chats to wrap up', condition: { kind: 'usage_above', usageWindow: '5h', percent: 90 }, enabled: true, cooldown: 5 });
ok(wrap.condition.usageWindow === 'five_hour', '"5h" means the five-hour window');
ok(/five-hour usage at 90% or more/.test(wrap.summary), 'its summary: ' + wrap.summary);
const stop = store.addTrigger({ name: 'stop', purpose: 'Stop job chats', condition: { kind: 'usage_above', usageWindow: 'five_hour', percent: 95 }, enabled: true });
const week = store.addTrigger({ name: 'week', purpose: 'Weekly warning', condition: { kind: 'usage_above', usageWindow: 'weekly', percent: 80 }, enabled: true });

// ─── the engine ───
const notes = [];
const mkEngine = () => W.createWakeupEngine({ store, now, maxPerHour: 20, report: async (note, item) => { notes.push({ note, id: item.id }); return { ok: true, said: 'ok' }; } });
let engine = mkEngine();
const reset5 = new Date(clock + 3 * HOUR).toISOString();
const resetW = new Date(clock + 3 * 24 * HOUR).toISOString();
const read = (five, wk, source = 'api') => ({ fiveHour: five == null ? null : { utilization: five, resetsAt: reset5 }, week: wk == null ? null : { utilization: wk, resetsAt: resetW }, source });
const step = async (rd) => { const out = engine.usage(rd); await Promise.all(out); await engine.idle(); return out.length; };

ok(await step(read(70, 50)) === 0 && notes.length === 0, 'below every threshold: nothing');
await step(read(90, 50));
ok(notes.length === 1 && notes[0].id === wrap.id, 'reaching 90% wakes the 90% trigger, not the 95% one');
ok(/five-hour usage is at 90%/.test(notes[0].note) && /the Usage API/.test(notes[0].note), 'the note says the window, the figure and the source');
clock += 10 * MIN;
await step(read(91, 50, 'claude'));
await step(read(89.6, 50, 'claude'));
await step(read(90, 50));
ok(notes.length === 1, 'once per crossing: no flapping between 89.6 and 90');
await step(read(96, 50, 'claude'));
ok(notes.length === 2 && notes[1].id === stop.id && /Claude Code/.test(notes[1].note), 'then 95% wakes the stop trigger, from a Claude Code reading');
await step(read(null, 82));
ok(notes.length === 3 && notes[2].id === week.id, 'a weekly reading alone checks only the weekly trigger');

// A restart inside the same window: the latch is gone, but it already fired.
engine = mkEngine();
clock += 10 * MIN;
await step(read(97, 85));
ok(notes.length === 3, 'after a restart, not again in the same window');

// The window resets: usage falls, then climbs again.
clock += 3 * HOUR;
const reset5b = new Date(clock + 5 * HOUR).toISOString();
engine.usage({ fiveHour: { utilization: 3, resetsAt: reset5b } });
clock += 2 * HOUR;
await step({ fiveHour: { utilization: 92, resetsAt: reset5b } });
ok(notes.length === 4 && notes[3].id === wrap.id, 'after the window resets, it fires again');

// Blocked by its cooldown: tried again on a later reading, not lost.
const cool = store.addTrigger({ name: 'cool', purpose: 'x', condition: { kind: 'usage_above', usageWindow: 'week', percent: 50 }, enabled: true, cooldown: 30 });
store.markFired(cool.id, clock - 5 * MIN);
const before = notes.length;
await step(read(null, 60));
ok(notes.length === before, 'cooling down: not fired');
clock += 30 * MIN;
await step(read(null, 60));
ok(notes.length === before + 1 && notes.at(-1).id === cool.id, 'after the cooldown, the next reading fires it');

// Off: nothing.
store.setEnabled(cool.id, false);
await step(read(null, 10)); await step(read(null, 99));
ok(notes.at(-1).id === cool.id && notes.length === before + 1, 'a trigger switched off is not woken');

// ─── the tool ───
const defs = W.wakeupToolDefs({ store, engine });
const add = defs.find(d => d.name === 'add_trigger');
ok(add.input_schema.properties.usage_window && /usage_above/.test(add.description), 'add_trigger offers usage_above and usage_window');
r = await add.run({ name: 'tool', purpose: 'x', kind: 'usage_above', usage_window: 'week', percent: 95, enabled: true });
ok(r.ok && store.find('tool').condition.usageWindow === 'week' && store.find('tool').condition.percent === 95, 'add_trigger makes a usage_above trigger');
r = await add.run({ name: 'tool2', purpose: 'x', kind: 'usage_above', percent: 95 });
ok(r.ok === false && /usageWindow/.test(r.error), 'without a window it is refused with a reason');

console.log(failed ? `\n${failed} FAILED` : '\nAll usage trigger tests pass.');
process.exit(failed ? 1 : 0);
