/**
 * voice/stt.js — speech to text through Deepgram (D37).
 *
 * Phone-recorded audio comes to the server and is sent on from here, so the
 * Deepgram key never leaves the server. Crundi's own words — its vocabulary,
 * plus the names of the projects and services that exist right now — go with
 * every request as keyterms, because a general recogniser has no reason to
 * expect "kanban" (Phase 0: Android's recognisers never once heard it).
 *
 * This file does one recording at a time (Deepgram's pre-recorded endpoint).
 * Live streaming, for Phase 3, uses the same keyterms and the same key.
 */

import { getSettings, addDeepgramSpend } from './settings.js';

const LISTEN_URL = 'https://api.deepgram.com/v1/listen';
const MODEL = 'nova-3';
/** Deepgram's keyterm prompting takes up to this many terms per request. */
const MAX_KEYTERMS = 100;

/** Words any Crundi request may use. */
export const CRUNDI_VOCABULARY = [
  'Crundi', 'Hey Crundi', 'kanban', 'kanban task', 'mindmap', 'backlog', 'to do', 'in progress', 'done',
  'subtask', 'todo', 'project', 'chat', 'terminal', 'service', 'schedule', 'usage', 'workbench',
  'panel', 'notes', 'park', 'resume', 'Claude', 'Opus', 'Sonnet', 'Haiku',
];

/**
 * The keyterms for a request: the fixed vocabulary, then project and service
 * names. Names are split on - and _ the way people say them ("api-server" is
 * said "api server"). Duplicates and empties are dropped; capped at MAX_KEYTERMS.
 */
export function keyterms({ projects = [], services = [] } = {}) {
  const out = [];
  const seen = new Set();
  const add = (t) => {
    const s = String(t || '').replace(/[-_]+/g, ' ').replace(/\s+/g, ' ').trim();
    if (!s || s.length > 50) return;
    const k = s.toLowerCase();
    if (seen.has(k)) return;
    seen.add(k);
    out.push(s);
  };
  CRUNDI_VOCABULARY.forEach(add);
  projects.forEach(add);
  services.forEach(add);
  return out.slice(0, MAX_KEYTERMS);
}

/** The request URL for a set of keyterms. */
export function listenUrl(terms, { language = 'en' } = {}) {
  const q = new URLSearchParams({ model: MODEL, smart_format: 'true', punctuate: 'true', language });
  for (const t of terms) q.append('keyterm', t);
  return `${LISTEN_URL}?${q}`;
}

/**
 * Transcribe one recording.
 * @param {Buffer} audio   the recorded bytes
 * @param {string} mime    e.g. 'audio/wav', 'audio/webm', 'audio/mp4'
 * @returns {Promise<{ok:true, text:string, alternatives:string[], confidence:number, seconds:number} | {ok:false, error:string}>}
 */
export async function transcribe(audio, mime, { projects = [], services = [], fetchImpl = fetch } = {}) {
  const key = getSettings().deepgramKey;
  if (!key) return { ok: false, error: 'No Deepgram key is set. Add one in Settings › Voice assistant.' };
  if (!audio || !audio.length) return { ok: false, error: 'No audio' };
  let res;
  try {
    res = await fetchImpl(listenUrl(keyterms({ projects, services })), {
      method: 'POST',
      headers: { Authorization: `Token ${key}`, 'Content-Type': mime || 'application/octet-stream' },
      body: audio,
    });
  } catch (e) {
    return { ok: false, error: `Couldn't reach Deepgram: ${e.message}` };
  }
  let body = null;
  try { body = await res.json(); } catch { body = null; }
  if (!res.ok) {
    const why = body && (body.err_msg || body.message || body.reason);
    return { ok: false, error: `Deepgram refused the request (${res.status})${why ? `: ${why}` : ''}` };
  }
  const alts = body?.results?.channels?.[0]?.alternatives || [];
  const seconds = Number(body?.metadata?.duration) || 0;
  addDeepgramSpend(seconds);
  return {
    ok: true,
    text: String(alts[0]?.transcript || '').trim(),
    alternatives: alts.slice(1).map(a => String(a.transcript || '').trim()).filter(Boolean),
    confidence: Number(alts[0]?.confidence) || 0,
    seconds,
  };
}
