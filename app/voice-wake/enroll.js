/**
 * "Hey Cru", set up from the owner's own voice. Plain JavaScript with no
 * dependencies: the PC's spotter process uses it, scripts/test-voice-wake-train.mjs
 * checks it on recorded speech, and the phone has the same rules in Kotlin
 * (android/.../voice/WakeProfile.kt), checked against numbers made here
 * (scripts/wake-fixture.mjs).
 *
 * The keyword model itself can't be retrained on a phone or a PC, and it
 * may not hear "Hey Cru" the way the owner says it at all. So the five
 * recordings are what the wake word is learnt from, two ways:
 *
 *  1. The owner's own phrase. Each recording is kept as a template (MFCCs,
 *     10 ms frames). Every short burst of speech on its own (a pause before
 *     and after, 0.35 to 2.2 s) is compared with them (dynamic time
 *     warping): close to them, it wakes, whatever the model makes of it.
 *     This holds to a strict limit, from how near the recordings are to
 *     each other.
 *  2. The model, tuned. The spotter, with "Hey Cru" spelt the ways it may
 *     hear it (Cru, Kru, Crew), is run over each recording at stricter and
 *     stricter thresholds; the profile keeps two steps looser than the
 *     strictest that hears as many as it can. When it fires (in the middle
 *     of a sentence, say), the last two seconds are compared with the
 *     templates at a looser limit.
 *
 * No recording is refused because the model didn't hear it: that is what
 * the first way is for. One is asked again only when it is empty, too short
 * or long, or sounds unlike the other four.
 *
 * It is not a speaker check: someone else saying "Hey Cru" the same way may
 * pass. It does keep out "crew" on the television and the near misses.
 */

export const RATE = 16000;
export const SAMPLES_NEEDED = 5;
/**
 * The wake phrase as the model's BPE tokens, spelt the ways it may hear it
 * (bpe.model: "HEY CRU" -> ▁HE Y ▁C RU, "HEY KRU", "HEY CREW").
 */
export const SPELLINGS = ['▁HE Y ▁C RU', '▁HE Y ▁K RU', '▁HE Y ▁C RE W'];
/** The plain line (no tuning), for the shipped keywords file. */
export const KEYWORD = SPELLINGS[0] + ' @HEY_CRU';
/** Trigger thresholds tried, strictest first. */
export const THRESHOLDS = [0.35, 0.3, 0.25, 0.2, 0.15, 0.1, 0.06];
export const KEYWORDS_SCORE = 1.5;
/** The keyword lines for a threshold: every spelling, each with its boost and threshold. */
export function keywordLines(threshold, score = KEYWORDS_SCORE) {
  return SPELLINGS.map(sp => `${sp} :${score} #${threshold} @HEY_CRU`).join('\n');
}
/** How much further than the recordings are from each other a wake the model heard may be. */
const LIMIT_MARGIN = 1.6;
/** ...and one from the owner's phrase alone, which nothing else vouches for. */
const STRICT_MARGIN = 1.3;
/** A burst this much shorter or longer than the owner's takes is not their "Hey Cru". */
const LENGTH_RANGE = [0.65, 1.5];
/** At least this share of a burst's loud frames must be voiced (vowels have a pitch; noise has none). */
const MIN_VOICED = 0.3;
/** Below this many takes heard by the model, the phrase wakes without its second opinion. */
const MODEL_TRUSTED = 3;
/** What the verifier looks at when the spotter fires. */
export const VERIFY_SECONDS = 2.0;

// ─── features ───

const FRAME = 400, HOP = 160, NFFT = 512, MELS = 26, CEPS = 12;

