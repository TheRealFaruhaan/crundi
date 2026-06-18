/**
 * scheduler.js — evaluates scheduled tasks and fires their actions.
 *
 * Edge-triggered: a schedule fires once when ALL of its conditions transition
 * from not-met to met, and re-arms when they go not-met again. A short cooldown
 * guards against double-fires. Runs on a ~30s interval.
 */

import { listSchedules, setRuntime } from './schedule-store.js';
import { getLatestStored } from './usage.js';
import { getTask } from './kanban-store.js';
import { getService, startService, stopService } from './services.js';

const TICK_MS = 30 * 1000;
const COOLDOWN_MS = 2 * 60 * 1000;

export function evalCondition(cond, project, deps) {
  try {
    if (cond.type === 'terminals') {
      const list = deps && deps.claudeTerminals ? deps.claudeTerminals.list() : [];
      const k = String(project).toLowerCase();
      const count = list.filter(t => String(t.project).toLowerCase() === k && t.status === 'running').length;
      const v = Number(cond.value);
      if (Number.isNaN(v)) return false;
      if (cond.op === 'below') return count < v;
      if (cond.op === 'above') return count > v;
      if (cond.op === 'equal') return count === v;
      return false;
    }
    if (cond.type === 'usage') {
      // Last stored sample on the server (no forced fetch).
      const u = getLatestStored();
      const src = !u || !u.ok ? null : (cond.metric === 'week' ? u.week : u.fiveHour);
      const util = src ? src.utilization : null;
      if (util == null) return false;
      const v = Number(cond.value);
      if (Number.isNaN(v)) return false;
      if (cond.op === 'below') return util < v;
      if (cond.op === 'above') return util > v;
      if (cond.op === 'equal') return Math.round(util) === Math.round(v);
      return false;
    }
    if (cond.type === 'kanban') {
      if (!cond.taskId) return false;
      // getTask returns an envelope { ok, task } — unwrap it.
      const res = getTask(project, cond.taskId);
      const task = res && res.ok ? res.task : null;
      if (!task) return false;
      if (cond.todoId) {
        const td = (task.todos || []).find(t => t.id === cond.todoId);
        if (!td) return false;
        // For a subtask, "status" is done | pending.
        return cond.status === 'done' ? !!td.done : !td.done;
      }
      return task.status === cond.status;
    }
  } catch { /* ignore */ }
  return false;
}

// Resolve the time gate, migrating any legacy time-type condition to a daily
// recurring `when`.
function effectiveWhen(sch) {
  if (sch.when && sch.when.time) return sch.when;
  const t = (sch.conditions || []).find(c => c.type === 'time');
  if (t && t.at) return { mode: 'recurring', days: [0, 1, 2, 3, 4, 5, 6], time: t.at };
  return null;
}
function sameDay(a, b) {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}
function ymd(d) {
  return d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
}
// "Due" = the current clock minute IS the scheduled minute, and this occurrence
// hasn't already run. Fires ONLY at the specified time — a missed run (app off,
// or conditions unmet during that minute) is NOT caught up later; the user must
// update the time to re-arm it. Matched against the SERVER's local clock (same
// machine for the desktop app).
export function isDue(sch) {
  const when = effectiveWhen(sch);
  if (!when || !when.time) return false;
  const now = new Date();
  const nowHM = String(now.getHours()).padStart(2, '0') + ':' + String(now.getMinutes()).padStart(2, '0');
  if (nowHM !== when.time) return false; // only at the exact scheduled minute
  const lastRun = sch.lastRun ? new Date(sch.lastRun) : null;
  if (when.mode === 'once') {
    if (when.date !== ymd(now)) return false;          // wrong day
    if (lastRun && sameDay(lastRun, now)) return false; // already ran today
    return true;
  }
  // recurring
  const days = Array.isArray(when.days) ? when.days : [];
  if (!days.includes(now.getDay())) return false;
  if (lastRun && sameDay(lastRun, now)) return false;   // already ran today
  return true;
}
function conditionsMet(sch, deps) {
  if (!isDue(sch)) return false; // time gate
  // Extra stacked conditions (time-type ignored — it lives in `when`).
  return (sch.conditions || []).filter(c => c.type !== 'time').every(c => evalCondition(c, sch.project, deps));
}

async function runAction(sch, deps) {
  const a = sch.action || {};
  const label = sch.name || 'Scheduled task';
  if (a.kind === 'agent') {
    if (!deps.claudeTerminals) return;
    await deps.claudeTerminals.create(sch.project, {
      skipPermissions: a.mode === 'skip',
      prompt: a.prompt || '',
      sessionMode: a.session === 'resume' ? 'resume' : a.session === 'continue' ? 'continue' : 'new',
      resumeId: a.sessionId || '',
      model: a.model || '',
      effort: a.effort || '',
      title: label,
    });
  } else if (a.kind === 'command') {
    if (!deps.claudeTerminals || !a.command) return;
    await deps.claudeTerminals.create(sch.project, { command: a.command, title: label });
  } else if (a.kind === 'service') {
    const svc = a.serviceKey && getService(a.serviceKey);
    if (!svc) return;
    const running = svc.status === 'running';
    if (a.op === 'start' && !running) startService(a.serviceKey);
    else if (a.op === 'stop' && running) stopService(a.serviceKey);
    else if (a.op === 'toggle') (running ? stopService : startService)(a.serviceKey);
  }
}

export function startScheduler(deps = {}) {
  let busy = false;
  async function tick() {
    if (busy) return;
    busy = true;
    try {
      for (const sch of listSchedules()) {
        if (!sch.enabled) continue;
        if (!isDue(sch)) continue;                          // not the scheduled minute (or already ran)
        // It's the scheduled minute — check the extra conditions; log any that block it.
        const extra = (sch.conditions || []).filter(c => c.type !== 'time');
        const unmet = extra.filter(c => !evalCondition(c, sch.project, deps));
        if (unmet.length) {
          console.log(`[scheduler] "${sch.name}" is due now but waiting on unmet condition(s): ${unmet.map(c => c.type).join(', ')}`);
          continue;
        }
        const cool = sch.lastRun && (Date.now() - new Date(sch.lastRun).getTime() < COOLDOWN_MS);
        if (cool) continue;                                 // guard rapid double-fire
        let fired = true;
        try { await runAction(sch, deps); } catch (err) { fired = false; console.warn(`[scheduler] "${sch.name}" failed:`, err?.message || err); }
        setRuntime(sch.id, { lastRun: new Date().toISOString() }); // isDue() blocks re-fire for this occurrence
        if (fired) { try { deps.onFire && deps.onFire(sch); } catch { /* non-fatal */ } }
        console.log(`[scheduler] fired "${sch.name}" (${sch.action?.kind}) for "${sch.project}"`);
      }
    } finally { busy = false; }
  }
  const timer = setInterval(tick, TICK_MS);
  setTimeout(tick, 5000); // first pass shortly after boot
  return () => clearInterval(timer);
}
