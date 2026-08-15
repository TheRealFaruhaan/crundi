/**
 * claude-chat.js — Crundi's Claude Code chat renderer (UI mode).
 *
 * Lives outside webapp-html.js on purpose: that file is a single template
 * literal where a bare newline in browser JS breaks the whole page. This is a
 * plain script served from /vendor/, so it can be written normally.
 *
 * Exposes window.CrundiChat.mount(el, opts) -> view. The host owns the socket
 * and the auth token; this module owns everything inside the cell body.
 *
 *   opts.sessionId   chat session id
 *   opts.apiFetch    (path, init) => Promise<Response>, adds the auth header
 *   opts.wsSend      (obj) => void, writes a frame on the shared WebSocket
 *   opts.onTitle     (title) => void, optional
 *
 * view.applyHistory(session)  render a full conversation (subscribe / reconnect)
 * view.applyEvent(ev)         apply one incremental server event
 * view.focus()                focus the composer
 * view.destroy()              detach listeners and clear the DOM
 */
(function () {
  'use strict';

  // ─── Styles (injected once) ───
  // Reuses the page's CSS custom properties so the chat inherits the theme.
  var STYLE_ID = 'cc-styles';
  var CSS = [
    '.cc-root{display:flex;flex-direction:column;height:100%;min-height:0;background:var(--bg-primary);font-size:13px;line-height:1.55}',
    '.cc-log{flex:1;overflow-y:auto;overflow-x:hidden;padding:12px 12px 4px;scroll-behavior:smooth}',
    '.cc-entry{margin-bottom:10px;animation:cc-in .18s ease}',
    '@keyframes cc-in{from{opacity:0;transform:translateY(3px)}to{opacity:1;transform:none}}',

    '.cc-user{display:flex;justify-content:flex-end}',
    '.cc-user-body{max-width:86%;background:var(--accent-dim);border:1px solid rgba(99,102,241,.35);color:var(--text-primary);padding:7px 11px;border-radius:12px 12px 3px 12px;white-space:pre-wrap;word-break:break-word}',

    '.cc-assistant{color:var(--text-primary);word-break:break-word}',
    '.cc-assistant p{margin:0 0 8px}.cc-assistant p:last-child{margin-bottom:0}',
    '.cc-assistant ul,.cc-assistant ol{margin:0 0 8px;padding-left:20px}',
    '.cc-assistant li{margin:2px 0}',
    '.cc-assistant h1,.cc-assistant h2,.cc-assistant h3{margin:12px 0 6px;font-size:14px;font-weight:650;color:var(--text-primary)}',
    '.cc-assistant a{color:var(--accent-hover)}',
    '.cc-assistant code{font-family:var(--mono);font-size:12px;background:var(--bg-tertiary,rgba(255,255,255,.06));padding:1px 5px;border-radius:4px}',
    '.cc-assistant pre{margin:0 0 8px;background:var(--bg-secondary,#111119);border:1px solid var(--border-subtle);border-radius:var(--radius-sm);padding:9px 11px;overflow-x:auto}',
    '.cc-assistant pre code{background:none;padding:0;font-size:12px;line-height:1.5}',
    '.cc-assistant blockquote{margin:0 0 8px;padding-left:10px;border-left:2px solid var(--border);color:var(--text-secondary)}',
    '.cc-cursor{display:inline-block;width:6px;height:13px;background:var(--accent);vertical-align:-2px;animation:cc-blink 1s steps(2) infinite}',
    '@keyframes cc-blink{0%,50%{opacity:1}51%,100%{opacity:0}}',

    '.cc-think{border-left:2px solid var(--border);padding-left:9px;color:var(--text-muted);font-size:12px;font-style:italic}',
    '.cc-think-head{cursor:pointer;user-select:none;color:var(--text-secondary);font-style:normal;display:flex;align-items:center;gap:5px}',
    '.cc-think-body{margin-top:4px;white-space:pre-wrap;max-height:260px;overflow-y:auto}',
    '.cc-collapsed .cc-think-body,.cc-collapsed .cc-tool-body{display:none}',
    '.cc-caret{transition:transform .15s}.cc-collapsed .cc-caret{transform:rotate(-90deg)}',

    '.cc-tool{border:1px solid var(--border-subtle);border-radius:var(--radius-sm);background:var(--bg-secondary,rgba(255,255,255,.02));overflow:hidden}',
    '.cc-tool-head{display:flex;align-items:center;gap:7px;padding:6px 9px;cursor:pointer;user-select:none}',
    '.cc-tool-name{font-family:var(--mono);font-size:12px;font-weight:600;color:var(--accent-hover)}',
    '.cc-tool-sum{color:var(--text-secondary);font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1;min-width:0}',
    '.cc-tool-body{border-top:1px solid var(--border-subtle);padding:8px 9px;font-family:var(--mono);font-size:11.5px;white-space:pre-wrap;word-break:break-word;max-height:340px;overflow:auto;color:var(--text-secondary)}',
    '.cc-tool.err .cc-tool-name{color:var(--red)}',
    '.cc-spin{width:9px;height:9px;border:1.5px solid var(--border);border-top-color:var(--accent);border-radius:50%;animation:cc-spin .7s linear infinite;flex:none}',
    '@keyframes cc-spin{to{transform:rotate(360deg)}}',
    '.cc-dot-ok{width:6px;height:6px;border-radius:50%;background:var(--green);flex:none}',
    '.cc-dot-err{width:6px;height:6px;border-radius:50%;background:var(--red);flex:none}',
    '.cc-diff-add{color:var(--green);background:var(--green-dim);display:block}',
    '.cc-diff-del{color:var(--red);background:var(--red-dim);display:block}',
    '.cc-todo{display:flex;gap:7px;align-items:flex-start;margin:2px 0}',
    '.cc-todo-done{color:var(--text-muted);text-decoration:line-through}',
    '.cc-todo-active{color:var(--accent-hover)}',

    '.cc-ask{border:1px solid var(--accent);border-radius:var(--radius);background:var(--accent-dim);padding:11px 12px;box-shadow:var(--shadow-sm)}',
    '.cc-ask.perm{border-color:var(--yellow);background:var(--yellow-dim)}',
    '.cc-ask-title{font-weight:650;margin-bottom:3px;color:var(--text-primary)}',
    '.cc-ask-sub{color:var(--text-secondary);font-size:12px;margin-bottom:8px;word-break:break-word}',
    '.cc-ask-reason{color:var(--text-muted);font-size:11.5px;margin-bottom:8px}',
    '.cc-ask-pre{font-family:var(--mono);font-size:11.5px;background:var(--bg-primary);border:1px solid var(--border-subtle);border-radius:var(--radius-sm);padding:7px 9px;margin-bottom:9px;max-height:200px;overflow:auto;white-space:pre-wrap;word-break:break-word;color:var(--text-secondary)}',
    '.cc-btns{display:flex;gap:7px;flex-wrap:wrap}',
    '.cc-btn{border:1px solid var(--border);background:var(--bg-tertiary,rgba(255,255,255,.05));color:var(--text-primary);padding:6px 12px;border-radius:var(--radius-sm);cursor:pointer;font-size:12px;font-family:inherit;transition:.12s}',
    '.cc-btn:hover{border-color:var(--accent);background:var(--accent-dim)}',
    '.cc-btn.primary{background:var(--accent);border-color:var(--accent);color:#fff;font-weight:600}',
    '.cc-btn.primary:hover{background:var(--accent-hover)}',
    '.cc-btn.danger:hover{border-color:var(--red);background:var(--red-dim);color:var(--red)}',
    '.cc-btn:disabled{opacity:.5;cursor:default}',

    '.cc-q{margin-bottom:11px}.cc-q:last-of-type{margin-bottom:9px}',
    '.cc-q-chip{display:inline-block;background:var(--accent);color:#fff;font-size:10.5px;font-weight:650;padding:1px 7px;border-radius:99px;margin-bottom:5px;text-transform:uppercase;letter-spacing:.03em}',
    '.cc-q-text{font-weight:600;margin-bottom:7px;color:var(--text-primary)}',
    '.cc-opt{display:flex;gap:8px;align-items:flex-start;border:1px solid var(--border);border-radius:var(--radius-sm);padding:7px 10px;margin-bottom:5px;cursor:pointer;transition:.12s;background:var(--bg-primary)}',
    '.cc-opt:hover{border-color:var(--accent-hover)}',
    '.cc-opt.sel{border-color:var(--accent);background:var(--accent-dim);box-shadow:var(--ring)}',
    '.cc-opt input{margin-top:3px;accent-color:var(--accent);flex:none}',
    '.cc-opt-label{font-weight:600;color:var(--text-primary)}',
    '.cc-opt-desc{color:var(--text-secondary);font-size:12px;margin-top:1px}',
    '.cc-opt-prev{font-family:var(--mono);font-size:11px;background:var(--bg-secondary,#111119);border:1px solid var(--border-subtle);border-radius:4px;padding:6px 8px;margin-top:5px;white-space:pre-wrap;overflow-x:auto;color:var(--text-secondary)}',
    '.cc-other{width:100%;box-sizing:border-box;background:var(--bg-primary);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);padding:6px 9px;font-family:inherit;font-size:12px;margin-top:4px}',
    '.cc-other:focus{outline:none;border-color:var(--accent);box-shadow:var(--ring)}',
    '.cc-answered{color:var(--text-secondary);font-size:12px;display:flex;align-items:center;gap:6px;flex-wrap:wrap}',
    '.cc-answered b{color:var(--text-primary)}',
    '.cc-tag{display:inline-block;background:var(--bg-tertiary,rgba(255,255,255,.06));border:1px solid var(--border-subtle);border-radius:99px;padding:1px 8px;font-size:11px}',
    '.cc-tag.ok{border-color:var(--green);color:var(--green)}',
    '.cc-tag.no{border-color:var(--red);color:var(--red)}',

    '.cc-result{color:var(--text-muted);font-size:11px;text-align:center;padding:3px 0;border-top:1px dashed var(--border-subtle)}',
    '.cc-notice{color:var(--text-muted);font-size:11.5px;text-align:center;font-style:italic}',
    '.cc-error{color:var(--red);background:var(--red-dim);border:1px solid var(--red);border-radius:var(--radius-sm);padding:7px 10px;font-size:12px;word-break:break-word}',

    '.cc-composer{border-top:1px solid var(--border);padding:8px;background:var(--bg-secondary,rgba(0,0,0,.2));flex:none}',
    '.cc-inrow{display:flex;gap:7px;align-items:flex-end}',
    '.cc-input{flex:1;min-height:34px;max-height:180px;resize:none;background:var(--bg-primary);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);padding:8px 10px;font-family:inherit;font-size:13px;line-height:1.45}',
    '.cc-input:focus{outline:none;border-color:var(--accent);box-shadow:var(--ring)}',
    '.cc-input:disabled{opacity:.55}',
    '.cc-meta{display:flex;gap:7px;align-items:center;margin-top:6px;font-size:11px;color:var(--text-muted);flex-wrap:wrap}',
    '.cc-sel{background:var(--bg-primary);border:1px solid var(--border);border-radius:4px;color:var(--text-secondary);font-size:11px;padding:2px 5px;font-family:inherit;cursor:pointer}',
    '.cc-sel:focus{outline:none;border-color:var(--accent)}',
    '.cc-meta-sp{flex:1}',
    '.cc-busy{color:var(--accent-hover)}',
    // Enter-key behaviour toggle, sitting beside the permission-mode dropdown.
    '.cc-toggle{background:var(--bg-primary);border:1px solid var(--border);border-radius:4px;color:var(--text-secondary);font-size:11px;padding:2px 7px;font-family:inherit;cursor:pointer;display:inline-flex;align-items:center;gap:4px}',
    '.cc-toggle:hover{border-color:var(--accent);color:var(--text-primary)}',
    '.cc-toggle b{font-family:var(--mono);color:var(--accent-hover);font-weight:600}',
    // Attach button matches the main input bar's paperclip.
    '.cc-attach{flex:none;width:34px;height:34px;display:inline-flex;align-items:center;justify-content:center;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-secondary);border-radius:var(--radius-sm);cursor:pointer;padding:0}',
    '.cc-attach:hover{color:var(--accent-hover);border-color:var(--accent)}',
    '.cc-attach.busy{opacity:.55;pointer-events:none}',
    '.cc-attach svg{width:16px;height:16px}',
    '.cc-sid{font-family:var(--mono);font-size:10.5px;cursor:pointer;border-bottom:1px dotted var(--border)}',
    '.cc-sid:hover{color:var(--text-secondary)}',
    '.cc-drop{position:absolute;inset:0;border:2px dashed var(--accent);border-radius:var(--radius);background:var(--accent-dim);display:flex;align-items:center;justify-content:center;color:var(--accent-hover);font-weight:600;pointer-events:none;z-index:30}',
    '.cc-slash{position:absolute;bottom:100%;left:0;right:0;margin-bottom:4px;background:var(--bg-secondary,#111119);border:1px solid var(--border);border-radius:var(--radius-sm);box-shadow:var(--shadow-md);max-height:190px;overflow-y:auto;z-index:20}',
    '.cc-slash-item{padding:5px 10px;cursor:pointer;font-size:12px;display:flex;gap:8px}',
    '.cc-slash-item.on{background:var(--accent-dim)}',
    '.cc-slash-item b{font-family:var(--mono);color:var(--accent-hover);font-weight:600}',
    '.cc-slash-item span{color:var(--text-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.cc-wrap{position:relative}'
  ].join('');

  function ensureStyles() {
    if (document.getElementById(STYLE_ID)) return;
    var s = document.createElement('style');
    s.id = STYLE_ID;
    s.textContent = CSS;
    document.head.appendChild(s);
  }

  // ─── Helpers ───

  function esc(s) {
    return String(s == null ? '' : s)
      .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
  }

  /** Strip ANSI escapes — decision_reason and tool output may carry them. */
  function stripAnsi(s) {
    return String(s == null ? '' : s).replace(/\x1b\[[0-9;?]*[a-zA-Z]/g, '');
  }

  function el(tag, cls, html) {
    var n = document.createElement(tag);
    if (cls) n.className = cls;
    if (html != null) n.innerHTML = html;
    return n;
  }

  /**
   * Minimal, safe markdown. Everything is HTML-escaped first, then a small set
   * of inline/block constructs is re-introduced — so no untrusted markup can
   * survive. Deliberately not a full parser; it covers what Claude emits.
   */
  function md(src) {
    var text = String(src == null ? '' : src);
    var fences = [];
    // Pull fenced code out first so its contents are never inline-formatted.
    text = text.replace(/```([a-zA-Z0-9_+-]*)\n?([\s\S]*?)```/g, function (m, lang, body) {
      fences.push('<pre><code data-lang="' + esc(lang) + '">' + esc(body.replace(/\n$/, '')) + '</code></pre>');
      return ' F' + (fences.length - 1) + ' ';
    });
    text = esc(text);

    var lines = text.split('\n');
    var out = [];
    var listType = null;
    var para = [];

    function flushPara() {
      if (!para.length) return;
      out.push('<p>' + inline(para.join('<br>')) + '</p>');
      para = [];
    }
    function closeList() {
      if (listType) { out.push('</' + listType + '>'); listType = null; }
    }

    for (var i = 0; i < lines.length; i++) {
      var ln = lines[i];
      var fence = ln.match(/^ F(\d+) $/);
      if (fence) { flushPara(); closeList(); out.push(fences[+fence[1]]); continue; }
      if (!ln.trim()) { flushPara(); closeList(); continue; }

      var h = ln.match(/^(#{1,6})\s+(.*)$/);
      if (h) { flushPara(); closeList(); var lv = Math.min(h[1].length + 2, 6); out.push('<h' + lv + '>' + inline(h[2]) + '</h' + lv + '>'); continue; }
      if (/^(---|\*\*\*|___)\s*$/.test(ln)) { flushPara(); closeList(); out.push('<hr>'); continue; }
      var q = ln.match(/^&gt;\s?(.*)$/);
      if (q) { flushPara(); closeList(); out.push('<blockquote>' + inline(q[1]) + '</blockquote>'); continue; }

      var ul = ln.match(/^\s*[-*+]\s+(.*)$/);
      var ol = ln.match(/^\s*\d+[.)]\s+(.*)$/);
      if (ul || ol) {
        flushPara();
        var want = ul ? 'ul' : 'ol';
        if (listType !== want) { closeList(); out.push('<' + want + '>'); listType = want; }
        out.push('<li>' + inline((ul || ol)[1]) + '</li>');
        continue;
      }
      closeList();
      para.push(ln);
    }
    flushPara();
    closeList();
    return out.join('');
  }

  function inline(s) {
    return s
      .replace(/`([^`]+)`/g, function (m, c) { return '<code>' + c + '</code>'; })
      .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
      .replace(/(^|[\s(])\*([^*\n]+)\*/g, '$1<em>$2</em>')
      .replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');
  }

  function shortPath(p) {
    var s = String(p || '').replace(/\\/g, '/');
    var parts = s.split('/');
    return parts.length > 3 ? '…/' + parts.slice(-2).join('/') : s;
  }

  function truncate(s, n) {
    s = String(s == null ? '' : s);
    return s.length > n ? s.slice(0, n) + '…' : s;
  }

  /** One-line summary for a tool card header. */
  function toolSummary(name, input) {
    input = input || {};
    switch (name) {
      case 'Read': return shortPath(input.file_path);
      case 'Write': return shortPath(input.file_path);
      case 'Edit': return shortPath(input.file_path);
      case 'NotebookEdit': return shortPath(input.notebook_path);
      case 'Bash': return truncate(input.command, 140);
      case 'Glob': return input.pattern + (input.path ? ' in ' + shortPath(input.path) : '');
      case 'Grep': return input.pattern + (input.glob ? ' (' + input.glob + ')' : '');
      case 'WebFetch': return input.url;
      case 'WebSearch': return input.query;
      case 'Task': case 'Agent': return input.description || input.subagent_type || '';
      case 'TodoWrite': return (input.todos || []).length + ' items';
      case 'Skill': return input.skill || '';
      default: {
        var keys = Object.keys(input);
        if (!keys.length) return '';
        return truncate(keys.map(function (k) { return k + '=' + JSON.stringify(input[k]); }).join(' '), 140);
      }
    }
  }

  /** Expanded body for a tool card: the interesting part of the input. */
  function toolDetailHtml(name, input) {
    input = input || {};
    if (name === 'Bash') {
      return '<div>' + esc(input.command || '') + '</div>'
        + (input.description ? '<div style="color:var(--text-muted);margin-top:5px">' + esc(input.description) + '</div>' : '');
    }
    if (name === 'Write') {
      return '<div class="cc-diff-add">' + esc(truncate(input.content, 4000)) + '</div>';
    }
    if (name === 'Edit') {
      var oldS = String(input.old_string == null ? '' : input.old_string);
      var newS = String(input.new_string == null ? '' : input.new_string);
      var h = '';
      oldS.split('\n').forEach(function (l) { h += '<span class="cc-diff-del">- ' + esc(l) + '</span>'; });
      newS.split('\n').forEach(function (l) { h += '<span class="cc-diff-add">+ ' + esc(l) + '</span>'; });
      return h;
    }
    if (name === 'TodoWrite') {
      return (input.todos || []).map(function (t) {
        var cls = t.status === 'completed' ? 'cc-todo-done' : t.status === 'in_progress' ? 'cc-todo-active' : '';
        var box = t.status === 'completed' ? '☑' : t.status === 'in_progress' ? '▸' : '☐';
        return '<div class="cc-todo ' + cls + '"><span>' + box + '</span><span>' + esc(t.activeForm && t.status === 'in_progress' ? t.activeForm : t.content) + '</span></div>';
      }).join('');
    }
    return esc(truncate(JSON.stringify(input, null, 2), 4000));
  }

  // ─── Mount ───

  function mount(host, opts) {
    ensureStyles();
    opts = opts || {};
    var sessionId = opts.sessionId;
    var apiFetch = opts.apiFetch;
    var wsSend = opts.wsSend || function () {};

    var root = el('div', 'cc-root');
    var log = el('div', 'cc-log');
    var composer = el('div', 'cc-composer');
    var wrap = el('div', 'cc-wrap');
    var inrow = el('div', 'cc-inrow');
    var input = el('textarea', 'cc-input');
    input.rows = 1;
    input.placeholder = 'Message Claude…  (Enter to send, Shift+Enter for newline)';
    var sendBtn = el('button', 'cc-btn primary', 'Send');
    var stopBtn = el('button', 'cc-btn danger', 'Stop');
    stopBtn.style.display = 'none';
    // Paperclip, mirroring the main input bar: uploads to crundi_attachments
    // and inserts the returned path into the message.
    var attachBtn = el('button', 'cc-attach',
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'
      + '<path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>');
    attachBtn.title = 'Attach a file (uploads to crundi_attachments)';
    var fileInput = el('input');
    fileInput.type = 'file';
    fileInput.style.display = 'none';
    inrow.appendChild(input);
    inrow.appendChild(attachBtn);
    inrow.appendChild(sendBtn);
    inrow.appendChild(stopBtn);
    inrow.appendChild(fileInput);

    var meta = el('div', 'cc-meta');
    var stateLbl = el('span', '', 'idle');
    var modeSel = el('select', 'cc-sel');
    [['default', 'ask permissions'], ['acceptEdits', 'accept edits'], ['plan', 'plan mode'], ['dontAsk', "don't ask"], ['bypassPermissions', 'bypass all']]
      .forEach(function (m) { var o = el('option'); o.value = m[0]; o.textContent = m[1]; modeSel.appendChild(o); });
    var enterBtn = el('button', 'cc-toggle');
    var sidLbl = el('span', 'cc-sid', '');
    sidLbl.style.display = 'none';
    var modelLbl = el('span', '', '');
    var costLbl = el('span', '', '');
    meta.appendChild(stateLbl);
    meta.appendChild(modeSel);
    meta.appendChild(enterBtn);
    meta.appendChild(el('span', 'cc-meta-sp'));
    meta.appendChild(sidLbl);
    meta.appendChild(modelLbl);
    meta.appendChild(costLbl);

    wrap.appendChild(inrow);
    composer.appendChild(wrap);
    composer.appendChild(meta);
    root.appendChild(log);
    root.appendChild(composer);
    host.appendChild(root);

    var entries = new Map();   // entry id -> { data, node }
    var state = 'idle';
    var slashCommands = [];
    var slashBox = null;
    var slashIdx = 0;
    var destroyed = false;
    var project = opts.project || '';
    var toast = opts.toast || function () {};
    // Claude Code's OWN session uuid (what --resume takes). Deliberately not
    // named sessionId — that is the Crundi cell id used for API/WS routing, and
    // conflating the two silently breaks every subscription.
    var claudeSessionId = '';

    // Enter behaviour: 'send' = Enter sends / Shift+Enter newline;
    // 'newline' = Enter newline / Ctrl+Enter sends. Shared by every chat cell.
    var ENTER_KEY = 'crundi_chat_enter';
    var enterMode = 'send';
    try { if (localStorage.getItem(ENTER_KEY) === 'newline') enterMode = 'newline'; } catch (e) {}

    function syncEnterMode() {
      var sends = enterMode === 'send';
      enterBtn.innerHTML = sends
        ? '<b>⏎</b> sends'
        : '<b>⌃⏎</b> sends';
      enterBtn.title = sends
        ? 'Enter sends, Shift+Enter makes a newline — click to swap'
        : 'Enter makes a newline, Ctrl+Enter sends — click to swap';
      input.placeholder = sends
        ? 'Message Claude…  (Enter to send, Shift+Enter for newline)'
        : 'Message Claude…  (Ctrl+Enter to send, Enter for newline)';
    }
    syncEnterMode();
    enterBtn.addEventListener('click', function () {
      enterMode = enterMode === 'send' ? 'newline' : 'send';
      try { localStorage.setItem(ENTER_KEY, enterMode); } catch (e) {}
      syncEnterMode();
      input.focus();
    });

    function setSessionId(id) {
      if (!id || id === claudeSessionId) return;
      claudeSessionId = id;
      sidLbl.style.display = '';
      sidLbl.textContent = id.slice(0, 8);
      sidLbl.title = 'Claude session ' + id + '  — click to copy (use it to resume this conversation)';
    }
    sidLbl.addEventListener('click', function () {
      if (!claudeSessionId) return;
      var done = function () { toast('Session id copied: ' + claudeSessionId); };
      try {
        if (navigator.clipboard && navigator.clipboard.writeText) { navigator.clipboard.writeText(claudeSessionId).then(done, done); return; }
      } catch (e) { /* fall through */ }
      // Clipboard API needs a secure context; fall back to a temp selection.
      var t = document.createElement('textarea');
      t.value = claudeSessionId;
      document.body.appendChild(t); t.select();
      try { document.execCommand('copy'); } catch (e) {}
      document.body.removeChild(t);
      done();
    });

    function atBottom() {
      return log.scrollHeight - log.scrollTop - log.clientHeight < 60;
    }
    function scrollDown(force) {
      if (force || atBottom()) log.scrollTop = log.scrollHeight;
    }

    function setState(s) {
      state = s;
      var busy = s === 'working';
      stateLbl.textContent = s === 'working' ? 'working…' : s === 'needs-input' ? 'needs your input' : 'idle';
      stateLbl.className = busy || s === 'needs-input' ? 'cc-busy' : '';
      stopBtn.style.display = busy ? '' : 'none';
      sendBtn.style.display = busy ? 'none' : '';
    }

    // ─── Entry rendering ───

    function renderEntry(e) {
      var node = el('div', 'cc-entry');
      node.dataset.id = e.id;
      paint(node, e);
      return node;
    }

    function paint(node, e) {
      node.innerHTML = '';
      switch (e.kind) {
        case 'user': {
          var w = el('div', 'cc-user');
          w.appendChild(el('div', 'cc-user-body', esc(e.text)));
          node.appendChild(w);
          break;
        }
        case 'assistant-text': {
          var a = el('div', 'cc-assistant', md(e.text));
          if (e.streaming) a.appendChild(el('span', 'cc-cursor'));
          node.appendChild(a);
          break;
        }
        case 'thinking':   node.appendChild(thinkingNode(e)); break;
        case 'tool':       node.appendChild(toolNode(e)); break;
        case 'permission': node.appendChild(permissionNode(e)); break;
        case 'question':   node.appendChild(questionNode(e)); break;
        case 'result': {
          if (e.subtype && e.subtype !== 'success') {
            node.appendChild(el('div', 'cc-error', esc(e.text || e.subtype)));
          } else {
            var bits = [];
            if (e.durationMs) bits.push((e.durationMs / 1000).toFixed(1) + 's');
            if (e.costUsd) bits.push('$' + e.costUsd.toFixed(4));
            node.appendChild(el('div', 'cc-result', esc(bits.join(' · '))));
          }
          break;
        }
        case 'notice': node.appendChild(el('div', 'cc-notice', esc(e.text))); break;
        case 'error':  node.appendChild(el('div', 'cc-error', esc(e.text))); break;
        // A transient entry the server retired (e.g. the startup placeholder).
        case 'gone':   node.style.display = 'none'; break;
      }
    }

    function thinkingNode(e) {
      var box = el('div', 'cc-think cc-collapsed');
      var head = el('div', 'cc-think-head', '<span class="cc-caret">▾</span><span>Thinking</span>');
      var body = el('div', 'cc-think-body', esc(e.text));
      head.addEventListener('click', function () { box.classList.toggle('cc-collapsed'); });
      box.appendChild(head);
      box.appendChild(body);
      return box;
    }

    function toolNode(e) {
      var box = el('div', 'cc-tool cc-collapsed' + (e.isError ? ' err' : ''));
      var status = e.status === 'running'
        ? '<span class="cc-spin"></span>'
        : (e.isError ? '<span class="cc-dot-err"></span>' : '<span class="cc-dot-ok"></span>');
      var head = el('div', 'cc-tool-head',
        status + '<span class="cc-tool-name">' + esc(e.name) + '</span>'
        + '<span class="cc-tool-sum">' + esc(toolSummary(e.name, e.input)) + '</span>'
        + '<span class="cc-caret" style="color:var(--text-muted)">▾</span>');
      var body = el('div', 'cc-tool-body');
      body.innerHTML = toolDetailHtml(e.name, e.input);
      if (e.result) {
        var sep = el('div', '', '');
        sep.style.cssText = 'margin-top:7px;padding-top:7px;border-top:1px solid var(--border-subtle);color:' + (e.isError ? 'var(--red)' : 'var(--text-muted)');
        sep.textContent = truncate(stripAnsi(e.result), 6000);
        body.appendChild(sep);
      }
      head.addEventListener('click', function () { box.classList.toggle('cc-collapsed'); });
      box.appendChild(head);
      box.appendChild(body);
      return box;
    }

    // Permission prompt: Allow / Allow always / Deny, mirroring the CLI's own
    // options. "Always" replays the CLI's permission_suggestions verbatim.
    function permissionNode(e) {
      var box = el('div', 'cc-ask perm');
      if (e.status !== 'pending') {
        box.appendChild(el('div', 'cc-answered',
          '<b>' + esc(e.displayName || e.toolName) + '</b>'
          + '<span class="cc-tag ' + (e.status === 'denied' ? 'no' : 'ok') + '">'
          + (e.status === 'denied' ? 'denied' : e.always ? 'always allowed' : 'allowed') + '</span>'));
        return box;
      }
      box.appendChild(el('div', 'cc-ask-title', esc(e.title || ('Claude wants to use ' + (e.displayName || e.toolName)))));
      if (e.description) box.appendChild(el('div', 'cc-ask-sub', esc(e.description)));
      if (e.decisionReason) box.appendChild(el('div', 'cc-ask-reason', esc(stripAnsi(e.decisionReason))));
      box.appendChild(el('div', 'cc-ask-pre', esc(truncate(JSON.stringify(e.input, null, 2), 2500))));

      var btns = el('div', 'cc-btns');
      var allow = el('button', 'cc-btn primary', 'Allow');
      allow.addEventListener('click', function () { respond(e, { behavior: 'allow' }); });
      btns.appendChild(allow);
      // The CLI tells us when a persistent rule must not be offered.
      if (!e.suppressAlwaysAllow && (e.suggestions || []).length) {
        var always = el('button', 'cc-btn', 'Allow always');
        always.addEventListener('click', function () { respond(e, { behavior: 'allow', always: true }); });
        btns.appendChild(always);
      }
      var deny = el('button', 'cc-btn danger', 'Deny');
      deny.addEventListener('click', function () { respond(e, { behavior: 'deny' }); });
      btns.appendChild(deny);
      box.appendChild(btns);
      return box;
    }

    // AskUserQuestion: 1–4 questions, each with 2–4 options, optional
    // multi-select, optional per-option preview, plus a free-text "Other".
    function questionNode(e) {
      var box = el('div', 'cc-ask');
      var questions = (e.input && e.input.questions) || [];

      if (e.status !== 'pending') {
        var answered = (e.answeredInput && e.answeredInput.answers) || {};
        var html = e.status === 'denied'
          ? '<span class="cc-tag no">dismissed</span>'
          : Object.keys(answered).map(function (k) {
              return '<span class="cc-tag ok">' + esc(answered[k]) + '</span>';
            }).join(' ');
        box.appendChild(el('div', 'cc-answered', html || '<span class="cc-tag">answered</span>'));
        return box;
      }

      var picks = questions.map(function () { return { chosen: [], other: '' }; });

      questions.forEach(function (q, qi) {
        var qEl = el('div', 'cc-q');
        if (q.header) qEl.appendChild(el('span', 'cc-q-chip', esc(q.header)));
        qEl.appendChild(el('div', 'cc-q-text', esc(q.question)));
        var multi = !!q.multiSelect;
        var name = 'q' + qi + '-' + e.id;

        (q.options || []).forEach(function (opt, oi) {
          var row = el('label', 'cc-opt');
          var inp = el('input');
          inp.type = multi ? 'checkbox' : 'radio';
          inp.name = name;
          inp.value = opt.label;
          var txt = el('div');
          txt.appendChild(el('div', 'cc-opt-label', esc(opt.label)));
          if (opt.description) txt.appendChild(el('div', 'cc-opt-desc', esc(opt.description)));
          if (opt.preview) txt.appendChild(el('div', 'cc-opt-prev', esc(opt.preview)));
          row.appendChild(inp);
          row.appendChild(txt);
          inp.addEventListener('change', function () {
            if (multi) {
              picks[qi].chosen = Array.prototype.slice
                .call(qEl.querySelectorAll('input[type=checkbox]'))
                .filter(function (x) { return x.checked; })
                .map(function (x) { return x.value; });
            } else {
              picks[qi].chosen = [opt.label];
              picks[qi].other = '';
              otherInput.value = '';
            }
            qEl.querySelectorAll('.cc-opt').forEach(function (r, ri) {
              r.classList.toggle('sel', multi
                ? qEl.querySelectorAll('input')[ri].checked
                : ri === oi);
            });
            syncSubmit();
          });
          qEl.appendChild(row);
        });

        // "Other" is always available — the tool description promises it.
        var otherInput = el('input', 'cc-other');
        otherInput.type = 'text';
        otherInput.placeholder = 'Other (type your own answer)…';
        otherInput.addEventListener('input', function () {
          picks[qi].other = otherInput.value;
          if (otherInput.value) {
            picks[qi].chosen = [];
            qEl.querySelectorAll('input[name="' + name + '"]').forEach(function (x) { x.checked = false; });
            qEl.querySelectorAll('.cc-opt').forEach(function (r) { r.classList.remove('sel'); });
          }
          syncSubmit();
        });
        qEl.appendChild(otherInput);
        box.appendChild(qEl);
      });

      var btns = el('div', 'cc-btns');
      var submit = el('button', 'cc-btn primary', 'Submit');
      submit.disabled = true;
      var skip = el('button', 'cc-btn danger', 'Dismiss');
      btns.appendChild(submit);
      btns.appendChild(skip);
      box.appendChild(btns);

      function answerFor(qi) {
        var p = picks[qi];
        if (p.other && p.other.trim()) return p.other.trim();
        return p.chosen.join(', ');
      }
      function syncSubmit() {
        submit.disabled = questions.some(function (q, qi) { return !answerFor(qi); });
      }

      submit.addEventListener('click', function () {
        var answers = {};
        questions.forEach(function (q, qi) { answers[q.question] = answerFor(qi); });
        respond(e, { behavior: 'allow', updatedInput: Object.assign({}, e.input, { answers: answers }) });
      });
      skip.addEventListener('click', function () {
        respond(e, { behavior: 'deny', message: 'The user dismissed the question.' });
      });
      return box;
    }

    // ─── Server actions ───

    function respond(e, payload) {
      // Optimistically lock the card so a double-click can't double-answer.
      var rec = entries.get(e.id);
      if (rec) {
        rec.data.status = payload.behavior === 'deny' ? 'denied' : 'allowed';
        rec.data.always = !!payload.always;
        if (payload.updatedInput) rec.data.answeredInput = payload.updatedInput;
        paint(rec.node, rec.data);
      }
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/respond', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(Object.assign({ requestId: e.requestId }, payload)),
      }).catch(function () { /* the socket will resync */ });
    }

    function doSend() {
      var text = input.value;
      if (!text.trim() || state === 'working') return;
      input.value = '';
      autoGrow();
      hideSlash();
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text: text }),
      }).then(function (r) { return r.json(); }).then(function (d) {
        if (d && d.ok === false) appendLocal({ kind: 'error', text: d.error || 'Failed to send' });
      }).catch(function (err) { appendLocal({ kind: 'error', text: String(err.message || err) }); });
    }

    function doStop() {
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/interrupt', { method: 'POST' }).catch(function () {});
    }

    function appendLocal(data) {
      data.id = 'local-' + Math.random().toString(16).slice(2);
      addEntry(data);
    }

    // ─── Attachments ───
    // Same contract as the main input bar: base64 the file to
    // /api/attachments/upload, then drop the returned repo-relative path into
    // the message so Claude can read it as a normal file.

    function b64(buf) {
      var bytes = new Uint8Array(buf), bin = '', CH = 0x8000;
      for (var i = 0; i < bytes.length; i += CH) bin += String.fromCharCode.apply(null, bytes.subarray(i, i + CH));
      return btoa(bin);
    }

    function insertPath(p) {
      var v = input.value;
      var pad = (!v || /\s$/.test(v)) ? '' : ' ';
      input.value = v + pad + p + ' ';
      autoGrow();
      input.focus();
    }

    function uploadFile(file) {
      if (!file) return;
      if (!project) { toast('No project for this chat', 'error'); return; }
      attachBtn.classList.add('busy');
      var name = file.name || ('image.' + ((file.type || 'image/png').split('/')[1] || 'png'));
      file.arrayBuffer().then(function (buf) {
        return apiFetch('/api/attachments/upload', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ project: project, name: name, data: b64(buf) }),
        });
      }).then(function (r) { return r.json(); }).then(function (d) {
        if (d && d.ok && d.path) { insertPath(d.path); toast('Attached: ' + (d.name || name)); }
        else toast('Upload failed: ' + ((d && d.error) || '?'), 'error');
      }).catch(function (err) {
        toast('Upload failed: ' + (err.message || err), 'error');
      }).then(function () { attachBtn.classList.remove('busy'); });
    }

    attachBtn.addEventListener('click', function () { fileInput.click(); });
    fileInput.addEventListener('change', function (e) {
      var f = e.target.files && e.target.files[0];
      fileInput.value = '';
      if (f) uploadFile(f);
    });

    // Paste an image straight into the composer.
    input.addEventListener('paste', function (e) {
      var items = (e.clipboardData && e.clipboardData.items) || [];
      for (var i = 0; i < items.length; i++) {
        if (items[i].kind === 'file') {
          var f = items[i].getAsFile();
          if (f) { e.preventDefault(); uploadFile(f); return; }
        }
      }
    });

    // Drag a file anywhere onto the chat to attach it.
    var dropHint = null;
    function showDrop(on) {
      if (on && !dropHint) {
        dropHint = el('div', 'cc-drop', 'Drop to attach');
        root.style.position = 'relative';
        root.appendChild(dropHint);
      } else if (!on && dropHint) { dropHint.remove(); dropHint = null; }
    }
    root.addEventListener('dragover', function (e) {
      if (!e.dataTransfer || !Array.prototype.includes.call(e.dataTransfer.types || [], 'Files')) return;
      e.preventDefault(); e.stopPropagation();
      e.dataTransfer.dropEffect = 'copy';
      showDrop(true);
    });
    root.addEventListener('dragleave', function (e) { if (!root.contains(e.relatedTarget)) showDrop(false); });
    root.addEventListener('drop', function (e) {
      var f = e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files[0];
      if (!f) { showDrop(false); return; }
      e.preventDefault(); e.stopPropagation();
      showDrop(false);
      uploadFile(f);
    });

    // ─── Composer behaviour ───

    function autoGrow() {
      input.style.height = 'auto';
      input.style.height = Math.min(input.scrollHeight, 180) + 'px';
    }

    function hideSlash() {
      if (slashBox) { slashBox.remove(); slashBox = null; }
    }

    function showSlash() {
      var v = input.value;
      // Only offer commands while the whole message is a single leading /token.
      var m = v.match(/^\/([a-z0-9_:-]*)$/i);
      if (!m || !slashCommands.length) { hideSlash(); return; }
      var q = m[1].toLowerCase();
      var hits = slashCommands.filter(function (c) {
        var n = (typeof c === 'string' ? c : c.name) || '';
        return n.toLowerCase().indexOf(q) === 0;
      }).slice(0, 40);
      if (!hits.length) { hideSlash(); return; }
      if (!slashBox) { slashBox = el('div', 'cc-slash'); wrap.appendChild(slashBox); }
      slashIdx = Math.min(slashIdx, hits.length - 1);
      slashBox.innerHTML = '';
      hits.forEach(function (c, i) {
        var name = (typeof c === 'string' ? c : c.name) || '';
        var desc = (typeof c === 'string' ? '' : (c.description || ''));
        var it = el('div', 'cc-slash-item' + (i === slashIdx ? ' on' : ''),
          '<b>/' + esc(name) + '</b><span>' + esc(desc) + '</span>');
        it.addEventListener('mousedown', function (ev) {
          ev.preventDefault();
          input.value = '/' + name + ' ';
          hideSlash();
          input.focus();
        });
        slashBox.appendChild(it);
      });
      slashBox._hits = hits;
    }

    function onKeyDown(ev) {
      if (slashBox && slashBox._hits) {
        if (ev.key === 'ArrowDown') { ev.preventDefault(); slashIdx = Math.min(slashIdx + 1, slashBox._hits.length - 1); showSlash(); return; }
        if (ev.key === 'ArrowUp') { ev.preventDefault(); slashIdx = Math.max(slashIdx - 1, 0); showSlash(); return; }
        if (ev.key === 'Tab' || (ev.key === 'Enter' && !ev.shiftKey)) {
          ev.preventDefault();
          var c = slashBox._hits[slashIdx];
          input.value = '/' + ((typeof c === 'string' ? c : c.name) || '') + ' ';
          hideSlash();
          return;
        }
        if (ev.key === 'Escape') { hideSlash(); return; }
      }
      if (ev.key !== 'Enter') return;
      if (enterMode === 'send') {
        // Enter sends; Shift+Enter (and Ctrl+Enter) fall through to a newline.
        if (!ev.shiftKey && !ev.ctrlKey && !ev.metaKey) { ev.preventDefault(); doSend(); }
      } else if (ev.ctrlKey || ev.metaKey) {
        ev.preventDefault(); doSend();
      }
    }

    input.addEventListener('keydown', onKeyDown);
    input.addEventListener('input', function () { slashIdx = 0; autoGrow(); showSlash(); });
    input.addEventListener('blur', function () { setTimeout(hideSlash, 120); });
    sendBtn.addEventListener('click', doSend);
    stopBtn.addEventListener('click', doStop);
    modeSel.addEventListener('change', function () {
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/permission-mode', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mode: modeSel.value }),
      }).catch(function () {});
    });

    // ─── Event application ───

    function addEntry(data) {
      var node = renderEntry(data);
      entries.set(data.id, { data: data, node: node });
      var stick = atBottom();
      log.appendChild(node);
      scrollDown(stick);
    }

    function applyHistory(session) {
      if (destroyed || !session) return;
      log.innerHTML = '';
      entries.clear();
      (session.messages || []).forEach(addEntry);
      slashCommands = session.slashCommands || [];
      modeSel.value = session.permissionMode || 'default';
      modelLbl.textContent = session.model || '';
      setSessionId(session.sessionId);
      if (session.totalCostUsd) costLbl.textContent = '$' + session.totalCostUsd.toFixed(4);
      setState(session.state || 'idle');
      input.disabled = session.status !== 'running';
      if (session.status !== 'running') {
        input.placeholder = 'Session ended.';
        stateLbl.textContent = 'exited';
      }
      scrollDown(true);
    }

    function applyEvent(ev) {
      if (destroyed || !ev) return;
      switch (ev.type) {
        case 'entry': addEntry(ev.entry); break;
        case 'patch': {
          var rec = entries.get(ev.id);
          if (!rec) break;
          Object.assign(rec.data, ev.patch);
          var stick = atBottom();
          paint(rec.node, rec.data);
          scrollDown(stick);
          break;
        }
        case 'delta': {
          var r = entries.get(ev.id);
          if (!r) break;
          r.data.text = (r.data.text || '') + ev.text;
          // Repaint just the text node; markdown is cheap enough per delta and
          // keeps fences/lists correct as they stream in.
          var stick2 = atBottom();
          paint(r.node, r.data);
          scrollDown(stick2);
          break;
        }
        case 'state': setState(ev.state); break;
        case 'init':
          slashCommands = ev.slashCommands || [];
          if (ev.model) modelLbl.textContent = ev.model;
          // The CLI only emits its session id once the first turn starts.
          setSessionId(ev.sessionId);
          break;
        case 'meta':
          if (ev.permissionMode) modeSel.value = ev.permissionMode;
          if (ev.model) modelLbl.textContent = ev.model;
          break;
        case 'exit':
          setState('idle');
          input.disabled = true;
          input.placeholder = 'Session ended.';
          stateLbl.textContent = 'exited';
          break;
      }
    }

    // Ask the server to stream this session (also replays history).
    wsSend({ type: 'subscribe-ui', id: sessionId });

    return {
      applyHistory: applyHistory,
      applyEvent: applyEvent,
      focus: function () { try { input.focus(); } catch (e) {} },
      resubscribe: function () { wsSend({ type: 'subscribe-ui', id: sessionId }); },
      destroy: function () {
        destroyed = true;
        try { wsSend({ type: 'unsubscribe-ui', id: sessionId }); } catch (e) {}
        input.removeEventListener('keydown', onKeyDown);
        hideSlash();
        try { host.removeChild(root); } catch (e) {}
        entries.clear();
      },
    };
  }

  window.CrundiChat = { mount: mount };
})();
