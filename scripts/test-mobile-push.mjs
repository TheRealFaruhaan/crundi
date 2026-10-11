#!/usr/bin/env node
/**
 * test-mobile-push.mjs — notifications to the phone app, by way of the relay.
 *
 * The relay here is a function standing in for the real one. What is checked
 * is this server's side of the contract in android/RELAY.md: that nothing
 * leaves without a key, that what leaves can be read by the phone it was
 * sealed for and by nothing else in between, and that each answer the relay
 * can give is acted on.
 *
 * Run: node scripts/test-mobile-push.mjs
 */

import { mkdtempSync, rmSync, readFileSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createECDH, createHmac, createDecipheriv, randomBytes } from 'node:crypto';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dataDir = mkdtempSync(join(tmpdir(), 'mpush-test-'));
process.env.DATA_DIR = dataDir;
const mp = await import('../src/mobile-push.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// ─── A phone: it makes its own keys, and can open what is sealed for them ───
function phone() {
  const ecdh = createECDH('prime256v1'); ecdh.generateKeys();
  const auth = randomBytes(16);
  const hkdf = (salt, ikm, info, len) => createHmac('sha256', createHmac('sha256', salt).update(ikm).digest()).update(Buffer.concat([info, Buffer.from([1])])).digest().subarray(0, len);
  return {
    keys: { p256dh: ecdh.getPublicKey().toString('base64url'), auth: auth.toString('base64url') },
    open(b64) {
      const buf = Buffer.from(b64, 'base64url');
      const salt = buf.subarray(0, 16), idlen = buf[20], serverPub = buf.subarray(21, 21 + idlen), body = buf.subarray(21 + idlen);
      const ikm = hkdf(auth, ecdh.computeSecret(serverPub), Buffer.concat([Buffer.from('WebPush: info\0'), ecdh.getPublicKey(), serverPub]), 32);
      const d = createDecipheriv('aes-128-gcm', hkdf(salt, ikm, Buffer.from('Content-Encoding: aes128gcm\0'), 16), hkdf(salt, ikm, Buffer.from('Content-Encoding: nonce\0'), 12));
      d.setAuthTag(body.subarray(body.length - 16));
      const plain = Buffer.concat([d.update(body.subarray(0, body.length - 16)), d.final()]);
      return JSON.parse(plain.subarray(0, plain.length - 1).toString());
    },
  };
}

// ─── A relay: records what it was sent and answers as told ───
function relay(answer) {
  const seen = [];
  const fetchImpl = async (url, init) => {
    const req = { url, method: init.method, auth: init.headers.Authorization, body: init.body ? JSON.parse(init.body) : null };
    seen.push(req);
    const a = typeof answer === 'function' ? answer(req) : answer;
    if (a instanceof Error) throw a;
    return { status: a.status, headers: { get: (h) => (h === 'retry-after' ? a.retryAfter || null : null) }, json: async () => { if (a.body === undefined) throw new Error('no body'); return a.body; } };
  };
  return { seen, fetchImpl };
}
const allSent = (req) => ({ status: 200, body: { ok: true, results: req.body.messages.map(() => ({ status: 'sent' })) } });
const KEY = 'crp_' + 'a1B2c3D4e5'.repeat(4);
const token = (n) => `fcm-token-${n}-` + 'x'.repeat(120);

// ─── Nothing without a key ───
const p1 = phone();
ok(mp.status().hasKey === false && mp.status().contact.telegram === 'TheRealFaruhaan' && /TheRealFaruhaan/.test(mp.status().keyHelp), 'a new server has no key, and says who to ask for one');
ok(mp.registerDevice({ deviceId: 'dev1', name: 'Pixel 8 Pro' }, { platform: 'android', token: token(1), keys: p1.keys }).ok, 'a paired phone can register before there is a key');
let r = relay(allSent);
ok(await mp.send('hello', {}, { fetchImpl: r.fetchImpl }) === false && r.seen.length === 0, 'with no key, nothing is sent to the relay at all');
ok((await mp.checkKey({ fetchImpl: r.fetchImpl })).ok === false && r.seen.length === 0, 'and there is nothing to check');

// ─── The key ───
for (const bad of ['', 'short', 'has spaces in it and is long enough', 'x'.repeat(201), null]) {
  ok(mp.setKey(bad).ok === false, 'not a key: ' + JSON.stringify(String(bad).slice(0, 24)));
}
ok(mp.setKey(`  ${KEY}\n`).ok && mp.status().hasKey && mp.status().keyHint === '…' + KEY.slice(-4), 'a pasted key is trimmed and stored');
ok(!JSON.stringify(mp.status()).includes(KEY), 'and is never handed back, only its last four characters');
ok((statSync(join(dataDir, 'mobile-push.json')).mode & 0o077) === 0 || process.platform === 'win32', 'the file holding it is readable by its owner only');
ok(mp.status().relayUrl === mp.DEFAULT_RELAY_URL && mp.status().customRelay === false, 'the published relay is used unless another is given');
ok(mp.setKey(KEY, 'http://relay.example.com').ok === false, 'a relay over plain http is refused: the key would travel in the clear');
ok(mp.setKey(KEY, 'https://user:pw@relay.example.com').ok === false && mp.setKey(KEY, 'ftp://relay.example.com').ok === false, 'so is anything that is not a plain https address');
ok(mp.setKey(KEY, 'https://relay.example.org/crundi/').ok && mp.status().relayUrl === 'https://relay.example.org/crundi' && mp.status().customRelay, 'a fork can point at its own relay');
ok(mp.cleanRelayUrl('http://127.0.0.1:9000') === 'http://127.0.0.1:9000' && mp.cleanRelayUrl('http://localhost:9000/') === 'http://localhost:9000', 'plain http is allowed to this machine only, for testing');
ok(mp.setKey(KEY, '').ok && mp.status().customRelay === false, 'and can go back to the published one');


// ─── Somebody else's relay (a custom build of the app) ───
{
  ok(mp.contact().telegram === 'TheRealFaruhaan' && /@TheRealFaruhaan on Telegram/.test(mp.keyHelp()), 'for the published relay, the publisher is who to ask');
  ok(mp.setRelay('https://relay.example.org').ok && mp.status().customRelay && mp.status().hasKey === false, 'changing the relay drops the key: it belongs to the relay that issued it');
  ok(mp.contact() === null && /whoever runs relay\.example\.org/.test(mp.keyHelp()) && !/Faruhaan/.test(mp.keyHelp()), 'and the publisher is no longer who to ask');
  let rr = relay({ status: 200, body: { ok: true, name: 'Acme Crundi', contact: { telegram: '@acme_ops', text: 'Email ops@acme.example for a key.' } } });
  ok((await mp.refreshRelayInfo({ fetchImpl: rr.fetchImpl })).ok && rr.seen[0].url === 'https://relay.example.org/v1/info' && rr.seen[0].auth === undefined, 'the relay is asked who runs it, and no key is sent to ask');
  ok(mp.contact().telegram === 'acme_ops' && mp.contact().url === 'https://t.me/acme_ops' && mp.status().relayName === 'Acme Crundi' && /@acme_ops on Telegram/.test(mp.keyHelp()), 'Settings then names that relay\'s own contact');
  rr = relay({ status: 200, body: { ok: true, name: '<b>x</b>', contact: { telegram: 'no spaces allowed', url: 'javascript:alert(1)', text: '<script>alert(1)</script> call us' } } });
  await mp.refreshRelayInfo({ fetchImpl: rr.fetchImpl });
  ok(mp.contact().telegram === '' && mp.contact().url === '' && !/[<>]/.test(mp.contact().text + mp.status().relayName), 'what a relay says about itself is cut down to what is safe to show');
  rr = relay({ status: 404, body: { ok: false } });
  ok((await mp.refreshRelayInfo({ fetchImpl: rr.fetchImpl })).ok === false, 'an address that is not a relay says so');
  ok((await mp.refreshRelayInfo({ fetchImpl: relay(new Error('ENOTFOUND')).fetchImpl })).ok === false, 'and one that cannot be reached is not an exception');
  mp.setKey(KEY);
  ok(mp.setRelay('https://relay.example.org').ok && mp.status().hasKey, 'setting the same relay again keeps the key');
  ok(mp.setRelay('http://relay.example.org').ok === false && mp.status().hasKey, 'a refused address changes nothing');
  ok(mp.setRelay('').ok && mp.status().customRelay === false && mp.status().hasKey === false && mp.contact().telegram === 'TheRealFaruhaan', 'going back to the published relay drops that key too, and the publisher is who to ask again');
  ok(mp.setRelay(mp.DEFAULT_RELAY_URL + '/').ok && mp.status().customRelay === false, 'the published address typed out is not a custom relay');
  mp.setKey(KEY);
}

r = relay({ status: 200, body: { ok: true, label: 'Test server' } });
let chk = await mp.checkKey({ fetchImpl: r.fetchImpl });
ok(chk.ok && chk.label === 'Test server' && r.seen[0].method === 'GET' && r.seen[0].url === mp.DEFAULT_RELAY_URL + '/v1/key' && r.seen[0].auth === 'Bearer ' + KEY, 'checking a key asks the relay, with the key as a bearer token');
for (const [status, words] of [[401, 'does not recognise'], [403, 'revoked'], [429, 'too many'], [500, '500']]) {
  chk = await mp.checkKey({ fetchImpl: relay({ status, body: { ok: false, error: 'x' } }).fetchImpl });
  ok(!chk.ok && chk.error.includes(words) && mp.status().lastError === chk.error, `a ${status} from the relay is explained in words`);
}
chk = await mp.checkKey({ fetchImpl: relay(new Error('getaddrinfo ENOTFOUND')).fetchImpl });
ok(!chk.ok && /Could not reach the relay/.test(chk.error), 'a relay that cannot be reached is not an exception');

// ─── Sending ───
const p2 = phone();
mp.registerDevice({ deviceId: 'dev2', name: 'iPhone' }, { platform: 'ios', token: token(2), keys: p2.keys });
r = relay(allSent);
ok(await mp.send('Claude finished in crundi.', { tag: 'finished', title: 'Crundi', url: '/?chat=abc' }, { fetchImpl: r.fetchImpl }) === true, 'a notification goes out');
const sentReq = r.seen[0];
ok(r.seen.length === 1 && sentReq.url === mp.DEFAULT_RELAY_URL + '/v1/push' && sentReq.auth === 'Bearer ' + KEY && sentReq.body.messages.length === 2, 'one request, with the key, carrying one message per phone');
const [m1, m2] = sentReq.body.messages;
ok(m1.platform === 'android' && m1.token === token(1) && m2.platform === 'ios' && m1.priority === 'high' && m1.collapse === 'finished' && m1.ttl > 0, 'each message names its phone, platform, priority and collapse tag');
ok(!JSON.stringify(sentReq.body).includes('Claude finished') && !JSON.stringify(sentReq.body).includes('chat=abc'), 'the relay is handed no readable text');
const got1 = p1.open(m1.payload), got2 = p2.open(m2.payload);
ok(got1.v === 1 && got1.body === 'Claude finished in crundi.' && got1.title === 'Crundi' && got1.kind === 'finished' && got1.url === '/?chat=abc' && got1.at > 0, 'the phone it was sealed for reads it back whole');
ok(got2.body === got1.body && m1.payload !== m2.payload, 'each phone gets its own sealing of the same notification');
let crossed = false;
try { p1.open(m2.payload); crossed = true; } catch { /* as it should be */ }
ok(!crossed, 'and one phone cannot open another\'s');
ok(mp.devices().every((d) => d.lastOkAt > 0) && mp.status().lastError === '', 'a delivery is remembered against each phone');

// A notification about one chat says which, so the phone can file it.
r = relay(allSent);
const MARK = { initials: 'C', hue: 316, color: '#b93195', colorDeep: '#8c2170' };
await mp.send('Done.', { tag: 'finished', url: '/chat/abc123', chat: { id: 'abc123', title: 'Fix the login redirect', project: 'crundi', mark: MARK } }, { fetchImpl: r.fetchImpl });
const about = p1.open(r.seen[0].body.messages[0].payload);
ok(about.chat && about.chat.id === 'abc123' && about.chat.title === 'Fix the login redirect' && about.chat.project === 'crundi' && about.url === '/chat/abc123', 'a notification about a chat carries its id, title and project');
ok(about.chat.mark.initials === 'C' && about.chat.mark.color === '#b93195' && about.chat.mark.colorDeep === '#8c2170', 'and the project\'s mark, so the phone can draw the bubble\'s head without asking');
ok(r.seen[0].body.messages[0].collapse === 'finished.abc123' && !JSON.stringify(r.seen[0].body).includes('Fix the login'), 'it collapses per chat, and the relay still reads none of it');
ok(!('chat' in got1), 'a notification that is not about a chat has no chat in it');
r = relay(allSent);
await mp.send('x', { tag: 'finished', chat: { id: 'x'.repeat(200), title: 'T'.repeat(500), project: 'p', mark: null } }, { fetchImpl: r.fetchImpl });
const big = p1.open(r.seen[0].body.messages[0].payload);
ok(big.chat.id.length === 80 && Buffer.byteLength(big.chat.title) <= 120 && big.chat.mark === null && r.seen[0].body.messages[0].collapse.length <= 40, 'over-long chat details are cut to size');

// A change of state, for the live card.
r = relay(allSent);
await mp.send('', { tag: 'state', silent: true, state: 'working', since: 1791740000000, url: '/chat/abc123', chat: { id: 'abc123', title: 'Fix the login redirect', project: 'crundi', mark: MARK } }, { fetchImpl: r.fetchImpl });
const st = p1.open(r.seen[0].body.messages[0].payload);
ok(st.kind === 'state' && st.silent === true && st.state === 'working' && st.since === 1791740000000 && st.body === '' && st.chat.id === 'abc123', 'a change of state says which chat, what state, and since when, and carries no text');
ok(r.seen[0].body.messages[0].collapse === 'state.abc123', 'only the latest state of a chat waits for a phone that is offline');
ok(!('silent' in about) && !('state' in about), 'an ordinary notification is not marked silent');
const srv = readFileSync(join(root, 'src', 'webapp.js'), 'utf8');
ok(/tellPhonesState\(term, state, prev\);/.test(srv) && /if \(term\.kind !== 'ui' \|\| term\.collaborator \|\| term\.background\) return;/.test(srv), 'the server reports every change of an owner chat\'s state, and nobody else\'s');
ok(/if \(!channels\.isEnabled\('mobile'\)\) return;\s+mobilePush\.send\(''/.test(srv), 'and only while the Phone app channel is on');
ok(/claudeUi\.onSessionGone\(\(sid\) => \{[^}]*tellPhonesGone\(sid\);/.test(srv), 'a chat closed while on a card is reported as finished, so its line does not linger');

// A permission request the phone can answer.
r = relay(allSent);
await mp.send('Claude needs your input.', { tag: 'needsInput', chat: { id: 'abc123', title: 'T', project: 'crundi', mark: MARK }, prompt: { requestId: 'req_1', tool: 'Bash', summary: 'npm run build' } }, { fetchImpl: r.fetchImpl });
const ask = p1.open(r.seen[0].body.messages[0].payload);
ok(ask.prompt && ask.prompt.requestId === 'req_1' && ask.prompt.tool === 'Bash' && ask.prompt.summary === 'npm run build', 'a notification about one waiting request says what Claude wants to do');
ok(!('prompt' in about), 'and one that is not about a request offers nothing to approve');
const ui = readFileSync(join(root, 'src', 'claude-ui.js'), 'utf8');
ok(/tool === 'AskUserQuestion' \|\| tool === 'ExitPlanMode'\) continue;/.test(ui), 'a question or a plan is never offered as a yes or no');
ok(/if \(asks\.length === 1\) about\.prompt = asks\[0\];/.test(srv), 'nor is anything offered when more than one request is waiting');

// Long text, and text that is mostly multi-byte.
r = relay(allSent);
await mp.send('漢'.repeat(4000), { tag: 'finished' }, { fetchImpl: r.fetchImpl });
const long = r.seen[0].body.messages[0];
ok(long.payload.length <= 3000, `a very long notification still fits what Google and Apple allow (${long.payload.length} characters)`);
const longBody = p1.open(long.payload).body;
ok(longBody.endsWith('…') && !longBody.includes('�') && /^漢+…$/.test(longBody), 'and is cut between characters, not through one');

// ─── What the relay can answer ───
r = relay({ status: 200, body: { ok: true, results: [{ status: 'gone' }, { status: 'sent' }] } });
ok(await mp.send('x', {}, { fetchImpl: r.fetchImpl }) === true && mp.devices().length === 1 && mp.devices()[0].id === 'dev2', 'a phone whose registration has ended is dropped; the others still get theirs');
mp.registerDevice({ deviceId: 'dev1', name: 'Pixel 8 Pro' }, { platform: 'android', token: token(1), keys: p1.keys });
r = relay({ status: 200, body: { ok: true, results: [{ status: 'failed', error: 'unavailable' }, { status: 'failed', error: 'unavailable' }] } });
ok(await mp.send('x', {}, { fetchImpl: r.fetchImpl }) === false && mp.devices().length === 2, 'a delivery that merely failed costs nobody their registration');
r = relay({ status: 403, body: { ok: false, error: 'key_revoked' } });
ok(await mp.send('x', {}, { fetchImpl: r.fetchImpl }) === false && /revoked/.test(mp.status().lastError) && mp.devices().length === 2, 'a revoked key stops delivery and says why, and forgets no phone');
r = relay(new Error('socket hang up'));
ok(await mp.send('x', {}, { fetchImpl: r.fetchImpl }) === false && /Could not reach/.test(mp.status().lastError), 'a relay that is down is a missed notification, not a crash');
r = relay({ status: 200, body: { ok: true, results: 'nonsense' } });
ok(await mp.send('x', {}, { fetchImpl: r.fetchImpl }) === false, 'an answer that is not the contract is not taken for success');

// More phones than fit one request.
for (let i = 10; i < 55; i++) mp.registerDevice({ deviceId: 'd' + i, name: 'Phone ' + i }, { platform: 'android', token: token(i), keys: phone().keys });
r = relay(allSent);
await mp.send('x', {}, { fetchImpl: r.fetchImpl });
ok(r.seen.length === 3 && r.seen.every((q) => q.body.messages.length <= 20) && r.seen.reduce((n, q) => n + q.body.messages.length, 0) === 47, 'many phones are sent in requests of at most twenty');
for (let i = 10; i < 55; i++) mp.removeDevice('d' + i);

// ─── Registering ───
const good = { platform: 'android', token: token(9), keys: phone().keys };
ok(mp.registerDevice({ deviceId: '', name: 'x' }, good).ok === false, 'a login that is not a paired device cannot register');
for (const [label, reg] of [['an unknown platform', { ...good, platform: 'toaster' }], ['a token too short to be one', { ...good, token: 'abc' }], ['a key of the wrong length', { ...good, keys: { ...good.keys, p256dh: 'AAAA' } }], ['no keys', { platform: 'android', token: token(9) }], ['nothing', null]]) {
  ok(mp.registerDevice({ deviceId: 'dev9', name: 'x' }, reg).ok === false, 'refused: ' + label);
}
const before = mp.devices().find((d) => d.id === 'dev1').addedAt;
mp.registerDevice({ deviceId: 'dev1', name: 'Pixel 8 Pro' }, { platform: 'android', token: token(77), keys: p1.keys });
ok(mp.devices().filter((d) => d.id === 'dev1').length === 1 && mp.devices().find((d) => d.id === 'dev1').addedAt === before, 'registering again replaces the token and stays one device');
ok(!JSON.stringify(mp.devices()).includes('fcm-token') && !JSON.stringify(mp.status()).includes(p1.keys.auth), 'the list for Settings carries no tokens and no keys');
ok(mp.removeDevice('dev1').removed === 1 && mp.removeDevice('dev1').removed === 0, 'a phone can be forgotten, once');

// ─── It survives a restart, and clearing the key ───
mp._reset();
ok(mp.status().hasKey && mp.devices().length === 1, 'the key and the phones are still there after a restart');
mp.clearKey();
r = relay(allSent);
ok(mp.status().hasKey === false && await mp.send('x', {}, { fetchImpl: r.fetchImpl }) === false && r.seen.length === 0 && mp.devices().length === 1, 'removing the key stops everything and keeps the phones');

// ─── Wired in ───
const src = readFileSync(join(root, 'src', 'webapp.js'), 'utf8');
ok(/id: 'mobile',\s+label: 'Phone app'/.test(src) && src.includes('send: (text, meta) => mobilePush.send(text, meta)'), 'it is a notification channel like the others');
ok(/if \(m\?\.device\?\.id\) mobilePush\.removeDevice\(m\.device\.id\)/.test(src), 'revoking a device stops its notifications');
ok(/const device = families\.get\(tokens\.get\(extractToken\(req\)\)\?\.familyId\)\?\.device;\s+if \(!device\?\.id\) return json/.test(src), 'which phone is registering is read from its login, not from what it says');
const gate = src.indexOf('if (principal && !mayAccess(principal, req.method, path))');
ok(gate > 0 && src.indexOf("path === '/api/push/mobile'") > gate, 'every phone push route is behind the login and the collaborator policy');
const { mayAccess, ROLE_COLLABORATOR } = await import('../src/access-policy.js');
const collab = { username: 'g', role: ROLE_COLLABORATOR, collabKey: 'tg:g', projects: ['p'], roots: {} };
ok(['/api/push/mobile', '/api/push/mobile/key', '/api/push/mobile/relay', '/api/push/mobile/register', '/api/push/mobile/test'].every((p) => !mayAccess(collab, 'POST', p) && !mayAccess(collab, 'GET', p)), 'and none of them is open to a collaborator');

rmSync(dataDir, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
