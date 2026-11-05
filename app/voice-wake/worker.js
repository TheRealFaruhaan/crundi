/**
 * The keyword spotter's own process (Electron utilityProcess), so decoding
 * never runs on the main process's thread and a crash in the engine takes
 * nothing else with it.
 *
 * From main:  { type: 'init', dir }            load the engine and model
 *             { type: 'pcm', samples }         Float32Array, 16 kHz mono
 *             { type: 'reset' }                forget what was heard
 * To main:    { type: 'ready' } | { type: 'error', message }
 *             { type: 'detected', keyword }
 */

import { createSpotter } from './spotter.js';
import { TUNING } from './manifest.js';

const port = process.parentPort;
let spot = null;
let lastHit = 0;

port.on('message', (e) => {
  const m = e.data || {};
  try {
    if (m.type === 'init') {
      if (!spot) spot = createSpotter(m.dir);
      port.postMessage({ type: 'ready' });
    } else if (m.type === 'pcm' && spot) {
      const s = m.samples instanceof Float32Array ? m.samples : new Float32Array(m.samples || []);
      if (!s.length) return;
      const k = spot.accept(s);
      if (k && Date.now() - lastHit > TUNING.cooldownMs) {
        lastHit = Date.now();
        port.postMessage({ type: 'detected', keyword: k });
      }
    } else if (m.type === 'reset' && spot) {
      spot.reset();
    }
  } catch (err) {
    port.postMessage({ type: 'error', message: String((err && err.message) || err) });
  }
});