let melBank = null, hamming = null, dctM = null;
function tables() {
  if (melBank) return;
  hamming = new Float64Array(FRAME);
  for (let i = 0; i < FRAME; i++) hamming[i] = 0.54 - 0.46 * Math.cos(2 * Math.PI * i / (FRAME - 1));
  const mel = (f) => 2595 * Math.log10(1 + f / 700), hz = (m) => 700 * (Math.pow(10, m / 2595) - 1);
  const lo = mel(20), hi = mel(7600), bins = [];
  for (let i = 0; i < MELS + 2; i++) bins.push(Math.floor((NFFT + 1) * hz(lo + (hi - lo) * i / (MELS + 1)) / RATE));
  melBank = [];
  for (let m = 1; m <= MELS; m++) {
    const row = new Float64Array(NFFT / 2 + 1);
    for (let k = bins[m - 1]; k < bins[m]; k++) row[k] = (k - bins[m - 1]) / Math.max(1, bins[m] - bins[m - 1]);
    for (let k = bins[m]; k < bins[m + 1]; k++) row[k] = (bins[m + 1] - k) / Math.max(1, bins[m + 1] - bins[m]);
    melBank.push(row);
  }
  dctM = [];
  for (let c = 1; c <= CEPS; c++) {
    const row = new Float64Array(MELS);
    for (let m = 0; m < MELS; m++) row[m] = Math.sqrt(2 / MELS) * Math.cos(Math.PI * c * (m + 0.5) / MELS);
    dctM.push(row);
  }
}

/** In-place radix-2 FFT of (re, im), length a power of two. */
function fft(re, im) {
  const n = re.length;
  for (let i = 1, j = 0; i < n; i++) {
    let bit = n >> 1;
    for (; j & bit; bit >>= 1) j ^= bit;
    j ^= bit;
    if (i < j) { let t = re[i]; re[i] = re[j]; re[j] = t; t = im[i]; im[i] = im[j]; im[j] = t; }
  }
  for (let len = 2; len <= n; len <<= 1) {
    const a = -2 * Math.PI / len, wr = Math.cos(a), wi = Math.sin(a);
    for (let i = 0; i < n; i += len) {
      let cr = 1, ci = 0;
      for (let k = 0; k < len / 2; k++) {
        const ur = re[i + k], ui = im[i + k];
        const vr = re[i + k + len / 2] * cr - im[i + k + len / 2] * ci;
        const vi = re[i + k + len / 2] * ci + im[i + k + len / 2] * cr;
        re[i + k] = ur + vr; im[i + k] = ui + vi;
        re[i + k + len / 2] = ur - vr; im[i + k + len / 2] = ui - vi;
        const nr = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = nr;
      }
    }
  }
}

/**
 * 12 MFCCs per 10 ms frame (25 ms window, 26 mel bands, 20-7600 Hz), with
 * the clip's mean taken away so the microphone and the room matter less.
 * @returns {Float64Array[]}
 */
export function mfcc(samples) {
  tables();
  const out = [], mels = [];
  const re = new Float64Array(NFFT), im = new Float64Array(NFFT);
  let loudest = 0;
  for (let start = 0; start + FRAME <= samples.length; start += HOP) {
    re.fill(0); im.fill(0);
    let prev = start > 0 ? samples[start - 1] : 0;
    for (let i = 0; i < FRAME; i++) { const x = samples[start + i]; re[i] = (x - 0.97 * prev) * hamming[i]; prev = x; }
    fft(re, im);
    const e = new Float64Array(MELS);
    for (let m = 0; m < MELS; m++) {
      let s = 0; const row = melBank[m];
      for (let k = 0; k <= NFFT / 2; k++) if (row[k]) s += row[k] * (re[k] * re[k] + im[k] * im[k]) / NFFT;
      e[m] = s; if (s > loudest) loudest = s;
    }
    mels.push(e);
  }
  // Noise reduction (spectral subtraction): the steady background in each
  // band (a hum, a fan, the room), from the clip's quietest fifth, is taken
  // away before anything is compared, in training and listening alike, so
  // only the voice is learnt and matched.
  if (mels.length >= 5) {
    const energy = mels.map((e, i) => [e.reduce((a, b) => a + b, 0), i]).sort((a, b) => a[0] - b[0]);
    const q = energy.slice(0, Math.max(1, Math.floor(mels.length / 5))).map(x => mels[x[1]]);
    const noise = new Float64Array(MELS);
    for (const e of q) for (let m = 0; m < MELS; m++) noise[m] += e[m] / q.length;
    for (const e of mels) for (let m = 0; m < MELS; m++) e[m] = Math.max(e[m] - 1.5 * noise[m], e[m] * 0.05);
  }
  // Anything 30 dB under the clip's loudest band counts as the same quiet,
  // so what is left of the background under the words changes little.
  const floor = Math.max(loudest * 1e-3, 1e-10);
  for (const e of mels) {
    const c = new Float64Array(CEPS);
    for (let j = 0; j < CEPS; j++) { let s = 0; const row = dctM[j]; for (let m = 0; m < MELS; m++) s += row[m] * Math.log(Math.max(e[m], floor)); c[j] = s; }
    out.push(c);
  }
  if (!out.length) return out;
  const mean = new Float64Array(CEPS);
  for (const f of out) for (let j = 0; j < CEPS; j++) mean[j] += f[j] / out.length;
  for (const f of out) for (let j = 0; j < CEPS; j++) f[j] -= mean[j];
  // How the sound moves (deltas) counts for more than its colour: holding
  // the phone nearer or further changes the colour, not the movement.
  return out.map((f, t) => {
    const a = out[Math.max(0, t - 1)], b = out[Math.min(out.length - 1, t + 1)];
    const v = new Float64Array(CEPS * 2);
    for (let j = 0; j < CEPS; j++) { v[j] = STATIC_WEIGHT * f[j]; v[CEPS + j] = DELTA_WEIGHT * (b[j] - a[j]) / 2; }
    return v;
  });
}

