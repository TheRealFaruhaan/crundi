/**
 * skills-github.js — install a skill from a GitHub link.
 *
 * Skills are usually shared as a command:
 *
 *   npx skills add https://github.com/latent-spaces/brag --skill brag-slim
 *
 * Running that here would execute downloaded code on the server, so nothing
 * from the repository is ever run: no npx, no git, no script. Crundi reads the
 * commit's file list from GitHub, finds the folders with a SKILL.md, downloads
 * only the chosen skills' files (or, when the list cannot be had, the
 * repository's zip), and hands them to skills-store.install() — the same door,
 * the same name checks, limits and conflict / overwrite rule as an upload.
 * Only files are copied. The only hosts asked are api.github.com,
 * raw.githubusercontent.com and codeload.github.com.
 *
 * What is accepted: a repository link (github.com/o/r), a link into it
 * (github.com/o/r/tree/<ref>/<path>, or /blob/…/SKILL.md), the o/r shorthand,
 * or the whole npx line with --skill. Any other host is refused.
 *
 * Where it came from (repo, ref, path, commit) is saved with the skill (see
 * SOURCE_MARK in skills-store.js), and update() installs again from there.
 *
 * `fetch` is injected (configure({ fetch })) so the tests run offline.
 */

import { readZip, writeZip } from './zip.js';
import * as store from './skills-store.js';

let fetchImpl = (...a) => fetch(...a);
const TIMEOUT_MS = 60 * 1000;

export function configure(opts = {}) {
  if (opts.fetch) fetchImpl = opts.fetch;
}

// ─── Reading what was pasted ───

const SEG_RE = /^[A-Za-z0-9_.-]+$/;
const okSeg = (s) => !!s && SEG_RE.test(s) && s !== '.' && s !== '..';

function cleanPath(p) {
  const parts = String(p || '').split('/').filter(Boolean);
  for (const x of parts) if (x === '.' || x === '..' || /[\0\\]/.test(x)) return null;
  return parts.join('/');
}

/**
 * Read a GitHub source out of a link, an o/r shorthand, or a whole
 * `npx skills add …` line.
 *
 * Returns { ok:true, owner, repo, ref, path, skill, refPath } or { ok:false, error }.
 * `ref` is '' for the default branch. A /tree/ link whose branch name has a
 * slash in it cannot be split from the path by reading alone; `refPath` keeps
 * everything after /tree/ so resolve() can try the longer branch names too.
 */
