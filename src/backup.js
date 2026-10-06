/**
 * backup.js — backups of the whole of Crundi to any S3-compatible store.
 *
 * What goes in:
 *   env/.env          sign-in, Telegram, tunnel, TLS and other settings
 *   data/…            everything in the data dir: projects (and the project
 *                     folder setup), pane layouts, kanban, notes, mindmap,
 *                     media, schedules, services, forwards, secrets (still
 *                     PIN-sealed), chat history, notification settings…
 *   claude/<dir>/…    the three most recent Claude transcripts per project
 *   skills/<name>/…   the global Claude skills that were put there (not the ones
 *                     Crundi ships or Claude Code syncs, which come back anyway)
 *   manifest.json     what this is, from where, and when
 * Left out: the browser profile (large, and only sign-ins), collaborator
 * worktrees (working copies), logs and temporary files.
 *
 * The archive is a simple record stream, gzipped, then encrypted with
 * AES-256-GCM under a key derived (scrypt) from the backup passphrase:
 *
 *   "CRUNDIBK" | u32 meta length | meta JSON (plain: kdf, iv, check, when) |
 *   ciphertext | 16-byte GCM tag
 *
 * Restoring downloads and decrypts into a staging folder; restore-apply.js
 * swaps it in at the next start, before anything has loaded the old files.
 *
 * Scheduled backups wait while you are using Crundi: when one falls due with
 * you here, a message says so and it starts once you have left.
 */

import { createCipheriv, createDecipheriv, randomBytes, scryptSync, createHash } from 'node:crypto';
import {
  existsSync, readFileSync, writeFileSync, mkdirSync, readdirSync, statSync, lstatSync, rmSync, renameSync,
  openSync, readSync, writeSync, closeSync, createReadStream, createWriteStream,
} from 'node:fs';
import { join, resolve as resolvePath, relative, sep, dirname } from 'node:path';
import { hostname, homedir } from 'node:os';
import { pipeline } from 'node:stream/promises';
import { Readable, Writable } from 'node:stream';
import { createGzip, createGunzip } from 'node:zlib';
import { config, envPath } from './config.js';
import { createS3 } from './s3.js';
import { backupFiles as skillBackupFiles } from './skills-store.js';

const MAGIC = Buffer.from('CRUNDIBK');
const REC_MAGIC = Buffer.from('CRA1');
const SCRYPT = { N: 1 << 15, r: 8, p: 1 };
const SUFFIX = '.crbk';
export const DEFAULT_KEEP = 14;
const TRANSCRIPTS_PER_PROJECT = 3;
const AWAY_BEFORE_RUN_MS = 2 * 60 * 1000;   // gone this long before a waiting backup starts

// Never backed up: the browser profile, working copies, logs and leftovers.
const SKIP_TOP = new Set(['chrome', 'worktrees', 'collab-mcp', 'update.log', 'update.sh', 'backup-tmp']);
const skipName = (n) => n.endsWith('.tmp') || /\.bak(\b|-|$)/.test(n) || n.startsWith('.restore');

const appDirOf = () => config.appDir;
const cfgFile = () => join(config.dataDir, 'backup.json');
const tmpDir = () => join(appDirOf(), '.backup-tmp');
export const stagingDir = () => join(appDirOf(), '.restore-staging');
export const restoreResultFile = () => join(appDirOf(), 'restore-result.json');

