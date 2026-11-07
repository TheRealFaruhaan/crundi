#!/usr/bin/env node
/**
 * test-auth-oauth.mjs — signing in with Google and GitHub.
 *
 * What matters here fails quietly when it is wrong: a state that can be
 * replayed, a verifier that is not sent, an account check that lets the wrong
 * person in, a secret that leaks into what the browser is given, a method that
 * shows on the login page before it is set up, or a switch that leaves the
 * server with no way in. Each is checked directly, against a fake provider.
 *
 * Run: node scripts/test-auth-oauth.mjs
 */

import { mkdtempSync, readFileSync, statSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

// A throwaway data dir and nothing from the real install: no bot, no password.
const tmp = mkdtempSync(join(tmpdir(), 'crundi-oauth-'));
process.env.HOME = tmp;
process.env.XDG_CONFIG_HOME = join(tmp, '.config');
process.env.APPDATA = join(tmp, '.config');
process.env.DATA_DIR = join(tmp, 'data');
for (const k of ['TELEGRAM_BOT_TOKEN', 'ALLOWED_USERNAME', 'CRUNDI_PASSWORD_HASH', 'CRUNDI_TOTP_SECRET']) process.env[k] = '';

const { createOAuth, pkcePair, NOT_ALLOWED, callbackPath } = await import('../src/auth-oauth.js');
const auth = await import('../src/auth-config.js');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};
const b64url = (b) => Buffer.from(b).toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const jwt = (claims) => `${b64url('{"alg":"RS256"}')}.${b64url(JSON.stringify(claims))}.sig`;

// ─── A fake Google and GitHub ───
let googleClaims = null;
let githubUser = null;
let lastTokenBody = null;
const fakeFetch = async (url, opts = {}) => {
  const reply = (obj, ok = true) => ({ ok, json: async () => obj });
  if (url.startsWith('https://oauth2.googleapis.com/token')) {
    lastTokenBody = new URLSearchParams(opts.body);
    return reply({ access_token: 'at', id_token: jwt(googleClaims) });
  }
  if (url.startsWith('https://github.com/login/oauth/access_token')) {
    lastTokenBody = new URLSearchParams(opts.body);
    return reply({ access_token: 'gh-at' });
  }
  if (url.startsWith('https://api.github.com/user')) return reply(githubUser);
  throw new Error('unexpected fetch ' + url);
};
let clock = Date.now();
const oauth = createOAuth({ fetch: fakeFetch, now: () => clock });

const gCreds = { clientId: 'gid.apps.googleusercontent.com', clientSecret: 'G-SECRET-123', allowedEmail: 'owner@example.com' };
const hCreds = { clientId: 'Iv1.abc', clientSecret: 'H-SECRET-456', allowedLogin: 'Owner' };
const REDIR = 'https://dev.example.com' + callbackPath('google');

// ─── PKCE and the URL handed to the browser ───
{
  const { verifier, challenge } = pkcePair();
  check('PKCE verifier is 43..128 characters', verifier.length >= 43 && verifier.length <= 128, verifier.length);
  check('PKCE challenge is base64url(sha256(verifier))', challenge === b64url(createHash('sha256').update(verifier).digest()));
  const s = oauth.start('google', { creds: gCreds, redirectUri: REDIR });
  const u = new URL(s.url);
  check('Google URL goes to Google', u.origin + u.pathname === 'https://accounts.google.com/o/oauth2/v2/auth');
  check('Google asks only for openid email', u.searchParams.get('scope') === 'openid email');
  check('state is in the URL and long', (u.searchParams.get('state') || '').length >= 40);
  check('S256 challenge is in the URL', u.searchParams.get('code_challenge_method') === 'S256' && !!u.searchParams.get('code_challenge'));
  check('a nonce is in the Google URL', !!u.searchParams.get('nonce'));
  check('the exact callback URL is in the URL', u.searchParams.get('redirect_uri') === REDIR);
  check('the client secret is never in the URL', !s.url.includes('G-SECRET'));
  const h = new URL(oauth.start('github', { creds: hCreds, redirectUri: REDIR.replace('google', 'github') }).url);
  check('GitHub URL goes to GitHub', h.origin + h.pathname === 'https://github.com/login/oauth/authorize');
  check('GitHub gets a state and a PKCE challenge', !!h.searchParams.get('state') && h.searchParams.get('code_challenge_method') === 'S256');
  check('GitHub asks for no scope (public profile only)', !h.searchParams.has('scope'));
  check('an unconfigured provider will not start', !oauth.start('google', { creds: {}, redirectUri: REDIR }).ok);
  check('an unknown provider will not start', !oauth.start('facebook', { creds: gCreds, redirectUri: REDIR }).ok);
}

