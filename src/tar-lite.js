/**
 * tar-lite.js — a small tar writer (ustar, with pax headers for long names),
 * enough to send a project's changed files to a worker (src/workers-jobs.js).
 * Regular files (mode kept, so executables stay executable) and symlinks.
 */
import { lstatSync, readlinkSync, openSync, readSync, closeSync } from 'node:fs';

const BLOCK = 512;

function octal(n, width) {
  const s = Math.max(0, Math.floor(n)).toString(8);
  return s.padStart(width - 1, '0') + '\0';
}

function header({ name, size, mode, mtime, type = '0', linkname = '' }) {
  const h = Buffer.alloc(BLOCK, 0);
  h.write(name.slice(0, 100), 0, 'utf8');
  h.write(octal(mode & 0o7777, 8), 100, 'ascii');
  h.write(octal(0, 8), 108, 'ascii');
  h.write(octal(0, 8), 116, 'ascii');
  h.write(octal(size, 12), 124, 'ascii');
  h.write(octal(mtime, 12), 136, 'ascii');
  h.fill(' ', 148, 156);
  h.write(type, 156, 'ascii');
  h.write(linkname.slice(0, 100), 157, 'utf8');
  h.write('ustar\0', 257, 'ascii');
  h.write('00', 263, 'ascii');
  let sum = 0;
  for (let i = 0; i < BLOCK; i++) sum += h[i];
  h.write(octal(sum, 7) + ' ', 148, 'ascii');
  return h;
}

function paxRecord(key, value) {
  const body = ` ${key}=${value}\n`;
  let len = Buffer.byteLength(body) + 1;
  while (Buffer.byteLength(String(len) + body) !== len) len = Buffer.byteLength(String(len) + body);
  return String(len) + body;
}

const pad = (n) => (n % BLOCK ? Buffer.alloc(BLOCK - (n % BLOCK), 0) : Buffer.alloc(0));

/**
 * Write `files` (relative paths under `root`) as a tar, calling `out(buf)` for
 * every piece in order (it may return a promise, which is awaited: that is
 * the backpressure). Files that vanished are skipped.
 */
export async function writeTar(root, files, out) {
  const CHUNK = 256 * 1024;
  for (const rel of files) {
    const abs = `${root}/${rel}`;
    let st;
    try { st = lstatSync(abs); } catch { continue; }
    const name = rel.split('\\').join('/');
    const isLink = st.isSymbolicLink();
    if (!isLink && !st.isFile()) continue;
    const linkname = isLink ? readlinkSync(abs) : '';
    const longName = Buffer.byteLength(name) > 100 || Buffer.byteLength(linkname) > 100;
    if (longName) {
      const pax = Buffer.from(paxRecord('path', name) + (linkname ? paxRecord('linkpath', linkname) : ''), 'utf8');
      await out(header({ name: 'PaxHeader', size: pax.length, mode: 0o644, mtime: st.mtimeMs / 1000, type: 'x' }));
      await out(pax); await out(pad(pax.length));
    }
    if (isLink) { await out(header({ name, size: 0, mode: 0o777, mtime: st.mtimeMs / 1000, type: '2', linkname })); continue; }
    await out(header({ name, size: st.size, mode: st.mode, mtime: st.mtimeMs / 1000 }));
    const fd = openSync(abs, 'r');
    try {
      let left = st.size, pos = 0;
      while (left > 0) {
        const buf = Buffer.alloc(Math.min(CHUNK, left));
        const n = readSync(fd, buf, 0, buf.length, pos);
        if (n <= 0) { await out(Buffer.alloc(left, 0)); break; }   // shrank meanwhile: pad to the size we declared
        await out(n === buf.length ? buf : buf.subarray(0, n));
        left -= n; pos += n;
      }
    } finally { closeSync(fd); }
    await out(pad(st.size));
  }
  await out(Buffer.alloc(BLOCK * 2, 0));
}
