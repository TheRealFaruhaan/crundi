/**
 * "Hey Cru" for the owner only (12 Nov, the owner: other people's speech
 * woke Cru; "owner detection should be there for hey cru as well, client
 * side"). The phrase match and the model (enroll.js) say whether "Hey Cru"
 * was said; this says whether it was the owner who said it.
 *
 * The voice model is the one the server checks the owner's voice with
 * (src/voice/speaker.js: 3D-Speaker ERes2Net, 26 MB, run by onnxruntime-node,
 * which the PC app ships), downloaded once into <userData>/voice-wake/speaker,
 * checked by size and SHA-256. Training keeps the owner's five "Hey Cru" takes
 * as voice embeddings (profile.voice); a wake is his when the mean of its
 * OWNER_K best cosines with them reaches OWNER_THRESHOLD, as the server's
 * short-word check scores a short clip (wordScore).
 *
 * The threshold, measured on the recordings the wake word was tuned with
 * (synthetic voices, "Hey Cru" clips of 0.55 to 1.6 s): every owner's own
 * calls scored 0.43 or more; other voices' calls and takes scored 0.40 or
 * less for two owners, and under 0.40 for 95% for the third.
 *
 * Without onnxruntime or the model (no network the first time), there is no
 * verdict: the wake stands, as before, and the status says why.
 */

import { join } from 'path';
import { createOnnxEmbedder, normalise, wordScore, MODEL } from '../../src/voice/speaker.js';

export const OWNER_THRESHOLD = 0.42;
export const OWNER_K = 3;
/** Under this much sound there is too little to tell: no verdict. */
export const OWNER_MIN_SECONDS = 0.3;
export const VOICE_MODEL = MODEL.id;

/** Where the voice model lives under <userData>/voice-wake. */
export const speakerDir = (dir) => join(dir, 'speaker');

/** Is [v] a usable voice part of a profile: this model, at least OWNER_K clips of its size? */
export function validVoice(v) {
  return !!(v && v.model === VOICE_MODEL && Array.isArray(v.clips) && v.clips.length >= OWNER_K
    && v.clips.every((c) => Array.isArray(c) && c.length === MODEL.dim && c.every(Number.isFinite)));
}

/** The voice's score for an embedding: the mean of the OWNER_K best cosines with the takes. */
export function ownerScore(voice, embedding) {
  return wordScore(normalise(embedding), voice.clips.map((c) => Float32Array.from(c)), OWNER_K);
}

/** true (the owner), false (another voice), null (no verdict: too short, or nothing to compare with). */
export function ownerVerdict(voice, embedding, seconds) {
  if (!validVoice(voice) || !embedding || seconds < OWNER_MIN_SECONDS) return null;
  const s = ownerScore(voice, embedding);
  return Number.isFinite(s) ? s >= OWNER_THRESHOLD : null;
}

/**
 * @param {string} dir  <userData>/voice-wake
 * @param {{ embedder?: { embed(s: Float32Array): Promise<Float32Array>, load(): Promise<unknown>, readonly state: string } }} [o]
 *   tests pass a fake embedder
 */
export function createOwnerCheck(dir, o = {}) {
  const embedder = o.embedder || createOnnxEmbedder({ modelDir: speakerDir(dir) });
  return {
    get state() { return embedder.state; },
    /** Fetch and load the model now (training, or a profile with a voice): resolves true when ready. */
    async load() { try { await embedder.load(); return true; } catch { return false; } },
    /** The profile's voice from the owner's takes (16 kHz, trimmed). Rejects when the model is not to be had. */
    async enrol(clips) {
      const out = [];
      for (const c of clips) out.push(Array.from(normalise(await embedder.embed(c)), (x) => Math.round(x * 1e4) / 1e4));
      return { model: VOICE_MODEL, clips: out };
    },
    /**
     * Is [clip] the owner? { ok, score } with ok true/false, or { ok: null, why } when
     * there is no verdict (no voice in the profile, too short, the model unavailable).
     */
    async check(voice, clip) {
      if (!validVoice(voice)) return { ok: null, why: 'no voice in the profile' };
      const seconds = clip.length / 16000;
      if (seconds < OWNER_MIN_SECONDS) return { ok: null, why: 'too short' };
      let e;
      try { e = await embedder.embed(clip); } catch (err) { return { ok: null, why: 'voice model unavailable: ' + String((err && err.message) || err).slice(0, 120) }; }
      const score = Math.round(ownerScore(voice, e) * 1000) / 1000;
      return { ok: score >= OWNER_THRESHOLD, score };
    },
  };
}
