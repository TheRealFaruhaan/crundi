/**
 * factory-triggers.js — what starts a factory run without the person
 * pressing Start.
 *
 * A trigger belongs to a project and names a factory. Kinds:
 *   schedule  at HH:MM (server time), on chosen weekdays (none = every day)
 *   kanban    a card enters a column of the project's board
 *   webhook   a POST to a public URL (alerting such as Grafana, CI, an issue
 *             tracker), secured either by a key in the URL / header, or by an
 *             HMAC-SHA256 signature of the body with a shared secret
 *
 * Each carries a change request whose text may hold placeholders, filled
 * when it fires: {{date}}, {{time}}; for kanban {{card.title}},
 * {{card.description}}, {{card.todos}}, {{card.id}}; for webhooks
 * {{payload.some.path}} (alias {{body.…}}; arrays by index, alerts.0.labels or
 * alerts[0].labels), {{payload}} (all of it, clipped), {{headers.name}},
 * {{query.name}}. A placeholder the request does not have comes out empty and
 * is listed for the Manager; it never stops the run. The Manager always gets
 * the whole request as well (a file in the change request).
 *
 * Webhooks may name an idempotency key ("{{payload.messageId}}"): a call whose
 * key was already seen is answered 200 and not processed again.
 *
 * One run per project still holds: a trigger that fires while a run is going
 * waits its turn (queued, started when the project is free), is skipped, or
 * (webhooks' default) is handed to the running run's Manager as a message,
 * marked as having arrived mid-run, when that run is of the same factory.
 *
 * Stored in <dataDir>/factory-triggers.json. Pure apart from load/save.
 * Spec: specs/factory.md §4.9.
 */

import { readFileSync, writeFileSync, renameSync, existsSync, mkdirSync } from 'fs';
import { join, dirname } from 'path';
import { randomBytes, createHmac, timingSafeEqual } from 'crypto';

export const TRIGGER_KINDS = ['schedule', 'kanban', 'webhook'];
export const KANBAN_COLUMNS = ['backlog', 'todo', 'in_progress', 'done'];
const QUEUE_MAX = 20;
const HISTORY_MAX = 20;
const SEEN_MAX = 1000;                       // idempotency keys remembered per trigger
const SEEN_TTL_MS = 30 * 24 * 3600_000;      // and for how long
const SIGNATURE_SKEW_MS = 5 * 60_000;        // a signed timestamp may be this old
export const WEBHOOK_AUTH = ['key', 'signature'];
export const IF_BUSY = ['queue', 'skip', 'manager'];
// Never shown to the Manager or written to the run: they are credentials.
const SECRET_HEADERS = new Set(['authorization', 'cookie', 'x-crundi-key', 'proxy-authorization']);
const hname = (v, d) => (/^[A-Za-z0-9-]{1,80}$/.test(str(v).trim()) ? str(v).trim().toLowerCase() : d);

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
    webhook: {
      key: prev?.webhook?.key || newKey(),
      secret: prev?.webhook?.secret || newKey(),
      auth: WEBHOOK_AUTH.includes(x.webhook?.auth) ? x.webhook.auth : 'key',
      signatureHeader: hname(x.webhook?.signatureHeader, 'x-crundi-signature'),
      timestampHeader: x.webhook?.timestampHeader ? hname(x.webhook.timestampHeader, '') : '',
      idempotency: str(x.webhook?.idempotency, 300).trim(),
    },
    cr: { title: str(x.cr?.title, 300), sections },
    mergeMode: x.mergeMode === 'auto' || x.mergeMode === 'ask' ? x.mergeMode : '',
    ifBusy: IF_BUSY.includes(x.ifBusy) ? x.ifBusy : kind === 'webhook' ? 'manager' : 'queue',
    createdAt: prev?.createdAt || new Date().toISOString(),
    lastFired: prev?.lastFired || '',
    history: prev?.history || [],
    queue: prev?.queue || [],
    seen: prev?.seen || [],
  };
}

/** What the browser and the tools may see of a trigger: not the remembered keys. */
export function publicTrigger(t) {
  if (!t) return t;
  const { seen, ...rest } = t;
  return { ...rest, seenCount: (seen || []).length };
}

/** Read a dotted path from an object ("issue.title"). */
function pick(obj, path) {
  let v = obj;
  for (const k of String(path).replace(/\[(\d+)\]/g, '.$1').split('.').filter(Boolean)) {
    if (v == null || typeof v !== 'object' || !Object.prototype.hasOwnProperty.call(v, k)) return undefined;
    v = v[k];
  }
  return v;
}

