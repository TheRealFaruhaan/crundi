/**
 * voice/speaker.js — is it the owner speaking? Speaker verification for Cru
 * (card: "Cru should not answer when the voice isn't the owner's").
 *
 * The model: 3D-Speaker ERes2Net trained on VoxCeleb (26 MB ONNX,
 * Apache-2.0), published by sherpa-onnx. It is downloaded on first use into
 * <dataDir>/voice-memory/models, checked by size and SHA-256, and never kept
 * in the repository. It runs here, in Node, through onnxruntime-node (present
 * through the optional @huggingface/transformers); without it there is simply
 * no voice check and everything else works.
 *
 * Its input is what sherpa-onnx computes for it (kaldi-native-fbank): 80 log
 * mel bins, 25 ms frames every 10 ms, a povey window, preemphasis 0.97, the
 * DC offset removed, no dither, edges reflected (snip_edges false), samples
 * in [-1, 1] (the model's normalize_samples=1), then each bin's mean over the
 * utterance subtracted (feature_normalize_type=global-mean). fbank() below
 * is that, checked against sherpa-onnx's own embeddings (cosine > 0.99).
 *
 * The owner's voice (the profile) is the mean of his L2-normalised
 * enrolment embeddings; a phrase is his when its embedding's cosine with the
 * profile is at least THRESHOLD. Less than MIN_SECONDS of speech is too
 * little to tell: the verdict is null (unknown), never a guess.
 *
 * Verdicts: true (the owner), false (another voice), null (no profile, too
 * short, no model). The profile lives in <dataDir>/voice-memory/speaker.json,
 * 0600; recordings themselves are never kept.
 */

