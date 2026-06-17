/**
 * media-store.js — Attached media for Crundi.
 *
 * Media items are files (images, PDFs, video/audio, or any other type) stored
 * on disk under <dataDir>/media/, with metadata in <dataDir>/media.json. Each
 * item may be free-standing (unlinked) or linked to a Kanban task, a Kanban
 * subtask (todo), or a Mindmap node — so you can attach a screenshot/spec to the
 * thing it documents. Items are also scoped to a project (alias) for filtering;
 * unlinked items may be "general" (project = null).
 *
 * The store is dependency-light (pure CRUD + file management). Link liveness
 * (whether the linked task/todo/node still exists) is resolved by the caller
 * (webapp.js) which already has the kanban/mindmap stores — same pattern as
 * enrichBoardWithMindmap.
 *
 * media.json shape:
 *   { items: [ { id, originalName, file, mime, ext, size, kind,
 *                project, link, createdAt } ] }
 *   link: null
 *       | { type: 'task',  taskId }
 *       | { type: 'todo',  taskId, todoId }
 *       | { type: 'node',  nodeId }
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, copyFileSync, rmSync, statSync } from 'fs';
import { join, extname, basename } from 'path';
import { randomBytes } from 'crypto';
import { config } from './config.js';

const FILE = () => join(config.dataDir, 'media.json');
const MEDIA_DIR = () => join(config.dataDir, 'media');

// ─── File-type classification ───
const IMAGE_EXT = ['.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp', '.svg', '.avif', '.ico'];
const VIDEO_EXT = ['.mp4', '.webm', '.ogv', '.mov', '.m4v', '.mkv'];
const AUDIO_EXT = ['.mp3', '.wav', '.ogg', '.oga', '.m4a', '.flac', '.aac'];
const PDF_EXT = ['.pdf'];

const MIME = {
  '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif',
  '.webp': 'image/webp', '.bmp': 'image/bmp', '.svg': 'image/svg+xml', '.avif': 'image/avif',
  '.ico': 'image/x-icon',
  '.pdf': 'application/pdf',
  '.mp4': 'video/mp4', '.webm': 'video/webm', '.ogv': 'video/ogg', '.mov': 'video/quicktime',
  '.m4v': 'video/x-m4v', '.mkv': 'video/x-matroska',
  '.mp3': 'audio/mpeg', '.wav': 'audio/wav', '.ogg': 'audio/ogg', '.oga': 'audio/ogg',
  '.m4a': 'audio/mp4', '.flac': 'audio/flac', '.aac': 'audio/aac',
};

/** Broad category used by the UI: image/pdf/video/audio are previewable; other = download. */
export function kindOf(ext) {
  ext = String(ext || '').toLowerCase();
  if (IMAGE_EXT.includes(ext)) return 'image';
  if (PDF_EXT.includes(ext)) return 'pdf';
  if (VIDEO_EXT.includes(ext)) return 'video';
  if (AUDIO_EXT.includes(ext)) return 'audio';
  return 'other';
}
export function mimeOf(ext) {
  return MIME[String(ext || '').toLowerCase()] || 'application/octet-stream';
}

function ensureDirs() {
  if (!existsSync(config.dataDir)) mkdirSync(config.dataDir, { recursive: true });
  if (!existsSync(MEDIA_DIR())) mkdirSync(MEDIA_DIR(), { recursive: true });
}

function load() {
  const file = FILE();
  if (!existsSync(file)) return { items: [] };
  try {
    const d = JSON.parse(readFileSync(file, 'utf-8'));
    return { items: Array.isArray(d.items) ? d.items : [] };
  } catch { return { items: [] }; }
}

function save(store) {
  ensureDirs();
  writeFileSync(FILE(), JSON.stringify(store, null, 2));
}

function genId() {
  return randomBytes(8).toString('hex');
}

function scopeOf(project) { return project ? String(project).toLowerCase() : null; }

/** Normalise a link descriptor; returns null for an unlinked item. */
function normalizeLink(link) {
  if (!link || typeof link !== 'object') return null;
  if (link.type === 'task' && link.taskId) return { type: 'task', taskId: String(link.taskId) };
  if (link.type === 'todo' && link.taskId && link.todoId) return { type: 'todo', taskId: String(link.taskId), todoId: String(link.todoId) };
  if (link.type === 'node' && link.nodeId) return { type: 'node', nodeId: String(link.nodeId) };
  return null;
}

