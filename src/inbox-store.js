/**
 * inbox-store.js — Crundi's own messages inbox.
 *
 * Everything meant for the owner lands here FIRST; copies then go out to
 * whichever notification channels are on (Telegram, the phone app, the
 * desktop app, browser push). So a message Claude sends with no channel
 * configured at all still reaches the owner: it is waiting in the inbox.
 *
 * ─── Contract (web page, Android app) ───
 *
 * Owner only: a collaborator is refused (403) by access-policy.js and again
 * by the routes themselves.
 *
 *   GET  /api/inbox/threads          -> { ok, threads:[thread], unread }
 *        newest activity first.
 *   GET  /api/inbox?thread=&limit=&before=   -> { ok, items, unread }
 *        newest first (a client shows a thread oldest -> newest); thread
 *        optional (all items); limit 1..200 (default 50); before = an item id
 *        or an `at` (ms): only items older than it.
 *   GET  /api/inbox/<id>             -> { ok, item, unread } (404 if gone)
 *   POST /api/inbox/read   { ids:[...] } | { thread } | { all:true }, optional
 *        read:false to mark unread       -> { ok, changed, unread }
 *   POST /api/inbox/delete { ids:[...] } | { thread } | { allRead:true }
 *                                        -> { ok, removed, unread }
 *   GET  /api/inbox/media/<id>/<name>  the item's copied file. Media urls are
 *        server-relative; fetch with the normal Authorization header (the web
 *        page adds ?token= for <img>); ?download=1 to download.
 *
 *   item = { id, at (ms since epoch, number), kind, title, text, project,
 *            chatId, chatTitle, media:[{ url, name, mime, size }], read,
 *            link, thread }
 *   kind = message | photo | file | alert | question | permission | report |
 *          schedule | system
 *   link = an in-app route (e.g. "/chat/<id>") or ''.
 *
 *   thread = { key, kind:'chat'|'project'|'cru'|'system', title, subtitle,
 *              project, chatId, last:{ id, at, kind, title, text }, unread,
 *              count }
 *   Thread keys, from the item's fields (stored on write; worked out on read
 *   for items from before threads):
 *     'cru'               Cru's reports
 *     'chat:<chatId>'     everything a chat sent. A chat resumed under a new
 *                         id keeps its thread (matched by its Claude
 *                         conversation id, kept on the item as `conv`).
 *     'project:<alias>'   a project's items with no chat (service down,
 *                         factory updates, the project's schedules)
 *     'system'            Crundi itself: anything with no project and no chat
 *
 *   Live: SSE event "inbox" on /api/events (owner pages only) with data
 *   { type:'inbox', unread, item?, thread? }: item when one was added, thread
 *   whenever one thread is involved.
 *   Phone pushes that are copies of an item carry inboxId and inboxThread
 *   (letters, digits, _ . : -).
 *   Capability: 'inbox' (src/client-version.js).
 *
 * ─── Storage ───
 *
 * <dataDir>/inbox.json (0600) and <dataDir>/inbox-media/<id>/<file> (0700
 * folders). Files are COPIED in, so the item survives the original being
 * deleted. Retention: unread items are kept; read items older than 90 days
 * go; past MAX_ITEMS the oldest read go first (then, only if every item is
 * unread, the oldest unread).
 */

import { readFileSync, existsSync, copyFileSync, writeFileSync, statSync, rmSync, readdirSync } from 'node:fs';
import { join, basename, extname } from 'node:path';
import { randomBytes } from 'node:crypto';
import { ensurePrivateDir, writePrivateFile } from './private-fs.js';

export const INBOX_KINDS = Object.freeze(['message', 'photo', 'file', 'alert', 'question', 'permission', 'report', 'schedule', 'system']);
export const MAX_ITEMS = 2000;
export const READ_TTL_MS = 90 * 24 * 60 * 60 * 1000;
/** A file bigger than this is not copied in; the item says so instead. */
export const MAX_MEDIA_BYTES = 200 * 1024 * 1024;
const MAX_TEXT = 20000;
const MAX_TITLE = 200;

const MIME = {
  '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.webp': 'image/webp',
  '.svg': 'image/svg+xml', '.pdf': 'application/pdf', '.txt': 'text/plain', '.md': 'text/markdown',
  '.json': 'application/json', '.csv': 'text/csv', '.html': 'text/html', '.zip': 'application/zip',
  '.gz': 'application/gzip', '.tar': 'application/x-tar', '.mp4': 'video/mp4', '.webm': 'video/webm',
  '.mov': 'video/quicktime', '.mp3': 'audio/mpeg', '.wav': 'audio/wav', '.ogg': 'audio/ogg', '.m4a': 'audio/mp4',
  '.log': 'text/plain', '.xml': 'application/xml', '.docx': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  '.xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
};
export const mimeOf = (name) => MIME[extname(String(name || '')).toLowerCase()] || 'application/octet-stream';

