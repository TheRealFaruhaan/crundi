/**
 * auth-oauth.js — signing in with Google or GitHub, through the OWNER's own
 * OAuth app.
 *
 * Both are the authorization-code flow, done server-side:
 *
 *   page ── start ──▶ server mints state + PKCE verifier, hands back the
 *                     provider's URL
 *   browser ────────▶ provider, which asks who you are
 *   provider ───────▶ /api/auth/oauth/<provider>/callback?code&state
 *   server ─────────▶ provider's token endpoint (code + verifier + secret),
 *                     then works out WHICH account it was, and allows it only
 *                     if it is the one configured
 *
 * The client secret never leaves the server, and the code alone is useless
 * without the verifier, which never leaves the server either.
 *
 * ─── Handing the session to the page ───
 *
 * Two ways, chosen by the server at start:
 *
 *  - redirect: the page is on the same origin as the callback. The callback
 *    issues a ONE-TIME handoff code (60 s, single use) and sends the browser to
 *    /#oauth=<code>; the page swaps it for the session and clears the URL.
 *    No token is ever in a URL.
 *
 *  - poll: the page cannot receive the redirect — the desktop app (Google
 *    refuses embedded browsers, so the flow runs in the system browser) or a
 *    page on another origin. The page polls with a secret only it holds. The
 *    browser that did the sign-in is shown a short code and has to confirm
 *    that the waiting app shows the same one, so a sign-in link sent by
 *    someone else cannot quietly sign THEIR app in with your account.
 *
 * Everything here is in memory: an in-flight sign-in does not survive a
 * restart, and does not need to.
 */

import { randomBytes, createHash, timingSafeEqual } from 'node:crypto';

export const PROVIDERS = {
  google: {
    label: 'Google',
    authUrl: 'https://accounts.google.com/o/oauth2/v2/auth',
    tokenUrl: 'https://oauth2.googleapis.com/token',
    scope: 'openid email',
  },
  github: {
    label: 'GitHub',
    authUrl: 'https://github.com/login/oauth/authorize',
    tokenUrl: 'https://github.com/login/oauth/access_token',
    userUrl: 'https://api.github.com/user',
    // Public profile only: the login and the numeric id are all we need, and
    // they need no scope at all.
    scope: '',
  },
};

export const NOT_ALLOWED = 'This account is not allowed here';

const STATE_TTL_MS = 10 * 60 * 1000;      // time to get through the provider's pages
const HANDOFF_TTL_MS = 60 * 1000;         // redirect → page swap
const POLL_TTL_MS = 10 * 60 * 1000;
const MAX_PENDING = 50;                    // a flood of starts evicts the oldest, never grows
const START_WINDOW_MS = 10 * 60 * 1000;
const STARTS_PER_WINDOW = 30;              // per address

const b64url = (buf) => Buffer.from(buf).toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const rand = (n = 32) => b64url(randomBytes(n));

export function pkcePair() {
  const verifier = rand(48);   // 64 chars, within RFC 7636's 43..128
  const challenge = b64url(createHash('sha256').update(verifier).digest());
  return { verifier, challenge };
}

function safeEqual(a, b) {
  const x = Buffer.from(String(a || '')); const y = Buffer.from(String(b || ''));
  return x.length === y.length && x.length > 0 && timingSafeEqual(x, y);
}

/** The callback path for a provider; the full URL is the public base + this. */
export function callbackPath(provider) { return `/api/auth/oauth/${provider}/callback`; }

/**
 * @param {object} [o]
 * @param {Function} [o.fetch]  injectable for tests
 * @param {Function} [o.now]
 */