const PH = /\{\{\s*([\w.\[\]-]+)\s*\}\}/g;
const lookup = (vars, name) => (name.startsWith('payload') && !('payload' in vars) ? pick({ payload: vars.body }, name) : pick(vars, name));

/** The placeholders in a text that this firing has nothing for. */
export function missingIn(text, vars) {
  const out = [];
  for (const m of str(text, 20000).matchAll(PH)) if (lookup(vars, m[1]) == null && !out.includes(m[1])) out.push(m[1]);
  return out;
}

/** Fill {{placeholders}} in a text from the firing's variables. Unknown ones become empty. */
export function fill(text, vars) {
  return str(text, 20000).replace(PH, (_, name) => {
    let v;
    try { v = lookup(vars || {}, name); } catch { v = undefined; }
    if (v == null) return '';
    if (typeof v === 'object') { try { v = JSON.stringify(v); } catch { v = ''; } }
    return String(v).slice(0, name === 'body' || name === 'payload' ? 4000 : 6000);
  });
}

/** The variables a firing offers, by kind. */
export function varsFor(kind, { now = new Date(), card = null, body = null, headers = null, query = null } = {}) {
  const pad = (n) => String(n).padStart(2, '0');
  const v = { date: `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`, time: `${pad(now.getHours())}:${pad(now.getMinutes())}` };
  if (kind === 'kanban' && card) {
    v.card = {
      id: card.id, title: card.title || '', description: card.description || '',
      todos: (card.todos || []).filter(t => !t.deleted).map(t => t.text).join('\n'),
    };
  }
  if (kind === 'webhook') {
    v.body = body && typeof body === 'object' ? body : body == null ? {} : { text: String(body) };
    v.payload = v.body;
    v.headers = cleanHeaders(headers);
    v.query = Object.fromEntries(Object.entries(query || {}).filter(([k]) => k !== 'key'));
  }
  return v;
}

function cleanHeaders(h) {
  const out = {};
  for (const [k, v] of Object.entries(h || {})) {
    const n = k.toLowerCase();
    if (SECRET_HEADERS.has(n) || /signature/.test(n)) continue;
    out[n] = Array.isArray(v) ? v.join(', ') : str(v, 2000);
  }
  return out;
}

/**
 * The change request a firing produces. Never fails on what the call left
 * out: a title that comes out empty becomes the trigger's name and the time.
 */
export function crFor(trigger, vars) {
  const sections = {};
  for (const [k, v] of Object.entries(trigger.cr.sections || {})) { const x = fill(v, vars).trim(); if (x) sections[k] = x; }
  let title = fill(trigger.cr.title, vars).replace(/\s+/g, ' ').trim();
  if (!title) title = `${trigger.name}: ${trigger.kind === 'webhook' ? 'webhook' : trigger.kind} at ${vars.date || ''} ${vars.time || ''}`.trim();
  const missing = [...new Set([trigger.cr.title, ...Object.values(trigger.cr.sections || {})].flatMap(x => missingIn(x, vars)))];
  return { title, sections, attachments: [], missing };
}

/** The idempotency key of a webhook call, from the trigger's template ('' when none). */
export function idemKeyFor(trigger, vars) {
  const tpl = trigger.webhook?.idempotency || '';
  if (!tpl) return '';
  return fill(tpl, vars).replace(/\s+/g, ' ').trim().slice(0, 300);
}

/**
 * Is this call from whoever holds the trigger's credential?
 * key:       ?key=, X-Crundi-Key or Authorization: Bearer <key>
 * signature: HMAC-SHA256 of the raw body (or "<timestamp>:<body>" when a
 *            timestamp header is set, as Grafana does) with the secret, hex or
 *            base64, optionally prefixed "sha256=" (GitHub's form). The key is
 *            not needed then.
 */
