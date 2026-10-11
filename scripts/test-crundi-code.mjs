#!/usr/bin/env node
/**
 * test-crundi-code.mjs — the picture a phone scans to pair (src/crundi-code.js).
 *
 * Two halves are held to each other here: what draws the code and what reads
 * it. The reader is given pictures the way a camera would give them, not the
 * neat grid the drawer worked from: turned, seen at an angle, blurred, noisy,
 * washed out, with the rest of a screen around it and every ring at a
 * different point of its turn. The pictures are painted by a small software
 * renderer in this file, so the test needs no browser.
 *
 * The rule that matters most is the last one: the reader may fail to read a
 * code, and it may never read one WRONG.
 *
 * Run: node scripts/test-crundi-code.mjs
 */

import * as cc from '../src/crundi-code.js';

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// A fixed sequence, so a failure can be run again and seen again.
let seed = 20260;
const rnd = () => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x7fffffff; };
const rint = (n) => Math.floor(rnd() * n);

// ─── Error correction ───
{
  let fixed = 0, refused = 0, wrong = 0, runs = 0;
  for (let t = 0; t < 1500; t++) {
    const cls = cc.CLASSES[t % 3];
    const data = Uint8Array.from({ length: cls.payload }, () => rint(256));
    const word = cc.rsEncode(data, cls.parity);
    const damage = (count) => { const bad = Uint8Array.from(word); const at = new Set(); while (at.size < count) at.add(rint(bad.length)); for (const p of at) bad[p] ^= 1 + rint(255); return bad; };
    const a = cc.rsDecode(damage(rint(cls.parity / 2 + 1)), cls.parity);
    if (a && a.every((x, i) => x === data[i])) fixed++;
    const b = cc.rsDecode(damage(cls.parity / 2 + 1 + rint(8)), cls.parity);
    runs++;
    if (!b) refused++; else if (!b.every((x, i) => x === data[i])) wrong++;
  }
  ok(fixed === runs, `up to half the parity in wrong bytes is put right (${fixed}/${runs})`);
  ok(wrong === 0, `more damage than that is refused, never guessed at (${refused} refused, ${wrong} wrong)`);
}

// ─── What fits ───
const CODE = '0123456789abcdef01234567';
const LINKS = {
  S: `https://dev.faruhaan.com/pair#${CODE}`,
  L: `https://crundi.example-team.dev:8443/pair#${CODE}`,
  XL: `https://colgate-instances-moderate-spoken.trycloudflare.com/pair#${CODE}`,
  XXL: `https://a-very-long-subdomain-name.another-long-part.example-company.co.uk/pair#${CODE}`,
};
for (const [size, link] of Object.entries(LINKS)) {
  const c = cc.encode(link);
  ok(c && c.cls.name === size && cc.decodeRings(c.rings, c.cls) === link, `${size}: ${link.replace(/#.*/, '')} goes in and comes back`);
  ok(c.rings.every((r, k) => r.length === c.cls.cells[k] && r[0] === 1), `${size}: every ring starts with its marker`);
}
ok(cc.decodeRings(cc.encode(`http://192.168.1.20:8888/pair#${'f'.repeat(24)}`).rings, cc.CLASSES[0]) === `http://192.168.1.20:8888/pair#${'f'.repeat(24)}`, 'a plain-http LAN address with a port survives');
for (const bad of [`https://${'x'.repeat(64)}/pair#${CODE}`, `https://[::1]/pair#${CODE}`, `https://dev.faruhaan.com/pair#${CODE}00`, `https://dev.faruhaan.com/other#${CODE}`, `https://Dev.Faruhaan.com/pair#${CODE}`, 'nonsense', '']) {
  ok(cc.encode(bad) === null && cc.toSvg(bad) === null, 'cannot be carried, so the caller falls back to the QR code: ' + JSON.stringify(bad.slice(0, 34)));
}
{
  // Whole pieces of common names cost one symbol each, which is what lets a
  // fifty-character quick tunnel address fit at all.
  const size = (host) => (cc.encode(`https://${host}/pair#${CODE}`) || { cls: { name: 'none' } }).cls.name;
  ok(size('colgate-instances-moderate-spoken.trycloudflare.com') === 'XL', 'a Cloudflare quick tunnel address (51 characters) fits');
  ok(size('myserver.tailnet-name.ts.net') === 'L' && size('www.crundi.io') === 'S' && size('192.168.1.20') === 'S', 'so do a Tailscale name, a short domain and a LAN address, in smaller sizes');
  for (const host of ['x.ngrok-free.app', 'a.app', 'www.www.com', 'co.uk.co.uk', '10.0.10.0.', 'trycloudflare.com', 'x.trycloudflare.community']) {
    const link = `https://${host}/pair#${CODE}`, c = cc.encode(link);
    if (!(c && cc.decodeRings(c.rings, c.cls) === link)) ok(false, 'a name made of the shorthand pieces comes back exactly: ' + host);
  }
  ok(true, 'names built from the shorthand pieces, and names that only look like them, come back exactly');
}
{
  // No ring should come out nearly empty or nearly solid, whatever the host.
  let worst = 1, best = 0;
  for (const host of ['aaaaaaaaaaaaaaaa', '0000.0000.0000', 'zzzzzzzzzzzzzzzzz', 'a.b']) {
    for (const r of cc.encode(`https://${host}/pair#${'0'.repeat(24)}`).rings) { const d = r.reduce((a, b) => a + b, 0) / r.length; worst = Math.min(worst, d); best = Math.max(best, d); }
  }
  ok(worst > 0.25 && best < 0.75, `a repetitive host name still draws mixed rings (${Math.round(worst * 100)}% to ${Math.round(best * 100)}% lit)`);
}

