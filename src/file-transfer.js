// Moving and copying files and folders for the Files tab.
//
// The caller has already decided the paths are ones this person may touch
// (see the route in webapp.js). What is decided here is what is safe to do to
// a disk: nothing is ever overwritten, a folder never goes inside itself, and
// one bad item does not leave the others half done without saying so.

import { existsSync, statSync, lstatSync, renameSync, cpSync, rmSync } from 'node:fs';
import { resolve, dirname, basename, extname, join, sep } from 'node:path';

/** A name that is free in `dir`: "a.txt", then "a (2).txt", "a (3).txt"... */
export function freeName(dir, name) {
  if (!existsSync(join(dir, name))) return name;
  // A dotfile (".env") is all name and no extension.
  const ext = name.startsWith('.') && name.indexOf('.', 1) < 0 ? '' : extname(name);
  const stem = ext ? name.slice(0, -ext.length) : name;
  for (let n = 2; n < 10000; n++) {
    const cand = `${stem} (${n})${ext}`;
    if (!existsSync(join(dir, cand))) return cand;
  }
  throw new Error(`Too many copies of ${name} in that folder`);
}

const isDir = (p) => { try { return statSync(p).isDirectory(); } catch { return false; } };

/**
 * The folder to show when the one asked for is gone (Claude deleted it, a
 * branch switch removed it): the project's folder, and if even that is
 * missing, the nearest folder above it that exists.
 *
 * `confined` is a collaborator held to their own worktree. For them the walk
 * upwards does not happen at all: their root or nothing, so a missing
 * worktree can never turn into a look at the folders above it.
 *
 * @returns {string|null} the folder to list, or null if there is none to offer
 */
export function listableDir(wanted, root, confined) {
  const want = resolve(String(wanted || root || ''));
  if (isDir(want)) return want;
  let cur = resolve(String(root || ''));
  if (isDir(cur)) return cur;
  if (confined) return null;
  for (let i = 0; i < 64; i++) {
    const up = dirname(cur);
    if (up === cur) break;
    cur = up;
    if (isDir(cur)) return cur;
  }
  return null;
}

const inside = (child, parent) => child === parent || child.startsWith(parent + sep);

/**
 * @param {{sources:string[], destDir:string, op:'copy'|'move', protect?:string[]}} o
 *   protect: paths that may not be moved (the project's root).
 * @returns {{ok:boolean, done:Array<{from:string,to:string,name:string,renamed:boolean}>, failed:Array<{from:string,error:string}>, error?:string}}
 */
export function transfer({ sources, destDir, op, protect = [] }) {
  const done = [], failed = [];
  if (op !== 'copy' && op !== 'move') return { ok: false, done, failed, error: 'Unknown operation' };
  const dest = resolve(String(destDir || ''));
  let dst; try { dst = statSync(dest); } catch { dst = null; }
  if (!dst || !dst.isDirectory()) return { ok: false, done, failed, error: 'The destination is not a folder' };
  const list = [...new Set((Array.isArray(sources) ? sources : []).map((s) => resolve(String(s || ''))))].filter(Boolean);
  if (!list.length) return { ok: false, done, failed, error: 'Nothing to ' + op };
  const guarded = protect.map((p) => resolve(p));

  for (const src of list) {
    const fail = (error) => failed.push({ from: src, error });
    let st; try { st = lstatSync(src); } catch { st = null; }
    if (!st) { fail('No longer there'); continue; }
    if (op === 'move' && guarded.includes(src)) { fail('This folder cannot be moved'); continue; }
    if (st.isDirectory() && inside(dest, src)) { fail(`A folder cannot be ${op === 'move' ? 'moved' : 'copied'} into itself`); continue; }
    if (op === 'move' && dirname(src) === dest) { fail('Already in that folder'); continue; }
    try {
      const name = freeName(dest, basename(src));
      const to = join(dest, name);
      if (op === 'copy') {
        cpSync(src, to, { recursive: true, errorOnExist: true, force: false, verbatimSymlinks: true });
      } else {
        try { renameSync(src, to); }
        catch (err) {
          // Another disk: a rename cannot cross it, so copy and then remove.
          if (err.code !== 'EXDEV') throw err;
          cpSync(src, to, { recursive: true, errorOnExist: true, force: false, verbatimSymlinks: true });
          rmSync(src, { recursive: true, force: true });
        }
      }
      done.push({ from: src, to, name, renamed: name !== basename(src) });
    } catch (err) { fail(String(err.message || err)); }
  }
  return { ok: done.length > 0, done, failed, ...(done.length ? {} : { error: failed[0] ? failed[0].error : 'Nothing was done' }) };
}
