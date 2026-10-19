#!/usr/bin/env node
/**
 * test-pairing.mjs — the one-time codes that turn a phone into a signed-in device.
 *
 * A pairing code is a login for whoever asked for it, handed over on a screen.
 * What has to hold: it works once, it stops working by itself, a wrong one
 * says nothing about the right ones, and the device it creates can be cut off
 * as a whole.
 *
 * Run: node scripts/test-pairing.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createPairing, pairingLink, parsePairingLink, cleanDevice, CODE_TTL_MS, CLAIMED_TTL_MS } from '../src/pairing.js';
import { mayAccess, ROLE_COLLABORATOR, ROLE_OWNER } from '../src/access-policy.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// ─── Codes ───
let t = 1_000_000;
const p = createPairing({ now: () => t });
const a = p.start({ username: 'owner' });
ok(a.ok && /^[a-f0-9]{24}$/.test(a.code) && a.expiresIn === CODE_TTL_MS / 1000, 'a code is 96 random bits and says how long it lasts');
ok(p.start({ username: 'owner' }).code !== a.code, 'two codes are never the same');
ok(p.start({ username: '' }).ok === false, 'nobody signed in, no code');
let who = p.claim(a.code);
ok(who && who.username === 'owner' && who.collabKey === '', 'claiming a code gives back who asked for it');
ok(p.claim(a.code) === null, 'and works exactly once');
ok(p.claim('') === null && p.claim(null) === null && p.claim('zz') === null && p.claim({}) === null, 'nonsense is not a code');
ok(p.claim(a.code.toUpperCase()) === null, 'nor is a near miss');

const c = p.start({ username: 'guest', collabKey: 'tg:guest' });
who = p.claim(c.code);
ok(who && who.collabKey === 'tg:guest', 'a collaborator\'s code carries their confinement with it');

const e = p.start({ username: 'owner' });
t += CODE_TTL_MS - 1;
const stillGood = createPairing({ now: () => t });
ok(!!stillGood, 'setup');
t += 1;
ok(p.claim(e.code) === null, 'a code is dead the moment its time is up');

const f = p.start({ username: 'owner' });
ok(p.cancel(f.code) === true && p.claim(f.code) === null, 'a code that was shown and withdrawn cannot be used');
ok(p.cancel('nope') === false, 'withdrawing one that is not there is not an error');

const flood = createPairing({ now: () => t });
let made = 0, refused = null;
for (let i = 0; i < 200; i++) { const r = flood.start({ username: 'owner' }); if (r.ok) made++; else refused = refused || r; }
ok(made === 50 && refused && /Too many/.test(refused.error), 'the number waiting at once has a ceiling');
t += CODE_TTL_MS;
ok(flood.size() === 0 && flood.start({ username: 'owner' }).ok, 'and expired ones make room again');

// ─── Telling the screen that it worked ───
{
  let now = 5_000_000;
  const q = createPairing({ now: () => now });
  const owner = { username: 'owner', collabKey: '' };
  const s = q.start(owner);
  let st = q.status(s.code, owner);
  ok(st.state === 'waiting' && st.expiresIn === CODE_TTL_MS / 1000, 'a code not yet used is waiting, and says for how long');
  ok(q.status(s.code, { username: 'someone-else', collabKey: '' }).state === 'expired', 'to anyone but whoever asked for it, it does not exist');
  ok(q.status(s.code, { username: 'owner', collabKey: 'tg:guest' }).state === 'expired', 'nor to a collaborator who happens to share the name');
  ok(q.status('f'.repeat(24), owner).state === 'expired' && q.status(null, owner).state === 'expired', 'a code that never existed reads the same as one that ran out');
  const got = q.claim(s.code, { name: 'Pixel 8 Pro', platform: 'android' });
  ok(got && got.device.name === 'Pixel 8 Pro', 'claiming records which device it was');
  st = q.status(s.code, owner);
  ok(st.state === 'paired' && st.device.name === 'Pixel 8 Pro' && st.device.platform === 'android', 'and the screen is told it worked, and what connected');
  ok(q.claim(s.code, { name: 'Second phone' }) === null && q.status(s.code, owner).device.name === 'Pixel 8 Pro', 'a spent code still cannot be claimed again, and its story does not change');
  ok(q.size() === 0, 'a spent code does not count against the number waiting');
  now += CLAIMED_TTL_MS;
  ok(q.status(s.code, owner).state === 'expired', 'and is forgotten soon after');
  const s2 = q.start(owner);
  q.claim(s2.code, {}); q.cancel(s2.code);
  ok(q.status(s2.code, owner).state === 'expired', 'a claim that fell through is forgotten outright, not reported as a success');
  now += CODE_TTL_MS;
  const s3 = q.start(owner);
  now += CODE_TTL_MS;
  ok(q.status(s3.code, owner).state === 'expired', 'an unused code that ran out is expired');
}

// ─── The link in the QR code ───
const link = pairingLink('https://dev.example.com/', a.code);
ok(link === `https://dev.example.com/pair#${a.code}`, 'the link is the server address with the code in the fragment');
ok(!new URL(link).search && !new URL(link).pathname.includes(a.code), 'so the code is never part of what a browser sends');
let parsed = parsePairingLink(link);
ok(parsed && parsed.baseUrl === 'https://dev.example.com' && parsed.code === a.code, 'a scanned link gives back the server and the code');
parsed = parsePairingLink(`http://192.168.1.20:8888/pair#${a.code}`);
ok(parsed && parsed.baseUrl === 'http://192.168.1.20:8888', 'a LAN address with a port survives');
for (const bad of ['', 'not a link', `https://x.example/other#${a.code}`, 'https://x.example/pair#short', `javascript:alert(1)//pair#${a.code}`, `crundi://x/pair#${a.code}`, `https://x.example/pair?c=${a.code}`]) {
  ok(parsePairingLink(bad) === null, 'not a pairing link: ' + JSON.stringify(bad.slice(0, 40)));
}

// ─── What a device says about itself ───
ok(cleanDevice({ name: '  Pixel 8\n\tPro  ', platform: 'android' }).name === 'Pixel 8 Pro', 'a device name is tidied');
ok(cleanDevice({ name: 'x'.repeat(500) }).name.length === 60, 'and cut to a length that fits a list');
ok(cleanDevice({}).name === 'Unnamed device' && cleanDevice(null).name === 'Unnamed device', 'a device with no name still gets one');
ok(cleanDevice({ platform: 'toaster' }).platform === '' && cleanDevice({ platform: 'ios' }).platform === 'ios', 'only known platforms are kept');

// ─── Who may ask for a code ───
const collab = { username: 'guest', role: ROLE_COLLABORATOR, collabKey: 'tg:guest', projects: ['p'], roots: {} };
ok(mayAccess(collab, 'POST', '/api/pair/start') && mayAccess(collab, 'POST', '/api/pair/cancel') && mayAccess(collab, 'POST', '/api/pair/status'), 'a collaborator may pair their own phone');
ok(mayAccess({ username: 'o', role: ROLE_OWNER }, 'POST', '/api/pair/start'), 'and so may the owner');

// ─── Wired in the way the comments promise ───
// Line endings normalised: a Windows checkout has CRLF, and the search for the
// end of the claim route below is for a bare LF.
const src = readFileSync(join(root, 'src', 'webapp.js'), 'utf8').split(/\r?\n/).join('\n');
ok(/login\('page'\), background: login\('background'\)/.test(src), 'a paired device gets two logins, one per role');
ok(/if \(m\?\.device\?\.id\) for \(const \[fid, f\] of families\) if \(f\.device\?\.id === m\.device\.id\) gone\.add\(fid\)/.test(src), 'revoking one login of a device revokes all of them');
ok(/\.\.\.\(\(device \|\| prev\?\.device\) \? \{ device: device \|\| prev\.device \} : \{\}\)/.test(src), 'the device survives a token rotation');
const claimAt = src.indexOf("path === '/api/pair/claim'");
const claimBody = src.slice(claimAt, src.indexOf('\n    }\n', claimAt));
ok(claimAt > 0 && !/validateToken\(req\)/.test(claimBody), 'claiming needs the code and no login');
const cv = await import('../src/client-version.js');
ok(cv.isGated('/api/pair/claim') && cv.isGated('/api/pair/start'), 'an app too old for this server is refused before it is given a login');
const statusAt = src.indexOf("path === '/api/pair/status' && req.method === 'POST'");
ok(statusAt > 0 && /validateToken\(req\)/.test(src.slice(statusAt, statusAt + 300)), 'asking what became of a code needs a login, and is a POST so the code stays out of logs');
ok(/pairing\.cancel\(body\.code\);\s+return json\(res, \{ ok: false/.test(src), 'a claim refused after the code was spent is not left looking like a success');
ok(cv.CAPABILITIES.includes('pairing'), 'the handshake says this server can pair');

// ─── The screen that shows the code ───
const pageSrc = readFileSync(join(root, 'src', 'webapp-html.js'), 'utf8');
const phone = readFileSync(join(root, 'app', 'vendor', 'crundi-phone.js'), 'utf8');
ok(pageSrc.includes('data-action="phone-connect"') && pageSrc.includes("case 'phone-connect':") && pageSrc.includes('/vendor/crundi-phone.js'), 'Settings has a Connect a phone button, and it is wired to the dialog');
ok(/if \(userRole !== 'collaborator' && window\.CrundiPhone\)/.test(pageSrc), 'the section is not drawn for a collaborator');
ok(/doneClass: crundiCodeDoneClass\(\), doneMs: CRUNDI_CODE_DONE_MS/.test(src), 'the server tells the page how to play the ending');
ok(phone.includes("'/api/pair/start'") && phone.includes("'/api/pair/status'") && phone.includes("'/api/pair/cancel'") && phone.includes('svg.classList.add(d.doneClass)'), 'the dialog asks for a code, watches it, plays the ending, and takes back a code it closes on');
ok(/if \(code && !paired\) post\(apiFetch, '\/api\/pair\/cancel'/.test(phone), 'a code is only withdrawn if no phone used it');
ok(phone.includes("'/api/push/mobile/key'") && phone.includes("input.type = 'password'") && !/keyHint[^;]*input\.value/.test(phone), 'the push key is entered in a masked field and never filled back in');
ok(pageSrc.includes("x.device && x.device.name ? x.device.name + ' (Crundi app)'"), 'a paired phone is listed by its name among the signed-in devices');

// A phone opening "localhost" opens itself. A pairing link made where Crundi
// is open at a loopback address (the desktop app) uses the public address,
// and with none there is no link at all, rather than one that cannot work.
const ps = src.slice(src.indexOf("path === '/api/pair/start'"), src.indexOf("path === '/api/pair/cancel'"));
ok(/loopback = \/\^\(localhost\|127/.test(ps) && /if \(!base \|\| loopback\) base = publicBaseUrl/.test(ps), 'a pairing link opened at localhost uses the public address instead');
ok(/if \(loopback && \/\^https\?:/.test(ps) && /A phone cannot reach this computer at "localhost"/.test(ps), 'and with no public address it says so, rather than handing out a localhost link');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
