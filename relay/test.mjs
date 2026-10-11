#!/usr/bin/env node
/**
 * relay/test.mjs — the push relay, against the contract in android/RELAY.md.
 *
 * The Worker is run here as a plain function with stand-ins for what
 * Cloudflare gives it (the key store, the rate limiter) and for Google. No
 * network, no account. What is held to: no service without a key, a key that
 * is stored only as a hash, the answers the contract promises, and that what
 * is forwarded to Google is exactly what arrived and nothing that was kept.
 *
 * Run: node relay/test.mjs
 */

import { generateKeyPairSync, createVerify } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { handle, _reset } from './src/index.js';

const here = dirname(fileURLToPath(import.meta.url));
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// ─── Stand-ins ───
function kv() {
  const map = new Map();
  return {
    map,
    async get(k, type) { const v = map.get(k); return v === undefined ? null : (type === 'json' ? JSON.parse(v) : v); },
    async put(k, v) { map.set(k, v); },
    // Two keys a page, so paging is actually walked.
    async list({ prefix, cursor }) { const all = [...map.keys()].filter((k) => k.startsWith(prefix)).sort(); const at = Number(cursor || 0); return { keys: all.slice(at, at + 2).map((name) => ({ name })), list_complete: at + 2 >= all.length, cursor: String(at + 2) }; },
  };
}
const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
const SERVICE_ACCOUNT = JSON.stringify({ client_email: 'relay@crundi-test.iam.gserviceaccount.com', project_id: 'crundi-test', private_key: privateKey.export({ type: 'pkcs8', format: 'pem' }) });
const ADMIN = 'admin-token-for-the-tests-0123456789';
const newEnv = (over = {}) => ({ KEYS: kv(), ADMIN_TOKEN: ADMIN, FCM_SERVICE_ACCOUNT: SERVICE_ACCOUNT, RELAY_NAME: 'Crundi', CONTACT_TELEGRAM: 'TheRealFaruhaan', ...over });

// Google, recorded. `fcm` decides what each send is answered with.
function google(fcm = () => ({ status: 200, body: { name: 'projects/crundi-test/messages/1' } })) {
  const calls = [];
  const fetchImpl = async (url, init) => {
    const call = { url: String(url), headers: init.headers || {}, body: init.body };
    calls.push(call);
    if (call.url === 'https://oauth2.googleapis.com/token') return new Response(JSON.stringify({ access_token: 'ya29.test-access', expires_in: 3600 }), { status: 200 });
    const a = fcm(JSON.parse(init.body), calls.length);
    return new Response(JSON.stringify(a.body || {}), { status: a.status });
  };
  return { calls, fetchImpl, sends: () => calls.filter((c) => c.url.includes('fcm.googleapis.com')) };
}
const call = async (env, method, path, { key, body, g } = {}) => {
  const res = await handle(new Request('https://relay.test' + path, { method, headers: { ...(key ? { Authorization: 'Bearer ' + key } : {}), 'Content-Type': 'application/json' }, body: body === undefined ? undefined : (typeof body === 'string' ? body : JSON.stringify(body)) }), env, { fetchImpl: (g || google()).fetchImpl });
  let data = null; try { data = await res.json(); } catch { /* none */ }
  return { status: res.status, data, headers: res.headers };
};
const msg = (over = {}) => ({ platform: 'android', token: 'fcm-token-' + 'x'.repeat(120), payload: 'QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo', ttl: 14400, priority: 'high', collapse: 'finished', ...over });

// ─── Who runs it ───
let env = newEnv();
let r = await call(env, 'GET', '/v1/info');
ok(r.status === 200 && r.data.ok && r.data.name === 'Crundi' && r.data.contact.telegram === 'TheRealFaruhaan', 'anyone can ask who runs the relay and who to ask for a key');

// ─── No service without a key ───
for (const [what, key] of [['no key', undefined], ['a made-up key', 'crp_' + 'z'.repeat(40)], ['something that is not a key at all', 'x']]) {
  r = await call(env, 'POST', '/v1/push', { key, body: { messages: [msg()] } });
  ok(r.status === 401 && r.data.error === 'invalid_key', `push with ${what} is refused`);
}
ok((await call(env, 'GET', '/v1/key')).status === 401, 'so is checking a key that was not sent');
ok((await call(env, 'GET', '/admin/keys')).status === 401 && (await call(env, 'POST', '/admin/keys', { key: 'not-the-admin-token-at-all-000', body: { label: 'x' } })).status === 401, 'issuing keys needs the admin token');
ok((await call(newEnv({ ADMIN_TOKEN: '' }), 'GET', '/admin/keys', { key: '' })).status === 401, 'and a relay deployed with no admin token issues none, to anyone');

