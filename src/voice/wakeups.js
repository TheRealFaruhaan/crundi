/**
 * voice/wakeups.js — Cru's own tickers and triggers: built-in, read-only
 * reasons for Cru to wake up without the owner saying anything.
 *
 *   - A ticker repeats on a schedule (every N minutes, or daily at HH:MM on
 *     some days), for a purpose ("a morning briefing").
 *   - A trigger fires when a condition is met: one of a fixed set of kinds,
 *     each with typed params, optionally only inside a time window. A one-off
 *     time is a trigger too (kind 'at').
 *
 * Nothing here runs a process or a shell command, and nothing changes
 * anything: a wake-up only hands Cru a note (report()), and Cru decides
 * whether to stay quiet, say something, or alert the owner. Every one is off
 * until switched on, by the owner or by Cru itself.
 *
 * Limits, so a flapping condition cannot burn the API budget: at most
 * MAX_PER_HOUR wake-ups in any rolling hour (all of them together), a
 * cooldown per trigger (DEFAULT_COOLDOWN_MIN unless set), none while the
 * month's spend limit is reached, and one at a time (a wake-up that finds
 * Cru busy is tried again on the next scan, a few times).
 *
 * Stored in <dataDir>/voice-wakeups.json, private to this user.
 *
 * REST (capability 'voice-wakeups'; see webapp.js):
 *   GET  /api/voice/wakeups  -> { ok, tickers: [Item], triggers: [Item], kinds, limits }
 *   POST /api/voice/wakeups  { id, action: 'enable'|'disable'|'delete' } -> the same shape
 *   SSE event 'voice-wakeups' {} when any of them changes.
 */

import { readFileSync, existsSync, readdirSync } from 'node:fs';
import { execFile } from 'node:child_process';
import { basename } from 'node:path';
import os from 'node:os';
import { randomBytes } from 'node:crypto';
import { writePrivateFile } from '../private-fs.js';

export const MIN_EVERY_MIN = 5;
export const MAX_EVERY_MIN = 7 * 24 * 60;
export const MAX_PER_HOUR = 12;
export const DEFAULT_COOLDOWN_MIN = 30;
export const MIN_COOLDOWN_MIN = 5;
export const MAX_TICKERS = 20;
export const MAX_TRIGGERS = 30;
/** A daily ticker missed by more than this (the server was down) waits for the next day. */
export const DAILY_GRACE_MS = 30 * 60 * 1000;
/** A one-off time missed by more than this is dropped rather than said late. */
export const AT_GRACE_MS = 12 * 60 * 60 * 1000;
/** How often a wake-up that found Cru busy is tried again. */
export const BUSY_RETRIES = 3;
const HOUR = 60 * 60 * 1000;
const DAY_NAMES = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];

/** The condition kinds, all built-in and read-only, with their params. */
export const CONDITION_KINDS = {
  cpu_above: { params: { percent: 'number', forMinutes: 'number' }, about: 'server CPU above percent for forMinutes' },
  memory_above: { params: { percent: 'number', forMinutes: 'number' }, about: 'server memory use above percent for forMinutes' },
  chat_opened: { params: { project: 'string', match: 'string' }, about: 'a chat is opened' },
  chat_closed: { params: { project: 'string', match: 'string' }, about: 'a chat is closed' },
  chat_finished: { params: { project: 'string', match: 'string' }, about: 'a chat finishes its turn' },
  chat_waiting: { params: { project: 'string', match: 'string' }, about: 'a chat waits on the owner (a question or a permission)' },
  service_stopped: { params: { service: 'string' }, about: 'a background service stops or crashes' },
  container_unhealthy: { params: { container: 'string' }, about: 'a Docker container turns unhealthy' },
  process_started: { params: { name: 'string' }, about: 'a process with this name starts' },
  process_stopped: { params: { name: 'string' }, about: 'the last process with this name stops' },
  collaborator_action: { params: { collaborator: 'string' }, about: 'a collaborator opens a chat, finishes a turn or closes a chat' },
  at: { params: { datetime: 'string' }, about: 'a one-off time' },
};
export const KINDS = Object.keys(CONDITION_KINDS);

// ─── time helpers (server local time) ───

/** "07:30" -> minutes after midnight, or null. */
export function parseHm(s) {
  const m = /^(\d{1,2}):(\d{2})$/.exec(String(s || '').trim());
  if (!m) return null;
  const h = Number(m[1]), mi = Number(m[2]);
  if (h > 23 || mi > 59) return null;
  return h * 60 + mi;
}
const hm = (mins) => `${String(Math.floor(mins / 60)).padStart(2, '0')}:${String(mins % 60).padStart(2, '0')}`;

