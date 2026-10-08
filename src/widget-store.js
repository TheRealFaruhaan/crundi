/**
 * widget-store.js — Claude-authored UI widgets: what exists, where it sits, and
 * the small amounts of state each one keeps.
 *
 * A widget has two homes:
 *
 *   SOURCE   <project>/.crundi/widgets/<id>/      edited like any other code
 *              widget.json     manifest: title, slot, data sources, actions
 *              index.html      the body: markup, <style>, <script>
 *              fixtures/*.json canned data for rendering a state on demand
 *
 *   STATE    <dataDir>/widgets/<project>/<id>/    owned by Crundi
 *              meta.json       placement, lifecycle, grant, faults
 *              data.json       values Claude pushed (the "push" sources)
 *              store.json      the widget's own key/value store
 *              events.json     things the person did, waiting for Claude
 *              versions/N.json a copy of the source at each change
 *
 * A project with no folder (or the global scope) keeps its source under STATE
 * in src/ instead. One folder per widget and write-then-rename throughout: the
 * whole-file JSON stores elsewhere in Crundi lose a write when two land at
 * once, and a widget is written by Claude, the page and a timer all at once.
 *
 * "Widget" on purpose. "Pane" is a parked terminal and "panel" is a tab or a
 * workbench cell; a third meaning for either would make the code unsearchable.
 */

import { join, resolve as resolvePath, sep, dirname } from 'node:path';
import {
  existsSync, mkdirSync, readFileSync, writeFileSync, renameSync, readdirSync,
  rmSync, statSync, realpathSync, cpSync,
} from 'node:fs';
import { createHash, randomBytes } from 'node:crypto';
import { config } from './config.js';
import { getProject } from './project-store.js';

export const SLOTS = ['dock', 'cell', 'tab', 'inline', 'chip'];
export const SOURCE_KINDS = ['push', 'session', 'file', 'sqlite', 'command', 'http', 'crundi'];
export const GLOBAL = '_global';

export const LIMITS = {
  html: 512 * 1024,          // index.html
  manifest: 64 * 1024,       // widget.json
  asset: 512 * 1024,         // one inlined image, script or stylesheet
  assetsTotal: 2 * 1024 * 1024,
  pushValue: 1024 * 1024,    // one pushed source
  store: 256 * 1024,         // the widget's own store
  events: 50,
  faults: 20,
  versions: 20,
  perProject: 40,
};

const validId = (id) => /^[a-z0-9][a-z0-9-]{0,39}$/.test(String(id || ''));
const validName = (n) => /^[a-zA-Z_][a-zA-Z0-9_-]{0,39}$/.test(String(n || ''));
export { validId, validName };

export function projectKey(alias) {
  const a = String(alias || '').toLowerCase().replace(/[^a-z0-9._-]/g, '_');
  return a || GLOBAL;
}

const rootDir = () => join(config.dataDir, 'widgets');
const stateDir = (alias, id) => join(rootDir(), projectKey(alias), id);
/** Crundi's own folder for a widget (meta, data, versions, renders). */
export function stateDirOf(alias, id) { return stateDir(alias, id); }

function readJson(file, fallback) {
  try { return JSON.parse(readFileSync(file, 'utf8')); } catch { return fallback; }
}

/** Write through a temp file, so a reader never sees half a document. */
function writeJson(file, value) {
  const tmp = file + '.' + randomBytes(4).toString('hex') + '.tmp';
  writeFileSync(tmp, JSON.stringify(value));
  renameSync(tmp, file);
}

function ensureDir(dir) { if (!existsSync(dir)) mkdirSync(dir, { recursive: true }); }

/** The project's folder on disk, or '' when it has none. */
export function projectRoot(alias) {
  if (!alias || projectKey(alias) === GLOBAL) return '';
  try {
    const p = getProject(alias);
    return p && p.path && existsSync(p.path) ? resolvePath(p.path) : '';
  } catch { return ''; }
}

/**
 * The folder a widget's relative paths and commands are anchored to.
 *
 * For a project widget that is the project. A GLOBAL widget shows in every
 * project, but it was written against one: its manifest says "data/app.db",
 * not an absolute path. It keeps that project as its home (meta.home), so
 * promoting a widget changes where it is shown and nothing about what it reads.
 */
