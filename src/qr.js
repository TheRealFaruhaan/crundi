// QR codes, for pairing a phone with this server.
//
// Written against the standard (ISO/IEC 18004) rather than pulling in a
// package, for the same reason web-push.js is: it is one small, fixed job. It
// does exactly what pairing needs and no more: bytes in, error correction
// level M, versions 1 to 10 (up to 213 bytes, where a pairing link is about
// 70). scripts/test-qr.mjs holds it to codes a real reader has checked.

// Per version, at level M: total codewords, error correction codewords per
// block, and the data blocks as [count, dataCodewords] pairs.
const VERSIONS = [
  null,
  { total: 26, ec: 10, blocks: [[1, 16]], align: [] },
  { total: 44, ec: 16, blocks: [[1, 28]], align: [6, 18] },
  { total: 70, ec: 26, blocks: [[1, 44]], align: [6, 22] },
  { total: 100, ec: 18, blocks: [[2, 32]], align: [6, 26] },
  { total: 134, ec: 24, blocks: [[2, 43]], align: [6, 30] },
  { total: 172, ec: 16, blocks: [[4, 27]], align: [6, 34] },
  { total: 196, ec: 18, blocks: [[4, 31]], align: [6, 22, 38] },
  { total: 242, ec: 22, blocks: [[2, 38], [2, 39]], align: [6, 24, 42] },
  { total: 292, ec: 22, blocks: [[3, 36], [2, 37]], align: [6, 26, 46] },
  { total: 346, ec: 26, blocks: [[4, 43], [1, 44]], align: [6, 28, 50] },
];
export const MAX_BYTES = 213;

const dataCodewords = (v) => VERSIONS[v].blocks.reduce((n, [count, size]) => n + count * size, 0);
const lengthBits = (v) => (v <= 9 ? 8 : 16);

// ─── Reed-Solomon over GF(256), polynomial 0x11d ───
const EXP = new Uint8Array(512), LOG = new Uint8Array(256);
for (let i = 0, x = 1; i < 255; i++) { EXP[i] = x; LOG[x] = i; x <<= 1; if (x & 0x100) x ^= 0x11d; }
for (let i = 255; i < 512; i++) EXP[i] = EXP[i - 255];
const mul = (a, b) => (a && b ? EXP[LOG[a] + LOG[b]] : 0);

function generator(degree) {
  let g = [1];
  for (let i = 0; i < degree; i++) {
    const next = new Array(g.length + 1).fill(0);
    for (let j = 0; j < g.length; j++) { next[j] ^= g[j]; next[j + 1] ^= mul(g[j], EXP[i]); }
    g = next;
  }
  return g;
}

function errorCorrection(data, degree) {
  const g = generator(degree);
  const rem = new Array(degree).fill(0);
  for (const byte of data) {
    const factor = byte ^ rem.shift();
    rem.push(0);
    for (let i = 0; i < degree; i++) rem[i] ^= mul(g[i + 1], factor);
  }
  return rem;
}

// ─── Data: mode, length, bytes, padding, then split into blocks ───
function codewords(bytes, v) {
  const capacity = dataCodewords(v);
  const bits = [];
  const put = (value, n) => { for (let i = n - 1; i >= 0; i--) bits.push((value >>> i) & 1); };
  put(0b0100, 4);                       // byte mode
  put(bytes.length, lengthBits(v));
  for (const b of bytes) put(b, 8);
  put(0, Math.min(4, capacity * 8 - bits.length));   // terminator
  while (bits.length % 8) bits.push(0);
  const data = [];
  for (let i = 0; i < bits.length; i += 8) data.push(bits.slice(i, i + 8).reduce((n, b) => (n << 1) | b, 0));
  for (let pad = 0xec; data.length < capacity; pad ^= 0xec ^ 0x11) data.push(pad);

  const spec = VERSIONS[v];
  const blocks = [];
  let at = 0;
  for (const [count, size] of spec.blocks) {
    for (let i = 0; i < count; i++) {
      const d = data.slice(at, at + size); at += size;
      blocks.push({ d, e: errorCorrection(d, spec.ec) });
    }
  }
  // Interleave: first codeword of every block, then the second, and so on.
  const out = [];
  const longest = Math.max(...blocks.map((b) => b.d.length));
  for (let i = 0; i < longest; i++) for (const b of blocks) if (i < b.d.length) out.push(b.d[i]);
  for (let i = 0; i < spec.ec; i++) for (const b of blocks) out.push(b.e[i]);
  return out;
}

