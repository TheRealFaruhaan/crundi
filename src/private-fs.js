/**
 * private-fs.js — Crundi's data folder is for its owner only.
 *
 * The data folder holds secrets.json, .api-key, the login sessions and the
 * rest of the server's state. A copy of secrets.json can be attacked offline,
 * and .api-key is a bearer credential for the loopback API, so nobody but the
 * user running Crundi may read any of it: folders 0700, files 0600.
 *
 * Three things here:
 *   ensurePrivateDir   create a folder (and any missing parents) 0700
 *   writePrivateFile   write a file 0600, atomically (tmp + rename) by default
 *   tightenPrivateTree at start, take group and other access off an existing
 *                      folder and what is in it (bounded, never follows links)
 *
 * Tightening only ever REMOVES the group and other bits (mode & ~0o077): the
 * owner's own bits, executables included, are kept, so a Chrome binary or a
 * script in the folder still runs.
 *
 * On Windows the POSIX modes mean nothing (the profile folder is already
 * private to its user), so the chmods are skipped there.
 */

import { mkdirSync, writeFileSync, renameSync, chmodSync, lstatSync, readdirSync, existsSync, unlinkSync } from 'node:fs';
import { dirname, join } from 'node:path';

export const PRIVATE_DIR_MODE = 0o700;
export const PRIVATE_FILE_MODE = 0o600;

const POSIX = process.platform !== 'win32';

/** Does this mode give anything to group or other? */
export const isOpenMode = (mode) => (mode & 0o077) !== 0;

function tighten(path, st) {
  if (!POSIX) return false;
  const s = st || lstatSync(path);
  if (s.isSymbolicLink() || !isOpenMode(s.mode)) return false;
  chmodSync(path, s.mode & 0o7700 & ~0o077);
  return true;
}

/** Create `dir` (and missing parents) owner-only; tighten it if it was open. */
export function ensurePrivateDir(dir) {
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true, mode: PRIVATE_DIR_MODE });
  try { tighten(dir); } catch { /* not ours to change: leave it */ }
  return dir;
}

/**
 * Write `data` to `file` with mode 0600. Atomic by default: a temporary file
 * beside it, made 0600 from the start, then renamed over it, so a crash leaves
 * the old or the new file and never half of one, and the new file is never
 * readable to anyone else, not even for a moment.
 */
export function writePrivateFile(file, data, { atomic = true, encoding } = {}) {
  // A missing folder is made owner-only; an existing one is left as it is
  // (it may be a checkout, for the dev fallback .env).
  const dir = dirname(file);
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true, mode: PRIVATE_DIR_MODE });
  const opts = { mode: PRIVATE_FILE_MODE, ...(encoding ? { encoding } : {}) };
  if (!atomic) {
    writeFileSync(file, data, opts);
    try { tighten(file); } catch { /* best effort */ }
    return;
  }
  const tmp = `${file}.${process.pid}.${Date.now().toString(36)}.tmp`;
  try {
    writeFileSync(tmp, data, { ...opts, flag: 'wx' });
    renameSync(tmp, file);
  } catch (err) {
    try { unlinkSync(tmp); } catch { /* never made */ }
    throw err;
  }
}

// Folders that can be huge and hold nothing of the server's own; the folder
// itself is still tightened (0700 stops anyone reaching what is inside).
const SKIP_INSIDE = new Set(['node_modules', '.git']);

/**
 * Take group and other access off `root` and everything under it, to `depth`
 * levels and at most `maxEntries` entries. Never follows a symbolic link.
 * @returns {{ changed: number, seen: number, truncated: boolean }}
 */
export function tightenPrivateTree(root, { depth = 8, maxEntries = 20000 } = {}) {
  const out = { changed: 0, seen: 0, truncated: false };
  if (!POSIX || !existsSync(root)) return out;
  const walk = (dir, level) => {
    let st;
    try { st = lstatSync(dir); } catch { return; }
    if (st.isSymbolicLink() || !st.isDirectory()) return;
    try { if (tighten(dir, st)) out.changed++; } catch { /* skip */ }
    if (level >= depth) return;
    let names = [];
    try { names = readdirSync(dir); } catch { return; }
    for (const name of names) {
      if (out.seen >= maxEntries) { out.truncated = true; return; }
      out.seen++;
      const p = join(dir, name);
      let s;
      try { s = lstatSync(p); } catch { continue; }
      if (s.isSymbolicLink()) continue;
      if (s.isDirectory()) {
        if (SKIP_INSIDE.has(name)) { try { if (tighten(p, s)) out.changed++; } catch { /* skip */ } continue; }
        walk(p, level + 1);
      } else {
        try { if (tighten(p, s)) out.changed++; } catch { /* skip */ }
      }
    }
  };
  walk(root, 0);
  return out;
}

/**
 * At start: the config folder (the .env and its backups) and the data folder,
 * owner-only. The config folder is done one level deep only: the data folder
 * inside it is walked on its own, and anything else there is not ours.
 */
export function securePrivateFolders({ appDir, dataDir } = {}) {
  const res = { changed: 0, truncated: false };
  if (appDir && existsSync(appDir)) {
    const r = tightenPrivateTree(appDir, { depth: 0 });
    // depth 0 tightens the folder itself; its top-level files too:
    try {
      for (const name of readdirSync(appDir)) {
        const p = join(appDir, name);
        const s = lstatSync(p);
        if ((s.isFile() || s.isDirectory()) && tighten(p, s)) r.changed++;
      }
    } catch { /* leave it */ }
    res.changed += r.changed;
  }
  if (dataDir) {
    ensurePrivateDir(dataDir);
    const r = tightenPrivateTree(dataDir);
    res.changed += r.changed;
    res.truncated = r.truncated;
  }
  return res;
}