// ─── The drawing ───
const svg = cc.toSvg(LINKS.S);
ok(svg.startsWith('<svg ') && svg.includes('data-size="S"') && (svg.match(/class="cc-ring /g) || []).length === 5, 'the SVG has five data rings');
ok((svg.match(/stroke="#fbbf24"\/><\/g>/g) || []).length === 5, 'and one amber marker in each');
ok((svg.match(/class="cc-find"/g) || []).length === 2 && svg.includes('fill="#0a0a0f"'), 'two finder rings, on its own dark ground');
ok(svg.includes('prefers-reduced-motion') && svg.includes('cc-spin'), 'it turns, and stays still for someone who asked for less motion');
ok(!/<script|javascript:|on[a-z]+=/i.test(svg), 'and carries no script');
ok(cc.doneClass() === 'cc-done' && cc.doneClass('x1') === 'x1-done' && cc.DONE_MS >= 1000, 'the class that plays the ending is named after the same id');
ok(svg.includes('.cc-done .cc-o0{animation:cc-gone') && svg.includes('class="cc-ripple"') && svg.includes('class="cc-tick"'), 'the ending is in the picture: rings wind in, a ripple, a tick');
ok(/\.cc-tick\{[^}]*scale:0/.test(svg) && /\.cc-ripple\{[^}]*opacity:0/.test(svg), 'and none of it shows until pairing has succeeded');
ok((svg.match(/class="cc-out cc-o\d"><g class="cc-ring cc-r\d">/g) || []).length === 5, 'each ring turns in one group and leaves in another, so leaving does not make it jump');
ok(cc.toSvg(LINKS.S, { id: 'x1' }).includes('id="x1-g0"') && !cc.toSvg(LINKS.S, { id: 'x1' }).includes('id="cc-'), 'two on one page do not share ids');

// ─── A camera's view of it ───
// Paint the code into pixels: where each pixel lands on the code, and what is
// there. `rot` is how far each ring has turned.
function paint(link, { W = 360, H = 360, size = 150, turn = 0, tiltX = 0, tiltY = 0, depth = 4, rot = null, page = [18, 18, 26], clutter = false, hole = null } = {}) {
  const { cls, rings } = cc.encode(link);
  const rots = rot || rings.map(() => rnd() * Math.PI * 2);
  const INDIGO = [129, 140, 248], AMBER = [251, 191, 36], DARK = [10, 10, 15];
  const shade = (x, y) => {
    if (Math.abs(x) > 1.14 || Math.abs(y) > 1.14) return clutter && (y < -1.5 || (x > 1.3 && y > 0.9)) ? [245, 245, 250] : page;
    const r = Math.hypot(x, y);
    if (hole && Math.hypot(x - hole[0], y - hole[1]) < hole[2]) return DARK;
    if (Math.abs(r - cc.R_INNER) < cc.FINDER_WIDTH / 2 || Math.abs(r - cc.R_OUTER) < cc.FINDER_WIDTH / 2) return INDIGO;
    if (r < 0.26) return Math.hypot(x, y + 0.2) < 0.045 ? AMBER : (Math.abs(x) < 0.19 && Math.abs(y - 0.03) < 0.15 ? [99, 102, 241] : DARK);
    for (let k = 0; k < rings.length; k++) {
      if (Math.abs(r - cc.RING_RADII[k]) >= cls.stroke / 2) continue;
      const n = rings[k].length, step = (Math.PI * 2) / n;
      let a = (Math.atan2(y, x) - rots[k]) % (Math.PI * 2); if (a < 0) a += Math.PI * 2;
      const i = Math.min(n - 1, Math.floor(a / step));
      if (!rings[k][i]) return DARK;
      // Dashes stop short of the cell edge unless the next cell is lit too.
      // The same gap the SVG leaves once its rounded ends are counted.
      const inset = 0.009 / cc.RING_RADII[k], into = a - i * step;
      const joinPrev = i > 1 && rings[k][i - 1], joinNext = i > 0 && i < n - 1 && rings[k][i + 1];
      if ((into < inset && !joinPrev) || (step - into < inset && !joinNext)) return DARK;
      return i === 0 ? AMBER : INDIGO;
    }
    return DARK;
  };
  // code point -> picture: turn, lean (a pinhole `depth` radii away), scale.
  const ct = Math.cos(turn), st = Math.sin(turn), cx_ = Math.cos(tiltX), sx_ = Math.sin(tiltX), cy_ = Math.cos(tiltY), sy_ = Math.sin(tiltY);
  // Rows of R = Ry(tiltY) Rx(tiltX) Rz(turn), applied to (x, y, 0).
  const m = [
    [cy_ * ct + sy_ * sx_ * st, -cy_ * st + sy_ * sx_ * ct],
    [cx_ * st, cx_ * ct],
    [-sy_ * ct + cy_ * sx_ * st, sy_ * st + cy_ * sx_ * ct],
  ];
  // Inverse by solving the 2x2 for each pixel (u, v): u = X/(1+Z/depth) ...
  const data = new Uint8ClampedArray(W * H * 4);
  const SS = 3;
  for (let py = 0; py < H; py++) {
    for (let px = 0; px < W; px++) {
      let R = 0, G = 0, B = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const u = (px + (sx + 0.5) / SS - W / 2) / size, v = (py + (sy + 0.5) / SS - H / 2) / size;
          // u (1 + Z/d) = X, v (1 + Z/d) = Y with X, Y, Z linear in (x, y).
          const a11 = m[0][0] - (u * m[2][0]) / depth, a12 = m[0][1] - (u * m[2][1]) / depth;
          const a21 = m[1][0] - (v * m[2][0]) / depth, a22 = m[1][1] - (v * m[2][1]) / depth;
          const det = a11 * a22 - a12 * a21;
          const c = shade((u * a22 - v * a12) / det, (v * a11 - u * a21) / det);
          R += c[0]; G += c[1]; B += c[2];
        }
      }
      const p = (py * W + px) * 4;
      data[p] = R / (SS * SS); data[p + 1] = G / (SS * SS); data[p + 2] = B / (SS * SS); data[p + 3] = 255;
    }
  }
  return { data, width: W, height: H };
}

// What a camera does to it.
function blur(img, radius) {
  const { width: W, height: H } = img;
  for (let pass = 0; pass < 2; pass++) {
    const src = Uint8ClampedArray.from(img.data);
    for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) for (let ch = 0; ch < 3; ch++) {
      let s = 0, n = 0;
      for (let d = -radius; d <= radius; d++) {
        const xx = pass ? x : x + d, yy = pass ? y + d : y;
        if (xx < 0 || yy < 0 || xx >= W || yy >= H) continue;
        s += src[(yy * W + xx) * 4 + ch]; n++;
      }
      img.data[(y * W + x) * 4 + ch] = s / n;
    }
  }
  return img;
}
function grade(img, { gain = 1, lift = 0, noise = 0, tint = [1, 1, 1] } = {}) {
  for (let i = 0; i < img.data.length; i += 4) for (let ch = 0; ch < 3; ch++) {
    img.data[i + ch] = img.data[i + ch] * gain * tint[ch] + lift + (noise ? (rnd() + rnd() - 1) * noise : 0);
  }
  return img;
}