// ─── The grid ───
function blankGrid(v) {
  const size = v * 4 + 17;
  const dark = Array.from({ length: size }, () => new Uint8Array(size));
  const fixed = Array.from({ length: size }, () => new Uint8Array(size));   // not data: never masked
  const set = (r, c, on) => { if (r >= 0 && r < size && c >= 0 && c < size) { dark[r][c] = on ? 1 : 0; fixed[r][c] = 1; } };

  // Finders, with their light separators.
  for (const [r0, c0] of [[0, 0], [0, size - 7], [size - 7, 0]]) {
    for (let r = -1; r <= 7; r++) {
      for (let c = -1; c <= 7; c++) {
        const ring = Math.max(Math.abs(r - 3), Math.abs(c - 3));
        set(r0 + r, c0 + c, ring !== 2 && ring <= 3);
      }
    }
  }
  // Alignment patterns, except where a finder already is.
  const centres = VERSIONS[v].align;
  for (const r of centres) {
    for (const c of centres) {
      if (fixed[r][c]) continue;
      for (let dr = -2; dr <= 2; dr++) for (let dc = -2; dc <= 2; dc++) set(r + dr, c + dc, Math.max(Math.abs(dr), Math.abs(dc)) !== 1);
    }
  }
  // Timing.
  for (let i = 8; i < size - 8; i++) {
    if (!fixed[6][i]) set(6, i, i % 2 === 0);
    if (!fixed[i][6]) set(i, 6, i % 2 === 0);
  }
  // The one module that is always dark, and room for the format bits.
  set(size - 8, 8, true);
  for (let i = 0; i < 9; i++) { if (!fixed[8][i]) set(8, i, false); if (!fixed[i][8]) set(i, 8, false); }
  for (let i = 0; i < 8; i++) { if (!fixed[8][size - 1 - i]) set(8, size - 1 - i, false); if (!fixed[size - 1 - i][8]) set(size - 1 - i, 8, false); }
  // Version information, from version 7.
  if (v >= 7) {
    let rem = v;
    for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >>> 11) * 0x1f25);
    const info = (v << 12) | rem;
    for (let i = 0; i < 18; i++) {
      const on = (info >>> i) & 1;
      const a = size - 11 + (i % 3), b = Math.floor(i / 3);
      set(a, b, on); set(b, a, on);
    }
  }
  return { size, dark, fixed, set };
}

function placeData(grid, words) {
  const { size, dark, fixed } = grid;
  let i = 0;
  // Two columns at a time, right to left, snaking up and down; the timing
  // column is stepped over.
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let n = 0; n < size; n++) {
      const upward = ((right + 1) & 2) === 0;
      const r = upward ? size - 1 - n : n;
      for (let k = 0; k < 2; k++) {
        const c = right - k;
        if (fixed[r][c]) continue;
        dark[r][c] = i < words.length * 8 ? (words[i >>> 3] >>> (7 - (i & 7))) & 1 : 0;
        i++;
      }
    }
  }
}

const MASKS = [
  (r, c) => (r + c) % 2 === 0,
  (r) => r % 2 === 0,
  (r, c) => c % 3 === 0,
  (r, c) => (r + c) % 3 === 0,
  (r, c) => (Math.floor(r / 2) + Math.floor(c / 3)) % 2 === 0,
  (r, c) => ((r * c) % 2) + ((r * c) % 3) === 0,
  (r, c) => (((r * c) % 2) + ((r * c) % 3)) % 2 === 0,
  (r, c) => (((r + c) % 2) + ((r * c) % 3)) % 2 === 0,
];

function applyMask(grid, m) {
  const { size, dark, fixed } = grid;
  for (let r = 0; r < size; r++) for (let c = 0; c < size; c++) if (!fixed[r][c] && MASKS[m](r, c)) dark[r][c] ^= 1;
}

