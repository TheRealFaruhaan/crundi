// A project's own icon, if it has one.
//
// A project is marked by a coloured tile with letters on it (project-mark.js),
// worked out from its name. Many projects already have a picture that says who
// they are far better: the favicon of the site, the icon of the app, a logo in
// an assets folder. Where one is found it is used in place of the letters, on
// the compact chat's header, on a phone's bubble and on its live card.
//
// Nothing is configured. Each time a chat is opened in a project the usual
// places are looked in, under the usual names, and the full path of what is
// found is kept with the project (project-store.js). The address handed out
// carries the file's time and size, so a picture that is edited gets a new
// address, and everything that had the old one fetches again by itself. A
// project with no icon, or whose icon has since gone, keeps its letters.

import { readFileSync, statSync, realpathSync, readdirSync } from 'node:fs';
import { join, sep, extname } from 'node:path';
import { deflateSync } from 'node:zlib';

// Folders, most telling first. A project's root, then where web projects keep
// what they serve, then where app projects keep theirs.
const DIRS = [
  '', 'public', 'static', 'assets', 'web/public', 'web/static', 'client/public', 'frontend/public', 'app/public',
  'src/assets', 'app/assets', 'www', 'images', 'img', 'icons', 'assets/icons', 'assets/images', 'assets/img',
  'public/icons', 'public/images', 'public/img', 'public/assets', 'static/icons', 'static/images', 'static/img',
  'resources', 'res', 'src/app',
  'android/app/src/main/res/mipmap-xxxhdpi', 'android/app/src/main/res/mipmap-xxhdpi',
  'app/src/main/res/mipmap-xxxhdpi', 'app/src/main/res/mipmap-xxhdpi',
];

// Names, best first, in three ranks. A picture made of dots comes before one
// made of lines, because a phone's notification can draw the first and not
// the second; and a big one before a small one.
const RASTER = [
  'icon-512.png', 'icon_512x512.png', 'icon-512x512.png', 'android-chrome-512x512.png',
  'icon-192.png', 'icon_192x192.png', 'icon-192x192.png', 'android-chrome-192x192.png',
  'apple-touch-icon.png', 'apple_touch_180.png', 'apple-touch-icon-180x180.png',
  'icon.png', 'app-icon.png', 'appicon.png', 'logo.png', 'ic_launcher.png',
  'favicon.png', 'favicon-96x96.png', 'favicon-32x32.png',
];
const ICO = ['favicon.ico', 'icon.ico'];
const VECTOR = ['icon.svg', 'logo.svg', 'favicon.svg'];

const TYPES = { '.png': 'image/png', '.ico': 'image/x-icon', '.svg': 'image/svg+xml' };
// An icon is small. Anything bigger under one of these names is something
// else (a photograph called logo.jpg), and is not worth sending to a phone.
export const MAX_BYTES = 2 * 1024 * 1024;

// A name is not enough. "logo.png" is as often a wide wordmark as an icon,
// and a wordmark squeezed into a round head is worse than two letters. An
// icon is square, or near enough, and not tiny; a file that is not is passed
// over, and the search goes on to the next name.
const SQUARE = 1.15;
const MIN_SIDE = 32;
function squareEnough(w, h) { return w >= MIN_SIDE && h >= MIN_SIDE && Math.max(w, h) / Math.min(w, h) <= SQUARE; }

