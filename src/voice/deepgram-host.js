/**
 * voice/deepgram-host.js — talk to the nearest Deepgram region.
 *
 * Deepgram answers on a US host and an EU one, with the same key, models and
 * voices on both. Which one is near depends on where this server is, so it is
 * measured, not configured: each host is timed and the faster one is used.
 *
 * Why it matters: speech is streamed, and a far host can deliver it slower than
 * it plays. Measured from a server in Germany, the US host took 36-50 s to send
 * 18 s of speech in 4 runs out of 6 (the start played, then it stuttered),
 * while the EU host sent it in 4-9 s every time.
 *
 * Until the first measurement finishes, the US host is used, as before.
 * DEEPGRAM_HOST in the environment pins one and skips measuring.
 */

import { connect as netConnect } from 'net';

export const HOSTS = ['api.deepgram.com', 'api.eu.deepgram.com'];
const DEFAULT = HOSTS[0];
const REMEASURE_MS = 60 * 60 * 1000;
const SAMPLES = 3;

let chosen = '';
let timer = null;

/** The host to use now, e.g. "api.eu.deepgram.com". */
export function deepgramHost() {
  const pinned = String(process.env.DEEPGRAM_HOST || '').trim();
  return pinned || chosen || DEFAULT;
}

/** https://<host><path> */
export const deepgramUrl = (path) => `https://${deepgramHost()}${path}`;
/** wss://<host><path> */
export const deepgramWsUrl = (path) => `wss://${deepgramHost()}${path}`;

/**
 * Best of a few TCP connects to a host, in ms (Infinity when it can't be
 * reached). The connect is one round trip, so it measures distance; an HTTP
 * request would mostly measure how long Deepgram takes to refuse it.
 */
function timeHost(host, connectImpl) {
  const once = () => new Promise((resolve) => {
    const t0 = performance.now();
    const sock = connectImpl({ host, port: 443, timeout: 5000 });
    const done = (ms) => { sock.destroy(); resolve(ms); };
    sock.once('connect', () => done(performance.now() - t0));
    sock.once('timeout', () => done(Infinity));
    sock.once('error', () => done(Infinity));
  });
  return (async () => {
    // The first connect also pays for DNS; best-of drops it.
    let best = Infinity;
    for (let i = 0; i < SAMPLES; i++) best = Math.min(best, await once());
    return best;
  })();
}

/** Time every host and keep the fastest. Returns what was measured. */
export async function measure({ connectImpl = netConnect, log = () => {} } = {}) {
  const times = {};
  for (const h of HOSTS) times[h] = await timeHost(h, connectImpl);
  const fastest = HOSTS.reduce((a, b) => (times[b] < times[a] ? b : a));
  if (Number.isFinite(times[fastest])) {
    if (fastest !== chosen) log(`deepgram: using ${fastest} (${HOSTS.map(h => `${h} ${Number.isFinite(times[h]) ? Math.round(times[h]) + ' ms' : 'unreachable'}`).join(', ')})`);
    chosen = fastest;
  }
  return { chosen: deepgramHost(), times };
}

/** Measure now and every hour after; safe to call more than once. */
export function startHostWatch(opts = {}) {
  if (timer || process.env.DEEPGRAM_HOST) return;
  measure(opts).catch(() => {});
  timer = setInterval(() => { measure(opts).catch(() => {}); }, REMEASURE_MS);
  timer.unref?.();
}

/** Tests only. */
export function _reset() { chosen = ''; if (timer) clearInterval(timer); timer = null; }
