#!/usr/bin/env node
// The frame harness (ARCHITECTURE.md, "The frame budget"). The app run with
// ZED_MEASUREMENTS=1 prints one line per drawn frame on stderr, gpui's own
// ("frame duration: 3.412ms"). This reads them as they come, stamps each
// with its arrival, and sums up any stretch of time:
//
//   import { FrameMeter } from '../tools/frames.mjs';
//   const m = new FrameMeter(app.stderr);       // the app's stderr (a stream)
//   const t = m.mark(); …; const s = m.since(t); // { frames, p50, p95, max, share7, idle }
//
// or, over a saved log:  node native-v2/tools/frames.mjs <stderr log>
//
// share7 is the share of frames at most 7 ms (the budget: 95%); `frames` in
// a stretch where nothing changes is the idle count (the budget: 0).

import { readFileSync } from 'node:fs';

const LINE = /frame duration: ([0-9.]+)(ns|µs|us|ms|s)\b/;
const UNIT = { ns: 1e-6, 'µs': 1e-3, us: 1e-3, ms: 1, s: 1000 };

/** A frame's duration in ms from one stderr line, or null. */
export function parse(line) {
  const m = LINE.exec(line);
  return m ? Number(m[1]) * UNIT[m[2]] : null;
}

/** p50, p95, max and the share at most 7 ms of a list of durations (ms). */
export function summary(ms) {
  const s = [...ms].sort((a, b) => a - b);
  const q = (p) => (s.length ? s[Math.min(s.length - 1, Math.ceil(p * s.length) - 1)] : 0);
  const r = (x) => Math.round(x * 100) / 100;
  return { frames: s.length, p50: r(q(0.5)), p95: r(q(0.95)), max: r(s.at(-1) ?? 0), share7: s.length ? r(s.filter((x) => x <= 7).length / s.length) : 1 };
}

export class FrameMeter {
  /** @param stream the app's stderr; `echo` gets every other line (the log). */
  constructor(stream, echo = () => {}) {
    this.frames = []; // [arrival ms, duration ms]
    let buf = '';
    stream.on('data', (d) => {
      buf += d;
      let i;
      while ((i = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, i); buf = buf.slice(i + 1);
        const ms = parse(line);
        if (ms === null) echo(line + '\n'); else this.frames.push([Date.now(), ms]);
      }
    });
  }
  /** Now, to measure from. */
  mark() { return Date.now(); }
  /** The frames drawn since `t` (and before `until`). */
  since(t, until = Infinity) { return summary(this.frames.filter(([at]) => at >= t && at < until).map(([, ms]) => ms)); }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const f = process.argv[2];
  if (!f) { console.error('usage: frames.mjs <stderr log>'); process.exit(2); }
  const ms = readFileSync(f, 'utf8').split('\n').map(parse).filter((x) => x !== null);
  console.log(JSON.stringify(summary(ms)));
}