export function dataRoot(alias, id) {
  if (projectKey(alias) !== GLOBAL) return projectRoot(alias);
  const meta = id ? readMeta(alias, id) : null;
  return meta && meta.home ? projectRoot(meta.home) : '';
}

/** Where a widget's commands run: its (home) project. Same thing as dataRoot, named for the reader. */
export function commandRoot(alias, id) { return dataRoot(alias, id); }

/** The project whose board, services and schedules a widget reads. */
export function homeAlias(alias, id) {
  if (projectKey(alias) !== GLOBAL) return projectKey(alias);
  const meta = id ? readMeta(alias, id) : null;
  return (meta && meta.home) || '';
}

/** Which scope holds this id: the project first, then global. Null if neither. */
export function resolveScope(alias, id) {
  if (!validId(id)) return null;
  if (readMeta(alias, id)) return projectKey(alias);
  if (projectKey(alias) !== GLOBAL && readMeta(GLOBAL, id)) return GLOBAL;
  return null;
}

/** Where a widget's source lives. In the project when it has a folder. */
export function sourceDir(alias, id) {
  const root = projectRoot(alias);
  return root ? join(root, '.crundi', 'widgets', id) : join(stateDir(alias, id), 'src');
}

// ─── Meta ───

function readMeta(alias, id) {
  if (!validId(id)) return null;
  return readJson(join(stateDir(alias, id), 'meta.json'), null);
}

function writeMeta(alias, meta) {
  const dir = stateDir(alias, meta.id);
  ensureDir(dir);
  meta.updatedAt = Date.now();
  writeJson(join(dir, 'meta.json'), meta);
  return meta;
}

export function get(alias, id) { return readMeta(alias, id); }

export function list(alias) {
  const dir = join(rootDir(), projectKey(alias));
  let names = [];
  try { names = readdirSync(dir); } catch { return []; }
  const out = [];
  for (const n of names) {
    if (!validId(n)) continue;
    const m = readMeta(alias, n);
    if (m) out.push(m);
  }
  return out.sort((a, b) => (a.createdAt || 0) - (b.createdAt || 0));
}

/** Every project that has at least one widget. */
export function listProjectsWithWidgets() {
  try { return readdirSync(rootDir()).filter(n => { try { return statSync(join(rootDir(), n)).isDirectory(); } catch { return false; } }); }
  catch { return []; }
}

// ─── Source ───

export function readManifest(alias, id) {
  const file = join(sourceDir(alias, id), 'widget.json');
  let raw;
  try { raw = readFileSync(file, 'utf8'); } catch { return { ok: true, manifest: {}, missing: true }; }
  if (raw.length > LIMITS.manifest) return { ok: false, error: `widget.json is over ${LIMITS.manifest / 1024} KB` };
  try {
    const m = JSON.parse(raw);
    if (!m || typeof m !== 'object' || Array.isArray(m)) return { ok: false, error: 'widget.json must be a JSON object' };
    return { ok: true, manifest: m };
  } catch (err) { return { ok: false, error: `widget.json is not valid JSON: ${err.message}` }; }
}

export function readHtml(alias, id) {
  const file = join(sourceDir(alias, id), 'index.html');
  try {
    const st = statSync(file);
    if (st.size > LIMITS.html) return { ok: false, error: `index.html is over ${LIMITS.html / 1024} KB` };
    return { ok: true, html: readFileSync(file, 'utf8') };
  } catch { return { ok: false, error: `No index.html in ${sourceDir(alias, id)}` }; }
}

export function listFixtures(alias, id) {
  try {
    return readdirSync(join(sourceDir(alias, id), 'fixtures'))
      .filter(n => /^[a-zA-Z0-9_-]{1,40}\.json$/.test(n)).map(n => n.slice(0, -5)).sort();
  } catch { return []; }
}

