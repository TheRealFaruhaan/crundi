// The Crundi code: the picture a phone scans to pair with this server.
//
// Not a QR code, on purpose. It is the logo with five rings of dashes turning
// around it, each ring at its own speed and in its own direction. The dashes
// are the data.
//
//   - Two plain rings, one round the logo and one round the outside, are what
//     a reader looks for. Each is a closed loop with nothing touching it, so
//     it can be found as a shape and fitted as an ellipse; between them they
//     say where the code is and how it is tilted.
//   - Five data rings lie between. A ring is cut into equal cells, and a cell
//     either has a dash in it or does not.
//   - One cell in every ring is amber. That is where the ring starts. Because
//     each ring says so for itself, the rings can turn independently and the
//     phone can be held at any angle: no frame needs the rings lined up.
//   - The bytes are spread through the rings with Reed-Solomon parity, so a
//     reflection across part of the screen does not stop a read, and a bad
//     read is recognised as bad instead of being believed.
//
// What it carries is the same link the fallback QR code carries
// (https://server/pair#code, see pairing.js), packed small: the scheme and
// port in a few bits, the host name at six bits a letter, the code as raw
// bytes. Four sizes, picked by how long the host name is.
//
// Both halves live in this one file and it touches nothing but arrays, so the
// same code runs on the server (to draw) and inside the app (to read).

// ─── Geometry, in units of the outer ring's radius ───
export const R_INNER = 0.38;           // finder ring round the logo
export const R_OUTER = 1.0;            // finder ring round the outside
export const RING_RADII = [0.50, 0.60, 0.70, 0.80, 0.90];
export const FINDER_WIDTH = 0.035;

// Cells per ring grow with the radius, so a cell is the same length of arc in
// every ring. `parity` is how many of the bytes are Reed-Solomon parity; half
// that many wrong bytes can be put right.
export const CLASSES = [
  { name: 'S', cells: [40, 48, 56, 64, 72], parity: 8 },
  { name: 'L', cells: [55, 66, 77, 88, 99], parity: 10 },
  { name: 'XL', cells: [70, 84, 98, 112, 126], parity: 12 },
  { name: 'XXL', cells: [85, 102, 119, 136, 153], parity: 14 },
].map((c) => {
  const bits = c.cells.reduce((n, x) => n + x - 1, 0);     // one cell per ring is the marker
  const bytes = Math.floor(bits / 8);
  // Thick enough to read, thin enough that one cell on its own is a dot and
  // not a blob running into its neighbours.
  const stroke = 0.62 * ((2 * Math.PI * RING_RADII[0]) / c.cells[0]);
  return Object.freeze({ ...c, bits, bytes, payload: bytes - c.parity, stroke });
});

const CODE_BYTES = 12;
const HOST_CHARS = 'abcdefghijklmnopqrstuvwxyz0123456789-.';
// A host name is written six bits a symbol, and 38 of the 64 values are the
// letters above. The rest stand for whole pieces of the names servers like
// this one actually have: a Cloudflare quick tunnel is fifty characters long
// and eighteen of them are always the same.
//
// THE ORDER IS THE FORMAT. A value means what it meant in the first app that
// could read it, for ever: add to the end, never insert, reorder or remove.
const HOST_WORDS = [
  '.trycloudflare.com', '.cfargotunnel.com', '.ngrok-free.app', '.ngrok.io', '.duckdns.org', '.ts.net',
  '.com', '.net', '.org', '.dev', '.app', '.io', '.co.uk', '.local', '.lan', '.home.arpa',
  'www.', '192.168.', '10.0.', '172.16.',
];
// Longest first, so '.ngrok-free.app' is not spent on '.app'.
const WORDS_BY_LENGTH = HOST_WORDS.map((w, i) => [w, HOST_CHARS.length + i]).sort((a, b) => b[0].length - a[0].length);

/** A host name as symbol values, or null if it has a character that is not allowed. */
function hostSymbols(host) {
  const out = [];
  for (let i = 0; i < host.length;) {
    const word = WORDS_BY_LENGTH.find(([w]) => host.startsWith(w, i));
    if (word) { out.push(word[1]); i += word[0].length; continue; }
    const v = HOST_CHARS.indexOf(host[i]);
    if (v < 0) return null;
    out.push(v); i++;
  }
  return out;
}
const hostBytes = (symbols) => Math.ceil((symbols * 6) / 8);

// ─── Reed-Solomon over GF(256), polynomial 0x11d ───
const EXP = new Uint8Array(512), LOG = new Uint8Array(256);
for (let i = 0, x = 1; i < 255; i++) { EXP[i] = x; LOG[x] = i; x <<= 1; if (x & 0x100) x ^= 0x11d; }
for (let i = 255; i < 512; i++) EXP[i] = EXP[i - 255];
const mul = (a, b) => (a && b ? EXP[LOG[a] + LOG[b]] : 0);
const div = (a, b) => (a ? EXP[LOG[a] + 255 - LOG[b]] : 0);
const pow = (n) => EXP[((n % 255) + 255) % 255];

