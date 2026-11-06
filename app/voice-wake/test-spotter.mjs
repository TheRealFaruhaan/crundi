// Check the "Hey Cru" spotter on recorded speech, in plain Node:
//   node app/voice-wake/test-spotter.mjs <installDir> <file.wav>...
// Installs the engine and model into <installDir> first (the same download
// and checks the app does), then reads each 16 kHz mono 16-bit WAV in 100 ms
// pieces, as the microphone would arrive, and prints what fired.
// A file whose name contains "negative" passes when nothing fires; any other
// passes when a keyword does. Exits 1 if any file fails.

import { readFileSync } from 'fs';
import { basename } from 'path';
import { ensureInstalled } from './install.js';
import { createSpotter } from './spotter.js';

const [dir, ...wavs] = process.argv.slice(2);
if (!dir || !wavs.length) { console.error('usage: test-spotter.mjs <installDir> <file.wav>...'); process.exit(2); }

function readWav(file) {
  const b = readFileSync(file);
  let off = 12, fmt = null;
  while (off + 8 <= b.length) {
    const id = b.toString('ascii', off, off + 4), size = b.readUInt32LE(off + 4);
    if (id === 'fmt ') fmt = { ch: b.readUInt16LE(off + 10), rate: b.readUInt32LE(off + 12), bits: b.readUInt16LE(off + 22) };
    if (id === 'data') {
      if (!fmt || fmt.ch !== 1 || fmt.rate !== 16000 || fmt.bits !== 16) throw new Error(file + ': need 16 kHz mono 16-bit');
      const n = Math.floor(size / 2), out = new Float32Array(n);
      for (let i = 0; i < n; i++) out[i] = b.readInt16LE(off + 8 + i * 2) / 32768;
      return out;
    }
    off += 8 + size + (size & 1);
  }
  throw new Error(file + ': no data chunk');
}

const t0 = Date.now();
await ensureInstalled(dir, { log: (s) => console.log(s) });
console.log('installed in', Date.now() - t0, 'ms');
const t1 = Date.now();
const spot = createSpotter(dir);
console.log('spotter ready in', Date.now() - t1, 'ms');

let failed = 0;
for (const f of wavs) {
  const s = readWav(f);
  spot.reset();
  const hits = [];
  const t = Date.now();
  for (let i = 0; i < s.length; i += 1600) {
    const k = spot.accept(s.subarray(i, i + 1600));
    if (k) hits.push(k + '@' + (i / 16000).toFixed(1) + 's');
  }
  // Trailing silence so the last word is decoded.
  const k = spot.accept(new Float32Array(16000));
  if (k) hits.push(k + '@end');
  const ms = Date.now() - t, audio = s.length / 16;
  const neg = /negative/.test(basename(f));
  const pass = neg ? hits.length === 0 : hits.length > 0;
  if (!pass) failed++;
  console.log((pass ? 'PASS ' : 'FAIL ') + basename(f).padEnd(40) + (hits.join(', ') || '-').padEnd(30) + ' ' + (ms / audio * 100).toFixed(1) + '% of real time');
}
spot.free();
process.exit(failed ? 1 : 0);