function cleanDays(days) {
  if (!Array.isArray(days)) return [];
  const names = ['sun', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat'];
  const out = new Set();
  for (const d of days) {
    const n = typeof d === 'number' ? d : names.indexOf(String(d).slice(0, 3).toLowerCase());
    if (Number.isInteger(n) && n >= 0 && n <= 6) out.add(n);
  }
  return [...out].sort();
}

/** A window { from, to, days }: from > to wraps past midnight; no from/to is all day; no days is every day. */
export function inWindow(win, date = new Date()) {
  if (!win) return true;
  const mins = date.getHours() * 60 + date.getMinutes();
  const from = parseHm(win.from), to = parseHm(win.to);
  let day = date.getDay();
  if (from != null && to != null && from > to && mins < to) day = (day + 6) % 7; // the night belongs to the day it began
  if (win.days && win.days.length && !win.days.includes(day)) return false;
  if (from == null && to == null) return true;
  if (from != null && to != null) return from <= to ? (mins >= from && mins < to) : (mins >= from || mins < to);
  if (from != null) return mins >= from;
  return mins < to;
}

function cleanWindow(i = {}) {
  const from = i.from ? parseHm(i.from) : null, to = i.to ? parseHm(i.to) : null;
  if (i.from && from == null) throw new Error('from must be a time like 07:00');
  if (i.to && to == null) throw new Error('to must be a time like 22:00');
  const days = cleanDays(i.days);
  if (from == null && to == null && !days.length) return null;
  return { ...(from != null ? { from: hm(from) } : {}), ...(to != null ? { to: hm(to) } : {}), ...(days.length ? { days } : {}) };
}

function windowText(w) {
  if (!w) return '';
  const days = w.days && w.days.length ? (w.days.length === 5 && [1, 2, 3, 4, 5].every(d => w.days.includes(d)) ? 'on weekdays' : w.days.length === 2 && w.days.includes(0) && w.days.includes(6) ? 'at weekends' : 'on ' + w.days.map(d => DAY_NAMES[d].slice(0, 3)).join(', ')) : '';
  const hours = w.from && w.to ? `between ${w.from} and ${w.to}` : w.from ? `after ${w.from}` : w.to ? `before ${w.to}` : '';
  return [hours, days].filter(Boolean).join(' ');
}

/** "2026-11-10T07:30" (local) or a full ISO time -> ms, or NaN. */
export function parseAt(s) {
  const t = String(s || '').trim();
  if (!t) return NaN;
  const m = /^(\d{4})-(\d{2})-(\d{2})[T ](\d{1,2}):(\d{2})$/.exec(t);
  if (m) return new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), Number(m[4]), Number(m[5])).getTime();
  return Date.parse(t);
}

/** A ticker's next due moment after [from] given its schedule, for its summary and for due(). */
export function tickerDue(t, date = new Date()) {
  if (!t.enabled) return false;
  const now = date.getTime();
  const since = Math.max(t.lastFiredAt || 0, t.enabledAt || 0);
  if (!inWindow(t.window, date)) return false;
  if (t.every) return now - since >= t.every * 60 * 1000;
  const at = parseHm(t.at);
  if (at == null) return false;
  if (t.days && t.days.length && !t.days.includes(date.getDay())) return false;
  const today = new Date(date.getFullYear(), date.getMonth(), date.getDate(), Math.floor(at / 60), at % 60).getTime();
  return now >= today && now - today <= DAILY_GRACE_MS && since < today;
}

export function describe(item) {
  if (item.type === 'ticker') {
    const sched = item.every
      ? `every ${item.every >= 60 && item.every % 60 === 0 ? (item.every / 60) + (item.every === 60 ? ' hour' : ' hours') : item.every + ' minutes'}`
      : `daily at ${item.at}${item.days && item.days.length ? ' ' + windowText({ days: item.days }) : ''}`;
    return [sched, windowText(item.window)].filter(Boolean).join(', ');
  }
  const c = item.condition || {};
  let s;
  switch (c.kind) {
    case 'cpu_above': s = `CPU above ${c.percent}%${c.forMinutes ? ` for ${c.forMinutes} min` : ''}`; break;
    case 'memory_above': s = `memory above ${c.percent}%${c.forMinutes ? ` for ${c.forMinutes} min` : ''}`; break;
    case 'chat_opened': case 'chat_closed': case 'chat_finished': case 'chat_waiting': {
      const verb = { chat_opened: 'is opened', chat_closed: 'is closed', chat_finished: 'finishes', chat_waiting: 'waits on Sir' }[c.kind];
      s = `${c.match ? `chat "${c.match}"` : 'a chat'}${c.project ? ` in ${c.project}` : ''} ${verb}`; break;
    }
    case 'service_stopped': s = `${c.service ? `service "${c.service}"` : 'a service'} stops`; break;
    case 'container_unhealthy': s = `${c.container ? `container "${c.container}"` : 'a container'} turns unhealthy`; break;
    case 'process_started': s = `process "${c.name}" starts`; break;
    case 'process_stopped': s = `process "${c.name}" stops`; break;
    case 'collaborator_action': s = `${c.collaborator ? `collaborator "${c.collaborator}"` : 'a collaborator'} does something`; break;
    case 'at': s = `once at ${String(c.datetime || '').replace('T', ' ')}`; break;
    default: s = String(c.kind || 'unknown');
  }
  const extra = [windowText(item.window), item.once || c.kind === 'at' ? '' : `at most every ${item.cooldown || DEFAULT_COOLDOWN_MIN} min`, item.once && c.kind !== 'at' ? 'once' : ''].filter(Boolean);
  return [s, ...extra].join(', ');
}

// ─── the store ───

const clip = (s, n) => String(s ?? '').replace(/\s+/g, ' ').trim().slice(0, n);

