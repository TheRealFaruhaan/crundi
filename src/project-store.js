/**
 * project-store.js — Simple project registry for Crundi.
 *
 * Projects are stored in data/projects.json as { alias: { path, name } }.
 * Also supports auto-discovery from PROJECTS_DIR.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, statSync, copyFileSync } from 'fs';
import { join } from 'path';
import { config } from './config.js';

const FILE = () => join(config.dataDir, 'projects.json');

function ensureDataDir() {
  if (!existsSync(config.dataDir)) mkdirSync(config.dataDir, { recursive: true });
}

function load() {
  const file = FILE();
  if (!existsSync(file)) return {};
  try { return JSON.parse(readFileSync(file, 'utf-8')); } catch { return {}; }
}

function save(map) {
  ensureDataDir();
  writeFileSync(FILE(), JSON.stringify(map, null, 2));
}

/** Normalize entry — old format was just a string path. */
function normalize(alias, entry) {
  if (typeof entry === 'string') return { alias, path: entry, name: alias };
  return { alias, path: entry.path, name: entry.name || alias };
}

/**
 * List all projects as an array of { alias, path, name }.
 * Merges registered projects with auto-discovered ones from PROJECTS_DIR.
 */
export function listProjects() {
  const registered = load();
  const result = {};

  // Auto-discovered projects from PROJECTS_DIR
  if (config.projectsDir && existsSync(config.projectsDir)) {
    try {
      for (const entry of readdirSync(config.projectsDir)) {
        const fullPath = join(config.projectsDir, entry);
        if (statSync(fullPath).isDirectory()) {
          const alias = entry.toLowerCase();
          result[alias] = { alias, path: fullPath, name: entry };
        }
      }
    } catch { /* skip */ }
  }

  // Registered projects override auto-discovered ones
  for (const [alias, entry] of Object.entries(registered)) {
    result[alias] = normalize(alias, entry);
  }

  return Object.values(result);
}

/** Get a single project by alias. Returns { alias, path, name } or null. */
export function getProject(alias) {
  const key = String(alias).toLowerCase();

  // Check registered first
  const map = load();
  if (map[key]) return normalize(key, map[key]);

  // Check auto-discovered
  if (config.projectsDir && existsSync(config.projectsDir)) {
    try {
      for (const entry of readdirSync(config.projectsDir)) {
        if (entry.toLowerCase() === key) {
          const fullPath = join(config.projectsDir, entry);
          if (statSync(fullPath).isDirectory()) {
            return { alias: key, path: fullPath, name: entry };
          }
        }
      }
    } catch { /* skip */ }
  }

  return null;
}

/**
 * Get project mode configuration.
 * @returns {{ mode: 'single'|'multi', projectsDir: string }}
 */
export function getProjectMode() {
  return { mode: config.projectMode, projectsDir: config.projectsDir };
}

/**
 * Register or update a project.
 * @param {string} alias
 * @param {string} path
 * @param {string} [name]
 * @param {{ create?: boolean }} [opts] — if create is true and path doesn't exist, mkdir it
 * @returns {{ ok: boolean, error?: string }}
 */
export function registerProject(alias, path, name, opts = {}) {
  const key = String(alias).toLowerCase();
  if (!key) return { ok: false, error: 'Alias is required' };
  if (!path) return { ok: false, error: 'Path is required' };
  if (opts.create && !existsSync(path)) {
    try {
      mkdirSync(path, { recursive: true });
    } catch (err) {
      return { ok: false, error: 'Failed to create directory: ' + err.message };
    }
  }
  const map = load();
  map[key] = { path, name: name || key };
  save(map);
  return { ok: true };
}

/** Remove a project by alias. */
export function removeProject(alias) {
  const key = String(alias).toLowerCase();
  const map = load();
  if (!map[key]) return { ok: false, error: 'Project not found' };
  delete map[key];
  save(map);
  return { ok: true };
}

/**
 * Import projects from old app data.
 * Old format: { "alias": "C:\\path" } or { "alias": { path, name } }
 */
export function importFromOldData(oldDataDir) {
  const oldFile = join(oldDataDir, 'data', 'projects.json');
  if (!existsSync(oldFile)) return { ok: false, error: 'No projects.json in old data' };
  try {
    const old = JSON.parse(readFileSync(oldFile, 'utf-8'));
    const map = load();
    let imported = 0;
    for (const [alias, entry] of Object.entries(old)) {
      if (map[alias]) continue; // don't overwrite existing
      if (typeof entry === 'string') {
        map[alias] = { path: entry, name: alias };
      } else if (entry?.path) {
        map[alias] = { path: entry.path, name: entry.name || alias };
      } else continue;
      imported++;
    }
    if (imported > 0) save(map);
    return { ok: true, imported };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}

/**
 * Import services from old app data.
 */
export function importServicesFromOldData(oldDataDir) {
  const oldFile = join(oldDataDir, 'data', 'services.json');
  const newFile = join(config.dataDir, 'services.json');
  if (!existsSync(oldFile)) return { ok: false, error: 'No services.json in old data' };
  if (existsSync(newFile)) return { ok: false, error: 'services.json already exists' };
  try {
    ensureDataDir();
    copyFileSync(oldFile, newFile);
    return { ok: true };
  } catch (err) {
    return { ok: false, error: err.message };
  }
}
