/**
 * factory-triggers.js — what starts a factory run without the person
 * pressing Start.
 *
 * A trigger belongs to a project and names a factory. Kinds:
 *   schedule  at HH:MM (server time), on chosen weekdays (none = every day)
 *   kanban    a card enters a column of the project's board
 *   webhook   a POST to a secret URL (CI, an issue tracker, a form)
 *
 * Each carries a change request whose text may hold placeholders, filled
 * when it fires: {{date}}, {{time}}; for kanban {{card.title}},
 * {{card.description}}, {{card.todos}}, {{card.id}}; for webhooks
 * {{body.some.path}} and {{body}} (the whole body, clipped).
 *
 * One run per project still holds: a trigger that fires while a run is going
 * either skips or waits its turn (queued, started when the project is free).
 *
 * Stored in <dataDir>/factory-triggers.json. Pure apart from load/save.
 * Spec: specs/factory.md §4.9.
 */

import { readFileSync, writeFileSync, renameSync, existsSync, mkdirSync } from 'fs';
import { join, dirname } from 'path';
import { randomBytes } from 'crypto';

export const TRIGGER_KINDS = ['schedule', 'kanban', 'webhook'];
export const KANBAN_COLUMNS = ['backlog', 'todo', 'in_progress', 'done'];
const QUEUE_MAX = 20;
const HISTORY_MAX = 20;

const str = (v, max = 2000) => (typeof v === 'string' ? v : v == null ? '' : String(v)).slice(0, max);
const newId = () => 'trg_' + randomBytes(6).toString('hex');
const newKey = () => randomBytes(24).toString('base64url');

/** A trigger, cleaned. `prev` keeps the id, key, history and queue. */
export function normalizeTrigger(t, prev = null) {
  const x = t && typeof t === 'object' ? t : {};
  const kind = TRIGGER_KINDS.includes(x.kind) ? x.kind : 'schedule';
  const days = Array.isArray(x.schedule?.days) ? [...new Set(x.schedule.days.map(Number).filter(d => d >= 0 && d <= 6))].sort() : [];
  const at = /^([01]\d|2[0-3]):[0-5]\d$/.test(str(x.schedule?.at)) ? x.schedule.at : '09:00';
  const sections = {};
  for (const [k, v] of Object.entries(x.cr?.sections || {})) if (/^[\w-]{1,40}$/.test(k)) sections[k] = str(v, 6000);
  return {
    id: prev?.id || (typeof x.id === 'string' && /^trg_[0-9a-f]{12}$/.test(x.id) ? x.id : newId()),
    name: str(x.name, 80).trim() || 'Trigger',
    enabled: x.enabled !== false,
    project: str(x.project, 100).toLowerCase(),
    factoryId: str(x.factoryId, 100),
    kind,
    schedule: { at, days },
    kanban: { status: KANBAN_COLUMNS.includes(x.kanban?.status) ? x.kanban.status : 'todo' },
    webhook: { key: prev?.webhook?.key || newKey() },
    cr: { title: str(x.cr?.title, 300), sections },
    mergeMode: x.mergeMode === 'auto' || x.mergeMode === 'ask' ? x.mergeMode : '',
    ifBusy: x.ifBusy === 'skip' ? 'skip' : 'queue',
    createdAt: prev?.createdAt || new Date().toISOString(),
    lastFired: prev?.lastFired || '',
    history: prev?.history || [],
    queue: prev?.queue || [],
  };
}

/** Read a dotted path from an object ("issue.title"). */
function pick(obj, path) {
  let v = obj;
  for (const k of path.split('.')) { if (v == null || typeof v !== 'object') return undefined; v = v[k]; }
  return v;
}

/** Fill {{placeholders}} in a text from the firing's variables. Unknown ones become empty. */
export function fill(text, vars) {
  return str(text, 20000).replace(/\{\{\s*([\w.]+)\s*\}\}/g, (_, name) => {
    let v = name === 'body' ? vars.body : name.startsWith('body.') ? pick(vars.body, name.slice(5)) : pick(vars, name);
    if (v == null) return '';
    if (typeof v === 'object') v = JSON.stringify(v);
    return String(v).slice(0, name === 'body' ? 4000 : 6000);
  });
}

