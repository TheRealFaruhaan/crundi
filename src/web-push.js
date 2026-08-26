/**
 * web-push.js — browser push notifications (RFC 8030 / 8291 / 8292).
 *
 * Works in an ordinary browser tab and in the desktop app alike, because both
 * are Chromium with a service worker — so this is one implementation covering
 * "notify me" everywhere except Telegram.
 *
 * Written against node:crypto rather than pulling in the `web-push` package.
 * The pieces are small and well specified — ECDH on P-256, HKDF, AES-128-GCM,
 * an ES256 JWT — and this avoids a dependency in the trusted path of a server
 * that already runs shell commands.
 *
 * Two layers, easy to conflate:
 *   VAPID   (RFC 8292) identifies THIS SERVER to the push service. One keypair
 *           per install, generated once and kept in the data directory.
 *   aes128gcm (RFC 8291) encrypts the payload to THAT SUBSCRIPTION, so the push
 *           service relays bytes it cannot read.
 */

import {
  createECDH, createHmac, createCipheriv, randomBytes,
  createSign, createPublicKey, createPrivateKey, generateKeyPairSync,
} from 'crypto';
import { readFileSync, writeFileSync, mkdirSync, existsSync, renameSync } from 'fs';
import { join } from 'path';
import { config } from './config.js';

const FILE = () => join(config.dataDir, 'web-push.json');
const TTL_S = 4 * 60 * 60;          // how long the push service should hold it
const MAX_PAYLOAD = 3800;           // 4096 minus padding and headers

// ─── base64url ───
const b64u = (buf) => Buffer.from(buf).toString('base64')
  .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const unb64u = (str) => Buffer.from(String(str).replace(/-/g, '+').replace(/_/g, '/'), 'base64');

// ─── Store: VAPID keypair + subscriptions ───

let store = null;

function load() {
  if (store) return store;
  try {
    if (existsSync(FILE())) {
      const d = JSON.parse(readFileSync(FILE(), 'utf-8'));
      if (d && d.vapid) { store = d; return store; }
    }
  } catch { /* regenerate below */ }
  store = { vapid: generateVapidKeys(), subscriptions: [] };
  save();
  return store;
}

function save() {
  try {
    mkdirSync(config.dataDir, { recursive: true });
    const tmp = FILE() + '.tmp';
    writeFileSync(tmp, JSON.stringify(store, null, 2), { mode: 0o600 });
    renameSync(tmp, FILE());
  } catch (err) {
    console.error('[web-push] Could not save:', err.message);
  }
}

function generateVapidKeys() {
  const { publicKey, privateKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
  const pubRaw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-65);   // uncompressed point
  return {
    publicKey: b64u(pubRaw),
    privateKeyPem: privateKey.export({ type: 'pkcs8', format: 'pem' }),
  };
}

/** The key a browser needs to subscribe. Safe to hand out; it is public. */
export function publicKey() { return load().vapid.publicKey; }

export function subscriptions() { return load().subscriptions; }

export function addSubscription(sub) {
  if (!sub?.endpoint || !sub?.keys?.p256dh || !sub?.keys?.auth) {
    return { ok: false, error: 'That is not a usable push subscription' };
  }
  const s = load();
  // Endpoints are unique per browser+install, so this doubles as the identity.
  const existing = s.subscriptions.findIndex(x => x.endpoint === sub.endpoint);
  const record = { endpoint: sub.endpoint, keys: sub.keys, addedAt: Date.now(), label: sub.label || '' };
  if (existing >= 0) s.subscriptions[existing] = record;
  else s.subscriptions.push(record);
  save();
  return { ok: true, count: s.subscriptions.length };
}

export function removeSubscription(endpoint) {
  const s = load();
  const before = s.subscriptions.length;
  s.subscriptions = s.subscriptions.filter(x => x.endpoint !== endpoint);
  if (s.subscriptions.length !== before) save();
  return { ok: true, removed: before - s.subscriptions.length };
}

// ─── RFC 8291 payload encryption (aes128gcm) ───

function hkdf(salt, ikm, info, length) {
  const prk = createHmac('sha256', salt).update(ikm).digest();
  const out = createHmac('sha256', prk).update(Buffer.concat([info, Buffer.from([1])])).digest();
  return out.subarray(0, length);
}

