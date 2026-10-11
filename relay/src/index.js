// The Crundi push relay.
//
// A Cloudflare Worker that stands between Crundi servers and Google's and
// Apple's push services. Those only take a push signed by whoever published
// the app, and a self-hosted server must not hold that signature, so this
// does. The contract it meets is android/RELAY.md; the other end of it is
// src/mobile-push.js.
//
// What it is careful about:
//
//   - It serves nobody without a key. Keys are issued by hand, one per
//     server, and only their hashes are kept: this code could not print a
//     key back if asked.
//   - It cannot read a notification. What arrives is ciphertext sealed for
//     one phone; it is passed on as it came and never logged.
//   - It keeps nothing about devices or messages. Keys and their labels are
//     all that is stored.
//
// Bindings (wrangler.toml and secrets):
//   KEYS                  KV namespace: issued keys, by hash
//   PER_KEY               rate limiter: requests per key per minute
//   ADMIN_TOKEN           secret: allows issuing and revoking keys
//   FCM_SERVICE_ACCOUNT   secret: the Firebase service account JSON (Android)
//   RELAY_NAME, CONTACT_TELEGRAM, CONTACT_URL, CONTACT_TEXT   who runs this

const MAX_MESSAGES = 20;
const MAX_PAYLOAD = 3000;
const MAX_BODY_BYTES = 120_000;
const KEY_RE = /^[A-Za-z0-9._~-]{20,200}$/;
const PAYLOAD_RE = /^[A-Za-z0-9_-]+$/;
const TAG_RE = /^[A-Za-z0-9._-]{1,40}$/;
const PLATFORMS = new Set(['android', 'ios']);
const MAX_TTL_S = 28 * 24 * 60 * 60;       // the most FCM will hold a message

const json = (status, body, headers = {}) => new Response(JSON.stringify(body), {
  status, headers: { 'Content-Type': 'application/json', 'Cache-Control': 'no-store', ...headers },
});
const fail = (status, error, headers) => json(status, { ok: false, error }, headers);

const hex = (buf) => [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, '0')).join('');
const sha256 = async (text) => hex(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text)));
const b64url = (bytes) => btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

function bearer(request) {
  const h = request.headers.get('authorization') || '';
  return h.startsWith('Bearer ') ? h.slice(7).trim() : '';
}

// ─── Keys ───
// Stored as  key:<sha256 of the key>  ->  { id, label, createdAt, revokedAt }.
// The id is the start of the hash: enough to point at a key in a list
// without the list being of any use to someone who reads it.
async function lookupKey(env, key) {
  if (!KEY_RE.test(key)) return null;
  const hash = await sha256(key);
  const record = await env.KEYS.get('key:' + hash, 'json');
  return record ? { ...record, hash } : null;
}

async function issueKey(env, label) {
  const raw = new Uint8Array(30);
  crypto.getRandomValues(raw);
  const key = 'crp_' + b64url(raw);
  const hash = await sha256(key);
  const record = { id: hash.slice(0, 12), label: String(label || '').replace(/[\u0000-\u001f\u007f<>]/g, ' ').trim().slice(0, 80) || 'Unlabelled', createdAt: Date.now(), revokedAt: 0 };
  await env.KEYS.put('key:' + hash, JSON.stringify(record));
  return { key, ...record };
}

async function listKeys(env) {
  const out = [];
  let cursor;
  do {
    const page = await env.KEYS.list({ prefix: 'key:', cursor });
    for (const k of page.keys) {
      const record = await env.KEYS.get(k.name, 'json');
      if (record) out.push(record);
    }
    cursor = page.list_complete ? undefined : page.cursor;
  } while (cursor);
  return out.sort((a, b) => b.createdAt - a.createdAt);
}

async function revokeKey(env, id) {
  let cursor;
  do {
    const page = await env.KEYS.list({ prefix: 'key:', cursor });
    for (const k of page.keys) {
      const record = await env.KEYS.get(k.name, 'json');
      if (record && record.id === id) {
        if (!record.revokedAt) { record.revokedAt = Date.now(); await env.KEYS.put(k.name, JSON.stringify(record)); }
        return record;
      }
    }
    cursor = page.list_complete ? undefined : page.cursor;
  } while (cursor);
  return null;
}