// ─── State handling ───
const googleOk = (state) => {
  const e = oauth._pending.get(state);
  googleClaims = { iss: 'https://accounts.google.com', aud: gCreds.clientId, exp: Math.floor(clock / 1000) + 600, nonce: e?.nonce, email: 'Owner@Example.com', email_verified: true, sub: '1001' };
};
{
  const s = oauth.start('google', { creds: gCreds, redirectUri: REDIR });
  const verifier = oauth._pending.get(s.state).verifier;
  googleOk(s.state);
  const r = await oauth.complete('google', { code: 'c1', state: s.state }, () => gCreds);
  check('the right Google account is let in', r.ok, r.error);
  check('the code exchange sends the PKCE verifier', lastTokenBody?.get('code_verifier') === verifier);
  check('the code exchange sends the same redirect URI', lastTokenBody?.get('redirect_uri') === REDIR);
  check('the code exchange is server-side with the secret', lastTokenBody?.get('client_secret') === gCreds.clientSecret);
  const again = await oauth.complete('google', { code: 'c1', state: s.state }, () => gCreds);
  check('a state works once only (replay refused)', !again.ok);
  const none = await oauth.complete('google', { code: 'c1', state: 'made-up' }, () => gCreds);
  check('an unknown state is refused', !none.ok);
  const nostate = await oauth.complete('google', { code: 'c1' }, () => gCreds);
  check('a missing state is refused', !nostate.ok);

  const s2 = oauth.start('google', { creds: gCreds, redirectUri: REDIR });
  const cross = await oauth.complete('github', { code: 'c', state: s2.state }, () => hCreds);
  check('a Google state cannot finish a GitHub sign-in', !cross.ok);

  const s3 = oauth.start('google', { creds: gCreds, redirectUri: REDIR });
  clock += 11 * 60 * 1000;
  googleOk(s3.state);
  const late = await oauth.complete('google', { code: 'c', state: s3.state }, () => gCreds);
  check('a state older than ten minutes is refused', !late.ok);

  const s4 = oauth.start('google', { creds: gCreds, redirectUri: REDIR });
  const cancelled = await oauth.complete('google', { error: 'access_denied', state: s4.state }, () => gCreds);
  check('a cancelled sign-in is refused with a plain message', !cancelled.ok && /cancelled/i.test(cancelled.error));
}

// ─── The wrong account ───
{
  const tryGoogle = async (mut) => {
    const s = oauth.start('google', { creds: gCreds, redirectUri: REDIR });
    googleOk(s.state); mut(googleClaims);
    return oauth.complete('google', { code: 'c', state: s.state }, () => gCreds);
  };
  let r = await tryGoogle((c) => { c.email = 'someone@example.com'; });
  check('a different Google email is refused', !r.ok && r.notAllowed && r.error === NOT_ALLOWED, r.error);
  r = await tryGoogle((c) => { c.email_verified = false; });
  check('an unverified email is refused, with the same message', !r.ok && r.error === NOT_ALLOWED, r.error);
  r = await tryGoogle((c) => { c.aud = 'someone-elses-client'; });
  check('an ID token for another client is refused', !r.ok);
  r = await tryGoogle((c) => { c.nonce = 'other'; });
  check('an ID token with the wrong nonce is refused', !r.ok);
  r = await tryGoogle((c) => { c.iss = 'https://evil.example'; });
  check('an ID token from another issuer is refused', !r.ok);
  r = await tryGoogle((c) => { c.exp = Math.floor(clock / 1000) - 5; });
  check('an expired ID token is refused', !r.ok);

  const tryGithub = async (user, creds = hCreds) => {
    const s = oauth.start('github', { creds, redirectUri: REDIR });
    githubUser = user;
    return oauth.complete('github', { code: 'c', state: s.state }, () => creds);
  };
  r = await tryGithub({ login: 'owner', id: 42 });
  check('the right GitHub username is let in (case-insensitive)', r.ok && r.account.id === '42', r.error);
  r = await tryGithub({ login: 'intruder', id: 7 });
  check('a different GitHub username is refused', !r.ok && r.error === NOT_ALLOWED);
  const pinned = { ...hCreds, allowedId: '42' };
  r = await tryGithub({ login: 'owner-renamed', id: 42 }, pinned);
  check('once the id is pinned, a renamed owner still gets in', r.ok);
  r = await tryGithub({ login: 'Owner', id: 99 }, pinned);
  check('once the id is pinned, someone who took the old username is refused', !r.ok && r.error === NOT_ALLOWED);
}

