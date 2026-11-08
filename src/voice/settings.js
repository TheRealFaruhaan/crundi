/**
 * voice/settings.js — the voice assistant's settings and its monthly spend.
 *
 * Kept in <dataDir>/voice.json, private to this user (0600). The API keys
 * (Claude, Deepgram) live here and only here: they are never sent to a phone
 * or a browser. publicSettings() is what clients see — whether a key is set,
 * never the key.
 *
 * Spend is counted per billing month, in US dollars, from the usage each
 * Claude response and each Deepgram transcription reports. Claude and
 * Deepgram bill separately, so each has its own month: Claude's starts on
 * billingDay (the day the Claude API account is billed), Deepgram's on
 * deepgramBillingDay; 1-31, a shorter month's last day when it lacks the
 * day, at 00:00 UTC. The limit is on the two together. When the month's spend reaches the limit, the
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
/** Deepgram's own monthly limit (D46). */
export const DEFAULT_DEEPGRAM_LIMIT_USD = 200;

/** Deepgram Nova-3 streaming + keyterms, US$ per minute (approximate; D37). */
export const DEEPGRAM_USD_PER_MIN = 0.0077 + 0.0013;
/** Deepgram Aura-2 text-to-speech, US$ per 1,000 characters (approximate). */
export const DEEPGRAM_TTS_USD_PER_1K_CHARS = 0.030;

/** What answers for Cru (an experiment): the Claude API, or the owner's own Claude Code in JSON mode. */
export const BRAINS = ['api', 'claude-code'];

