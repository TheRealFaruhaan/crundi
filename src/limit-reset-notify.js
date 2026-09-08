/**
 * limit-reset-notify.js — tell the user when a usage window has reset.
 *
 * ─── What counts as a reset ───
 *
 * The usage API reports a `resetsAt` per window. A reset is that timestamp
 * PASSING, not a new one appearing — and the difference matters for the 5-hour
 * window, which is rolling: it only starts on your first message, so after it
 * lapses there may be no next window at all until you send something. Waiting
 * for a new `resetsAt` would mean the "5 hours are up" message arrives when you
 * next use Claude, which is the one moment you no longer need telling.
 *
 * ─── Not announcing stale news ───
 *
 * If Crundi is off for three days, the window it was watching lapsed long ago.
 * Firing on startup would be technically true and useless: the notification
 * says "your tokens just refreshed" about something that happened on Tuesday.
 * So an elapsed window is only announced if it elapsed RECENTLY; otherwise the
 * baseline is quietly re-taken and nothing is said.
 *
 * The same rule handles first-ever startup, where there is no baseline to
 * compare against and therefore nothing that can honestly be called a reset.
 *
 * ─── Firing once ───
 *
 * The watched timestamp is persisted, so a restart between the window lapsing
 * and the next tick cannot produce a second notification for the same reset.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, renameSync } from 'fs';
import { join } from 'path';
import { config } from './config.js';
import { pickMessage, rememberPick } from './limit-reset-messages.js';

/** How late an elapsed window may be noticed and still be worth announcing. */
export const STALE_MS = 30 * 60 * 1000;
/** How often to look. Matches the usage cache TTL, so it adds no extra fetches. */
export const TICK_MS = 60 * 1000;

const WINDOWS = [
  { kind: 'five', field: 'fiveHour', label: '5-hour' },
  { kind: 'week', field: 'week', label: 'weekly' },
];

/**
 * @param getUsage  () => Promise<usage>  the same shape usage.getUsage returns
 * @param notify    (text, meta) => void  called once per detected reset
 * @param enabled   () => boolean         skip the poll entirely when nothing
 *                                        would be delivered anyway
 */
export function createLimitResetNotifier({
  getUsage,
  notify,
  enabled = () => true,
  // How long to hold a five-hour announcement waiting for a line the warmer's
  // ping is generating. Zero when nothing is going to offer one.
  graceMs = () => 0,
  offerTtlMs = 10 * 60 * 1000,
  now = () => Date.now(),
  staleMs = STALE_MS,
  rand = Math.random,
  stateFile = () => join(config.dataDir, 'limit-reset.json'),
} = {}) {
  // kind -> { watching: ISO|null, due: number|null, recent: number[] }
  let state = {
    five: { watching: null, due: null, recent: [] },
    week: { watching: null, due: null, recent: [] },
  };
  let loaded = false;
  // A line Claude wrote, waiting to be used instead of one from the list. Not
  // persisted: a message generated before a restart is stale by definition.
  const offers = new Map();

  function load() {
    if (loaded) return state;
    loaded = true;
    try {
      const f = stateFile();
      if (existsSync(f)) {
        const d = JSON.parse(readFileSync(f, 'utf-8'));
        for (const { kind } of WINDOWS) {
          const s = d && d[kind];
          if (!s) continue;
          if (typeof s.watching === 'string') state[kind].watching = s.watching;
          if (Number.isFinite(s.due)) state[kind].due = s.due;
          if (Array.isArray(s.recent)) state[kind].recent = s.recent.filter(Number.isInteger);
        }
      }
    } catch { /* defaults are fine — worst case one baseline is retaken */ }
    return state;
  }

  function save() {
    try {
      mkdirSync(config.dataDir, { recursive: true });
      const f = stateFile();
      const tmp = f + '.tmp';
      writeFileSync(tmp, JSON.stringify(state, null, 2));
      renameSync(tmp, f);
    } catch { /* non-fatal: at worst a reset is announced twice */ }
  }

  /**
   * Advance one window. Pure decision-making around the two side effects, so
   * the rules can be tested without a clock or a usage API.
   * @returns 'fired' | 'baselined' | 'stale' | 'waiting' | 'idle'
   */
  function step(kind, seenIso, t) {
    const s = state[kind];
    const seenAt = seenIso ? Date.parse(seenIso) : NaN;
    const seenOpen = Number.isFinite(seenAt) && seenAt > t;

    if (s.watching) {
      const watchedAt = Date.parse(s.watching);
      if (Number.isFinite(watchedAt) && watchedAt <= t) {
        const lateBy = t - watchedAt;
        // Adopt the next window (if one has opened) before deciding whether to
        // speak, so an early return can never leave the old one armed.
        s.watching = seenOpen ? seenIso : null;
        if (lateBy > staleMs) { save(); return 'stale'; }
        // Owed, not necessarily said yet — the warmer may be mid-way through
        // writing a better line than anything in the list.
        s.due = t;
        save();
      }
      // Still open. If the API now reports a different future reset for this
      // window, follow it — the old anchor would otherwise never elapse.
      else if (seenOpen && seenIso !== s.watching) { s.watching = seenIso; save(); return 'baselined'; }
    }

    if (s.due != null) {
      const offer = takeOffer(kind, t);
      const grace = Number(graceMs(kind)) || 0;
      // Wait a little for a written line, but never silently: once the grace is
      // up the list speaks. An announcement that never arrives because the
      // generator hung is worse than a familiar one.
      if (!offer && t - s.due < grace) return 'deferred';
      let text = offer;
      if (!text) {
        const p = pickMessage(kind, s.recent, rand);
        text = p.text;
        s.recent = rememberPick(s.recent, p.index);
      }
      s.due = null;
      save();
      try { notify(text, { kind, window: kind, generated: !!offer }); } catch { /* delivery is not ours to fix */ }
      return offer ? 'fired-generated' : 'fired';
    }

    if (!s.watching && seenOpen) { s.watching = seenIso; save(); return 'baselined'; }
    return s.watching ? 'waiting' : 'idle';
  }

  /** Take a generated line if one is waiting and still fresh. */
  function takeOffer(kind, t) {
    const o = offers.get(kind);
    if (!o) return '';
    offers.delete(kind);
    return (t - o.at) <= offerTtlMs ? o.text : '';
  }

  /** Hand over a line Claude wrote, to be used instead of one from the list. */
  function offer(kind, text, at) {
    const t = String(text || '').trim();
    if (!t || !state[kind]) return false;
    offers.set(kind, { text: t, at: Number.isFinite(at) ? at : now() });
    return true;
  }

  async function tick() {
    if (!enabled()) return {};
    load();
    let usage;
    try { usage = await getUsage(); } catch { return {}; }
    if (!usage || !usage.ok) return {};
    const t = now();
    const out = {};
    for (const { kind, field } of WINDOWS) {
      const w = usage[field];
      out[kind] = step(kind, w && w.resetsAt ? w.resetsAt : null, t);
    }
    return out;
  }

  let timer = null;
  function start() {
    if (timer) return;
    // The first tick only takes a baseline: there is nothing to compare against
    // yet, and the staleness rule keeps a long downtime quiet regardless.
    tick().catch(() => {});
    timer = setInterval(() => { tick().catch(() => {}); }, TICK_MS);
    if (timer.unref) timer.unref();
  }
  function stop() { if (timer) { clearInterval(timer); timer = null; } }

  return { tick, start, stop, step, offer, _state: () => (load(), state) };
}
