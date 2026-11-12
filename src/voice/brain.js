/**
 * voice/brain.js — which brain answers for Cru (the "brain" setting): the
 * Claude API (agent.js) or the owner's Claude Code in JSON mode
 * (agent-cli.js). Both have the same interface; this picks one per request
 * and logs every turn, so the two can be compared on speed, cost and cache
 * use (brain-log.jsonl, stats()).
 *
 * Each brain keeps its own conversation; memory_search and the notebook are
 * shared, so switching loses nothing that was said.
 */

import { appendFileSync, existsSync, readFileSync, statSync, renameSync } from 'node:fs';
import * as voiceSettings from './settings.js';
import { createTurnQueue, TURN_WAIT_MS, BUSY_ERROR } from './turns.js';

const LOG_MAX_BYTES = 4 * 1024 * 1024;

/** How long a report or wake-up may wait behind other turns before it is given up as busy (the caller keeps it). */
export const REPORT_WAIT_MS = 10 * 60 * 1000;

export function createVoiceBrain({ api, cli, settings = voiceSettings, logFile = '', ownerWaitMs = TURN_WAIT_MS, reportWaitMs = REPORT_WAIT_MS } = {}) {
  const which = () => (settings.getSettings().brain === 'claude-code' && cli ? 'claude-code' : 'api');
  const pick = () => (which() === 'claude-code' ? cli : api);

  function log(r, { report = false, said = '', device = '' } = {}) {
    if (!logFile || !r) return;
    const s = settings.getSettings();
    const line = {
      at: new Date().toISOString(),
      brain: r.brain || which(),
      model: s.model,
      ok: !!r.ok,
      report,
      // Where it was said (voice/device.js), failed and cancelled turns too.
      ...(device ? { device } : {}),
      ms: r.ms || 0,
      apiMs: r.apiMs || 0,
      rounds: r.rounds || 0,
      tools: r.tools || [],
      costUsd: r.costUsd || 0,
      cache: r.cache || null,
      outputTokens: r.outputTokens || 0,
      saidChars: String(said).length,
      spokeChars: String(r.speak || '').length,
      error: r.ok ? undefined : String(r.error || '').slice(0, 200),
    };
    try {
      if (existsSync(logFile) && statSync(logFile).size > LOG_MAX_BYTES) renameSync(logFile, logFile + '.1');
      appendFileSync(logFile, JSON.stringify(line) + '\n', { mode: 0o600 });
    } catch { /* the log is best effort */ }
  }

  async function timed(fn, meta) {
    const brain = which();
    const t0 = Date.now();
    const r = await fn();
    if (r && typeof r === 'object') { r.brain = r.brain || brain; r.ms = r.ms || (Date.now() - t0); }
    log(r, meta);
    return r;
  }

  // Asked for by Cru during a turn (compact_conversation, start_fresh): done
  // once the turn is over, never under it, and without holding up the reply.
  let next = null;
  function afterTurn(r) {
    const n = next;
    next = null;
    if (!n) return r;
    const b = pick();
    // Next in the queue, ahead of anything waiting: what comes after starts on the new conversation.
    queue.run(() => (n.kind === 'fresh' && b.fresh ? b.fresh(n.summary) : b.compact({ force: true })), { front: true }).catch(() => {});
    return r;
  }

  // One turn at a time (turns.js createTurnQueue): a report or a wake-up that
  // comes while the owner is answered waits for his turn to end, and his turns
  // go ahead of news still waiting. Neither is refused for being busy.
  const queue = createTurnQueue();

  return {
    get which() { return which(); },
    turn: (text, opts = {}) => queue.run(() => timed(() => pick().turn(text, opts), { said: text, device: opts.device || '' }).then(afterTurn), { owner: true, waitMs: ownerWaitMs }),
    report: (note, opts = {}) => queue.run(() => timed(() => pick().report(note, opts), { report: true, said: note }).then(afterTurn), { waitMs: reportWaitMs })
      .then((r) => (r && r.ok === false && r.error === BUSY_ERROR ? { ...r, error: 'busy' } : r)),
    /** Cru's own wish: 'compact' (summarise everything) or 'fresh' (only [summary] goes on). */
    requestAfterTurn(kind, summary = '') { next = { kind: kind === 'fresh' ? 'fresh' : 'compact', summary: String(summary || '').slice(0, 4000) }; },
    /** The owner cancelled what they said: whichever brain is answering stops. */
    // His turns still waiting in the queue go first; the one running is stopped
    // only when it is his (a report being said is not what he took back), or
    // when [all] (kid mode starting: whatever runs stops).
    cancel: ({ all = false } = {}) => {
      const q = queue.cancel();
      if (!all && q.running && !q.ownerRunning) return { ok: true, cancelled: q.dropped > 0, acted: [] };
      const a = api && api.cancel ? api.cancel() : { cancelled: false, acted: [] };
      const c = cli && cli.cancel ? cli.cancel() : { cancelled: false, acted: [] };
      return { ok: true, cancelled: !!(q.dropped || a.cancelled || c.cancelled), acted: [...(a.acted || []), ...(c.acted || [])] };
    },
    warm: (opts) => pick().warm(opts),
    // In the queue too, so it never takes the brain from under a turn about to start. Idle compaction skips a busy queue.
    compact: (opts = {}) => (!opts.force && (queue.busy || queue.waiting) ? Promise.resolve({ ok: false, skipped: 'busy' }) : queue.run(() => pick().compact(opts))),
    /** Tests and diagnostics: turns running and waiting. */
    get queue() { return { busy: queue.busy, waiting: queue.waiting }; },
    reset: () => pick().reset(),
    get lastActivity() { return pick().lastActivity; },
    /** Per brain and model, from the log: how many turns, how fast, what each cost. */
    stats({ since = 0 } = {}) {
      let lines = [];
      try { lines = readFileSync(logFile, 'utf-8').split('\n').filter(Boolean).map(l => JSON.parse(l)); } catch { /* none yet */ }
      const groups = new Map();
      for (const l of lines) {
        if (Date.parse(l.at) < since) continue;
        const k = `${l.brain} · ${l.model}`;
        const g = groups.get(k) || { brain: l.brain, model: l.model, turns: 0, failed: 0, ms: [], costUsd: 0, cacheRead: 0, cacheWritten: 0, uncached: 0, tools: 0 };
        if (!l.ok) { g.failed++; groups.set(k, g); continue; }
        g.turns++; g.ms.push(l.ms); g.costUsd += l.costUsd || 0; g.tools += (l.tools || []).length;
        g.cacheRead += l.cache?.read || 0; g.cacheWritten += l.cache?.written || 0; g.uncached += l.cache?.uncached || 0;
        groups.set(k, g);
      }
      return [...groups.values()].map((g) => {
        const ms = g.ms.sort((a, b) => a - b);
        const mid = ms.length ? ms[Math.floor(ms.length / 2)] : 0;
        const input = g.cacheRead + g.cacheWritten + g.uncached;
        return {
          brain: g.brain, model: g.model, turns: g.turns, failed: g.failed,
          medianMs: mid, p90Ms: ms.length ? ms[Math.min(ms.length - 1, Math.floor(ms.length * 0.9))] : 0,
          costUsd: Math.round(g.costUsd * 10000) / 10000,
          costPerTurnUsd: g.turns ? Math.round((g.costUsd / g.turns) * 100000) / 100000 : 0,
          cacheReadShare: input ? Math.round((g.cacheRead / input) * 100) / 100 : 0,
          toolsPerTurn: g.turns ? Math.round((g.tools / g.turns) * 10) / 10 : 0,
        };
      });
    },
  };
}
