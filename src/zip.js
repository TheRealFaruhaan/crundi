/**
 * zip.js — read and write plain .zip archives with nothing but zlib.
 *
 * Skills travel as zips (a .skill file is one with a different extension), and
 * Crundi had no archive library. The format needed here is small: stored and
 * deflated entries, one disk, no encryption, no zip64. Anything else is refused
 * by name rather than half-read.
 *
 * Reading is defensive because the bytes come from an upload: sizes are capped
 * before anything is inflated, every entry's length and CRC are checked, and
 * names are returned exactly as written — deciding whether a name is safe to
 * put on disk is the caller's job (see skills-store.js), not this file's.
 */

import { inflateRawSync, deflateRawSync } from 'node:zlib';

const SIG_LOCAL = 0x04034b50;
const SIG_CENTRAL = 0x02014b50;
const SIG_END = 0x06054b50;

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = (c & 1) ? (0xedb88320 ^ (c >>> 1)) : (c >>> 1);
    t[n] = c >>> 0;
  }
  return t;
})();

export function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/** Does this buffer start like a zip? (An empty archive starts with the end record.) */
export function looksLikeZip(buf) {
  if (!buf || buf.length < 4) return false;
  const sig = buf.readUInt32LE(0);
  return sig === SIG_LOCAL || sig === SIG_END;
}

/**
 * List a zip's entries.
 *
 * Returns [{ name, isDir, isSymlink, mode, size, read() }]. `read()` inflates
 * that one entry and verifies it; nothing is decompressed until it is called.
 * Throws an Error with a plain-language message for anything unsupported or
 * over the limits.
 *
 * @param {Buffer} buf
 * @param {{ maxEntries?: number, maxTotal?: number, maxEntry?: number }} [limits]
 */
export function readZip(buf, limits = {}) {
  const maxEntries = limits.maxEntries ?? 5000;
  const maxTotal = limits.maxTotal ?? 100 * 1024 * 1024;
  const maxEntry = limits.maxEntry ?? 50 * 1024 * 1024;

  if (!Buffer.isBuffer(buf) || buf.length < 22) throw new Error('Not a zip archive');
  // The end record is the last thing in the file, followed only by a comment
  // of at most 65535 bytes. Search backwards for it.
  let end = -1;
  for (let i = buf.length - 22, stop = Math.max(0, buf.length - 22 - 0xffff); i >= stop; i--) {
    if (buf.readUInt32LE(i) === SIG_END) { end = i; break; }
  }
  if (end < 0) throw new Error('Not a zip archive');

  const count = buf.readUInt16LE(end + 10);
  const cdSize = buf.readUInt32LE(end + 12);
  const cdOffset = buf.readUInt32LE(end + 16);
  if (buf.readUInt16LE(end + 4) !== 0 || buf.readUInt16LE(end + 6) !== 0) throw new Error('Multi-part zip archives are not supported');
  if (count === 0xffff || cdSize === 0xffffffff || cdOffset === 0xffffffff) throw new Error('This zip is too large (zip64 is not supported)');
  if (count > maxEntries) throw new Error(`Too many files in the archive (${count}; the limit is ${maxEntries})`);
  if (cdOffset + cdSize > end) throw new Error('The zip archive is damaged');

  const entries = [];
  let total = 0;
  let p = cdOffset;
  for (let i = 0; i < count; i++) {
    if (p + 46 > buf.length || buf.readUInt32LE(p) !== SIG_CENTRAL) throw new Error('The zip archive is damaged');
    const madeBy = buf.readUInt16LE(p + 4);
    const flags = buf.readUInt16LE(p + 8);
    const method = buf.readUInt16LE(p + 10);
    const crc = buf.readUInt32LE(p + 16);
    const csize = buf.readUInt32LE(p + 20);
    const usize = buf.readUInt32LE(p + 24);
    const nameLen = buf.readUInt16LE(p + 28);
    const extraLen = buf.readUInt16LE(p + 30);
    const commentLen = buf.readUInt16LE(p + 32);
    const extAttr = buf.readUInt32LE(p + 38);
    const localOffset = buf.readUInt32LE(p + 42);
    const name = buf.subarray(p + 46, p + 46 + nameLen).toString('utf8');
    p += 46 + nameLen + extraLen + commentLen;

    if (flags & 0x1) throw new Error('Password-protected zip archives are not supported');
    if (csize === 0xffffffff || usize === 0xffffffff || localOffset === 0xffffffff) throw new Error('This zip is too large (zip64 is not supported)');
    if (method !== 0 && method !== 8) throw new Error(`"${name}" uses a compression method that is not supported`);
    if (usize > maxEntry) throw new Error(`"${name}" is too large (${usize} bytes)`);
    total += usize;
    if (total > maxTotal) throw new Error('The archive unpacks to more than the size limit');

    // Unix permissions, when the archive was made on a Unix (host 3).
    const mode = (madeBy >>> 8) === 3 ? (extAttr >>> 16) : 0;
    const isDir = name.endsWith('/') || (mode & 0o170000) === 0o040000;
    const isSymlink = (mode & 0o170000) === 0o120000;

    entries.push({
      name, isDir, isSymlink, mode: mode & 0o777, size: usize,
      read() {
        if (isDir) return Buffer.alloc(0);
        if (localOffset + 30 > buf.length || buf.readUInt32LE(localOffset) !== SIG_LOCAL) throw new Error('The zip archive is damaged');
        const start = localOffset + 30 + buf.readUInt16LE(localOffset + 26) + buf.readUInt16LE(localOffset + 28);
        if (start + csize > buf.length) throw new Error('The zip archive is damaged');
        const raw = buf.subarray(start, start + csize);
        let out;
        if (method === 0) out = Buffer.from(raw);
        else {
          // maxOutputLength is what stops a small entry inflating into gigabytes:
          // the declared size is only a claim until this has run.
          try { out = inflateRawSync(raw, { maxOutputLength: Math.max(usize, 1) }); }
          catch { throw new Error(`"${name}" could not be unpacked`); }
        }
        if (out.length !== usize || crc32(out) !== crc) throw new Error(`"${name}" is damaged`);
        return out;
      },
    });
  }
  return entries;
}

