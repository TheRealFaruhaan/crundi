/**
 * voice/settings.js — the voice assistant's settings and its monthly spend.
 *
 * Kept in <dataDir>/voice.json, private to this user (0600). The API keys
 * (Claude, Deepgram) live here and only here: they are never sent to a phone
 * or a browser. publicSettings() is what clients see — whether a key is set,
 * never the key.
 *
 * Spend is counted per calendar month (UTC), in US dollars, from the usage
 * each Claude response and each Deepgram transcription reports. When the
 * month's spend reaches the limit, the assistant stops answering and says so
 * (D25); a new month starts from zero.
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

/** The month's spend, reset to zero when the month has changed. */
function spendNow(now) {
  const s = load();
  const m = thisMonth(now);
  if (s.spend.month !== m) s.spend = { month: m, claudeUsd: 0, deepgramUsd: 0, turns: 0 };
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
  return {
    model: s.model,
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
