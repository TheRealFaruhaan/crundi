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

import { getSettings, addDeepgramTtsSpend, deepgramOverLimit } from './settings.js';
import { deepgramUrl } from './deepgram-host.js';

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
    const res = await fetchImpl(deepgramUrl('/v1/models'), { headers: { Authorization: `Token ${key}` } });
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
/** The talking speed for Deepgram (Aura-2 only, 0.7 to 1.5; left out at normal). */
export function speedParam(model, rate) {
  const r = Number(rate ?? getSettings().speechRate) || 1;
  return /^aura-2-/.test(model) && r !== 1 ? { speed: String(Math.min(1.5, Math.max(0.7, r))) } : {};
}

export async function speak(text, voice, { fetchImpl = fetch, rate } = {}) {
  const key = getSettings().deepgramKey;
  if (!key) return { ok: false, error: 'No Deepgram key is set. Add one in Settings › Voice assistant.' };
  const model = voice || getSettings().ttsVoice;
  if (!isVoiceModel(model)) return { ok: false, error: 'Choose a Deepgram voice first' };
  if (deepgramOverLimit()) return { ok: false, limitReached: true, error: 'This month\'s Deepgram spend has reached its limit' };
  const t = String(text || '').trim().slice(0, MAX_CHARS);
  if (!t) return { ok: false, error: 'Nothing to say' };
  let res;
  try {
    res = await fetchImpl(`${deepgramUrl('/v1/speak')}?${new URLSearchParams({ model, encoding: 'mp3', ...speedParam(model, rate) })}`, {
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
  if (deepgramOverLimit()) return { ok: false, limitReached: true, error: 'This month\'s Deepgram spend has reached its limit' };
  const t = String(text || '').trim().slice(0, MAX_CHARS);
  if (!t) return { ok: false, error: 'Nothing to say' };
  let res;
  try {
    res = await fetchImpl(`${deepgramUrl('/v1/speak')}?${new URLSearchParams({ model, encoding: 'linear16', sample_rate: String(STREAM_RATE), container: 'none', ...speedParam(model) })}`, {
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

// ─── Flux (kid mode's voice) ───
//
// Deepgram's Flux voices (flux-<name>-en) speak through /v2/speak and take an
// expressivity, -2 (subdued) to 2 (lively), set per request. Kid mode uses
// one (Settings: kidVoice, default flux-gemma-en); a failure falls back to
// the owner's Aura voice. Spend is counted like Aura's, per character, until
// Deepgram's own Flux price is known.
export const FLUX_VOICES = ['flux-gemma-en', 'flux-heather-en', 'flux-brooke-en', 'flux-meghan-en', 'flux-cole-en'];
export const isFluxModel = (m) => /^flux-[a-z]+-[a-z]{2}$/.test(String(m || ''));
/** Kid Cru's moods, as the expressivity Flux takes. */
export const MOOD_EXPRESSIVITY = { excited: 2, happy: 1, calm: 0, gentle: -1 };

/** Drop a WAV header, if Deepgram sends one, so the stream is raw PCM. */
function rawPcm(body) {
  let first = true, buf = new Uint8Array(0);
  return body.pipeThrough(new TransformStream({
    transform(chunk, ctl) {
      if (!first) { ctl.enqueue(chunk); return; }
      const m = new Uint8Array(buf.length + chunk.length); m.set(buf); m.set(chunk, buf.length); buf = m;
      if (buf.length < 64) return;
      first = false;
      const riff = buf[0] === 0x52 && buf[1] === 0x49 && buf[2] === 0x46 && buf[3] === 0x46;
      if (!riff) { ctl.enqueue(buf); return; }
      // The "data" chunk's bytes start 8 after its tag.
      for (let i = 12; i + 8 <= buf.length; i++) if (buf[i] === 0x64 && buf[i + 1] === 0x61 && buf[i + 2] === 0x74 && buf[i + 3] === 0x61) { ctl.enqueue(buf.slice(i + 8)); return; }
      ctl.enqueue(buf.slice(44));
    },
    flush(ctl) { if (first && buf.length) ctl.enqueue(buf); },
  }));
}

/**
 * Kid mode's speech, streamed as raw PCM like speakStream: a Flux voice at
 * [expressivity] (-2..2) and [rate] (0.5..1.5); on any failure, the owner's
 * Aura voice (speakStream) instead.
 */
export async function speakKidStream(text, { voice = '', expressivity = 2, rate = 1, fetchImpl = fetch, fallback = speakStream } = {}) {
  const model = voice || getSettings().kidVoice || '';
  if (!isFluxModel(model)) return fallback(text, isVoiceModel(model) ? model : '', { fetchImpl });
  const key = getSettings().deepgramKey;
  if (!key) return { ok: false, error: 'No Deepgram key is set. Add one in Settings › Voice assistant.' };
  if (deepgramOverLimit()) return { ok: false, limitReached: true, error: 'This month\'s Deepgram spend has reached its limit' };
  const t = String(text || '').trim().slice(0, MAX_CHARS);
  if (!t) return { ok: false, error: 'Nothing to say' };
  const ex = Math.max(-2, Math.min(2, Math.round(Number(expressivity) || 0)));
  const sp = Math.round(Math.max(0.5, Math.min(1.5, Number(rate) || 1)) * 20) / 20;
  const q = new URLSearchParams({ model, encoding: 'linear16', sample_rate: String(STREAM_RATE), expressivity: String(ex), ...(sp !== 1 ? { speed: String(sp) } : {}) });
  try {
    const res = await fetchImpl(`${deepgramUrl('/v2/speak')}?${q}`, { method: 'POST', headers: { Authorization: `Token ${key}`, 'Content-Type': 'application/json' }, body: JSON.stringify({ text: t }) });
    if (res.ok && res.body) { addDeepgramTtsSpend(t.length); return { ok: true, body: rawPcm(res.body), rate: STREAM_RATE, voice: model }; }
  } catch { /* the fallback speaks */ }
  return fallback(text, '', { fetchImpl });
}