// ─── Issuing ───
const issued = await call(env, 'POST', '/admin/keys', { key: ADMIN, body: { label: 'Faruhaan dev server' } });
const KEY = issued.data.key;
ok(issued.status === 200 && /^crp_[A-Za-z0-9_-]{40}$/.test(KEY) && issued.data.label === 'Faruhaan dev server' && issued.data.id.length === 12, 'the admin can issue a key, with a label');
ok(![...env.KEYS.map.entries()].some(([k, v]) => k.includes(KEY) || v.includes(KEY)), 'what is stored is a hash: the key itself is nowhere in the store');
const second = (await call(env, 'POST', '/admin/keys', { key: ADMIN, body: { label: '<b>Another</b>\nserver' } })).data;
const third = (await call(env, 'POST', '/admin/keys', { key: ADMIN, body: {} })).data;
ok(second.key !== KEY && !/[<>\n]/.test(second.label) && third.label === 'Unlabelled', 'every key is different, and a label is tidied');
r = await call(env, 'GET', '/admin/keys', { key: ADMIN });
ok(r.data.keys.length === 3 && r.data.keys.every((k) => !('key' in k) && !('hash' in k)) && !JSON.stringify(r.data).includes(KEY), 'the list of keys shows labels and ids, never a key');
ok((await call(env, 'GET', '/admin/keys', { key: KEY })).status === 401, 'a server\'s key is not an admin token');

// ─── Checking a key ───
r = await call(env, 'GET', '/v1/key', { key: KEY });
ok(r.status === 200 && r.data.ok && r.data.label === 'Faruhaan dev server', 'a good key is confirmed, with its label');

// ─── Sending ───
let g = google();
r = await call(env, 'POST', '/v1/push', { key: KEY, g, body: { messages: [msg(), msg({ token: 'fcm-token-' + 'y'.repeat(120), priority: 'normal', collapse: 'needsInput' })] } });
ok(r.status === 200 && r.data.ok && r.data.results.length === 2 && r.data.results.every((x) => x.status === 'sent'), 'two messages in, two results out, in order');
// The messages go out side by side, so which reaches Google first is not
// fixed. Tell them apart by their token, not by the order they arrived in.
const byToken = (t) => g.sends().find((c) => JSON.parse(c.body).message.token === t);
const sends = [byToken(msg().token), byToken('fcm-token-' + 'y'.repeat(120))];
ok(g.sends().length === 2 && sends[0] && sends[1] && sends[0].url === 'https://fcm.googleapis.com/v1/projects/crundi-test/messages:send' && sends[0].headers.Authorization === 'Bearer ya29.test-access', 'each goes to Google for the right project, with a Google access token');
const sentBody = JSON.parse(sends[0].body).message;
ok(sentBody.token === msg().token && sentBody.data.c === msg().payload && Object.keys(sentBody.data).length === 1, 'the ciphertext is passed on exactly as it came, as the only data');
ok(!sentBody.notification && sentBody.android.priority === 'HIGH' && sentBody.android.ttl === '14400s' && sentBody.android.collapse_key === 'finished', 'as a data message, with the priority, lifetime and collapse tag asked for');
ok(JSON.parse(sends[1].body).message.android.priority === 'NORMAL', 'normal priority is honoured');
ok(!JSON.stringify(sends.map((s) => s.body)).includes(KEY), 'the server\'s key is not passed on to Google');
ok(g.calls.filter((c) => c.url.includes('oauth2')).length === 1, 'two messages at once still ask Google for one token, not one each');

// The token request really is a signed assertion Google would accept.
const tokenCall = g.calls.find((c) => c.url.includes('oauth2'));
const assertion = decodeURIComponent(/assertion=([^&]+)/.exec(tokenCall.body)[1]);
const [h, p, s] = assertion.split('.');
const claims = JSON.parse(Buffer.from(p, 'base64url').toString());
ok(createVerify('RSA-SHA256').update(h + '.' + p).verify(publicKey, Buffer.from(s, 'base64url')), 'the request for a Google token is signed with the service account\'s key');
ok(claims.iss === 'relay@crundi-test.iam.gserviceaccount.com' && claims.scope === 'https://www.googleapis.com/auth/firebase.messaging' && claims.aud === 'https://oauth2.googleapis.com/token' && claims.exp - claims.iat === 3600, 'and asks for messaging and nothing else');
g = google();
await call(env, 'POST', '/v1/push', { key: KEY, g, body: { messages: [msg()] } });
ok(!g.calls.some((c) => c.url.includes('oauth2')), 'the Google token is reused, not asked for again on every push');

