/**
 * chat-images.js — pictures Claude puts in front of the person, in the chat.
 *
 * A chat's transcript is text: a tool result that carries an image reaches the
 * page as the word "[image]". So when Claude has something worth looking at (a
 * screenshot of the page it just fixed, a chart, a before-and-after) it calls
 * show_image, and the chat draws the pictures under that call.
 *
 * The bytes are COPIED here, per chat, when the call is made. The transcript
 * outlives the turn; a screenshot in /tmp does not, and a path that still
 * exists may by then be a different picture.
 *
 * Only raster images identified by their own bytes (PNG, JPEG, GIF, WebP). No
 * SVG: an SVG is a document that can carry script, and these are served from
 * Crundi's own origin.
 */

import { join } from 'node:path';
import { mkdirSync, writeFileSync, readFileSync, readdirSync, statSync, rmSync, existsSync } from 'node:fs';
import { randomBytes } from 'node:crypto';
import { config } from './config.js';
import { decodeImage } from './image-input.js';

export const MAX_PER_CALL = 8;
const MAX_PER_CHAT = 120;                 // oldest dropped beyond this
const MAX_AGE_MS = 45 * 24 * 3600 * 1000; // a chat untouched this long loses its pictures

const MIME = { png: 'image/png', jpg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp' };
const validSession = (id) => /^[0-9a-zA-Z_-]{6,64}$/.test(String(id || ''));
const validName = (n) => /^[0-9a-f]{16}\.(png|jpg|gif|webp)$/.test(String(n || ''));

const root = () => join(config.dataDir, 'chat-images');
const dirFor = (sessionId) => join(root(), String(sessionId));

/**
 * Copy images into a chat's store.
 * @param {string} sessionId
 * @param {Array<{path?:string, data?:string, caption?:string}>} items
 * @returns {{ok:true, images:Array<{name:string, caption:string, bytes:number}>}|{ok:false, error:string}}
 */
export function addImages(sessionId, items) {
  if (!validSession(sessionId)) return { ok: false, error: 'This is not a Crundi chat, so there is nowhere to show an image. Use send_photo_to_user to reach the person on Telegram.' };
  const list = (Array.isArray(items) ? items : []).filter(Boolean);
  if (!list.length) return { ok: false, error: 'Give at least one image: path (a file on this machine) or paths (several).' };
  if (list.length > MAX_PER_CALL) return { ok: false, error: `At most ${MAX_PER_CALL} images in one call; you gave ${list.length}. Split them, or pick the ones that matter.` };
  // All or nothing: decode every one before writing any, so a typo in the
  // third path does not leave two orphans on screen.
  const decoded = [];
  for (const [i, it] of list.entries()) {
    const img = decodeImage({ path: it.path, data: it.data });
    if (!img.ok) return { ok: false, error: `Image ${i + 1}${it.path ? ` (${it.path})` : ''}: ${String(img.error).replace("Telegram's limit for photos is 10 MB", 'the limit is 10 MB')}` };
    decoded.push({ img, caption: String(it.caption || '').replace(/\s+/g, ' ').trim().slice(0, 200) });
  }
  const dir = dirFor(sessionId);
  mkdirSync(dir, { recursive: true });
  const images = [];
  for (const d of decoded) {
    const name = randomBytes(8).toString('hex') + '.' + d.img.ext;
    writeFileSync(join(dir, name), d.img.buffer);
    images.push({ name, caption: d.caption, bytes: d.img.buffer.length });
  }
  prune(dir);
  return { ok: true, images };
}

function prune(dir) {
  try {
    const files = readdirSync(dir).filter(validName)
      .map((n) => ({ n, t: statSync(join(dir, n)).mtimeMs })).sort((a, b) => a.t - b.t);
    for (const f of files.slice(0, Math.max(0, files.length - MAX_PER_CHAT))) rmSync(join(dir, f.n), { force: true });
  } catch { /* tidy next time */ }
}

/** One stored image, or null. Both names are checked: nothing is joined blind. */
export function readImage(sessionId, name) {
  if (!validSession(sessionId) || !validName(name)) return null;
  const file = join(dirFor(sessionId), name);
  if (!existsSync(file)) return null;
  try { return { buffer: readFileSync(file), mime: MIME[name.split('.').pop()] }; } catch { return null; }
}

/** Drop the pictures of chats nobody has touched in a long while. */
export function sweep(now = Date.now()) {
  let removed = 0;
  try {
    for (const id of readdirSync(root())) {
      const dir = join(root(), id);
      let newest = 0;
      try { for (const n of readdirSync(dir)) newest = Math.max(newest, statSync(join(dir, n)).mtimeMs); } catch { continue; }
      if (now - newest > MAX_AGE_MS) { rmSync(dir, { recursive: true, force: true }); removed++; }
    }
  } catch { /* no store yet */ }
  return removed;
}