const read = (img) => { const got = cc.decodeImage(img); return got ? got.link : null; };
const deg = (d) => (d * Math.PI) / 180;
const VIEWS = [
  ['S, straight on', () => paint(LINKS.S)],
  ['L, straight on', () => paint(LINKS.L)],
  ['XL, straight on', () => paint(LINKS.XL)],
  ['S, small in a wide frame with a bright screen round it', () => paint(LINKS.S, { W: 640, H: 480, size: 80, clutter: true })],
  ['S, phone turned a third of the way round', () => paint(LINKS.S, { turn: deg(123) })],
  ['S, leaning 30 degrees back', () => paint(LINKS.S, { tiltX: deg(30), size: 140 })],
  ['S, leaning 40 degrees sideways and turned, close up', () => paint(LINKS.S, { tiltY: deg(40), turn: deg(-50), depth: 3, size: 130 })],
  ['L, leaning both ways', () => paint(LINKS.L, { tiltX: deg(-22), tiltY: deg(26), turn: deg(15), size: 140 })],
  ['XL, leaning 30 degrees', () => paint(LINKS.XL, { tiltX: deg(30), turn: deg(70), size: 150 })],
  ['S, out of focus', () => blur(paint(LINKS.S), 2)],
  ['S, grainy', () => grade(paint(LINKS.S), { noise: 34 })],
  ['S, washed out by a reflection (dim, with the blacks lifted)', () => grade(paint(LINKS.S), { gain: 0.55, lift: 55 })],
  ['S, warm white balance', () => grade(paint(LINKS.S), { tint: [1.0, 0.86, 0.62] })],
  ['S, cold white balance', () => grade(paint(LINKS.S), { tint: [0.7, 0.9, 1.0] })],
  ['S, everything at once: leaning, soft, grainy, dim', () => grade(blur(paint(LINKS.S, { tiltX: deg(20), tiltY: deg(-15), turn: deg(200), size: 145 }), 1), { gain: 0.7, lift: 30, noise: 16 })],
  ['XL, soft and grainy', () => grade(blur(paint(LINKS.XL, { size: 165 }), 1), { noise: 14 })],
  ['XXL, straight on', () => paint(LINKS.XXL, { W: 420, H: 420, size: 180 })],
  ['XXL, leaning 25 degrees and turned', () => paint(LINKS.XXL, { W: 420, H: 420, size: 175, tiltY: deg(25), turn: deg(-100) })],
  ['S, a smudge over part of two rings', () => paint(LINKS.S, { hole: [0.52, 0.42, 0.1] })],
  ['L, a smudge over part of two rings', () => paint(LINKS.L, { hole: [-0.6, 0.3, 0.1] })],
];
let never = true;
for (const [name, make] of VIEWS) {
  const want = LINKS[name.split(',')[0]];
  const got = read(make());
  if (got && got !== want) never = false;
  ok(got === want, 'read: ' + name);
}

