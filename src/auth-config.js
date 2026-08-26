/**
 * auth-config.js — which ways in are switched on, and their secrets.
 *
 * Two methods: Telegram, and a password with a TOTP code. Either is enough,
 * both can be on, and at least one must stay on once any is configured.
 *
 * ─── The open first run ───
 *
 * A brand-new install has no method configured, and cannot have one: a fresh
 * container has nobody to ask. Rather than refuse to start — which leaves you
 * with no way to configure it either — the server runs OPEN until the first
 * method is set up, and says so loudly in the log and in the UI.
 *
 * This is a real hole for as long as it lasts, so it is drawn as narrowly as
 * possible: it applies only while NOTHING is configured, it ends permanently
 * the moment anything is, and it can never come back — disabling the last
 * remaining method is refused rather than dropping back to open.
 *
 * ─── Where the secrets live ───
 *
 * Environment variables bootstrap an install that has no UI yet (a container
 * given CRUNDI_PASSWORD_HASH at launch). Anything set from Settings is written
 * to the data directory instead, because .env is not always writable and a
 * change to how you log in should not need a restart to take effect.
 *
 * The store wins over the environment for the password, so changing it in
 * Settings does what it looks like it does even when an env var also exists.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, renameSync } from 'fs';
import { join } from 'path';
import { config } from './config.js';
import { verifyPassword, verifyTotp, hashPassword, generateTotpSecret, totpUri } from './auth-password.js';

const FILE = () => join(config.dataDir, 'auth.json');

let store = null;   // { password: {hash, totpSecret, enabled}, telegram: {enabled} }

function load() {
  if (store) return store;
  try {
    if (existsSync(FILE())) {
      const d = JSON.parse(readFileSync(FILE(), 'utf-8'));
      if (d && typeof d === 'object') { store = d; return store; }
    }
  } catch { /* fall through to defaults */ }
  store = {};
  return store;
}

function save() {
  try {
    mkdirSync(config.dataDir, { recursive: true });
    const tmp = FILE() + '.tmp';
    writeFileSync(tmp, JSON.stringify(store, null, 2), { mode: 0o600 });
    renameSync(tmp, FILE());
    return true;
  } catch (err) {
    console.error('[auth] Could not save the sign-in settings:', err.message);
    return false;
  }
}

// ─── What is configured ───

function passwordCreds() {
  const s = load();
  if (s.password && s.password.hash && s.password.totpSecret) {
    return s.password.enabled === false ? null : s.password;
  }
  // Bootstrap from the environment when Settings has never been used.
  if (config.passwordHash && config.totpSecret) {
    return { hash: config.passwordHash, totpSecret: config.totpSecret, enabled: true, fromEnv: true };
  }
  return null;
}

function telegramOn() {
  const s = load();
  if (!config.telegramConfigured) return false;      // no bot token, nothing to enable
  return s.telegram?.enabled !== false;
}

export function methods() {
  const password = !!passwordCreds();
  const telegram = telegramOn();
  return {
    telegram,
    password,
    // False puts the server in setup mode: unauthenticated, but able to do
    // nothing except configure a method (enforced in webapp.js).
    anyConfigured: telegram || password,
  };
}

/** True while nothing is configured — see the setup-mode gate in webapp.js. */
export function isOpen() { return !methods().anyConfigured; }

// ─── Verifying a login ───

export function checkPasswordLogin(password, code) {
  const creds = passwordCreds();
  if (!creds) return false;
  // Both factors, always. Evaluated unconditionally so the work done does not
  // depend on which one was wrong.
  const okPass = verifyPassword(password, creds.hash);
  const okCode = verifyTotp(creds.totpSecret, code);
  return okPass && okCode;
}

// ─── Changing what is configured ───

/**
 * Turn on (or change) password login. Returns the TOTP secret to enrol; a new
 * one is minted whenever the password is set, so a rotated password never
 * leaves an old device still able to produce valid codes.
 */
export function setPassword(password) {
  const pw = String(password || '');
  if (pw.length < 12) {
    return { ok: false, error: 'Use at least 12 characters — this login can reach a shell on this machine.' };
  }
  const s = load();
  const totpSecret = generateTotpSecret();
  s.password = { hash: hashPassword(pw), totpSecret, enabled: true };
  if (!save()) return { ok: false, error: 'Could not save the sign-in settings' };
  return { ok: true, totpSecret, uri: totpUri(totpSecret, { account: config.localUsername || 'crundi' }) };
}

export function setTelegramEnabled(enabled) {
  const want = !!enabled;
  if (want && !config.telegramConfigured) {
    return { ok: false, error: 'Set a Telegram bot token and username first (Settings → Telegram Bot Token).' };
  }
  if (!want && !guardLastMethod('telegram')) {
    return { ok: false, error: 'That is the only way to sign in. Set up password login first.' };
  }
  const s = load();
  s.telegram = { ...(s.telegram || {}), enabled: want };
  if (!save()) return { ok: false, error: 'Could not save the sign-in settings' };
  return { ok: true };
}

export function setPasswordEnabled(enabled) {
  const want = !!enabled;
  const s = load();
  if (want && !(s.password?.hash || (config.passwordHash && config.totpSecret))) {
    return { ok: false, error: 'Set a password first.' };
  }
  if (!want && !guardLastMethod('password')) {
    return { ok: false, error: 'That is the only way to sign in. Set up Telegram login first.' };
  }
  s.password = { ...(s.password || {}), enabled: want };
  // An env-provided password cannot be switched off by deleting it, so record
  // the decision explicitly rather than relying on absence.
  if (want && !s.password.hash && config.passwordHash) {
    s.password.hash = config.passwordHash;
    s.password.totpSecret = config.totpSecret;
  }
  if (!save()) return { ok: false, error: 'Could not save the sign-in settings' };
  return { ok: true };
}

/** Would turning `which` off leave nothing to sign in with? */
function guardLastMethod(which) {
  const m = methods();
  const remaining = (which === 'telegram' ? m.password : m.telegram);
  return remaining;   // true → safe to disable, something else is still on
}

/** Everything Settings needs to render the sign-in section. */
export function status() {
  const m = methods();
  const s = load();
  return {
    ...m,
    passwordFromEnv: !!(passwordCreds()?.fromEnv),
    telegramAvailable: !!config.telegramConfigured,
    passwordSet: !!(s.password?.hash || config.passwordHash),
  };
}

/** Log the open state at startup — it should never pass unnoticed. */
export function warnIfOpen() {
  if (!isOpen()) return;
  console.warn('');
  console.warn('  ┌─────────────────────────────────────────────────────────────┐');
  console.warn('  │  NO SIGN-IN METHOD IS CONFIGURED                            │');
  console.warn('  │                                                             │');
  console.warn('  │  Until one is, this server does exactly one thing: let      │');
  console.warn('  │  someone configure it. Nothing else is reachable.           │');
  console.warn('  │                                                             │');
  console.warn('  │  But that someone is whoever gets there first. Open the     │');
  console.warn('  │  web UI and set up a sign-in method before exposing this    │');
  console.warn('  │  port to anything you do not trust.                         │');
  console.warn('  └─────────────────────────────────────────────────────────────┘');
  console.warn('');
}

/** Test seam: drop the cached store so a reload picks the file up again. */
export function _reload() { store = null; }