/** data + parity. The first byte is the highest power of the polynomial. */
export function rsEncode(data, nsym) {
  let g = [1];
  for (let i = 0; i < nsym; i++) {
    const next = new Array(g.length + 1).fill(0);
    for (let j = 0; j < g.length; j++) { next[j] ^= g[j]; next[j + 1] ^= mul(g[j], EXP[i]); }
    g = next;
  }
  const rem = new Array(nsym).fill(0);
  for (const byte of data) {
    const f = byte ^ rem.shift();
    rem.push(0);
    for (let i = 0; i < nsym; i++) rem[i] ^= mul(g[i + 1], f);
  }
  return Uint8Array.from([...data, ...rem]);
}

/**
 * Put right up to nsym/2 wrong bytes.
 * @returns {Uint8Array|null} the data without its parity, or null when there
 *   are more errors than can be corrected. Null matters as much as the fix:
 *   it is what stops a misread from being taken for a code.
 */
export function rsDecode(word, nsym) {
  const n = word.length;
  const msg = Uint8Array.from(word);
  const evalAt = (x) => { let y = 0; for (let j = 0; j < n; j++) y = mul(y, x) ^ msg[j]; return y; };
  const synd = [];
  let clean = true;
  for (let i = 0; i < nsym; i++) { synd.push(evalAt(EXP[i])); if (synd[i]) clean = false; }
  if (clean) return msg.slice(0, n - nsym);

  // Berlekamp-Massey: the shortest polynomial whose roots mark the errors.
  let C = [1], B = [1], L = 0, m = 1, b = 1;
  for (let k = 0; k < nsym; k++) {
    let d = synd[k];
    for (let i = 1; i <= L; i++) d ^= mul(C[i] || 0, synd[k - i]);
    if (!d) { m++; continue; }
    const T = C.slice();
    const coef = div(d, b);
    while (C.length < B.length + m) C.push(0);
    for (let i = 0; i < B.length; i++) C[i + m] ^= mul(coef, B[i]);
    if (2 * L <= k) { L = k + 1 - L; B = T; b = d; m = 1; } else m++;
  }
  if (L * 2 > nsym) return null;

  // Where they are: position j holds the power n-1-j.
  const at = [];
  for (let p = 0; p < n; p++) {
    let y = 0;
    for (let i = L; i >= 0; i--) y = mul(y, pow(-p)) ^ (C[i] || 0);
    if (!y) at.push(p);
  }
  if (at.length !== L) return null;

  // How wrong each is (Forney).
  const omega = new Array(nsym).fill(0);
  for (let i = 0; i < nsym; i++) for (let j = 0; j <= Math.min(i, L); j++) omega[i] ^= mul(synd[i - j], C[j] || 0);
  for (const p of at) {
    const xi = pow(-p);
    let num = 0, den = 0;
    for (let i = nsym - 1; i >= 0; i--) num = mul(num, xi) ^ omega[i];
    for (let i = 1; i <= L; i += 2) den ^= mul(C[i] || 0, pow(-p * (i - 1)));
    if (!den) return null;
    msg[n - 1 - p] ^= mul(pow(p), div(num, den));
  }
  for (let i = 0; i < nsym; i++) if (evalAt(EXP[i])) return null;
  return msg.slice(0, n - nsym);
}

// A fixed scramble laid over the bytes, so a host name full of the same letter
// does not draw a ring that is all dash or all gap. Nothing secret about it.
function whiten(bytes) {
  const out = new Uint8Array(bytes.length);
  let s = 0x5a;
  for (let i = 0; i < bytes.length; i++) { s = (s * 73 + 41) & 0xff; out[i] = bytes[i] ^ s ^ (i * 29 & 0xff); }
  return out;
}

// ─── The link, packed ───
/** https://host[:port]/pair#<24 hex> -> { https, host, port, code }, or null. */
function splitLink(link) {
  const m = /^(https?):\/\/([a-z0-9.-]{1,253})(?::(\d{1,5}))?\/pair#([a-f0-9]{24})$/.exec(String(link || ''));
  if (!m) return null;
  const port = m[3] ? Number(m[3]) : 0;
  if (port > 65535) return null;
  const symbols = hostSymbols(m[2]);
  if (!symbols || symbols.length > 63) return null;          // the length is written in six bits
  return { https: m[1] === 'https', symbols, port, code: m[4] };
}