function writeFormat(grid, m) {
  const { size, dark } = grid;
  const data = (0b00 << 3) | m;                // level M is 00
  let rem = data;
  for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537);
  const bits = ((data << 10) | rem) ^ 0x5412;
  const bit = (i) => (bits >>> i) & 1;
  for (let i = 0; i <= 5; i++) dark[i][8] = bit(i);
  dark[7][8] = bit(6); dark[8][8] = bit(7); dark[8][7] = bit(8);
  for (let i = 9; i < 15; i++) dark[8][14 - i] = bit(i);
  for (let i = 0; i < 8; i++) dark[8][size - 1 - i] = bit(i);
  for (let i = 8; i < 15; i++) dark[size - 15 + i][8] = bit(i);
}

// How hard a grid is to read, by the standard's four rules. Lower is better.
function penalty(grid) {
  const { size, dark } = grid;
  let score = 0;
  const line = (get) => {
    for (let a = 0; a < size; a++) {
      let run = 1;
      for (let b = 1; b <= size; b++) {
        if (b < size && get(a, b) === get(a, b - 1)) { run++; continue; }
        if (run >= 5) score += run - 2;
        run = 1;
      }
      for (let b = 0; b + 10 < size; b++) {
        const p = [];
        for (let k = 0; k < 11; k++) p.push(get(a, b + k));
        const s = p.join('');
        if (s === '10111010000' || s === '00001011101') score += 40;
      }
    }
  };
  line((a, b) => dark[a][b]);
  line((a, b) => dark[b][a]);
  for (let r = 0; r < size - 1; r++) {
    for (let c = 0; c < size - 1; c++) {
      const x = dark[r][c];
      if (x === dark[r][c + 1] && x === dark[r + 1][c] && x === dark[r + 1][c + 1]) score += 3;
    }
  }
  let on = 0;
  for (let r = 0; r < size; r++) for (let c = 0; c < size; c++) on += dark[r][c];
  score += Math.floor(Math.abs((on * 20) / (size * size) - 10)) * 10;
  return score;
}

/**
 * Encode text as a QR code.
 *
 * @param {string|Uint8Array} input
 * @param {{ mask?: number }} [opts] mask forces one of the eight patterns; the
 *   default is whichever reads best. Only the tests have a reason to force it.
 * @returns {{ size: number, version: number, mask: number, rows: Uint8Array[] }}
 *   rows[r][c] is 1 for a dark module. No quiet zone: toSvg adds it.
 */
export function encode(input, opts = {}) {
  const bytes = typeof input === 'string' ? Buffer.from(input, 'utf8') : Buffer.from(input || []);
  let version = 0;
  for (let v = 1; v < VERSIONS.length; v++) {
    if (4 + lengthBits(v) + bytes.length * 8 <= dataCodewords(v) * 8) { version = v; break; }
  }
  if (!version) throw new Error(`Too long for a QR code here: ${bytes.length} bytes, the limit is ${MAX_BYTES}.`);
  const words = codewords(bytes, version);

  const build = (m) => {
    const g = blankGrid(version);
    placeData(g, words);
    applyMask(g, m);
    writeFormat(g, m);
    return g;
  };
  let best = null, mask = -1;
  const forced = Number.isInteger(opts.mask) && opts.mask >= 0 && opts.mask < 8;
  for (let m = 0; m < 8; m++) {
    if (forced && m !== opts.mask) continue;
    const g = build(m);
    const p = penalty(g);
    if (!best || p < best.p) { best = { g, p }; mask = m; }
  }
  return { size: best.g.size, version, mask, rows: best.g.dark };
}

/**
 * An SVG of the code, dark on light with the four-module quiet zone a reader
 * needs. Always black on white, whatever the page's theme: a reversed code is
 * something many readers refuse.
 */
export function toSvg(input, { quiet = 4 } = {}) {
  const { size, rows } = typeof input === 'object' && input && input.rows ? input : encode(input);
  const full = size + quiet * 2;
  let d = '';
  for (let r = 0; r < size; r++) {
    for (let c = 0; c < size; c++) {
      if (!rows[r][c]) continue;
      let run = 1;
      while (c + run < size && rows[r][c + run]) run++;
      d += `M${c + quiet} ${r + quiet}h${run}v1h-${run}z`;
      c += run - 1;
    }
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${full} ${full}" shape-rendering="crispEdges" role="img" aria-label="QR code">`
    + `<rect width="${full}" height="${full}" fill="#fff"/><path d="${d}" fill="#000"/></svg>`;
}