export function verifyWebhook(t, { raw = '', headers = {}, key = '' }, now = Date.now()) {
  const eq = (a, b) => { const x = Buffer.from(String(a)), y = Buffer.from(String(b)); return x.length === y.length && x.length > 0 && timingSafeEqual(x, y); };
  const h = (n) => { const v = headers[n] ?? headers[n.toLowerCase()]; return Array.isArray(v) ? v[0] : (v == null ? '' : String(v)); };
  if (t.webhook.auth === 'signature') {
    const got = h(t.webhook.signatureHeader).trim().replace(/^sha256=/i, '');
    if (!got) return { ok: false, status: 401, error: `Missing the signature header (${t.webhook.signatureHeader}).` };
    let signed = String(raw || '');
    if (t.webhook.timestampHeader) {
      const ts = h(t.webhook.timestampHeader).trim();
      if (!ts) return { ok: false, status: 401, error: `Missing the timestamp header (${t.webhook.timestampHeader}).` };
      const n = Number(ts);
      const at = Number.isFinite(n) ? (n < 1e12 ? n * 1000 : n) : Date.parse(ts);
      if (!Number.isFinite(at) || Math.abs(now - at) > SIGNATURE_SKEW_MS) return { ok: false, status: 401, error: 'The signed timestamp is too old or in the future.' };
      signed = ts + ':' + signed;
    }
    const mac = createHmac('sha256', t.webhook.secret).update(signed, 'utf8').digest();
    if (eq(got.toLowerCase(), mac.toString('hex')) || eq(got, mac.toString('base64')) || eq(got, mac.toString('base64url'))) return { ok: true };
    return { ok: false, status: 401, error: 'Bad signature.' };
  }
  const bearer = h('authorization').replace(/^Bearer\s+/i, '');
  const given = key || h('x-crundi-key') || bearer;
  return eq(given, t.webhook.key) ? { ok: true } : { ok: false, status: 403, error: 'Wrong key.' };
}

/**
 * The whole call, for the Manager: method, headers (credentials taken out),
 * query (key taken out), and the body as it came.
 */
export function requestRecord({ method = 'POST', headers = {}, query = {}, body = null, raw = '' }, extra = {}) {
  return {
    receivedAt: new Date().toISOString(), method, ...extra,
    headers: cleanHeaders(headers),
    query: Object.fromEntries(Object.entries(query || {}).filter(([k]) => k !== 'key')),
    body: body && typeof body === 'object' ? body : str(raw, 200000),
  };
}

/** The markdown that tells the Manager how a run (or a message) came about. */
export function requestContext(t, { cr, idem, file = '', request = null }) {
  const lines = [];
  if (t.kind !== 'webhook') return `Started by the trigger "${t.name}".`;
  lines.push(`Started by the webhook trigger "${t.name}"${request && request.receivedAt ? ` (called ${request.receivedAt})` : ''}.`);
  if (t.webhook.idempotency) lines.push(idem ? `Idempotency key: ${idem} (from ${t.webhook.idempotency}). A repeat with this key will not reach you.` : `No idempotency key was found in this call (the trigger reads it from ${t.webhook.idempotency}), so repeats of it cannot be told apart.`);
  if (cr.missing && cr.missing.length) lines.push(`Not in this call, so left empty above: ${cr.missing.map(m => '{{' + m + '}}').join(', ')}. Look in the full request for what came instead.`);
  lines.push(file ? `The full request (headers, query and body) is in ${file}.` : 'The full request (headers, query and body) is attached as webhook-request.json.');
  if (request) lines.push('', 'The body as it came:', '```', bodyPreview(request.body), '```');
  return lines.join('\n');
}