function pack(parts, size) {
  const need = 1 + CODE_BYTES + (parts.port ? 2 : 0) + hostBytes(parts.symbols.length);
  if (need > size) return null;
  const out = new Uint8Array(size);
  let o = 0;
  out[o++] = (parts.https ? 0x80 : 0) | (parts.port ? 0x40 : 0) | parts.symbols.length;
  for (let i = 0; i < CODE_BYTES; i++) out[o++] = parseInt(parts.code.substr(i * 2, 2), 16);
  if (parts.port) { out[o++] = parts.port >> 8; out[o++] = parts.port & 0xff; }
  let acc = 0, n = 0;
  for (const v of parts.symbols) {
    acc = (acc << 6) | v; n += 6;
    while (n >= 8) { out[o++] = (acc >> (n - 8)) & 0xff; n -= 8; }
    acc &= (1 << n) - 1;
  }
  if (n) out[o++] = (acc << (8 - n)) & 0xff;
  return out;
}

function unpack(bytes) {
  let o = 0;
  const head = bytes[o++];
  const len = head & 0x3f;
  if (!len) return null;
  let code = '';
  for (let i = 0; i < CODE_BYTES; i++) code += bytes[o++].toString(16).padStart(2, '0');
  let port = 0;
  if (head & 0x40) { port = (bytes[o] << 8) | bytes[o + 1]; o += 2; if (!port) return null; }
  if (o + hostBytes(len) > bytes.length) return null;
  let host = '', acc = 0, n = 0, read = 0;
  while (read < len) {
    acc = ((acc << 8) | bytes[o++]) & 0xffff; n += 8;
    while (n >= 6 && read < len) {
      const v = (acc >> (n - 6)) & 0x3f; n -= 6; read++;
      const piece = v < HOST_CHARS.length ? HOST_CHARS[v] : HOST_WORDS[v - HOST_CHARS.length];
      if (piece === undefined) return null;            // a value no app has been taught yet
      host += piece;
    }
  }
  // Everything after the host is padding and was written as zero. Anything
  // else there means this is not something encode() produced.
  for (; o < bytes.length; o++) if (bytes[o]) return null;
  return `${head & 0x80 ? 'https' : 'http'}://${host}${port ? ':' + port : ''}/pair#${code}`;
}

/**
 * The rings for a pairing link.
 *
 * @returns {{ cls: object, rings: Uint8Array[] } | null} rings[k][i] is 1 where
 *   ring k has a dash in cell i; cell 0 is the amber marker and is always 1.
 *   Null when the link cannot be carried: a host name too long for the largest
 *   size, or one that is not plain letters, digits, dots and hyphens (an IPv6
 *   address, say). The caller shows the QR code instead.
 */
export function encode(link) {
  const parts = splitLink(link);
  if (!parts) return null;
  for (const cls of CLASSES) {
    const payload = pack(parts, cls.payload);
    if (!payload) continue;
    const word = whiten(rsEncode(payload, cls.parity));
    const rings = [];
    let bit = 0;
    for (const n of cls.cells) {
      const ring = new Uint8Array(n);
      ring[0] = 1;
      for (let i = 1; i < n; i++, bit++) {
        // The few cells left over after the last whole byte alternate, which
        // looks like the rest and reads as nothing.
        ring[i] = bit < word.length * 8 ? (word[bit >> 3] >> (7 - (bit & 7))) & 1 : bit & 1;
      }
      rings.push(ring);
    }
    return { cls, rings };
  }
  return null;
}

/** The link back from rings already read in order (cell 0 the marker), or null. */
export function decodeRings(rings, cls) {
  const word = new Uint8Array(cls.bytes);
  let bit = 0;
  for (let k = 0; k < cls.cells.length; k++) {
    for (let i = 1; i < cls.cells[k]; i++, bit++) {
      if (bit < cls.bytes * 8 && rings[k][i]) word[bit >> 3] |= 1 << (7 - (bit & 7));
    }
  }
  const payload = rsDecode(whiten(word), cls.parity);
  return payload ? unpack(payload) : null;
}

// ─── Drawing ───
const f = (x) => (Math.round(x * 100) / 100).toString();

/**
 * The code as an animated SVG, or null when encode() cannot carry the link.
 *
 * The rings turn, slowly and in alternating directions, and the whole thing
 * opens outward from the logo when it first appears. None of the movement
 * carries anything: a still frame at any moment is a complete code. It also
 * carries its own ending, for when pairing succeeds: see doneClass.
 */
