/**
 * factory-journal.js — a run as a fold over its events. Pure.
 *
 * Every input that changes a run is an event, appended to the run's journal
 * (events.jsonl) and flushed BEFORE it takes effect. The run is whatever
 * reduce() makes of those events, in order. run.json is only a cache of the
 * result: if it is missing, damaged, or behind the journal (the process died
 * between the two writes), the run is rebuilt by replaying the journal.
 *
 * This works because everything reduce() calls is deterministic: the engine
 * (factory-engine.js) is pure, ids come from counters, and time comes only
 * from each event's own `at`, never from the clock.
 *
 * ─── Effects ───
 *
 * Engine events produce effects (start someone, make a workspace, merge…).
 * They are given ids and kept in `pending` until an `effect-done` event says
 * they happened — with their result in the same event (the folder a workspace
 * got, which branches merged), so an effect and its outcome are recorded
 * atomically. A run rebuilt after a crash therefore knows exactly what was left.
 *
 * ─── What is NOT here ───
 *
 * Which chat process belongs to whom, who is busy right now, the permission
 * requests in flight: all of that dies with the process and is rebuilt on
 * recovery, so the runner keeps it in memory only. The conversation ids that
 * let a session be resumed ARE here (`session` events).
 *
 * Spec: specs/factory.md §4.7.
 */

import * as Engine from './factory-engine.js';
import { MANAGER_ID } from './factory-model.js';

export const JOURNAL_VERSION = 1;
const LOG_MAX = 500;

const clone = (o) => (typeof structuredClone === 'function' ? structuredClone(o) : JSON.parse(JSON.stringify(o)));
const ms = (iso) => { const t = Date.parse(iso || ''); return Number.isFinite(t) ? t : 0; };

/** Statuses in which the run still holds its project. */
export const UNFINISHED = new Set(['starting', 'running', 'waiting', 'paused', 'finishing', 'crashed']);

function emptyRecord() {
  return {
    v: JOURNAL_VERSION,
    events: 0,
    state: null,            // the engine's run
    run: {
      factoryId: '', factoryVersion: 0, mergeMode: 'ask', managerMode: 'normal',
      repo: '', root: '', worktree: '', branch: '', parentBranch: '', baseCommit: '', dirty: false,
      skillExcludes: [], missingSkills: [], setup: null,
      ws: {},               // wsId -> { path, branch, parent, shared?, orphan? }
      sessions: {},         // employeeId -> { sessionUuid, chatId }
      retired: {},          // employeeId -> true while stopped by the collector
      notices: {},          // tokenId -> text for that token's next step
      rules: [],
      waits: {},            // tokenId -> { kind, nodeId, ws, startedAt, deadline, timeoutAt, key }
      perms: { seq: 0, outstanding: [], log: [] },
      capExtra: 0, capMinutesExtra: 0, capHit: null, paused: null,
      crash: null,          // { reason, at, from }
      time: { activeMs: 0, since: null, maxMinutes: 0 },
      startedAt: '', stoppedAt: '',
      effectSeq: 0,
      pending: [],          // effects not yet done, each with an `id`
      // The end: { card, how: 'merged'|'kept'|'discarded'|'undone', commit, before, after, at }
      result: { card: '', how: '', commit: '', before: '', after: '', message: '', at: '' },
      mergeAttempts: 0,     // automatic merges tried this run (auto mode)
    },
  };
}

/** Rebuild a run from all its events. */
export function replay(events) {
  let rec = null;
  for (const ev of events) rec = reduce(rec, ev);
  return rec;
}

/**
 * Apply one event. Never mutates its input. An event that does not apply
 * (a handoff the engine refuses) leaves the run as it was: the runner checks
 * before writing, so in a healthy journal that never happens, and a replay
 * stays robust if it does.
 */
