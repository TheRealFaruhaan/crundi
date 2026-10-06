/**
 * skills-sync.js — keep the skills Crundi ships up to date in the user's
 * Claude config, and keep its hands off every other skill there.
 *
 * ~/.claude/skills is shared ground: Crundi's own skills sit next to ones the
 * user uploaded, wrote by hand, or got from somewhere else. The rules:
 *
 *   An included skill (a folder under this build's skills/) is installed if it
 *   is missing, replaced in place if its content differs from the shipped copy,
 *   and left untouched if it is identical. It is always the same folder, so an
 *   update can never leave a second copy of the same skill beside the first.
 *
 *   A skill Crundi installed before and no longer ships is removed, so a
 *   renamed skill does not live on under its old name next to the new one.
 *
 *   Nothing else is read, written or removed. A skill is "Crundi's" only if it
 *   carries the marker file this routine writes (or is the `crundi` skill, which
 *   was installed for several releases before the marker existed).
 *
 *   If the user has their own skill under a name Crundi now ships, the update
 *   still goes in — and theirs is moved out to a folder beside skills/, whole,
 *   rather than deleted or left to shadow the included one.
 *
 * One implementation for every way Crundi gets onto a machine: install.sh runs
 * it (scripts/sync-skills.mjs), and the server runs it at start, which is what
 * covers the container image, the Windows server package and the desktop app.
 */

import {
  existsSync, readFileSync, writeFileSync, mkdirSync, readdirSync, lstatSync, statSync,
  renameSync, rmSync, cpSync,
} from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { homedir } from 'node:os';
import { randomBytes } from 'node:crypto';

const __dirname = dirname(fileURLToPath(import.meta.url));

/** Must match BUNDLED_MARK in skills-store.js; kept here so this file loads nothing else. */
export const BUNDLED_MARK = '.crundi-bundled';

/** Installed without a marker by releases up to 1.19.12. Never add to this. */
const LEGACY = new Set(['crundi']);

/** Where this build keeps the skills it ships, or '' if it ships none. */
export function shippedDir() {
  const candidates = [
    join(__dirname, '..', 'skills'),
    // The desktop app: src/ is unpacked to resources/app.asar.unpacked/src and
    // the skills are an extra resource beside it, at resources/skills. The
    // server there is a plain Node child, so process.resourcesPath is not set.
    join(__dirname, '..', '..', 'skills'),
  ];
  if (process.resourcesPath) candidates.push(join(process.resourcesPath, 'skills'));
  return candidates.find(d => shippedSkills(d).length > 0) || '';
}

function shippedSkills(source) {
  try {
    return readdirSync(source).filter(n => !n.startsWith('.') && existsSync(join(source, n, 'SKILL.md'))).sort();
  } catch { return []; }
}

/** Every file under a folder as relative paths, sorted; the marker left out. */
function tree(dir) {
  const out = [];
  const visit = (abs, rel) => {
    for (const n of readdirSync(abs).sort()) {
      const p = join(abs, n);
      const r = rel ? rel + '/' + n : n;
      const st = lstatSync(p);
      if (st.isDirectory()) visit(p, r);
      else if (r !== BUNDLED_MARK) out.push(r);
    }
  };
  visit(dir, '');
  return out;
}

function sameContent(a, b) {
  let ta, tb;
  try { ta = tree(a); tb = tree(b); } catch { return false; }
  if (ta.length !== tb.length || ta.some((p, i) => p !== tb[i])) return false;
  for (const p of ta) {
    try { if (!readFileSync(join(a, p)).equals(readFileSync(join(b, p)))) return false; }
    catch { return false; }
  }
  return true;
}

function isOurs(dir, name) {
  return existsSync(join(dir, BUNDLED_MARK)) || LEGACY.has(name);
}

function mark(dir, version) {
  writeFileSync(join(dir, BUNDLED_MARK), JSON.stringify({
    by: 'crundi', version: version || '', note: 'Installed and replaced by Crundi. Copy this skill to change it.',
  }, null, 2) + '\n');
}