// ─── What Google can answer ───
_reset();
g = google((body) => (body.message.token.includes('gone') ? { status: 404, body: { error: { status: 'NOT_FOUND', details: [{ errorCode: 'UNREGISTERED' }] } } } : body.message.token.includes('busy') ? { status: 503, body: { error: { status: 'UNAVAILABLE' } } } : { status: 200 }));
r = await call(env, 'POST', '/v1/push', { key: KEY, g, body: { messages: [msg(), msg({ token: 'gone-' + 'g'.repeat(40) }), msg({ token: 'busy-' + 'b'.repeat(40) })] } });
ok(r.data.results[0].status === 'sent' && r.data.results[1].status === 'gone' && r.data.results[2].status === 'failed' && r.data.results[2].error === 'UNAVAILABLE', 'an uninstalled app is reported as gone, a hiccup as failed, and neither spoils the rest');
_reset();
g = google(() => ({ status: 401, body: { error: { status: 'UNAUTHENTICATED' } } }));
await call(env, 'POST', '/v1/push', { key: KEY, g, body: { messages: [msg()] } });
const again = google();
await call(env, 'POST', '/v1/push', { key: KEY, g: again, body: { messages: [msg()] } });
ok(again.calls.some((c) => c.url.includes('oauth2')), 'if Google refuses the relay\'s own token, a new one is made next time');
_reset();
g = { calls: [], fetchImpl: async () => { throw new Error('secret-looking text ya29.should-not-leak'); } };
r = await call(env, 'POST', '/v1/push', { key: KEY, g, body: { messages: [msg()] } });
ok(r.status === 200 && r.data.results[0].status === 'failed' && r.data.results[0].error === 'relay_error' && !JSON.stringify(r.data).includes('ya29'), 'a failure on the way to Google is reported without repeating what went wrong');
_reset();
r = await call(newEnvWithKey(env, { FCM_SERVICE_ACCOUNT: '' }), 'POST', '/v1/push', { key: KEY, body: { messages: [msg()] } });
ok(r.data.results[0].status === 'failed' && r.data.results[0].error === 'android_not_configured', 'a relay with no Firebase account says so, per message');
r = await call(env, 'POST', '/v1/push', { key: KEY, body: { messages: [msg({ platform: 'ios' })] } });
ok(r.data.results[0].status === 'failed' && r.data.results[0].error === 'ios_not_configured', 'iOS is answered honestly as not there yet');
function newEnvWithKey(from, over) { return { ...from, ...over }; }

// ─── Bad requests ───
const bads = [
  ['no messages', { messages: [] }], ['not a list', { messages: 'x' }], ['more than twenty', { messages: Array.from({ length: 21 }, () => msg()) }],
  ['an unknown platform', { messages: [msg({ platform: 'toaster' })] }], ['a token too short', { messages: [msg({ token: 'abc' })] }],
  ['a payload too long', { messages: [msg({ payload: 'A'.repeat(3001) })] }], ['a payload that is not base64url', { messages: [msg({ payload: 'has spaces+and/slashes==' })] }],
  ['no payload', { messages: [msg({ payload: '' })] }], ['one bad message among good ones', { messages: [msg(), null] }], ['not JSON', '{nope'],
];
for (const [what, body] of bads) {
  g = google();
  r = await call(env, 'POST', '/v1/push', { key: KEY, g, body });
  ok(r.status === 400 && r.data.error === 'bad_request' && g.sends().length === 0, 'refused, and nothing sent: ' + what);
}
g = google();
r = await call(env, 'POST', '/v1/push', { key: KEY, g, body: { messages: [msg({ ttl: 9e9, collapse: 'not a tag!', priority: 'urgent' })] } });
const tidy = JSON.parse(g.sends()[0].body).message.android;
ok(r.status === 200 && tidy.ttl === '2419200s' && !('collapse_key' in tidy) && tidy.priority === 'HIGH', 'an over-long lifetime is capped, and a malformed tag or priority is dropped, not passed on');
ok((await call(env, 'GET', '/v1/push', { key: KEY })).status === 405 && (await call(env, 'GET', '/nothing-here')).status === 404, 'the wrong method and the wrong path are refused');

