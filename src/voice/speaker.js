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
  if (!(seconds >= MIN_SECONDS) || !Number.isFinite(score)) return null;
  return score >= threshold;
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
 * @returns { unrecognised, admin: { owner: true|false, reason }, confirmOk }
 */
export function turnPolicy({ enrolled = false, ownerOnly = true, typed = false, verdict = null, answering = false } = {}) {
  const owner = verdict ? verdict.owner : null;
  const short = !!verdict && owner === null && verdict.reason === 'short';
  const unrecognised = enrolled && ownerOnly && !typed && (owner === false || short);
  // A bare "yes" is too short to tell whose voice it is. It may still answer a
  // question that a recognised request asked moments ago (answering: such a
  // confirmation is waiting): then it may confirm or cancel that one action,
  // and nothing else. Another voice (false) never confirms anything.
  const confirmOk = !unrecognised || (short && answering);
  let admin;
  if (!enrolled) admin = { owner: true, reason: 'no voice enrolled' };
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
      list.push({ norm, owner: v.owner ?? null, reason: v.reason || '', score: v.score, at: now() });
      if (list.length > max) list = list.slice(-max);
    },
    /** { owner, reason } for [text], or null: nothing heard matches (typed, or the phone's own recogniser). */
    lookup(text) {
      const norm = normText(text), t = now();
      if (!norm) return null;
      const fresh = list.filter((e) => t - e.at <= windowMs);
      list = fresh;
      const exact = fresh.filter((e) => e.norm === norm).pop();
      if (exact) return { owner: exact.owner, reason: exact.reason, score: exact.score };
      const parts = fresh.filter((e) => e.norm.length >= 3 && (' ' + norm + ' ').includes(' ' + e.norm + ' '));
      if (!parts.length) return null;
      if (parts.some((e) => e.owner === false)) return { owner: false, reason: 'other' };
      if (parts.some((e) => e.owner === true)) return { owner: true, reason: 'owner' };
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

  function loadProfile() {
    if (profile !== null) return profile || null;
    try {
      const p = JSON.parse(readFileSync(file, 'utf8'));
      profile = p && Array.isArray(p.mean) && p.mean.length ? { ...p, vec: Float32Array.from(p.mean) } : false;
    } catch { profile = false; }
    return profile || null;
  }
  function liveSession() {
    if (session && now() - session.started > SESSION_MS) session = null;
    return session;
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
      if (secs < MIN_SECONDS) return { owner: null, reason: 'short', seconds: round1(secs) };
      try {
        const score = cosine(normalise(await embedSpeech(samples)), p.vec);
        const owner = decide(score, secs);
        return { owner, reason: owner ? 'owner' : 'other', score: Math.round(score * 1000) / 1000, seconds: round1(secs) };
      } catch { return { owner: null, reason: 'unavailable', seconds: round1(secs) }; }
    },

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
      const rec = { version: 1, model: MODEL.id, dim: mean.length, mean: Array.from(mean, (x) => Math.round(x * 1e6) / 1e6), count: s.phrases.length, seconds: round1(s.phrases.reduce((a, x) => a + x.seconds, 0)), enrolledAt: new Date(now()).toISOString() };
      mkdirSync(dir, { recursive: true, mode: 0o700 });
      writePrivateFile(file, JSON.stringify(rec));
      profile = null;
      session = null;
      recent.clear();
      return { ok: true, phrases: rec.count, seconds: rec.seconds, enrolledAt: rec.enrolledAt };
    },

    /** Forget the owner's voice (and any enrolment in progress). */
    forget() {
      try { if (existsSync(file)) unlinkSync(file); } catch { /* gone */ }
      profile = false; session = null; recent.clear();
      return { ok: true };
    },
  };
}

const round1 = (x) => Math.round(x * 10) / 10;

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
