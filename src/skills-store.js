/**
 * skills-store.js — the Claude skills on this machine, as files.
 *
 * A skill is a folder with a SKILL.md in it. Claude Code reads them from two
 * places and this module manages exactly those, nothing of its own:
 *
 *   ~/.claude/skills/<name>/              every project on this machine  ("global")
 *   <project>/.claude/skills/<name>/      that project only              ("project:<alias>")
 *
 * There is no database and no index. What is on disk is the truth, so a skill
 * dropped in by hand, by git, or by another tool shows up here like any other,
 * and nothing here can drift from what Claude actually loads.
 *
 * Three kinds are never modified:
 *
 *   bundled  the skills Crundi itself ships (skills/ in this repo). The
 *            installer replaces them on every install, so an edit made here
 *            would be silently lost on the next update. They can be read,
 *            downloaded and copied — the copy is yours.
 *   synced   ~/.claude/skills/synced/…, which Claude Code downloads from the
 *            signed-in account and rewrites itself.
 *   linked   a skill folder that is a symlink. Following it would edit
 *            whatever it points at, which may be anywhere.
 *
 * Uploads are untrusted bytes. Every name that reaches the disk goes through
 * safeRel()/checkName() first, archives are unpacked into a staging folder next
 * to the destination and renamed into place, and symlinks inside an archive are
 * dropped rather than created.
 */

import {
  existsSync, readFileSync, writeFileSync, mkdirSync, readdirSync, lstatSync, statSync,
  renameSync, rmSync, cpSync, chmodSync, realpathSync,
} from 'node:fs';
import { join, dirname, resolve, relative, sep, basename, extname } from 'node:path';
import { homedir } from 'node:os';
import { randomBytes } from 'node:crypto';
import { readZip, writeZip, looksLikeZip } from './zip.js';
import { shippedDir } from './skills-sync.js';

/** Written by the installers into every skill they place, and only by them. */
export const BUNDLED_MARK = '.crundi-bundled';

export const LIMITS = {
  upload: 50 * 1024 * 1024,        // the archive as uploaded
  unpacked: 100 * 1024 * 1024,     // everything in it, unpacked
  files: 2000,                     // files in one skill
  editable: 1024 * 1024,           // largest file the editor will open or save
  file: 25 * 1024 * 1024,          // largest single file added to a skill
};

// ─── Where things are ───

let resolveProject = () => null;
let listProjects = () => [];
let globalOverride = '';
let bundledOverride = null;

/**
 * Wire in the project lookup. `getProject(alias)` returns { alias, path, name }
 * or null; `projects()` returns all of them. Tests also pass `globalDir` and
 * `bundled` so nothing touches the real home directory.
 */
export function configure(opts = {}) {
  if (opts.getProject) resolveProject = opts.getProject;
  if (opts.projects) listProjects = opts.projects;
  if (opts.globalDir !== undefined) globalOverride = opts.globalDir;
  if (opts.bundled !== undefined) bundledOverride = opts.bundled ? new Set(opts.bundled) : null;
}

export function globalDir() {
  return globalOverride || join(homedir(), '.claude', 'skills');
}

/** Names of the skills this build of Crundi ships. `crundi` always counts. */
export function bundledNames() {
  if (bundledOverride) return bundledOverride;
  const out = new Set(['crundi']);
  // The one folder this build actually installs from; see shippedDir().
  const dir = shippedDir();
  try {
    for (const n of dir ? readdirSync(dir) : []) {
      if (!n.startsWith('.') && existsSync(join(dir, n, 'SKILL.md'))) out.add(n);
    }
  } catch { /* this build ships none */ }
  return out;
}

/**
 * 'global' | 'synced' | 'project:<alias>'; a bare project alias is accepted
 * too. Nothing at all is '' — never a quiet default to "every project", which
 * is the widest place a skill can go.
 */
export function normScope(scope) {
  const s = String(scope ?? '').trim();
  if (!s) return '';
  if (s === 'global' || s === 'synced') return s;
  return 'project:' + s.replace(/^project:/, '').toLowerCase();
}

function scopeRoot(scope) {
  const s = normScope(scope);
  if (!s) return { ok: false, error: 'Say where the skill is: every project, or one of them' };
  if (s === 'global') return { ok: true, scope: s, root: globalDir() };
  if (s === 'synced') return { ok: true, scope: s, root: join(globalDir(), 'synced') };
  const project = resolveProject(s.slice('project:'.length));
  if (!project || !project.path) return { ok: false, error: 'Project not found' };
  return { ok: true, scope: 'project:' + String(project.alias || s.slice(8)).toLowerCase(), root: join(project.path, '.claude', 'skills'), project };
}