/** The variables a firing offers, by kind. */
export function varsFor(kind, { now = new Date(), card = null, body = null } = {}) {
  const pad = (n) => String(n).padStart(2, '0');
  const v = { date: `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`, time: `${pad(now.getHours())}:${pad(now.getMinutes())}` };
  if (kind === 'kanban' && card) {
    v.card = {
      id: card.id, title: card.title || '', description: card.description || '',
      todos: (card.todos || []).filter(t => !t.deleted).map(t => t.text).join('\n'),
    };
  }
  if (kind === 'webhook') v.body = body && typeof body === 'object' ? body : body == null ? {} : { text: String(body) };
  return v;
}

/** The change request a firing produces. */
export function crFor(trigger, vars) {
  const sections = {};
  for (const [k, v] of Object.entries(trigger.cr.sections || {})) sections[k] = fill(v, vars).trim();
  return { title: fill(trigger.cr.title, vars).replace(/\s+/g, ' ').trim(), sections, attachments: [] };
}

/**
 * Is a schedule trigger due at `now`? Due once per matching minute: at its
 * time, on its days, and not already fired this minute (or later).
 */
export function isDue(trigger, now = new Date()) {
  if (!trigger.enabled || trigger.kind !== 'schedule') return false;
  const [h, m] = trigger.schedule.at.split(':').map(Number);
  if (now.getHours() !== h || now.getMinutes() !== m) return false;
  if (trigger.schedule.days.length && !trigger.schedule.days.includes(now.getDay())) return false;
  const slot = new Date(now); slot.setSeconds(0, 0);
  return !(trigger.lastFired && Date.parse(trigger.lastFired) >= slot.getTime());
}

/** Note the outcome of a firing, newest first, bounded. */
export function record(trigger, entry) {
  trigger.history = [{ at: new Date().toISOString(), ...entry }, ...(trigger.history || [])].slice(0, HISTORY_MAX);
}

export function enqueue(trigger, item) {
  trigger.queue = [...(trigger.queue || []), { at: new Date().toISOString(), ...item }].slice(-QUEUE_MAX);
}

// ─── Store ───

export function createTriggerStore(dataDir) {
  const FILE = join(dataDir, 'factory-triggers.json');
  let cache = null;
  const load = () => {
    if (cache) return cache;
    try { cache = existsSync(FILE) ? (JSON.parse(readFileSync(FILE, 'utf-8')).triggers || []) : []; } catch { cache = []; }
    cache = cache.map(t => normalizeTrigger(t, t));
    return cache;
  };
  const save = () => {
    mkdirSync(dirname(FILE), { recursive: true });
    const tmp = FILE + '.tmp';
    writeFileSync(tmp, JSON.stringify({ triggers: cache }, null, 2));
    renameSync(tmp, FILE);
  };
  return {
    list: ({ project } = {}) => load().filter(t => !project || t.project === String(project).toLowerCase()),
    get: (id) => load().find(t => t.id === id) || null,
    upsert(input) {
      const all = load();
      const prev = input?.id ? all.find(t => t.id === input.id) : null;
      const t = normalizeTrigger(input, prev);
      if (!t.project) return { ok: false, error: 'Choose a project.' };
      if (!t.factoryId) return { ok: false, error: 'Choose a factory.' };
      if (!t.cr.title.trim()) return { ok: false, error: 'Give the change request a title (placeholders are fine).' };
      cache = prev ? all.map(x => (x.id === t.id ? t : x)) : [...all, t];
      save();
      return { ok: true, trigger: t };
    },
    remove(id) {
      const all = load();
      if (!all.some(t => t.id === id)) return { ok: false, error: 'Trigger not found.' };
      cache = all.filter(t => t.id !== id);
      save();
      return { ok: true };
    },
    rotateKey(id) {
      const t = load().find(x => x.id === id);
      if (!t) return { ok: false, error: 'Trigger not found.' };
      t.webhook.key = newKey();
      save();
      return { ok: true, trigger: t };
    },
    /** Change a trigger in place (history, queue, lastFired) and save. */
    touch(id, fn) {
      const t = load().find(x => x.id === id);
      if (!t) return null;
      fn(t);
      save();
      return t;
    },
  };
}

// ─── Firing ───

