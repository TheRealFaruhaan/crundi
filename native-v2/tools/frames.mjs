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

/** p50, p95, max, the share at most 7 ms and the frames over 7 and 16 ms of a list of durations (ms). */
export function summary(ms) {
  const s = [...ms].sort((a, b) => a - b);
  const q = (p) => (s.length ? s[Math.min(s.length - 1, Math.ceil(p * s.length) - 1)] : 0);
  const r = (x) => Math.round(x * 100) / 100;
  return { frames: s.length, p50: r(q(0.5)), p95: r(q(0.95)), max: r(s.at(-1) ?? 0), share7: s.length ? r(s.filter((x) => x <= 7).length / s.length) : 1, over7: s.filter((x) => x > 7).length, over16: s.filter((x) => x > 16).length };
}

/** The journal's lines summed up: each draw phase's p50 / p95 (µs), the
 * present's, and the views whose own work took 0.5 ms or more (count, total
 * ms, by phase and name). */
export function phases(lines) {
  const q = (a, p) => { const s = [...a].sort((x, y) => x - y); return s.length ? s[Math.min(s.length - 1, Math.ceil(p * s.length) - 1)] : 0; };
  const draws = lines.filter((f) => f[0] === 'draw').map((f) => f.slice(1).map(Number));
  const pres = lines.filter((f) => f[0] === 'present').map((f) => Number(f[1]));
  const col = (i) => draws.map((d) => d[i]);
  const views = {};
  for (const f of lines.filter((f) => f[0] === 'view')) { const k = `${f[1]} ${f.slice(3).join(' ')}`; const v = views[k] ||= { n: 0, ms: 0 }; v.n++; v.ms += Number(f[2]) / 1000; }
  for (const v of Object.values(views)) v.ms = Math.round(v.ms);
  const pq = (a) => [q(a, 0.5), q(a, 0.95)];
  return { draws: draws.length, layout: pq(col(0)), prepaint: pq(col(1)), paint: pq(col(2)), draw: pq(col(3)), present: pq(pres), views };
}

export class FrameMeter {
  /** @param stream the app's stderr; `echo` gets every other line (the log). */
  constructor(stream, echo = () => {}) {
    this.frames = []; // [arrival ms, duration ms]
    // The frame journal's lines (a `--features frame-journal` build with
    // CRUNDI_FRAME_JOURNAL=1: crates/app/src/journal.rs), [arrival, fields].
    this.journal = [];
    let buf = '';
    stream.on('data', (d) => {
      buf += d;
      let i;
      while ((i = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, i); buf = buf.slice(i + 1);
        const ms = parse(line);
        if (ms !== null) this.frames.push([Date.now(), ms]);
        else if (line.startsWith('fj ')) this.journal.push([Date.now(), line.slice(3).split(' ')]);
        else echo(line + '\n');
      }
    });
  }
  /** Now, to measure from. */
  mark() { return Date.now(); }
  /** The frames drawn since `t` (and before `until`). */
  since(t, until = Infinity) {
    const s = summary(this.frames.filter(([at]) => at >= t && at < until).map(([, ms]) => ms));
    // The journal arrives up to 100 ms late: its lines are taken from t to until + 150 ms.
    const j = this.journal.filter(([at]) => at >= t + 150 && at < (until === Infinity ? Infinity : until + 150)).map(([, f]) => f);
    if (j.length) s.journal = phases(j);
    return s;
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const f = process.argv[2];
  if (!f) { console.error('usage: frames.mjs <stderr log>'); process.exit(2); }
  const ms = readFileSync(f, 'utf8').split('\n').map(parse).filter((x) => x !== null);
  console.log(JSON.stringify(summary(ms)));
}
