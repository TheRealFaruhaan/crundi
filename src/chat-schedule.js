/**
 * chat-schedule.js — deferred chat messages.
 *
 * "Write it now, send it when X happens." Distinct from schedule-store.js,
 * which fires project TASKS (spawn an agent, run a command, poke a service) on
 * condition sets. This is narrower and chat-shaped: a queued user turn for a
 * conversation, with one of three triggers.
 *
 *   time        — at a wall-clock moment
 *   turn-end    — when the running turn finishes (or the next one, if idle)
 *   limit-reset — when the rolling 5-hour usage window rolls over
 *
 * Keyed by PROJECT, never by session id. Session ids are minted per process and
 * do not survive a restart, so anything pinned to one is orphaned the moment
 * Crundi reopens — the same trap that stranded chat drafts. The project is the
 * identity that lasts, matching the stored transcript and the draft.
 *
 * Delivery only ever happens into a live, idle session for that project. A
 * scheduled message never spawns Claude by itself: starting an unattended agent
 * on a timer is a much bigger promise than "send this text later", and it is
 * not one the user made here. With nothing running the item simply waits, and
 * says so, rather than firing into the void.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, renameSync } from 'fs';
import { join } from 'path';
import { randomBytes } from 'crypto';
import { config } from './config.js';

const TICK_MS = 15_000;
const MAX_ITEMS = 200;
const MAX_TEXT = 20_000;
export const TRIGGERS = ['time', 'turn-end', 'limit-reset'];

export function createChatSchedule({ claudeUi, getLatestUsage } = {}) {
  const FILE = () => join(config.dataDir, 'chat-schedule.json');
  let items = [];
  let timer = null;
  let listeners = new Set();
  // The window reset we last observed, so a rollover is detectable as a CHANGE
  // rather than by watching the clock pass a boundary we might sleep through.
  let seenReset = null;

  function genId() { return randomBytes(8).toString('hex'); }

  function load() {
    try {
      if (!existsSync(FILE())) return;
      const d = JSON.parse(readFileSync(FILE(), 'utf-8'));
      if (d && Array.isArray(d.items)) items = d.items;
      if (d && d.seenReset) seenReset = d.seenReset;
    } catch { /* start empty rather than crash the server */ }
  }

  function save() {
    try {
      mkdirSync(config.dataDir, { recursive: true });
      const tmp = FILE() + '.tmp';
      writeFileSync(tmp, JSON.stringify({ items, seenReset }));
      renameSync(tmp, FILE());
    } catch { /* best effort */ }
  }

  load();

  function emit() {
    for (const fn of listeners) { try { fn(); } catch { /* ignore */ } }
  }

  /**
   * When this item is expected to fire, as a sortable number — the UI leads with
   * the next one to go off.
   *
   * turn-end sorts to the very front: it fires at the end of the current turn,
   * which is sooner than any clock-based trigger the user would plausibly set.
   */
  function nextFireAt(it, now = Date.now()) {
    if (it.trigger.type === 'time') return Date.parse(it.trigger.at) || 0;
    if (it.trigger.type === 'turn-end') return now;
    if (it.trigger.type === 'limit-reset') {
      const r = currentReset();
      return r ? Date.parse(r) : now + 5 * 3600 * 1000;
    }
    return now;
  }

  function currentReset() {
    try {
      const u = getLatestUsage ? getLatestUsage() : null;
      return (u && u.ok && u.fiveHour && u.fiveHour.resetsAt) || null;
    } catch { return null; }
  }

  function decorate(it) {
    return { ...it, nextFireAt: nextFireAt(it) };
  }

  function list(project) {
    const k = String(project || '').toLowerCase();
    return items
      .filter(it => it.status === 'pending' && (!k || it.project === k))
      .map(decorate)
      .sort((a, b) => a.nextFireAt - b.nextFireAt);
  }

  /** Recently fired/failed items, newest first — so a send is not silent. */
  function recent(project, limit = 10) {
    const k = String(project || '').toLowerCase();
    return items
      .filter(it => it.status !== 'pending' && (!k || it.project === k))
      .sort((a, b) => (b.firedAt || 0) - (a.firedAt || 0))
      .slice(0, limit);
  }

  function add({ project, text, trigger } = {}) {
    const k = String(project || '').toLowerCase();
    if (!k) return { ok: false, error: 'A project is required' };
    const body = String(text || '').trim();
    if (!body) return { ok: false, error: 'The message is empty' };
    const type = trigger && trigger.type;
    if (!TRIGGERS.includes(type)) return { ok: false, error: `Unknown trigger "${type}"` };

    let at = null;
    if (type === 'time') {
      at = Date.parse(trigger.at);
      if (!Number.isFinite(at)) return { ok: false, error: 'That is not a valid time' };
      if (at < Date.now() - 60_000) return { ok: false, error: 'That time has already passed' };
      at = new Date(at).toISOString();
    }

    const it = {
      id: genId(), project: k, text: body.slice(0, MAX_TEXT),
      trigger: type === 'time' ? { type, at } : { type },
      status: 'pending', createdAt: Date.now(), firedAt: 0, error: '',
    };
    load();   // pick up anything the other process added since we last read
    items.push(it);
    if (items.length > MAX_ITEMS) {
      // Drop the oldest settled ones first; pending work is never discarded.
      const settled = items.filter(x => x.status !== 'pending').sort((a, b) => a.createdAt - b.createdAt);
      const cull = new Set(settled.slice(0, items.length - MAX_ITEMS).map(x => x.id));
      items = items.filter(x => !cull.has(x.id));
    }
    save();
    emit();
    return { ok: true, item: decorate(it) };
  }

  function remove(id) {
    load();
    const before = items.length;
    items = items.filter(it => it.id !== id);
    if (items.length === before) return { ok: false, error: 'Not found' };
    save();
    emit();
    return { ok: true };
  }

  /** A live session for this project that can take a turn right now. */
  function idleSessionFor(project) {
    if (!claudeUi) return null;
    const mine = claudeUi.list().filter(s => s.project === project && s.status === 'running');
    return mine.find(s => s.agentState === 'idle') || null;
  }

  function deliver(it) {
    const s = idleSessionFor(it.project);
    if (!s) return false;   // nothing live and free — try again next tick
    const r = claudeUi.sendMessage(s.id, it.text);
    it.firedAt = Date.now();
    if (r && r.ok) {
      it.status = 'sent';
      console.log(`[chat-schedule] sent a ${it.trigger.type} message to "${it.project}"`);
    } else {
      it.status = 'failed';
      it.error = (r && r.error) || 'Send failed';
      console.warn(`[chat-schedule] delivery failed for "${it.project}": ${it.error}`);
    }
    return true;
  }

  /**
   * Re-read before acting. config.dataDir is shared between a released Crundi
   * and a --dev run on the same machine (--dev isolates the port, not the data
   * dir), so two processes tick over ONE file. Without this each holds a stale
   * copy, both see the same item as pending, and it gets delivered twice.
   */
  function refresh() { load(); }

  /** Fire everything whose trigger has come due. */
  function tick() {
    refresh();
    if (!items.some(it => it.status === 'pending')) return;
    const now = Date.now();

    // Detect a window rollover as a change in the reported reset time. Watching
    // for the clock to pass a boundary would miss it whenever the process was
    // asleep or the poll landed on the wrong side of it.
    const reset = currentReset();
    let rolled = false;
    if (reset && seenReset && reset !== seenReset) rolled = true;
    if (reset && reset !== seenReset) { seenReset = reset; save(); }

    let changed = false;
    for (const it of items) {
      if (it.status !== 'pending') continue;
      const t = it.trigger.type;
      let due = false;
      if (t === 'time') due = Date.parse(it.trigger.at) <= now;
      else if (t === 'limit-reset') due = rolled;
      if (!due) continue;
      if (deliver(it)) changed = true;
    }
    if (changed) { save(); emit(); }
  }

  /**
   * A session finished a turn. Anything waiting on turn-end for that project
   * goes now — this is the moment it was waiting for.
   */
  function onTurnEnd(project) {
    refresh();
    const k = String(project || '').toLowerCase();
    let changed = false;
    for (const it of items) {
      if (it.status !== 'pending' || it.project !== k) continue;
      if (it.trigger.type !== 'turn-end') continue;
      if (deliver(it)) changed = true;
      // One per turn end: a burst of queued turns would otherwise all land at
      // once, and the second could not be delivered anyway (the session is busy
      // again the instant the first is sent).
      if (changed) break;
    }
    if (changed) { save(); emit(); }
  }

  function start() {
    if (timer) return;
    timer = setInterval(tick, TICK_MS);
    if (timer.unref) timer.unref();
  }

  function stop() { clearInterval(timer); timer = null; }
  function onChange(fn) { listeners.add(fn); return () => listeners.delete(fn); }

  start();

  return { list, recent, add, remove, onTurnEnd, onChange, tick, stop, TRIGGERS };
}