export function readFixture(alias, id, name) {
  if (!/^[a-zA-Z0-9_-]{1,40}$/.test(String(name || ''))) return { ok: false, error: 'Bad fixture name' };
  const file = join(sourceDir(alias, id), 'fixtures', name + '.json');
  try {
    const v = JSON.parse(readFileSync(file, 'utf8'));
    if (!v || typeof v !== 'object' || Array.isArray(v)) return { ok: false, error: `fixtures/${name}.json must be an object of { sourceName: value }` };
    return { ok: true, data: v };
  } catch (err) {
    return { ok: false, error: existsSync(file) ? `fixtures/${name}.json is not valid JSON: ${err.message}` : `No fixture "${name}" (have: ${listFixtures(alias, id).join(', ') || 'none'})` };
  }
}

/** Write source files on Claude's behalf (the inline `html` / `manifest` args). */
export function writeSource(alias, id, { html, manifest, fixtures } = {}) {
  const dir = sourceDir(alias, id);
  ensureDir(dir);
  if (typeof html === 'string') {
    if (Buffer.byteLength(html) > LIMITS.html) return { ok: false, error: `html is over ${LIMITS.html / 1024} KB` };
    writeFileSync(join(dir, 'index.html'), html);
  }
  if (manifest && typeof manifest === 'object') {
    const body = JSON.stringify(manifest, null, 2);
    if (body.length > LIMITS.manifest) return { ok: false, error: 'manifest is too large' };
    writeFileSync(join(dir, 'widget.json'), body + '\n');
  }
  if (fixtures && typeof fixtures === 'object') {
    ensureDir(join(dir, 'fixtures'));
    for (const [name, value] of Object.entries(fixtures)) {
      if (!/^[a-zA-Z0-9_-]{1,40}$/.test(name)) return { ok: false, error: `Bad fixture name "${name}"` };
      writeFileSync(join(dir, 'fixtures', name + '.json'), JSON.stringify(value, null, 2) + '\n');
    }
  }
  return { ok: true, dir };
}

const ASSET_EXT = new Set(['.js', '.mjs', '.css', '.png', '.jpg', '.jpeg', '.gif', '.webp', '.svg', '.ico', '.woff', '.woff2']);
function isSourceFile(rel, name) {
  if (rel === '' && (name === 'index.html' || name === 'widget.json')) return true;
  if (rel === 'fixtures/') return name.endsWith('.json');
  const dot = name.lastIndexOf('.');
  return dot > 0 && ASSET_EXT.has(name.slice(dot).toLowerCase());
}

/** A fingerprint of the source, so a change can be told from a no-op. */
export function sourceHash(alias, id) {
  const dir = sourceDir(alias, id);
  const h = createHash('sha256');
  const walk = (d, rel) => {
    let names = [];
    try { names = readdirSync(d).sort(); } catch { return; }
    for (const n of names) {
      if (n.startsWith('.')) continue;
      const p = join(d, n);
      let st; try { st = statSync(p); } catch { continue; }
      if (st.isDirectory()) { if (rel.split('/').length < 3) walk(p, rel + n + '/'); continue; }
      if (st.size > LIMITS.html) continue;
      // Only what the frame is BUILT from. A widget may keep data beside its
      // source ("./state.json", a log, a small database) that something
      // rewrites every few seconds; counting that as a change to the source
      // reloaded the frame, and cut a new version, on every write.
      if (!isSourceFile(rel, n)) continue;
      h.update(rel + n + '\0');
      try { h.update(readFileSync(p)); } catch { /* unreadable: leave it out */ }
      h.update('\0');
    }
  };
  walk(dir, '');
  return h.digest('hex').slice(0, 16);
}

// ─── Grants ───
//
// What a widget may do without anyone watching: run commands, fetch URLs, read
// outside its project, act on a click. Claude already has a shell as the
// owner, so this is not a fence around Claude. It is the owner seeing, and
// agreeing to, what keeps running after the chat that wrote it is gone.

function realRoot(root) { try { return realpathSync(root); } catch { return root; } }

function insideRoot(root, target) {
  if (!root) return false;
  const r = root.endsWith(sep) ? root : root + sep;
  return target === root || target.startsWith(r);
}

