/**
 * voice/cues.js — "One moment, Sir.": the short spoken cue a client plays
 * the moment the owner's words have gone to Claude, so he knows Cru has
 * them and is working, without waiting on Claude (or on the network).
 *
 * A few fixed phrases, said once per Deepgram voice and talking speed with
 * the ordinary TTS (its characters added to the Deepgram spend as usual) and
 * kept on disk:
 *
 *   <dataDir>/voice-cues/<voice>/<rate>/<id>.mp3     id: a hash of the phrase
 *
 * so a cue always sounds like the voice in use. The first time a voice and
 * speed are asked for, the clips are made in the background and listCues()
 * offers whatever is ready (none, the first time); it never waits on
 * Deepgram. With no Deepgram voice (or Deepgram over its limit) the clients
 * say the phrase with the device's own voice: no clips are needed.
 *
 *   GET /api/voice/cues          → { ok, voice, rate, device, cues: [{ id, text, url|null }] }
 *   GET /api/voice/cues/clip?voice=&rate=&id=   the mp3
 */

import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { config } from '../config.js';
import { getSettings, deepgramOverLimit } from './settings.js';
import { speak, isVoiceModel } from './tts.js';

/** Short, a butler's register; picked at random, never the same twice running. */
export const CUES = [
  'One moment, Sir.',
  'Let me check.',
  'On it, Sir.',
  'Just a moment.',
  'Looking now, Sir.',
  'Right away, Sir.',
  'Bear with me, Sir.',
  'Let me see.',
];

export const cueId = (text) => createHash('sha1').update(text).digest('hex').slice(0, 12);

/** The speed as a folder name: "1", "1.25". */
export function rateKey(rate) {
  const r = Math.min(1.5, Math.max(0.7, Number(rate) || 1));
  return String(Math.round(r * 100) / 100);
}

const RATE_RE = /^(0|1)(\.\d{1,2})?$/;
const ID_RE = /^[0-9a-f]{12}$/;

/** Where one clip lives, or null for a name that isn't one of ours. */
export function clipPath(voice, rate, id, dataDir = config.dataDir) {
  if (!isVoiceModel(voice) || !RATE_RE.test(String(rate)) || !ID_RE.test(String(id))) return null;
  if (!CUES.some(t => cueId(t) === id)) return null;
  return join(dataDir, 'voice-cues', voice, String(rate), `${id}.mp3`);
}

/** One background job per voice and speed. */
const making = new Map();

/**
 * Make the clips that are missing for [voice] at [rate], one at a time. A
 * phrase Deepgram refuses is tried again next time. Resolves to how many
 * were made.
 */
export function makeCues(voice, rate, { dataDir = config.dataDir, speakImpl = speak } = {}) {
  const key = `${voice}/${rate}`;
  if (making.has(key)) return making.get(key);
  const job = (async () => {
    let made = 0;
    for (const text of CUES) {
      const p = clipPath(voice, rate, cueId(text), dataDir);
      if (!p || existsSync(p)) continue;
      if (deepgramOverLimit()) break;
      const r = await speakImpl(text, voice, { rate: Number(rate) });
      if (!r || !r.ok || !r.audio?.length) continue;
      mkdirSync(join(p, '..'), { recursive: true });
      const tmp = `${p}.tmp`;
      writeFileSync(tmp, r.audio);
      renameSync(tmp, p);
      made++;
    }
    return made;
  })().catch(() => 0).finally(() => making.delete(key));
  making.set(key, job);
  return job;
}

/**
 * The cues for the voice in use now. Starts making any missing clips in the
 * background (never waits for them). `device`: say the text with the
 * device's own voice instead (no Deepgram voice, no key, or over the limit).
 */
export function listCues({ dataDir = config.dataDir, speakImpl = speak } = {}) {
  const s = getSettings();
  const voice = isVoiceModel(s.ttsVoice) ? s.ttsVoice : '';
  const rate = rateKey(s.speechRate);
  const device = !voice || !s.deepgramKey || deepgramOverLimit();
  const cues = CUES.map((text) => {
    const id = cueId(text);
    const p = device ? null : clipPath(voice, rate, id, dataDir);
    const ready = !!p && existsSync(p);
    return { id, text, url: ready ? `/api/voice/cues/clip?${new URLSearchParams({ voice, rate, id })}` : null };
  });
  if (!device && cues.some(c => !c.url)) makeCues(voice, rate, { dataDir, speakImpl });
  return { ok: true, voice: device ? '' : voice, rate, device, cues };
}

/** The mp3 for GET /api/voice/cues/clip, or null. */
export function readClip(voice, rate, id, dataDir = config.dataDir) {
  const p = clipPath(voice, rate, id, dataDir);
  if (!p || !existsSync(p)) return null;
  try { return readFileSync(p); } catch { return null; }
}
