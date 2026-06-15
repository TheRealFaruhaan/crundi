/**
 * usage.js — Real Claude account usage for Crundi.
 *
 * Reads the Claude Code OAuth access token from ~/.claude/.credentials.json and
 * calls Anthropic's own usage endpoint (the same one the `/usage` command uses).
 * This returns REAL, account-wide utilization — not just this app's tokens — as
 * percentages for the rolling 5-hour window and the weekly (7-day) window, with
 * reset timestamps. No local aggregation, no configured budgets needed.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import { config } from './config.js';

const CRED_FILE = () => join(homedir(), '.claude', '.credentials.json');
const USAGE_URL = 'https://api.anthropic.com/api/oauth/usage';
const TTL_OK_MS = 60_000;     // refresh good data at most once a minute (~60 req/hr, well under limit)
const TTL_ERR_MS = 300_000;   // back off 5 min after a failure (e.g. 429) so we don't hammer
const MIN_FETCH_MS = 15_000;  // hard floor: even force can't fetch more often than this (anti-burst)

const FIVE_H_MS = 5 * 60 * 60 * 1000;
const WEEK_MS = 7 * 24 * 60 * 60 * 1000;
const HISTORY_FILE = () => join(config.dataDir, 'usage-history.json');
const HISTORY_MIN_GAP_MS = 5 * 60 * 1000;  // append at most every 5 min (unless utilization moved)
const HISTORY_MAX = 6000;                  // ~3 weeks at 5-min cadence; trimmed beyond this
const HISTORY_TTL_MS = 30 * 24 * 60 * 60 * 1000; // drop records older than 30 days

let cache = { data: null, at: 0 };
let history = null; // lazy-loaded array of records

function ttlFor(data) { return data && data.ok ? TTL_OK_MS : TTL_ERR_MS; }

function readOAuth() {
  try {
    if (!existsSync(CRED_FILE())) return null;
    const j = JSON.parse(readFileSync(CRED_FILE(), 'utf-8'));
    return j.claudeAiOauth || null;
  } catch { return null; }
}

function normWindow(w) {
  if (!w || typeof w.utilization !== 'number') return null;
  return { utilization: Math.round(w.utilization), resetsAt: w.resets_at || null };
}

/**
 * "Optimal" pace at a moment: the % of the rolling window already elapsed.
 * If usage tracks below this line you're spending slower than the clock.
 */
function optimalPct(resetsAt, windowMs, now = Date.now()) {
  if (!resetsAt) return null;
  const reset = new Date(resetsAt).getTime();
  if (!Number.isFinite(reset)) return null;
  const start = reset - windowMs;
  return Math.max(0, Math.min(100, Math.round(((now - start) / windowMs) * 100)));
}

function loadHistory() {
  if (history) return history;
  try {
    if (existsSync(HISTORY_FILE())) {
      const d = JSON.parse(readFileSync(HISTORY_FILE(), 'utf-8'));
      history = Array.isArray(d) ? d : (Array.isArray(d.records) ? d.records : []);
    } else history = [];
  } catch { history = []; }
  return history;
}

function saveHistory() {
  try {
    if (!existsSync(config.dataDir)) mkdirSync(config.dataDir, { recursive: true });
    writeFileSync(HISTORY_FILE(), JSON.stringify(history));
  } catch { /* non-fatal */ }
}

/** Append a usage sample, throttled so the log stays compact. */
function recordHistory(data) {
  if (!data || !data.ok) return;
  const h = loadHistory();
  const now = Date.now();
  const rec = {
    at: now,
    five: data.fiveHour ? data.fiveHour.utilization : null,
    week: data.week ? data.week.utilization : null,
    fiveOpt: data.fiveHour ? optimalPct(data.fiveHour.resetsAt, FIVE_H_MS, now) : null,
    weekOpt: data.week ? optimalPct(data.week.resetsAt, WEEK_MS, now) : null,
    fiveReset: data.fiveHour ? data.fiveHour.resetsAt : null,
    weekReset: data.week ? data.week.resetsAt : null,
  };
  const last = h[h.length - 1];
  const moved = !last || last.five !== rec.five || last.week !== rec.week;
  if (last && !moved && now - last.at < HISTORY_MIN_GAP_MS) return; // throttle flat samples
  h.push(rec);
  // trim by age and size
  const cutoff = now - HISTORY_TTL_MS;
  let trimmed = h.filter(r => r.at >= cutoff);
  if (trimmed.length > HISTORY_MAX) trimmed = trimmed.slice(trimmed.length - HISTORY_MAX);
  history = trimmed;
  saveHistory();
}

/** Most recent stored sample (for instant display on load). */
export function getLatestStored() {
  const h = loadHistory();
  const last = h[h.length - 1];
  if (!last) return { ok: false, empty: true };
  return {
    ok: true,
    fromHistory: true,
    fetchedAt: new Date(last.at).toISOString(),
    fiveHour: last.five == null ? null : { utilization: last.five, resetsAt: last.fiveReset },
    week: last.week == null ? null : { utilization: last.week, resetsAt: last.weekReset },
  };
}

/** History records since `sinceMs` ago (default 7 days). */
export function getHistory(sinceMs = WEEK_MS) {
  const h = loadHistory();
  const cutoff = Date.now() - sinceMs;
  return h.filter(r => r.at >= cutoff);
}

/**
 * Fetch the real usage. Cached for 60s. Pass { force:true } to bypass the cache.
 * @returns {Promise<object>} { ok, fiveHour, week, weekOpus, weekSonnet, ... }
 */
export async function getUsage({ force = false } = {}) {
  if (cache.data) {
    const age = Date.now() - cache.at;
    if (!force && age < ttlFor(cache.data)) return cache.data;
    if (force && age < MIN_FETCH_MS) return cache.data; // throttle bursts even when forced
  }

  const oauth = readOAuth();
  if (!oauth || !oauth.accessToken) {
    return (cache = { data: { ok: false, error: 'Not signed in to Claude (no OAuth token found)' }, at: Date.now() }).data;
  }

  try {
    const res = await fetch(USAGE_URL, {
      headers: {
        Authorization: `Bearer ${oauth.accessToken}`,
        'anthropic-beta': 'oauth-2025-04-20',
        'anthropic-version': '2023-06-01',
        'Content-Type': 'application/json',
      },
    });
    if (res.status === 401) {
      return (cache = { data: { ok: false, error: 'Claude session expired — use Claude once to refresh it', status: 401 }, at: Date.now() }).data;
    }
    if (!res.ok) {
      return (cache = { data: { ok: false, error: `Usage API returned ${res.status}` }, at: Date.now() }).data;
    }
    const j = await res.json();
    const data = {
      ok: true,
      subscriptionType: oauth.subscriptionType || null,
      rateLimitTier: oauth.rateLimitTier || null,
      fiveHour: normWindow(j.five_hour),
      week: normWindow(j.seven_day),
      weekOpus: normWindow(j.seven_day_opus),
      weekSonnet: normWindow(j.seven_day_sonnet),
      extraUsage: j.extra_usage || null,
      fetchedAt: new Date().toISOString(),
    };
    recordHistory(data);
    return (cache = { data, at: Date.now() }).data;
  } catch (err) {
    return (cache = { data: { ok: false, error: err.message }, at: Date.now() }).data;
  }
}