/**
 * Resolve a source's path.
 *
 *   "./state.json"      the widget's OWN folder. This is data that belongs to
 *                       the widget: it travels with it when it goes global, and
 *                       is then in Crundi's backup.
 *   "data/app.db"       the (home) project. Project data: read where it is,
 *                       never copied, global or not.
 *
 * A global widget written before it was promoted may name its own folder the
 * long way (".crundi/widgets/<id>/state.json"). That folder has moved, so the
 * path is followed to where the folder is now.
 */
export function resolveDataPath(alias, p, id) {
  const root = dataRoot(alias, id);
  const raw = String(p || '');
  if (!raw) return { ok: false, error: 'Missing path' };
  const own = id ? sourceDir(alias, id) : '';
  if (own && (raw.startsWith('./') || raw.startsWith('.\\'))) {
    const abs = resolvePath(own, raw);
    let real = abs; try { real = realpathSync(abs); } catch { /* not there yet */ }
    let realOwn = own; try { realOwn = realpathSync(own); } catch { /* keep */ }
    // "./../../etc" is not the widget's folder any more; judge it like any other path.
    if (insideRoot(realOwn, real)) return { ok: true, path: real, inProject: true, own: true };
  }
  let abs = raw.startsWith('~/') ? join(process.env.HOME || '', raw.slice(2)) : raw;
  abs = resolvePath(root || '/', abs);
  if (own && root && projectKey(alias) === GLOBAL) {
    const was = join(root, '.crundi', 'widgets', id);
    if (insideRoot(was, abs)) abs = join(own, abs.slice(was.length));
  }
  let real = abs;
  try { real = realpathSync(abs); } catch { /* not there yet: judge the spelling */ }
  let realRoot_ = root;
  try { if (root) realRoot_ = realpathSync(root); } catch { /* keep */ }
  let realOwn = own; try { if (own) realOwn = realpathSync(own); } catch { /* keep */ }
  const isOwn = !!own && insideRoot(realOwn, real);
  return { ok: true, path: real, inProject: isOwn || insideRoot(realRoot_, real), own: isOwn };
}

/**
 * The parts of a manifest that need the owner's say-so, as a stable list.
 * The same list is hashed for the grant, so changing any of it asks again.
 */
export function privileged(alias, manifest, id) {
  const out = [];
  const sources = (manifest && manifest.sources) || {};
  for (const name of Object.keys(sources).sort()) {
    const s = sources[name] || {};
    if (s.kind === 'command') out.push({ what: 'command', name, detail: String(Array.isArray(s.run) ? s.run.join(' ') : s.run || '') });
    else if (s.kind === 'http') out.push({ what: 'fetch', name, detail: String(s.url || '') });
    else if (s.kind === 'file' || s.kind === 'sqlite') {
      const r = resolveDataPath(alias, s.path, id);
      if (r.ok && !r.inProject) out.push({ what: s.kind === 'sqlite' ? 'database outside the project' : 'file outside the project', name, detail: r.path });
    }
  }
  const actions = (manifest && manifest.actions) || {};
  for (const name of Object.keys(actions).sort()) {
    const a = actions[name] || {};
    if (a.kind === 'command') out.push({ what: 'action: command', name, detail: String(a.run || '') });
    else if (a.kind === 'tool') out.push({ what: 'action: tool', name, detail: String(a.tool || '') + ' ' + JSON.stringify(a.args || {}) });
    else if (a.kind === 'prompt') out.push({ what: 'action: message to the chat', name, detail: String(a.text || '') });
  }
  if (Array.isArray(manifest && manifest.allow) && manifest.allow.includes('prompt')) {
    out.push({ what: 'send any message to the chat', name: 'prompt', detail: '' });
  }
  return out;
}

export function grantHashOf(items) {
  if (!items.length) return '';
  return createHash('sha256').update(JSON.stringify(items)).digest('hex').slice(0, 24);
}

/** { needed, granted, items } for a widget as its manifest stands now. */
export function grantState(alias, id, manifest) {
  const items = privileged(alias, manifest, id);
  const hash = grantHashOf(items);
  const meta = readMeta(alias, id);
  return { items, hash, needed: !!hash, granted: !hash || (!!meta && meta.grantHash === hash) };
}