const STATIC_WEIGHT = 0.6, DELTA_WEIGHT = 1.8;

/**
 * The same take as heard another way: muffled and echoing (the phone
 * further off, on a table, turned away), and brighter (closer, a harder
 * room). Training keeps these as templates too, so a call heard in a way
 * the five takes weren't still matches, while the limit stays as strict.
 */
export function variants(clip) {
  const peakOf = (a) => { let p = 0; for (let i = 0; i < a.length; i++) p = Math.max(p, Math.abs(a[i])); return p; };
  const p0 = peakOf(clip) || 1;
  const norm = (a) => { const k = p0 / (peakOf(a) || 1); for (let i = 0; i < a.length; i++) a[i] *= k; return a; };
  const far = new Float32Array(clip.length);
  let y = 0;
  for (let i = 0; i < clip.length; i++) { y += 0.45 * (clip[i] - y); far[i] = y; }
  const echo = new Float32Array(clip.length);
  for (let i = 0; i < clip.length; i++) echo[i] = far[i] + 0.35 * (far[i - 480] || 0) + 0.2 * (far[i - 1120] || 0) + 0.1 * (far[i - 1900] || 0);
  const bright = new Float32Array(clip.length);
  for (let i = 0; i < clip.length; i++) bright[i] = clip[i] - 0.6 * (i ? clip[i - 1] : 0);
  return [norm(echo), norm(bright)];
}

/**
 * The spoken part of a recording: from a little before the first loud 10 ms
 * to a little after the last. Null when nothing in it stands out from its
 * own quiet (nothing was said, or the microphone is off).
 */
export function trim(samples, padMs = 100) {
  const step = RATE / 100, n = Math.floor(samples.length / step);
  if (n < 5) return null;
  const rms = new Float64Array(n);
  for (let i = 0; i < n; i++) { let s = 0; for (let k = 0; k < step; k++) { const x = samples[i * step + k]; s += x * x; } rms[i] = Math.sqrt(s / step); }
  const sorted = Array.from(rms).sort((a, b) => a - b);
  const floor = sorted[Math.floor(n * 0.1)], peak = sorted[n - 1];
  if (peak < 0.01 || peak < floor * 4) return null;
  const thr = Math.max(peak * 0.12, floor * 2.5);
  let a = 0, b = n - 1;
  while (a < n && rms[a] < thr) a++;
  while (b > a && rms[b] < thr) b--;
  const pad = Math.round(padMs / 10);
  const from = Math.max(0, a - pad) * step, to = Math.min(n, b + 1 + pad) * step;
  return samples.subarray(from, to);
}

/**
 * How unlike [clip] is to [template]: dynamic time warping, the template
 * matched against the best stretch of the clip (it may hold more than the
 * phrase), as the mean distance per step. Lower is closer.
 */
