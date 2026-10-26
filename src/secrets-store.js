/**
 * secrets-store.js — Global encrypted secrets store for Crundi.
 *
 * Each secret has a plaintext `name` and `description` (so they can be searched
 * without authentication) and an encrypted `value`. The value is sealed with
 * AES-256-GCM using a key derived (scrypt) from the secret's lock: a
 * passphrase, or (secrets made before 1.21.2) a 6-digit PIN.
 *
 * The lock is NEVER stored. Correctness is proven only by a successful
 * decryption — a wrong one fails the GCM auth tag and is reported as
 * incorrect. There is consequently no recovery: if a secret's lock is lost,
 * the only option is to delete and recreate that single secret.
 *
 * Locks
 *   'passphrase'  at least 8 characters and not digits only. Every new secret,
 *                 and every secret whose lock is changed, uses one.
 *   'pin'         exactly 6 digits. Only secrets made before passphrases; they
 *                 keep opening, and say needsUpgrade until re-locked.
 *
 * Key derivation
 *   Each secret records its own scrypt parameters (kdf: { name, N, r, p }), so
 *   raising the cost never locks an old secret out: it opens with what it was
 *   sealed with until it is sealed again. A record without kdf is the original
 *   N=2^14. New seals use KDF_CURRENT (N=2^17, r=8, p=1: about 0.35 s and
 *   128 MB on the dev server, against 0.035 s before; an offline guess at a
 *   6-digit PIN went from hours to days, and a passphrase is out of reach).
 *   Derivation runs on the libuv pool (async scrypt), so an unlock never
 *   blocks the server.
 *
 * Stored in <dataDir>/secrets.json (0600, written atomically) as:
 *   { secrets: [ { id, name, description, lock, kdf, salt, iv, tag, data, createdAt, updatedAt } ] }
 * where salt/iv/tag/data are base64.
 */

import { readFileSync, existsSync } from 'fs';
import { join } from 'path';
import { randomBytes, scrypt, createCipheriv, createDecipheriv } from 'crypto';
import { config } from './config.js';
import { writePrivateFile } from './private-fs.js';

const FILE = () => join(config.dataDir, 'secrets.json');
const KEY_LEN = 32;          // AES-256

/** The original parameters: every record without a kdf field. */
export const KDF_LEGACY = Object.freeze({ name: 'scrypt', N: 16384, r: 8, p: 1 });
/** What new and re-locked secrets are sealed with. */
export const KDF_CURRENT = Object.freeze({ name: 'scrypt', N: 131072, r: 8, p: 1 });

export const MIN_PASSPHRASE = 8;
const MAX_LOCK = 1024;
export const PASSPHRASE_RULE = `At least ${MIN_PASSPHRASE} characters, and not only digits`;

function load() {
  const file = FILE();
  if (!existsSync(file)) return { secrets: [] };
  try {
    const data = JSON.parse(readFileSync(file, 'utf-8'));
    return { secrets: Array.isArray(data.secrets) ? data.secrets : [] };
  } catch { return { secrets: [] }; }
}

function save(store) {
  writePrivateFile(FILE(), JSON.stringify(store, null, 2));
}

function genId() {
  return randomBytes(8).toString('hex');
}

const lockOf = (s) => (s && s.lock === 'passphrase' ? 'passphrase' : 'pin');

/** The parameters a record was sealed with, checked so a tampered file cannot ask for absurd work. */
function kdfOf(s) {
  const k = s && s.kdf;
  if (!k) return KDF_LEGACY;
  const N = Number(k.N), r = Number(k.r), p = Number(k.p);
  const pow2 = Number.isInteger(N) && N >= 1024 && N <= 2 ** 20 && (N & (N - 1)) === 0;
  if (k.name !== 'scrypt' || !pow2 || !Number.isInteger(r) || r < 1 || r > 32 || !Number.isInteger(p) || p < 1 || p > 4) return null;
  return { name: 'scrypt', N, r, p };
}

