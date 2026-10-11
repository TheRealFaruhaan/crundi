#!/usr/bin/env node
/**
 * test-project-icon.mjs: a project's own icon is found, and only a real one.
 *
 * Run: node scripts/test-project-icon.mjs
 */
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, symlinkSync, utimesSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { inflateSync } from 'node:zlib';
import { findProjectIcon, looksLikeIcon, pngFromIco, pngFromIcoBitmap, readProjectIcon, storedIcon, projectIconUrl, isProjectIconUrl } from '../src/project-icon.js';

let failures = 0;
const ok = (cond, label) => { if (!cond) failures++; console.log(`${cond ? 'PASS' : 'FAIL'}  ${label}`); };

// The start of a PNG is enough to tell its size: signature, then IHDR.
function png(w, h) {
  const b = Buffer.alloc(64);
  b.writeUInt32BE(0x89504e47, 0); b.writeUInt32BE(0x0d0a1a0a, 4);
  b.writeUInt32BE(13, 8); b.write('IHDR', 12, 'latin1');
  b.writeUInt32BE(w, 16); b.writeUInt32BE(h, 20);
  return b;
}
function ico(inner) {
  const head = Buffer.alloc(22);
  head.writeUInt16LE(0, 0); head.writeUInt16LE(1, 2); head.writeUInt16LE(1, 4);
  head[6] = 64; head[7] = 64;
  head.writeUInt32LE(inner.length, 14); head.writeUInt32LE(22, 18);
  return Buffer.concat([head, inner]);
}