export function toSvg(link, { id = 'cc' } = {}) {
  const code = encode(link);
  if (!code) return null;
  const U = 100;                                   // one radius, in SVG units
  const { cls, rings } = code;
  const sw = cls.stroke * U;
  let body = '';
  rings.forEach((ring, k) => {
    const n = ring.length, r = RING_RADII[k] * U, step = (2 * Math.PI) / n;
    // A rounded end sticks out half the stroke; pull each dash in by that and
    // a little more, so a gap between two dashes stays a gap.
    const inset = (sw / 2 + 0.9) / r;
    const arc = (from, cells) => {
      let a0 = from * step + inset, a1 = (from + cells) * step - inset;
      // In the denser sizes one cell is narrower than its own rounded ends, so
      // there is no arc left to draw: it is a dot, drawn as the shortest arc
      // there is, in the middle of the cell.
      if (a1 - a0 < 0.004) { const mid = (from + cells / 2) * step; a0 = mid - 0.002; a1 = mid + 0.002; }
      const big = a1 - a0 > Math.PI ? 1 : 0;
      return `M${f(r * Math.cos(a0))} ${f(r * Math.sin(a0))}A${f(r)} ${f(r)} 0 ${big} 1 ${f(r * Math.cos(a1))} ${f(r * Math.sin(a1))}`;
    };
    let d = '';
    for (let i = 1; i < n; i++) {
      if (!ring[i]) continue;
      let run = 1;
      while (i + run < n && ring[i + run]) run++;
      d += arc(i, run);
      i += run - 1;
    }
    // Two groups, because two things move it. The inner one turns for as
    // long as the code is shown; the outer one is what pulls the ring into
    // the logo when pairing succeeds. On one element, starting the second
    // animation would restart the first and the ring would jump.
    body += `<g class="${id}-out ${id}-o${k}"><g class="${id}-ring ${id}-r${k}"><path d="${d}" stroke="url(#${id}-g${k % 2})"/><path d="${arc(0, 1)}" stroke="#fbbf24"/></g></g>`;
  });

  const spin = [70, -90, 110, -130, 150];           // seconds a turn; the sign is the direction
  // Each ring begins part-way through its turn, or the five markers would
  // start in a row and the code would open looking like a clock hand.
  const START = [0.62, 0.17, 0.81, 0.36, 0.05];
  const css = `
.${id}-ring{fill:none;stroke-width:${f(sw)};stroke-linecap:round;transform-origin:0 0}
.${id}-ring path{filter:drop-shadow(0 0 1.1px rgba(129,140,248,.45))}
.${id}-find{fill:none;stroke:#818cf8;stroke-width:${f(FINDER_WIDTH * U)};transform-origin:0 0}
${spin.map((s, k) => `.${id}-r${k}{animation:${id}-in .9s ${f(0.12 * k)}s cubic-bezier(.2,.9,.2,1) both,${id}-spin ${Math.abs(s)}s linear ${f(-Math.abs(s) * START[k])}s ${s < 0 ? 'reverse ' : ''}infinite}`).join('\n')}
.${id}-logo{transform-origin:0 0;animation:${id}-pop .7s cubic-bezier(.2,1.4,.3,1) both}
.${id}-find{animation:${id}-fade .8s ease both}
@keyframes ${id}-spin{to{rotate:360deg}}
@keyframes ${id}-in{from{opacity:0;scale:.55}to{opacity:1;scale:1}}
@keyframes ${id}-pop{from{opacity:0;scale:.4}to{opacity:1;scale:1}}
@keyframes ${id}-fade{from{opacity:0}to{opacity:1}}
.${id}-out{transform-origin:0 0}
.${id}-ripple{fill:none;stroke:#10b981;stroke-width:2.5;opacity:0;transform-origin:0 0}
.${id}-tick{transform-origin:0 0;scale:0}
.${id}-tick path{fill:none;stroke:#fff;stroke-width:3.2;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:1;stroke-dashoffset:1}
${spin.map((s, k) => `.${id}-done .${id}-o${k}{animation:${id}-gone${s < 0 ? 'b' : 'a'} .62s ${f(0.05 * (4 - k))}s cubic-bezier(.6,0,.85,.35) forwards}`).join('\n')}
.${id}-done .${id}-find{animation:${id}-seal .6s .2s cubic-bezier(.6,0,.3,1) forwards}
.${id}-done .${id}-find+.${id}-find{animation:${id}-close .6s .2s cubic-bezier(.6,0,.3,1) forwards}
.${id}-done .${id}-ripple{animation:${id}-ripple .9s .75s ease-out forwards}
.${id}-done .${id}-logo{animation:${id}-hop .5s .7s ease}
.${id}-done .${id}-tick{animation:${id}-pop .45s .85s cubic-bezier(.2,1.5,.3,1) forwards}
.${id}-done .${id}-tick path{animation:${id}-draw .3s 1.05s ease-out forwards}
@keyframes ${id}-gonea{to{opacity:0;scale:.3;rotate:170deg}}
@keyframes ${id}-goneb{to{opacity:0;scale:.3;rotate:-170deg}}
@keyframes ${id}-seal{to{stroke:#10b981;stroke-width:4.5}}
@keyframes ${id}-close{to{stroke:#10b981;scale:.38;opacity:0}}
@keyframes ${id}-ripple{from{opacity:.85;scale:1}to{opacity:0;scale:2.5}}
@keyframes ${id}-hop{40%{scale:1.14}}
@keyframes ${id}-draw{to{stroke-dashoffset:0}}
@media (prefers-reduced-motion:reduce){.${id}-ring,.${id}-logo,.${id}-find{animation:none}
.${id}-done .${id}-out,.${id}-done .${id}-find+.${id}-find{animation:none;opacity:0}
.${id}-done .${id}-find{animation:none;stroke:#10b981}
.${id}-done .${id}-logo,.${id}-done .${id}-ripple{animation:none}
.${id}-done .${id}-tick{animation:none;scale:1}.${id}-done .${id}-tick path{animation:none;stroke-dashoffset:0}}`;

  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="-114 -114 228 228" role="img" aria-label="Crundi pairing code" data-size="${cls.name}">`
    + `<style>${css}</style>`
    + `<defs>`
    + `<linearGradient id="${id}-g0" gradientUnits="userSpaceOnUse" x1="-100" y1="-100" x2="100" y2="100"><stop offset="0" stop-color="#a5b4fc"/><stop offset="1" stop-color="#6366f1"/></linearGradient>`
    + `<linearGradient id="${id}-g1" gradientUnits="userSpaceOnUse" x1="100" y1="-100" x2="-100" y2="100"><stop offset="0" stop-color="#7dd3fc"/><stop offset="1" stop-color="#818cf8"/></linearGradient>`
    + `<linearGradient id="${id}-lg" gradientUnits="userSpaceOnUse" x1="14" y1="14" x2="82" y2="82"><stop offset="0" stop-color="#818cf8"/><stop offset="1" stop-color="#4f46e5"/></linearGradient>`
    + `<mask id="${id}-eh"><rect width="96" height="96" fill="#fff"/><rect x="32" y="43" width="10.5" height="18" rx="5.2" fill="#000"/><rect x="53.5" y="43" width="10.5" height="18" rx="5.2" fill="#000"/></mask>`
    + `</defs>`
    // Always its own dark ground, whatever page it sits on: the reader looks
    // for light marks on dark.
    + `<rect x="-114" y="-114" width="228" height="228" rx="26" fill="#0a0a0f"/>`
    + `<circle class="${id}-find" r="${f(R_INNER * U)}"/><circle class="${id}-find" r="${f(R_OUTER * U)}"/>`
    + body
    + `<g class="${id}-logo"><g transform="translate(-24 -22.5) scale(.5)">`
    + `<path d="M48 27V17" fill="none" stroke-linecap="round" stroke="#fbbf24" stroke-width="5.5"/><circle cx="48" cy="13" r="6.5" fill="#fbbf24"/>`
    + `<rect x="11" y="26" width="74" height="57" rx="23" fill="url(#${id}-lg)" mask="url(#${id}-eh)"/>`
    + `</g></g>`
    // Drawn only once pairing has succeeded (see doneClass): a ring that
    // spreads out from the logo, and a tick on its shoulder.
    + `<circle class="${id}-ripple" r="${f(R_INNER * U)}"/>`
    + `<g transform="translate(25 23)"><g class="${id}-tick"><circle r="12.5" fill="#10b981" stroke="#0a0a0f" stroke-width="3"/><path pathLength="1" d="M-5.6 .6l3.7 3.9 7.6-8.4"/></g></g>`
    + `</svg>`;
}

/**
 * The class to put on the <svg> when the phone has been paired.
 *
 * The rings wind into the logo, the outer ring closes onto the inner one and
 * turns green, and a tick appears. It plays once and stays; DONE_MS is how
 * long it takes, for whoever wants to close the dialog afterwards. Once it is
 * on, the picture is no longer a code and cannot be scanned, which is the
 * point: that code has been spent.
 */
export const doneClass = (id = 'cc') => `${id}-done`;
export const DONE_MS = 1500;

// ─── Reading ───

// Fit an ellipse to a blob of pixels: least squares on the general conic
// A x^2 + B xy + C y^2 + D x + E y = 1, then turned into a centre and the
// symmetric matrix S that carries the unit circle onto the ellipse.
function fitEllipse(xs, ys, count, stride) {
  let cx = 0, cy = 0, n = 0;
  for (let i = 0; i < count; i += stride) { cx += xs[i]; cy += ys[i]; n++; }
  cx /= n; cy /= n;
  let sc = 0;
  for (let i = 0; i < count; i += stride) sc += Math.abs(xs[i] - cx) + Math.abs(ys[i] - cy);
  sc = sc / n || 1;
  const M = Array.from({ length: 5 }, () => new Float64Array(6));
  for (let i = 0; i < count; i += stride) {
    const x = (xs[i] - cx) / sc, y = (ys[i] - cy) / sc;
    const row = [x * x, x * y, y * y, x, y];
    for (let a = 0; a < 5; a++) { for (let b = 0; b < 5; b++) M[a][b] += row[a] * row[b]; M[a][5] += row[a]; }
  }
  for (let c = 0; c < 5; c++) {
    let best = c;
    for (let r = c + 1; r < 5; r++) if (Math.abs(M[r][c]) > Math.abs(M[best][c])) best = r;
    if (Math.abs(M[best][c]) < 1e-12) return null;
    [M[c], M[best]] = [M[best], M[c]];
    for (let r = 0; r < 5; r++) {
      if (r === c) continue;
      const k = M[r][c] / M[c][c];
      for (let j = c; j < 6; j++) M[r][j] -= k * M[c][j];
    }
  }
  const [A, B, C, D, E] = M.map((row, i) => row[5] / row[i]);
  const det = 4 * A * C - B * B;
  if (det <= 1e-12) return null;                    // not an ellipse
  const x0 = (B * E - 2 * C * D) / det, y0 = (B * D - 2 * A * E) / det;
  const k = 1 - (A * x0 * x0 + B * x0 * y0 + C * y0 * y0 + D * x0 + E * y0);
  if (k <= 0) return null;
  // (p-c)' P (p-c) = 1, and S = P^(-1/2).
  const p11 = A / k, p12 = B / (2 * k), p22 = C / k;
  const tr = p11 + p22, disc = Math.sqrt(Math.max(0, (p11 - p22) ** 2 / 4 + p12 * p12));
  const l1 = tr / 2 + disc, l2 = tr / 2 - disc;
  if (l2 <= 0) return null;
  let vx = p12, vy = l1 - p11;
  if (Math.abs(vx) + Math.abs(vy) < 1e-12) { vx = 1; vy = 0; }
  const vn = Math.hypot(vx, vy); vx /= vn; vy /= vn;
  const s1 = 1 / Math.sqrt(l1), s2 = 1 / Math.sqrt(l2);
  return {
    cx: cx + x0 * sc, cy: cy + y0 * sc,
    // S = s1 v v' + s2 w w', w perpendicular to v.
    s11: (s1 * vx * vx + s2 * vy * vy) * sc, s12: ((s1 - s2) * vx * vy) * sc, s22: (s1 * vy * vy + s2 * vx * vx) * sc,
    p11: p11 / (sc * sc), p12: p12 / (sc * sc), p22: p22 / (sc * sc),
    size: Math.sqrt(s1 * s2) * sc,
  };
}

// Every closed ring in the picture, each as a fitted ellipse.
function findRings(v, W, H) {
  // Light against its surroundings: compare each pixel with the average of a
  // window round it, from an integral image. A plain global threshold fails as
  // soon as part of the screen behind the code is bright.
  const integ = new Float64Array((W + 1) * (H + 1));
  for (let y = 0; y < H; y++) {
    let row = 0;
    for (let x = 0; x < W; x++) { row += v[y * W + x]; integ[(y + 1) * (W + 1) + x + 1] = integ[y * (W + 1) + x + 1] + row; }
  }
  const half = Math.max(8, Math.round(Math.min(W, H) / 12));
  const on = new Uint8Array(W * H);
  for (let y = 0; y < H; y++) {
    const y0 = Math.max(0, y - half), y1 = Math.min(H, y + half + 1);
    for (let x = 0; x < W; x++) {
      const x0 = Math.max(0, x - half), x1 = Math.min(W, x + half + 1);
      const sum = integ[y1 * (W + 1) + x1] - integ[y0 * (W + 1) + x1] - integ[y1 * (W + 1) + x0] + integ[y0 * (W + 1) + x0];
      if (v[y * W + x] > (sum / ((x1 - x0) * (y1 - y0))) * 1.3 + 14) on[y * W + x] = 1;
    }
  }

  const rings = [];
  const xs = new Int32Array(W * H), ys = new Int32Array(W * H);
  const minSide = Math.max(14, Math.min(W, H) / 24);
  for (let start = 0; start < W * H; start++) {
    if (on[start] !== 1) continue;
    // Flood fill one blob, eight ways. xs/ys double as the work list.
    let count = 0, minX = W, maxX = 0, minY = H, maxY = 0;
    xs[0] = start % W; ys[0] = (start / W) | 0; on[start] = 2; count = 1;
    for (let i = 0; i < count; i++) {
      const x = xs[i], y = ys[i];
      if (x < minX) minX = x; if (x > maxX) maxX = x; if (y < minY) minY = y; if (y > maxY) maxY = y;
      for (let dy = -1; dy <= 1; dy++) {
        const yy = y + dy; if (yy < 0 || yy >= H) continue;
        for (let dx = -1; dx <= 1; dx++) {
          const xx = x + dx; if (xx < 0 || xx >= W) continue;
          const j = yy * W + xx;
          if (on[j] === 1) { on[j] = 2; xs[count] = xx; ys[count] = yy; count++; }
        }
      }
    }
    const w = maxX - minX + 1, h = maxY - minY + 1;
    if (w < minSide || h < minSide || w > h * 2.5 || h > w * 2.5) continue;
    if (count > w * h * 0.4 || count < (w + h)) continue;      // a ring is mostly hole
    const e = fitEllipse(xs, ys, count, Math.max(1, Math.floor(count / 3000)));
    if (!e) continue;
    // Is it really a ring: every pixel near the ellipse, and all the way round.
    let dev = 0, seen = 0, n = 0;
    const stride = Math.max(1, Math.floor(count / 2000));
    for (let i = 0; i < count; i += stride) {
      const dx = xs[i] - e.cx, dy = ys[i] - e.cy;
      dev += Math.abs(Math.sqrt(e.p11 * dx * dx + 2 * e.p12 * dx * dy + e.p22 * dy * dy) - 1); n++;
      seen |= 1 << (Math.floor(((Math.atan2(dy, dx) + Math.PI) / (2 * Math.PI)) * 24) % 24);
    }
    if (dev / n > 0.07 || seen !== 0xffffff) continue;
    rings.push(e);
  }
  return rings;
}

// The centre of a conic given as a symmetric 3x3 [a, b, c, d, e, f] for
// a x^2 + 2b xy + c y^2 + 2d x + 2e y + f = 0, and its shape about that centre.
function conicEllipse([a, b, c, d, e, f]) {
  const det = a * c - b * b;
  if (Math.abs(det) < 1e-12) return null;
  const x0 = (b * e - c * d) / det, y0 = (b * d - a * e) / det;
  const k = -(a * x0 * x0 + 2 * b * x0 * y0 + c * y0 * y0 + 2 * d * x0 + 2 * e * y0 + f);
  if (!(k > 0) && !(k < 0)) return null;
  const p11 = a / k, p12 = b / k, p22 = c / k;
  const tr = p11 + p22, disc = Math.sqrt(Math.max(0, (p11 - p22) ** 2 / 4 + p12 * p12));
  const l1 = tr / 2 + disc, l2 = tr / 2 - disc;
  if (l2 <= 0) return null;
  let vx = p12, vy = l1 - p11;
  if (Math.abs(vx) + Math.abs(vy) < 1e-12) { vx = 1; vy = 0; }
  const vn = Math.hypot(vx, vy); vx /= vn; vy /= vn;
  const s1 = 1 / Math.sqrt(l1), s2 = 1 / Math.sqrt(l2);
  return { x0, y0, s11: s1 * vx * vx + s2 * vy * vy, s12: (s1 - s2) * vx * vy, s22: s1 * vy * vy + s2 * vx * vx };
}

/**
 * From the two finder rings, a function that says where any point of the code
 * is in the picture.
 *
 * Seen at an angle, a circle is an ellipse, but an ellipse alone does not say
 * which way the code leans: its centre is not where the circle's centre went,
 * and equal steps round it are not equal steps round the circle. Two circles
 * with one centre do say. Undo the lean correctly and their two ellipses share
 * a centre again; undo it wrongly and they do not. So the lean (two numbers,
 * the line the code's plane vanishes to) is solved for until the centres meet,
 * and what is left is a plain stretch that the outer ring gives directly.
 *
 * @returns {(radius: number, ux: number, uy: number) => [number, number]}
 */
function locate(inner, outer) {
  const scale = outer.size, ox = outer.cx, oy = outer.cy;
  // Each ring as a conic, in coordinates centred on the outer ring and one
  // outer-radius big, so the numbers below are all of order one.
  const conic = (e) => {
    const cx = (e.cx - ox) / scale, cy = (e.cy - oy) / scale;
    const a = e.p11 * scale * scale, b = e.p12 * scale * scale, c = e.p22 * scale * scale;
    return [a, b, c, -(a * cx + b * cy), -(b * cx + c * cy), a * cx * cx + 2 * b * cx * cy + c * cy * cy - 1];
  };
  // The same conic after the picture is straightened by (x, y) / (1 + l1 x + l2 y).
  const straighten = ([a, b, c, d, e, f], l1, l2) => [
    a - 2 * d * l1 + f * l1 * l1, b - d * l2 - e * l1 + f * l1 * l2, c - 2 * e * l2 + f * l2 * l2,
    d - f * l1, e - f * l2, f,
  ];
  const ci = conic(inner), co = conic(outer);
  const miss = (l1, l2) => {
    const a = conicEllipse(straighten(ci, l1, l2)), b = conicEllipse(straighten(co, l1, l2));
    return a && b ? [a.x0 - b.x0, a.y0 - b.y0] : null;
  };
  let l1 = 0, l2 = 0;
  for (let it = 0; it < 12; it++) {
    const m = miss(l1, l2);
    if (!m) { l1 = l2 = 0; break; }
    if (Math.hypot(m[0], m[1]) < 1e-6) break;
    const h = 1e-4, mx = miss(l1 + h, l2), my = miss(l1, l2 + h);
    if (!mx || !my) { l1 = l2 = 0; break; }
    const j11 = (mx[0] - m[0]) / h, j21 = (mx[1] - m[1]) / h, j12 = (my[0] - m[0]) / h, j22 = (my[1] - m[1]) / h;
    const det = j11 * j22 - j12 * j21;
    if (Math.abs(det) < 1e-9) break;
    l1 -= (j22 * m[0] - j12 * m[1]) / det;
    l2 -= (j11 * m[1] - j21 * m[0]) / det;
    // Further than this is a code seen nearly edge-on, which cannot be read
    // whatever is solved for.
    if (Math.hypot(l1, l2) > 0.9) { l1 = l2 = 0; break; }
  }
  const flat = conicEllipse(straighten(co, l1, l2)) || conicEllipse(co);
  if (!flat) return null;
  return (radius, ux, uy) => {
    const k = radius / R_OUTER;
    const qx = flat.x0 + k * (flat.s11 * ux + flat.s12 * uy), qy = flat.y0 + k * (flat.s12 * ux + flat.s22 * uy);
    const w = 1 - l1 * qx - l2 * qy;
    return [ox + (scale * qx) / w, oy + (scale * qy) / w];
  };
}

// Read one data ring: where its cells fall, which are lit, and which is amber.
function readRing(img, at, radius, cells, stroke) {
  const { data, width: W, height: H } = img;
  const OS = 6, total = cells * OS;
  const lum = new Float32Array(total), amber = new Float32Array(total);
  for (let j = 0; j < total; j++) {
    const a = (2 * Math.PI * j) / total, ux = Math.cos(a), uy = Math.sin(a);
    let r = 0, g = 0, b = 0;
    // Three samples across the width of the dash, not one on its centre line.
    for (const off of [-0.25, 0, 0.25]) {
      const [px, py] = at(radius + off * stroke, ux, uy);
      const x = Math.round(px), y = Math.round(py);
      if (x < 0 || y < 0 || x >= W || y >= H) return null;
      const p = (y * W + x) * 4;
      r += data[p]; g += data[p + 1]; b += data[p + 2];
    }
    lum[j] = Math.max(r, g, b) / 3;
    amber[j] = ((r + g) / 2 - b) / 3;
  }
  // The dashes can be anywhere round the ring, so find where the cell
  // boundaries are: the offset at which cells differ most from each other.
  let bestPhase = 0, bestScore = -1, bestVals = null;
  for (let ph = 0; ph < OS; ph++) {
    const vals = new Float32Array(cells);
    let sum = 0, sq = 0;
    for (let i = 0; i < cells; i++) {
      const c = ph + i * OS + (OS >> 1);
      const x = (lum[(c - 1) % total] + lum[c % total] + lum[(c + 1) % total]) / 3;
      vals[i] = x; sum += x; sq += x * x;
    }
    const score = sq / cells - (sum / cells) ** 2;
    if (score > bestScore) { bestScore = score; bestPhase = ph; bestVals = vals; }
  }
  let lo = 255, hi = 0;
  for (const x of bestVals) { if (x < lo) lo = x; if (x > hi) hi = x; }
  if (hi - lo < 30) return null;
  const mid = (lo + hi) / 2;
  let sync = -1, syncScore = 0;
  const bits = new Uint8Array(cells);
  for (let i = 0; i < cells; i++) {
    if (bestVals[i] <= mid) continue;
    bits[i] = 1;
    const c = bestPhase + i * OS + (OS >> 1);
    const a = (amber[(c - 1) % total] + amber[c % total] + amber[(c + 1) % total]) / 3;
    // Amber is red and green without blue; every other dash is mostly blue.
    if (a > syncScore) { syncScore = a; sync = i; }
  }
  if (sync < 0 || syncScore < 20) return null;
  const out = new Uint8Array(cells);
  for (let i = 0; i < cells; i++) out[i] = bits[(sync + i) % cells];
  return out;
}

/**
 * Find and read a Crundi code in a picture.
 *
 * @param {{ data: Uint8Array|Uint8ClampedArray, width: number, height: number }} img
 *   RGBA, as a canvas gives it. A camera frame around 640 pixels wide is plenty.
 * @returns {{ link: string, size: string } | null}
 */
export function decodeImage(img) {
  const { data, width: W, height: H } = img;
  if (!data || W < 32 || H < 32) return null;
  const v = new Uint8Array(W * H);
  for (let i = 0, p = 0; i < v.length; i++, p += 4) v[i] = Math.max(data[p], data[p + 1], data[p + 2]);
  const found = findRings(v, W, H).sort((a, b) => b.size - a.size);
  for (const outer of found) {
    for (const inner of found) {
      const ratio = inner.size / outer.size;
      if (ratio < 0.30 || ratio > 0.47) continue;                      // R_INNER / R_OUTER is 0.38
      if (Math.hypot(inner.cx - outer.cx, inner.cy - outer.cy) > outer.size * 0.2) continue;
      const at = locate(inner, outer);
      if (!at) continue;
      for (const cls of CLASSES) {
        const rings = [];
        for (let k = 0; k < cls.cells.length; k++) {
          const ring = readRing(img, at, RING_RADII[k], cls.cells[k], cls.stroke);
          if (!ring) break;
          rings.push(ring);
        }
        if (rings.length !== cls.cells.length) continue;
        const link = decodeRings(rings, cls);
        if (link) return { link, size: cls.name };
      }
    }
  }
  return null;
}
