// "Hey Cru" for the owner only, end to end on recordings, the way the PC's
// spotter process runs it (app/voice-wake/worker.js: trainWithVoice,
// createWake, ownerChecked) with the real voice model.
//
//   node scripts/test-voice-wake-voices.mjs [installDir]
//
// installDir (default /tmp/vw-install, or VW_INSTALL_DIR) holds the engine and
// model as install.js leaves them, and speaker/ the voice model (owner.js).
// Recordings: VW_TRAIN_DIR (default /tmp/cru-train: train1..5.wav the owner,
// pos-*, neg-*, other-*) and VW_EVAL_DIR (default /tmp/cru-eval:
// <voice>-train1..5, <voice>-pos-*, <voice>-neg-*). Skips, exit 0, when
// anything is missing. Each voice with five takes is the owner in turn: its
// own calls must all wake, and nobody else's call, nor any hum or noise, may
// wake for alan (the train set's owner); the others are reported.

import { existsSync, readFileSync, readdirSync } from 'fs';
import { join } from 'path';
import { createWake, trainWithVoice, ownerChecked } from '../app/voice-wake/worker.js';
import { createOwnerCheck, speakerDir, validVoice } from '../app/voice-wake/owner.js';
import { ENGINE, MODEL } from '../app/voice-wake/manifest.js';
import { MODEL as VOICE } from '../src/voice/speaker.js';

const dir = process.argv[2] || process.env.VW_INSTALL_DIR || '/tmp/vw-install';
const trainDir = process.env.VW_TRAIN_DIR || '/tmp/cru-train';
const evalDir = process.env.VW_EVAL_DIR || '/tmp/cru-eval';
if (![join(dir, ENGINE.id), join(dir, MODEL.id), join(speakerDir(dir), VOICE.file), join(trainDir, 'train1.wav')].every(existsSync)) {
  console.log('skip: needs the engine, the model and speaker/' + VOICE.file + ' in ' + dir + ', and recordings in ' + trainDir);
  process.exit(0);
}
let failed = 0;
const check = (c, l) => { console.log((c ? 'ok   ' : 'FAIL ') + l); if (!c) failed++; };
function readWav(f) { const b = readFileSync(f); let o = 12; while (o < b.length) { const id = b.toString('ascii', o, o + 4), sz = b.readUInt32LE(o + 4); if (id === 'data') { const n = sz >> 1, a = new Float32Array(n); for (let i = 0; i < n; i++) a[i] = b.readInt16LE(o + 8 + i * 2) / 32768; return a; } o += 8 + sz + (sz & 1); } throw new Error(f + ': no data'); }

// Every file, whose voice it is, and whether it is a call ("Hey Cru", "Hey crew", a take).
const files = [];
for (const f of readdirSync(trainDir).filter((f) => f.endsWith('.wav'))) files.push({ f: join(trainDir, f), voice: /lessac/.test(f) ? 'lessac' : /noise/.test(f) ? 'noise' : 'alan', call: /^(pos-|train\d|other-)|heycrew/.test(f), take: /^train\d/.test(f) });
if (existsSync(evalDir)) for (const f of readdirSync(evalDir).filter((f) => f.endsWith('.wav'))) files.push({ f: join(evalDir, f), voice: f.split('-')[0], call: /-(pos|train\d)/.test(f), take: /-train\d/.test(f) });
const owners = [...new Set(files.map((x) => x.voice))].filter((v) => files.filter((x) => x.voice === v && x.take).length === 5);

let clock = 0;
const owner = createOwnerCheck(dir);
async function run(wake, profile, file) {
  const a = readWav(file), all = new Float32Array(16000 + a.length + 16000);
  all.set(a, 16000); wake.reset(); clock += 60000;
  let woke = null;
  for (let i = 0; i < all.length; i += 1600) {
    clock += 100;
    const r = wake.accept(all.subarray(i, i + 1600));
    if (r) { const m = await ownerChecked(r, profile, owner); if (m.type === 'detected') woke = m; else if (r.ok) wake.refused(); }
  }
  return woke;
}

for (const o of owners) {
  const takes = files.filter((x) => x.voice === o && x.take).sort((a, b) => a.f.localeCompare(b.f)).map((x) => readWav(x.f));
  const t = await trainWithVoice(dir, takes, owner);
  check(t.ok && validVoice(t.profile.voice), o + ': trains, with its voice' + (t.voiceWhy ? ' (' + t.voiceWhy + ')' : ''));
  if (!t.ok) continue;
  const profile = JSON.parse(JSON.stringify(t.profile));
  const wake = createWake(dir, profile, { now: () => clock });
  const tally = { own: [0, 0], others: [0, 0], sounds: [0, 0] }, wrong = [];
  for (const x of files) {
    if (x.voice === o && x.take) continue;
    const kind = x.voice === o ? (x.call ? 'own' : null) : x.call ? 'others' : 'sounds';
    if (!kind) continue;
    const w = await run(wake, profile, x.f);
    tally[kind][1]++; if (w) tally[kind][0]++;
    if (kind === 'own' && !w && !/sentence|slow|after/.test(x.f)) wrong.push('missed ' + x.f.split('/').pop());
    if (kind !== 'own' && w) wrong.push('woke for ' + x.f.split('/').pop() + ' (voice ' + w.voice + ')');
  }
  wake.free();
  console.log('     ' + o.padEnd(9) + 'own calls woke ' + tally.own.join(' of ') + ' | other voices\' calls woke ' + tally.others.join(' of ') + ' | other sounds woke ' + tally.sounds.join(' of '));
  check(!wrong.some((w) => w.startsWith('missed')), o + ': every one of its own calls wakes' + (wrong.filter((w) => w.startsWith('missed')).length ? ': ' + wrong.filter((w) => w.startsWith('missed')).join(', ') : ''));
  check(tally.sounds[0] === 0, o + ': no hum, noise or other speech wakes');
  if (o === 'alan') check(tally.others[0] === 0, 'alan: nobody else\'s "Hey Cru" wakes' + (wrong.length ? ': ' + wrong.join(', ') : ''));
  else if (wrong.length) console.log('     ' + wrong.join('; '));
}
console.log(failed ? failed + ' failed' : 'all passed');
process.exit(failed ? 1 : 0);
