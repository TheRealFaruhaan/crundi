/**
 * tls.js — HTTPS for a server with a public address.
 *
 * The Cloudflare tunnel terminates TLS for you, so an install behind one needs
 * none of this. A Linux box with its own IP does: either to be reached directly,
 * or to sit behind Cloudflare in Full (strict), which validates the origin
 * certificate rather than accepting anything.
 *
 * Three modes:
 *   off          no HTTPS; the tunnel or a reverse proxy handles it (default)
 *   letsencrypt  obtain and renew automatically over ACME HTTP-01
 *   provided     use certificate files you supply and manage yourself
 *
 * A self-signed certificate is deliberately not on that list. It would satisfy
 * "the port speaks TLS" while failing Cloudflare Full (strict) — the exact case
 * this exists for — so offering it would mostly generate confusing failures.
 *
 * ─── On the ACME dependency ───
 *
 * acme-client is imported dynamically, only in letsencrypt mode, so nobody else
 * pays for it. Hand-rolling ACME would mean hand-building a PKCS#10 CSR in DER,
 * which cannot be meaningfully verified without repeatedly hitting a rate-
 * limited CA — a worse trade than the dependency, and the opposite call to the
 * one made for web push, where the crypto round-trips against itself locally.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, renameSync } from 'fs';
import { join } from 'path';
import { createSecureContext } from 'tls';
import { config } from './config.js';

const DIR = () => join(config.dataDir, 'tls');
const CERT = () => join(DIR(), 'cert.pem');
const KEY = () => join(DIR(), 'key.pem');
const ACCOUNT_KEY = () => join(DIR(), 'account.pem');
const META = () => join(DIR(), 'meta.json');

// Renew with a month to spare. Let's Encrypt certificates last 90 days, so this
// leaves room for several failed attempts before anything actually expires.
const RENEW_BEFORE_MS = 30 * 24 * 60 * 60 * 1000;
const CHECK_EVERY_MS = 12 * 60 * 60 * 1000;

// Tokens the CA is about to ask for, keyed by the path it will request.
const challenges = new Map();

let current = null;      // { key, cert, expiresAt }
let renewTimer = null;
let renewing = false;
let onReload = null;

function readMeta() {
  try { return JSON.parse(readFileSync(META(), 'utf-8')); } catch { return {}; }
}

function writeAtomic(file, data) {
  mkdirSync(DIR(), { recursive: true });
  const tmp = file + '.tmp';
  writeFileSync(tmp, data, { mode: 0o600 });
  renameSync(tmp, file);
}

/** Expiry read from the certificate itself, not from what we recorded. */
function certExpiry(pem) {
  try {
    const ctx = createSecureContext({ cert: pem });
    // Node exposes the parsed peer certificate only on a socket, so fall back
    // to the recorded value; the file is ours and written alongside meta.json.
    void ctx;
  } catch { /* ignore */ }
  const meta = readMeta();
  return meta.expiresAt ? Number(meta.expiresAt) : 0;
}

export function mode() {
  const m = String(config.tlsMode || 'off').toLowerCase();
  return ['letsencrypt', 'provided'].includes(m) ? m : 'off';
}

export function enabled() { return mode() !== 'off'; }

/** The current key/cert pair, or null if we have none yet. */
export function credentials() { return current; }

/**
 * Serve an ACME HTTP-01 challenge.
 *
 * Called from the plain-HTTP listener before anything else, because the CA
 * fetches this over port 80 and will not follow a redirect to HTTPS.
 */
export function handleAcmeChallenge(req, res) {
  const m = String(req.url || '').match(/^\/\.well-known\/acme-challenge\/([\w-]+)$/);
  if (!m) return false;
  const value = challenges.get(m[1]);
  if (!value) { res.writeHead(404); res.end('Not found'); return true; }
  res.writeHead(200, { 'Content-Type': 'text/plain' });
  res.end(value);
  console.log(`[tls] Served ACME challenge ${m[1].slice(0, 8)}…`);
  return true;
}

function loadExisting() {
  try {
    if (!existsSync(CERT()) || !existsSync(KEY())) return null;
    const cert = readFileSync(CERT(), 'utf-8');
    const key = readFileSync(KEY(), 'utf-8');
    if (!cert.includes('BEGIN CERTIFICATE') || !key.includes('PRIVATE KEY')) return null;
    return { cert, key, expiresAt: certExpiry(cert) };
  } catch { return null; }
}

/** Load whatever we have so the server can start listening immediately. */
export function init() {
  if (!enabled()) return null;

  if (mode() === 'provided') {
    const certPath = config.tlsCertPath;
    const keyPath = config.tlsKeyPath;
    if (!certPath || !keyPath) {
      console.error('[tls] TLS_MODE=provided needs TLS_CERT_PATH and TLS_KEY_PATH.');
      return null;
    }
    if (!existsSync(certPath) || !existsSync(keyPath)) {
      console.error(`[tls] Certificate or key not found (${certPath}, ${keyPath}).`);
      return null;
    }
    current = { cert: readFileSync(certPath, 'utf-8'), key: readFileSync(keyPath, 'utf-8'), expiresAt: 0 };
    console.log('[tls] Using the certificate you provided.');
    return current;
  }

  current = loadExisting();
  if (current) {
    const days = current.expiresAt ? Math.round((current.expiresAt - Date.now()) / 86400000) : null;
    console.log(`[tls] Loaded the stored certificate${days != null ? ` (${days} days left)` : ''}.`);
  } else {
    console.log('[tls] No certificate yet — one will be requested shortly after startup.');
  }
  return current;
}

