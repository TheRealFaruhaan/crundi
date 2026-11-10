/**
 * voice/turns.js — the owner's turns on /api/voice/turn, made safe to send
 * again (10 Nov 2026).
 *
 * What happened: a typed "Yes" started a 69-second turn. The phone's request
 * came again 13 s later (the connection dropped, the HTTP client resent it),
 * and the copy was refused with "Still answering the last request". The
 * phone took that refusal as the turn's answer: it went back to Ready with
 * the warning, and the "Yes" it had sent never appeared, while the first copy
 * went on and did what was asked.
 *
 * So:
 *  - a turn may carry a requestId. The same id again (a resend, a retry)
 *    gets the same turn's result, waited for if it is still running, never a
 *    second run and never "busy";
 *  - a new turn that finds Cru busy (another of the owner's turns, a wake-up,
 *    a report) waits for it to finish, up to [waitMs], instead of failing.
 *    Busy is checked before anything is done, so asking again is safe.
 */

export const BUSY_ERROR = 'Still answering the last request';
export const TURN_WAIT_MS = 3 * 60 * 1000;
const KEEP_MS = 10 * 60 * 1000;
const MAX_KEPT = 50;

export function createTurnGate({ now = () => Date.now(), sleep = (ms) => new Promise((ok) => setTimeout(ok, ms)), waitMs = TURN_WAIT_MS, pollMs = 400 } = {}) {
  const seen = new Map();   // requestId -> { at, promise }

  const prune = () => {
    const t = now();
    for (const [id, e] of seen) if (t - e.at > KEEP_MS) seen.delete(id);
    while (seen.size > MAX_KEPT) seen.delete(seen.keys().next().value);
  };

  /** [fn]() runs the turn; refused as busy, it is asked again until Cru is free (or [waitMs] has passed). */
  async function whenFree(fn) {
    const until = now() + waitMs;
    for (;;) {
      const r = await fn();
      if (!(r && r.ok === false && r.error === BUSY_ERROR) || now() >= until) return r;
      await sleep(pollMs);
    }
  }

  return {
    /** One of the owner's turns: [requestId] (optional) makes a resend get the same result. */
    run(requestId, fn) {
      prune();
      const id = String(requestId || '').trim().slice(0, 100);
      if (id && seen.has(id)) return seen.get(id).promise.then((r) => (r && typeof r === 'object' ? { ...r, repeated: true } : r));
      const promise = whenFree(fn);
      if (id) seen.set(id, { at: now(), promise });
      return promise;
    },
    get size() { return seen.size; },
  };
}
