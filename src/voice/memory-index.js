/**
 * voice/memory-index.js: the voice assistant's searchable memory (Phase 2,
 * §2.5.5, D30, D47).
 *
 * One SQLite file (node:sqlite, no native dependency) in
 * <dataDir>/voice-memory/index.db holding every turn, every compaction
 * summary and the notebook, searched two ways and merged:
 *
 *   - keyword: an FTS5 table, BM25 ranking: practically free on CPU;
 *   - meaning: a small local embedding model (all-MiniLM-L6-v2, 384
 *     dimensions, ~23 MB, ~10-15 ms a sentence on this server), its vectors
 *     kept as blobs in the same file and compared with every one at search
 *     time (milliseconds at one person's scale; no vector database).
 *
 * The two rankings are merged by reciprocal rank fusion. If the model can't
 * be loaded (the optional dependency missing, no network for the first
 * download), search is keyword only and says so; nothing else breaks.
 */

import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const MODEL = 'Xenova/all-MiniLM-L6-v2';
const DIMS = 384;
const RRF_K = 60;

/**
 * The embedder: loads the model once, the first time it is needed, and never
 * blocks a write (writes queue for it).
 * @param {object} o
 * @param {string} o.cacheDir  where the model is downloaded and kept
 * @param {Function} [o.load]  tests pass a fake: () => (texts) => Promise<number[][]>
 */
export function createEmbedder({ cacheDir, load } = {}) {
  let fn = null;
  let failed = '';
  let loading = null;
  async function ready() {
    if (fn) return fn;
    if (failed) return null;
    if (!loading) loading = (async () => {
      try {
        if (load) { fn = await load(); return fn; }
        const t = await import('@huggingface/transformers');
        t.env.cacheDir = cacheDir;
        t.env.allowRemoteModels = true;
        const pipe = await t.pipeline('feature-extraction', MODEL, { dtype: 'q8' });
        fn = async (texts) => (await pipe(texts, { pooling: 'mean', normalize: true })).tolist();
        return fn;
      } catch (e) {
        failed = String(e.message || e).slice(0, 200);
        return null;
      }
    })();
    return loading;
  }
  return {
    /** Vectors for the texts, or null when meaning search is unavailable. */
    async embed(texts) { const f = await ready(); return f ? f(texts) : null; },
    get status() { return fn ? 'ready' : failed ? `unavailable: ${failed}` : 'not loaded yet'; },
  };
}

const toBlob = (v) => Buffer.from(new Float32Array(v).buffer);
const fromBlob = (b) => new Float32Array(b.buffer, b.byteOffset, b.byteLength / 4);

/** An FTS5 query from free text: each word as a prefix term, OR'd, so a few words find. */
export function ftsQuery(text) {
  const words = String(text || '').toLowerCase().match(/[\p{L}\p{N}]{2,}/gu) || [];
  return [...new Set(words)].slice(0, 12).map(w => `"${w}"*`).join(' OR ');
}

/** Reciprocal rank fusion of ranked id lists. Pure, so it can be tested. */
export function fuse(lists, k = RRF_K) {
  const score = new Map();
  for (const list of lists) list.forEach((id, i) => score.set(id, (score.get(id) || 0) + 1 / (k + i + 1)));
  return [...score.entries()].sort((a, b) => b[1] - a[1]).map(([id]) => id);
}

/**
 * @param {object} o
 * @param {string} o.dir       <dataDir>/voice-memory
 * @param {object} [o.embedder] createEmbedder(); tests pass a fake
 */
