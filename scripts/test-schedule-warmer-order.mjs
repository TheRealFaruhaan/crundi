#!/usr/bin/env node
/**
 * test-schedule-warmer-order.mjs — the limit warmer goes first.
 *
 * The rule is an ORDER, not an outcome, and that is what makes it easy to break
 * without noticing: the message still arrives, on time, at the right session.
 * Nothing errors. What is lost is invisible from the delivery — the warmer never
 * pings, so it never opens the window the message was supposed to run into, and
 * never writes the line the announcement was supposed to use.
 *
 * Measured at a real rollover before this was fixed:
 *   boundary   09:50:00.4
 *   delivered  09:50:07.8   <-- schedule's own 15s tick won
 *   last ping  04:50:17     <-- five hours earlier; the warmer never ran
 *
 * Run: node scripts/test-schedule-warmer-order.mjs
 */

import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'crundi-order-'));
process.env.CRUNDI_DATA_DIR = dir;
const { config } = await import('../src/config.js');
config.dataDir = dir;
const { createChatSchedule } = await import('../src/chat-schedule.js');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

const T = Date.parse('2026-09-09T09:50:00.362Z');
let clock = T - 60_000;
const realNow = Date.now;
Date.now = () => clock;

const sent = [];
const claudeUi = {
  list: () => [{ id: 's1', project: 'demo', status: 'running', agentState: 'idle' }],
  sendMessage: (id, text) => { sent.push({ text, at: clock }); return { ok: true }; },
};

function make({ warmerOn = true, file = 'chat-schedule.json' } = {}) {
  let usage = { ok: true, fiveHour: { resetsAt: '2026-09-09T09:50:00.362941+00:00' }, week: null };
  const s = createChatSchedule({
    claudeUi,
    getLatestUsage: () => usage,
    warmerActive: () => warmerOn,
  });
  return { s, setUsage: (u) => { usage = u; } };
}

// ─── The warmer is on: it must be given the rollover first ───
{
  sent.length = 0;
  const { s, setUsage } = make();
  s.add({ project: 'demo', text: 'run me at the reset', trigger: { type: 'limit-reset' } });
  s.tick();                                   // anchor the window
  check('nothing before the boundary', sent.length === 0, sent.length);

  clock = T + 8_000;                          // the instant it used to fire
  setUsage({ ok: true, fiveHour: null, week: null });
  s.tick(); s.tick();
  check('does NOT fire 8s in, ahead of the warmer', sent.length === 0, sent.length);

  clock = T + 90_000;                         // warmer still pinging
  s.tick();
  check('still holding at 90s', sent.length === 0, sent.length);

  // The warmer finishes its whole pass and pushes.
  clock = T + 100_000;
  s.onLimitReset();
  check('fires when the warmer hands it over', sent.length === 1, sent.length);
  check('and it fired at the warmer’s moment', sent[0].at === T + 100_000, sent[0].at - T);

  clock = T + 10 * 60_000;
  s.tick(); s.tick();
  check('not delivered twice', sent.length === 1, sent.length);
}

// ─── The warmer never comes: the net still catches it ───
{
  sent.length = 0;
  const { s, setUsage } = make({ file: 'b.json' });
  rmSync(join(dir, 'chat-schedule.json'), { force: true });
  const s2 = createChatSchedule({
    claudeUi,
    getLatestUsage: () => ({ ok: true, fiveHour: { resetsAt: '2026-09-09T09:50:00.362941+00:00' }, week: null }),
    warmerActive: () => true,
  });
  clock = T - 60_000;
  s2.add({ project: 'demo', text: 'stranded?', trigger: { type: 'limit-reset' } });
  s2.tick();
  clock = T + 30_000;
  s2.tick();
  check('a broken warmer does not strand the message forever', true);
  // The grace runs from when the lapse was NOTICED (T+30s here), not from the
  // boundary — a process that was asleep through the rollover must still leave
  // the warmer its chance rather than skipping straight past it.
  clock = T + 30_000 + 5 * 60_000 + 1000;     // grace elapsed, no push ever came
  s2.tick();
  check('the fallback delivers after the grace', sent.length === 1, sent.length);
}

// ─── No warmer at all: no reason to wait ───
{
  sent.length = 0;
  rmSync(join(dir, 'chat-schedule.json'), { force: true });
  const s3 = createChatSchedule({
    claudeUi,
    getLatestUsage: () => ({ ok: true, fiveHour: { resetsAt: '2026-09-09T09:50:00.362941+00:00' }, week: null }),
    warmerActive: () => false,
  });
  clock = T - 60_000;
  s3.add({ project: 'demo', text: 'no warmer here', trigger: { type: 'limit-reset' } });
  s3.tick();
  clock = T + 2_000;
  s3.tick();
  check('with the warmer off it fires promptly', sent.length === 1, sent.length);
}

// ─── Turning the warmer off takes the queue with it ───
//
// Leaving these queued would leave items that LOOK scheduled but have nothing
// left to notice the reset for them — they would sit until the 2-hour TTL
// expired them silently, which reads as "it forgot" rather than "you turned
// that off".
{
  sent.length = 0;
  rmSync(join(dir, 'chat-schedule.json'), { force: true });
  let warmerOn = true;
  const s4 = createChatSchedule({
    claudeUi,
    getLatestUsage: () => ({ ok: true, fiveHour: { resetsAt: '2026-09-09T09:50:00.362941+00:00' }, week: null }),
    warmerActive: () => warmerOn,
  });
  clock = T - 60_000;
  s4.add({ project: 'demo', text: 'one', trigger: { type: 'limit-reset' } });
  s4.add({ project: 'demo', text: 'two', trigger: { type: 'limit-reset' } });
  s4.add({ project: 'demo', text: 'by time', trigger: { type: 'time', at: new Date(T + 60 * 60_000).toISOString() } });
  check('counts what is queued on the reset', s4.countPendingLimitReset() === 2, s4.countPendingLimitReset());

  const cleared = s4.clearLimitReset();
  warmerOn = false;
  check('clearing reports how many went', cleared === 2, cleared);
  check('none left on the reset', s4.countPendingLimitReset() === 0);
  // The unrelated item must survive: this clears one trigger, not the queue.
  check('a time-triggered message is untouched',
    s4.list().filter(i => i.status === 'pending').length === 1,
    s4.list().filter(i => i.status === 'pending').map(i => i.text).join(','));

  // And the rollover itself is no longer owed to anything.
  clock = T + 30 * 60_000;
  s4.tick(); s4.tick();
  check('nothing is delivered on the reset afterwards', sent.length === 0, sent.length);
}

Date.now = realNow;
rmSync(dir, { recursive: true, force: true });

if (failures) {
  console.error(`\n${failures} failure(s).`);
  process.exit(1);
}
console.log('\nThe warmer finishes first; the schedule runs last.');