function encryptPayload(plaintext, p256dhB64, authB64) {
  const clientPub = unb64u(p256dhB64);        // 65-byte uncompressed point
  const authSecret = unb64u(authB64);         // 16 bytes

  // Ephemeral keypair for this message — the "as" side of the exchange.
  const ecdh = createECDH('prime256v1');
  ecdh.generateKeys();
  const serverPub = ecdh.getPublicKey();
  const sharedSecret = ecdh.computeSecret(clientPub);

  const salt = randomBytes(16);

  // RFC 8291 §3.3: the info string binds both public keys into the derivation,
  // so a shared secret cannot be replayed against a different subscription.
  const keyInfo = Buffer.concat([
    Buffer.from('WebPush: info\0'), clientPub, serverPub,
  ]);
  const ikm = hkdf(authSecret, sharedSecret, keyInfo, 32);
  const cek = hkdf(salt, ikm, Buffer.from('Content-Encoding: aes128gcm\0'), 16);
  const nonce = hkdf(salt, ikm, Buffer.from('Content-Encoding: nonce\0'), 12);

  // A single record, so the padding delimiter is 0x02 ("last record").
  const padded = Buffer.concat([Buffer.from(plaintext, 'utf8'), Buffer.from([2])]);
  const cipher = createCipheriv('aes-128-gcm', cek, nonce);
  const body = Buffer.concat([cipher.update(padded), cipher.final(), cipher.getAuthTag()]);

  // aes128gcm content-coding header: salt | rs | idlen | keyid
  const header = Buffer.alloc(21);
  salt.copy(header, 0);
  header.writeUInt32BE(4096, 16);
  header.writeUInt8(serverPub.length, 20);

  return Buffer.concat([header, serverPub, body]);
}

// ─── RFC 8292 VAPID ───

function vapidHeader(endpoint) {
  const { vapid } = load();
  const aud = new URL(endpoint).origin;
  const header = b64u(JSON.stringify({ typ: 'JWT', alg: 'ES256' }));
  const claims = b64u(JSON.stringify({
    aud,
    exp: Math.floor(Date.now() / 1000) + 12 * 60 * 60,
    sub: config.vapidSubject || 'mailto:crundi@localhost',
  }));
  const signingInput = `${header}.${claims}`;

  const key = createPrivateKey(vapid.privateKeyPem);
  const der = createSign('SHA256').update(signingInput).sign(key);
  // Node signs ECDSA as DER; JWS wants the raw r||s pair.
  return `${signingInput}.${b64u(derToJose(der))}`;
}

function derToJose(der) {
  let offset = 2;
  if (der[1] & 0x80) offset += der[1] & 0x7f;        // long-form length
  const readInt = () => {
    if (der[offset++] !== 0x02) throw new Error('Malformed ECDSA signature');
    let len = der[offset++];
    let val = der.subarray(offset, offset + len);
    offset += len;
    if (val.length > 32) val = val.subarray(val.length - 32);   // strip sign byte
    return Buffer.concat([Buffer.alloc(32 - val.length), val]); // left-pad to 32
  };
  return Buffer.concat([readInt(), readInt()]);
}

// ─── Sending ───

/**
 * Push to every stored subscription.
 *
 * A 404 or 410 means the browser threw the subscription away (cleared data,
 * uninstalled, permission revoked). Those are pruned rather than retried
 * forever — otherwise a stale endpoint is a failure on every notification.
 */
export async function send(text, meta = {}) {
  const s = load();
  if (!s.subscriptions.length) return false;

  const payload = JSON.stringify({
    title: meta.title || 'Crundi',
    body: String(text).slice(0, MAX_PAYLOAD),
    tag: meta.tag || 'crundi',
    url: meta.url || '/',
  });

  const dead = [];
  let delivered = 0;
  await Promise.all(s.subscriptions.map(async (sub) => {
    try {
      const body = encryptPayload(payload, sub.keys.p256dh, sub.keys.auth);
      const res = await fetch(sub.endpoint, {
        method: 'POST',
        headers: {
          TTL: String(TTL_S),
          'Content-Encoding': 'aes128gcm',
          'Content-Type': 'application/octet-stream',
          Authorization: `vapid t=${vapidHeader(sub.endpoint)}, k=${s.vapid.publicKey}`,
        },
        body,
      });
      if (res.status === 404 || res.status === 410) { dead.push(sub.endpoint); return; }
      if (!res.ok) {
        console.warn(`[web-push] ${res.status} from ${new URL(sub.endpoint).host}`);
        return;
      }
      delivered++;
    } catch (err) {
      console.warn(`[web-push] send failed: ${err.message}`);
    }
  }));

  if (dead.length) {
    s.subscriptions = s.subscriptions.filter(x => !dead.includes(x.endpoint));
    save();
    console.log(`[web-push] Dropped ${dead.length} expired subscription(s)`);
  }
  return delivered > 0;
}

export const __testing = { encryptPayload, hkdf, derToJose, b64u, unb64u };
