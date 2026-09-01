/**
 * image-input.js — accept an image as a file path or as the bytes themselves.
 *
 * Tools that produce images mostly produce BYTES: a browser screenshot comes
 * back as base64 over CDP, and a caller holding one had to write a temp file
 * purely to hand it to something that only spoke paths. Now either works.
 *
 * The bytes are sniffed rather than trusted. A caller can label anything
 * "image/png"; what decides is the first few bytes of the file, because that is
 * what Telegram will look at too, and "sendPhoto failed" from a mislabelled
 * blob is a much worse error message than "that is not an image".
 */

import { existsSync, statSync, readFileSync } from 'fs';

// Telegram rejects photos above 10 MB. Catching it here means an error that
// says what is wrong instead of an HTTP failure from the API.
export const MAX_PHOTO_BYTES = 10 * 1024 * 1024;

/** The image format these bytes actually are, by magic number, or ''. */
export function sniffImage(buf) {
  if (!buf || buf.length < 12) return '';
  if (buf[0] === 0x89 && buf[1] === 0x50 && buf[2] === 0x4e && buf[3] === 0x47) return 'png';
  if (buf[0] === 0xff && buf[1] === 0xd8 && buf[2] === 0xff) return 'jpg';
  if (buf[0] === 0x47 && buf[1] === 0x49 && buf[2] === 0x46 && buf[3] === 0x38) return 'gif';
  if (buf.subarray(0, 4).toString('latin1') === 'RIFF' && buf.subarray(8, 12).toString('latin1') === 'WEBP') return 'webp';
  return '';
}

/**
 * Turn { path } or { data } into bytes ready to send.
 *
 * `data` takes base64, with or without a data: URL wrapper, since callers
 * holding a screenshot have it in one form or the other and should not have to
 * care which.
 *
 * @returns {{ok:true, buffer:Buffer, ext:string, filename:string, source:string}|{ok:false, error:string}}
 */
export function decodeImage({ path = '', data = '' } = {}) {
  const p = String(path || '').trim();
  const d = String(data || '').trim();
  if (!p && !d) return { ok: false, error: 'Give either a file path or image data.' };
  if (p && d) return { ok: false, error: 'Give a file path or image data, not both.' };

  let buffer;
  let source;
  if (p) {
    if (!existsSync(p)) return { ok: false, error: `No such file: ${p}` };
    let st;
    try { st = statSync(p); } catch (err) { return { ok: false, error: String(err.message || err) }; }
    if (st.isDirectory()) return { ok: false, error: `${p} is a directory, not an image.` };
    if (st.size === 0) return { ok: false, error: `${p} is empty.` };
    if (st.size > MAX_PHOTO_BYTES) {
      return { ok: false, error: `That image is ${(st.size / 1048576).toFixed(1)} MB; Telegram's limit for photos is 10 MB.` };
    }
    try { buffer = readFileSync(p); } catch (err) { return { ok: false, error: String(err.message || err) }; }
    source = 'path';
  } else {
    // Tolerate a data: URL, raw base64, and whitespace/newlines from wrapping.
    const cleaned = d.replace(/^data:[^,]*,/, '').replace(/\s+/g, '');
    if (!cleaned) return { ok: false, error: 'The image data is empty.' };
    if (!/^[A-Za-z0-9+/=_-]+$/.test(cleaned)) return { ok: false, error: 'The image data is not base64.' };
    try {
      // base64url shows up often enough to be worth accepting silently.
      buffer = Buffer.from(cleaned.replace(/-/g, '+').replace(/_/g, '/'), 'base64');
    } catch (err) {
      return { ok: false, error: 'Could not decode the image data: ' + String(err.message || err) };
    }
    if (!buffer.length) return { ok: false, error: 'The image data decoded to nothing.' };
    if (buffer.length > MAX_PHOTO_BYTES) {
      return { ok: false, error: `That image is ${(buffer.length / 1048576).toFixed(1)} MB; Telegram's limit for photos is 10 MB.` };
    }
    source = 'data';
  }

  const ext = sniffImage(buffer);
  if (!ext) {
    return {
      ok: false,
      error: source === 'path'
        ? 'That file is not a PNG, JPEG, GIF or WebP.'
        : 'Those bytes are not a PNG, JPEG, GIF or WebP. If you passed base64 of something else, that is the problem.',
    };
  }
  return { ok: true, buffer, ext, filename: `image.${ext}`, source };
}
