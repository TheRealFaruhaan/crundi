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

/**
 * One turn of Cru's at a time, in order (12 Nov 2026).
 *
 * What happened: the owner asked something, and while Cru answered, a chat
 * it watches finished its turn. The report waited (polling, 30 s at most)
 * for Cru to be free and was then dropped as "busy", and the owner's own
 * turns polled for Cru too, racing the reports for the moment it came free.
 * (On the voice screen the report, queued behind the answer, also left the
 * screen stuck thinking: crundi-voice.js nextReport.)
 *
 * So every turn (the owner's, a report, a wake-up, a compaction) goes through
 * one queue:
 *  - nothing is refused for being busy: it waits for the turn before it;
 *  - the owner's turns go ahead of reports and wake-ups still waiting (his
 *    answer is never held up by news), and in order among themselves;
 *  - [front] goes next of all (a compaction Cru asked for in its turn);
 *  - one that waits longer than its [waitMs] is answered BUSY_ERROR, never
 *    started (a turn that never ends must not hold everything for ever);
 *  - cancel() drops the owner's turns still waiting (answered cancelled),
 *    and says whether the one running is his.
 */
export function createTurnQueue({ setTimer = setTimeout, clearTimer = clearTimeout } = {}) {
  const waiting = [];
  let running = null;

  function pump() {
    if (running || !waiting.length) return;
    const job = waiting.shift();
    if (job.timer) clearTimer(job.timer);
    running = job;
    let p;
    try { p = Promise.resolve(job.fn()); } catch (e) { p = Promise.reject(e); }
    p.then(job.ok, job.fail).finally(() => { running = null; pump(); });
  }

  return {
    /** [fn] runs when its turn comes; resolves with what it resolves with. */
    run(fn, { owner = false, front = false, waitMs = 0 } = {}) {
      return new Promise((ok, fail) => {
        const job = { fn, ok, fail, owner, front, timer: null };
        if (front) waiting.unshift(job);
        else if (owner) { const i = waiting.findIndex((j) => !j.owner && !j.front); if (i < 0) waiting.push(job); else waiting.splice(i, 0, job); }
        else waiting.push(job);
        // Started at once (pump below), the timer is cleared there.
        if (waitMs > 0) {
          job.timer = setTimer(() => {
            const i = waiting.indexOf(job);
            if (i >= 0) { waiting.splice(i, 1); ok({ ok: false, error: BUSY_ERROR }); }
          }, waitMs);
          job.timer?.unref?.();
        }
        pump();
      });
    },
    /** The owner's waiting turns are dropped; { dropped, ownerRunning }. */
    cancel() {
      let dropped = 0;
      for (let i = waiting.length - 1; i >= 0; i--) {
        const j = waiting[i];
        if (!j.owner) continue;
        waiting.splice(i, 1);
        if (j.timer) clearTimer(j.timer);
        j.ok({ ok: false, cancelled: true, acted: [] });
        dropped++;
      }
      return { dropped, ownerRunning: !!(running && running.owner), running: !!running };
    },
    get busy() { return !!running; },
    get waiting() { return waiting.length; },
  };
}