export function grant(alias, id, hash) {
  const meta = readMeta(alias, id);
  if (!meta) return { ok: false, error: 'No such widget' };
  const m = readManifest(alias, id);
  if (!m.ok) return m;
  const want = grantHashOf(privileged(alias, m.manifest, id));
  // The page approves what it was SHOWN. If the manifest moved on since, the
  // approval is for something else and must not carry over.
  if (hash !== want) return { ok: false, error: 'The widget changed since this was shown. Review it again.', stale: true };
  meta.grantHash = want;
  meta.grantedAt = Date.now();
  writeMeta(alias, meta);
  return { ok: true };
}

export function revoke(alias, id) {
  const meta = readMeta(alias, id);
  if (!meta) return { ok: false, error: 'No such widget' };
  delete meta.grantHash; delete meta.grantedAt;
  writeMeta(alias, meta);
  return { ok: true };
}

// ─── Create, place, close ───

function cleanTitle(t, fallback) {
  return String(t == null ? '' : t).replace(/[\u0000-\u001f]/g, ' ').trim().slice(0, 80) || fallback;
}

/**
 * Create or update a widget's record. `placement` is Claude's request; what
 * the person did to it afterwards (meta.user) is kept and wins on screen.
 */
export function upsert(alias, id, { title, icon, slot, chat, beside, lifecycle, size, chip } = {}) {
  if (!validId(id)) return { ok: false, error: 'Widget id must be lowercase letters, digits and dashes (max 40), e.g. "build-status"' };
  let meta = readMeta(alias, id);
  const created = !meta;
  if (created) {
    if (list(alias).length >= LIMITS.perProject) return { ok: false, error: `This project already has ${LIMITS.perProject} widgets. Remove one first.` };
    meta = { id, project: projectKey(alias), createdAt: Date.now(), rev: 0, state: 'open', slot: 'cell', lifecycle: 'task', user: {} };
  }
  if (slot !== undefined) {
    if (!SLOTS.includes(slot)) return { ok: false, error: `slot must be one of: ${SLOTS.join(', ')}` };
    // Asking for a different slot is a fresh request: the person's earlier
    // move applied to the old one.
    if (slot !== meta.slot && meta.user) delete meta.user.slot;
    meta.slot = slot;
  }
  if (title !== undefined || created) meta.title = cleanTitle(title, meta.title || id);
  if (icon !== undefined) meta.icon = String(icon || '').replace(/[^a-z0-9-]/g, '').slice(0, 32);
  if (chat !== undefined) meta.chat = String(chat || '').replace(/[^0-9a-zA-Z_-]/g, '').slice(0, 64);
  if (beside !== undefined) meta.beside = ['right', 'below'].includes(beside) ? beside : '';
  if (lifecycle !== undefined) meta.lifecycle = lifecycle === 'pinned' ? 'pinned' : 'task';
  if (size && typeof size === 'object') {
    meta.size = {};
    for (const k of ['w', 'h']) { const n = Number(size[k]); if (Number.isFinite(n) && n > 0) meta.size[k] = Math.min(4000, Math.round(n)); }
  }
  if (chip !== undefined) meta.chip = chip && typeof chip === 'object' ? { source: String(chip.source || '').slice(0, 40), path: String(chip.path || '').slice(0, 120), label: String(chip.label || '').slice(0, 40) } : null;
  // Opening it again brings it back, unless the person closed it themselves.
  if (meta.state !== 'open') { meta.state = 'open'; if (meta.user) delete meta.user.closed; }
  writeMeta(alias, meta);
  return { ok: true, widget: meta, created };
}

/** Bump the document revision: clients reload the frame when it moves. */
export function touchDoc(alias, id) {
  const meta = readMeta(alias, id);
  if (!meta) return null;
  meta.rev = (meta.rev || 0) + 1;
  return writeMeta(alias, meta);
}

export function setState(alias, id, state) {
  const meta = readMeta(alias, id);
  if (!meta) return { ok: false, error: 'No such widget' };
  meta.state = state === 'closed' ? 'closed' : 'open';
  if (meta.state === 'open' && meta.user) delete meta.user.closed;
  writeMeta(alias, meta);
  return { ok: true, widget: meta };
}

