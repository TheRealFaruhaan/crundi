/**
 * voice/settings.js — the voice assistant's settings and its monthly spend.
 *
 * Kept in <dataDir>/voice.json, private to this user (0600). The API keys
 * (Claude, Deepgram) live here and only here: they are never sent to a phone
 * or a browser. publicSettings() is what clients see — whether a key is set,
 * never the key.
 *
 * Spend is counted per billing month, in US dollars, from the usage each
 * Claude response and each Deepgram transcription reports. The month starts
 * on billingDay (the day the API account is billed, 1-31; in a shorter month,
 * its last day) at 00:00 UTC. When the month's spend reaches the limit, the
 * assistant stops answering and says so (D25); a new month starts from zero.
 */

import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { config } from '../config.js';
import { writePrivateFile } from '../private-fs.js';

/**
 * Models offered in Voice assistant settings (D8). Prices: US$ per million
 * tokens. Cache writes cost 1.25x input for 5 minutes and 2x for 1 hour; the
 * agent writes 1-hour entries, so a write reported without the breakdown is
 * priced as 1 hour.
 */
export const MODELS = {
  'claude-haiku-5-5': { label: 'Claude Haiku 5.5', input: 0.10, output: 0.50, cacheRead: 0.01 },
  'claude-sonnet-5-5': { label: 'Claude Sonnet 5.5', input: 2.00, output: 10.00, cacheRead: 0.20 },
  'claude-opus-5-5': { label: 'Claude Opus 5.5', input: 4.00, output: 20.00, cacheRead: 0.20 },
};
export const DEFAULT_MODEL = 'claude-haiku-5-5';
export const DEFAULT_LIMIT_USD = 50;

/** Deepgram Nova-3 streaming + keyterms, US$ per minute (approximate; D37). */
export const DEEPGRAM_USD_PER_MIN = 0.0077 + 0.0013;
/** Deepgram Aura-2 text-to-speech, US$ per 1,000 characters (approximate). */
export const DEEPGRAM_TTS_USD_PER_1K_CHARS = 0.030;

const DEFAULTS = {
  model: DEFAULT_MODEL,
  anthropicKey: '',
  deepgramKey: '',
  // A Deepgram Aura voice (e.g. aura-2-draco-en), or '' for the device's own.
  ttsVoice: '',
  monthlyLimitUsd: DEFAULT_LIMIT_USD,
  // The day of the month the API account is billed: the spend month starts then.
  billingDay: 1,
  spend: { month: '', claudeUsd: 0, deepgramUsd: 0, turns: 0 },
};

const FILE = () => join(config.dataDir, 'voice.json');
let cache = null;

function load() {
  if (cache) return cache;
  let saved = {};
  try { if (existsSync(FILE())) saved = JSON.parse(readFileSync(FILE(), 'utf-8')); } catch { saved = {}; }
  cache = { ...DEFAULTS, ...saved, spend: { ...DEFAULTS.spend, ...(saved.spend || {}) } };
  return cache;
}

function save() {
  writePrivateFile(FILE(), JSON.stringify(cache, null, 2));
}

export function thisMonth(now = new Date()) {
  return now.toISOString().slice(0, 7);
}

/** `day` of the month y/m (0-based), or the month's last day when it has fewer. */
function dayIn(y, m, day) {
  const last = new Date(Date.UTC(y, m + 1, 0)).getUTCDate();
  return new Date(Date.UTC(y, m, Math.min(day, last)));
}

/**
 * The billing month `now` falls in: { start, next } as Dates (00:00 UTC).
 * With billingDay 1 that is the calendar month.
 */
export function billingPeriod(now = new Date(), billingDay = 1) {
  const day = Math.min(31, Math.max(1, Math.round(Number(billingDay) || 1)));
  const y = now.getUTCFullYear(), m = now.getUTCMonth();
  let start = dayIn(y, m, day);
  if (start > now) start = dayIn(y, m - 1, day);
  const next = dayIn(start.getUTCFullYear(), start.getUTCMonth() + 1, day);
  return { start, next };
}

const ymd = (d) => d.toISOString().slice(0, 10);

/** The month's spend, reset to zero when a new billing month has begun. */
function spendNow(now) {
  const s = load();
  const key = ymd(billingPeriod(now, s.billingDay).start);
  if (s.spend.month !== key) {
    // Spend counted before billing months (a calendar month, "2026-11") carries
    // over into the period starting that month's first day, so nothing is lost.
    const carried = s.spend.month === key.slice(0, 7) && key.endsWith('-01');
    s.spend = carried ? { ...s.spend, month: key } : { month: key, claudeUsd: 0, deepgramUsd: 0, turns: 0 };
  }
  return s.spend;
}

