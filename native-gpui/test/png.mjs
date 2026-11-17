// Pixels from the app's own snapshots (testctl "snapshot"), for the checks
// that are about what is drawn: a state's colour on a dot, a badge, a ring.
import { inflateSync } from 'node:zlib';
import { readFileSync } from 'node:fs';
// A minimal PNG reader: 8-bit RGB/RGBA, not interlaced (what the snapshot writes).
export function readPng(buf) {
  let o = 8, w = 0, h = 0, ct = 6; const idat = [];
  while (o < buf.length) {
    const len = buf.readUInt32BE(o), type = buf.toString('ascii', o + 4, o + 8), data = buf.subarray(o + 8, o + 8 + len);
    if (type === 'IHDR') { w = data.readUInt32BE(0); h = data.readUInt32BE(4); ct = data[9]; }
    else if (type === 'IDAT') idat.push(data);
    else if (type === 'IEND') break;
    o += 12 + len;
  }
  const bpp = ct === 6 ? 4 : 3, stride = w * bpp, raw = inflateSync(Buffer.concat(idat)), px = Buffer.alloc(h * stride);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)], src = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)), cur = px.subarray(y * stride, (y + 1) * stride), prev = y ? px.subarray((y - 1) * stride, y * stride) : null;
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? cur[i - bpp] : 0, b = prev ? prev[i] : 0, c = prev && i >= bpp ? prev[i - bpp] : 0;
      let v = src[i];
      if (f === 1) v += a; else if (f === 2) v += b; else if (f === 3) v += (a + b) >> 1;
      else if (f === 4) { const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c); v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c; }
      cur[i] = v & 255;
    }
  }
  return { w, h, at: (x, y) => { const i = y * stride + x * bpp; return [px[i], px[i + 1], px[i + 2]]; } };
}
// How many pixels in the box are within tol of the colour (#rrggbb).
export function count(img, x0, y0, x1, y1, hex, tol = 10) {
  const t = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  let n = 0;
  for (let y = Math.max(0, Math.round(y0)); y <= Math.min(img.h - 1, Math.round(y1)); y++)
    for (let x = Math.max(0, Math.round(x0)); x <= Math.min(img.w - 1, Math.round(x1)); x++) {
      const p = img.at(x, y); if (Math.abs(p[0] - t[0]) <= tol && Math.abs(p[1] - t[1]) <= tol && Math.abs(p[2] - t[2]) <= tol) n++;
    }
  return n;
}