export function createOAuth({ fetch: fetchImpl = globalThis.fetch, now = () => Date.now() } = {}) {
  const pending = new Map();   // state → { provider, verifier, nonce, purpose, mode, pollId, redirectUri, createdAt }
  const handoffs = new Map();  // code → { session, expiresAt }
  const polls = new Map();     // pollId → { purpose, provider, status, confirmCode, confirmToken, account, error, expiresAt }
  const starts = new Map();    // ip → [timestamps]

  function sweep() {
    const t = now();
    for (const [k, v] of pending) if (t - v.createdAt > STATE_TTL_MS) pending.delete(k);
    for (const [k, v] of handoffs) if (t >= v.expiresAt) handoffs.delete(k);
    for (const [k, v] of polls) if (t >= v.expiresAt) polls.delete(k);
    for (const [k, v] of starts) { const keep = v.filter((x) => t - x < START_WINDOW_MS); if (keep.length) starts.set(k, keep); else starts.delete(k); }
  }

  /** Too many starts from one address? Counts the attempt either way. */
  function limited(ip) {
    sweep();
    const list = starts.get(ip || '?') || [];
    list.push(now());
    starts.set(ip || '?', list);
    return list.length > STARTS_PER_WINDOW;
  }

  /**
   * Begin a flow.
   * @param {'google'|'github'} provider
   * @param {object} o
   * @param {{clientId:string}} o.creds
   * @param {string} o.redirectUri   exact URL registered at the provider
   * @param {'login'|'test'} [o.purpose]
   * @param {'redirect'|'poll'} [o.mode]
   */
  function start(provider, { creds, redirectUri, purpose = 'login', mode = 'redirect' }) {
    const p = PROVIDERS[provider];
    if (!p) return { ok: false, error: 'Unknown sign-in method' };
    if (!creds?.clientId) return { ok: false, error: `${p.label} sign-in is not set up` };
    sweep();
    while (pending.size >= MAX_PENDING) pending.delete(pending.keys().next().value);
    const state = rand(32);
    const nonce = rand(16);
    const { verifier, challenge } = pkcePair();
    const entry = { provider, verifier, nonce, purpose, mode, redirectUri, createdAt: now() };
    const out = { ok: true, mode };
    if (mode === 'poll') {
      const pollId = rand(32);
      // Four digits: enough to tell "my app" from "someone else's", short
      // enough to compare at a glance.
      const confirmCode = String(1000 + (randomBytes(2).readUInt16BE(0) % 9000));
      polls.set(pollId, { purpose, provider, status: 'waiting', confirmCode, confirmToken: rand(24), expiresAt: now() + POLL_TTL_MS });
      entry.pollId = pollId;
      out.pollId = pollId;
      out.confirmCode = confirmCode;
    }
    pending.set(state, entry);
    const q = new URLSearchParams({
      client_id: creds.clientId,
      redirect_uri: redirectUri,
      response_type: 'code',
      state,
      code_challenge: challenge,
      code_challenge_method: 'S256',
    });
    if (p.scope) q.set('scope', p.scope);
    if (provider === 'google') { q.set('nonce', nonce); q.set('prompt', 'select_account'); }
    if (provider === 'github') q.set('allow_signup', 'false');
    out.url = `${p.authUrl}?${q}`;
    out.state = state;
    return out;
  }

  /** Take (and so spend) a pending state. A state is good for one callback only. */
  function takeState(provider, state) {
    sweep();
    const e = state && pending.get(String(state));
    if (!e) return null;
    pending.delete(String(state));
    if (e.provider !== provider) return null;
    return e;
  }

  /**
   * Swap the code for WHO signed in. Does not decide whether they are allowed.
   * @returns {Promise<{ok:true, account:{email?,login?,id?}} | {ok:false, error:string}>}
   */
  async function identify(provider, code, entry, creds) {
    const p = PROVIDERS[provider];
    const body = new URLSearchParams({
      client_id: creds.clientId,
      client_secret: creds.clientSecret,
      code: String(code || ''),
      redirect_uri: entry.redirectUri,
      grant_type: 'authorization_code',
      code_verifier: entry.verifier,
    });
    let tok;
    try {
      const r = await fetchImpl(p.tokenUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded', Accept: 'application/json' },
        body: body.toString(),
        signal: AbortSignal.timeout(15000),
      });
      tok = await r.json();
    } catch (err) {
      return { ok: false, error: `Could not reach ${p.label}: ${err.message}` };
    }
    if (!tok || tok.error || !tok.access_token) {
      return { ok: false, error: `${p.label} refused the sign-in${tok?.error ? ` (${tok.error})` : ''}` };
    }

    if (provider === 'google') {
      // Received straight from Google's token endpoint over TLS, so per OpenID
      // Connect Core 3.1.3.7 the TLS connection stands in for the signature
      // check; the claims are still checked one by one.
      const claims = decodeJwt(tok.id_token);
      if (!claims) return { ok: false, error: 'Google sent no ID token' };
      const issOk = claims.iss === 'https://accounts.google.com' || claims.iss === 'accounts.google.com';
      const audOk = Array.isArray(claims.aud) ? claims.aud.includes(creds.clientId) : claims.aud === creds.clientId;
      const expOk = Number(claims.exp) * 1000 > now();
      const nonceOk = safeEqual(claims.nonce, entry.nonce);
      if (!issOk || !audOk || !expOk || !nonceOk) return { ok: false, error: 'The ID token from Google did not check out' };
      return {
        ok: true,
        account: {
          email: String(claims.email || '').toLowerCase(),
          emailVerified: claims.email_verified === true || claims.email_verified === 'true',
          id: String(claims.sub || ''),
        },
      };
    }

    // GitHub: the token says nothing about who; ask.
    try {
      const r = await fetchImpl(p.userUrl, {
        headers: { Authorization: `Bearer ${tok.access_token}`, Accept: 'application/vnd.github+json', 'User-Agent': 'Crundi' },
        signal: AbortSignal.timeout(15000),
      });
      const u = await r.json();
      if (!r.ok || !u || !u.login || !u.id) return { ok: false, error: 'GitHub did not say who signed in' };
      return { ok: true, account: { login: String(u.login), id: String(u.id) } };
    } catch (err) {
      return { ok: false, error: `Could not reach GitHub: ${err.message}` };
    }
  }

  /**
   * Is this the configured account? Google: the allowed email, verified.
   * GitHub: the stored numeric id once known (a username can be given up and
   * taken by someone else), the username until then.
   */
  function allowed(provider, account, creds) {
    if (!account) return false;
    if (provider === 'google') {
      const want = String(creds.allowedEmail || '').trim().toLowerCase();
      return !!want && account.emailVerified === true && safeEqual(account.email, want);
    }
    if (provider === 'github') {
      if (creds.allowedId) return safeEqual(account.id, String(creds.allowedId));
      const want = String(creds.allowedLogin || '').trim().replace(/^@/, '').toLowerCase();
      return !!want && safeEqual(String(account.login).toLowerCase(), want);
    }
    return false;
  }

  /**
   * Handle a callback end to end, short of making the session.
   * @returns {Promise<{ok:boolean, error?:string, entry?:object, account?:object}>}
   */
  async function complete(provider, query, getCreds) {
    const entry = takeState(provider, query.state);
    if (!entry) return { ok: false, error: 'This sign-in link has expired or was already used. Start again.' };
    const pollRec = entry.pollId ? polls.get(entry.pollId) : null;
    const fail = (error, extra = {}) => {
      if (pollRec) { pollRec.status = 'failed'; pollRec.error = error; Object.assign(pollRec, extra); }
      return { ok: false, error, entry, ...extra };
    };
    if (query.error) return fail(query.error === 'access_denied' ? 'Sign-in was cancelled.' : `${PROVIDERS[provider].label} said: ${String(query.error).slice(0, 80)}`);
    if (!query.code) return fail('No code came back from the sign-in.');
    const creds = getCreds();
    if (!creds?.clientId || !creds?.clientSecret) return fail(`${PROVIDERS[provider].label} sign-in is not set up`);
    const who = await identify(provider, query.code, entry, creds);
    if (!who.ok) return fail(who.error);
    if (!allowed(provider, who.account, creds)) return fail(NOT_ALLOWED, { account: who.account, notAllowed: true });
    if (pollRec) { pollRec.status = entry.purpose === 'test' ? 'done' : 'confirm'; pollRec.account = who.account; }
    return { ok: true, entry, account: who.account };
  }

  /**
   * One-time code the redirected page swaps for its session. Holds only the
   * fact that the sign-in succeeded; the session itself is made at the swap,
   * so it is never in a URL and never made for a page that does not collect it.
   */
  function issueHandoff(grant) {
    sweep();
    const code = rand(32);
    handoffs.set(code, { grant, expiresAt: now() + HANDOFF_TTL_MS });
    return code;
  }
  function redeemHandoff(code) {
    sweep();
    const e = code && handoffs.get(String(code));
    if (!e) return null;
    handoffs.delete(String(code));
    return e.grant;
  }

  function pollStatus(pollId) {
    sweep();
    return (pollId && polls.get(String(pollId))) || null;
  }
  /** The confirm form's token, for the page shown in the browser that signed in. */
  function confirmTokenFor(pollId) { return pollStatus(pollId)?.confirmToken || ''; }
  /**
   * The browser that signed in confirmed the app shows the same code. Found by
   * its own token, so the poll id (the app's secret) is never in that page.
   */
  function confirmPoll(confirmToken) {
    sweep();
    for (const rec of polls.values()) {
      if (rec.status === 'confirm' && safeEqual(confirmToken, rec.confirmToken)) { rec.status = 'done'; return true; }
    }
    return false;
  }
  /** The poller collects a finished result exactly once. */
  function collectPoll(pollId) {
    const rec = pollStatus(pollId);
    if (!rec) return { status: 'expired' };
    if (rec.status === 'done' || rec.status === 'failed') polls.delete(String(pollId));
    return rec;
  }

  return { start, complete, takeState, identify, allowed, limited, issueHandoff, redeemHandoff, pollStatus, confirmTokenFor, confirmPoll, collectPoll, _pending: pending, _polls: polls };
}