/** The whole record, keys included. Server side only. */
export function getSettings() {
  return load();
}

/** What a client may see: no keys, only whether each is set. */
export function publicSettings(now = new Date()) {
  const s = load();
  const spend = spendNow(now);
  const total = spend.claudeUsd + spend.deepgramUsd;
  const period = billingPeriod(now, s.billingDay);
  return {
    model: s.model,
    billingDay: s.billingDay,
    periodStart: ymd(period.start),
    nextReset: ymd(period.next),
    models: Object.entries(MODELS).map(([id, m]) => ({ id, label: m.label })),
    anthropicKeySet: !!s.anthropicKey,
    deepgramKeySet: !!s.deepgramKey,
    ttsVoice: s.ttsVoice || '',
    monthlyLimitUsd: s.monthlyLimitUsd,
    spend: { ...spend, totalUsd: round(total), claudeUsd: round(spend.claudeUsd), deepgramUsd: round(spend.deepgramUsd) },
    limitReached: total >= s.monthlyLimitUsd,
  };
}

/**
 * Change settings. Keys: a string sets it, '' clears it, undefined leaves it.
 * Returns { ok, error? }.
 */
export function updateSettings(patch = {}) {
  const s = load();
  if (patch.model !== undefined) {
    if (!MODELS[patch.model]) return { ok: false, error: `Unknown model: ${patch.model}` };
    s.model = patch.model;
  }
  if (patch.monthlyLimitUsd !== undefined) {
    const n = Number(patch.monthlyLimitUsd);
    if (!Number.isFinite(n) || n < 0 || n > 100000) return { ok: false, error: 'The monthly limit must be a number of dollars from 0 to 100000' };
    s.monthlyLimitUsd = n;
  }
  if (patch.billingDay !== undefined) {
    const d = Number(patch.billingDay);
    if (!Number.isInteger(d) || d < 1 || d > 31) return { ok: false, error: 'The billing day is a day of the month, 1 to 31' };
    s.billingDay = d;
  }
  if (patch.ttsVoice !== undefined) {
    const v = String(patch.ttsVoice || '');
    if (v && !/^aura(-2)?-[a-z0-9]+-[a-z]{2}$/.test(v)) return { ok: false, error: `Not a Deepgram voice: ${v}` };
    s.ttsVoice = v;
  }
  for (const k of ['anthropicKey', 'deepgramKey']) {
    if (patch[k] !== undefined) s[k] = String(patch[k]).trim();
  }
  save();
  return { ok: true };
}

/** Claude's price for one response's usage, in US$. */
export function claudeCost(model, usage = {}) {
  const p = MODELS[model] || MODELS[DEFAULT_MODEL];
  const M = 1e6;
  const written = usage.cache_creation_input_tokens || 0;
  const by = usage.cache_creation || {};
  const fiveMin = Math.min(written, by.ephemeral_5m_input_tokens || 0);
  return ((usage.input_tokens || 0) * p.input
    + (usage.output_tokens || 0) * p.output
    + (usage.cache_read_input_tokens || 0) * p.cacheRead
    + fiveMin * p.input * 1.25
    + (written - fiveMin) * p.input * 2) / M;
}

export function addClaudeSpend(model, usage, now = new Date()) {
  const spend = spendNow(now);
  spend.claudeUsd += claudeCost(model, usage);
  save();
}

export function countTurn(now = new Date()) {
  spendNow(now).turns += 1;
  save();
}

export function addDeepgramSpend(seconds, now = new Date()) {
  const spend = spendNow(now);
  spend.deepgramUsd += (Math.max(0, seconds) / 60) * DEEPGRAM_USD_PER_MIN;
  save();
}

export function addDeepgramTtsSpend(chars, now = new Date()) {
  const spend = spendNow(now);
  spend.deepgramUsd += (Math.max(0, chars) / 1000) * DEEPGRAM_TTS_USD_PER_1K_CHARS;
  save();
}

/** Has this month's spend reached the limit? */
export function overLimit(now = new Date()) {
  const s = load();
  const spend = spendNow(now);
  return spend.claudeUsd + spend.deepgramUsd >= s.monthlyLimitUsd;
}

/** Tests only: forget the in-memory copy so the file is read again. */
export function _reload() { cache = null; }

function round(n) { return Math.round(n * 10000) / 10000; }