/** Does this file LOOK like an icon: a real picture of its kind, and square? */
export function looksLikeIcon(buf, type) {
  if (!buf || buf.length < 16) return false;
  if (type === 'image/png') {
    if (buf.length < 24 || buf.readUInt32BE(0) !== 0x89504e47 || buf.toString('latin1', 12, 16) !== 'IHDR') return false;
    return squareEnough(buf.readUInt32BE(16), buf.readUInt32BE(20));
  }
  // A .ico is an icon by definition: every picture in one is square.
  if (type === 'image/x-icon') return buf.readUInt16LE(0) === 0 && buf.readUInt16LE(2) === 1 && buf.readUInt16LE(4) > 0;
  if (type === 'image/svg+xml') {
    const head = buf.toString('utf8', 0, Math.min(buf.length, 4096));
    const tag = /<svg\b[^>]*>/i.exec(head);
    if (!tag) return false;
    const box = /\bviewBox\s*=\s*["']\s*[-\d.]+[\s,]+[-\d.]+[\s,]+([\d.]+)[\s,]+([\d.]+)/i.exec(tag[0]);
    if (box) { const w = Number(box[1]), h = Number(box[2]); return w > 0 && h > 0 && Math.max(w, h) / Math.min(w, h) <= SQUARE; }
    const w = /\bwidth\s*=\s*["']([\d.]+)/i.exec(tag[0]), h = /\bheight\s*=\s*["']([\d.]+)/i.exec(tag[0]);
    if (w && h) { const a = Number(w[1]), b = Number(h[1]); return a > 0 && b > 0 && Math.max(a, b) / Math.min(a, b) <= SQUARE; }
    // No size given at all: it cannot be told, so it is not used.
    return false;
  }
  return false;
}

function usable(root, rel) {
  try {
    const file = join(root, rel);
    const st = statSync(file);
    if (!st.isFile() || st.size < 16 || st.size > MAX_BYTES) return null;
    // A link out of the project is not the project's icon, and following it
    // would let a project hand out any file on the machine.
    const real = realpathSync(file);
    const base = realpathSync(root);
    if (real !== base && !real.startsWith(base + sep)) return null;
    const type = TYPES[extname(rel).toLowerCase()] || '';
    if (!looksLikeIcon(readFileSync(real), type)) return null;
    return { file: real, rel, type, size: st.size, mtimeMs: Math.floor(st.mtimeMs) };
  } catch { return null; }
}

// ─── A mark made for a dark background ───
// Crundi is dark. A logo drawn in a dark colour, which is what most projects
// put first, all but disappears on it; many projects also keep a version for
// dark backgrounds ("logo-white.svg", "mark-dark.png", "icon-inverse.svg"),
// often in a brand folder. Where there is one, it is used before anything else.
const DARK_DIRS = [...DIRS,
  'brand', 'branding', 'logo', 'logos', 'public/brand', 'public/branding', 'public/logo', 'public/logos',
  'static/brand', 'static/logos', 'assets/brand', 'assets/branding', 'assets/logo', 'assets/logos',
  'src/assets/brand', 'src/assets/logos', 'src/assets/images', 'src/assets/icons', 'website/static/img', 'docs/static/img',
];
// The file is a mark of some kind...
const MARK_WORD = /(icon|logo|mark|favicon|symbol|emblem|glyph)/i;
// ...for a dark background. "light" is left out on purpose: "logo-light" is as
// often the one for a light page as the light-coloured one.
const DARK_WORD = /(?:^|[-_.\s])(white|dark|darkmode|dark-mode|on-dark|ondark|inverse|inverted|reversed|negative|night)(?=[-_.\s]|$)/i;
const TYPE_RANK = { '.png': 0, '.ico': 1, '.svg': 2 };

// "dark" in a name can also mean a mark drawn IN a dark colour. An SVG says
// its colours in the open, so one whose colours are all dark is passed over.
function svgAllDark(buf) {
  const text = buf.toString('utf8', 0, Math.min(buf.length, 256 * 1024));
  const named = { white: 'ffffff', black: '000000' };
  const found = [];
  const re = /(?:fill|stroke|stop-color|color)\s*[=:]\s*["']?\s*(#[0-9a-f]{3,8}\b|white\b|black\b|rgba?\([^)]*\))/gi;
  let m;
  while ((m = re.exec(text))) {
    let v = m[1].toLowerCase(), r, g, b;
    if (named[v]) v = '#' + named[v];
    if (v[0] === '#') {
      let h = v.slice(1);
      if (h.length === 3 || h.length === 4) h = h.split('').map((c) => c + c).join('');
      r = parseInt(h.slice(0, 2), 16); g = parseInt(h.slice(2, 4), 16); b = parseInt(h.slice(4, 6), 16);
    } else {
      const n = v.match(/[\d.]+/g) || [];
      [r, g, b] = n.map(Number);
    }
    if ([r, g, b].some((x) => !Number.isFinite(x))) continue;
    found.push((0.2126 * r + 0.7152 * g + 0.0722 * b) / 255);
  }
  return found.length > 0 && found.every((l) => l < 0.3);
}

function sameAsPlain(root, dir, name, stem, ext, file) {
  const plain = stem.replace(DARK_WORD, '') + ext;
  if (!plain || plain === name) return false;
  try { return readFileSync(join(root, dir, plain)).equals(readFileSync(file)); } catch { return false; }
}

/** The project's mark for dark backgrounds, or null. */
export function findDarkIcon(root) {
  const hits = [];
  DARK_DIRS.forEach((dir, dirRank) => {
    let names;
    try { names = readdirSync(dir ? join(root, dir) : root); } catch { return; }
    for (const name of names) {
      const ext = extname(name).toLowerCase();
      if (!(ext in TYPE_RANK)) continue;
      const stem = name.slice(0, -ext.length);
      if (!MARK_WORD.test(stem) || !DARK_WORD.test(stem)) continue;
      const hit = usable(root, dir ? `${dir}/${name}` : name);
      if (!hit) continue;
      if (ext === '.svg') { try { if (svgAllDark(readFileSync(hit.file))) continue; } catch { continue; } }
      // A "dark" copy that is the same file as the plain one is no better, and
      // would only push a better picture of the same thing (a PNG) aside.
      if (sameAsPlain(root, dir, name, stem, ext, hit.file)) continue;
      // A mark or icon before a whole logo, a big picture before a small one.
      const kind = /(icon|mark|symbol|favicon|emblem|glyph)/i.test(stem) ? 0 : 1;
      let side = 0;
      if (ext === '.png') { try { side = readFileSync(hit.file).readUInt32BE(16); } catch { /* unknown */ } }
      hits.push({ hit, key: [TYPE_RANK[ext], kind, -side, dirRank, name] });
    }
  });
  if (!hits.length) return null;
  hits.sort((a, b) => { for (let i = 0; i < a.key.length; i++) { if (a.key[i] < b.key[i]) return -1; if (a.key[i] > b.key[i]) return 1; } return 0; });
  return hits[0].hit;
}

/**
 * The project's icon file, or null. A mark made for a dark background comes
 * first, where the project has one.
 * @returns {{ file: string, rel: string, type: string, size: number, mtimeMs: number } | null}
 */
export function findProjectIcon(root) {
  if (!root) return null;
  const dark = findDarkIcon(root);
  if (dark) return dark;
  for (const names of [RASTER, ICO, VECTOR]) {
    for (const name of names) {
      for (const dir of DIRS) {
        const hit = usable(root, dir ? `${dir}/${name}` : name);
        if (hit) return hit;
      }
    }
  }
  return null;
}

/**
 * The picture inside a .ico file, where it holds one as PNG (the usual way
 * for anything made in the last fifteen years). A phone cannot draw a .ico,
 * and can draw this. Null when the file only holds old-style bitmaps.
 */
export function pngFromIco(buf) {
  if (!buf || buf.length < 22 || buf.readUInt16LE(0) !== 0 || buf.readUInt16LE(2) !== 1) return null;
  const count = buf.readUInt16LE(4);
  let best = null, bestSize = -1;
  for (let i = 0; i < count && 6 + (i + 1) * 16 <= buf.length; i++) {
    const at = 6 + i * 16;
    const width = buf[at] || 256;
    const bytes = buf.readUInt32LE(at + 8), offset = buf.readUInt32LE(at + 12);
    if (offset + bytes > buf.length || bytes < 8) continue;
    const isPng = buf.readUInt32BE(offset) === 0x89504e47;
    if (isPng && width > bestSize) { bestSize = width; best = buf.subarray(offset, offset + bytes); }
  }
  return best;
}

// ─── An old-style .ico, turned into a PNG ───
// Older favicons hold plain bitmaps, which a phone cannot draw either. The
// biggest one is read and written out again as a PNG. Done in memory when the
// icon is asked for: nothing is written into the project's folder, and the
// answer is kept by everything that fetched it, under the icon's address.

let crcTable = null;
function crc32(buf) {
  if (!crcTable) {
    crcTable = new Uint32Array(256);
    for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; crcTable[n] = c >>> 0; }
  }
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = crcTable[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, 'latin1');
  data.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
  return out;
}
/** Pixels (RGBA, top row first) as a PNG file. */
export function pngFromRgba(rgba, width, height) {
  const head = Buffer.alloc(13);
  head.writeUInt32BE(width, 0); head.writeUInt32BE(height, 4);
  head[8] = 8; head[9] = 6;   // eight bits a channel, colour with alpha
  const rows = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) rgba.copy(rows, y * (width * 4 + 1) + 1, y * width * 4, (y + 1) * width * 4);
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', head), chunk('IDAT', deflateSync(rows)), chunk('IEND', Buffer.alloc(0))]);
}

/**
 * The biggest plain bitmap in a .ico, as a PNG. Null when there is none this
 * can read (a compressed or oddly made one): the letters are drawn then.
 */
export function pngFromIcoBitmap(buf) {
  if (!buf || buf.length < 22 || buf.readUInt16LE(0) !== 0 || buf.readUInt16LE(2) !== 1) return null;
  const count = buf.readUInt16LE(4);
  let best = null;
  for (let i = 0; i < count && 6 + (i + 1) * 16 <= buf.length; i++) {
    const at = 6 + i * 16;
    const bytes = buf.readUInt32LE(at + 8), offset = buf.readUInt32LE(at + 12);
    if (bytes < 40 || offset + bytes > buf.length || buf.readUInt32LE(offset) !== 40) continue;
    const width = buf.readInt32LE(offset + 4), height = Math.abs(buf.readInt32LE(offset + 8)) >> 1;
    const bpp = buf.readUInt16LE(offset + 14), compression = buf.readUInt32LE(offset + 16);
    if (compression !== 0 || width < 1 || height < 1 || width > 1024 || height > 1024 || ![1, 4, 8, 24, 32].includes(bpp)) continue;
    if (!best || width * bpp > best.width * best.bpp) best = { offset, bytes, width, height, bpp };
  }
  if (!best) return null;
  const { offset, bytes, width, height, bpp } = best;
  const end = offset + bytes;
  let colours = bpp <= 8 ? buf.readUInt32LE(offset + 32) || (1 << bpp) : 0;
  const palette = offset + 40;
  const pixels = palette + colours * 4;
  const rowBytes = ((width * bpp + 31) >> 5) << 2;
  const maskRow = ((width + 31) >> 5) << 2;
  const mask = pixels + rowBytes * height;
  if (pixels + rowBytes * height > end) return null;
  const hasMask = mask + maskRow * height <= end;
  const rgba = Buffer.alloc(width * height * 4);
  let anyAlpha = false;
  for (let y = 0; y < height; y++) {
    const src = pixels + (height - 1 - y) * rowBytes;   // stored bottom row first
    for (let x = 0; x < width; x++) {
      const o = (y * width + x) * 4;
      let r, g, b, a = 255;
      if (bpp === 32) { b = buf[src + x * 4]; g = buf[src + x * 4 + 1]; r = buf[src + x * 4 + 2]; a = buf[src + x * 4 + 3]; if (a) anyAlpha = true; }
      else if (bpp === 24) { b = buf[src + x * 3]; g = buf[src + x * 3 + 1]; r = buf[src + x * 3 + 2]; }
      else {
        const idx = bpp === 8 ? buf[src + x] : bpp === 4 ? (buf[src + (x >> 1)] >> (x & 1 ? 0 : 4)) & 15 : (buf[src + (x >> 3)] >> (7 - (x & 7))) & 1;
        const p = palette + idx * 4;
        if (p + 3 >= end) return null;
        b = buf[p]; g = buf[p + 1]; r = buf[p + 2];
      }
      rgba[o] = r; rgba[o + 1] = g; rgba[o + 2] = b; rgba[o + 3] = a;
    }
  }
  // Without a channel of its own, what is see-through is said by the mask
  // that follows the picture: one bit a pixel, set where nothing is drawn.
  if (bpp !== 32 || !anyAlpha) {
    for (let y = 0; y < height; y++) {
      const src = mask + (height - 1 - y) * maskRow;
      for (let x = 0; x < width; x++) {
        const clear = hasMask && ((buf[src + (x >> 3)] >> (7 - (x & 7))) & 1) === 1;
        rgba[(y * width + x) * 4 + 3] = clear ? 0 : 255;
      }
    }
  }
  return pngFromRgba(rgba, width, height);
}

/** The bytes to answer with, and what they are. A .ico always leaves as a PNG if it can. */
export function readProjectIcon(icon) {
  const body = readFileSync(icon.file);
  if (icon.type === 'image/x-icon') {
    let png = pngFromIco(body);
    if (!png) { try { png = pngFromIcoBitmap(body); } catch { png = null; } }
    if (png) return { body: png, type: 'image/png' };
  }
  return { body, type: icon.type || 'application/octet-stream' };
}

/**
 * The stored icon of a project, checked: the file that was found when a chat
 * was last opened there, if it is still on disk and still an icon. One stat
 * and one small read, which is cheap enough to do every time a mark is made.
 *
 * @param {string} root  the project's folder
 * @param {string} file  the full path kept in the projects list ('' for none)
 */
export function storedIcon(root, file) {
  if (!root || !file) return null;
  try {
    const base = realpathSync(root);
    const real = realpathSync(file);
    if (!real.startsWith(base + sep)) return null;
    return usable(base, real.slice(base.length + 1).split(sep).join('/'));
  } catch { return null; }
}

/**
 * The address a page or a phone fetches the icon from, or '' when the project
 * has none (and the letters are drawn, as before). The `v` is the file's time
 * and size: a changed file is a new address, so everything that had the old
 * one fetches again by itself.
 */
export function projectIconUrl(name, root, file) {
  const icon = storedIcon(root, file);
  if (!icon || !name) return '';
  return `/api/project-icon?project=${encodeURIComponent(name)}&v=${icon.mtimeMs.toString(36)}-${icon.size.toString(36)}`;
}

/** Is this an address this file hands out? What a phone is sent is checked against it. */
export function isProjectIconUrl(u) {
  return typeof u === 'string' && u.length <= 300 && /^\/api\/project-icon\?project=[^&#\s]{1,200}&v=[0-9a-z-]{1,40}$/.test(u);
}