/** What the person did to it on screen. Reported back to Claude. */
export function setUser(alias, id, patch = {}) {
  const meta = readMeta(alias, id);
  if (!meta) return { ok: false, error: 'No such widget' };
  meta.user = meta.user || {};
  if (patch.slot !== undefined) { if (patch.slot && SLOTS.includes(patch.slot)) meta.user.slot = patch.slot; else delete meta.user.slot; }
  if (patch.closed !== undefined) { if (patch.closed) { meta.user.closed = true; meta.state = 'closed'; } else { delete meta.user.closed; meta.state = 'open'; } }
  if (patch.collapsed !== undefined) { if (patch.collapsed) meta.user.collapsed = true; else delete meta.user.collapsed; }
  if (patch.pinned !== undefined) meta.lifecycle = patch.pinned ? 'pinned' : 'task';
  meta.user.at = Date.now();
  writeMeta(alias, meta);
  return { ok: true, widget: meta };
}

/** Where it actually shows: the person's choice over Claude's request. */
export function effectiveSlot(meta) { return (meta.user && meta.user.slot) || meta.slot || 'cell'; }

export function remove(alias, id, { keepSource = false } = {}) {
  if (!validId(id)) return { ok: false, error: 'Bad widget id' };
  const dir = stateDir(alias, id);
  if (!existsSync(dir)) return { ok: false, error: 'No such widget' };
  const src = sourceDir(alias, id);
  rmSync(dir, { recursive: true, force: true });
  if (!keepSource && src !== join(dir, 'src')) { try { rmSync(src, { recursive: true, force: true }); } catch { /* leave it */ } }
  return { ok: true };
}

// ─── Scope: project ⇄ global ───

/**
 * Move a widget between a project and the global scope, with everything it
 * has: source, pushed data, its store, versions and the owner's grant (what it
 * may do is unchanged, because its home project is).
 *
 * @param {string} from  alias it lives under now
 * @param {string} to    alias to move it to (GLOBAL for global)
 */
export function moveScope(from, id, to) {
  const meta = readMeta(from, id);
  if (!meta) return { ok: false, error: 'No such widget' };
  const fromKey = projectKey(from), toKey = projectKey(to);
  if (fromKey === toKey) return { ok: true, widget: meta, unchanged: true };
  if (toKey !== GLOBAL && fromKey !== GLOBAL) return { ok: false, error: 'A widget moves between its project and global, not between projects' };
  if (readMeta(to, id)) return { ok: false, error: `There is already a ${toKey === GLOBAL ? 'global' : 'project'} widget with the id "${id}". Rename or remove one first.` };
  if (toKey !== GLOBAL) {
    // Back to a project: only its home, or its relative paths would point elsewhere.
    if (meta.home && meta.home !== toKey) return { ok: false, error: `This widget belongs to the project "${meta.home}"; it can only go back there.` };
    if (!projectRoot(to)) return { ok: false, error: 'That project has no folder to hold the widget\'s files' };
    if (list(to).length >= LIMITS.perProject) return { ok: false, error: `That project already has ${LIMITS.perProject} widgets.` };
  } else if (list(GLOBAL).length >= LIMITS.perProject) return { ok: false, error: `There are already ${LIMITS.perProject} global widgets.` };

  const oldState = stateDir(from, id), oldSrc = sourceDir(from, id);
  const newState = stateDir(to, id), newSrc = sourceDir(to, id);

  // What moves is the widget's own folder, whole: its source and any data it
  // keeps there. Project data its sources read (anything outside that folder)
  // is the project's: it is not copied, and is read where it lies.
  const m0 = readManifest(from, id);
  const specs0 = (m0.ok && m0.manifest.sources) || {};
  const inPlace = [];
  for (const [name, spec] of Object.entries(specs0)) {
    if (!spec || (spec.kind !== 'file' && spec.kind !== 'sqlite') || !spec.path) continue;
    const r = resolveDataPath(from, spec.path, id);
    if (r.ok && !r.own) inPlace.push({ source: name, path: r.path });
  }
  try {
    ensureDir(newState);
    // State first, without any source it holds: that is copied to its new home below.
    cpSync(oldState, newState, { recursive: true, filter: (p) => p !== join(oldState, 'src') });
    if (existsSync(oldSrc)) { ensureDir(dirname(newSrc)); cpSync(oldSrc, newSrc, { recursive: true }); }
  } catch (err) {
    try { rmSync(newState, { recursive: true, force: true }); } catch { /* nothing to undo */ }
    return { ok: false, error: `Could not move the widget's files: ${err.message}` };
  }
  const moved = readJson(join(newState, 'meta.json'), meta);
  moved.project = toKey;
  if (toKey === GLOBAL) { moved.home = fromKey; moved.lifecycle = 'pinned'; }
  else delete moved.home;
  moved.updatedAt = Date.now();
  writeJson(join(newState, 'meta.json'), moved);
  // Only now is the original state let go.
  try { rmSync(oldState, { recursive: true, force: true }); } catch { /* a stray copy is harmless */ }
  // Going global, the widget's folder in the project is LEFT where it is: it
  // is a copy now, no longer read, and deleting somebody's files out of their
  // project because a panel was promoted is not ours to do. Coming back, the
  // global folder was copied over it above, so it is current again.
  const left = toKey === GLOBAL && oldSrc !== join(oldState, 'src') && existsSync(oldSrc) ? oldSrc : '';
  return { ok: true, widget: moved, left, inPlace };
}