/** True when a secret should be re-locked: a 6-digit PIN, or weaker parameters than now. */
function needsUpgradeOf(s) {
  if (lockOf(s) !== 'passphrase') return true;
  const k = kdfOf(s);
  return !k || k.N < KDF_CURRENT.N || k.r < KDF_CURRENT.r;
}

/** Is this a 6-digit PIN? (Only old secrets have one.) */
export function isValidPin(pin) {
  return /^\d{6}$/.test(String(pin ?? ''));
}

/**
 * May this be a NEW lock? At least 8 characters (counted as characters, not
 * bytes) and not digits only.
 * @returns {string} '' when it may, else the reason
 */
export function passphraseProblem(text) {
  const t = typeof text === 'string' ? text : String(text ?? '');
  const n = [...t].length;
  if (n < MIN_PASSPHRASE || /^\d*$/.test(t)) return PASSPHRASE_RULE;
  if (t.length > MAX_LOCK) return `At most ${MAX_LOCK} characters`;
  return '';
}

/** Could `lock` open this secret at all? Cheap checks before the slow ones. */
function lockShapeProblem(s, lock) {
  const t = String(lock ?? '');
  if (lockOf(s) === 'pin') return isValidPin(t) ? '' : 'PIN must be exactly 6 digits';
  return t && t.length <= MAX_LOCK ? '' : 'Enter the passphrase';
}

const wrongLock = (s) => (lockOf(s) === 'pin' ? 'Incorrect PIN' : 'Incorrect passphrase');

function deriveKey(lock, salt, kdf) {
  const { N, r, p } = kdf;
  return new Promise((resolve, reject) => {
    // maxmem: scrypt needs about 128*N*r*p bytes; Node's default cap is 32 MB.
    scrypt(String(lock), salt, KEY_LEN, { N, r, p, maxmem: 128 * N * r * p + 32 * 1024 * 1024 }, (err, key) => (err ? reject(err) : resolve(key)));
  });
}

/** Seal `value` under `lock` with the current parameters: the crypto fields of a record. */
async function seal(value, lock) {
  const salt = randomBytes(16);
  const iv = randomBytes(12);
  const key = await deriveKey(lock, salt, KDF_CURRENT);
  const cipher = createCipheriv('aes-256-gcm', key, iv);
  const enc = Buffer.concat([cipher.update(String(value), 'utf-8'), cipher.final()]);
  return {
    kdf: { ...KDF_CURRENT },
    salt: salt.toString('base64'),
    iv: iv.toString('base64'),
    tag: cipher.getAuthTag().toString('base64'),
    data: enc.toString('base64'),
  };
}

/** Open a record with `lock`. @returns {Promise<string|null>} the value, or null when the lock is wrong */
async function open(s, lock) {
  const kdf = kdfOf(s);
  if (!kdf) return null;
  try {
    const key = await deriveKey(lock, Buffer.from(s.salt, 'base64'), kdf);
    const decipher = createDecipheriv('aes-256-gcm', key, Buffer.from(s.iv, 'base64'));
    decipher.setAuthTag(Buffer.from(s.tag, 'base64'));
    return Buffer.concat([decipher.update(Buffer.from(s.data, 'base64')), decipher.final()]).toString('utf-8');
  } catch {
    // GCM auth failure → wrong lock (or tampered data).
    return null;
  }
}

/**
 * Public-safe view of a secret (no ciphertext / crypto material). `lock` and
 * `needsUpgrade` say how to ask for it and whether to offer re-locking; they
 * say nothing about the lock itself.
 */
