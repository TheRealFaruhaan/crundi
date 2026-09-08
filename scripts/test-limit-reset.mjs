#!/usr/bin/env node
/**
 * test-limit-reset.mjs — the reset detector and the message lists.
 *
 * Every interesting case here is a way of getting the notification WRONG in a
 * way that still looks like it works: firing twice for one reset, firing on
 * first startup for a reset that never happened, announcing on Friday that
 * Tuesday's window refreshed. None of those throw, and none of them are visible
 * from the code — only from driving the clock.
 *
 * Run: node scripts/test-limit-reset.mjs
 */

import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createLimitResetNotifier, STALE_MS } from '../src/limit-reset-notify.js';
import { FIVE_HOUR, WEEK, pickMessage, rememberPick, AVOID_RECENT, sanitiseGenerated, composePrompt } from '../src/limit-reset-messages.js';

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

// ─── The message lists ───

check('50 five-hour messages', FIVE_HOUR.length === 50, FIVE_HOUR.length);
check('50 weekly messages', WEEK.length === 50, WEEK.length);
check('no duplicate five-hour lines', new Set(FIVE_HOUR).size === FIVE_HOUR.length);
check('no duplicate weekly lines', new Set(WEEK).size === WEEK.length);
check('nothing blank', [...FIVE_HOUR, ...WEEK].every((m) => m.trim().length > 10));

// The picker must not repeat a recent line — the failure this guards is subtle:
// with 50 lines and a blind choice, an immediate repeat is a 1-in-50 event that
// WILL happen, and that is exactly when randomness stops feeling random.
{
  const recent = [];
  let repeats = 0;
  let last = -1;
  let hist = recent;
  for (let i = 0; i < 500; i++) {
    const { index } = pickMessage('five', hist);
    if (index === last) repeats++;
    last = index;
    hist = rememberPick(hist, index);
  }
  check('never repeats back to back over 500 picks', repeats === 0, repeats);
  check('avoid-history stays bounded', hist.length === AVOID_RECENT, hist.length);
}
// A corrupted or over-long history must not leave it with nothing to say.
check('still picks when everything is in the avoid list',
  pickMessage('five', FIVE_HOUR.map((_, i) => i)).text.length > 0);

// ─── The detector ───

const dir = mkdtempSync(join(tmpdir(), 'crundi-limit-'));
const iso = (ms) => new Date(ms).toISOString();

function make({ t0 = Date.UTC(2026, 0, 1, 12, 0, 0), file = 'a.json', graceMs = 0 } = {}) {
  let clock = t0;
  const fired = [];
  const n = createLimitResetNotifier({
    getUsage: async () => usage,
    notify: (text, meta) => fired.push({ text, kind: meta.kind }),
    now: () => clock,
    stateFile: () => join(dir, file),
    graceMs: () => graceMs,
    rand: () => 0.5,
  });
  let usage = { ok: true, fiveHour: null, week: null };
  return {
    fired,
    at: (ms) => { clock = ms; },
    advance: (ms) => { clock += ms; },
    set: (five, week) => {
      usage = {
        ok: true,
        fiveHour: five == null ? null : { resetsAt: iso(five) },
        week: week == null ? null : { resetsAt: iso(week) },
      };
    },
    tick: () => n.tick(),
    offer: (k, t) => n.offer(k, t, clock),
    state: () => n._state(),
  };
}

const T0 = Date.UTC(2026, 0, 1, 12, 0, 0);
const HOUR = 3600_000;

// First sighting is a baseline, never a notification: nothing has reset.
{
  const s = make({ file: 'baseline.json' });
  s.set(T0 + 2 * HOUR, T0 + 100 * HOUR);
  const r = await s.tick();
  check('first sighting only baselines', r.five === 'baselined' && s.fired.length === 0, JSON.stringify(r));
}

// The window lapsing is the reset — even with no new window open behind it,
// which is the normal case for the rolling 5-hour window while you are idle.
{
  const s = make({ file: 'lapse.json' });
  s.set(T0 + 2 * HOUR, T0 + 100 * HOUR);
  await s.tick();
  s.at(T0 + 2 * HOUR + 60_000);
  s.set(null, T0 + 100 * HOUR);          // no next window yet
  const r = await s.tick();
  check('fires when the window lapses with no successor', r.five === 'fired' && s.fired.length === 1,
    s.fired[0] && s.fired[0].text.slice(0, 40));
  check('the message is a five-hour one', s.fired[0] && FIVE_HOUR.includes(s.fired[0].text));

  // The bug that would be invisible in production: firing again every minute
  // until a new window opens.
  s.advance(60_000);
  await s.tick();
  s.advance(60_000);
  await s.tick();
  check('does not fire again while no new window exists', s.fired.length === 1, s.fired.length);
}

// Once the next window opens it is adopted, and its own lapse fires once more.
{
  const s = make({ file: 'next.json' });
  s.set(T0 + 1 * HOUR, T0 + 100 * HOUR);
  await s.tick();
  s.at(T0 + 1 * HOUR + 1000);
  s.set(T0 + 6 * HOUR, T0 + 100 * HOUR);  // reset, new window already open
  await s.tick();
  check('fires once on the rollover', s.fired.length === 1, s.fired.length);
  s.at(T0 + 6 * HOUR + 1000);
  await s.tick();
  check('fires again for the next window', s.fired.length === 2, s.fired.length);
}

// A window that lapsed while Crundi was off is old news, not an announcement.
{
  const s = make({ file: 'stale.json' });
  s.set(T0 + 1 * HOUR, T0 + 100 * HOUR);
  await s.tick();
  s.at(T0 + 1 * HOUR + STALE_MS + 60_000);
  s.set(T0 + 8 * HOUR, T0 + 100 * HOUR);
  const r = await s.tick();
  check('a long-elapsed window is not announced', r.five === 'stale' && s.fired.length === 0, r.five);
  check('but it re-baselines onto the current window',
    s.state().five.watching === iso(T0 + 8 * HOUR), s.state().five.watching);
}