function cleanCondition(c = {}) {
  const kind = String(c.kind || '');
  if (!CONDITION_KINDS[kind]) throw new Error(`Unknown condition kind "${kind}". One of: ${KINDS.join(', ')}`);
  const out = { kind };
  for (const [k, type] of Object.entries(CONDITION_KINDS[kind].params)) {
    if (c[k] === undefined || c[k] === null || c[k] === '') continue;
    out[k] = type === 'number' ? Number(c[k]) : clip(c[k], 120);
  }
  if (kind === 'cpu_above' || kind === 'memory_above') {
    if (!(out.percent > 0 && out.percent < 100)) throw new Error('percent must be between 1 and 99');
    out.forMinutes = Math.max(0, Math.min(120, Math.round(out.forMinutes ?? 5)));
  }
  if ((kind === 'process_started' || kind === 'process_stopped') && !out.name) throw new Error('A process trigger needs the process name');
  if (kind === 'at') {
    const t = parseAt(out.datetime);
    if (!Number.isFinite(t)) throw new Error('datetime must be like 2026-11-10T07:30 (server local time)');
  }
  return out;
}

function cleanTickerSchedule(i, was = {}) {
  const out = {};
  const every = i.every !== undefined && i.every !== null && i.every !== '' ? Number(i.every) : (i.at ? null : was.every);
  const at = i.at !== undefined && i.at !== null && i.at !== '' ? String(i.at) : (i.every ? null : was.at);
  if (every) {
    if (!Number.isFinite(every) || every < MIN_EVERY_MIN) throw new Error(`every must be at least ${MIN_EVERY_MIN} minutes`);
    out.every = Math.min(MAX_EVERY_MIN, Math.round(every));
    out.at = null; out.days = [];
  } else if (at) {
    const m = parseHm(at);
    if (m == null) throw new Error('at must be a time like 07:30');
    out.at = hm(m); out.every = null;
    out.days = i.days !== undefined ? cleanDays(i.days) : (was.days || []);
  } else throw new Error('A ticker needs every (minutes) or at (HH:MM)');
  return out;
}