/** A file name that is safe as one path segment and readable in a list. */
export function safeName(name) {
  let n = basename(String(name || '').replace(/\\/g, '/')).replace(/[\u0000-\u001f<>:"/\\|?*]/g, '_').trim();
  if (!n || n === '.' || n === '..') n = 'file';
  return n.slice(-120);
}

// Leading pictographs (the old Telegram texts start with one) are not shown
// in the inbox: the UI draws its own flat icon per kind.
const LEAD_SYMBOLS = /^[\p{Extended_Pictographic}←-⇿⌀-⏿☀-➿️‍\s]+/u;
export function cleanTitle(s) {
  return String(s || '').replace(LEAD_SYMBOLS, '').replace(/\s+/g, ' ').trim().slice(0, MAX_TITLE);
}

/** One notification text -> a title (its first line) and the rest. */
export function splitText(message) {
  const s = String(message || '').replace(/\r\n/g, '\n').trim();
  const nl = s.indexOf('\n');
  if (nl < 0) return { title: cleanTitle(s), text: '' };
  return { title: cleanTitle(s.slice(0, nl)), text: s.slice(nl + 1).trim() };
}

/** The thread an item belongs to, from its own fields. */
export function threadOf(it) {
  if (!it) return 'system';
  if (typeof it.thread === 'string' && it.thread) return it.thread;
  if (it.kind === 'report' && it.chatId) return 'cru';
  if (it.chatId) return 'chat:' + it.chatId;
  if (it.project) return 'project:' + String(it.project).toLowerCase();
  return 'system';
}
export function threadKind(key) {
  const k = String(key || '');
  return k === 'cru' ? 'cru' : k.startsWith('chat:') ? 'chat' : k.startsWith('project:') ? 'project' : 'system';
}
const THREAD_KEY = /^(cru|system|chat:[A-Za-z0-9_.:-]{1,80}|project:[A-Za-z0-9_.-]{1,80})$/;
export const isThreadKey = (k) => THREAD_KEY.test(String(k || ''));

const newId = (now) => Number(now).toString(36) + randomBytes(5).toString('hex');

export function createInboxStore({ dataDir, now = () => Date.now(), maxItems = MAX_ITEMS, readTtlMs = READ_TTL_MS, maxMediaBytes = MAX_MEDIA_BYTES, convOf = () => '' } = {}) {
  if (!dataDir) throw new Error('createInboxStore needs a dataDir');
  const FILE = join(dataDir, 'inbox.json');
  const MEDIA = join(dataDir, 'inbox-media');
  const listeners = new Set();
  let items = null;   // newest first

  function load() {
    if (items) return items;
    items = [];
    try {
      if (existsSync(FILE)) {
        const d = JSON.parse(readFileSync(FILE, 'utf8'));
        if (Array.isArray(d?.items)) items = d.items.filter((x) => x && typeof x.id === 'string');
      }
    } catch (err) { console.warn('[inbox] Could not read inbox.json, starting empty:', err.message); }
    items.sort((a, b) => (b.at - a.at) || (a.id < b.id ? 1 : -1));
    for (const it of items) if (!it.thread) it.thread = threadOf(it);
    if (prune(false)) save();
    return items;
  }

  function save() {
    try {
      ensurePrivateDir(dataDir);
      writePrivateFile(FILE, JSON.stringify({ v: 1, items }));
      return true;
    } catch (err) {
      console.error('[inbox] Could not save:', err.message);
      return false;
    }
  }

  function dropMedia(id) {
    if (!/^[a-z0-9]+$/.test(id)) return;
    try { rmSync(join(MEDIA, id), { recursive: true, force: true }); } catch { /* already gone */ }
  }

  function emit(ev) {
    for (const fn of listeners) { try { fn(ev); } catch { /* a listener must not break the store */ } }
  }

  /** Retention. Returns how many were removed. */
  function prune(announce = true) {
    const list = items || load();
    const cutoff = now() - readTtlMs;
    const gone = new Set();
    for (const it of list) if (it.read && it.at < cutoff) gone.add(it.id);
    let left = list.length - gone.size;
    if (left > maxItems) {
      // Oldest read first…
      for (let i = list.length - 1; i >= 0 && left > maxItems; i--) {
        const it = list[i];
        if (it.read && !gone.has(it.id)) { gone.add(it.id); left--; }
      }
      // …and only when everything left is unread, the oldest unread.
      for (let i = list.length - 1; i >= 0 && left > maxItems; i--) {
        const it = list[i];
        if (!gone.has(it.id)) { gone.add(it.id); left--; }
      }
    }
    if (!gone.size) return 0;
    items = list.filter((it) => !gone.has(it.id));
    for (const id of gone) dropMedia(id);
    if (announce) { save(); emit({ type: 'inbox', unread: unread() }); }
    return gone.size;
  }

  function unread() { return load().reduce((n, it) => n + (it.read ? 0 : 1), 0); }

  function mediaUrl(id, name) { return `/api/inbox/media/${id}/${encodeURIComponent(name)}`; }

  /**
   * Copy files into the item's own folder. Each entry is { path, name?, mime? }
   * or { buffer, name, mime? }. A file too big or unreadable is listed with
   * no url and a note in `missing`.
   */
  function copyMedia(id, files) {
    const out = [];
    const missing = [];
    const used = new Set();
    for (const f of (Array.isArray(files) ? files : []).slice(0, 10)) {
      if (!f) continue;
      let name = safeName(f.name || (f.path ? basename(f.path) : 'file'));
      while (used.has(name)) name = '1-' + name;
      used.add(name);
      const mime = String(f.mime || mimeOf(name));
      try {
        let size;
        if (f.buffer) size = f.buffer.length;
        else { const st = statSync(f.path); if (!st.isFile()) throw new Error('not a file'); size = st.size; }
        if (size > maxMediaBytes) { missing.push(`${name} (${Math.round(size / 1048576)} MB) is too large to keep in the inbox`); out.push({ url: '', name, mime, size }); continue; }
        const dir = join(MEDIA, id);
        ensurePrivateDir(dir);
        const dest = join(dir, name);
        if (f.buffer) writeFileSync(dest, f.buffer, { mode: 0o600 });
        else copyFileSync(f.path, dest);
        out.push({ url: mediaUrl(id, name), name, mime, size });
      } catch (err) {
        missing.push(`${name}: ${err.message}`);
      }
    }
    return { media: out, missing };
  }

  /** Add an item. Returns the stored item (with `missing` notes for files not kept). */
  function add({ kind = 'message', title = '', text = '', project = '', chatId = '', chatTitle = '', link = '', files = [], collapse = false, thread = '', cru = false } = {}) {
    load();
    const at = now();
    const id = newId(at);
    const k = INBOX_KINDS.includes(kind) ? kind : 'message';
    const { media, missing } = copyMedia(id, files);
    let t = cleanTitle(title);
    let body = String(text || '').trim().slice(0, MAX_TEXT);
    if (!t && body) { const s = splitText(body); t = s.title; body = s.text; }
    if (!t) t = k === 'photo' ? 'A picture' : k === 'file' ? (media[0]?.name || 'A file') : 'Message';
    if (missing.length) body = (body ? body + '\n\n' : '') + missing.join('\n');
    const item = {
      id, at, kind: k, title: t, text: body,
      project: String(project || '').slice(0, 80),
      chatId: String(chatId || '').slice(0, 80),
      chatTitle: String(chatTitle || '').slice(0, 160),
      media,
      read: false,
      link: typeof link === 'string' && /^\/[A-Za-z0-9/_\-.?=&#%]*$/.test(link) ? link.slice(0, 300) : (chatId ? `/chat/${String(chatId).slice(0, 80)}` : ''),
    };
    // Which conversation: Cru's, an explicit one, the chat's (followed by its
    // Claude conversation id when the chat came back under a new id), the
    // project's, or Crundi's.
    let conv = '';
    if (item.chatId) { try { conv = String(convOf(item.chatId) || '').slice(0, 80); } catch { conv = ''; } }
    if (conv) item.conv = conv;
    if (cru) item.thread = 'cru';
    else if (thread && isThreadKey(thread)) item.thread = thread;
    else if (item.chatId) {
      const before = conv ? items.find((it) => it.conv === conv && it.thread && it.thread.startsWith('chat:')) : null;
      item.thread = before ? before.thread : 'chat:' + item.chatId;
    } else item.thread = threadOf(item);
    // collapse: the same notice again (an update still available after a
    // restart) replaces the unread one instead of stacking up.
    if (collapse) {
      const same = items.filter((it) => !it.read && it.kind === item.kind && it.title === item.title);
      if (same.length) { items = items.filter((it) => !same.includes(it)); for (const it of same) dropMedia(it.id); }
    }
    items.unshift(item);
    prune(false);
    save();
    emit({ type: 'inbox', unread: unread(), item, thread: item.thread });
    return missing.length ? { ...item, missing } : item;
  }

  function list({ limit = 50, before = null, thread = '' } = {}) {
    const all = thread ? load().filter((it) => it.thread === thread) : load();
    const n = Math.max(1, Math.min(200, Number(limit) || 50));
    let start = 0;
    if (before !== null && before !== undefined && before !== '') {
      const asNum = Number(before);
      if (Number.isFinite(asNum) && /^\d+$/.test(String(before))) {
        start = all.findIndex((it) => it.at < asNum);
      } else {
        const i = all.findIndex((it) => it.id === String(before));
        start = i < 0 ? all.length : i + 1;
      }
      if (start < 0) start = all.length;
    }
    return { items: all.slice(start, start + n).map((it) => ({ ...it, media: (it.media || []).map((m) => ({ ...m })) })), unread: unread() };
  }

  function get(id) { return load().find((it) => it.id === id) || null; }

  function markRead({ ids = null, all = false, read = true, thread = '' } = {}) {
    const want = all || thread ? null : new Set((Array.isArray(ids) ? ids : []).map(String));
    let changed = 0;
    const touched = new Set();
    for (const it of load()) {
      if (thread && it.thread !== thread) continue;
      if (want && !want.has(it.id)) continue;
      if (it.read !== !!read) { it.read = !!read; changed++; touched.add(it.thread); }
    }
    if (changed) { save(); emit({ type: 'inbox', unread: unread(), ...(touched.size === 1 ? { thread: [...touched][0] } : {}) }); }
    return changed;
  }

  function remove({ ids = null, allRead = false, thread = '' } = {}) {
    const want = new Set((Array.isArray(ids) ? ids : []).map(String));
    const keep = [];
    const gone = [];
    const touched = new Set();
    for (const it of load()) {
      if (want.has(it.id) || (allRead && it.read) || (thread && it.thread === thread)) { gone.push(it.id); touched.add(it.thread); } else keep.push(it);
    }
    if (!gone.length) return 0;
    items = keep;
    for (const id of gone) dropMedia(id);
    save();
    emit({ type: 'inbox', unread: unread(), ...(touched.size === 1 ? { thread: [...touched][0] } : {}) });
    return gone.length;
  }

  /** Every thread, newest activity first. */
  function threads() {
    const by = new Map();
    for (const it of load()) {   // newest first, so the first seen is the last
      const key = it.thread || threadOf(it);
      let t = by.get(key);
      if (!t) {
        const kind = threadKind(key);
        t = {
          key, kind, title: '', subtitle: '', project: '', chatId: '',
          last: { id: it.id, at: it.at, kind: it.kind, title: it.title, text: String(it.text || '').slice(0, 300) },
          unread: 0, count: 0,
        };
        by.set(key, t);
      }
      t.count++;
      if (!it.read) t.unread++;
      if (!t.project && it.project) t.project = it.project;
      if (!t.chatId && it.chatId) t.chatId = it.chatId;
      if (!t.title && t.kind === 'chat' && it.chatTitle) t.title = it.chatTitle;
    }
    for (const t of by.values()) {
      if (t.kind === 'cru') { t.title = 'Cru'; t.subtitle = 'Reports on the chats Cru follows'; t.chatId = ''; }
      else if (t.kind === 'system') { t.title = 'Crundi'; t.subtitle = 'Updates from Crundi itself'; }
      else if (t.kind === 'project') { t.project = t.project || t.key.slice('project:'.length); t.title = t.project; t.subtitle = 'Project'; t.chatId = ''; }
      else { t.title = t.title || 'Chat'; t.subtitle = t.project; }
    }
    return [...by.values()];
  }

  /** The file behind a media url, or null. Never outside the item's folder. */
  function mediaPath(id, name) {
    const it = get(String(id || ''));
    if (!it) return null;
    const m = (it.media || []).find((x) => x.name === name && x.url);
    if (!m) return null;
    const p = join(MEDIA, it.id, safeName(m.name));
    return existsSync(p) ? { path: p, name: m.name, mime: m.mime, size: m.size } : null;
  }

  /** Folders in inbox-media with no item behind them (a crash mid-add). */
  function sweepOrphans() {
    try {
      if (!existsSync(MEDIA)) return 0;
      const known = new Set(load().map((it) => it.id));
      let n = 0;
      for (const d of readdirSync(MEDIA)) if (!known.has(d)) { dropMedia(d); n++; }
      return n;
    } catch { return 0; }
  }

  function onChange(fn) { listeners.add(fn); return () => listeners.delete(fn); }

  return { add, list, get, markRead, remove, unread, threads, prune, mediaPath, sweepOrphans, onChange, file: FILE, mediaDir: MEDIA };
}