/** The body, pretty and clipped, for reading inline. */
export function bodyPreview(body, max = 12000) {
  let txt;
  try { txt = typeof body === 'string' ? body : JSON.stringify(body, null, 2); } catch { txt = String(body); }
  txt = txt || '';
  return txt.length > max ? txt.slice(0, max) + `\n… (${txt.length - max} more characters in the file)` : txt;
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
    rotateKey(id, which = 'key') {
      const t = load().find(x => x.id === id);
      if (!t) return { ok: false, error: 'Trigger not found.' };
      if (which === 'secret') t.webhook.secret = newKey(); else t.webhook.key = newKey();
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
export function createTriggerDispatcher({ store, startRun, activeRunFor, tellRun = null, notify = () => {} }) {
  /**
   * A trigger fired. Runs it, or, when its project is busy: hands it to the
   * running Manager (same factory, "manager"), queues it, or skips it.
   */
  async function fire(t, vars, { source, cardId = '', request = null, idem = '' }) {
    const cr = crFor(t, vars);
    const active = activeRunFor(t.project);
    if (active) {
      if (t.ifBusy === 'manager' && tellRun && active.factoryId === t.factoryId) {
        const r = await tellRun(active.id, { trigger: t, cr, request, idem, source });
        if (r && r.ok) {
          store.touch(t.id, x => { x.lastFired = new Date().toISOString(); record(x, { outcome: 'told the Manager', source, runId: active.id, title: cr.title, ...(idem ? { key: idem } : {}) }); });
          return { ok: true, told: true, runId: active.id };
        }
        // The Manager cannot be reached (stopped, crashed): keep it for later.
        store.touch(t.id, x => { x.lastFired = new Date().toISOString(); enqueue(x, { cr, source, cardId, request, idem }); record(x, { outcome: 'queued', source, title: cr.title, note: `Could not reach the running Manager: ${(r && r.error) || 'no answer'}.` }); });
        return { ok: true, queued: true };
      }
      if (t.ifBusy === 'skip') {
        store.touch(t.id, x => { x.lastFired = new Date().toISOString(); record(x, { outcome: 'skipped', source, note: 'A run was already going in this project.' }); });
        return { ok: false, skipped: true, error: 'A run is already going in this project; skipped.' };
      }
      store.touch(t.id, x => { x.lastFired = new Date().toISOString(); enqueue(x, { cr, source, cardId, request, idem }); record(x, { outcome: 'queued', source, title: cr.title, ...(t.ifBusy === 'manager' ? { note: 'The run going is of another factory.' } : {}) }); });
      return { ok: true, queued: true };
    }
    return start(t, cr, source, cardId, request, idem);
  }

  async function start(t, cr, source, cardId = '', request = null, idem = '') {
    const files = request ? [{ name: 'webhook-request.json', content: JSON.stringify(request, null, 2) }] : [];
    const crIn = { ...cr, context: requestContext(t, { cr, idem, request }) };
    delete crIn.missing;
    const r = await startRun({ project: t.project, factoryId: t.factoryId, cr: crIn, files, mergeMode: t.mergeMode || undefined, ignoreMissingSkills: true, trigger: { id: t.id, name: t.name, source, ...(cardId ? { cardId } : {}), ...(idem ? { key: idem } : {}) } });
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
        await start(t, next.cr, next.source + ' (queued)', next.cardId || '', next.request || null, next.idem || '');
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
    /**
     * A POST to a webhook trigger's URL.
     * @param {string} id
     * @param {{raw?:string, body?:any, headers?:object, query?:object, key?:string, method?:string}} req
     */
    async onWebhook(id, req = {}) {
      const t = store.get(id);
      if (!t || t.kind !== 'webhook') return { ok: false, status: 404, error: 'No such trigger.' };
      const v = verifyWebhook(t, { raw: req.raw || '', headers: req.headers || {}, key: req.key || '' });
      if (!v.ok) return v;
      if (!t.enabled) return { ok: false, status: 409, error: 'This trigger is turned off.' };
      const vars = varsFor('webhook', { body: req.body, headers: req.headers, query: req.query });
      let idem = '';
      try { idem = idemKeyFor(t, vars); } catch { idem = ''; }
      if (idem) {
        // Checked and claimed in one synchronous step: a burst of the same
        // call gets one through, however close together they arrive.
        let dup = false;
        store.touch(t.id, x => {
          const cutoff = Date.now() - SEEN_TTL_MS;
          x.seen = (x.seen || []).filter(s2 => Date.parse(s2.at) >= cutoff);
          if (x.seen.some(s2 => s2.key === idem)) { dup = true; record(x, { outcome: 'duplicate', source: 'webhook', key: idem, note: 'Already had this one; not processed again.' }); return; }
          x.seen = [...x.seen, { key: idem, at: new Date().toISOString() }].slice(-SEEN_MAX);
        });
        if (dup) return { ok: true, duplicate: true };
      }
      const request = requestRecord({ method: req.method || 'POST', headers: req.headers, query: req.query, body: req.body, raw: req.raw }, { trigger: t.name, ...(idem ? { idempotencyKey: idem } : {}) });
      let r;
      try { r = await fire(t, vars, { source: 'webhook', request, idem }); }
      catch (err) { r = { ok: false, error: err.message }; }
      // Not processed after all: let the sender's retry through.
      if (!r.ok && !r.skipped && idem) store.touch(t.id, x => { x.seen = (x.seen || []).filter(s2 => s2.key !== idem); });
      return r.ok ? { ok: true, queued: !!r.queued, told: !!r.told, runId: r.runId || r.run?.id || '' } : { ok: false, status: r.skipped ? 409 : 500, error: r.error };
    },
    /** "Run now" from the UI: fires as if it had just happened (no card or body). */
    async runNow(id) {
      const t = store.get(id);
      if (!t) return { ok: false, error: 'Trigger not found.' };
      return fire(t, varsFor(t.kind, {}), { source: 'run now' });
    },
    /** What the change request would look like, for the editor. */
    preview(t, sample = {}) {
      const n = normalizeTrigger(t);
      const vars = varsFor(n.kind, sample);
      return { ...crFor(n, vars), idem: n.kind === 'webhook' ? idemKeyFor(n, vars) : '', idemMissing: n.kind === 'webhook' ? missingIn(n.webhook.idempotency, vars) : [] };
    },
  };
}