export function parseSource(text) {
  const src = String(text ?? '').trim();
  if (!src) return { ok: false, error: 'Paste a GitHub link, or an npx skills add line' };
  if (src.length > 2000) return { ok: false, error: 'That is too long to be a link' };
  const tokens = src.split(/\s+/);
  let skill = '';
  let target = '';
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    const m = /^(?:--skill|-s)(?:=(.*))?$/.exec(t);
    if (m) {
      skill = m[1] !== undefined ? m[1] : (tokens[i + 1] || '');
      if (m[1] === undefined) i++;
      continue;
    }
    if (t.startsWith('-')) continue;                       // -y, --yes, -g, --global …
    if (!target && /^(npx|pnpx|bunx|npm|pnpm|yarn|exec|dlx|skills(@\S*)?|add|install|i)$/i.test(t)) continue;
    if (!target) target = t;
  }
  skill = skill.replace(/^['"]|['"]$/g, '').trim();
  target = target.replace(/^['"<]|['">]$/g, '');
  if (!target) return { ok: false, error: 'There is no GitHub link in that' };

  let rest;
  const url = /^(?:(https?):\/\/)?(?:www\.)?([^/\s]+)(\/.*)?$/i.exec(target);
  if (/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(target) && !/\.(com|org|net|io)\//i.test(target + '/')) {
    rest = target;                                          // o/r shorthand
  } else if (/^git@github\.com:/i.test(target)) {
    rest = target.slice('git@github.com:'.length);
  } else if (url && url[2].toLowerCase() === 'github.com') {
    rest = (url[3] || '').replace(/^\/+/, '');
  } else {
    return { ok: false, error: 'Only GitHub links are supported (github.com/owner/repo)' };
  }
  rest = rest.replace(/[?#].*$/, '');
  const parts = rest.split('/').filter(Boolean);
  const owner = parts[0] || '';
  const repo = (parts[1] || '').replace(/\.git$/i, '');
  if (!okSeg(owner) || !okSeg(repo)) return { ok: false, error: 'That link does not name a GitHub repository (github.com/owner/repo)' };

  let ref = '';
  let path = '';
  let refPath = '';
  if (parts.length > 2) {
    if (parts[2] !== 'tree' && parts[2] !== 'blob') return { ok: false, error: 'That link is not to a repository folder' };
    if (parts.length < 4) return { ok: false, error: 'That link does not say which branch' };
    let segs;
    try { segs = parts.slice(3).map(decodeURIComponent); } catch { return { ok: false, error: 'That link is damaged' }; }
    ref = segs[0];
    const p = cleanPath(segs.slice(1).join('/'));
    if (p === null || /[\0\s]/.test(ref) || ref.includes('..')) return { ok: false, error: 'That link has a path that is not allowed' };
    path = p.replace(/(^|\/)SKILL\.md$/i, '').replace(/\/$/, '');
    refPath = segs.join('/');
  }
  if (skill && skill !== '*' && !/^[A-Za-z0-9_.: -]{1,128}$/.test(skill)) return { ok: false, error: 'That skill name is not valid' };
  return { ok: true, owner, repo, ref, path, skill, refPath };
}

// ─── Talking to GitHub ───

async function getJson(url) {
  let res;
  try {
    res = await fetchImpl(url, {
      headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'crundi', 'X-GitHub-Api-Version': '2022-11-28' },
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
  } catch (err) { return { status: 0, error: 'Could not reach GitHub: ' + (err && err.message || err) }; }
  let body = null;
  try { body = await res.json(); } catch { body = null; }
  return { status: res.status, body };
}

const limited = (status) => status === 403 || status === 429;

/**
 * Settle which branch or tag and which commit to download. Returns
 * { ok, ref, commit, path } or { ok:false, error }. When the API cannot be
 * used (rate limited, as it is after 60 unauthenticated calls an hour) the
 * download goes ahead by name and the commit is read from the archive.
 */
export async function resolve(src) {
  const base = `https://api.github.com/repos/${src.owner}/${src.repo}`;
  let ref = src.ref;
  let path = src.path;
  if (!ref) {
    const r = await getJson(base);
    if (r.status === 404) return { ok: false, error: `There is no public GitHub repository ${src.owner}/${src.repo}` };
    if (r.status === 200 && r.body && typeof r.body.default_branch === 'string') ref = r.body.default_branch;
    else if (r.status && !limited(r.status)) return { ok: false, error: `GitHub answered ${r.status} for ${src.owner}/${src.repo}` };
    else if (!r.status) return { ok: false, error: r.error };
    else return { ok: true, ref: 'HEAD', commit: '', path };
  }
  // A branch name may have slashes in it: try the shortest first.
  const segs = src.refPath ? src.refPath.split('/') : [ref];
  for (let n = 1; n <= Math.min(segs.length, 5); n++) {
    const tryRef = segs.slice(0, n).join('/');
    const r = await getJson(`${base}/commits/${encodeURIComponent(tryRef)}`);
    if (r.status === 200 && r.body && /^[0-9a-f]{40}$/.test(String(r.body.sha || ''))) {
      const p = src.refPath ? cleanPath(segs.slice(n).join('/')) : path;
      return { ok: true, ref: tryRef, commit: r.body.sha, path: String(p || '').replace(/(^|\/)SKILL\.md$/i, '').replace(/\/$/, '') };
    }
    if (!r.status) return { ok: false, error: r.error };
    if (limited(r.status)) return { ok: true, ref, commit: '', path };
    if (r.status === 404 && n === 1 && !src.refPath) {
      const repo = await getJson(base);
      if (repo.status === 404) return { ok: false, error: `There is no public GitHub repository ${src.owner}/${src.repo}` };
    }
  }
  return { ok: false, error: `${src.owner}/${src.repo} has no branch, tag or commit called "${ref}"` };
}

// The zip of a whole repository is the fallback (see load()): it carries
// everything, demo videos included, so it may be larger than an upload. What
// is installed out of it is still held to the upload limits.
const ARCHIVE_MAX = 3 * store.LIMITS.upload;
const PARALLEL = 6;

/** Read a response body, refusing it as soon as it passes `max` bytes. */
async function readCapped(res, max, what = 'That repository') {
  const tooBig = () => new Error(`${what} is too large to install from (the limit is ${Math.round(max / 1048576)} MB)`);
  const declared = Number(res.headers && typeof res.headers.get === 'function' ? res.headers.get('content-length') : 0);
  if (declared > max) throw tooBig();
  if (res.body && typeof res.body.getReader === 'function') {
    const reader = res.body.getReader();
    const chunks = [];
    let size = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.length;
      if (size > max) { try { await reader.cancel(); } catch { /* gone */ } throw tooBig(); }
      chunks.push(Buffer.from(value));
    }
    return Buffer.concat(chunks);
  }
  const buf = Buffer.from(await res.arrayBuffer());
  if (buf.length > max) throw tooBig();
  return buf;
}

/** The zip comment GitHub writes into its archives: the commit id. */
function zipComment(buf) {
  for (let i = buf.length - 22, stop = Math.max(0, buf.length - 22 - 0xffff); i >= stop; i--) {
    if (buf.readUInt32LE(i) === 0x06054b50) {
      const len = buf.readUInt16LE(i + 20);
      return buf.subarray(i + 22, i + 22 + len).toString('utf8');
    }
  }
  return '';
}

/** Run `fn` over `items`, a few at a time. */
async function each(items, fn) {
  let i = 0;
  const worker = async () => { while (i < items.length) { const x = items[i++]; await fn(x); } };
  await Promise.all(Array.from({ length: Math.min(PARALLEL, items.length) }, worker));
}

// ─── The repository's files ───
// Either way, a file is { path, entry } where entry is { size, mode, read(),
// fetch() }: read() is synchronous (what skills-store and writeZip need) and
// works once fetch() has run. From a zip, fetch() has nothing to do.

/**
 * The file list of one commit (the git trees API) with each file fetched on
 * its own from raw.githubusercontent.com, so a skill of a few files does not
 * download the whole repository. null when the list cannot be had (rate
 * limited, too large to list in one answer); the zip is used then.
 */
async function treeFiles(owner, repo, commit) {
  const r = await getJson(`https://api.github.com/repos/${owner}/${repo}/git/trees/${commit}?recursive=1`);
  if (r.status !== 200 || !r.body || !Array.isArray(r.body.tree) || r.body.truncated) return null;
  const files = [];
  for (const t of r.body.tree) {
    // Blobs only: not folders, not submodules (type commit), not links (120000).
    if (!t || t.type !== 'blob' || typeof t.path !== 'string' || t.mode === '120000') continue;
    const path = t.path;
    const size = Number(t.size) || 0;
    const entry = {
      size, mode: t.mode === '100755' ? 0o755 : 0o644, data: null,
      read() { if (!this.data) throw new Error(`"${path}" was not downloaded`); return this.data; },
      async fetch() {
        if (this.data) return;
        if (!store.safeRel(path)) throw new Error(`The repository has a file path that is not allowed: "${path.slice(0, 120)}"`);
        let res;
        try {
          res = await fetchImpl(`https://raw.githubusercontent.com/${owner}/${repo}/${commit}/${path.split('/').map(encodeURIComponent).join('/')}`, {
            headers: { 'User-Agent': 'crundi' }, signal: AbortSignal.timeout(TIMEOUT_MS),
          });
        } catch (err) { throw new Error('Could not download from GitHub: ' + (err && err.message || err)); }
        if (res.status !== 200) throw new Error(`GitHub answered ${res.status} for "${path}"`);
        const data = await readCapped(res, store.LIMITS.file, `"${path}"`);
        if (data.length !== size) throw new Error(`"${path}" did not download whole`);
        this.data = data;
      },
    };
    files.push({ path, entry });
  }
  return files;
}

async function downloadZip(owner, repo, ref) {
  let res;
  try {
    res = await fetchImpl(`https://codeload.github.com/${owner}/${repo}/zip/${ref.split('/').map(encodeURIComponent).join('/')}`, {
      headers: { 'User-Agent': 'crundi' }, redirect: 'follow', signal: AbortSignal.timeout(TIMEOUT_MS),
    });
  } catch (err) { throw new Error('Could not download from GitHub: ' + (err && err.message || err)); }
  if (res.status === 404) throw new Error(`Could not find ${owner}/${repo} at "${ref}" on GitHub`);
  if (res.status !== 200) throw new Error(`GitHub answered ${res.status} to the download`);
  return readCapped(res, ARCHIVE_MAX);
}

/** The files in a repository zip, without the <repo>-<ref>/ folder GitHub wraps them in. */
function zipFiles(data) {
  // Nothing is inflated by listing, so the whole repository may be listed;
  // the limits that matter are applied to the skills taken out of it.
  const entries = readZip(data, { maxEntries: 0xfffe, maxTotal: Number.MAX_SAFE_INTEGER, maxEntry: Number.MAX_SAFE_INTEGER });
  const names = entries.map(e => String(e.name).replace(/\\/g, '/'));
  const first = names.length ? names[0].split('/')[0] : '';
  const wrapped = first && names.every(n => n === first + '/' || n.startsWith(first + '/'));
  const files = [];
  for (let i = 0; i < entries.length; i++) {
    const e = entries[i];
    if (e.isDir || e.isSymlink) continue;
    const path = wrapped ? names[i].slice(first.length + 1) : names[i];
    if (path) files.push({ path, entry: { size: e.size, mode: e.mode, read: () => e.read(), fetch: async () => {} } });
  }
  return files;
}

// ─── Finding the skills ───

const SKIP = /(^|\/)(\.git|node_modules|__MACOSX)(\/|$)/;

/**
 * The skills among a repository's files at or under `base`: a SKILL.md at
 * `base` itself, or else every top-most folder with a SKILL.md below it
 * (skills/<name>/, .claude/skills/<name>/, or side by side). Each is { root,
 * name, title, description, files: [{ rel, entry }], size }. Only the SKILL.md
 * files are read.
 */
export async function findSkills(all, base = '', repoName = '') {
  const files = all.filter(f => !SKIP.test(f.path) && (!base || f.path.startsWith(base + '/')));
  const roots = [];
  for (const f of files) {
    const parts = f.path.split('/');
    if (parts[parts.length - 1].toLowerCase() === 'skill.md') roots.push(parts.slice(0, -1).join('/'));
  }
  roots.sort((a, b) => a.length - b.length || (a < b ? -1 : 1));
  const top = [];
  for (const r of roots) {
    if (top.some(t => t === '' || r === t || r.startsWith(t + '/'))) continue;
    top.push(r);
  }
  const found = top.slice(0, 200).map(root => {
    const own = files.filter(f => root === '' || f.path.startsWith(root + '/'))
      .map(f => ({ rel: root === '' ? f.path : f.path.slice(root.length + 1), entry: f.entry }));
    return { root, own, md: own.find(f => f.rel.toLowerCase() === 'skill.md') };
  });
  const mds = found.map(x => x.md).filter(md => md && md.entry.size <= store.LIMITS.editable);
  await each(mds, async (md) => { try { await md.entry.fetch(); } catch { /* listed without its description */ } });
  return found.map(({ root, own, md }) => {
    let fm = { name: '', description: '' };
    if (md && md.entry.size <= store.LIMITS.editable) { try { fm = store.parseFrontmatter(md.entry.read().toString('utf8')); } catch { /* damaged or not fetched */ } }
    // The name install() will give it, so the list shows what will appear.
    const fromFm = store.slug(fm.name);
    const folder = root ? root.split('/').pop() : repoName;
    const name = (store.checkName(fromFm) ? '' : fromFm) || store.slug(folder);
    return { root, name, title: fm.name || name, description: fm.description || '', files: own, size: own.reduce((n, f) => n + f.entry.size, 0) };
  });
}

function matches(s, want) {
  const w = String(want).trim().toLowerCase();
  return s.name === w || String(s.title).toLowerCase() === w || (s.root && s.root.split('/').pop().toLowerCase() === w) || s.name === store.slug(w);
}

const summary = (s) => ({ name: s.name, title: s.title, description: s.description, path: s.root, files: s.files.length, size: s.size });

/**
 * Fetch and list. Returns { ok, source: { repo, ref, path, commit, url }, skills, found }
 * where `skills` is the whole list and `found` the parsed entries (internal).
 */
async function load(text) {
  const src = typeof text === 'object' && text ? { ok: true, refPath: '', skill: '', ...text } : parseSource(text);
  if (!src.ok) return src;
  const r = await resolve(src);
  if (!r.ok) return r;
  let commit = r.commit;
  let files = null;
  try {
    // One commit's file list and only the files wanted; the whole zip when
    // that cannot be had (no commit because the API is rate limited, or a
    // list too long for one answer).
    if (commit) files = await treeFiles(src.owner, src.repo, commit);
    if (!files) {
      const data = await downloadZip(src.owner, src.repo, commit || r.ref);
      if (!commit) { const c = zipComment(data).trim(); if (/^[0-9a-f]{40}$/.test(c)) commit = c; }
      files = zipFiles(data);
    }
  } catch (err) { return { ok: false, error: err.message }; }
  let found;
  try { found = await findSkills(files, r.path, src.repo); }
  catch (err) { return { ok: false, error: err.message }; }
  const repo = `${src.owner}/${src.repo}`;
  const ref = r.ref === 'HEAD' ? '' : r.ref;
  const source = { repo, ref, path: r.path, commit, url: `https://github.com/${repo}${ref ? '/tree/' + ref + (r.path ? '/' + r.path : '') : ''}` };
  if (!found.length) return { ok: false, error: `There is no SKILL.md in ${repo}${r.path ? ' at ' + r.path : ''}, so it holds no skill`, source };
  return { ok: true, src, source, found, skills: found.map(summary) };
}

/** List the skills a link points at, without installing anything. */
export async function preview(text) {
  const l = await load(text);
  if (!l.ok) return l;
  const { found, src, ...out } = l;
  return { ...out, skill: src.skill || '' };
}

/**
 * Install from a link. opts: { skill, all, name, overwrite }. A `skill` given
 * here wins over one in the pasted line.
 *
 * With several skills and no choice the reply is { ok:false, choose:true,
 * skills } so the person can pick one, or ask for all.
 */
export async function installFromLink(scope, text, opts = {}) {
  const l = await load(text);
  if (!l.ok) return l;
  const want = String(opts.skill || l.src.skill || '').trim();
  let chosen;
  if (opts.updating) {
    // An update takes the one skill at its path, or the one of its name there.
    chosen = l.found.length === 1 ? l.found : l.found.filter(s => matches(s, opts.updating)).slice(0, 1);
    if (!chosen.length) return { ok: false, error: `${l.source.repo} no longer has this skill${l.source.path ? ' at ' + l.source.path : ''}`, source: l.source };
  } else if (opts.all || want === '*') chosen = l.found;
  else if (want) {
    chosen = l.found.filter(s => matches(s, want)).slice(0, 1);
    if (!chosen.length) return { ok: false, error: `There is no skill called "${want}" there. It has: ${l.found.map(s => s.name).join(', ')}`, source: l.source, skills: l.skills };
  } else if (l.found.length === 1) chosen = l.found;
  else return { ok: false, choose: true, error: `${l.source.repo} holds ${l.found.length} skills. Choose one, or install all of them.`, source: l.source, skills: l.skills };

  // The upload limits, per skill and for the lot, before anything is unpacked.
  const L = store.LIMITS;
  let total = 0;
  for (const s of chosen) {
    if (s.files.length > L.files) return { ok: false, error: `"${s.name}" has too many files (the limit is ${L.files})` };
    for (const f of s.files) if (f.entry.size > L.file) return { ok: false, error: `"${s.name}/${f.rel}" is too large` };
    total += s.size;
  }
  if (total > L.unpacked) return { ok: false, error: 'That is too large to install as a skill' };

  // Repacked under each skill's repository path, so install() sees the same
  // roots and can say where each one came from.
  const wrap = store.slug(l.src.repo) || 'skill';
  const at = new Map();
  const files = [];
  try {
    await each(chosen.flatMap(s => s.files.map(f => f.entry)), (e) => e.fetch());
    for (const s of chosen) {
      const root = s.root || wrap;
      at.set(root, s.root);
      for (const f of s.files) files.push({ name: root + '/' + f.rel, data: f.entry.read(), mode: f.entry.mode || 0o644 });
    }
  } catch (err) { return { ok: false, error: err.message }; }
  const { repo, ref, commit } = l.source;
  const r = store.install(scope, {
    filename: wrap + '.zip', data: writeZip(files), name: chosen.length === 1 ? (opts.name || '') : '', overwrite: !!opts.overwrite,
    source: (root) => (at.has(root) ? { repo, ref, path: at.get(root), commit } : null),
  });
  return { ...r, source: l.source, skills: l.skills };
}

/**
 * Install a skill again from where it came from, replacing it. Says
 * upToDate:true and changes nothing when the commit is the same, unless
 * `force`.
 */
export async function update(scope, name, { force = false } = {}) {
  const d = store.get(scope, name);
  if (!d.ok) return d;
  const s = d.skill.source;
  if (!s) return { ok: false, error: 'This skill was not installed from a link, so there is nothing to update it from' };
  if (d.skill.readOnly) return { ok: false, error: 'This skill cannot be changed', readOnly: true };
  const [owner, repo] = s.repo.split('/');
  const src = { owner, repo, ref: s.ref, path: s.path, refPath: '', skill: '' };
  if (!force && s.commit && s.ref) {
    const r = await resolve(src);
    if (!r.ok) return r;
    if (r.commit && r.commit === s.commit) return { ok: true, upToDate: true, source: { ...s }, skill: d.skill };
  }
  const r = await installFromLink(d.skill.scope, src, { updating: name, name, overwrite: true });
  if (!r.ok && r.error && /no SKILL\.md/.test(r.error)) return { ...r, error: `${s.repo} no longer has this skill${s.path ? ' at ' + s.path : ''}` };
  return { ...r, upToDate: false };
}