// The rings turn all the time. Any moment is as good as any other.
{
  let good = 0;
  for (let i = 0; i < 12; i++) if (read(paint(LINKS.S, { W: 300, H: 300, size: 125 })) === LINKS.S) good++;
  ok(good === 12, `read at twelve different moments of the turn (${good}/12)`);
}

// ─── Never wrong ───
{
  const noise = { data: Uint8ClampedArray.from({ length: 240 * 240 * 4 }, () => rint(256)), width: 240, height: 240 };
  ok(read(noise) === null, 'static is not a code');
  ok(read({ data: new Uint8ClampedArray(240 * 240 * 4), width: 240, height: 240 }) === null, 'a black frame is not a code');
  ok(read({ data: new Uint8ClampedArray(16), width: 2, height: 2 }) === null && read({}) === null, 'nor is nothing at all');

  // Two rings and no dashes: the finder with nothing to find.
  const empty = paint(LINKS.S);
  for (let y = 0; y < 360; y++) for (let x = 0; x < 360; x++) {
    const r = Math.hypot(x - 180, y - 180) / 150;
    if (r > 0.42 && r < 0.96) { const p = (y * 360 + x) * 4; empty.data[p] = 10; empty.data[p + 1] = 10; empty.data[p + 2] = 15; }
  }
  ok(read(empty) === null, 'two bare rings are not a code');

  // Damage that grows until it cannot be read. Each step is either the right
  // link or nothing.
  let stillRight = 0, gaveUp = 0;
  for (let size = 0.05; size <= 0.6; size += 0.05) {
    const got = read(paint(LINKS.S, { hole: [0.6, 0.3, size] }));
    if (got === LINKS.S) stillRight++; else if (got === null) gaveUp++; else never = false;
  }
  ok(stillRight >= 2 && gaveUp >= 2, `a growing smudge is read until it is too big, then refused (${stillRight} read, ${gaveUp} refused)`);

  // And a pile of badly degraded frames.
  let tried = 0, readOk = 0;
  for (let i = 0; i < 40; i++) {
    const img = grade(blur(paint([LINKS.S, LINKS.L, LINKS.XL][i % 3], { W: 260, H: 260, size: 60 + rint(60), tiltX: deg(rint(70) - 35), tiltY: deg(rint(70) - 35), turn: rnd() * 6.28, depth: 2.5 + rnd() * 2 }), rint(3)), { gain: 0.4 + rnd() * 0.6, lift: rint(70), noise: rint(60) });
    const got = read(img); tried++;
    if (got) { if (got === [LINKS.S, LINKS.L, LINKS.XL][i % 3]) readOk++; else never = false; }
  }
  console.log(`      (${readOk}/${tried} of the badly degraded frames were still read)`);
  ok(never, 'across every picture in this file, no code was ever read as a different link');
}

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
