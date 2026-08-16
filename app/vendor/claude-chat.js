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
    '.cc-root.cc-drop{outline:2px dashed var(--accent);outline-offset:-2px}',
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
    '.cc-thought{display:flex;align-items:center;gap:6px;color:var(--text-muted);font-size:12px;font-style:italic;user-select:none}',
    '.cc-thought-dot{width:5px;height:5px;border-radius:50%;background:var(--text-muted);opacity:.55;flex:0 0 auto}',
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
    // Narrow cell (phone, or a slim mosaic column): the message box gets the
    // full width on its own row and the controls sit underneath, instead of all
    // five competing for one line and squeezing the box to half width.
    '.cc-narrow .cc-inrow{flex-wrap:wrap}',
    '.cc-narrow .cc-input{flex:1 1 100%;order:1}',
    '.cc-narrow .cc-actions{order:2;display:flex;gap:7px;width:100%;align-items:center}',
    '.cc-narrow .cc-actions .cc-btn{flex:1;padding:8px 10px}',
    '.cc-narrow .cc-attach,.cc-narrow .cc-stop{flex:none}',
    '.cc-narrow .cc-meta{gap:6px;font-size:10.5px}',
    '.cc-narrow .cc-sid{display:none}',          // duplicated by the cell header
    '.cc-actions{display:contents}',              // wide: behaves as if unwrapped
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
    '.cc-toggle b{font-family:var(--mono);font-weight:600}',
    // Enter-key switch: both options always visible, thumb marks the live one.
    '.cc-sw{position:relative;display:inline-flex;align-items:center;background:var(--bg-tertiary,rgba(255,255,255,.06));border-radius:999px;padding:2px;line-height:1}',
    '.cc-sw-thumb{position:absolute;top:2px;bottom:2px;left:2px;width:calc(50% - 2px);border-radius:999px;background:var(--accent-dim);border:1px solid var(--accent);transition:transform .18s cubic-bezier(.4,0,.2,1)}',
    '.cc-sw.alt .cc-sw-thumb{transform:translateX(100%)}',
    '.cc-sw-opt{position:relative;z-index:1;padding:2px 8px;font-size:11px;opacity:.45;transition:opacity .18s ease}',
    '.cc-sw:not(.alt) .cc-sw-opt:nth-child(2),.cc-sw.alt .cc-sw-opt:nth-child(3){opacity:1}',
    '.cc-sw:not(.alt) .cc-sw-opt:nth-child(2) b,.cc-sw.alt .cc-sw-opt:nth-child(3) b{color:var(--accent-hover)}',
    // The switch shows WHICH key; this says what it does.
    '.cc-sw-lbl{font-size:11px;color:inherit}',
    // Stop: icon by default, widens to reveal "Sure?" once armed.
    '.cc-stop{flex:none;height:34px;min-width:34px;display:inline-flex;align-items:center;justify-content:center;border:1px solid var(--border);background:var(--bg-primary);color:var(--red);border-radius:var(--radius-sm);cursor:pointer;padding:0 8px;transition:.15s}',
    '.cc-stop:hover{color:var(--red);border-color:var(--red)}',
    '.cc-stop svg{width:15px;height:15px;flex:none}',
    '.cc-stop .cc-stop-label{max-width:0;margin-left:0;overflow:hidden;white-space:nowrap;font-size:11px;font-family:inherit;opacity:0;transition:max-width .18s ease,margin-left .18s ease,opacity .18s ease}',
    '.cc-stop.cc-armed{color:var(--red);border-color:var(--red);background:var(--red-dim)}',
    '.cc-stop.cc-armed .cc-stop-label{max-width:44px;margin-left:5px;opacity:1}',
    // Attach button matches the main input bar's paperclip.
    '.cc-attach{flex:none;width:34px;height:34px;display:inline-flex;align-items:center;justify-content:center;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-secondary);border-radius:var(--radius-sm);cursor:pointer;padding:0}',
    '.cc-attach:hover{color:var(--accent-hover);border-color:var(--accent)}',
    '.cc-attach.busy{opacity:.55;pointer-events:none}',
    '.cc-attach svg{width:16px;height:16px}',
    // Queued input: one bubble however many lines were added, click to reclaim.
    '.cc-queue{margin-bottom:7px;border:1px dashed rgba(99,102,241,.55);background:rgba(99,102,241,.08);border-radius:10px;padding:7px 11px;cursor:pointer;transition:.14s}',
    '.cc-queue:hover{border-color:var(--accent);background:var(--accent-dim)}',
    '.cc-queue-head{display:flex;align-items:center;gap:6px;font-size:10.5px;text-transform:uppercase;letter-spacing:.05em;color:var(--accent-hover);font-weight:700;margin-bottom:3px}',
    '.cc-queue-body{white-space:pre-wrap;word-break:break-word;color:var(--text-primary);font-size:12.5px}',
    '.cc-queue-hint{font-size:10.5px;color:var(--text-muted);margin-top:4px}',
    // In-log activity row: some turns (notably /compact) stream nothing at all,
    // so without this the log looks frozen while the header says "working".
    '.cc-activity{display:flex;align-items:center;gap:8px;color:var(--text-muted);font-size:12px;font-style:italic;padding:2px 0}',
    '.cc-activity .cc-spin{width:11px;height:11px}',
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
    // Icon button between attach and send. Interrupting mid-turn is
    // destructive and easy to hit by accident on a phone, so it arms on the
    // first tap and only fires on the second.
    var stopBtn = el('button', 'cc-stop',
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"'
      + ' stroke-linecap="round" stroke-linejoin="round"><rect x="6" y="6" width="12" height="12" rx="2"/></svg>'
      + '<span class="cc-stop-label">Sure?</span>');
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
    // Wrapped in .cc-actions so a narrow cell can move them to their own row;
    // at full width the wrapper is display:contents and changes nothing.
    var actions = el('div', 'cc-actions');
    actions.appendChild(attachBtn);
    actions.appendChild(stopBtn);
    actions.appendChild(sendBtn);
    inrow.appendChild(input);
    inrow.appendChild(actions);
    inrow.appendChild(fileInput);

    var meta = el('div', 'cc-meta');
    var stateLbl = el('span', '', 'idle');
    // Only modes the CLI will actually accept at runtime. bypassPermissions is
    // deliberately absent: it can only be set at launch, and offering it here
    // meant the dropdown showed a mode that was never in effect.
    var modeSel = el('select', 'cc-sel');
    var MODES = [['default', 'ask permissions'], ['acceptEdits', 'accept edits'],
      ['auto', 'auto'], ['plan', 'plan mode'], ['dontAsk', "don't ask"]];
    function buildModes(isBypass) {
      modeSel.innerHTML = '';
      // A bypass session is fixed for its lifetime — show that and nothing else.
      (isBypass ? [['bypassPermissions', 'bypass all']] : MODES).forEach(function (m) {
        var o = el('option'); o.value = m[0]; o.textContent = m[1]; modeSel.appendChild(o);
      });
      modeSel.disabled = !!isBypass;
      modeSel.title = isBypass
        ? 'This chat was launched with permissions bypassed — the mode is fixed for its lifetime'
        : 'Permission mode for this chat';
    }
    buildModes(false);
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
    var queued = [];           // lines typed while busy; flushed as one message
    var queueNode = null;      // the single "Queued" bubble above the composer
    var queueTimer = null;     // ticker that decides when the batch goes out
    var lastTypeAt = 0;        // last keystroke, so we never send mid-thought
    var activityNode = null;   // in-log "working" row for turns that stream nothing

    // Enter behaviour: 'send' = Enter sends / Shift+Enter newline;
    // 'newline' = Enter newline / Ctrl+Enter sends. Shared by every chat cell.
    var ENTER_KEY = 'crundi_chat_enter';
    var enterMode = 'send';
    try { if (localStorage.getItem(ENTER_KEY) === 'newline') enterMode = 'newline'; } catch (e) {}

    function syncEnterMode() {
      var sends = enterMode === 'send';
      // Both options stay on screen inside a switch track, with the thumb over
      // the active one — the state is readable without clicking to find out.
      enterBtn.innerHTML =
        '<span class="cc-sw' + (sends ? '' : ' alt') + '">'
        + '<span class="cc-sw-thumb"></span>'
        + '<span class="cc-sw-opt"><b>⏎</b></span>'
        + '<span class="cc-sw-opt"><b>⌃⏎</b></span>'
        + '</span><span class="cc-sw-lbl">sends</span>';
      enterBtn.title = sends
        ? 'Enter sends, Shift+Enter makes a newline — click to swap'
        : 'Enter makes a newline, Ctrl+Enter sends — click to swap';
      // The long hint wraps to three lines in a phone-width cell and buries the
      // box; the toggle beside it already says which key sends.
      if (narrow) input.placeholder = 'Message Claude…';
      else input.placeholder = sends
        ? 'Message Claude…  (Enter to send, Shift+Enter for newline)'
        : 'Message Claude…  (Ctrl+Enter to send, Enter for newline)';
    }

    // Track the CELL's width, not the viewport: a chat can be a slim mosaic
    // column on a wide screen and needs the same compact treatment.
    var narrow = false;
    function syncWidth() {
      var w = root.clientWidth || 9999;
      var want = w < 520;
      if (want === narrow) return;
      narrow = want;
      root.classList.toggle('cc-narrow', narrow);
      syncEnterMode();
    }
    var widthObserver = null;
    if (window.ResizeObserver) {
      widthObserver = new ResizeObserver(syncWidth);
      widthObserver.observe(root);
    } else {
      window.addEventListener('resize', syncWidth);
    }
    setTimeout(syncWidth, 0);
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

    // ─── Scroll retention across re-parenting ───
    //
    // The workbench re-parents cell elements whenever it re-renders the grid
    // (mosaic arrange / replaceChildren), and moving a node resets scrollTop on
    // every scrollable descendant — so returning to a chat would land at the
    // top. The same happens when a hidden cell (display:none, clientHeight 0)
    // is shown again. Remember where the reader was and put them back.
    var stickBottom = true;   // fresh chats start pinned to the newest message
    var lastTop = 0;
    var lastH = 0;
    log.addEventListener('scroll', function () {
      if (!log.clientHeight) return; // a reset while hidden is not a user scroll
      stickBottom = atBottom();
      lastTop = log.scrollTop;
    });
    function restoreScroll() {
      if (!log.clientHeight) return;
      // scroll-behavior:smooth would animate the restore (and lose a race with
      // the next render); a restore must be instant.
      var prev = log.style.scrollBehavior;
      log.style.scrollBehavior = 'auto';
      log.scrollTop = stickBottom ? log.scrollHeight : lastTop;
      log.style.scrollBehavior = prev;
    }
    // Self-heal for visibility toggles the host does not tell us about, and
    // for the on-screen keyboard: opening it shrinks the viewport, so the log
    // gets shorter while scrollTop stays put and a reader who was pinned to the
    // newest message silently ends up above it.
    if (window.ResizeObserver) {
      var logObserver = new ResizeObserver(function () {
        var h = log.clientHeight;
        if (h && !lastH) restoreScroll();          // 0 → visible: position wiped
        else if (h !== lastH && stickBottom) restoreScroll(); // resized while pinned
        lastH = h;
      });
      try { logObserver.observe(log); } catch (e) { logObserver = null; }
    }
    // iOS Safari resizes the visual viewport without necessarily resizing the
    // log element, so the observer above can miss the keyboard entirely.
    var vv = window.visualViewport;
    function onViewport() { if (stickBottom) restoreScroll(); }
    if (vv) { vv.addEventListener('resize', onViewport); vv.addEventListener('scroll', onViewport); }
    // Focusing the composer is the strongest signal the keyboard is coming;
    // the geometry settles a beat after the event, hence the delayed re-pin.
    input.addEventListener('focus', function () {
      if (!stickBottom) return;
      restoreScroll();
      setTimeout(restoreScroll, 150);
      setTimeout(restoreScroll, 400);
    });

    function setState(s) {
      var was = state;
      state = s;
      var busy = s === 'working';
      stateLbl.textContent = busy ? 'working…' : s === 'needs-input' ? 'needs your input' : 'idle';
      stateLbl.className = busy || s === 'needs-input' ? 'cc-busy' : '';
      stopBtn.style.display = busy ? '' : 'none';
      if (!busy) disarmStop(); // never leave it primed across turns
      // Send stays available while busy — it queues rather than sending, so the
      // label says so instead of the button vanishing.
      sendBtn.style.display = '';
      sendBtn.textContent = s === 'idle' ? 'Send' : 'Queue';
      syncActivity();
      // Turn finished: send everything typed in the meantime as one message.
      if (s === 'idle' && was !== 'idle') flushQueue();
    }

    // Some turns produce no visible output for a long time — /compact is the
    // clearest case, since the CLI does the summarising internally and streams
    // nothing until it is done. Without a row in the log the conversation looks
    // frozen even though the header badge says working.
    function syncActivity() {
      if (state !== 'working') {
        if (activityNode) { activityNode.remove(); activityNode = null; }
        return;
      }
      var last = null;
      for (var i = log.children.length - 1; i >= 0; i--) {
        var rec = entries.get(log.children[i].dataset.id);
        if (rec && rec.data.kind === 'user') { last = rec.data; break; }
        if (rec && (rec.data.kind === 'assistant-text' || rec.data.kind === 'tool')) break;
      }
      var label = (last && String(last.text || '').trim().indexOf('/compact') === 0)
        ? 'Compacting the conversation… this can take a while on a long session'
        : 'Working…';
      if (!activityNode) {
        activityNode = el('div', 'cc-activity', '<span class="cc-spin"></span><span></span>');
        log.appendChild(activityNode);
      } else if (activityNode.parentNode !== log || activityNode.nextSibling) {
        log.appendChild(activityNode); // keep it pinned to the bottom
      }
      activityNode.lastChild.textContent = label;
      scrollDown(false);
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
      // Models from Opus 4.7 on return thinking blocks with no text (see
      // handleStreamEvent in claude-ui.js). An expander over an empty body
      // reads as broken, so show a flat chip instead — same signal that
      // reasoning happened, no affordance promising content that isn't there.
      if (!e.text) {
        var n = e.tokens;
        return el('div', 'cc-thought',
          '<span class="cc-thought-dot"></span><span>Thought'
          + (n ? ' for ~' + (n >= 1000 ? (n / 1000).toFixed(1) + 'k' : n) + ' tokens' : '')
          + '</span>');
      }
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

    function postMessage(text) {
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text: text }),
      }).then(function (r) { return r.json(); }).then(function (d) {
        if (d && d.ok === false) appendLocal({ kind: 'error', text: d.error || 'Failed to send' });
      }).catch(function (err) { appendLocal({ kind: 'error', text: String(err.message || err) }); });
    }

    // ─── Queued input ───
    // Typing while Claude is busy batches into ONE message, then goes out
    // mid-turn: the CLI picks stdin up at the next tool boundary and acts on it
    // without waiting for the turn to finish (verified — an injection after the
    // first Bash call was obeyed two seconds later, in the same turn). So the
    // queue only exists to group a fast burst of lines and give a moment to take
    // them back; it is flushed shortly after typing stops, not at turn end.
    // The CLI accepts injected input at any pause in the turn — between thinking
    // steps as well as tool calls — and places it itself. So the host has no
    // reason to wait for a particular boundary; the only reason to hold at all is
    // to batch what is still being typed.
    //
    // The first attempt debounced from the last Enter, which fired while the user
    // was still typing the NEXT line and split a batch into separate messages.
    // The quiet period is measured from the last KEYSTROKE instead, so a pending
    // batch keeps waiting for as long as typing continues.
    var QUIET_MS = 1500;
    var TICK_MS = 300;

    function enqueue(text) {
      queued.push(text);
      lastTypeAt = Date.now();
      renderQueue();
      startTicker();
    }

    function startTicker() {
      if (queueTimer) return;
      queueTimer = setInterval(function () {
        if (!queued.length || destroyed) { stopTicker(); return; }
        // Never inject while a permission prompt is outstanding: the CLI is
        // blocked waiting for a control_response, so that must be answered first.
        if (state === 'needs-input') return;
        if (Date.now() - lastTypeAt >= QUIET_MS) flushQueue();
      }, TICK_MS);
    }

    function stopTicker() {
      if (queueTimer) { clearInterval(queueTimer); queueTimer = null; }
    }

    function queuedText() { return queued.join('\n'); }

    // Pull every queued line back into the composer for editing, newest last.
    function unqueue() {
      if (!queued.length) return;
      stopTicker();
      var restored = queuedText();
      queued = [];
      renderQueue();
      var cur = input.value.trim();
      input.value = cur ? restored + '\n' + cur : restored;
      autoGrow();
      input.focus();
      try { input.setSelectionRange(input.value.length, input.value.length); } catch (e) {}
    }

    function renderQueue() {
      if (!queued.length) {
        if (queueNode) { queueNode.remove(); queueNode = null; }
        return;
      }
      if (!queueNode) {
        queueNode = el('div', 'cc-queue');
        queueNode.title = 'Click to edit — brings every queued line back to the message box';
        queueNode.addEventListener('click', unqueue);
        wrap.insertBefore(queueNode, inrow);
      }
      var n = queued.length;
      queueNode.innerHTML = '<div class="cc-queue-head">'
        + '<span>Sending</span><span style="opacity:.7;font-weight:500;text-transform:none;letter-spacing:0">'
        + (n === 1 ? '1 line' : n + ' lines') + ' · as one message, at the next tool call</span></div>'
        + '<div class="cc-queue-body">' + esc(queuedText()) + '</div>'
        + '<div class="cc-queue-hint">Click to take it back</div>';
    }

    function flushQueue() {
      stopTicker();

      if (!queued.length || destroyed) return;
      var text = queuedText();
      queued = [];
      renderQueue();
      postMessage(text);
    }

    function doSend() {
      var text = input.value.replace(/\s+$/, '');
      if (!text.trim()) return;
      input.value = '';
      saveDraft();
      autoGrow();
      // Sending is an explicit "I want to see what happens next", so re-pin to
      // the bottom even if the reader had scrolled up to check something.
      stickBottom = true;
      scrollDown(true);
      hideSlash();
      // Busy (working, or blocked on a prompt) → queue it for the next turn.
      if (state !== 'idle') { enqueue(text); return; }
      postMessage(text);
      // Go busy immediately rather than waiting for the server's state event —
      // otherwise a fast second Enter still sees 'idle' and fires a competing
      // message instead of queueing behind this one.
      setState('working');
    }

    // Two-step stop. The first click arms and reveals "Sure?"; a second within
    // the window interrupts. Auto-disarms so a stray tap can't leave it primed.
    var STOP_ARM_MS = 3000;
    var stopArmedAt = 0;
    var stopTimer = null;
    function disarmStop() {
      stopArmedAt = 0;
      clearTimeout(stopTimer);
      stopBtn.classList.remove('cc-armed');
      stopBtn.title = 'Stop this turn (click twice)';
    }
    function doStop() {
      if (!stopArmedAt || Date.now() - stopArmedAt > STOP_ARM_MS) {
        stopArmedAt = Date.now();
        stopBtn.classList.add('cc-armed');
        stopBtn.title = 'Click again to interrupt';
        clearTimeout(stopTimer);
        stopTimer = setTimeout(disarmStop, STOP_ARM_MS);
        return;
      }
      disarmStop();
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/interrupt', { method: 'POST' }).catch(function () {});
    }
    disarmStop();

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
      saveDraft();
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

    // ─── Drop target ───
    //
    // Two sources, same as a terminal cell:
    //   • Workbench panel rows (Files / Git / Kanban / Mindmap / Media) drag a
    //     text/plain ref like "[File Path: C:\p\x.js]".
    //   • The OS drags real files, which arrive on dataTransfer.files.
    // Images upload to crundi_attachments and insert the returned path; other
    // files insert their path directly. stopPropagation keeps the workbench's
    // .terminal-wrap handler from also routing the drop to the bottom input bar.

    function onDragOver(e) {
      if (!e.dataTransfer) return;
      e.preventDefault();
      e.stopPropagation();
      e.dataTransfer.dropEffect = 'copy';
      root.classList.add('cc-drop');
    }
    function onDragLeave(e) {
      if (!root.contains(e.relatedTarget)) root.classList.remove('cc-drop');
    }
    function onDrop(e) {
      if (!e.dataTransfer) return;
      e.preventDefault();
      e.stopPropagation();
      root.classList.remove('cc-drop');
      var files = e.dataTransfer.files;
      if (files && files.length) {
        var paths = [];
        for (var i = 0; i < files.length; i++) {
          var f = files[i];
          if (f.type && f.type.indexOf('image/') === 0) { uploadFile(f); continue; }
          // Electron exposes the real path via the preload bridge; a plain
          // browser gives us only the name, same limitation as the input bar.
          var p = (window.api && window.api.getPathForFile && window.api.getPathForFile(f)) || f.path || f.name;
          if (p) paths.push(p);
        }
        if (paths.length) {
          insertPath(paths.join(' '));
          toast(paths.length === 1 ? 'File added' : paths.length + ' files added');
        }
        return;
      }
      var text = e.dataTransfer.getData('text/plain');
      if (text) insertPath(text);
    }
    root.addEventListener('dragover', onDragOver);
    root.addEventListener('dragleave', onDragLeave);
    root.addEventListener('drop', onDrop);

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

    // ─── Draft persistence ───
    // Switching project or tab tears the cell down and rebuilds it, so an
    // unsent message would be lost. Keyed per session, and deliberately NOT
    // cleared on destroy() — destroy is exactly the case we're protecting.
    var DRAFT_KEY = 'crundi_chat_draft_' + sessionId;
    function saveDraft() {
      try {
        if (input.value) localStorage.setItem(DRAFT_KEY, input.value);
        else localStorage.removeItem(DRAFT_KEY);
      } catch (e) {}
    }
    try {
      var draft = localStorage.getItem(DRAFT_KEY);
      if (draft) { input.value = draft; setTimeout(autoGrow, 0); }
    } catch (e) {}

    input.addEventListener('keydown', onKeyDown);
    input.addEventListener('input', function () {
      lastTypeAt = Date.now(); // keeps a pending batch waiting while you type
      slashIdx = 0; autoGrow(); showSlash(); saveDraft();
    });
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
      // The activity row is not a conversation entry; keep it last.
      if (activityNode && activityNode.parentNode === log) log.appendChild(activityNode);
      scrollDown(stick);
    }

    function applyHistory(session) {
      if (destroyed || !session) return;
      log.innerHTML = '';
      entries.clear();
      activityNode = null; // detached by the wipe above; setState re-creates it
      (session.messages || []).forEach(addEntry);
      slashCommands = session.slashCommands || [];
      buildModes(session.skipPermissions);
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
      stickBottom = true; // a full replay always lands on the newest message
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
        // Server spliced older messages in front of the live ones (a resumed
        // conversation whose stored transcript was replayed) — take the whole
        // snapshot rather than trying to merge.
        case 'history': applyHistory(ev.session); break;
        case 'state': setState(ev.state); break;
        case 'init':
          slashCommands = ev.slashCommands || [];
          if (ev.model) modelLbl.textContent = ev.model;
          if (ev.permissionMode) modeSel.value = ev.permissionMode;
          // Fires twice: once when the process is ready (no session id yet) and
          // again on the first turn, which is when the CLI reveals its id.
          setSessionId(ev.sessionId);
          break;
        case 'meta':
          if (ev.permissionMode) modeSel.value = ev.permissionMode;
          if (ev.model) modelLbl.textContent = ev.model;
          break;
        case 'exit':
          // Nothing will flush the queue now, so hand the text back rather than
          // silently dropping what the user typed.
          if (queued.length) unqueue();
          setState('idle');
          if (activityNode) { activityNode.remove(); activityNode = null; }
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
      restoreScroll: restoreScroll,
      // Used by the workbench's pointer-drag (touch) path, which can't rely on
      // HTML5 drag events — see insertRefToTarget in webapp-html.js.
      insertText: function (text) { if (text) insertPath(text); },
      destroy: function () {
        destroyed = true;
        stopTicker(); // closing a cell must not leave an interval running
        root.removeEventListener('dragover', onDragOver);
        root.removeEventListener('dragleave', onDragLeave);
        root.removeEventListener('drop', onDrop);
        if (logObserver) { try { logObserver.disconnect(); } catch (e) {} }
        if (vv) { vv.removeEventListener('resize', onViewport); vv.removeEventListener('scroll', onViewport); }
        if (widthObserver) { try { widthObserver.disconnect(); } catch (e) {} }
        else window.removeEventListener('resize', syncWidth);
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
