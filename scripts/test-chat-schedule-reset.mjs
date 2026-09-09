#!/usr/bin/env node
/**
 * test-chat-schedule-reset.mjs — the "send at limit reset" trigger.
 *
 * Driven by the shape the REAL usage API produces at a rollover, taken from
 * this machine's usage-history.json:
 *
 *   ...T18:45  fiveReset: "2026-09-08T18:50:00.449612+00:00"
 *   ...T18:50  fiveReset: null          <-- no window is open until you send
 *
 * That null is the whole bug. The old code treated it as "forget the anchor",
 * so the rollover was observed and discarded within the same ~30 seconds, and a
 * message with nothing idle to land in waited forever. Nothing threw, and the
 * item sat there looking patiently pending.
 *
 * Run: node scripts/test-chat-schedule-reset.mjs
 */

import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'crundi-cs-'));
process.env.CRUNDI_DATA_DIR = dir;
const { config } = await import('../src/config.js');
config.dataDir = dir;
const { createChatSchedule } = await import('../src/chat-schedule.js');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

const T = Date.parse('2026-09-08T18:50:00.449Z');
let clock = T - 5 * 60_000;
const realNow = Date.now;
Date.now = () => clock;

// A fake session whose idleness we control — the second half of the bug.
let idle = true;
const sent = [];
const claudeUi = {
  list: () => [{ id: 's1', project: 'demo', status: 'running', agentState: idle ? 'idle' : 'working' }],
  sendMessage: (id, text) => { sent.push(text); return { ok: true }; },
};

// The usage snapshot, switched to the real post-rollover shape on cue.
let usage = { ok: true, fiveHour: { resetsAt: '2026-09-08T18:50:00.449612+00:00' }, week: null };
const make = () => createChatSchedule({ claudeUi, getLatestUsage: () => usage });

const sched = make();
sched.add({ project: 'demo', text: 'continue now', trigger: { type: 'limit-reset' } });

// Before the boundary: nothing is owed.
sched.tick();
check('nothing fires before the reset', sent.length === 0, sent.length);

// The boundary passes while the session is BUSY — the case that used to lose it.
idle = false;
clock = T + 1000;
sched.tick();
check('busy session does not consume the reset', sent.length === 0, sent.length);

// The real API now reports no open window. This is the tick that used to
// destroy the anchor and strand the message permanently.
usage = { ok: true, fiveHour: null, week: null };
clock = T + 30_000;
sched.tick();
sched.tick();
check('still owed after the API reports no open window', sent.length === 0, sent.length);

// Minutes later the session frees up. The message should go now.
idle = true;
clock = T + 3 * 60_000;
sched.tick();
check('delivers once the session is idle again', sent.length === 1, sent.join(' | '));

// And exactly once.
sched.tick();
sched.tick();
check('does not deliver twice', sent.length === 1, sent.length);

// A brand-new window opening must not re-arm the consumed rollover.
usage = { ok: true, fiveHour: { resetsAt: '2026-09-08T23:50:00.100000+00:00' }, week: null };
const sched2 = make();
sched2.add({ project: 'demo', text: 'second one', trigger: { type: 'limit-reset' } });
clock = T + 10 * 60_000;
sched2.tick();
check('a fresh window does not fire the next item early', sent.length === 1, sent.length);

// ...but its own rollover does.
clock = Date.parse('2026-09-08T23:50:00.100Z') + 2000;
sched2.tick();
check('the next rollover fires the next item', sent.length === 2, sent.join(' | '));

Date.now = realNow;
rmSync(dir, { recursive: true, force: true });

if (failures) {
  console.error(`\n${failures} failure(s).`);
  process.exit(1);
}
console.log('\nA limit-reset message survives until something can receive it.');
