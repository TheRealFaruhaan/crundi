/**
 * voice/kid-games.js — the kid games library (conventions: kid-games/README.md).
 *
 *   <dataDir>/kid-games/<id>/index.html     the game start_game loads
 *                            index.prev.html the version before the last change
 *                            meta.json       id, title, skills, ageRange, version, createdBy, enabled …
 *   <dataDir>/kid-games/plays.jsonl          one line per game played
 *   <dataDir>/kid-games/trivia.extra.json    questions and facts the workshop added
 *
 * Built-in games (src/voice/kid-games/builtin, catalog.json) are seeded into
 * the library and refreshed when a newer one ships, unless the workshop has
 * changed that game. Every game runs in the live screen's sandbox, with the
 * kit (kit.js), its options, what the child knows (init) and, for games that
 * use it, the trivia bank put in front of it.
 *
 * validateGame refuses anything that could reach the network or break the
 * frame; the workshop's changes go live only after it (afterWorkshop).
 */

import { existsSync, readFileSync, readdirSync, mkdirSync, copyFileSync, appendFileSync, statSync, renameSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { Script } from 'node:vm';
import { writePrivateFile } from '../private-fs.js';
import { SKILL_IDS } from './kid-skills.js';

const HERE = dirname(fileURLToPath(import.meta.url));
export const BUILTIN_DIR = join(HERE, 'kid-games');
export const MAX_GAME = 200 * 1024;
/** The waiting scene's id (kid-games/waiting.html): for any wait in kid mode. */
export const WAITING_ID = '_waiting';
const ID_RE = /^[a-z0-9][a-z0-9-]{1,40}$/;

const readJson = (f, dflt) => { try { return JSON.parse(readFileSync(f, 'utf-8')); } catch { return dflt; } };
const sha = (s) => createHash('sha256').update(String(s)).digest('hex').slice(0, 16);
const iso = (t = Date.now()) => new Date(t).toISOString();

/** The picture names the kit can draw (kid.pic). */
export function kitPictures(kit = readFileSync(join(BUILTIN_DIR, 'kit.js'), 'utf-8')) {
  return new Set([...kit.matchAll(/^\s+'?([a-z-]+)'?: S\(/gm)].map((m) => m[1]));
}

/**
 * The checks a game must pass before it goes live. Static: no way out to the
 * network, no storage, scripts that parse, within the size cap.
 * @returns {{ok:boolean, problems:string[]}}
 */
/** [h] without HTML, CSS and JS comments (the network checks look at what runs, not at what is said about it). */
export function stripComments(h) {
  return String(h || '')
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    // A // comment: at a line start or after whitespace, ; { } ( ) , (never the // of a URL, which follows a colon).
    .replace(/(^|[\s;{}(),>])\/\/[^\n]*/g, '$1');
}

export function validateGame(html) {
  const problems = [];
  const h = String(html || '');
  if (!h.trim()) problems.push('The file is empty');
  if (Buffer.byteLength(h, 'utf8') > MAX_GAME) problems.push(`Bigger than ${MAX_GAME / 1024} KB`);
  const code = stripComments(h);
  // Only the SVG (and XLink/XHTML) namespaces may be written as URLs.
  const urls = (code.match(/\b(?:https?|wss?|ftp):\/\/[^\s"'<>)]*/gi) || []).filter((u) => !/^http:\/\/www\.w3\.org\/(2000\/svg|1999\/xlink|1999\/xhtml)\b/.test(u));
  if (urls.length) problems.push(`Network address: ${urls[0].slice(0, 80)}`);
  const rules = [
    [/\bfetch\s*\(/, 'fetch()'], [/\bXMLHttpRequest\b/, 'XMLHttpRequest'], [/\bWebSocket\s*\(/, 'WebSocket'], [/\bEventSource\s*\(/, 'EventSource'],
    [/\bsendBeacon\s*\(/, 'sendBeacon'], [/\bimport\s*\(/, 'import()'], [/^\s*import\s+[\w{*'"]/m, 'import'], [/\bimportScripts\s*\(/, 'importScripts'],
    [/\bnew\s+(?:Shared)?Worker\s*\(|navigator\.serviceWorker/, 'workers'], [/\bRTCPeerConnection\b/, 'WebRTC'],
    [/<script\b[^>]*\bsrc\s*=/i, '<script src>'], [/<link\b[^>]*\bhref\s*=/i, '<link href>'], [/<(iframe|frame|object|embed|base|form)\b/i, 'iframe/object/embed/base/form'],
    [/<meta\b[^>]*http-equiv/i, '<meta http-equiv>'],
    [/\b(?:src|href|action|poster|srcset)\s*=\s*["']?\s*(?!data:|#|blob:)[a-z][a-z0-9+.-]*:/i, 'a link to another place'],
    [/\b(?:src|href|action|poster)\s*=\s*["']?\s*\/\//i, 'a protocol-relative link'],
    [/url\(\s*(?!["']?\s*(?:data:|blob:|#))/i, 'url() other than data:'], [/@import\b/i, '@import'],
    [/\beval\s*\(|\bnew\s+Function\s*\(|setTimeout\s*\(\s*["'`]/, 'eval / new Function'],
    [/\b(?:localStorage|sessionStorage|indexedDB|document\.cookie)\b/, 'storage'],
    [/\bwindow\.open\s*\(|\b(?:top|parent)\.location\b|\blocation\s*=[^=]|location\.(?:href|assign|replace)\b/, 'navigation'],
    [/javascript:/i, 'javascript: URL'],
  ];
  for (const [re, what] of rules) if (re.test(code)) problems.push(`Not allowed: ${what}`);
  // Each inline script must parse.
  for (const m of h.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script\s*>/gi)) {
    try { new Script(m[1], { filename: 'game.js' }); } catch (e) { problems.push(`Script error: ${String(e.message).slice(0, 160)}`); }
  }
  if ((code.match(/<script\b/gi) || []).length !== (code.match(/<\/script\s*>/gi) || []).length) problems.push('A <script> is not closed');
  return { ok: problems.length === 0, problems };
}

/** The workshop's extra questions and facts, checked; bad ones are dropped (and listed). */
export function validateBankItems(items, pictures = kitPictures()) {
  const ok = [], problems = [];
  const areas = new Set(SKILL_IDS.filter((s) => s.startsWith('world.')));
  const clean = (s, n) => typeof s === 'string' && s.trim() && s.length <= n && !/https?:|www\.|<|>|\b\d{4,}\b/.test(s);
  for (const it of Array.isArray(items) ? items : []) {
    const why = !it || typeof it !== 'object' ? 'not an object'
      : !areas.has(it.area) ? `unknown area ${it && it.area}`
      : !['name', 'sound', 'trivia', 'fact'].includes(it.kind) ? 'unknown kind'
      : it.kind === 'fact' ? (clean(it.fact, 160) ? '' : 'a fact is plain text up to 160 characters')
      : !clean(it.q, 120) ? 'a question is plain text up to 120 characters'
      : !Array.isArray(it.choices) || it.choices.length < 2 || it.choices.length > 4 ? 'two to four choices'
      : !it.choices.includes(it.answer) ? 'the answer must be one of the choices'
      : it.choices.some((c) => !pictures.has(c)) ? 'every choice must be a picture the kit can draw'
      : it.fact != null && !clean(it.fact, 160) ? 'a fact is plain text up to 160 characters' : '';
    if (why) problems.push(`${(it && it.id) || '?'}: ${why}`); else ok.push({ ...it, id: String(it.id || '').slice(0, 60) || `extra-${ok.length}` });
  }
  return { items: ok.slice(0, 2000), problems };
}

/**
 * @param {{dir: string, builtinDir?: string, initFor?: (skills) => object, onPlay?: (play) => void, now?: () => number}} o
 */
export function createGameLibrary({ dir, builtinDir = BUILTIN_DIR, initFor = () => ({ type: 'init', levels: {}, mastery: {}, focus: {}, avoid: {} }), onPlay = () => {}, now = () => Date.now() } = {}) {
  const kit = readFileSync(join(builtinDir, 'kit.js'), 'utf-8');
  const catalog = readJson(join(builtinDir, 'catalog.json'), []);
  const playsFile = join(dir, 'plays.jsonl');
  mkdirSync(dir, { recursive: true, mode: 0o700 });

  const gameDir = (id) => join(dir, id);
  const metaOf = (id) => readJson(join(gameDir(id), 'meta.json'), null);
  const writeMeta = (id, m) => writePrivateFile(join(gameDir(id), 'meta.json'), JSON.stringify(m, null, 1));

  /** Built-ins into the library: new ones copied, untouched ones refreshed when Crundi ships a newer version. */
  function seed() {
    for (const c of catalog) {
      if (!ID_RE.test(c.id)) continue;
      const src = join(builtinDir, 'builtin', `${c.id}.html`);
      if (!existsSync(src)) continue;
      const m = metaOf(c.id);
      const d = gameDir(c.id);
      if (!m || !existsSync(join(d, 'index.html'))) {
        mkdirSync(d, { recursive: true, mode: 0o700 });
        copyFileSync(src, join(d, 'index.html'));
        writeMeta(c.id, { id: c.id, title: c.title, skills: c.skills || [], ageRange: c.ageRange || '', version: c.version || 1, builtinVersion: c.version || 1, createdBy: 'builtin', createdAt: iso(now()), updatedAt: iso(now()), enabled: true, options: c.options || '', uses: c.uses || [] });
      } else if (m.createdBy === 'builtin' && !m.changedByWorkshop && (m.builtinVersion || 0) < (c.version || 1)) {
        copyFileSync(join(d, 'index.html'), join(d, 'index.prev.html'));
        copyFileSync(src, join(d, 'index.html'));
        writeMeta(c.id, { ...m, title: c.title, skills: c.skills || m.skills, ageRange: c.ageRange || m.ageRange, version: (m.version || 1) + 1, builtinVersion: c.version, updatedAt: iso(now()), options: c.options || m.options, uses: c.uses || m.uses || [] });
      }
    }
  }
  seed();

  function list() {
    const out = [];
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (!e.isDirectory() || !ID_RE.test(e.name)) continue;
      const m = metaOf(e.name);
      if (m && existsSync(join(gameDir(e.name), 'index.html'))) out.push({ ...m, id: e.name, hasPrev: existsSync(join(gameDir(e.name), 'index.prev.html')) });
    }
    return out.sort((a, b) => String(a.title).localeCompare(String(b.title)));
  }
  const get = (id) => (ID_RE.test(String(id)) ? list().find((g) => g.id === id) || null : null);

  /** The trivia bank: Crundi's own, plus what the workshop added (checked). */
  function bank() {
    const base = readJson(join(builtinDir, 'trivia.json'), { items: [] }).items || [];
    const extra = readJson(join(dir, 'trivia.extra.json'), { items: [] });
    const v = validateBankItems(Array.isArray(extra) ? extra : extra.items || []);
    return { items: base.concat(v.items) };
  }

  const json = (v) => JSON.stringify(v).replace(/</g, '\\u003c').replace(/[\u2028\u2029]/g, (c) => '\\u' + c.charCodeAt(0).toString(16));
  /** What the live screen's frame gets for a game: kit, options, init, bank, then the game. */
  function doc(id, options = {}) {
    // The waiting scene (waiting.html): never in the list of games, always there.
    if (id === WAITING_ID) {
      const html = readFileSync(join(builtinDir, 'waiting.html'), 'utf-8');
      return { title: String(options.title || 'Just a moment'), html: `<script>window.CRU_WAITING=true;window.CRU_OPTIONS=${json(options || {})};window.CRU_INIT=${json(initFor([]))};</script><script>${kit}</script>` + html };
    }
    const g = get(id);
    if (!g || g.enabled === false) return null;
    let html;
    try { html = readFileSync(join(gameDir(id), 'index.html'), 'utf-8'); } catch { return null; }
    const init = initFor(g.skills || []);
    const prelude = `<script>window.CRU_OPTIONS=${json(options || {})};window.CRU_INIT=${json(init)};${(g.uses || []).includes('trivia') ? `window.CRU_BANK=${json(bank())};` : ''}</script><script>${kit}</script>`;
    const body = /<body\b[^>]*>/i.exec(html);
    return { title: g.title, html: body ? html.slice(0, body.index + body[0].length) + prelude + html.slice(body.index + body[0].length) : prelude + html };
  }

  // ─── the play log ───
  let play = null;
  function startPlay(id, options = {}) {
    endPlay(false);
    play = { game: id, startedAt: now(), events: { tap: 0, correct: 0, wrong: 0, hint: 0, level: 0, finished: 0, result: 0 }, wrongs: [], options };
  }
  /** An event from the game frame ({ kind, detail, skill?, item?, ok?, ms? }). */
  function event(e = {}) {
    if (!play) return;
    const k = String(e.kind || 'tap');
    if (k in play.events) play.events[k]++;
    if (k === 'wrong' && e.detail && play.wrongs.length < 30) play.wrongs.push(String(e.detail).slice(0, 30));
    if (k === 'finished') endPlay(true);
  }
  function endPlay(finished) {
    if (!play) return null;
    const p = play; play = null;
    const line = { game: p.game, startedAt: iso(p.startedAt), seconds: Math.round((now() - p.startedAt) / 1000), finished: !!finished, events: p.events, wrongs: p.wrongs, options: p.options };
    try { appendFileSync(playsFile, JSON.stringify(line) + '\n', { mode: 0o600 }); } catch { /* best effort */ }
    try { onPlay(line); } catch { /* best effort */ }
    return line;
  }
  function plays({ since = 0 } = {}) {
    let out = [];
    try { out = readFileSync(playsFile, 'utf-8').split('\n').filter(Boolean).map((l) => JSON.parse(l)); } catch { /* none */ }
    return out.filter((p) => Date.parse(p.startedAt) >= since);
  }

  // ─── the parent's controls ───
  function setEnabled(id, on) {
    const m = metaOf(id);
    if (!m) return { ok: false, error: 'No such game' };
    if (on && m.validation && m.validation.ok === false) return { ok: false, error: 'This version failed its checks; roll it back instead' };
    writeMeta(id, { ...m, enabled: !!on, updatedAt: iso(now()) });
    return { ok: true };
  }
  function rollback(id) {
    const d = gameDir(id), prev = join(d, 'index.prev.html');
    const m = metaOf(id);
    if (!m || !existsSync(prev)) return { ok: false, error: 'There is no earlier version' };
    const v = validateGame(readFileSync(prev, 'utf-8'));
    if (!v.ok) return { ok: false, error: `The earlier version fails its checks: ${v.problems[0]}` };
    renameSync(join(d, 'index.html'), join(d, 'index.rolled-back.html'));
    renameSync(prev, join(d, 'index.html'));
    writeMeta(id, { ...m, version: Math.max(1, (m.version || 1) - 1), enabled: true, rolledBackAt: iso(now()), updatedAt: iso(now()), validation: { ok: true, at: iso(now()), problems: [] } });
    return { ok: true };
  }

  // ─── the workshop's before and after ───
  /** Before a run: each game's hash, and a copy of what is live now. */
  function snapshot() {
    const snap = {};
    for (const g of list()) {
      const f = join(gameDir(g.id), 'index.html');
      const html = readFileSync(f, 'utf-8');
      snap[g.id] = { hash: sha(html), version: g.version || 1 };
      copyFileSync(f, join(gameDir(g.id), 'index.live.html'));
    }
    snap['trivia.extra.json'] = { hash: existsSync(join(dir, 'trivia.extra.json')) ? sha(readFileSync(join(dir, 'trivia.extra.json'), 'utf-8')) : '' };
    return snap;
  }
  /**
   * After a run: every new or changed game is checked (and, with [render],
   * drawn in the sandbox); a failure puts the live version back (or leaves a
   * new game off). Changed games keep their previous version as index.prev.html.
   * @returns {Promise<{updated:string[], created:string[], failed:{id, problems}[], bank?:{added, dropped}}>}
   */
  async function afterWorkshop(snap, { render = null } = {}) {
    const res = { updated: [], created: [], failed: [] };
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (!e.isDirectory()) continue;
      const id = e.name, d = gameDir(id), f = join(d, 'index.html');
      if (!existsSync(f)) continue;
      if (!ID_RE.test(id)) continue;
      const html = readFileSync(f, 'utf-8');
      const was = snap[id];
      const live = join(d, 'index.live.html');
      if (was && was.hash === sha(html)) { if (existsSync(live)) rmSync(live); continue; }
      let m = readJson(join(d, 'meta.json'), null) || {};
      const v = validateGame(html);
      if (v.ok && render) {
        try { const r = await render(id); if (r && r.ok === false) v.problems.push(...(r.problems || [r.error || 'did not render'])); } catch (err) { v.problems.push(`render: ${err.message}`); }
        v.ok = v.problems.length === 0;
      }
      const skills = (Array.isArray(m.skills) ? m.skills : []).filter((s) => SKILL_IDS.includes(s));
      if (was) {
        if (v.ok) {
          if (existsSync(live)) renameSync(live, join(d, 'index.prev.html'));
          writeMeta(id, { ...m, id, skills: skills.length ? skills : m.skills, version: Math.max((was.version || 1) + 1, Number(m.version) || 0), changedByWorkshop: true, updatedAt: iso(now()), enabled: m.enabled !== false, validation: { ok: true, at: iso(now()), problems: [] } });
          res.updated.push(id);
        } else {
          // The new version stays on disk for a look, the live one is put back.
          renameSync(f, join(d, 'index.failed.html'));
          if (existsSync(live)) renameSync(live, f);
          writeMeta(id, { ...(readJson(join(d, 'meta.json'), {}) || {}), id, lastFailed: { at: iso(now()), problems: v.problems.slice(0, 10) } });
          res.failed.push({ id, problems: v.problems });
        }
      } else {
        m = { title: String(m.title || id).slice(0, 60), skills, ageRange: String(m.ageRange || ''), options: String(m.options || '').slice(0, 300), uses: Array.isArray(m.uses) ? m.uses.filter((u) => u === 'trivia') : [] };
        writeMeta(id, { ...m, id, version: 1, createdBy: 'workshop', createdAt: iso(now()), updatedAt: iso(now()), enabled: v.ok, isNew: true, validation: { ok: v.ok, at: iso(now()), problems: v.problems.slice(0, 10) } });
        (v.ok ? res.created : res.failed).push(v.ok ? id : { id, problems: v.problems });
      }
    }
    for (const g of list()) { const live = join(gameDir(g.id), 'index.live.html'); if (existsSync(live)) rmSync(live); }
    // The bank: what fails is dropped from the file, so the next run sees why.
    const bf = join(dir, 'trivia.extra.json');
    if (existsSync(bf) && (snap['trivia.extra.json'] || {}).hash !== sha(readFileSync(bf, 'utf-8'))) {
      const raw = readJson(bf, null);
      const v = validateBankItems(raw && (Array.isArray(raw) ? raw : raw.items));
      writePrivateFile(bf, JSON.stringify({ items: v.items }, null, 0));
      res.bank = { kept: v.items.length, dropped: v.problems.slice(0, 20) };
    }
    return res;
  }
  /** The parent has seen the new games (Settings): they stop being "new". */
  function markSeen() { for (const g of list()) if (g.isNew) writeMeta(g.id, { ...metaOf(g.id), isNew: false }); }

  return { dir, list, get, has: (id) => !!get(id), doc, bank, startPlay, event, endPlay, plays, setEnabled, rollback, snapshot, afterWorkshop, markSeen, seed, get playing() { return play; } };
}
