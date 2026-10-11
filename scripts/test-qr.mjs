#!/usr/bin/env node
/**
 * test-qr.mjs — the QR codes shown for pairing a phone.
 *
 * A QR code that is nearly right is not a QR code: one wrong module in the
 * format bits and no reader will take it, and nothing on the server would ever
 * notice. So the encoder is pinned to codes that WERE read back. When
 * src/qr.js was written, every size from version 1 to 10 under all eight masks
 * (184 codes) was decoded by a real reader (jsQR) and compared module for
 * module against a reference encoder (the `qrcode` package). The fingerprints
 * below are from that run. If one changes, the encoder changed, and it has to
 * be checked against a reader again before the fingerprint is updated.
 *
 * Run: node scripts/test-qr.mjs
 */

import { createHash } from 'node:crypto';
import { encode, toSvg, MAX_BYTES } from '../src/qr.js';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const print = (q) => createHash('sha256').update(q.rows.map((r) => Array.from(r).join('')).join('\n')).digest('hex').slice(0, 16);
const filler = (n) => 'https://x.example/'.padEnd(n, 'abcXYZ019-_/#');

const KNOWN = [
  ['a', 1, 5, 'e8ab680854e4019c'],
  ['https://dev.faruhaan.com/pair#0123456789abcdef0123456789abcdef', 4, 2, 'd90c92d654848501'],
  ['héllo wörld ✓', 2, 4, '6cf98500e9c5b526'],
  [filler(123), 8, 2, '3c2d4fe9002b17fe'],
  [filler(213), 10, 3, 'ea2d2d716963f458'],
];
for (const [text, version, mask, fp] of KNOWN) {
  const q = encode(text);
  const label = text.length > 40 ? `${Buffer.byteLength(text)} bytes` : JSON.stringify(text);
  ok(q.version === version && q.mask === mask && print(q) === fp, `${label}: version ${version}, mask ${mask}, the grid a reader accepted`);
}

// ─── Shape ───
for (const n of [1, 14, 15, 26, 27, 42, 43, 62, 63, 84, 85, 106, 107, 122, 123, 152, 153, 180, 181, 213]) {
  const q = encode(filler(n).slice(0, n));
  const finder = (r0, c0) => [0, 6].every((d) => q.rows[r0 + d].slice(c0, c0 + 7).every((x) => x === 1)) && q.rows[r0 + 3][c0 + 3] === 1 && q.rows[r0 + 1][c0 + 1] === 0;
  if (!(q.size === q.version * 4 + 17 && q.rows.length === q.size && finder(0, 0) && finder(0, q.size - 7) && finder(q.size - 7, 0) && q.rows[q.size - 8][8] === 1)) {
    ok(false, `${n} bytes: the grid has its three finders and the fixed dark module`);
  }
}
ok(true, 'every size up to the limit has its three finders and the fixed dark module');
ok(encode(filler(14).slice(0, 14)).version === 1 && encode(filler(15).slice(0, 15)).version === 2, 'the smallest version that fits is used');
let threw = '';
try { encode('x'.repeat(MAX_BYTES + 1)); } catch (e) { threw = e.message; }
ok(/Too long/.test(threw), 'text past the limit is an error, not a broken code');
ok(encode('', {}).version === 1, 'empty text is still a valid code');

// ─── The picture ───
const svg = toSvg('https://dev.faruhaan.com/pair#0123456789abcdef0123456789abcdef');
ok(svg.startsWith('<svg ') && svg.includes('viewBox="0 0 41 41"'), 'the SVG leaves a four-module quiet zone on every side (33 + 8)');
ok(svg.includes('fill="#fff"') && svg.includes('fill="#000"'), 'and is black on white whatever the theme');
ok(!/[<>"]/.test(svg.replace(/<svg [^>]*>|<rect [^>]*>|<path d="[MhvzZ0-9 -]*" fill="#000"\/>|<\/svg>/g, '')), 'and contains nothing but its own three elements');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