export function createWakeupStore({ file = '', now = () => Date.now(), onChange = () => {}, builtins = [] } = {}) {
  let data = { tickers: [], triggers: [] };
  try { if (file && existsSync(file)) { const d = JSON.parse(readFileSync(file, 'utf-8')); data = { tickers: d.tickers || [], triggers: d.triggers || [] }; } } catch { /* start empty */ }
  // Built-in tickers (the heartbeat): always there, off until switched on, never deleted.
  for (const b of builtins) {
    if (data.tickers.some(t => t.id === b.id)) continue;
    data.tickers.unshift({
      id: b.id, type: 'ticker', builtin: true, name: b.name, purpose: b.purpose, ...cleanTickerSchedule(b), window: cleanWindow({ from: b.from, to: b.to, days: b.windowDays }),
      enabled: false, enabledAt: 0, createdBy: 'owner', createdAt: now(), lastFiredAt: 0, fires: 0,
    });
  }
  const save = () => { if (file) { try { writePrivateFile(file, JSON.stringify(data, null, 2)); } catch (e) { console.warn('[voice] could not save the wake-ups:', e.message); } } onChange(); };
  const newId = (p) => `${p}_${randomBytes(4).toString('hex')}`;
  const all = () => [...data.tickers, ...data.triggers];
  const nameTaken = (name, list, except) => list.some(x => x.id !== except && x.name.toLowerCase() === name.toLowerCase());

  /** By id, or by name (case-insensitive); [type] narrows it. */
  function find(ref, type = '') {
    const r = String(ref || '').trim();
    const list = type === 'ticker' ? data.tickers : type === 'trigger' ? data.triggers : all();
    return list.find(x => x.id === r) || list.find(x => x.name.toLowerCase() === r.toLowerCase()) || null;
  }
  const view = (x) => ({ ...x, summary: describe(x) });

  return {
    file,
    list: () => ({ tickers: data.tickers.map(view), triggers: data.triggers.map(view) }),
    find,
    addTicker(i = {}, by = 'cru') {
      if (data.tickers.length >= MAX_TICKERS) throw new Error(`There are already ${MAX_TICKERS} tickers; delete one first`);
      const name = clip(i.name, 60);
      if (!name) throw new Error('A ticker needs a name');
      if (nameTaken(name, data.tickers)) throw new Error(`A ticker called "${name}" already exists`);
      const purpose = clip(i.purpose, 500);
      if (!purpose) throw new Error('A ticker needs a purpose: what it wakes you for');
      const t = {
        id: newId('tk'), type: 'ticker', name, purpose, ...cleanTickerSchedule(i), window: cleanWindow({ from: i.from, to: i.to, days: i.windowDays }),
        enabled: i.enabled === true, enabledAt: i.enabled === true ? now() : 0, createdBy: by === 'owner' ? 'owner' : 'cru',
        createdAt: now(), lastFiredAt: 0, fires: 0,
      };
      data.tickers.push(t); save();
      return view(t);
    },
    updateTicker(ref, i = {}) {
      const t = find(ref, 'ticker');
      if (!t) throw new Error(`No ticker "${ref}"`);
      if (i.name !== undefined) { const n = clip(i.name, 60); if (!n) throw new Error('The name cannot be empty'); if (nameTaken(n, data.tickers, t.id)) throw new Error(`A ticker called "${n}" already exists`); t.name = n; }
      if (i.purpose !== undefined) { const p = clip(i.purpose, 500); if (!p) throw new Error('The purpose cannot be empty'); t.purpose = p; }
      if (i.every !== undefined || i.at !== undefined || i.days !== undefined) Object.assign(t, cleanTickerSchedule(i, t));
      if (i.from !== undefined || i.to !== undefined || i.windowDays !== undefined) t.window = cleanWindow({ from: i.from ?? t.window?.from, to: i.to ?? t.window?.to, days: i.windowDays ?? t.window?.days });
      save();
      return view(t);
    },
    addTrigger(i = {}, by = 'cru') {
      if (data.triggers.length >= MAX_TRIGGERS) throw new Error(`There are already ${MAX_TRIGGERS} triggers; delete one first`);
      const name = clip(i.name, 60);
      if (!name) throw new Error('A trigger needs a name');
      if (nameTaken(name, data.triggers)) throw new Error(`A trigger called "${name}" already exists`);
      const purpose = clip(i.purpose, 500);
      if (!purpose) throw new Error('A trigger needs a purpose: what to do when it fires');
      const condition = cleanCondition(i.condition || { kind: i.kind, ...i });
      const cd = i.cooldown === undefined || i.cooldown === null || i.cooldown === '' ? DEFAULT_COOLDOWN_MIN : Number(i.cooldown);
      const g = {
        id: newId('tr'), type: 'trigger', name, purpose, condition, window: cleanWindow(i),
        once: condition.kind === 'at' ? true : i.once === true,
        cooldown: Math.max(MIN_COOLDOWN_MIN, Math.min(24 * 60, Math.round(Number.isFinite(cd) ? cd : DEFAULT_COOLDOWN_MIN))),
        enabled: i.enabled === true, enabledAt: i.enabled === true ? now() : 0, createdBy: by === 'owner' ? 'owner' : 'cru',
        createdAt: now(), lastFiredAt: 0, fires: 0,
      };
      data.triggers.push(g); save();
      return view(g);
    },
    setEnabled(ref, on, type = '') {
      const x = find(ref, type);
      if (!x) throw new Error(`No ${type || 'ticker or trigger'} "${ref}"`);
      const was = x.enabled;
      x.enabled = !!on;
      if (on && !was) x.enabledAt = now();
      save();
      return view(x);
    },
    remove(ref, type = '') {
      const x = find(ref, type);
      if (!x) throw new Error(`No ${type || 'ticker or trigger'} "${ref}"`);
      if (x.builtin) throw new Error(`"${x.name}" is built in: switch it off instead`);
      data.tickers = data.tickers.filter(t => t.id !== x.id);
      data.triggers = data.triggers.filter(t => t.id !== x.id);
      save();
      return { ok: true, deleted: x.name };
    },
    /** A wake-up was delivered to Cru. A once-trigger switches itself off. */
    markFired(id, at = now(), { quiet = false } = {}) {
      const x = find(id);
      if (!x) return;
      x.lastFiredAt = at; if (!quiet) x.fires = (x.fires || 0) + 1;
      if (x.type === 'trigger' && x.once) x.enabled = false;
      save();
    },
    /** Without saving (a missed one-off time, dropped). */
    disable(id) { const x = find(id); if (x && x.enabled) { x.enabled = false; save(); } },
    enabled: () => all().filter(x => x.enabled),
  };
}

// ─── reading the machine (read-only) ───

/** CPU use between two os.cpus() readings, 0-100. */
export function cpuPercent(prev, cur) {
  if (!prev || !cur || prev.length !== cur.length) return null;
  let idle = 0, total = 0;
  for (let i = 0; i < cur.length; i++) {
    const a = prev[i].times, b = cur[i].times;
    const t = (b.user - a.user) + (b.nice - a.nice) + (b.sys - a.sys) + (b.idle - a.idle) + (b.irq - a.irq);
    total += t; idle += b.idle - a.idle;
  }
  return total > 0 ? Math.round((1 - idle / total) * 1000) / 10 : null;
}

export function memoryPercent() {
  const total = os.totalmem(), free = os.freemem();
  return total ? Math.round((1 - free / total) * 1000) / 10 : null;
}

/**
 * The names of running processes, lower-cased: the command name and the
 * program's file name. Linux reads /proc; elsewhere `ps` or `tasklist` (fixed
 * commands, no shell). null when the list cannot be had.
 */
