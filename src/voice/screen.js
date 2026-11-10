/**
 * voice/screen.js — Cru's live screen (capability 'voice-screen').
 *
 * One panel on the voice screen that Cru fills and changes as the
 * conversation goes: things better seen than heard (lists, status,
 * comparisons, steps, a game for a child). It stays up until the owner closes
 * it (the X, POST /api/voice/screen/close) or Cru clears it.
 *
 * The state is kept on the server (a file), so a voice screen opened later
 * shows it again; every change is pushed to open voice screens as
 * { type: 'screen', screen } on /api/voice/listen.
 *
 * Shape (what GET /api/voice/screen answers as `screen`, null when nothing):
 *   {
 *     title: string,
 *     blocks: Block[],                 drawn by the client in Crundi's style
 *     html?: string,                   Cru's own page, shown in a sandboxed frame
 *     doc?: string,                    html (or a game) wrapped for that frame: CSP, kit, bridge
 *     game?: { id, options },          a built-in game (kid-games.js), in doc
 *     video?: { kind: 'video'|'playlist', id, title, embed },   play_video only
 *     full?: true,                     Cru's UI takes the whole content area (the bottom bar stays);
 *                                      the owner's top-bar button goes back to the normal view (normal())
 *     kid: boolean,                    set while kid mode was on
 *     rev: number, at: ms
 *   }
 * Block (each may carry an `id`, for update_screen):
 *   { type: 'heading', text }          { type: 'text', text }
 *   { type: 'list', items: [string], ordered? }
 *   { type: 'checklist', items: [{ text, done }] }
 *   { type: 'table', rows: [[key, value, ...]], headers? }
 *   { type: 'progress', label, value }  value 0..100
 *   { type: 'image', url, alt }        https: (not in kid mode) or data:image
 *   { type: 'chips', items: [{ text, tone }] }   tone ok|warn|bad|info|muted
 *
 * The html frame is sandbox="allow-scripts" (no same-origin) and its CSP
 * allows no network at all: only data: and blob: for pictures and sound.
 */

