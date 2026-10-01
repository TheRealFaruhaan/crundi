/**
 * crundi-notes.js — Crundi's block notes (the Notes tab and Notes panes).
 *
 * Lives outside webapp-html.js for the same reason claude-chat.js does: that
 * file is one template literal, where an unescaped newline or backtick in
 * browser JS breaks the whole page. This is a plain script served from
 * /vendor/.
 *
 *   window.CrundiNotes.mountEditor(el, opts)  -> editor     one page
 *   window.CrundiNotes.mountPicker(el, opts)  -> picker     new page / open one
 *   window.CrundiNotes.mountManager(el, opts) -> manager    the Notes tab
 *
 * Shared opts: project, apiFetch(path, init), toast(msg, kind), clientId.
 * The host owns the auth token and the text zoom (the --nt-fs CSS variable on
 * any ancestor); every size in here is multiplied by it.
 *
 * A page is a title and a list of blocks (see src/notes-store.js). Text blocks
 * hold a little inline HTML; the server keeps only an allow-list of tags, and
 * this side sanitises again before drawing, so neither trusts the other.
 */
(function () {
  'use strict';

  // ─── Styles (injected once) ───
  var STYLE_ID = 'nt-styles';
  var F = function (px) { return 'calc(' + px + 'px * var(--nt-fs, 1))'; };
  var CSS = [
    // One quiet column of prose (~72 characters a line); the block controls
    // live in the margin to its left and only show on hover.
    '.nt-root{position:relative;height:100%;min-height:0;overflow-y:auto;overflow-x:hidden;background:var(--bg-primary);color:var(--text-primary);font-size:' + F(15) + ';line-height:1.65}',
    '.nt-page{max-width:calc(72ch + 72px);margin:0 auto;padding:30px 28px 160px 56px;box-sizing:border-box}',
    '.nt-bar{position:absolute;top:10px;right:14px;z-index:5;display:flex;align-items:center;gap:8px;font-size:' + F(11.5) + ';color:var(--text-muted);pointer-events:none}',
    '.nt-bar .sp{display:none}',
    '.nt-save{display:inline-flex;align-items:center;gap:5px}',
    '.nt-save i{width:6px;height:6px;border-radius:50%;background:var(--green,#10b981)}',
    '.nt-save.busy i{background:var(--yellow,#f59e0b)}',
    '.nt-save.err i{background:var(--red,#ef4444)}',
    '.nt-title{font-size:' + F(28) + ';font-weight:700;letter-spacing:-.01em;line-height:1.22;margin:2px 0 18px;outline:none;word-break:break-word}',
    '.nt-title:empty::before{content:"Untitled";color:var(--text-muted)}',
    '.nt-block{position:relative;display:flex;align-items:flex-start;gap:6px;margin:1px 0;border-radius:6px}',
    '.nt-gutter{position:absolute;left:-40px;top:2px;display:flex;gap:1px;opacity:0;transition:opacity .12s}',
    '.nt-block:hover > .nt-gutter,.nt-block.menu-open > .nt-gutter{opacity:1}',
    '.nt-gutter button{width:19px;height:22px;display:flex;align-items:center;justify-content:center;border:none;background:none;border-radius:4px;color:var(--text-muted);cursor:pointer;padding:0;font-size:' + F(14) + ';line-height:1}',
    '.nt-gutter button:hover{background:var(--bg-hover,#22223a);color:var(--text-primary)}',
    '.nt-grip{width:19px;height:22px;display:flex;align-items:center;justify-content:center;border-radius:4px;color:var(--text-muted);cursor:grab;letter-spacing:-3px;font-size:' + F(14) + ';user-select:none;touch-action:none}',
    '.nt-grip:hover{background:var(--bg-hover,#22223a);color:var(--text-primary)}',
    '.nt-text{flex:1;min-width:0;outline:none;white-space:pre-wrap;word-break:break-word;overflow-wrap:anywhere;padding:2px 0}',
    '.nt-text:empty::before{content:attr(data-ph);color:var(--text-muted);pointer-events:none}',
    '.nt-text:not(:focus):empty::before{content:""}',
    '.nt-block.t-p .nt-text:not(:focus):empty::before{content:""}',
    '.nt-block.t-h1 .nt-text{font-size:' + F(22) + ';font-weight:700;letter-spacing:-.005em;line-height:1.3;margin-top:18px}',
    '.nt-block.t-h2 .nt-text{font-size:' + F(18.5) + ';font-weight:650;line-height:1.35;margin-top:14px}',
    '.nt-block.t-h3 .nt-text{font-size:' + F(16) + ';font-weight:650;margin-top:8px}',
    '.nt-block.t-quote .nt-text{border-left:3px solid var(--border,#2a2a3d);padding-left:12px;color:var(--text-secondary)}',
    '.nt-mark{flex:none;width:22px;display:flex;justify-content:center;padding-top:2px;color:var(--text-secondary);user-select:none}',
    '.nt-block.t-bullet .nt-mark::before{content:"";width:6px;height:6px;border-radius:50%;background:currentColor;margin-top:.62em}',
    '.nt-block.t-number .nt-mark{font-variant-numeric:tabular-nums;justify-content:flex-end;padding-right:2px}',
    '.nt-check{appearance:none;-webkit-appearance:none;flex:none;width:17px;height:17px;margin:calc((1.6em - 17px) / 2 + 2px) 4px 0 2px;border:1.5px solid #4a4a68;border-radius:4px;background:var(--bg-primary);cursor:pointer;display:grid;place-content:center;transition:background .14s,border-color .14s}',
    '.nt-check::after{content:"";width:8px;height:4px;margin-top:-2px;border-left:2px solid #fff;border-bottom:2px solid #fff;transform:rotate(-45deg) scale(0);transition:transform .14s}',
    '.nt-check:checked{background:var(--accent,#6366f1);border-color:var(--accent,#6366f1)}',
    '.nt-check:checked::after{transform:rotate(-45deg) scale(1)}',
    '.nt-block.t-todo.done .nt-text{color:var(--text-muted);text-decoration:line-through}',
    '.nt-code{flex:1;min-width:0;margin:4px 0;padding:12px 14px;background:var(--bg-secondary,#12121a);border:1px solid var(--border-subtle,#1e1e30);border-radius:8px;font-family:var(--mono,monospace);font-size:' + F(12.5) + ';line-height:1.55;white-space:pre-wrap;word-break:break-word;overflow-wrap:anywhere;outline:none;tab-size:2}',
    '.nt-code-wrap{flex:1;min-width:0;position:relative}',
    '.nt-lang{position:absolute;top:8px;right:10px;font-size:' + F(10.5) + ';color:var(--text-muted);font-family:var(--mono,monospace)}',
    '.nt-hr{flex:1;border:none;border-top:1px solid var(--border,#2a2a3d);margin:14px 0;cursor:pointer}',
    '.nt-block.t-divider.sel .nt-hr{border-top-color:var(--accent,#6366f1)}',
    '.nt-table-wrap{flex:1;min-width:0;margin:6px 0}',
    '.nt-table{width:100%;table-layout:fixed;border-collapse:collapse}',
    '.nt-table td{border:1px solid var(--border,#2a2a3d);padding:6px 9px;vertical-align:top;word-break:break-word;overflow-wrap:anywhere;white-space:pre-wrap;outline:none;min-width:40px}',
    '.nt-table.head tr:first-child td{background:var(--bg-secondary,#12121a);font-weight:600}',
    '.nt-table td:focus{box-shadow:inset 0 0 0 2px rgba(99,102,241,.5)}',
    '.nt-tbar{display:none;gap:4px;margin-top:5px;flex-wrap:wrap}',
    '.nt-table-wrap:focus-within .nt-tbar{display:flex}',
    '.nt-tbar button{height:24px;padding:0 8px;border:1px solid var(--border,#2a2a3d);border-radius:6px;background:none;color:var(--text-secondary);cursor:pointer;font-size:' + F(11.5) + '}',
    '.nt-tbar button:hover{color:var(--text-primary);background:var(--bg-tertiary,#1a1a28)}',
    '.nt-root a{color:var(--accent-hover,#818cf8);text-decoration:underline;text-underline-offset:2px;cursor:text}',
    '.nt-root code{font-family:var(--mono,monospace);font-size:.88em;background:var(--bg-tertiary,#1a1a28);border:1px solid var(--border-subtle,#1e1e30);border-radius:4px;padding:0 4px}',
    '.nt-add-end{display:block;width:100%;min-height:80px;cursor:text}',
    // floating format toolbar
    '.nt-fmt{position:fixed;z-index:800;display:flex;align-items:center;gap:2px;padding:4px;background:var(--bg-secondary,#12121a);border:1px solid var(--border,#2a2a3d);border-radius:8px;box-shadow:0 8px 24px rgba(0,0,0,.5)}',
    '.nt-fmt button{min-width:28px;height:26px;padding:0 6px;border:none;border-radius:5px;background:none;color:var(--text-primary);cursor:pointer;font-size:13px}',
    '.nt-fmt button:hover,.nt-fmt button.on{background:var(--bg-hover,#22223a);color:var(--accent-hover,#818cf8)}',
    '.nt-fmt input{width:220px;height:26px;border:1px solid var(--border,#2a2a3d);border-radius:5px;background:var(--bg-primary);color:var(--text-primary);padding:0 8px;font-size:12.5px;outline:none}',
    // menus (slash and block)
    '.nt-menu{position:fixed;z-index:800;min-width:220px;max-height:320px;overflow-y:auto;padding:4px;background:var(--bg-secondary,#12121a);border:1px solid var(--border,#2a2a3d);border-radius:8px;box-shadow:0 10px 30px rgba(0,0,0,.55)}',
    '.nt-menu .lbl{padding:6px 9px 3px;font-size:11px;color:var(--text-muted)}',
    '.nt-menu button{display:flex;align-items:center;gap:10px;width:100%;padding:6px 9px;border:none;border-radius:6px;background:none;color:var(--text-primary);text-align:left;cursor:pointer;font-size:13px}',
    '.nt-menu button .k{width:24px;height:24px;flex:none;display:flex;align-items:center;justify-content:center;border:1px solid var(--border,#2a2a3d);border-radius:5px;font-size:11px;color:var(--text-secondary);font-weight:600}',
    '.nt-menu button .d{margin-left:auto;font-size:11px;color:var(--text-muted)}',
    '.nt-menu button.act,.nt-menu button:hover{background:var(--bg-hover,#22223a)}',
    '.nt-menu .sep{height:1px;margin:4px 2px;background:var(--border-subtle,#1e1e30)}',
    '.nt-menu .danger:hover{background:rgba(239,68,68,.15);color:var(--red,#ef4444)}',
    // picker and manager
    '.nt-pick{height:100%;overflow-y:auto;padding:22px;box-sizing:border-box;display:flex;flex-direction:column;align-items:center}',
    '.nt-pick-box{width:100%;max-width:520px}',
    '.nt-pick h3{margin:0 0 4px;font-size:16px}',
    '.nt-pick p{margin:0 0 14px;color:var(--text-muted);font-size:12.5px}',
    '.nt-new{display:flex;align-items:center;justify-content:center;gap:8px;width:100%;padding:11px;border:1px dashed var(--border,#2a2a3d);border-radius:10px;background:none;color:var(--text-primary);cursor:pointer;font-size:13.5px;font-weight:600;margin-bottom:12px}',
    '.nt-new:hover{border-color:var(--accent,#6366f1);border-style:solid;background:var(--accent-dim,rgba(99,102,241,.15))}',
    '.nt-search{width:100%;box-sizing:border-box;margin-bottom:8px;padding:8px 11px;border:1px solid var(--border,#2a2a3d);border-radius:8px;background:var(--bg-primary);color:var(--text-primary);font-size:13px;outline:none}',
    '.nt-search:focus{border-color:var(--accent,#6366f1)}',
    '.nt-list{display:flex;flex-direction:column;gap:4px}',
    '.nt-item{display:flex;flex-direction:column;gap:2px;width:100%;padding:9px 11px;border:1px solid transparent;border-radius:8px;background:var(--bg-card,#14141f);color:var(--text-primary);text-align:left;cursor:pointer}',
    '.nt-item:hover{border-color:var(--border,#2a2a3d)}',
    '.nt-item.cur{border-color:var(--accent,#6366f1);background:var(--accent-dim,rgba(99,102,241,.15))}',
    '.nt-item .t{font-weight:600;font-size:13.5px;word-break:break-word}',
    '.nt-item .pv{font-size:12px;color:var(--text-secondary);display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;word-break:break-word}',
    '.nt-item .m{font-size:11px;color:var(--text-muted)}',
    '.nt-item.busy{opacity:.55;cursor:not-allowed}',
    '.nt-empty{padding:18px;text-align:center;color:var(--text-muted);font-size:12.5px}',
    '.nt-mgr{display:flex;height:100%;min-height:0}',
    '.nt-side{width:280px;flex:none;display:flex;flex-direction:column;min-height:0;border-right:1px solid var(--border,#2a2a3d);background:var(--bg-secondary,#12121a)}',
    '.nt-side-top{padding:12px;display:flex;flex-direction:column;gap:8px}',
    '.nt-side-top .row{display:flex;gap:6px}',
    '.nt-side-top .nt-new{margin:0;padding:8px}',
    '.nt-side .nt-list{flex:1;overflow-y:auto;padding:0 10px 12px}',
    '.nt-side .nt-item{background:transparent}',
    '.nt-side .nt-item:hover{background:var(--bg-tertiary,#1a1a28)}',
    '.nt-seg{display:flex;border:1px solid var(--border,#2a2a3d);border-radius:8px;overflow:hidden}',
    '.nt-seg button{flex:1;padding:5px 8px;border:none;background:none;color:var(--text-secondary);cursor:pointer;font-size:12px}',
    '.nt-seg button.on{background:var(--accent-dim,rgba(99,102,241,.15));color:var(--accent-hover,#818cf8)}',
    '.nt-main{flex:1;min-width:0;min-height:0;display:flex;flex-direction:column}',
    '.nt-main > .nt-host{flex:1;min-height:0}',
    '.nt-main-empty{flex:1;display:flex;align-items:center;justify-content:center;color:var(--text-muted);font-size:13px;text-align:center;padding:20px}',
    '.nt-trash-acts{display:flex;gap:6px;margin-top:4px}',
    '.nt-trash-acts button{height:24px;padding:0 9px;border:1px solid var(--border,#2a2a3d);border-radius:6px;background:none;color:var(--text-secondary);cursor:pointer;font-size:11.5px}',
    '.nt-trash-acts button:hover{color:var(--text-primary);background:var(--bg-tertiary,#1a1a28)}',
    '.nt-trash-acts button.danger:hover{color:var(--red,#ef4444);border-color:var(--red,#ef4444)}',
    '@media (max-width:700px){.nt-mgr{flex-direction:column}.nt-side{width:auto;max-height:40%;border-right:none;border-bottom:1px solid var(--border,#2a2a3d)}.nt-page{padding:16px 16px 120px 34px}}',
    '@media (prefers-reduced-motion: reduce){.nt-check,.nt-check::after,.nt-gutter{transition:none}}'
  ].join('');

  function ensureStyles() {
    if (document.getElementById(STYLE_ID)) return;
    var s = document.createElement('style');
    s.id = STYLE_ID; s.textContent = CSS;
    document.head.appendChild(s);
  }

  // ─── Helpers ───
  function el(tag, cls, html) { var n = document.createElement(tag); if (cls) n.className = cls; if (html != null) n.innerHTML = html; return n; }
  function esc(s) {
    return String(s == null ? '' : s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
  }
  function genId() { var s = ''; for (var i = 0; i < 16; i++) s += Math.floor(Math.random() * 16).toString(16); return s; }
  function ago(iso) {
    var t = new Date(iso).getTime(); if (!t) return '';
    var s = Math.max(0, (Date.now() - t) / 1000);
    if (s < 60) return 'just now';
    if (s < 3600) return Math.floor(s / 60) + 'm ago';
    if (s < 86400) return Math.floor(s / 3600) + 'h ago';
    return Math.floor(s / 86400) + 'd ago';
  }

  /**
   * The same allow-list as the server (src/notes-store.js sanitizeInline),
   * done with the DOM: unknown elements are unwrapped, every attribute but a
   * safe <a href> is removed.
   */
  var KEEP = { STRONG: 'strong', B: 'strong', EM: 'em', I: 'em', U: 'u', S: 's', STRIKE: 's', DEL: 's', CODE: 'code', A: 'a', BR: 'br' };
  function sanitize(html) {
    var box = document.createElement('div');
    box.innerHTML = String(html == null ? '' : html);
    (function walk(node) {
      var kids = Array.prototype.slice.call(node.childNodes);
      for (var i = 0; i < kids.length; i++) {
        var k = kids[i];
        if (k.nodeType === 3) continue;
        if (k.nodeType !== 1) { node.removeChild(k); continue; }
        var tag = k.tagName;
        if (/^(SCRIPT|STYLE|IFRAME|OBJECT|EMBED|TEMPLATE)$/.test(tag)) { node.removeChild(k); continue; }
        walk(k);
        var want = KEEP[tag];
        if (tag === 'DIV' || tag === 'P') {
          // A block element inside a line is a line break, not a block.
          var frag = document.createDocumentFragment();
          if (k.previousSibling) frag.appendChild(document.createElement('br'));
          while (k.firstChild) frag.appendChild(k.firstChild);
          node.replaceChild(frag, k);
          continue;
        }
        if (!want) { var f2 = document.createDocumentFragment(); while (k.firstChild) f2.appendChild(k.firstChild); node.replaceChild(f2, k); continue; }
        var href = want === 'a' ? (k.getAttribute('href') || '').trim() : '';
        var repl = document.createElement(want);
        while (k.firstChild) repl.appendChild(k.firstChild);
        if (want === 'a') {
          if (/^(https?:\/\/|mailto:)/i.test(href) && !/["<>]/.test(href)) {
            repl.setAttribute('href', href); repl.setAttribute('target', '_blank'); repl.setAttribute('rel', 'noopener noreferrer');
            repl.setAttribute('title', href + ' (Ctrl+click to open)');
          }
        }
        node.replaceChild(repl, k);
      }
    })(box);
    var out = box.innerHTML;
    return out === '<br>' ? '' : out;
  }

  /** Escape plain text and turn its URLs into links. */
  function linkifyText(text) {
    return esc(text).replace(/\bhttps?:\/\/[^\s<]+/g, function (u) {
      var m = u.match(/(?:[.,;:!?)\]'"]|&#39;|&quot;|&gt;)+$/); var tail = '';
      if (m) { tail = m[0]; u = u.slice(0, -tail.length); }
      return '<a href="' + u + '" target="_blank" rel="noopener noreferrer">' + u + '</a>' + tail;
    }).replace(/\n/g, '<br>');
  }
  function isUrl(s) { return /^https?:\/\/\S+$/.test(String(s || '').trim()); }

  // Caret helpers for contenteditable elements.
  function caretOffset(elm) {
    var sel = window.getSelection();
    if (!sel.rangeCount) return 0;
    var r = sel.getRangeAt(0);
    if (!elm.contains(r.startContainer)) return 0;
    var pre = r.cloneRange(); pre.selectNodeContents(elm); pre.setEnd(r.startContainer, r.startOffset);
    return pre.toString().length;
  }
  function atStart(elm) { var sel = window.getSelection(); return sel.isCollapsed && caretOffset(elm) === 0; }
  function atEnd(elm) { var sel = window.getSelection(); return sel.isCollapsed && caretOffset(elm) >= (elm.textContent || '').length; }
  function placeCaret(elm, where) {
    elm.focus();
    var r = document.createRange();
    if (where === 'start') { r.selectNodeContents(elm); r.collapse(true); }
    else if (typeof where === 'number') {
      var left = where, found = false;
      var w = document.createTreeWalker(elm, NodeFilter.SHOW_TEXT, null);
      var n;
      while ((n = w.nextNode())) {
        if (left <= n.length) { r.setStart(n, left); r.collapse(true); found = true; break; }
        left -= n.length;
      }
      if (!found) { r.selectNodeContents(elm); r.collapse(false); }
    } else { r.selectNodeContents(elm); r.collapse(false); }
    var sel = window.getSelection(); sel.removeAllRanges(); sel.addRange(r);
  }
  /** Split an element's HTML at the caret: [before, after]. */
  function splitAtCaret(elm) {
    var sel = window.getSelection();
    if (!sel.rangeCount) return [elm.innerHTML, ''];
    var r = sel.getRangeAt(0);
    if (!sel.isCollapsed) r.deleteContents();
    var a = document.createRange(); a.selectNodeContents(elm); a.setEnd(r.startContainer, r.startOffset);
    var b = document.createRange(); b.selectNodeContents(elm); b.setStart(r.startContainer, r.startOffset);
    var da = el('div'); da.appendChild(a.cloneContents());
    var db = el('div'); db.appendChild(b.cloneContents());
    return [sanitize(da.innerHTML), sanitize(db.innerHTML)];
  }
  function caretRectTop(elm) {
    var sel = window.getSelection();
    if (!sel.rangeCount) return null;
    var r = sel.getRangeAt(0).cloneRange();
    var rects = r.getClientRects();
    if (rects.length) return rects[0];
    var span = document.createElement('span'); span.textContent = '​';
    r.insertNode(span); var rr = span.getBoundingClientRect(); span.parentNode.removeChild(span);
    return rr;
  }
  function onFirstLine(elm) {
    var c = caretRectTop(elm); if (!c) return true;
    var e = elm.getBoundingClientRect();
    var lh = parseFloat(getComputedStyle(elm).lineHeight) || 20;
    return c.top - e.top < lh * 0.9;
  }
  function onLastLine(elm) {
    var c = caretRectTop(elm); if (!c) return true;
    var e = elm.getBoundingClientRect();
    var lh = parseFloat(getComputedStyle(elm).lineHeight) || 20;
    return e.bottom - c.bottom < lh * 0.9;
  }

  var TYPES = [
    { t: 'p', k: 'T', name: 'Text', d: '' },
    { t: 'h1', k: 'H1', name: 'Heading 1', d: '#' },
    { t: 'h2', k: 'H2', name: 'Heading 2', d: '##' },
    { t: 'h3', k: 'H3', name: 'Heading 3', d: '###' },
    { t: 'todo', k: '☐', name: 'To-do', d: '[]' },
    { t: 'bullet', k: '•', name: 'Bulleted list', d: '-' },
    { t: 'number', k: '1.', name: 'Numbered list', d: '1.' },
    { t: 'quote', k: '“', name: 'Quote', d: '>' },
    { t: 'code', k: '</>', name: 'Code', d: '```' },
    { t: 'table', k: '▦', name: 'Table', d: '' },
    { t: 'divider', k: '—', name: 'Divider', d: '---' }
  ];
  var PH = { p: "Type '/' for blocks", h1: 'Heading 1', h2: 'Heading 2', h3: 'Heading 3', todo: 'To-do', bullet: 'List', number: 'List', quote: 'Quote' };
  var TEXTY = { p: 1, h1: 1, h2: 1, h3: 1, todo: 1, bullet: 1, number: 1, quote: 1 };
  var LISTY = { todo: 1, bullet: 1, number: 1 };

  // ─── Markdown (what Claude reads when a block is dropped on a chat) ───
  function inlineMd(html) {
    var box = document.createElement('div');
    box.innerHTML = sanitize(html);
    function walk(n) {
      var out = '';
      Array.prototype.forEach.call(n.childNodes, function (k) {
        if (k.nodeType === 3) { out += k.nodeValue.replace(/\u00a0/g, ' '); return; }
        if (k.nodeType !== 1) return;
        var inner = walk(k);
        switch (k.tagName) {
          case 'STRONG': out += inner.trim() ? '**' + inner + '**' : inner; break;
          case 'EM': out += inner.trim() ? '*' + inner + '*' : inner; break;
          case 'S': out += inner.trim() ? '~~' + inner + '~~' : inner; break;
          case 'CODE': out += '`' + k.textContent + '`'; break;
          case 'A': var h = k.getAttribute('href'); out += h ? '[' + inner + '](' + h + ')' : inner; break;
          case 'BR': out += '\n'; break;
          default: out += inner;
        }
      });
      return out;
    }
    return walk(box);
  }
  /** Blocks as Markdown, numbering consecutive numbered items. */
  function toMarkdown(blocks) {
    var out = [], n = 0;
    (blocks || []).forEach(function (b) {
      if (b.type !== 'number') n = 0;
      var t = b.type === 'code' || b.type === 'table' || b.type === 'divider' ? '' : inlineMd(b.text || '');
      var indentRest = function (s, pad) { return s.replace(/\n/g, '\n' + pad); };
      switch (b.type) {
        case 'h1': out.push('# ' + t); break;
        case 'h2': out.push('## ' + t); break;
        case 'h3': out.push('### ' + t); break;
        case 'todo': out.push('- [' + (b.checked ? 'x' : ' ') + '] ' + indentRest(t, '  ')); break;
        case 'bullet': out.push('- ' + indentRest(t, '  ')); break;
        case 'number': n++; out.push(n + '. ' + indentRest(t, '   ')); break;
        case 'quote': out.push('> ' + t.replace(/\n/g, '\n> ')); break;
        case 'divider': out.push('---'); break;
        case 'code': out.push('```' + (b.lang || '') + '\n' + (b.text || '') + '\n```'); break;
        case 'table':
          var rows = (b.rows || []).map(function (r) { return r.map(function (c) { return inlineMd(c).replace(/\n/g, ' ').replace(/\|/g, '\\|'); }); });
          if (!rows.length) break;
          var line = function (r) { return '| ' + r.join(' | ') + ' |'; };
          var head = rows[0], body = rows.slice(1);
          out.push([line(head), line(head.map(function () { return '---'; }))].concat(body.map(line)).join('\n'));
          break;
        default: out.push(t);
      }
    });
    // Lists and to-dos stay tight; everything else gets a blank line between.
    var res = '';
    for (var i = 0; i < out.length; i++) {
      var tight = i > 0 && /^(todo|bullet|number)$/.test((blocks[i] || {}).type) && /^(todo|bullet|number)$/.test((blocks[i - 1] || {}).type);
      res += (i === 0 ? '' : tight ? '\n' : '\n\n') + out[i];
    }
    return res;
  }

  // One floating thing at a time (format bar, slash menu, block menu).
  var floating = null;
  function closeFloating() { if (floating) { try { floating.close(); } catch (e) {} floating = null; } }
  document.addEventListener('mousedown', function (e) {
    if (floating && floating.node && !floating.node.contains(e.target)) closeFloating();
  }, true);

  // ═══════════════════════════════ Editor ═══════════════════════════════

  function mountEditor(host, opts) {
    ensureStyles();
    opts = opts || {};
    var project = opts.project, pageId = opts.pageId;
    var apiFetch = opts.apiFetch, toast = opts.toast || function () {};
    var clientId = opts.clientId || genId();
    var root = el('div', 'nt-root');
    var bar = el('div', 'nt-bar');
    var saveLbl = el('span', 'nt-save', '<i></i><span>Loading…</span>');
    bar.appendChild(saveLbl); bar.appendChild(el('span', 'sp'));
    var updatedLbl = el('span', '', '');   // kept for the page's tooltip only
    saveLbl.style.pointerEvents = 'auto';
    var page = el('div', 'nt-page');
    var title = el('div', 'nt-title');
    title.contentEditable = 'true'; title.spellcheck = true;
    var list = el('div', 'nt-blocks');
    var tail = el('div', 'nt-add-end');
    page.appendChild(title); page.appendChild(list); page.appendChild(tail);
    root.appendChild(bar); root.appendChild(page);
    host.appendChild(root);
    root.addEventListener('scroll', function () { if (floating && floating.kind !== 'item') closeFloating(); });

    var version = 0, dirty = false, saving = false, saveTimer = null, destroyed = false, loaded = false;

    function setSave(state, text) {
      saveLbl.className = 'nt-save' + (state === 'busy' ? ' busy' : state === 'err' ? ' err' : '');
      saveLbl.lastChild.textContent = text;
    }

    // ─── building blocks ───
    function blockEl(b) {
      var w = el('div', 'nt-block t-' + b.type + (b.type === 'todo' && b.checked ? ' done' : ''));
      w.dataset.id = b.id || genId();
      w.dataset.type = b.type;
      var g = el('div', 'nt-gutter');
      var add = el('button', '', '+'); add.title = 'Add a block below'; add.type = 'button';
      // A span, not a button: the host's drag starts on it (it ignores buttons),
      // and a plain click still opens the block menu.
      var grip = el('span', 'nt-grip', '⋮⋮'); grip.setAttribute('role', 'button'); grip.tabIndex = -1;
      grip.title = 'Click: turn into, move, duplicate, delete. Drag: onto a chat or terminal to send it as Markdown';
      g.appendChild(add); g.appendChild(grip);
      w.appendChild(g);
      if (opts.dragBlock) opts.dragBlock(w, grip, function () { return toMarkdown([readBlock(w)]); });
      add.addEventListener('mousedown', function (e) { e.preventDefault(); });
      add.addEventListener('click', function () { var nb = insertAfter(w, { type: 'p', text: '' }); openSlash(nb, true); });
      grip.addEventListener('mousedown', function (e) { e.preventDefault(); });
      grip.addEventListener('click', function () { openBlockMenu(w, grip); });

      if (b.type === 'divider') {
        var hr = el('hr', 'nt-hr'); hr.tabIndex = 0;
        hr.addEventListener('keydown', function (e) {
          if (e.key === 'Backspace' || e.key === 'Delete') { e.preventDefault(); var p = prevBlock(w) || nextBlock(w); removeBlock(w); if (p) focusBlock(p, 'end'); changed(); }
          else if (e.key === 'Enter') { e.preventDefault(); focusBlock(insertAfter(w, { type: 'p', text: '' }), 'start'); }
          else if (e.key === 'ArrowUp') { var p2 = prevBlock(w); if (p2) { e.preventDefault(); focusBlock(p2, 'end'); } }
          else if (e.key === 'ArrowDown') { var n2 = nextBlock(w); if (n2) { e.preventDefault(); focusBlock(n2, 'start'); } }
        });
        hr.addEventListener('focus', function () { w.classList.add('sel'); });
        hr.addEventListener('blur', function () { w.classList.remove('sel'); });
        w.appendChild(hr);
        return w;
      }
      if (b.type === 'code') {
        var cw = el('div', 'nt-code-wrap');
        var pre = el('pre', 'nt-code'); pre.contentEditable = 'true'; pre.spellcheck = false;
        pre.textContent = b.text || '';
        if (b.lang) cw.appendChild(el('span', 'nt-lang', esc(b.lang)));
        w.dataset.lang = b.lang || '';
        cw.appendChild(pre);
        w.appendChild(cw);
        pre.addEventListener('keydown', function (e) { codeKey(e, w, pre); });
        pre.addEventListener('paste', function (e) { e.preventDefault(); var t = (e.clipboardData || window.clipboardData).getData('text/plain'); document.execCommand('insertText', false, t); });
        pre.addEventListener('input', changed);
        return w;
      }
      if (b.type === 'table') {
        var tw = el('div', 'nt-table-wrap');
        var tbl = el('table', 'nt-table' + (b.header ? ' head' : ''));
        (b.rows && b.rows.length ? b.rows : [['', ''], ['', '']]).forEach(function (r) {
          var tr = el('tr');
          r.forEach(function (c) { tr.appendChild(cellEl(c)); });
          tbl.appendChild(tr);
        });
        tw.appendChild(tbl);
        var tb = el('div', 'nt-tbar');
        [['+ Row', function () { addRow(tbl); }], ['+ Column', function () { addCol(tbl); }], ['− Row', function () { delRow(tbl); }], ['− Column', function () { delCol(tbl); }], ['Header row', function () { tbl.classList.toggle('head'); changed(); }]]
          .forEach(function (x) { var bt = el('button', '', x[0]); bt.type = 'button'; bt.addEventListener('mousedown', function (e) { e.preventDefault(); }); bt.addEventListener('click', x[1]); tb.appendChild(bt); });
        tw.appendChild(tb);
        w.appendChild(tw);
        return w;
      }
      if (b.type === 'todo') {
        var cb = el('input', 'nt-check'); cb.type = 'checkbox'; cb.checked = !!b.checked;
        cb.addEventListener('change', function () { w.classList.toggle('done', cb.checked); changed(); });
        w.appendChild(cb);
      } else if (b.type === 'bullet' || b.type === 'number') {
        w.appendChild(el('span', 'nt-mark'));
      }
      var t = el('div', 'nt-text');
      t.contentEditable = 'true';
      t.dataset.ph = PH[b.type] || '';
      t.innerHTML = sanitize(b.text || '');
      w.appendChild(t);
      wireText(t, w);
      return w;
    }

    function cellEl(html) {
      var td = el('td'); td.contentEditable = 'true'; td.innerHTML = sanitize(html || '');
      td.addEventListener('input', changed);
      td.addEventListener('paste', function (e) { pasteInto(e, td, null); });
      td.addEventListener('keydown', function (e) { cellKey(e, td); });
      td.addEventListener('mouseup', function () { setTimeout(function () { maybeFormatBar(td); }, 0); });
      td.addEventListener('keyup', function (e) { if (e.shiftKey) maybeFormatBar(td); });
      return td;
    }
    function addRow(tbl, after) {
      var cols = tbl.rows[0] ? tbl.rows[0].cells.length : 2;
      var tr = el('tr'); for (var i = 0; i < cols; i++) tr.appendChild(cellEl(''));
      if (after && after.nextSibling) tbl.insertBefore(tr, after.nextSibling); else tbl.appendChild(tr);
      changed(); placeCaret(tr.cells[0], 'start'); return tr;
    }
    function addCol(tbl) {
      if (tbl.rows[0] && tbl.rows[0].cells.length >= 20) { toast('A table can have up to 20 columns here', 'error'); return; }
      Array.prototype.forEach.call(tbl.rows, function (tr) { tr.appendChild(cellEl('')); });
      changed();
    }
    function delRow(tbl) {
      if (tbl.rows.length <= 1) return;
      var cur = document.activeElement && document.activeElement.closest('tr');
      (cur && tbl.contains(cur) ? cur : tbl.rows[tbl.rows.length - 1]).remove(); changed();
    }
    function delCol(tbl) {
      if (!tbl.rows[0] || tbl.rows[0].cells.length <= 1) return;
      var cur = document.activeElement && document.activeElement.closest('td');
      var idx = cur && tbl.contains(cur) ? cur.cellIndex : tbl.rows[0].cells.length - 1;
      Array.prototype.forEach.call(tbl.rows, function (tr) { if (tr.cells[idx]) tr.cells[idx].remove(); }); changed();
    }
    function cellKey(e, td) {
      var tbl = td.closest('table');
      if (e.key === 'Tab') {
        e.preventDefault();
        var cells = Array.prototype.slice.call(tbl.querySelectorAll('td'));
        var i = cells.indexOf(td) + (e.shiftKey ? -1 : 1);
        if (i >= cells.length) { addRow(tbl); return; }
        if (i >= 0) placeCaret(cells[i], 'end');
      } else if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        var tr = td.parentNode, below = tr.nextSibling;
        if (below && below.cells[td.cellIndex]) placeCaret(below.cells[td.cellIndex], 'end');
        else { var w = td.closest('.nt-block'); focusBlock(insertAfter(w, { type: 'p', text: '' }), 'start'); }
      } else if ((e.ctrlKey || e.metaKey) && fmtKey(e)) { /* handled */ }
    }

    function renumber() {
      var n = 0;
      Array.prototype.forEach.call(list.children, function (w) {
        if (w.dataset.type === 'number') { n++; var m = w.querySelector('.nt-mark'); if (m) m.textContent = n + '.'; }
        else n = 0;
      });
    }

    function render(blocks) {
      list.innerHTML = '';
      (blocks && blocks.length ? blocks : [{ id: genId(), type: 'p', text: '' }]).forEach(function (b) { list.appendChild(blockEl(b)); });
      renumber();
    }

    // ─── block navigation and edits ───
    function prevBlock(w) { return w.previousElementSibling; }
    function nextBlock(w) { return w.nextElementSibling; }
    function editable(w) { return w && (w.querySelector('.nt-text') || w.querySelector('.nt-code') || w.querySelector('td') || w.querySelector('.nt-hr')); }
    function focusBlock(w, where) {
      if (!w) return;
      var e = editable(w); if (!e) return;
      if (e.tagName === 'HR') { e.focus(); return; }
      if (e.tagName === 'TD' && where === 'end') { var cells = w.querySelectorAll('td'); e = cells[cells.length - 1]; }
      placeCaret(e, where);
    }
    function insertAfter(w, b) {
      b.id = b.id || genId();
      var nb = blockEl(b);
      if (w && w.nextSibling) list.insertBefore(nb, w.nextSibling); else list.appendChild(nb);
      renumber(); changed();
      return nb;
    }
    function removeBlock(w) { w.remove(); if (!list.children.length) list.appendChild(blockEl({ id: genId(), type: 'p', text: '' })); renumber(); }
    /** Change a block's type, keeping its text where both kinds have text. */
    function convert(w, type, extra) {
      var b = readBlock(w);
      var nb = { id: b.id, type: type };
      if (type === 'code') nb.text = b.type === 'code' ? b.text : plainOf(b.text || '');
      else if (type === 'table') nb.rows = [['', '', ''], ['', '', ''], ['', '', '']];
      else if (type !== 'divider') nb.text = b.type === 'code' ? esc(b.text || '').replace(/\n/g, '<br>') : (b.text || '');
      if (extra) for (var k in extra) nb[k] = extra[k];
      var fresh = blockEl(nb);
      list.replaceChild(fresh, w);
      renumber(); changed();
      return fresh;
    }
    function plainOf(html) { var d = el('div'); d.innerHTML = String(html || '').replace(/<br\s*\/?>/gi, '\n'); return d.textContent || ''; }

    function readBlock(w) {
      var type = w.dataset.type, b = { id: w.dataset.id, type: type };
      if (type === 'divider') return b;
      if (type === 'code') { b.text = (w.querySelector('.nt-code') || {}).textContent || ''; b.lang = w.dataset.lang || ''; return b; }
      if (type === 'table') {
        var tbl = w.querySelector('table');
        b.rows = Array.prototype.map.call(tbl.rows, function (tr) { return Array.prototype.map.call(tr.cells, function (td) { return sanitize(td.innerHTML); }); });
        b.header = tbl.classList.contains('head');
        return b;
      }
      b.text = sanitize((w.querySelector('.nt-text') || {}).innerHTML || '');
      if (type === 'todo') b.checked = !!(w.querySelector('.nt-check') || {}).checked;
      return b;
    }
    function serialize() { return Array.prototype.map.call(list.children, readBlock); }

    // ─── text block keys ───
    function wireText(t, w) {
      t.addEventListener('input', function () { if (floating && floating.kind === 'fmt') closeFloating(); shortcuts(t, w); slashFollow(t, w); changed(); });
      // The format bar belongs to a selection: leaving the block ends it.
      t.addEventListener('blur', function () { setTimeout(function () { if (floating && floating.kind === 'fmt' && !(floating.node.contains(document.activeElement))) closeFloating(); }, 0); });
      t.addEventListener('keydown', function (e) { textKey(e, t, w); });
      t.addEventListener('paste', function (e) { pasteInto(e, t, w); });
      t.addEventListener('mouseup', function () { setTimeout(function () { maybeFormatBar(t); }, 0); });
      t.addEventListener('keyup', function (e) { if (e.shiftKey || e.key === 'Shift') maybeFormatBar(t); });
      t.addEventListener('click', function (e) {
        var a = e.target.closest && e.target.closest('a[href]');
        if (a && (e.ctrlKey || e.metaKey)) { e.preventDefault(); window.open(a.getAttribute('href'), '_blank', 'noopener'); }
      });
    }

    function textKey(e, t, w) {
      if (floating && floating.kind === 'slash' && floating.key(e)) return;
      var type = w.dataset.type;
      if ((e.ctrlKey || e.metaKey) && fmtKey(e)) return;
      if (e.key === 'Enter' && e.shiftKey) { e.preventDefault(); document.execCommand('insertLineBreak'); return; }
      if (e.key === 'Enter') {
        e.preventDefault();
        // An empty list item ends the list.
        if (LISTY[type] && !t.textContent.trim()) { convert(w, 'p'); focusBlock(list.querySelector('[data-id="' + w.dataset.id + '"]'), 'start'); return; }
        var parts = splitAtCaret(t);
        t.innerHTML = parts[0];
        var nextType = LISTY[type] ? type : 'p';
        var nb = insertAfter(w, { type: nextType, text: parts[1], checked: false });
        focusBlock(nb, 'start');
        return;
      }
      if (e.key === 'Backspace' && atStart(t)) {
        if (type !== 'p') { e.preventDefault(); var c = convert(w, 'p'); focusBlock(c, 'start'); return; }
        var p = prevBlock(w);
        if (!p) return;
        e.preventDefault();
        if (p.dataset.type === 'divider') { removeBlock(p); changed(); return; }
        var pt = p.querySelector('.nt-text');
        if (pt) {
          var len = (pt.textContent || '').length;
          pt.innerHTML = sanitize(pt.innerHTML + t.innerHTML);
          removeBlock(w); placeCaret(pt, len); changed();
        } else if (!t.textContent) { removeBlock(w); focusBlock(p, 'end'); changed(); }
        else focusBlock(p, 'end');
        return;
      }
      if (e.key === 'Delete' && atEnd(t)) {
        var n = nextBlock(w), nt = n && n.querySelector('.nt-text');
        if (nt) { e.preventDefault(); var l2 = (t.textContent || '').length; t.innerHTML = sanitize(t.innerHTML + nt.innerHTML); removeBlock(n); placeCaret(t, l2); changed(); }
        return;
      }
      if (e.key === 'ArrowUp' && onFirstLine(t)) { var pb = prevBlock(w); if (pb) { e.preventDefault(); focusBlock(pb, 'end'); } return; }
      if (e.key === 'ArrowDown' && onLastLine(t)) { var nb2 = nextBlock(w); if (nb2) { e.preventDefault(); focusBlock(nb2, 'start'); } return; }
      if (e.key === 'Escape') { closeFloating(); }
    }

    function codeKey(e, w, pre) {
      if (e.key === 'Tab') { e.preventDefault(); document.execCommand('insertText', false, '  '); return; }
      if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); document.execCommand('insertText', false, '\n'); return; }
      // Leaving a code block: Shift+Enter at the end, or arrows past its edges.
      if (e.key === 'Enter' && e.shiftKey) { e.preventDefault(); focusBlock(insertAfter(w, { type: 'p', text: '' }), 'start'); return; }
      if (e.key === 'Backspace' && !pre.textContent) { e.preventDefault(); var c = convert(w, 'p'); focusBlock(c, 'start'); return; }
      if (e.key === 'ArrowUp' && onFirstLine(pre)) { var p = prevBlock(w); if (p) { e.preventDefault(); focusBlock(p, 'end'); } }
      if (e.key === 'ArrowDown' && onLastLine(pre)) { var n = nextBlock(w); if (n) { e.preventDefault(); focusBlock(n, 'start'); } else if (atEnd(pre)) { e.preventDefault(); focusBlock(insertAfter(w, { type: 'p', text: '' }), 'start'); } }
    }

    /** Markdown-style shortcuts typed at the start of a block. */
    function shortcuts(t, w) {
      var txt = (t.textContent || '').replace(/ /g, ' ');
      var rules = [[/^# $/, 'h1'], [/^## $/, 'h2'], [/^### $/, 'h3'], [/^[-*] $/, 'bullet'], [/^1[.)] $/, 'number'], [/^\[ ?\] $/, 'todo'], [/^\[x\] $/i, 'todo-x'], [/^> $/, 'quote']];
      for (var i = 0; i < rules.length; i++) {
        if (rules[i][0].test(txt) && t.innerHTML.replace(/&nbsp;/g, ' ').replace(/<br>$/, '') === txt.replace(/&/g, '&amp;')) {
          var type = rules[i][1] === 'todo-x' ? 'todo' : rules[i][1];
          t.innerHTML = '';
          var c = convert(w, type, rules[i][1] === 'todo-x' ? { checked: true } : null);
          focusBlock(c, 'start');
          return;
        }
      }
      if (/^```[a-zA-Z0-9+#._-]*$/.test(txt) && txt.length >= 3 && w.dataset.type === 'p') {
        // Three backticks (optionally with a language) start a code block once Enter or space would.
      }
      if (txt === '---' && w.dataset.type === 'p') {
        var d = convert(w, 'divider');
        focusBlock(insertAfter(d, { type: 'p', text: '' }), 'start');
        return;
      }
      if (/^```([a-zA-Z0-9+#._-]*) $/.test(txt) && w.dataset.type === 'p') {
        var lang = txt.slice(3).trim();
        var cb = convert(w, 'code', { text: '', lang: lang });
        focusBlock(cb, 'start');
      }
    }

    /** Inline formatting shortcuts; true when handled. */
    function fmtKey(e) {
      var k = e.key.toLowerCase();
      if (k === 'b') { e.preventDefault(); document.execCommand('bold'); changed(); return true; }
      if (k === 'i') { e.preventDefault(); document.execCommand('italic'); changed(); return true; }
      if (k === 'u') { e.preventDefault(); document.execCommand('underline'); changed(); return true; }
      if (k === 'e') { e.preventDefault(); toggleCode(); return true; }
      if (k === 'k') { e.preventDefault(); var t = document.activeElement; showFormatBar(t, true); return true; }
      if (k === 's' && e.shiftKey) { e.preventDefault(); document.execCommand('strikeThrough'); changed(); return true; }
      return false;
    }
    function toggleCode() {
      var sel = window.getSelection(); if (!sel.rangeCount || sel.isCollapsed) return;
      var r = sel.getRangeAt(0);
      var inCode = r.commonAncestorContainer.nodeType === 1 ? r.commonAncestorContainer.closest('code') : r.commonAncestorContainer.parentNode.closest('code');
      if (inCode) { var f = document.createDocumentFragment(); while (inCode.firstChild) f.appendChild(inCode.firstChild); inCode.parentNode.replaceChild(f, inCode); }
      else { var c = document.createElement('code'); c.textContent = r.toString(); r.deleteContents(); r.insertNode(c); sel.removeAllRanges(); var nr = document.createRange(); nr.selectNodeContents(c); sel.addRange(nr); }
      changed();
    }

    /** Paste as plain text, URLs as links; several lines become several blocks. */
    function pasteInto(e, t, w) {
      if (floating && floating.kind === 'fmt') closeFloating();
      var text = (e.clipboardData || window.clipboardData).getData('text/plain');
      if (text == null) return;
      e.preventDefault();
      var sel = window.getSelection();
      // A URL pasted over selected text links that text.
      if (isUrl(text) && sel.rangeCount && !sel.isCollapsed) {
        document.execCommand('createLink', false, text.trim());
        tidyLinks(t); changed(); return;
      }
      var lines = text.replace(/\r\n?/g, '\n').split('\n');
      if (!w || lines.length === 1) {
        document.execCommand('insertHTML', false, linkifyText(text));
        tidyLinks(t); changed(); return;
      }
      // First line joins the current block; the rest become paragraphs after it.
      document.execCommand('insertHTML', false, linkifyText(lines[0]));
      var parts = splitAtCaret(t);
      t.innerHTML = parts[0];
      var last = w;
      for (var i = 1; i < lines.length; i++) {
        last = insertAfter(last, { type: 'p', text: linkifyText(lines[i]) + (i === lines.length - 1 ? parts[1] : '') });
      }
      focusBlock(last, 'end'); changed();
    }
    /** Links made by execCommand carry no target; give them the safe set. */
    function tidyLinks(scope) {
      Array.prototype.forEach.call(scope.querySelectorAll('a[href]'), function (a) {
        var h = a.getAttribute('href') || '';
        if (!/^(https?:\/\/|mailto:)/i.test(h)) { a.removeAttribute('href'); return; }
        a.setAttribute('target', '_blank'); a.setAttribute('rel', 'noopener noreferrer');
        a.setAttribute('title', h + ' (Ctrl+click to open)');
      });
    }

    // ─── floating format bar ───
    function maybeFormatBar(t) {
      var sel = window.getSelection();
      if (!sel.rangeCount || sel.isCollapsed || !t.contains(sel.anchorNode)) { if (floating && floating.kind === 'fmt') closeFloating(); return; }
      showFormatBar(t, false);
    }
    function showFormatBar(t, linkMode) {
      var sel = window.getSelection();
      if (!sel.rangeCount) return;
      var range = sel.getRangeAt(0).cloneRange();
      var rect = range.getBoundingClientRect();
      if (!rect.width && !linkMode) return;
      closeFloating();
      var bar = el('div', 'nt-fmt');
      var btns = [['<b>B</b>', 'Bold (Ctrl+B)', function () { document.execCommand('bold'); }],
        ['<i>I</i>', 'Italic (Ctrl+I)', function () { document.execCommand('italic'); }],
        ['<u>U</u>', 'Underline (Ctrl+U)', function () { document.execCommand('underline'); }],
        ['<s>S</s>', 'Strikethrough (Ctrl+Shift+S)', function () { document.execCommand('strikeThrough'); }],
        ['&lt;/&gt;', 'Code (Ctrl+E)', toggleCode]];
      btns.forEach(function (x) {
        var b = el('button', '', x[0]); b.title = x[1]; b.type = 'button';
        b.addEventListener('mousedown', function (e) { e.preventDefault(); });
        b.addEventListener('click', function () { x[2](); changed(); position(); });
        bar.appendChild(b);
      });
      var lb = el('button', '', 'Link'); lb.title = 'Link (Ctrl+K)'; lb.type = 'button';
      lb.addEventListener('mousedown', function (e) { e.preventDefault(); });
      lb.addEventListener('click', function () { linkInput(); });
      bar.appendChild(lb);
      document.body.appendChild(bar);
      function position() {
        var r = range.getBoundingClientRect();
        var top = r.top - bar.offsetHeight - 8; if (top < 8) top = r.bottom + 8;
        bar.style.top = top + 'px';
        bar.style.left = Math.max(8, Math.min(r.left + r.width / 2 - bar.offsetWidth / 2, window.innerWidth - bar.offsetWidth - 8)) + 'px';
      }
      function linkInput() {
        var existing = (function () { var n = range.commonAncestorContainer; n = n.nodeType === 1 ? n : n.parentNode; var a = n.closest && n.closest('a[href]'); return a ? a.getAttribute('href') : ''; })();
        bar.innerHTML = '';
        var inp = el('input'); inp.placeholder = 'Paste or type a link, Enter to apply'; inp.value = existing;
        var rm = el('button', '', 'Remove'); rm.type = 'button'; rm.title = 'Remove the link';
        bar.appendChild(inp); if (existing) bar.appendChild(rm);
        position(); inp.focus();
        var apply = function (url) {
          var s = window.getSelection(); s.removeAllRanges(); s.addRange(range);
          if (url) {
            if (!/^(https?:\/\/|mailto:)/i.test(url)) url = 'https://' + url.replace(/^\/+/, '');
            document.execCommand('createLink', false, url);
          } else document.execCommand('unlink');
          tidyLinks(t.closest('.nt-block') || t); changed(); closeFloating();
        };
        inp.addEventListener('keydown', function (e) {
          if (e.key === 'Enter') { e.preventDefault(); apply(inp.value.trim()); }
          else if (e.key === 'Escape') { e.preventDefault(); closeFloating(); var s = window.getSelection(); s.removeAllRanges(); s.addRange(range); }
        });
        rm.addEventListener('click', function () { apply(''); });
      }
      floating = { kind: 'fmt', node: bar, close: function () { bar.remove(); } };
      if (linkMode) linkInput(); else position();
    }

    // ─── slash menu ───
    function openSlash(w, fromButton) {
      var t = editable(w);
      if (!t) return;
      if (fromButton) placeCaret(t, 'end');
      closeFloating();
      var menu = el('div', 'nt-menu');
      var items = [], act = 0, query = '';
      var startLen = fromButton ? -1 : Math.max(0, caretOffset(t) - 1);   // where the "/" is
      function draw() {
        menu.innerHTML = '<div class="lbl">' + (query ? 'Blocks matching “' + esc(query) + '”' : 'Turn this into, or add') + '</div>';
        items = TYPES.filter(function (x) { return !query || x.name.toLowerCase().indexOf(query.toLowerCase()) >= 0 || x.t.indexOf(query.toLowerCase()) === 0; });
        if (!items.length) { menu.appendChild(el('div', 'nt-empty', 'No such block')); return; }
        if (act >= items.length) act = items.length - 1;
        items.forEach(function (x, i) {
          var b = el('button', i === act ? 'act' : '', '<span class="k">' + esc(x.k) + '</span>' + esc(x.name) + (x.d ? '<span class="d">' + esc(x.d) + '</span>' : ''));
          b.type = 'button';
          b.addEventListener('mousedown', function (e) { e.preventDefault(); });
          b.addEventListener('click', function () { choose(x.t); });
          menu.appendChild(b);
        });
      }
      function place() {
        var r = caretRectTop(t) || t.getBoundingClientRect();
        var top = r.bottom + 6; if (top + 330 > window.innerHeight) top = Math.max(8, r.top - 330);
        menu.style.top = top + 'px'; menu.style.left = Math.max(8, Math.min(r.left, window.innerWidth - 240)) + 'px';
      }
      function choose(type) {
        // Drop the "/query" that was typed, then convert (or add below if this block has text).
        if (startLen >= 0) {
          var full = t.textContent || '';
          var html = t.innerHTML;
          var cut = '/' + query;
          if (full.slice(startLen, startLen + cut.length) === cut) {
            placeCaret(t, startLen);
            var s = window.getSelection(); var r = s.getRangeAt(0);
            for (var i = 0; i < cut.length; i++) s.modify('extend', 'forward', 'character');
            document.execCommand('delete');
          } else t.innerHTML = html;
        }
        closeFloating();
        var hasText = !!(t.textContent || '').trim();
        var target = hasText && TEXTY[w.dataset.type] ? insertAfter(w, { type: 'p', text: '' }) : w;
        var nb = convert(target, type);
        if (type === 'divider') focusBlock(insertAfter(nb, { type: 'p', text: '' }), 'start');
        else focusBlock(nb, type === 'table' ? 'start' : 'end');
      }
      floating = {
        kind: 'slash', node: menu,
        close: function () { menu.remove(); },
        key: function (e) {
          if (e.key === 'ArrowDown') { e.preventDefault(); act = Math.min(items.length - 1, act + 1); draw(); return true; }
          if (e.key === 'ArrowUp') { e.preventDefault(); act = Math.max(0, act - 1); draw(); return true; }
          if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); if (items[act]) choose(items[act].t); return true; }
          if (e.key === 'Escape') { e.preventDefault(); closeFloating(); return true; }
          return false;
        },
        follow: function () {
          if (startLen < 0) return;
          var full = t.textContent || '';
          if (full.charAt(startLen) !== '/' || caretOffset(t) <= startLen) { closeFloating(); return; }
          query = full.slice(startLen + 1, caretOffset(t));
          if (/\s/.test(query) || query.length > 20) { closeFloating(); return; }
          draw(); place();
        },
      };
      draw();
      document.body.appendChild(menu);
      place();
    }
    function slashFollow(t, w) {
      if (floating && floating.kind === 'slash') { floating.follow(); return; }
      // "/" typed at the start of a block, or after a space, opens the menu.
      var off = caretOffset(t), txt = t.textContent || '';
      if (txt.charAt(off - 1) === '/' && (off === 1 || /\s/.test(txt.charAt(off - 2)))) openSlash(w, false);
    }

    // ─── block menu (the grip) ───
    function openBlockMenu(w, anchor) {
      closeFloating();
      var menu = el('div', 'nt-menu');
      w.classList.add('menu-open');
      menu.appendChild(el('div', 'lbl', 'Turn into'));
      TYPES.forEach(function (x) {
        if (x.t === w.dataset.type) return;
        var b = el('button', '', '<span class="k">' + esc(x.k) + '</span>' + esc(x.name)); b.type = 'button';
        b.addEventListener('click', function () { closeFloating(); var nb = convert(w, x.t); focusBlock(nb, 'end'); });
        menu.appendChild(b);
      });
      menu.appendChild(el('div', 'sep'));
      [['↑', 'Move up', function () { var p = w.previousElementSibling; if (p) { list.insertBefore(w, p); renumber(); changed(); } }],
        ['↓', 'Move down', function () { var n = w.nextElementSibling; if (n) { list.insertBefore(n, w); renumber(); changed(); } }],
        ['⎘', 'Duplicate', function () { var b = readBlock(w); b.id = genId(); var nb = insertAfter(w, b); focusBlock(nb, 'end'); }]]
        .forEach(function (x) { var b = el('button', '', '<span class="k">' + x[0] + '</span>' + x[1]); b.type = 'button'; b.addEventListener('click', function () { closeFloating(); x[2](); }); menu.appendChild(b); });
      var del = el('button', 'danger', '<span class="k">✕</span>Delete'); del.type = 'button';
      del.addEventListener('click', function () { closeFloating(); var p = prevBlock(w) || nextBlock(w); removeBlock(w); changed(); if (p) focusBlock(p, 'end'); });
      menu.appendChild(del);
      document.body.appendChild(menu);
      var r = anchor.getBoundingClientRect();
      var top = r.bottom + 4; if (top + 330 > window.innerHeight) top = Math.max(8, r.top - 330);
      menu.style.top = top + 'px'; menu.style.left = Math.max(8, Math.min(r.left, window.innerWidth - 240)) + 'px';
      floating = { kind: 'block', node: menu, close: function () { menu.remove(); w.classList.remove('menu-open'); } };
    }

    // ─── title ───
    title.addEventListener('keydown', function (e) {
      if (e.key === 'Enter') { e.preventDefault(); focusBlock(list.firstElementChild, 'start'); }
      else if (e.key === 'ArrowDown') { e.preventDefault(); focusBlock(list.firstElementChild, 'start'); }
    });
    title.addEventListener('paste', function (e) { e.preventDefault(); document.execCommand('insertText', false, ((e.clipboardData || window.clipboardData).getData('text/plain') || '').replace(/\s+/g, ' ')); });
    title.addEventListener('input', function () { if (opts.onTitle) opts.onTitle(title.textContent.trim()); changed(); });
    tail.addEventListener('mousedown', function (e) {
      e.preventDefault();
      var last = list.lastElementChild;
      var lt = last && last.querySelector('.nt-text');
      if (lt && !lt.textContent && last.dataset.type === 'p') focusBlock(last, 'end');
      else focusBlock(insertAfter(last, { type: 'p', text: '' }), 'start');
    });

    // ─── saving ───
    function changed() {
      if (!loaded) return;
      dirty = true;
      setSave('busy', 'Editing…');
      clearTimeout(saveTimer);
      saveTimer = setTimeout(save, 700);
    }
    function save() {
      if (destroyed || !dirty) return;
      if (saving) { clearTimeout(saveTimer); saveTimer = setTimeout(save, 400); return; }
      saving = true; dirty = false;
      setSave('busy', 'Saving…');
      var body = { action: 'save', project: project, id: pageId, title: title.textContent.trim(), blocks: serialize(), baseVersion: version, clientId: clientId };
      apiFetch('/api/notes', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
        .then(function (r) { return r.json(); })
        .then(function (d) {
          saving = false;
          if (d.ok) { version = d.version; setSave('ok', 'Saved'); saveLbl.title = 'Saved just now'; if (dirty) { clearTimeout(saveTimer); saveTimer = setTimeout(save, 400); } return; }
          if (d.conflict) return onConflict(body, d.page);
          dirty = true; setSave('err', d.error || 'Not saved');
          toast(d.error || 'Could not save the page', 'error');
        })
        .catch(function () {
          saving = false; dirty = true;
          setSave('err', 'Offline — will retry');
          clearTimeout(saveTimer); saveTimer = setTimeout(save, 4000);
        });
    }
    /**
     * Someone else saved this page since we loaded it. Nothing is thrown
     * away: what was typed here becomes a copy, and this pane shows theirs.
     */
    function onConflict(mine, theirs) {
      apiFetch('/api/notes', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action: 'create', project: project, title: (mine.title || 'Untitled') + ' (your copy)', blocks: mine.blocks, clientId: clientId }) })
        .then(function (r) { return r.json(); })
        .then(function (d) { toast('This page was changed elsewhere. Your version was saved as “' + ((d.page && d.page.title) || 'a copy') + '”.', 'error'); })
        .catch(function () {});
      showPage(theirs);
    }

    function showPage(p) {
      version = p.version || 1;
      title.textContent = p.title || '';
      render(p.blocks);
      loaded = true; dirty = false;
      setSave('ok', 'Saved');
      saveLbl.title = p.updatedAt ? 'Last edited ' + ago(p.updatedAt) : '';
      if (opts.onTitle) opts.onTitle(p.title || '');
    }

    function load() {
      setSave('busy', 'Loading…');
      return apiFetch('/api/notes/page?project=' + encodeURIComponent(project) + '&id=' + encodeURIComponent(pageId))
        .then(function (r) { return r.json(); })
        .then(function (d) {
          if (destroyed) return;
          if (!d.ok) { setSave('err', d.error || 'Not found'); page.innerHTML = '<div class="nt-empty">' + esc(d.error || 'This page could not be opened.') + '</div>'; if (opts.onMissing) opts.onMissing(); return; }
          if (d.page.deleted) { setSave('err', 'In the trash'); page.innerHTML = '<div class="nt-empty">This page is in the trash. Restore it from the Notes tab to edit it.</div>'; return; }
          showPage(d.page);
          if (opts.focus) { if (!title.textContent) placeCaret(title, 'end'); else focusBlock(list.firstElementChild, 'end'); }
        })
        .catch(function (err) { setSave('err', 'Could not load'); toast(err.message, 'error'); });
    }
    load();

    // Flush on the way out (closing the pane, switching pages, reloading).
    function flush() { if (dirty && !destroyed) { clearTimeout(saveTimer); save(); } }
    window.addEventListener('beforeunload', flush);

    return {
      get pageId() { return pageId; },
      /** A save from elsewhere: reload when nothing here is unsaved. */
      remoteChange: function (info) {
        if (!info || info.id !== pageId || info.by === clientId) return;
        if (info.action === 'delete' || info.action === 'purge') { load(); return; }
        if (info.version && info.version <= version) return;
        if (!dirty && !saving) load();
      },
      focus: function () { focusBlock(list.firstElementChild, 'end'); },
      flush: flush,
      destroy: function () {
        flush();
        destroyed = true;
        closeFloating();
        window.removeEventListener('beforeunload', flush);
        clearTimeout(saveTimer);
        root.remove();
      },
    };
  }

  // ═══════════════════════════════ Picker ═══════════════════════════════
  // What a new Notes pane shows: start a page, or open one of the project's.

  function fetchPages(opts, includeDeleted) {
    return opts.apiFetch('/api/notes?project=' + encodeURIComponent(opts.project) + (includeDeleted ? '&includeDeleted=1' : ''))
      .then(function (r) { return r.json(); })
      .then(function (d) { if (!d.ok) throw new Error(d.error || 'Could not list pages'); return d.pages || []; });
  }
  function createPage(opts) {
    return opts.apiFetch('/api/notes', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action: 'create', project: opts.project, title: '', clientId: opts.clientId }) })
      .then(function (r) { return r.json(); })
      .then(function (d) { if (!d.ok) throw new Error(d.error || 'Could not create a page'); return d.page; });
  }
  function itemHtml(p, busy, cur) {
    return '<span class="t">' + esc(p.title || 'Untitled') + '</span>'
      + (p.preview ? '<span class="pv">' + esc(p.preview) + '</span>' : '')
      + '<span class="m">' + (busy ? 'Open in another pane' : 'Edited ' + esc(ago(p.updatedAt))) + (cur ? ' · open here' : '') + '</span>';
  }

  function mountPicker(host, opts) {
    ensureStyles();
    var root = el('div', 'nt-pick');
    var box = el('div', 'nt-pick-box');
    box.innerHTML = '<h3>Notes</h3><p>Start a new page, or open one of this project’s.</p>';
    var nw = el('button', 'nt-new', '+ New page'); nw.type = 'button';
    var search = el('input', 'nt-search'); search.placeholder = 'Search pages';
    var listEl = el('div', 'nt-list');
    box.appendChild(nw); box.appendChild(search); box.appendChild(listEl);
    root.appendChild(box); host.appendChild(root);
    var pages = [];
    function draw() {
      var q = search.value.trim().toLowerCase();
      var busy = opts.busyIds ? opts.busyIds() : {};
      var shown = pages.filter(function (p) { return !q || (p.title || '').toLowerCase().indexOf(q) >= 0 || (p.preview || '').toLowerCase().indexOf(q) >= 0; });
      listEl.innerHTML = '';
      if (!shown.length) { listEl.appendChild(el('div', 'nt-empty', pages.length ? 'No page matches.' : 'No pages yet in this project.')); return; }
      shown.forEach(function (p) {
        var isBusy = !!busy[p.id];
        var b = el('button', 'nt-item' + (isBusy ? ' busy' : ''), itemHtml(p, isBusy)); b.type = 'button';
        if (isBusy) b.title = 'Already open in another pane: one page, one pane';
        b.addEventListener('click', function () { if (!isBusy) opts.onPick(p.id, p.title); });
        listEl.appendChild(b);
      });
    }
    function reload() { fetchPages(opts).then(function (ps) { pages = ps; draw(); }).catch(function (e) { listEl.innerHTML = '<div class="nt-empty">' + esc(e.message) + '</div>'; }); }
    nw.addEventListener('click', function () {
      nw.disabled = true;
      createPage(opts).then(function (p) { opts.onPick(p.id, '', true); }).catch(function (e) { nw.disabled = false; (opts.toast || function () {})(e.message, 'error'); });
    });
    search.addEventListener('input', draw);
    reload();
    return { reload: reload, redraw: draw, destroy: function () { root.remove(); } };
  }

  // ═══════════════════════════════ Manager ═══════════════════════════════
  // The Notes tab: every page of the project, with the one selected open.

  function mountManager(host, opts) {
    ensureStyles();
    var root = el('div', 'nt-mgr');
    var side = el('div', 'nt-side');
    var top = el('div', 'nt-side-top');
    var nw = el('button', 'nt-new', '+ New page'); nw.type = 'button';
    var search = el('input', 'nt-search'); search.placeholder = 'Search pages'; search.style.margin = '0';
    var seg = el('div', 'nt-seg', '<button type="button" class="on" data-v="pages">Pages</button><button type="button" data-v="trash">Trash</button>');
    top.appendChild(nw); top.appendChild(search); top.appendChild(seg);
    var listEl = el('div', 'nt-list');
    side.appendChild(top); side.appendChild(listEl);
    var main = el('div', 'nt-main');
    root.appendChild(side); root.appendChild(main);
    host.appendChild(root);
    var pages = [], view = 'pages', currentId = opts.initialPageId || null, editor = null, listTimer = null;

    function busyIds() { return opts.busyIds ? opts.busyIds() : {}; }
    function showEmpty(msg) {
      if (editor) { editor.destroy(); editor = null; }
      main.innerHTML = '<div class="nt-main-empty">' + esc(msg) + '</div>';
    }
    function open(id, focus) {
      if (busyIds()[id]) { (opts.toast || function () {})('That page is open in a workbench pane. One page, one pane.', 'error'); return; }
      if (editor) { editor.destroy(); editor = null; }
      currentId = id;
      if (opts.onCurrent) opts.onCurrent(id);
      main.innerHTML = '';
      var h = el('div', 'nt-host'); main.appendChild(h);
      editor = mountEditor(h, { project: opts.project, pageId: id, apiFetch: opts.apiFetch, toast: opts.toast, clientId: opts.clientId, focus: !!focus, dragBlock: opts.dragBlock,
        onTitle: function (t) { var p = pages.filter(function (x) { return x.id === id; })[0]; if (p) { p.title = t; draw(); } } });
      draw();
    }
    function draw() {
      var q = search.value.trim().toLowerCase();
      var busy = busyIds();
      var shown = pages.filter(function (p) { return (view === 'trash' ? p.deleted : !p.deleted) && (!q || (p.title || '').toLowerCase().indexOf(q) >= 0 || (p.preview || '').toLowerCase().indexOf(q) >= 0); });
      listEl.innerHTML = '';
      if (!shown.length) { listEl.appendChild(el('div', 'nt-empty', view === 'trash' ? 'The trash is empty.' : pages.some(function (p) { return !p.deleted; }) ? 'No page matches.' : 'No pages yet. Start one with New page.')); return; }
      shown.forEach(function (p) {
        var isBusy = !!busy[p.id] && p.id !== currentId;
        var b = el(view === 'trash' ? 'div' : 'button', 'nt-item' + (p.id === currentId && view !== 'trash' ? ' cur' : '') + (isBusy ? ' busy' : ''), itemHtml(p, isBusy));
        if (view === 'trash') {
          var acts = el('div', 'nt-trash-acts', '<button type="button" data-a="restore">Restore</button><button type="button" class="danger" data-a="purge">Delete for good</button>');
          acts.addEventListener('click', function (e) {
            var a = e.target.getAttribute('data-a'); if (!a) return;
            if (a === 'purge' && !confirm('Delete “' + (p.title || 'Untitled') + '” for good? This cannot be undone.')) return;
            act(a, p.id).then(reload);
          });
          b.appendChild(acts);
        } else {
          b.type = 'button';
          b.addEventListener('click', function () { if (p.id !== currentId) open(p.id); });
          b.addEventListener('contextmenu', function (e) { e.preventDefault(); itemMenu(p, b); });
          var more = el('span', 'm', ''); b.appendChild(more);
        }
        listEl.appendChild(b);
      });
    }
    function itemMenu(p, anchor) {
      closeFloating();
      var menu = el('div', 'nt-menu');
      var del = el('button', 'danger', '<span class="k">✕</span>Move to trash'); del.type = 'button';
      del.addEventListener('click', function () {
        closeFloating();
        act('delete', p.id).then(function () { if (p.id === currentId) { currentId = null; showEmpty('Moved to the trash. Pick a page, or start a new one.'); } reload(); });
      });
      menu.appendChild(del);
      document.body.appendChild(menu);
      var r = anchor.getBoundingClientRect();
      menu.style.top = (r.top + 8) + 'px'; menu.style.left = Math.min(r.right - 20, window.innerWidth - 240) + 'px';
      floating = { kind: 'item', node: menu, close: function () { menu.remove(); } };
    }
    function act(action, id) {
      return opts.apiFetch('/api/notes', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action: action, project: opts.project, id: id, clientId: opts.clientId }) })
        .then(function (r) { return r.json(); })
        .then(function (d) { if (!d.ok) (opts.toast || function () {})(d.error || 'Failed', 'error'); return d; });
    }
    function reload() {
      return fetchPages(opts, true).then(function (ps) {
        pages = ps; draw();
        if (!currentId && view === 'pages') {
          var first = ps.filter(function (p) { return !p.deleted && !busyIds()[p.id]; })[0];
          if (first) open(first.id); else showEmpty(ps.some(function (p) { return !p.deleted; }) ? 'Every page is open in a workbench pane.' : 'No pages yet. Start one with New page.');
        }
      }).catch(function (e) { listEl.innerHTML = '<div class="nt-empty">' + esc(e.message) + '</div>'; });
    }
    nw.addEventListener('click', function () {
      nw.disabled = true;
      createPage(opts).then(function (p) { nw.disabled = false; view = 'pages'; segSel(); pages.unshift({ id: p.id, title: '', preview: '', updatedAt: p.updatedAt }); open(p.id, true); })
        .catch(function (e) { nw.disabled = false; (opts.toast || function () {})(e.message, 'error'); });
    });
    search.addEventListener('input', draw);
    function segSel() { Array.prototype.forEach.call(seg.querySelectorAll('button'), function (b) { b.classList.toggle('on', b.getAttribute('data-v') === view); }); }
    seg.addEventListener('click', function (e) { var v = e.target.getAttribute('data-v'); if (!v) return; view = v; segSel(); draw(); });
    if (currentId) open(currentId); else showEmpty('Loading…');
    reload();

    return {
      get currentId() { return currentId; },
      remoteChange: function (info) {
        if (editor) editor.remoteChange(info);
        // Our own typing saves often: refresh the list (titles, previews)
        // once it settles rather than on every save.
        if (info && info.by === opts.clientId && info.action === 'save') { clearTimeout(listTimer); listTimer = setTimeout(reload, 1500); return; }
        reload();
      },
      redraw: draw,
      flush: function () { if (editor) editor.flush(); },
      destroy: function () { clearTimeout(listTimer); if (editor) editor.destroy(); closeFloating(); root.remove(); },
    };
  }

  window.CrundiNotes = { mountEditor: mountEditor, mountPicker: mountPicker, mountManager: mountManager, sanitize: sanitize, toMarkdown: toMarkdown };
})();