// ─── Names and paths ───

const NAME_RE = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
// Folder names Claude Code or Crundi use for something else inside a skills dir.
const RESERVED = new Set(['synced']);

/** A name for a new skill: what Claude accepts, and safe as a folder everywhere. */
export function checkName(name) {
  const n = String(name || '');
  if (!n) return 'Give the skill a name';
  if (n.length > 64) return 'A skill name can be at most 64 characters';
  if (!NAME_RE.test(n)) return 'Use lowercase letters, digits and hyphens for a skill name';
  if (RESERVED.has(n)) return `"${n}" is reserved`;
  return '';
}

/** Turn any text into a usable skill name, or '' if nothing is left. */
export function slug(text) {
  return String(text || '').toLowerCase().normalize('NFKD').replace(/[^a-z0-9._-]+/g, '-')
    .replace(/[._-]{2,}/g, '-').replace(/^[._-]+|[._-]+$/g, '').slice(0, 64).replace(/[._-]+$/g, '');
}

/** An existing skill's folder name: anything that is one path segment. */
function okSegment(n) {
  const s = String(n || '');
  return !!s && s !== '.' && s !== '..' && !/[\\/\0]/.test(s) && s.length <= 255;
}

/**
 * A path inside a skill, normalised to forward slashes; null if it could leave
 * the skill or cannot exist on every platform Crundi runs on.
 */