function publicView(s) {
  return {
    id: s.id, name: s.name, description: s.description, createdAt: s.createdAt, updatedAt: s.updatedAt,
    lock: lockOf(s), needsUpgrade: needsUpgradeOf(s),
  };
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
 * Create a new secret, encrypting `value` under a passphrase (`pin`, the
 * field's old name, kept so the request stays the same shape).
 * @returns {Promise<{ ok: boolean, id?: string, error?: string }>}
 */
export async function addSecret({ name, description = '', value, pin } = {}) {
  const n = String(name || '').trim();
  if (!n) return { ok: false, error: 'Name is required' };
  if (value === undefined || value === null || value === '') return { ok: false, error: 'Value is required' };
  const bad = passphraseProblem(pin);
  if (bad) return { ok: false, error: `Passphrase: ${bad}.` };

  const sealed = await seal(value, pin);
  const now = new Date().toISOString();
  const secret = { id: genId(), name: n, description: String(description || ''), lock: 'passphrase', ...sealed, createdAt: now, updatedAt: now };
  const store = load();
  store.secrets.push(secret);
  save(store);
  return { ok: true, id: secret.id };
}

/** Update plaintext metadata (name/description). No lock needed — not encrypted. */
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
 * Decrypt and return a secret's value using its lock (a PIN or a passphrase).
 * @returns {Promise<{ ok: boolean, value?: string, name?: string, lock?: string, needsUpgrade?: boolean, error?: string }>}
 */
export async function decryptSecret(id, pin) {
  const s = load().secrets.find(x => x.id === id);
  if (!s) return { ok: false, error: 'Secret not found' };
  const bad = lockShapeProblem(s, pin);
  if (bad) return { ok: false, error: bad };
  const value = await open(s, pin);
  if (value === null) return { ok: false, error: wrongLock(s) };
  return { ok: true, value, name: s.name, lock: lockOf(s), needsUpgrade: needsUpgradeOf(s) };
}

/**
 * Edit a secret, proven by its current lock (`pin`):
 *   newPin       re-lock it with this passphrase (and the current parameters)
 *   value        a new value, under the same lock unless newPin is given
 *   name, description   as updateSecretMeta
 * Anything that touches the sealed part re-seals with KDF_CURRENT. A value
 * change alone keeps a 6-digit PIN a PIN (it still says needsUpgrade).
 * @returns {Promise<{ ok: boolean, error?: string, relocked?: boolean, secret?: object }>}
 */
export async function editSecret(id, { pin, newPin, value, name, description } = {}) {
  const before = load().secrets.find(x => x.id === id);
  if (!before) return { ok: false, error: 'Secret not found' };
  const relock = newPin !== undefined && newPin !== null && newPin !== '';
  const revalue = value !== undefined && value !== null;
  if (revalue && value === '') return { ok: false, error: 'Value is required' };
  if (relock) {
    const bad = passphraseProblem(newPin);
    if (bad) return { ok: false, error: `New passphrase: ${bad}.` };
  }
  const shape = lockShapeProblem(before, pin);
  if (shape) return { ok: false, error: shape };
  const current = await open(before, pin);
  if (current === null) return { ok: false, error: wrongLock(before) };

  const sealed = relock || revalue ? await seal(revalue ? value : current, relock ? newPin : pin) : null;
  // Read again: the store may have changed while the keys were derived.
  const store = load();
  const s = store.secrets.find(x => x.id === id);
  if (!s || s.salt !== before.salt || s.data !== before.data) return { ok: false, error: 'The secret changed meanwhile; try again.' };
  if (sealed) Object.assign(s, sealed, relock ? { lock: 'passphrase' } : {});
  if (name !== undefined && name !== null && String(name).trim()) s.name = String(name).trim();
  if (description !== undefined && description !== null) s.description = String(description);
  s.updatedAt = new Date().toISOString();
  save(store);
  return { ok: true, relocked: relock, secret: publicView(s) };
}

/** Permanently delete a secret. This is the only path for a lost lock. */
export function deleteSecret(id) {
  const store = load();
  const before = store.secrets.length;
  store.secrets = store.secrets.filter(x => x.id !== id);
  if (store.secrets.length === before) return { ok: false, error: 'Secret not found' };
  save(store);
  return { ok: true };
}