const root = mkdtempSync(join(tmpdir(), 'crundi-icon-'));
try {
  ok(findProjectIcon(root) === null, 'a project with no icon has none');
  ok(projectIconUrl('p', root, '') === '', 'and is given no address, so its letters are drawn');

  // Not just any picture.
  mkdirSync(join(root, 'public'), { recursive: true });
  writeFileSync(join(root, 'public', 'screenshot.png'), png(512, 512));
  writeFileSync(join(root, 'public', 'hero.png'), png(512, 512));
  ok(findProjectIcon(root) === null, 'a picture that is not named as an icon is not taken for one');
  writeFileSync(join(root, 'public', 'logo.png'), png(900, 240));
  ok(findProjectIcon(root) === null, 'a wide wordmark called logo.png is passed over');
  writeFileSync(join(root, 'favicon.png'), png(16, 16));
  ok(findProjectIcon(root) === null, 'so is one too small to draw');
  writeFileSync(join(root, 'icon.png'), Buffer.from('this is not a picture at all, only a name'));
  ok(findProjectIcon(root) === null, 'and a file that is not a picture, whatever it is called');

  writeFileSync(join(root, 'public', 'favicon.svg'), '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0h24v24H0z"/></svg>');
  let found = findProjectIcon(root);
  ok(found && found.rel === 'public/favicon.svg', 'a square drawing named favicon is an icon');
  writeFileSync(join(root, 'public', 'favicon.ico'), ico(png(64, 64)));
  found = findProjectIcon(root);
  ok(found && found.rel === 'public/favicon.ico', 'a .ico is preferred to a drawing: a phone can draw what is inside it');
  const served = readProjectIcon(found);
  ok(served.type === 'image/png' && served.body.readUInt32BE(0) === 0x89504e47, 'and it is served as the PNG it holds');
  ok(pngFromIco(Buffer.from('nonsense that is long enough to read')) === null, 'something that is not a .ico yields nothing');
  writeFileSync(join(root, 'public', 'apple-touch-icon.png'), png(180, 180));
  writeFileSync(join(root, 'public', 'icon-512.png'), png(512, 512));
  found = findProjectIcon(root);
  ok(found && found.rel === 'public/icon-512.png', 'a big square PNG named as an icon wins');

  // Stored, checked, and kept up to date.
  const url1 = projectIconUrl('My Project', root, found.file);
  ok(isProjectIconUrl(url1) && url1.startsWith('/api/project-icon?project=My%20Project&v='), 'the address names the project and a version');
  writeFileSync(found.file, Buffer.concat([png(512, 512), Buffer.from('changed')]));
  utimesSync(found.file, new Date(), new Date(Date.now() + 5000));
  const url2 = projectIconUrl('My Project', root, found.file);
  ok(url2 && url2 !== url1, 'a changed file has a new address, so everything fetches it again');
  rmSync(found.file);
  ok(storedIcon(root, found.file) === null && projectIconUrl('My Project', root, found.file) === '', 'a stored icon that has gone falls back to the letters');
  ok(storedIcon(root, join(root, 'public', 'hero.png')) !== null, 'a stored path is trusted for its name, having been chosen once');
  writeFileSync(join(root, 'public', 'logo.png'), png(900, 240));
  ok(storedIcon(root, join(root, 'public', 'logo.png')) === null, 'but not when it has stopped being an icon');

  // Never a file outside the project.
  const outside = mkdtempSync(join(tmpdir(), 'crundi-icon-out-'));
  try {
    writeFileSync(join(outside, 'secret.png'), png(256, 256));
    const bare = mkdtempSync(join(tmpdir(), 'crundi-icon-bare-'));
    try {
      symlinkSync(join(outside, 'secret.png'), join(bare, 'icon.png'));
      ok(findProjectIcon(bare) === null, 'a link out of the project is not its icon');
      ok(storedIcon(bare, join(outside, 'secret.png')) === null, 'nor is a stored path that points outside it');
    } finally { rmSync(bare, { recursive: true, force: true }); }
  } finally { rmSync(outside, { recursive: true, force: true }); }

  ok(!isProjectIconUrl('/api/secrets?project=x&v=1') && !isProjectIconUrl('https://evil.example/api/project-icon?project=x&v=1'), 'only this address is ever sent to a phone');
  // An old-style .ico: a 2 by 2 bitmap, 24 bits, bottom row first, with a mask.
  {
    const dib = Buffer.alloc(40 + 16 + 8);
    dib.writeUInt32LE(40, 0); dib.writeInt32LE(2, 4); dib.writeInt32LE(4, 8); dib.writeUInt16LE(1, 12); dib.writeUInt16LE(24, 14);
    // bottom row: blue, green.  top row: red, white (masked out).
    Buffer.from([255, 0, 0, 0, 255, 0, 0, 0]).copy(dib, 40);
    Buffer.from([0, 0, 255, 255, 255, 255, 0, 0]).copy(dib, 48);
    dib[56] = 0;            // mask, bottom row: both drawn
    dib[60] = 0b01000000;   // mask, top row: the second one is see-through
    const out = pngFromIcoBitmap(ico(dib));
    ok(out && out.readUInt32BE(0) === 0x89504e47 && out.readUInt32BE(16) === 2 && out.readUInt32BE(20) === 2, 'an old-style .ico is turned into a PNG of the same size');
    const idat = out.indexOf('IDAT', 0, 'latin1');
    const raw = inflateSync(out.subarray(idat + 4, idat + 4 + out.readUInt32BE(idat - 4)));
    const px = (x, y) => [...raw.subarray(y * 9 + 1 + x * 4, y * 9 + 5 + x * 4)].join(',');
    ok(px(0, 0) === '255,0,0,255' && px(1, 0).endsWith(',0') && px(0, 1) === '0,0,255,255' && px(1, 1) === '0,255,0,255', 'with its rows the right way up, its colours in order and its mask as see-through');
    ok(looksLikeIcon(out, 'image/png') === false && pngFromIcoBitmap(Buffer.from('not an icon, just some bytes here')) === null, 'and nonsense yields nothing');
  }
  ok(looksLikeIcon(png(64, 70), 'image/png') && !looksLikeIcon(png(64, 90), 'image/png'), 'near enough square counts, an oblong does not');
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log(failures ? `\n${failures} failed` : '\nAll passed');
process.exit(failures ? 1 : 0);
