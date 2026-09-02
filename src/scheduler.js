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
/**
 * Wall-clock fields for an instant, in a named zone.
 *
 * "09:00" means nine in the morning WHERE THE USER IS. This used to be matched
 * against the server's local clock, which was the same machine back when Crundi
 * was only a desktop app. On a server in another country it is not: a schedule
 * written in Male (UTC+5) against a host on CEST fires three hours out, and a
 * date or weekday near midnight lands on the wrong day entirely.
 *
 * Falls back to the host's own zone when a schedule carries none, which is
 * every schedule written before this existed - same behaviour as before for
 * them, rather than silently moving when they fire.
 */
const DOW = { Sun: 0, Mon: 1, Tue: 2, Wed: 3, Thu: 4, Fri: 5, Sat: 6 };

function clockIn(tz, date = new Date()) {
  if (!tz) {
    return {
      hm: String(date.getHours()).padStart(2, '0') + ':' + String(date.getMinutes()).padStart(2, '0'),
      ymd: date.getFullYear() + '-' + String(date.getMonth() + 1).padStart(2, '0') + '-' + String(date.getDate()).padStart(2, '0'),
      dow: date.getDay(),
    };
  }
  try {
    const parts = new Intl.DateTimeFormat('en-US', {
      timeZone: tz,
      // h23 rather than hour12:false: the latter reports midnight as "24" in
      // some runtimes, which would never match a stored "00:00".
      hourCycle: 'h23',
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', weekday: 'short',
    }).formatToParts(date).reduce((acc, p) => { acc[p.type] = p.value; return acc; }, {});
    return {
      hm: parts.hour + ':' + parts.minute,
      ymd: parts.year + '-' + parts.month + '-' + parts.day,
      dow: DOW[parts.weekday] ?? 0,
    };
  } catch {
    // An unknown zone must not stop schedules running altogether.
    return clockIn(null, date);
  }
}
// "Due" = the current clock minute IS the scheduled minute, and this occurrence
// hasn't already run. Fires ONLY at the specified time — a missed run (app off,
// or conditions unmet during that minute) is NOT caught up later; the user must
// update the time to re-arm it. Matched against the SERVER's local clock (same
// machine for the desktop app).
export function isDue(sch) {
  const when = effectiveWhen(sch);
  if (!when || !when.time) return false;
  // The zone the schedule was written in, not the host's.
  const tz = sch.tz || when.tz || null;
  const now = clockIn(tz);
  if (now.hm !== when.time) return false; // only at the exact scheduled minute
  // "Already ran today" has to be asked in the same zone, or a run late in the
  // user's evening looks like a different day to the server and repeats.
  const ranToday = sch.lastRun ? clockIn(tz, new Date(sch.lastRun)).ymd === now.ymd : false;
  if (when.mode === 'once') {
    if (when.date !== now.ymd) return false;   // wrong day
    if (ranToday) return false;
    return true;
  }
  // recurring
  const days = Array.isArray(when.days) ? when.days : [];
  if (!days.includes(now.dow)) return false;
  if (ranToday) return false;
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
  } else if (a.kind === 'chat') {
    // A chat that runs unattended and tidies itself up. Unlike 'agent', which
    // opens a terminal panel and leaves it there, this one closes when the work
    // is genuinely done — and stays open when it is not.
    if (!deps.runScheduledChat) return;
    await deps.runScheduledChat(sch);
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