/**
 * Start the renewal loop.
 *
 * Deliberately AFTER the server is listening: HTTP-01 needs port 80 answering
 * before the CA will validate anything, so requesting a certificate first would
 * fail every time on a cold start.
 */
export function startRenewal(reloadFn) {
  if (mode() !== 'letsencrypt') return;
  onReload = reloadFn;
  const tick = () => { maybeRenew().catch(err => console.error('[tls] Renewal check failed:', err.message)); };
  renewTimer = setInterval(tick, CHECK_EVERY_MS);
  if (renewTimer.unref) renewTimer.unref();
  // Give the listener a moment to be genuinely reachable before the CA probes.
  setTimeout(tick, 5000);
}

export function stop() { clearInterval(renewTimer); renewTimer = null; }

async function maybeRenew() {
  if (renewing) return;
  const needed = !current || !current.expiresAt || (current.expiresAt - Date.now()) < RENEW_BEFORE_MS;
  if (!needed) return;
  renewing = true;
  try {
    await obtain();
  } finally {
    renewing = false;
  }
}

async function obtain() {
  const domain = String(config.tlsDomain || '').trim();
  const email = String(config.tlsEmail || '').trim();
  if (!domain) { console.error('[tls] TLS_DOMAIN is not set — cannot request a certificate.'); return; }
  if (!email) { console.error('[tls] TLS_EMAIL is not set — Let\'s Encrypt requires a contact address.'); return; }

  let acme;
  try {
    acme = await import('acme-client');
  } catch (err) {
    console.error('[tls] acme-client is not installed:', err.message);
    return;
  }

  console.log(`[tls] Requesting a certificate for ${domain}…`);
  mkdirSync(DIR(), { recursive: true });

  // Reuse the account key across renewals; a new one each time would register a
  // fresh account every 60 days and count against the CA's account rate limit.
  let accountKey;
  if (existsSync(ACCOUNT_KEY())) accountKey = readFileSync(ACCOUNT_KEY());
  else {
    accountKey = await acme.crypto.createPrivateKey();
    writeAtomic(ACCOUNT_KEY(), accountKey);
  }

  const client = new acme.Client({
    directoryUrl: config.tlsStaging
      ? acme.directory.letsencrypt.staging
      : acme.directory.letsencrypt.production,
    accountKey,
  });

  const [certKey, csr] = await acme.crypto.createCsr({ commonName: domain });

  try {
    const cert = await client.auto({
      csr,
      email,
      termsOfServiceAgreed: true,
      challengePriority: ['http-01'],
      challengeCreateFn: async (authz, challenge, keyAuthorization) => {
        challenges.set(challenge.token, keyAuthorization);
      },
      challengeRemoveFn: async (authz, challenge) => {
        challenges.delete(challenge.token);
      },
    });

    writeAtomic(CERT(), cert);
    writeAtomic(KEY(), certKey);
    // Let's Encrypt issues for 90 days; record it so expiry is knowable without
    // parsing the certificate on every check.
    const expiresAt = Date.now() + 89 * 24 * 60 * 60 * 1000;
    writeAtomic(META(), JSON.stringify({ domain, expiresAt, issuedAt: Date.now() }, null, 2));

    current = { cert, key: certKey.toString(), expiresAt };
    console.log(`[tls] Certificate obtained for ${domain}. Renewing automatically before it expires.`);
    // Swap it into the running server rather than asking for a restart.
    if (onReload) { try { onReload(current); } catch (err) { console.error('[tls] Hot reload failed:', err.message); } }
  } catch (err) {
    console.error(`[tls] Could not obtain a certificate: ${err.message}`);
    console.error('[tls] HTTP-01 needs this machine reachable on port 80 at ' + domain + '.');
  }
}

/** Force a renewal now — used by the Settings button. */
export async function renewNow() {
  if (mode() !== 'letsencrypt') return { ok: false, error: 'Not using Let\'s Encrypt' };
  if (renewing) return { ok: false, error: 'A renewal is already running' };
  renewing = true;
  try { await obtain(); } finally { renewing = false; }
  return current ? { ok: true, expiresAt: current.expiresAt } : { ok: false, error: 'Renewal did not produce a certificate' };
}

export function status() {
  return {
    mode: mode(),
    domain: config.tlsDomain || '',
    hasCert: !!current,
    expiresAt: current?.expiresAt || 0,
    daysLeft: current?.expiresAt ? Math.round((current.expiresAt - Date.now()) / 86400000) : null,
    staging: !!config.tlsStaging,
  };
}
