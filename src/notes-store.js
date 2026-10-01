/**
 * notes-store.js — per-project note pages (the Notes panel and tab).
 *
 * One file per page under <dataDir>/notes/<alias>/<id>.json, so typing in one
 * page rewrites that page only. A page is a title and a list of blocks:
 *
 *   { id, type, text?, checked?, lang?, rows? }
 *     type: p | h1 | h2 | h3 | todo | bullet | number | quote | code | table | divider
 *     text: inline HTML for text blocks (sanitised: b/strong/i/em/u/s/code/a/br)
 *           plain text for code blocks
 *     rows: table cells, rows of inline HTML (sanitised the same way)
 *
 * Saves are versioned. Each save names the version it was based on; a save
 * from a stale copy (another device edited the page meanwhile) is refused with
 * the current page, so the client reloads instead of silently overwriting.
 *
 * Inline HTML is stored because that is what a rich editor edits, and it is
 * rendered back with innerHTML — so the server keeps only an allow-list of
 * tags and safe link targets, whatever a client sends.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, renameSync, unlinkSync } from 'fs';
import { join } from 'path';
import { randomBytes } from 'crypto';
import { config } from './config.js';

export const BLOCK_TYPES = ['p', 'h1', 'h2', 'h3', 'todo', 'bullet', 'number', 'quote', 'code', 'table', 'divider'];
const MAX_BLOCKS = 3000;
const MAX_PAGE_BYTES = 2 * 1024 * 1024;
const MAX_TITLE = 200;
const TABLE_MAX = 60;           // rows and columns, each

const dirFor = (alias) => join(config.dataDir, 'notes', String(alias || '').toLowerCase().replace(/[^a-z0-9._-]/g, '_'));
const fileFor = (alias, id) => join(dirFor(alias), id + '.json');
const genId = () => randomBytes(8).toString('hex');
const validId = (id) => /^[0-9a-f]{16}$/.test(String(id || ''));

/**
 * Keep only harmless inline markup: the allowed tags, with no attributes
 * except an <a>'s http(s)/mailto href. Everything else (other tags, event
 * handlers, styles, scripts) is dropped; text is left as it is.
 */
