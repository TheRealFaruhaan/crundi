/**
 * auth-config.js — which ways in are switched on, and their secrets.
 *
 * Four methods: Telegram, a password with a TOTP code, and Google or GitHub
 * through the owner's own OAuth app (auth-oauth.js). Any one is enough, any
 * mix can be on, and at least one must stay on once any is configured.
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
  // An explicit decision in Settings beats the environment, and it has to be
  // checked BEFORE either source. Testing it inside the "we have a stored hash"
  // branch meant disabling an env-provided password wrote {enabled:false} with
  // no hash, fell through to the env bootstrap, and left the password live —
  // the switch reported success and changed nothing.
  if (s.password && s.password.enabled === false) return null;
  if (s.password && s.password.hash && s.password.totpSecret) return s.password;
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

// ─── Google and GitHub ───

export const OAUTH_PROVIDERS = ['google', 'github'];

/** Client ID, secret and the allowed account all present. */
function oauthConfigured(provider) {
  const o = load()[provider];
  if (!o || !o.clientId || !o.clientSecret) return false;
  return provider === 'google' ? !!o.allowedEmail : !!(o.allowedLogin || o.allowedId);
}

function oauthOn(provider) {
  return oauthConfigured(provider) && load()[provider].enabled === true;
}

/** Everything the flow needs, secret included. Server-side only. */
export function oauthCreds(provider) {
  if (!OAUTH_PROVIDERS.includes(provider)) return null;
  const o = load()[provider];
  return o ? { ...o } : null;
}

export function methods() {
  const password = !!passwordCreds();
  const telegram = telegramOn();
  const google = oauthOn('google');
  const github = oauthOn('github');
  return {
    telegram,
    password,
    google,
    github,
    // False puts the server in setup mode: unauthenticated, but able to do
    // nothing except configure a method (enforced in webapp.js).
    anyConfigured: telegram || password || google || github,
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
    return { ok: false, error: 'That is the only way to sign in. Turn on another method first.' };
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
    return { ok: false, error: 'That is the only way to sign in. Turn on another method first.' };
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
  // true → safe to disable, something else is still on
  return ['telegram', 'password', ...OAUTH_PROVIDERS].some((k) => k !== which && m[k]);
}

const LABEL = { google: 'Google', github: 'GitHub' };

/**
 * Set the client ID, secret and allowed account. A blank secret keeps the
 * stored one (the page never has it to send back). Any change to the
 * credentials or the account marks the method untested again: it has to be
 * shown to work before it can be turned on, so a typo in a secret cannot be
 * what you are left signing in with.
 */
export function setOAuthConfig(provider, { clientId, clientSecret, allowed } = {}) {
  if (!OAUTH_PROVIDERS.includes(provider)) return { ok: false, error: 'Unknown sign-in method' };
  const s = load();
  const prev = s[provider] || {};
  const id = String(clientId ?? prev.clientId ?? '').trim();
  const secret = String(clientSecret || '').trim() || prev.clientSecret || '';
  let acct = String(allowed ?? (provider === 'google' ? prev.allowedEmail : prev.allowedLogin) ?? '').trim();
  if (!id) return { ok: false, error: 'Enter the client ID.' };
  if (!secret) return { ok: false, error: 'Enter the client secret.' };
  if (id.length > 300 || secret.length > 300) return { ok: false, error: 'That is too long to be a client ID or secret.' };
  const next = { ...prev, clientId: id, clientSecret: secret };
  if (provider === 'google') {
    acct = acct.toLowerCase();
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(acct)) return { ok: false, error: 'Enter the Google email address allowed to sign in.' };
    next.allowedEmail = acct;
  } else {
    acct = acct.replace(/^@/, '');
    if (!/^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})$/.test(acct)) return { ok: false, error: 'Enter the GitHub username allowed to sign in.' };
    // A different username is a different person: forget the id learned
    // from the old one.
    if (String(prev.allowedLogin || '').toLowerCase() !== acct.toLowerCase()) delete next.allowedId;
    next.allowedLogin = acct;
  }
  const changed = ['clientId', 'clientSecret', 'allowedEmail', 'allowedLogin'].some((k) => (prev[k] || '') !== (next[k] || ''));
  if (changed) { next.verified = false; delete next.verifiedAs; }
  s[provider] = next;
  if (!save()) return { ok: false, error: 'Could not save the sign-in settings' };
  return { ok: true };
}

/** A Test sign-in went through as the allowed account. */
export function markOAuthVerified(provider, account) {
  const s = load();
  const o = s[provider];
  if (!o) return false;
  o.verified = true;
  o.verifiedAs = provider === 'google' ? account.email : account.login;
  // Pin GitHub to the numeric id: a username can be renamed and re-registered
  // by someone else, the id cannot.
  if (provider === 'github' && account.id) o.allowedId = String(account.id);
  return save();
}

/** A real sign-in went through: pin the GitHub id if it is not yet known. */
export function noteOAuthAccount(provider, account) {
  const o = load()[provider];
  if (provider === 'github' && o && !o.allowedId && account?.id) { o.allowedId = String(account.id); save(); }
}

export function setOAuthEnabled(provider, enabled) {
  if (!OAUTH_PROVIDERS.includes(provider)) return { ok: false, error: 'Unknown sign-in method' };
  const want = !!enabled;
  const s = load();
  if (want && !oauthConfigured(provider)) return { ok: false, error: `Fill in the ${LABEL[provider]} client ID, secret and allowed account first.` };
  if (want && s[provider].verified !== true) return { ok: false, error: `Run Test first, so ${LABEL[provider]} sign-in is known to work before it is turned on.` };
  if (!want && !guardLastMethod(provider)) return { ok: false, error: 'That is the only way to sign in. Turn on another method first.' };
  if (!s[provider]) return { ok: true };
  s[provider].enabled = want;
  if (!save()) return { ok: false, error: 'Could not save the sign-in settings' };
  return { ok: true };
}

/** Forget a provider altogether (refused if it is the last way in). */
export function clearOAuth(provider) {
  if (!OAUTH_PROVIDERS.includes(provider)) return { ok: false, error: 'Unknown sign-in method' };
  if (methods()[provider] && !guardLastMethod(provider)) return { ok: false, error: 'That is the only way to sign in. Turn on another method first.' };
  const s = load();
  delete s[provider];
  if (!save()) return { ok: false, error: 'Could not save the sign-in settings' };
  return { ok: true };
}

/** What Settings may see of a provider: never the secret, only whether it is set. */
function oauthView(provider) {
  const o = load()[provider] || {};
  return {
    configured: oauthConfigured(provider),
    clientId: o.clientId || '',
    secretSet: !!o.clientSecret,
    allowed: (provider === 'google' ? o.allowedEmail : o.allowedLogin) || '',
    ...(provider === 'github' ? { idPinned: !!o.allowedId } : {}),
    verified: o.verified === true,
    verifiedAs: o.verifiedAs || '',
  };
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
    oauth: { google: oauthView('google'), github: oauthView('github') },
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