import { existsSync, readFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { writePrivateFile } from '../private-fs.js';

export const MAX_HTML = 200 * 1024;
const MAX_BLOCKS = 60;
const MAX_ITEMS = 200;
const MAX_TEXT = 4000;
const MAX_STATE = 400 * 1024;
export const BLOCK_TYPES = ['heading', 'text', 'list', 'checklist', 'table', 'progress', 'image', 'chips'];
const TONES = ['ok', 'warn', 'bad', 'info', 'muted'];

/** The frame's CSP: scripts and styles inline only; no network; pictures and sound from data:/blob:. */
export const SCREEN_CSP = [
  "default-src 'none'",
  "script-src 'unsafe-inline'",
  "style-src 'unsafe-inline'",
  'img-src data: blob:',
  'font-src data:',
  'media-src data: blob:',
  "connect-src 'none'",
  "frame-src 'none'",
  "form-action 'none'",
  "base-uri 'none'",
].join('; ');

const clipText = (s, n = MAX_TEXT) => String(s == null ? '' : s).slice(0, n);

/** One block, checked and trimmed; throws on what cannot be shown. */
export function cleanBlock(b, { kid = false } = {}) {
  if (!b || typeof b !== 'object') throw new Error('A block is an object with a type');
  const type = String(b.type || '');
  if (!BLOCK_TYPES.includes(type)) throw new Error(`Unknown block type "${type}". Use one of: ${BLOCK_TYPES.join(', ')}`);
  const out = { type };
  if (b.id != null && String(b.id).trim()) out.id = String(b.id).trim().slice(0, 60);
  const items = (list) => (Array.isArray(list) ? list : []).slice(0, MAX_ITEMS);
  switch (type) {
    case 'heading': case 'text': out.text = clipText(b.text); break;
    case 'list': out.items = items(b.items).map((x) => clipText(typeof x === 'object' && x ? x.text : x, 500)); if (b.ordered) out.ordered = true; break;
    case 'checklist': out.items = items(b.items).map((x) => (typeof x === 'object' && x ? { text: clipText(x.text, 500), done: !!x.done } : { text: clipText(x, 500), done: false })); break;
    case 'table': {
      out.rows = items(b.rows).map((r) => (Array.isArray(r) ? r : (r && typeof r === 'object' ? [r.key ?? r.label ?? '', r.value ?? ''] : [r])).slice(0, 8).map((c) => clipText(c, 500)));
      if (Array.isArray(b.headers) && b.headers.length) out.headers = b.headers.slice(0, 8).map((c) => clipText(c, 120));
      break;
    }
    case 'progress': {
      let v = Number(b.value);
      if (!Number.isFinite(v)) v = 0;
      if (v > 0 && v <= 1 && b.max == null && !Number.isInteger(v)) v *= 100;
      if (Number(b.max) > 0) v = (v / Number(b.max)) * 100;
      out.value = Math.max(0, Math.min(100, Math.round(v * 10) / 10));
      out.label = clipText(b.label, 200);
      break;
    }
    case 'image': {
      const url = String(b.url || '');
      const data = /^data:image\/(png|jpe?g|gif|webp|svg\+xml);/i.test(url);
      if (!data && !(kid ? false : /^https:\/\/[^\s"'<>]+$/i.test(url))) throw new Error(kid ? 'In kid mode a picture must be a data:image URL' : 'An image URL must be https: or data:image');
      if (url.length > 150 * 1024) throw new Error('That picture is too big (150 KB at most)');
      out.url = url; out.alt = clipText(b.alt, 200);
      break;
    }
    case 'chips': out.items = items(b.items).slice(0, 30).map((x) => (typeof x === 'object' && x ? { text: clipText(x.text, 80), tone: TONES.includes(x.tone) ? x.tone : 'info' } : { text: clipText(x, 80), tone: 'info' })); break;
  }
  return out;
}

/** Cru's html: a fragment or a page, capped. */
export function cleanHtml(html) {
  const h = String(html || '');
  if (Buffer.byteLength(h, 'utf8') > MAX_HTML) throw new Error(`That page is too big for the live screen (${Math.round(MAX_HTML / 1024)} KB at most)`);
  return h;
}

const escapeHtml = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

/**
 * The frame's document: the CSP first, Crundi's dark look, and a tiny bridge:
 *   cru.say(text)   the voice screen says [text] aloud at once (no turn)
 *   cru.tell(text)  [text] goes to Cru as something tapped on the screen
 * Both are messages to the parent; the parent checks they come from this frame.
 */
export function screenDoc(body, { title = '', kid = false } = {}) {
  const src = String(body || '').replace(/^﻿/, '').replace(/<!doctype[^>]*>/i, '');
  const m = /<body\b[^>]*>([\s\S]*?)<\/body\s*>/i.exec(src);
  const head = m ? ((/<head\b[^>]*>([\s\S]*?)<\/head\s*>/i.exec(src) || [])[1] || '').match(/<(style|script)\b[\s\S]*?<\/\1\s*>/gi) || [] : [];
  const inner = m ? m[1] : src.replace(/<\/?(?:html|head)\b[^>]*>/gi, '').replace(/<meta\b[^>]*>/gi, '');
  return '<!doctype html><html lang="en"><head><meta charset="utf-8">'
    + `<meta http-equiv="Content-Security-Policy" content="${SCREEN_CSP}">`
    + `<meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="${kid ? 'light' : 'dark'}">`
    + `<title>${escapeHtml(title || 'Cru')}</title>`
    + '<style>html,body{margin:0;background:transparent;color:#e8e8f0;font:15px/1.45 system-ui,-apple-system,sans-serif}body{padding:12px;box-sizing:border-box}'
    + 'a{color:#818cf8}button{font:inherit;cursor:pointer;border-radius:10px;border:1px solid #2a2a3d;background:#1a1a28;color:#e8e8f0;padding:8px 14px}'
    + 'table{border-collapse:collapse}td,th{border-bottom:1px solid #2a2a3d;padding:4px 8px;text-align:left}'
    // The kid screen is light and sunny: dark friendly text, bright buttons.
    + (kid ? 'html,body{color:#312e81;font:22px/1.35 "Comic Sans MS","Chalkboard SE",system-ui,sans-serif}a{color:#4f46e5}button{font-size:24px;padding:14px 22px;border-radius:20px;border:0;background:#6366f1;color:#fff;box-shadow:0 5px 0 rgba(49,46,129,.25)}td,th{border-color:rgba(49,46,129,.15)}' : '') + '</style>'
    + '<script>(function(){var p=window.parent;window.cru={say:function(t){p.postMessage({cru:"say",text:String(t||"").slice(0,300)},"*")},tell:function(t){p.postMessage({cru:"tell",text:String(t||"").slice(0,300)},"*")}};})();</script>'
    + head.join('\n')
    + '</head><body>' + inner + '</body></html>';
}

/**
 * The live screen's state and its changes.
 * @param {{file?: string, onChange?: (screen|null) => void, kid?: () => boolean, gameDoc?: (id, options) => {html,title}|null}} o
 */
export function createScreen({ file = '', onChange = () => {}, kid = () => false, gameDoc = null } = {}) {
  let state = null;
  let rev = 0;
  let normalAt = 0;
  try { if (file && existsSync(file)) { const s = JSON.parse(readFileSync(file, 'utf-8')); state = s && s.screen ? s.screen : null; rev = Number(s && s.rev) || 0; } } catch { state = null; }

  const persist = () => {
    if (!file) return;
    try { mkdirSync(dirname(file), { recursive: true, mode: 0o700 }); writePrivateFile(file, JSON.stringify({ rev, screen: state })); } catch { /* best effort */ }
  };
  /** As clients get it: the frame's document built from html or the game. */
  const view = () => {
    if (!state) return null;
    const v = { ...state };
    if (state.game && gameDoc) {
      const g = gameDoc(state.game.id, state.game.options || {});
      if (g) v.doc = screenDoc(g.html, { title: g.title, kid: state.kid });
    } else if (state.html) v.doc = screenDoc(state.html, { title: state.title, kid: state.kid });
    return v;
  };
  const commit = (next) => {
    if (next) {
      const size = JSON.stringify(next).length;
      if (size > MAX_STATE) throw new Error('The live screen would be too big; show less at once');
    }
    state = next ? { ...next, rev: ++rev, at: Date.now() } : null;
    if (!next) rev++;
    persist();
    try { onChange(view()); } catch { /* a client gone */ }
    return view();
  };
  const blocksOf = (list, isKid) => {
    const arr = Array.isArray(list) ? list : [];
    if (arr.length > MAX_BLOCKS) throw new Error(`At most ${MAX_BLOCKS} blocks`);
    return arr.map((b) => cleanBlock(b, { kid: isKid }));
  };

  return {
    get: view,
    /** show_screen: the whole panel at once. */
    show({ title = '', blocks = [], html = '', full = false } = {}) {
      const isKid = !!kid();
      const b = blocksOf(blocks, isKid);
      const h = cleanHtml(html);
      if (!b.length && !h.trim() && !String(title).trim()) throw new Error('Nothing to show: give a title, blocks or html');
      return commit({ title: clipText(title, 200), blocks: b, ...(h.trim() ? { html: h } : {}), ...(full === true && !isKid ? { full: true } : {}), kid: isKid });
    },
    /**
     * update_screen: a patch to what is shown.
     *   title, html ('' removes it), replace: [block with id], append: [{id, items}],
     *   add: [block], remove: [id], full: true|false (the whole content area, or back to the panel)
     */
    update({ title, html, replace = [], append = [], add = [], remove = [], full } = {}) {
      if (!state) throw new Error('Nothing is on the live screen; use show_screen first');
      if (state.video || state.game) throw new Error('A video or a game is on the screen; use show_screen (or start_game) to replace it');
      const isKid = !!kid();
      const next = { ...state, blocks: state.blocks.slice() };
      if (title !== undefined) next.title = clipText(title, 200);
      if (html !== undefined) { const h = cleanHtml(html); if (h.trim()) next.html = h; else delete next.html; }
      const find = (id) => next.blocks.findIndex((b) => b.id === String(id));
      for (const r of Array.isArray(replace) ? replace : []) {
        if (!r || r.id == null) throw new Error('A replacement block needs the id of the block it replaces');
        const i = find(r.id);
        if (i < 0) throw new Error(`No block with id "${r.id}"`);
        next.blocks[i] = cleanBlock(r, { kid: isKid });
      }
      for (const a of Array.isArray(append) ? append : []) {
        const i = find(a && a.id);
        if (i < 0) throw new Error(`No block with id "${a && a.id}"`);
        const b = next.blocks[i];
        if (!Array.isArray(b.items) && !Array.isArray(b.rows)) throw new Error(`Block "${a.id}" has no items to add to`);
        const merged = b.rows ? { ...b, rows: b.rows.concat(a.rows || a.items || []) } : { ...b, items: b.items.concat(a.items || []) };
        next.blocks[i] = cleanBlock(merged, { kid: isKid });
      }
      for (const b of Array.isArray(add) ? add : []) next.blocks.push(cleanBlock(b, { kid: isKid }));
      const gone = new Set((Array.isArray(remove) ? remove : []).map(String));
      if (gone.size) next.blocks = next.blocks.filter((b) => !gone.has(b.id));
      if (next.blocks.length > MAX_BLOCKS) throw new Error(`At most ${MAX_BLOCKS} blocks`);
      next.kid = state.kid || isKid;
      if (full !== undefined) { if (full === true && !next.kid) next.full = true; else delete next.full; }
      return commit(next);
    },
    /** A built-in game (start_game). */
    game(id, options = {}, title = '') {
      if (!gameDoc || !gameDoc(id, options)) throw new Error(`No game called "${id}"`);
      if (JSON.stringify(options || {}).length > 20000) throw new Error('Those options are too big');
      return commit({ title: clipText(title, 200), blocks: [], game: { id, options }, kid: !!kid() });
    },
    /** A video (play_video): only ever built by the server from an approved id. */
    video(v) { return commit({ title: clipText(v.title, 200), blocks: [], video: v, kid: !!kid() }); },
    /**
     * The owner's top-bar button while Cru's UI has the whole content area:
     * back to the normal view, what Cru showed kept as the usual panel.
     * Answers false when nothing was taken over.
     */
    normal() {
      if (!state || !state.full) return false;
      const next = { ...state };
      delete next.full;
      normalAt = Date.now();
      commit(next);
      return true;
    },
    /**
     * The panel's expand button (the owner, 11 Nov: once back in the normal
     * view there was no way to the full view again): the panel over the whole
     * content area. Not in kid mode, not for a video. False when nothing changed.
     */
    expand() {
      if (!state || state.full || state.kid || state.video) return false;
      commit({ ...state, full: true });
      return true;
    },
    /** When the owner last went back to the normal view (ms, 0: never). */
    get normalAt() { return normalAt; },
    /** clear_screen, the owner's X, kid mode starting or ending. */
    clear() { if (!state) return null; return commit(null); },
    get rev() { return rev; },
  };
}

/** Cru's three tools for it (safe: they change only what is on the voice screen). */
export function makeScreenTools(screen) {
  const blockSchema = {
    type: 'object',
    description: 'A block: { type: heading|text|list|checklist|table|progress|image|chips, id?, ... } — heading/text: text; list: items[string], ordered?; checklist: items[{text,done}]; table: rows[[key,value]], headers?; progress: label, value 0-100; image: url (https or data:image), alt; chips: items[{text, tone: ok|warn|bad|info|muted}]',
  };
  return [
    {
      name: 'show_screen',
      description: 'Put something on the live screen: one panel on the voice screen that stays up until the owner closes it or you clear it. For what is better seen than heard (lists, status, comparisons, steps, a table, progress). Give blocks (drawn in Crundi\'s style; give a block an id to change it later) and/or html (your own page in a sandboxed frame with no network: inline CSS and JS only, pictures as data: URLs; up to 200 KB; window.cru.say(text) speaks, window.cru.tell(text) sends you what was tapped). Replaces what was there. full: true takes over the whole content area for your own UI (the owner can always go back to the normal view). Keep the spoken reply short; the screen carries the detail.',
      input_schema: {
        type: 'object', additionalProperties: false,
        properties: {
          title: { type: 'string', description: 'The panel\'s title' },
          blocks: { type: 'array', items: blockSchema },
          html: { type: 'string', description: 'Optional: your own page (fragment or whole document)' },
          full: { type: 'boolean', description: 'true: take over the whole content area of the Cru screen (the conversation and the orb make way; the bottom bar to type or speak stays). For your own UI that needs the room: a dashboard, a board, a game, a form, a walkthrough. The owner can go back to the normal view at any time with the button on the top bar; what you showed then stays as the usual panel. Not in kid mode (the kid screen is already whole).' },
        },
      },
      run: (input = {}) => {
        const s = screen.show(input);
        return { ok: true, shown: true, rev: s.rev, blocks: s.blocks.length, html: !!s.html, ...(s.full ? { full: true, note: 'The owner can leave the full view at any time from the top bar; your spoken or written reply still reaches him under it.' } : {}) };
      },
    },
    {
      name: 'update_screen',
      description: 'Change what is on the live screen without redrawing it all: a new title, replace blocks by id, append items to a list/checklist/table block by id, add blocks at the end, remove blocks by id, or replace the html (\'\' removes it).',
      input_schema: {
        type: 'object', additionalProperties: false,
        properties: {
          title: { type: 'string' },
          html: { type: 'string' },
          replace: { type: 'array', items: blockSchema, description: 'Blocks that replace the block with the same id' },
          append: { type: 'array', items: { type: 'object', properties: { id: { type: 'string' }, items: { type: 'array', items: {} }, rows: { type: 'array', items: {} } }, required: ['id'] }, description: 'Items (or table rows) to add to the block with that id' },
          add: { type: 'array', items: blockSchema, description: 'New blocks at the end' },
          remove: { type: 'array', items: { type: 'string' }, description: 'Ids of blocks to remove' },
          full: { type: 'boolean', description: 'true: the whole content area (as show_screen full); false: back to the normal panel. Leave it out to keep it as it is. If the owner went back to the normal view himself, take it over again only when he asks.' },
        },
      },
      run: (input = {}) => { const s = screen.update(input); return { ok: true, rev: s.rev, blocks: s.blocks.length, full: !!s.full }; },
    },
    {
      name: 'clear_screen',
      description: 'Take the live screen panel away (when what it shows is no longer useful, or the owner says so).',
      input_schema: { type: 'object', additionalProperties: false, properties: {} },
      run: () => { screen.clear(); return { ok: true, cleared: true }; },
    },
  ];
}

export const SCREEN_TOOL_NAMES = ['show_screen', 'update_screen', 'clear_screen'];