export async function listProcessNames() {
  if (process.platform === 'linux' && existsSync('/proc')) {
    const names = new Set();
    let dirs = [];
    try { dirs = readdirSync('/proc').filter(d => /^\d+$/.test(d)); } catch { return null; }
    for (const d of dirs) {
      try { names.add(readFileSync(`/proc/${d}/comm`, 'utf-8').trim().toLowerCase()); } catch { continue; }
      try {
        const argv0 = readFileSync(`/proc/${d}/cmdline`, 'utf-8').split('\0');
        if (argv0[0]) names.add(basename(argv0[0]).toLowerCase());
        // An interpreter's script ("node server.js", "python bot.py"): its file name counts too.
        if (argv0[1] && /^(node|python\d*(\.\d+)?|ruby|perl|php|bun|deno|java)$/.test(basename(argv0[0]).toLowerCase())) {
          const script = argv0.slice(1).find(a => a && !a.startsWith('-'));
          if (script) names.add(basename(script).toLowerCase());
        }
      } catch { /* gone */ }
    }
    return names;
  }
  const run = (cmd, args) => new Promise((ok) => {
    execFile(cmd, args, { timeout: 8000, windowsHide: true, maxBuffer: 8 * 1024 * 1024 }, (err, out) => ok(err ? null : String(out)));
  });
  if (process.platform === 'win32') {
    const out = await run('tasklist', ['/fo', 'csv', '/nh']);
    if (out == null) return null;
    const names = new Set();
    for (const line of out.split(/\r?\n/)) { const m = /^"([^"]+)"/.exec(line); if (m) { const n = m[1].toLowerCase(); names.add(n); names.add(n.replace(/\.exe$/, '')); } }
    return names;
  }
  const out = await run('ps', ['-axo', 'comm=']);
  if (out == null) return null;
  return new Set(out.split('\n').map(l => basename(l.trim()).toLowerCase()).filter(Boolean));
}

// ─── the engine ───

const lc = (s) => String(s || '').toLowerCase();
const chatMatches = (c, chat) => (!c.project || lc(chat.project) === lc(c.project))
  && (!c.match || chat.id === c.match || lc(chat.title).includes(lc(c.match)));
const nameMatches = (want, ...names) => !want || names.some(n => lc(n) === lc(want) || lc(n).includes(lc(want)));

/**
 * Decides when tickers and triggers fire and hands Cru the note.
 *
 * @param store       createWakeupStore()
 * @param report      async (note, item) -> { ok, quiet?, error?, busy? }: wakes Cru and delivers what it says
 * @param overLimit   () => boolean: the month's spend limit is reached
 * @param maxPerHour  the global cap
 */