export function safeRel(p) {
  const s = String(p ?? '').replace(/\\/g, '/');
  if (!s || s.includes('\0') || s.startsWith('/') || /^[a-zA-Z]:/.test(s) || s.length > 1024) return null;
  const parts = s.split('/').filter((x, i, a) => !(x === '' && i === a.length - 1));
  if (!parts.length) return null;
  for (const part of parts) {
    if (!part || part === '.' || part === '..') return null;
    if (/[\x00-\x1f<>:"|?*]/.test(part) || /[ .]$/.test(part) || part.length > 255) return null;
  }
  return parts.join('/');
}

function inside(root, target) {
  const r = relative(root, target);
  return r === '' || (!r.startsWith('..') && !r.includes('..' + sep) && !/^[a-zA-Z]:/.test(r) && !r.startsWith(sep));
}

// ─── Reading a skill ───

/**
 * The YAML front matter of a SKILL.md, as far as Claude's own fields need:
 * `key: value`, quoted values, and `>` / `|` blocks. Not a YAML parser.
 */
export function parseFrontmatter(text) {
  const out = { name: '', description: '', has: false };
  const src = String(text || '').replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
  if (!src.startsWith('---\n')) return out;
  const end = src.indexOf('\n---', 3);
  if (end < 0) return out;
  out.has = true;
  const lines = src.slice(4, end).split('\n');
  for (let i = 0; i < lines.length; i++) {
    const m = /^([A-Za-z_][\w-]*):\s*(.*)$/.exec(lines[i]);
    if (!m) continue;
    let value = m[2].trim();
    if (/^[>|][+-]?$/.test(value)) {
      const folded = value[0] === '>';
      const block = [];
      while (i + 1 < lines.length && (/^\s+/.test(lines[i + 1]) || lines[i + 1] === '')) block.push(lines[++i].trim());
      value = block.join(folded ? ' ' : '\n').trim();
    } else if (value === '') {
      // A plain value continued on indented lines.
      const block = [];
      while (i + 1 < lines.length && /^\s+\S/.test(lines[i + 1])) block.push(lines[++i].trim());
      value = block.join(' ');
    } else if ((value.startsWith('"') && value.endsWith('"') && value.length > 1)
      || (value.startsWith("'") && value.endsWith("'") && value.length > 1)) {
      const q = value[0];
      value = value.slice(1, -1);
      value = q === '"' ? value.replace(/\\(["\\n])/g, (_, c) => (c === 'n' ? '\n' : c)) : value.replace(/''/g, "'");
    }
    const key = m[1].toLowerCase();
    if (key === 'name' || key === 'description') out[key] = value;
  }
  return out;
}

/**
 * Every regular file under a skill, as { path, size, mtime, mode }. Symlinks
 * are skipped. Stops at `limit` and says so in `more`: a skill folder somebody
 * made by hand can hold anything, a virtualenv included.
 */
function walk(dir, limit = LIMITS.files) {
  const files = [];
  let links = 0;
  let more = false;
  const visit = (abs, rel) => {
    let names;
    try { names = readdirSync(abs); } catch { return; }
    for (const n of names.sort()) {
      if (more) return;
      if (files.length >= limit) { more = true; return; }
      const p = join(abs, n);
      const r = rel ? rel + '/' + n : n;
      let st;
      try { st = lstatSync(p); } catch { continue; }
      if (st.isSymbolicLink()) { links++; continue; }
      if (st.isDirectory()) { visit(p, r); continue; }
      if (!st.isFile()) continue;
      if (r === BUNDLED_MARK) continue;
      files.push({ path: r, size: st.size, mtime: st.mtimeMs, mode: st.mode & 0o777 });
    }
  };
  visit(dir, '');
  return { files, links, more };
}

/** Where a synced skill lives: synced/<bucket>/<name>. The first bucket that has it. */
function syncedDir(root, name) {
  try {
    for (const bucket of readdirSync(root).sort()) {
      const d = join(root, bucket, name);
      if (existsSync(join(d, 'SKILL.md'))) return d;
    }
  } catch { /* nothing synced */ }
  return '';
}

function describe(scope, name, dir) {
  let st;
  try { st = lstatSync(dir); } catch { return null; }
  const linked = st.isSymbolicLink();
  let isDir = st.isDirectory();
  if (linked) { try { isDir = statSync(dir).isDirectory(); } catch { isDir = false; } }
  if (!isDir) return null;
  const md = join(dir, 'SKILL.md');
  const hasMd = existsSync(md);
  let fm = { name: '', description: '', has: false };
  if (hasMd) { try { fm = parseFrontmatter(readFileSync(md, 'utf8').slice(0, 64 * 1024)); } catch { /* unreadable */ } }
  const { files, more } = walk(dir);
  const bundled = scope === 'global' && (bundledNames().has(name) || existsSync(join(dir, BUNDLED_MARK)));
  const origin = scope === 'synced' ? 'synced' : bundled ? 'bundled' : linked ? 'linked' : 'user';
  let problem = '';
  if (!hasMd) problem = 'No SKILL.md, so Claude will not load it';
  else if (!fm.has) problem = 'SKILL.md has no front matter (name and description)';
  else if (!fm.description) problem = 'SKILL.md has no description, so Claude cannot tell when to use it';
  return {
    scope, name,
    title: fm.name || name,
    description: fm.description || '',
    files: files.length,
    // True when the folder holds more than was counted; `files` and `size` are
    // then a floor, not a total.
    more,
    size: files.reduce((n, f) => n + f.size, 0),
    modifiedAt: files.reduce((n, f) => Math.max(n, f.mtime), 0) || st.mtimeMs,
    origin, readOnly: origin !== 'user', problem,
  };
}

function skillsIn(scope, root) {
  const out = [];
  let names;
  try { names = readdirSync(root); } catch { return out; }
  for (const n of names.sort()) {
    if (n.startsWith('.') || RESERVED.has(n)) continue;
    const d = describe(scope, n, join(root, n));
    if (d) out.push(d);
  }
  return out;
}

/** Find one skill. Returns { ok, scope, name, dir, info } or { ok:false, error }. */
function locate(scope, name) {
  const sr = scopeRoot(scope);
  if (!sr.ok) return sr;
  if (!okSegment(name) || String(name).startsWith('.')) return { ok: false, error: 'Skill not found' };
  const dir = sr.scope === 'synced' ? syncedDir(sr.root, name) : join(sr.root, name);
  if (!dir || !inside(sr.root, dir)) return { ok: false, error: 'Skill not found' };
  const info = describe(sr.scope, name, dir);
  if (!info) return { ok: false, error: 'Skill not found' };
  return { ok: true, scope: sr.scope, root: sr.root, name, dir, info };
}

const WHY_READ_ONLY = {
  bundled: 'This skill ships with Crundi and is replaced on every update. Copy it to change it.',
  synced: 'This skill is synced from your Claude account by Claude Code. Copy it to change it.',
  linked: 'This skill is a link to a folder somewhere else. Edit it there, or copy it.',
};

function writable(found) {
  if (!found.info.readOnly) return null;
  return { ok: false, error: WHY_READ_ONLY[found.info.origin] || 'This skill cannot be changed', readOnly: true };
}

// ─── Listing ───

/** The places a skill can live, for the scope pickers. */
export function scopes() {
  const out = [{ id: 'global', label: 'All projects', path: globalDir() }];
  for (const p of listProjects()) {
    if (!p || !p.path) continue;
    out.push({ id: 'project:' + String(p.alias).toLowerCase(), label: p.name || p.alias, path: join(p.path, '.claude', 'skills') });
  }
  return out;
}

export function list() {
  const skills = [];
  skills.push(...skillsIn('global', globalDir()));
  const syncedRoot = join(globalDir(), 'synced');
  const seen = new Set();
  try {
    for (const bucket of readdirSync(syncedRoot).sort()) {
      for (const s of skillsIn('synced', join(syncedRoot, bucket))) {
        if (seen.has(s.name)) continue;
        seen.add(s.name);
        skills.push(s);
      }
    }
  } catch { /* nothing synced */ }
  for (const p of listProjects()) {
    if (!p || !p.path) continue;
    skills.push(...skillsIn('project:' + String(p.alias).toLowerCase(), join(p.path, '.claude', 'skills')));
  }
  return { ok: true, skills, scopes: scopes() };
}

export function get(scope, name) {
  const f = locate(scope, name);
  if (!f.ok) return f;
  const { files, links, more } = walk(f.dir);
  return {
    ok: true, skill: f.info, path: f.dir,
    files: files.map(x => ({ path: x.path, size: x.size, mtime: x.mtime })),
    more, skippedLinks: links,
    readOnlyReason: f.info.readOnly ? (WHY_READ_ONLY[f.info.origin] || '') : '',
  };
}

// ─── Files inside a skill ───

function fileTarget(found, rel) {
  const r = safeRel(rel);
  if (!r || r === BUNDLED_MARK) return { ok: false, error: 'Not a valid file path' };
  const abs = resolve(found.dir, r);
  if (!inside(found.dir, abs) || abs === resolve(found.dir)) return { ok: false, error: 'Not a valid file path' };
  return { ok: true, rel: r, abs };
}

/** True when every existing folder on the way to `abs` really is inside the skill. */
function noLinkEscape(found, abs) {
  let base;
  try { base = realpathSync(found.dir); } catch { return false; }
  let p = dirname(abs);
  while (!existsSync(p)) { const up = dirname(p); if (up === p) return false; p = up; }
  let real;
  try { real = realpathSync(p); } catch { return false; }
  if (!inside(base, real)) return false;
  try { if (lstatSync(abs).isSymbolicLink()) return false; } catch { /* does not exist yet */ }
  return true;
}

export function readFile(scope, name, rel) {
  const f = locate(scope, name);
  if (!f.ok) return f;
  const t = fileTarget(f, rel);
  if (!t.ok) return t;
  if (!noLinkEscape(f, t.abs)) return { ok: false, error: 'File not found' };
  let st;
  try { st = statSync(t.abs); } catch { return { ok: false, error: 'File not found' }; }
  if (!st.isFile()) return { ok: false, error: 'File not found' };
  const base = { ok: true, path: t.rel, size: st.size, readOnly: f.info.readOnly };
  if (st.size > LIMITS.editable) return { ...base, binary: true, tooLarge: true };
  const buf = readFileSync(t.abs);
  if (buf.subarray(0, 8192).includes(0)) return { ...base, binary: true };
  return { ...base, text: buf.toString('utf8') };
}

/** The raw bytes of one file, for a download. */
export function readFileRaw(scope, name, rel) {
  const f = locate(scope, name);
  if (!f.ok) return f;
  const t = fileTarget(f, rel);
  if (!t.ok) return t;
  if (!noLinkEscape(f, t.abs)) return { ok: false, error: 'File not found' };
  try {
    if (!statSync(t.abs).isFile()) return { ok: false, error: 'File not found' };
    return { ok: true, filename: basename(t.abs), data: readFileSync(t.abs) };
  } catch { return { ok: false, error: 'File not found' }; }
}

/** Create or replace one file. `content` is text; `data` is a Buffer (an uploaded asset). */
export function writeFile(scope, name, rel, { content, data } = {}) {
  const f = locate(scope, name);
  if (!f.ok) return f;
  const ro = writable(f);
  if (ro) return ro;
  const t = fileTarget(f, rel);
  if (!t.ok) return t;
  let buf;
  if (Buffer.isBuffer(data)) {
    if (data.length > LIMITS.file) return { ok: false, error: 'That file is too large to add to a skill' };
    buf = data;
  } else {
    if (typeof content !== 'string') return { ok: false, error: 'Nothing to save' };
    buf = Buffer.from(content, 'utf8');
    if (buf.length > LIMITS.editable) return { ok: false, error: 'That is too large to save from the editor' };
  }
  try { if (existsSync(t.abs) && statSync(t.abs).isDirectory()) return { ok: false, error: 'There is a folder with that name' }; } catch { /* fine */ }
  if (!noLinkEscape(f, t.abs)) return { ok: false, error: 'Not a valid file path' };
  const existed = existsSync(t.abs);
  if (!existed && walk(f.dir).files.length >= LIMITS.files) return { ok: false, error: 'This skill already has the most files it can hold' };
  try {
    mkdirSync(dirname(t.abs), { recursive: true });
    if (!noLinkEscape(f, t.abs)) return { ok: false, error: 'Not a valid file path' };
    writeFileSync(t.abs, buf);
  } catch (err) { return { ok: false, error: err.message }; }
  return { ok: true, path: t.rel, size: buf.length, created: !existed };
}

export function deleteFile(scope, name, rel) {
  const f = locate(scope, name);
  if (!f.ok) return f;
  const ro = writable(f);
  if (ro) return ro;
  const t = fileTarget(f, rel);
  if (!t.ok) return t;
  if (t.rel === 'SKILL.md') return { ok: false, error: 'A skill needs its SKILL.md. Delete the skill instead.' };
  if (!noLinkEscape(f, t.abs) || !existsSync(t.abs)) return { ok: false, error: 'File not found' };
  try {
    rmSync(t.abs, { recursive: true, force: true });
    // Leave no empty folders behind, up to the skill itself.
    for (let p = dirname(t.abs); p !== resolve(f.dir) && inside(f.dir, p); p = dirname(p)) {
      if (readdirSync(p).length) break;
      rmSync(p, { recursive: true, force: true });
    }
  } catch (err) { return { ok: false, error: err.message }; }
  return { ok: true };
}

// ─── Creating, installing ───

function tempName(prefix) { return `.${prefix}-${randomBytes(6).toString('hex')}`; }

/** May a new skill take this name in this scope? '' if so. */
function nameTaken(sr, name, { overwrite = false } = {}) {
  const bad = checkName(name);
  if (bad) return { error: bad };
  if (sr.scope === 'synced') return { error: 'Synced skills are managed by Claude Code' };
  if (sr.scope === 'global' && bundledNames().has(name)) {
    return { error: `"${name}" is the name of a skill that ships with Crundi. Choose another name.` };
  }
  const dir = join(sr.root, name);
  if (existsSync(dir)) {
    const info = describe(sr.scope, name, dir);
    if (info && info.readOnly) return { error: WHY_READ_ONLY[info.origin] || 'That skill cannot be replaced' };
    if (!overwrite) return { conflict: true };
  }
  return {};
}

/** Put a staged folder where the skill goes, replacing what was there. */
function swapIn(sr, name, staged) {
  const dest = join(sr.root, name);
  let old = '';
  if (existsSync(dest)) {
    old = join(sr.root, tempName('crundi-old'));
    renameSync(dest, old);
  }
  try { renameSync(staged, dest); }
  catch (err) {
    if (old) { try { renameSync(old, dest); } catch { /* leave the old copy under its temp name */ } }
    throw err;
  }
  if (old) rmSync(old, { recursive: true, force: true });
}

const TEMPLATE = (name, description) => `---
name: ${name}
description: ${description || 'Describe what this skill does and when Claude should use it.'}
---

# ${name}

Write the instructions Claude should follow when this skill is used.
`;

export function create(scope, { name, description } = {}) {
  const sr = scopeRoot(scope);
  if (!sr.ok) return sr;
  const n = String(name || '').trim();
  const taken = nameTaken(sr, n);
  if (taken.error) return { ok: false, error: taken.error };
  if (taken.conflict) return { ok: false, error: `A skill called "${n}" already exists there`, conflict: true, existing: [n] };
  const desc = String(description || '').replace(/\s+/g, ' ').trim().slice(0, 1024);
  try {
    mkdirSync(join(sr.root, n), { recursive: true });
    writeFileSync(join(sr.root, n, 'SKILL.md'), TEMPLATE(n, desc));
  } catch (err) { return { ok: false, error: err.message }; }
  return { ok: true, skill: describe(sr.scope, n, join(sr.root, n)) };
}

const JUNK = /(^|\/)(__MACOSX\/|\.DS_Store$|Thumbs\.db$|desktop\.ini$|\.git\/)/;

/**
 * Work out which skills an archive holds.
 *
 * A skill is any folder with a SKILL.md that has no SKILL.md above it. That one
 * rule covers the shapes skills actually arrive in: files at the top level, one
 * wrapping folder (what a .skill file and a GitHub download both are), and a
 * pack with several skills side by side or under skills/.
 */
function skillsInArchive(entries) {
  const files = [];
  const dropped = [];
  for (const e of entries) {
    if (e.isDir) continue;
    const raw = String(e.name).replace(/\\/g, '/').replace(/^\.\//, '');
    if (JUNK.test(raw)) continue;
    if (e.isSymlink) { dropped.push(raw + ' (a link)'); continue; }
    const rel = safeRel(raw);
    if (!rel) throw new Error(`The archive has a file path that is not allowed: "${raw.slice(0, 120)}"`);
    files.push({ rel, entry: e });
  }
  const roots = [];
  for (const f of files) {
    const parts = f.rel.split('/');
    if (parts[parts.length - 1].toLowerCase() === 'skill.md') roots.push(parts.slice(0, -1).join('/'));
  }
  roots.sort((a, b) => a.length - b.length);
  const top = [];
  for (const r of roots) {
    if (top.some(t => t === '' || r === t || r.startsWith(t + '/'))) continue;
    top.push(r);
  }
  return {
    dropped,
    skills: top.map(root => ({
      root,
      files: files.filter(f => root === '' || f.rel.startsWith(root + '/')).map(f => {
        let rel = root === '' ? f.rel : f.rel.slice(root.length + 1);
        // Claude looks for exactly SKILL.md; a skill.md would install and never load.
        if (rel.toLowerCase() === 'skill.md') rel = 'SKILL.md';
        return { rel, entry: f.entry };
      }),
    })),
  };
}

/**
 * Install from an upload: a .zip or .skill archive holding one or more skills,
 * or a lone SKILL.md.
 *
 * Nothing is written until every skill in the upload has a usable name and a
 * free place to go, so a pack never half-installs. With a clash and no
 * `overwrite`, the reply is { ok:false, conflict:true, existing:[names] }.
 *
 * @param {string} scope
 * @param {{ filename?: string, data: Buffer, name?: string, overwrite?: boolean }} opts
 */
export function install(scope, { filename = '', data, name = '', overwrite = false } = {}) {
  const sr = scopeRoot(scope);
  if (!sr.ok) return sr;
  if (sr.scope === 'synced') return { ok: false, error: 'Synced skills are managed by Claude Code' };
  if (!Buffer.isBuffer(data) || !data.length) return { ok: false, error: 'The upload is empty' };
  if (data.length > LIMITS.upload) return { ok: false, error: 'That upload is too large (the limit is 50 MB)' };

  const stem = slug(basename(String(filename || ''), extname(String(filename || ''))));
  const wanted = String(name || '').trim();
  let plan;
  let dropped = [];
  try {
    if (looksLikeZip(data)) {
      const found = skillsInArchive(readZip(data, { maxEntries: LIMITS.files * 5, maxTotal: LIMITS.unpacked, maxEntry: LIMITS.file }));
      dropped = found.dropped;
      if (!found.skills.length) return { ok: false, error: 'There is no SKILL.md in that archive, so it is not a skill' };
      if (wanted && found.skills.length > 1) return { ok: false, error: 'That archive holds several skills, so they keep their own names' };
      plan = found.skills.map(s => {
        const md = s.files.find(f => f.rel === 'SKILL.md');
        const fm = parseFrontmatter(md.entry.read().toString('utf8'));
        const fromFm = slug(fm.name);
        const fromDir = slug(s.root.split('/').pop());
        return { name: wanted || (checkName(fromFm) ? '' : fromFm) || fromDir || stem, files: s.files, fm };
      });
    } else {
      if (data.subarray(0, 8192).includes(0)) return { ok: false, error: 'Upload a .zip or .skill archive, or a SKILL.md file' };
      const text = data.toString('utf8');
      const fm = parseFrontmatter(text);
      if (!fm.has) return { ok: false, error: 'That file is not a SKILL.md: it has no front matter with a name and description' };
      const fromFm = slug(fm.name);
      plan = [{ name: wanted || (checkName(fromFm) ? '' : fromFm) || (stem === 'skill' ? '' : stem), single: data, fm }];
    }
  } catch (err) { return { ok: false, error: err.message }; }

  // Everything is checked before anything is written.
  const conflicts = [];
  const seen = new Set();
  for (const p of plan) {
    if (!p.name) return { ok: false, error: 'Could not tell what to call this skill. Give it a name.', needsName: true };
    if (seen.has(p.name)) return { ok: false, error: `The archive holds two skills called "${p.name}"` };
    seen.add(p.name);
    if (p.files && p.files.length > LIMITS.files) return { ok: false, error: `"${p.name}" has too many files (the limit is ${LIMITS.files})` };
    const taken = nameTaken(sr, p.name, { overwrite });
    if (taken.error) return { ok: false, error: taken.error };
    if (taken.conflict) conflicts.push(p.name);
  }
  if (conflicts.length) {
    return {
      ok: false, conflict: true, existing: conflicts,
      error: conflicts.length === 1 ? `A skill called "${conflicts[0]}" already exists there` : `These skills already exist there: ${conflicts.join(', ')}`,
    };
  }

  const installed = [];
  try {
    mkdirSync(sr.root, { recursive: true });
    for (const p of plan) {
      const staged = join(sr.root, tempName('crundi-new'));
      try {
        mkdirSync(staged, { recursive: true });
        if (p.single) writeFileSync(join(staged, 'SKILL.md'), p.single);
        else {
          for (const f of p.files) {
            const abs = resolve(staged, f.rel);
            if (!inside(staged, abs)) throw new Error('The archive has a file path that is not allowed');
            mkdirSync(dirname(abs), { recursive: true });
            writeFileSync(abs, f.entry.read());
            // Scripts in a skill are run by Claude; keep them runnable.
            if (f.entry.mode & 0o111) { try { chmodSync(abs, 0o755); } catch { /* not on this filesystem */ } }
          }
        }
        swapIn(sr, p.name, staged);
      } catch (err) {
        rmSync(staged, { recursive: true, force: true });
        throw err;
      }
      installed.push(describe(sr.scope, p.name, join(sr.root, p.name)));
    }
  } catch (err) {
    return { ok: false, error: err.message, installed };
  }
  return { ok: true, installed, dropped };
}

/** Install from a folder, archive or SKILL.md already on this machine (the MCP tool's way in). */
export function installFromPath(scope, path, opts = {}) {
  const abs = resolve(String(path || ''));
  let st;
  try { st = statSync(abs); } catch { return { ok: false, error: 'Nothing at that path' }; }
  if (st.isFile()) {
    if (st.size > LIMITS.upload) return { ok: false, error: 'That file is too large (the limit is 50 MB)' };
    return install(scope, { ...opts, filename: basename(abs), data: readFileSync(abs) });
  }
  if (!st.isDirectory()) return { ok: false, error: 'Nothing at that path' };
  if (!existsSync(join(abs, 'SKILL.md'))) return { ok: false, error: 'That folder has no SKILL.md, so it is not a skill' };
  const { files } = walk(abs, LIMITS.files + 1);
  if (files.length > LIMITS.files) return { ok: false, error: `That folder has too many files (the limit is ${LIMITS.files})` };
  if (files.reduce((n, f) => n + f.size, 0) > LIMITS.unpacked) return { ok: false, error: 'That folder is too large to install as a skill' };
  // Through the same door as an upload, so a folder gets the same checks.
  const zip = writeZip(files.map(f => ({ name: basename(abs) + '/' + f.path, data: readFileSync(join(abs, f.path)), mode: f.mode, mtime: f.mtime })));
  return install(scope, { ...opts, filename: basename(abs) + '.zip', data: zip });
}

// ─── Renaming, moving, removing ───

export function rename(scope, name, to) {
  const f = locate(scope, name);
  if (!f.ok) return f;
  const ro = writable(f);
  if (ro) return ro;
  const next = String(to || '').trim();
  if (next === name) return { ok: true, skill: f.info };
  const sr = { scope: f.scope, root: f.root };
  const taken = nameTaken(sr, next);
  if (taken.error) return { ok: false, error: taken.error };
  if (taken.conflict) return { ok: false, error: `A skill called "${next}" already exists there`, conflict: true, existing: [next] };
  try {
    renameSync(f.dir, join(f.root, next));
    // Claude takes the name from the front matter when there is one, so a
    // folder rename alone would change nothing it can see.
    const md = join(f.root, next, 'SKILL.md');
    if (existsSync(md)) {
      const text = readFileSync(md, 'utf8');
      const fm = parseFrontmatter(text);
      if (fm.has && fm.name === name) {
        const end = text.indexOf('\n---', 3);
        const head = text.slice(0, end).replace(/^(name:[ \t]*).*$/m, (_, k) => k + next);
        writeFileSync(md, head + text.slice(end));
      }
    }
  } catch (err) { return { ok: false, error: err.message }; }
  return { ok: true, skill: describe(f.scope, next, join(f.root, next)) };
}

export function remove(scope, name) {
  const f = locate(scope, name);
  if (!f.ok) return f;
  if (f.info.origin === 'bundled' || f.info.origin === 'synced') return writable(f);
  try {
    // A linked skill: take away the link, never what it points at.
    rmSync(f.dir, { recursive: f.info.origin !== 'linked', force: true });
  } catch (err) { return { ok: false, error: err.message }; }
  return { ok: true };
}

/**
 * Copy a skill to another scope, or move it there.
 *
 * Copying works from anywhere, including the read-only kinds — that is how a
 * bundled or synced skill becomes one you can edit. Moving needs a skill that
 * can be removed from where it is.
 */
export function transfer(scope, name, to, { move = false, overwrite = false, as = '' } = {}) {
  const f = locate(scope, name);
  if (!f.ok) return f;
  if (move) { const ro = writable(f); if (ro) return ro; }
  const sr = scopeRoot(to);
  if (!sr.ok) return sr;
  const next = String(as || '').trim() || name;
  if (sr.scope === f.scope && next === name) return { ok: false, error: 'It is already there' };
  const taken = nameTaken(sr, next, { overwrite });
  if (taken.error) return { ok: false, error: taken.error };
  if (taken.conflict) return { ok: false, error: `A skill called "${next}" already exists there`, conflict: true, existing: [next] };
  const staged = join(sr.root, tempName('crundi-new'));
  try {
    mkdirSync(sr.root, { recursive: true });
    cpSync(f.dir, staged, {
      recursive: true, dereference: false,
      // The copy is the user's own: it must not carry the installer's marker,
      // or the next update would treat it as Crundi's to replace.
      filter: (src) => {
        if (basename(src) === BUNDLED_MARK) return false;
        try { return !lstatSync(src).isSymbolicLink() || src === f.dir; } catch { return false; }
      },
    });
    swapIn(sr, next, staged);
    if (move) rmSync(f.dir, { recursive: true, force: true });
  } catch (err) {
    rmSync(staged, { recursive: true, force: true });
    return { ok: false, error: err.message };
  }
  return { ok: true, skill: describe(sr.scope, next, join(sr.root, next)) };
}

/** The skill as a zip, wrapped in a folder of its own name (the shape install() takes back). */
export function exportZip(scope, name) {
  const f = locate(scope, name);
  if (!f.ok) return f;
  const { files, more } = walk(f.dir);
  // A zip that quietly left files out would look like a complete copy.
  if (more) return { ok: false, error: `This skill has more than ${LIMITS.files} files, which is too many to download from here` };
  if (files.reduce((n, x) => n + x.size, 0) > LIMITS.unpacked) return { ok: false, error: 'This skill is too large to download from here' };
  try {
    const data = writeZip(files.map(x => ({ name: name + '/' + x.path, data: readFileSync(join(f.dir, x.path)), mode: x.mode, mtime: x.mtime })));
    return { ok: true, filename: name + '.zip', data };
  } catch (err) { return { ok: false, error: err.message }; }
}

// ─── For backups ───

/**
 * The global skills a backup should carry: the ones somebody put there. Bundled
 * skills come back with the install and synced ones with the account, so
 * neither is worth the space — and restoring a stale copy over them would be
 * exactly the overwrite this module exists to prevent.
 */
export function backupFiles() {
  const out = [];
  for (const s of skillsIn('global', globalDir())) {
    if (s.origin !== 'user') continue;
    const dir = join(globalDir(), s.name);
    for (const f of walk(dir, Infinity).files) out.push({ abs: join(dir, f.path), rel: s.name + '/' + f.path, size: f.size, mode: f.mode, mtime: f.mtime });
  }
  return out;
}
