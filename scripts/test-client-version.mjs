#!/usr/bin/env node
/**
 * test-client-version.mjs — which clients the server refuses for being too old.
 *
 * The cases that matter are the ones that must NOT be refused: a caller that
 * does not say what it is (the MCP bridge, a hook, curl) has nothing to compare
 * and would take every script on the machine down with it, and an out-of-date
 * client must still reach the page, the handshake and the login, or it has no
 * way to be told to update.
 *
 * The user agents are real ones, copied from a server's session list.
 *
 * Run: node scripts/test-client-version.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as cv from '../src/client-version.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const req = (headers = {}, url = '/api/status') => ({ url, headers });

const UA_CLIENT = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) CrundiClient/1.19.23 Chrome/146.0.7680.179 Electron/41.2.0 Safari/537.36';
const UA_ALLINONE = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Crundi/1.19.27 Chrome/146.0.7680.179 Electron/41.2.0 Safari/537.36';
const UA_CHROME = 'Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Mobile Safari/537.36';
const UA_ANDROID = UA_CHROME + ' CrundiAndroid/1.2.0 (play)';

// ─── Who is calling ───
let c = cv.identifyClient(req({ 'user-agent': UA_CLIENT }));
ok(c.kind === 'desktop' && c.version === '1.19.23', 'a released desktop client is recognised from the user agent it already sends');
c = cv.identifyClient(req({ 'user-agent': UA_ALLINONE }));
ok(c.kind === 'desktop' && c.version === '1.19.27', 'the all-in-one desktop app is recognised too');
c = cv.identifyClient(req({ 'user-agent': UA_ANDROID }));
ok(c.kind === 'android' && c.version === '1.2.0' && c.flavor === 'play', 'the Android shell is recognised, with how it was installed');
c = cv.identifyClient(req({ 'user-agent': UA_CHROME }));
ok(c.kind === '', 'a plain browser that sends no page version is not identified');
c = cv.identifyClient(req({ 'user-agent': UA_CHROME, 'x-crundi-web': '1.19.27' }));
ok(c.kind === 'web' && c.version === '1.19.27', 'a page that sends its version is a web client');
c = cv.identifyClient(req({ 'user-agent': UA_CLIENT, 'x-crundi-web': '1.19.20' }));
ok(c.kind === 'desktop' && c.web === '1.19.20', 'a shell carries the version of the page inside it as well');
c = cv.identifyClient(req({ 'x-crundi-client': 'android-github/1.0.0-dev.3' }));
ok(c.kind === 'android' && c.flavor === 'github' && c.version === '1.0.0-dev.3', 'an explicit tag is read, prerelease and all');
c = cv.identifyClient(req({}, '/ws?token=abc&client=ios/2.0.0&web=1.19.27'));
ok(c.kind === 'ios' && c.version === '2.0.0' && c.web === '1.19.27', 'a socket, which cannot set headers, says it in the query');
c = cv.identifyClient(req({ 'x-crundi-client': 'toaster/1.0.0', 'user-agent': 'curl/8.5.0' }));
ok(c.kind === '', 'a kind nobody has heard of is not identified');
ok(cv.identifyClient(req({ 'x-crundi-web': '<script>' })).kind === '', 'a version that is not a version is ignored');

// ─── Who is refused ───
const MIN = { web: '1.20.0', desktop: '1.19.25', android: '1.2.0', ios: '' };
const refused = (headers, url) => cv.refusalFor(cv.identifyClient(req(headers, url)), MIN);

ok(refused({ 'user-agent': 'curl/8.5.0' }) === null, 'curl is never refused');
ok(refused({}) === null, 'a caller with no headers at all (the MCP bridge, a hook) is never refused');
ok(refused({ 'user-agent': UA_CHROME }) === null, 'a page from before the gate existed is not refused');
let r = refused({ 'user-agent': UA_CLIENT });
ok(r && r.kind === 'desktop' && r.minimum === '1.19.25' && r.version === '1.19.23', 'a desktop client below the minimum is refused, and told both versions');
ok(refused({ 'user-agent': UA_ALLINONE, 'x-crundi-web': '1.20.0' }) === null, 'a desktop client at or above it is not');
ok(refused({ 'x-crundi-client': 'android-play/1.2.0', 'x-crundi-web': '1.20.0' }) === null, 'exactly the minimum is accepted');
ok(!!refused({ 'x-crundi-client': 'android-play/1.2.0-dev.9' }), 'a prerelease of the minimum is below it');
ok(refused({ 'x-crundi-client': 'ios/0.0.1' }) === null, 'a kind with no minimum accepts anything');
r = refused({ 'user-agent': UA_CHROME, 'x-crundi-web': '1.19.27' });
ok(r && r.kind === 'web' && r.minimum === '1.20.0', 'a page below the web minimum is refused');
r = refused({ 'user-agent': UA_ALLINONE, 'x-crundi-web': '1.19.27' });
ok(r && r.kind === 'web', 'a current shell showing a stale page is refused for the page');
r = refused({ 'user-agent': UA_CLIENT, 'x-crundi-web': '1.19.27' });
ok(r && r.kind === 'desktop', 'when both are stale the shell is reported, being the one somebody must update by hand');
ok(cv.refusalFor(cv.identifyClient(req({ 'user-agent': UA_CLIENT })), { web: '', desktop: '', android: '', ios: '' }) === null,
  'with no minimums set, nothing is refused');

// ─── The shipped minimums ───
// The page and the all-in-one desktop app carry the server's own version. If a
// minimum is ever set above that, the server refuses itself: a login screen
// that reloads once and then says "update required" to everybody.
const own = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
const { compareVersions } = await import('../src/whatsnew.js');
for (const kind of ['web', 'desktop']) {
  const min = cv.MIN_CLIENT[kind];
  ok(!min || compareVersions(own, min) >= 0, `this server (${own}) meets its own ${kind} minimum (${min || 'none'})`);
}
ok(cv.refusalFor(cv.identifyClient(req({ 'user-agent': UA_CLIENT })), cv.MIN_CLIENT)?.kind === 'desktop',
  'a desktop client from before the check existed is refused by the shipped minimums');
ok(cv.refusalFor(cv.identifyClient(req({ 'user-agent': UA_CHROME, 'x-crundi-web': own })), cv.MIN_CLIENT) === null,
  'and this server\'s own page is not');

// ─── What an out-of-date client can still reach ───
for (const p of ['/', '/sw.js', '/manifest.webmanifest', '/vendor/xterm.js', '/assets/icon_128x128.png', cv.HANDSHAKE_PATH, '/api/auth/password', '/api/auth/refresh']) {
  ok(cv.isGated(p) === false, 'open to an old client: ' + p);
}
for (const p of ['/api/status', '/api/ui-sessions', '/api/ui-sessions/abc/send', '/ws', '/api/files/upload']) {
  ok(cv.isGated(p) === true, 'closed to an old client: ' + p);
}

// ─── The handshake ───
let h = cv.handshake(req({ 'user-agent': UA_CLIENT }), '1.20.0', MIN);
ok(h.ok === true && h.server.version === '1.20.0' && h.server.minClient.desktop === '1.19.25' && Array.isArray(h.server.capabilities),
  'the handshake states the server version, its minimums and what it can do');
ok(h.client.accepted === false && h.client.refused.minimum === '1.19.25', 'and tells a stale client it is refused, and why');
h = cv.handshake(req({ 'user-agent': 'curl/8.5.0' }), '1.20.0', MIN);
ok(h.client.accepted === true && h.client.refused === null && h.client.kind === '', 'an unidentified caller is told it is accepted');
const body = cv.refusalBody({ kind: 'desktop', flavor: '', version: '1.19.23', minimum: '1.19.25' }, '1.20.0');
ok(body.ok === false && body.error === 'client_too_old' && body.client.version === '1.19.23' && body.minimum === '1.19.25' && body.server === '1.20.0',
  'a refusal carries the client version, the minimum and the server version');

// ─── The override used to try it out ───
const m = cv.readMinimums({ CRUNDI_MIN_CLIENT: 'web=1.20.0, desktop=9.0.0,toaster=1.0.0,ios=nope' });
ok(m.web === '1.20.0' && m.desktop === '9.0.0' && m.ios === '' && !('toaster' in m), 'CRUNDI_MIN_CLIENT sets known kinds and ignores the rest');

// ─── It is actually wired in ───
const server = readFileSync(join(root, 'src', 'webapp.js'), 'utf8');
const gateAt = server.indexOf('clientVersion.isGated(path)');
ok(gateAt > 0 && gateAt < server.indexOf("// Serve web app"), 'the server checks the gate before it routes anything');
ok(/refusalFor\(clientVersion\.identifyClient\(req\)\)\) \{\s+socket\.write\('HTTP\/1\.1 426/.test(server), 'and before it accepts a socket');
const page = readFileSync(join(root, 'src', 'webapp-html.js'), 'utf8');
ok(page.includes("opts.headers['X-Crundi-Web'] = PAGE_VERSION") && page.includes('res.status === 426'), 'the page sends its version and handles a refusal');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