export function distance(template, clip) {
  const m = template.length, n = clip.length;
  if (!m || !n) return Infinity;
  const d = (a, b) => { let s = 0; for (let j = 0; j < a.length; j++) { const t = a[j] - b[j]; s += t * t; } return Math.sqrt(s); };
  // cost and steps so far, two rows; the clip's start is free (row 0 is zero).
  let pc = new Float64Array(n + 1), pl = new Float64Array(n + 1);
  let cc = new Float64Array(n + 1), cl = new Float64Array(n + 1);
  for (let i = 1; i <= m; i++) {
    cc[0] = Infinity; cl[0] = 0;
    for (let j = 1; j <= n; j++) {
      const x = d(template[i - 1], clip[j - 1]);
      // diagonal, along the clip, along the template
      let best = pc[j - 1], len = pl[j - 1];
      if (cc[j - 1] < best) { best = cc[j - 1]; len = cl[j - 1]; }
      if (pc[j] < best) { best = pc[j]; len = pl[j]; }
      cc[j] = best + x; cl[j] = len + 1;
    }
    [pc, cc] = [cc, pc]; [pl, cl] = [cl, pl];
  }
  let best = Infinity;
  for (let j = 1; j <= n; j++) if (pl[j] > 0) best = Math.min(best, pc[j] / Math.max(pl[j], m));
  return best;
}

/**
 * Both ends held: DTW of [a] against [b] from start to end, as the mean
 * distance per step. For a burst that is the whole phrase (the phrase
 * match), unlike [distance], which lets the clip hold more.
 */
export function fullDistance(a, b) {
  const m = a.length, n = b.length;
  if (!m || !n) return Infinity;
  const d = (x, y) => { let s = 0; for (let j = 0; j < x.length; j++) { const t = x[j] - y[j]; s += t * t; } return Math.sqrt(s); };
  let pc = new Float64Array(n + 1).fill(Infinity), pl = new Float64Array(n + 1);
  let cc = new Float64Array(n + 1), cl = new Float64Array(n + 1);
  pc[0] = 0;
  for (let i = 1; i <= m; i++) {
    cc[0] = Infinity; cl[0] = 0;
    for (let j = 1; j <= n; j++) {
      const x = d(a[i - 1], b[j - 1]);
      let best = pc[j - 1], len = pl[j - 1];
      if (cc[j - 1] < best) { best = cc[j - 1]; len = cl[j - 1]; }
      if (pc[j] < best) { best = pc[j]; len = pl[j]; }
      cc[j] = best + x; cl[j] = len + 1;
    }
    [pc, cc] = [cc, pc]; [pl, cl] = [cl, pl];
  }
  return pl[n] > 0 ? pc[n] / pl[n] : Infinity;
}

/**
 * The share of a clip's loud 30 ms frames that are voiced: a clear pitch
 * between 70 and 400 Hz (normalised autocorrelation over 0.45). Speech
 * has it in its vowels; a fan, a tap or a clatter hardly ever does.
 */
export function voicedShare(clip) {
  const N = 480, H = 240;
  let peak = 0;
  for (let i = 0; i + N <= clip.length; i += H) { let e = 0; for (let k = 0; k < N; k++) e += clip[i + k] * clip[i + k]; peak = Math.max(peak, e); }
  if (!peak) return 0;
  let loud = 0, voiced = 0;
  for (let i = 0; i + N <= clip.length; i += H) {
    let e = 0; for (let k = 0; k < N; k++) e += clip[i + k] * clip[i + k];
    if (e < peak * 0.05) continue;
    loud++;
    let best = 0;
    for (let lag = 40; lag <= 228; lag++) {
      let c = 0, e1 = 0, e2 = 0;
      for (let k = 0; k + lag < N; k++) { const a = clip[i + k], b = clip[i + k + lag]; c += a * b; e1 += a * a; e2 += b * b; }
      const r = c / Math.sqrt((e1 * e2) || 1);
      if (r > best) best = r;
    }
    if (best > 0.45) voiced++;
  }
  return loud ? voiced / loud : 0;
}

// ─── training ───

/**
 * Check one recording as it is made: something was said, and it is long
 * enough to be "Hey Cru" and short enough not to be a sentence.
 * @returns {{ ok: boolean, why?: string, clip?: Float32Array }}
 */
export function checkSample(samples) {
  const clip = trim(samples);
  if (!clip) return { ok: false, why: 'Nothing was heard. Speak a little closer.' };
  const ms = clip.length / RATE * 1000;
  if (ms < 350) return { ok: false, why: 'That was too short. Say the whole of "Hey Cru".' };
  if (ms > 2200) return { ok: false, why: 'That was too long. Say just "Hey Cru".' };
  return { ok: true, clip };
}