/** Payload of a JWT, unverified (see the note where it is used). */
export function decodeJwt(jwt) {
  const parts = String(jwt || '').split('.');
  if (parts.length !== 3) return null;
  try { return JSON.parse(Buffer.from(parts[1].replace(/-/g, '+').replace(/_/g, '/'), 'base64').toString('utf8')); } catch { return null; }
}

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

/**
 * A small standalone page for the browser that did the sign-in, when it is not
 * the page that asked (the desktop app's system browser, a Settings test).
 */
export function oauthPage({ title, text, code = '', form = null, ok = true }) {
  const formHtml = form
    ? `<form method="POST" action="${esc(form.action)}">${Object.entries(form.fields).map(([k, v]) => `<input type="hidden" name="${esc(k)}" value="${esc(v)}">`).join('')}<button type="submit">${esc(form.button)}</button></form>`
    : '';
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="referrer" content="no-referrer"><title>${esc(title)} - Crundi</title>
<style>body{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#0a0a0f;color:#e6e6ef;font:15px/1.5 system-ui,-apple-system,Segoe UI,sans-serif}
.box{max-width:380px;padding:32px 36px;border:1px solid #26263a;border-radius:14px;background:#12121b;text-align:center}
h1{font-size:1.1rem;margin:0 0 10px;color:${ok ? '#e6e6ef' : '#f87171'}}p{color:#a1a1b5;margin:0 0 16px}
.code{font:600 2rem/1 ui-monospace,monospace;letter-spacing:.3em;margin:6px 0 18px;color:#fff}
button{padding:10px 18px;border:0;border-radius:8px;background:#6366f1;color:#fff;font:600 0.95rem system-ui,sans-serif;cursor:pointer}</style></head>
<body><div class="box"><h1>${esc(title)}</h1><p>${esc(text)}</p>${code ? `<div class="code">${esc(code)}</div>` : ''}${formHtml}</div></body></html>`;
}
