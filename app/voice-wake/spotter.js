/**
 * The keyword spotter itself: sherpa-onnx's WebAssembly build for Node,
 * loaded from the folder install.js filled. Used by worker.js (the utility
 * process) and by test-spotter.mjs (the check on recorded speech).
 */

import { createRequire } from 'module';
import { join } from 'path';
import { ENGINE, MODEL, KEYWORDS, TUNING, SAMPLE_RATE } from './manifest.js';

/**
 * @param {string} baseDir  <userData>/voice-wake
 * @param {object} [over]   TUNING overrides (tests)
 * @returns {{ accept(samples: Float32Array): string|null, listening: boolean, reset(): void, free(): void }}
 */
export function createSpotter(baseDir, over = {}) {
  const tune = { ...TUNING, ...over };
  const engineDir = join(baseDir, ENGINE.id, 'package');
  const modelDir = join(baseDir, MODEL.id);
  const req = createRequire(join(engineDir, 'package.json'));

  // The glue attaches its exports to the object handed in, synchronously
  // (the wasm is read with readFileSync and compiled before it returns).
  const Module = {};
  req(join(engineDir, 'sherpa-onnx-wasm-nodejs.js'))(Module);
  const { createKws } = req(join(engineDir, 'sherpa-onnx-kws.js'));

  const kws = createKws(Module, {
    featConfig: { samplingRate: SAMPLE_RATE, featureDim: 80 },
    modelConfig: {
      transducer: {
        encoder: join(modelDir, '..', MODEL.encoder),
        decoder: join(modelDir, '..', MODEL.decoder),
        joiner: join(modelDir, '..', MODEL.joiner),
      },
      tokens: join(modelDir, '..', MODEL.tokens),
      provider: 'cpu',
      modelType: '',
      numThreads: 1,
      debug: 0,
      modelingUnit: 'bpe',
      bpeVocab: '',
    },
    maxActivePaths: tune.maxActivePaths,
    numTrailingBlanks: tune.numTrailingBlanks,
    keywordsScore: tune.keywordsScore,
    keywordsThreshold: tune.keywordsThreshold,
    keywords: over.keywords || KEYWORDS,
  });
  if (!kws.handle) throw new Error('The keyword spotter could not be created (model files missing or damaged)');
  let stream = kws.createStream();

  // A quiet room costs nothing: audio is decoded only from just before
  // something louder than the room's own noise until a while after it ends.
  // The floor follows the room (down quickly, up slowly); 0.5 s is kept from
  // before the sound starts so the "H" of "Hey" is not lost.
  const gate = { floor: 0.002, open: false, quietMs: 0, pre: [], preLen: 0 };
  const PRE_SAMPLES = SAMPLE_RATE / 2, HANG_MS = 1500, MIN_RMS = 0.0025;

  function decode(samples) {
    stream.acceptWaveform(SAMPLE_RATE, samples);
    let hit = null;
    while (kws.isReady(stream)) {
      kws.decode(stream);
      const r = kws.getResult(stream);
      if (r && r.keyword) {
        hit = r.keyword;
        // Start the next search from here, not from the keyword again.
        kws.reset(stream);
      }
    }
    return hit;
  }

  return {
    accept(samples) {
      if (tune.gate === false) return decode(samples);
      let sum = 0;
      for (let i = 0; i < samples.length; i++) sum += samples[i] * samples[i];
      const rms = Math.sqrt(sum / (samples.length || 1));
      const loud = rms > Math.max(MIN_RMS, gate.floor * 3);
      if (!loud) gate.floor = rms < gate.floor ? gate.floor * 0.7 + rms * 0.3 : gate.floor * 0.995 + rms * 0.005;
      const ms = samples.length / SAMPLE_RATE * 1000;
      if (loud) {
        gate.quietMs = 0;
        if (!gate.open) {
          gate.open = true;
          let hit = null;
          for (const p of gate.pre) hit = decode(p) || hit;
          gate.pre = []; gate.preLen = 0;
          return decode(samples) || hit;
        }
        return decode(samples);
      }
      if (gate.open) {
        gate.quietMs += ms;
        const hit = decode(samples);
        if (gate.quietMs >= HANG_MS) {
          gate.open = false;
          // Flush what is left of the last word, then start clean.
          const tail = decode(new Float32Array(SAMPLE_RATE / 2));
          stream.free(); stream = kws.createStream();
          return hit || tail;
        }
        return hit;
      }
      gate.pre.push(samples.slice());
      gate.preLen += samples.length;
      while (gate.preLen - gate.pre[0].length >= PRE_SAMPLES) gate.preLen -= gate.pre.shift().length;
      return null;
    },
    get listening() { return gate.open; },
    reset() {
      try { stream.free(); } catch { /* gone */ }
      gate.open = false; gate.quietMs = 0; gate.pre = []; gate.preLen = 0;
      stream = kws.createStream();
    },
    free() {
      try { stream.free(); } catch { /* gone */ }
      try { kws.free(); } catch { /* gone */ }
    },
  };
}
