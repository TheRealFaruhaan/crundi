/**
 * voice/tts.js — Deepgram Aura voices: the list, and speaking a reply.
 *
 * The list comes from Deepgram itself (GET /v1/models, the TTS part), so the
 * picker shows whatever Deepgram offers now, with each voice's accent and
 * tags, rather than a list copied into this file. If Deepgram can't be asked,
 * a short list of known Aura-2 names is offered instead, accent unknown.
 *
 * The chosen voice is a setting (ttsVoice). Empty means the device's own
 * voice (the phone's text-to-speech, or the browser's).
 */

import { getSettings, addDeepgramTtsSpend } from './settings.js';

const MODELS_URL = 'https://api.deepgram.com/v1/models';
const SPEAK_URL = 'https://api.deepgram.com/v1/speak';
const LIST_TTL_MS = 60 * 60 * 1000;
const MAX_CHARS = 2000;

/** Offered when Deepgram's own list can't be fetched. */
const FALLBACK = ['draco', 'pandora', 'hyperion', 'theia', 'arcas', 'orion', 'apollo', 'zeus', 'atlas', 'odysseus', 'thalia', 'athena']
  .map(n => ({ model: `aura-2-${n}-en`, name: n[0].toUpperCase() + n.slice(1), accent: '', gender: '', tags: [], sample: '' }));

let cached = { at: 0, voices: null };

/** A voice model name Crundi will send to Deepgram. */
export function isVoiceModel(s) {
  return /^aura(-2)?-[a-z0-9]+-[a-z]{2}$/.test(String(s || ''));
}

/** Deepgram's /v1/models answer → the picker's list. Pure, so it can be tested. */
export function parseVoices(body) {
  const tts = Array.isArray(body?.tts) ? body.tts : [];
  const seen = new Set();
  const out = [];
  for (const m of tts) {
    const model = String(m.canonical_name || m.name || '');
    if (!isVoiceModel(model) || seen.has(model)) continue;
    const langs = (m.languages || []).map(String);
    if (langs.length && !langs.some(l => l.toLowerCase().startsWith('en'))) continue;
    seen.add(model);
    const md = m.metadata || {};
    const tags = (md.tags || []).map(String);
    const gender = tags.find(t => /^(masculine|feminine|male|female)$/i.test(t)) || '';
    out.push({
      model,
      name: String(m.name && m.name !== model ? m.name : model.split('-').slice(-2, -1)[0] || model).replace(/^./, c => c.toUpperCase()),
      accent: String(md.accent || ''),
      gender: /^(masculine|male)$/i.test(gender) ? 'male' : /^(feminine|female)$/i.test(gender) ? 'female' : '',
      tags,
      sample: String(md.sample || ''),
      aura2: model.startsWith('aura-2-'),
    });
  }
  // Aura-2 first, British first, then by name.
  const brit = (v) => /brit|english \(uk\)|uk|england/i.test(v.accent) ? 0 : 1;
  out.sort((a, b) => (Number(b.aura2) - Number(a.aura2)) || (brit(a) - brit(b)) || a.name.localeCompare(b.name));
  return out;
}

/** @returns {Promise<{ok:true, voices:object[], fromDeepgram:boolean} | {ok:false, error:string}>} */
export async function listVoices({ fetchImpl = fetch, force = false } = {}) {
  const key = getSettings().deepgramKey;
  if (!key) return { ok: false, error: 'No Deepgram key is set. Add one in Settings › Voice assistant.' };
  if (!force && cached.voices && Date.now() - cached.at < LIST_TTL_MS) return { ok: true, voices: cached.voices, fromDeepgram: true };
  try {
    const res = await fetchImpl(MODELS_URL, { headers: { Authorization: `Token ${key}` } });
    if (!res.ok) throw new Error(`Deepgram answered ${res.status}`);
    const voices = parseVoices(await res.json());
    if (!voices.length) throw new Error('Deepgram listed no voices');
    cached = { at: Date.now(), voices };
    return { ok: true, voices, fromDeepgram: true };
  } catch {
    return { ok: true, voices: FALLBACK, fromDeepgram: false };
  }
}

/**
 * Speak `text` with an Aura voice.
 * @returns {Promise<{ok:true, audio:Buffer, mime:string} | {ok:false, error:string}>}
 */
export async function speak(text, voice, { fetchImpl = fetch } = {}) {
  const key = getSettings().deepgramKey;
  if (!key) return { ok: false, error: 'No Deepgram key is set. Add one in Settings › Voice assistant.' };
  const model = voice || getSettings().ttsVoice;
  if (!isVoiceModel(model)) return { ok: false, error: 'Choose a Deepgram voice first' };
  const t = String(text || '').trim().slice(0, MAX_CHARS);
  if (!t) return { ok: false, error: 'Nothing to say' };
  let res;
  try {
    res = await fetchImpl(`${SPEAK_URL}?${new URLSearchParams({ model, encoding: 'mp3' })}`, {
      method: 'POST',
      headers: { Authorization: `Token ${key}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ text: t }),
    });
  } catch (e) {
    return { ok: false, error: `Couldn't reach Deepgram: ${e.message}` };
  }
  if (!res.ok) {
    let why = '';
    try { const b = await res.json(); why = b.err_msg || b.message || b.reason || ''; } catch { /* not JSON */ }
    return { ok: false, error: `Deepgram couldn't speak that (${res.status})${why ? `: ${why}` : ''}` };
  }
  addDeepgramTtsSpend(t.length);
  return { ok: true, audio: Buffer.from(await res.arrayBuffer()), mime: res.headers?.get?.('content-type') || 'audio/mpeg' };
}

/** Raw speech for streaming: 16-bit mono PCM at this rate, no container. */
export const STREAM_RATE = 24000;

/**
 * Speak `text` as it is made: Deepgram's answer body, raw PCM (linear16,
 * STREAM_RATE, mono), to pipe to the client while it arrives, so playback
 * starts after the first bytes rather than the whole sentence.
 * @returns {Promise<{ok:true, body:ReadableStream, rate:number} | {ok:false, error:string}>}
 */
export async function speakStream(text, voice, { fetchImpl = fetch } = {}) {
  const key = getSettings().deepgramKey;
  if (!key) return { ok: false, error: 'No Deepgram key is set. Add one in Settings › Voice assistant.' };
  const model = voice || getSettings().ttsVoice;
  if (!isVoiceModel(model)) return { ok: false, error: 'Choose a Deepgram voice first' };
  const t = String(text || '').trim().slice(0, MAX_CHARS);
  if (!t) return { ok: false, error: 'Nothing to say' };
  let res;
  try {
    res = await fetchImpl(`${SPEAK_URL}?${new URLSearchParams({ model, encoding: 'linear16', sample_rate: String(STREAM_RATE), container: 'none' })}`, {
      method: 'POST',
      headers: { Authorization: `Token ${key}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ text: t }),
    });
  } catch (e) {
    return { ok: false, error: `Couldn't reach Deepgram: ${e.message}` };
  }
  if (!res.ok || !res.body) {
    let why = '';
    try { const b = await res.json(); why = b.err_msg || b.message || b.reason || ''; } catch { /* not JSON */ }
    return { ok: false, error: `Deepgram couldn't speak that (${res.status})${why ? `: ${why}` : ''}` };
  }
  addDeepgramTtsSpend(t.length);
  return { ok: true, body: res.body, rate: STREAM_RATE };
}

/** Tests only. */
export function _clearCache() { cached = { at: 0, voices: null }; }