/**
 * Fires triggers. startRun is the runner's; activeRunFor tells if a project
 * is busy. Every outcome lands in the trigger's history.
 * @param {object} o
 * @param {ReturnType<createTriggerStore>} o.store
 * @param {(args) => Promise<object>} o.startRun
 * @param {(project:string) => object|null} o.activeRunFor
 * @param {(msg:string, trigger:object, result:object) => void} [o.notify]
 */
export function createTriggerDispatcher({ store, startRun, activeRunFor, notify = () => {} }) {
  async function fire(t, vars, { source, cardId = '' }) {
    const cr = crFor(t, vars);
    if (activeRunFor(t.project)) {
      if (t.ifBusy === 'skip') {
        store.touch(t.id, x => { x.lastFired = new Date().toISOString(); record(x, { outcome: 'skipped', source, note: 'A run was already going in this project.' }); });
        return { ok: false, skipped: true, error: 'A run is already going in this project; skipped.' };
      }
      store.touch(t.id, x => { x.lastFired = new Date().toISOString(); enqueue(x, { cr, source, cardId }); record(x, { outcome: 'queued', source, title: cr.title }); });
      return { ok: true, queued: true };
    }
    return start(t, cr, source, cardId);
  }

  async function start(t, cr, source, cardId = '') {
    const r = await startRun({ project: t.project, factoryId: t.factoryId, cr, mergeMode: t.mergeMode || undefined, ignoreMissingSkills: true, trigger: { id: t.id, name: t.name, source, ...(cardId ? { cardId } : {}) } });
    store.touch(t.id, x => {
      x.lastFired = new Date().toISOString();
      record(x, r.ok ? { outcome: 'started', source, runId: r.run.id, title: cr.title } : { outcome: 'failed', source, title: cr.title, note: r.error || 'It could not start.' });
    });
    notify(r.ok ? `Trigger "${t.name}" started a factory run in ${t.project}: ${cr.title}` : `Trigger "${t.name}" could not start a run in ${t.project}: ${r.error}`, t, r);
    return r;
  }

  return {
    /** Every minute: schedules that are due, and queued firings whose project is free. */
    async tick(now = new Date()) {
      for (const t of store.list()) {
        if (isDue(t, now)) await fire(t, varsFor('schedule', { now }), { source: 'schedule' });
      }
      for (const t of store.list()) {
        if (!t.enabled || !(t.queue || []).length || activeRunFor(t.project)) continue;
        const next = t.queue[0];
        store.touch(t.id, x => { x.queue = x.queue.slice(1); });
        await start(t, next.cr, next.source + ' (queued)', next.cardId || '');
      }
    },
    /** A kanban card moved into `status` in `project`. */
    async onKanban(project, card, status) {
      const out = [];
      for (const t of store.list({ project })) {
        if (!t.enabled || t.kind !== 'kanban' || t.kanban.status !== status) continue;
        out.push(await fire(t, varsFor('kanban', { card }), { source: `card "${String(card.title || '').slice(0, 60)}"`, cardId: card.id }));
      }
      return out;
    },
    /** A POST to a webhook trigger's URL. */
    async onWebhook(id, key, body) {
      const t = store.get(id);
      if (!t || t.kind !== 'webhook') return { ok: false, status: 404, error: 'No such trigger.' };
      const a = Buffer.from(String(key || '')), b = Buffer.from(t.webhook.key);
      const { timingSafeEqual } = await import('crypto');
      if (a.length !== b.length || !timingSafeEqual(a, b)) return { ok: false, status: 403, error: 'Wrong key.' };
      if (!t.enabled) return { ok: false, status: 409, error: 'This trigger is turned off.' };
      const r = await fire(t, varsFor('webhook', { body }), { source: 'webhook' });
      return r.ok ? { ok: true, queued: !!r.queued, runId: r.run?.id || '' } : { ok: false, status: 409, error: r.error };
    },
    /** "Run now" from the UI: fires as if it had just happened (no card or body). */
    async runNow(id) {
      const t = store.get(id);
      if (!t) return { ok: false, error: 'Trigger not found.' };
      return fire(t, varsFor(t.kind, {}), { source: 'run now' });
    },
    /** What the change request would look like, for the editor. */
    preview(t, sample = {}) {
      return crFor(normalizeTrigger(t), varsFor(t.kind, sample));
    },
  };
}