/** A remembered turn ("Owner: …" or "Owner (<device>): …", then "\nCru: …\n<detail>") as its parts. */
export function splitTurn({ id, at, text }) {
  const s = String(text || '');
  const cut = s.indexOf('\nCru: ');
  let said = cut >= 0 ? s.slice(0, cut) : s;
  const rest = cut >= 0 ? s.slice(cut + 6) : '';
  // "Owner (PC app): …": the device it came from, when recorded (voice/device.js).
  const from = /^Owner \(([^()\n]*)\): /.exec(said);
  said = from ? said.slice(from[0].length) : said.replace(/^Owner: /, '');
  const nl = rest.indexOf('\n');
  const speak = nl >= 0 ? rest.slice(0, nl) : rest;
  const show = nl >= 0 ? rest.slice(nl + 1) : '';
  // Cru speaking up unprompted (a report, a ticker, a trigger): not the owner's words.
  const report = /^\[(Report|Ticker|Trigger|Wake)/i.test(said);
  return { id: Number(id), at: Number(at), said: report ? '' : said, speak, show, report, ...(from ? { device: from[1] } : {}) };
}

export function createMemoryIndex({ dir, embedder } = {}) {
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true, mode: 0o700 });
  const db = new DatabaseSync(join(dir, 'index.db'));
  db.exec(`
    CREATE TABLE IF NOT EXISTS entries (id INTEGER PRIMARY KEY, kind TEXT NOT NULL, ref TEXT, at INTEGER NOT NULL, project TEXT, text TEXT NOT NULL, vec BLOB);
    CREATE INDEX IF NOT EXISTS entries_ref ON entries(kind, ref);
    CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(text, content='entries', content_rowid='id');
    CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN INSERT INTO entries_fts(rowid, text) VALUES (new.id, new.text); END;
    CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN INSERT INTO entries_fts(entries_fts, rowid, text) VALUES ('delete', old.id, old.text); END;
  `);
  const emb = embedder || createEmbedder({ cacheDir: join(dir, 'models') });
  const queue = [];
  let draining = false;

  async function drain() {
    if (draining) return;
    draining = true;
    try {
      while (queue.length) {
        const batch = queue.splice(0, 16);
        const vecs = await emb.embed(batch.map(b => b.text));
        if (!vecs) { queue.length = 0; break; }
        const up = db.prepare('UPDATE entries SET vec = ? WHERE id = ?');
        batch.forEach((b, i) => { if (vecs[i]?.length === DIMS) up.run(toBlob(vecs[i]), b.id); });
      }
    } finally { draining = false; }
  }

  /**
   * Remember something. kind: 'turn' | 'summary' | 'note'. ref: for notes,
   * the file path (an edit replaces the old entry).
   */
  function add(kind, text, { ref = null, project = null, at = Date.now() } = {}) {
    const t = String(text || '').trim();
    if (ref) db.prepare('DELETE FROM entries WHERE kind = ? AND ref = ?').run(kind, ref);
    if (!t) return null;
    const r = db.prepare('INSERT INTO entries (kind, ref, at, project, text) VALUES (?, ?, ?, ?, ?)').run(kind, ref, at, project, t.slice(0, 8000));
    const id = Number(r.lastInsertRowid);
    queue.push({ id, text: t.slice(0, 2000) });
    drain().catch(() => {});
    return id;
  }

  /**
   * Search memory: keyword and meaning, merged.
   * @returns {Promise<{results: {kind,at,project,text}[], meaning: boolean}>}
   */
  async function search(query, { since = 0, project = null, limit = 8 } = {}) {
    const q = String(query || '').trim();
    if (!q) return { results: [], meaning: false };
    const where = ['e.at >= ?']; const args = [Number(since) || 0];
    if (project) { where.push('(e.project = ? OR e.project IS NULL)'); args.push(String(project).toLowerCase()); }
    let keyword = [];
    const fq = ftsQuery(q);
    if (fq) {
      keyword = db.prepare(`SELECT e.id FROM entries_fts f JOIN entries e ON e.id = f.rowid WHERE entries_fts MATCH ? AND ${where.join(' AND ')} ORDER BY bm25(entries_fts) LIMIT 30`)
        .all(fq, ...args).map(r => r.id);
    }
    let meaning = [];
    const qv = await emb.embed([q]).catch(() => null);
    if (qv && qv[0]) {
      const v = qv[0];
      const rows = db.prepare(`SELECT e.id, e.vec FROM entries e WHERE e.vec IS NOT NULL AND ${where.join(' AND ')}`).all(...args);
      meaning = rows.map(r => { const w = fromBlob(r.vec); let d = 0; for (let i = 0; i < DIMS; i++) d += v[i] * w[i]; return [r.id, d]; })
        .filter(([, d]) => d > 0.2).sort((a, b) => b[1] - a[1]).slice(0, 30).map(([id]) => id);
    }
    const ids = fuse([keyword, meaning]).slice(0, limit);
    const get = db.prepare('SELECT kind, at, project, text FROM entries WHERE id = ?');
    return { results: ids.map(id => get.get(id)).filter(Boolean).map(r => ({ ...r, at: new Date(r.at).toISOString() })), meaning: !!(qv && qv[0]) };
  }

  return {
    add, search,
    /** Tests: let queued embeddings finish. */
    /** The latest turns, newest first, before [before] (ms epoch): the voice screen's chat view. */
    turns({ before = 0, limit = 50 } = {}) {
      const n = Math.max(1, Math.min(200, Number(limit) || 50));
      const rows = before > 0
        ? db.prepare("SELECT id, at, text FROM entries WHERE kind = 'turn' AND at < ? ORDER BY at DESC, id DESC LIMIT ?").all(Number(before), n)
        : db.prepare("SELECT id, at, text FROM entries WHERE kind = 'turn' ORDER BY at DESC, id DESC LIMIT ?").all(n);
      return rows.map(splitTurn);
    },
    async settle() { await drain(); },
    get embedder() { return emb.status; },
    count: () => db.prepare('SELECT COUNT(*) AS n FROM entries').get().n,
    close: () => db.close(),
  };
}
