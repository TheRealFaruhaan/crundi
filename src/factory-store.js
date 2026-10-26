/**
 * factory-store.js — the factory library on disk.
 *
 * Factories are global (not project scoped). Each lives in its own file,
 *   <dataDir>/factories/<id>.json
 * so a bad write damages one factory, not the library, and two edits to
 * different factories never race on the same file.
 *
 * Saving never refuses an invalid factory: it is kept as a draft, and the
 * validation result comes back with it so the editor can draw the badges.
 * Only a valid factory can be started (the runner calls validateFactory again
 * on its snapshot at start, so this file is not the safety boundary).
 *
 * Every save bumps `version`; a save carrying a stale baseVersion is refused
 * with conflict:true, so two open editors cannot silently overwrite each other.
 * Runs snapshot the factory at start, so edits never reach a running factory.
 *
 * Deleting moves the file into factories/.trash/ — nothing is destroyed here.
 *
 * Spec: specs/factory.md §5.1, §6.
 */

import { normalizeTemplate } from './factory-cr.js';
import { readFileSync, writeFileSync, mkdirSync, existsSync, renameSync, readdirSync } from 'fs';
import { join } from 'path';
import { config } from './config.js';
import { normalizeFactory, validateFactory, skeletonFactory, genId, needsLayout, layoutFactory, normalizeSecretIds } from './factory-model.js';

export const EXPORT_FORMAT = 'crundi-factory';
export const EXPORT_FORMAT_VERSION = 1;

const DIR = () => join(config.dataDir, 'factories');
const TRASH = () => join(DIR(), '.trash');
const ID_RE = /^[A-Za-z0-9_-]{1,64}$/;
const fileOf = id => join(DIR(), id + '.json');

function ensureDir(d) { if (!existsSync(d)) mkdirSync(d, { recursive: true }); }

function readOne(id) {
  if (!ID_RE.test(String(id || ''))) return null;
  const p = fileOf(id);
  if (!existsSync(p)) return null;
  try { return normalizeFactory({ ...JSON.parse(readFileSync(p, 'utf-8')), id }); } catch { return null; }
}

function writeOne(f) {
  ensureDir(DIR());
  const tmp = fileOf(f.id) + '.tmp';
  writeFileSync(tmp, JSON.stringify(f, null, 2), { mode: 0o600 });
  renameSync(tmp, fileOf(f.id));
}

function summary(f) {
  const v = validateFactory(f);
  return {
    id: f.id,
    name: f.name,
    description: f.description,
    version: f.version,
    createdAt: f.createdAt,
    updatedAt: f.updatedAt,
    nodeCount: f.nodes.length,
    employeeCount: f.roster.length,
    runnable: v.ok,
    errorCount: v.errors.length,
    warningCount: v.warnings.length,
    crTemplate: normalizeTemplate(f.settings && f.settings.crTemplate),
    // The secrets a run may use (ids): the start form asks their PINs.
    secrets: normalizeSecretIds(f.settings && f.settings.secrets),
  };
}

// ─── Reads ───

/** Every factory, newest first, as summaries (no graph). */
export function listFactories() {
  if (!existsSync(DIR())) return [];
  const out = [];
  for (const name of readdirSync(DIR())) {
    if (!name.endsWith('.json')) continue;
    const f = readOne(name.slice(0, -5));
    if (f) out.push(summary(f));
  }
  return out.sort((a, b) => String(b.updatedAt).localeCompare(String(a.updatedAt)));
}

/** One factory with its validation, or null. */
export function getFactory(id) {
  const f = readOne(id);
  return f ? { factory: f, validation: validateFactory(f) } : null;
}

// ─── Writes ───

function stamp(f, { fresh = false } = {}) {
  const now = new Date().toISOString();
  f.version = fresh ? 1 : f.version + 1;
  if (fresh || !f.createdAt) f.createdAt = now;
  f.updatedAt = now;
  return f;
}

/** A new factory: the minimal valid skeleton, or a given definition. */
export function createFactory({ name, definition } = {}) {
  // Written as text (a chat, an import without positions): laid out here.
  const base = definition ? normalizeFactory(needsLayout(definition) ? layoutFactory(definition) : definition) : skeletonFactory(name || 'New factory');
  if (name) base.name = String(name).slice(0, 80).trim() || base.name;
  base.id = genId('f_');
  stamp(base, { fresh: true });
  writeOne(base);
  return { ok: true, factory: base, validation: validateFactory(base) };
}

/**
 * Save a whole definition over an existing factory.
 * @param {string} id
 * @param {object} definition
 * @param {{ baseVersion?: number }} [opts]  the version the editor loaded; refused if stale
 */
export function saveFactory(id, definition, { baseVersion } = {}) {
  const cur = readOne(id);
  if (!cur) return { ok: false, error: 'Factory not found', notFound: true };
  if (baseVersion != null && Number(baseVersion) !== cur.version) {
    return { ok: false, conflict: true, error: 'This factory was changed somewhere else. Reload it to see the latest version.', version: cur.version };
  }
  const laid = needsLayout(definition) ? layoutFactory(definition) : definition;
  const next = normalizeFactory({ ...laid, id: cur.id, createdAt: cur.createdAt, version: cur.version });
  stamp(next);
  writeOne(next);
  return { ok: true, factory: next, validation: validateFactory(next) };
}

/** Copy a factory under a new id and name. */
export function duplicateFactory(id, { name } = {}) {
  const cur = readOne(id);
  if (!cur) return { ok: false, error: 'Factory not found', notFound: true };
  return createFactory({ name: name || `${cur.name} (copy)`, definition: { ...cur, id: '' } });
}

/** Move a factory to the trash. Runs that already started keep their snapshot. */
export function deleteFactory(id) {
  if (!readOne(id)) return { ok: false, error: 'Factory not found', notFound: true };
  ensureDir(TRASH());
  renameSync(fileOf(id), join(TRASH(), `${id}.${Date.now()}.json`));
  return { ok: true };
}

// ─── Import / export ───

/** A portable copy: no id or timestamps, wrapped with a format marker. */
export function exportFactory(id) {
  const cur = readOne(id);
  if (!cur) return { ok: false, error: 'Factory not found', notFound: true };
  const { id: _i, version: _v, createdAt: _c, updatedAt: _u, ...rest } = cur;
  return { ok: true, data: { format: EXPORT_FORMAT, formatVersion: EXPORT_FORMAT_VERSION, factory: rest } };
}

/**
 * Import an exported factory (object or JSON text) as a new factory. A bare
 * definition without the wrapper is accepted too. It is saved even if invalid
 * (as a draft), with its validation returned.
 */
export function importFactory(input) {
  let data = input;
  if (typeof data === 'string') {
    try { data = JSON.parse(data); } catch { return { ok: false, error: 'That is not valid JSON.' }; }
  }
  if (!data || typeof data !== 'object') return { ok: false, error: 'Nothing to import.' };
  if (data.format != null) {
    if (data.format !== EXPORT_FORMAT) return { ok: false, error: 'That file is not a Crundi factory.' };
    if (Number(data.formatVersion) > EXPORT_FORMAT_VERSION) return { ok: false, error: 'That factory was exported by a newer Crundi. Update to import it.' };
    data = data.factory;
  }
  if (!data || typeof data !== 'object' || !Array.isArray(data.nodes)) return { ok: false, error: 'That file has no factory in it.' };
  return createFactory({ definition: { ...data, id: '' } });
}