// ─── Pushed data ───

function dataFile(alias, id) { return join(stateDir(alias, id), 'data.json'); }

export function getPushed(alias, id) { return readJson(dataFile(alias, id), {}); }

function applyPatch(base, patch) {
  // A shallow-recursive merge, with null deleting a key. Arrays replace.
  if (patch === null || typeof patch !== 'object' || Array.isArray(patch)) return patch;
  const out = base && typeof base === 'object' && !Array.isArray(base) ? { ...base } : {};
  for (const [k, v] of Object.entries(patch)) {
    if (v === null) delete out[k];
    else out[k] = applyPatch(out[k], v);
  }
  return out;
}

/**
 * Set, merge into, or append to one pushed source.
 * @param {{value?:any, merge?:any, append?:any, max?:number}} change
 */
export function push(alias, id, source, change = {}) {
  if (!readMeta(alias, id)) return { ok: false, error: 'No such widget' };
  if (!validName(source)) return { ok: false, error: 'Bad source name' };
  const all = getPushed(alias, id);
  const cur = all[source] || { value: undefined, rev: 0 };
  let value = cur.value;
  if (Object.prototype.hasOwnProperty.call(change, 'value')) value = change.value;
  if (change.merge !== undefined) value = applyPatch(value, change.merge);
  if (change.append !== undefined) {
    const add = Array.isArray(change.append) ? change.append : [change.append];
    const max = Math.max(1, Math.min(5000, Number(change.max) || 500));
    value = (Array.isArray(value) ? value : []).concat(add).slice(-max);
  }
  const body = JSON.stringify(value === undefined ? null : value);
  if (body.length > LIMITS.pushValue) return { ok: false, error: `That value is over ${LIMITS.pushValue / 1024} KB. Push a summary, or bind a file or sqlite source.` };
  all[source] = { value: value === undefined ? null : value, rev: (cur.rev || 0) + 1, at: Date.now() };
  ensureDir(stateDir(alias, id));
  writeJson(dataFile(alias, id), all);
  return { ok: true, rev: all[source].rev };
}

// ─── The widget's own store ───

function storeFile(alias, id) { return join(stateDir(alias, id), 'store.json'); }

export function storeAll(alias, id) { return readJson(storeFile(alias, id), {}); }

export function storeSet(alias, id, key, value) {
  if (!readMeta(alias, id)) return { ok: false, error: 'No such widget' };
  const k = String(key || '').slice(0, 80);
  if (!k) return { ok: false, error: 'Missing key' };
  const all = storeAll(alias, id);
  if (value === undefined || value === null) delete all[k]; else all[k] = value;
  if (JSON.stringify(all).length > LIMITS.store) return { ok: false, error: `The widget's store is full (${LIMITS.store / 1024} KB)` };
  writeJson(storeFile(alias, id), all);
  return { ok: true };
}

