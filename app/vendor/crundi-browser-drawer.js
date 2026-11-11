/**
 * crundi-browser-drawer.js — the PC client's browser drawer (web UI side).
 *
 * A drawer that slides in at the right of the project area (below the app's
 * top bar). The project area narrows to make room; a divider sets the split.
 * Each project has its own tabs; switching project shows that project's.
 *
 * Every tab is an Electron <webview> in .brd-views, created once and NEVER
 * moved in the DOM: moving one reloads its page. Hidden tabs, other projects'
 * tabs and a closed drawer are not display:none either (that shrinks a page
 * to 0x0 and stops it rendering, so a chat could not use it): they stay laid
 * out at full size with opacity 0 and no pointer events. The closed drawer
 * keeps its last width, out of the flow, behind the page.
 *
 * The main process (app/browser-drawer.js) holds each tab's webContents and
 * the debugger; the server's chats reach a tab through it and ask this file,
 * via onRequest, to list, open, show, close, resize, draw on or read a tab.
 *
 * Lives outside webapp-html.js on purpose (one template literal there; a
 * stray backslash in browser JS breaks the whole page).
 *
 *   CrundiBrowserDrawer.init(host) → drawer
 *   host: { root, split, toggleBtn, api, getProject, ic, esc, toast,
 *           onLayout, listChats(project), sendToChat(chatId, text, pngB64) }
 *   drawer: { setProject(alias), toggle(), open(), close(), isOpen() }
 */
