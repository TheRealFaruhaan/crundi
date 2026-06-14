/**
 * secrets-store.js — Global encrypted secrets store for Crundi.
 *
 * Each secret has a plaintext `name` and `description` (so they can be searched
 * without authentication) and an encrypted `value`. The value is sealed with
 * AES-256-GCM using a key derived (scrypt) from a per-secret 6-digit PIN.
 *
 * The PIN is NEVER stored. Correctness is proven only by a successful
 * decryption — a wrong PIN fails the GCM auth tag and is reported as an
 * incorrect PIN. There is consequently no recovery: if a secret's PIN is lost,
 * the only option is to delete and recreate that single secret.
 *
 * Stored in <dataDir>/secrets.json as:
 *   { secrets: [ { id, name, description, salt, iv, tag, data, createdAt, updatedAt } ] }
 * where salt/iv/tag/data are base64.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { join } from 'path';
import { randomBytes, scryptSync, createCipheriv, createDecipheriv } from 'crypto';
import { config } from './config.js';

const FILE = () => join(config.dataDir, 'secrets.json');
const KEY_LEN = 32;          // AES-256
const SCRYPT_PARAMS = { N: 16384, r: 8, p: 1 };

function ensureDataDir() {
  if (!existsSync(config.dataDir)) mkdirSync(config.dataDir, { recursive: true });
}

function load() {
  const file = FILE();
  if (!existsSync(file)) return { secrets: [] };
  try {
    const data = JSON.parse(readFileSync(file, 'utf-8'));
    return { secrets: Array.isArray(data.secrets) ? data.secrets : [] };
  } catch { return { secrets: [] }; }
}

function save(store) {
  ensureDataDir();
  writeFileSync(FILE(), JSON.stringify(store, null, 2));
}

function genId() {
  return randomBytes(8).toString('hex');
}

function isValidPin(pin) {
  return /^\d{6}$/.test(String(pin || ''));
}

function deriveKey(pin, salt) {
  return scryptSync(String(pin), salt, KEY_LEN, SCRYPT_PARAMS);
}

/** Public-safe view of a secret (no ciphertext / crypto material). */
function publicView(s) {
  return { id: s.id, name: s.name, description: s.description, createdAt: s.createdAt, updatedAt: s.updatedAt };
}

// ─── Reads (no auth — metadata only, never the value) ───

export function listSecrets() {
  return load().secrets.map(publicView);
}

/**
 * Search secrets by matching the query against name AND description
 * (case-insensitive substring). Empty query returns everything. Never returns
 * the encrypted value.
 */
export function searchSecrets(query) {
  const q = String(query || '').trim().toLowerCase();
  const all = load().secrets;
  if (!q) return all.map(publicView);
  return all
    .filter(s => (s.name || '').toLowerCase().includes(q) || (s.description || '').toLowerCase().includes(q))
    .map(publicView);
}

export function getSecretMeta(id) {
  const s = load().secrets.find(x => x.id === id);
  return s ? publicView(s) : null;
}

// ─── Writes ───

/**
 * Create a new secret, encrypting `value` with a key derived from `pin`.
 * @returns {{ ok: boolean, id?: string, error?: string }}
 */
export function addSecret({ name, description = '', value, pin } = {}) {
  const n = String(name || '').trim();
  if (!n) return { ok: false, error: 'Name is required' };
  if (value === undefined || value === null || value === '') return { ok: false, error: 'Value is required' };
  if (!isValidPin(pin)) return { ok: false, error: 'PIN must be exactly 6 digits' };

  const salt = randomBytes(16);
  const iv = randomBytes(12);
  const key = deriveKey(pin, salt);
  const cipher = createCipheriv('aes-256-gcm', key, iv);
  const enc = Buffer.concat([cipher.update(String(value), 'utf-8'), cipher.final()]);
  const tag = cipher.getAuthTag();

  const now = new Date().toISOString();
  const secret = {
    id: genId(),
    name: n,
    description: String(description || ''),
    salt: salt.toString('base64'),
    iv: iv.toString('base64'),
    tag: tag.toString('base64'),
    data: enc.toString('base64'),
    createdAt: now,
    updatedAt: now,
  };
  const store = load();
  store.secrets.push(secret);
  save(store);
  return { ok: true, id: secret.id };
}

/** Update plaintext metadata (name/description). No PIN needed — not encrypted. */
export function updateSecretMeta(id, { name, description } = {}) {
  const store = load();
  const s = store.secrets.find(x => x.id === id);
  if (!s) return { ok: false, error: 'Secret not found' };
  if (name !== undefined && String(name).trim()) s.name = String(name).trim();
  if (description !== undefined) s.description = String(description);
  s.updatedAt = new Date().toISOString();
  save(store);
  return { ok: true };
}

/**
 * Decrypt and return a secret's value using `pin`.
 * @returns {{ ok: boolean, value?: string, name?: string, error?: string }}
 */
export function decryptSecret(id, pin) {
  if (!isValidPin(pin)) return { ok: false, error: 'PIN must be exactly 6 digits' };
  const s = load().secrets.find(x => x.id === id);
  if (!s) return { ok: false, error: 'Secret not found' };
  try {
    const salt = Buffer.from(s.salt, 'base64');
    const iv = Buffer.from(s.iv, 'base64');
    const tag = Buffer.from(s.tag, 'base64');
    const data = Buffer.from(s.data, 'base64');
    const key = deriveKey(pin, salt);
    const decipher = createDecipheriv('aes-256-gcm', key, iv);
    decipher.setAuthTag(tag);
    const dec = Buffer.concat([decipher.update(data), decipher.final()]);
    return { ok: true, value: dec.toString('utf-8'), name: s.name };
  } catch {
    // GCM auth failure → wrong PIN (or tampered data).
    return { ok: false, error: 'Incorrect PIN' };
  }
}

/** Permanently delete a secret. This is the only path for a lost PIN. */
export function deleteSecret(id) {
  const store = load();
  const before = store.secrets.length;
  store.secrets = store.secrets.filter(x => x.id !== id);
  if (store.secrets.length === before) return { ok: false, error: 'Secret not found' };
  save(store);
  return { ok: true };
}