// ─── Events for Claude, and faults ───

function eventsFile(alias, id) { return join(stateDir(alias, id), 'events.json'); }

export function addEvent(alias, id, name, payload) {
  if (!readMeta(alias, id)) return { ok: false, error: 'No such widget' };
  const list = readJson(eventsFile(alias, id), []);
  let p = payload;
  try { if (JSON.stringify(p === undefined ? null : p).length > 8192) p = { truncated: true }; } catch { p = null; }
  list.push({ name: String(name || '').slice(0, 60), payload: p === undefined ? null : p, at: Date.now() });
  writeJson(eventsFile(alias, id), list.slice(-LIMITS.events));
  return { ok: true };
}

export function drainEvents(alias, id, { keep = false } = {}) {
  const list = readJson(eventsFile(alias, id), []);
  if (list.length && !keep) writeJson(eventsFile(alias, id), []);
  return list;
}

export function addFault(alias, id, fault) {
  const meta = readMeta(alias, id);
  if (!meta) return;
  const text = String((fault && fault.message) || fault || '').slice(0, 500);
  if (!text) return;
  meta.faults = (meta.faults || []).filter(f => f.message !== text);
  meta.faults.push({ message: text, where: String((fault && fault.where) || '').slice(0, 120), rev: meta.rev || 0, at: Date.now() });
  meta.faults = meta.faults.slice(-LIMITS.faults);
  writeMeta(alias, meta);
}

export function clearFaults(alias, id) {
  const meta = readMeta(alias, id);
  if (!meta || !(meta.faults || []).length) return;
  meta.faults = [];
  writeMeta(alias, meta);
}

// ─── Versions ───

function versionsDir(alias, id) { return join(stateDir(alias, id), 'versions'); }

/** Keep a copy of the source as it stands, if it differs from the last copy. */
export function snapshot(alias, id) {
  const meta = readMeta(alias, id);
  if (!meta) return { ok: false, error: 'No such widget' };
  const hash = sourceHash(alias, id);
  if (meta.sourceHash === hash) return { ok: true, version: meta.version || 0, changed: false };
  const h = readHtml(alias, id);
  const m = readManifest(alias, id);
  const dir = versionsDir(alias, id);
  ensureDir(dir);
  const n = (meta.version || 0) + 1;
  writeJson(join(dir, n + '.json'), { version: n, at: Date.now(), html: h.ok ? h.html : '', manifest: m.ok ? m.manifest : {} });
  // Oldest out. Rollback is for "the last few tries", not an archive.
  try {
    const have = readdirSync(dir).map(f => parseInt(f, 10)).filter(Number.isFinite).sort((a, b) => a - b);
    for (const old of have.slice(0, Math.max(0, have.length - LIMITS.versions))) rmSync(join(dir, old + '.json'), { force: true });
  } catch { /* tidy next time */ }
  meta.version = n; meta.sourceHash = hash;
  writeMeta(alias, meta);
  return { ok: true, version: n, changed: true };
}

export function listVersions(alias, id) {
  try {
    return readdirSync(versionsDir(alias, id)).map(f => parseInt(f, 10)).filter(Number.isFinite).sort((a, b) => b - a)
      .map(n => { const v = readJson(join(versionsDir(alias, id), n + '.json'), null); return v ? { version: n, at: v.at, bytes: (v.html || '').length } : null; })
      .filter(Boolean);
  } catch { return []; }
}

/** Put an earlier copy back as the source. The current one is kept as a version first. */
export function rollback(alias, id, version) {
  const v = readJson(join(versionsDir(alias, id), parseInt(version, 10) + '.json'), null);
  if (!v) return { ok: false, error: `No version ${version}` };
  snapshot(alias, id);
  const w = writeSource(alias, id, { html: v.html, manifest: v.manifest });
  if (!w.ok) return w;
  snapshot(alias, id);
  touchDoc(alias, id);
  return { ok: true, restored: v.version };
}