import { existsSync, readFileSync, unlinkSync, statSync, createWriteStream, renameSync, mkdirSync, createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { writePrivateFile } from '../private-fs.js';

export const SAMPLE_RATE = 16000;
/** Cosine with the profile at or above this: the owner (research, Nov 2026: 0.40 on raw cosine). */
export const THRESHOLD = 0.40;
/** Seconds of speech below which there is no verdict. */
export const MIN_SECONDS = 1.5;
/**
 * Shorter words (9 Nov, the owner: "shorter words should be recognised too"):
 * from 0.8 s a check is made against a slightly lower bar. Measured on the
 * owner's voice (ERes2Net, 1 s clips, 3 enrolment phrases): at 0.36 the owner
 * was refused about 5% of the time in a quiet room and no impostor of 1072
 * passed; in noise about 1% did. Admin mode still needs MIN_SECONDS.
 */
export const SHORT_MIN_SECONDS = 0.8;
export const SHORT_THRESHOLD = 0.36;
/** An enrolment needs this many phrases, and offers ENROL_PHRASES. */
export const MIN_PHRASES = 5;
/** A phrase that agrees less than this with the ones before it is asked again. */
export const MIN_CONSISTENCY = 0.35;
/** An enrolment session not finished in this long is forgotten. */
const SESSION_MS = 30 * 60 * 1000;

export const MODEL = {
  id: 'eres2net-voxceleb',
  file: '3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx',
  url: 'https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx',
  bytes: 26485263,
  sha256: 'c59158379255ad66e161679cca6af8d52d51e389e3224ab7d7a7baae295c2db5',
  dim: 192,
};

/** What the owner reads during enrolment: Cru-style commands and a longer sentence or two. */
export const ENROL_PHRASES = [
  'Hey Cru, what is running on the server right now?',
  'Open the Crundi project and show me the latest changes.',
  'Cru, remind me to check the build in twenty minutes.',
  'The quick brown fox jumps over the lazy dog, while the patient owner reads this sentence aloud.',
  'Start the development server and tell me when it is ready.',
  'Add a card to the backlog: fix the login page on small phones.',
  'How much have I spent on voice this month?',
  'When the tests have finished, read me anything that failed and what it said.',
  'Thank you, Cru. That will be all for now.',
];

/**
 * Short replies (9 Nov, the owner: "I don't mind pre-training for that"): an
 * optional step after the phrases. Each is said WORD_REPEATS times in one
 * recording, with a pause between; every saying is kept as a clip, and a
 * reply under SHORT_MIN_SECONDS is then checked against those clips.
 */
export const ENROL_WORDS = ['Yes', 'No', 'Yeah', 'Okay', 'Go ahead', 'Do it', 'Stop', 'Cancel', 'This one', 'That one'];
export const WORD_REPEATS = 3;
/** Short replies taught before they are used. */
export const MIN_WORDS = 5;
/** Below this much speech there is no check even with short replies taught. */
export const WORD_MIN_SECONDS = 0.3;
/** A saying longer than this while teaching is not a short reply. */
export const WORD_MAX_SECONDS = 1.6;
/** Teaching: a reply whose sayings are on average less like the profile than this is asked again. */
export const WORD_ENROL_MIN = 0.25;
/**
 * The check under MIN_SECONDS with short replies taught (decideShort): the
 * clip's word score is the mean of its WORD_K best cosines with the taught
 * clips. The owner: word score >= WORD_THRESHOLD and profile cosine >=
 * WORD_MAIN_MIN. Another voice (false) only when both are clearly low (word
 * < WORD_REJECT, profile < WORD_REJECT_MAIN); anything between is no verdict
 * ('short'), which still may answer a waiting confirmation (turnPolicy).
 *
 * Measured 9 Nov 2026 (ERes2Net through sherpa-onnx, word clips cut by
 * Parakeet word times, padded 0.08 s as the live stream does; 24 taught clips):
 *  - same word taught (CMU Arctic speakers and the owner's own recordings,
 *    a word said again against clips that include it): 0.3-0.5 s accepted
 *    87% (the owner 13 of 14), 0.5-0.8 s 100% (owner 14 of 14); other
 *    speakers saying the same word: 0 of 873 and 0 of 328 passed.
 *  - taught clips of other words (the harder case): 0.3-0.5 s accepted 64%
 *    (owner 41%), 0.5-0.8 s 87% (owner 70%); impostors 0 of 6881 and 3 of
 *    12609 (0.02%, all Arctic studio speakers against each other; 0 of 7332
 *    against the owner).
 *  - called another voice: no genuine clip of 757 under 0.8 s, 2 of 360 at
 *    0.8-1.5 s (the same 2 as the phrases' bar alone); about 93% of
 *    impostor clips. The rest is no verdict.
 *  - 0.8-1.5 s: the word rule let no impostor through that SHORT_THRESHOLD
 *    did not already (that bar passed 136 of 13787 there, nearly all Arctic
 *    studio voices against each other).
 * Quiet recordings only; no noise or phone-channel measurement, and the
 * owner's same-word clips came from one recording, so expect a little less.
 */
export const WORD_K = 3;
// 0.37 (10 Nov, from 0.48): the owner's own live "yes" scored 0.39 against his taught
// replies (the settings page's recording and the live stream sound a little different).
// Against the owner's taught clips, other speakers' short clips did not reach 0.38
// (0 of ~7,300 in the calibration set; /tmp/spk-short), so the bar can sit there.
export const WORD_THRESHOLD = 0.37;
export const WORD_MAIN_MIN = 0.30;
export const WORD_REJECT = 0.30;
export const WORD_REJECT_MAIN = 0.25;

// ─── features: kaldi-native-fbank as sherpa-onnx configures it ───

const FRAME_LEN = 400;   // 25 ms
const FRAME_SHIFT = 160; // 10 ms
const NFFT = 512;
const NUM_BINS = 80;
const PREEMPH = 0.97;
const FLT_EPS = 1.1920928955078125e-7;

let tables = null;
function getTables() {
  if (tables) return tables;
  const win = new Float64Array(FRAME_LEN);
  const a = (2 * Math.PI) / (FRAME_LEN - 1);
  for (let i = 0; i < FRAME_LEN; i++) win[i] = Math.pow(0.5 - 0.5 * Math.cos(a * i), 0.85);
  const mel = (f) => 1127 * Math.log(1 + f / 700);
  const nBins = NFFT / 2, width = SAMPLE_RATE / NFFT;
  const lo = mel(20), hi = mel(SAMPLE_RATE / 2 - 400), delta = (hi - lo) / (NUM_BINS + 1);
  const banks = [];
  for (let b = 0; b < NUM_BINS; b++) {
    const l = lo + b * delta, c = lo + (b + 1) * delta, r = lo + (b + 2) * delta;
    let first = -1; const w = [];
    for (let i = 0; i < nBins; i++) {
      const m = mel(width * i);
      if (m > l && m < r) { if (first < 0) first = i; w.push(m <= c ? (m - l) / (c - l) : (r - m) / (r - c)); }
    }
    banks.push({ first, w: Float64Array.from(w) });
  }
  // Radix-2 FFT tables.
  const rev = new Uint16Array(NFFT);
  const bits = Math.log2(NFFT);
  for (let i = 0; i < NFFT; i++) { let r = 0; for (let j = 0; j < bits; j++) r |= ((i >> j) & 1) << (bits - 1 - j); rev[i] = r; }
  const cos = new Float64Array(NFFT / 2), sin = new Float64Array(NFFT / 2);
  for (let i = 0; i < NFFT / 2; i++) { cos[i] = Math.cos((2 * Math.PI * i) / NFFT); sin[i] = -Math.sin((2 * Math.PI * i) / NFFT); }
  tables = { win, banks, rev, cos, sin };
  return tables;
}

function fft(re, im, t) {
  const n = NFFT;
  for (let i = 0; i < n; i++) {
    const j = t.rev[i];
    if (j > i) { let x = re[i]; re[i] = re[j]; re[j] = x; x = im[i]; im[i] = im[j]; im[j] = x; }
  }
  for (let size = 2; size <= n; size <<= 1) {
    const half = size >> 1, step = n / size;
    for (let s = 0; s < n; s += size) {
      for (let k = 0; k < half; k++) {
        const wr = t.cos[k * step], wi = t.sin[k * step];
        const a = s + k, b = a + half;
        const xr = re[b] * wr - im[b] * wi, xi = re[b] * wi + im[b] * wr;
        re[b] = re[a] - xr; im[b] = im[a] - xi;
        re[a] += xr; im[a] += xi;
      }
    }
  }
}

/** Frames kaldi makes from n samples with snip_edges false. */
export const numFrames = (n) => (n <= 0 ? 0 : Math.floor((n + FRAME_SHIFT / 2) / FRAME_SHIFT));

/**
 * 80-bin log mel filterbank of 16 kHz samples in [-1, 1], as kaldi-native-fbank
 * with sherpa-onnx's options. { frames, dim: 80, data: Float32Array frames*80 }.
 */
export function fbank(samples) {
  const t = getTables();
  const n = samples.length, frames = numFrames(n);
  const data = new Float32Array(frames * NUM_BINS);
  const w = new Float64Array(FRAME_LEN), re = new Float64Array(NFFT), im = new Float64Array(NFFT);
  const power = new Float64Array(NFFT / 2 + 1);
  for (let f = 0; f < frames; f++) {
    const start = f * FRAME_SHIFT + FRAME_SHIFT / 2 - FRAME_LEN / 2;
    let mean = 0;
    for (let i = 0; i < FRAME_LEN; i++) {
      let s = start + i;
      while (s < 0 || s >= n) s = s < 0 ? -s - 1 : 2 * n - 1 - s;
      w[i] = samples[s]; mean += w[i];
    }
    mean /= FRAME_LEN;
    for (let i = 0; i < FRAME_LEN; i++) w[i] -= mean;
    for (let i = FRAME_LEN - 1; i > 0; i--) w[i] -= PREEMPH * w[i - 1];
    w[0] -= PREEMPH * w[0];
    re.fill(0); im.fill(0);
    for (let i = 0; i < FRAME_LEN; i++) re[i] = w[i] * t.win[i];
    fft(re, im, t);
    for (let i = 0; i <= NFFT / 2; i++) power[i] = re[i] * re[i] + im[i] * im[i];
    for (let b = 0; b < NUM_BINS; b++) {
      const { first, w: bw } = t.banks[b];
      let e = 0;
      for (let k = 0; k < bw.length; k++) e += bw[k] * power[first + k];
      data[f * NUM_BINS + b] = Math.log(Math.max(e, FLT_EPS));
    }
  }
  return { frames, dim: NUM_BINS, data };
}

/** Each bin's mean over the frames subtracted (3D-Speaker's global-mean), in place. */
export function subtractMean({ frames, dim, data }) {
  if (!frames) return data;
  const m = new Float64Array(dim);
  for (let f = 0; f < frames; f++) for (let d = 0; d < dim; d++) m[d] += data[f * dim + d];
  for (let d = 0; d < dim; d++) m[d] /= frames;
  for (let f = 0; f < frames; f++) for (let d = 0; d < dim; d++) data[f * dim + d] -= m[d];
  return data;
}

// ─── audio in ───

/** 16-bit little-endian PCM to floats in [-1, 1]. */
export function pcm16ToFloat(buf) {
  const b = Buffer.isBuffer(buf) ? buf : Buffer.from(buf.buffer || buf, buf.byteOffset || 0, buf.byteLength);
  const n = b.length >> 1, out = new Float32Array(n);
  for (let i = 0; i < n; i++) out[i] = b.readInt16LE(i * 2) / 32768;
  return out;
}

/**
 * Resample [x] from [from] Hz to 16 kHz: a box low-pass over each output
 * sample's span when going down, linear interpolation when going up.
 */
export function resample(x, from, to = SAMPLE_RATE) {
  if (from === to) return x;
  const ratio = from / to, n = Math.floor(x.length / ratio), out = new Float32Array(n);
  if (ratio > 1) {
    for (let i = 0; i < n; i++) {
      const a = Math.floor(i * ratio), b = Math.min(x.length, Math.max(a + 1, Math.floor((i + 1) * ratio)));
      let s = 0; for (let j = a; j < b; j++) s += x[j];
      out[i] = s / (b - a);
    }
  } else {
    for (let i = 0; i < n; i++) {
      const p = i * ratio, a = Math.floor(p), f = p - a;
      out[i] = x[a] * (1 - f) + (a + 1 < x.length ? x[a + 1] : x[a]) * f;
    }
  }
  return out;
}

/** A WAV file (PCM 16-bit or float 32, any rate, mono or more) as 16 kHz mono floats. */
export function parseWav(buf) {
  const b = Buffer.from(buf);
  if (b.length < 12 || b.toString('ascii', 0, 4) !== 'RIFF' || b.toString('ascii', 8, 12) !== 'WAVE') throw new Error('Not a WAV file');
  let fmt = null, data = null;
  for (let p = 12; p + 8 <= b.length;) {
    const id = b.toString('ascii', p, p + 4);
    let size = b.readUInt32LE(p + 4);
    const body = p + 8;
    if (body + size > b.length) size = b.length - body; // a streamed WAV may say 0 or too much
    if (id === 'fmt ') fmt = { format: b.readUInt16LE(body), channels: b.readUInt16LE(body + 2), rate: b.readUInt32LE(body + 4), bits: b.readUInt16LE(body + 14) };
    else if (id === 'data') { data = b.subarray(body, body + size); break; }
    p = body + size + (size & 1);
  }
  if (!fmt || !data) throw new Error('A WAV file without its format or its audio');
  const fmtCode = fmt.format === 0xfffe ? (fmt.bits === 32 ? 3 : 1) : fmt.format;
  const ch = Math.max(1, fmt.channels), bytes = fmt.bits / 8;
  if (!((fmtCode === 1 && fmt.bits === 16) || (fmtCode === 3 && fmt.bits === 32))) throw new Error('WAV must be 16-bit PCM or 32-bit float');
  const frames = Math.floor(data.length / (bytes * ch)), mono = new Float32Array(frames);
  for (let i = 0; i < frames; i++) {
    let s = 0;
    for (let c = 0; c < ch; c++) {
      const o = (i * ch + c) * bytes;
      s += fmtCode === 1 ? data.readInt16LE(o) / 32768 : data.readFloatLE(o);
    }
    mono[i] = s / ch;
  }
  return resample(mono, fmt.rate);
}

/**
 * An uploaded recording as 16 kHz mono floats: audio/wav, or raw
 * audio/L16;rate=N (16-bit little-endian, as /api/voice/listen takes it).
 */
export function decodeAudio(buf, contentType = '') {
  const ct = String(contentType).toLowerCase();
  if (/wav/.test(ct) || (buf.length > 12 && Buffer.from(buf).toString('ascii', 0, 4) === 'RIFF')) return parseWav(buf);
  if (/audio\/l16|linear16|pcm/.test(ct)) {
    const rate = Number((ct.match(/rate=(\d+)/) || [])[1]) || SAMPLE_RATE;
    if (rate < 8000 || rate > 96000) throw new Error('Unsupported sample rate');
    return resample(pcm16ToFloat(buf), rate);
  }
  throw new Error('Send audio/wav or audio/L16;rate=16000 (16-bit PCM)');
}

/**
 * Where the speech is in 16 kHz samples, by frame energy: frames 9 dB
 * above the recording's own quiet (its 10th percentile) and above -55 dBFS,
 * short pauses between words included (close to silero VAD's count).
 * { seconds, start, end } (sample indices of the first and last speech frame).
 */
export function findSpeech(x) {
  const step = FRAME_SHIFT, n = Math.floor(x.length / step);
  if (!n) return { seconds: 0, start: 0, end: 0 };
  const db = new Float64Array(n);
  for (let f = 0; f < n; f++) {
    let s = 0; for (let i = f * step; i < (f + 1) * step; i++) s += x[i] * x[i];
    db[f] = 10 * Math.log10(s / step + 1e-12);
  }
  const sorted = Float64Array.from(db).sort();
  const floor = sorted[Math.floor(n * 0.1)], loud = sorted[Math.floor(n * 0.9)];
  // Below the loud end too: speech with hardly a pause in it has no quiet to measure from.
  const gate = Math.max(Math.min(floor + 9, loud - 6), -55);
  // Speech frames, with pauses of up to 0.2 s between words counted as speech.
  let count = 0, first = -1, last = -1;
  for (let f = 0; f < n; f++) {
    if (db[f] <= gate) continue;
    count += last >= 0 && f - last - 1 <= 20 ? f - last : 1;
    if (first < 0) first = f;
    last = f;
  }
  return { seconds: (count * step) / SAMPLE_RATE, start: first < 0 ? 0 : first * step, end: last < 0 ? 0 : (last + 1) * step };
}

/**
 * The separate sayings in a recording of a short reply said a few times
 * ("yes … yes … yes"): runs of speech frames (by findSpeech's gate) apart by
 * more than [gap] seconds. Each { samples (padded 0.08 s, as the live
 * stream's words are), seconds }; blips under 0.12 s are left out.
 */
export function splitSayings(x, { gap = 0.3, pad = 0.08 } = {}) {
  const step = FRAME_SHIFT, n = Math.floor(x.length / step);
  if (!n) return [];
  const db = new Float64Array(n);
  for (let f = 0; f < n; f++) {
    let s = 0; for (let i = f * step; i < (f + 1) * step; i++) s += x[i] * x[i];
    db[f] = 10 * Math.log10(s / step + 1e-12);
  }
  const sorted = Float64Array.from(db).sort();
  const floor = sorted[Math.floor(n * 0.1)], loud = sorted[Math.floor(n * 0.9)], peak = sorted[n - 1];
  // Not far below the loudest sound either: a quiet recording's 90th percentile is the room.
  const gate = Math.max(Math.min(floor + 9, loud - 6), peak - 18, -55);
  const maxGap = Math.round(gap * 100), runs = [];
  for (let f = 0; f < n; f++) {
    if (db[f] <= gate) continue;
    const r = runs[runs.length - 1];
    if (r && f - r[1] - 1 <= maxGap) r[1] = f; else runs.push([f, f]);
  }
  const p = Math.round(pad * SAMPLE_RATE);
  // Background sounds (a click, a cough far off, a television) are much quieter than the
  // owner saying the word into the mic: only runs within 12 dB of the loudest are sayings.
  const runPeak = (a, b) => { let m = -Infinity; for (let f = a; f <= b; f++) if (db[f] > m) m = db[f]; return m; };
  return runs.filter(([a, b]) => runPeak(a, b) >= peak - 12)
    .map(([a, b]) => ({ a: a * step, b: (b + 1) * step, seconds: ((b + 1 - a) * step) / SAMPLE_RATE }))
    .filter((r) => r.seconds >= 0.15)
    .map((r) => ({ samples: x.subarray(Math.max(0, r.a - p), Math.min(x.length, r.b + p)), seconds: Math.round(r.seconds * 100) / 100 }));
}

/** A recording with the silence before and after the speech cut (0.2 s kept each side). */
export function trimSilence(x) {
  const s = findSpeech(x);
  if (!s.seconds) return { samples: x.subarray(0, 0), seconds: 0 };
  const pad = 0.2 * SAMPLE_RATE;
  return { samples: x.subarray(Math.max(0, s.start - pad), Math.min(x.length, s.end + pad)), seconds: s.seconds };
}

// ─── vectors and verdicts ───

export function normalise(v) {
  let s = 0; for (let i = 0; i < v.length; i++) s += v[i] * v[i];
  const k = 1 / (Math.sqrt(s) || 1), out = new Float32Array(v.length);
  for (let i = 0; i < v.length; i++) out[i] = v[i] * k;
  return out;
}
export function cosine(a, b) {
  let d = 0, x = 0, y = 0;
  for (let i = 0; i < a.length; i++) { d += a[i] * b[i]; x += a[i] * a[i]; y += b[i] * b[i]; }
  return d / (Math.sqrt(x * y) || 1);
}
/** The profile: the normalised mean of the normalised embeddings. */
export function meanProfile(list) {
  const m = new Float32Array(list[0].length);
  for (const v of list) { const u = normalise(v); for (let i = 0; i < m.length; i++) m[i] += u[i]; }
  return normalise(m);
}

/** true / false / null from a score and how much speech it rests on. */
export function decide(score, seconds, threshold = THRESHOLD) {
  if (!Number.isFinite(score)) return null;
  if (seconds >= MIN_SECONDS) return score >= threshold;
  if (seconds >= SHORT_MIN_SECONDS) return score >= Math.min(threshold, SHORT_THRESHOLD);
  return null;
}

/** Mean of the [k] best cosines of normalised [v] with the taught [clips] (normalised). */
export function wordScore(v, clips, k = WORD_K) {
  if (!clips || !clips.length) return NaN;
  const c = clips.map((x) => cosine(v, x)).sort((a, b) => b - a);
  const n = Math.min(k, c.length);
  let s = 0; for (let i = 0; i < n; i++) s += c[i];
  return s / n;
}

/**
 * true / false / null for a clip under MIN_SECONDS when short replies are
 * taught: [word] its wordScore, [main] its cosine with the profile. From
 * SHORT_MIN_SECONDS the phrases' own bar (decide) may say yes too, and only a
 * clearly low score says another voice; under WORD_MIN_SECONDS, null.
 */
export function decideShort({ word, main, seconds }) {
  if (!Number.isFinite(word) || !Number.isFinite(main) || seconds < WORD_MIN_SECONDS) return null;
  if (seconds >= MIN_SECONDS) return decide(main, seconds);
  if (word >= WORD_THRESHOLD && main >= WORD_MAIN_MIN) return true;
  if (seconds >= SHORT_MIN_SECONDS && decide(main, seconds)) return true;
  if (word < WORD_REJECT && main < WORD_REJECT_MAIN) return false;
  return null;
}

/** The taught clips in a profile file's words, normalised; null when none. */
function wordClips(words) {
  const list = words && Array.isArray(words.list) ? words.list : [];
  const clips = [];
  for (const w of list) for (const c of (Array.isArray(w.clips) ? w.clips : [])) if (Array.isArray(c) && c.length) clips.push(normalise(Float32Array.from(c)));
  return clips.length ? clips : null;
}

/**
 * What a turn may do, given its voice verdict (pure; webapp.js and the tests).
 *
 *  - unrecognised: with a profile and "Only act on my voice" on, a spoken
 *    turn that was another voice (false), or too short to tell, still gets
 *    answers but changes nothing (actions.js gated()). A spoken turn that
 *    could not be checked at all (the phone's own recogniser, no model) is
 *    not held back here: there is nothing to judge it by.
 *  - admin: with a profile, admin tools need the owner's voice (true) or a
 *    typed turn; without one they are allowed (the owner's own choice).
 *
 * @param o.enrolled  a profile exists
 * @param o.ownerOnly the "Only act on my voice" setting
 * @param o.typed     typed by the signed-in owner (no voice at all)
 * @param o.verdict   { owner: true|false|null, reason } from the voice check, or null (none found)
 * @param o.answering a confirmation asked during a recognised (or typed) turn is waiting, recent
 * @param o.recentOwner a full-length turn in the owner's recognised voice (or typed) within ADMIN_FOLLOW_MS, no other voice since
 * @returns { unrecognised, admin: { owner: true|false, reason }, confirmOk }
 */
export const ADMIN_FOLLOW_MS = 120_000;
export function turnPolicy({ enrolled = false, ownerOnly = true, typed = false, verdict = null, answering = false, recentOwner = false } = {}) {
  const owner = verdict ? verdict.owner : null;
  const short = !!verdict && owner === null && verdict.reason === 'short';
  // Too short to tell, moments after a full sentence recognised as the owner's
  // (recentOwner: within ADMIN_FOLLOW_MS, no other voice since): taken as the
  // owner carrying on (10 Nov). Another voice (false) never is.
  const carryOn = short && recentOwner;
  const unrecognised = enrolled && ownerOnly && !typed && (owner === false || (short && !carryOn));
  // A bare "yes" is too short to tell whose voice it is. It may still answer a
  // question that a recognised request asked moments ago (answering: such a
  // confirmation is waiting): then it may confirm or cancel that one action,
  // and nothing else. Another voice (false) never confirms anything.
  const confirmOk = !unrecognised || (short && answering);
  let admin;
  if (!enrolled) admin = { owner: true, reason: 'no voice enrolled' };
  // A taught short reply ("yes", "go ahead") matched as his, moments after a
  // full sentence recognised as his (recentOwner, nobody else in between):
  // enough to answer Cru in admin mode (the owner, 9 Nov: "the only reason I
  // wanted it was to use in admin mode"). A brief match on its own is not.
  else if (owner === true && verdict && verdict.brief && !typed && Number.isFinite(verdict.word) && recentOwner) admin = { owner: true, reason: 'a taught short reply in the owner\'s voice, just after a recognised request' };
  else if (owner === true && verdict && verdict.brief && !typed) admin = { owner: false, reason: 'That was too short to be sure enough for admin mode. Nothing was done: ask them to say it a bit longer.' };
  else if (owner === true) admin = { owner: true, reason: 'the owner\'s voice' };
  else if (owner === false) admin = { owner: false, reason: 'The speaker was not recognised as the owner, and admin mode only obeys the owner. Nothing was done: say you couldn\'t recognise who they are, so you can\'t do that.' };
  else if (typed) admin = { owner: true, reason: 'typed by the owner' };
  else if (short) admin = { owner: false, reason: 'That was too short to tell who is speaking, and admin mode only obeys the owner. Nothing was done: ask them to say it a bit longer so you can recognise who they are.' };
  else admin = { owner: false, reason: 'Who is speaking could not be checked for that request, and admin mode only obeys the owner. Nothing was done: ask them to say it again, a bit longer, or to type it.' };
  return { unrecognised, admin, confirmOk };
}

/** Text as compared between an utterance and the turn it became. */
export const normText = (t) => String(t || '').toLowerCase().replace(/[^\p{L}\p{N}]+/gu, ' ').trim();

/**
 * The last verdicts, by what was said, so a turn posted with that text can
 * find its own (within windowMs). A turn of several utterances (said while
 * Cru was thinking) gets false if any part was another voice, true if a part
 * was the owner's and none another's.
 */
export function createRecentVerdicts({ windowMs = 120_000, max = 64, now = () => Date.now() } = {}) {
  let list = [];
  return {
    remember(text, v) {
      const norm = normText(text);
      if (!norm || !v) return;
      // brief and word travel with it: admin mode tells a full sentence from a short reply by them.
      list.push({ norm, owner: v.owner ?? null, reason: v.reason || '', score: v.score, brief: !!v.brief, ...(Number.isFinite(v.word) ? { word: v.word } : {}), seconds: v.seconds, at: now() });
      if (list.length > max) list = list.slice(-max);
    },
    /** { owner, reason } for [text], or null: nothing heard matches (typed, or the phone's own recogniser). */
    lookup(text) {
      const norm = normText(text), t = now();
      if (!norm) return null;
      const fresh = list.filter((e) => t - e.at <= windowMs);
      list = fresh;
      const exact = fresh.filter((e) => e.norm === norm).pop();
      if (exact) return { owner: exact.owner, reason: exact.reason, score: exact.score, brief: exact.brief, ...(Number.isFinite(exact.word) ? { word: exact.word } : {}), seconds: exact.seconds };
      const parts = fresh.filter((e) => e.norm.length >= 3 && (' ' + norm + ' ').includes(' ' + e.norm + ' '));
      if (!parts.length) return null;
      if (parts.some((e) => e.owner === false)) return { owner: false, reason: 'other' };
      // Pieces of a turn: brief unless one owner piece was a full-length check.
      if (parts.some((e) => e.owner === true)) return { owner: true, reason: 'owner', brief: !parts.some((e) => e.owner === true && !e.brief) };
      return { owner: null, reason: parts[parts.length - 1].reason };
    },
    clear() { list = []; },
  };
}

// ─── the model ───

async function sha256File(file) {
  const h = createHash('sha256');
  await pipeline(createReadStream(file), h);
  return h.digest('hex');
}

/**
 * Make sure the model is in [dir], downloading it once. Resolves with its
 * path; rejects with a reason (no network, wrong size or hash).
 */
export async function ensureModel(dir, { fetchImpl = globalThis.fetch, model = MODEL } = {}) {
  const file = join(dir, model.file);
  if (existsSync(file) && statSync(file).size === model.bytes) return file;
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  const tmp = `${file}.${process.pid}.part`;
  const res = await fetchImpl(model.url, { redirect: 'follow' });
  if (!res.ok || !res.body) throw new Error(`model download failed (HTTP ${res.status})`);
  await pipeline(Readable.fromWeb(res.body), createWriteStream(tmp, { mode: 0o600 }));
  try {
    const size = statSync(tmp).size;
    if (size !== model.bytes) throw new Error(`model download is ${size} bytes, expected ${model.bytes}`);
    const hash = await sha256File(tmp);
    if (hash !== model.sha256) throw new Error('model download failed its checksum');
    renameSync(tmp, file);
  } catch (e) { try { unlinkSync(tmp); } catch { /* gone */ } throw e; }
  return file;
}

/** onnxruntime-node, or null when it is not installed (an optional dependency). */
let ortPromise = null;
export function loadOrt() {
  if (!ortPromise) ortPromise = import('onnxruntime-node').then((m) => m.default || m, () => null);
  return ortPromise;
}

/**
 * The embedder: (samples 16 kHz Float32Array) => Promise<Float32Array(192)>.
 * The session is made once; onnxruntime runs it off the event loop.
 */
export function createOnnxEmbedder({ modelDir, fetchImpl } = {}) {
  let session = null, loading = null, failed = '', ort = null;
  async function ready() {
    if (session) return session;
    if (!loading) loading = (async () => {
      try {
        ort = await loadOrt();
        if (!ort) throw new Error('onnxruntime-node is not installed');
        const file = await ensureModel(modelDir, { fetchImpl });
        session = await ort.InferenceSession.create(file, { intraOpNumThreads: 2, interOpNumThreads: 1, graphOptimizationLevel: 'all' });
        failed = '';
        return session;
      } catch (e) {
        failed = String(e.message || e).slice(0, 200);
        loading = null; // try again next time (the network may be back)
        throw e;
      }
    })();
    return loading;
  }
  return {
    async available() { return !!(await loadOrt()); },
    get state() { return session ? 'ready' : failed ? `unavailable: ${failed}` : loading ? 'loading' : 'not loaded yet'; },
    load: ready,
    async embed(samples) {
      const s = await ready();
      const feats = fbank(samples);
      subtractMean(feats);
      const x = new ort.Tensor('float32', feats.data, [1, feats.frames, feats.dim]);
      const out = await s.run({ [s.inputNames[0]]: x });
      return Float32Array.from(out[s.outputNames[0]].data);
    },
  };
}

// ─── the owner's voice ───

/**
 * @param o.dir       <dataDir>/voice-memory (speaker.json, models/)
 * @param o.embedder  createOnnxEmbedder(); tests pass a fake { available, embed, state }
 */
export function createSpeaker({ dir, embedder, now = () => Date.now() } = {}) {
  const file = join(dir, 'speaker.json');
  const emb = embedder || createOnnxEmbedder({ modelDir: join(dir, 'models') });
  const recent = createRecentVerdicts({ now });
  let profile = null;
  let session = null; // { started, phrases: [{ v, seconds }] }

  let wordSession = null; // { started, words: Map(index -> [Float32Array]) }

  function loadProfile() {
    if (profile !== null) return profile || null;
    try {
      const p = JSON.parse(readFileSync(file, 'utf8'));
      profile = p && Array.isArray(p.mean) && p.mean.length ? { ...p, vec: Float32Array.from(p.mean), clips: wordClips(p.words) } : false;
    } catch { profile = false; }
    return profile || null;
  }
  function liveSession() {
    if (session && now() - session.started > SESSION_MS) session = null;
    return session;
  }
  function liveWords() {
    if (wordSession && now() - wordSession.started > SESSION_MS) wordSession = null;
    return wordSession;
  }
  function readRecord() {
    try { return JSON.parse(readFileSync(file, 'utf8')); } catch { return null; }
  }
  function writeRecord(rec) {
    mkdirSync(dir, { recursive: true, mode: 0o700 });
    writePrivateFile(file, JSON.stringify(rec));
    profile = null;
  }

  async function embedSpeech(samples) {
    const v = await emb.embed(samples);
    if (!v || !v.length) throw new Error('no embedding');
    return v;
  }

  return {
    get enrolled() { return !!loadProfile(); },
    recent,

    async status() {
      const p = loadProfile(), s = liveSession();
      return {
        available: await emb.available(),
        model: emb.state,
        enrolled: !!p,
        phrases: p ? p.count : 0,
        seconds: p ? p.seconds : 0,
        enrolledAt: p ? p.enrolledAt : null,
        threshold: THRESHOLD,
        minSeconds: MIN_SECONDS,
        minPhrases: MIN_PHRASES,
        enrolling: s ? { phrases: s.phrases.length, seconds: round1(s.phrases.reduce((a, x) => a + x.seconds, 0)) } : null,
        prompts: ENROL_PHRASES,
        // Short replies (capability 'voice-short-words').
        shortWords: p && p.clips ? { trained: true, words: p.words.list.length, clips: p.clips.length, at: p.words.at || null } : { trained: false, words: 0, clips: 0, at: null },
        wordPrompts: ENROL_WORDS,
        wordRepeats: WORD_REPEATS,
        minWords: MIN_WORDS,
        wordsEnrolling: liveWords() ? { words: wordSession.words.size } : null,
      };
    },

    /**
     * Is [samples] (16 kHz floats of speech) the owner? { owner, reason,
     * score?, seconds }. reason: owner | other | short | no-profile | unavailable.
     */
    async verify(samples, { seconds } = {}) {
      const p = loadProfile();
      const secs = seconds ?? samples.length / SAMPLE_RATE;
      if (!p) return { owner: null, reason: 'no-profile', seconds: secs };
      const clips = p.clips;
      if (secs < (clips ? WORD_MIN_SECONDS : SHORT_MIN_SECONDS)) return { owner: null, reason: 'short', seconds: round2(secs) };
      try {
        const v = normalise(await embedSpeech(samples));
        const score = cosine(v, p.vec);
        const word = clips && secs < MIN_SECONDS ? wordScore(v, clips) : null;
        const owner = word === null ? decide(score, secs) : decideShort({ word, main: score, seconds: secs });
        const r3 = (x) => Math.round(x * 1000) / 1000;
        // brief: checked on less than MIN_SECONDS (enough for everyday requests, not for admin mode).
        return { owner, reason: owner === null ? 'short' : owner ? 'owner' : 'other', score: r3(score), ...(word === null ? {} : { word: r3(word) }), seconds: round2(secs), brief: secs < MIN_SECONDS };
      } catch { return { owner: null, reason: 'unavailable', seconds: round2(secs) }; }
    },

    /** Short replies taught (the live stream checks from WORD_MIN_SECONDS then). */
    get shortWords() { const p = loadProfile(); return !!(p && p.clips); },

    /**
     * One enrolment phrase (16 kHz floats, the whole recording). [restart]
     * begins a new session. { ok, accepted, seconds, consistency, phrases,
     * problem? }: a phrase too short or unlike the others is not kept.
     */
    async enrolPhrase(samples, { restart = false } = {}) {
      if (restart || !liveSession()) session = { started: now(), phrases: [] };
      const { samples: speech, seconds } = trimSilence(samples);
      const base = { ok: true, seconds: round1(seconds), phrases: session.phrases.length };
      if (seconds < MIN_SECONDS) return { ...base, accepted: false, consistency: null, problem: seconds < 0.3 ? 'Nothing was heard. Check the microphone and try again.' : 'That was too short: read the whole phrase.' };
      let v;
      try { v = normalise(await embedSpeech(speech)); } catch (e) { return { ok: false, error: `The voice check is not available: ${e.message}` }; }
      const prev = session.phrases.map((p) => p.v);
      const consistency = prev.length ? Math.round(cosine(v, meanProfile(prev)) * 1000) / 1000 : null;
      if (consistency !== null && prev.length >= 2 && consistency < MIN_CONSISTENCY) {
        return { ...base, accepted: false, consistency, problem: 'That did not sound like the earlier phrases (another voice, or a lot of noise?). Try it again.' };
      }
      session.phrases.push({ v, seconds });
      return { ...base, accepted: true, consistency, phrases: session.phrases.length };
    },

    /** The session becomes the profile. */
    finish() {
      const s = liveSession();
      if (!s || s.phrases.length < MIN_PHRASES) return { ok: false, error: `Read at least ${MIN_PHRASES} phrases first (${s ? s.phrases.length : 0} so far).` };
      const mean = meanProfile(s.phrases.map((p) => p.v));
      // A new enrolment starts without short replies: they are taught again after it.
      const rec = { version: 1, model: MODEL.id, dim: mean.length, mean: Array.from(mean, (x) => Math.round(x * 1e6) / 1e6), count: s.phrases.length, seconds: round1(s.phrases.reduce((a, x) => a + x.seconds, 0)), enrolledAt: new Date(now()).toISOString() };
      writeRecord(rec);
      session = null;
      wordSession = null;
      recent.clear();
      return { ok: true, phrases: rec.count, seconds: rec.seconds, enrolledAt: rec.enrolledAt };
    },

    /**
     * One short reply ([word]: its index in ENROL_WORDS), said WORD_REPEATS
     * times in one recording. [restart] begins the step afresh. { ok,
     * accepted, word, sayings, seconds (all of them), each (seconds each), likeYou,
     * words, problem? }.
     */
    async enrolWord(samples, { word, restart = false } = {}) {
      const p = loadProfile();
      if (!p) return { ok: false, error: 'Enrol your voice with the phrases first.' };
      const w = Number(word);
      if (!Number.isInteger(w) || w < 0 || w >= ENROL_WORDS.length) return { ok: false, error: 'No such word' };
      if (restart || !liveWords()) wordSession = { started: now(), words: new Map() };
      // However many times it was said, and however the pauses fell: every
      // saying is used. A run too long for one word (two sayings run together)
      // is cut into word-sized pieces. Nothing is judged against the phrases'
      // voiceprint: short words score low against it, which is the reason for
      // this separate print (the owner, 10 Nov).
      const found = splitSayings(samples);
      const pieces = [];
      for (const x of found) {
        if (x.seconds <= WORD_MAX_SECONDS) { if (x.seconds >= 0.2) pieces.push(x); continue; }
        const n = Math.ceil(x.seconds / 0.9), len = Math.floor(x.samples.length / n);
        for (let i = 0; i < n; i++) { const part = x.samples.subarray(i * len, (i + 1) * len); pieces.push({ samples: part, seconds: round2(part.length / SAMPLE_RATE) }); }
      }
      const base = { ok: true, accepted: false, word: w, sayings: pieces.length, seconds: round2(pieces.reduce((a, x) => a + x.seconds, 0)), each: pieces.map((x) => x.seconds), likeYou: null, words: wordSession.words.size };
      if (!pieces.length) return { ...base, problem: 'Nothing was heard. Check the microphone and try again.' };
      const vs = [];
      try { for (const x of pieces.slice(0, 8)) vs.push(normalise(await embedSpeech(x.samples))); } catch (e) { return { ok: false, error: `The voice check is not available: ${e.message}` }; }
      // Said again: added to what was learnt for this word (up to 12 clips).
      const had = wordSession.words.get(w) || [];
      wordSession.words.set(w, [...had, ...vs].slice(-12));
      return { ...base, accepted: true, words: wordSession.words.size };
    },

    /** The short replies said become part of the profile (at least MIN_WORDS of them). */
    finishWords() {
      const s = liveWords(), rec = readRecord();
      if (!rec || !Array.isArray(rec.mean)) return { ok: false, error: 'Enrol your voice with the phrases first.' };
      if (!s || s.words.size < MIN_WORDS) return { ok: false, error: `Say at least ${MIN_WORDS} of the short replies first (${s ? s.words.size : 0} so far).` };
      const list = [...s.words.entries()].sort((a, b) => a[0] - b[0])
        .map(([i, vs]) => ({ word: ENROL_WORDS[i], clips: vs.map((v) => Array.from(v, (x) => Math.round(x * 1e5) / 1e5)) }));
      rec.words = { at: new Date(now()).toISOString(), list };
      writeRecord(rec);
      wordSession = null;
      recent.clear();
      return { ok: true, words: list.length, clips: list.reduce((a, x) => a + x.clips.length, 0) };
    },

    /** Forget the short replies only (the phrases' profile stays). */
    forgetWords() {
      wordSession = null;
      const rec = readRecord();
      if (rec && rec.words) { delete rec.words; writeRecord(rec); recent.clear(); }
      return { ok: true };
    },

    /** Forget the owner's voice (and any enrolment in progress). */
    forget() {
      try { if (existsSync(file)) unlinkSync(file); } catch { /* gone */ }
      profile = false; session = null; wordSession = null; recent.clear();
      return { ok: true };
    },
  };
}

const round1 = (x) => Math.round(x * 10) / 10;
const round2 = (x) => Math.round(x * 100) / 100;

// ─── the live stream's recent audio ───

/**
 * The last ~30 s of the stream as 16 kHz floats, indexed on the stream's own
 * timeline (seconds of audio forwarded to Deepgram, which its word times
 * use). Chunks at another rate are resampled as they come.
 */
export function createAudioRing({ seconds = 30, rate = SAMPLE_RATE } = {}) {
  const size = seconds * SAMPLE_RATE;
  const buf = new Float32Array(size);
  let total = 0; // 16 kHz samples ever written
  let carry = 0; // input samples (fractional) owed to the next chunk when resampling
  const ratio = rate / SAMPLE_RATE;
  function write(x) {
    for (let i = 0; i < x.length; i++) buf[(total + i) % size] = x[i];
    total += x.length;
  }
  return {
    /** A chunk of 16-bit little-endian PCM at the stream's rate. */
    push(chunk) {
      const x = pcm16ToFloat(chunk);
      if (ratio === 1) { write(x); return; }
      // Keep the output count exact across chunks: out samples = in samples / ratio.
      const want = Math.floor((x.length + carry) / ratio);
      carry = (x.length + carry) - want * ratio;
      const y = resample(x, rate);
      if (y.length >= want) write(y.subarray(0, want));
      else { const z = new Float32Array(want); z.set(y); if (y.length) z.fill(y[y.length - 1], y.length); write(z); }
    },
    get seconds() { return total / SAMPLE_RATE; },
    /** Samples for [from, to) seconds of the stream, or null if not (all) kept. */
    span(from, to) {
      const a = Math.max(0, Math.round(from * SAMPLE_RATE)), b = Math.min(total, Math.round(to * SAMPLE_RATE));
      if (b <= a || a < total - size) return null;
      const out = new Float32Array(b - a);
      for (let i = a; i < b; i++) out[i - a] = buf[i % size];
      return out;
    },
  };
}

/**
 * The audio of an utterance's words: each word's [start, end] (padded a
 * little, overlaps merged) from the ring, joined. { samples, seconds }, or
 * null when the ring no longer has it.
 */
export function utteranceAudio(ring, spans, pad = 0.08) {
  if (!ring || !spans || !spans.length) return null;
  const s = spans.map(([a, b]) => [Math.max(0, a - pad), b + pad]).sort((x, y) => x[0] - y[0]);
  const merged = [];
  for (const [a, b] of s) { const m = merged[merged.length - 1]; if (m && a <= m[1]) m[1] = Math.max(m[1], b); else merged.push([a, b]); }
  const parts = [];
  for (const [a, b] of merged) { const x = ring.span(a, b); if (x) parts.push(x); }
  const n = parts.reduce((k, x) => k + x.length, 0);
  if (!n) return null;
  const out = new Float32Array(n);
  let o = 0; for (const x of parts) { out.set(x, o); o += x.length; }
  // Seconds of speech: the words themselves, not the padding around them.
  const words = [];
  for (const [a, b] of [...spans].sort((x, y) => x[0] - y[0])) { const m = words[words.length - 1]; if (m && a <= m[1]) m[1] = Math.max(m[1], b); else words.push([a, b]); }
  const seconds = Math.min(n / SAMPLE_RATE, words.reduce((k, [a, b]) => k + Math.max(0, b - a), 0));
  return { samples: out, seconds };
}
