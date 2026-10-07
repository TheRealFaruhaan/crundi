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

  var cfg = { apiFetch: null, toast: function () {}, isMobile: function () { return false; }, liveChats: function () { return []; }, onChange: function () {} };
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
      '.wg-zoom{position:fixed;inset:0;z-index:1200;background:rgba(0,0,0,.82);display:flex;align-items:center;justify-content:center;padding:16px;cursor:zoom-out}',
      '.wg-zoom img{max-width:100%;max-height:100%;border-radius:8px;border:1px solid var(--border)}',
      // chips
      // In the desktop app the whole top bar is a window-drag area, and a drag
      // area swallows clicks: a chip there could be seen but never pressed.
      // no-drag gives it back to the pointer. A browser ignores the property.
      '.wg-chips{display:inline-flex;gap:6px;align-items:center;min-width:0;position:relative;z-index:2;-webkit-app-region:no-drag}',
      '.wg-chip{-webkit-app-region:no-drag}',
      '.wg-chip{display:inline-flex;align-items:center;gap:6px;max-width:220px;padding:3px 9px;border-radius:99px;border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:11.5px;cursor:pointer;white-space:nowrap}',
      '.wg-chip:hover{border-color:var(--accent)}',
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
      return api('/api/widgets/doc' + qs(v.project, v.id)).then(function (d) {
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
        });
      } else if (m.t === 'height') {
        v.height = Number(m.h) || 0;
        v.applyHeight();
      } else if (m.t === 'fault') {
        // A handful is a diagnosis; a flood is a loop. Stop reporting after a few.
        if (v.faults++ < 5) api('/api/widgets/fault', { project: v.project, id: v.id, message: String(m.message || '').slice(0, 500), where: String(m.where || '').slice(0, 120) });
      } else if (m.t === 'call') {
        v.handleCall(m);
      }
    };

    v.handleCall = function (m) {
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
    if (!p || !cfg.apiFetch) { widgets = []; cfg.onChange(); return Promise.resolve(); }
    return api('/api/widgets?project=' + encodeURIComponent(p)).then(function (d) {
      if (mine !== loadSeq) return;
      widgets = d.ok ? (d.widgets || []) : [];
      cfg.onChange();
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

  function mountCell(cellEl, id) {
    var body = cellEl.querySelector('.wg-cell-body');
    if (!body) return null;
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
        var collapse = el('button', 'wg-ibtn', IC.up);
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
        head.appendChild(title);
        head.appendChild(el('span', '', '')).setAttribute('data-wg-badge', '1');
        head.appendChild(slotSelect(w));
        head.appendChild(collapse);
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
        var cb = item.querySelectorAll('.wg-ibtn')[0];
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
          var z = el('div', 'wg-zoom'); var big = document.createElement('img'); big.src = img.src; big.alt = img.alt;
          z.appendChild(big); z.addEventListener('click', function () { z.remove(); });
          document.body.appendChild(z);
        });
        fig.appendChild(img);
        fig.appendChild(document.createTextNode(s.frame + (s.state !== 'live' ? ' · ' + s.state : '')));
        row.appendChild(fig);
      });
    });
    return row;
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

  function paintChips() {
    if (!chipEl) return;
    var list = widgets.filter(function (w) { return placeOf(w) === 'chip'; });
    chipEl.innerHTML = '';
    chipEl.style.display = list.length ? '' : 'none';
    list.forEach(function (w) {
      var b = el('button', 'wg-chip', IC.panel);
      b.title = w.title;
      var label = (w.chip && w.chip.label) || w.title;
      var l = el('span', 'l'); l.textContent = label;
      b.appendChild(l);
      if (chipValues[w.id]) { var vEl = el('span', 'v'); vEl.textContent = chipValues[w.id]; b.appendChild(vEl); }
      b.addEventListener('click', function () { openPop(w.id); });
      chipEl.appendChild(b);
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
    if (!project) { host.innerHTML = '<div class="wg-msg">' + IC.panel + '<div>Select a project to see its panels.</div></div>'; return; }
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
    if (!d || d.project !== pkey(project)) return;
    if (d.kind === 'data') {
      views.forEach(function (v) { if (v.id === d.id && v.pkey === d.project) v.sendData(d.source); });
      var w = byId(d.id);
      if (w && placeOf(w) === 'chip' && (!d.source || !w.chip || w.chip.source === d.source)) loadChipValue(w);
      return;
    }
    if (d.kind === 'doc') {
      views.forEach(function (v) { if (v.id === d.id && v.pkey === d.project) v.load(); });
      refresh();
      return;
    }
    if (d.kind === 'render') { if (tabState.pane === 'checks' && tabState.selected === d.id) showPane('checks'); return; }
    refresh().then(function () { if (tabState.el && tabState.selected === d.id) showPane(tabState.pane); });
  }

  window.CrundiWidgets = {
    configure: function (o) { for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) cfg[k] = o[k]; injectStyles(); },
    setProject: function (alias) { if (alias === project) return Promise.resolve(); project = alias || ''; widgets = []; chipValues = {}; closePop(); tabState.selected = ''; return refresh(); },
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