/** Would the spotter (made with [makeSpotter]({ keywordsThreshold })) hear this clip? */
function heard(spot, clip) {
  spot.reset();
  const padded = new Float32Array(RATE * 0.3 + clip.length + RATE);
  padded.set(clip, RATE * 0.3);
  let hit = false;
  for (let i = 0; i < padded.length; i += 1600) if (spot.accept(padded.subarray(i, i + 1600))) hit = true;
  return hit;
}

/**
 * The profile from the owner's recordings.
 * @param {Float32Array[]} recordings  SAMPLES_NEEDED of them, 16 kHz mono
 * @param {(tuning: {keywords, keywordsScore, keywordsThreshold}) => {accept, reset, free}} makeSpotter
 * @returns {{ ok: true, profile } | { ok: false, why: string, redo: number[] }}
 */
export function train(recordings, makeSpotter) {
  const clips = [], redo = [];
  recordings.forEach((r, i) => { const c = checkSample(r); if (c.ok) clips.push(c.clip); else { clips.push(null); redo.push(i); } });
  if (redo.length) return { ok: false, why: 'Some recordings had nothing clear in them.', redo };

  // The voice match: templates, and how far apart they are.
  // The takes as said; the checks and limits come from these alone.
  const templates = clips.map(c => mfcc(c));
  const nearest = templates.map((t, i) => Math.min(...templates.map((u, j) => j === i ? Infinity : distance(u, t))));
  const sorted = [...nearest].sort((a, b) => a - b), median = sorted[Math.floor(sorted.length / 2)];
  const odd = nearest.map((d, i) => d > median * 2 ? i : -1).filter(i => i >= 0);
  if (odd.length && odd.length < templates.length / 2) {
    return { ok: false, why: 'One recording sounds unlike the others. Say it again the way you normally would.', redo: odd };
  }
  const spread = Math.max(...nearest);
  // For the phrase match: each take against the others, whole to whole, both ways.
  const near = templates.map((t, i) => Math.min(...templates.map((u, j) => j === i ? Infinity : Math.max(fullDistance(u, t), fullDistance(t, u)))));
  const nearSorted = [...near].sort((a, b) => a - b);
  const typical = nearSorted[Math.floor(nearSorted.length / 2)];
  const frames = templates.map(t => t.length).sort((a, b) => a - b)[Math.floor(templates.length / 2)];

  // The model, tuned: the strictest threshold that hears as many takes as
  // any does, two steps looser. Takes it never hears are fine: the owner's
  // phrase (the templates) wakes for those.
  let best = THRESHOLDS.length - 1, most = 0;
  for (let t = 0; t < THRESHOLDS.length; t++) {
    const spot = makeSpotter({ keywords: keywordLines(THRESHOLDS[t]), keywordsScore: KEYWORDS_SCORE, keywordsThreshold: THRESHOLDS[t], gate: false });
    let n = 0;
    try { n = clips.filter(c => heard(spot, c)).length; } finally { spot.free(); }
    if (n > most) { most = n; best = t; }
    if (n === clips.length) break;
  }
  const threshold = THRESHOLDS[Math.min(THRESHOLDS.length - 1, best + 2)];
  return {
    ok: true,
    profile: {
      version: 4,
      keyword: keywordLines(threshold),
      keywordsScore: KEYWORDS_SCORE,
      keywordsThreshold: threshold,
      limit: Math.round(spread * LIMIT_MARGIN * 1000) / 1000,
      strictLimit: Math.round(typical * STRICT_MARGIN * 1000) / 1000,
      /** The first [takes] templates are the takes; the rest, the same takes heard other ways ([variants]). */
      takes: templates.length,
      /** The owner's takes' usual length, 10 ms frames. */
      phraseFrames: frames,
      /** How many takes the model itself heard (the rest wake by the phrase alone). */
      modelHeard: most,
      templates: [...templates, ...clips.flatMap(c => variants(c).map(v => mfcc(v)))].map(t => t.map(f => Array.from(f, x => Math.round(x * 1000) / 1000))),
      trainedAt: new Date().toISOString(),
    },
  };
}

/**
 * The spotter fired: is it the owner's "Hey Cru"? [recent] is the last
 * VERIFY_SECONDS of audio, ending now.
 * @returns {{ ok: boolean, score: number }}
 */
