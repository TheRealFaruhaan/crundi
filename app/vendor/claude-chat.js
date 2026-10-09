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
    '.cc-root{display:flex;flex-direction:column;height:100%;min-height:0;background:var(--bg-primary);font-size:calc(13px*var(--cc-fs,1));line-height:1.55}',
    // Holds the scrolling log plus the floating agent dock, so the dock
    // anchors just above the composer and does not scroll away with the log.
    '.cc-logwrap{position:relative;flex:1;min-height:0;display:flex;flex-direction:column}',
    '.cc-root.cc-dropping{outline:2px dashed var(--accent);outline-offset:-2px}',
    '.cc-log{flex:1;overflow-y:auto;overflow-x:hidden;padding:12px 12px 4px;scroll-behavior:smooth}',
    // While text in a conversation is selected, nothing else on the page can be.
    // See "Selection stays in the conversation" in mount(). The path classes
    // keep the log's own ancestors out of it: user-select:none on an ancestor
    // is inherited by everything under it that says "auto", the log included.
    'html.cc-selecting *:not(.cc-sel-path):not(.cc-sel-host):not(.cc-sel-host *){-webkit-user-select:none!important;user-select:none!important}',
    '.cc-entry{margin-bottom:10px;animation:cc-in .18s ease}',
    '@keyframes cc-in{from{opacity:0;transform:translateY(3px)}to{opacity:1;transform:none}}',

    '.cc-user{display:flex;justify-content:flex-end}',
    '.cc-user.by-owner{flex-direction:column;align-items:flex-end}',
    '.cc-user-by{font-size:calc(11px*var(--cc-fs,1));font-weight:600;color:var(--sky,#38bdf8);margin:0 4px 2px 0}',
    '.cc-user-body{max-width:86%;background:var(--accent-dim);border:1px solid rgba(99,102,241,.35);color:var(--text-primary);padding:7px 11px;border-radius:12px 12px 3px 12px;white-space:pre-wrap;word-break:break-word}',

    '.cc-assistant{color:var(--text-primary);word-break:break-word}',
    '.cc-assistant p{margin:0 0 8px}.cc-assistant p:last-child{margin-bottom:0}',
    '.cc-assistant ul,.cc-assistant ol{margin:0 0 8px;padding-left:20px}',
    '.cc-assistant li{margin:2px 0}',
    '.cc-assistant h1,.cc-assistant h2,.cc-assistant h3{margin:12px 0 6px;font-size:calc(14px*var(--cc-fs,1));font-weight:650;color:var(--text-primary)}',
    '.cc-assistant a,.cc-user-body a{color:var(--accent-hover);text-decoration:underline;text-underline-offset:2px;word-break:break-all}',
    '.cc-user-body a{color:inherit}',
    // A path the host can open: dotted underline, so it reads as a file, not a web link.
    '.cc-path{cursor:pointer;text-decoration:underline dotted;text-underline-offset:3px;word-break:break-all}',
    '.cc-path:hover{color:var(--accent-hover);text-decoration-style:solid}',
    // Hover copy button for code, links and paths in Claude's replies.
    '.cc-copyhint{position:fixed;z-index:60;display:none;align-items:center;justify-content:center;width:24px;height:22px;padding:0;border:1px solid var(--border,#2a2a3d);border-radius:6px;background:var(--bg-secondary,#12121a);color:var(--text-secondary);cursor:pointer;box-shadow:0 4px 12px rgba(0,0,0,.35)}',
    '.cc-copyhint.on{display:inline-flex}',
    '.cc-copyhint:hover{color:var(--text-primary);background:var(--bg-hover,#22223a)}',
    '.cc-copyhint.done{color:var(--green,#22c55e)}',
    '.cc-copyhint svg{width:13px;height:13px}',
    '.cc-assistant code{font-family:var(--mono);font-size:calc(12px*var(--cc-fs,1));background:var(--bg-tertiary,rgba(255,255,255,.06));padding:1px 5px;border-radius:4px}',
    '.cc-assistant pre{margin:0 0 8px;background:var(--bg-secondary,#111119);border:1px solid var(--border-subtle);border-radius:var(--radius-sm);padding:9px 11px;overflow-x:auto}',
    '.cc-assistant pre code{background:none;padding:0;font-size:calc(12px*var(--cc-fs,1));line-height:1.5}',
    '.cc-assistant blockquote{margin:0 0 8px;padding-left:10px;border-left:2px solid var(--border);color:var(--text-secondary)}',
    '.cc-cursor{display:inline-block;width:6px;height:13px;background:var(--accent);vertical-align:-2px;animation:cc-blink 1s steps(2) infinite}',
    '@keyframes cc-blink{0%,50%{opacity:1}51%,100%{opacity:0}}',

    '.cc-think{border-left:2px solid var(--border);padding-left:9px;color:var(--text-muted);font-size:calc(12px*var(--cc-fs,1));font-style:italic}',
    '.cc-think-head{cursor:pointer;user-select:none;color:var(--text-secondary);font-style:normal;display:flex;align-items:center;gap:5px}',
    '.cc-think-body{margin-top:4px;white-space:pre-wrap;max-height:260px;overflow-y:auto;transition:max-height .28s ease,opacity .2s ease,margin-top .28s ease}',
    // Thinking folds away with a short slide instead of vanishing.
    '.cc-think.cc-collapsed .cc-think-body{display:block;max-height:0;opacity:0;margin-top:0;overflow:hidden}',
    // While it is thinking: the label shimmers and a caret blinks at the end.
    '.cc-think.live .cc-think-head span:last-child{background:linear-gradient(90deg,var(--text-secondary) 0%,var(--text-primary) 45%,var(--text-secondary) 90%);background-size:220% 100%;-webkit-background-clip:text;background-clip:text;color:transparent;animation:cc-shimmer 1.6s linear infinite}',
    '@keyframes cc-shimmer{from{background-position:120% 0}to{background-position:-120% 0}}',
    '.cc-think-caret{display:inline-block;width:2px;height:1em;margin-left:2px;vertical-align:-2px;background:var(--text-muted);animation:cc-blink 1s steps(1) infinite}',
    '@keyframes cc-blink{50%{opacity:0}}',
    '@media (prefers-reduced-motion: reduce){.cc-think.live .cc-think-head span:last-child{animation:none;color:var(--text-secondary);background:none}.cc-think-caret{animation:none}.cc-think-body{transition:none}}',
    '.cc-collapsed .cc-think-body,.cc-collapsed .cc-tool-body{display:none}',
    '.cc-thought{display:flex;align-items:center;gap:6px;color:var(--text-muted);font-size:calc(12px*var(--cc-fs,1));font-style:italic;user-select:none}',
    '.cc-thought-dot{width:5px;height:5px;border-radius:50%;background:var(--text-muted);opacity:.55;flex:0 0 auto}',
    '.cc-caret{transition:transform .15s}.cc-collapsed .cc-caret{transform:rotate(-90deg)}',

    '.cc-tool{border:1px solid var(--border-subtle);border-radius:var(--radius-sm);background:var(--bg-secondary,rgba(255,255,255,.02));overflow:hidden}',
    '.cc-tool-head{display:flex;align-items:center;gap:7px;padding:6px 9px;cursor:pointer;user-select:none}',
    '.cc-tool-name{font-family:var(--mono);font-size:calc(12px*var(--cc-fs,1));font-weight:600;color:var(--accent-hover)}',
    '.cc-tool-sum{color:var(--text-secondary);font-size:calc(12px*var(--cc-fs,1));overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1;min-width:0}',
    '.cc-tool-body{border-top:1px solid var(--border-subtle);padding:8px 9px;font-family:var(--mono);font-size:calc(11.5px*var(--cc-fs,1));white-space:pre-wrap;word-break:break-word;max-height:340px;overflow:auto;color:var(--text-secondary)}',
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
    '.cc-ask-sub{color:var(--text-secondary);font-size:calc(12px*var(--cc-fs,1));margin-bottom:8px;word-break:break-word}',
    // The choices a closed question offered. Indented and dimmed: they are
    // a record of what was on the table, not buttons — that request is gone.
    '.cc-ask-opt{color:var(--text-muted);font-size:calc(12px*var(--cc-fs,1));margin:0 0 4px 12px;word-break:break-word}',
    '.cc-ask-opt:before{content:"\\2022 ";color:var(--text-muted)}',
    '.cc-ask-reason{color:var(--text-muted);font-size:calc(11.5px*var(--cc-fs,1));margin-bottom:8px}',
    '.cc-ask-pre{font-family:var(--mono);font-size:calc(11.5px*var(--cc-fs,1));background:var(--bg-primary);border:1px solid var(--border-subtle);border-radius:var(--radius-sm);padding:7px 9px;margin-bottom:9px;max-height:200px;overflow:auto;white-space:pre-wrap;word-break:break-word;color:var(--text-secondary)}',
    '.cc-btns{display:flex;gap:7px;flex-wrap:wrap}',
    '.cc-btn{border:1px solid var(--border);background:var(--bg-tertiary,rgba(255,255,255,.05));color:var(--text-primary);padding:6px 12px;border-radius:var(--radius-sm);cursor:pointer;font-size:calc(12px*var(--cc-fs,1));font-family:inherit;transition:.12s}',
    '.cc-btn:hover{border-color:var(--accent);background:var(--accent-dim)}',
    '.cc-btn.primary{background:var(--accent);border-color:var(--accent);color:#fff;font-weight:600}',
    '.cc-btn.primary:hover{background:var(--accent-hover)}',
    '.cc-btn.danger:hover{border-color:var(--red);background:var(--red-dim);color:var(--red)}',
    '.cc-btn:disabled{opacity:.5;cursor:default}',

    '.cc-q{margin-bottom:11px}.cc-q:last-of-type{margin-bottom:9px}',
    '.cc-q-chip{display:inline-block;background:var(--accent);color:#fff;font-size:calc(10.5px*var(--cc-fs,1));font-weight:650;padding:1px 7px;border-radius:99px;margin-bottom:5px;text-transform:uppercase;letter-spacing:.03em}',
    '.cc-q-text{font-weight:600;margin-bottom:7px;color:var(--text-primary)}',
    '.cc-opt{display:flex;gap:8px;align-items:flex-start;border:1px solid var(--border);border-radius:var(--radius-sm);padding:7px 10px;margin-bottom:5px;cursor:pointer;transition:.12s;background:var(--bg-primary)}',
    '.cc-opt:hover{border-color:var(--accent-hover)}',
    '.cc-opt.sel{border-color:var(--accent);background:var(--accent-dim);box-shadow:var(--ring)}',
    '.cc-opt input{margin-top:3px;accent-color:var(--accent);flex:none}',
    // Note on a choice: a button in the corner, and the note under the choice.
    '.cc-opt{position:relative;padding-right:36px}',
    '.cc-opt > div{flex:1;min-width:0}',
    '.cc-opt-nb{position:absolute;top:5px;right:5px;width:26px;height:26px;display:flex;align-items:center;justify-content:center;border:1px solid transparent;border-radius:6px;background:none;color:var(--text-muted);cursor:pointer;padding:0}',
    '.cc-opt-nb svg{width:14px;height:14px}',
    '.cc-opt-nb:hover{color:var(--text-primary);background:var(--bg-tertiary,#1a1a28);border-color:var(--border)}',
    '.cc-opt-nb.on{color:var(--accent-hover)}',
    '.cc-opt-note{display:flex;align-items:center;gap:6px;margin-top:7px}',
    '.cc-opt-note input{flex:1;min-width:0;margin:0;background:var(--bg-primary);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);padding:5px 8px;font-family:inherit;font-size:calc(12px*var(--cc-fs,1))}',
    '.cc-opt-note input:focus{outline:none;border-color:var(--accent);box-shadow:var(--ring)}',
    '.cc-opt-note button{flex:none;width:24px;height:24px;display:flex;align-items:center;justify-content:center;border:none;border-radius:6px;background:none;color:var(--text-muted);cursor:pointer;font-size:calc(12px*var(--cc-fs,1));padding:0}',
    '.cc-opt-note button:hover{color:var(--text-primary);background:rgba(148,163,184,.15)}',
    '.cc-ans-note{flex-basis:100%;color:var(--text-muted);font-size:calc(11.5px*var(--cc-fs,1))}',
    '.cc-opt-label{font-weight:600;color:var(--text-primary)}',
    '.cc-opt-desc{color:var(--text-secondary);font-size:calc(12px*var(--cc-fs,1));margin-top:1px}',
    '.cc-opt-prev{font-family:var(--mono);font-size:calc(11px*var(--cc-fs,1));background:var(--bg-secondary,#111119);border:1px solid var(--border-subtle);border-radius:4px;padding:6px 8px;margin-top:5px;white-space:pre-wrap;overflow-x:auto;color:var(--text-secondary)}',
    '.cc-other{width:100%;box-sizing:border-box;background:var(--bg-primary);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);padding:6px 9px;font-family:inherit;font-size:calc(12px*var(--cc-fs,1));margin-top:4px}',
    '.cc-other:focus{outline:none;border-color:var(--accent);box-shadow:var(--ring)}',
    '.cc-answered{color:var(--text-secondary);font-size:calc(12px*var(--cc-fs,1));display:flex;align-items:center;gap:6px;flex-wrap:wrap}',
    '.cc-answered b{color:var(--text-primary)}',
    '.cc-tag{display:inline-block;background:var(--bg-tertiary,rgba(255,255,255,.06));border:1px solid var(--border-subtle);border-radius:99px;padding:1px 8px;font-size:calc(11px*var(--cc-fs,1))}',
    '.cc-tag.ok{border-color:var(--green);color:var(--green)}',
    '.cc-tag.no{border-color:var(--red);color:var(--red)}',

    '.cc-result{color:var(--text-muted);font-size:calc(11px*var(--cc-fs,1));text-align:center;padding:3px 0;border-top:1px dashed var(--border-subtle)}',
    '.cc-notice{color:var(--text-muted);font-size:calc(11.5px*var(--cc-fs,1));text-align:center;font-style:italic}',
    '.cc-error{color:var(--red);background:var(--red-dim);border:1px solid var(--red);border-radius:var(--radius-sm);padding:7px 10px;font-size:calc(12px*var(--cc-fs,1));word-break:break-word}',

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
    '.cc-narrow .cc-meta{gap:6px;font-size:calc(10.5px*var(--cc-fs,1))}',
    '.cc-narrow .cc-sid{display:none}',          // duplicated by the cell header
    '.cc-actions{display:contents}',              // wide: behaves as if unwrapped
    '.cc-input{flex:1;min-height:34px;max-height:180px;resize:none;background:var(--bg-primary);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);padding:8px 10px;font-family:inherit;font-size:calc(13px*var(--cc-fs,1));line-height:1.45}',
    '.cc-input:focus{outline:none;border-color:var(--accent);box-shadow:var(--ring)}',
    '.cc-input:disabled{opacity:.55}',
    '.cc-meta{display:flex;gap:7px;align-items:center;margin-top:6px;font-size:calc(11px*var(--cc-fs,1));color:var(--text-muted);flex-wrap:wrap}',
    '.cc-sel{background:var(--bg-primary);border:1px solid var(--border);border-radius:4px;color:var(--text-secondary);font-size:calc(11px*var(--cc-fs,1));padding:2px 5px;font-family:inherit;cursor:pointer}',
    '.cc-sel:focus{outline:none;border-color:var(--accent)}',
    // Permission mode: an icon that says which mode is on, and opens a chooser.
    // The <select> behind it is kept (hidden) as the value everything reads.
    '.cc-sel.cc-mode-store{display:none}',
    '.cc-mode{display:inline-flex;align-items:center;justify-content:center;gap:5px;min-width:calc(26px*var(--cc-fs,1));height:calc(22px*var(--cc-fs,1));padding:0 6px;border-radius:5px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-secondary);cursor:pointer;font-family:inherit;font-size:calc(11px*var(--cc-fs,1))}',
    '.cc-mode:hover{border-color:var(--accent);color:var(--text-primary)}',
    '.cc-mode svg{width:calc(13px*var(--cc-fs,1));height:calc(13px*var(--cc-fs,1));flex-shrink:0}',
    // Narrow pane or a phone: the icon alone, and big enough for a finger.
    '.cc-narrow .cc-mode-l{display:none}',
    '.cc-narrow .cc-mode{min-width:calc(34px*var(--cc-fs,1));height:calc(28px*var(--cc-fs,1));padding:0}',
    '@media (max-width:768px){.cc-mode-l{display:none}.cc-mode{min-width:calc(34px*var(--cc-fs,1));height:calc(28px*var(--cc-fs,1));padding:0}}',
    '.cc-mode[data-mode="plan"]{color:var(--sky,#38bdf8);border-color:rgba(56,189,248,.45)}',
    '.cc-mode[data-mode="acceptEdits"],.cc-mode[data-mode="auto"]{color:var(--accent-hover)}',
    '.cc-mode[data-mode="dontAsk"]{color:var(--yellow)}',
    '.cc-mode[data-mode="bypassPermissions"]{color:var(--red);border-color:rgba(239,68,68,.5)}',
    '.cc-modal-back{position:fixed;inset:0;z-index:1100;background:rgba(0,0,0,.55);display:flex;align-items:center;justify-content:center;padding:16px}',
    '.cc-modal{width:min(380px,100%);max-height:min(560px,calc(100svh - 32px));overflow:auto;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius);box-shadow:var(--shadow-lg);padding:6px}',
    '.cc-modal-h{display:flex;align-items:center;gap:8px;padding:8px 8px 6px;font-size:11px;font-weight:700;letter-spacing:.05em;text-transform:uppercase;color:var(--text-secondary)}',
    '.cc-modal-h span{flex:1}',
    '.cc-modal-x{background:none;border:0;color:var(--text-secondary);font-size:18px;line-height:1;cursor:pointer;padding:4px 8px;border-radius:6px}',
    '.cc-modal-x:hover{color:var(--text-primary);background:var(--bg-tertiary)}',
    '.cc-mode-opt{display:flex;align-items:center;gap:12px;width:100%;text-align:left;padding:10px;min-height:52px;border:1px solid transparent;border-radius:8px;background:none;color:var(--text-primary);cursor:pointer;font-family:inherit}',
    '.cc-mode-opt:hover{background:var(--bg-tertiary)}',
    '.cc-mode-opt.on{background:var(--accent-dim);border-color:rgba(99,102,241,.5)}',
    '.cc-mode-opt .i{width:32px;height:32px;border-radius:8px;display:inline-flex;align-items:center;justify-content:center;background:var(--bg-primary);border:1px solid var(--border);color:var(--text-secondary);flex-shrink:0}',
    '.cc-mode-opt .i svg{width:16px;height:16px}',
    '.cc-mode-opt[data-mode="plan"] .i{color:var(--sky,#38bdf8)}',
    '.cc-mode-opt[data-mode="acceptEdits"] .i,.cc-mode-opt[data-mode="auto"] .i{color:var(--accent-hover)}',
    '.cc-mode-opt[data-mode="dontAsk"] .i{color:var(--yellow)}',
    '.cc-mode-opt[data-mode="bypassPermissions"] .i{color:var(--red)}',
    '.cc-mode-opt .t{flex:1;min-width:0;display:flex;flex-direction:column;gap:2px}',
    '.cc-mode-opt .n{font-size:13px;font-weight:600}',
    '.cc-mode-opt .d{font-size:11.5px;color:var(--text-secondary);line-height:1.35}',
    '.cc-mode-opt .ck{color:var(--accent-hover);flex-shrink:0;visibility:hidden}',
    '.cc-mode-opt.on .ck{visibility:visible}',
    '.cc-meta-sp{flex:1}',
    '.cc-busy{color:var(--accent-hover)}',
    // Enter-key behaviour toggle, sitting beside the permission-mode dropdown.
    '.cc-toggle{background:none;border:0;color:var(--text-secondary);font-size:calc(11px*var(--cc-fs,1));padding:0;font-family:inherit;cursor:pointer;display:inline-flex;align-items:center;gap:5px}',
    '.cc-toggle:hover{color:var(--text-primary)}',
    '.cc-toggle b{font-family:var(--mono);font-weight:600}',
    // Enter-key switch: both options always visible, thumb marks the live one.
    '.cc-sw{position:relative;display:inline-flex;align-items:center;background:var(--bg-tertiary,rgba(255,255,255,.06));border-radius:999px;padding:2px;line-height:1}',
    '.cc-sw-thumb{position:absolute;top:2px;bottom:2px;left:2px;width:calc(50% - 2px);border-radius:999px;background:var(--accent-dim);border:1px solid var(--accent);transition:transform .18s cubic-bezier(.4,0,.2,1)}',
    '.cc-sw.alt .cc-sw-thumb{transform:translateX(100%)}',
    '.cc-sw-opt{position:relative;z-index:1;padding:2px 8px;font-size:calc(11px*var(--cc-fs,1));opacity:.45;transition:opacity .18s ease}',
    '.cc-sw:not(.alt) .cc-sw-opt:nth-child(2),.cc-sw.alt .cc-sw-opt:nth-child(3){opacity:1}',
    '.cc-sw:not(.alt) .cc-sw-opt:nth-child(2) b,.cc-sw.alt .cc-sw-opt:nth-child(3) b{color:var(--accent-hover)}',
    // The switch shows WHICH key; this says what it does.
    '.cc-sw-lbl{font-size:calc(11px*var(--cc-fs,1));color:inherit}',
    // Stop: icon by default, widens to reveal "Sure?" once armed.
    '.cc-stop{flex:none;height:34px;min-width:34px;display:inline-flex;align-items:center;justify-content:center;border:1px solid var(--border);background:var(--bg-primary);color:var(--red);border-radius:var(--radius-sm);cursor:pointer;padding:0 8px;transition:.15s}',
    '.cc-stop:hover{color:var(--red);border-color:var(--red)}',
    '.cc-stop svg{width:15px;height:15px;flex:none}',
    '.cc-stop .cc-stop-label{max-width:0;margin-left:0;overflow:hidden;white-space:nowrap;font-size:calc(11px*var(--cc-fs,1));font-family:inherit;opacity:0;transition:max-width .18s ease,margin-left .18s ease,opacity .18s ease}',
    '.cc-stop.cc-armed{color:var(--red);border-color:var(--red);background:var(--red-dim)}',
    '.cc-stop.cc-armed .cc-stop-label{max-width:44px;margin-left:5px;opacity:1}',
    // Attach button matches the main input bar's paperclip.
    '.cc-attach{flex:none;width:34px;height:34px;display:inline-flex;align-items:center;justify-content:center;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-secondary);border-radius:var(--radius-sm);cursor:pointer;padding:0}',
    '.cc-attach:hover{color:var(--accent-hover);border-color:var(--accent)}',
    '.cc-attach.busy{opacity:.55;pointer-events:none}',
    // Upload progress. A screenshot over a phone connection is not instant,
    // and a dimmed paperclip does not say whether anything is happening.
    '.cc-up{display:none;align-items:center;gap:8px;padding:4px 2px 0}',
    '.cc-up.on{display:flex}',
    '.cc-up-name{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:calc(11px*var(--cc-fs,1));color:var(--text-muted)}',
    '.cc-up-bar{flex:2;height:4px;border-radius:999px;background:var(--bg-tertiary);overflow:hidden}',
    '.cc-up-fill{display:block;height:100%;width:0;background:var(--accent);border-radius:999px;transition:width .15s ease}',
    '.cc-up-pct{flex:none;font-size:calc(11px*var(--cc-fs,1));color:var(--text-muted);font-variant-numeric:tabular-nums;min-width:34px;text-align:right}',
    '.cc-attach svg{width:16px;height:16px}',
    // Queued input: one bubble however many lines were added, click to reclaim.
    '.cc-queue{margin-bottom:7px;border:1px dashed rgba(99,102,241,.55);background:rgba(99,102,241,.08);border-radius:10px;padding:7px 11px;cursor:pointer;transition:.14s}',
    '.cc-queue:hover{border-color:var(--accent);background:var(--accent-dim)}',
    '.cc-queue-head{display:flex;align-items:center;gap:6px;font-size:calc(10.5px*var(--cc-fs,1));text-transform:uppercase;letter-spacing:.05em;color:var(--accent-hover);font-weight:700;margin-bottom:3px}',
    // Clamped: a long queued message used to push the transcript and the
    // composer off a phone screen entirely. The hint says when it is trimmed.
    '.cc-queue-body{white-space:pre-wrap;word-break:break-word;color:var(--text-primary);font-size:calc(12.5px*var(--cc-fs,1));display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:4;line-clamp:4;overflow:hidden}',
    '.cc-queue-hint{font-size:calc(10.5px*var(--cc-fs,1));color:var(--text-muted);margin-top:4px}',
    '.cc-queue.sent{border-style:solid;border-color:rgba(148,163,184,.45);background:rgba(148,163,184,.09);cursor:default}',
    '.cc-queue.sent:hover{border-color:rgba(148,163,184,.45);background:rgba(148,163,184,.09)}',
    '.cc-queue.sent .cc-queue-head{color:var(--text-muted)}',
    '.cc-queue.sent .cc-queue-body{color:var(--text-secondary)}',
    // In-log activity row: some turns (notably /compact) stream nothing at all,
    // so without this the log looks frozen while the header says "working".
    '.cc-activity{display:flex;align-items:center;gap:8px;color:var(--text-muted);font-size:calc(12px*var(--cc-fs,1));font-style:italic;padding:2px 0}',
    '.cc-activity .cc-spin{width:11px;height:11px}',
    '.cc-sid{font-family:var(--mono);font-size:calc(10.5px*var(--cc-fs,1));cursor:pointer;border-bottom:1px dotted var(--border)}',
    '.cc-sid:hover{color:var(--text-secondary)}',
    '.cc-slash{position:absolute;bottom:100%;left:0;right:0;margin-bottom:4px;background:var(--bg-secondary,#111119);border:1px solid var(--border);border-radius:var(--radius-sm);box-shadow:var(--shadow-md);max-height:190px;overflow-y:auto;z-index:20}',
    '.cc-slash-item{padding:5px 10px;cursor:pointer;font-size:calc(12px*var(--cc-fs,1));display:flex;gap:8px}',
    '.cc-slash-item.on{background:var(--accent-dim)}',
    '.cc-slash-item b{font-family:var(--mono);color:var(--accent-hover);font-weight:600}',
    '.cc-slash-item span{color:var(--text-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.cc-wrap{position:relative}',

    // Floating dock over the bottom of the transcript. It holds a row of two
    // small badges - background commands and subagents - and the suggested
    // reply.
    //
    // It used to hold a pill per agent and per background command, capped at
    // four finished ones but with no cap on running ones - so a turn that
    // fanned out to ten agents and five commands stacked fifteen pills over
    // the conversation, covering most of a phone screen. Now each kind is one
    // badge: an icon and a count, which opens a list.
    '.cc-agents{position:absolute;right:10px;bottom:8px;display:flex;flex-direction:column;align-items:flex-end;gap:5px;z-index:15;pointer-events:none;max-width:min(320px,calc(100% - 20px))}',
    '.cc-agents:empty{display:none}',
    '.cc-agbadges{display:flex;align-items:center;justify-content:flex-end;gap:6px;max-width:100%}',
    '.cc-agbadge{pointer-events:auto;display:none;align-items:center;gap:6px;min-height:30px;background:var(--bg-secondary,#111119);border:1px solid var(--border);border-radius:999px;padding:4px 10px 4px 8px;color:var(--text-secondary);cursor:pointer;box-shadow:var(--shadow-md);font:inherit;font-size:calc(12px*var(--cc-fs,1));line-height:1}',
    '.cc-agbadge.on{display:inline-flex}',
    '.cc-agbadge:hover,.cc-agbadge.open{border-color:var(--accent);color:var(--text-primary)}',
    '.cc-agbadge:focus-visible{outline:2px solid var(--accent);outline-offset:2px}',
    '.cc-agbadge svg{width:calc(15px*var(--cc-fs,1));height:calc(15px*var(--cc-fs,1));flex:none}',
    '.cc-agbadge.busy svg{color:var(--accent-hover)}',
    '.cc-agbadge-n{font-family:var(--mono);font-weight:600;font-variant-numeric:tabular-nums;color:var(--text-primary)}',
    // An agent arrived: the badge pops and throws an accent ring. One finished:
    // a green ring. Both are one-shot; nothing here loops but the spinner.
    '.cc-agbadge.ev-add{animation:cc-ag-pop .5s cubic-bezier(.2,.9,.3,1.4)}',
    '.cc-agbadge.ev-done{animation:cc-ag-done .9s ease-out}',
    '@keyframes cc-ag-pop{0%{transform:scale(.6);box-shadow:0 0 0 0 var(--accent)}55%{transform:scale(1.14)}100%{transform:scale(1);box-shadow:0 0 0 9px transparent}}',
    '@keyframes cc-ag-done{0%{box-shadow:0 0 0 0 var(--green);border-color:var(--green)}30%{transform:scale(1.08);border-color:var(--green)}100%{transform:scale(1);box-shadow:0 0 0 10px transparent}}',
    '@media (prefers-reduced-motion: reduce){.cc-agbadge.ev-add,.cc-agbadge.ev-done{animation:none}}',

    // The agents list: same sheet as a transcript, one row per agent.
    '.cc-aglist-acts{flex:0 0 auto;background:none;border:1px solid var(--border);border-radius:999px;color:var(--text-secondary);cursor:pointer;font:inherit;font-size:calc(11px*var(--cc-fs,1));padding:3px 9px;white-space:nowrap}',
    '.cc-aglist-acts:hover{color:var(--red);border-color:var(--red)}',
    '.cc-aglist-body{flex:1;overflow-y:auto;padding:6px}',
    '.cc-agrow{display:flex;align-items:center;gap:9px;padding:9px 8px;border-radius:var(--radius-sm);cursor:pointer;min-width:0}',
    '.cc-agrow:hover,.cc-agrow:focus-visible{background:var(--bg-hover);outline:none}',
    '.cc-agrow-st{flex:none;width:12px;display:flex;justify-content:center}',
    '.cc-agrow-tx{flex:1 1 auto;min-width:0;display:flex;flex-direction:column;gap:2px}',
    '.cc-agrow-t{color:var(--text-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.cc-agrow-s{font-family:var(--mono);font-size:calc(10.5px*var(--cc-fs,1));color:var(--text-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.cc-agrow.done .cc-agrow-t{color:var(--text-secondary)}',
    '.cc-agrow-go{flex:none;color:var(--text-muted)}',
    '.cc-agrow-x{flex:none;background:none;border:0;color:var(--text-muted);cursor:pointer;font-size:calc(13px*var(--cc-fs,1));line-height:1;padding:6px;min-width:28px;border-radius:var(--radius-sm)}',
    '.cc-agrow-x:hover{color:var(--red)}',
    '.cc-aglist-foot{flex:none;padding:8px 12px;border-top:1px solid var(--border-subtle);color:var(--text-muted);font-size:calc(11px*var(--cc-fs,1));line-height:1.45}',
    '.cc-agpanel-back{flex:0 0 auto;background:none;border:0;color:var(--text-muted);cursor:pointer;font:inherit;font-size:calc(12px*var(--cc-fs,1));padding:2px 6px 2px 0;white-space:nowrap}',
    '.cc-agpanel-back:hover{color:var(--text-primary)}',
    // The title is what tells two transcripts apart; the status line gives way first.
    '.cc-agpanel-sub{flex-shrink:5}',
    // In a list the title is one short word and must not be the thing that gives way.
    '.cc-aglist .cc-agpanel-title{flex:0 0 auto}',
    '.cc-aglist .cc-agpanel-sub{flex:1 1 auto}',

    // Expanded transcript, anchored inside the chat cell.
    '.cc-agpanel{position:absolute;inset:8px;background:var(--bg-primary);border:1px solid var(--border);border-radius:var(--radius);box-shadow:var(--shadow-md);display:flex;flex-direction:column;z-index:25;animation:cc-in .16s ease}',
    // The close button must survive any title. The subtitle carries free text
    // (the agent's current step), and with flex:none it could not shrink — on a
    // narrow screen it pushed the ✕ off the right edge, leaving the panel with
    // no visible way out. Both text spans shrink; only the button does not.
    '.cc-agpanel-head{display:flex;align-items:center;gap:8px;padding:8px 10px;border-bottom:1px solid var(--border-subtle);flex:none;min-width:0}',
    '.cc-agpanel-title{font-weight:600;color:var(--text-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1 1 auto;min-width:0}',
    '.cc-agpanel-sub{font-size:calc(10.5px*var(--cc-fs,1));color:var(--text-muted);font-family:var(--mono);flex:0 1 auto;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.cc-agpanel-x{flex:0 0 auto;background:none;border:0;color:var(--text-muted);cursor:pointer;font-size:calc(15px*var(--cc-fs,1));line-height:1;padding:2px 4px;min-width:24px}',
    '.cc-agpanel-x:hover{color:var(--text-primary)}',
    '.cc-agpanel-body{flex:1;overflow-y:auto;padding:10px 12px}',
    '.cc-agpanel-prompt{background:var(--bg-secondary,#111119);border:1px solid var(--border-subtle);border-radius:var(--radius-sm);padding:7px 9px;margin-bottom:10px;color:var(--text-secondary);white-space:pre-wrap;word-break:break-word;font-size:calc(12px*var(--cc-fs,1));max-height:150px;overflow:auto}',
    '.cc-agpanel-empty{color:var(--text-muted);font-style:italic}',
    '.cc-agentrow{display:flex;align-items:center;gap:8px;cursor:pointer}',
    '.cc-agentrow:hover{border-color:var(--accent)}',
    '.cc-agentrow-go{margin-left:auto;color:var(--accent-hover);font-size:calc(11px*var(--cc-fs,1));flex:none}',

    // Goal mode. Present only while a goal is set, so a normal chat is unchanged.
    // The plan overlays the log rather than sitting beside it: it is a
    // reference you consult and dismiss, and the chat underneath keeps its
    // scroll position while you read.
    '.cc-plan{position:absolute;inset:0;z-index:20;background:var(--bg-primary);display:none;flex-direction:column}',
    '.cc-plan.open{display:flex}',
    '.cc-plan-hd{flex:none;display:flex;align-items:center;gap:8px;padding:7px 10px;border-bottom:1px solid var(--border);background:var(--bg-secondary,#111119)}',
    '.cc-plan-tag{font-family:var(--mono);font-size:calc(9.5px*var(--cc-fs,1));letter-spacing:.08em;font-weight:700;color:var(--accent-hover);flex:none}',
    '.cc-plan-st{flex:1;color:var(--text-muted);font-size:calc(11px*var(--cc-fs,1));overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.cc-plan-x{flex:none;background:none;border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-muted);cursor:pointer;font-size:calc(10.5px*var(--cc-fs,1));padding:1px 7px}',
    '.cc-plan-x:hover{color:var(--text-primary);border-color:var(--accent)}',
    '.cc-plan-body{flex:1;overflow-y:auto;padding:12px 14px}',
    '.cc-plan-body h1,.cc-plan-body h2,.cc-plan-body h3{margin:14px 0 6px;font-size:calc(13px*var(--cc-fs,1))}',
    '.cc-plan-body h1:first-child,.cc-plan-body h2:first-child{margin-top:0}',
    '.cc-goalbar{flex:none;border-top:1px solid var(--border-subtle);background:var(--accent-dim);padding:6px 10px;font-size:calc(11.5px*var(--cc-fs,1))}',
    '.cc-goal-head{display:flex;align-items:center;gap:8px}',
    '.cc-goal-tag{display:flex;align-items:center;gap:5px;font-family:var(--mono);font-size:calc(9.5px*var(--cc-fs,1));letter-spacing:.08em;font-weight:700;color:var(--accent-hover);flex:none}',
    '.cc-goal-cond{flex:1;color:var(--text-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.cc-goal-n{flex:none;color:var(--text-muted);font-family:var(--mono);font-size:calc(10px*var(--cc-fs,1))}',
    '.cc-goal-x{flex:none;background:none;border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-muted);cursor:pointer;font-size:calc(10.5px*var(--cc-fs,1));padding:1px 7px}',
    '.cc-goal-x:hover{color:var(--red);border-color:var(--red)}',
    '.cc-goal-idle{color:var(--text-muted);margin-top:4px}',
    '.cc-gv{border-left:2px solid var(--accent);background:var(--bg-secondary,#111119);border-radius:0 var(--radius-sm) var(--radius-sm) 0;padding:6px 10px}',
    '.cc-gv-head{display:flex;align-items:center;gap:6px;color:var(--accent-hover);font-size:calc(11px*var(--cc-fs,1));font-weight:600;margin-bottom:3px}',
    '.cc-gv-dot{width:5px;height:5px;border-radius:50%;background:var(--accent);flex:none}',
    '.cc-gv-body{color:var(--text-secondary);white-space:pre-wrap;word-break:break-word;font-size:calc(12px*var(--cc-fs,1))}',

    // Scheduled messages.
    '.cc-sched{flex:none;background:none;border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-muted);cursor:pointer;padding:3px 5px;display:flex;align-items:center}',
    '.cc-sched svg{width:13px;height:13px}',
    '.cc-sched:hover{color:var(--accent-hover);border-color:var(--accent)}',
    '.cc-narrow .cc-sched{padding:7px 9px}',
    '.cc-narrow .cc-sched svg{width:15px;height:15px}',
    '.cc-schedpanel{position:absolute;left:8px;right:8px;bottom:8px;max-height:82%;display:flex;flex-direction:column;background:var(--bg-primary);border:1px solid var(--border);border-radius:var(--radius);box-shadow:var(--shadow-md);z-index:28;animation:cc-in .16s ease}',
    '.cc-sched-head{display:flex;align-items:center;padding:8px 10px;border-bottom:1px solid var(--border-subtle);flex:none}',
    '.cc-sched-title{flex:1;font-weight:600;color:var(--text-primary)}',
    '.cc-sched-x{background:none;border:0;color:var(--text-muted);cursor:pointer;font-size:calc(15px*var(--cc-fs,1));line-height:1;padding:2px 4px}',
    '.cc-sched-x:hover{color:var(--text-primary)}',
    '.cc-sched-body{padding:10px;overflow-y:auto;display:flex;flex-direction:column;gap:8px}',
    '.cc-sched-msg{width:100%;box-sizing:border-box;resize:vertical;background:var(--bg-secondary,#111119);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);font:inherit;padding:7px 9px}',
    '.cc-sched-trigs{display:flex;gap:6px;flex-wrap:wrap}',
    '.cc-sched-trig{flex:1;min-width:96px;background:var(--bg-secondary,#111119);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-secondary);cursor:pointer;padding:6px 8px;font-size:calc(11.5px*var(--cc-fs,1))}',
    '.cc-sched-trig.on{background:var(--accent-dim);border-color:var(--accent);color:var(--text-primary)}',
    // Shown, not hidden: the trigger exists, it just needs the limit warmer on.
    '.cc-sched-trig.off{opacity:0.45;cursor:not-allowed}',
    '.cc-queue.sent.recallable{cursor:pointer}',
    // The CLI's predicted next message: tap to send it, x to drop it.
    // A ghost of the user's own bubble at the end of the conversation: same
    // shape and alignment, dashed and muted so it never reads as sent.
    '.cc-sug-row{pointer-events:auto;display:flex;flex-direction:column;align-items:flex-end;gap:3px;max-width:100%;margin-top:3px}',
    '.cc-sug-cap{font-size:calc(10.5px*var(--cc-fs,1));color:var(--text-muted);margin-right:4px;background:var(--bg-primary);padding:0 4px;border-radius:4px}',
    '.cc-sug{position:relative;max-width:100%;padding:7px 30px 7px 11px;border:1px dashed rgba(129,140,248,.55);background:var(--bg-secondary,#12121a);box-shadow:0 4px 14px rgba(0,0,0,.35);color:var(--text-secondary);border-radius:12px 12px 3px 12px;cursor:pointer;white-space:pre-wrap;word-break:break-word;text-align:left;font:inherit;transition:background .14s,border-color .14s,color .14s}',
    '.cc-sug:hover,.cc-sug:focus-visible{border-style:solid;border-color:rgba(99,102,241,.55);background:var(--accent-dim);color:var(--text-primary);outline:none}',
    '.cc-sug-x{position:absolute;top:4px;right:4px;width:22px;height:22px;display:flex;align-items:center;justify-content:center;background:none;border:none;border-radius:6px;color:var(--text-muted);cursor:pointer;font-size:calc(11px*var(--cc-fs,1));line-height:1;padding:0}',
    '.cc-sug-x:hover{color:var(--text-primary);background:rgba(148,163,184,.15)}',
    '.cc-sched-at{background:var(--bg-secondary,#111119);border:1px solid var(--border);border-radius:var(--radius-sm);color:var(--text-primary);font:inherit;padding:6px 8px}',
    '.cc-sched-note{color:var(--text-muted);font-size:calc(11px*var(--cc-fs,1))}',
    '.cc-sched-note:empty{display:none}',
    '.cc-sched-list{display:flex;flex-direction:column;gap:6px;border-top:1px solid var(--border-subtle);padding-top:8px;margin-top:2px}',
    '.cc-sched-empty{color:var(--text-muted);font-style:italic;font-size:calc(12px*var(--cc-fs,1))}',
    '.cc-sched-item{position:relative;background:var(--bg-secondary,#111119);border:1px solid var(--border-subtle);border-radius:var(--radius-sm);padding:6px 26px 6px 9px}',
    '.cc-sched-item.next{border-color:var(--accent)}',
    '.cc-sched-when{font-size:calc(10.5px*var(--cc-fs,1));color:var(--text-muted);font-family:var(--mono);margin-bottom:2px}',
    '.cc-sched-item.next .cc-sched-when b{color:var(--accent-hover)}',
    '.cc-sched-text{color:var(--text-secondary);font-size:calc(12px*var(--cc-fs,1));white-space:pre-wrap;word-break:break-word;max-height:52px;overflow:hidden}',
    '.cc-sched-del{position:absolute;top:4px;right:4px;background:none;border:0;color:var(--text-muted);cursor:pointer;font-size:calc(12px*var(--cc-fs,1));line-height:1;padding:2px 4px}',
    '.cc-sched-del:hover{color:var(--red)}'
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
    // Code spans and [text](url) links are set aside first, so a URL inside
    // either is not linked a second time; bare URLs are linked after.
    var held = [];
    var hold = function (html) { held.push(html); return '\u0001' + (held.length - 1) + '\u0001'; };
    s = s
      .replace(/`([^`]+)`/g, function (m, c) { return hold(looksLikePath(c) ? '<code class="cc-path" data-path="' + c + '" title="Open ' + c + '">' + c + '</code>' : '<code>' + c + '</code>'); })
      .replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, function (m, t, u) { return hold('<a href="' + u + '" target="_blank" rel="noopener noreferrer">' + t + '</a>'); })
      .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
      .replace(/(^|[\s(])\*([^*\n]+)\*/g, '$1<em>$2</em>');
    s = linkify(s);
    return s.replace(/\u0001(\d+)\u0001/g, function (m, i) { return held[+i]; });
  }

  /**
   * In already-escaped text: absolute file paths become openable links (the
   * host opens them in its file viewer), and bare http(s) URLs become links.
   * Trailing punctuation stays outside either, so "see https://x.com." links
   * x.com and "in /a/b.js:12," opens /a/b.js.
   */
  function linkify(escaped) {
    var held = [];
    var hold = function (html) { held.push(html); return '\u0002' + (held.length - 1) + '\u0002'; };
    var trim = function (u) { var m = u.match(/(?:[.,;:!?)\]'"]|&#39;|&quot;|&gt;)+$/); return m ? [u.slice(0, -m[0].length), m[0]] : [u, '']; };
    var out = String(escaped).replace(/\bhttps?:\/\/[^\s<\u0001\u0002]+/g, function (u) {
      var t = trim(u);
      if (!t[0]) return t[1];
      return hold('<a href="' + t[0] + '" target="_blank" rel="noopener noreferrer">' + t[0] + '</a>') + t[1];
    });
    out = out.replace(/(^|[\s(\[>]|&quot;|&#39;)(\/(?:[\w.@+~%,=-]+\/)+[\w.@+~%,=-]+(?::\d+(?::\d+)?)?)/g, function (m, pre, pth) {
      var t = trim(pth);
      return pre + hold('<a class="cc-path" data-path="' + t[0] + '" title="Open ' + t[0] + '">' + t[0] + '</a>') + t[1];
    });
    return out.replace(/\u0002(\d+)\u0002/g, function (m, i) { return held[+i]; });
  }

  /** A code span that is just a file path (src/a.js, ./x/y.md, /etc/z) is openable. */
  function looksLikePath(c) {
    if (!c || c.length > 260 || /\s/.test(c) || /^https?:/.test(c) || /[<>&*|$`]/.test(c)) return false;
    var bare = c.replace(/:\d+(:\d+)?$/, '');
    if (/^(\.{0,2}\/)?[\w.@+~-]+(\/[\w.@+~-]+)+$/.test(bare)) return true;
    // A bare name only with a real file extension, so obj.prop is left alone.
    return /^[\w-][\w.@+~-]*\.(js|mjs|cjs|ts|tsx|jsx|json|md|txt|log|py|sh|bash|yml|yaml|toml|ini|conf|cfg|env|html|htm|css|scss|xml|svg|png|jpe?g|gif|webp|pdf|csv|sql|go|rs|rb|php|java|kt|swift|c|h|cc|cpp|hpp|vue|lock|dockerfile|service)$/i.test(bare);
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

  // Which open chat currently owns each project-level draft, so two chats in
  // one project never fight over it. Page-lifetime only; a claim is released on
  // destroy() so reopening a closed chat can take the draft back.
  var CLAIMED = {};

  // Which chat the user last put the cursor in. A paste on a phone frequently
  // arrives with focus already moved off the field (the paste UI takes it), so
  // activeElement alone is not enough to tell whose paste it is — and without
  // that, every open chat would upload the same image.
  var LAST_FOCUSED = null;

  // Session-keyed drafts outlive the sessions they name (a restart strands
  // them), so without this they would accumulate in localStorage forever.
  var DRAFT_TTL_MS = 30 * 24 * 60 * 60 * 1000;
  function pruneDrafts() {
    try {
      var now = Date.now();
      var kill = [];
      for (var i = 0; i < localStorage.length; i++) {
        var k = localStorage.key(i);
        // Answer drafts leak the same way and are worse: their key contains
        // BOTH a session id and an entry id, so once either is regenerated the
        // card can never come back to clear it. Left alone they accumulate
        // until the origin's storage quota is full, at which point every
        // setItem here throws and is swallowed — drafts, prefs and workbench
        // state all stop saving, silently.
        if (!k || (k.indexOf('crundi_chat_draft_') !== 0 && k.indexOf('crundi_chat_ans_') !== 0
                   && k.indexOf('crundi_chat_dis_') !== 0)) continue;
        var raw = localStorage.getItem(k);
        if (!raw || raw.charAt(0) !== '{') continue;   // untimestamped: leave it
        try {
          var d = JSON.parse(raw);
          if (d && d.t && now - d.t > DRAFT_TTL_MS) kill.push(k);
        } catch (e) {}
      }
      kill.forEach(function (k) { localStorage.removeItem(k); });
    } catch (e) {}
  }

  // Dismissing an agent bubble is permanent and global. It must not come back
  // on refresh, when the desktop app reopens, or when the same conversation is
  // resumed under a fresh session id - so this is keyed on toolUseId (unique
  // per agent) rather than per session, and lives at module scope so two chats
  // open at once see each other's dismissals immediately.
  //
  // Each id carries its own timestamp. The ids outlive the sessions that made
  // them, so without expiry and a cap the set would grow until the origin's
  // storage quota is full - at which point every setItem in this file starts
  // failing silently, taking drafts and prefs down with it.
  var DIS_KEY = 'crundi_chat_dismissed';
  var DIS_TTL_MS = 90 * 24 * 60 * 60 * 1000;
  var DIS_MAX = 2000;
  var dismissedIds = null;   // id -> when it was dismissed

  function loadDismissed() {
    if (dismissedIds) return dismissedIds;
    dismissedIds = {};
    try {
      var raw = localStorage.getItem(DIS_KEY);
      var d = raw ? JSON.parse(raw) : null;
      var ids = (d && d.ids) || {};
      var now = Date.now();
      if (Object.prototype.toString.call(ids) === '[object Array]') {
        // A build in between stored a plain array; keep those dismissed.
        ids.forEach(function (id) { dismissedIds[id] = now; });
      } else {
        Object.keys(ids).forEach(function (id) {
          if (now - ids[id] < DIS_TTL_MS) dismissedIds[id] = ids[id];
        });
      }
    } catch (e) {}
    return dismissedIds;
  }

  function isDismissed(id) { return !!loadDismissed()[id]; }

  function markDismissed(id) {
    if (!id) return;
    var map = loadDismissed();
    map[id] = Date.now();
    var keys = Object.keys(map);
    if (keys.length > DIS_MAX) {
      keys.sort(function (a, b) { return map[b] - map[a]; })
          .slice(DIS_MAX)
          .forEach(function (k) { delete map[k]; });
    }
    try { localStorage.setItem(DIS_KEY, JSON.stringify({ t: Date.now(), ids: map })); } catch (e) {}
  }

  // ─── Mount ───

  function mount(host, opts) {
    ensureStyles();
    opts = opts || {};
    var sessionId = opts.sessionId;
    var project = opts.project || '';
    var apiFetch = opts.apiFetch;
    var apiUpload = opts.apiUpload || null;   // absent in older hosts
    var openPath = opts.openPath || null;      // host opens a file path in its viewer
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
    // Schedule-a-message. Lives next to the "sends" toggle on a wide cell and
    // beside the paperclip on a narrow one; syncWidth() moves the single node
    // rather than rendering two and hiding one.
    var schedBtn = el('button', 'cc-sched',
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'
      + '<circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 15 14"/></svg>');
    schedBtn.title = 'Schedule this message';
    var actions = el('div', 'cc-actions');
    actions.appendChild(attachBtn);
    actions.appendChild(stopBtn);
    actions.appendChild(sendBtn);
    inrow.appendChild(input);
    inrow.appendChild(actions);
    inrow.appendChild(fileInput);

    // Upload progress, between the input row and the status line.
    var upRow = el('div', 'cc-up');
    var upName = el('span', 'cc-up-name');
    var upBar = el('div', 'cc-up-bar');
    var upFill = el('span', 'cc-up-fill');
    var upPct = el('span', 'cc-up-pct');
    upBar.appendChild(upFill);
    upRow.appendChild(upName);
    upRow.appendChild(upBar);
    upRow.appendChild(upPct);

    var meta = el('div', 'cc-meta');
    var stateLbl = el('span', '', 'idle');
    // Only modes the CLI will actually accept at runtime. bypassPermissions is
    // deliberately absent: it can only be set at launch, and offering it here
    // meant the dropdown showed a mode that was never in effect.
    var modeSel = el('select', 'cc-sel cc-mode-store');
    var MODES = [['default', 'ask permissions'], ['acceptEdits', 'accept edits'],
      ['auto', 'auto'], ['plan', 'plan mode'], ['dontAsk', "don't ask"]];
    // What the composer shows in place of a dropdown: one icon per mode. The
    // closed control is that icon, so the mode is readable at a glance and the
    // chooser can say what each one does, which a native dropdown never could.
    var MODE_SVG = function (body) { return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + body + '</svg>'; };
    var MODE_INFO = {
      'default': { icon: MODE_SVG('<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="M9.6 9.2a2.5 2.5 0 0 1 4.8.8c0 1.6-2.4 2-2.4 3.4"/><line x1="12" y1="16.6" x2="12.01" y2="16.6"/>'), name: 'Ask permissions', desc: 'Asks you before editing files or running commands.' },
      'acceptEdits': { icon: MODE_SVG('<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/>'), name: 'Accept edits', desc: 'Edits files without asking. Still asks before running commands.' },
      'auto': { icon: MODE_SVG('<polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>'), name: 'Auto', desc: 'Decides for itself what is safe to do, and asks about the rest.' },
      'plan': { icon: MODE_SVG('<rect x="8" y="2" width="8" height="4" rx="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><line x1="9" y1="12" x2="15" y2="12"/><line x1="9" y1="16" x2="13" y2="16"/>'), name: 'Plan mode', desc: 'Reads and plans only. Changes nothing until you approve the plan.' },
      'dontAsk': { icon: MODE_SVG('<circle cx="12" cy="12" r="9"/><line x1="5.6" y1="5.6" x2="18.4" y2="18.4"/>'), name: "Don't ask", desc: 'Never asks. Anything not already allowed is refused.' },
      'bypassPermissions': { icon: MODE_SVG('<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><line x1="12" y1="8" x2="12" y2="12.5"/><line x1="12" y1="16" x2="12.01" y2="16"/>'), name: 'Bypass all', desc: 'Runs everything without asking. Only for a chat launched with skip permissions.' }
    };
    var modeBtn = el('button', 'cc-mode');
    modeBtn.type = 'button';
    modeBtn.setAttribute('aria-haspopup', 'dialog');
    function syncModeBtn() {
      var m = modeSel.value || 'default';
      var info = MODE_INFO[m] || MODE_INFO['default'];
      if (modeBtn.getAttribute('data-mode') === m) return;
      modeBtn.setAttribute('data-mode', m);
      // Icon always; the name beside it where the composer has room (CSS).
      modeBtn.innerHTML = info.icon + '<span class="cc-mode-l">' + esc(info.name) + '</span>';
      modeBtn.title = 'Permission mode: ' + info.name + '. Click to change.';
      modeBtn.setAttribute('aria-label', 'Permission mode: ' + info.name + '. Change');
    }
    var modeModal = null;
    function closeModeModal() { if (modeModal) { modeModal.remove(); modeModal = null; document.removeEventListener('keydown', modeModalKey, true); } }
    function modeModalKey(e) { if (e.key === 'Escape') { e.stopPropagation(); closeModeModal(); } }
    function openModeModal() {
      closeModeModal();
      var back = el('div', 'cc-modal-back');
      var box = el('div', 'cc-modal');
      box.setAttribute('role', 'dialog');
      box.setAttribute('aria-label', 'Permission mode');
      var head = el('div', 'cc-modal-h', '<span>Permission mode</span>');
      var x = el('button', 'cc-modal-x', '\u00d7');
      x.type = 'button'; x.title = 'Close';
      x.addEventListener('click', closeModeModal);
      head.appendChild(x);
      box.appendChild(head);
      for (var i = 0; i < modeSel.options.length; i++) {
        (function (value) {
          var info = MODE_INFO[value] || { icon: MODE_INFO['default'].icon, name: value, desc: '' };
          var b = el('button', 'cc-mode-opt' + (value === modeSel.value ? ' on' : ''));
          b.type = 'button';
          b.setAttribute('data-mode', value);
          b.innerHTML = '<span class="i">' + info.icon + '</span><span class="t"><span class="n">' + esc(info.name) + '</span><span class="d">' + esc(info.desc) + '</span></span>'
            + '<span class="ck">' + MODE_SVG('<polyline points="20 6 9 17 4 12"/>') + '</span>';
          b.addEventListener('click', function () {
            if (modeSel.value !== value) {
              modeSel.value = value;
              // The same event a hand on the dropdown raised: one path to the server.
              modeSel.dispatchEvent(new Event('change'));
            }
            closeModeModal();
          });
          box.appendChild(b);
        })(modeSel.options[i].value);
      }
      back.appendChild(box);
      back.addEventListener('click', function (e) { if (e.target === back) closeModeModal(); });
      document.body.appendChild(back);
      document.addEventListener('keydown', modeModalKey, true);
      modeModal = back;
      var on = box.querySelector('.cc-mode-opt.on');
      if (on) { try { on.focus(); } catch (e) { /* not focusable yet */ } }
    }
    modeBtn.addEventListener('click', openModeModal);
    function buildModes(isBypass) {
      modeSel.innerHTML = '';
      // Only switching INTO bypass is a launch-time decision. A session started
      // WITH it can move freely to plan (or any other mode) and back — verified
      // against the CLI, which answers set_permission_mode with {mode:"plan"}
      // and then {mode:"bypassPermissions"} on a --dangerously-skip-permissions
      // session. Locking the picker to "bypass all" cost the user plan mode for
      // no reason.
      (isBypass ? [['bypassPermissions', 'bypass all']].concat(MODES) : MODES).forEach(function (m) {
        var o = el('option'); o.value = m[0]; o.textContent = m[1]; modeSel.appendChild(o);
      });
      modeSel.disabled = false;
      modeSel.title = isBypass
        ? 'Permission mode — this chat can return to "bypass all" because it was launched for it'
        : 'Permission mode for this chat';
      syncModeBtn();
    }
    function setModeIfKnown(mode) {
      for (var i = 0; i < modeSel.options.length; i++) {
        if (modeSel.options[i].value === mode) { modeSel.value = mode; syncModeBtn(); return; }
      }
    }
    buildModes(false);
    var enterBtn = el('button', 'cc-toggle');
    var sidLbl = el('span', 'cc-sid', '');
    sidLbl.style.display = 'none';
    var modelLbl = el('span', '', '');
    var costLbl = el('span', '', '');
    meta.appendChild(stateLbl);
    meta.appendChild(modeSel);
    meta.appendChild(modeBtn);
    meta.appendChild(enterBtn);
    meta.appendChild(schedBtn);
    meta.appendChild(el('span', 'cc-meta-sp'));
    meta.appendChild(sidLbl);
    meta.appendChild(modelLbl);
    meta.appendChild(costLbl);

    wrap.appendChild(inrow);
    wrap.appendChild(upRow);
    composer.appendChild(wrap);
    composer.appendChild(meta);
    var logWrap = el('div', 'cc-logwrap');
    var agentDock = el('div', 'cc-agents');
    // Two badges side by side, each an icon and a count of what has not been
    // put away: background commands on the left, subagents on the right. Each
    // is hidden while it has nothing to count.
    var badgeRow = el('div', 'cc-agbadges');
    var taskBadge = el('button', 'cc-agbadge cc-agbadge-task');
    taskBadge.type = 'button';
    var agentBadge = el('button', 'cc-agbadge cc-agbadge-agent');
    agentBadge.type = 'button';
    badgeRow.appendChild(taskBadge);
    badgeRow.appendChild(agentBadge);
    agentDock.appendChild(badgeRow);
    logWrap.appendChild(log);
    logWrap.appendChild(agentDock);
    // Hidden until there is a plan AND the header pill is clicked.
    var planPanel = el('div', 'cc-plan');
    var planHd = el('div', 'cc-plan-hd');
    var planSt = el('span', 'cc-plan-st');
    var planX = el('button', 'cc-plan-x', 'Close');
    planHd.appendChild(el('span', 'cc-plan-tag', 'PLAN'));
    planHd.appendChild(planSt);
    planHd.appendChild(planX);
    var planBody = el('div', 'cc-plan-body');
    planPanel.appendChild(planHd);
    planPanel.appendChild(planBody);
    logWrap.appendChild(planPanel);
    // Goal mode is opt-in (`/goal <condition>`), so this stays out of the way
    // entirely until the user asks for it.
    var goalBar = el('div', 'cc-goalbar');
    goalBar.style.display = 'none';
    root.appendChild(logWrap);
    root.appendChild(goalBar);
    root.appendChild(composer);
    host.appendChild(root);
    // A file path in any message opens in the host's file viewer.
    root.addEventListener('click', function (e) {
      var pth = e.target.closest && e.target.closest('.cc-path');
      if (!pth || !openPath) return;
      e.preventDefault();
      openPath(pth.getAttribute('data-path'));
    });

    // Hovering a code block, a code span or a link in Claude's reply shows a
    // small copy button by it. One button, moved to whatever is hovered.
    var COPY_IC = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>';
    var DONE_IC = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>';
    var copyHint = el('button', 'cc-copyhint', COPY_IC);
    copyHint.type = 'button';
    root.appendChild(copyHint);
    var copyFor = null, copyHideT = null;
    function copyTargetOf(t) {
      var n = t && t.closest && t.closest('.cc-assistant pre, .cc-assistant code, .cc-assistant a, .cc-user-body a');
      if (!n || !root.contains(n)) return null;
      var pre = n.closest('pre');
      return pre && root.contains(pre) ? pre : n;
    }
    function copyTextOf(n) {
      if (n.tagName === 'PRE') return n.textContent;
      if (n.tagName === 'A') return n.getAttribute('data-path') || n.getAttribute('href') || n.textContent;
      return n.getAttribute('data-path') || n.textContent;
    }
    function placeCopyHint(n) {
      clearTimeout(copyHideT);
      if (copyFor !== n) { copyHint.classList.remove('done'); copyHint.innerHTML = COPY_IC; }
      copyFor = n;
      var what = n.tagName === 'PRE' ? 'code' : n.getAttribute('data-path') ? 'path' : n.tagName === 'A' ? 'link' : 'code';
      copyHint.title = 'Copy ' + what;
      copyHint.classList.add('on');
      var bw = copyHint.offsetWidth || 24, bh = copyHint.offsetHeight || 22, x, y;
      if (n.tagName === 'PRE') {
        var r = n.getBoundingClientRect();
        x = r.right - bw - 6; y = r.top + 6;
      } else {
        // Just after the end of the span (its last line, if it wraps).
        var rs = n.getClientRects(), last = rs[rs.length - 1] || n.getBoundingClientRect();
        x = last.right + 3; y = last.top + last.height / 2 - bh / 2;
        if (x + bw > window.innerWidth - 4) x = last.right - bw;
      }
      copyHint.style.left = Math.round(x) + 'px';
      copyHint.style.top = Math.round(y) + 'px';
    }
    function hideCopyHint() { copyHint.classList.remove('on'); copyFor = null; }
    log.addEventListener('mouseover', function (e) {
      if (e.target === copyHint || copyHint.contains(e.target)) return;
      var n = copyTargetOf(e.target);
      if (n) placeCopyHint(n);
      else if (copyFor) { clearTimeout(copyHideT); copyHideT = setTimeout(hideCopyHint, 250); }
    });
    log.addEventListener('mouseleave', function () { clearTimeout(copyHideT); copyHideT = setTimeout(hideCopyHint, 250); });
    copyHint.addEventListener('mouseenter', function () { clearTimeout(copyHideT); });
    copyHint.addEventListener('mouseleave', function () { clearTimeout(copyHideT); copyHideT = setTimeout(hideCopyHint, 250); });
    log.addEventListener('scroll', function () { if (copyFor) hideCopyHint(); }, { passive: true });
    copyHint.addEventListener('mousedown', function (e) { e.preventDefault(); });
    copyHint.addEventListener('click', function (e) {
      e.preventDefault(); e.stopPropagation();
      if (!copyFor) return;
      var txt = copyTextOf(copyFor), n = copyFor;
      var ok = function () { if (copyFor === n) { copyHint.classList.add('done'); copyHint.innerHTML = DONE_IC; } toast('Copied'); };
      (navigator.clipboard && navigator.clipboard.writeText ? navigator.clipboard.writeText(txt) : Promise.reject()).then(ok, function () { toast('Could not copy', 'error'); });
    });

    var entries = new Map();   // entry id -> { data, node }
    var plan = null;           // { text, filePath, at, status } from ExitPlanMode
    var planOpen = false;
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
    // Written to the CLI's stdin but not yet taken into the turn. Measured on
    // this machine: a message handed over mid-turn waited 14.8 SECONDS before
    // the CLI picked it up, alongside the next tool result. The drawer used to
    // vanish at hand-over, so for those 14.8 seconds the UI showed nothing
    // pending while the message had, as far as Claude was concerned, not
    // arrived. It now stays until --replay-user-messages says otherwise.
    var handedOver = [];       // {text, uuid, state} written to stdin, oldest first
    var sentNode = null;       // the separate "handed over" drawer
    var suggestion = null;     // {text, uuid}: the CLI's guess at the next message
    var sugNode = null;        // its tap-to-send bubble, last in the floating dock
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
      // Narrow: sit with the paperclip, ahead of stop/send. Wide: back on the
      // meta strip beside the enter toggle.
      if (narrow) actions.insertBefore(schedBtn, stopBtn);
      else meta.insertBefore(schedBtn, meta.querySelector('.cc-meta-sp'));
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
    // Following the newest message is the reader's choice, kept in
    // stickBottom (set when they scroll). Asking "are we within 60px of the
    // bottom?" mid-stream failed: the smooth scroll was still catching up when
    // the next chunk landed, so a growing thinking block shook it off the end.
    function followingBottom() { return stickBottom || atBottom(); }
    function scrollDown(force) {
      if (!(force || atBottom())) return;
      // Instant, not smooth: streaming text adds a line every few ms and an
      // animation always lags behind it.
      var prev = log.style.scrollBehavior;
      log.style.scrollBehavior = 'auto';
      log.scrollTop = log.scrollHeight;
      log.style.scrollBehavior = prev;
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
    // ─── Selection stays in the conversation ───
    //
    // On a phone, select some text, scroll until the start of the selection is
    // above the top of the log, then drag the end handle. The browser extends
    // the selection between two SCREEN points - the finger, and where the other
    // handle is drawn. That handle cannot be drawn at text that is scrolled out
    // of the log, so it is pinned to the edge, and the point under it is no
    // longer the text: it is whatever sits there. The selection jumped to the
    // page header and the project name above the chat.
    //
    // Three things, because none is enough alone:
    //
    //  1. While a selection lives in the log, the rest of the page is made
    //     unselectable (the cc-selecting rule), so the worst case is a
    //     selection that is wrong inside the chat, never one that leaves it.
    //  2. The selection's ends are remembered. An end whose handle is scrolled
    //     out of sight cannot have been moved by the reader, so if it changes
    //     it is put back.
    //  3. Dragging a handle near the top or bottom edge scrolls the log. The
    //     browser does that for the page but not for a scrolling box inside
    //     it, which left a long selection impossible to extend past one screen.
    var selSaved = null;       // { s, e } as { n: node, o: offset }: the selection as last seen
    var selTouches = 0;        // fingers on the page (a handle drag is not one: the page never sees it)
    var selMouse = false;
    var selTouchEndAt = 0;
    var selScrollAt = 0;
    var SEL_EDGE = 56;         // px from an edge of the log that counts as "at the edge"

    function selSame(a, b) { return a.n === b.n && a.o === b.o; }
    function selCmp(a, b) {    // negative when a comes before b
      try {
        var ra = document.createRange(), rb = document.createRange();
        ra.setStart(a.n, a.o); ra.collapse(true);
        rb.setStart(b.n, b.o); rb.collapse(true);
        return ra.compareBoundaryPoints(Range.START_TO_START, rb);
      } catch (e) { return 0; }
    }
    /** Where one end of the selection is on screen, or null if it cannot be told. */
    function selRect(p, isEnd) {
      try {
        if (p.n.nodeType === 3 && p.n.length) {
          var r = document.createRange();
          var at = isEnd ? Math.max(0, Math.min(p.o, p.n.length) - 1) : Math.min(p.o, p.n.length - 1);
          r.setStart(p.n, at); r.setEnd(p.n, at + 1);
          var rects = r.getClientRects();
          if (rects.length) return isEnd ? rects[rects.length - 1] : rects[0];
        }
        var node = p.n;
        if (node.nodeType === 1 && node.childNodes.length) {
          node = node.childNodes[Math.max(0, Math.min(isEnd ? p.o - 1 : p.o, node.childNodes.length - 1))];
        }
        while (node && node.nodeType !== 1) node = node.parentNode;
        return node ? node.getBoundingClientRect() : null;
      } catch (e) { return null; }
    }
    function selHiddenAbove(p) { var r = selRect(p, false); return !!r && r.bottom <= log.getBoundingClientRect().top + 2; }
    function selHiddenBelow(p) { var r = selRect(p, true); return !!r && r.top >= log.getBoundingClientRect().bottom - 2; }
    /** No text in the log before (or after) this point: it is the log's own start (or end). */
    function selAtLogEdge(p, atEnd) {
      try {
        var r = document.createRange();
        r.selectNodeContents(log);
        if (atEnd) r.setStart(p.n, p.o); else r.setEnd(p.n, p.o);
        return !r.toString().trim();
      } catch (e) { return false; }
    }

    function selContain(on) {
      var top = document.documentElement;
      if (on) {
        if (log.classList.contains('cc-sel-host')) return;
        log.classList.add('cc-sel-host');
        for (var n = log.parentNode; n && n.nodeType === 1 && n !== top; n = n.parentNode) n.classList.add('cc-sel-path');
        top.classList.add('cc-selecting');
        return;
      }
      if (!log.classList.contains('cc-sel-host')) return;
      log.classList.remove('cc-sel-host');
      if (document.querySelector('.cc-sel-host')) return;   // another chat has the selection now
      top.classList.remove('cc-selecting');
      [].forEach.call(document.querySelectorAll('.cc-sel-path'), function (n) { n.classList.remove('cc-sel-path'); });
    }
    function selEnd() { selSaved = null; selContain(false); }

    function selAutoScroll(p, isEnd) {
      var now = Date.now();
      if (now - selScrollAt < 160) return;
      var r = selRect(p, isEnd);
      if (!r) return;
      var box = log.getBoundingClientRect();
      var step = Math.max(72, Math.round(log.clientHeight * 0.3));
      if (r.bottom > box.bottom - SEL_EDGE && log.scrollTop + log.clientHeight < log.scrollHeight - 1) {
        selScrollAt = now;
        log.scrollBy({ top: step, behavior: 'smooth' });
      } else if (r.top < box.top + SEL_EDGE && log.scrollTop > 0) {
        selScrollAt = now;
        log.scrollBy({ top: -step, behavior: 'smooth' });
      }
    }

    function onSelectionChange() {
      if (destroyed) return;
      var sel = document.getSelection && document.getSelection();
      if (!sel || !sel.rangeCount || sel.isCollapsed || !log.clientHeight) { selEnd(); return; }
      var r = sel.getRangeAt(0);
      var s = { n: r.startContainer, o: r.startOffset }, e = { n: r.endContainer, o: r.endOffset };
      var sIn = log.contains(s.n), eIn = log.contains(e.n);
      if (selSaved && (!selSaved.s.n.isConnected || !selSaved.e.n.isConnected)) selSaved = null;   // the text was re-rendered
      if (!selSaved) {
        if (sIn && eIn) { selSaved = { s: s, e: e }; selContain(true); } else selContain(false);
        return;
      }
      // A finger or the mouse is on the page: the reader is making this
      // selection directly (a new long-press, a mouse drag), and the browser
      // gets that right. Only a handle drag - which the page never sees as a
      // touch - is second-guessed.
      var direct = selTouches > 0 || selMouse || Date.now() - selTouchEndAt < 350;
      if (direct) {
        if (!sIn && !eIn) { selEnd(); return; }
      } else {
        var fixS = !selSame(s, selSaved.s) && selHiddenAbove(selSaved.s);
        var fixE = !selSame(e, selSaved.e) && selHiddenBelow(selSaved.e);
        // "Select all" moves both ends to the ends of the log, on purpose. The
        // browser may put them just outside it (the page's own start and end);
        // with the rest of the page unselectable that is the same text, so say
        // it in the log's terms and leave it be.
        var all = (fixS || fixE || !sIn || !eIn) && selAtLogEdge(s, false) && selAtLogEdge(e, true);
        if (all) {
          fixS = false; fixE = false;
          if (!sIn || !eIn) {
            try { sel.setBaseAndExtent(log, 0, log, log.childNodes.length); s = { n: log, o: 0 }; e = { n: log, o: log.childNodes.length }; } catch (err) {}
          }
        } else {
          // An end dragged right out of the log has nowhere to go; keep it.
          if (!fixS && !sIn) fixS = true;
          if (!fixE && !eIn) fixE = true;
        }
        if (fixS || fixE) {
          var ns = fixS ? selSaved.s : s, ne = fixE ? selSaved.e : e;
          if (selCmp(ns, ne) < 0) {
            try {
              // The end being put back is the fixed one, so it is the base; the
              // other is the one under the reader's finger.
              if (fixE && !fixS) sel.setBaseAndExtent(ne.n, ne.o, ns.n, ns.o);
              else sel.setBaseAndExtent(ns.n, ns.o, ne.n, ne.o);
              s = ns; e = ne;
            } catch (err) { /* leave what the browser chose */ }
          }
        }
      }
      var movedS = !selSame(s, selSaved.s), movedE = !selSame(e, selSaved.e);
      selSaved = { s: s, e: e };
      selContain(true);
      // One end moved: follow it. Both moved: a new selection, nothing to follow.
      // A mouse drag is scrolled by the browser already.
      if (!selMouse && movedS !== movedE) selAutoScroll(movedE ? e : s, movedE);
    }
    function onSelTouch(ev) {
      selTouches = ev.touches ? ev.touches.length : 0;
      if (!selTouches) selTouchEndAt = Date.now();
    }
    function onSelMouseDown() { selMouse = true; }
    function onSelMouseUp() { selMouse = false; }
    document.addEventListener('selectionchange', onSelectionChange);
    window.addEventListener('touchstart', onSelTouch, { capture: true, passive: true });
    window.addEventListener('touchend', onSelTouch, { capture: true, passive: true });
    window.addEventListener('touchcancel', onSelTouch, { capture: true, passive: true });
    window.addEventListener('mousedown', onSelMouseDown, true);
    window.addEventListener('mouseup', onSelMouseUp, true);
    function selDispose() {
      document.removeEventListener('selectionchange', onSelectionChange);
      window.removeEventListener('touchstart', onSelTouch, true);
      window.removeEventListener('touchend', onSelTouch, true);
      window.removeEventListener('touchcancel', onSelTouch, true);
      window.removeEventListener('mousedown', onSelMouseDown, true);
      window.removeEventListener('mouseup', onSelMouseUp, true);
      selEnd();
    }

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
      // A turn under way means the suggestion answered an older one.
      if (s !== 'idle') suggestion = null;
      renderSuggestion();
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
          // In a collaborator's chat the owner can type too. Their messages are
          // labelled: while one is the latest, Claude's prompts are approved
          // without asking, so both people should be able to see whose turn it is.
          var labelled = e.by === 'owner' || e.by === 'system';
          var w = el('div', 'cc-user' + (labelled ? ' by-owner' : ''));
          if (e.by === 'owner') w.appendChild(el('div', 'cc-user-by', 'Owner'));
          // Crundi posting the output of a command the owner ran.
          if (e.by === 'system') w.appendChild(el('div', 'cc-user-by', 'Crundi'));
          w.appendChild(el('div', 'cc-user-body', linkify(esc(e.text))));
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
        case 'goal-verdict': {
          var gv = el('div', 'cc-gv');
          gv.appendChild(el('div', 'cc-gv-head',
            '<span class="cc-gv-dot"></span><span>Goal not yet met · check ' + (e.n || 1) + '</span>'));
          gv.appendChild(el('div', 'cc-gv-body', esc(e.text)));
          node.appendChild(gv);
          break;
        }
        case 'notice': node.appendChild(el('div', 'cc-notice', esc(e.text))); break;
        case 'error':  node.appendChild(el('div', 'cc-error', esc(e.text))); break;
        // A transient entry the server retired (e.g. the startup placeholder).
        case 'gone':   node.style.display = 'none'; break;
      }
    }

    var thinkUser = new Map();     // id -> 'open' | 'closed', set by the user
    var thinkAuto = new Map();     // id -> 'open' | 'closed', set by the rules
    var thinkTimers = new Map();   // id -> timer that folds it after it finishes
    var THINK_LINGER_MS = 5000;
    function thinkCollapsed(e) {
      var u = thinkUser.get(e.id);
      if (u) return u === 'closed';
      var a = thinkAuto.get(e.id);
      if (a) return a === 'closed';
      return !e.streaming;          // from history, or already finished: closed
    }
    // Fold a block now (unless the user has chosen for it), animating in place.
    function autoCloseThinking(id) {
      clearTimeout(thinkTimers.get(id)); thinkTimers.delete(id);
      if (thinkUser.has(id)) return;
      thinkAuto.set(id, 'closed');
      var rec = entries.get(id);
      var box = rec && rec.node.querySelector('.cc-think');
      if (box) box.classList.add('cc-collapsed');
    }
    // Called whenever a thinking entry is added or changes.
    function thinkingChanged(e, isNew) {
      if (isNew && e.streaming) {
        // A newer thought folds the earlier ones away.
        thinkAuto.forEach(function (v, id) { if (id !== e.id && v === 'open') autoCloseThinking(id); });
        thinkAuto.set(e.id, 'open');
      }
      if (!e.streaming && thinkAuto.get(e.id) === 'open' && !thinkTimers.has(e.id)) {
        thinkTimers.set(e.id, setTimeout(function () { autoCloseThinking(e.id); }, THINK_LINGER_MS));
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
      // Open while it is thinking, folded away a little after it finishes or
      // as soon as a newer one starts, and closed when loaded from history.
      // Once you open or close one yourself, that wins and it stays put.
      // Kept by entry id, since streaming repaints the node.
      var box = el('div', 'cc-think' + (thinkCollapsed(e) ? ' cc-collapsed' : '') + (e.streaming ? ' live' : ''));
      var head = el('div', 'cc-think-head', '<span class="cc-caret">▾</span><span>Thinking</span>');
      var body = el('div', 'cc-think-body', esc(e.text) + (e.streaming ? '<span class="cc-think-caret"></span>' : ''));
      head.addEventListener('click', function () {
        var closed = box.classList.toggle('cc-collapsed');
        thinkUser.set(e.id, closed ? 'closed' : 'open');
      });
      box.appendChild(head);
      box.appendChild(body);
      return box;
    }

    function toolNode(e) {
      // A Task/Agent tool row IS a subagent. Give it the same drill-in as the
      // agents list, so the transcript stays reachable in chronological place
      // after the agent has been dismissed from the badge.
      if (e.toolUseId && agents.has(e.toolUseId) && agents.get(e.toolUseId).meta.kind !== 'task') {
        var ab = el('div', 'cc-tool cc-agentrow');
        var am = agents.get(e.toolUseId).meta;
        var arun = am.status === 'running' || !am.status;
        ab.innerHTML =
          (arun ? '<span class="cc-spin"></span>'
                : '<span class="' + (am.status === 'failed' ? 'cc-dot-err' : 'cc-dot-ok') + '"></span>')
          + '<span class="cc-tool-name">' + esc(am.subagentType || 'Agent') + '</span>'
          + '<span class="cc-tool-sum">' + esc(am.description || '') + '</span>'
          + '<span class="cc-agentrow-go">transcript ›</span>';
        ab.addEventListener('click', function () { toggleAgentPanel(e.toolUseId, false); });
        return ab;
      }
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
      // The host may put something of its own under a finished tool call (a
      // panel Claude opened inline, the screenshots from a check). It is the
      // host's node, built from the host's data: nothing from the tool's
      // result is written into the page here.
      if (opts.toolExtra && e.status !== 'running') {
        var extra = null;
        try { extra = opts.toolExtra(e); } catch (x) { extra = null; }
        if (extra) {
          var wrap = el('div', 'cc-tool-wrap');
          wrap.appendChild(box);
          wrap.appendChild(extra);
          return wrap;
        }
      }
      return box;
    }

    // Permission prompt: Allow / Allow always / Deny, mirroring the CLI's own
    // options. "Always" replays the CLI's permission_suggestions verbatim.
    function permissionNode(e) {
      var box = el('div', 'cc-ask perm');
      if (e.status !== 'pending') {
        // Anything that is not an actual answer must not read as one. This
        // branch used to print "allowed" for every status except denied, so a
        // question nobody ever answered came back claiming you had approved it.
        var closed = e.status === 'unanswered' || e.status === 'cancelled';
        var tag = e.status === 'denied' ? 'denied'
          : closed ? 'never answered'
            : e.always ? 'always allowed' : 'allowed';
        box.appendChild(el('div', 'cc-answered',
          '<b>' + esc(e.displayName || e.toolName) + '</b>'
          + '<span class="cc-tag ' + (e.status === 'denied' || closed ? 'no' : 'ok') + '">' + tag + '</span>'));
        // Show WHAT it wanted. A closed ask you cannot see the contents of
        // tells you a decision was missed but not which one.
        if (closed && e.title) box.appendChild(el('div', 'cc-ask-sub', esc(e.title)));
        return box;
      }
      box.appendChild(el('div', 'cc-ask-title', esc(e.title || ('Claude wants to use ' + (e.displayName || e.toolName)))));
      if (e.description) box.appendChild(el('div', 'cc-ask-sub', esc(e.description)));
      if (e.decisionReason) box.appendChild(el('div', 'cc-ask-reason', esc(stripAnsi(e.decisionReason))));
      box.appendChild(el('div', 'cc-ask-pre', esc(truncate(JSON.stringify(e.input, null, 2), 2500))));

      // ─── Escalated to the project owner ───
      //
      // An outside collaborator cannot answer their own permission prompt: on
      // a machine with passwordless sudo, one approval is the whole machine.
      // The card still appears here so they can SEE what was asked and why
      // they are stuck, but the only button is the one that unsticks them.
      if (e.escalated) {
        box.appendChild(el('div', 'cc-ask-reason',
          'Sent to the project owner. You will see the answer here.'));
        var ebtns = el('div', 'cc-btns');
        var drop = el('button', 'cc-btn', 'Carry on without this');
        drop.addEventListener('click', function () {
          drop.disabled = true;
          // Withdrawing is not denying: the server tells Claude the
          // collaborator took it back and to stop asking, rather than that the
          // owner refused, which Claude should treat far more seriously.
          apiFetch('/api/collab/cancel', {
            method: 'POST',
            body: JSON.stringify({ id: e.approvalId }),
          }).catch(function () { drop.disabled = false; });
        });
        ebtns.appendChild(drop);
        box.appendChild(ebtns);
        return box;
      }

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

      // A restart orphans an in-flight question: its control request belonged
      // to a process that has exited, so no control response can satisfy it.
      // The card stays fully answerable anyway — the answer goes back as an
      // ordinary message instead. Claude has the whole transcript on resume, so
      // it reads the same either way, and a decision you already made is not
      // work worth throwing away.
      var orphaned = e.status === 'unanswered' || e.status === 'cancelled';
      if (e.status !== 'pending' && !orphaned) {
        var answered = (e.answeredInput && e.answeredInput.answers) || {};
        var html = e.status === 'denied'
          ? '<span class="cc-tag no">dismissed</span>'
          : Object.keys(answered).map(function (k) {
              return '<span class="cc-tag ok">' + esc(answered[k]) + '</span>';
            }).join(' ');
        var ann = (e.answeredInput && e.answeredInput.annotations) || {};
        Object.keys(ann).forEach(function (k) {
          if (ann[k] && ann[k].notes) html += '<div class="cc-ans-note">Note: ' + esc(ann[k].notes) + '</div>';
        });
        box.appendChild(el('div', 'cc-answered', html || '<span class="cc-tag">answered</span>'));
        return box;
      }
      if (orphaned) {
        // Say why the buttons behave differently, rather than letting it look
        // like an ordinary live question that happens to be old.
        box.appendChild(el('div', 'cc-ask-sub',
          'This question outlived the session that asked it. Answering sends your choice as a message.'));
      }

      // note: free text the CLI passes to Claude as annotations[question].notes.
      // One per question; noteOpt is the choice it is shown under.
      var picks = questions.map(function () { return { chosen: [], other: '', note: '', noteOpt: '' }; });
      var NOTE_SVG = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'
        + '<path d="M15.5 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8.5z"/><path d="M15 3v6h6"/><line x1="8" y1="13" x2="14" y2="13"/><line x1="8" y1="17" x2="12" y2="17"/></svg>';
      var qNodes = [];

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
          var nb = el('button', 'cc-opt-nb', NOTE_SVG);
          nb.type = 'button';
          nb.title = 'Add a note to this choice';
          nb.addEventListener('click', function (ev) {
            ev.preventDefault(); ev.stopPropagation();
            // Picks the choice first if it is not already picked.
            if (!inp.checked) { inp.checked = true; inp.dispatchEvent(new Event('change')); }
            picks[qi].noteOpt = opt.label;
            drawNote(qi, true);
            saveAnswer();
          });
          row.appendChild(nb);
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
            var choiceInputs = qEl.querySelectorAll('input[name="' + name + '"]');
            qEl.querySelectorAll('.cc-opt').forEach(function (r, ri) {
              r.classList.toggle('sel', multi ? choiceInputs[ri].checked : ri === oi);
            });
            // A note follows the selection: it moves to the newly picked choice.
            if (picks[qi].noteOpt && picks[qi].chosen.indexOf(picks[qi].noteOpt) < 0) {
              picks[qi].noteOpt = picks[qi].chosen[picks[qi].chosen.length - 1] || '';
              drawNote(qi, false);
            }
            syncSubmit();
            saveAnswer();
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
          saveAnswer();
        });
        qEl.appendChild(otherInput);
        box.appendChild(qEl);
        qNodes.push({ qEl: qEl, otherInput: otherInput, name: name, noteBox: null });
      });

      // Show the question's note under the choice it belongs to, or nowhere.
      function drawNote(qi, focus) {
        var n = qNodes[qi], p = picks[qi];
        if (!n) return;
        if (n.noteBox) { n.noteBox.remove(); n.noteBox = null; }
        n.qEl.querySelectorAll('.cc-opt-nb').forEach(function (b) { b.classList.remove('on'); });
        if (!p.noteOpt) return;
        var rows = n.qEl.querySelectorAll('.cc-opt'), inputs = n.qEl.querySelectorAll('input[name="' + n.name + '"]');
        var row = null;
        for (var i = 0; i < inputs.length; i++) if (inputs[i].value === p.noteOpt) row = rows[i];
        if (!row) return;
        var nbtn = row.querySelector('.cc-opt-nb'); if (nbtn) nbtn.classList.add('on');
        var boxN = el('div', 'cc-opt-note');
        var ti = el('input');
        ti.type = 'text';
        ti.placeholder = 'Note for Claude about this choice';
        ti.value = p.note;
        var x = el('button', '', '\u2715');
        x.type = 'button';
        x.title = 'Remove the note';
        ti.addEventListener('input', function () { p.note = ti.value; saveAnswer(); });
        // Typing a space or Enter in the note must not toggle the choice it sits in.
        ti.addEventListener('click', function (ev) { ev.stopPropagation(); });
        ti.addEventListener('keydown', function (ev) { if (ev.key === 'Enter') ev.preventDefault(); ev.stopPropagation(); });
        x.addEventListener('click', function (ev) {
          ev.preventDefault(); ev.stopPropagation();
          p.note = ''; p.noteOpt = ''; drawNote(qi, false); saveAnswer();
        });
        boxN.appendChild(ti);
        boxN.appendChild(x);
        row.querySelector('div').appendChild(boxN);
        n.noteBox = boxN;
        if (focus) ti.focus();
      }

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

      // Half-finished answers survive leaving the cell, exactly like the message
      // draft does. Answering one of Claude's questions often means going away
      // to look something up first, and the card is rebuilt from scratch when
      // the panel remounts — so without this, the trip costs you the answer.
      var ANS_KEY = 'crundi_chat_ans_' + sessionId + '_' + e.id;
      function saveAnswer() {
        try {
          var any = picks.some(function (p) { return (p.chosen && p.chosen.length) || p.other || p.note; });
          if (any) localStorage.setItem(ANS_KEY, JSON.stringify({ v: picks, t: Date.now() }));
          else localStorage.removeItem(ANS_KEY);
        } catch (err) {}
      }
      function clearAnswer() { try { localStorage.removeItem(ANS_KEY); } catch (err) {} }
      function restoreAnswer() {
        var saved = null;
        try {
          var rawAns = JSON.parse(localStorage.getItem(ANS_KEY) || 'null');
          // Older answers were a bare array, before they carried a timestamp.
          saved = Array.isArray(rawAns) ? rawAns : (rawAns && rawAns.v);
        } catch (err) { saved = null; }
        if (!saved || !saved.length) return;
        saved.forEach(function (p, qi) {
          if (!p || !qNodes[qi]) return;
          picks[qi].chosen = p.chosen || [];
          picks[qi].other = p.other || '';
          picks[qi].note = p.note || '';
          picks[qi].noteOpt = p.noteOpt || '';
          var n = qNodes[qi];
          n.otherInput.value = picks[qi].other;
          var inputs = n.qEl.querySelectorAll('input[name="' + n.name + '"]');
          var rows = n.qEl.querySelectorAll('.cc-opt');
          for (var i = 0; i < inputs.length; i++) {
            var on = picks[qi].chosen.indexOf(inputs[i].value) >= 0;
            inputs[i].checked = on;
            if (rows[i]) rows[i].classList.toggle('sel', on);
          }
          drawNote(qi, false);
        });
        syncSubmit();
      }
      restoreAnswer();

      submit.addEventListener('click', function () {
        var answers = {}, annotations = {}, anyNote = false;
        questions.forEach(function (q, qi) {
          answers[q.question] = answerFor(qi);
          var note = (picks[qi].note || '').trim();
          if (note) { annotations[q.question] = { notes: note }; anyNote = true; }
        });
        clearAnswer();
        if (orphaned) {
          var lines = questions.map(function (q, qi) {
            var note = (picks[qi].note || '').trim();
            return q.question + ' -> ' + answerFor(qi) + (note ? ' (note: ' + note + ')' : '');
          });
          answerOrphaned(e, answers,
            'Answering the question from before the session restarted:' + String.fromCharCode(10) + lines.join(String.fromCharCode(10)));
          return;
        }
        // Notes ride the tool's own annotations field, which the CLI puts in
        // the answer Claude reads, so nothing is appended to the answer text.
        var upd = { answers: answers };
        if (anyNote) upd.annotations = annotations;
        respond(e, { behavior: 'allow', updatedInput: Object.assign({}, e.input, upd) });
      });
      skip.addEventListener('click', function () {
        clearAnswer();
        // Dismissing an orphaned card just closes it. There is nothing waiting
        // on an answer, so sending "I dismissed it" would be noise.
        if (orphaned) { answerOrphaned(e, {}, ''); return; }
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

    /**
     * Answer a question whose original request is gone.
     *
     * Recorded on the SERVER, not just here: marking it only in this browser
     * would leave the next reload — or the next restart — offering the same
     * question again as though it had never been answered.
     */
    function answerOrphaned(e, answers, text) {
      var rec = entries.get(e.id);
      if (rec) {
        rec.data.status = 'answered-late';
        rec.data.answeredInput = Object.assign({}, e.input, { answers: answers });
        paint(rec.node, rec.data);
      }
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/answer-closed', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ entryId: e.id, answers: answers, text: text }),
      }).then(function (r) { return r.json(); }).then(function (d) {
        if (d && d.ok === false) appendLocal({ kind: 'error', text: d.error || 'Could not record that answer' });
      }).catch(function (err) { appendLocal({ kind: 'error', text: String(err.message || err) }); });
    }

    // `slot` is the drawer entry for a message sent while busy, if any. The
    // server's reply settles it: sent straight through (the turn had already
    // ended) means it is in the transcript and the drawer must let go; failed
    // means it never went, so the words go back in the box.
    function postMessage(text, slot) {
      // Any message going out retires the suggestion (the server does too).
      suggestion = null;
      renderSuggestion();
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text: text }),
      }).then(function (r) { return r.json(); }).then(function (d) {
        if (d && d.ok === false) { giveBack(slot); appendLocal({ kind: 'error', text: d.error || 'Failed to send' }); return; }
        if (!slot || handedOver.indexOf(slot) < 0) return;
        if (d && d.injected === false) {
          handedOver = handedOver.filter(function (h) { return h !== slot; });
          renderQueue();
        } else if (d && d.uuid && !slot.uuid) {
          slot.uuid = d.uuid;
        }
      }).catch(function (err) { giveBack(slot); appendLocal({ kind: 'error', text: String(err.message || err) }); });
    }

    function giveBack(slot) {
      if (!slot || handedOver.indexOf(slot) < 0) return;
      handedOver = handedOver.filter(function (h) { return h !== slot; });
      renderQueue();
      var cur = input.value.trim();
      input.value = cur ? slot.text + '\n' + cur : slot.text;
      saveDraft();
      autoGrow();
    }

    /**
     * Ask the session to drop a handed-over message before Claude reads it.
     *
     * cancelled=false is not an error: it means the CLI had already dequeued it
     * for execution. Saying so plainly beats a success toast for something that
     * did not happen — the user is about to see their message answered.
     */
    function recallQueued() {
      // Everything still waiting goes back at once: what Claude has been
      // handed but not read, plus anything held locally. The drawers show it
      // stacked as one message and Claude would read it as one, so taking back
      // just the oldest line left the rest to go out on their own.
      var held = queued.length ? queuedText() : '';
      if (held) { stopTicker(); queued = []; renderQueue(); }
      var slots = handedOver.filter(function (h) { return h.uuid && h.state === 'queued'; });
      function putBack(texts) {
        if (held) texts.push(held);
        if (!texts.length) return;
        var restored = texts.join('\n');
        var cur = input.value.trim();
        input.value = cur ? restored + '\n' + cur : restored;
        saveDraft();
        autoGrow();
        input.focus();
        try { input.setSelectionRange(input.value.length, input.value.length); } catch (e) {}
      }
      if (!slots.length) { putBack([]); return; }
      Promise.all(slots.map(function (slot) {
        return apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/cancel-queued', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ uuid: slot.uuid }),
        }).then(function (r) { return r.json(); })
          .then(function (d) { return { slot: slot, d: d }; })
          .catch(function (err) { return { slot: slot, d: { ok: false, error: String(err.message || err) } }; });
      })).then(function (results) {
        var back = [], failed = '';
        results.forEach(function (x) {
          if (x.d && x.d.ok !== false && x.d.cancelled) {
            handedOver = handedOver.filter(function (h) { return h !== x.slot; });
            back.push(x.slot.text);
          } else if (x.d && x.d.ok !== false) {
            // Not an error: the CLI had already started reading this one.
            x.slot.state = 'started';
          } else if (!failed) {
            failed = (x.d && x.d.error) || 'Could not take it back';
          }
        });
        renderQueue();
        putBack(back);
        if (failed) appendLocal({ kind: 'error', text: failed });
      });
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
    //
    // So there is no batching delay: Enter hands the message over at once, and
    // the "Sending" drawer's take-back (a cancel on the wire) covers changing
    // your mind for as long as Claude has not picked it up. Holding it until
    // typing went quiet used to cost at least 1.5s, and indefinitely while the
    // next line was being typed. The one wait left is a pending permission
    // prompt, which has to be answered before the CLI reads anything else.
    var TICK_MS = 300;

    function enqueue(text) {
      queued.push(text);
      if (state !== 'needs-input') { flushQueue(); return; }
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
        flushQueue();
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

    // The drawer body is clamped to a few lines in CSS, so a long message can
    // no longer take the whole pane. Say when that happened: text that is
    // silently cut off reads as text that was lost.
    function markTrimmed(node) {
      var body = node.querySelector('.cc-queue-body');
      var hint = node.querySelector('.cc-queue-hint');
      if (!body || !hint) return;
      var trimmed = body.scrollHeight - body.clientHeight > 2;
      node.classList.toggle('trimmed', trimmed);
      if (trimmed) hint.textContent = 'Shown trimmed \u00b7 ' + hint.textContent;
    }

    // A mouse and keyboard can also use the up arrow; a touchscreen just taps.
    function takeBackHint() {
      var desktop = window.matchMedia && window.matchMedia('(any-pointer: fine)').matches;
      return desktop ? 'Click or press \u2191 to take it back' : 'Tap to take it back';
    }

    function renderQueue() {
      renderSent();
      if (!queued.length) {
        if (queueNode) { queueNode.remove(); queueNode = null; }
        return;
      }
      if (!queueNode) {
        queueNode = el('div', 'cc-queue');
        queueNode.title = 'Click to edit — brings every queued line back to the message box';
        queueNode.addEventListener('click', recallQueued);
        wrap.insertBefore(queueNode, inrow);
      }
      var n = queued.length;
      queueNode.innerHTML = '<div class="cc-queue-head">'
        + '<span>Sending</span><span style="opacity:.7;font-weight:500;text-transform:none;letter-spacing:0">'
        + (n === 1 ? '1 line' : n + ' lines') + ' · as one message, at the next tool call</span></div>'
        + '<div class="cc-queue-body">' + esc(queuedText()) + '</div>'
        + '<div class="cc-queue-hint">' + takeBackHint() + '</div>';
      markTrimmed(queueNode);
    }

    // Suggested next message. Shown only while idle on a live session; a tap
    // sends it as typed, the x drops it here and on the server so a reload
    // does not bring it back.
    function renderSuggestion() {
      if (!suggestion || state !== 'idle' || input.disabled) {
        if (sugNode) { sugNode.remove(); sugNode = null; }
        return;
      }
      var stick = atBottom();
      // Last in the floating dock, under the badges. resetAgents empties the
      // dock, so it is re-created when missing.
      if (!sugNode || sugNode.parentNode !== agentDock) {
        sugNode = el('div', 'cc-sug-row');
        sugNode.addEventListener('click', function (e) {
          if (e.target.closest('.cc-sug-x')) { dismissSuggestion(); return; }
          if (e.target.closest('.cc-sug')) sendSuggestion();
        });
        agentDock.appendChild(sugNode);
      }
      sugNode.innerHTML = '<div class="cc-sug-cap">Suggested, tap to send</div>'
        + '<div class="cc-sug" role="button" tabindex="0" title="Tap to send">' + esc(suggestion.text)
        + '<button class="cc-sug-x" title="Dismiss" aria-label="Dismiss suggestion">\u2715</button></div>';
      dockLayout();
      scrollDown(stick);
    }

    function sendSuggestion() {
      if (!suggestion || state !== 'idle') return;
      var text = suggestion.text;
      stickBottom = true;
      scrollDown(true);
      postMessage(text);
      setState('working');
    }

    function dismissSuggestion() {
      if (!suggestion) return;
      suggestion = null;
      renderSuggestion();
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/dismiss-suggestion', { method: 'POST' })
        .catch(function () {});
    }

    // The second drawer: written to the CLI's stdin, not yet taken into the
    // turn. Deliberately its own node rather than a mode of the first — a new
    // batch being typed must not hide the one still in flight, which is the
    // whole failure this exists to fix.
    function renderSent() {
      if (!handedOver.length) {
        if (sentNode) { sentNode.remove(); sentNode = null; }
        return;
      }
      if (!sentNode) {
        sentNode = el('div', 'cc-queue sent');
        sentNode.addEventListener('click', recallQueued);
        sentNode.title = 'Already handed to Claude — it joins the conversation the moment Claude picks it up';
        wrap.insertBefore(sentNode, queueNode || inrow);
      }
      // Recallable only while the CLI reports it QUEUED. Once it says
      // 'started' the message has drained into a turn and is genuinely gone —
      // that is a real signal now, not the assumption this used to make.
      var recallable = handedOver.some(function (h) { return h.uuid && h.state === 'queued'; });
      var started = handedOver.some(function (h) { return h.state === 'started'; });
      // One action, worded exactly as the drawer above it: click, and the text
      // comes back to the box to edit. Whether that needs a cancel on the wire
      // is our problem — it is not a second concept for the user to learn, and
      // the two drawers must not look like different features.
      var hint = recallable
        ? takeBackHint()
        : (started
          ? 'Claude is reading this one — too late to edit'
          : 'Waiting for the session to confirm');
      sentNode.classList.toggle('recallable', recallable);
      sentNode.title = recallable
        ? 'Click to edit — brings it back to the message box'
        : 'Claude has this one; it joins the conversation as it is read';
      sentNode.innerHTML = '<div class="cc-queue-head">'
        + '<span>' + (recallable ? 'Sending' : 'Sent') + '</span><span style="opacity:.7;font-weight:500;text-transform:none;letter-spacing:0">'
        + (recallable ? 'not read yet' : started ? 'Claude is reading it'
          : 'waiting for Claude to pick it up') + '</span></div>'
        + '<div class="cc-queue-body">' + esc(handedOver.map(function (h) { return h.text; }).join('\n')) + '</div>'
        + '<div class="cc-queue-hint">' + hint + '</div>';
      markTrimmed(sentNode);
    }

    function flushQueue() {
      stopTicker();

      if (!queued.length || destroyed) return;
      var text = queuedText();
      queued = [];
      // Stays on screen, now marked as handed over, until the server reports
      // that the CLI actually took it (the 'injected' event).
      // uuid arrives with the CLI's 'queued' lifecycle frame; until then
      // there is nothing to name in a recall request.
      var slot = { text: text, uuid: null, state: 'sent' };
      handedOver.push(slot);
      renderQueue();
      postMessage(text, slot);
      // Go busy immediately, for the same reason doSend does. A message queued
      // just as the turn ends flushes AFTER the server has reported idle, so
      // until its next state event lands the UI says idle while Claude is
      // already working on it. Optimistic here, corrected by the server either
      // way — and injections that land mid-turn are already 'working', where
      // this is a no-op.
      setState('working');
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

    // An attachment's path goes in where the caret is (or replaces a
    // selection), spaced from its neighbours — not on the end of the message.
    function insertPath(p) {
      var v = input.value;
      var a = typeof input.selectionStart === 'number' ? input.selectionStart : v.length;
      var b = typeof input.selectionEnd === 'number' ? input.selectionEnd : v.length;
      var before = v.slice(0, a), after = v.slice(b);
      var ins = ((before && !/\s$/.test(before)) ? ' ' : '') + p + ((after && /^\s/.test(after)) ? '' : ' ');
      input.value = before + ins + after;
      var caret = before.length + ins.length;
      try { input.setSelectionRange(caret, caret); } catch (e) {}
      saveDraft();
      autoGrow();
      input.focus();
    }

    /** Show, move, or hide the upload bar. pct null hides it. */
    function showUpload(name, pct) {
      if (pct === null) { upRow.classList.remove('on'); return; }
      upRow.classList.add('on');
      upName.textContent = name;
      var p = Math.max(0, Math.min(1, pct));
      upFill.style.width = (p * 100).toFixed(0) + '%';
      // 100% while the server is still storing it would read as finished, so
      // the last step says so instead of showing a number.
      upPct.textContent = p >= 1 ? 'saving' : (p * 100).toFixed(0) + '%';
    }

    function uploadFile(file) {
      if (!file) return;
      if (!project) { toast('No project for this chat', 'error'); return; }
      attachBtn.classList.add('busy');
      var name = file.name || ('image.' + ((file.type || 'image/png').split('/')[1] || 'png'));
      showUpload(name, 0);

      var done = function (d) {
        if (d && d.ok && d.path) { insertPath(d.path); toast('Attached: ' + (d.name || name)); }
        else toast('Upload failed: ' + ((d && d.error) || '?'), 'error');
      };
      var failed = function (err) { toast('Upload failed: ' + (err.message || err), 'error'); };
      var always = function () {
        attachBtn.classList.remove('busy');
        // Leave the finished bar visible for a moment; vanishing instantly on a
        // fast connection just flickers.
        setTimeout(function () { showUpload(name, null); }, 400);
      };

      file.arrayBuffer().then(function (buf) {
        var payload = { project: project, name: name, data: b64(buf) };
        if (apiUpload) {
          return apiUpload('/api/attachments/upload', payload, function (p) { showUpload(name, p); })
            .then(done, failed).then(always);
        }
        // Older host with no uploader: no progress to report, so the bar just
        // sits at the start rather than lying about how far along it is.
        return apiFetch('/api/attachments/upload', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload),
        }).then(function (r) { return r.json(); }).then(done, failed).then(always);
      }).catch(function (err) { failed(err); always(); });
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
      root.classList.add('cc-dropping');
    }
    // Has the drag really left the chat? relatedTarget cannot say: for a drag
    // from outside the window it is often null even while still inside, and
    // it is null again whenever what lies under the pointer changes, which
    // showing the drop hint itself does. So the answer comes from where the
    // pointer is, not from which element the browser last named.
    function dragStillInside(e) {
      var r = root.getBoundingClientRect();
      return e.clientX > r.left && e.clientX < r.right && e.clientY > r.top && e.clientY < r.bottom;
    }
    function onDragLeave(e) {
      if (!dragStillInside(e)) root.classList.remove('cc-dropping');
    }
    function onDrop(e) {
      if (!e.dataTransfer) return;
      e.preventDefault();
      e.stopPropagation();
      root.classList.remove('cc-dropping');
      var files = e.dataTransfer.files;
      if (files && files.length) {
        var paths = [];
        for (var i = 0; i < files.length; i++) {
          var f = files[i];
          if (f.type && f.type.indexOf('image/') === 0) { uploadFile(f); continue; }
          // The desktop app knows where the file really is, so its path is
          // enough. A browser is told only the name, which is no use to
          // Claude: there the file is uploaded and the stored path is what
          // goes in. One or the other, never both.
          var p = (window.api && window.api.getPathForFile && window.api.getPathForFile(f)) || f.path || '';
          if (p) paths.push(p); else uploadFile(f);
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
    /**
     * Paste an image into the chat.
     *
     * Bound to WINDOW, not the textarea. A paste does not necessarily target
     * the field — on Android an image paste often lands on the document — so a
     * listener on the input alone never fires and the screenshot is lost with
     * no sign that anything happened.
     *
     * clipboardData.files is the reliable accessor; .items needs a kind check
     * and misses cases .files catches.
     *
     * Scoped to the chat the user is actually in: several chats can be open at
     * once and every one of them would otherwise upload the same image.
     */
    var lastPasteEvent = null;
    function onPaste(e) {
      if (destroyed) return;
      // Bound on the element AND on window, so the same event arrives twice as
      // it bubbles. Upload it once.
      if (e === lastPasteEvent) return;
      lastPasteEvent = e;
      if (!root.contains(document.activeElement) && LAST_FOCUSED !== sessionId) return;
      var files = (e.clipboardData && e.clipboardData.files) || [];
      for (var i = 0; i < files.length; i++) {
        if (files[i]) { e.preventDefault(); uploadFile(files[i]); return; }
      }
      var items = (e.clipboardData && e.clipboardData.items) || [];
      for (var j = 0; j < items.length; j++) {
        if (items[j].kind === 'file') {
          var f = items[j].getAsFile();
          if (f) { e.preventDefault(); uploadFile(f); return; }
        }
      }
    }
    // BOTH, deliberately.
    //
    // window catches a paste that does not target the field, which is common on
    // Android. The element listener is what Google's own AI-mode box has - its
    // "Ask anything" box is an ordinary <textarea> with paste bound directly on
    // it, and the Samsung keyboard DOES offer it a screenshot. Chromium decides
    // what to advertise to the keyboard from the focused editable, so a handler
    // on the element itself is not redundant with one on window.
    window.addEventListener('paste', onPaste);
    input.addEventListener('paste', onPaste);
    input.addEventListener('focus', function () { LAST_FOCUSED = sessionId; });

    // Drag a file anywhere onto the chat to attach it.
    // The sign that a drop will land here is the same dashed outline a drag
    // from the Files tab gives a pane, and nothing more. There used to be a
    // "Drop to attach" sheet over the chat as well; it shared a class name
    // with the outline's own mark, so its rules (absolutely placed, a flex
    // row, no pointer events) fell on the whole chat: the layout broke, and
    // with no pointer events the chat could neither take the drop nor notice
    // the drag leaving.
    function showDrop(on) { root.classList.toggle('cc-dropping', !!on); }
    root.addEventListener('dragover', function (e) {
      if (!e.dataTransfer || !Array.prototype.includes.call(e.dataTransfer.types || [], 'Files')) return;
      e.preventDefault(); e.stopPropagation();
      e.dataTransfer.dropEffect = 'copy';
      showDrop(true);
    });
    root.addEventListener('dragleave', function (e) { if (!dragStillInside(e)) showDrop(false); });
    // A drag that ends anywhere else (dropped on another pane, let go outside
    // the window, cancelled with Escape) leaves no hint behind.
    var dropDone = function () { showDrop(false); };
    window.addEventListener('drop', dropDone, true);
    window.addEventListener('dragend', dropDone, true);
    root._ccDropDone = dropDone;
    // Leaving the WINDOW can report a last position still inside the chat. No
    // dragover arrives after that, so the mark is dropped when they stop.
    var dropIdle = 0;
    root.addEventListener('dragover', function () { clearTimeout(dropIdle); dropIdle = setTimeout(dropDone, 600); });
    // (The drop itself is taken by onDrop above. A second handler here used
    // to upload the first file as well, so one dropped file arrived twice.)

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

    // Whether the caret is on the first VISUAL row of the input. Checking for a
    // newline before it was not enough: a long line wraps, and its second row
    // has none. A hidden copy of the box, styled the same, shows where the text
    // up to the caret ends.
    function caretOnTopRow() {
      var pos = input.selectionStart;
      if (pos === 0) return true;
      if (input.value.slice(0, pos).indexOf('\n') >= 0) return false;
      var cs = window.getComputedStyle(input);
      var m = document.createElement('div');
      ['boxSizing', 'width', 'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft',
        'borderTopWidth', 'borderRightWidth', 'borderBottomWidth', 'borderLeftWidth',
        'fontFamily', 'fontSize', 'fontWeight', 'fontStyle', 'letterSpacing', 'lineHeight',
        'textTransform', 'wordSpacing', 'tabSize'].forEach(function (k) { m.style[k] = cs[k]; });
      m.style.position = 'absolute'; m.style.visibility = 'hidden'; m.style.top = '0'; m.style.left = '-9999px';
      m.style.whiteSpace = 'pre-wrap'; m.style.overflowWrap = 'break-word'; m.style.wordBreak = cs.wordBreak;
      m.style.borderStyle = 'solid'; m.style.overflow = 'hidden';
      m.textContent = input.value.slice(0, pos);
      var mark = document.createElement('span');
      mark.textContent = '\u200b';
      m.appendChild(mark);
      document.body.appendChild(m);
      var lh = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.45;
      var top = mark.offsetTop - (parseFloat(cs.paddingTop) || 0) - (parseFloat(cs.borderTopWidth) || 0);
      m.remove();
      return top < lh / 2;
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
      // Up arrow on the input's top line takes back what is waiting to go —
      // the same as clicking the drawer. Anywhere lower it moves the caret as
      // usual, and with nothing to take back it does nothing special.
      if (ev.key === 'ArrowUp' && !ev.shiftKey && !ev.ctrlKey && !ev.metaKey && !ev.altKey
          && input.selectionStart === input.selectionEnd
          && caretOnTopRow()) {
        var canRecall = queued.length || handedOver.some(function (h) { return h.uuid && h.state === 'queued'; });
        if (canRecall) { ev.preventDefault(); recallQueued(); return; }
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
    // unsent message would be lost. Deliberately NOT cleared on destroy() —
    // destroy is exactly the case we're protecting.
    //
    // Keyed per session AND per project, because the session id is not durable
    // enough on its own: restarting Crundi drops every in-memory session, so
    // relaunching the chat mints a NEW id and a draft saved under the old one
    // is stranded forever. The conversation survives a restart (we replay the
    // stored transcript), so the draft has to survive with it — and the project
    // is the identity that lasts, matching one-transcript-per-project. The
    // project copy is only adopted by a chat that has no draft of its own and
    // while no other open chat has claimed it, so opening a second chat in the
    // same project cannot steal the first one's text.
    var DRAFT_KEY = 'crundi_chat_draft_' + sessionId;
    var DRAFT_PROJ_KEY = project ? 'crundi_chat_draft_p_' + project : '';
    var claimedProject = false;

    function writeDraft(key, text) {
      if (!key) return;
      if (text) localStorage.setItem(key, JSON.stringify({ v: text, t: Date.now() }));
      else localStorage.removeItem(key);
    }
    function readDraft(key) {
      if (!key) return '';
      var raw = localStorage.getItem(key);
      if (!raw) return '';
      // Drafts written before this became a timestamped record are bare text.
      if (raw.charAt(0) !== '{') return raw;
      try { return JSON.parse(raw).v || ''; } catch (e) { return raw; }
    }
    function saveDraft() {
      try {
        writeDraft(DRAFT_KEY, input.value);
        if (!DRAFT_PROJ_KEY) return;
        // The claim has to gate the WRITE, not just the read. Gating on the
        // local flag alone let a second chat in the same project overwrite the
        // first one's project-level copy the moment it was typed in — and then,
        // when the FIRST chat sent its message, clear the second one's. Only the
        // owner (or nobody) may touch it.
        var owner = CLAIMED[DRAFT_PROJ_KEY];
        if (owner && owner !== sessionId) return;
        if (input.value) {
          writeDraft(DRAFT_PROJ_KEY, input.value);
          claimedProject = true;
          CLAIMED[DRAFT_PROJ_KEY] = sessionId;
        } else if (claimedProject) {
          writeDraft(DRAFT_PROJ_KEY, '');
          claimedProject = false;
          delete CLAIMED[DRAFT_PROJ_KEY];
        }
      } catch (e) {}
    }
    try {
      pruneDrafts();
      var draft = readDraft(DRAFT_KEY);
      if (!draft && DRAFT_PROJ_KEY && !CLAIMED[DRAFT_PROJ_KEY]) {
        draft = readDraft(DRAFT_PROJ_KEY);   // stranded by a restart — adopt it
      }
      if (draft) {
        input.value = draft;
        setTimeout(autoGrow, 0);
        if (DRAFT_PROJ_KEY) { claimedProject = true; CLAIMED[DRAFT_PROJ_KEY] = sessionId; }
        saveDraft();   // re-anchor under this session's id
      }
    } catch (e) {}

    input.addEventListener('keydown', onKeyDown);
    input.addEventListener('input', function () {
      slashIdx = 0; autoGrow(); showSlash(); saveDraft();
    });
    input.addEventListener('blur', function () { setTimeout(hideSlash, 120); });
    sendBtn.addEventListener('click', doSend);
    stopBtn.addEventListener('click', doStop);
    modeSel.addEventListener('change', function () {
      syncModeBtn();
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/permission-mode', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mode: modeSel.value }),
      }).catch(function () {});
    });

    // ─── Event application ───

    // ─── Subagents ───
    //
    // Claude's Task subagents stream their own turns up the same connection.
    // The server routes them out of the transcript (see claude-ui.js) and sends
    // them here instead, so they surface as bubbles floating over the log —
    // present and inspectable, but never mistaken for the conversation itself.

    var agents = new Map();   // toolUseId -> { meta, messages, dismissed }
    var openAgent = null;     // toolUseId of the open transcript, if any
    var agentPanel = null;
    var agentList = null;     // the list sheet, while it is open
    var listKind = '';        // which list that is: 'agent' or 'task'
    var agentsLive = false;   // false while a stored session is being replayed

    // Putting one away has to stick. It lived only in the record before, so a
    // refresh - or reopening the desktop app - rebuilt every one from the
    // replayed session and handed back the exact clutter the X removed.
    // Same {t, ids} shape as the drafts so the TTL sweep above reaps it too.
    function agentRec(toolUseId) {
      var rec = agents.get(toolUseId);
      if (!rec) {
        rec = { meta: { toolUseId: toolUseId, kind: 'agent', description: '', status: 'running' }, messages: [],
                dismissed: isDismissed(toolUseId) };
        agents.set(toolUseId, rec);
      }
      return rec;
    }

    function agentLabel(m) {
      if (m.kind === 'task') return m.description || m.summary || 'Background task';
      return m.description || (m.subagentType ? m.subagentType + ' agent' : 'Agent');
    }

    function agentRunning(m) { return m.status === 'running' || !m.status; }

    // The server tracks two kinds of background work the same way, because the
    // CLI reports them the same way: subagents, and background commands and
    // Monitors ("task"). They are counted and listed apart - an agent has a
    // transcript to read, a command has output - but behave alike.
    function kindOf(m) { return m.kind === 'task' ? 'task' : 'agent'; }

    /** What is on show for one kind: everything of it not yet dismissed. */
    function shownOf(kind) {
      var out = [];
      agents.forEach(function (rec) {
        if (kindOf(rec.meta) === kind && !rec.dismissed) out.push(rec);
      });
      return out;
    }

    // The dock floats over the bottom of the log. Keep the suggestion its last
    // item whatever gets appended, and pad the log by the dock's height so the
    // newest message can always scroll clear of it rather than sit underneath.
    var LOG_PAD = 4;
    function dockLayout() {
      if (sugNode && sugNode.parentNode === agentDock && agentDock.lastChild !== sugNode) agentDock.appendChild(sugNode);
      var shown = !!(sugNode && sugNode.parentNode === agentDock)
        || agentBadge.classList.contains('on') || taskBadge.classList.contains('on');
      var pad = shown ? Math.max(LOG_PAD, agentDock.offsetHeight + 14) : LOG_PAD;
      if (log.style.paddingBottom !== pad + 'px') {
        var stick = stickBottom;
        log.style.paddingBottom = pad + 'px';
        if (stick) { var prev = log.style.scrollBehavior; log.style.scrollBehavior = 'auto'; log.scrollTop = log.scrollHeight; log.style.scrollBehavior = prev; }
      }
    }
    if (window.MutationObserver) { try { new MutationObserver(dockLayout).observe(agentDock, { childList: true }); } catch (e) {} }
    if (window.ResizeObserver) { try { new ResizeObserver(dockLayout).observe(agentDock); } catch (e) {} }

    var SVG_OPEN = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">';
    var KINDS = {
      agent: {
        badge: agentBadge, one: 'agent', many: 'agents', title: 'Agents', back: '‹ Agents',
        icon: SVG_OPEN + '<path d="M12 8V4H8"/><rect width="16" height="12" x="4" y="8" rx="2"/><path d="M2 14h2"/><path d="M20 14h2"/><path d="M15 13v2"/><path d="M9 13v2"/></svg>',
        foot: 'A finished agent leaves this list after 5 minutes. Its transcript stays on its Agent row in the chat.',
      },
      task: {
        badge: taskBadge, one: 'background command', many: 'background commands', title: 'Commands', back: '‹ Commands',
        icon: SVG_OPEN + '<polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg>',
        foot: 'A finished command leaves this list after 5 minutes. Its output stays on its row in the chat. Dismissing one does not stop it.',
      },
    };

    /** Play a one-shot animation on a kind's badge: "add" or "done". */
    function badgeEvent(kind, what) {
      if (!agentsLive) return;   // a replayed session is not news
      // A sheet is open over the badges: the ring would glow out from under its edge.
      if (agentList || agentPanel) return;
      var badge = KINDS[kind].badge;
      var cls = 'ev-' + what;
      badge.classList.remove('ev-add', 'ev-done');
      // Reading a layout property between the two is what restarts an
      // animation that is already running.
      void badge.offsetWidth;
      badge.classList.add(cls);
      clearTimeout(badge._evTimer);
      badge._evTimer = setTimeout(function () { badge.classList.remove(cls); }, 1000);
    }

    function syncBadge() {
      var changed = false;
      ['task', 'agent'].forEach(function (kind) {
        var k = KINDS[kind], badge = k.badge;
        var shown = shownOf(kind);
        var running = shown.filter(function (r) { return agentRunning(r.meta); }).length;
        var open = !!agentList && listKind === kind;
        if (badge.classList.contains('on') !== shown.length > 0) changed = true;
        badge.classList.toggle('on', shown.length > 0);
        badge.classList.toggle('busy', running > 0);
        badge.classList.toggle('open', open);
        if (!shown.length) return;
        badge.innerHTML = k.icon + '<span class="cc-agbadge-n">' + shown.length + '</span>'
          + (running ? '<span class="cc-spin"></span>' : '<span class="cc-dot-ok"></span>');
        var label = shown.length + ' ' + (shown.length === 1 ? k.one : k.many)
          + (running ? ', ' + running + ' running' : ', all finished');
        badge.title = label + ' — click to see ' + (shown.length === 1 ? 'it' : 'them');
        badge.setAttribute('aria-label', label);
        badge.setAttribute('aria-expanded', open ? 'true' : 'false');
      });
      if (changed) dockLayout();
      if (agentList) paintAgentList();
    }

    /**
     * Record a dismissal with the CONVERSATION, not just this browser.
     *
     * localStorage still runs alongside it for instant feedback and for hosts
     * where the call fails, but the server copy is what makes a dismissal stick
     * across devices, origins and reinstalls.
     */
    function persistDismissed(ids) {
      if (!ids.length) return;
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sessionId) + '/dismiss-agents', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ toolUseIds: ids }),
      }).catch(function () { /* localStorage still covers this browser */ });
    }

    // Dismissing takes one off its badge; it does not stop it - that is the
    // CLI's business. Its row in the transcript stays, so the transcript or the
    // output is still reachable afterwards. For a command it also stops that
    // command holding the chat "busy", which is what keeps an idle chat open.
    function dismissAgent(toolUseId) {
      var rec = agents.get(toolUseId);
      if (!rec || rec.dismissed) return;
      rec.dismissed = true;
      markDismissed(toolUseId);
      persistDismissed([toolUseId]);
      syncBadge();
    }

    /** Everything in the open list, and only that: the other kind is untouched. */
    function dismissAllShown(kind) {
      var ids = shownOf(kind).map(function (rec) {
        rec.dismissed = true;
        markDismissed(rec.meta.toolUseId);
        return rec.meta.toolUseId;
      });
      persistDismissed(ids);
      closeAgentList();
      syncBadge();
    }

    function applyAgent(meta) {
      if (!meta || !meta.toolUseId) return;
      var known = agents.has(meta.toolUseId);
      var rec = agentRec(meta.toolUseId);
      var wasShown = known && !rec.dismissed;
      var wasKind = kindOf(rec.meta);
      var wasRunning = agentRunning(rec.meta);
      // `count` is the server's tally; our own messages array is authoritative.
      var count = rec.meta.count;
      rec.meta = meta;
      // The server remembers dismissals, and makes its own: a finished agent or
      // command is dismissed there five minutes after it ended, and that
      // arrives here as this flag. localStorage alone was per-origin and
      // per-device, so the same conversation on a phone brought them all back.
      if (meta.dismissed) rec.dismissed = true;
      if (meta.count == null) rec.meta.count = count;
      var kind = kindOf(meta);
      var isShown = !rec.dismissed;
      syncBadge();
      if (isShown && (!wasShown || wasKind !== kind)) badgeEvent(kind, 'add');
      else if (isShown && wasRunning && !agentRunning(meta)) badgeEvent(kind, 'done');
      // The tool row that spawned this agent was painted before we knew it was
      // one; repaint it now so it picks up the drill-in.
      entries.forEach(function (r) {
        if (r.data.kind === 'tool' && r.data.toolUseId === meta.toolUseId) paint(r.node, r.data);
      });
      if (openAgent === meta.toolUseId) paintAgentPanel();
    }

    function addAgentEntry(toolUseId, entry) {
      var rec = agentRec(toolUseId);
      rec.messages.push(entry);
      if (openAgent === toolUseId) appendAgentEntry(entry);
    }

    function patchAgentEntry(toolUseId, id, patch) {
      var rec = agents.get(toolUseId);
      if (!rec) return;
      for (var i = 0; i < rec.messages.length; i++) {
        if (rec.messages[i].id === id) {
          Object.assign(rec.messages[i], patch);
          if (openAgent === toolUseId && agentPanel) {
            var n = agentPanel.querySelector('[data-agid="' + id + '"]');
            if (n) paint(n, rec.messages[i]);
          }
          return;
        }
      }
    }

    function appendAgentEntry(entry) {
      if (!agentPanel) return;
      var body = agentPanel.querySelector('.cc-agpanel-body');
      if (!body) return;
      var empty = body.querySelector('.cc-agpanel-empty');
      if (empty) empty.remove();
      var stick = body.scrollTop + body.clientHeight >= body.scrollHeight - 40;
      var node = el('div', 'cc-entry');
      node.dataset.agid = entry.id;
      paint(node, entry);
      body.appendChild(node);
      if (stick) body.scrollTop = body.scrollHeight;
    }

    function paintAgentPanel() {
      if (!agentPanel || !openAgent) return;
      var rec = agents.get(openAgent);
      if (!rec) return;
      var m = rec.meta;
      var running = m.status === 'running' || !m.status;
      var head = agentPanel.querySelector('.cc-agpanel-title');
      var sub = agentPanel.querySelector('.cc-agpanel-sub');
      if (head) head.textContent = agentLabel(m);
      if (sub) {
        var bits = [];
        if (m.subagentType) bits.push(m.subagentType);
        if (running && m.step) bits.push(m.step);
        else bits.push(m.status || 'done');
        if (m.usage && m.usage.tool_uses) bits.push(m.usage.tool_uses + ' tools');
        if (m.usage && m.usage.total_tokens) bits.push(Math.round(m.usage.total_tokens / 1000) + 'k tok');
        sub.textContent = bits.join(' · ');
      }
    }

    // The chat row for a tool call, by the CLI's tool_use id. A background
    // task's output lives there, not on the task itself.
    function findToolEntry(toolUseId) {
      var found = null;
      entries.forEach(function (r) {
        if (r && r.data && r.data.kind === 'tool' && r.data.toolUseId === toolUseId) found = r.data;
      });
      return found;
    }

    function taskToolNode(e) {
      var n = toolNode(e);
      n.classList.remove('cc-collapsed');
      return n;
    }

    // fromList: opened from the agents list, so offer the way back to it.
    function toggleAgentPanel(toolUseId, fromList) {
      if (openAgent === toolUseId) { closeAgentPanel(); return; }
      closeAgentPanel();
      closeAgentList();
      var rec = agents.get(toolUseId);
      if (!rec) return;
      openAgent = toolUseId;
      agentPanel = el('div', 'cc-agpanel');
      var head = el('div', 'cc-agpanel-head');
      if (fromList === true) {
        var backKind = kindOf(rec.meta);
        var back = el('button', 'cc-agpanel-back', KINDS[backKind].back);
        back.type = 'button';
        back.title = 'Back to the list';
        back.addEventListener('click', function () { closeAgentPanel(); openAgentList(backKind); });
        head.appendChild(back);
      }
      head.appendChild(el('span', 'cc-agpanel-title', ''));
      head.appendChild(el('span', 'cc-agpanel-sub', ''));
      var x = el('button', 'cc-agpanel-x', '✕');
      x.addEventListener('click', closeAgentPanel);
      head.appendChild(x);
      var body = el('div', 'cc-agpanel-body');
      if (rec.meta.prompt) body.appendChild(el('div', 'cc-agpanel-prompt', esc(rec.meta.prompt)));
      if (rec.meta.summary) {
        var sum = el('div', 'cc-assistant', md(rec.meta.summary));
        var sw = el('div', 'cc-entry');
        sw.appendChild(sum);
        body.appendChild(sw);
      }
      // A background command is a task, not an agent: it has no transcript,
      // but it does have output, on its tool row in the chat. Show that row
      // here, expanded. The panel used to say only "This task has finished.",
      // which read as if the command had produced nothing at all.
      var taskTool = rec.meta.kind === 'task' ? findToolEntry(toolUseId) : null;
      if (taskTool) {
        var tw = el('div', 'cc-entry cc-agpanel-tasktool');
        tw.appendChild(taskToolNode(taskTool));
        body.appendChild(tw);
      }
      if (!rec.messages.length && !taskTool) {
        body.appendChild(el('div', 'cc-agpanel-empty',
          rec.meta.kind === 'task'
            ? (rec.meta.status && rec.meta.status !== 'running'
                ? 'This task has finished.'
                : 'Running in the background — it reports when something happens.')
            : (rec.meta.status && rec.meta.status !== 'running'
                ? 'This agent’s transcript was not kept.'
                : 'Waiting for the agent’s first step…')));
      }
      agentPanel.appendChild(head);
      agentPanel.appendChild(body);
      logWrap.appendChild(agentPanel);
      rec.messages.forEach(appendAgentEntry);
      paintAgentPanel();
      body.scrollTop = body.scrollHeight;
    }

    function closeAgentPanel() {
      if (agentPanel) agentPanel.remove();
      agentPanel = null;
      openAgent = null;
    }

    // ─── The list a badge opens ───
    //
    // Everything of one kind that is on show, running ones first. A row opens
    // that agent's transcript, or that command's output; its X dismisses it.

    function agentRowSub(m) {
      var bits = [];
      if (m.subagentType) bits.push(m.subagentType);
      if (agentRunning(m)) bits.push(m.step || 'running');
      else bits.push(m.status === 'completed' || !m.status ? 'finished' : m.status);
      var tok = m.usage && m.usage.total_tokens;
      if (tok) bits.push((tok >= 1000 ? Math.round(tok / 1000) + 'k' : String(tok)) + ' tok');
      return bits.join(' · ');
    }

    function paintAgentList() {
      if (!agentList) return;
      var shown = shownOf(listKind);
      if (!shown.length) { closeAgentList(); return; }
      // Running first, then the most recently finished; ties keep their order.
      shown = shown.map(function (r, i) { return { r: r, i: i }; }).sort(function (a, b) {
        var ra = agentRunning(a.r.meta), rb = agentRunning(b.r.meta);
        if (ra !== rb) return ra ? -1 : 1;
        if (!ra) { var d = (b.r.meta.endedAt || 0) - (a.r.meta.endedAt || 0); if (d) return d; }
        return a.i - b.i;
      }).map(function (x) { return x.r; });
      var running = shown.filter(function (r) { return agentRunning(r.meta); }).length;
      var sub = agentList.querySelector('.cc-agpanel-sub');
      if (sub) sub.textContent = (running ? running + ' running' : '') + (running && shown.length - running ? ' · ' : '')
        + (shown.length - running ? (shown.length - running) + ' finished' : '');
      var body = agentList.querySelector('.cc-aglist-body');
      var top = body.scrollTop;
      body.innerHTML = shown.map(function (rec) {
        var m = rec.meta, run = agentRunning(m);
        return '<div class="cc-agrow' + (run ? '' : ' done') + '" role="button" tabindex="0" data-id="' + esc(m.toolUseId) + '">'
          + '<span class="cc-agrow-st">' + (run ? '<span class="cc-spin"></span>'
              : '<span class="' + (m.status === 'failed' ? 'cc-dot-err' : 'cc-dot-ok') + '"></span>') + '</span>'
          + '<span class="cc-agrow-tx"><span class="cc-agrow-t">' + esc(agentLabel(m)) + '</span>'
          + '<span class="cc-agrow-s">' + esc(agentRowSub(m)) + '</span></span>'
          + '<span class="cc-agrow-go" aria-hidden="true">›</span>'
          + '<button type="button" class="cc-agrow-x" title="Dismiss" aria-label="Dismiss ' + esc(agentLabel(m)) + '">✕</button>'
          + '</div>';
      }).join('');
      body.scrollTop = top;
    }

    function openAgentList(kind) {
      kind = kind === 'task' ? 'task' : 'agent';
      if (agentList && listKind === kind) return;
      if (!shownOf(kind).length) return;
      closeAgentPanel();
      closeAgentList();
      listKind = kind;
      var k = KINDS[kind];
      agentList = el('div', 'cc-agpanel cc-aglist');
      agentList.setAttribute('role', 'dialog');
      agentList.setAttribute('aria-label', k.title);
      var head = el('div', 'cc-agpanel-head');
      head.appendChild(el('span', 'cc-agpanel-title', k.title));
      head.appendChild(el('span', 'cc-agpanel-sub', ''));
      var all = el('button', 'cc-aglist-acts', 'Dismiss all');
      all.type = 'button';
      all.addEventListener('click', function () { dismissAllShown(kind); });
      head.appendChild(all);
      var x = el('button', 'cc-agpanel-x', '✕');
      x.type = 'button';
      x.title = 'Close';
      x.addEventListener('click', closeAgentList);
      head.appendChild(x);
      var body = el('div', 'cc-aglist-body');
      function act(ev) {
        var row = ev.target.closest('.cc-agrow');
        if (!row) return;
        if (ev.target.closest('.cc-agrow-x')) { ev.stopPropagation(); dismissAgent(row.dataset.id); return; }
        toggleAgentPanel(row.dataset.id, true);
      }
      body.addEventListener('click', act);
      body.addEventListener('keydown', function (ev) {
        if ((ev.key === 'Enter' || ev.key === ' ') && ev.target.classList.contains('cc-agrow')) { ev.preventDefault(); act(ev); }
      });
      agentList.addEventListener('keydown', function (ev) { if (ev.key === 'Escape') { ev.stopPropagation(); closeAgentList(); k.badge.focus(); } });
      agentList.appendChild(head);
      agentList.appendChild(body);
      agentList.appendChild(el('div', 'cc-aglist-foot', k.foot));
      logWrap.appendChild(agentList);
      paintAgentList();
      syncBadge();
      var first = body.querySelector('.cc-agrow');
      if (first) { try { first.focus({ preventScroll: true }); } catch (e) { first.focus(); } }
    }

    function closeAgentList() {
      if (!agentList) return;
      agentList.remove();
      agentList = null;
      listKind = '';
      [agentBadge, taskBadge].forEach(function (b) {
        b.classList.remove('open');
        b.setAttribute('aria-expanded', 'false');
      });
    }

    function toggleAgentList(kind) {
      if (agentList && listKind === kind) { closeAgentList(); return; }
      openAgentList(kind);
    }

    function resetAgents() {
      closeAgentPanel();
      closeAgentList();
      agents.clear();
      agentDock.innerHTML = '';
      agentDock.appendChild(badgeRow);   // the wipe above detaches it
      agentBadge.classList.remove('ev-add', 'ev-done');
      taskBadge.classList.remove('ev-add', 'ev-done');
      syncBadge();
    }

    function addEntry(data) {
      if (data.kind === 'thinking') thinkingChanged(data, true);
      var node = renderEntry(data);
      entries.set(data.id, { data: data, node: node });
      var stick = followingBottom();
      log.appendChild(node);
      // The activity row is not a conversation entry; keep it last.
      if (activityNode && activityNode.parentNode === log) log.appendChild(activityNode);
      scrollDown(stick);
    }

    // ─── Scheduled messages ───
    // Opens with whatever is in the composer already copied in, so the common
    // case (write it, then decide it should go later) is one click. Opened
    // empty it is simply the list — the same panel, nothing special-cased.

    var schedPanel = null;

    function trigLabel(it) {
      var t = it.trigger.type;
      if (t === 'time') return new Date(it.trigger.at).toLocaleString();
      if (t === 'turn-end') return 'when the turn ends';
      if (t === 'limit-reset') return 'when the 5h window resets';
      return t;
    }

    function closeSched() {
      if (schedPanel) schedPanel.remove();
      schedPanel = null;
    }

    var schedWarm = true;
    var syncSchedNote = function () {};

    function loadSched() {
      if (!schedPanel || !project) return;
      var listEl = schedPanel.querySelector('.cc-sched-list');
      apiFetch('/api/chat-schedule?project=' + encodeURIComponent(project))
        .then(function (r) { return r.json(); })
        .then(function (d) {
          if (!schedPanel || !listEl) return;
          // The limit-reset trigger rides on the limit warmer, so it is shown
          // but disabled when that is off — hiding it would just make the
          // feature look like it does not exist.
          var warm = !!(d && d.limitWarmup);
          var rb = schedPanel.querySelector('.cc-sched-trig[data-trig="limit-reset"]');
          if (rb) {
            rb.disabled = !warm;
            rb.classList.toggle('off', !warm);
            rb.title = warm ? '' : 'Needs the limit warmer, which is off';
            // If it was already selected and the warmer has since been turned
            // off, fall back rather than leaving a dead selection armed.
            if (!warm && rb.classList.contains('on')) {
              var tb = schedPanel.querySelector('.cc-sched-trig[data-trig="time"]');
              if (tb) tb.click();
            }
          }
          schedWarm = warm;
          syncSchedNote();
          var items = (d && d.items) || [];
          if (!items.length) {
            listEl.innerHTML = '<div class="cc-sched-empty">Nothing scheduled for this project.</div>';
            return;
          }
          // Server sorts by next firing; the first row is what goes next.
          listEl.innerHTML = items.map(function (it, i) {
            return '<div class="cc-sched-item' + (i === 0 ? ' next' : '') + '" data-sid="' + esc(it.id) + '">'
              + '<div class="cc-sched-when">' + (i === 0 ? '<b>next</b> · ' : '') + esc(trigLabel(it)) + '</div>'
              + '<div class="cc-sched-text">' + esc(it.text) + '</div>'
              + '<button class="cc-sched-del" title="Cancel this message">✕</button>'
              + '</div>';
          }).join('');
          listEl.querySelectorAll('.cc-sched-del').forEach(function (b) {
            b.addEventListener('click', function () {
              var id = b.closest('.cc-sched-item').dataset.sid;
              apiFetch('/api/chat-schedule/' + encodeURIComponent(id), { method: 'DELETE' })
                .then(function () { loadSched(); })
                .catch(function () {});
            });
          });
        })
        .catch(function () {
          if (listEl) listEl.innerHTML = '<div class="cc-sched-empty">Could not load scheduled messages.</div>';
        });
    }

    function openSched() {
      if (schedPanel) { closeSched(); return; }
      schedPanel = el('div', 'cc-schedpanel');
      // Default the time field to a round-ish moment shortly ahead, so the
      // common "later today" case needs one edit, not five.
      var soon = new Date(Date.now() + 30 * 60000);
      soon.setSeconds(0, 0);
      var localIso = new Date(soon.getTime() - soon.getTimezoneOffset() * 60000).toISOString().slice(0, 16);

      schedPanel.innerHTML =
        '<div class="cc-sched-head"><span class="cc-sched-title">Schedule a message</span>'
        + '<button class="cc-sched-x">✕</button></div>'
        + '<div class="cc-sched-body">'
        + '<textarea class="cc-sched-msg" rows="3" placeholder="Message to send later…"></textarea>'
        + '<div class="cc-sched-trigs">'
        + '<button class="cc-sched-trig on" data-trig="time">At a time</button>'
        + '<button class="cc-sched-trig" data-trig="turn-end">Turn ends</button>'
        + '<button class="cc-sched-trig" data-trig="limit-reset">Limit resets</button>'
        + '</div>'
        + '<input type="datetime-local" class="cc-sched-at" value="' + localIso + '">'
        + '<div class="cc-sched-note"></div>'
        + '<button class="cc-btn primary cc-sched-save">Schedule</button>'
        + '<div class="cc-sched-list"></div>'
        + '</div>';
      root.appendChild(schedPanel);

      var msg = schedPanel.querySelector('.cc-sched-msg');
      var atEl = schedPanel.querySelector('.cc-sched-at');
      var note = schedPanel.querySelector('.cc-sched-note');
      var trig = 'time';

      // Carry the composer across. Empty is fine — the panel is then just the list.
      msg.value = input.value;

      function syncTrig() {
        atEl.style.display = trig === 'time' ? '' : 'none';
        note.textContent = trig === 'turn-end'
          ? 'Sends as soon as the current turn finishes. If nothing is running, it waits for the next turn to end.'
          : trig === 'limit-reset'
            ? 'Sends when the rolling 5-hour usage window rolls over.'
            : '';
      }
      syncSchedNote = syncTrig;
      syncTrig();

      schedPanel.querySelectorAll('.cc-sched-trig').forEach(function (b) {
        b.addEventListener('click', function () {
          if (b.disabled) {
            note.textContent = 'Turn on the limit warmer in Settings to use this — it is what notices the window rolling over.';
            return;
          }
          trig = b.dataset.trig;
          schedPanel.querySelectorAll('.cc-sched-trig').forEach(function (x) {
            x.classList.toggle('on', x === b);
          });
          syncTrig();
        });
      });
      schedPanel.querySelector('.cc-sched-x').addEventListener('click', closeSched);

      schedPanel.querySelector('.cc-sched-save').addEventListener('click', function () {
        var text = msg.value.trim();
        if (!text) { note.textContent = 'Write a message first.'; return; }
        var payload = { project: project, text: text, trigger: { type: trig } };
        if (trig === 'time') {
          if (!atEl.value) { note.textContent = 'Pick a time.'; return; }
          payload.trigger.at = new Date(atEl.value).toISOString();
        }
        apiFetch('/api/chat-schedule', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload),
        })
          .then(function (r) { return r.json(); })
          .then(function (d) {
            if (!d.ok) { note.textContent = d.error || 'Could not schedule that.'; return; }
            // It is queued now, so clear the composer it came from — leaving it
            // there invites sending the same thing twice.
            if (input.value.trim() === text) { input.value = ''; saveDraft(); autoGrow(); }
            msg.value = '';
            note.textContent = '';
            loadSched();
          })
          .catch(function (err) { note.textContent = 'Could not schedule that: ' + err.message; });
      });

      loadSched();
      msg.focus();
    }

    schedBtn.addEventListener('click', openSched);
    agentBadge.addEventListener('click', function () { toggleAgentList('agent'); });
    taskBadge.addEventListener('click', function () { toggleAgentList('task'); });
    syncBadge();

    // ─── Goal mode ───
    // The CLI runs the loop inside a single turn, pushed on by a Stop hook, so
    // there is no per-iteration state to drive here — the bar exists to answer
    // "why is it still going?", which is otherwise invisible.
    var goal = null;

    /** What the plan's status means, in the reader's terms rather than ours. */
    function planStatusLabel(st) {
      if (st === 'executing') return 'approved \u2014 being worked through';
      if (st === 'rejected') return 'sent back for revision';
      return 'waiting for your approval';
    }

    function renderPlan() {
      var has = !!(plan && plan.text);
      if (!has) planOpen = false;
      if (has) {
        planSt.textContent = planStatusLabel(plan.status);
        planBody.innerHTML = md(plan.text);
      } else {
        planBody.innerHTML = '';
      }
      if (planOpen) planPanel.classList.add('open');
      else planPanel.classList.remove('open');
    }

    /**
     * Tell the host that something the chat's title bar shows has changed.
     *
     * A plan change moves no agent state, so it triggers no state broadcast --
     * without this the header pill would not appear until something unrelated
     * happened to refresh it.
     */
    function notifyMeta(patch) {
      if (!opts.onChatMeta) return;
      try { opts.onChatMeta(patch); } catch (e) { /* host's problem, not ours */ }
    }

    /** Show/hide the plan. Returns the state it settled on, for the header pill. */
    function togglePlan(force) {
      var want = force == null ? !planOpen : !!force;
      planOpen = want && !!(plan && plan.text);
      renderPlan();
      return planOpen;
    }

    planX.addEventListener('click', function () { togglePlan(false); });

    function renderGoal() {
      if (!goal) { goalBar.style.display = 'none'; goalBar.innerHTML = ''; return; }
      goalBar.style.display = '';
      goalBar.innerHTML = '';
      var head = el('div', 'cc-goal-head');
      head.appendChild(el('span', 'cc-goal-tag', goal.looping ? '<span class="cc-spin"></span>GOAL' : 'GOAL'));
      head.appendChild(el('span', 'cc-goal-cond', esc(goal.condition)));
      var n = el('span', 'cc-goal-n', goal.verdicts
        ? goal.verdicts + (goal.verdicts === 1 ? ' check' : ' checks')
        : (goal.looping ? 'running' : 'set'));
      head.appendChild(n);
      var x = el('button', 'cc-goal-x', 'clear');
      x.title = 'Stop working towards this goal (/goal clear)';
      x.addEventListener('click', clearGoal);
      head.appendChild(x);
      goalBar.appendChild(head);
      if (!goal.looping && goal.verdicts) {
        goalBar.appendChild(el('div', 'cc-goal-idle',
          'The goal loop has stopped. Send /goal to ask for its current verdict.'));
      }
    }

    function clearGoal() {
      // Route it through the normal send path so the CLI sees the real command
      // and the transcript records that the user cleared it.
      if (state !== 'idle') { enqueue('/goal clear'); return; }
      postMessage('/goal clear');
      setState('working');
    }

    // Bring the log in line with a full snapshot WITHOUT rebuilding it, when
    // the snapshot is the same conversation plus/minus a few entries — the
    // normal case on a reconnect (minimised window, phone asleep, network
    // blip). Rebuilding wiped the log, which reset scrollTop to 0, and the
    // smooth scroll back down then swept the whole transcript top to bottom.
    // Returns false when the order cannot be reconciled; the caller rebuilds.
    function reconcileLog(msgs) {
      var idx = {};
      for (var i = 0; i < msgs.length; i++) idx[msgs[i].id] = i;
      var onScreen = [];
      for (var c = 0; c < log.children.length; c++) {
        var cid = log.children[c].dataset && log.children[c].dataset.id;
        if (cid && entries.has(cid)) onScreen.push(cid);
      }
      if (!onScreen.length) return false;
      // Kept entries must already be in the snapshot's order, and nothing new
      // may need to go in between them (only after the last one).
      var last = -1;
      for (var k = 0; k < onScreen.length; k++) {
        if (!(onScreen[k] in idx)) continue;
        if (idx[onScreen[k]] < last) return false;
        last = idx[onScreen[k]];
      }
      for (var j = 0; j <= last; j++) if (!entries.has(msgs[j].id)) return false;
      // Gone from the server's copy (trimmed, or a local-only error line).
      onScreen.forEach(function (id) {
        if (id in idx) return;
        var r = entries.get(id);
        if (r && r.node.parentNode) r.node.parentNode.removeChild(r.node);
        entries.delete(id);
      });
      // Changed while we were away (a tool finished, an answer landed).
      for (var u = 0; u <= last; u++) {
        var rec = entries.get(msgs[u].id);
        if (JSON.stringify(rec.data) !== JSON.stringify(msgs[u])) { rec.data = msgs[u]; paint(rec.node, rec.data); }
      }
      // New since then: appended, which also follows the bottom if pinned.
      for (var n = last + 1; n < msgs.length; n++) addEntry(msgs[n]);
      return true;
    }

    function applyHistory(session) {
      if (destroyed || !session) return;
      var msgs = session.messages || [];
      var firstLoad = !entries.size;
      var wasStuck = stickBottom, keepTop = log.scrollTop;
      var smooth = log.style.scrollBehavior;
      var reconciled = !firstLoad && reconcileLog(msgs);
      if (!reconciled) {
        // A rebuild must not animate: set the position directly afterwards.
        log.style.scrollBehavior = 'auto';
        log.innerHTML = '';
        entries.clear();
        activityNode = null; // detached by the wipe above; setState re-creates it
        msgs.forEach(addEntry);
      }
      resetAgents();
      // Agents that come back with a stored session are not news: no animation.
      agentsLive = false;
      (session.agents || []).forEach(function (a) {
        if (!a || !a.toolUseId) return;
        var rec = agentRec(a.toolUseId);
        rec.messages = a.messages || [];
        applyAgent(a);
      });
      agentsLive = true;
      goal = session.goal || null;
      renderGoal();
      plan = session.plan || null;
      renderPlan();
      notifyMeta({ plan: plan, permissionMode: session.permissionMode || 'default' });
      // A reload mid-turn must not lose sight of a message the CLI has been
      // handed but not yet taken — otherwise it is invisible until it lands.
      handedOver = (session.pendingInjections || []).map(function (p) {
        return { text: p.text, uuid: p.uuid || null, state: p.state || 'sent' };
      });
      renderQueue();
      slashCommands = session.slashCommands || [];
      buildModes(session.skipPermissions);
      modeSel.value = session.permissionMode || 'default';
      syncModeBtn();
      modelLbl.textContent = session.model || '';
      setSessionId(session.sessionId);
      if (session.totalCostUsd) costLbl.textContent = '$' + session.totalCostUsd.toFixed(4);
      suggestion = session.suggestion || null;
      setState(session.state || 'idle');
      input.disabled = session.status !== 'running';
      renderSuggestion();
      if (session.status !== 'running') {
        input.placeholder = 'Session ended.';
        stateLbl.textContent = 'exited';
      }
      if (firstLoad) {
        // Opening a chat lands on the newest message, instantly.
        stickBottom = true;
        log.scrollTop = log.scrollHeight;
      } else if (!reconciled) {
        // Rebuilt: put the reader back where they were, no sweep.
        stickBottom = wasStuck;
        log.scrollTop = wasStuck ? log.scrollHeight : keepTop;
      } else if (wasStuck) {
        // Reconciled and following the bottom: glide from here to the new end.
        stickBottom = true;
        scrollDown(true);
      }
      log.style.scrollBehavior = smooth;
    }

    function applyEvent(ev) {
      if (destroyed || !ev) return;
      switch (ev.type) {
        case 'entry': addEntry(ev.entry); break;
        case 'patch': {
          var rec = entries.get(ev.id);
          if (!rec) break;
          Object.assign(rec.data, ev.patch);
          if (rec.data.kind === 'thinking') thinkingChanged(rec.data, false);
          var stick = followingBottom();
          paint(rec.node, rec.data);
          scrollDown(stick);
          // An open task panel shows this same command: keep it current, so a
          // command still running when the bubble was opened fills in its
          // output as it arrives.
          if (agentPanel && openAgent && rec.data.kind === 'tool' && rec.data.toolUseId === openAgent) {
            var tt = agentPanel.querySelector('.cc-agpanel-tasktool');
            if (tt) { tt.innerHTML = ''; tt.appendChild(taskToolNode(rec.data)); }
          }
          break;
        }
        case 'delta': {
          var r = entries.get(ev.id);
          if (!r) break;
          r.data.text = (r.data.text || '') + ev.text;
          // Repaint just the text node; markdown is cheap enough per delta and
          // keeps fences/lists correct as they stream in.
          var stick2 = followingBottom();
          paint(r.node, r.data);
          // A long thought scrolls inside its own box: keep its newest line in view.
          var tb = r.data.kind === 'thinking' && r.node.querySelector('.cc-think-body');
          if (tb) tb.scrollTop = tb.scrollHeight;
          scrollDown(stick2);
          break;
        }
        // Server spliced older messages in front of the live ones (a resumed
        // conversation whose stored transcript was replayed) — take the whole
        // snapshot rather than trying to merge.
        case 'history': applyHistory(ev.session); break;
        case 'goal': goal = ev.goal; renderGoal(); break;
        // The CLI has taken a message we handed over. It is now in the
        // conversation proper, so the drawer's job is done. `assumed` means
        // the turn ended without an acknowledgement rather than with one;
        // either way it is no longer in flight.
        // The CLI's own account of a handed-over message: 'queued' opens the
        // recall window, 'started' shuts it, terminal states end it.
        case 'queued-state': {
          var slot = null;
          for (var qi = 0; qi < handedOver.length; qi++) {
            if (handedOver[qi].uuid === ev.uuid) { slot = handedOver[qi]; break; }
          }
          // First sighting: match by text, oldest first, and adopt the uuid.
          if (!slot) {
            for (var qj = 0; qj < handedOver.length; qj++) {
              if (!handedOver[qj].uuid && handedOver[qj].text === ev.text) { slot = handedOver[qj]; slot.uuid = ev.uuid; break; }
            }
          }
          if (!slot) break;
          if (ev.state === 'cancelled' || ev.state === 'discarded' || ev.state === 'refused') {
            handedOver = handedOver.filter(function (h) { return h !== slot; });
          } else { slot.state = ev.state; }
          renderQueue();
          break;
        }
        case 'injected': {
          // Clear the one that landed, by its text; the oldest only as a fallback.
          if (!handedOver.length) break;
          var hit = -1;
          for (var hi = 0; hi < handedOver.length; hi++) { if (handedOver[hi].text === ev.text) { hit = hi; break; } }
          handedOver.splice(hit < 0 ? 0 : hit, 1);
          renderQueue();
          break;
        }
        case 'agent': applyAgent(ev.agent); break;
        case 'agent-entry': addAgentEntry(ev.toolUseId, ev.entry); break;
        case 'agent-patch': patchAgentEntry(ev.toolUseId, ev.id, ev.patch); break;
        case 'state': setState(ev.state); break;
        case 'suggestion': suggestion = ev.suggestion || null; renderSuggestion(); break;
        case 'init':
          slashCommands = ev.slashCommands || [];
          if (ev.model) modelLbl.textContent = ev.model;
          if (ev.permissionMode) setModeIfKnown(ev.permissionMode);
          // Fires twice: once when the process is ready (no session id yet) and
          // again on the first turn, which is when the CLI reveals its id.
          setSessionId(ev.sessionId);
          break;
        case 'meta':
          // Assigning a value with no matching <option> silently sets
          // selectedIndex to -1 and the picker renders EMPTY. The CLI can
          // report a mode this session's list does not carry (e.g. a
          // bypassPermissions default on a session not launched for it).
          if (ev.permissionMode) { setModeIfKnown(ev.permissionMode); notifyMeta({ permissionMode: ev.permissionMode }); }
          if (ev.model) modelLbl.textContent = ev.model;
          // Only plan-change metas carry the key at all, so an absent `plan`
          // must not be read as "the plan went away".
          if ('plan' in ev) { plan = ev.plan || null; renderPlan(); notifyMeta({ plan: plan }); }
          break;
        case 'exit':
          // Nothing will flush the queue now, so hand the text back rather than
          // silently dropping what the user typed.
          if (queued.length) unqueue();
          // Anything already handed over is beyond recall and will never be
          // acknowledged now, so stop implying it is still on its way.
          if (handedOver.length) { handedOver = []; renderQueue(); }
          setState('idle');
          if (activityNode) { activityNode.remove(); activityNode = null; }
          input.disabled = true;
          input.placeholder = 'Session ended.';
          stateLbl.textContent = 'exited';
          suggestion = null;
          renderSuggestion();
          break;
      }
    }

    // Ask the server to stream this session (also replays history).
    wsSend({ type: 'subscribe-ui', id: sessionId });

    return {
      applyHistory: applyHistory,
      applyEvent: applyEvent,
      togglePlan: togglePlan,
      focus: function () { try { input.focus(); } catch (e) {} },
      resubscribe: function () { wsSend({ type: 'subscribe-ui', id: sessionId }); },
      restoreScroll: restoreScroll,
      // Used by the workbench's pointer-drag (touch) path, which can't rely on
      // HTML5 drag events — see insertRefToTarget in webapp-html.js.
      insertText: function (text) { if (text) insertPath(text); },
      destroy: function () {
        closeModeModal();
        destroyed = true;
        // Let a later chat in this project pick the draft back up.
        if (DRAFT_PROJ_KEY && CLAIMED[DRAFT_PROJ_KEY] === sessionId) delete CLAIMED[DRAFT_PROJ_KEY];
        selDispose();
        stopTicker(); // closing a cell must not leave an interval running
        window.removeEventListener('paste', onPaste);
        input.removeEventListener('paste', onPaste);
        if (root._ccDropDone) { window.removeEventListener('drop', root._ccDropDone, true); window.removeEventListener('dragend', root._ccDropDone, true); }
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