export function sanitizeInline(html) {
  let s = String(html == null ? '' : html);
  if (s.length > 200_000) s = s.slice(0, 200_000);
  // Whole elements whose content must never survive as text either.
  s = s.replace(/<(script|style|iframe|object|embed|template)[\s\S]*?<\/\1\s*>/gi, '');
  const ALLOWED = { b: 'strong', strong: 'strong', i: 'em', em: 'em', u: 'u', s: 's', strike: 's', del: 's', code: 'code', a: 'a', br: 'br' };
  return s.replace(/<\/?([a-zA-Z][a-zA-Z0-9]*)\b([^>]*)>/g, (m, name, attrs) => {
    const tag = ALLOWED[name.toLowerCase()];
    if (!tag) return '';
    const closing = m.startsWith('</');
    if (tag === 'br') return closing ? '' : '<br>';
    if (closing) return '</' + tag + '>';
    if (tag === 'a') {
      const hm = attrs.match(/\bhref\s*=\s*("([^"]*)"|'([^']*)'|([^\s>]+))/i);
      const href = hm ? (hm[2] ?? hm[3] ?? hm[4] ?? '') : '';
      const ok = /^(https?:\/\/|mailto:)/i.test(href.trim()) && !/["<>]/.test(href);
      return ok ? '<a href="' + href.trim() + '" target="_blank" rel="noopener noreferrer">' : '<a>';
    }
    return '<' + tag + '>';
  });
}

/** A block as stored: known type, sanitised content, nothing extra. */
function cleanBlock(b) {
  if (!b || typeof b !== 'object') return null;
  const type = BLOCK_TYPES.includes(b.type) ? b.type : 'p';
  const out = { id: /^[0-9a-zA-Z_-]{1,40}$/.test(String(b.id || '')) ? String(b.id) : genId(), type };
  if (type === 'divider') return out;
  if (type === 'code') {
    out.text = String(b.text == null ? '' : b.text).slice(0, 500_000);
    out.lang = String(b.lang || '').replace(/[^a-zA-Z0-9+#._-]/g, '').slice(0, 24);
    return out;
  }
  if (type === 'table') {
    const rows = Array.isArray(b.rows) ? b.rows.slice(0, TABLE_MAX) : [];
    const cols = Math.min(TABLE_MAX, Math.max(1, ...rows.map(r => (Array.isArray(r) ? r.length : 0))));
    out.rows = (rows.length ? rows : [['']]).map(r => {
      const cells = Array.isArray(r) ? r.slice(0, cols) : [];
      while (cells.length < cols) cells.push('');
      return cells.map(c => sanitizeInline(c));
    });
    out.header = !!b.header;
    return out;
  }
  out.text = sanitizeInline(b.text);
  if (type === 'todo') out.checked = !!b.checked;
  return out;
}

function cleanBlocks(blocks) {
  const list = (Array.isArray(blocks) ? blocks : []).slice(0, MAX_BLOCKS).map(cleanBlock).filter(Boolean);
  return list.length ? list : [{ id: genId(), type: 'p', text: '' }];
}

function readPage(alias, id) {
  if (!validId(id)) return null;
  try { return JSON.parse(readFileSync(fileFor(alias, id), 'utf-8')); } catch { return null; }
}

function writePage(alias, page) {
  const dir = dirFor(alias);
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
  const body = JSON.stringify(page);
  if (body.length > MAX_PAGE_BYTES) return { ok: false, error: 'This page is too large to save (over 2 MB).' };
  const f = fileFor(alias, page.id);
  const tmp = f + '.tmp';
  writeFileSync(tmp, body);
  renameSync(tmp, f);
  return { ok: true };
}

/** Plain-text preview of a page's first lines, for the page list. */
function previewOf(page) {
  const plain = (h) => String(h || '').replace(/<br>/g, ' ').replace(/<[^>]+>/g, '').replace(/&nbsp;/g, ' ')
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&').replace(/\s+/g, ' ').trim();
  const parts = [];
  for (const b of page.blocks || []) {
    const t = b.type === 'code' ? String(b.text || '') : b.type === 'table' ? (b.rows || []).flat().map(plain).join(' ') : plain(b.text);
    if (t) parts.push(t);
    if (parts.join(' ').length > 160) break;
  }
  return parts.join(' · ').slice(0, 160);
}

const summary = (p) => ({
  id: p.id, title: p.title || '', createdAt: p.createdAt, updatedAt: p.updatedAt,
  deleted: !!p.deleted, deletedAt: p.deletedAt || null, preview: previewOf(p), version: p.version || 1,
});

/** Every page of a project (newest first); trash only when asked for. */
export function listPages(alias, { includeDeleted = false } = {}) {
  const dir = dirFor(alias);
  if (!existsSync(dir)) return [];
  const out = [];
  for (const f of readdirSync(dir)) {
    if (!/^[0-9a-f]{16}\.json$/.test(f)) continue;
    const p = readPage(alias, f.slice(0, 16));
    if (!p || (p.deleted && !includeDeleted)) continue;
    out.push(summary(p));
  }
  return out.sort((a, b) => String(b.updatedAt).localeCompare(String(a.updatedAt)));
}

export function getPage(alias, id) {
  const p = readPage(alias, id);
  return p ? { ok: true, page: p } : { ok: false, error: 'Page not found' };
}

export function createPage(alias, { title = '', blocks } = {}) {
  if (!String(alias || '').trim()) return { ok: false, error: 'Project is required' };
  const now = new Date().toISOString();
  const page = {
    id: genId(), title: String(title || '').trim().slice(0, MAX_TITLE),
    blocks: cleanBlocks(blocks), version: 1, createdAt: now, updatedAt: now, deleted: false, deletedAt: null,
  };
  const w = writePage(alias, page);
  return w.ok ? { ok: true, page } : w;
}

/**
 * Save a page's title and/or blocks. baseVersion is the version the client
 * was editing; if the page moved on since, nothing is written and the current
 * page comes back with conflict: true.
 */
export function savePage(alias, id, { title, blocks, baseVersion } = {}) {
  const page = readPage(alias, id);
  if (!page) return { ok: false, error: 'Page not found' };
  if (page.deleted) return { ok: false, error: 'This page is in the trash' };
  if (baseVersion != null && Number(baseVersion) !== (page.version || 1)) {
    return { ok: false, conflict: true, error: 'This page was changed elsewhere', page };
  }
  if (title !== undefined) page.title = String(title || '').trim().slice(0, MAX_TITLE);
  if (blocks !== undefined) page.blocks = cleanBlocks(blocks);
  page.version = (page.version || 1) + 1;
  page.updatedAt = new Date().toISOString();
  const w = writePage(alias, page);
  return w.ok ? { ok: true, version: page.version, updatedAt: page.updatedAt, title: page.title } : w;
}

export function deletePage(alias, id) {
  const page = readPage(alias, id);
  if (!page) return { ok: false, error: 'Page not found' };
  page.deleted = true; page.deletedAt = new Date().toISOString();
  page.version = (page.version || 1) + 1;
  const w = writePage(alias, page);
  return w.ok ? { ok: true } : w;
}

export function restorePage(alias, id) {
  const page = readPage(alias, id);
  if (!page) return { ok: false, error: 'Page not found' };
  page.deleted = false; page.deletedAt = null;
  page.version = (page.version || 1) + 1;
  page.updatedAt = new Date().toISOString();
  const w = writePage(alias, page);
  return w.ok ? { ok: true } : w;
}

/** Remove a trashed page for good. Refuses one that is not in the trash. */
export function purgePage(alias, id) {
  const page = readPage(alias, id);
  if (!page) return { ok: false, error: 'Page not found' };
  if (!page.deleted) return { ok: false, error: 'Move the page to the trash first' };
  try { unlinkSync(fileFor(alias, id)); } catch (err) { return { ok: false, error: err.message }; }
  return { ok: true };
}
