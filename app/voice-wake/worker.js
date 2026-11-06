/**
 * The keyword spotter's own process (Electron utilityProcess), so decoding
 * never runs on the main process's thread and a crash in the engine takes
 * nothing else with it.
 *
 * From main:  { type: 'init', dir, profile }   load the engine and model, tuned by the owner's profile
 *             { type: 'profile', profile }     a new profile (trained again) or null (forgotten)
 *             { type: 'pcm', samples }         Float32Array, 16 kHz mono
 *             { type: 'reset' }                forget what was heard
 *             { type: 'train', id, dir, recordings }   five Float32Arrays -> enroll.js train()
 * To main:    { type: 'ready' } | { type: 'error', message }
 *             { type: 'detected', keyword, score, via }   a wake: via 'phrase' (the owner's recordings matched a burst of speech) or 'model' (the spotter fired and the voice matched)
 *             { type: 'rejected', keyword, score }        the spotter fired, the voice did not match
 *             { type: 'trained', id, result }        train()'s answer ({ ok, profile } | { ok:false, why, redo })
 *
 * The wake itself is in createWake() so it can be checked in plain Node
 * (scripts/test-voice-wake-train.mjs) exactly as it runs here.
 */

import { createSpotter } from './spotter.js';
import { TUNING, SAMPLE_RATE } from './manifest.js';
import { train, verify, validProfile, VERIFY_SECONDS, createSegmenter, matchPhrase, keywordLines, CONFIRM_THRESHOLD, RATE } from './enroll.js';

/** What the spotter is made with for this profile. */
export function profileTuning(profile) {
  return { keywords: profile.keyword, keywordsScore: profile.keywordsScore, keywordsThreshold: profile.keywordsThreshold };
}

/**
 * The two ways "Hey Cru" wakes (enroll.js): every burst of speech on its
 * own compared with the owner's recordings (strict), and the spotter tuned
 * by [profile] with a rolling buffer of the last VERIFY_SECONDS for the
 * voice match on its hits (looser).
 * @returns {{ accept(samples: Float32Array): null|{ ok: boolean, keyword: string, score: number, via: string }, reset(): void, free(): void }}
 */
export function createWake(dir, profile, opts = {}) {
  if (!validProfile(profile)) throw new Error('Hey Cru is not trained');
  const now = opts.now || (() => Date.now());
  const spot = createSpotter(dir, profileTuning(profile));
  const seg = createSegmenter();
  // The model's second opinion on a burst the phrase matched: its loosest
  // lines, made the first time it is needed, run on that burst only.
  let loose = null;
  const confirm = (clip) => {
    if (!loose) loose = createSpotter(dir, { keywords: keywordLines(CONFIRM_THRESHOLD), keywordsScore: profile.keywordsScore, keywordsThreshold: CONFIRM_THRESHOLD, gate: false });
    loose.reset();
    const pad = new Float32Array(RATE * 0.3 + clip.length + RATE); pad.set(clip, RATE * 0.3);
    let hit = false;
    for (let i = 0; i < pad.length; i += 1600) if (loose.accept(pad.subarray(i, i + 1600))) hit = true;
    return hit;
  };
  const ring = new Float32Array(Math.round(VERIFY_SECONDS * SAMPLE_RATE));
  let filled = 0, at = 0, lastHit = -Infinity;

  function push(s) {
    if (s.length >= ring.length) { ring.set(s.subarray(s.length - ring.length)); at = 0; filled = ring.length; return; }
    const first = Math.min(s.length, ring.length - at);
    ring.set(s.subarray(0, first), at);
    if (first < s.length) ring.set(s.subarray(first), 0);
    at = (at + s.length) % ring.length;
    filled = Math.min(ring.length, filled + s.length);
  }
  function recent() {
    const out = new Float32Array(filled);
    if (filled < ring.length) out.set(ring.subarray(0, filled));
    else { out.set(ring.subarray(at)); out.set(ring.subarray(0, at), ring.length - at); }
    return out;
  }

  return {
    accept(samples) {
      push(samples);
      const k = spot.accept(samples);
      const burst = seg.accept(samples);
      if (now() - lastHit <= TUNING.cooldownMs) return null;
      // The owner's own phrase, whatever the model made of it.
      if (burst) {
        const p = matchPhrase(profile, burst, confirm);
        if (p.ok) { lastHit = now(); return { ok: true, keyword: 'HEY_CRU', score: p.score, via: 'phrase' }; }
      }
      if (!k) return null;
      const v = verify(profile, recent());
      // Only a wake starts the cooldown: a rejected "crew" must not hide the
      // owner's "Hey Cru" right after it.
      if (v.ok) lastHit = now();
      return { ok: v.ok, keyword: k, score: v.score, via: 'model' };
    },
    reset() { spot.reset(); seg.reset(); filled = 0; at = 0; },
    free() { spot.free(); if (loose) loose.free(); },
  };
}

/** train() with spotters made in [dir], as enroll.js expects (no gate). */
export function trainIn(dir, recordings) {
  const recs = (recordings || []).map((r) => (r instanceof Float32Array ? r : new Float32Array(r || [])));
  return train(recs, (tuning) => createSpotter(dir, { ...tuning, gate: false }));
}

const port = process.parentPort;
if (port) {
  let wake = null;
  let dir = '';

  const load = (profile) => {
    if (wake) { try { wake.free(); } catch { /* gone */ } wake = null; }
    if (validProfile(profile)) wake = createWake(dir, profile);
  };

  port.on('message', (e) => {
    const m = e.data || {};
    try {
      if (m.type === 'init') {
        dir = m.dir;
        if (!wake) load(m.profile);
        port.postMessage(wake ? { type: 'ready' } : { type: 'error', message: 'Hey Cru is not trained' });
      } else if (m.type === 'profile') {
        load(m.profile);
        if (wake) port.postMessage({ type: 'ready' });
      } else if (m.type === 'pcm' && wake) {
        const s = m.samples instanceof Float32Array ? m.samples : new Float32Array(m.samples || []);
        if (!s.length) return;
        const r = wake.accept(s);
        if (r) port.postMessage({ type: r.ok ? 'detected' : 'rejected', keyword: r.keyword, score: r.score, via: r.via });
      } else if (m.type === 'reset' && wake) {
        wake.reset();
      } else if (m.type === 'train') {
        let result;
        try { result = trainIn(m.dir, m.recordings); }
        catch (err) { result = { ok: false, why: 'Training failed: ' + String((err && err.message) || err), redo: [] }; }
        port.postMessage({ type: 'trained', id: m.id, result });
      }
    } catch (err) {
      port.postMessage({ type: 'error', message: String((err && err.message) || err) });
    }
  });
}
