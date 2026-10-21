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
ok(page.includes("const needsDev = String(body.minimum).indexOf('-') >= 0;") && page.includes("updater.setUpdateChannel('dev')") && page.includes("s.channel !== 'dev'"), 'a client on production releases, refused for want of a dev build, is offered the dev channel from the refusal screen');

// ─── The phone app signs in by pairing only ───
const appReq = { url: '/', headers: { 'user-agent': 'Mozilla/5.0 (Linux; Android 16; wv) Chrome/154 Mobile Safari/537.36 CrundiAndroid/1.0.1 (play)' } };
const webReq = { url: '/', headers: { 'user-agent': 'Mozilla/5.0 (Linux; Android 10; K) Chrome/155 Mobile Safari/537.36' } };
ok(['/api/auth', '/api/auth/password', '/api/auth/collab', '/api/auth/webapp', '/api/auth/setup', '/auth/telegram/callback'].every((p) => cv.isUnpairedSignIn(appReq, p)), 'every way of signing in that is not pairing is refused to the phone app');
ok(['/api/auth/refresh', '/api/auth/logout', '/api/auth/methods', '/api/pair/claim', '/api/ui-sessions'].every((p) => !cv.isUnpairedSignIn(appReq, p)), 'renewing, signing out and pairing are not');
ok(!cv.isUnpairedSignIn(webReq, '/api/auth/password'), 'a browser on the same phone signs in as it always did');
ok(cv.PAIR_REQUIRED.code === 'pair_required' && cv.PAIR_REQUIRED.ok === false, 'the refusal names itself, so the page can show pairing and not an error');
const refuseAt = server.indexOf('clientVersion.isUnpairedSignIn(req, path)');
ok(refuseAt > 0 && refuseAt < server.indexOf("// Serve web app"), 'and the server asks before any sign-in route can answer');
ok(page.includes("navigator.userAgent.indexOf('CrundiAndroid/') >= 0) { showPairAgain(); return; }"), 'the page shows the app a way to pair, not a sign-in form');
ok(server.includes("entry.answered === false") && server.includes('entry.answered = false;'), 'a renewal whose answer was never used is answered again, and only that one');


// ─── The phone app keeps its page's login ───
// Two pages of the app (the main window, a bubble) each holding the page's
// refresh token got the phone cut off as if it had been stolen. The app now
// keeps that login and renews it for all of them.
const chatPage = readFileSync(join(root, 'app', 'vendor', 'crundi-chat-page.js'), 'utf8');
ok(cv.CAPABILITIES.includes('app-page-login'), 'the server says it can give the app a page login to keep');
const pl = server.slice(server.indexOf("path === '/api/pair/page-login'"), server.indexOf('// ─── SSE endpoint ───'));
ok(pl.length > 0 && /dev\.role !== 'background'/.test(pl), 'only a paired phone\'s own background login may ask for its page login');
ok(/f\.device\?\.role !== 'page'/.test(pl) && !pl.includes('revokeFamily('), 'it retires that phone\'s old page login alone, not the whole phone');
ok(/role: 'page' \}\)/.test(pl), 'and the new one is the phone\'s page login, listed and revoked with the phone');
ok(page.includes('const APP_LOGIN = !!window.CrundiLogin;') && chatPage.includes('var APP_LOGIN = !FRAME_LOGIN && !!window.CrundiLogin;'), 'both pages take the login from the app when it offers one');
ok(page.includes('window.crundiFrameSession = {') && /FRAME_LOGIN = \(function \(\) \{ try \{ return window\.top !== window\.self && window\.top\.location\.origin === location\.origin && window\.top\.crundiFrameSession/.test(chatPage), 'a chat page embedded in the main page (the live graph) borrows its login, which works inside the app too');
const mainLogin = page.slice(page.indexOf('const APP_LOGIN'), page.indexOf('function clearSession()'));
ok(/if \(APP_LOGIN\) \{ token = d\.token; scheduleRefresh\(d\.expiresIn\); return true; \}/.test(mainLogin), 'and keep nothing of it in their own storage');
ok(page.includes("askAppLogin('renew')") && chatPage.includes("askAppLogin('renew')"), 'they renew by asking the app, never with a refresh token of their own');
ok(server.includes('entry.spentBy = renewer(req);') && /presented again by \$\{renewer\(req\)\}/.test(server), 'a replayed renewal is logged with who spent it and who brought it back');

// A dropped connection inside the app checks the token; it does not renew it.
ok(page.includes("APP_LOGIN) await apiFetch('/api/session-check')") && chatPage.includes("return apiFetch('/api/session-check')"), 'a reconnect inside the app checks the login instead of renewing it');
ok(!/setTimeout\(function \(\) \{ if \(!stopped\) refreshSession\(\)/.test(chatPage) && !/retry % 3 === 0\) refreshSession\(\)/.test(chatPage), 'the bubble page does not renew on every reconnect');
ok(/path === '\/api\/session-check'[\s\S]{0,120}validateToken\(req\)/.test(server), 'the check is a plain yes or no on the token');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