const DEFAULTS = {
  model: DEFAULT_MODEL,
  brain: 'api',
  anthropicKey: '',
  deepgramKey: '',
  // Optional: an Anthropic Admin API key, only to read the account's own usage report.
  anthropicAdminKey: '',
  // A Deepgram Aura voice (e.g. aura-2-draco-en), or '' for the device's own.
  ttsVoice: '',
  // Claude's monthly limit (the voice assistant stops answering at it).
  monthlyLimitUsd: DEFAULT_LIMIT_USD,
  // Deepgram's own (at it, listening and speaking fall back to the phone's own).
  deepgramLimitUsd: DEFAULT_DEEPGRAM_LIMIT_USD,
  // The day of the month each account is billed: its spend month starts then.
  billingDay: 1,
  deepgramBillingDay: 1,
  // How long the owner may pause before what they said is sent (D44), ms.
  pauseMs: 2000,
  // How fast Cru talks: Deepgram Aura-2's speed (0.7 to 1.5), and the phone's
  // or browser's own voice at the same rate.
  speechRate: 1,
  // How loud Cru talks, 10 to 100 (%): its replies and cues on every client.
  volume: 100,
  // Full admin mode (9 Nov): Cru may run any command as the user Crundi runs
  // as. Switched on by hand only (Settings), never by Cru; Sonnet or above.
  adminMode: false,
  // "Only act on my voice" (speaker.js): with the owner's voice enrolled, a
  // spoken request in another voice gets answers but changes nothing.
  ownerOnly: true,
  // After this many seconds of silence the voice screen closes (every client;
  // "Hey Cru" opens it again).
  screenCloseSeconds: 60,
  // After this many minutes without a turn the Claude Code brain's process stops.
  cliIdleMinutes: 30,
  // month: the Claude month's first day; deepgramMonth: Deepgram's.
  spend: { month: '', deepgramMonth: '', claudeUsd: 0, deepgramUsd: 0, turns: 0 },
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

/**
 * Spend counted before billing months was keyed by calendar month ("2026-11"):
 * it carries over into a period that starts on that month's first day.
 */
const carries = (was, key) => !!was && was === key.slice(0, 7) && key.endsWith('-01');

/** The month's spend; each provider's part restarts when its own billing month does. */
function spendNow(now) {
  const s = load();
  const sp = s.spend;
  const legacy = sp.deepgramMonth === undefined || sp.deepgramMonth === '' ? sp.month : sp.deepgramMonth;
  const ck = ymd(billingPeriod(now, s.billingDay).start);
  const dk = ymd(billingPeriod(now, s.deepgramBillingDay).start);
  if (sp.month !== ck) {
    if (!carries(sp.month, ck)) { sp.claudeUsd = 0; sp.turns = 0; }
    sp.month = ck;
  }
  if (sp.deepgramMonth !== dk) {
    if (!(legacy === dk || carries(legacy, dk))) sp.deepgramUsd = 0;
    sp.deepgramMonth = dk;
  }
  return sp;
}

/** The whole record, keys included. Server side only. */
/** Admin mode needs a capable model: Sonnet or above, never Haiku. */
export function adminCapable(model) { return !/haiku/i.test(String(model || '')); }
/** Is admin mode on right now (and its model able)? */
export function adminOn(s = load()) { return s.adminMode === true && adminCapable(s.model || DEFAULT_MODEL); }

export function getSettings() {
  return load();
}

/** What a client may see: no keys, only whether each is set. */
export function publicSettings(now = new Date()) {
  const s = load();
  const spend = spendNow(now);
  const total = spend.claudeUsd + spend.deepgramUsd;
  const period = billingPeriod(now, s.billingDay);
  const dg = billingPeriod(now, s.deepgramBillingDay);
  return {
    model: s.model,
    brain: BRAINS.includes(s.brain) ? s.brain : 'api',
    // Claude's billing month (periodStart/nextReset), then Deepgram's.
    billingDay: s.billingDay,
    periodStart: ymd(period.start),
    nextReset: ymd(period.next),
    deepgramBillingDay: s.deepgramBillingDay,
    pauseMs: s.pauseMs,
    speechRate: s.speechRate || 1,
    volume: Number.isFinite(s.volume) ? s.volume : 100,
    adminMode: adminOn(s),
    adminCapable: adminCapable(s.model || DEFAULT_MODEL),
    ownerOnly: s.ownerOnly !== false,
    screenCloseSeconds: s.screenCloseSeconds || 60,
    cliIdleMinutes: s.cliIdleMinutes || 30,
    deepgramPeriodStart: ymd(dg.start),
    deepgramNextReset: ymd(dg.next),
    models: Object.entries(MODELS).map(([id, m]) => ({ id, label: m.label })),
    anthropicKeySet: !!s.anthropicKey,
    deepgramKeySet: !!s.deepgramKey,
    anthropicAdminKeySet: !!s.anthropicAdminKey,
    ttsVoice: s.ttsVoice || '',
    monthlyLimitUsd: s.monthlyLimitUsd,
    spend: { ...spend, totalUsd: round(total), claudeUsd: round(spend.claudeUsd), deepgramUsd: round(spend.deepgramUsd) },
    // Each provider has its own month and limit (D46); limitReached is Claude's.
    deepgramLimitUsd: s.deepgramLimitUsd,
    limitReached: spend.claudeUsd >= s.monthlyLimitUsd,
    deepgramLimitReached: spend.deepgramUsd >= s.deepgramLimitUsd,
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
    // Haiku may not hold admin mode: switching to it switches admin mode off.
    if (!adminCapable(s.model)) s.adminMode = false;
  }
  if (patch.adminMode !== undefined) {
    if (patch.adminMode === true && !adminCapable(s.model || DEFAULT_MODEL)) return { ok: false, error: 'Admin mode needs Sonnet or above. Choose another model first.' };
    s.adminMode = patch.adminMode === true;
  }
  if (patch.ownerOnly !== undefined) {
    if (typeof patch.ownerOnly !== 'boolean') return { ok: false, error: 'ownerOnly is true or false' };
    s.ownerOnly = patch.ownerOnly;
  }
  if (patch.brain !== undefined) {
    if (!BRAINS.includes(patch.brain)) return { ok: false, error: `Unknown brain: ${patch.brain}` };
    s.brain = patch.brain;
  }
  if (patch.deepgramLimitUsd !== undefined) {
    const n = Number(patch.deepgramLimitUsd);
    if (!Number.isFinite(n) || n < 0 || n > 100000) return { ok: false, error: 'The Deepgram limit must be a number of dollars from 0 to 100000' };
    s.deepgramLimitUsd = n;
  }
  if (patch.monthlyLimitUsd !== undefined) {
    const n = Number(patch.monthlyLimitUsd);
    if (!Number.isFinite(n) || n < 0 || n > 100000) return { ok: false, error: 'The monthly limit must be a number of dollars from 0 to 100000' };
    s.monthlyLimitUsd = n;
  }
  for (const k of ['billingDay', 'deepgramBillingDay']) {
    if (patch[k] === undefined) continue;
    const d = Number(patch[k]);
    if (!Number.isInteger(d) || d < 1 || d > 31) return { ok: false, error: 'A billing day is a day of the month, 1 to 31' };
    s[k] = d;
  }
  if (patch.pauseMs !== undefined) {
    const n = Number(patch.pauseMs);
    if (!Number.isFinite(n) || n < 1000 || n > 5000) return { ok: false, error: 'The pause before sending is 1 to 5 seconds' };
    s.pauseMs = Math.round(n);
  }
  if (patch.speechRate !== undefined) {
    const n = Number(patch.speechRate);
    if (!Number.isFinite(n) || n < 0.7 || n > 1.5) return { ok: false, error: 'Talking speed is 0.7 to 1.5 (1 is normal)' };
    s.speechRate = Math.round(n * 20) / 20;
  }
  if (patch.volume !== undefined) {
    const n = Math.round(Number(patch.volume));
    if (!Number.isFinite(n) || n < 10 || n > 100) return { ok: false, error: 'Volume is 10 to 100 (%)' };
    s.volume = n;
  }
  if (patch.screenCloseSeconds !== undefined) {
    const n = Number(patch.screenCloseSeconds);
    if (!Number.isInteger(n) || n < 15 || n > 600) return { ok: false, error: 'The voice screen closes after 15 to 600 seconds of silence' };
    s.screenCloseSeconds = n;
  }
  if (patch.cliIdleMinutes !== undefined) {
    const n = Number(patch.cliIdleMinutes);
    if (!Number.isInteger(n) || n < 1 || n > 240) return { ok: false, error: 'Claude Code stops after 1 to 240 minutes idle' };
    s.cliIdleMinutes = n;
  }
  if (patch.ttsVoice !== undefined) {
    const v = String(patch.ttsVoice || '');
    if (v && !/^aura(-2)?-[a-z0-9]+-[a-z]{2}$/.test(v)) return { ok: false, error: `Not a Deepgram voice: ${v}` };
    s.ttsVoice = v;
  }
  for (const k of ['anthropicKey', 'deepgramKey', 'anthropicAdminKey']) {
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

/** Has Deepgram's spend this month reached its own limit? */
export function deepgramOverLimit(now = new Date()) {
  const s = load();
  return spendNow(now).deepgramUsd >= s.deepgramLimitUsd;
}

/** Has Claude's spend this month reached the limit? */
export function overLimit(now = new Date()) {
  const s = load();
  const spend = spendNow(now);
  return spend.claudeUsd >= s.monthlyLimitUsd;
}

/** Tests only: forget the in-memory copy so the file is read again. */
export function _reload() { cache = null; }

function round(n) { return Math.round(n * 10000) / 10000; }
