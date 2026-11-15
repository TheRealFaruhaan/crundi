#!/usr/bin/env node
/**
 * test-usage-view.mjs — the usage meter through a rate-limited API
 * (src/usage.js): a 429 keeps the last good reading (marked stale, with its
 * real time) instead of leaving clients on whatever they had, Claude Code's
 * newer report wins over it, Retry-After is honoured (capped), and the next
 * good answer replaces it all.
 * Pure node: fetch and the credentials file are faked. Run: node scripts/test-usage-view.mjs
 */
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const home = mkdtempSync(join(tmpdir(), 'cru-usage-view-'));
process.env.HOME = home;
process.env.USERPROFILE = home;   // os.homedir() on Windows (the release jobs run there)
process.env.DATA_DIR = join(home, 'data');
mkdirSync(join(home, '.claude'), { recursive: true });
writeFileSync(join(home, '.claude', '.credentials.json'), JSON.stringify({ claudeAiOauth: { accessToken: 't', refreshToken: 'r', expiresAt: Date.now() + 3600e3, subscriptionType: 'max' } }));

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

let calls = 0, next = null;
globalThis.fetch = async () => { calls++; return next(); };
const good = (five, week) => () => ({ ok: true, status: 200, headers: new Map(), json: async () => ({ five_hour: { utilization: five, resets_at: '2026-11-15T19:30:00Z' }, seven_day: { utilization: week, resets_at: '2026-11-18T21:00:00Z' } }) });
const limited = (retry) => () => ({ ok: false, status: 429, headers: new Map(retry == null ? [] : [['retry-after', String(retry)]]), json: async () => ({}) });

const U = await import('../src/usage.js');
const realNow = Date.now; let clock = realNow();
Date.now = () => clock;
const MIN = 60_000;

// retryAfterMs
ok(U.retryAfterMs(null) === 5 * MIN, 'no Retry-After: the default 5 min');
ok(U.retryAfterMs('600') === 10 * MIN, 'Retry-After seconds honoured');
ok(U.retryAfterMs('7200') === 15 * MIN, 'a long Retry-After is capped at 15 min');
ok(U.retryAfterMs('30') === 5 * MIN, 'a short one still waits the default');

// Nothing known yet: the error comes back.
next = limited(null);
let v = await U.getUsageView();
ok(v.ok === false && /429/.test(v.error), 'nothing known: the error');

// A good reading, then a 429: the good one, stale.
clock += 6 * MIN; next = good(40, 70);
v = await U.getUsageView();
ok(v.ok && !v.stale && v.fiveHour.utilization === 40, 'a good reading is live');
const goodAt = v.fetchedAt;
clock += 2 * MIN; next = limited(600);
v = await U.getUsageView();
ok(v.ok && v.stale && v.fiveHour.utilization === 40 && v.fetchedAt === goodAt && /429/.test(v.error), 'a 429 serves the last good reading, stale, with its own time');
ok(v.subscriptionType === 'max', 'the stale reading keeps the plan');

// Retry-After 600 s: no fetch for 10 minutes.
const before = calls;
clock += 6 * MIN; await U.getUsageView();
ok(calls === before, 'Retry-After 10 min: not asked again at 6 min');
clock += 5 * MIN; next = limited(null); await U.getUsageView();
ok(calls === before + 1, 'asked again after 10 min');

// Claude Code's report, newer than the last API reading, wins.
U.noteRateLimitEvent({ type: 'rate_limit_event', rate_limit_info: { unifiedWindows: { five_hour: { utilization: 0.04, resetsAt: 1794369600 }, seven_day: { utilization: 0.01, resetsAt: 1794430800 } } } });
v = U.bestKnown();
ok(v.ok && v.stale && v.fromClaude && v.fiveHour.utilization === 4 && v.week.utilization === 1, "Claude Code's newer report wins");
ok(Date.parse(v.fetchedAt) === clock, "its time is the report's");

// The API answers again: live, not stale.
clock += 6 * MIN; next = good(5, 2);
v = await U.getUsageView();
ok(v.ok && !v.stale && v.fiveHour.utilization === 5 && v.week.utilization === 2, 'the next good answer is live again');

// After a restart (memory empty): the stored history stands in.
U._resetCache(); U._resetLatest();
clock += 6 * MIN; next = limited(null);
v = await U.getUsageView();
ok(v.ok && v.stale && v.fiveHour.utilization === 5, 'after a restart a 429 serves the stored reading');

Date.now = realNow;
if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nall passed');