export function verify(profile, recent) {
  const clip = trim(recent, 60) || recent;
  const feats = mfcc(clip);
  let best = Infinity;
  for (const t of profile.templates) best = Math.min(best, distance(t, feats));
  return { ok: best <= profile.limit, score: Math.round(best * 1000) / 1000 };
}

/**
 * A burst of speech on its own (see createSegmenter): the owner's "Hey
 * Cru", by the templates alone, at the strict limit?
 * @returns {{ ok: boolean, score: number }}
 */
export function matchPhrase(profile, segment, confirm = null) {
  const no = (why, score = Infinity) => ({ ok: false, score, why });

  const c = checkSample(segment);
  if (!c.ok) return no('length');
  // As long as the owner's takes, and speech (voiced), not a noise.
  const frames = profile.phraseFrames;
  const len = (c.clip.length - 400) / 160 + 1;
  if (len < frames * LENGTH_RANGE[0] || len > frames * LENGTH_RANGE[1]) return no('length');
  if (voicedShare(c.clip) < MIN_VOICED) return no('not speech');
  const feats = mfcc(c.clip);
  let best = Infinity;
  for (const t of profile.templates) best = Math.min(best, Math.max(fullDistance(t, feats), fullDistance(feats, t)));
  const score = Math.round(best * 1000) / 1000;
  if (best > strictLimit(profile)) return no('unlike', score);
  // When the model can hear this voice, it must agree, if only faintly.
  if (confirm && (profile.modelHeard ?? 0) >= MODEL_TRUSTED && !confirm(c.clip)) return no('model', score);
  return { ok: true, score };
}

/** The loosest keyword lines, for the model's second opinion on a burst the phrase matched. */
export const CONFIRM_THRESHOLD = 0.04;

/** A version 1 profile (before the phrase match) had no strict limit: the same margin, from its limit. */
export function strictLimit(p) { return p.strictLimit > 0 ? p.strictLimit : p.limit / LIMIT_MARGIN * STRICT_MARGIN; }

/**
 * Bursts of speech with a pause either side, as they end: feed it the
 * microphone in pieces (any size); it returns the burst's audio (with a
 * little before) once 0.3 s of quiet follows it, else null. Bursts longer
 * than 2.5 s (talk, not a call) are dropped. The quiet level follows the
 * room, as the spotter's gate does.
 */
export function createSegmenter() {
  let floor = 0.002, active = false, long = false, quietMs = 0, loudMs = 0;
  let parts = [], pre = null;
  const MIN_RMS = 0.006, END_QUIET_MS = 300, MAX_MS = 2500;
  return {
    accept(samples) {
      let sum = 0;
      for (let i = 0; i < samples.length; i++) sum += samples[i] * samples[i];
      const rms = Math.sqrt(sum / (samples.length || 1));
      const ms = samples.length / RATE * 1000;
      const loud = rms > Math.max(MIN_RMS, floor * 3);
      if (!loud) floor = rms < floor ? floor * 0.7 + rms * 0.3 : floor * 0.995 + rms * 0.005;
      if (!active) {
        if (loud) { active = true; long = false; quietMs = 0; loudMs = ms; parts = pre ? [pre, samples.slice()] : [samples.slice()]; }
        else pre = samples.slice();
        return null;
      }
      parts.push(samples.slice());
      if (loud) { quietMs = 0; loudMs += ms; } else quietMs += ms;
      if (loudMs + quietMs > MAX_MS) { long = true; parts = []; }
      if (quietMs < END_QUIET_MS) return null;
      active = false; pre = null;
      if (long) return null;
      const n = parts.reduce((a, p) => a + p.length, 0), out = new Float32Array(n);
      let o = 0; for (const p of parts) { out.set(p, o); o += p.length; }
      parts = [];
      return out;
    },
    reset() { active = false; long = false; quietMs = 0; loudMs = 0; parts = []; pre = null; },
  };
}

/** A stored profile that this code can use. */
export function validProfile(p) {
  // Version 4: deltas in the features; anything older compares unlike things and must be trained again.
  return !!(p && p.version === 4 && p.phraseFrames > 0 && Array.isArray(p.templates) && p.templates.length >= 3 && p.limit > 0 && p.keywordsThreshold > 0 && typeof p.keyword === 'string');
}