// ─── Handing the session over ───
{
  const code = oauth.issueHandoff({ provider: 'google' });
  check('a handoff code redeems once', !!oauth.redeemHandoff(code));
  check('a handoff code does not redeem twice', !oauth.redeemHandoff(code));
  const c2 = oauth.issueHandoff({ provider: 'google' });
  clock += 61 * 1000;
  check('a handoff code dies after a minute', !oauth.redeemHandoff(c2));

  // Poll mode: the desktop app.
  const s = oauth.start('google', { creds: gCreds, redirectUri: REDIR, mode: 'poll' });
  check('poll mode returns a poll id and a 4-digit code', !!s.pollId && /^\d{4}$/.test(s.confirmCode));
  check('nothing is ready before the callback', oauth.collectPoll(s.pollId).status === 'waiting');
  googleOk(s.state);
  const r = await oauth.complete('google', { code: 'c', state: s.state }, () => gCreds);
  check('the callback succeeds in poll mode', r.ok && r.entry.pollId === s.pollId);
  check('a successful callback still waits for the browser to confirm', oauth.collectPoll(s.pollId).status === 'confirm');
  check('a wrong confirm token does nothing', !oauth.confirmPoll('nope'));
  check('the right confirm token confirms', oauth.confirmPoll(oauth.confirmTokenFor(s.pollId)));
  check('the app collects the result', oauth.collectPoll(s.pollId).status === 'done');
  check('and only once', oauth.collectPoll(s.pollId).status === 'expired');

  const t = oauth.start('google', { creds: gCreds, redirectUri: REDIR, mode: 'poll' });
  googleOk(t.state); googleClaims.email = 'x@y.z';
  await oauth.complete('google', { code: 'c', state: t.state }, () => gCreds);
  const f = oauth.collectPoll(t.pollId);
  check('a refused account reaches the poller as failed', f.status === 'failed' && f.error === NOT_ALLOWED);
  check('a refused poll cannot be confirmed into a session', !oauth.confirmPoll(f.confirmToken));
}

// ─── Rate limiting ───
{
  let hit = false;
  for (let i = 0; i < 40; i++) if (oauth.limited('203.0.113.9')) { hit = true; break; }
  check('too many starts from one address are limited', hit);
  check('another address is not', !oauth.limited('203.0.113.10'));
}

// ─── Which methods exist, and the at-least-one rule ───
{
  check('a fresh install is open (setup mode)', auth.isOpen());
  let r = auth.setOAuthEnabled('google', true);
  check('Google cannot be turned on before it is configured', !r.ok, r.error);
  r = auth.setOAuthConfig('google', { clientId: gCreds.clientId, clientSecret: gCreds.clientSecret, allowed: 'Owner@Example.com' });
  check('Google settings save', r.ok, r.error);
  check('a saved but untested Google is not listed as a method', auth.methods().google === false);
  r = auth.setOAuthEnabled('google', true);
  check('Google cannot be turned on before a test passes', !r.ok, r.error);
  auth.markOAuthVerified('google', { email: 'owner@example.com' });
  r = auth.setOAuthEnabled('google', true);
  check('a tested Google turns on', r.ok, r.error);
  check('it is then listed', auth.methods().google === true && auth.methods().github === false);
  check('and the server is no longer open', !auth.isOpen());
  r = auth.setOAuthEnabled('google', false);
  check('the only method on cannot be turned off', !r.ok, r.error);
  r = auth.clearOAuth('google');
  check('the only method on cannot be removed', !r.ok, r.error);

  r = auth.setOAuthConfig('github', { clientId: hCreds.clientId, clientSecret: hCreds.clientSecret, allowed: '@Owner' });
  check('GitHub settings save (leading @ dropped)', r.ok && auth.oauthCreds('github').allowedLogin === 'Owner', r.error);
  auth.markOAuthVerified('github', { login: 'Owner', id: '42' });
  check('a passed GitHub test pins the account id', auth.oauthCreds('github').allowedId === '42');
  check('GitHub turns on after its test', auth.setOAuthEnabled('github', true).ok);
  check('with GitHub on, Google can be turned off', auth.setOAuthEnabled('google', false).ok);
  check('Google is then no longer listed', auth.methods().google === false && auth.methods().github === true);

  r = auth.setOAuthConfig('github', { clientId: hCreds.clientId, clientSecret: '', allowed: 'Owner' });
  check('a blank secret keeps the saved one', r.ok && auth.oauthCreds('github').clientSecret === hCreds.clientSecret);
  check('saving the same settings keeps the test result', auth.status().oauth.github.verified === true);
  auth.setOAuthConfig('github', { clientId: hCreds.clientId, clientSecret: 'H-NEW-SECRET', allowed: 'Owner' });
  check('a new secret marks it untested again', auth.status().oauth.github.verified === false);
  auth.setOAuthConfig('github', { clientId: hCreds.clientId, clientSecret: '', allowed: 'someone-else' });
  check('a different username forgets the pinned id', !auth.oauthCreds('github').allowedId);
  check('a bad email is refused', !auth.setOAuthConfig('google', { clientId: 'x', clientSecret: 'y', allowed: 'not an email' }).ok);

  // Secrets stay on the server.
  const st = JSON.stringify(auth.status());
  check('Settings status never carries a client secret', !st.includes('G-SECRET') && !st.includes('H-SECRET') && !st.includes('H-NEW-SECRET') && !/clientSecret/.test(st));
  check('Settings status says whether a secret is set', auth.status().oauth.github.secretSet === true);
  const mode = statSync(join(process.env.DATA_DIR, 'auth.json')).mode & 0o777;
  check('the file holding the secrets is 0600', mode === 0o600, mode.toString(8));
}

