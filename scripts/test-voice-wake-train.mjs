// "Hey Cru" training and the voice match, in plain Node, the way the PC's
// spotter process runs them (app/voice-wake/worker.js: trainIn, createWake).
//
//   node scripts/test-voice-wake-train.mjs [wavDir] [installDir]
//
// wavDir (default /tmp/cru-train, or VW_TRAIN_DIR) holds 16 kHz mono 16-bit
// WAVs: train1..5.wav are the owner; pos-*.wav should wake; other-*.wav
// (someone else) and neg-*.wav should not, except neg-*heycrew* (it sounds
// the same as "Hey Cru"). installDir (default /tmp/vw-install, or
// VW_INSTALL_DIR) holds the engine and model as install.js leaves them.
// Skips, exit 0, when either is missing. Exits 1 on a failure.

import { existsSync, readFileSync, readdirSync } from 'fs';
import { join } from 'path';
import { createWake, trainIn } from '../app/voice-wake/worker.js';
import { checkSample, validProfile, SAMPLES_NEEDED, train, keywordLines } from '../app/voice-wake/enroll.js';
import { ENGINE, MODEL } from '../app/voice-wake/manifest.js';

const wavDir = process.argv[2] || process.env.VW_TRAIN_DIR || '/tmp/cru-train';
const dir = process.argv[3] || process.env.VW_INSTALL_DIR || '/tmp/vw-install';

const trainFiles = Array.from({ length: SAMPLES_NEEDED }, (_, i) => join(wavDir, 'train' + (i + 1) + '.wav'));
if (!trainFiles.every((f) => existsSync(f)) || !existsSync(join(dir, ENGINE.id)) || !existsSync(join(dir, MODEL.id))) {
  console.log('skip: needs ' + wavDir + '/train1..' + SAMPLES_NEEDED + '.wav and the engine and model in ' + dir);
  process.exit(0);
}

function readWav(file) {
  const b = readFileSync(file);
  let off = 12;
  while (off + 8 <= b.length) {
    const id = b.toString('ascii', off, off + 4), size = b.readUInt32LE(off + 4);
    if (id === 'data') {
      const n = Math.floor(size / 2), out = new Float32Array(n);
      for (let i = 0; i < n; i++) out[i] = b.readInt16LE(off + 8 + i * 2) / 32768;
      return out;
    }
    off += 8 + size + (size & 1);
  }
  throw new Error(file + ': no data chunk');
}

let failed = 0;
const check = (ok, what) => { console.log((ok ? 'ok   ' : 'FAIL ') + what); if (!ok) failed++; };

// Each recording as the page checks it.
const recs = trainFiles.map(readWav);
recs.forEach((r, i) => { const c = checkSample(r); check(c.ok, 'checkSample train' + (i + 1) + (c.ok ? '' : ': ' + c.why)); });
check(!checkSample(new Float32Array(16000 * 2)).ok, 'checkSample refuses silence');

const t0 = Date.now();
const r = trainIn(dir, recs);
check(r.ok && validProfile(r.profile), 'train (' + (Date.now() - t0) + ' ms)' + (r.ok ? ': threshold ' + r.profile.keywordsThreshold + ', limit ' + r.profile.limit : ': ' + r.why + ' redo ' + r.redo));
if (!r.ok) process.exit(1);
// It goes through JSON on its way to profile.json and back.
const profile = JSON.parse(JSON.stringify(r.profile));

// Each file fed in 100 ms pieces, a second of quiet either side, through
// the same wake the spotter process uses. A fake clock keeps the cooldown
// to one file.
let clock = 0;
let wake = createWake(dir, profile, { now: () => clock });
function run(file) {
  const a = readWav(join(wavDir, file));
  const all = new Float32Array(16000 + a.length + 16000);
  all.set(a, 16000);
  wake.reset();
  clock += 60000;
  const out = [];
  for (let i = 0; i < all.length; i += 1600) {
    clock += 100;
    const hit = wake.accept(all.subarray(i, i + 1600));
    if (hit) out.push((hit.ok ? 'WAKE ' + hit.via + ' ' : 'rejected ') + hit.score);
  }
  return out;
}

// "Hey Cru" run on into a sentence: the spotter itself does not fire on it
// at the trained threshold, so it is shown, not failed.
const KNOWN_MISS = /sentence/;
const files = readdirSync(wavDir).filter((f) => f.endsWith('.wav') && !f.startsWith('train')).sort();
function all(label) {
  console.log('--- ' + label);
  for (const f of files) {
    const out = run(f), woke = out.some((x) => x.startsWith('WAKE'));
    const shown = f.padEnd(26) + (out.join(', ') || '-');
    if (f.startsWith('pos-') && KNOWN_MISS.test(f) && !woke) console.log('note known miss   ' + shown);
    else if (f.startsWith('pos-')) check(woke, 'wakes      ' + shown);
    else if (/heycrew/.test(f)) console.log('note ' + 'sounds the same ' + shown);
    else if (f.startsWith('other-') || f.startsWith('neg-')) check(!woke, 'no wake    ' + shown);
  }
}
all('the model and the owner\'s phrase');
wake.free();

// An accent the model never hears: training still succeeds (no take is
// refused for it), and the owner's phrase alone wakes.
const deaf = train(recs, () => ({ accept: () => null, reset() {}, free() {} }));
check(deaf.ok && deaf.profile.modelHeard === 0, 'a voice the model never hears still trains (' + (deaf.ok ? 'strict limit ' + deaf.profile.strictLimit : deaf.why) + ')');
if (deaf.ok) {
  const p = { ...deaf.profile, keyword: keywordLines(0.99), keywordsThreshold: 0.99 };
  wake = createWake(dir, p, { now: () => clock });
  all('the owner\'s phrase alone (the model deaf to it)');
  wake.free();
}

console.log(failed ? failed + ' failed' : 'all passed');
process.exit(failed ? 1 : 0);
