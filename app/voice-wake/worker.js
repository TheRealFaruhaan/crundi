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
import { train, verify, validProfile, VERIFY_SECONDS, createSegmenter, matchPhrase, confirmLines, listenLines, spotted, checkSample, trim, CONFIRM_THRESHOLD, WAKE } from './enroll.js';
import { createOwnerCheck, validVoice } from './owner.js';

/** What the spotter is made with for this profile: its "Hey Cru" lines and the decoys competing with them. */
export function profileTuning(profile) {
  return { keywords: listenLines(profile), keywordsScore: profile.keywordsScore, keywordsThreshold: profile.keywordsThreshold };
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
  // lines, the decoys among them, made the first time it is needed, run on
  // that burst only. Gives what it heard (matchPhrase decides).
  let loose = null;
  const confirm = (clip) => {
    if (!loose) loose = createSpotter(dir, { keywords: confirmLines(profile), keywordsScore: profile.keywordsScore, keywordsThreshold: CONFIRM_THRESHOLD, gate: false });
    return spotted(loose, clip);
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
        // The clip it matched goes with the wake: the owner's voice is checked on it (owner.js).
        if (p.ok) { lastHit = now(); return { ok: true, keyword: 'HEY_CRU', score: p.score, via: 'phrase', clip: trim(burst) || burst }; }
      }
      // A decoy ("hey you", "hey who"...) won over "Hey Cru": not a call.
      if (!k || k !== WAKE) return null;
      const heardNow = recent();
      const v = verify(profile, heardNow);
      // Only a wake starts the cooldown: a rejected "crew" must not hide the
      // owner's "Hey Cru" right after it.
      if (v.ok) lastHit = now();
      return { ok: v.ok, keyword: k, score: v.score, via: 'model', clip: trim(heardNow, 60) || heardNow };
    },
    /** A wake refused afterwards (not the owner's voice): no cooldown, so his own call right after counts. */
    refused() { lastHit = -Infinity; },
    reset() { spot.reset(); seg.reset(); filled = 0; at = 0; },
    free() { spot.free(); if (loose) loose.free(); },
  };
}

/** train() with spotters made in [dir], as enroll.js expects (no gate). */
export function trainIn(dir, recordings) {
  const recs = (recordings || []).map((r) => (r instanceof Float32Array ? r : new Float32Array(r || [])));
  return train(recs, (tuning) => createSpotter(dir, { ...tuning, gate: false }));
}

/**
 * trainIn, then the owner's voice from the same takes (owner.js), kept as
 * profile.voice. The voice model not to be had (no network the first time,
 * no onnxruntime): the profile is kept without it, and [voiceWhy] says why.
 */
export async function trainWithVoice(dir, recordings, owner = createOwnerCheck(dir)) {
  const r = trainIn(dir, recordings);
  if (!r.ok) return r;
  try {
    const clips = (recordings || []).map((x) => checkSample(x instanceof Float32Array ? x : new Float32Array(x || [])).clip).filter(Boolean);
    r.profile.voice = await owner.enrol(clips);
  } catch (err) {
    r.voiceWhy = 'The voice check could not be set up (' + String((err && err.message) || err).slice(0, 120) + '): Hey Cru works, for any voice, until it is trained again.';
  }
  return r;
}

/**
 * A wake from createWake, checked for the owner's voice when the profile has
 * one: { type: 'detected' | 'rejected', ... } for the controller.
 */
export async function ownerChecked(r, profile, owner) {
  const base = { keyword: r.keyword, score: r.score, via: r.via };
  if (!r.ok) return { type: 'rejected', ...base };
  if (!profile || !validVoice(profile.voice) || !owner || !r.clip) return { type: 'detected', ...base, voice: 'off' };
  const v = await owner.check(profile.voice, r.clip);
  if (v.ok === false) return { type: 'rejected', ...base, voice: v.score, why: 'not the owner\'s voice' };
  return { type: 'detected', ...base, voice: v.ok ? v.score : 'no verdict: ' + v.why };
}

const port = process.parentPort;
if (port) {
  let wake = null;
  let dir = '';
  let current = null, owner = null;

  const load = (profile) => {
    if (wake) { try { wake.free(); } catch { /* gone */ } wake = null; }
    current = validProfile(profile) ? profile : null;
    if (current) wake = createWake(dir, profile);
    // The voice model ready before the first call (fetched once if missing).
    if (current && validVoice(current.voice)) { if (!owner) owner = createOwnerCheck(dir); owner.load(); }
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
        if (r) {
          const w = wake, p = current;
          ownerChecked(r, p, owner).then((msg) => {
            if (msg.type === 'rejected' && r.ok && w === wake) w.refused();
            port.postMessage(msg);
          }, (err) => port.postMessage({ type: 'error', message: String((err && err.message) || err) }));
        }
      } else if (m.type === 'reset' && wake) {
        wake.reset();
      } else if (m.type === 'train') {
        trainWithVoice(m.dir, m.recordings)
          .catch((err) => ({ ok: false, why: 'Training failed: ' + String((err && err.message) || err), redo: [] }))
          .then((result) => port.postMessage({ type: 'trained', id: m.id, result }));
      }
    } catch (err) {
      port.postMessage({ type: 'error', message: String((err && err.message) || err) });
    }
  });
}
