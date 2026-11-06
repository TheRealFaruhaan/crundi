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

const LOG_MAX_BYTES = 4 * 1024 * 1024;

export function createVoiceBrain({ api, cli, settings = voiceSettings, logFile = '' } = {}) {
  const which = () => (settings.getSettings().brain === 'claude-code' && cli ? 'claude-code' : 'api');
  const pick = () => (which() === 'claude-code' ? cli : api);

  function log(r, { report = false, said = '' } = {}) {
    if (!logFile || !r) return;
    const s = settings.getSettings();
    const line = {
      at: new Date().toISOString(),
      brain: r.brain || which(),
      model: s.model,
      ok: !!r.ok,
      report,
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

  return {
    get which() { return which(); },
    turn: (text, opts = {}) => timed(() => pick().turn(text, opts), { said: text }),
    report: (note, opts = {}) => timed(() => pick().report(note, opts), { report: true, said: note }),
    /** The owner cancelled what they said: whichever brain is answering stops. */
    cancel: () => {
      const a = api && api.cancel ? api.cancel() : { cancelled: false, acted: [] };
      const c = cli && cli.cancel ? cli.cancel() : { cancelled: false, acted: [] };
      return { ok: true, cancelled: !!(a.cancelled || c.cancelled), acted: [...(a.acted || []), ...(c.acted || [])] };
    },
    warm: (opts) => pick().warm(opts),
    compact: (opts) => pick().compact(opts),
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