export function reduce(prev, ev) {
  const rec = prev ? clone(prev) : emptyRecord();
  const r = rec.run;
  rec.events++;
  const at = ev.at || '';
  switch (ev.type) {
    case 'start': {
      rec.state = Engine.createRun({ id: ev.id, project: ev.project, factory: ev.factory, task: ev.task, now: at });
      Object.assign(r, {
        factoryId: ev.factoryId || '', factoryVersion: ev.factoryVersion || 0,
        mergeMode: ev.mergeMode || rec.state.snapshot.settings.mergeMode,
        managerMode: rec.state.snapshot.settings.managerMode,
        repo: ev.wt.repo, root: ev.wt.root, worktree: ev.wt.path, branch: ev.wt.branch,
        parentBranch: ev.wt.parentBranch, baseCommit: ev.wt.baseCommit, dirty: !!ev.wt.dirty,
        skillExcludes: ev.skillExcludes || [], missingSkills: ev.missingSkills || [],
        cr: ev.cr || null,
        trigger: ev.trigger || null,
        startedAt: at,
      });
      r.ws.main = { path: ev.wt.path, branch: ev.wt.branch };
      r.time.maxMinutes = rec.state.snapshot.settings.caps.maxMinutes;
      // The team's permissions: presets and the project's own scripts, granted from the start.
      r.rules = Array.isArray(ev.rules) ? ev.rules.slice() : [];
      r.teamMode = rec.state.snapshot.settings.teamMode || 'ask';
      r.policy = '';
      break;
    }
    case 'setup':
      r.setup = { status: ev.status, command: ev.command, output: ev.output || '' };
      if (ev.status === 'failed') r.notices.t0 = `The factory's setup command failed (exit ${ev.code}):\n$ ${ev.command}\n${ev.output || ''}`;
      break;
    case 'begin': {
      const s = Engine.start(rec.state);
      if (!s.ok) { rec.state.status = 'stopped'; r.stoppedAt = at; break; }
      rec.state = s.run;
      queue(rec, s.effects);
      r.time.since = at;
      break;
    }
    case 'handoff': {
      const caps = { maxHandoffs: rec.state.snapshot.settings.caps.maxHandoffs + r.capExtra };
      const res = Engine.handoff(rec.state, ev.tokenId, ev.payload, { treeHash: ev.treeHash || '', caps });
      if (!res.ok) break;
      rec.state = res.run;
      queue(rec, res.effects, at);
      if (ev.by && r.sessions[ev.by] && ev.sessionUuid) r.sessions[ev.by].sessionUuid = ev.sessionUuid;
      // The skills loaded during that step, and the commit it ended on, ride
      // on its history entry (the next step is told what changed since).
      const h = [...rec.state.history].reverse().find(x => x.type === 'handoff' || x.type === 'delegate-done' || x.type === 'delegate-escalated');
      if (h) {
        if (Array.isArray(ev.skills) && ev.skills.length) h.skills = ev.skills.slice(0, 20);
        if (ev.commit) h.commit = ev.commit;
        if (!h.by && ev.by) h.by = ev.by;
      }
      break;
    }
    case 'delegate': {
      const caps = { maxHandoffs: rec.state.snapshot.settings.caps.maxHandoffs + r.capExtra };
      const res = Engine.delegate(rec.state, ev.tokenId, ev.employeeId, ev.task, { caps });
      if (!res.ok) break;
      rec.state = res.run;
      queue(rec, res.effects, at);
      break;
    }
    case 'team-mode':
      r.teamMode = ev.mode === 'bypass' ? 'bypass' : 'ask';
      break;
    case 'policy':
      r.policy = String(ev.text || '').slice(0, 4000);
      break;
    case 'ask-reply': {
      const res = Engine.askReply(rec.state, ev.tokenId, ev.replyId, ev.note || '');
      if (!res.ok) break;
      rec.state = res.run;
      queue(rec, res.effects, at);
      break;
    }
    case 'wait-done': {
      const res = Engine.waitDone(rec.state, ev.tokenId, ev.outcome, ev.detail || '');
      if (r.waits) delete r.waits[ev.tokenId];
      if (!res.ok) break;
      rec.state = res.run;
      queue(rec, res.effects, at);
      break;
    }
    case 'escalate': {
      const res = Engine.escalate(rec.state, ev.tokenId, ev.reason || '');
      if (!res.ok) break;
      rec.state = res.run;
      queue(rec, res.effects, at);
      break;
    }
    case 'effect-done': {
      const ef = r.pending.find(e => e.id === ev.effectId);
      r.pending = r.pending.filter(e => e.id !== ev.effectId);
      if (!ef) break;
      const res = ev.result || {};
      if (ef.type === 'wait' && res.timeoutAt) {
        if (!r.waits) r.waits = {};
        r.waits[ef.tokenId] = res;
      }
      if (ef.type === 'workspace') {
        r.ws[ef.wsId] = res.path
          ? { path: res.path, branch: res.branch || '', parent: ef.parentWs, ...(res.shared ? { shared: true } : {}) }
          : { ...(r.ws[ef.parentWs] || r.ws.main), parent: ef.parentWs, shared: true };
      }
      if (ef.type === 'merge') {
        for (const w of res.merged || []) delete r.ws[w];
        for (const c of res.conflicts || []) if (r.ws[c.ws]) r.ws[c.ws].orphan = true;
        if ((res.conflicts || []).length) {
          if (ef.abort) r.notices[ef.contTokenId] = [r.notices[ef.contTokenId], res.reason].filter(Boolean).join('\n');
          // The continuation must not go on as if merged; the runner escalates it next.
          else r.pending = r.pending.filter(p => p.tokenId !== ef.contTokenId);
        }
      }
      if (ef.type === 'finish') closeClock(r, at);
      break;
    }
    case 'notice':
      r.notices[ev.tokenId] = [r.notices[ev.tokenId], ev.text].filter(Boolean).join('\n');
      break;
    case 'notice-clear':
      delete r.notices[ev.tokenId];
      break;
    case 'session':
      delete r.retired[ev.employeeId];
      r.sessions[ev.employeeId] = { sessionUuid: ev.sessionUuid || r.sessions[ev.employeeId]?.sessionUuid || '', chatId: ev.chatId || r.sessions[ev.employeeId]?.chatId || '' };
      break;
    case 'retire':
      r.retired[ev.employeeId] = true;
      if (ev.sessionUuid) r.sessions[ev.employeeId] = { ...(r.sessions[ev.employeeId] || {}), sessionUuid: ev.sessionUuid };
      history(rec, { type: 'retire', employeeId: ev.employeeId });
      break;
    case 'rule': {
      // Rules are per employee: who is the employee it was granted for, '*' for everyone.
      const who = ev.who || '*';
      r.rules = (r.rules || []).map(x => typeof x === 'string' ? { rule: x, who: '*' } : x);
      if (!r.rules.some(x => x.rule === ev.rule && x.who === who)) r.rules.push({ rule: ev.rule, who });
      break;
    }
    case 'manager-mode':
      r.managerMode = ev.mode === 'bypass' ? 'bypass' : 'normal';
      break;
    case 'perm-asked':
      r.perms.seq = Math.max(r.perms.seq, Number(String(ev.id).slice(1)) || 0);
      r.perms.outstanding.push({ id: ev.id, employeeId: ev.employeeId, employee: ev.employee, toolName: ev.toolName, input: ev.input, at });
      break;
    case 'perm-decided':
      r.perms.outstanding = r.perms.outstanding.filter(p => p.id !== ev.id);
      r.perms.log.push({ at, id: ev.id, employee: ev.employee, toolName: ev.toolName, input: ev.input, decision: ev.decision, by: ev.by, reason: ev.reason || '', rule: ev.rule || undefined });
      if (r.perms.log.length > LOG_MAX) r.perms.log.splice(0, r.perms.log.length - LOG_MAX);
      break;
    case 'cap':
      r.capHit = { which: ev.which, limit: ev.limit, at };
      if (ev.which === 'maxMinutes') closeClock(r, at);
      break;
    case 'pause':
      r.paused = { by: ev.by || 'person', at, ...(ev.resetsAt ? { resetsAt: ev.resetsAt } : {}) };
      closeClock(r, at);
      break;
    case 'resume':
      if (r.capHit?.which === 'maxHandoffs') r.capExtra += rec.state.snapshot.settings.caps.maxHandoffs;
      if (r.capHit?.which === 'maxMinutes') r.capMinutesExtra += rec.state.snapshot.settings.caps.maxMinutes;
      r.capHit = null;
      r.paused = null;
      rec.state.status = 'running';
      openClock(r, at);
      break;
    case 'reopen': {
      const res = Engine.reopen(rec.state, ev.reason || '', { asMessage: ev.asMessage !== false });
      if (!res.ok) break;
      rec.state = res.run;
      queue(rec, res.effects, at);
      if (ev.auto) r.mergeAttempts++;
      openClock(r, at);
      break;
    }
    case 'card':
      r.result.card = ev.entryId || '';
      break;
    case 'merged':
    case 'kept':
    case 'discarded': {
      const how = ev.type;
      r.result = { ...r.result, how, commit: ev.commit || '', before: ev.before || '', after: ev.after || '', message: ev.message || '', at };
      rec.state.status = 'finished';
      rec.state.outcome = { ...(rec.state.outcome || {}), how, ...(ev.commit ? { commit: ev.commit } : {}) };
      r.perms.outstanding = [];
      closeClock(r, at);
      break;
    }
    case 'undone':
      r.result = { ...r.result, how: 'undone', at };
      rec.state.outcome = { ...(rec.state.outcome || {}), how: 'undone' };
      break;
    case 'stop':
      rec.state.status = 'stopped';
      r.stoppedAt = at;
      r.perms.outstanding = [];
      closeClock(r, at);
      break;
    case 'crash':
      r.crash = { reason: ev.reason || 'it was interrupted', at, from: rec.state.status === 'crashed' ? (r.crash?.from || 'running') : rec.state.status };
      rec.state.status = 'crashed';
      // The time it actually died is the last thing it did, not now.
      closeClock(r, ev.diedAt || at);
      break;
    case 'recover': {
      const from = r.crash?.from || 'running';
      rec.state.status = ['running', 'waiting', 'paused', 'finishing'].includes(from) ? from : 'running';
      if (rec.state.status !== 'paused') r.paused = null;
      // Branch folders that vanished: those branches carry on in main.
      for (const w of ev.missingWs || []) if (r.ws[w]) r.ws[w] = { ...r.ws.main, parent: r.ws[w].parent, shared: true };
      if (ev.skillExcludes) r.skillExcludes = [...new Set([...r.skillExcludes, ...ev.skillExcludes])];
      // Activations are rebuilt from the tokens; whatever else was pending stays.
      r.pending = r.pending.filter(e => e.type !== 'activate');
      r.perms.outstanding = [];
      r.crash = null;
      history(rec, { type: 'recover', reason: ev.reason || '', note: ev.note || '' });
      if (rec.state.status !== 'paused' && rec.state.status !== 'finishing') openClock(r, at);
      break;
    }
    default:
      break;
  }
  applyHolds(rec);
  if (rec.state) rec.state.updatedAt = at || rec.state.updatedAt;
  return rec;
}

