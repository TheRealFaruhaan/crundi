/**
 * usage.js — Real Claude account usage for Crundi.
 *
 * Reads the Claude Code OAuth access token from ~/.claude/.credentials.json and
 * calls Anthropic's own usage endpoint (the same one the `/usage` command uses).
 * This returns REAL, account-wide utilization — not just this app's tokens — as
 * percentages for the rolling 5-hour window and the weekly (7-day) window, with
 * reset timestamps. No local aggregation, no configured budgets needed.
 */

import { readFileSync, existsSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';

const CRED_FILE = () => join(homedir(), '.claude', '.credentials.json');
const USAGE_URL = 'https://api.anthropic.com/api/oauth/usage';
const TTL_OK_MS = 60_000;     // refresh good data at most once a minute (~60 req/hr, well under limit)
const TTL_ERR_MS = 300_000;   // back off 5 min after a failure (e.g. 429) so we don't hammer
const MIN_FETCH_MS = 15_000;  // hard floor: even force can't fetch more often than this (anti-burst)

let cache = { data: null, at: 0 };

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
    return (cache = { data, at: Date.now() }).data;
  } catch (err) {
    return (cache = { data: { ok: false, error: err.message }, at: Date.now() }).data;
  }
}