// Both windows are independent, and the weekly one uses the weekly list.
{
  const s = make({ file: 'both.json' });
  // Both must lapse inside the staleness window, or the older one is correctly
  // treated as old news — which is what the first draft of this test tripped on.
  s.set(T0 + 2 * HOUR, T0 + 2 * HOUR + 30_000);
  await s.tick();
  s.at(T0 + 2 * HOUR + 60_000);
  s.set(T0 + 7 * HOUR, T0 + 170 * HOUR);
  await s.tick();
  check('both windows fire when both lapse', s.fired.length === 2, s.fired.length);
  const week = s.fired.find((f) => f.kind === 'week');
  check('the weekly message comes from the weekly list', !!week && WEEK.includes(week.text));
  const five = s.fired.find((f) => f.kind === 'five');
  check('the five-hour message comes from its own list', !!five && FIVE_HOUR.includes(five.text));
}

// A restart between the lapse and the next tick must not re-announce it. Same
// state file, fresh instance — the persisted watch is what prevents a double.
{
  const s1 = make({ file: 'restart.json' });
  s1.set(T0 + 1 * HOUR, T0 + 100 * HOUR);
  await s1.tick();
  s1.at(T0 + 1 * HOUR + 1000);
  s1.set(T0 + 6 * HOUR, T0 + 100 * HOUR);
  await s1.tick();
  check('fired before the restart', s1.fired.length === 1, s1.fired.length);

  const s2 = make({ file: 'restart.json' });
  s2.at(T0 + 1 * HOUR + 2000);
  s2.set(T0 + 6 * HOUR, T0 + 100 * HOUR);
  const r = await s2.tick();
  check('a restart does not re-announce the same reset', s2.fired.length === 0, r.five);
}

// Nothing is fetched, and nothing is written, when there is nowhere to send it.
{
  let fetches = 0;
  const n = createLimitResetNotifier({
    getUsage: async () => { fetches++; return { ok: true, fiveHour: null, week: null }; },
    notify: () => {},
    enabled: () => false,
    stateFile: () => join(dir, 'disabled.json'),
  });
  await n.tick();
  check('disabled means no usage request at all', fetches === 0, fetches);
  check('and no state file written', !existsSync(join(dir, 'disabled.json')));
}

// A failed usage fetch must not be read as a reset.
{
  const n = createLimitResetNotifier({
    getUsage: async () => ({ ok: false, error: 'rate limited' }),
    notify: () => { throw new Error('notified on a failed fetch'); },
    stateFile: () => join(dir, 'err.json'),
  });
  await n.tick();
  check('a failed usage fetch is not a reset', true);
}


// ─── Claude writing its own line ───
//
// Every case here is a reply the CLI calls a success and that must NOT be shown
// to anyone. Rejecting costs one fallback to the list; showing costs a
// notification that reads like a bug report.
{
  const good = "Tokens are back. Go ruin a perfectly good abstraction.";
  check('a clean line survives', sanitiseGenerated(good) === good, sanitiseGenerated(good));
  check('JSON output mode is unwrapped',
    sanitiseGenerated(JSON.stringify({ result: good })) === good);
  check('surrounding quotes are stripped', sanitiseGenerated('"' + good + '"') === good);
  check('curly quotes too', sanitiseGenerated('\u201c' + good + '\u201d') === good);
  check('a preamble plus the line is rejected',
    sanitiseGenerated("Here you go:\n" + good) === '');
  check('a list of options is rejected',
    sanitiseGenerated("1. one liner here\n2. another one here") === '');
  check('a markdown bullet is rejected', sanitiseGenerated('- ' + good) === '');
  check('a refusal is rejected', sanitiseGenerated("I can't help with that request.") === '');
  check('an over-long ramble is rejected', sanitiseGenerated('x'.repeat(400)) === '');
  check('an empty reply is rejected', sanitiseGenerated('   ') === '');
  const prompt = composePrompt('five', ['example one']);
  check('the prompt forbids reusing the examples', /do not reuse/i.test(prompt));
}

// A generated line is preferred; the list covers the grace expiring.
{
  const s = make({ file: 'offer.json' });
  s.set(T0 + 1 * HOUR, T0 + 100 * HOUR);
  await s.tick();
  s.at(T0 + 1 * HOUR + 1000);
  s.set(null, T0 + 100 * HOUR);
  s.offer('five', 'Fresh tokens. Go break the build with confidence.');
  const r = await s.tick();
  check('a written line is used instead of the list', r.five === 'fired-generated'
    && s.fired[0].text === 'Fresh tokens. Go break the build with confidence.', r.five);
}
{
  const s = make({ file: 'grace.json', graceMs: 120_000 });
  s.set(T0 + 1 * HOUR, T0 + 100 * HOUR);
  await s.tick();
  s.at(T0 + 1 * HOUR + 1000);
  s.set(null, T0 + 100 * HOUR);
  const r1 = await s.tick();
  check('holds briefly for a line being written', r1.five === 'deferred' && s.fired.length === 0, r1.five);
  s.at(T0 + 1 * HOUR + 121_000);
  const r2 = await s.tick();
  check('but always says something in the end', r2.five === 'fired' && s.fired.length === 1, r2.five);
  check('and it came from the list', FIVE_HOUR.includes(s.fired[0].text));
}

rmSync(dir, { recursive: true, force: true });

if (failures) {
  console.error(`\n${failures} failure(s).`);
  process.exit(1);
}
console.log('\nLimit resets are announced once, on time, and never for old news.');