function queue(rec, effects, at) {
  const r = rec.run;
  for (const e of effects) {
    if (e.type === 'cap') {
      r.capHit = { which: e.which, limit: e.limit, at };
      continue;
    }
    r.pending.push({ ...e, id: 'e' + (++r.effectSeq) });
  }
}

function history(rec, entry) {
  if (!rec.state) return;
  rec.state.history.push({ seq: ++rec.state.seq, ...entry });
}

function openClock(r, at) { if (!r.time.since) r.time.since = at; }
function closeClock(r, at) {
  if (r.time.since) { r.time.activeMs += Math.max(0, ms(at) - ms(r.time.since)); r.time.since = null; }
}

/** The engine works out status from its tokens; holds win over it. */
function applyHolds(rec) {
  const s = rec.state;
  if (!s) return;
  const r = rec.run;
  if (s.status === 'crashed' || s.status === 'stopped' || s.status === 'finished') return;
  if (r.paused && s.status !== 'finishing') s.status = 'paused';
  else if (r.capHit && s.status === 'running') s.status = 'waiting';
}

// ─── Derived views ───

/** Questions waiting on the person (Ask me), derived from the tokens. */
export function asksOf(rec) {
  if (!rec?.state) return [];
  const nodes = new Map(rec.state.snapshot.nodes.map(n => [n.id, n]));
  return rec.state.tokens.filter(t => t.status === 'ask').map(t => {
    const n = nodes.get(t.nodeId);
    return { tokenId: t.id, nodeId: t.nodeId, prompt: n?.ask?.prompt || '', replies: n?.ask?.replies || [] };
  });
}

/** Milliseconds of active run time left before maxMinutes, at `nowMs`. */
export function timeLeft(rec, nowMs) {
  const t = rec.run.time;
  const limit = (t.maxMinutes + rec.run.capMinutesExtra) * 60_000;
  const used = t.activeMs + (t.since ? Math.max(0, nowMs - ms(t.since)) : 0);
  return limit - used;
}

/** Is the run holding new steps back (a cap, or paused)? */
export function isHeld(rec) { return !!(rec.run.capHit || rec.run.paused); }

export { MANAGER_ID };