// ─── Revoking ───
r = await call(env, 'POST', '/admin/keys/revoke', { key: ADMIN, body: { id: issued.data.id } });
ok(r.status === 200 && r.data.revokedAt > 0, 'the admin can revoke a key by its id');
g = google();
r = await call(env, 'POST', '/v1/push', { key: KEY, g, body: { messages: [msg()] } });
ok(r.status === 403 && r.data.error === 'key_revoked' && g.sends().length === 0 && (await call(env, 'GET', '/v1/key', { key: KEY })).status === 403, 'a revoked key is refused as revoked, and sends nothing');
ok((await call(env, 'GET', '/v1/key', { key: second.key })).status === 200, 'the other servers\' keys still work');
ok((await call(env, 'POST', '/admin/keys/revoke', { key: ADMIN, body: { id: 'nope' } })).status === 404, 'revoking a key that does not exist says so');

// ─── The key store lags, as the real one does ───
{
  // A list that has not caught up with what was just written.
  const slow = newEnv();
  const realList = slow.KEYS.list.bind(slow.KEYS);
  slow.KEYS.list = async () => ({ keys: [], list_complete: true });
  const fresh = (await call(slow, 'POST', '/admin/keys', { key: ADMIN, body: { label: 'Just issued' } })).data;
  ok((await call(slow, 'GET', '/admin/keys', { key: ADMIN })).data.keys.length === 0, 'setup: the list does not show the new key yet');
  const gone = await call(slow, 'POST', '/admin/keys/revoke', { key: ADMIN, body: { id: fresh.id } });
  ok(gone.status === 200 && gone.data.revokedAt > 0 && (await call(slow, 'GET', '/v1/key', { key: fresh.key })).status === 403, 'a key can be revoked the moment it is issued, before the list has caught up');
  // A key stored before id records were written is still found.
  slow.KEYS.list = realList;
  const old = (await call(slow, 'POST', '/admin/keys', { key: ADMIN, body: { label: 'Old' } })).data;
  slow.KEYS.map.delete('id:' + old.id);
  ok((await call(slow, 'POST', '/admin/keys/revoke', { key: ADMIN, body: { id: old.id } })).status === 200 && (await call(slow, 'GET', '/v1/key', { key: old.key })).status === 403, 'and one from before that shortcut existed is still found');
  ok((await call(slow, 'GET', '/admin/keys', { key: ADMIN })).data.keys.every((k) => k.label), 'the shortcut records do not show up as keys in the list');
  ok((await call(slow, 'POST', '/admin/keys/revoke', { key: ADMIN, body: { id: '../../etc' } })).status === 404, 'an id that is not an id is not looked up');
}

// ─── Rate limit ───
let allowed = 2;
const limited = { ...env, PER_KEY: { limit: async ({ key }) => ({ success: key.length === 64 && allowed-- > 0 }) } };
const codes = [];
for (let i = 0; i < 3; i++) codes.push(await call(limited, 'POST', '/v1/push', { key: second.key, body: { messages: [msg()] } }));
ok(codes[0].status === 200 && codes[1].status === 200 && codes[2].status === 429 && codes[2].data.error === 'rate_limited' && codes[2].headers.get('retry-after') === '60', 'past the limit a server is told to wait, and for how long');
ok((await call(limited, 'GET', '/v1/key', { key: second.key })).status === 200, 'and can still check its key');

// ─── The contract's words ───
const contract = readFileSync(join(here, '..', 'android', 'RELAY.md'), 'utf8');
for (const word of ['invalid_key', 'key_revoked', 'rate_limited', 'bad_request', '/v1/push', '/v1/key', '/v1/info']) {
  ok(contract.includes(word), 'the contract document names what the relay answers: ' + word);
}
const server = readFileSync(join(here, '..', 'src', 'mobile-push.js'), 'utf8');
const toml = readFileSync(join(here, 'wrangler.toml'), 'utf8');
const addr = /DEFAULT_RELAY_URL = 'https:\/\/([^']+)'/.exec(server)[1];
ok(toml.includes(`pattern = "${addr}"`), `the relay is deployed at the address servers are built to use (${addr})`);
ok(!/ADMIN_TOKEN\s*=|private_key|BEGIN [A-Z ]*PRIVATE KEY/.test(toml), 'and its configuration file holds no secret');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
