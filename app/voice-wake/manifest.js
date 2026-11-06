/**
 * "Hey Cru" on the PC (D40): what the keyword spotter is made of, pinned.
 *
 * Nothing here is bundled into the installer. The first time the person turns
 * the option on, the two archives are fetched into
 * <userData>/voice-wake/, checked against the SHA-256 below (the archive and
 * then every file taken out of it), and only the files listed are kept.
 *
 * Engine: sherpa-onnx's WebAssembly build for Node (the npm package
 * `sherpa-onnx`, not `sherpa-onnx-node`): one .wasm and its JavaScript glue,
 * no native module, so nothing to rebuild for Electron's ABI. It runs in an
 * Electron utility process and reads the model straight from disk (the build
 * uses NODERAWFS).
 *
 * Model: k2-fsa's open-vocabulary English keyword spotter (a 3.3M-parameter
 * streaming zipformer trained on GigaSpeech). The keywords are not trained in:
 * they are BPE tokens, below, made with the model's own bpe.model
 * (sentencepiece), so the wake word can change without a new model.
 *
 * Sizes: download 4.3 MB (engine) + 17.6 MB (model) once; kept on disk
 * 15.4 MB + 5.3 MB. Small enough to bundle, but the wasm alone would add
 * 15 MB to every installer for an option that is off by default.
 */

import { KEYWORD } from './enroll.js';

export const ENGINE = {
  id: 'sherpa-onnx-2.0.1',
  url: 'https://registry.npmjs.org/sherpa-onnx/-/sherpa-onnx-2.0.1.tgz',
  sha256: '031b235001fb41cc20ad405abe48e07891094047d6f462c9bdbb5a57434d2d54',
  size: 4307523,
  // Inside the tarball everything is under package/.
  files: {
    'package/package.json': 'ba007a6184beaf549365e63a5fda5fadb57fb340a075af8acace669719cad413',
    'package/sherpa-onnx-wasm-nodejs.js': '51be1e6f24df5f9a0564a8701b0987349708ff5c40e04d1e9b1ebe270894868c',
    'package/sherpa-onnx-wasm-nodejs.wasm': '9b2851acd37eee1a04207fdfcb70adaff99963b8fda55f6a9dd7bcabd0ecd1e3',
    'package/sherpa-onnx-kws.js': '03b44e372795fd907e84eecd29419e1d052dc6704ac06935c9bf9fe19b3e3434',
  },
};

const MODEL_DIR = 'sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01';

export const MODEL = {
  id: MODEL_DIR,
  url: 'https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/' + MODEL_DIR + '.tar.bz2',
  sha256: 'f170013b4716e41b62b9bfd809687c207cef798ef9bc6534d524e17af9b6561a',
  size: 17626723,
  // The combination k2-fsa ships in its mobile package: int8 encoder and
  // joiner, float decoder (the decoder is small and int8 costs it accuracy).
  files: {
    [MODEL_DIR + '/encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx']: '1e721676515bcd42a186979733981213c66c80db680e1cc582dfedf3be76e678',
    [MODEL_DIR + '/decoder-epoch-12-avg-2-chunk-16-left-64.onnx']: 'f61ebd3eed3773a44d088d53dfae92dbb6aec4839f4dcaee2d402414741663a3',
    [MODEL_DIR + '/joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx']: 'eae9da0c7e1e6c6a3f4cc42d167899c388f6c6701b94cb96320e4f55df79624c',
    [MODEL_DIR + '/tokens.txt']: 'fd2ded4050a55d2b1578870ba8697d02371980217806b7558bd0a5cc60f3ba53',
  },
  encoder: MODEL_DIR + '/encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx',
  decoder: MODEL_DIR + '/decoder-epoch-12-avg-2-chunk-16-left-64.onnx',
  joiner: MODEL_DIR + '/joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx',
  tokens: MODEL_DIR + '/tokens.txt',
};

/**
 * The wake word, as the model's BPE tokens. Made with
 *   sentencepiece.SentencePieceProcessor(model_file='bpe.model')
 *     .encode('HEY CRU', out_type=str)
 * which gives ▁HE Y ▁C RU. The text after @ is what the spotter reports.
 * There is no bare "Cru": said alone it is "crew", far too common a word.
 *
 * This is only the default (test-spotter.mjs). The app listens with the
 * owner's trained profile (enroll.js): its keyword (enroll.js KEYWORD, the
 * same line), boost and trigger threshold replace these and TUNING's.
 */
export const KEYWORDS = KEYWORD;

export const TUNING = {
  keywordsScore: 1.5,       // boost for the keyword paths while decoding
  keywordsThreshold: 0.2,   // probability a keyword must reach to fire
  numTrailingBlanks: 1,
  maxActivePaths: 4,
  cooldownMs: 2500,         // one wake per utterance
};

export const SAMPLE_RATE = 16000;
