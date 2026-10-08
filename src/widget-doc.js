/**
 * widget-doc.js — turns a widget's source into the document its frame loads,
 * and builds the harness page Claude's own test renders go through.
 *
 * The frame is `sandbox="allow-scripts"` with no allow-same-origin, so it has
 * an opaque origin: it cannot read Crundi's localStorage (where the session
 * tokens live), cannot call /api, cannot navigate the top window. The CSP
 * below then removes the network altogether. What is left is a page that can
 * draw, and talk to its host by postMessage. That is the whole security model,
 * and it is why the document is assembled here rather than served as a file.
 */

import { readFileSync, statSync } from 'node:fs';
import { join, resolve as resolvePath, sep, extname, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as store from './widget-store.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const RUNTIME_DIR = join(__dirname, 'widget-runtime');

export const CSP = [
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

/** The sandbox tokens every widget frame gets. Never add allow-same-origin. */
export const SANDBOX = 'allow-scripts';

/**
 * Permissions-policy features handed to the frame. Audio only: without it a
 * panel cannot make a sound until it is touched, again after every reload.
 * Not a sandbox token; it grants no network, storage or origin.
 */
export const FRAME_ALLOW = 'autoplay';

// Re-read when the file changes, so editing the kit does not need a restart.
const fileCache = new Map();
function runtimeFile(name) {
  const p = join(RUNTIME_DIR, name);
  let mtime = 0;
  try { mtime = statSync(p).mtimeMs; } catch { return ''; }
  const hit = fileCache.get(name);
  if (hit && hit.mtime === mtime) return hit.text;
  const text = readFileSync(p, 'utf8');
  fileCache.set(name, { mtime, text });
  return text;
}

// ─── Design tokens, taken from the app itself so they cannot drift ───

let appCss = null;
async function loadAppCss() {
  if (appCss) return appCss;
  let page = '';
  try {
    const { getWebappHtml } = await import('./webapp-html.js');
    page = getWebappHtml('');
  } catch { /* fall through to the fallback tokens */ }
  const style = /<style>([\s\S]*?)<\/style>/.exec(page);
  const css = style ? style[1] : '';
  const root = /:root\s*\{([^}]*)\}/.exec(css);
  appCss = {
    all: css,
    tokens: root ? root[1].replace(/\/\*[\s\S]*?\*\//g, '').trim() : FALLBACK_TOKENS,
  };
  return appCss;
}

const FALLBACK_TOKENS = `--bg-primary:#0a0a0f;--bg-secondary:#12121a;--bg-tertiary:#1a1a28;--bg-hover:#22223a;--bg-card:#14141f;
--border:#2a2a3d;--border-subtle:#1e1e30;--text-primary:#e8e8f0;--text-secondary:#8888a8;--text-muted:#5a5a78;
--accent:#6366f1;--accent-hover:#818cf8;--accent-dim:rgba(99,102,241,0.15);--green:#10b981;--green-dim:rgba(16,185,129,0.15);
--red:#ef4444;--red-dim:rgba(239,68,68,0.15);--yellow:#f59e0b;--yellow-dim:rgba(245,158,11,0.15);--sky:#38bdf8;
--radius:10px;--radius-sm:6px;--mono:"Cascadia Code","Fira Code","JetBrains Mono","Consolas",monospace;
--accent-grad:linear-gradient(135deg,#6366f1 0%,#818cf8 100%);--shadow-sm:0 1px 2px rgba(0,0,0,0.45);
--shadow-md:0 6px 20px rgba(0,0,0,0.5);--surface-hi:inset 0 1px 0 rgba(255,255,255,0.045);--ring:0 0 0 3px rgba(99,102,241,0.28);`;

/** Token names and values, for the authoring guide. */
export async function tokenList() {
  const { tokens } = await loadAppCss();
  const out = [];
  for (const m of tokens.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/gi)) out.push({ name: m[1], value: m[2].trim() });
  return out;
}

// ─── Source → frame document ───

const MIME = {
  '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.webp': 'image/webp',
  '.svg': 'image/svg+xml', '.ico': 'image/x-icon', '.woff2': 'font/woff2', '.woff': 'font/woff',
};

const isRemote = (u) => /^(?:[a-z][a-z0-9+.-]*:)?\/\//i.test(u);
const isInlineUrl = (u) => /^(data:|blob:|#|about:)/i.test(u);

/** Read a file from the widget's own folder, refusing anything outside it. */
function readLocal(dir, ref, budget) {
  const clean = String(ref || '').split(/[?#]/)[0];
  if (!clean || clean.startsWith('/')) return null;
  const abs = resolvePath(dir, clean);
  const root = dir.endsWith(sep) ? dir : dir + sep;
  if (!abs.startsWith(root)) return null;
  try {
    const st = statSync(abs);
    if (!st.isFile() || st.size > store.LIMITS.asset) return { tooBig: true };
    if (budget.used + st.size > store.LIMITS.assetsTotal) return { tooBig: true };
    budget.used += st.size;
    return { buf: readFileSync(abs), ext: extname(abs).toLowerCase() };
  } catch { return null; }
}

/** Pull local scripts, stylesheets and images into the document itself. */
function inlineAssets(html, dir, problems) {
  const budget = { used: 0 };
  const note = (ref, why) => { if (problems.length < 12) problems.push(`${ref}: ${why}`); };
  const blocked = (ref) => note(ref, 'blocked. Widgets have no network; put the file in the widget folder and reference it relatively.');

  let out = html.replace(/<script\b([^>]*?)\bsrc\s*=\s*(["'])(.*?)\2([^>]*)>\s*<\/script>/gi, (m, pre, _q, src, post) => {
    if (isRemote(src)) { blocked(src); return `<!-- blocked: ${src.replace(/--/g, '')} -->`; }
    const f = readLocal(dir, src, budget);
    if (!f) { note(src, 'not found in the widget folder'); return ''; }
    if (f.tooBig) { note(src, 'too large to inline'); return ''; }
    const attrs = (pre + ' ' + post).replace(/\b(type|defer|async|crossorigin|integrity)\s*=\s*(["']).*?\2|\b(defer|async)\b/gi, '').trim();
    return `<script${attrs ? ' ' + attrs : ''}>${f.buf.toString('utf8').replace(/<\/script/gi, '<\\/script')}</script>`;
  });

  out = out.replace(/<link\b[^>]*?>/gi, (m) => {
    const href = (/\bhref\s*=\s*(["'])(.*?)\1/i.exec(m) || [])[2] || '';
    if (!/\brel\s*=\s*(["'])?stylesheet/i.test(m)) return isRemote(href) ? (blocked(href), '') : m;
    if (isRemote(href)) { blocked(href); return ''; }
    const f = readLocal(dir, href, budget);
    if (!f) { note(href, 'not found in the widget folder'); return ''; }
    if (f.tooBig) { note(href, 'too large to inline'); return ''; }
    return `<style>${f.buf.toString('utf8').replace(/<\/style/gi, '<\\/style')}</style>`;
  });

  out = out.replace(/(<(?:img|source|video|audio|image|use)\b[^>]*?\s)(src|href|xlink:href|poster)\s*=\s*(["'])(.*?)\3/gi, (m, head, attr, q, ref) => {
    if (!ref || isInlineUrl(ref)) return m;
    if (isRemote(ref)) { blocked(ref); return `${head}${attr}=${q}${q}`; }
    const f = readLocal(dir, ref, budget);
    if (!f) { note(ref, 'not found in the widget folder'); return m; }
    if (f.tooBig) { note(ref, 'too large to inline (512 KB each, 2 MB in total)'); return m; }
    const mime = MIME[f.ext];
    if (!mime) { note(ref, 'unsupported file type'); return m; }
    return `${head}${attr}=${q}data:${mime};base64,${f.buf.toString('base64')}${q}`;
  });

  // url(...) in CSS and remote @import are the other ways out.
  out = out.replace(/@import\s+(?:url\()?\s*["']?(https?:)?\/\/[^;]+;/gi, (m) => { blocked(m.slice(0, 80)); return ''; });
  return out;
}

/** Accept either a fragment or a whole page; return what goes in <head> and <body>. */
function splitAuthorHtml(html) {
  const src = String(html || '').replace(/^\uFEFF/, '').replace(/<!doctype[^>]*>/i, '');
  const body = /<body\b[^>]*>([\s\S]*?)<\/body\s*>/i.exec(src);
  if (!body) return { head: '', body: src.replace(/<\/?(?:html|head)\b[^>]*>/gi, '') };
  const head = /<head\b[^>]*>([\s\S]*?)<\/head\s*>/i.exec(src);
  // Keep the author's styles and scripts from <head>; drop titles and metas,
  // which mean nothing in a frame and could try to restate the CSP.
  const keep = head ? (head[1].match(/<(style|script)\b[\s\S]*?<\/\1\s*>|<link\b[^>]*>/gi) || []).join('\n') : '';
  return { head: keep, body: body[1] };
}

const jsonForScript = (v) => JSON.stringify(v).replace(/</g, '\\u003c').replace(/[\u2028\u2029]/g, (c) => '\\u' + c.charCodeAt(0).toString(16));

function placeholder(title, message) {
  return `<div class="c-empty"><div class="c-title">${escapeHtml(title)}</div><div>${escapeHtml(message)}</div></div>`;
}

function escapeHtml(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

/**
 * The complete document for a widget's frame.
 * @returns {Promise<{ok:boolean, html:string, title:string, manifest:object, problems:string[], error?:string}>}
 */
export async function buildDoc(alias, id, { harness = false, chip = false } = {}) {
  const meta = store.get(alias, id);
  const problems = [];
  const m = store.readManifest(alias, id);
  if (!m.ok) problems.push(m.error);
  const manifest = m.ok ? m.manifest : {};
  const title = (meta && meta.title) || manifest.title || id;
  const h = store.readHtml(alias, id);
  let head = '', body = '';
  if (h.ok) {
    const parts = splitAuthorHtml(h.html);
    const dir = store.sourceDir(alias, id);
    head = inlineAssets(parts.head, dir, problems);
    body = inlineAssets(parts.body, dir, problems);
  } else {
    problems.push(h.error);
    body = placeholder(title, 'This widget has no index.html yet.');
  }
  const { tokens } = await loadAppCss();
  const boot = { id, title, harness: !!harness, chip: !!chip };
  const html = '<!doctype html><html lang="en"' + (chip ? ' data-frame="chipbar"' : '') + '><head><meta charset="utf-8">'
    + `<meta http-equiv="Content-Security-Policy" content="${CSP}">`
    + '<meta name="viewport" content="width=device-width, initial-scale=1">'
    // Without this the frame's canvas is opaque white wherever the body is
    // transparent (inline cards), whatever the page around it looks like.
    + '<meta name="color-scheme" content="dark">'
    + `<title>${escapeHtml(title)}</title>`
    + `<style data-crundi>:root{${tokens}}\n${runtimeFile('kit.css')}</style>`
    + `<script data-crundi>window.__CRUNDI_BOOT__=${jsonForScript(boot)};</script>`
    + `<script data-crundi>${runtimeFile('runtime.js')}</script>`
    + (harness ? `<script data-crundi>${runtimeFile('lint.js')}</script>` : '')
    + head
    + '</head><body>' + body + '</body></html>';
  return { ok: h.ok && m.ok, html, title, manifest, problems, error: h.ok ? (m.ok ? '' : m.error) : h.error };
}

// ─── The harness: a widget inside Crundi's real chrome, for a headless render ───

/**
 * Named frames. Width and height are the browser viewport; `mobile` turns on
 * touch, a phone user agent and 2x density in the headless browser.
 */
export const FRAMES = {
  cell: { width: 560, height: 460, slot: 'cell', label: 'workbench cell (desktop)' },
  dock: { width: 420, height: 620, slot: 'dock', label: 'docked above a chat (desktop, narrow cell)' },
  'dock-wide': { width: 1100, height: 620, slot: 'dock', label: 'docked beside a chat (desktop, wide cell)' },
  tab: { width: 1280, height: 760, slot: 'tab', label: 'full tab (desktop)' },
  inline: { width: 560, height: 420, slot: 'inline', label: 'inline card in the chat' },
  chip: { width: 520, height: 96, slot: 'chipbar', label: 'top bar chip: only the [data-chip] element, one line' },
  'mobile-chip': { width: 390, height: 150, slot: 'chipbar', mobile: true, label: 'the chip on a phone: a row in the list that drops from the top bar, the face beside the name, at most 200px wide' },
  mobile: { width: 390, height: 844, slot: 'cell', mobile: true, label: 'phone, workbench cell' },
  'mobile-dock': { width: 390, height: 844, slot: 'dock', mobile: true, label: 'phone, docked above a chat' },
  'mobile-tab': { width: 390, height: 844, slot: 'tab', mobile: true, label: 'phone, full tab' },
};

const HARNESS_CSS = `
  html, body { height: 100%; }
  body { display: flex; flex-direction: column; }
  .hz-main { flex: 1; min-height: 0; display: flex; padding: 8px; gap: 8px; }
  .hz-main.flush { padding: 0; }
  .hz-cell { flex: 1 1 0; min-width: 0 !important; }
  .hz-frame { border: 0; width: 100%; height: 100%; display: block; background: var(--bg-primary); }
  .hz-chat { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .hz-chat.side { flex-direction: row; }
  .hz-dock { flex: 0 0 auto; max-height: 46%; border-bottom: 1px solid var(--border); background: var(--bg-primary); display: flex; flex-direction: column; min-height: 0; }
  .hz-chat.side .hz-dock { order: 2; flex: 0 0 380px; max-height: none; border-bottom: 0; border-left: 1px solid var(--border); }
  .hz-dock-head { display: flex; align-items: center; gap: 6px; padding: 4px 8px; font-size: 11px; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase; color: var(--text-secondary); background: var(--bg-secondary); border-bottom: 1px solid var(--border-subtle); flex-shrink: 0; }
  .hz-dock-body { flex: 1 1 auto; min-height: 0; overflow: auto; }
  .hz-chat.side .hz-dock-body .hz-frame { height: 100%; }
  .hz-chat.side .hz-dock, .hz-chat.side .hz-dock-body { height: 100%; }
  .hz-talk { flex: 1; min-height: 0; overflow: hidden; padding: 12px; display: flex; flex-direction: column; gap: 10px; justify-content: flex-end; }
  .hz-bubble { max-width: 84%; padding: 8px 12px; border-radius: 12px; font-size: 13px; line-height: 1.45; background: var(--bg-tertiary); color: var(--text-primary); }
  .hz-bubble.me { align-self: flex-end; background: var(--accent-dim); }
  .hz-bubble.ghost { color: var(--text-secondary); }
  .hz-composer { margin: 0 12px 12px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--radius); color: var(--text-muted); font-size: 13px; background: var(--bg-secondary); flex-shrink: 0; }
  .hz-inline { align-self: stretch; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; background: var(--bg-card); }
  .hz-inline-head { display: flex; align-items: center; gap: 6px; padding: 6px 10px; font-size: 12px; font-weight: 600; color: var(--text-secondary); border-bottom: 1px solid var(--border-subtle); }
  .hz-tabfill { flex: 1; min-height: 0; display: flex; }
  .hz-chip { display: inline-flex; align-items: center; height: 26px; padding: 0 9px; border-radius: 99px; border: 1px solid var(--border); background: var(--bg-tertiary); }
  .hz-chip .hz-frame { width: 24px; height: 24px; background: transparent; }
  .hz-chiplist { margin: 6px 8px; padding: 8px; display: flex; flex-direction: column; gap: 4px; background: var(--bg-secondary); border: 1px solid var(--border); border-radius: var(--radius); box-shadow: var(--shadow-lg); }
  .hz-chiplist-h { font-size: 11px; font-weight: 700; letter-spacing: .05em; text-transform: uppercase; color: var(--text-secondary); padding: 2px 6px 6px; }
  .hz-chiprow { display: flex; align-items: center; gap: 10px; min-height: 46px; padding: 6px 12px; border-radius: 10px; border: 1px solid var(--border); background: var(--bg-primary); }
  .hz-chiprow-n { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; }
  .hz-chiprow .hz-frame { width: 24px; height: 24px; flex: 0 0 auto; background: transparent; }
  .hz-label { position: fixed; right: 6px; bottom: 4px; font: 10px/1 var(--mono); color: var(--text-muted); opacity: 0.7; pointer-events: none; }
`;

const TAB_ICONS = ['Workbench', 'Git', 'Files', 'Kanban', 'Notes', 'Panels'];

function chromeTop(mobile, active) {
  const tabs = TAB_ICONS.map((t) => `<button class="tab-btn${t === active ? ' active' : ''}"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="4" y="4" width="16" height="16" rx="3"/></svg><span class="tab-label">${t}</span></button>`).join('');
  return `<div class="topbar"><span style="font-weight:700;letter-spacing:-0.01em">Crundi</span><span style="color:var(--text-muted);font-size:12px">demo-project</span></div>`
    + `<div class="tab-bar visible">${tabs}</div>`;
}

function cellHead(title) {
  return `<div class="term-head"><span class="term-drag">⋮⋮</span><span class="wb-head-ic"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="3" y1="9" x2="21" y2="9"/></svg></span>`
    + `<span class="term-title" style="cursor:default">${escapeHtml(title)}</span><span class="term-head-spacer" style="flex:1"></span>`
    + `<button class="term-font-btn">↻</button><button class="term-head-btn term-close">×</button></div>`;
}

/**
 * @param {object} o
 * @param {string} o.doc          the widget document (from buildDoc with harness:true)
 * @param {string} o.title
 * @param {string} o.frame        a key of FRAMES
 * @param {object} o.data         { source: { value, error } } to feed the widget
 * @param {object} [o.store]
 */
export async function buildHarnessPage({ doc, title, frame, data, store: kv = {}, stateName = '' }) {
  const f = FRAMES[frame] || FRAMES.cell;
  const { all } = await loadAppCss();
  const mobile = !!f.mobile;
  const iframe = `<iframe class="hz-frame" id="hz-frame" sandbox="${SANDBOX}" allow="${FRAME_ALLOW}" title="${escapeHtml(title)}"></iframe>`;
  let main;
  if (f.slot === 'dock') {
    const side = !mobile && f.width >= 900;
    main = `<div class="hz-main${mobile ? ' flush' : ''}"><div class="term-cell hz-cell" data-chat="1">`
      + `<div class="term-head"><span class="term-drag">⋮⋮</span><span class="term-status-dot"></span><span class="term-title">Chat</span></div>`
      + `<div class="term-body" style="padding:0;display:flex;flex-direction:column"><div class="hz-chat${side ? ' side' : ''}">`
      + `<div class="hz-dock"><div class="hz-dock-head">${escapeHtml(title)}</div><div class="hz-dock-body">${iframe}</div></div>`
      + `<div style="flex:1;min-width:0;min-height:0;display:flex;flex-direction:column"><div class="hz-talk"><div class="hz-bubble me">Run the migration and keep me posted.</div><div class="hz-bubble ghost">Working on it. Progress is in the panel.</div></div><div class="hz-composer">Message Claude…</div></div>`
      + `</div></div></div></div>`;
  } else if (f.slot === 'inline') {
    main = `<div class="hz-main"><div class="term-cell hz-cell" data-chat="1"><div class="term-head"><span class="term-status-dot"></span><span class="term-title">Chat</span></div>`
      + `<div class="term-body" style="padding:0;display:flex;flex-direction:column"><div class="hz-talk" style="justify-content:flex-start;overflow:auto">`
      + `<div class="hz-bubble me">What does the build look like?</div>`
      + `<div class="hz-inline"><div class="hz-inline-head">${escapeHtml(title)}</div><div id="hz-auto" style="height:120px">${iframe}</div></div>`
      + `</div><div class="hz-composer">Message Claude…</div></div></div></div>`;
  } else if (f.slot === 'chipbar' && mobile) {
    // A phone has no room for chips in the bar: one button there drops a list,
    // and the chip is a row of it, its face beside its name.
    main = `<div class="topbar"><span style="font-size:1.3rem">\u2630</span><span style="color:var(--text-muted);font-size:12px;white-space:nowrap">/ demo-project</span><span style="flex:1"></span>`
      + `<span class="hz-chip" style="border-color:var(--accent);color:var(--accent-hover);font-size:12px;font-weight:650">\u25a6 2</span><span class="status-badge connected">\u25cf</span></div>`
      + `<div class="hz-chiplist"><div class="hz-chiplist-h">Panels</div>`
      + `<div class="hz-chiprow"><span class="hz-chiprow-n">${escapeHtml(title)}</span><iframe class="hz-frame" id="hz-frame" sandbox="${SANDBOX}" allow="${FRAME_ALLOW}" title="${escapeHtml(title)}"></iframe></div>`
      + `<div class="hz-chiprow" style="opacity:.55"><span class="hz-chiprow-n">Another panel</span><span style="color:var(--text-secondary);font-size:12px">3 today</span></div></div>`;
  } else if (f.slot === 'chipbar') {
    // The real top bar, with the chip where it sits: before the status badges.
    main = `<div class="topbar"><span style="font-weight:700">Crundi</span><span style="color:var(--text-muted);font-size:12px;white-space:nowrap">/ demo-project</span><span style="flex:1"></span>`
      + `<span class="hz-chip"><iframe class="hz-frame" id="hz-frame" sandbox="${SANDBOX}" allow="${FRAME_ALLOW}" title="${escapeHtml(title)}"></iframe></span>`
      + `<span class="status-badge connected">connected</span></div>`
      + `<div style="padding:10px 16px;color:var(--text-muted);font-size:12px">The chip is the element marked data-chip, shown alone on one line. Tapping it opens the full panel.</div>`;
  } else if (f.slot === 'tab') {
    main = `<div class="hz-tabfill">${iframe}</div>`;
  } else {
    main = `<div class="hz-main${mobile ? ' flush' : ''}"><div class="term-cell wb-cell hz-cell">${cellHead(title)}<div class="term-body wb-cell-body" style="padding:0">${iframe}</div></div></div>`;
  }
  const withChrome = (f.slot === 'tab' || mobile) && f.slot !== 'chipbar';
  const payload = {
    doc, data, store: kv,
    context: { frame: f.slot, platform: mobile ? 'mobile' : 'desktop', touch: mobile, harness: true, title },
    // Frames that size to their content in the real page do so here too.
    auto: f.slot === 'chipbar' ? 'chip' : f.slot === 'inline' ? 'inline' : (f.slot === 'dock' && !(!mobile && f.width >= 900) ? 'dock' : ''),
  };
  return '<!doctype html><html lang="en"><head><meta charset="utf-8">'
    + '<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1">'
    + `<title>Crundi widget harness</title><link rel="icon" href="data:,"><style>${all}\n${HARNESS_CSS}</style></head><body>`
    + (withChrome ? chromeTop(mobile, f.slot === 'tab' ? 'Panels' : 'Workbench') : '')
    + main
    + `<div class="hz-label">${escapeHtml(frame)}${stateName ? ' · ' + escapeHtml(stateName) : ''} · ${f.width}×${f.height}</div>`
    + `<script>window.__HZ__=${jsonForScript(payload)};</script><script>${HARNESS_HOST_JS}</script>`
    + '</body></html>';
}

// The harness's side of the bridge. It answers every call with a stub, feeds
// the data it was given, then asks the frame to lint itself.
const HARNESS_HOST_JS = `
(function () {
  var P = window.__HZ__, frame = document.getElementById('hz-frame');
  var asked = false;
  window.__wgAskLint = function () { if (asked) return false; asked = true; send({ t: 'lint', seq: 1 }); return true; };
  var R = window.__wgResult = { ready: false, done: false, faults: [], calls: [], lint: null, height: 0 };
  function send(m) { m.crundi = 1; frame.contentWindow.postMessage(m, '*'); }
  function ctx() { var r = frame.getBoundingClientRect(); var c = {}; for (var k in P.context) c[k] = P.context[k]; c.width = Math.round(r.width); c.height = Math.round(r.height); return c; }
  window.addEventListener('message', function (e) {
    if (e.source !== frame.contentWindow) return;
    var m = e.data; if (!m || m.crundi !== 1) return;
    if (m.t === 'ready') {
      R.ready = true;
      send({ t: 'init', data: P.data, store: P.store, context: ctx() });
      // Size first, then judge: a chip or an inline card linted at its
      // starting size would be reported as overflowing a frame it never has.
      setTimeout(function () { send({ t: 'measure' }); }, 250);
      setTimeout(function () { send({ t: 'measure' }); }, 450);
      // The lint itself is asked for from outside (window.__wgAskLint), by
      // the renderer, AFTER it has made the browser draw a frame. A headless
      // page does not repaint on its own, and until it does the widget's frame
      // still believes it is the size it started at: a 174px chip was being
      // judged in the 24px it began with. If nobody asks, do it anyway.
      R.readyAt = Date.now();
      setTimeout(function () { if (!asked) window.__wgAskLint(); }, 4000);
    } else if (m.t === 'fault') { R.faults.push({ message: m.message, where: m.where });
    } else if (m.t === 'height') {
      R.height = m.h;
      if (P.auto === 'inline') { var box = document.getElementById('hz-auto'); if (box) box.style.height = Math.max(40, Math.min(460, m.h)) + 'px'; }
      // The same ceiling the page applies: 260px, or 132px on a phone. A face
      // wider than that is cut off there, and the lint will say so.
      if (P.auto === 'chip') { R.chipWidth = m.w || 0; frame.style.width = Math.max(24, Math.min(P.context.platform === 'mobile' ? 200 : 260, m.w || 24)) + 'px'; }
      if (P.auto === 'dock') {
        // Same rule as the page: content height, up to about 44% of the chat pane.
        var pane = frame.closest('.term-body');
        var max = Math.max(120, Math.round((pane ? pane.clientHeight : 400) * 0.44) - 30);
        frame.style.height = Math.max(36, Math.min(max, m.h)) + 'px';
      }
    } else if (m.t === 'call') {
      R.calls.push({ op: m.op, args: m.args });
      send({ t: 'result', seq: m.seq, ok: true, value: { ok: true, harness: true } });
    } else if (m.t === 'lint') {
      R.lint = m.report;
      // One more frame so anything the lint pass did not wait for has painted.
      requestAnimationFrame(function () { setTimeout(function () { R.done = true; }, 80); });
    }
  });
  frame.srcdoc = P.doc;
  setTimeout(function () { if (!R.ready) { R.faults.push({ message: 'The widget never signalled ready. A script error before the page finished loading stops it; check the console lines below.' }); R.done = true; } }, 5000);
  setTimeout(function () { R.done = true; }, 8000);
})();
`;