function dosDateTime(ms) {
  const d = new Date(ms || Date.now());
  const year = Math.min(Math.max(d.getFullYear(), 1980), 2107);
  return {
    time: (d.getHours() << 11) | (d.getMinutes() << 5) | (d.getSeconds() >> 1),
    date: ((year - 1980) << 9) | ((d.getMonth() + 1) << 5) | d.getDate(),
  };
}

/**
 * Build a zip from [{ name, data, mode?, mtime? }]. Names use forward slashes.
 * @returns {Buffer}
 */
export function writeZip(files) {
  const parts = [];
  const central = [];
  let offset = 0;
  for (const f of files) {
    const name = Buffer.from(String(f.name).replace(/\\/g, '/'), 'utf8');
    const data = Buffer.isBuffer(f.data) ? f.data : Buffer.from(f.data || '');
    const packed = deflateRawSync(data);
    // Already-compressed files (images, archives) only grow under deflate.
    const store = packed.length >= data.length;
    const body = store ? data : packed;
    const crc = crc32(data);
    const { time, date } = dosDateTime(f.mtime);
    const mode = (f.mode || 0o644) & 0o777;

    const local = Buffer.alloc(30);
    local.writeUInt32LE(SIG_LOCAL, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x0800, 6);                 // names are UTF-8
    local.writeUInt16LE(store ? 0 : 8, 8);
    local.writeUInt16LE(time, 10);
    local.writeUInt16LE(date, 12);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(body.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(name.length, 26);
    local.writeUInt16LE(0, 28);

    const cd = Buffer.alloc(46);
    cd.writeUInt32LE(SIG_CENTRAL, 0);
    cd.writeUInt16LE((3 << 8) | 20, 4);             // made on Unix, so the mode below means something
    cd.writeUInt16LE(20, 6);
    cd.writeUInt16LE(0x0800, 8);
    cd.writeUInt16LE(store ? 0 : 8, 10);
    cd.writeUInt16LE(time, 12);
    cd.writeUInt16LE(date, 14);
    cd.writeUInt32LE(crc, 16);
    cd.writeUInt32LE(body.length, 20);
    cd.writeUInt32LE(data.length, 24);
    cd.writeUInt16LE(name.length, 28);
    cd.writeUInt32LE(((0o100000 | mode) << 16) >>> 0, 38);
    cd.writeUInt32LE(offset, 42);

    parts.push(local, name, body);
    central.push(cd, name);
    offset += local.length + name.length + body.length;
  }
  const cdBuf = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(SIG_END, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(cdBuf.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...parts, cdBuf, end]);
}
