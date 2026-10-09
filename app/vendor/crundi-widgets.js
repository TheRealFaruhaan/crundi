/* eslint-disable */
/**
 * crundi-widgets.js — the page's side of Claude-authored widgets.
 *
 * A widget is somebody else's HTML and script. It is only ever put on the page
 * inside <iframe sandbox="allow-scripts"> fed by srcdoc: an opaque origin, so
 * it cannot read this page's localStorage (where the session tokens are),
 * cannot call /api, cannot touch this DOM. Everything it gets, it gets from the
 * small postMessage bridge below, and everything it asks for goes through the
 * server, which checks it against the widget's manifest and the owner's grant.
 *
 * NEVER add allow-same-origin to that sandbox, and never write a widget's
 * markup into this document. Both undo the whole arrangement.
 *
 * Exposes window.CrundiWidgets. The host page (webapp-html.js) decides where
 * things go; this file draws them.
 */
(function () {
  'use strict';

  var cfg = { apiFetch: null, toast: function () {}, isMobile: function () { return false; }, liveChats: function () { return []; }, onChange: function () {}, onChrome: function () {} };
  var cellSig = null;        // which panels are workbench panes, as last told to the host
  var project = '';          // alias as the host knows it
  var widgets = [];          // public meta for the current project
  var views = [];            // every mounted frame
  var loadSeq = 0;
  var tabState = { el: null, selected: '', view: null, pane: '' };
  var chipEl = null;
  var chipValues = {};       // id -> text
  var pop = null;            // { id, back, view }

  var SLOT_LABEL = { dock: 'Docked to chat', cell: 'Workbench pane', tab: 'Panels tab only', inline: 'Inline in chat', chip: 'Top bar chip' };
  var MOVABLE = ['dock', 'cell', 'tab', 'chip'];

  function pkey(alias) { return String(alias || '').toLowerCase().replace(/[^a-z0-9._-]/g, '_') || '_global'; }
  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]; }); }
  function el(tag, cls, html) { var n = document.createElement(tag); if (cls) n.className = cls; if (html != null) n.innerHTML = html; return n; }
  function svg(body) { return '<svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + body + '</svg>'; }
  var IC = {
    panel: svg('<rect x="3" y="3" width="7" height="9" rx="1.5"/><rect x="14" y="3" width="7" height="5" rx="1.5"/><rect x="14" y="12" width="7" height="9" rx="1.5"/><rect x="3" y="16" width="7" height="5" rx="1.5"/>'),
    x: svg('<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>'),
    refresh: svg('<polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.5 9a9 9 0 0 1 14.9-3.4L23 10M1 14l4.6 4.4A9 9 0 0 0 20.5 15"/>'),
    up: svg('<polyline points="18 15 12 9 6 15"/>'),
    down: svg('<polyline points="6 9 12 15 18 9"/>'),
    pin: svg('<line x1="12" y1="17" x2="12" y2="22"/><path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24z"/>'),
    shield: svg('<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>'),
    alert: svg('<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>'),
    eye: svg('<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>'),
    history: svg('<polyline points="1 4 1 10 7 10"/><path d="M3.5 15a9 9 0 1 0 2.1-9.4L1 10"/><polyline points="12 7 12 12 15 14"/>'),
    trash: svg('<polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>'),
    globe: svg('<circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>'),
    more: svg('<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>'),
    download: svg('<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>'),
    expand: svg('<polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/>')
  };

  function api(path, body) {
    var opts = body ? { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) } : {};
    return cfg.apiFetch(path, opts).then(function (r) { return r.json(); }).catch(function (e) { return { ok: false, error: e.message || 'Request failed' }; });
  }
  function qs(alias, id, extra) { return '?project=' + encodeURIComponent(alias) + '&id=' + encodeURIComponent(id) + (extra || ''); }

  // ─── Styles (once) ───

  function injectStyles() {
    if (document.getElementById('wg-styles')) return;
    var s = document.createElement('style');
    s.id = 'wg-styles';
    s.textContent = [
      '.wg-view{position:relative;display:flex;flex-direction:column;min-height:0;min-width:0;background:var(--bg-primary)}',
      '.wg-view.fill{height:100%}',
      '.wg-frame{border:0;display:block;width:100%;flex:1 1 auto;min-height:0;background:var(--bg-primary);color-scheme:dark}',
      '.wg-view.auto .wg-frame{flex:0 0 auto}',
      '.wg-banner{flex-shrink:0;font-size:12px;line-height:1.4;padding:8px 10px;border-bottom:1px solid var(--border);background:var(--bg-secondary);color:var(--text-secondary)}',
      '.wg-banner.grant{background:var(--yellow-dim,rgba(245,158,11,.15));color:var(--text-primary)}',
      '.wg-banner.err{background:var(--red-dim);color:var(--red)}',
      '.wg-banner b{color:var(--text-primary);font-weight:650}',
      '.wg-grant-list{margin:6px 0 8px;padding:0;list-style:none;display:flex;flex-direction:column;gap:4px}',
      '.wg-grant-list li{display:flex;flex-direction:column;gap:1px;padding:5px 7px;border-radius:6px;background:rgba(0,0,0,.25)}',
      '.wg-grant-list .w{font-size:10.5px;font-weight:700;letter-spacing:.04em;text-transform:uppercase;color:var(--yellow)}',
      '.wg-grant-list code{font-family:var(--mono);font-size:11.5px;overflow-wrap:anywhere;color:var(--text-primary)}',
      '.wg-btn{display:inline-flex;align-items:center;gap:5px;min-height:28px;padding:4px 10px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:12px;cursor:pointer;white-space:nowrap}',
      '.wg-btn:hover{border-color:var(--accent)}',
      '.wg-btn.primary{background:var(--accent);border-color:var(--accent);color:#fff}',
      '.wg-btn.danger:hover{border-color:var(--red);color:var(--red)}',
      '.wg-row{display:flex;gap:6px;flex-wrap:wrap;align-items:center}',
      '.wg-msg{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:6px;padding:28px 16px;text-align:center;color:var(--text-secondary);font-size:13px;height:100%}',
      '.wg-msg .ic{font-size:26px;opacity:.5}',
      '.wg-select{min-height:24px;padding:2px 4px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-primary);color:var(--text-secondary);font-size:11px;max-width:132px}',
      // cell
      '.wg-cell-body{padding:0!important;overflow:hidden}',
      '.wg-cell-body>.wg-view{position:absolute;inset:0}',
      '.term-cell[data-wgid] .term-title{min-width:64px;flex:0 1 auto}',
      '.term-cell[data-wgid] .wg-select{max-width:112px;flex:0 1 auto;min-width:0}',
      '.wg-badge{font-size:10px;font-weight:700;letter-spacing:.04em;text-transform:uppercase;padding:2px 6px;border-radius:99px;border:1px solid var(--border);color:var(--text-secondary);white-space:nowrap}',
      '.wg-badge.warn{border-color:transparent;background:var(--yellow-dim,rgba(245,158,11,.15));color:var(--yellow)}',
      '.wg-badge.err{border-color:transparent;background:var(--red-dim);color:var(--red)}',
      // dock
      '.term-cell[data-chat] .term-body.wg-has-dock{display:flex;flex-direction:column}',
      '.term-cell[data-chat] .term-body.wg-has-dock>.chat-mount{flex:1 1 0;height:auto;min-height:0;min-width:0}',
      '.term-cell[data-chat] .term-body.wg-has-dock.wg-side{flex-direction:row}',
      '.wg-dock{flex:0 0 auto;display:flex;flex-direction:column;min-height:0;max-height:46%;overflow:auto;border-bottom:1px solid var(--border);background:var(--bg-primary)}',
      '.wg-dock:empty{display:none}',
      '.wg-side>.wg-dock{order:2;flex:0 0 min(400px,42%);max-height:none;border-bottom:0;border-left:1px solid var(--border)}',
      '.wg-dock-item{display:flex;flex-direction:column;min-height:0;flex:0 0 auto}',
      '.wg-side>.wg-dock>.wg-dock-item:only-child{flex:1 1 auto}',
      '.wg-side>.wg-dock>.wg-dock-item:only-child>.wg-dock-body,.wg-side>.wg-dock>.wg-dock-item:only-child>.wg-dock-body>.wg-view{height:100%;flex:1 1 auto}',
      '.wg-dock-head{display:flex;align-items:center;gap:6px;padding:3px 6px 3px 8px;background:var(--bg-secondary);border-bottom:1px solid var(--border-subtle);flex-shrink:0;min-height:28px}',
      '.wg-dock-item.collapsed>.wg-dock-head{border-bottom:0}',
      // The dock header folds like a pane header: tools inline when wide, a
      // second row behind a menu button when narrow or on a phone.
      '.wg-dock-head{container-type:inline-size;flex-wrap:wrap}',
      '.wg-dock-tools{display:contents}',
      '.wg-ibtn.wg-dock-more{display:none}',
      '@container (max-width:420px){.wg-dock-tools{display:none;order:9;flex:0 0 100%;justify-content:flex-end;align-items:center;gap:6px;padding:4px 0 2px}.wg-dock-head.open .wg-dock-tools{display:flex}.wg-ibtn.wg-dock-more{display:inline-flex}.wg-dock-head.open .wg-dock-more{color:var(--accent-hover)}}',
      '@media (max-width:768px){.wg-dock-tools{display:none;order:9;flex:0 0 100%;justify-content:flex-end;align-items:center;gap:6px;padding:4px 0 2px}.wg-dock-head.open .wg-dock-tools{display:flex}.wg-ibtn.wg-dock-more{display:inline-flex}.wg-dock-head.open .wg-dock-more{color:var(--accent-hover)}}',
      '.wg-dock-item.collapsed>.wg-dock-body{display:none}',
      '.wg-dock-title{font-size:11px;font-weight:700;letter-spacing:.05em;text-transform:uppercase;color:var(--text-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1 1 auto;min-width:0;display:flex;align-items:center;gap:6px;cursor:pointer}',
      '.wg-dock-title .ic{color:var(--accent-hover);font-size:13px}',
      '.wg-ibtn{border:1px solid transparent;background:transparent;color:var(--text-secondary);cursor:pointer;border-radius:var(--radius-sm);font-size:12px;line-height:1;padding:5px 6px;flex-shrink:0;display:inline-flex}',
      '.wg-ibtn:hover{color:var(--text-primary);border-color:var(--border);background:var(--bg-tertiary)}',
      '.wg-ibtn.on{color:var(--accent-hover)}',
      // inline
      '.wg-inline{margin:6px 0 2px;border:1px solid var(--border);border-radius:var(--radius);overflow:hidden;background:var(--bg-card,var(--bg-secondary))}',
      '.wg-inline-head{display:flex;align-items:center;gap:6px;padding:5px 6px 5px 10px;font-size:12px;font-weight:600;color:var(--text-secondary);border-bottom:1px solid var(--border-subtle)}',
      '.wg-inline-head .t{flex:1 1 auto;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
      '.wg-shots{display:flex;gap:8px;overflow-x:auto;padding:8px 2px 4px}',
      '.wg-shot{flex:0 0 auto;display:flex;flex-direction:column;gap:4px;font-size:10.5px;color:var(--text-muted);font-family:var(--mono)}',
      '.wg-shot img{display:block;height:150px;width:auto;border:1px solid var(--border);border-radius:6px;background:var(--bg-primary);cursor:zoom-in}',
      // media shown in a chat
      '.wg-media{margin:6px 0 2px}',
      '.wg-media-row{display:flex;gap:8px;overflow-x:auto;padding-bottom:4px;align-items:flex-start}',
      '.wg-media.files .wg-media-row,.wg-media.audio .wg-media-row{flex-direction:column;overflow:visible}',
      '.wg-mitem{margin:0;flex:0 0 auto;display:flex;flex-direction:column;align-items:flex-start;gap:4px;max-width:100%;width:min-content;min-width:min(150px,100%)}',
      // The box has its shape from the start (aspect-ratio and width are set
      // inline from the item's size); the content fills it when it arrives.
      '.wg-mbox{position:relative;max-width:100%;border:1px solid var(--border);border-radius:8px;background:var(--bg-secondary);overflow:hidden;flex:0 0 auto}',
      '.wg-mimg img{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;display:block;opacity:0;transition:opacity .15s ease;cursor:zoom-in}',
      '.wg-mimg.ready img{opacity:1}',
      '.wg-mimg.gone{display:flex;align-items:center;justify-content:center;padding:10px;font-size:11.5px;color:var(--text-muted);text-align:center;border-style:dashed}',
      '.wg-mvid video{position:absolute;inset:0;width:100%;height:100%;display:block;background:#000;cursor:pointer}',
      '.wg-maud{width:min(100%,440px);border-radius:999px;background:var(--bg-card,var(--bg-secondary))}',
      '.wg-maud audio{display:none}',
      // The player's own controls: one bar, under a video's picture or as the whole of an audio clip.
      '.wg-pl-bar{display:flex;align-items:center;gap:6px;padding:5px 8px;color:var(--text-primary)}',
      '.wg-mvid .wg-pl-bar{position:absolute;left:0;right:0;bottom:0;padding:22px 8px 6px;background:linear-gradient(to top,rgba(0,0,0,.78),rgba(0,0,0,0));color:#fff;transition:opacity .2s ease}',
      '.wg-mvid.idle .wg-pl-bar{opacity:0;pointer-events:none}.wg-mvid.idle{cursor:none}',
      '.wg-pl-btn{appearance:none;border:0;background:transparent;color:inherit;width:32px;height:32px;flex:0 0 auto;border-radius:999px;display:inline-flex;align-items:center;justify-content:center;cursor:pointer;padding:0}',
      '.wg-pl-btn:hover{background:rgba(255,255,255,.12)}.wg-pl-btn .ic{width:16px;height:16px}',
      '.wg-maud .wg-pl-btn.play{background:var(--accent);color:#fff}.wg-maud .wg-pl-btn.play:hover{filter:brightness(1.1)}',
      '.wg-pl-btn.big{position:absolute;left:50%;top:50%;width:56px;height:56px;margin:-28px 0 0 -28px;background:rgba(0,0,0,.55);color:#fff;border:1px solid rgba(255,255,255,.25);backdrop-filter:blur(4px);transition:opacity .15s ease,transform .15s ease}',
      '.wg-pl-btn.big:hover{background:rgba(0,0,0,.7);transform:scale(1.06)}.wg-pl-btn.big .ic{width:24px;height:24px;margin-left:2px}',
      '.wg-pl:not(.paused) .wg-pl-btn.big{opacity:0;pointer-events:none}',
      '.wg-pl-time{font:500 11.5px var(--mono,monospace);font-variant-numeric:tabular-nums;white-space:nowrap;flex:0 0 auto;opacity:.9}',
      '.wg-pl-seek{flex:1 1 auto;min-width:40px;margin:0 8px;height:26px;display:flex;align-items:center;cursor:pointer;touch-action:none;outline:none}',
      '.wg-pl-track{position:relative;width:100%;height:4px;border-radius:999px;background:rgba(255,255,255,.22);transition:height .12s ease}',
      '.wg-maud .wg-pl-track{background:var(--border)}',
      '.wg-pl-seek:hover .wg-pl-track,.wg-pl.seeking .wg-pl-track,.wg-pl-seek:focus-visible .wg-pl-track{height:6px}',
      '.wg-pl-buf,.wg-pl-fill{position:absolute;left:0;top:0;bottom:0;border-radius:999px;width:0}',
      '.wg-pl-buf{background:rgba(255,255,255,.28)}.wg-pl-fill{background:var(--accent)}',
      '.wg-pl-knob{position:absolute;top:50%;left:0;width:12px;height:12px;margin:-6px 0 0 -6px;border-radius:50%;background:#fff;box-shadow:0 1px 3px rgba(0,0,0,.5);transform:scale(0);transition:transform .12s ease}',
      '.wg-pl-seek:hover .wg-pl-knob,.wg-pl.seeking .wg-pl-knob,.wg-pl-seek:focus-visible .wg-pl-knob{transform:scale(1)}',
      '@media (hover:none){.wg-pl-knob{transform:scale(1)}.wg-pl-btn{width:38px;height:38px}}',
      '.wg-pl.busy .wg-pl-fill{animation:wg-pl-busy 1s ease-in-out infinite}@keyframes wg-pl-busy{50%{opacity:.45}}',
      // Loading and buffering: a ring in the middle of a picture or video, and in place of an audio clip's play button.
      '@keyframes wg-spin{to{transform:rotate(360deg)}}',
      '.wg-membed{background:#0f0f14}.wg-membed iframe{position:absolute;inset:0;width:100%;height:100%;border:0;display:block;background:transparent}',
      '.wg-membed.loading::after{content:"";position:absolute;inset:0;margin:auto;box-sizing:border-box;width:34px;height:34px;border-radius:50%;border:3px solid rgba(255,255,255,.18);border-top-color:var(--accent);animation:wg-spin .8s linear infinite;pointer-events:none}',
      '.wg-membed.gone{display:flex;align-items:center;justify-content:center;font-size:11.5px;color:var(--text-muted);border-style:dashed}',
      '.wg-membed-open{color:var(--accent-hover,var(--accent))}',
      '.wg-mimg.loading::after,.wg-mvid.busy::after{content:"";position:absolute;inset:0;margin:auto;box-sizing:border-box;width:34px;height:34px;border-radius:50%;border:3px solid rgba(255,255,255,.18);border-top-color:var(--accent);animation:wg-spin .8s linear infinite;pointer-events:none}',
      '.wg-mvid.busy::after{width:48px;height:48px;border-color:rgba(255,255,255,.3);border-top-color:#fff;filter:drop-shadow(0 1px 3px rgba(0,0,0,.6))}',
      '.wg-mvid.busy .wg-pl-btn.big{opacity:0;pointer-events:none}',
      '.wg-maud .wg-pl-btn.play{position:relative}',
      '.wg-maud.busy .wg-pl-btn.play .ic{opacity:0}',
      '.wg-maud.busy .wg-pl-btn.play::after{content:"";position:absolute;inset:0;margin:auto;box-sizing:border-box;width:20px;height:20px;border-radius:50%;border:2px solid rgba(255,255,255,.35);border-top-color:#fff;animation:wg-spin .8s linear infinite}',
      '.wg-mvid:fullscreen{width:100%!important;height:100%;aspect-ratio:auto!important;border:0;border-radius:0;background:#000}',
      '.wg-mvid:fullscreen video{object-fit:contain}',
      '.wg-mvid.under .wg-pl-bar{background:none;padding-top:6px}',
      '.wg-pl-fit{display:none;align-items:center;justify-content:center;line-height:1;appearance:none;border:1px solid rgba(255,255,255,.35);background:transparent;color:inherit;height:26px;padding:0 9px;border-radius:999px;font:600 11px var(--mono,monospace);flex:0 0 auto;cursor:pointer}',
      '.wg-pl-btn.turn{display:none}',
      '@media (hover:none){.wg-mvid:fullscreen .wg-pl-fit,.wg-mvid:fullscreen .wg-pl-btn.turn{display:inline-flex;align-items:center}}',
      '.wg-media.audio .wg-mitem{width:min(100%,440px)}',
      '.wg-mitem figcaption,.wg-media-cap{font-size:11.5px;color:var(--text-secondary);line-height:1.35;max-width:440px;overflow-wrap:anywhere}',
      '.wg-media-cap{margin-top:4px;max-width:none}',
      '.wg-mfile{display:flex;align-items:center;gap:10px;width:min(100%,460px);padding:9px 10px;border:1px solid var(--border);border-radius:var(--radius);background:var(--bg-card,var(--bg-secondary))}',
      '.wg-mfile-ic{width:34px;height:34px;flex:0 0 auto;display:inline-flex;align-items:center;justify-content:center;border-radius:8px;background:var(--bg-primary);border:1px solid var(--border);font-size:15px;font-family:var(--mono);color:var(--text-secondary)}',
      '.wg-mfile-meta{flex:1 1 auto;min-width:0;display:flex;flex-direction:column;gap:2px}',
      '.wg-mfile-n{font-size:13px;font-weight:600;color:var(--text-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
      '.wg-mfile-s{font-size:11.5px;color:var(--text-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
      '.wg-zoom{position:fixed;inset:0;z-index:1200;background:rgba(0,0,0,.86);display:flex;align-items:center;justify-content:center;padding:16px;cursor:zoom-out;touch-action:none;overflow:hidden;user-select:none;-webkit-user-select:none;overscroll-behavior:contain}',
      '.wg-zoom.zoomed img{cursor:grab}.wg-zoom.zoomed.grabbing img{cursor:grabbing}',
      '.wg-zoom-bar{position:absolute;top:max(10px,env(safe-area-inset-top));right:10px;display:flex;align-items:center;gap:2px;padding:3px;border-radius:999px;background:rgba(20,20,28,.88);border:1px solid var(--border);cursor:default}',
      '.wg-zoom-btn{appearance:none;border:0;background:transparent;color:var(--text-primary);min-width:34px;height:34px;padding:0 8px;border-radius:999px;display:inline-flex;align-items:center;justify-content:center;font:600 12px var(--mono,monospace);cursor:pointer}',
      '.wg-zoom-btn:hover{background:rgba(255,255,255,.1)}.wg-zoom-btn .ic{width:16px;height:16px}.wg-zoom-btn.pct{min-width:52px;color:var(--text-secondary)}',
      '.wg-zoom img{max-width:100%;max-height:100%;border-radius:8px;border:1px solid var(--border);cursor:zoom-in;transform-origin:center center;will-change:transform;-webkit-user-drag:none}',
      // chips
      // In the desktop app the whole top bar is a window-drag area, and a drag
      // area swallows clicks: a chip there could be seen but never pressed.
      // no-drag gives it back to the pointer. A browser ignores the property.
      // The chips share the top bar with the project name and the status
      // badges. They may shrink as a group but must never spill over their
      // neighbours: when there is not room for all of them the row scrolls
      // sideways inside its own box. (It used to be allowed to shrink while
      // its chips were not, so on a phone they were drawn over the badges.)
      '.wg-chips{display:inline-flex;gap:6px;align-items:center;flex:0 1 auto;min-width:0;max-width:100%;overflow-x:auto;overflow-y:hidden;scrollbar-width:none;overscroll-behavior-x:contain;position:relative;z-index:2;-webkit-app-region:no-drag}',
      '.wg-chips::-webkit-scrollbar{display:none}',
      '.wg-chips>.wg-chip{flex:0 0 auto}',
      '.wg-chip{-webkit-app-region:no-drag}',
      '.wg-chip{display:inline-flex;align-items:center;gap:6px;max-width:220px;padding:3px 9px;border-radius:99px;border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:11.5px;cursor:pointer;white-space:nowrap}',
      '.wg-chip:hover{border-color:var(--accent)}',
      '.wg-chip-plain{display:inline-flex;align-items:center;gap:6px;min-width:0}',
      // Its name, for the phone's list, where a face alone would not say whose it is.
      '.wg-chip-name{display:none}',
      // The phone's one button for all chips (see the phone rules below).
      '.wg-chip-toggle{display:none;align-items:center;gap:5px;flex-shrink:0;position:relative;z-index:2;height:28px;padding:0 9px;border-radius:99px;border:1px solid var(--border);background:var(--bg-tertiary);color:var(--accent-hover);font-size:12px;font-weight:650;cursor:pointer;-webkit-app-region:no-drag}',
      '.wg-chip-toggle.on{border-color:var(--accent)}',
      '.wg-chip-toggle[hidden]{display:none!important}',
      '.wg-chip-toggle b{color:var(--text-primary);font-variant-numeric:tabular-nums}',
      // One panel: no count, so nothing may hold a gap open beside the icon.
      '.wg-chip-toggle b:empty{display:none}',
      // A live chip: the panel's own face, in a frame the pointer passes through
      // so the whole chip is still one button.
      '.wg-chip.live{padding:0 9px;height:26px;max-width:none}',
      '.wg-chip.live .wg-chip-plain{display:none}',
      '.wg-chip.live.empty .wg-chip-plain{display:inline-flex}',
      '.wg-chip.live.empty .wg-chip-face{display:none}',
      '.wg-chip-face{display:inline-flex;align-items:center;height:24px}',
      '.wg-chip-face .wg-view{background:transparent;height:24px;flex-direction:row}',
      '.wg-chip-face .wg-banner{display:none}',
      '.wg-chip-face .wg-frame{width:16px;height:24px;flex:0 0 auto;background:transparent;pointer-events:none}',
      '.wg-chip .ic{color:var(--accent-hover)}',
      '.wg-chip .l{color:var(--text-secondary)}',
      '.wg-chip .v{overflow:hidden;text-overflow:ellipsis;font-variant-numeric:tabular-nums}',
      // popover
      '.wg-pop-back{position:fixed;inset:0;z-index:950;background:rgba(0,0,0,.45)}',
      '.wg-pop{position:fixed;z-index:951;top:calc(var(--topbar-height) + 8px);right:12px;width:min(440px,calc(100vw - 24px));height:min(560px,calc(100dvh - var(--topbar-height) - 24px));display:flex;flex-direction:column;background:var(--bg-primary);border:1px solid var(--border);border-radius:var(--radius);box-shadow:var(--shadow-lg);overflow:hidden}',
      '.wg-pop>.wg-view{flex:1 1 auto}',
      '.wg-picker-list{position:absolute;inset:0;overflow:auto;padding:10px;display:flex;flex-direction:column;gap:4px}',
      '.wg-picker-h{font-size:11px;font-weight:700;letter-spacing:.05em;text-transform:uppercase;color:var(--text-secondary);padding:4px 6px 8px}',
      '.wg-picker-list .wg-tab-item .s{display:inline}',
      // tab
      '.wg-tab{display:flex;height:100%;min-height:0}',
      '.wg-tab-list{flex:0 0 232px;border-right:1px solid var(--border);overflow:auto;padding:8px;display:flex;flex-direction:column;gap:4px;background:var(--bg-secondary)}',
      '.wg-tab-item{display:flex;align-items:center;gap:8px;padding:8px 10px;border-radius:var(--radius-sm);border:1px solid transparent;background:transparent;color:var(--text-primary);cursor:pointer;text-align:left;font-size:13px;min-height:40px;width:100%}',
      '.wg-tab-item:hover{background:var(--bg-tertiary)}',
      '.wg-tab-item.sel{background:var(--bg-tertiary);border-color:var(--border)}',
      '.wg-tab-item.closed{color:var(--text-muted)}',
      '.wg-tab-item .n{flex:1 1 auto;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
      '.wg-tab-item .s{font-size:10.5px;color:var(--text-muted);white-space:nowrap}',
      '.wg-tab-item .ic{color:var(--accent-hover);flex-shrink:0}',
      '.wg-tab-main{flex:1 1 auto;min-width:0;display:flex;flex-direction:column;min-height:0}',
      '.wg-tab-bar{display:flex;align-items:center;gap:6px;padding:6px 10px;border-bottom:1px solid var(--border);background:var(--bg-secondary);flex-wrap:wrap}',
      '.wg-tab-bar .t{font-weight:650;font-size:13px;margin-right:auto;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
      '.wg-tab-stage{flex:1 1 auto;min-height:0;position:relative;overflow:auto}',
      '.wg-tab-stage>.wg-view{position:absolute;inset:0}',
      '.wg-tab-side{padding:12px;display:flex;flex-direction:column;gap:10px}',
      '.wg-ver{display:flex;align-items:center;gap:10px;padding:8px 10px;border:1px solid var(--border);border-radius:var(--radius-sm);font-size:12px;color:var(--text-secondary)}',
      '.wg-ver b{color:var(--text-primary)}',
      '@media (max-width:768px){',
      '.wg-tab{flex-direction:column}',
      '.wg-tab-list{flex:0 0 auto;flex-direction:row;border-right:0;border-bottom:1px solid var(--border);overflow-x:auto;padding:6px}',
      '.wg-tab-item{width:auto;flex:0 0 auto;max-width:200px;min-height:40px}',
      '.wg-tab-item .s{display:none}',
      '.wg-btn{min-height:40px;padding:8px 12px}',
      '.wg-ibtn{padding:10px}',
      '.wg-select{min-height:36px;font-size:13px;max-width:124px}',
      // A phone header has no room for it; the Panels tab moves a panel.
      '.term-cell[data-wgid] [data-wg-slot]{display:none}',
      '.wg-dock{max-height:42%}',
      // Anchored to the TOP on a phone, never the bottom. A sheet pinned to
      // bottom:0 sits on the layout viewport, which on Android Chrome runs
      // under the browser's own bars: its last rows were cut off. svh is the
      // viewport with those bars showing, so this always fits.
      '.wg-pop{top:calc(var(--topbar-height) + 52px);right:8px;left:8px;bottom:auto;width:auto;height:min(68svh,560px)}',
      '.wg-picker-list .wg-tab-item{width:100%;max-width:none}',
      '.wg-chip{max-width:140px}.wg-chip .l{display:none}',
      // On a phone the bar has no room for chips at all. There is one button
      // in the bar instead, and the SAME chips become the rows of a list that
      // drops from it. They are hidden with visibility, never display: a live
      // chip is a running frame (a timer, a meter) and has to keep running,
      // and keep being measured, while the list is closed.
      '.topbar>.wg-chips{position:fixed;top:calc(var(--topbar-height) + 6px);left:8px;right:8px;max-width:none;max-height:min(62svh,420px);flex-direction:column;align-items:stretch;gap:4px;padding:8px;overflow-x:hidden;overflow-y:auto;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius);box-shadow:var(--shadow-lg);z-index:940;visibility:hidden;opacity:0;pointer-events:none;transform:translateY(-6px);transition:opacity .14s ease,transform .14s ease,visibility 0s linear .14s}',
      '.topbar>.wg-chips.open{visibility:visible;opacity:1;pointer-events:auto;transform:none;transition:opacity .14s ease,transform .14s ease}',
      '.topbar>.wg-chips::before{content:"Panels";font-size:11px;font-weight:700;letter-spacing:.05em;text-transform:uppercase;color:var(--text-secondary);padding:2px 6px 6px}',
      '.wg-chips>.wg-chip,.wg-chips>.wg-chip.live{max-width:none;width:100%;height:auto;min-height:46px;padding:6px 12px;border-radius:10px;justify-content:flex-start;gap:10px;background:var(--bg-primary)}',
      '.wg-chips>.wg-chip .wg-chip-name{display:inline;flex:1 1 auto;min-width:0;overflow:hidden;text-overflow:ellipsis;text-align:left;font-size:13px;color:var(--text-primary)}',
      '.wg-chips>.wg-chip.live .wg-chip-plain{display:none}',
      '.wg-chips>.wg-chip .wg-chip-plain .l{display:none}',
      '.wg-chips>.wg-chip .wg-chip-plain .ic{display:none}',
      '.wg-chips>.wg-chip .wg-chip-plain .v{color:var(--text-secondary)}',
      '.wg-chip-toggle{display:inline-flex}',
      '.wg-shot img{height:180px}',
      '}'
    ].join('\n');
    document.head.appendChild(s);
  }

  // ─── One frame ───

  /**
   * @param {HTMLElement} host
   * @param {{project:string,id:string,frame:string,autoHeight?:boolean,maxHeight?:()=>number}} o
   */
  function createView(host, o) {
    injectStyles();
    var v = {
      project: o.project, pkey: pkey(o.project), id: o.id, frame: o.frame || 'cell',
      root: el('div', 'wg-view ' + (o.autoHeight ? 'auto' : 'fill')),
      banners: el('div'), iframe: document.createElement('iframe'),
      destroyed: false, started: false, faults: 0, calls: [], height: 0, ro: null, ctxTimer: null, seq: 0
    };
    v.iframe.className = 'wg-frame';
    // The only tokens this frame ever gets. See the note at the top of the file.
    v.iframe.setAttribute('sandbox', 'allow-scripts');
    // Sound. A frame with an opaque origin does not inherit "the person has
    // interacted with this page", so a panel could not play anything until it
    // was touched itself, and lost that again on every reload (and panels
    // reload whenever Claude saves a change). This hands down the page's own
    // standing for audio and nothing else: it is a permissions-policy grant,
    // not a sandbox token, and gives no network, storage or origin.
    v.iframe.setAttribute('allow', 'autoplay');
    v.iframe.setAttribute('referrerpolicy', 'no-referrer');
    v.iframe.setAttribute('title', 'Panel');
    v.root.appendChild(v.banners);
    v.root.appendChild(v.iframe);
    host.appendChild(v.root);

    v.post = function (m) {
      if (v.destroyed || !v.iframe.contentWindow) return;
      m.crundi = 1;
      try { v.iframe.contentWindow.postMessage(m, '*'); } catch (e) { /* frame gone */ }
    };

    v.context = function () {
      var r = v.iframe.getBoundingClientRect();
      var mobile = !!cfg.isMobile();
      return { frame: v.frame, platform: mobile ? 'mobile' : 'desktop', touch: mobile || ('ontouchstart' in window), width: Math.round(r.width), height: Math.round(r.height) };
    };

    v.applyHeight = function () {
      if (!o.autoHeight) return;
      if (v.root.closest('.wg-side')) { v.iframe.style.height = '100%'; return; }
      var max = o.maxHeight ? o.maxHeight() : 520;
      v.iframe.style.height = Math.max(36, Math.min(max, v.height || 80)) + 'px';
    };

    v.banner = function (kind, node) {
      var b = el('div', 'wg-banner ' + kind);
      b.appendChild(node);
      v.banners.appendChild(b);
      return b;
    };

    v.load = function () {
      var mine = ++v.seq;
      // The chip asks for the chip build of the panel: same source, told from
      // its first byte to show only the element marked data-chip.
      return api('/api/widgets/doc' + qs(v.project, v.id, o.chip ? '&chip=1' : '')).then(function (d) {
        if (v.destroyed || mine !== v.seq) return;
        v.banners.innerHTML = '';
        v.started = false; v.faults = 0; v.missed = false;
        if (!d.ok) {
          v.iframe.style.display = 'none';
          v.banner('err', document.createTextNode(d.error || 'This panel could not be loaded.'));
          return;
        }
        v.iframe.style.display = '';
        v.iframe.setAttribute('title', (d.widget && d.widget.title) || 'Panel');
        if (o.onMeta && d.widget) { try { o.onMeta(d.widget); } catch (e) { /* host's problem */ } }
        if (d.grant && d.grant.needed && !d.grant.granted) v.banner('grant', grantNode(v, d.grant));
        v.iframe.srcdoc = d.html;
        v.applyHeight();
      });
    };

    v.sendData = function (source) {
      if (v.destroyed) return;
      // A change that lands while the frame is still starting must not be
      // dropped: remember it, and catch up once the frame has its first data.
      if (!v.started) { v.missed = true; return; }
      api('/api/widgets/data' + qs(v.project, v.id, source ? '&source=' + encodeURIComponent(source) : '')).then(function (d) {
        if (v.destroyed || !d.ok) return;
        for (var name in d.data) if (Object.prototype.hasOwnProperty.call(d.data, name)) v.post({ t: 'data', source: name, value: d.data[name].value, error: d.data[name].error || '' });
      });
    };

    v.onMessage = function (m) {
      if (m.t === 'ready') {
        api('/api/widgets/data' + qs(v.project, v.id)).then(function (d) {
          if (v.destroyed) return;
          v.started = true;
          v.post({ t: 'init', data: d.ok ? d.data : {}, store: (d.ok && d.store) || {}, context: v.context() });
          if (v.missed) { v.missed = false; v.sendData(); }
          // Frames that size to their content (a chip, a dock, an inline card):
          // ask outright once the first data is in, not only on the next paint.
          if (o.chip || o.autoHeight) setTimeout(function () { v.post({ t: 'measure' }); }, 200);
        });
      } else if (m.t === 'height') {
        v.height = Number(m.h) || 0;
        if (o.onSize) { try { o.onSize(Number(m.w) || 0, v.height); } catch (e) { /* host's problem */ } }
        v.applyHeight();
      } else if (m.t === 'fault') {
        // A handful is a diagnosis; a flood is a loop. Stop reporting after a few.
        if (v.faults++ < 5) api('/api/widgets/fault', { project: v.project, id: v.id, message: String(m.message || '').slice(0, 500), where: String(m.where || '').slice(0, 120) });
      } else if (m.t === 'call') {
        v.handleCall(m);
      }
    };

    v.handleCall = function (m) {
      // Something done inside a panel is the person being here; the page
      // cannot see pointer or keys that land in the frame.
      if (window.crundiNoteInput) { try { window.crundiNoteInput(); } catch (e) { /* not this page */ } }
      var now = Date.now();
      v.calls = v.calls.filter(function (t) { return now - t < 10000; });
      if (v.calls.length >= 40) return v.post({ t: 'result', seq: m.seq, ok: false, error: 'Too many requests; slow down.' });
      v.calls.push(now);
      var a = m.args || {};
      if (m.op === 'toast') { cfg.toast(String(a.text || '').slice(0, 200), a.kind === 'error' ? 'error' : ''); return v.post({ t: 'result', seq: m.seq, ok: true, value: true }); }
      if (m.op === 'openLink') {
        var url = String(a.url || '');
        // http(s) only: a widget must not be able to open javascript: or data: in a real tab.
        if (!/^https?:\/\//i.test(url)) return v.post({ t: 'result', seq: m.seq, ok: false, error: 'Only http and https links can be opened' });
        window.open(url, '_blank', 'noopener,noreferrer');
        return v.post({ t: 'result', seq: m.seq, ok: true, value: true });
      }
      api('/api/widgets/call', { project: v.project, id: v.id, op: String(m.op || ''), args: a }).then(function (r) {
        if (r && r.ok) v.post({ t: 'result', seq: m.seq, ok: true, value: r });
        else v.post({ t: 'result', seq: m.seq, ok: false, error: (r && r.error) || 'Failed' });
      });
    };

    if (window.ResizeObserver) {
      v.ro = new ResizeObserver(function () {
        clearTimeout(v.ctxTimer);
        v.ctxTimer = setTimeout(function () { if (v.started) v.post({ t: 'context', context: v.context() }); v.applyHeight(); }, 150);
      });
      v.ro.observe(v.iframe);
    }

    v.destroy = function () {
      if (v.destroyed) return;
      v.destroyed = true;
      clearTimeout(v.ctxTimer);
      try { if (v.ro) v.ro.disconnect(); } catch (e) { /* ignore */ }
      var i = views.indexOf(v); if (i >= 0) views.splice(i, 1);
      try { v.root.remove(); } catch (e) { /* ignore */ }
    };

    views.push(v);
    v.load();
    return v;
  }

  function grantNode(v, g) {
    var box = el('div');
    box.appendChild(el('div', '', '<b>This panel is asking for more than it gets by default.</b> It keeps doing these after the chat that built it is gone.'));
    var ul = el('ul', 'wg-grant-list');
    (g.items || []).forEach(function (it) {
      var li = el('li');
      li.appendChild(el('span', 'w', esc(it.what)));
      if (it.detail) { var c = el('code'); c.textContent = it.detail; li.appendChild(c); }
      ul.appendChild(li);
    });
    box.appendChild(ul);
    var row = el('div', 'wg-row');
    var yes = el('button', 'wg-btn primary', IC.shield + 'Approve');
    var no = el('button', 'wg-btn', 'Not now');
    yes.addEventListener('click', function () {
      yes.disabled = true;
      api('/api/widgets/grant', { project: v.project, id: v.id, hash: g.hash }).then(function (r) {
        if (!r.ok) { cfg.toast(r.error || 'Could not approve', 'error'); if (r.stale) v.load(); else yes.disabled = false; return; }
        cfg.toast('Approved');
      });
    });
    no.addEventListener('click', function () { var b = box.closest('.wg-banner'); if (b) b.remove(); });
    row.appendChild(yes); row.appendChild(no);
    box.appendChild(row);
    return box;
  }

  // One listener for every frame: match the message to its view by window.
  window.addEventListener('message', function (e) {
    var m = e.data;
    if (!m || m.crundi !== 1) return;
    for (var i = 0; i < views.length; i++) {
      if (views[i].iframe.contentWindow === e.source) { views[i].onMessage(m); return; }
    }
  });

  // Keep-alive: tells the server which widgets are on screen (so their data
  // sources run), and drops views whose element left the page without saying.
  setInterval(function () {
    for (var i = views.length - 1; i >= 0; i--) if (!views[i].root.isConnected) views[i].destroy();
    if (document.hidden || !cfg.apiFetch) return;
    var by = {};
    views.forEach(function (v) { (by[v.project] = by[v.project] || {})[v.id] = 1; });
    widgets.forEach(function (w) { if (w.state === 'open' && w.slot === 'chip') (by[project] = by[project] || {})[w.id] = 1; });
    Object.keys(by).forEach(function (p) { api('/api/widgets/watch', { project: p, ids: Object.keys(by[p]) }); });
  }, 25000);

  // ─── List and placement ───

  function byId(id) { for (var i = 0; i < widgets.length; i++) if (widgets[i].id === id) return widgets[i]; return null; }

  /** Where a widget shows right now, given which chats are on screen. */
  function placeOf(w) {
    if (w.state !== 'open') return 'none';
    if (w.slot === 'dock') {
      if (!w.chat) return 'cell';                                   // made from a terminal: no chat to dock to
      return cfg.liveChats().indexOf(w.chat) >= 0 ? 'dock' : 'none'; // its chat is gone: Panels tab only
    }
    if (w.slot === 'inline' || w.slot === 'tab') return 'none';
    return w.slot;                                                   // cell | chip
  }

  function refresh() {
    var mine = ++loadSeq, p = project;
    if (!cfg.apiFetch) { widgets = []; cellSig = ''; cfg.onChange(); return Promise.resolve(); }
    // No project selected still has something to show: the global panels.
    return api('/api/widgets?project=' + encodeURIComponent(p)).then(function (d) {
      if (mine !== loadSeq) return;
      widgets = d.ok ? (d.widgets || []) : [];
      // Rebuilding the workbench moves every pane, which reloads every frame
      // in it and makes the whole page blink. That is only worth it when the
      // SET of panes changed. A panel docking, closing in a dock, being
      // retitled or edited by Claude touches only its own corner (onChrome).
      var sig = widgets.filter(function (w) { return placeOf(w) === 'cell'; }).map(function (w) { return w.id; }).join(',');
      if (sig !== cellSig) { cellSig = sig; cfg.onChange(); }
      else cfg.onChrome();
      renderChips();
      renderTabList();
      repaintPickers();
    });
  }

  function userPatch(id, patch) {
    return api('/api/widgets/user', { project: project, id: id, patch: patch }).then(function (r) {
      if (!r.ok) cfg.toast(r.error || 'Could not update the panel', 'error');
      return r;
    });
  }

  function slotSelect(w, onPick) {
    var sel = el('select', 'wg-select');
    sel.title = 'Where this panel sits';
    sel.setAttribute('aria-label', 'Where this panel sits');
    MOVABLE.forEach(function (s) {
      if (s === 'dock' && !w.chat) return;
      var o = document.createElement('option'); o.value = s; o.textContent = SLOT_LABEL[s];
      if (s === w.slot) o.selected = true;
      sel.appendChild(o);
    });
    if (MOVABLE.indexOf(w.slot) < 0) { var cur = document.createElement('option'); cur.value = w.slot; cur.textContent = SLOT_LABEL[w.slot] || w.slot; cur.selected = true; sel.insertBefore(cur, sel.firstChild); }
    sel.addEventListener('change', function () { userPatch(w.id, { slot: sel.value }).then(function () { if (onPick) onPick(sel.value); }); });
    sel.addEventListener('mousedown', function (e) { e.stopPropagation(); });
    return sel;
  }

  // ─── Workbench cell ───

  function cellHeadHtml(id) {
    var w = byId(id) || { title: id };
    return '<span class="term-drag" title="Drag to reorder">⋮⋮</span>'
      + '<span class="wb-head-ic">' + IC.panel + '</span>'
      + '<span class="term-title" style="cursor:default;">' + esc(w.title || id) + '</span>'
      + (w.needsGrant ? '<span class="wg-badge warn" title="Waiting for your approval">approve</span>' : '')
      + '<span class="term-head-spacer"></span>'
      + '<span data-wg-slot="' + esc(id) + '"></span>'
      + '<button class="term-font-btn" data-action="widget-reload" data-wgid="' + esc(id) + '" title="Reload">' + IC.refresh + '</button>'
      + '<button class="term-font-btn pane-pin" data-action="pane-pin" title="Pin size and position">' + IC.pin + '</button>'
      + '<button class="term-head-btn term-close" data-action="widget-close" data-wgid="' + esc(id) + '" title="Close panel">×</button>';
  }

  /** Bring a pane's header up to date without rebuilding it. */
  function refreshCellHead(cellEl, id) {
    var w = byId(id); if (!w) return;
    var t = cellEl.querySelector('.term-title');
    if (t && t.textContent !== (w.title || id)) t.textContent = w.title || id;
    var badge = cellEl.querySelector('.term-head > .wg-badge');
    if (w.needsGrant && !badge) {
      badge = el('span', 'wg-badge warn', 'approve'); badge.title = 'Waiting for your approval';
      if (t && t.nextSibling) t.parentNode.insertBefore(badge, t.nextSibling);
    } else if (!w.needsGrant && badge) badge.remove();
  }

  function mountCell(cellEl, id) {
    var body = cellEl.querySelector('.wg-cell-body');
    if (!body) return null;
    refreshCellHead(cellEl, id);
    var slot = cellEl.querySelector('[data-wg-slot]');
    var w = byId(id);
    if (slot && w && !slot.firstChild) slot.appendChild(slotSelect(w));
    if (body._wgView && !body._wgView.destroyed) return body._wgView;
    body.innerHTML = '';
    body._wgView = createView(body, { project: project, id: id, frame: 'cell' });
    return body._wgView;
  }

  function reload(id) { views.forEach(function (v) { if (v.id === id && v.pkey === pkey(project)) v.load(); }); }

  // ─── Dock (inside a chat cell) ───

  function syncDock(termBody, chatId) {
    injectStyles();
    var want = widgets.filter(function (w) { return w.chat === chatId && placeOf(w) === 'dock'; });
    var dock = termBody.querySelector(':scope > .wg-dock');
    if (!want.length) {
      if (dock) { dock.querySelectorAll('.wg-dock-item').forEach(function (n) { if (n._wgView) n._wgView.destroy(); }); dock.remove(); }
      termBody.classList.remove('wg-has-dock', 'wg-side');
      if (termBody._wgRo) { termBody._wgRo.disconnect(); termBody._wgRo = null; }
      return;
    }
    if (!dock) {
      dock = el('div', 'wg-dock');
      termBody.insertBefore(dock, termBody.firstChild);
      termBody.classList.add('wg-has-dock');
      if (window.ResizeObserver) {
        // Wide cell: the dock sits beside the conversation. Narrow: above it.
        termBody._wgRo = new ResizeObserver(function () {
          var side = termBody.clientWidth >= 900 && !cfg.isMobile();
          if (side !== termBody.classList.contains('wg-side')) {
            termBody.classList.toggle('wg-side', side);
            views.forEach(function (v) { if (termBody.contains(v.root)) v.applyHeight(); });
          }
        });
        termBody._wgRo.observe(termBody);
      }
    }
    var have = {};
    dock.querySelectorAll(':scope > .wg-dock-item').forEach(function (n) {
      var id = n.getAttribute('data-wid');
      if (!want.some(function (w) { return w.id === id; })) { if (n._wgView) n._wgView.destroy(); n.remove(); }
      else have[id] = n;
    });
    want.forEach(function (w) {
      var item = have[w.id];
      if (!item) {
        item = el('div', 'wg-dock-item');
        item.setAttribute('data-wid', w.id);
        var head = el('div', 'wg-dock-head');
        var title = el('span', 'wg-dock-title');
        var collapse = el('button', 'wg-ibtn wg-dock-collapse', IC.up);
        collapse.title = 'Collapse';
        var close = el('button', 'wg-ibtn', IC.x);
        close.title = 'Close panel';
        var bodyEl = el('div', 'wg-dock-body');
        var toggle = function () {
          var c = !item.classList.contains('collapsed');
          item.classList.toggle('collapsed', c);
          collapse.innerHTML = c ? IC.down : IC.up;
          collapse.title = c ? 'Expand' : 'Collapse';
          userPatch(w.id, { collapsed: c });
        };
        title.addEventListener('click', toggle);
        collapse.addEventListener('click', toggle);
        close.addEventListener('click', function () { userPatch(w.id, { closed: true }); });
        // Only placement folds away. It is the wide control and the one used
        // least; in a narrow dock or on a phone it sits behind a menu button.
        // Collapse stays in the header at every width: it is what a dock is
        // for, and one tap must be enough.
        var tools = el('span', 'wg-dock-tools');
        tools.appendChild(slotSelect(w));
        var more = el('button', 'wg-ibtn wg-dock-more', IC.more);
        more.title = 'More'; more.setAttribute('aria-label', 'More'); more.setAttribute('aria-haspopup', 'true');
        more.addEventListener('click', function (e) { e.stopPropagation(); head.classList.toggle('open'); });
        head.appendChild(title);
        head.appendChild(el('span', '', '')).setAttribute('data-wg-badge', '1');
        head.appendChild(tools);
        head.appendChild(collapse);
        head.appendChild(more);
        head.appendChild(close);
        item.appendChild(head);
        item.appendChild(bodyEl);
        dock.appendChild(item);
        item._wgView = createView(bodyEl, {
          project: project, id: w.id, frame: 'dock', autoHeight: true,
          maxHeight: function () { return Math.max(120, Math.round(termBody.clientHeight * 0.44) - 30); }
        });
      }
      item.querySelector('.wg-dock-title').innerHTML = IC.panel + '<span style="overflow:hidden;text-overflow:ellipsis">' + esc(w.title) + '</span>';
      var badge = item.querySelector('[data-wg-badge]');
      badge.className = w.needsGrant ? 'wg-badge warn' : '';
      badge.textContent = w.needsGrant ? 'approve' : '';
      var isCollapsed = !!w.collapsed;
      if (item.classList.contains('collapsed') !== isCollapsed) {
        item.classList.toggle('collapsed', isCollapsed);
        var cb = item.querySelector('.wg-dock-collapse');
        if (cb) cb.innerHTML = isCollapsed ? IC.down : IC.up;
      }
    });
  }

  // ─── Inline (in the chat transcript) ───

  function inlineNode(alias, id) {
    injectStyles();
    var box = el('div', 'wg-inline');
    var head = el('div', 'wg-inline-head', IC.panel);
    var t = el('span', 't'); t.textContent = (byId(id) || {}).title || id;
    var open = el('button', 'wg-ibtn', IC.expand);
    open.title = 'Open in the Panels tab';
    open.addEventListener('click', function () { if (cfg.openTab) cfg.openTab(id); });
    head.appendChild(t); head.appendChild(open);
    var body = el('div');
    box.appendChild(head); box.appendChild(body);
    createView(body, { project: alias || project, id: id, frame: 'inline', autoHeight: true, maxHeight: function () { return 460; }, onMeta: function (w) { t.textContent = w.title || id; } });
    return box;
  }

  /** The screenshots from Claude's last check of a widget, for the transcript. */
  function shotsNode(alias, id) {
    injectStyles();
    var row = el('div', 'wg-shots');
    api('/api/widgets/renders' + qs(alias || project, id)).then(function (d) {
      var shots = d.ok && d.renders && d.renders.shots || [];
      if (!shots.length) { row.remove(); return; }
      shots.forEach(function (s) {
        var name = s.frame + '--' + s.state;
        var fig = el('div', 'wg-shot');
        var img = document.createElement('img');
        img.alt = 'Render: ' + s.frame;
        img.loading = 'lazy';
        cfg.apiFetch('/api/widgets/render-image' + qs(alias || project, id, '&name=' + encodeURIComponent(name) + '&t=' + (d.renders.at || 0)))
          .then(function (r) { return r.ok ? r.blob() : null; })
          .then(function (b) { if (b) img.src = URL.createObjectURL(b); })
          .catch(function () {});
        img.addEventListener('click', function () {
          if (!img.src) return;
          zoomImage(img.src, img.alt);
        });
        fig.appendChild(img);
        fig.appendChild(document.createTextNode(s.frame + (s.state !== 'live' ? ' · ' + s.state : '')));
        row.appendChild(fig);
      });
    });
    return row;
  }

  // ─── Media shown in a chat (show_image / show_video / show_audio / show_file) ───
  //
  // Three rules shape this.
  //
  // The box comes first. Every item arrives with its size, so its place is
  // laid out at the right dimensions before a byte of it has loaded. A picture
  // that turned up late used to push the rest of the conversation down, after
  // the chat had already scrolled to where you were reading.
  //
  // Nothing loads until it is near the screen, and it is let go again when it
  // is far from it. A long transcript can hold hundreds of megabytes of
  // pictures and video; holding all of that for a conversation you are reading
  // the end of is how a phone tab gets killed.
  //
  // `items` are built by the HOST from validated names. Nothing here is taken
  // from a tool's output as markup: captions and file names are set as text.

  var mediaRoots = [];   // one pair of observers per scrolling container

  function scrollRootOf(node) {
    for (var n = node.parentElement; n && n !== document.body; n = n.parentElement) {
      var oy = getComputedStyle(n).overflowY;
      if ((oy === 'auto' || oy === 'scroll') && n.scrollHeight > n.clientHeight + 4) return n;
    }
    return null;   // the page itself
  }

  /** Watch a box: load() when it nears the screen, unload() when it is far off. */
  function watchMedia(box, load, unload) {
    if (!window.IntersectionObserver) { load(); return; }
    box._wgLoad = load; box._wgUnload = unload; box._wgLoaded = false;
    // The scroller only exists once the node is in the page.
    var attach = function () {
      if (!box.isConnected) { if ((box._wgTries = (box._wgTries || 0) + 1) < 40) setTimeout(attach, 150); return; }
      var root = scrollRootOf(box), entry = null;
      for (var i = 0; i < mediaRoots.length; i++) if (mediaRoots[i].root === root) entry = mediaRoots[i];
      if (!entry) {
        var onNear = function (list) { list.forEach(function (e) { var t = e.target; if (e.isIntersecting && !t._wgLoaded) { t._wgLoaded = true; t._wgLoad(); } }); };
        var onFar = function (list) { list.forEach(function (e) { var t = e.target; if (!e.isIntersecting && t._wgLoaded) { t._wgLoaded = false; t._wgUnload(); } }); };
        entry = {
          root: root,
          // About a screen ahead in either direction...
          near: new IntersectionObserver(onNear, { root: root, rootMargin: '900px 0px 900px 0px' }),
          // ...and let go well past that, so scrolling back a little is free.
          far: new IntersectionObserver(onFar, { root: root, rootMargin: '4500px 0px 4500px 0px' })
        };
        mediaRoots.push(entry);
      }
      entry.near.observe(box); entry.far.observe(box);
    };
    attach();
  }

  function fmtDuration(sec) {
    var s = Math.max(0, Math.round(Number(sec) || 0)), m = Math.floor(s / 60);
    return m + ':' + ('0' + (s % 60)).slice(-2);
  }
  function fmtBytes(n) {
    var v = Number(n) || 0, u = ['B', 'KB', 'MB', 'GB'], i = 0;
    while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
    return (i ? v.toFixed(v < 10 ? 1 : 0) : String(v)) + ' ' + u[i];
  }
  /**
   * An image, opened large. Zoom: ctrl + wheel (a trackpad pinch arrives as
   * that too), two fingers, a double click or double tap, or the buttons.
   * Move: drag. Close: the backdrop, the cross, or Escape.
   */
  function zoomImage(src, alt) {
    var z = el('div', 'wg-zoom'); var big = document.createElement('img'); big.src = src; big.alt = alt || ''; big.draggable = false;
    var bar = el('div', 'wg-zoom-bar');
    var mk = function (label, title, body) { var b = el('button', 'wg-zoom-btn'); b.type = 'button'; b.title = title; b.setAttribute('aria-label', title); if (body) b.innerHTML = svg(body); else b.textContent = label; bar.appendChild(b); return b; };
    var bOut = mk('', 'Zoom out', '<line x1="5" y1="12" x2="19" y2="12"/>');
    var bPct = mk('100%', 'Back to fit'); bPct.classList.add('pct');
    var bIn = mk('', 'Zoom in', '<line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>');
    var bClose = mk('', 'Close', '<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>');
    z.appendChild(big); z.appendChild(bar);
    var s = 1, x = 0, y = 0, MAX = 8;
    var apply = function (smooth) {
      // Never dragged further than the picture's own edge, plus a little.
      var mx = Math.max(0, (big.offsetWidth * s - z.clientWidth) / 2) + (s > 1 ? 40 : 0);
      var my = Math.max(0, (big.offsetHeight * s - z.clientHeight) / 2) + (s > 1 ? 40 : 0);
      x = Math.max(-mx, Math.min(mx, x)); y = Math.max(-my, Math.min(my, y));
      big.style.transition = smooth ? 'transform .16s ease' : 'none';
      big.style.transform = 'translate(' + x + 'px,' + y + 'px) scale(' + s + ')';
      z.classList.toggle('zoomed', s > 1.001);
      bPct.textContent = Math.round(s * 100) + '%';
    };
    /** Zoom to ns, keeping the point (cx, cy) of the screen under the same spot of the picture. */
    var zoomAt = function (cx, cy, ns, smooth) {
      ns = Math.max(1, Math.min(MAX, ns));
      var r = z.getBoundingClientRect(), ox = cx - (r.left + r.width / 2), oy = cy - (r.top + r.height / 2);
      x = x + (ox - x) * (1 - ns / s); y = y + (oy - y) * (1 - ns / s);
      s = ns; if (s <= 1.001) { s = 1; x = 0; y = 0; }
      apply(smooth);
    };
    var centre = function () { var r = z.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
    var close = function () { document.removeEventListener('keydown', onKey, true); z.remove(); };
    var onKey = function (e) {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); }
      else if (e.key === '+' || e.key === '=') { var c = centre(); zoomAt(c[0], c[1], s * 1.4, true); }
      else if (e.key === '-') { var c2 = centre(); zoomAt(c2[0], c2[1], s / 1.4, true); }
      else if (e.key === '0') { zoomAt(0, 0, 1, true); }
    };
    document.addEventListener('keydown', onKey, true);
    bar.addEventListener('pointerdown', function (e) { e.stopPropagation(); });
    bOut.addEventListener('click', function () { var c = centre(); zoomAt(c[0], c[1], s / 1.4, true); });
    bIn.addEventListener('click', function () { var c = centre(); zoomAt(c[0], c[1], s * 1.4, true); });
    bPct.addEventListener('click', function () { zoomAt(0, 0, 1, true); });
    bClose.addEventListener('click', close);

    z.addEventListener('wheel', function (e) {
      e.preventDefault();                       // or ctrl + wheel zooms the whole page
      if (e.ctrlKey || e.metaKey) zoomAt(e.clientX, e.clientY, s * Math.exp(-e.deltaY * (Math.abs(e.deltaY) < 50 ? 0.01 : 0.002)));
      else if (s > 1) { x -= e.deltaX; y -= e.deltaY; apply(); }
    }, { passive: false });

    var pts = {}, count = 0, moved = 0, downOn = null, lastTap = 0, pinch = null;
    var two = function () { var k = Object.keys(pts), a = pts[k[0]], b = pts[k[1]]; return { d: Math.hypot(a.x - b.x, a.y - b.y) || 1, mx: (a.x + b.x) / 2, my: (a.y + b.y) / 2 }; };
    z.addEventListener('pointerdown', function (e) {
      if (e.button) return;
      e.preventDefault();
      if (!count) { moved = 0; downOn = e.target; }
      pts[e.pointerId] = { x: e.clientX, y: e.clientY }; count = Object.keys(pts).length;
      try { z.setPointerCapture(e.pointerId); } catch (err) { /* already gone */ }
      if (count === 2) { pinch = two(); moved = 99; }
      z.classList.add('grabbing');
    });
    z.addEventListener('pointermove', function (e) {
      var p = pts[e.pointerId]; if (!p) return;
      var dx = e.clientX - p.x, dy = e.clientY - p.y;
      p.x = e.clientX; p.y = e.clientY;
      if (count === 1) {
        moved += Math.abs(dx) + Math.abs(dy);
        if (s > 1) { x += dx; y += dy; apply(); }
      } else if (count === 2 && pinch) {
        var now = two();
        x += now.mx - pinch.mx; y += now.my - pinch.my;
        zoomAt(now.mx, now.my, s * now.d / pinch.d);
        pinch = now;
      }
    });
    var up = function (e) {
      if (!pts[e.pointerId]) return;
      delete pts[e.pointerId]; count = Object.keys(pts).length; pinch = null;
      if (count) return;
      z.classList.remove('grabbing');
      if (e.type === 'pointercancel' || moved > 6) return;
      // A plain tap or click.
      if (downOn !== big) { close(); return; }
      var t = Date.now();
      if (t - lastTap < 320) { lastTap = 0; zoomAt(e.clientX, e.clientY, s > 1 ? 1 : 2.5, true); }
      else lastTap = t;
    };
    z.addEventListener('pointerup', up); z.addEventListener('pointercancel', up);
    z._close = close;   // the back button closes it through this
    document.body.appendChild(z);
    apply();
  }

  /** The box an image or video sits in: the right shape from the first frame. */
  function sizedBox(it, single, cls) {
    var w = Number(it.w) || 0, h = Number(it.h) || 0;
    var box = el('div', 'wg-mbox ' + cls);
    if (w > 0 && h > 0) {
      box.style.aspectRatio = w + ' / ' + h;
      if (single) box.style.width = Math.round(Math.min(w, 420 * w / h, 640)) + 'px';   // never taller than 420
      else box.style.width = Math.round(220 * w / h) + 'px';                            // a row 220 tall
    } else {
      // Size unknown (stored by an older version, or nothing could measure it).
      box.style.aspectRatio = '16 / 9';
      box.style.width = single ? '420px' : '391px';
    }
    return box;
  }

  function imageItem(it, single) {
    var box = sizedBox(it, single, 'wg-mimg');
    var img = document.createElement('img');
    img.alt = it.caption || it.filename || 'Image';
    img.decoding = 'async';
    box.appendChild(img);
    var objUrl = '';
    watchMedia(box, function () {
      box.classList.add('loading');
      cfg.apiFetch(it.url)
        .then(function (r) { return r.ok ? r.blob() : null; })
        .then(function (b) {
          box.classList.remove('loading');
          if (!b) { box.classList.add('gone'); box.textContent = 'This image is no longer stored.'; return; }
          if (!box._wgLoaded) return;                 // scrolled far away while it was coming
          objUrl = URL.createObjectURL(b); img.src = objUrl; box.classList.add('ready');
        })
        .catch(function () { /* a network blip: it retries the next time it nears the screen */ box.classList.remove('loading'); box._wgLoaded = false; });
    }, function () {
      box.classList.remove('ready'); box.classList.remove('loading');
      img.removeAttribute('src');
      if (objUrl) { URL.revokeObjectURL(objUrl); objUrl = ''; }
    });
    img.addEventListener('click', function () { if (img.src) zoomImage(img.src, img.alt); });
    return box;
  }

  var PL_IC = {
    play: '<polygon points="7 4 20 12 7 20 7 4" fill="currentColor"/>',
    pause: '<rect x="6" y="4" width="4" height="16" rx="1" fill="currentColor"/><rect x="14" y="4" width="4" height="16" rx="1" fill="currentColor"/>',
    vol: '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.5 8.5a5 5 0 0 1 0 7"/><path d="M18.5 5.5a9 9 0 0 1 0 13"/>',
    mute: '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="22" y1="9" x2="16" y2="15"/><line x1="16" y1="9" x2="22" y2="15"/>',
    turn: '<rect x="8" y="3" width="8" height="14" rx="1.5"/><path d="M3 15a8 8 0 0 0 8 6"/><polyline points="8 22 11 21 10 18"/><path d="M21 9a8 8 0 0 0-3-5"/>',
    full: '<path d="M8 3H5a2 2 0 0 0-2 2v3"/><path d="M21 8V5a2 2 0 0 0-2-2h-3"/><path d="M16 21h3a2 2 0 0 0 2-2v-3"/><path d="M3 16v3a2 2 0 0 0 2 2h3"/>'
  };

  /**
   * Video and audio stream from the server (so they can seek), which needs a
   * URL the element can fetch by itself. The controls are ours, so the player
   * looks like the rest of the app in every browser.
   */
  function playerItem(it, single) {
    var isVideo = it.kind === 'video';
    var box = isVideo ? sizedBox(it, single, 'wg-mvid') : el('div', 'wg-mbox wg-maud');
    box.classList.add('wg-pl', 'paused');
    var m = document.createElement(isVideo ? 'video' : 'audio');
    m.preload = 'none';                 // nothing is fetched until play is pressed
    if (isVideo) { m.playsInline = true; m.setAttribute('playsinline', ''); }
    box.appendChild(m);

    var btn = function (cls, title, body) { var b = el('button', 'wg-pl-btn ' + cls); b.type = 'button'; b.title = title; b.setAttribute('aria-label', title); b.innerHTML = svg(body); return b; };
    var bar = el('div', 'wg-pl-bar');
    var bPlay = btn('play', 'Play', PL_IC.play);
    var seek = el('div', 'wg-pl-seek'); seek.tabIndex = 0; seek.setAttribute('role', 'slider'); seek.setAttribute('aria-label', 'Position');
    var buf = el('div', 'wg-pl-buf'), fill = el('div', 'wg-pl-fill'), knob = el('div', 'wg-pl-knob');
    var track = el('div', 'wg-pl-track'); track.appendChild(buf); track.appendChild(fill); track.appendChild(knob); seek.appendChild(track);
    var time = el('span', 'wg-pl-time');
    var bMute = btn('mute', 'Mute', PL_IC.vol);
    bar.appendChild(bPlay); bar.appendChild(time); bar.appendChild(seek); bar.appendChild(bMute);
    var bFull = null, big = null, bFit = null, bTurn = null;
    if (isVideo) {
      // How the picture fills a full screen: only shown there, and only on touch screens.
      bFit = el('button', 'wg-pl-fit'); bFit.type = 'button'; bFit.title = 'How the video fills the screen'; bFit.textContent = 'Fit'; bar.appendChild(bFit);
      // Turn the whole screen between upright and sideways, whichever way the
      // phone is held. Full screen on a touch screen only, and only where the
      // browser lets a page do it (Android; not iPhone).
      if (window.screen && screen.orientation && screen.orientation.lock) { bTurn = btn('turn', 'Rotate the screen', PL_IC.turn); bar.appendChild(bTurn); }
      bFull = btn('full', 'Full screen', PL_IC.full); bar.appendChild(bFull);
      big = btn('big', 'Play', PL_IC.play); box.appendChild(big);
    }
    box.appendChild(bar);

    var total = function () { return (isFinite(m.duration) && m.duration > 0) ? m.duration : (Number(it.duration) || 0); };
    var dragging = false, dragAt = 0;
    var paint = function () {
      var d = total(), t = dragging ? dragAt : (m.currentTime || 0), p = d ? Math.max(0, Math.min(1, t / d)) : 0;
      fill.style.width = (p * 100) + '%'; knob.style.left = (p * 100) + '%';
      var b = 0; try { if (d && m.buffered.length) b = m.buffered.end(m.buffered.length - 1) / d; } catch (e) { /* nothing buffered */ }
      buf.style.width = (Math.min(1, b) * 100) + '%';
      time.textContent = fmtDuration(t) + (d ? ' / ' + fmtDuration(d) : '');
      seek.setAttribute('aria-valuenow', String(Math.round(p * 100)));
    };
    var state = function () {
      var off = m.paused || m.ended;
      box.classList.toggle('paused', off);
      bPlay.innerHTML = svg(off ? PL_IC.play : PL_IC.pause); bPlay.title = off ? 'Play' : 'Pause'; bPlay.setAttribute('aria-label', bPlay.title);
      bMute.innerHTML = svg(m.muted || m.volume === 0 ? PL_IC.mute : PL_IC.vol); bMute.title = m.muted ? 'Unmute' : 'Mute'; bMute.setAttribute('aria-label', bMute.title);
      if (off) box.classList.remove('idle');
    };
    var toggle = function () {
      if (!box.isConnected) return;
      // The file itself is asked for only now, on the first press.
      if (!m.getAttribute('src')) setSrc(false);
      if (m.paused || m.ended) { if (m.readyState < 3) box.classList.add('busy'); var pr = m.play(); if (pr && pr.catch) pr.catch(function () { /* the error handler below deals with it */ }); }
      else m.pause();
    };
    bPlay.addEventListener('click', toggle);
    if (big) big.addEventListener('click', toggle);
    bMute.addEventListener('click', function () { m.muted = !m.muted; });
    ['play', 'pause', 'ended', 'volumechange', 'emptied'].forEach(function (n) { m.addEventListener(n, state); });
    ['timeupdate', 'durationchange', 'progress', 'seeked', 'loadedmetadata', 'emptied'].forEach(function (n) { m.addEventListener(n, paint); });
    m.addEventListener('waiting', function () { box.classList.add('busy'); });
    m.addEventListener('seeking', function () { if (m.readyState < 3 && !m.paused) box.classList.add('busy'); });
    ['playing', 'pause', 'canplay', 'seeked', 'emptied', 'error'].forEach(function (n) { m.addEventListener(n, function () { box.classList.remove('busy'); }); });

    // Seeking: press or drag anywhere on the track; arrows from the keyboard.
    var at = function (e) { var r = track.getBoundingClientRect(); return Math.max(0, Math.min(1, (e.clientX - r.left) / (r.width || 1))) * total(); };
    var go = function (t) { if (!m.getAttribute('src')) setSrc(false); try { m.currentTime = Math.max(0, Math.min(total() || t, t)); } catch (e) { /* not seekable yet */ } paint(); };
    seek.addEventListener('pointerdown', function (e) {
      if (e.button || !total()) return;
      e.preventDefault(); dragging = true; dragAt = at(e); box.classList.add('seeking');
      try { seek.setPointerCapture(e.pointerId); } catch (err) { /* already gone */ }
      paint();
    });
    seek.addEventListener('pointermove', function (e) { if (dragging) { dragAt = at(e); paint(); } });
    var drop = function (e) { if (!dragging) return; dragging = false; box.classList.remove('seeking'); if (e.type === 'pointerup') go(dragAt); else paint(); };
    seek.addEventListener('pointerup', drop); seek.addEventListener('pointercancel', drop);
    seek.addEventListener('keydown', function (e) {
      if (e.key === 'ArrowRight') { e.preventDefault(); go((m.currentTime || 0) + 5); }
      else if (e.key === 'ArrowLeft') { e.preventDefault(); go((m.currentTime || 0) - 5); }
      else if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); toggle(); }
    });

    if (isVideo) {
      // A press on the picture plays or pauses. While playing, the bar steps
      // back after a moment and returns when the pointer moves or a finger lands.
      var idleT = 0;
      var wake = function () { box.classList.remove('idle'); clearTimeout(idleT); idleT = setTimeout(function () { if (!m.paused && !dragging) box.classList.add('idle'); }, 2600); };
      m.addEventListener('click', function () {
        // A finger on a hidden bar brings it back first, rather than pausing.
        if (box.classList.contains('idle')) { wake(); return; }
        toggle();
      });
      m.addEventListener('dblclick', function () { fullscreen(); });
      box.addEventListener('pointermove', wake); box.addEventListener('pointerdown', wake);
      m.addEventListener('play', wake);
      var fullscreen = function () {
        var d = document, on = d.fullscreenElement || d.webkitFullscreenElement;
        if (on) { leaveFull(); return; }
        var req = box.requestFullscreen || box.webkitRequestFullscreen;
        if (req) { var p = req.call(box); if (p && p.catch) p.catch(function () { /* refused */ }); }
        else if (m.webkitEnterFullscreen) m.webkitEnterFullscreen();     // iPhone: only the video itself can
      };
      bFull.addEventListener('click', fullscreen);
      // Out of full screen. A screen we turned sideways is turned upright
      // first: leaving while it is still sideways makes the page lay itself
      // out for a wide screen for a moment, and the browser reports the turn
      // as failed.
      var turned = false;
      var leaveFull = function () {
        var d = document, out = function () { if (d.fullscreenElement || d.webkitFullscreenElement) { try { (d.exitFullscreen || d.webkitExitFullscreen).call(d); } catch (e) { /* already out */ } } };
        if (!turned) { out(); return; }
        turned = false;
        var done = false, once = function () { if (!done) { done = true; out(); } };
        try { var p = screen.orientation.lock('portrait'); if (p && p.then) p.then(once, once); else once(); } catch (e) { once(); }
        setTimeout(once, 700);      // never stuck in full screen if the browser does not answer
      };
      box._wgLeaveFull = leaveFull;

      // Full screen only. The bar sits just under the picture when the screen
      // is taller than the video (a wide video on an upright phone), and never
      // lower than the bottom edge: when the picture reaches it, the bar lies
      // over the picture as it does inline.
      var FITS = [['contain', 'Fit'], ['cover', 'Fill'], ['fill', 'Stretch']], fitAt = 0;
      var isFull = function () { var f = document.fullscreenElement || document.webkitFullscreenElement; return f === box; };
      var place = function () {
        if (!isFull()) { bar.style.bottom = ''; m.style.objectFit = ''; box.classList.remove('under'); return; }
        m.style.objectFit = FITS[fitAt][0];
        var bw = box.clientWidth, bh = box.clientHeight, vw = m.videoWidth || Number(it.w) || 16, vh = m.videoHeight || Number(it.h) || 9;
        var picH = fitAt === 0 ? Math.min(bh, bw * vh / vw) : bh;
        var gap = (bh - picH) / 2, barH = bar.offsetHeight || 60;
        // Room under the picture for the whole bar: put it there. Otherwise the bottom edge.
        var touch = !!(window.matchMedia && window.matchMedia('(hover: none)').matches);   // a phone or tablet; a desktop keeps the bar at the bottom
        var under = touch && gap >= barH;
        bar.style.bottom = under ? Math.round(gap - barH) + 'px' : '0px';
        box.classList.toggle('under', under);
      };
      if (bTurn) bTurn.addEventListener('click', function () {
        var sideways = String(screen.orientation.type || '').indexOf('landscape') === 0;
        var p = null;
        // Only a refusal while still in full screen is worth a message: a turn
        // cut short by leaving full screen is not a failure.
        var failed = function (e) { if (isFull() && !(e && e.name === 'AbortError')) cfg.toast('This browser will not rotate the screen', 'error'); };
        turned = !sideways;
        try { p = screen.orientation.lock(sideways ? 'portrait' : 'landscape'); } catch (e) { turned = false; failed(e); }
        if (p && p.catch) p.catch(failed);
      });
      // Leaving full screen hands the screen back to however the phone is held.
      var letGo = function () { if (!isFull() && bTurn) { turned = false; try { screen.orientation.unlock(); } catch (e) { /* nothing was held */ } } };
      document.addEventListener('fullscreenchange', letGo); document.addEventListener('webkitfullscreenchange', letGo);
      bFit.addEventListener('click', function () { fitAt = (fitAt + 1) % FITS.length; bFit.textContent = FITS[fitAt][1]; place(); });
      document.addEventListener('fullscreenchange', place); document.addEventListener('webkitfullscreenchange', place);
      window.addEventListener('resize', function () { if (isFull()) place(); });
      m.addEventListener('loadedmetadata', place);
    }

    var retried = false;
    function setSrc(keepTime) {
      var was = keepTime ? m.currentTime : 0;
      m.src = cfg.mediaUrl(it.url);
      if (was > 0) { var once = function () { m.removeEventListener('loadedmetadata', once); try { m.currentTime = was; } catch (e) { /* not seekable yet */ } }; m.addEventListener('loadedmetadata', once); m.load(); }
    }
    // Near the screen: only the still picture a video shows before it plays.
    // Nothing of the video or the sound is fetched until play is pressed.
    watchMedia(box, function () { retried = false; if (isVideo && it.posterUrl) m.poster = cfg.mediaUrl(it.posterUrl); }, function () {
      // Something still playing is being listened to: it stays, however far the page has scrolled.
      // So does a video in full screen: there it has left the chat's scrolling
      // area, which looks exactly like being scrolled far away.
      var fs = document.fullscreenElement || document.webkitFullscreenElement;
      if ((!m.paused && !m.ended) || fs === box) { box._wgLoaded = true; return; }
      m.removeAttribute('src'); m.removeAttribute('poster');
      try { m.load(); } catch (e) { /* releases the buffer */ }
      state(); paint();
    });
    // The URL carries a sign-in token that lasts minutes. A video paused for a
    // while fails on its next request: fetch a fresh one, once, where it was.
    m.addEventListener('error', function () {
      if (!box._wgLoaded || retried || !m.getAttribute('src')) return;
      retried = true;
      (cfg.refreshAuth ? cfg.refreshAuth() : Promise.resolve()).then(function () { if (box._wgLoaded) setSrc(true); });
    });
    m.addEventListener('playing', function () { retried = false; });
    state(); paint();
    return box;
  }

  /**
   * A frame: a site's own embed (a video, a post, a player), any https page,
   * or HTML of Claude's own. Made when it nears the screen and removed when it
   * is far off, like the pictures. Always sandboxed:
   *  - a web address keeps its own origin (a player needs its cookies and
   *    storage) but can never navigate this page;
   *  - Claude's HTML gets no origin at all, so it can reach nothing of ours.
   */
  function embedItem(it) {
    var box = el('div', 'wg-mbox wg-membed');
    if (it.w > 0 && it.h > 0) {
      box.style.aspectRatio = it.w + ' / ' + it.h;
      box.style.width = it.w >= it.h ? '640px' : Math.round(560 * it.w / it.h) + 'px';
    } else {
      box.style.height = (it.height || 420) + 'px';
      box.style.width = (it.width || 640) + 'px';
    }
    var frame = null, seq = 0;
    watchMedia(box, function () {
      var mine = ++seq;
      box.classList.add('loading');
      var f = document.createElement('iframe');
      f.title = it.title || it.provider || 'Embedded content';
      f.setAttribute('referrerpolicy', 'strict-origin-when-cross-origin');   // YouTube refuses a frame that hides where it is
      f.setAttribute('allow', 'autoplay; encrypted-media; picture-in-picture; fullscreen; clipboard-write; web-share');
      f.setAttribute('allowfullscreen', '');
      f.addEventListener('load', function () { box.classList.remove('loading'); });
      var put = function () { if (mine !== seq) return; frame = f; box.appendChild(f); };
      if (it.htmlUrl) {
        f.setAttribute('sandbox', 'allow-scripts allow-popups allow-popups-to-escape-sandbox allow-forms allow-modals allow-pointer-lock');
        cfg.apiFetch(it.htmlUrl).then(function (r) { return r.ok ? r.text() : null; }).then(function (t) {
          if (mine !== seq) return;
          if (t == null) { box.classList.remove('loading'); box.classList.add('gone'); box.textContent = 'This embed is no longer stored.'; return; }
          f.srcdoc = t; put();
        }).catch(function () { box.classList.remove('loading'); box._wgLoaded = false; });
      } else {
        f.setAttribute('sandbox', 'allow-scripts allow-same-origin allow-popups allow-popups-to-escape-sandbox allow-forms allow-presentation');
        f.src = it.src; put();
      }
    }, function () {
      // Full screen is not "far away" (see the player).
      var fs = document.fullscreenElement || document.webkitFullscreenElement;
      if (fs && frame && (fs === frame || fs === box)) { box._wgLoaded = true; return; }
      seq++; box.classList.remove('loading');
      if (frame) { frame.remove(); frame = null; }
    });
    return box;
  }

  function fileItem(it) {
    var card = el('div', 'wg-mfile');
    var ic = el('span', 'wg-mfile-ic', cfg.fileIcon ? cfg.fileIcon(it.filename || 'file') : IC.panel);
    var meta = el('span', 'wg-mfile-meta');
    var n = el('span', 'wg-mfile-n'); n.textContent = it.filename || 'File'; n.title = it.filename || '';
    var sz = el('span', 'wg-mfile-s'); sz.textContent = fmtBytes(it.size) + (it.caption ? ' · ' + it.caption : '');
    meta.appendChild(n); meta.appendChild(sz);
    var b = el('button', 'wg-btn', IC.download + 'Download');
    b.type = 'button';
    b.addEventListener('click', function () {
      if (cfg.download) cfg.download({ url: it.url, name: it.filename || 'download', size: it.size });
    });
    card.appendChild(ic); card.appendChild(meta); card.appendChild(b);
    return card;
  }

  /**
   * @param {Array<{kind:string,url:string,posterUrl?:string,w?:number,h?:number,duration?:number,filename?:string,size?:number,caption?:string}>} items
   */
  function mediaNode(items, caption) {
    injectStyles();
    var files = items.every(function (it) { return it.kind === 'file'; });
    var audio = items.every(function (it) { return it.kind === 'audio'; });
    var single = items.length === 1;
    var box = el('div', 'wg-media' + (single ? ' one' : '') + (files ? ' files' : '') + (audio ? ' audio' : ''));
    var row = el('div', 'wg-media-row');
    items.forEach(function (it) {
      if (it.kind === 'file') { row.appendChild(fileItem(it)); return; }
      var fig = el('figure', 'wg-mitem');
      if (it.kind === 'embed') {
        fig.appendChild(embedItem(it));
        var ec = el('figcaption');
        var what = [it.title || '', it.caption || ''].filter(Boolean).join(' · ') || it.provider || (it.htmlUrl ? 'Embedded HTML' : '');
        var span = el('span'); span.textContent = what; ec.appendChild(span);
        if (it.openUrl) {
          var a = document.createElement('a'); a.href = it.openUrl; a.target = '_blank'; a.rel = 'noopener noreferrer'; a.className = 'wg-membed-open';
          a.textContent = 'Open' + (it.provider && what !== it.provider ? ' on ' + it.provider : '');
          if (what) ec.appendChild(document.createTextNode(' · '));
          ec.appendChild(a);
        }
        fig.appendChild(ec); row.appendChild(fig);
        return;
      }
      fig.appendChild(it.kind === 'image' ? imageItem(it, single) : playerItem(it, single));
      var label = it.kind === 'image' ? (it.caption || '') : [it.caption || it.filename || '', it.duration ? fmtDuration(it.duration) : ''].filter(Boolean).join(' · ');
      if (label) { var c = el('figcaption'); c.textContent = label; fig.appendChild(c); }
      row.appendChild(fig);
    });
    box.appendChild(row);
    if (caption) { var all = el('div', 'wg-media-cap'); all.textContent = caption; box.appendChild(all); }
    return box;
  }

  // ─── Chips ───

  function pathGet(obj, path) {
    if (!path) return obj;
    var cur = obj, parts = String(path).split('.');
    for (var i = 0; i < parts.length; i++) { if (cur == null) return undefined; cur = cur[parts[i]]; }
    return cur;
  }

  function loadChipValue(w) {
    if (!w.chip || !w.chip.source) return;
    api('/api/widgets/data' + qs(project, w.id, '&source=' + encodeURIComponent(w.chip.source))).then(function (d) {
      var entry = d.ok && d.data && d.data[w.chip.source];
      var val = entry ? pathGet(entry.value, w.chip.path) : undefined;
      chipValues[w.id] = val == null ? '' : (typeof val === 'object' ? JSON.stringify(val) : String(val)).slice(0, 60);
      paintChips();
    });
  }

  var chipNodes = {};   // id -> { el, view, live, sig }
  var chipToggle = null;

  /** Open or close the phone's list of chips. */
  function setChipList(open) {
    if (!chipEl) return;
    chipEl.classList.toggle('open', !!open);
    if (chipToggle) { chipToggle.classList.toggle('on', !!open); chipToggle.setAttribute('aria-expanded', open ? 'true' : 'false'); }
  }

  /** The phone's single top bar button, shown only when there is a chip to list. */
  function syncChipToggle(count) {
    if (!chipEl || !chipEl.parentElement) return;
    if (!chipToggle) {
      chipToggle = el('button', 'wg-chip-toggle', IC.panel + '<b></b>');
      chipToggle.type = 'button';
      chipToggle.title = 'Panels';
      chipToggle.setAttribute('aria-label', 'Panels');
      chipToggle.setAttribute('aria-haspopup', 'true');
      chipToggle.addEventListener('click', function (e) { e.stopPropagation(); setChipList(!chipEl.classList.contains('open')); });
      chipEl.parentElement.insertBefore(chipToggle, chipEl);
      // A tap anywhere else puts the list away.
      document.addEventListener('click', function (e) {
        if (!chipEl.classList.contains('open')) return;
        if (e.target.closest && (e.target.closest('.wg-chips') || e.target.closest('.wg-chip-toggle'))) return;
        setChipList(false);
      }, true);
    }
    chipToggle.hidden = !count;
    chipToggle.querySelector('b').textContent = count > 1 ? String(count) : '';
    if (!count) setChipList(false);
  }

  function dropChip(id) {
    var c = chipNodes[id]; if (!c) return;
    if (c.view) { try { c.view.destroy(); } catch (e) { /* ignore */ } }
    c.el.remove();
    delete chipNodes[id];
  }

  /**
   * Draw the chips. Reconciled by id, never rebuilt wholesale: a live chip is
   * a frame, and replacing it on every data tick would reload it every time.
   */
  function paintChips() {
    if (!chipEl) return;
    var list = widgets.filter(function (w) { return placeOf(w) === 'chip'; });
    chipEl.style.display = list.length ? '' : 'none';
    syncChipToggle(list.length);
    var want = {};
    list.forEach(function (w) { want[w.id] = 1; });
    Object.keys(chipNodes).forEach(function (id) { if (!want[id]) dropChip(id); });
    list.forEach(function (w) {
      // A panel with a chip face draws itself; one waiting for approval, or
      // without a face, gets the plain labelled chip.
      var live = !!w.chipLive && !w.needsGrant;
      var c = chipNodes[w.id];
      if (c && c.live !== live) { dropChip(w.id); c = null; }
      if (!c) {
        var b = el('button', 'wg-chip' + (live ? ' live' : ''));
        b.appendChild(el('span', 'wg-chip-name'));
        b.addEventListener('click', function () { setChipList(false); openPop(w.id); });
        c = chipNodes[w.id] = { el: b, view: null, live: live, sig: '' };
        if (live) {
          var holder = el('span', 'wg-chip-face');
          b.appendChild(holder);
          c.view = createView(holder, {
            project: project, id: w.id, frame: 'chipbar', chip: true,
            onSize: function (wd) {
              // On a phone the chip is a row of the dropped list, with room beside its name.
              var max = cfg.isMobile() ? 200 : 260;
              c.view.iframe.style.width = Math.max(16, Math.min(max, wd || 16)) + 'px';
              // Nothing marked data-chip came up after all: show the label instead.
              b.classList.toggle('empty', !wd);
            }
          });
        }
        chipEl.appendChild(b);
      }
      c.el.title = w.title + (w.needsGrant ? ' (waiting for your approval)' : '');
      var nm = c.el.querySelector('.wg-chip-name'); if (nm && nm.textContent !== w.title) nm.textContent = w.title;
      c.el.setAttribute('aria-label', w.title);
      if (!live || c.el.classList.contains('empty') || !c.plain) {
        var sig = [w.title, (w.chip && w.chip.label) || '', chipValues[w.id] || '', w.needsGrant ? 1 : 0].join('|');
        if (sig !== c.sig) {
          c.sig = sig;
          var plain = c.plain;
          if (!plain) { plain = c.plain = el('span', 'wg-chip-plain'); c.el.insertBefore(plain, c.el.querySelector('.wg-chip-name').nextSibling); }
          plain.innerHTML = IC.panel;
          var l = el('span', 'l'); l.textContent = (w.chip && w.chip.label) || w.title; plain.appendChild(l);
          if (chipValues[w.id]) { var vEl = el('span', 'v'); vEl.textContent = chipValues[w.id]; plain.appendChild(vEl); }
          if (w.needsGrant) plain.appendChild(el('span', 'wg-badge warn', 'approve'));
        }
      }
    });
  }

  function renderChips() {
    if (!chipEl) return;
    widgets.forEach(function (w) { if (placeOf(w) === 'chip') loadChipValue(w); });
    paintChips();
  }

  function closePop() {
    if (!pop) return;
    try { pop.view.destroy(); } catch (e) { /* ignore */ }
    pop.back.remove(); pop.box.remove();
    pop = null;
  }

  function openPop(id) {
    closePop();
    injectStyles();
    var w = byId(id); if (!w) return;
    var back = el('div', 'wg-pop-back');
    var box = el('div', 'wg-pop');
    var head = el('div', 'wg-dock-head');
    var t = el('span', 'wg-dock-title', IC.panel + '<span>' + esc(w.title) + '</span>');
    t.style.cursor = 'default';
    var x = el('button', 'wg-ibtn', IC.x); x.title = 'Close';
    head.appendChild(t); head.appendChild(slotSelect(w, closePop)); head.appendChild(x);
    box.appendChild(head);
    document.body.appendChild(back); document.body.appendChild(box);
    back.addEventListener('click', closePop);
    x.addEventListener('click', closePop);
    pop = { id: id, back: back, box: box, view: createView(box, { project: project, id: id, frame: 'chip' }) };
  }

  // ─── Picker: choose a panel, inside the pane it will fill ───
  // The workbench's + menu and an empty pane open a "Panel" pane first; this
  // draws the list in it. Closed panels are listed too: reopening is the point.

  var pickers = [];   // { host, onPick }

  function paintPicker(p) {
    var host = p.host;
    host.innerHTML = '';
    var list = el('div', 'wg-picker-list');
    host.appendChild(list);
    if (!widgets.length) {
      list.appendChild(el('div', 'wg-msg', IC.panel + '<div><b style="color:var(--text-primary)">No panels in this project yet</b></div>'
        + '<div style="max-width:300px">Ask Claude for one in a chat: "make a panel showing…", or "show me a live panel of this task".</div>'));
      return;
    }
    list.appendChild(el('div', 'wg-picker-h', 'Choose a panel to open here'));
    var sorted = widgets.slice().sort(function (a, b) { return (b.updatedAt || 0) - (a.updatedAt || 0); });
    sorted.forEach(function (w) {
      var where = placeOf(w);
      var b = el('button', 'wg-tab-item', IC.panel);
      var n = el('span', 'n'); n.textContent = w.title; b.appendChild(n);
      var st = el('span', 's');
      st.textContent = w.state !== 'open' ? 'closed' : where === 'cell' ? 'already open' : where === 'dock' ? 'docked to a chat' : where === 'chip' ? 'top bar' : 'Panels tab';
      b.appendChild(st);
      if (where === 'cell') { b.disabled = true; b.style.opacity = '0.55'; b.title = 'Already in the workbench'; }
      b.addEventListener('click', function () {
        b.disabled = true;
        // The host swaps this pane for the panel once the server says it is a
        // pane; told first so the swap is ready when that news arrives.
        try { p.onPick(w.id); } catch (e) { /* host's problem */ }
        userPatch(w.id, { closed: false, slot: 'cell' }).then(function (r) { if (!r || !r.ok) b.disabled = false; });
      });
      list.appendChild(b);
    });
  }

  function mountPicker(host, o) {
    injectStyles();
    for (var i = 0; i < pickers.length; i++) if (pickers[i].host === host) return;
    var p = { host: host, onPick: (o && o.onPick) || function () {} };
    pickers.push(p);
    paintPicker(p);
    // The list in memory may be stale (a panel made a moment ago in a chat).
    refresh();
  }

  function repaintPickers() {
    pickers = pickers.filter(function (p) { return p.host.isConnected; });
    pickers.forEach(paintPicker);
  }

  // ─── Panels tab ───

  function renderTabList() {
    var host = tabState.el;
    if (!host || !host.isConnected) return;
    var listEl = host.querySelector('.wg-tab-list');
    if (!listEl) return mountTab(host);
    if (!widgets.length) return mountTab(host);
    listEl.innerHTML = '';
    var sorted = widgets.slice().sort(function (a, b) { return (a.state === 'open' ? 0 : 1) - (b.state === 'open' ? 0 : 1) || (b.updatedAt || 0) - (a.updatedAt || 0); });
    if (!byId(tabState.selected)) tabState.selected = sorted[0] ? sorted[0].id : '';
    sorted.forEach(function (w) {
      var b = el('button', 'wg-tab-item' + (w.id === tabState.selected ? ' sel' : '') + (w.state !== 'open' ? ' closed' : ''), IC.panel);
      var n = el('span', 'n'); n.textContent = w.title; b.appendChild(n);
      if (w.global) { var gb = el('span', 'wg-badge', 'global'); gb.title = 'Shown in every project'; b.appendChild(gb); }
      if (w.needsGrant) b.appendChild(el('span', 'wg-badge warn', 'approve'));
      else if (w.faults) b.appendChild(el('span', 'wg-badge err', 'error'));
      else { var s = el('span', 's'); s.textContent = w.state !== 'open' ? 'closed' : (SLOT_LABEL[w.slot] || w.slot).split(' ')[0].toLowerCase(); b.appendChild(s); }
      b.addEventListener('click', function () { selectInTab(w.id); });
      listEl.appendChild(b);
    });
    paintTabBar();
  }

  function paintTabBar() {
    var host = tabState.el; if (!host) return;
    var bar = host.querySelector('.wg-tab-bar'); if (!bar) return;
    var w = byId(tabState.selected);
    bar.innerHTML = '';
    if (!w) return;
    var t = el('span', 't'); t.textContent = w.title; bar.appendChild(t);
    var mk = function (html, title, fn, cls) { var b = el('button', 'wg-btn' + (cls ? ' ' + cls : ''), html); b.title = title; b.addEventListener('click', fn); bar.appendChild(b); return b; };
    if (w.state === 'open') bar.appendChild(slotSelect(w));
    mk(IC.pin + (w.lifecycle === 'pinned' ? 'Pinned' : 'Pin'), w.lifecycle === 'pinned' ? 'Pinned: meant to stay. Click to unpin.' : 'Keep this panel after the task is done', function () { userPatch(w.id, { pinned: w.lifecycle !== 'pinned' }); });
    // Global: shown in every project (and, as a chip, in the top bar everywhere).
    // Going back is only offered from the project it came from.
    if (!w.global) {
      mk(IC.globe + 'Make global', 'Show this panel in every project. As a top bar chip it then stays there across projects and chats.', function () {
        api('/api/widgets/scope', { project: project, id: w.id, global: true }).then(function (r) {
          if (!r.ok) return cfg.toast(r.error || 'Could not make it global', 'error');
          var n = (r.inPlace || []).length;
          cfg.toast('Now global: shown in every project' + (n ? '. It still reads ' + n + ' file' + (n === 1 ? '' : 's') + ' from this project' : ''));
        });
      });
    } else if (!w.home || w.home === pkey(project)) {
      mk(IC.globe + 'Global', 'Global: shown in every project. Click to keep it in this project only.', function () {
        api('/api/widgets/scope', { project: project, id: w.id, global: false }).then(function (r) {
          if (!r.ok) return cfg.toast(r.error || 'Could not change it', 'error');
          cfg.toast('Back in this project only');
        });
      }, 'primary');
    } else {
      var from = el('span', 'wg-badge', 'global · from ' + w.home); from.title = 'A global panel. Open the project it came from to make it project-only again.'; bar.appendChild(from);
    }
    mk(IC.refresh, 'Reload', function () { showPane(''); reload(w.id); });
    mk(IC.eye + 'Checks', 'The screenshots from Claude\'s last check of this panel', function () { showPane(tabState.pane === 'checks' ? '' : 'checks'); });
    mk(IC.history + 'History', 'Earlier versions', function () { showPane(tabState.pane === 'history' ? '' : 'history'); });
    if (w.state === 'open') mk('Close', 'Close this panel (it can be reopened)', function () { userPatch(w.id, { closed: true }); });
    else mk('Reopen', 'Show this panel again', function () { userPatch(w.id, { closed: false }); }, 'primary');
    mk(IC.trash, 'Delete this panel and its files', function () {
      if (!window.confirm('Delete "' + w.title + '" and its source files? This cannot be undone.')) return;
      api('/api/widgets/delete', { project: project, id: w.id }).then(function (r) { if (!r.ok) cfg.toast(r.error || 'Could not delete', 'error'); });
    }, 'danger');
  }

  function showPane(pane) {
    tabState.pane = pane;
    var host = tabState.el; if (!host) return;
    var stage = host.querySelector('.wg-tab-stage'); if (!stage) return;
    var w = byId(tabState.selected);
    if (tabState.view) { tabState.view.destroy(); tabState.view = null; }
    stage.innerHTML = '';
    if (!w) return;
    if (pane === 'checks') {
      var side = el('div', 'wg-tab-side');
      side.appendChild(el('div', '', '<b>What Claude saw when it last checked this panel.</b>'));
      var shots = shotsNode(project, w.id);
      side.appendChild(shots);
      var none = el('div', '', 'No check has been run yet. Claude renders a panel with widget_render before calling it done.');
      none.style.color = 'var(--text-muted)';
      side.appendChild(none);
      setTimeout(function () { if (shots.isConnected && shots.children.length) none.remove(); }, 1500);
      stage.appendChild(side);
      return;
    }
    if (pane === 'history') {
      var box = el('div', 'wg-tab-side');
      stage.appendChild(box);
      api('/api/widgets/versions' + qs(project, w.id)).then(function (d) {
        var list = (d.ok && d.versions) || [];
        if (!list.length) { box.appendChild(el('div', '', 'No versions yet.')); return; }
        list.forEach(function (ver, i) {
          var row = el('div', 'wg-ver');
          row.appendChild(el('span', '', '<b>v' + ver.version + '</b>'));
          var when = el('span', ''); when.textContent = new Date(ver.at).toLocaleString(); when.style.flex = '1'; row.appendChild(when);
          if (i === 0) row.appendChild(el('span', 'wg-badge', 'current'));
          else {
            var b = el('button', 'wg-btn', 'Restore');
            b.addEventListener('click', function () {
              api('/api/widgets/rollback', { project: project, id: w.id, version: ver.version }).then(function (r) {
                if (!r.ok) return cfg.toast(r.error || 'Could not restore', 'error');
                cfg.toast('Restored v' + ver.version); showPane('');
              });
            });
            row.appendChild(b);
          }
          box.appendChild(row);
        });
      });
      return;
    }
    if (w.state !== 'open') {
      stage.appendChild(el('div', 'wg-msg', IC.panel + '<div>This panel is closed.</div><div style="font-size:12px">Reopen it to see it here and wherever it was placed.</div>'));
      return;
    }
    tabState.view = createView(stage, { project: project, id: w.id, frame: 'tab' });
  }

  function selectInTab(id) {
    tabState.selected = id;
    tabState.pane = '';
    renderTabList();
    showPane('');
  }

  function mountTab(host) {
    injectStyles();
    tabState.el = host;
    if (tabState.view) { tabState.view.destroy(); tabState.view = null; }
    if (!project && !widgets.length) { host.innerHTML = '<div class="wg-msg">' + IC.panel + '<div>No global panels yet.</div><div style="max-width:340px;font-size:12px">Select a project to see its panels. Any of them can be made global from here, to show in every project.</div></div>'; return; }
    if (!widgets.length) {
      host.innerHTML = '<div class="wg-msg">' + IC.panel + '<div><b style="color:var(--text-primary)">No panels yet</b></div>'
        + '<div style="max-width:360px">Claude builds these when something is better seen than read: progress on a long task, a table from a database, a build board. Ask for one, or let it decide.</div></div>';
      return;
    }
    host.innerHTML = '<div class="wg-tab"><div class="wg-tab-list"></div><div class="wg-tab-main"><div class="wg-tab-bar"></div><div class="wg-tab-stage"></div></div></div>';
    renderTabList();
    showPane(tabState.pane);
  }

  function unmountTab() {
    if (tabState.view) { tabState.view.destroy(); tabState.view = null; }
    tabState.el = null;
  }

  // ─── Server events ───

  function handleEvent(d) {
    // Global panels show in every project, so their events are everyone's.
    var G = d && d.project === '_global';
    if (!d || (!G && d.project !== pkey(project))) return;
    if (d.kind === 'data') {
      views.forEach(function (v) { if (v.id === d.id && (G || v.pkey === d.project)) v.sendData(d.source); });
      var w = byId(d.id);
      if (w && placeOf(w) === 'chip' && (!d.source || !w.chip || w.chip.source === d.source)) loadChipValue(w);
      return;
    }
    if (d.kind === 'store') {
      // One copy of the panel saved something: hand every copy the store.
      views.forEach(function (v) {
        if (v.id !== d.id || !(G || v.pkey === d.project) || !v.started) return;
        api('/api/widgets/data' + qs(v.project, v.id, '&storeOnly=1')).then(function (r) { if (r.ok && !v.destroyed) v.post({ t: 'store', store: r.store || {} }); });
      });
      return;
    }
    if (d.kind === 'doc') {
      views.forEach(function (v) { if (v.id === d.id && (G || v.pkey === d.project)) v.load(); });
      refresh();
      return;
    }
    if (d.kind === 'render') { if (tabState.pane === 'checks' && tabState.selected === d.id) showPane('checks'); return; }
    refresh().then(function () { if (tabState.el && tabState.selected === d.id) showPane(tabState.pane); });
  }

  window.CrundiWidgets = {
    configure: function (o) { for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) cfg[k] = o[k]; injectStyles(); },
    setProject: function (alias) {
      if (alias === project) return Promise.resolve();
      project = alias || '';
      // A global panel belongs to every project, so its chip is not torn down
      // when the project changes: it is a running frame, and whatever it was
      // doing (a timer counting, a sound about to play) would be lost with it.
      Object.keys(chipNodes).forEach(function (id) { var w = byId(id); if (!w || !w.global) dropChip(id); });
      widgets = widgets.filter(function (w) { return w.global; }); cellSig = null; chipValues = {}; closePop(); tabState.selected = ''; return refresh(); },
    refresh: refresh,
    list: function () { return widgets.slice(); },
    /** Ids of the widgets that are workbench panes right now. */
    cellIds: function () { return widgets.filter(function (w) { return placeOf(w) === 'cell'; }).map(function (w) { return w.id; }); },
    get: byId,
    handleEvent: handleEvent,
    cellHeadHtml: cellHeadHtml,
    mountCell: mountCell,
    destroyIn: function (root) { views.slice().forEach(function (v) { if (root.contains(v.root)) v.destroy(); }); },
    syncDock: syncDock,
    inlineNode: inlineNode,
    shotsNode: shotsNode,
    mediaNode: mediaNode,
    setChipHost: function (n) { chipEl = n; renderChips(); },
    mountPicker: mountPicker,
    mountTab: mountTab,
    unmountTab: unmountTab,
    selectInTab: function (id) { tabState.selected = id; tabState.pane = ''; },
    reload: reload,
    close: function (id) { return userPatch(id, { closed: true }); },
    userPatch: userPatch,
    SLOT_LABEL: SLOT_LABEL
  };
})();
