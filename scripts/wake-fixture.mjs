// Numbers from app/voice-wake/enroll.js for the phone's WakeProfileTest, so
// the PC and the phone train and match "Hey Cru" the same way.
//   node scripts/wake-fixture.mjs   (writes android/app/src/test/resources/wake-fixture.json)
import { writeFileSync } from 'fs';
import { pathToFileURL } from 'url';
import * as E from '../app/voice-wake/enroll.js';

/** A made-up utterance: two voiced syllables with a little noise, from a fixed seed. */
export function signal(seed, pitch = 140, stretch = 1) {
  let x = seed >>> 0;
  const rnd = () => { x = (Math.imul(x, 1664525) + 1013904223) >>> 0; return x / 4294967296 - 0.5; };
  const n = Math.round(16000 * 1.6 * stretch), out = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    const t = i / 16000, u = t / stretch;
    const env = Math.exp(-(((u - 0.45) / 0.12) ** 2)) + 0.9 * Math.exp(-(((u - 0.95) / 0.16) ** 2));
    const f0 = pitch * (1 + 0.08 * Math.sin(2 * Math.PI * 1.5 * u));
    let v = 0;
    for (let h = 1; h <= 8; h++) v += Math.sin(2 * Math.PI * f0 * h * t) / h * (u < 0.7 ? (h === 2 || h === 5 ? 1.6 : 1) : (h === 3 || h === 7 ? 1.6 : 1));
    out[i] = 0.12 * env * v + 0.003 * rnd();
  }
  return out;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) write();

function write() {
const a = signal(1), b = signal(2, 150, 1.1), c = signal(3, 220, 0.8);
const fa = E.mfcc(a), ta = E.trim(a);
const fixture = {
  note: 'Made by scripts/wake-fixture.mjs from app/voice-wake/enroll.js; checked by WakeProfileTest.',
  signal: { seeds: [1, 2, 3], pitches: [140, 150, 220], stretches: [1, 1.1, 0.8] },
  frames: fa.length,
  mfccFirst: Array.from(fa[10]),
  mfccMiddle: Array.from(fa[Math.floor(fa.length / 2)]),
  trimmed: ta ? ta.length : -1,
  distanceAB: E.distance(fa, E.mfcc(b)),
  distanceAC: E.distance(fa, E.mfcc(c)),
  distanceAA: E.distance(fa, fa),
};
writeFileSync(new URL('../android/app/src/test/resources/wake-fixture.json', import.meta.url), JSON.stringify(fixture, null, 1) + '\n');
console.log({ frames: fixture.frames, trimmed: fixture.trimmed, AB: fixture.distanceAB, AC: fixture.distanceAC, AA: fixture.distanceAA });
}
