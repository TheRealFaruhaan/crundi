/**
 * auth-password.js — password + TOTP login.
 *
 * The second way in, for installs where Telegram is not wanted or not possible
 * — a container on a private network, say. Telegram remains the default; this
 * exists so that choosing not to use it does not mean having no login at all.
 *
 * Password: scrypt with a per-install random salt. Compared in constant time.
 * Second factor: TOTP (RFC 6238), 6 digits on a 30-second step, verified
 * against the neighbouring windows so a slightly wrong clock still works.
 *
 * A password alone is deliberately not enough. This server can run arbitrary
 * code on the machine it sits on, so a single guessable secret between the
 * internet and a shell is not a trade worth offering.
 *
 * Credentials live in the environment (CRUNDI_PASSWORD_HASH, CRUNDI_TOTP_SECRET)
 * rather than in the data directory: the data directory is a volume you might
 * copy between machines, and copying a login with it is rarely what you meant.
 */

import { createHmac, randomBytes, scryptSync, timingSafeEqual } from 'crypto';

const SCRYPT_N = 16384;   // ~100ms on a modern CPU; enough to make guessing dear
const SCRYPT_r = 8;
const SCRYPT_p = 1;
const KEY_LEN = 32;
const TOTP_STEP_S = 30;
const TOTP_DIGITS = 6;
// One step either side. Enough for ordinary clock drift, and no more: each extra
// window is another valid code an attacker gets to guess.
const TOTP_WINDOW = 1;

// ─── Password ───

/** Hash a password for storage. Returns `scrypt$<salt-hex>$<key-hex>`. */
export function hashPassword(password) {
  const salt = randomBytes(16);
  const key = scryptSync(String(password), salt, KEY_LEN, { N: SCRYPT_N, r: SCRYPT_r, p: SCRYPT_p });
  return `scrypt$${salt.toString('hex')}$${key.toString('hex')}`;
}

/** Constant-time check of a password against a stored hash. */
export function verifyPassword(password, stored) {
  try {
    const parts = String(stored || '').split('$');
    if (parts.length !== 3 || parts[0] !== 'scrypt') return false;
    const salt = Buffer.from(parts[1], 'hex');
    const expected = Buffer.from(parts[2], 'hex');
    if (!salt.length || expected.length !== KEY_LEN) return false;
    const actual = scryptSync(String(password ?? ''), salt, KEY_LEN, { N: SCRYPT_N, r: SCRYPT_r, p: SCRYPT_p });
    return timingSafeEqual(actual, expected);
  } catch { return false; }
}

// ─── TOTP ───

const B32 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

/** A fresh base32 secret, the length authenticator apps expect. */
export function generateTotpSecret(bytes = 20) {
  const buf = randomBytes(bytes);
  let bits = 0, value = 0, out = '';
  for (const byte of buf) {
    value = (value << 8) | byte;
    bits += 8;
    while (bits >= 5) { out += B32[(value >>> (bits - 5)) & 31]; bits -= 5; }
  }
  if (bits > 0) out += B32[(value << (5 - bits)) & 31];
  return out;
}

function base32Decode(secret) {
  const clean = String(secret || '').toUpperCase().replace(/=+$/, '').replace(/\s+/g, '');
  let bits = 0, value = 0;
  const out = [];
  for (const ch of clean) {
    const idx = B32.indexOf(ch);
    if (idx === -1) throw new Error('Invalid base32 in the TOTP secret');
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) { out.push((value >>> (bits - 8)) & 0xff); bits -= 8; }
  }
  return Buffer.from(out);
}

/** The code for a given counter — HOTP, which TOTP is a time-based case of. */
function hotp(keyBuf, counter) {
  const buf = Buffer.alloc(8);
  // Counter is 64-bit big-endian. Written as two 32-bit halves because a plain
  // writeUInt32BE pair avoids BigInt on a hot-ish path and is exactly as correct
  // for any counter that fits in 53 bits (year 292473178 at a 30s step).
  buf.writeUInt32BE(Math.floor(counter / 0x100000000), 0);
  buf.writeUInt32BE(counter >>> 0, 4);
  const mac = createHmac('sha1', keyBuf).update(buf).digest();
  const offset = mac[mac.length - 1] & 0x0f;
  const bin = ((mac[offset] & 0x7f) << 24) | ((mac[offset + 1] & 0xff) << 16)
            | ((mac[offset + 2] & 0xff) << 8) | (mac[offset + 3] & 0xff);
  return String(bin % 10 ** TOTP_DIGITS).padStart(TOTP_DIGITS, '0');
}

/** The code valid right now, for a given secret. */
export function totpNow(secret, atMs = Date.now()) {
  return hotp(base32Decode(secret), Math.floor(atMs / 1000 / TOTP_STEP_S));
}

/** Check a submitted code, allowing one step of clock drift either way. */
export function verifyTotp(secret, code, atMs = Date.now()) {
  const given = String(code ?? '').replace(/\s+/g, '');
  if (!/^\d{6}$/.test(given)) return false;
  let key;
  try { key = base32Decode(secret); } catch { return false; }
  if (!key.length) return false;
  const counter = Math.floor(atMs / 1000 / TOTP_STEP_S);
  for (let d = -TOTP_WINDOW; d <= TOTP_WINDOW; d++) {
    const expected = hotp(key, counter + d);
    // Same length by construction, so timingSafeEqual is safe to call directly.
    if (timingSafeEqual(Buffer.from(expected), Buffer.from(given))) return true;
  }
  return false;
}

/** otpauth:// URI for an authenticator app (render as a QR code). */
export function totpUri(secret, { issuer = 'Crundi', account = 'crundi' } = {}) {
  const label = encodeURIComponent(`${issuer}:${account}`);
  const params = new URLSearchParams({
    secret, issuer, algorithm: 'SHA1', digits: String(TOTP_DIGITS), period: String(TOTP_STEP_S),
  });
  return `otpauth://totp/${label}?${params.toString()}`;
}

// Exported for the HOTP test vectors in RFC 4226, which is the only way to know
// this implementation is right rather than merely self-consistent.
export const __testing = { hotp, base32Decode };
