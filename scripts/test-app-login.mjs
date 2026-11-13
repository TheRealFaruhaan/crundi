#!/usr/bin/env node
/**
 * test-app-login.mjs — signing an app in through the browser (src/app-login.js).
 *
 * What has to hold: the link's id can approve but never collect; only the
 * app's pollId collects, once; denial and expiry end it; the page sees the
 * device and the code to compare, and nothing once it is decided.
 *
 * Run: node scripts/test-app-login.mjs
 */

import { createAppLogins, APP_LOGIN_TTL_MS } from '../src/app-login.js';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

let t = 1_000_000;
const a = createAppLogins({ now: () => t });

const s = a.start({ name: 'Faruhaan\u0007 PC', platform: 'desktop' });
ok(s && /^[a-f0-9]{24}$/.test(s.id) && /^[a-f0-9]{48}$/.test(s.pollId) && s.id !== s.pollId, 'an id for the link and a separate secret to collect with');
ok(/^[A-HJ-NP-Z2-9]{4}-[A-HJ-NP-Z2-9]{4}$/.test(s.confirmCode), 'a code to compare, without look-alike letters');
ok(s.expiresIn === APP_LOGIN_TTL_MS / 1000, 'it says how long it lasts');

const info = a.info(s.id);
ok(info && info.device.name === 'Faruhaan PC' && info.device.platform === 'desktop' && info.confirmCode === s.confirmCode, 'the page sees the device (cleaned) and the code');
ok(a.info('nope') === null, 'an unknown id shows nothing');

ok(a.poll(s.pollId).status === 'waiting', 'the app waits while nobody has answered');
ok(a.poll(s.id).status === 'expired', 'the link id cannot collect the login');

let made = null;
ok(a.approve(s.id, (device) => { made = device; return { username: 'owner', device: { id: 'd1', ...device }, sessions: { page: { token: 'T', refreshToken: 'R', expiresIn: 900 } } }; }), 'allowing it makes the login');
ok(made && made.name === 'Faruhaan PC', 'the login is made for the device that asked');
ok(a.info(s.id) === null, 'once decided, the page shows nothing');
ok(a.approve(s.id, () => ({})) === false, 'it cannot be allowed twice');
const got = a.poll(s.pollId);
ok(got.status === 'approved' && got.sessions.page.token === 'T' && got.username === 'owner', 'the app collects its login');
ok(a.poll(s.pollId).status === 'expired', 'and only once');

const d = a.start({ name: 'Other' });
ok(a.deny(d.id) && a.poll(d.pollId).status === 'denied', 'denied is told to the app');
ok(a.poll(d.pollId).status === 'expired', 'and then forgotten');

const e = a.start({ name: 'Late' });
t += APP_LOGIN_TTL_MS + 1;
ok(a.poll(e.pollId).status === 'expired' && a.info(e.id) === null && a.approve(e.id, () => ({})) === false, 'after ten minutes it is gone');

const many = createAppLogins({ now: () => t });
let last = null;
for (let i = 0; i < 101; i++) last = many.start({ name: 'x' + i });
ok(last === null, 'a cap on pending logins (nobody can fill memory)');

console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