// The admin token is compared by hash, so the comparison takes the same time
// whatever was sent.
async function isAdmin(request, env) {
  const given = bearer(request);
  if (!given || !env.ADMIN_TOKEN) return false;
  const [a, b] = await Promise.all([sha256(given), sha256(env.ADMIN_TOKEN)]);
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

// ─── Android: Firebase Cloud Messaging ───
// An access token is good for an hour and costs a signature and a round trip
// to make, so it is kept for as long as this instance of the Worker lives.
let fcmAccess = { token: '', expiresAt: 0, project: '' };

async function fcmToken(env, fetchImpl) {
  let account;
  try { account = JSON.parse(env.FCM_SERVICE_ACCOUNT || ''); } catch { return null; }
  if (!account?.client_email || !account?.private_key || !account?.project_id) return null;
  if (fcmAccess.token && fcmAccess.project === account.project_id && Date.now() < fcmAccess.expiresAt - 60_000) return fcmAccess;

  const now = Math.floor(Date.now() / 1000);
  const enc = (o) => b64url(new TextEncoder().encode(JSON.stringify(o)));
  const unsigned = enc({ alg: 'RS256', typ: 'JWT' }) + '.' + enc({
    iss: account.client_email, scope: 'https://www.googleapis.com/auth/firebase.messaging',
    aud: 'https://oauth2.googleapis.com/token', iat: now, exp: now + 3600,
  });
  const der = Uint8Array.from(atob(account.private_key.replace(/-----[^-]+-----/g, '').replace(/\s+/g, '')), (c) => c.charCodeAt(0));
  const key = await crypto.subtle.importKey('pkcs8', der, { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' }, false, ['sign']);
  const sig = await crypto.subtle.sign('RSASSA-PKCS1-v1_5', key, new TextEncoder().encode(unsigned));
  const res = await fetchImpl('https://oauth2.googleapis.com/token', {
    method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: 'grant_type=' + encodeURIComponent('urn:ietf:params:oauth:grant-type:jwt-bearer') + '&assertion=' + unsigned + '.' + b64url(sig),
  });
  if (!res.ok) return null;
  const data = await res.json();
  if (!data.access_token) return null;
  fcmAccess = { token: data.access_token, expiresAt: Date.now() + (Number(data.expires_in) || 3600) * 1000, project: account.project_id };
  return fcmAccess;
}

async function sendAndroid(env, m, fetchImpl) {
  const access = await fcmToken(env, fetchImpl);
  if (!access) return { status: 'failed', error: 'android_not_configured' };
  const res = await fetchImpl(`https://fcm.googleapis.com/v1/projects/${access.project}/messages:send`, {
    method: 'POST',
    headers: { Authorization: 'Bearer ' + access.token, 'Content-Type': 'application/json' },
    // A data message: nothing is shown by the system, the app is woken to
    // decrypt `c` and show the notification itself.
    body: JSON.stringify({ message: {
      token: m.token,
      data: { c: m.payload },
      android: { priority: m.priority === 'normal' ? 'NORMAL' : 'HIGH', ttl: m.ttl + 's', ...(m.collapse ? { collapse_key: m.collapse } : {}) },
    } }),
  });
  if (res.ok) return { status: 'sent' };
  let code = '';
  try { const e = (await res.json()).error; code = (e?.details || []).map((d) => d.errorCode).find(Boolean) || e?.status || ''; } catch { /* no body */ }
  // The app was uninstalled or the token replaced: the server should forget it.
  if (res.status === 404 || code === 'UNREGISTERED') return { status: 'gone' };
  // Our own credentials were refused. Forget the cached token so the next
  // request signs a new one instead of repeating the refusal for an hour.
  if (res.status === 401 || res.status === 403) fcmAccess = { token: '', expiresAt: 0, project: '' };
  return { status: 'failed', error: String(code || res.status).slice(0, 60) };
}

// ─── iOS: not built yet ───
async function sendIos() {
  return { status: 'failed', error: 'ios_not_configured' };
}

// ─── A message, checked ───
function cleanMessage(m) {
  if (!m || typeof m !== 'object') return null;
  const token = String(m.token || ''), payload = String(m.payload || '');
  if (!PLATFORMS.has(m.platform)) return null;
  if (token.length < 16 || token.length > 4096) return null;
  if (!payload || payload.length > MAX_PAYLOAD || !PAYLOAD_RE.test(payload)) return null;
  const ttl = Math.min(MAX_TTL_S, Math.max(0, Math.floor(Number(m.ttl) || 0)));
  return { platform: m.platform, token, payload, ttl, priority: m.priority === 'normal' ? 'normal' : 'high', collapse: TAG_RE.test(String(m.collapse || '')) ? String(m.collapse) : '' };
}

async function readJson(request) {
  const text = await request.text();
  if (text.length > MAX_BODY_BYTES) return null;
  try { return JSON.parse(text); } catch { return null; }
}

/**
 * @param {Request} request
 * @param {object} env the bindings listed at the top of this file
 * @param {{ fetchImpl?: typeof fetch }} [opts] the tests pass their own fetch
 */
export async function handle(request, env, { fetchImpl = fetch } = {}) {
  const url = new URL(request.url);
  const path = url.pathname.replace(/\/+$/, '') || '/';
  const method = request.method;

  // Who runs this relay, for a server that has no key yet.
  if (path === '/v1/info' && method === 'GET') {
    return json(200, {
      ok: true,
      name: env.RELAY_NAME || 'Crundi push relay',
      contact: { telegram: env.CONTACT_TELEGRAM || '', url: env.CONTACT_URL || '', text: env.CONTACT_TEXT || '' },
    });
  }

  // ─── Issuing and revoking keys ───
  if (path.startsWith('/admin/')) {
    if (!(await isAdmin(request, env))) return fail(401, 'invalid_key');
    if (path === '/admin/keys' && method === 'GET') return json(200, { ok: true, keys: await listKeys(env) });
    if (path === '/admin/keys' && method === 'POST') {
      const body = await readJson(request);
      // The only time the key itself is ever sent anywhere.
      return json(200, { ok: true, ...(await issueKey(env, body?.label)) });
    }
    if (path === '/admin/keys/revoke' && method === 'POST') {
      const body = await readJson(request);
      const record = await revokeKey(env, String(body?.id || ''));
      return record ? json(200, { ok: true, ...record }) : fail(404, 'bad_request');
    }
    return fail(404, 'bad_request');
  }

  if (path !== '/v1/key' && path !== '/v1/push') return fail(404, 'bad_request');

  // ─── Everything below needs a server's key ───
  const record = await lookupKey(env, bearer(request));
  if (!record) return fail(401, 'invalid_key');
  if (record.revokedAt) return fail(403, 'key_revoked');

  if (path === '/v1/key' && method === 'GET') return json(200, { ok: true, label: record.label });

  if (path === '/v1/push' && method === 'POST') {
    if (env.PER_KEY) {
      const { success } = await env.PER_KEY.limit({ key: record.hash });
      if (!success) return fail(429, 'rate_limited', { 'Retry-After': '60' });
    }
    const body = await readJson(request);
    const list = body?.messages;
    if (!Array.isArray(list) || !list.length || list.length > MAX_MESSAGES) return fail(400, 'bad_request');
    const messages = list.map(cleanMessage);
    if (messages.some((m) => !m)) return fail(400, 'bad_request');
    const results = await Promise.all(messages.map(async (m) => {
      try {
        return m.platform === 'android' ? await sendAndroid(env, m, fetchImpl) : await sendIos(env, m, fetchImpl);
      } catch {
        // Nothing about the failure is echoed: an exception's text could
        // carry part of a token or of our own credentials.
        return { status: 'failed', error: 'relay_error' };
      }
    }));
    return json(200, { ok: true, results });
  }

  return fail(405, 'bad_request');
}

export default {
  async fetch(request, env) {
    try {
      return await handle(request, env);
    } catch {
      return fail(500, 'relay_error');
    }
  },
};

// For the tests: forget the cached Google token between cases.
export function _reset() { fcmAccess = { token: '', expiresAt: 0, project: '' }; }