(function () {
  'use strict';

  var PARTITIONS = { client: 'persist:crundi-browser', server: 'persist:crundi-browser-srv' };
  var START = 'https://www.google.com';
  var MAX_TABS = 12;
  var MIN_W = 320, MIN_AREA = 420, DEFAULT_W = 560;
  var STORE = 'crundi_brd';

  var DEVICES = [
    { name: 'Responsive', responsive: true },
    { name: 'iPhone SE', w: 375, h: 667, dpr: 2, mobile: true, ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1' },
    { name: 'iPhone 12 Pro', w: 390, h: 844, dpr: 3, mobile: true, ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1' },
    { name: 'iPhone 15 Pro', w: 393, h: 852, dpr: 3, mobile: true, ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1' },
    { name: 'iPhone 16 Pro', w: 393, h: 852, dpr: 3, mobile: true, ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1' },
    { name: 'iPhone 16 Pro Max', w: 430, h: 932, dpr: 3, mobile: true, ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1' },
    { name: 'iPad Mini', w: 768, h: 1024, dpr: 2, mobile: true, ua: 'Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1' },
    { name: 'iPad Air', w: 820, h: 1180, dpr: 2, mobile: true, ua: 'Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1' },
    { name: 'iPad Pro', w: 1024, h: 1366, dpr: 2, mobile: true, ua: 'Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1' },
    { name: 'Pixel 5', w: 393, h: 851, dpr: 2.75, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 11; Pixel 5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.91 Mobile Safari/537.36' },
    { name: 'Pixel 7', w: 412, h: 915, dpr: 2.625, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36' },
    { name: 'Pixel 8 Pro', w: 430, h: 932, dpr: 2.625, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36' },
    { name: 'Galaxy S22', w: 360, h: 800, dpr: 3, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 13; SM-S901B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36' },
    { name: 'Galaxy S24 Ultra', w: 412, h: 915, dpr: 2.625, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 14; SM-S928B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36' },
    { name: 'Galaxy Z Fold 5', w: 344, h: 882, dpr: 2.8, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 13; SM-F946B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Mobile Safari/537.36' },
    { name: 'Galaxy Z Flip 5', w: 264, h: 844, dpr: 2.8, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 13; SM-F731B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Mobile Safari/537.36' },
    { name: 'Surface Pro 9', w: 1200, h: 1800, dpr: 2, mobile: true, ua: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36' },
    { name: 'Desktop 1280', w: 1280, h: 800, dpr: 1, mobile: false, ua: '' },
    { name: 'Full HD 1920', w: 1920, h: 1080, dpr: 1, mobile: false, ua: '' },
  ];

  var CSS = [
    '.main-layout { position: relative; }',
    '.brd-split { flex: 0 0 5px; cursor: col-resize; background: var(--border); position: relative; z-index: 3; }',
    '.brd-split:hover, .brd-split.on { background: var(--accent); }',
    '.brd { flex: 0 0 var(--brd-w, 560px); width: var(--brd-w, 560px); min-width: 0; display: flex; flex-direction: column; background: var(--bg-secondary); overflow: hidden; position: relative; }',
    // Closed: out of the flow, same size, behind the page and invisible, so its pages keep rendering.
    '.brd.brd-closed { position: absolute; top: 0; right: 0; bottom: 0; opacity: 0; pointer-events: none; z-index: 0; }',
    '.brd-split.brd-closed { display: none; }',
    '.brd-tabs { display: flex; align-items: center; gap: 2px; padding: 4px 6px 0; background: var(--bg-primary); border-bottom: 1px solid var(--border); flex: 0 0 auto; min-height: 34px; overflow-x: auto; scrollbar-width: none; }',
    '.brd-tab { display: inline-flex; align-items: center; gap: 6px; max-width: 200px; min-width: 64px; height: 29px; padding: 0 6px 0 10px; border: 1px solid transparent; border-bottom: none; border-radius: 6px 6px 0 0; background: transparent; color: var(--text-secondary); cursor: pointer; font-size: 0.76rem; flex: 0 1 auto; }',
    '.brd-tab:hover { background: var(--bg-tertiary); }',
    '.brd-tab.on { background: var(--bg-secondary); border-color: var(--border); color: var(--text-primary); }',
    '.brd-tab .brd-t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1 1 auto; }',
    '.brd-tab .brd-x { border: none; background: none; color: inherit; cursor: pointer; width: 18px; height: 18px; border-radius: 4px; padding: 0; display: inline-flex; align-items: center; justify-content: center; opacity: 0.6; }',
    '.brd-tab .brd-x:hover { opacity: 1; background: var(--bg-primary); }',
    '.brd-net-dot { font-size: 0.6rem; font-weight: 700; letter-spacing: 0.04em; padding: 1px 4px; border-radius: 3px; flex: 0 0 auto; }',
    '.brd-net-dot.client { color: var(--text-muted); border: 1px solid var(--border); }',
    '.brd-net-dot.server { color: #fff; background: var(--accent); }',
    '.brd-net-dot.bad { background: var(--red, #ef4444); }',
    '.brd-ibtn { display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px; flex: 0 0 auto; border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary); border-radius: 6px; cursor: pointer; padding: 0; }',
    '.brd-ibtn:hover:not(:disabled) { border-color: var(--accent); color: var(--text-primary); }',
    '.brd-ibtn:disabled { opacity: 0.35; cursor: default; }',
    '.brd-ibtn.on { background: var(--accent); border-color: var(--accent); color: #fff; }',
    '.brd-ibtn .ic, .brd-x .ic { width: 14px; height: 14px; }',
    '.brd-tabs .brd-ibtn { width: 26px; height: 26px; border: none; background: none; margin-bottom: 2px; }',
    '.brd-tabs .brd-sp { flex: 1 1 auto; }',
    '.brd-bar, .brd-annbar { display: flex; align-items: center; gap: 5px; padding: 6px 8px; border-bottom: 1px solid var(--border); flex: 0 0 auto; }',
    '.brd-url { flex: 1 1 auto; min-width: 60px; height: 28px; padding: 0 12px; border: 1px solid var(--border); background: var(--bg-primary); color: var(--text-primary); border-radius: 999px; font-size: 0.78rem; font-family: var(--mono); outline: none; }',
    '.brd-url:focus { border-color: var(--accent); }',
    '.brd-sel { height: 28px; max-width: 130px; border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary); border-radius: 6px; font-size: 0.72rem; cursor: pointer; padding: 0 4px; flex: 0 1 auto; min-width: 0; }',
    '.brd-sel.brd-netsel.server { border-color: var(--accent); color: var(--text-primary); }',
    '.brd-annbar { background: var(--bg-tertiary); font-size: 0.74rem; color: var(--text-secondary); flex-wrap: wrap; }',
    '.brd-annbar[hidden] { display: none; }',
    '.brd-annbar .brd-btn { height: 26px; padding: 0 10px; border: 1px solid var(--border); background: var(--bg-primary); color: var(--text-primary); border-radius: 6px; cursor: pointer; font-size: 0.74rem; }',
    '.brd-annbar .brd-btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }',
    '.brd-annbar .brd-btn:disabled { opacity: 0.4; cursor: default; }',
    '.brd-stage { flex: 1 1 auto; min-height: 0; position: relative; overflow: hidden; background-color: var(--bg-primary); background-image: radial-gradient(var(--border) 1px, transparent 1px); background-size: 16px 16px; }',
    '.brd-views { position: absolute; inset: 0; }',
    // Every <webview>: laid out, rendering; only the shown one is visible and takes input.
    '.brd-wv { position: absolute; left: 0; top: 0; width: 100%; height: 100%; opacity: 0; pointer-events: none; z-index: 0; background: #fff; }',
    '.brd-wv.on { opacity: 1; pointer-events: auto; z-index: 1; }',
    'body.brd-dragging .brd-wv, body.brd-dragging iframe { pointer-events: none !important; }',
    '.brd-ann { position: absolute; z-index: 2; pointer-events: none; overflow: hidden; }',
    '.brd-ann.drawing { pointer-events: auto; cursor: crosshair; }',
    '.brd-ann svg { position: absolute; inset: 0; width: 100%; height: 100%; overflow: visible; }',
    '.brd-lbl { position: absolute; max-width: 240px; padding: 3px 7px; border-radius: 6px; font: 12px/1.35 system-ui, sans-serif; color: #fff; white-space: pre-wrap; word-break: break-word; box-shadow: 0 2px 6px rgba(0,0,0,0.35); pointer-events: none; }',
    '.brd-lbl.you { background: #ea580c; } .brd-lbl.claude { background: #6366f1; }',
    '.brd-lbl .brd-who { font-weight: 700; opacity: 0.85; margin-right: 4px; }',
    '.brd-note { position: absolute; z-index: 4; display: flex; gap: 4px; padding: 4px; background: var(--bg-secondary); border: 1px solid var(--accent); border-radius: 8px; box-shadow: 0 4px 14px rgba(0,0,0,0.4); pointer-events: auto; }',
    '.brd-note input { width: 220px; height: 26px; border: 1px solid var(--border); background: var(--bg-primary); color: var(--text-primary); border-radius: 5px; padding: 0 8px; font-size: 0.78rem; outline: none; }',
    '.brd-empty, .brd-diag { position: absolute; inset: 0; display: flex; flex-direction: column; gap: 10px; align-items: center; justify-content: center; color: var(--text-muted); font-size: 0.82rem; text-align: center; padding: 24px; z-index: 3; }',
    '.brd-empty[hidden], .brd-diag[hidden] { display: none; }',
    '.brd-diag { pointer-events: none; z-index: 1; }',
    '.brd-empty .brd-btn { height: 30px; padding: 0 14px; border: 1px solid var(--accent); background: var(--accent); color: #fff; border-radius: 6px; cursor: pointer; }',
    '.brd.brd-loading .brd-ibtn[data-brd="reload"] .ic { animation: brd-spin 0.8s linear infinite; }',
    '@keyframes brd-spin { to { transform: rotate(360deg); } }',
  ].join('\n');

  function uid() { return Date.now().toString(36) + Math.random().toString(36).slice(2, 8); }
  function clamp(v, a, b) { return Math.max(a, Math.min(b, v)); }

  function init(host) {
    var api = host.api;
    var ic = host.ic || function () { return ''; };
    var esc = host.esc || function (s) { return String(s); };
    var toast = host.toast || function () {};
    var root = host.root, split = host.split;

    // ─── State (persisted) ───
    var st = { open: false, width: DEFAULT_W, projects: {} };
    try { var saved = JSON.parse(localStorage.getItem(STORE) || 'null'); if (saved && typeof saved === 'object') st = Object.assign(st, saved); } catch (e) { /* fresh */ }
    if (!st.projects || typeof st.projects !== 'object') st.projects = {};
    function persist() {
      var out = { open: st.open, width: st.width, projects: {} };
      Object.keys(st.projects).forEach(function (p) {
        var ps = st.projects[p];
        out.projects[p] = { active: ps.active, tabs: ps.tabs.map(function (t) { return { id: t.id, url: t.url, title: t.title, device: t.device, custom: t.custom || null, network: t.network }; }) };
      });
      try { localStorage.setItem(STORE, JSON.stringify(out)); } catch (e) { /* full */ }
    }
    function proj(p) {
      if (!st.projects[p]) st.projects[p] = { active: null, tabs: [] };
      var ps = st.projects[p];
      if (!Array.isArray(ps.tabs)) ps.tabs = [];
      return ps;
    }

    var project = null;
    var runtime = {};   // tabId → { wv, attached: Promise, ready: bool, state, ann: [], scale, box }
    var info = { mode: 'local', status: 'direct', labels: { client: 'This PC', server: 'Server' } };
    var annotating = false, annTool = 'box';

    function findTab(tabId) {
      var ps = Object.keys(st.projects);
      for (var i = 0; i < ps.length; i++) {
        var t = st.projects[ps[i]].tabs.find(function (x) { return x.id === tabId; });
        if (t) return { tab: t, project: ps[i] };
      }
      return null;
    }
    function activeTab() {
      if (!project) return null;
      var ps = proj(project);
      return ps.tabs.find(function (t) { return t.id === ps.active; }) || ps.tabs[0] || null;
    }

    // ─── DOM ───
    var style = document.createElement('style'); style.textContent = CSS; document.head.appendChild(style);
    root.innerHTML = ''
      + '<div class="brd-tabs" role="tablist"></div>'
      + '<div class="brd-bar">'
      + '<button class="brd-ibtn" data-brd="back" title="Back" disabled>' + ic('chevron-left') + '</button>'
      + '<button class="brd-ibtn" data-brd="forward" title="Forward" disabled>' + ic('chevron-right') + '</button>'
      + '<button class="brd-ibtn" data-brd="reload" title="Reload">' + ic('refresh') + '</button>'
      + '<input class="brd-url" type="text" spellcheck="false" autocomplete="off" placeholder="Enter URL or search">'
      + '<select class="brd-sel brd-netsel" title="Which network this tab uses"><option value="client">This PC</option><option value="server">Server</option></select>'
      + '<select class="brd-sel brd-dev" title="Emulate device"></select>'
      + '<button class="brd-ibtn" data-brd="annotate" title="Annotate the page">' + ic('message-square') + '</button>'
      + '<button class="brd-ibtn" data-brd="devtools" title="DevTools">' + ic('terminal') + '</button>'
      + '</div>'
      + '<div class="brd-annbar" hidden>'
      + '<button class="brd-ibtn" data-tool="pin" title="Pin a note">' + ic('pin') + '</button>'
      + '<button class="brd-ibtn on" data-tool="box" title="Draw a box">' + ic('square') + '</button>'
      + '<button class="brd-ibtn" data-tool="pen" title="Draw freehand">' + ic('pencil') + '</button>'
      + '<span class="brd-count"></span>'
      + '<span style="flex:1 1 auto"></span>'
      + '<select class="brd-sel brd-chat" title="Chat to send to"></select>'
      + '<button class="brd-btn primary" data-ann="send" title="Attach a screenshot with these marks, and what they point at, to the chat">Send to chat</button>'
      + '<button class="brd-btn" data-ann="clear" title="Remove your marks">Clear</button>'
      + '<button class="brd-btn" data-ann="done">Done</button>'
      + '</div>'
      + '<div class="brd-stage"><div class="brd-views"></div><div class="brd-ann"><svg xmlns="http://www.w3.org/2000/svg"></svg></div>'
      + '<div class="brd-diag" hidden></div>'
      + '<div class="brd-empty" hidden><div>No browser tabs in this project yet.</div><button class="brd-btn" data-brd="new">Open a tab</button></div></div>';
    var tabsEl = root.querySelector('.brd-tabs');
    var urlEl = root.querySelector('.brd-url');
    var netSel = root.querySelector('.brd-netsel');
    var devSel = root.querySelector('.brd-dev');
    var annBar = root.querySelector('.brd-annbar');
    var chatSel = root.querySelector('.brd-chat');
    var stage = root.querySelector('.brd-stage');
    var views = root.querySelector('.brd-views');
    var annEl = root.querySelector('.brd-ann');
    var svg = annEl.querySelector('svg');
    var diagEl = root.querySelector('.brd-diag');
    var emptyEl = root.querySelector('.brd-empty');
    devSel.innerHTML = DEVICES.map(function (d, i) { return '<option value="' + i + '">' + esc(d.responsive ? 'Responsive' : d.name + '  ' + d.w + 'x' + d.h) + '</option>'; }).join('') + '<option value="custom" hidden>Custom</option>';

    // ─── Open / close / width ───
    // st.width is the person's choice and is kept as chosen; what is drawn is
    // that, narrowed only while the window cannot fit it (and never derived
    // from a layout that has no size yet, as at startup).
    function maxWidth() {
      var lay = root.parentNode;
      var lw = lay ? lay.clientWidth : 0;
      return lw > 0 ? Math.max(MIN_W, lw - MIN_AREA) : Infinity;
    }
    function applyWidth() {
      st.width = Math.max(MIN_W, Math.round(st.width || DEFAULT_W));
      var w = Math.min(st.width, maxWidth());
      root.style.setProperty('--brd-w', w + 'px');
    }
    function applyOpen() {
      root.classList.toggle('brd-closed', !st.open);
      split.classList.toggle('brd-closed', !st.open);
      root.setAttribute('aria-hidden', st.open ? 'false' : 'true');
      if (host.toggleBtn) host.toggleBtn.classList.toggle('active', st.open);
      if (!st.open) {
        setAnnotating(false);
        // Keyboard focus must not stay in an invisible page.
        var a = document.activeElement; if (a && root.contains(a)) { try { a.blur(); } catch (e) { /* ignore */ } }
      }
      applyWidth();
      layout();
      if (host.onLayout) host.onLayout();
    }
    function open() { if (!st.open) { st.open = true; persist(); applyOpen(); } if (project && !proj(project).tabs.length) newTab(START, 'client', true); else showActive(); }
    function close() { if (st.open) { st.open = false; persist(); applyOpen(); } }

    split.addEventListener('pointerdown', function (e) {
      if (e.button !== 0) return;
      e.preventDefault();
      var startX = e.clientX, startW = Math.min(st.width, maxWidth());
      // A full-window shield: the pages under the pointer would swallow the moves.
      var shield = document.createElement('div');
      shield.style.cssText = 'position:fixed;inset:0;z-index:2147483646;cursor:col-resize';
      document.body.appendChild(shield);
      document.body.classList.add('brd-dragging'); split.classList.add('on');
      function move(ev) { st.width = clamp(startW + (startX - ev.clientX), MIN_W, maxWidth()); applyWidth(); layout(); }
      function up() {
        shield.remove(); document.body.classList.remove('brd-dragging'); split.classList.remove('on');
        window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up);
        persist(); if (host.onLayout) host.onLayout();
      }
      window.addEventListener('pointermove', move); window.addEventListener('pointerup', up);
    });
    split.addEventListener('dblclick', function () { st.width = DEFAULT_W; applyWidth(); layout(); persist(); if (host.onLayout) host.onLayout(); });

    // ─── Tabs ───
    function netLabel(n) { return n === 'server' ? (info.labels && info.labels.server) || 'Server' : (info.labels && info.labels.client) || 'This PC'; }
    function serverBad() { return info.mode === 'remote' && info.status !== 'tunnel'; }
    function renderTabs() {
      var ps = project ? proj(project) : { tabs: [] };
      var act = activeTab();
      var h = ps.tabs.map(function (t) {
        var net = t.network === 'server' ? '<span class="brd-net-dot server' + (serverBad() ? ' bad' : '') + '" title="' + esc('Network: ' + netLabel('server')) + '">SRV</span>' : '';
        return '<div class="brd-tab' + (act && act.id === t.id ? ' on' : '') + '" role="tab" data-tab="' + esc(t.id) + '" title="' + esc((t.title || t.url || '') + '\n' + (t.url || '') + '\nNetwork: ' + netLabel(t.network)) + '">'
          + net + '<span class="brd-t">' + esc(t.title || t.url || 'New tab') + '</span>'
          + '<button class="brd-x" data-close="' + esc(t.id) + '" title="Close tab">' + ic('x') + '</button></div>';
      }).join('');
      tabsEl.innerHTML = h
        + '<button class="brd-ibtn" data-brd="new" title="New tab">' + ic('plus') + '</button>'
        + '<span class="brd-sp"></span>'
        + '<button class="brd-ibtn" data-brd="hide" title="Hide the browser (pages keep running)">' + ic('chevron-right') + '</button>';
      emptyEl.hidden = !!ps.tabs.length;
    }
    function renderBar() {
      var t = activeTab();
      var r = t && runtime[t.id];
      var s = (r && r.state) || {};
      root.querySelector('[data-brd="back"]').disabled = !s.canGoBack;
      root.querySelector('[data-brd="forward"]').disabled = !s.canGoForward;
      root.classList.toggle('brd-loading', !!s.loading);
      if (document.activeElement !== urlEl) urlEl.value = t ? (t.url && t.url !== 'about:blank' ? t.url : '') : '';
      netSel.value = t ? (t.network || 'client') : 'client';
      netSel.classList.toggle('server', !!t && t.network === 'server');
      netSel.options[0].textContent = netLabel('client');
      netSel.options[1].textContent = netLabel('server');
      netSel.title = t ? 'This tab uses: ' + netLabel(t.network) + (t.network === 'server' ? ' (its traffic goes through the server)' : '') : '';
      devSel.value = t ? (t.device === 'custom' ? 'custom' : String(t.device || 0)) : '0';
      var custom = devSel.querySelector('option[value="custom"]');
      custom.hidden = !(t && t.device === 'custom');
      if (t && t.custom) custom.textContent = 'Custom  ' + t.custom.w + 'x' + t.custom.h;
      var diag = r && r.diag;
      diagEl.hidden = !diag; diagEl.textContent = diag || '';
      [netSel, devSel, urlEl].forEach(function (x) { x.disabled = !t; });
      root.querySelector('[data-brd="annotate"]').classList.toggle('on', annotating);
    }

    function newTab(url, network, activate, opts) {
      opts = opts || {};
      var p = opts.project || project;
      if (!p) { toast('Select a project first', 'error'); return null; }
      var ps = proj(p);
      if (ps.tabs.length >= MAX_TABS) { if (!opts.quiet) toast('At most ' + MAX_TABS + ' tabs per project', 'error'); return null; }
      var t = { id: uid(), url: url || 'about:blank', title: '', device: 0, network: network === 'server' ? 'server' : 'client' };
      ps.tabs.push(t);
      if (activate || !ps.active) ps.active = t.id;
      persist();
      var r = ensure(t, p, { load: opts.load !== false });
      if (p === project) { renderTabs(); showActive(); }
      return { tab: t, rt: r };
    }

    // Make the tab's <webview> if it has none. Never moves an existing one.
    function ensure(t, p, opts) {
      if (runtime[t.id] && runtime[t.id].wv) return runtime[t.id];
      opts = opts || {};
      var r = runtime[t.id] = runtime[t.id] || { ann: [], state: {} };
      var wv = document.createElement('webview');
      wv.className = 'brd-wv';
      wv.setAttribute('partition', PARTITIONS[t.network] || PARTITIONS.client);
      wv.setAttribute('src', 'about:blank');
      wv.setAttribute('tabindex', '-1');
      // Links that open a window become drawer tabs (the main process refuses the window itself).
      wv.setAttribute('allowpopups', '');
      wv.dataset.tab = t.id;
      r.wv = wv; r.network = t.network;
      var load = opts.load !== false;
      r.attached = new Promise(function (resolve, reject) {
        wv.addEventListener('dom-ready', function once() {
          wv.removeEventListener('dom-ready', once);
          var id; try { id = wv.getWebContentsId(); } catch (e) { reject(e); return; }
          api.attach({ tabId: t.id, project: p, network: t.network, wcId: id }).then(function (res) {
            if (!res || !res.ok) { reject(new Error((res && res.error) || 'attach failed')); return; }
            r.ready = true; r.emuKey = '';
            layout();
            if (load && t.url && t.url !== 'about:blank') api.navigate(t.id, t.url);
            resolve();
          }, reject);
        });
      });
      r.attached.catch(function (err) { r.diag = 'This tab could not start: ' + (err && err.message || err); renderBar(); });
      views.appendChild(wv);
      layout();
      return r;
    }

    function closeTab(tabId) {
      var f = findTab(tabId); if (!f) return;
      var ps = proj(f.project);
      var i = ps.tabs.findIndex(function (x) { return x.id === tabId; });
      ps.tabs.splice(i, 1);
      if (ps.active === tabId) ps.active = ps.tabs[Math.max(0, i - 1)] ? ps.tabs[Math.max(0, i - 1)].id : null;
      var r = runtime[tabId];
      if (r && r.wv) { try { r.wv.remove(); } catch (e) { /* gone */ } }
      delete runtime[tabId];
      api.detach(tabId);
      persist();
      if (f.project === project) { renderTabs(); showActive(); }
    }

    function activate(tabId) {
      var f = findTab(tabId); if (!f) return;
      proj(f.project).active = tabId; persist();
      if (f.project === project) { renderTabs(); showActive(); }
    }

    // Show the current project's active tab; every other page stays laid out, invisible.
    function showActive() {
      var t = st.open ? activeTab() : null;
      if (t) ensure(t, project);
      Object.keys(runtime).forEach(function (id) { var r = runtime[id]; if (r.wv) r.wv.classList.toggle('on', !!t && id === t.id); });
      renderBar(); renderAnn(); layout();
    }

    // ─── Layout + device emulation ───
    function boxFor(t, sw, sh) {
      var dev = t.device === 'custom' ? (t.custom ? { w: t.custom.w, h: t.custom.h, dpr: 1, mobile: false, ua: '' } : null) : DEVICES[t.device || 0];
      if (!dev || dev.responsive) return { left: 0, top: 0, width: sw, height: sh, scale: 1, config: null, pageW: sw, pageH: sh };
      var pad = 10;
      var scale = Math.min(Math.max(40, sw - pad * 2) / dev.w, Math.max(40, sh - pad * 2) / dev.h);
      var bw = Math.round(dev.w * scale), bh = Math.round(dev.h * scale);
      return { left: Math.round((sw - bw) / 2), top: Math.round((sh - bh) / 2), width: bw, height: bh, scale: scale, pageW: dev.w, pageH: dev.h,
        config: { width: dev.w, height: dev.h, deviceScaleFactor: dev.dpr || 0, mobile: !!dev.mobile, ua: dev.ua || '' } };
    }
    function layout() {
      var sw = Math.max(2, Math.round(stage.clientWidth)), sh = Math.max(2, Math.round(stage.clientHeight));
      Object.keys(runtime).forEach(function (id) {
        var r = runtime[id]; if (!r.wv) return;
        var f = findTab(id); if (!f) return;
        var b = boxFor(f.tab, sw, sh);
        r.box = b;
        var s = r.wv.style;
        var px = function (v) { return v + 'px'; };
        if (s.left !== px(b.left)) s.left = px(b.left);
        if (s.top !== px(b.top)) s.top = px(b.top);
        if (s.width !== px(b.width)) s.width = px(b.width);
        if (s.height !== px(b.height)) s.height = px(b.height);
        if (r.ready) {
          var key = JSON.stringify(b.config) + '|' + b.scale.toFixed(3);
          if (key !== r.emuKey) { r.emuKey = key; api.emulate(id, b.config, b.scale); }
        }
      });
      positionAnn();
    }
    if (window.ResizeObserver) new ResizeObserver(function () { layout(); }).observe(stage);
    window.addEventListener('resize', function () { applyWidth(); layout(); });

    // ─── Annotations ───
    // Kept per tab in page CSS pixels (viewport), so they line up at any
    // device scale: overlay px = page px * box.scale.
    function positionAnn() {
      var t = activeTab(); var r = t && runtime[t.id];
      if (!st.open || !r || !r.box) { annEl.style.display = 'none'; return; }
      annEl.style.display = '';
      annEl.style.left = r.box.left + 'px'; annEl.style.top = r.box.top + 'px';
      annEl.style.width = r.box.width + 'px'; annEl.style.height = r.box.height + 'px';
      renderAnn();
    }
    var SVGNS = 'http://www.w3.org/2000/svg';
    function el(name, attrs) { var e = document.createElementNS(SVGNS, name); Object.keys(attrs).forEach(function (k) { e.setAttribute(k, attrs[k]); }); return e; }
    function colorOf(a) { return a.by === 'claude' ? '#6366f1' : '#ea580c'; }
    function renderAnn() {
      var t = activeTab(); var r = t && runtime[t.id];
      while (svg.firstChild) svg.removeChild(svg.firstChild);
      annEl.querySelectorAll('.brd-lbl').forEach(function (x) { x.remove(); });
      if (!r || !r.box) return;
      var k = r.box.scale || 1;
      var defs = el('defs', {});
      ['#6366f1', '#ea580c'].forEach(function (c, i) {
        var m = el('marker', { id: 'brd-arr' + i, viewBox: '0 0 10 10', refX: '9', refY: '5', markerWidth: '7', markerHeight: '7', orient: 'auto-start-reverse' });
        m.appendChild(el('path', { d: 'M0,0 L10,5 L0,10 z', fill: c })); defs.appendChild(m);
      });
      svg.appendChild(defs);
      r.ann.forEach(function (a, i) {
        var c = colorOf(a);
        if (a.kind === 'box') svg.appendChild(el('rect', { x: a.x * k, y: a.y * k, width: a.width * k, height: a.height * k, fill: c + '22', stroke: c, 'stroke-width': 2.5, rx: 3 }));
        else if (a.kind === 'arrow') svg.appendChild(el('line', { x1: a.x * k, y1: a.y * k, x2: a.toX * k, y2: a.toY * k, stroke: c, 'stroke-width': 3, 'marker-end': 'url(#brd-arr' + (a.by === 'claude' ? 0 : 1) + ')' }));
        else if (a.kind === 'pen' && a.points) svg.appendChild(el('polyline', { points: a.points.map(function (p) { return (p[0] * k) + ',' + (p[1] * k); }).join(' '), fill: 'none', stroke: c, 'stroke-width': 3, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }));
        else if (a.kind === 'pin') {
          var g = el('g', {});
          g.appendChild(el('circle', { cx: a.x * k, cy: a.y * k, r: 11, fill: c, stroke: '#fff', 'stroke-width': 2 }));
          var tx = el('text', { x: a.x * k, y: a.y * k + 4, 'text-anchor': 'middle', fill: '#fff', 'font-size': 11, 'font-weight': 700, 'font-family': 'system-ui, sans-serif' });
          tx.textContent = String(i + 1); g.appendChild(tx); svg.appendChild(g);
        }
        if (a.text || a.kind === 'label') {
          var lbl = document.createElement('div');
          lbl.className = 'brd-lbl ' + (a.by === 'claude' ? 'claude' : 'you');
          var ax = a.kind === 'box' ? a.x * k : (a.kind === 'arrow' ? a.x * k : a.x * k + 17);
          var ay = a.kind === 'box' ? a.y * k + a.height * k + 4 : (a.kind === 'arrow' ? a.y * k - 24 : a.y * k - 11);
          if (a.kind === 'pen' && a.points && a.points.length) { ax = a.points[a.points.length - 1][0] * k + 6; ay = a.points[a.points.length - 1][1] * k; }
          lbl.style.left = Math.max(0, Math.min(ax, r.box.width - 60)) + 'px';
          lbl.style.top = Math.max(0, Math.min(ay, r.box.height - 22)) + 'px';
          lbl.innerHTML = '<span class="brd-who">' + (i + 1) + (a.by === 'claude' ? ' Claude' : '') + '</span>' + esc(a.text || '');
          annEl.appendChild(lbl);
        }
      });
      var cnt = root.querySelector('.brd-count');
      var mine = r.ann.filter(function (a) { return a.by !== 'claude'; }).length;
      cnt.textContent = r.ann.length ? (mine + ' yours' + (r.ann.length - mine ? ', ' + (r.ann.length - mine) + ' from Claude' : '')) : 'Draw on the page';
    }

    function setAnnotating(on) {
      annotating = !!on && !!activeTab();
      annBar.hidden = !annotating;
      annEl.classList.toggle('drawing', annotating);
      if (annotating) fillChats();
      var b = root.querySelector('[data-brd="annotate"]'); if (b) b.classList.toggle('on', annotating);
      closeNote();
      layout();
    }
    function fillChats() {
      var list = (host.listChats && project) ? host.listChats(project) : [];
      var keep = chatSel.value;
      chatSel.innerHTML = list.length ? list.map(function (c) { return '<option value="' + esc(c.id) + '">' + esc(c.title || 'Chat') + '</option>'; }).join('') : '<option value="">No chat open in this project</option>';
      var pick = list.find(function (c) { return c.id === keep; }) || list.find(function (c) { return c.focused; }) || list[0];
      if (pick) chatSel.value = pick.id;
      root.querySelector('[data-ann="send"]').disabled = !list.length;
    }

    // Drawing: pointer coordinates → page CSS px.
    var drag = null;
    function toPage(ev) { var t = activeTab(); var r = runtime[t.id]; var b = annEl.getBoundingClientRect(); var k = r.box.scale || 1; return [Math.round((ev.clientX - b.left) / k), Math.round((ev.clientY - b.top) / k)]; }
    annEl.addEventListener('pointerdown', function (ev) {
      if (!annotating || ev.button !== 0 || ev.target.closest('.brd-note')) return;
      ev.preventDefault(); closeNote();
      var t = activeTab(); if (!t) return; var r = runtime[t.id];
      var p = toPage(ev);
      if (annTool === 'pin') { var a = { id: uid(), by: 'you', kind: 'pin', x: p[0], y: p[1], text: '' }; r.ann.push(a); renderAnn(); askNote(a); return; }
      var a2 = annTool === 'pen' ? { id: uid(), by: 'you', kind: 'pen', points: [p], x: p[0], y: p[1], text: '' } : { id: uid(), by: 'you', kind: 'box', x: p[0], y: p[1], width: 1, height: 1, text: '' };
      drag = { a: a2, start: p };
      r.ann.push(a2);
      try { annEl.setPointerCapture(ev.pointerId); } catch (e) { /* synthetic pointer */ }
    });
    annEl.addEventListener('pointermove', function (ev) {
      if (!drag) return;
      var p = toPage(ev); var a = drag.a;
      if (a.kind === 'pen') { a.points.push(p); }
      else { a.x = Math.min(drag.start[0], p[0]); a.y = Math.min(drag.start[1], p[1]); a.width = Math.abs(p[0] - drag.start[0]); a.height = Math.abs(p[1] - drag.start[1]); }
      renderAnn();
    });
    annEl.addEventListener('pointerup', function () {
      if (!drag) return;
      var a = drag.a; drag = null;
      var t = activeTab(); var r = runtime[t.id];
      if ((a.kind === 'box' && (a.width < 4 || a.height < 4)) || (a.kind === 'pen' && a.points.length < 3)) { r.ann.splice(r.ann.indexOf(a), 1); renderAnn(); return; }
      if (a.kind === 'pen') {
        // Simplify the stroke, and anchor it at its middle point for "what is under it".
        var pts = a.points.filter(function (_, i) { return i % 2 === 0 || i === a.points.length - 1; }).slice(-400);
        a.points = pts; var mid = pts[Math.floor(pts.length / 2)]; a.x = mid[0]; a.y = mid[1];
      }
      renderAnn(); askNote(a);
    });
    var noteEl = null;
    function closeNote() { if (noteEl) { noteEl.remove(); noteEl = null; } }
    function askNote(a) {
      closeNote();
      var t = activeTab(); var r = runtime[t.id]; var k = r.box.scale || 1;
      noteEl = document.createElement('div'); noteEl.className = 'brd-note';
      noteEl.innerHTML = '<input type="text" placeholder="Note (Enter to keep, Esc to skip)">';
      var ax = (a.kind === 'box' ? a.x + a.width : a.x) * k, ay = (a.kind === 'box' ? a.y + a.height : a.y) * k;
      noteEl.style.left = Math.max(0, Math.min(ax, r.box.width - 240)) + 'px';
      noteEl.style.top = Math.max(0, Math.min(ay + 6, r.box.height - 40)) + 'px';
      annEl.appendChild(noteEl);
      var inp = noteEl.querySelector('input');
      inp.addEventListener('keydown', function (e) {
        if (e.key === 'Enter') { a.text = inp.value.trim().slice(0, 500); closeNote(); renderAnn(); }
        else if (e.key === 'Escape') { closeNote(); }
        e.stopPropagation();
      });
      inp.addEventListener('blur', function () { if (noteEl && inp.value.trim()) { a.text = inp.value.trim().slice(0, 500); renderAnn(); } setTimeout(closeNote, 0); });
      setTimeout(function () { try { inp.focus(); } catch (e) { /* ignore */ } }, 0);
    }

    // A screenshot of the page with the marks drawn in, as base64 PNG.
    function composite(tabId) {
      var r = runtime[tabId];
      return api.capture(tabId).then(function (cap) {
        if (!cap || !cap.ok) throw new Error((cap && cap.error) || 'capture failed');
        return new Promise(function (resolve, reject) {
          var img = new Image();
          img.onload = function () {
            var cv = document.createElement('canvas'); cv.width = img.width; cv.height = img.height;
            var g = cv.getContext('2d'); g.drawImage(img, 0, 0);
            var k = (img.width / (r.box.width || img.width)) * (r.box.scale || 1);
            r.ann.forEach(function (a, i) {
              var c = colorOf(a);
              g.lineWidth = 3 * img.width / (r.box.width || img.width); g.strokeStyle = c; g.fillStyle = c;
              if (a.kind === 'box') { g.strokeRect(a.x * k, a.y * k, a.width * k, a.height * k); }
              else if (a.kind === 'arrow') { g.beginPath(); g.moveTo(a.x * k, a.y * k); g.lineTo(a.toX * k, a.toY * k); g.stroke(); var ang = Math.atan2(a.toY - a.y, a.toX - a.x); var hl = 12 * k; g.beginPath(); g.moveTo(a.toX * k, a.toY * k); g.lineTo(a.toX * k - hl * Math.cos(ang - 0.45), a.toY * k - hl * Math.sin(ang - 0.45)); g.lineTo(a.toX * k - hl * Math.cos(ang + 0.45), a.toY * k - hl * Math.sin(ang + 0.45)); g.closePath(); g.fill(); }
              else if (a.kind === 'pen' && a.points) { g.beginPath(); a.points.forEach(function (p, j) { if (j) g.lineTo(p[0] * k, p[1] * k); else g.moveTo(p[0] * k, p[1] * k); }); g.stroke(); }
              var lx = a.kind === 'box' ? a.x * k : a.x * k, ly = a.kind === 'box' ? a.y * k : a.y * k;
              var rad = 11 * img.width / (r.box.width || img.width);
              g.beginPath(); g.arc(a.kind === 'pin' ? lx : lx + rad, a.kind === 'pin' ? ly : ly + rad, rad, 0, Math.PI * 2); g.fill();
              g.fillStyle = '#fff'; g.font = 'bold ' + Math.round(rad * 1.1) + 'px system-ui, sans-serif'; g.textAlign = 'center'; g.textBaseline = 'middle';
              g.fillText(String(i + 1), a.kind === 'pin' ? lx : lx + rad, a.kind === 'pin' ? ly : ly + rad);
            });
            resolve(cv.toDataURL('image/png').split(',')[1]);
          };
          img.onerror = function () { reject(new Error('could not read the capture')); };
          img.src = 'data:image/png;base64,' + cap.data;
        });
      });
    }
    function anchorOf(a) {
      if (a.kind === 'box') return [a.x + a.width / 2, a.y + a.height / 2];
      if (a.kind === 'arrow') return [a.toX, a.toY];
      return [a.x, a.y];
    }
    function annList(r) {
      return r.ann.map(function (a, i) {
        var o = { n: i + 1, by: a.by === 'claude' ? 'claude' : 'person', kind: a.kind, text: a.text || '', x: a.x, y: a.y, anchor: anchorOf(a) };
        if (a.kind === 'box') { o.width = a.width; o.height = a.height; }
        if (a.kind === 'arrow') { o.toX = a.toX; o.toY = a.toY; }
        if (a.kind === 'pen') o.points = a.points.length;
        return o;
      });
    }

    function sendToChat() {
      var t = activeTab(); if (!t) return;
      var r = runtime[t.id]; var chatId = chatSel.value;
      if (!chatId) { toast('Open a chat in this project first', 'error'); return; }
      var btn = root.querySelector('[data-ann="send"]'); btn.disabled = true;
      var list = annList(r);
      Promise.all([composite(t.id), api.describe(t.id, list.map(function (a) { return a.anchor; })).catch(function () { return { elements: [] }; })]).then(function (res) {
        var png = res[0], els = (res[1] && res[1].elements) || [];
        var s = (r.state && r.state.url) || t.url || '';
        var lines = ['Browser: ' + s + (t.title ? ' (' + t.title + ')' : '') + ', viewport ' + Math.round(r.box.pageW) + 'x' + Math.round(r.box.pageH) + (t.network === 'server' ? ', server network' : '') + ' [drawer tab key: drawer:' + t.id + ']'];
        list.forEach(function (a, i) {
          var e = els[i];
          var where = a.kind === 'box' ? 'box at ' + a.x + ',' + a.y + ' size ' + a.width + 'x' + a.height : a.kind + ' at ' + Math.round(a.anchor[0]) + ',' + Math.round(a.anchor[1]);
          lines.push(a.n + '. ' + (a.by === 'claude' ? '(Claude) ' : '') + where + (e ? ' on <' + e.tag + '> ' + e.selector + (e.text ? ' "' + e.text.slice(0, 80) + '"' : '') : '') + (a.text ? ': ' + a.text : ''));
        });
        if (!list.length) lines.push('(no marks: the page as it is)');
        return host.sendToChat(chatId, lines.join('\n'), png);
      }).then(function () { toast('Added to the chat: press Send there'); }, function (err) { toast('Could not attach: ' + (err && err.message || err), 'error'); })
        .then(function () { btn.disabled = false; });
    }

    // ─── Toolbar ───
    function go(v) {
      var t = activeTab(); v = (v || '').trim(); if (!t || !v) return;
      var url = v;
      var hasScheme = /^https?:\/\//i.test(v) || v === 'about:blank';
      var looksHost = v.indexOf(' ') < 0 && (v.indexOf('.') > 0 || /^localhost(:\d+)?(\/|$)/i.test(v) || /^[\d.]+:\d+/.test(v));
      if (!hasScheme && !looksHost) url = 'https://www.google.com/search?q=' + encodeURIComponent(v);
      else if (!hasScheme) url = (/^(localhost|127\.|\[::1\])/i.test(v) ? 'http://' : 'https://') + v;
      t.url = url; persist();
      var r = ensure(t, project);
      r.attached.then(function () { api.navigate(t.id, url); });
      urlEl.blur();
    }
    urlEl.addEventListener('keydown', function (e) { if (e.key === 'Enter') go(urlEl.value); else if (e.key === 'Escape') { urlEl.blur(); renderBar(); } });
    devSel.addEventListener('change', function () {
      var t = activeTab(); if (!t) return;
      t.device = devSel.value === 'custom' ? 'custom' : (Number(devSel.value) || 0); persist();
      layout(); renderBar();
    });
    netSel.addEventListener('change', function () { var t = activeTab(); if (t) setNetwork(t.id, netSel.value); });
    root.addEventListener('click', function (e) {
      var x = e.target.closest('[data-close]'); if (x) { e.stopPropagation(); closeTab(x.dataset.close); return; }
      var tb = e.target.closest('.brd-tab'); if (tb) { activate(tb.dataset.tab); return; }
      var tool = e.target.closest('[data-tool]');
      if (tool) { annTool = tool.dataset.tool; root.querySelectorAll('[data-tool]').forEach(function (b) { b.classList.toggle('on', b === tool); }); return; }
      var an = e.target.closest('[data-ann]');
      if (an) {
        var t0 = activeTab(); var r0 = t0 && runtime[t0.id];
        if (an.dataset.ann === 'done') setAnnotating(false);
        else if (an.dataset.ann === 'clear' && r0) { r0.ann = r0.ann.filter(function (a) { return a.by === 'claude'; }); renderAnn(); }
        else if (an.dataset.ann === 'send') sendToChat();
        return;
      }
      var b = e.target.closest('[data-brd]'); if (!b) return;
      var a = b.dataset.brd, t = activeTab();
      if (a === 'new') newTab(START, t ? t.network : 'client', true);
      else if (a === 'hide') close();
      else if (!t) return;
      else if (a === 'back' || a === 'forward' || a === 'reload') api.nav(t.id, a);
      else if (a === 'devtools') api.devtools(t.id);
      else if (a === 'annotate') setAnnotating(!annotating);
    });
    tabsEl.addEventListener('auxclick', function (e) { var tb = e.target.closest('.brd-tab'); if (tb && e.button === 1) closeTab(tb.dataset.tab); });

    // Switching network: a different partition, so a new <webview> (made in
    // place of the old, not moved); the page reloads at its current address.
    function setNetwork(tabId, network) {
      var f = findTab(tabId); if (!f) return Promise.reject(new Error('no such tab'));
      var t = f.tab; network = network === 'server' ? 'server' : 'client';
      if (t.network === network && runtime[tabId] && runtime[tabId].wv) return Promise.resolve();
      var r = runtime[tabId];
      var url = (r && r.state && r.state.url) || t.url;
      t.network = network; t.url = url; persist();
      var ann = r ? r.ann : [];
      if (r && r.wv) { try { r.wv.remove(); } catch (e) { /* gone */ } }
      delete runtime[tabId];
      var nr = ensure(t, f.project);
      nr.ann = ann;
      if (f.project === project) { renderTabs(); showActive(); }
      return nr.attached;
    }

    // ─── Page state from the main process ───
    api.onState(function (s) {
      var f = findTab(s.tabId); var r = runtime[s.tabId];
      if (!f || !r) return;
      if (s.diag) { r.diag = s.diag; if (f.project === project) renderBar(); return; }
      var prev = r.state && r.state.url;
      r.state = s; r.diag = '';
      if (s.url && s.url !== 'about:blank') f.tab.url = s.url;
      if (s.title) f.tab.title = s.title;
      // A new page: the old marks no longer point at anything.
      var strip = function (u) { return String(u || '').split('#')[0]; };
      if (prev && s.url && strip(prev) !== strip(s.url) && r.ann.length) { r.ann = []; }
      persist();
      if (f.project === project) { renderTabs(); renderBar(); renderAnn(); }
    });
    function refreshInfo(i) { if (i) info = i; if (project) { renderTabs(); renderBar(); } }
    api.info().then(refreshInfo, function () {});
    if (api.onInfo) api.onInfo(refreshInfo);

    // ─── Requests from the main process (the server's chats) ───
    function tabOut(t, p) {
      var r = runtime[t.id];
      return { tabId: t.id, project: p, url: (r && r.state && r.state.url) || t.url, title: t.title || '', network: t.network || 'client',
        active: proj(p).active === t.id, shown: st.open && p === project && (activeTab() || {}).id === t.id, loaded: !!(r && r.ready),
        device: t.device === 'custom' ? (t.custom ? 'custom ' + t.custom.w + 'x' + t.custom.h : 'custom') : (DEVICES[t.device || 0] || DEVICES[0]).name };
    }
    function mustTab(tabId) { var f = findTab(tabId); if (!f) throw new Error('No such drawer tab (it may have been closed)'); return f; }
    function showTab(f) {
      proj(f.project).active = f.tab.id; persist();
      if (f.project === project) { if (!st.open) { st.open = true; persist(); applyOpen(); } renderTabs(); showActive(); }
    }
    var handlers = {
      list: function (a) {
        var p = String(a.project || '').toLowerCase();
        return { tabs: proj(p).tabs.map(function (t) { return tabOut(t, p); }) };
      },
      open: function (a) {
        var p = String(a.project || '').toLowerCase();
        if (!p) throw new Error('no project');
        var n = newTab('about:blank', a.network, !!a.activate, { project: p, load: false, quiet: true });
        if (!n) throw new Error('This project already has ' + MAX_TABS + ' drawer tabs: close one first.');
        if (a.show) showTab({ tab: n.tab, project: p });
        return n.rt.attached.then(function () { return { tabId: n.tab.id }; });
      },
      ensure: function (a) {
        var f = mustTab(a.tabId);
        if (a.project && String(a.project).toLowerCase() !== f.project) throw new Error('That drawer tab belongs to another project.');
        var r = ensure(f.tab, f.project);
        return r.attached.then(function () { return { project: f.project, network: f.tab.network }; });
      },
      close: function (a) { mustTab(a.tabId); closeTab(a.tabId); return {}; },
      resize: function (a) {
        var f = mustTab(a.tabId);
        var w = clamp(Math.round(Number(a.width) || 0), 200, 3840), h = clamp(Math.round(Number(a.height) || 0), 200, 3840);
        f.tab.device = 'custom'; f.tab.custom = { w: w, h: h }; persist();
        layout(); if (f.project === project) renderBar();
        return {};
      },
      'set-network': function (a) { return setNetwork(a.tabId, a.network).then(function () { return {}; }); },
      annotate: function (a) {
        var f = mustTab(a.tabId);
        var r = runtime[f.tab.id] || ensure(f.tab, f.project);
        if (a.clear) r.ann = r.ann.filter(function (x) { return x.by !== 'claude'; });
        var drawn = 0;
        (a.shapes || []).forEach(function (s) {
          var x = { id: uid(), by: 'claude', kind: s.kind, x: s.x, y: s.y, text: s.text || '' };
          if (s.kind === 'box') { x.width = s.width; x.height = s.height; }
          if (s.kind === 'arrow') { x.toX = s.toX; x.toY = s.toY; }
          if (s.kind === 'label' || s.kind === 'pin' || s.kind === 'box' || s.kind === 'arrow') { r.ann.push(x); drawn++; }
        });
        if (a.show) showTab(f);
        else if (f.project === project) renderAnn();
        return { drawn: drawn, total: r.ann.length };
      },
      annotations: function (a) {
        var f = mustTab(a.tabId);
        var r = runtime[f.tab.id];
        if (!r || !r.box) throw new Error('That tab is not loaded');
        var out = { annotations: annList(r), viewport: { width: Math.round(r.box.pageW), height: Math.round(r.box.pageH) } };
        if (!a.image) return out;
        return composite(f.tab.id).then(function (png) { out.image = png; return out; });
      },
    };
    api.onRequest(function (req) {
      var fn = handlers[req && req.cmd];
      if (!fn) { api.reply(req.reqId, { ok: false, error: 'unknown request ' + (req && req.cmd) }); return; }
      Promise.resolve().then(function () { return fn(req.args || {}); }).then(function (res) {
        api.reply(req.reqId, Object.assign({ ok: true }, res || {}));
      }, function (err) { api.reply(req.reqId, { ok: false, error: (err && err.message) || String(err) }); });
    });

    // ─── Project switching ───
    function setProject(alias) {
      var p = alias ? String(alias).toLowerCase() : null;
      if (p === project) return;
      setAnnotating(false);
      project = p;
      renderTabs(); showActive();
    }

    applyOpen();
    api.ready();

    return {
      setProject: setProject,
      toggle: function () { if (st.open) close(); else open(); },
      open: open, close: close,
      isOpen: function () { return st.open; },
    };
  }

  window.CrundiBrowserDrawer = { init: init, DEVICES: DEVICES, PARTITIONS: PARTITIONS };
})();