// ─── The routes: what the browser is given ───
{
  const web = readFileSync(join(root, 'src', 'webapp.js'), 'utf8');
  const methodsBlock = web.slice(web.indexOf("path === '/api/auth/methods' && req.method === 'GET'"), web.indexOf("path === '/api/auth/setup' && req.method === 'POST'"));
  check('found the /api/auth/methods handler', methodsBlock.length > 0 && methodsBlock.length < 3000);
  check('/api/auth/methods says only whether Google / GitHub are on', /google: m\.google,/.test(methodsBlock) && /github: m\.github,/.test(methodsBlock));
  check('/api/auth/methods never touches the OAuth credentials', !/oauthCreds|clientSecret|clientId|allowed/.test(methodsBlock.replace(/\/\/.*$/gm, '')));
  const cfgGet = web.slice(web.indexOf("path === '/api/auth/config' && req.method === 'GET'"), web.indexOf("path === '/api/auth/config' && req.method === 'POST'"));
  check('/api/auth/config GET returns status() and callback URLs only', /authConfig\.status\(\)/.test(cfgGet) && !/oauthCreds/.test(cfgGet));
  check('/api/settings never reads the OAuth credentials', !/oauthCreds/.test(web.slice(web.indexOf("path === '/api/settings' && req.method === 'GET'"), web.indexOf("path === '/api/settings' && req.method === 'POST'"))));
  const cb = web.slice(web.indexOf('/^\\/api\\/auth\\/oauth\\/(google|github)\\/callback$/'), web.indexOf("path === '/api/auth/oauth/confirm'"));
  check('found the callback handler', cb.length > 0);
  check('the callback never puts a token in the URL', !/#token=|refresh=/.test(cb) && /#oauth=/.test(cb));
  check('a test start needs the signed-in owner', /purpose === 'test'[\s\S]{0,200}validateToken\(req\)[\s\S]{0,80}ROLE_OWNER/.test(web));
  check('a login start needs the method turned on', /authConfig\.methods\(\)\[provider\]/.test(web));

  const cv = await import('../src/client-version.js');
  const appReq = { url: '/', headers: { 'user-agent': 'Mozilla/5.0 (Linux; Android 16; wv) Chrome/154 Mobile Safari/537.36 CrundiAndroid/1.0.1 (play)' } };
  check('the phone app cannot sign in this way (it pairs)', ['/api/auth/oauth/start', '/api/auth/oauth/exchange', '/api/auth/oauth/poll'].every((p) => cv.isUnpairedSignIn(appReq, p)));

  const html = readFileSync(join(root, 'src', 'webapp-html.js'), 'utf8');
  check('the login page has a Google and a GitHub button', html.includes('id="oauth-btn-google"') && html.includes('id="oauth-btn-github"'));
  check('buttons show only for methods that are on', /filter\(\(k\) => methods\[k\] === true\)/.test(html));
  check('the page swaps the handoff code before checking auth', /await consumeOAuthHandoff\(\)/.test(html));
}

rmSync(tmp, { recursive: true, force: true });
console.log(failures ? `\n${failures} FAILED` : '\nAll OAuth sign-in checks passed.');
process.exit(failures ? 1 : 0);
