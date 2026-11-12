/**
 * The voice model's own code for "Hey Cru"'s owner check (owner.js): the
 * features, the scores, the download and the embedder, as src/voice/speaker.js
 * has them for the server's check.
 *
 * A copy, not an import: the client-only PC build (build/electron-builder-
 * client.yml) has no src/, and the server's Docker image has no app/voice-wake,
 * so neither can load the other's file. scripts/test-voice-wake-owner.mjs
 * checks the two agree (the same features for the same audio, the same model),
 * so they cannot drift apart. onnxruntime-node is loaded only when asked for,
 * and its absence is an answer (no voice check), never an error at start.
 */

import { existsSync, statSync, createWriteStream, renameSync, mkdirSync, createReadStream, unlinkSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

export const SAMPLE_RATE = 16000;
export const WORD_K = 3;

export const MODEL = {
  id: 'eres2net-voxceleb',
  file: '3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx',
  url: 'https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx',
  bytes: 26485263,
  sha256: 'c59158379255ad66e161679cca6af8d52d51e389e3224ab7d7a7baae295c2db5',
  dim: 192,
};

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
/** Mean of the [k] best cosines of normalised [v] with the taught [clips] (normalised). */
export function wordScore(v, clips, k = WORD_K) {
  if (!clips || !clips.length) return NaN;
  const c = clips.map((x) => cosine(v, x)).sort((a, b) => b - a);
  const n = Math.min(k, c.length);
  let s = 0; for (let i = 0; i < n; i++) s += c[i];
  return s / n;
}

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