function linkMatches(item, filter) {
  if (!filter) return true;
  const l = item.link;
  // Filter by broad kind: 'kanban' = task or todo, 'mindmap' = node.
  if (filter.kind === 'kanban') return !!l && (l.type === 'task' || l.type === 'todo');
  if (filter.kind === 'mindmap') return !!l && l.type === 'node';
  if (filter.kind === 'unlinked') return !l;
  // Filter by an exact target.
  if (filter.type === 'task') return !!l && l.type === 'task' && l.taskId === String(filter.taskId);
  if (filter.type === 'todo') return !!l && l.type === 'todo' && l.taskId === String(filter.taskId) && l.todoId === String(filter.todoId);
  if (filter.type === 'node') return !!l && l.type === 'node' && l.nodeId === String(filter.nodeId);
  return true;
}

// ─── Reads ───

/**
 * List media, newest first. Options:
 *   project   — only this alias (null/undefined = all projects).
 *   includeGeneral — when filtering by project, also include unlinked general
 *                    items (project = null). Default false.
 *   linkFilter — { kind:'kanban'|'mindmap'|'unlinked' } or an exact target
 *                { type:'task'|'todo'|'node', ... }.
 */
export function listMedia({ project = null, includeGeneral = false, linkFilter = null } = {}) {
  const store = load();
  const p = scopeOf(project);
  let items = store.items.slice();
  if (p) items = items.filter(it => it.project === p || (includeGeneral && !it.project));
  if (linkFilter) items = items.filter(it => linkMatches(it, linkFilter));
  items.sort((a, b) => String(b.createdAt || '').localeCompare(String(a.createdAt || '')));
  return items;
}

export function getMedia(id) {
  return load().items.find(it => it.id === id) || null;
}

/** Absolute on-disk path of an item's file (or null). */
export function mediaFilePath(item) {
  if (!item || !item.file) return null;
  return join(MEDIA_DIR(), item.file);
}

// ─── Writes ───

function makeItem({ originalName, ext, mime, size, project, link }) {
  const id = genId();
  const cleanExt = ext && /^\.[a-z0-9]+$/i.test(ext) ? ext.toLowerCase() : '';
  return {
    id,
    originalName: String(originalName || 'file'),
    file: id + cleanExt,
    mime: mime || mimeOf(cleanExt),
    ext: cleanExt,
    size: Number(size) || 0,
    kind: kindOf(cleanExt),
    project: scopeOf(project),
    link: normalizeLink(link),
    createdAt: new Date().toISOString(),
  };
}

/** Add media from an in-memory buffer (used by web uploads). */
export function addMediaFromBuffer({ name, buffer, project = null, link = null } = {}) {
  if (!buffer || !buffer.length) return { ok: false, error: 'Empty file' };
  const original = basename(String(name || 'file'));
  const ext = extname(original);
  const store = load();
  const item = makeItem({ originalName: original, ext, size: buffer.length, project, link });
  ensureDirs();
  try {
    writeFileSync(join(MEDIA_DIR(), item.file), buffer);
  } catch (err) { return { ok: false, error: err.message }; }
  store.items.push(item);
  save(store);
  return { ok: true, item };
}

/** Add media by copying a file that already exists on disk (used by MCP). */
export function addMediaFromPath({ path, name = null, project = null, link = null } = {}) {
  const src = String(path || '');
  if (!src || !existsSync(src)) return { ok: false, error: 'Source file not found: ' + src };
  let size = 0;
  try { const st = statSync(src); if (!st.isFile()) return { ok: false, error: 'Not a file: ' + src }; size = st.size; }
  catch (err) { return { ok: false, error: err.message }; }
  const original = basename(name ? String(name) : src);
  const ext = extname(original) || extname(src);
  const store = load();
  const item = makeItem({ originalName: original, ext, size, project, link });
  ensureDirs();
  try {
    copyFileSync(src, join(MEDIA_DIR(), item.file));
  } catch (err) { return { ok: false, error: err.message }; }
  store.items.push(item);
  save(store);
  return { ok: true, item };
}

export function deleteMedia(id) {
  const store = load();
  const idx = store.items.findIndex(it => it.id === id);
  if (idx < 0) return { ok: false, error: 'Media not found' };
  const [item] = store.items.splice(idx, 1);
  try { const fp = join(MEDIA_DIR(), item.file); if (existsSync(fp)) rmSync(fp, { force: true }); } catch { /* ignore */ }
  save(store);
  return { ok: true, item };
}
