/**
 * Persistent service registry.
 *
 * Users explicitly register services — no auto-discovery — with a human name,
 * working directory, and start command. Stored in <dataDir>/services.json.
 *
 * Layout on disk:
 *   {
 *     "<alias>:<name>": {
 *       key, alias, name, cwd, command, stopCommand, tunnelPort, createdAt
 *     }
 *   }
 *
 * The key is also what src/services.js uses as the running-map key, so there's
 * one identifier for registered + live state.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { join } from 'path';
import { config } from './config.js';

const FILE = () => join(config.dataDir, 'services.json');

function ensureDataDir() {
  if (!existsSync(config.dataDir)) mkdirSync(config.dataDir, { recursive: true });
}

function loadAll() {
  if (!existsSync(FILE())) return {};
  try { return JSON.parse(readFileSync(FILE(), 'utf-8')) || {}; } catch { return {}; }
}

function saveAll(map) {
  ensureDataDir();
  writeFileSync(FILE(), JSON.stringify(map, null, 2));
}

export function makeKey(alias, name) {
  return `${String(alias).toLowerCase()}:${String(name).trim()}`;
}

export function listAllRegistered() {
  return Object.values(loadAll());
}

export function listRegisteredForProject(alias) {
  const a = String(alias).toLowerCase();
  return Object.values(loadAll()).filter(s => s.alias === a);
}

export function getRegistered(key) {
  return loadAll()[key] || null;
}

/**
 * Register a new service.
 * @param {{ alias: string, name: string, cwd: string, command: string, stopCommand?: string, tunnelPort?: number }} entry
 * @returns {{ ok: boolean, key?: string, error?: string }}
 */
export function registerService({ alias, name, cwd, command, stopCommand = '', tunnelPort = 0, tunnelEnabled }) {
  const a = String(alias || '').toLowerCase().trim();
  const n = String(name || '').trim();
  const c = String(cwd || '').trim();
  const cmd = String(command || '').trim();
  if (!a) return { ok: false, error: 'Project alias is required' };
  if (!n) return { ok: false, error: 'Service name is required' };
  if (!/^[\w .-]+$/.test(n)) return { ok: false, error: 'Name may only contain letters, numbers, spaces, dot, dash, underscore' };
  if (!c) return { ok: false, error: 'Working directory is required' };
  if (!cmd) return { ok: false, error: 'Start command is required' };

  const map = loadAll();
  const key = makeKey(a, n);
  if (map[key]) return { ok: false, error: `A service named "${n}" is already registered for ${a}` };

  map[key] = {
    key,
    alias: a,
    name: n,
    cwd: c,
    command: cmd,
    stopCommand: stopCommand ? String(stopCommand).trim() : '',
    tunnelPort: Number(tunnelPort) || 0,
    // Tunnel on/off is separate from the port. Default: on when a port was given.
    tunnelEnabled: tunnelEnabled !== undefined ? !!tunnelEnabled : (Number(tunnelPort) || 0) > 0,
    createdAt: new Date().toISOString(),
  };
  saveAll(map);
  return { ok: true, key };
}

/**
 * Update fields on an existing registration (e.g. tunnelPort).
 * Only supplied keys are overwritten; the rest stay as-is.
 * @param {string} key
 * @param {Partial<{ cwd: string, command: string, stopCommand: string, tunnelPort: number }>} updates
 */
export function updateRegistered(key, updates) {
  const map = loadAll();
  if (!map[key]) return { ok: false, error: 'Not registered' };
  if (updates.tunnelPort !== undefined) map[key].tunnelPort = Number(updates.tunnelPort) || 0;
  if (updates.tunnelEnabled !== undefined) map[key].tunnelEnabled = !!updates.tunnelEnabled;
  if (updates.command !== undefined) map[key].command = String(updates.command).trim();
  if (updates.stopCommand !== undefined) map[key].stopCommand = String(updates.stopCommand).trim();
  if (updates.cwd !== undefined) map[key].cwd = String(updates.cwd).trim();
  saveAll(map);
  return { ok: true };
}

/**
 * Delete a registered service.
 * The caller (services.js) must ensure it isn't running before invoking this —
 * this module doesn't know about the live process map.
 */
export function deleteRegistered(key) {
  const map = loadAll();
  if (!map[key]) return { ok: false, error: 'Not registered' };
  delete map[key];
  saveAll(map);
  return { ok: true };
}

/**
 * Drop every registered service for a project alias (used when a project itself
 * is removed). Returns the number of entries removed.
 */
export function deleteRegisteredForProject(alias) {
  const a = String(alias).toLowerCase();
  const map = loadAll();
  let removed = 0;
  for (const [k, v] of Object.entries(map)) {
    if (v.alias === a) { delete map[k]; removed++; }
  }
  if (removed) saveAll(map);
  return removed;
}