let pkgVersion = '';
try { pkgVersion = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf-8')).version || ''; } catch { /* ignore */ }

// ─── settings ───

const DEFAULTS = {
  storage: { endpoint: '', region: 'auto', bucket: '', prefix: 'crundi/', accessKeyId: '', secretAccessKey: '', pathStyle: true },
  passphrase: '',
  schedule: { enabled: false, frequency: 'daily', time: '03:00', weekday: 0, everyHours: 12 },
  keep: DEFAULT_KEEP,
  state: { lastSlot: 0, last: null },
};

export function readConfig() {
  let c = {};
  try { c = JSON.parse(readFileSync(cfgFile(), 'utf-8')) || {}; } catch { /* none yet */ }
  return {
    storage: { ...DEFAULTS.storage, ...(c.storage || {}) },
    passphrase: String(c.passphrase || ''),
    schedule: { ...DEFAULTS.schedule, ...(c.schedule || {}) },
    keep: c.keep === undefined ? DEFAULT_KEEP : Math.max(0, Math.round(Number(c.keep) || 0)),
    state: { ...DEFAULTS.state, ...(c.state || {}) },
  };
}

function writeConfig(c) {
  mkdirSync(config.dataDir, { recursive: true });
  const tmp = cfgFile() + '.tmp';
  writeFileSync(tmp, JSON.stringify(c, null, 2), { mode: 0o600 });
  renameSync(tmp, cfgFile());
}

export function normPrefix(p) {
  let s = String(p == null ? '' : p).trim().replace(/^\/+/, '');
  if (s && !s.endsWith('/')) s += '/';
  return s;
}

export function isConfigured(c = readConfig()) {
  const s = c.storage;
  return !!(s.endpoint && s.bucket && s.accessKeyId && s.secretAccessKey && c.passphrase);
}

/** Settings as the UI may see them: secrets reduced to "is it set". */
export function publicConfig(c = readConfig()) {
  return {
    storage: { ...c.storage, secretAccessKey: '', hasSecret: !!c.storage.secretAccessKey },
    hasPassphrase: !!c.passphrase,
    schedule: c.schedule,
    keep: c.keep,
    configured: isConfigured(c),
  };
}

/**
 * Apply a settings change from the UI. Blank secret / passphrase fields mean
 * "leave as it is", so the form never has to hold them.
 */
export function updateConfig(body = {}) {
  const c = readConfig();
  if (body.storage) {
    const s = body.storage;
    for (const k of ['endpoint', 'region', 'bucket', 'accessKeyId']) if (s[k] !== undefined) c.storage[k] = String(s[k] || '').trim();
    if (s.prefix !== undefined) c.storage.prefix = normPrefix(s.prefix);
    if (s.pathStyle !== undefined) c.storage.pathStyle = s.pathStyle !== false && s.pathStyle !== 'false';
    if (s.secretAccessKey) c.storage.secretAccessKey = String(s.secretAccessKey).trim();
    if (c.storage.endpoint && !/^https?:\/\//i.test(c.storage.endpoint)) return { ok: false, error: 'The endpoint must start with https://' };
  }
  if (body.passphrase !== undefined && body.passphrase !== '') {
    const p = String(body.passphrase);
    if (p.length < 8) return { ok: false, error: 'Use a passphrase of at least 8 characters' };
    c.passphrase = p;
  }
  const wasEnabled = c.schedule.enabled;
  if (body.schedule) {
    const s = body.schedule;
    if (s.enabled !== undefined) c.schedule.enabled = !!s.enabled;
    if (['daily', 'weekly', 'hours'].includes(s.frequency)) c.schedule.frequency = s.frequency;
    if (s.time !== undefined) {
      if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(String(s.time))) return { ok: false, error: 'Time must be HH:MM' };
      c.schedule.time = String(s.time);
    }
    if (s.weekday !== undefined) c.schedule.weekday = Math.min(6, Math.max(0, Math.round(Number(s.weekday) || 0)));
    if (s.everyHours !== undefined) c.schedule.everyHours = Math.min(168, Math.max(1, Math.round(Number(s.everyHours) || 12)));
  }
  if (body.keep !== undefined) c.keep = Math.max(0, Math.min(1000, Math.round(Number(body.keep) || 0)));
  if (c.schedule.enabled && !isConfigured(c)) return { ok: false, error: 'Set up the storage and a passphrase before turning on scheduled backups' };
  // Turning the schedule on (or changing it) starts from now: a slot that
  // already passed today is not run on the spot.
  if (body.schedule) c.state.lastSlot = mostRecentSlot(c.schedule, Date.now()) || 0;
  if (!wasEnabled && c.schedule.enabled) c.state.lastSlot = mostRecentSlot(c.schedule, Date.now()) || 0;
  writeConfig(c);
  return { ok: true, config: publicConfig(c) };
}

// ─── schedule ───

/** The latest scheduled time at or before `now` (ms), or 0. Server local time. */
export function mostRecentSlot(sch, now = Date.now()) {
  const [hh, mm] = String(sch.time || '03:00').split(':').map(Number);
  const d = new Date(now);
  if (sch.frequency === 'hours') {
    // Every N hours, counted from the set time each day.
    const n = Math.max(1, Number(sch.everyHours) || 12);
    const anchor = new Date(d); anchor.setHours(hh, mm, 0, 0);
    if (anchor.getTime() > now) anchor.setDate(anchor.getDate() - 1);
    const step = n * 3600_000;
    let t = anchor.getTime() + Math.floor((now - anchor.getTime()) / step) * step;
    // A day boundary restarts the count at the set time.
    const nextAnchor = new Date(anchor); nextAnchor.setDate(nextAnchor.getDate() + 1);
    if (t >= nextAnchor.getTime()) t = nextAnchor.getTime();
    return t;
  }
  const s = new Date(d); s.setHours(hh, mm, 0, 0);
  if (sch.frequency === 'weekly') {
    const back = (s.getDay() - (Number(sch.weekday) || 0) + 7) % 7;
    s.setDate(s.getDate() - back);
    if (s.getTime() > now) s.setDate(s.getDate() - 7);
    return s.getTime();
  }
  if (s.getTime() > now) s.setDate(s.getDate() - 1);
  return s.getTime();
}

/** The next scheduled time after `now`. */
export function nextSlot(sch, now = Date.now()) {
  // Walk forward in small steps from the last slot; cheap and exact for all three kinds.
  const last = mostRecentSlot(sch, now);
  const stepMs = sch.frequency === 'hours' ? Math.max(1, Number(sch.everyHours) || 12) * 3600_000 : sch.frequency === 'weekly' ? 7 * 86400_000 : 86400_000;
  let probe = last + stepMs;
  // DST shifts and the hours-reset can move it; settle on the real slot.
  for (let i = 0; i < 4; i++) {
    const s = mostRecentSlot(sch, probe + 2 * 3600_000);
    if (s > now) return s;
    probe += stepMs;
  }
  return probe;
}

// ─── archive ───

function deriveKey(passphrase, salt) {
  return scryptSync(String(passphrase), salt, 32, { ...SCRYPT, maxmem: 128 * SCRYPT.N * SCRYPT.r * 2 });
}
const checkOf = (key) => createHash('sha256').update(key).update('crundi-backup-check').digest().subarray(0, 16).toString('base64');

function walk(root, relBase, out, skipTop) {
  let names = [];
  try { names = readdirSync(root); } catch { return; }
  for (const n of names) {
    if (skipName(n)) continue;
    if (skipTop && SKIP_TOP.has(n)) continue;
    const abs = join(root, n);
    let st;
    try { st = lstatSync(abs); } catch { continue; }
    if (st.isSymbolicLink()) continue;
    const rel = relBase + '/' + n;
    if (st.isDirectory()) walk(abs, rel, out, false);
    else if (st.isFile()) out.push({ abs, rel, size: st.size, mode: st.mode & 0o777, mtime: st.mtimeMs });
  }
}

export function claudeProjectsDir() { return join(homedir(), '.claude', 'projects'); }
export function encodeProjectDir(p) { return resolvePath(p).replace(/[^a-zA-Z0-9]/g, '-'); }

/** Projects as stored, without going through project-store's discovery. */
function projectList() {
  const out = [];
  try {
    const raw = JSON.parse(readFileSync(join(config.dataDir, 'projects.json'), 'utf-8'));
    // { alias: { path, name, … } } (older files: an array of { alias, path }).
    const items = Array.isArray(raw) ? raw.map(p => [p && p.alias, p]) : Object.entries(raw.projects || raw);
    for (const [alias, p] of items) if (p && p.path) out.push({ alias: String((p.alias || alias) || '').toLowerCase(), path: String(p.path) });
  } catch { /* none */ }
  if (config.projectsDir && existsSync(config.projectsDir)) {
    try {
      for (const n of readdirSync(config.projectsDir)) {
        const abs = join(config.projectsDir, n);
        try { if (statSync(abs).isDirectory() && !out.some(x => x.path === abs)) out.push({ alias: n.toLowerCase(), path: abs }); } catch { /* skip */ }
      }
    } catch { /* ignore */ }
  }
  return out;
}

/** Everything that goes into a backup, and the manifest describing it. */
export function collect() {
  const files = [];
  if (envPath && existsSync(envPath)) {
    const st = statSync(envPath);
    files.push({ abs: envPath, rel: 'env/.env', size: st.size, mode: 0o600, mtime: st.mtimeMs });
  }
  walk(config.dataDir, 'data', files, true);
  const projects = projectList();
  let transcripts = 0;
  const seen = new Set();
  for (const p of projects) {
    const enc = encodeProjectDir(p.path);
    if (seen.has(enc)) continue;
    seen.add(enc);
    const dir = join(claudeProjectsDir(), enc);
    let list = [];
    try {
      list = readdirSync(dir).filter(n => n.endsWith('.jsonl')).map(n => {
        const st = statSync(join(dir, n));
        return { abs: join(dir, n), rel: 'claude/' + enc + '/' + n, size: st.size, mode: 0o600, mtime: st.mtimeMs };
      }).sort((a, b) => b.mtime - a.mtime).slice(0, TRANSCRIPTS_PER_PROJECT);
    } catch { /* no transcripts for this one */ }
    transcripts += list.length;
    files.push(...list);
  }
  // Global skills (~/.claude/skills). Project skills live in the project folders,
  // which a backup does not carry.
  let skills = 0;
  try {
    const list = skillBackupFiles();
    skills = new Set(list.map(f => f.rel.split('/')[0])).size;
    for (const f of list) files.push({ abs: f.abs, rel: 'skills/' + f.rel, size: f.size, mode: f.mode, mtime: f.mtime });
  } catch { /* an unreadable skills folder must not fail the backup */ }
  const manifest = {
    format: 1, createdAt: new Date().toISOString(), crundiVersion: pkgVersion,
    host: hostname(), platform: process.platform, home: homedir(),
    appDir: config.appDir, dataDir: config.dataDir,
    projects, transcripts, skills,
    files: files.length, bytes: files.reduce((n, f) => n + f.size, 0),
  };
  return { files, manifest };
}

/** The record stream: manifest first, then every file, then an end marker. */
async function* records(files, manifest, onFile) {
  yield REC_MAGIC;
  const rec = (h) => { const j = Buffer.from(JSON.stringify(h)); const l = Buffer.alloc(4); l.writeUInt32BE(j.length); return Buffer.concat([l, j]); };
  const mj = Buffer.from(JSON.stringify(manifest, null, 2));
  yield rec({ p: 'manifest.json', s: mj.length, m: 0o600, t: Date.now() });
  yield mj;
  for (const f of files) {
    // Sizes can change between listing and reading (a store saving); send
    // what is there now and say so in the header.
    let size;
    try { size = statSync(f.abs).size; } catch { continue; }
    yield rec({ p: f.rel, s: size, m: f.mode, t: f.mtime });
    let sent = 0;
    if (size) {
      const fd = openSync(f.abs, 'r');
      try {
        const buf = Buffer.alloc(Math.min(size, 1 << 20));
        while (sent < size) {
          const n = readSync(fd, buf, 0, Math.min(buf.length, size - sent), sent);
          if (!n) break;
          sent += n;
          yield Buffer.from(buf.subarray(0, n));
        }
      } finally { closeSync(fd); }
    }
    if (sent < size) yield Buffer.alloc(size - sent);   // shrank meanwhile: pad to the promised size
    if (onFile) onFile(f, size);
  }
  const end = Buffer.alloc(4); end.writeUInt32BE(0);
  yield end;
}

/** Write an encrypted archive of `files` to `out`. */
export async function writeArchive(out, passphrase, { files, manifest }, meta = {}, onFile) {
  const salt = randomBytes(16), iv = randomBytes(12);
  const key = deriveKey(passphrase, salt);
  const head = {
    v: 1, kdf: { alg: 'scrypt', ...SCRYPT, salt: salt.toString('base64') }, cipher: 'aes-256-gcm',
    iv: iv.toString('base64'), check: checkOf(key),
    createdAt: manifest.createdAt, host: manifest.host, crundiVersion: manifest.crundiVersion, ...meta,
  };
  const hj = Buffer.from(JSON.stringify(head));
  const lenBuf = Buffer.alloc(4); lenBuf.writeUInt32BE(hj.length);
  writeFileSync(out, Buffer.concat([MAGIC, lenBuf, hj]), { mode: 0o600 });
  const cipher = createCipheriv('aes-256-gcm', key, iv);
  await pipeline(Readable.from(records(files, manifest, onFile)), createGzip({ level: 6 }), cipher, createWriteStream(out, { flags: 'a' }));
  const fd = openSync(out, 'a');
  try { writeSync(fd, cipher.getAuthTag()); } finally { closeSync(fd); }
  return { size: statSync(out).size, head };
}

/** The plain header of an archive file. */
export function readHead(file) {
  const fd = openSync(file, 'r');
  try {
    const pre = Buffer.alloc(12);
    readSync(fd, pre, 0, 12, 0);
    if (!pre.subarray(0, 8).equals(MAGIC)) throw new Error('This is not a Crundi backup');
    const len = pre.readUInt32BE(8);
    if (len > 65536) throw new Error('This backup is damaged');
    const hj = Buffer.alloc(len);
    readSync(fd, hj, 0, len, 12);
    return { head: JSON.parse(hj.toString('utf-8')), offset: 12 + len };
  } finally { closeSync(fd); }
}

/** Is a record path one we may write, and where under `dest`? */
function safeTarget(dest, p) {
  const s = String(p || '');
  if (!s || s.includes('\0') || s.startsWith('/') || /^[a-zA-Z]:/.test(s) || s.split(/[\\/]/).some(x => x === '..' || x === '')) return null;
  if (!(s === 'manifest.json' || s.startsWith('env/') || s.startsWith('data/') || s.startsWith('claude/') || s.startsWith('skills/'))) return null;
  const t = resolvePath(dest, s);
  const r = relative(dest, t);
  if (!r || r.startsWith('..') || r.includes('..' + sep)) return null;
  return t;
}

/**
 * Decrypt and unpack an archive into `dest`. Throws on a wrong passphrase
 * (before writing anything) or on any damage (the GCM tag is checked at the
 * end; the caller discards `dest` then).
 */
export async function extractArchive(file, passphrase, dest, { onFile } = {}) {
  const { head, offset } = readHead(file);
  if (head.v !== 1 || head.cipher !== 'aes-256-gcm' || !head.kdf || head.kdf.alg !== 'scrypt') throw new Error('This backup was made by a newer Crundi; update before restoring it');
  const kdf = head.kdf;
  const key = scryptSync(String(passphrase), Buffer.from(kdf.salt, 'base64'), 32, { N: kdf.N, r: kdf.r, p: kdf.p, maxmem: 128 * kdf.N * kdf.r * 2 });
  if (checkOf(key) !== head.check) { const e = new Error('Wrong passphrase for this backup'); e.code = 'BAD_PASSPHRASE'; throw e; }
  const total = statSync(file).size;
  if (total < offset + 16) throw new Error('This backup is damaged (too short)');
  const tagBuf = Buffer.alloc(16);
  const fd = openSync(file, 'r');
  try { readSync(fd, tagBuf, 0, 16, total - 16); } finally { closeSync(fd); }
  const decipher = createDecipheriv('aes-256-gcm', key, Buffer.from(head.iv, 'base64'));
  decipher.setAuthTag(tagBuf);
  mkdirSync(dest, { recursive: true });

  // Parse the record stream as it arrives.
  let buf = Buffer.alloc(0), state = 'magic', need = 4, cur = null, manifest = null, done = false, count = 0;
  const sink = new Writable({
    write(chunk, _enc, cb) {
      try {
        buf = buf.length ? Buffer.concat([buf, chunk]) : chunk;
        for (;;) {
          if (state === 'data') {
            if (!cur.left) { finishFile(); continue; }
            if (!buf.length) break;
            const n = Math.min(cur.left, buf.length);
            if (cur.fd != null) writeSync(cur.fd, buf, 0, n);
            if (cur.collect) cur.collect.push(Buffer.from(buf.subarray(0, n)));
            cur.left -= n; buf = buf.subarray(n);
            if (!cur.left) finishFile();
            continue;
          }
          if (buf.length < need) break;
          if (state === 'magic') {
            if (!buf.subarray(0, 4).equals(REC_MAGIC)) throw new Error('This backup is damaged (bad contents)');
            buf = buf.subarray(4); state = 'len'; need = 4; continue;
          }
          if (state === 'len') {
            const l = buf.readUInt32BE(0); buf = buf.subarray(4);
            if (l === 0) { done = true; state = 'end'; need = Infinity; break; }
            if (l > 1 << 20) throw new Error('This backup is damaged (bad record)');
            state = 'hdr'; need = l; continue;
          }
          if (state === 'hdr') {
            const h = JSON.parse(buf.subarray(0, need).toString('utf-8')); buf = buf.subarray(need);
            const t = safeTarget(dest, h.p);
            if (!t) throw new Error('This backup contains an unsafe path: ' + String(h.p).slice(0, 80));
            mkdirSync(dirname(t), { recursive: true });
            cur = { p: h.p, t, left: Number(h.s) || 0, fd: openSync(t, 'w', (h.m & 0o777) || 0o600), collect: h.p === 'manifest.json' ? [] : null };
            state = 'data'; need = 0; continue;
          }
          break;
        }
        cb();
      } catch (err) { cb(err); }
    },
  });
  function finishFile() {
    if (cur.fd != null) closeSync(cur.fd);
    if (cur.collect) { try { manifest = JSON.parse(Buffer.concat(cur.collect).toString('utf-8')); } catch { /* checked below */ } }
    count++;
    if (onFile) onFile(cur.p);
    cur = null; state = 'len'; need = 4;
  }
  try {
    await pipeline(createReadStream(file, { start: offset, end: total - 17 }), decipher, createGunzip(), sink);
  } catch (err) {
    if (cur && cur.fd != null) try { closeSync(cur.fd); } catch { /* ignore */ }
    if (/auth|unable to authenticate|Unsupported state/i.test(err.message)) throw new Error('This backup is damaged or was changed after it was made');
    throw err;
  }
  if (!done || !manifest) throw new Error('This backup is incomplete');
  return { head, manifest, files: count };
}

// ─── object names ───

const pad = (n) => String(n).padStart(2, '0');
function stamp(d = new Date()) { return d.getUTCFullYear() + pad(d.getUTCMonth() + 1) + pad(d.getUTCDate()) + '-' + pad(d.getUTCHours()) + pad(d.getUTCMinutes()) + pad(d.getUTCSeconds()) + String(d.getUTCMilliseconds()).padStart(3, '0'); }
const safeHost = () => hostname().toLowerCase().replace(/[^a-z0-9.-]/g, '-').slice(0, 60) || 'crundi';

/** crundi-<host>-<UTC yyyymmdd-hhmmssmmm>-<scheduled|manual>.crbk */
export function objectName(reason, d = new Date()) { return `crundi-${safeHost()}-${stamp(d)}-${reason === 'scheduled' ? 'scheduled' : 'manual'}${SUFFIX}`; }

export function parseObjectName(key) {
  const base = String(key).split('/').pop();
  const m = base.match(/^crundi-(.+)-(\d{8})-(\d{6})(\d{3})?-(scheduled|manual)\.crbk$/);
  if (!m) return null;
  const [, host, ymd, hms, ms, reason] = m;
  const at = Date.UTC(+ymd.slice(0, 4), +ymd.slice(4, 6) - 1, +ymd.slice(6, 8), +hms.slice(0, 2), +hms.slice(2, 4), +hms.slice(4, 6), +(ms || 0));
  return { host, at, reason };
}

/** Which scheduled backups of this host to delete so `keep` remain (0 = keep all). */
export function toPrune(items, keep, host = safeHost()) {
  if (!keep) return [];
  return items.filter(i => i.host === host && i.reason === 'scheduled').sort((a, b) => b.at - a.at).slice(keep);
}

// ─── the service ───

/**
 * deps:
 *   isPresent()    someone is at Crundi now
 *   awayFor(ms)    nobody has been at Crundi for at least ms
 *   broadcast(st)  push a status change to the owner's open pages
 *   notify(text)   tell the owner out of band (Telegram), when it matters
 */
export function createBackups(deps = {}) {
  let running = null;    // { phase, reason, done, total, startedAt }
  let waiting = null;    // { slot, since }
  let timer = null;
  let lastRestore = null; // the latest restore attempt: { ok, at, error?, manifest?, projects? }

  function s3For(storage) {
    return createS3({ ...storage, pathStyle: storage.pathStyle !== false });
  }

  function status() {
    const c = readConfig();
    let restored = null;
    try { restored = JSON.parse(readFileSync(restoreResultFile(), 'utf-8')); } catch { /* none */ }
    return {
      ...publicConfig(c),
      running: running ? { ...running } : null,
      waiting: waiting ? { since: waiting.since, slot: waiting.slot } : null,
      last: c.state.last,
      nextAt: c.schedule.enabled && isConfigured(c) ? (waiting ? waiting.slot : nextSlot(c.schedule)) : null,
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      restored,
      staged: existsSync(join(stagingDir(), 'READY')),
      restoreAttempt: lastRestore,
    };
  }
  const changed = () => { try { deps.broadcast && deps.broadcast(status()); } catch { /* ignore */ } };

  async function test(storageOverride) {
    const c = readConfig();
    const storage = { ...c.storage, ...(storageOverride || {}) };
    if (storageOverride && !storageOverride.secretAccessKey) storage.secretAccessKey = c.storage.secretAccessKey;
    const s3 = s3For(storage);
    await s3.check(normPrefix(storage.prefix));
    return { ok: true };
  }

  async function list(storageOverride) {
    const c = readConfig();
    const storage = storageOverride ? { ...DEFAULTS.storage, ...storageOverride } : c.storage;
    const s3 = s3For(storage);
    const prefix = normPrefix(storage.prefix);
    const items = (await s3.list(prefix))
      .filter(o => o.key.endsWith(SUFFIX))
      .map(o => ({ ...o, ...(parseObjectName(o.key) || { host: '', at: Date.parse(o.lastModified) || 0, reason: 'manual' }) }))
      .sort((a, b) => b.at - a.at);
    return items;
  }

  /** Make a backup now. reason: 'manual' | 'scheduled'. */
  async function run(reason = 'manual') {
    if (running) return { ok: false, error: 'A backup is already running' };
    const c = readConfig();
    if (!isConfigured(c)) return { ok: false, error: 'Set up the storage and a passphrase first' };
    running = { phase: 'collecting', reason, done: 0, total: 0, startedAt: Date.now() };
    changed();
    const started = Date.now();
    mkdirSync(tmpDir(), { recursive: true });
    const name = objectName(reason);
    const file = join(tmpDir(), name);
    let result;
    try {
      const set = collect();
      running = { ...running, phase: 'packing', total: set.manifest.bytes };
      changed();
      let packed = 0, lastPush = 0;
      await writeArchive(file, c.passphrase, set, { reason }, (f, size) => {
        packed += size; running.done = packed;
        if (Date.now() - lastPush > 1000) { lastPush = Date.now(); changed(); }
      });
      const size = statSync(file).size;
      running = { ...running, phase: 'uploading', done: 0, total: size };
      changed();
      const s3 = s3For(c.storage);
      const key = normPrefix(c.storage.prefix) + name;
      await s3.putFile(key, file, { onProgress: (d) => { running.done = d; if (Date.now() - lastPush > 1000) { lastPush = Date.now(); changed(); } } });
      // Keep the newest N scheduled backups of this machine.
      let pruned = 0;
      if (c.keep) {
        running = { ...running, phase: 'tidying' };
        changed();
        try {
          const all = await list(c.storage);
          for (const old of toPrune(all, c.keep)) { await s3.remove(old.key); pruned++; }
        } catch (err) { console.warn('[backup] Could not remove old backups:', err.message); }
      }
      result = { ok: true, at: Date.now(), reason, key, size, files: set.manifest.files, transcripts: set.manifest.transcripts, tookMs: Date.now() - started, pruned };
      console.log(`[backup] ${reason} backup uploaded: ${key} (${Math.round(size / 1024)} KB, ${set.manifest.files} files)`);
    } catch (err) {
      result = { ok: false, at: Date.now(), reason, error: err.message };
      console.warn('[backup] Backup failed:', err.message);
      if (deps.notify) { try { deps.notify(`Crundi backup failed: ${err.message}`); } catch { /* ignore */ } }
    } finally {
      try { rmSync(file, { force: true }); } catch { /* ignore */ }
      running = null;
    }
    const c2 = readConfig();
    c2.state.last = result;
    writeConfig(c2);
    changed();
    return result;
  }

  /**
   * Download and unpack a backup into the staging folder; it is applied at
   * the next start. storageOverride: from the first-run screen, before any
   * settings exist here. passphrase: blank = the one saved in settings.
   */
  async function restore({ key, passphrase, storage: storageOverride } = {}) {
    if (running) return { ok: false, error: 'A backup is running; try again when it is done' };
    const c = readConfig();
    const storage = storageOverride ? { ...DEFAULTS.storage, ...storageOverride } : c.storage;
    const pass = passphrase || (!storageOverride ? c.passphrase : '');
    if (!pass) return { ok: false, error: 'Enter the passphrase this backup was made with' };
    if (!key || !String(key).endsWith(SUFFIX)) return { ok: false, error: 'Choose a backup' };
    running = { phase: 'downloading', reason: 'restore', done: 0, total: 0, startedAt: Date.now() };
    changed();
    mkdirSync(tmpDir(), { recursive: true });
    const file = join(tmpDir(), 'restore' + SUFFIX);
    const stage = stagingDir();
    try {
      rmSync(stage, { recursive: true, force: true });
      const s3 = s3For(storage);
      let lastPush = 0;
      await s3.getToFile(key, file, { onProgress: (d, t) => { running.done = d; running.total = t; if (Date.now() - lastPush > 1000) { lastPush = Date.now(); changed(); } } });
      running = { ...running, phase: 'unpacking' };
      changed();
      const r = await extractArchive(file, pass, stage);
      writeFileSync(join(stage, 'READY'), JSON.stringify({ key, at: Date.now(), manifest: r.manifest }), { mode: 0o600 });
      console.log(`[backup] Restore staged from ${key} (${r.files} files); applied at the next start`);
      const projects = (r.manifest.projects || []).map(p => ({ alias: p.alias, path: p.path, exists: existsSync(p.path) }));
      lastRestore = { ok: true, staged: true, at: Date.now(), key, manifest: { createdAt: r.manifest.createdAt, host: r.manifest.host, crundiVersion: r.manifest.crundiVersion, files: r.files, transcripts: r.manifest.transcripts }, projects };
      return lastRestore;
    } catch (err) {
      rmSync(stage, { recursive: true, force: true });
      lastRestore = { ok: false, at: Date.now(), key, error: err.message, code: err.code };
      return lastRestore;
    } finally {
      try { rmSync(file, { force: true }); } catch { /* ignore */ }
      running = null;
      changed();
    }
  }

  async function remove(key) {
    const c = readConfig();
    if (!String(key || '').endsWith(SUFFIX) || !String(key).startsWith(normPrefix(c.storage.prefix))) return { ok: false, error: 'Not a backup in this folder' };
    await s3For(c.storage).remove(key);
    return { ok: true };
  }

  function cancelStaged() {
    rmSync(stagingDir(), { recursive: true, force: true });
    lastRestore = null;
    changed();
    return { ok: true };
  }

  /** Skip the waiting scheduled backup (until the next slot). */
  function skip() {
    if (!waiting) return { ok: false, error: 'Nothing is waiting' };
    const c = readConfig();
    c.state.lastSlot = waiting.slot;
    writeConfig(c);
    waiting = null;
    changed();
    return { ok: true };
  }

  /** Run the waiting scheduled backup now, without waiting for you to leave. */
  async function runWaitingNow() {
    if (!waiting) return run('manual');
    const slot = waiting.slot;
    waiting = null;
    const c = readConfig(); c.state.lastSlot = slot; writeConfig(c);
    return run('scheduled');
  }

  async function tick() {
    if (running) return;
    const c = readConfig();
    if (!c.schedule.enabled || !isConfigured(c)) { if (waiting) { waiting = null; changed(); } return; }
    const slot = mostRecentSlot(c.schedule);
    if (!slot || slot <= (c.state.lastSlot || 0)) return;
    // You are here: say so once, then wait until you have gone.
    if (deps.isPresent && deps.isPresent()) {
      if (!waiting || waiting.slot !== slot) { waiting = { slot, since: Date.now() }; changed(); }
      return;
    }
    if (waiting && deps.awayFor && !deps.awayFor(AWAY_BEFORE_RUN_MS)) return;
    waiting = null;
    c.state.lastSlot = slot;
    writeConfig(c);
    await run('scheduled');
  }

  function start() {
    if (timer) return;
    timer = setInterval(() => { tick().catch(err => console.warn('[backup] Scheduler:', err.message)); }, 60_000);
    timer.unref?.();
    setTimeout(() => { tick().catch(() => {}); }, 15_000).unref?.();
  }
  function stop() { if (timer) clearInterval(timer); timer = null; }

  return { status, test, list, run, restore, remove, cancelStaged, skip, runWaitingNow, tick, start, stop, update: (b) => { const r = updateConfig(b); if (r.ok) changed(); return r; } };
}