export function createWakeupEngine({ store, report, overLimit = () => false, now = () => Date.now(), maxPerHour = MAX_PER_HOUR, log = () => {}, prepare = () => '' }) {
  const fired = [];                 // when each wake-up went to Cru, oldest first
  const state = new Map();          // trigger id -> { aboveSince, latched, retries }
  const retry = new Map();          // item id -> { what, tries }
  const inflight = new Set();       // item ids handed to Cru and not answered yet
  let last = null;                  // the previous sample, for what changed
  let chain = Promise.resolve();
  const st = (id) => { if (!state.has(id)) state.set(id, {}); return state.get(id); };
  const prune = (t) => { while (fired.length && t - fired[0] >= HOUR) fired.shift(); };

  /** Why [item] may not fire now, or '' when it may. */
  function blocked(item, t = now()) {
    if (!item.enabled) return 'off';
    if (!inWindow(item.window, new Date(t))) return 'outside its time window';
    if (item.type === 'trigger' && !item.once && item.lastFiredAt && t - item.lastFiredAt < item.cooldown * 60 * 1000) return 'cooling down';
    prune(t);
    if (fired.length >= maxPerHour) return `the cap of ${maxPerHour} wake-ups an hour`;
    if (overLimit()) return 'the monthly spend limit';
    return '';
  }

  function noteFor(item, what, t) {
    const when = new Date(t).toLocaleString('en-GB', { weekday: 'long', day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit' });
    const head = item.type === 'ticker'
      ? `[Ticker "${item.name}" (${item.id}, ${describe(item)}) — purpose: ${item.purpose}] It is ${when}.${item.fires ? ` It has woken you ${item.fires} time${item.fires === 1 ? '' : 's'} before.` : ' This is its first tick.'}`
      : `[Trigger "${item.name}" (${item.id}, ${describe(item)}) fired — purpose: ${item.purpose} — what happened: ${what}] It is ${when}.${item.once ? ' It was a one-off and is now off.' : ''}`;
    return head;
  }

  /** Queue one wake-up (one at a time). Resolves to what happened. */
  function fire(item, what = '', { tries = 0 } = {}) {
    const t = now();
    const why = inflight.has(item.id) ? 'already waking' : blocked(item, t);
    if (why) { log(`[voice] ${item.type} "${item.name}" not fired: ${why}`); return Promise.resolve({ ok: false, skipped: why }); }
    // More for the note (the heartbeat's checklist), or null: nothing to do, so no call at all (not counted).
    let extra = '';
    try { extra = prepare(item); } catch { extra = ''; }
    if (extra === null) { store.markFired(item.id, t, { quiet: true }); return Promise.resolve({ ok: true, skipped: 'nothing to check' }); }
    // Counted before it runs, so a burst cannot get past the cap; in flight, so not twice.
    fired.push(t);
    inflight.add(item.id);
    const job = chain.then(async () => {
      let r;
      try { r = await report(noteFor(item, what, t) + (extra || ''), item); } catch (e) { r = { ok: false, error: e.message }; }
      inflight.delete(item.id);
      if (r && r.busy) {
        // Cru was busy: not a wake-up spent, so it is given back and tried on a later scan.
        const k = fired.indexOf(t); if (k >= 0) fired.splice(k, 1);
        if (tries < BUSY_RETRIES) retry.set(item.id, { what, tries: tries + 1 });
      } else store.markFired(item.id, t);
      return r || { ok: false };
    });
    chain = job.catch(() => {});
    return job;
  }

  /** Times: tickers due, one-off times reached, and wake-ups that found Cru busy. */
  function scan(t = now()) {
    const date = new Date(t);
    const out = [];
    for (const [id, r] of [...retry]) {
      retry.delete(id);
      const x = store.find(id);
      if (x && x.enabled) out.push(fire({ ...x, lastFiredAt: 0 }, r.what, { tries: r.tries }));
    }
    for (const x of store.enabled()) {
      if (retry.has(x.id) || inflight.has(x.id)) continue;
      if (x.type === 'ticker') { if (tickerDue(x, date)) out.push(fire(x)); continue; }
      if (x.condition.kind !== 'at') continue;
      const at = parseAt(x.condition.datetime);
      if (!(t >= at)) continue;
      if (t - at > AT_GRACE_MS) { store.disable(x.id); continue; }
      out.push(fire(x, `the time set for it, ${String(x.condition.datetime).replace('T', ' ')}, has come`));
    }
    return out;
  }

  const kindsOn = () => new Set(store.enabled().filter(x => x.type === 'trigger').map(x => x.condition.kind));

  /**
   * One reading of the machine. Each field is optional (left out when not
   * needed or not available): cpuPct, memPct, chats [{id,title,project,collaborator}],
   * services [{key,name,status}], containers [{name,health}], processes Set of names.
   * The first reading of each only sets the baseline.
   */
  function sample(s = {}, t = now()) {
    const out = [];
    const prev = last || {};
    const triggers = store.enabled().filter(x => x.type === 'trigger');
    for (const g of triggers) {
      const c = g.condition;
      if (c.kind === 'cpu_above' || c.kind === 'memory_above') {
        const v = c.kind === 'cpu_above' ? s.cpuPct : s.memPct;
        if (v == null) continue;
        const m = st(g.id);
        if (v <= c.percent) { m.aboveSince = 0; m.latched = false; continue; }
        if (!m.aboveSince) m.aboveSince = t;
        if (!m.latched && t - m.aboveSince >= (c.forMinutes || 0) * 60 * 1000) {
          const r = fire(g, `${c.kind === 'cpu_above' ? 'CPU' : 'memory'} is at ${Math.round(v)}%${c.forMinutes ? `, above ${c.percent}% for ${Math.round((t - m.aboveSince) / 60000)} min` : ''}`);
          m.latched = true;   // once per crossing: it must fall back below to fire again
          out.push(r);
        }
        continue;
      }
      if ((c.kind === 'chat_opened' || c.kind === 'chat_closed') && s.chats && prev.chats) {
        const before = new Map(prev.chats.map(x => [x.id, x])), now_ = new Map(s.chats.map(x => [x.id, x]));
        const changed = c.kind === 'chat_opened' ? s.chats.filter(x => !before.has(x.id)) : prev.chats.filter(x => !now_.has(x.id));
        const hit = changed.filter(x => !x.collaborator && chatMatches(c, x));
        if (hit.length) out.push(fire(g, hit.map(x => `the "${x.title || 'untitled'}" chat in ${x.project} was ${c.kind === 'chat_opened' ? 'opened' : 'closed'}`).join('; ')));
        continue;
      }
      if (c.kind === 'collaborator_action' && s.chats && prev.chats) {
        const before = new Set(prev.chats.map(x => x.id)), now_ = new Set(s.chats.map(x => x.id));
        const hit = [
          ...s.chats.filter(x => x.collaborator && !before.has(x.id) && nameMatches(c.collaborator, x.collaborator.name, x.collaborator.id)).map(x => `${x.collaborator.name || 'a collaborator'} opened a chat in ${x.project}`),
          ...prev.chats.filter(x => x.collaborator && !now_.has(x.id) && nameMatches(c.collaborator, x.collaborator.name, x.collaborator.id)).map(x => `${x.collaborator.name || 'a collaborator'}'s chat in ${x.project} was closed`),
        ];
        if (hit.length) out.push(fire(g, hit.join('; ')));
        continue;
      }
      if (c.kind === 'service_stopped' && s.services && prev.services) {
        const was = new Map(prev.services.map(x => [x.key, x.status]));
        const hit = s.services.filter(x => was.get(x.key) === 'running' && x.status !== 'running' && nameMatches(c.service, x.name, x.key));
        if (hit.length) out.push(fire(g, hit.map(x => `the service "${x.name}" ${x.status === 'crashed' ? 'crashed' : 'stopped'}`).join('; ')));
        continue;
      }
      if (c.kind === 'container_unhealthy' && s.containers && prev.containers) {
        const was = new Map(prev.containers.map(x => [x.name, x.health]));
        const hit = s.containers.filter(x => x.health === 'unhealthy' && was.has(x.name) && was.get(x.name) !== 'unhealthy' && nameMatches(c.container, x.name));
        if (hit.length) out.push(fire(g, hit.map(x => `the container "${x.name}" is unhealthy`).join('; ')));
        continue;
      }
      if ((c.kind === 'process_started' || c.kind === 'process_stopped') && s.processes && prev.processes) {
        const n = lc(c.name);
        const has = (set) => set.has(n) || set.has(n + '.exe');
        if (c.kind === 'process_started' && has(s.processes) && !has(prev.processes)) out.push(fire(g, `a process called "${c.name}" started`));
        if (c.kind === 'process_stopped' && !has(s.processes) && has(prev.processes)) out.push(fire(g, `the process "${c.name}" is no longer running`));
      }
    }
    // What was read is the next baseline. A field left out (not needed, or not
    // available) is forgotten, so a stale reading never makes a change later.
    last = { chats: s.chats || null, services: s.services || null, containers: s.containers || null, processes: s.processes || null };
    return out;
  }

  /** Something that happened: 'chat_finished' | 'chat_waiting' with { chat, what }. */
  function event(kind, { chat, what = '' } = {}) {
    const out = [];
    if (!chat) return out;
    for (const g of store.enabled()) {
      if (g.type !== 'trigger') continue;
      const c = g.condition;
      if (chat.collaborator) {
        if (c.kind === 'collaborator_action' && kind === 'chat_finished' && nameMatches(c.collaborator, chat.collaborator.name, chat.collaborator.id)) {
          out.push(fire(g, `${chat.collaborator.name || 'a collaborator'}'s chat "${chat.title || 'untitled'}" in ${chat.project} finished a turn`));
        }
        continue;
      }
      if (c.kind !== kind || !chatMatches(c, chat)) continue;
      out.push(fire(g, `the "${chat.title || 'untitled'}" chat in ${chat.project} ${kind === 'chat_finished' ? 'finished its turn' : `is waiting on Sir${what ? `: ${what}` : ''}`}`));
    }
    return out;
  }

  return {
    fire, scan, sample, event, kindsOn,
    /** For the UI and list_wakeups. */
    limits() { prune(now()); return { perHour: maxPerHour, firedLastHour: fired.length, minEvery: MIN_EVERY_MIN, defaultCooldown: DEFAULT_COOLDOWN_MIN }; },
    /** Tests. */
    get firedTimes() { return [...fired]; },
    idle: () => chain,
  };
}

// ─── Cru's tools ───

const str = (description, extra = {}) => ({ type: 'string', description, ...extra });
const num = (description) => ({ type: 'number', description });
const bool = (description) => ({ type: 'boolean', description });
const days = { type: 'array', items: { type: 'string', enum: ['sun', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat'] }, description: 'Only on these days (none: every day)' };
const obj = (properties = {}, required = []) => ({ type: 'object', properties, required, additionalProperties: false });

export const WAKEUP_TOOL_NAMES = ['list_wakeups', 'add_ticker', 'update_ticker', 'set_ticker_enabled', 'delete_ticker', 'add_trigger', 'set_trigger_enabled', 'delete_trigger', 'alert_owner'];

/**
 * Action definitions for actions.js (its gated(): read runs as a lookup,
 * safe runs without a yes, even locked). None of them can change anything
 * outside this store, except alert_owner, which only notifies.
 *
 * @param store   createWakeupStore()
 * @param engine  createWakeupEngine() (for its limits)
 * @param alert   async ({ title, body, urgent }) -> result: rings or notifies the owner's phone
 */
export function wakeupToolDefs({ store, engine, alert }) {
  const brief = (x) => ({ id: x.id, name: x.name, purpose: x.purpose, enabled: x.enabled, summary: x.summary || describe(x), lastFiredAt: x.lastFiredAt ? new Date(x.lastFiredAt).toISOString() : null, fires: x.fires || 0, createdBy: x.createdBy });
  const tryIt = (fn) => { try { return fn(); } catch (e) { return { ok: false, error: e.message }; } };
  const windowProps = {
    from: str('Only from this time of day, HH:MM (server local time)'),
    to: str('Only until this time of day, HH:MM; earlier than from wraps past midnight'),
    days,
  };
  return [
    {
      name: 'list_wakeups',
      description: 'Your tickers and triggers: each one\'s name, purpose, schedule or condition, whether it is on, and when it last woke you; and the hourly cap.',
      input_schema: obj({}),
      read: true,
      run: () => { const l = store.list(); return { ok: true, tickers: l.tickers.map(brief), triggers: l.triggers.map(brief), limits: engine ? engine.limits() : {} }; },
    },
    {
      name: 'add_ticker',
      description: `Add a ticker of your own: it wakes you on a schedule (every N minutes, at least ${MIN_EVERY_MIN}; or daily at HH:MM, optionally only on some days), with a purpose that says what to do on each tick (e.g. "good morning and a briefing: usage, services, open chats"). It is off unless enabled is true: switch it on yourself when it would help, or when the owner asks. Optional from/to/windowDays keep an every-N ticker within hours. You can only look and talk on a tick; nothing runs.`,
      input_schema: obj({
        name: str('A short name, e.g. "morning"'), purpose: str('What it is for and what to do on each tick'),
        every: num(`Minutes between ticks (at least ${MIN_EVERY_MIN})`), at: str('Daily time HH:MM (server local time), instead of every'), days,
        from: windowProps.from, to: windowProps.to, windowDays: { ...days, description: 'With every: only on these days' },
        enabled: bool('Switch it on now'), asked_by_owner: bool('True when the owner asked for it'),
      }, ['name', 'purpose']),
      safe: true,
      run: (i) => tryIt(() => ({ ok: true, ticker: brief(store.addTicker(i, i.asked_by_owner ? 'owner' : 'cru')) })),
    },
    {
      name: 'update_ticker',
      description: 'Change a ticker\'s name, purpose or schedule (by id or name).',
      input_schema: obj({ ticker: str('Its id or name'), name: str('New name'), purpose: str('New purpose'), every: num('Minutes between ticks'), at: str('Daily time HH:MM'), days, from: windowProps.from, to: windowProps.to, windowDays: { ...days, description: 'With every: only on these days' } }, ['ticker']),
      safe: true,
      run: (i) => tryIt(() => { const { ticker, ...rest } = i; return { ok: true, ticker: brief(store.updateTicker(ticker, rest)) }; }),
    },
    {
      name: 'set_ticker_enabled',
      description: 'Switch a ticker on or off (by id or name). No confirmation needed. Switch one off as soon as there is nothing left for it to do.',
      input_schema: obj({ ticker: str('Its id or name'), enabled: bool('On (true) or off (false)') }, ['ticker', 'enabled']),
      safe: true,
      run: (i) => tryIt(() => ({ ok: true, ticker: brief(store.setEnabled(i.ticker, i.enabled === true, 'ticker')) })),
    },
    {
      name: 'delete_ticker',
      description: 'Delete a ticker (by id or name). Prefer switching it off when it may be wanted again.',
      input_schema: obj({ ticker: str('Its id or name') }, ['ticker']),
      safe: true,
      run: (i) => tryIt(() => store.remove(i.ticker, 'ticker')),
    },
    {
      name: 'add_trigger',
      description: `Add a trigger: it wakes you when a condition is met, so you can decide whether to tell the owner. kind is one of: ${KINDS.map(k => `${k} (${CONDITION_KINDS[k].about}${Object.keys(CONDITION_KINDS[k].params).length ? '; ' + Object.keys(CONDITION_KINDS[k].params).join(', ') : ''})`).join('; ')}. For chats, project and match (an id or part of the title) narrow it; for services, containers and collaborators, part of the name. at takes datetime like 2026-11-10T07:30 (server local time) and fires once. from/to/days limit it to a time window (e.g. only in the morning). It fires at most once per cooldown (default ${DEFAULT_COOLDOWN_MIN} minutes) unless once is true. Off unless enabled is true. Built-in and read-only: you cannot watch arbitrary commands.`,
      input_schema: obj({
        name: str('A short name'), purpose: str('Why it exists and what to do when it fires'),
        kind: str('The condition', { enum: KINDS }),
        percent: num('cpu_above / memory_above: the threshold, 1-99'), forMinutes: num('cpu_above / memory_above: for how long (default 5)'),
        project: str('chat_*: only chats in this project'), match: str('chat_*: a chat id or part of its title'),
        service: str('service_stopped: part of the service name'), container: str('container_unhealthy: part of the container name'),
        process: str('process_started / process_stopped: the process name, e.g. "ffmpeg"'), collaborator: str('collaborator_action: part of their name'),
        datetime: str('at: when, e.g. 2026-11-10T07:30'),
        ...windowProps,
        once: bool('Fire once, then switch off'), cooldown: num(`Minutes before it may fire again (at least ${MIN_COOLDOWN_MIN})`),
        enabled: bool('Switch it on now'), asked_by_owner: bool('True when the owner asked for it'),
      }, ['name', 'purpose', 'kind']),
      safe: true,
      run: (i) => tryIt(() => {
        const condition = { kind: i.kind, percent: i.percent, forMinutes: i.forMinutes, project: i.project, match: i.match, service: i.service, container: i.container, name: i.process, collaborator: i.collaborator, datetime: i.datetime };
        return { ok: true, trigger: brief(store.addTrigger({ name: i.name, purpose: i.purpose, condition, from: i.from, to: i.to, days: i.days, once: i.once, cooldown: i.cooldown, enabled: i.enabled }, i.asked_by_owner ? 'owner' : 'cru')) };
      }),
    },
    {
      name: 'set_trigger_enabled',
      description: 'Switch a trigger on or off (by id or name). No confirmation needed. Switch one off as soon as what it waits for no longer matters.',
      input_schema: obj({ trigger: str('Its id or name'), enabled: bool('On (true) or off (false)') }, ['trigger', 'enabled']),
      safe: true,
      run: (i) => tryIt(() => ({ ok: true, trigger: brief(store.setEnabled(i.trigger, i.enabled === true, 'trigger')) })),
    },
    {
      name: 'delete_trigger',
      description: 'Delete a trigger (by id or name).',
      input_schema: obj({ trigger: str('Its id or name') }, ['trigger']),
      safe: true,
      run: (i) => tryIt(() => store.remove(i.trigger, 'trigger')),
    },
    {
      name: 'alert_owner',
      description: 'Alert the owner on his phone at once. urgent rings it until seen; otherwise a notification. Only when he must know or act right now (an outage, something he asked to be woken for); never for ordinary news, which you simply say. Limited per hour.',
      input_schema: obj({ title: str('One short line'), body: str('A sentence or two more, optional'), urgent: bool('Ring the phone') }, ['title']),
      safe: true,
      run: async (i) => {
        if (!alert) return { ok: false, error: 'Alerts are not available on this server' };
        try { return await alert({ title: String(i.title || ''), body: String(i.body || ''), urgent: i.urgent === true }); }
        catch (e) { return { ok: false, error: e.message }; }
      },
    },
  ];
}
