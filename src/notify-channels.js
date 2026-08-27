/**
 * notify-channels.js — where notifications go.
 *
 * Separate from auth-config.js on purpose. Telegram happens to be both a way in
 * and a way to be told about things, but those are different questions: you
 * might sign in with a password and still want Telegram alerts, or sign in with
 * Telegram and prefer push to your browser.
 *
 * A channel is anything that can deliver a line of text. Registering one is
 * enough to make it appear in Settings and start receiving events, so adding
 * the next one is a matter of writing its send() rather than touching the
 * notification call sites.
 *
 * Unlike sign-in methods, having NO channel enabled is perfectly reasonable —
 * it means "don't tell me". So there is no last-one-standing guard here.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, renameSync } from 'fs';
import { join } from 'path';
import { config } from './config.js';

const FILE = () => join(config.dataDir, 'channels.json');

// id → { id, label, describe(), available(), send(text), enabledByDefault }
const registry = new Map();
let store = null;

function load() {
  if (store) return store;
  try {
    if (existsSync(FILE())) {
      const d = JSON.parse(readFileSync(FILE(), 'utf-8'));
      if (d && typeof d === 'object') { store = d; return store; }
    }
  } catch { /* defaults */ }
  store = {};
  return store;
}

function save() {
  try {
    mkdirSync(config.dataDir, { recursive: true });
    const tmp = FILE() + '.tmp';
    writeFileSync(tmp, JSON.stringify(store, null, 2));
    renameSync(tmp, FILE());
    return true;
  } catch (err) {
    console.error('[channels] Could not save:', err.message);
    return false;
  }
}

/** Make a channel known to Settings and to send(). */
export function register(channel) {
  if (!channel?.id || typeof channel.send !== 'function') return;
  registry.set(channel.id, channel);
}

export function isEnabled(id) {
  const ch = registry.get(id);
  if (!ch) return false;
  if (ch.available && !ch.available()) return false;
  const s = load();
  const saved = s[id]?.enabled;
  // Unset means "follow the channel's default", so an upgrade that adds a
  // channel does not silently start sending through it.
  return saved === undefined ? !!ch.enabledByDefault : !!saved;
}

export function setEnabled(id, enabled) {
  const ch = registry.get(id);
  if (!ch) return { ok: false, error: `Unknown channel "${id}"` };
  if (enabled && ch.available && !ch.available()) {
    return { ok: false, error: ch.unavailableReason?.() || 'That channel is not set up yet' };
  }
  const s = load();
  s[id] = { ...(s[id] || {}), enabled: !!enabled };
  if (!save()) return { ok: false, error: 'Could not save the channel settings' };
  return { ok: true };
}

/** Everything Settings needs to draw the list. */
export function list() {
  return [...registry.values()].map(ch => ({
    id: ch.id,
    label: ch.label || ch.id,
    describe: ch.describe ? ch.describe() : '',
    available: ch.available ? !!ch.available() : true,
    unavailableReason: ch.available && !ch.available() ? (ch.unavailableReason?.() || '') : '',
    enabled: isEnabled(ch.id),
  }));
}

/**
 * Deliver to every enabled channel.
 *
 * One channel failing must not stop the others: a dead Telegram token should
 * not silently cost you your browser notifications too.
 */
export async function deliver(text, meta = {}) {
  const results = [];
  for (const ch of registry.values()) {
    if (!isEnabled(ch.id)) continue;
    try {
      const r = await ch.send(text, meta);
      results.push({ id: ch.id, ok: r !== false });
    } catch (err) {
      console.warn(`[channels] ${ch.id} failed: ${err.message}`);
      results.push({ id: ch.id, ok: false, error: err.message });
    }
  }
  return results;
}

/** True if anything at all would receive a notification right now. */
export function anyEnabled() {
  return [...registry.keys()].some(isEnabled);
}

export function _reset() { store = null; registry.clear(); }
