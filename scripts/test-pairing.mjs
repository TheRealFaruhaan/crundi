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
import { createPairing, pairingLink, parsePairingLink, cleanDevice, CODE_TTL_MS } from '../src/pairing.js';
import { mayAccess, ROLE_COLLABORATOR, ROLE_OWNER } from '../src/access-policy.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// ─── Codes ───
let t = 1_000_000;
const p = createPairing({ now: () => t });
const a = p.start({ username: 'owner' });
ok(a.ok && /^[a-f0-9]{32}$/.test(a.code) && a.expiresIn === CODE_TTL_MS / 1000, 'a code is 128 random bits and says how long it lasts');
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
ok(mayAccess(collab, 'POST', '/api/pair/start') && mayAccess(collab, 'POST', '/api/pair/cancel'), 'a collaborator may pair their own phone');
ok(mayAccess({ username: 'o', role: ROLE_OWNER }, 'POST', '/api/pair/start'), 'and so may the owner');

// ─── Wired in the way the comments promise ───
const src = readFileSync(join(root, 'src', 'webapp.js'), 'utf8');
ok(/login\('page'\), background: login\('background'\)/.test(src), 'a paired device gets two logins, one per role');
ok(/if \(m\?\.device\?\.id\) for \(const \[fid, f\] of families\) if \(f\.device\?\.id === m\.device\.id\) gone\.add\(fid\)/.test(src), 'revoking one login of a device revokes all of them');
ok(/\.\.\.\(\(device \|\| prev\?\.device\) \? \{ device: device \|\| prev\.device \} : \{\}\)/.test(src), 'the device survives a token rotation');
const claimAt = src.indexOf("path === '/api/pair/claim'");
const claimBody = src.slice(claimAt, src.indexOf('\n    }\n', claimAt));
ok(claimAt > 0 && !/validateToken\(req\)/.test(claimBody), 'claiming needs the code and no login');
const cv = await import('../src/client-version.js');
ok(cv.isGated('/api/pair/claim') && cv.isGated('/api/pair/start'), 'an app too old for this server is refused before it is given a login');
ok(cv.CAPABILITIES.includes('pairing'), 'the handshake says this server can pair');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