/**
 * Bring `target` (a ~/.claude/skills) in line with the skills in `source`.
 *
 * @param {{ source?: string, target?: string, aside?: string, version?: string }} [opts]
 * @returns {{ ok: boolean, installed: string[], updated: string[], current: string[],
 *             removed: string[], setAside: {name:string,to:string}[], skipped: {name:string,why:string}[] }}
 */
export function syncBundled(opts = {}) {
  const source = opts.source ?? shippedDir();
  const target = opts.target ?? join(homedir(), '.claude', 'skills');
  const aside = opts.aside ?? join(dirname(target), 'skills-replaced-by-crundi');
  const result = { ok: true, installed: [], updated: [], current: [], removed: [], setAside: [], skipped: [] };

  const names = source ? shippedSkills(source) : [];
  // A build that ships no skills (the client-only app) must not conclude that
  // every skill Crundi ever installed has been withdrawn.
  if (!names.length) return result;

  mkdirSync(target, { recursive: true });
  const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);

  for (const name of names) {
    const src = join(source, name);
    const dest = join(target, name);
    let st = null;
    try { st = lstatSync(dest); } catch { /* not installed */ }

    if (st && st.isSymbolicLink()) {
      // Somebody pointed this name at a folder of their own (a checkout, usually).
      result.skipped.push({ name, why: 'it is a link to another folder' });
      continue;
    }
    if (st && isOurs(dest, name) && sameContent(src, dest)) {
      if (!existsSync(join(dest, BUNDLED_MARK))) { try { mark(dest, opts.version); } catch { /* read-only home */ } }
      result.current.push(name);
      continue;
    }

    const staged = join(target, `.crundi-new-${randomBytes(6).toString('hex')}`);
    try {
      cpSync(src, staged, { recursive: true });
      mark(staged, opts.version);
      if (st) {
        if (isOurs(dest, name)) {
          // Removed whole, not copied over: a file dropped from the skill in
          // this release must not survive from the last one.
          rmSync(dest, { recursive: true, force: true });
          result.updated.push(name);
        } else {
          mkdirSync(aside, { recursive: true });
          const to = join(aside, `${name}-${stamp}`);
          renameSync(dest, to);
          result.setAside.push({ name, to });
          result.installed.push(name);
        }
      } else result.installed.push(name);
      renameSync(staged, dest);
    } catch (err) {
      rmSync(staged, { recursive: true, force: true });
      result.ok = false;
      result.skipped.push({ name, why: err.message });
    }
  }

  // Skills Crundi installed in an earlier release and no longer ships.
  const shipped = new Set(names);
  let present = [];
  try { present = readdirSync(target); } catch { /* nothing there */ }
  for (const name of present) {
    if (shipped.has(name) || name.startsWith('.')) continue;
    const dir = join(target, name);
    try {
      if (!statSync(dir).isDirectory() || lstatSync(dir).isSymbolicLink()) continue;
      if (!existsSync(join(dir, BUNDLED_MARK))) continue;
      rmSync(dir, { recursive: true, force: true });
      result.removed.push(name);
    } catch { /* leave it */ }
  }
  return result;
}

/** One line per thing that changed, for a log or an installer's output. */
export function describeSync(r, target) {
  const lines = [];
  for (const n of r.installed) lines.push(`Installed the '${n}' skill to ${join(target, n)}`);
  for (const n of r.updated) lines.push(`Updated the '${n}' skill in ${join(target, n)}`);
  for (const n of r.removed) lines.push(`Removed the '${n}' skill, which Crundi no longer ships`);
  for (const s of r.setAside) lines.push(`Your own '${s.name}' skill had the name of one Crundi now ships; it was moved to ${s.to}`);
  for (const s of r.skipped) lines.push(`Left the '${s.name}' skill alone: ${s.why}`);
  return lines;
}
