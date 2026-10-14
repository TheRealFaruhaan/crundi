/**
 * crundi-docker.js — the Docker tab.
 *
 * Containers (with a per-container overview, live logs, charts, a shell and
 * the raw inspect), Compose stacks, images, volumes, networks and the engine
 * itself. The machine-wide view of Docker, owner-only; the server side is
 * src/docker.js and the /api/docker/* routes.
 *
 * Lives outside webapp-html.js for the reason claude-chat.js and
 * crundi-notes.js do: that file is one template literal, where a stray
 * backslash or backtick in browser JS silently breaks the whole page. This is a
 * plain script served from /vendor/.
 *
 *   window.CrundiDocker.mount(el, opts) -> view   { show(), hide() }
 *   window.CrundiDocker.handleMessage(msg)        docker-* socket messages
 *   window.CrundiDocker.onReconnect()             after the socket comes back
 *
 * opts: apiFetch(path, init), toast(msg, kind), send(obj) -> bool (the host's
 * socket), icon(name) -> svg, Terminal, FitAddon, termTheme, termFont.
 */
(function () {
  'use strict';

  // ─── Styles ───
  var STYLE_ID = 'dk-styles';
  var CSS = [
    '.dk-root{display:flex;flex-direction:column;height:100%;min-height:0;position:relative;color:var(--text-primary)}',
    '.dk-nav{display:flex;align-items:center;gap:8px;padding:10px 14px;border-bottom:1px solid var(--border);flex:none;flex-wrap:wrap}',
    '.dk-seg{display:flex;gap:2px;padding:2px;background:var(--bg-tertiary);border:1px solid var(--border);border-radius:8px;overflow-x:auto;scrollbar-width:none;max-width:100%}',
    '.dk-seg::-webkit-scrollbar{display:none}',
    '.dk-seg button{display:inline-flex;align-items:center;gap:6px;border:none;background:none;color:var(--text-secondary);font-size:.78rem;padding:6px 11px;border-radius:6px;cursor:pointer;white-space:nowrap}',
    '.dk-seg button:hover{color:var(--text-primary)}',
    '.dk-seg button.on{background:var(--bg-hover);color:var(--text-primary);box-shadow:inset 0 0 0 1px var(--border)}',
    '.dk-seg button .n{font-family:var(--mono);font-size:.68rem;color:var(--text-muted)}',
    '.dk-seg .ic{width:14px;height:14px}',
    '.dk-spacer{flex:1}',
    '.dk-filter{min-width:120px;width:180px;padding:6px 9px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:.78rem}',
    '.dk-filter:focus{outline:none;border-color:var(--accent)}',
    '.dk-body{flex:1;min-height:0;overflow:auto;padding:14px}',
    '.dk-quiet{color:var(--text-muted);font-size:.8rem;padding:4px 2px}',
    '.dk-empty{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:10px;text-align:center;padding:48px 20px;color:var(--text-muted);font-size:.85rem;line-height:1.5}',
    '.dk-empty .ic{width:40px;height:40px;opacity:.45}',
    '.dk-empty b{color:var(--text-primary);font-weight:600;font-size:.95rem}',
    '.dk-empty code{font-family:var(--mono);font-size:.76rem;background:var(--bg-tertiary);padding:2px 6px;border-radius:4px;color:var(--text-secondary)}',

    // Buttons
    '.dk-btn{display:inline-flex;align-items:center;gap:5px;cursor:pointer;padding:5px 10px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-secondary);font-size:.76rem;white-space:nowrap;line-height:1.2}',
    '.dk-btn:hover{border-color:var(--accent);color:var(--text-primary)}',
    '.dk-btn .ic{width:13px;height:13px}',
    '.dk-btn.primary{background:var(--accent);border-color:var(--accent);color:#fff}',
    '.dk-btn.primary:hover{background:var(--accent-hover)}',
    '.dk-btn.danger:hover,.dk-btn.armed{border-color:var(--red);color:var(--red);background:var(--red-dim)}',
    '.dk-btn[disabled]{opacity:.45;cursor:not-allowed;pointer-events:none}',
    '.dk-btn.icon{padding:5px 7px}',
    '.dk-btns{display:flex;gap:6px;flex-wrap:wrap;align-items:center}',

    // Summary + bay plan
    '.dk-summary{display:flex;align-items:center;gap:18px;flex-wrap:wrap;margin-bottom:10px}',
    '.dk-stat{display:flex;flex-direction:column;gap:1px}',
    '.dk-stat .v{font-family:var(--mono);font-size:1.05rem;font-variant-numeric:tabular-nums;color:var(--text-primary)}',
    '.dk-stat .l{font-size:.64rem;text-transform:uppercase;letter-spacing:.07em;color:var(--text-muted)}',
    '.dk-bay{display:flex;flex-wrap:wrap;gap:4px;margin:0 0 14px;padding:10px;border:1px solid var(--border-subtle);border-radius:var(--radius-sm);background:var(--bg-secondary)}',
    '.dk-bay i{width:14px;height:14px;border-radius:3px;cursor:pointer;display:block;background:var(--bg-hover);transition:transform .12s}',
    '.dk-bay i:hover{transform:scale(1.25)}',

    // State colours (dots, bay, chips)
    '.dk-s-running{background:var(--green)!important}',
    '.dk-s-paused{background:var(--yellow)!important}',
    '.dk-s-restarting{background:var(--accent)!important}',
    '.dk-s-unhealthy{background:var(--yellow)!important}',
    '.dk-s-dead{background:var(--red)!important}',
    '.dk-s-exited,.dk-s-created,.dk-s-removing{background:var(--text-muted)!important}',
    '.dk-dot{width:8px;height:8px;border-radius:50%;flex:none;background:var(--text-muted)}',
    '.dk-dot.dk-s-running{box-shadow:0 0 0 3px var(--green-dim)}',
    '.dk-dot.dk-s-unhealthy,.dk-dot.dk-s-paused{box-shadow:0 0 0 3px var(--yellow-dim)}',
    '.dk-chip{display:inline-flex;align-items:center;gap:5px;font-size:.64rem;font-family:var(--mono);padding:1px 7px;border-radius:999px;background:var(--bg-tertiary);color:var(--text-secondary);white-space:nowrap;max-width:100%;overflow:hidden;text-overflow:ellipsis}',
    '.dk-chip.ok{background:var(--green-dim);color:var(--green)}',
    '.dk-chip.warn{background:var(--yellow-dim);color:var(--yellow)}',
    '.dk-chip.bad{background:var(--red-dim);color:var(--red)}',
    '.dk-chip.link{cursor:pointer}',
    '.dk-chip .ic{width:11px;height:11px}',
    '.dk-chip.fwd{background:var(--accent-dim);color:var(--accent-hover,#818cf8)}',
    '.dk-port{display:flex;align-items:center;gap:10px;flex-wrap:wrap;padding:7px 0;border-bottom:1px solid var(--border-subtle);font-size:.78rem}',
    '.dk-port:last-child{border-bottom:none}',
    '.dk-port .k{color:var(--text-muted);min-width:90px}',
    '.dk-port .v{font-family:var(--mono);flex:1;min-width:0}',
    '.dk-fwdform{display:flex;gap:8px;flex-wrap:wrap;align-items:center;padding:10px;margin:6px 0 4px;border-radius:var(--radius-sm);background:var(--bg-tertiary);border:1px solid var(--border)}',
    '.dk-fwdform input[type=text],.dk-fwdform select{padding:6px 9px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font-size:.78rem}',
    '.dk-fwdform input[type=text]{flex:1;min-width:120px;font-family:var(--mono)}',
    '.dk-fwdform label{display:inline-flex;align-items:center;gap:5px;font-size:.76rem;color:var(--text-secondary)}',
    '.dk-help{font-size:.74rem;color:var(--text-muted);margin:0 2px 10px;line-height:1.5}',
    '.dk-chip.link:hover{color:var(--text-primary)}',

    // Lists
    '.dk-list{display:flex;flex-direction:column;border:1px solid var(--border);border-radius:var(--radius);background:var(--bg-card);overflow:hidden}',
    '.dk-row{display:grid;grid-template-columns:14px minmax(0,1.6fr) minmax(0,1fr) 64px 76px auto;align-items:center;gap:12px;padding:9px 12px;border-bottom:1px solid var(--border-subtle);cursor:pointer}',
    '.dk-row:last-child{border-bottom:none}',
    '.dk-row:hover{background:var(--bg-hover)}',
    '.dk-row.plain{cursor:default;grid-template-columns:minmax(0,1.6fr) minmax(0,1fr) auto}',
    '.dk-row.plain:hover{background:none}',
    '.dk-main{min-width:0}',
    '.dk-name{font-size:.84rem;color:var(--text-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.dk-sub{font-size:.7rem;color:var(--text-muted);font-family:var(--mono);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;margin-top:2px}',
    '.dk-mid{min-width:0;display:flex;flex-wrap:wrap;gap:4px;align-items:center}',
    '.dk-num{font-family:var(--mono);font-size:.76rem;font-variant-numeric:tabular-nums;color:var(--text-secondary);text-align:right}',
    '.dk-num small{display:block;font-size:.6rem;color:var(--text-muted);text-transform:uppercase;letter-spacing:.06em}',
    '.dk-group{font-size:.66rem;text-transform:uppercase;letter-spacing:.07em;color:var(--text-muted);margin:16px 2px 6px}',
    '.dk-group:first-child{margin-top:0}',
    '.dk-toolbar{display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-bottom:12px}',
    '.dk-toolbar .dk-filter{flex:1;max-width:340px}',

    // Stacks
    '.dk-stack{border:1px solid var(--border);border-radius:var(--radius);background:var(--bg-card);margin-bottom:12px;overflow:hidden}',
    '.dk-stack-head{display:flex;align-items:flex-start;gap:12px;padding:12px 14px;flex-wrap:wrap;border-bottom:1px solid var(--border-subtle)}',
    '.dk-stack-head .dk-main{flex:1;min-width:180px}',
    '.dk-stack-name{font-size:.95rem;font-weight:600;display:flex;align-items:center;gap:8px;flex-wrap:wrap}',
    '.dk-svc{display:grid;grid-template-columns:14px minmax(0,1fr) minmax(0,1fr) auto;gap:12px;align-items:center;padding:8px 14px;border-bottom:1px solid var(--border-subtle);cursor:pointer;font-size:.8rem}',
    '.dk-svc:last-child{border-bottom:none}',
    '.dk-svc:hover{background:var(--bg-hover)}',

    // Detail
    '.dk-detail{display:flex;flex-direction:column;height:100%;min-height:0}',
    '.dk-dhead{display:flex;align-items:center;gap:10px;padding:10px 14px;border-bottom:1px solid var(--border);flex:none;flex-wrap:wrap}',
    '.dk-dtitle{flex:1;min-width:140px}',
    '.dk-dtitle .dk-name{font-size:1rem;font-weight:600;display:flex;align-items:center;gap:8px}',
    '.dk-prompt{display:flex;align-items:center;gap:8px;flex-wrap:wrap;padding:10px 14px;background:var(--bg-secondary);border-bottom:1px solid var(--border);font-size:.8rem;flex:none}',
    '.dk-prompt input[type=text]{flex:1;min-width:160px;padding:6px 9px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:.8rem;font-family:var(--mono)}',
    '.dk-prompt label{display:inline-flex;align-items:center;gap:5px;color:var(--text-secondary)}',
    '.dk-dtabs{display:flex;gap:2px;padding:0 10px;border-bottom:1px solid var(--border);flex:none;overflow-x:auto;scrollbar-width:none}',
    '.dk-dtabs button{border:none;background:none;color:var(--text-secondary);font-size:.8rem;padding:9px 12px;cursor:pointer;border-bottom:2px solid transparent;white-space:nowrap}',
    '.dk-dtabs button.on{color:var(--text-primary);border-bottom-color:var(--accent)}',
    '.dk-pane{flex:1;min-height:0;display:none;flex-direction:column}',
    '.dk-pane.on{display:flex}',
    '.dk-pane.scroll{overflow:auto;padding:14px}',

    // Overview
    '.dk-sec{border:1px solid var(--border);border-radius:var(--radius-sm);background:var(--bg-secondary);padding:12px 14px;margin-bottom:12px}',
    '.dk-sec h5{margin:0 0 8px;font-size:.66rem;text-transform:uppercase;letter-spacing:.07em;color:var(--text-muted);font-weight:600}',
    '.dk-kv{display:grid;grid-template-columns:minmax(90px,160px) minmax(0,1fr);gap:5px 14px;font-size:.78rem}',
    '.dk-kv .k{color:var(--text-muted)}',
    '.dk-kv .v{font-family:var(--mono);color:var(--text-primary);word-break:break-all;user-select:text;-webkit-user-select:text}',
    '.dk-kv .v.hid{color:var(--text-muted);font-family:inherit;font-style:italic}',
    '.dk-note{font-size:.72rem;color:var(--text-muted);margin-top:8px}',
    '.dk-cols{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:12px}',
    '.dk-cols .dk-sec{margin-bottom:0}',
    '.dk-sec details summary{cursor:pointer;font-size:.78rem;color:var(--text-secondary)}',

    // Logs
    '.dk-logbar{display:flex;align-items:center;gap:8px;padding:8px 12px;border-bottom:1px solid var(--border-subtle);flex:none;flex-wrap:wrap;font-size:.76rem}',
    '.dk-logbar label{display:inline-flex;align-items:center;gap:5px;color:var(--text-secondary);cursor:pointer}',
    '.dk-logs{flex:1;min-height:0;overflow:auto;background:var(--bg-primary);padding:8px 12px;font-family:var(--mono);font-size:.74rem;line-height:1.55;color:var(--text-secondary);user-select:text;-webkit-user-select:text}',
    '.dk-logs.wrap .ln{white-space:pre-wrap;word-break:break-word}',
    '.dk-logs .ln{white-space:pre}',
    '.dk-logs .ln.err{color:#fca5a5}',
    '.dk-logs .ts{color:var(--text-muted);margin-right:8px}',
    '.dk-logs.nots .ts{display:none}',
    '.dk-logs mark{background:var(--accent-dim);color:var(--text-primary);border-radius:2px}',
    '.dk-logs .sys{color:var(--text-muted);font-style:italic}',
    '.dk-a-b{font-weight:700}.dk-a-d{opacity:.65}',
    '.dk-a-30{color:#5a5a78}.dk-a-31{color:#f87171}.dk-a-32{color:#34d399}.dk-a-33{color:#fbbf24}.dk-a-34{color:#818cf8}.dk-a-35{color:#c084fc}.dk-a-36{color:#22d3ee}.dk-a-37{color:#e8e8f0}',

    // Stats
    '.dk-charts{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:12px}',
    '.dk-chart{border:1px solid var(--border);border-radius:var(--radius-sm);background:var(--bg-secondary);padding:12px 14px}',
    '.dk-chart .h{display:flex;align-items:baseline;justify-content:space-between;gap:10px;margin-bottom:8px}',
    '.dk-chart .t{font-size:.66rem;text-transform:uppercase;letter-spacing:.07em;color:var(--text-muted)}',
    '.dk-chart .v{font-family:var(--mono);font-size:.95rem;font-variant-numeric:tabular-nums}',
    '.dk-chart .v small{font-size:.7rem;color:var(--text-muted);margin-left:4px}',
    '.dk-chart svg{width:100%;height:84px;display:block}',
    '.dk-chart .leg{display:flex;gap:12px;font-size:.68rem;color:var(--text-muted);margin-top:6px;font-family:var(--mono)}',
    '.dk-chart .leg i{display:inline-block;width:8px;height:8px;border-radius:2px;margin-right:4px;vertical-align:-1px}',

    // Shell + inspect
    '.dk-shell{flex:1;min-height:0;background:#0a0a0f;padding:6px 0 0 8px;position:relative}',
    '.dk-shell .xterm{height:100%}',
    '.dk-shellbar{display:flex;align-items:center;gap:8px;padding:8px 12px;font-size:.76rem;color:var(--text-secondary);border-bottom:1px solid var(--border-subtle);flex:none}',
    '.dk-pre{flex:1;min-height:0;overflow:auto;margin:0;background:var(--bg-primary);padding:12px 14px;font-family:var(--mono);font-size:.72rem;line-height:1.5;color:var(--text-secondary);white-space:pre;user-select:text;-webkit-user-select:text}',

    // System
    '.dk-disk{width:100%;border-collapse:collapse;font-size:.8rem}',
    '.dk-disk th{text-align:left;font-size:.64rem;text-transform:uppercase;letter-spacing:.07em;color:var(--text-muted);font-weight:600;padding:6px 8px;border-bottom:1px solid var(--border)}',
    '.dk-disk td{padding:9px 8px;border-bottom:1px solid var(--border-subtle);font-variant-numeric:tabular-nums}',
    '.dk-disk tr:last-child td{border-bottom:none}',
    '.dk-disk td.n{font-family:var(--mono);text-align:right}',
    '.dk-disk th.n{text-align:right}',
    '.dk-warn{font-size:.74rem;color:var(--yellow);margin:4px 0;font-family:var(--mono)}',

    // Job drawer
    '.dk-job{position:absolute;left:12px;right:12px;bottom:12px;max-height:46%;display:flex;flex-direction:column;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius);box-shadow:0 12px 40px rgba(0,0,0,.45);z-index:20}',
    '.dk-job .h{display:flex;align-items:center;gap:10px;padding:9px 12px;border-bottom:1px solid var(--border-subtle);font-size:.8rem}',
    '.dk-job .h b{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:600}',
    '.dk-job pre{flex:1;min-height:60px;overflow:auto;margin:0;padding:10px 12px;font-family:var(--mono);font-size:.72rem;line-height:1.5;color:var(--text-secondary);white-space:pre-wrap;word-break:break-word}',
    '.dk-spin{width:12px;height:12px;border:2px solid var(--border);border-top-color:var(--accent);border-radius:50%;animation:dk-spin .8s linear infinite;flex:none}',
    '@keyframes dk-spin{to{transform:rotate(360deg)}}',

    '@media (max-width:768px){',
    '  .dk-nav{padding:8px 10px}',
    '  .dk-filter{flex:1;width:auto}',
    '  .dk-nav.in-detail [data-a="refresh"],.dk-nav.in-detail .dk-spacer{display:none}',
    '  .dk-body{padding:10px}',
    '  .dk-row{grid-template-columns:14px minmax(0,1fr) auto;gap:10px}',
    '  .dk-row .dk-mid,.dk-row .dk-num.mem{display:none}',
    '  .dk-row .dk-acts{display:none}',
    '  .dk-row.plain{grid-template-columns:minmax(0,1fr) auto}',
    '  .dk-row.fwdrow .dk-mid{display:flex;grid-column:1/-1;order:3}',
    '  .dk-svc{grid-template-columns:14px minmax(0,1fr) auto}',
    '  .dk-svc .dk-sub.img{display:none}',
    '  .dk-kv{grid-template-columns:minmax(0,1fr)}',
    '  .dk-kv .k{margin-top:6px}',
    '  .dk-job{left:6px;right:6px;bottom:6px;max-height:60%}',
    '  .dk-disk .hide-m{display:none}',
    '}',
    '@media (prefers-reduced-motion:reduce){.dk-spin{animation:none}.dk-bay i{transition:none}}',
  ].join('\n');

  function injectStyles() {
    if (document.getElementById(STYLE_ID)) return;
    var s = document.createElement('style');
    s.id = STYLE_ID;
    s.textContent = CSS;
    document.head.appendChild(s);
  }

  // ─── Small helpers ───

  function esc(s) {
    return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }

  function bytes(n) {
    if (n == null || !isFinite(n)) return '—';
    if (n < 1024) return Math.round(n) + ' B';
    var u = ['KB', 'MB', 'GB', 'TB'];
    var v = n / 1024, i = 0;
    while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
    return (v >= 100 ? Math.round(v) : v.toFixed(1)) + ' ' + u[i];
  }

  function pct(n) {
    if (n == null || !isFinite(n)) return '—';
    return (n >= 100 ? Math.round(n) : n.toFixed(1)) + '%';
  }

  /** "3 days ago" from a unix-seconds number or an ISO string. */
  function ago(t) {
    var ms = typeof t === 'number' ? t * 1000 : Date.parse(t);
    if (!ms || ms < 0 || isNaN(ms)) return '—';
    var s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
    if (s < 60) return 'just now';
    var m = Math.floor(s / 60); if (m < 60) return m + ' min ago';
    var h = Math.floor(m / 60); if (h < 48) return h + ' h ago';
    var d = Math.floor(h / 24); if (d < 60) return d + ' days ago';
    var mo = Math.floor(d / 30); if (mo < 24) return mo + ' months ago';
    return Math.floor(d / 365) + ' years ago';
  }

  function when(iso) {
    var ms = Date.parse(iso);
    if (!ms || ms < 86400000) return '—';
    return new Date(ms).toLocaleString() + ' (' + ago(iso) + ')';
  }

  function stateKey(c) {
    if (c.health === 'unhealthy' && c.state === 'running') return 'unhealthy';
    return c.state || 'exited';
  }

  function portText(p) {
    if (!p.public) return p.private + '/' + p.type;
    var ip = p.ip && p.ip !== '0.0.0.0' && p.ip !== '::' ? p.ip + ':' : '';
    return ip + p.public + '→' + p.private + (p.type !== 'tcp' ? '/' + p.type : '');
  }

  function matches(filter, parts) {
    if (!filter) return true;
    var hay = parts.join(' ').toLowerCase();
    return filter.toLowerCase().split(/\s+/).every(function (w) { return !w || hay.indexOf(w) >= 0; });
  }

  // ANSI SGR colours (the common ones) to spans; everything else is dropped.
  var ESC = String.fromCharCode(27);
  var ANSI_RE = new RegExp(ESC + '\\[([0-9;?]*)([A-Za-z])', 'g');
  function ansiHtml(text, highlight) {
    var out = '', open = 0, last = 0, m, cls = [];
    ANSI_RE.lastIndex = 0;
    function piece(s) {
      var h = esc(s);
      if (highlight) {
        var lower = s.toLowerCase(), hl = highlight.toLowerCase(), at = 0, built = '', idx;
        while ((idx = lower.indexOf(hl, at)) >= 0) {
          built += esc(s.slice(at, idx)) + '<mark>' + esc(s.slice(idx, idx + hl.length)) + '</mark>';
          at = idx + hl.length;
        }
        h = built + esc(s.slice(at));
      }
      return h;
    }
    while ((m = ANSI_RE.exec(text))) {
      out += piece(text.slice(last, m.index));
      last = ANSI_RE.lastIndex;
      if (m[2] !== 'm') continue;
      var codes = (m[1] || '0').split(';');
      for (var i = 0; i < codes.length; i++) {
        var c = parseInt(codes[i] || '0', 10);
        if (c === 0) cls = [];
        else if (c === 1) cls.push('dk-a-b');
        else if (c === 2) cls.push('dk-a-d');
        else if (c === 39) cls = cls.filter(function (x) { return !/^dk-a-\d/.test(x); });
        else if (c >= 30 && c <= 37) { cls = cls.filter(function (x) { return !/^dk-a-\d/.test(x); }); cls.push('dk-a-' + c); }
        else if (c >= 90 && c <= 97) { cls = cls.filter(function (x) { return !/^dk-a-\d/.test(x); }); cls.push('dk-a-' + (c - 60)); }
      }
      while (open) { out += '</span>'; open--; }
      if (cls.length) { out += '<span class="' + cls.join(' ') + '">'; open++; }
    }
    out += piece(text.slice(last));
    while (open) { out += '</span>'; open--; }
    return out;
  }
  function stripAnsi(s) { ANSI_RE.lastIndex = 0; return s.replace(ANSI_RE, ''); }

  // ─── Charts ───

  /** A small area chart. series: [{ values, color }]. Values share one scale. */
  function chartSvg(series, maxHint) {
    var W = 300, H = 84, n = 90;
    var max = maxHint || 0;
    series.forEach(function (s) { s.values.forEach(function (v) { if (v != null && v > max) max = v; }); });
    if (!max) max = 1;
    var parts = '';
    // A faint mid line gives the eye a scale without numbers on the axis.
    parts += '<line x1="0" y1="' + H / 2 + '" x2="' + W + '" y2="' + H / 2 + '" stroke="var(--border-subtle)" stroke-dasharray="3 4"/>';
    series.forEach(function (s) {
      var vals = s.values.slice(-n);
      if (vals.length < 2) return;
      var step = W / (n - 1), x0 = W - (vals.length - 1) * step;
      var pts = vals.map(function (v, i) {
        var y = H - 2 - ((v || 0) / max) * (H - 6);
        return (x0 + i * step).toFixed(1) + ',' + y.toFixed(1);
      });
      parts += '<path d="M' + pts[0] + ' L' + pts.slice(1).join(' L') + ' L' + W + ',' + H + ' L' + x0.toFixed(1) + ',' + H + ' Z" fill="' + s.color + '" opacity="0.13"/>';
      parts += '<path d="M' + pts[0] + ' L' + pts.slice(1).join(' L') + '" fill="none" stroke="' + s.color + '" stroke-width="1.6" vector-effect="non-scaling-stroke" stroke-linejoin="round"/>';
    });
    return '<svg viewBox="0 0 ' + W + ' ' + H + '" preserveAspectRatio="none" aria-hidden="true">' + parts + '</svg>';
  }

  // ─── The view ───

  var VIEWS = [
    { key: 'containers', label: 'Containers', icon: 'docker' },
    { key: 'stacks', label: 'Stacks', icon: 'layers' },
    { key: 'images', label: 'Images', icon: 'archive' },
    { key: 'volumes', label: 'Volumes', icon: 'hard-drive' },
    { key: 'networks', label: 'Networks', icon: 'network' },
    { key: 'forwards', label: 'Forwards', icon: 'globe' },
    { key: 'system', label: 'System', icon: 'server' },
  ];

  var active = null;   // the one mounted view, for socket messages
  var refSeq = 0;

  function mount(el, opts) {
    injectStyles();
    var api = opts.apiFetch;
    var toast = opts.toast || function () {};
    var icon = opts.icon || function () { return ''; };
    var send = opts.send || function () { return false; };

    var st = {
      visible: false,
      status: null,
      view: 'containers',
      filter: {},
      data: {},
      detail: null,      // { id, name, tab, ... }
      // Crundi's forwards, matched to containers by port (see loadForwards).
      fwd: { list: null, byPort: {}, at: 0, options: null },
      job: null,         // { id, title, text, next, done, code }
      timer: null,
    };

    el.innerHTML = '<div class="dk-root">'
      + '<div class="dk-nav"><div class="dk-seg" data-r="seg"></div><div class="dk-spacer"></div>'
      + '<input class="dk-filter" data-r="filter" type="search" placeholder="Filter" autocomplete="off" spellcheck="false">'
      + '<button class="dk-btn icon" data-a="refresh" title="Refresh">' + icon('refresh') + '</button></div>'
      + '<div class="dk-body" data-r="body"></div>'
      + '<div data-r="job"></div>'
      + '</div>';
    var root = el.firstChild;
    var R = function (k) { return root.querySelector('[data-r="' + k + '"]'); };
    var nav = root.querySelector('.dk-nav');
    var body = R('body');
    var filterEl = R('filter');

    async function getJson(path, init) {
      var res = await api(path, init);
      var d = await res.json().catch(function () { return { ok: false, error: 'Bad reply from the server' }; });
      return d;
    }
    function post(path, payload) {
      return getJson(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload || {}) });
    }

    // ── Navigation ──

    function drawSeg() {
      var counts = {
        containers: st.data.containers ? st.data.containers.length : null,
        stacks: st.data.stacks ? st.data.stacks.length : null,
        images: st.data.images ? st.data.images.length : null,
        volumes: st.data.volumes ? st.data.volumes.length : null,
        networks: st.data.networks ? st.data.networks.length : null,
        forwards: st.fwd.list ? st.fwd.list.length : null,
      };
      R('seg').innerHTML = VIEWS.map(function (v) {
        var on = !st.detail && st.view === v.key;
        var n = counts[v.key];
        return '<button data-a="view" data-v="' + v.key + '" class="' + (on ? 'on' : '') + '">' + icon(v.icon)
          + '<span>' + v.label + '</span>' + (n != null ? '<span class="n">' + n + '</span>' : '') + '</button>';
      }).join('');
      filterEl.style.display = (st.detail || st.view === 'system') ? 'none' : '';
      nav.classList.toggle('in-detail', !!st.detail);
      filterEl.value = st.filter[st.view] || '';
    }

    function setView(v) {
      closeDetail();
      st.view = v;
      drawSeg();
      body.innerHTML = '<div class="dk-quiet">Loading…</div>';
      load(true).then(schedule);
    }

    // ── Loading and polling ──

    // A poll skips its turn while a request is out; a first load (a view just
    // opened) never waits behind one, it simply makes the slower reply stale.
    var loading = false;
    async function load(first) {
      if (!st.visible || (loading && !first)) return;
      loading = true;
      try {
        if (st.detail) { await refreshDetailHead(); return; }
        var v = st.view;
        var path = {
          containers: '/api/docker/containers?stats=1',
          stacks: '/api/docker/stacks',
          images: '/api/docker/images',
          volumes: '/api/docker/volumes',
          networks: '/api/docker/networks',
          system: '/api/docker/system',
          forwards: '/api/docker/forwards',
        }[v];
        var d = await getJson(path);
        if (v !== st.view || st.detail) return;   // moved on while it loaded
        if (!d.ok) { body.innerHTML = '<div class="dk-empty">' + icon('docker') + '<b>Docker did not answer</b><span>' + esc(d.error || '') + '</span></div>'; return; }
        st.data[v] = d[v] || d;
        if (v === 'system') loadDisk();
        if (v === 'forwards') takeForwards(d.forwards || []);
        if (v === 'containers') loadForwards(false);
        if (v === 'containers') {
          // Stacks count follows from the container labels, so it stays honest
          // on the nav without a second request.
          var seen = {};
          d.containers.forEach(function (c) { if (c.project) seen[c.project] = 1; });
          if (!st.data.stacks || st.data.stacks.length !== Object.keys(seen).length) st.data.stacks = Object.keys(seen).map(function (k) { return { name: k }; });
        }
        drawSeg();
        draw();
      } catch (err) {
        if (first) body.innerHTML = '<div class="dk-empty">' + icon('docker') + '<b>Could not load</b><span>' + esc(err.message) + '</span></div>';
      } finally {
        loading = false;
      }
    }

    function schedule() {
      clearTimeout(st.timer);
      if (!st.visible) return;
      var every = st.detail ? 4000 : ({ containers: 3000, stacks: 5000, forwards: 10000 }[st.view] || 0);
      if (!every) return;
      st.timer = setTimeout(function () {
        if (!document.hidden) load(false).then(schedule); else schedule();
      }, every);
    }

    function draw() {
      // Never repaint under someone halfway through a confirm or typing.
      if (body.querySelector('.dk-btn.armed') || body.querySelector('.dk-fwdform')) return;
      var fn = { containers: drawContainers, stacks: drawStacks, images: drawImages, volumes: drawVolumes, networks: drawNetworks, forwards: drawForwards, system: drawSystem }[st.view];
      var top = body.scrollTop;
      fn();
      body.scrollTop = top;
    }

    // ── Containers ──

    function drawContainers() {
      var list = st.data.containers || [];
      if (!list.length) { body.innerHTML = '<div class="dk-empty">' + icon('docker') + '<b>No containers</b><span>Nothing has been created on this machine yet.</span></div>'; return; }
      var f = st.filter.containers || '';
      var count = { running: 0, paused: 0, stopped: 0 }, cpu = 0, mem = 0;
      list.forEach(function (c) {
        if (c.state === 'running') count.running++; else if (c.state === 'paused') count.paused++; else count.stopped++;
        if (c.stats) { cpu += c.stats.cpuPct || 0; mem += c.stats.memory || 0; }
      });
      var html = '<div class="dk-summary">'
        + stat(count.running, 'running') + stat(count.paused, 'paused') + stat(count.stopped, 'stopped')
        + stat(pct(cpu), 'CPU, of one core') + stat(bytes(mem), 'memory')
        + '</div>';
      // The bay plan: every container at a glance, by state.
      html += '<div class="dk-bay">' + list.map(function (c) {
        return '<i class="dk-s-' + stateKey(c) + '" data-a="open" data-id="' + esc(c.id) + '" title="' + esc(c.name + ' — ' + c.status) + '"></i>';
      }).join('') + '</div>';
      var shown = list.filter(function (c) { return matches(f, [c.name, c.image, c.project, c.service, c.status, c.state, c.id]); });
      if (!shown.length) { body.innerHTML = html + '<div class="dk-quiet">Nothing matches the filter.</div>'; return; }
      html += '<div class="dk-list">' + shown.map(containerRow).join('') + '</div>';
      body.innerHTML = html;
    }

    function stat(v, l) { return '<div class="dk-stat"><span class="v">' + esc(v) + '</span><span class="l">' + esc(l) + '</span></div>'; }

    function containerRow(c) {
      var s = c.stats;
      var chips = '';
      if (c.project) chips += '<span class="dk-chip">' + esc(c.project) + '</span>';
      c.ports.filter(function (p) { return p.public; }).slice(0, 3).forEach(function (p) {
        chips += '<span class="dk-chip">' + esc(portText(p)) + '</span>';
        (st.fwd.byPort[p.public] || []).forEach(function (fw) { chips += fwdChip(fw); });
      });
      if (c.health) chips += '<span class="dk-chip ' + (c.health === 'healthy' ? 'ok' : c.health === 'unhealthy' ? 'bad' : 'warn') + '">' + esc(c.health) + '</span>';
      var act = c.running
        ? '<button class="dk-btn icon" data-a="ctr" data-verb="stop" data-arm="Stop?" data-id="' + esc(c.id) + '" title="Stop">' + icon('stop') + '</button>'
        : c.state === 'paused'
          ? '<button class="dk-btn icon" data-a="ctr" data-verb="unpause" data-id="' + esc(c.id) + '" title="Resume">' + icon('play') + '</button>'
          : '<button class="dk-btn icon" data-a="ctr" data-verb="start" data-id="' + esc(c.id) + '" title="Start">' + icon('play') + '</button>';
      return '<div class="dk-row" data-a="open" data-id="' + esc(c.id) + '">'
        + '<span class="dk-dot dk-s-' + stateKey(c) + '"></span>'
        + '<div class="dk-main"><div class="dk-name">' + esc(c.name) + '</div><div class="dk-sub">' + esc(c.image) + ' · ' + esc(c.status) + '</div></div>'
        + '<div class="dk-mid">' + chips + '</div>'
        + '<div class="dk-num">' + (s ? pct(s.cpuPct) : '') + (s ? '<small>cpu</small>' : '') + '</div>'
        + '<div class="dk-num mem">' + (s ? bytes(s.memory) : '') + (s ? '<small>mem</small>' : '') + '</div>'
        + '<div class="dk-acts">' + act + '</div>'
        + '</div>';
    }

    async function containerVerb(id, verb, btn) {
      if (btn) { btn.disabled = true; }
      var d = await post('/api/docker/containers/action', { id: id, action: verb });
      if (!d.ok) toast(d.error || ('Could not ' + verb + ' it'), 'error');
      else toast({ start: 'Started ', stop: 'Stopped ', restart: 'Restarted ', pause: 'Paused ', unpause: 'Resumed ', kill: 'Killed ' }[verb] + d.name, 'success');
      if (st.detail && st.detail.id === id) {
        await refreshDetailHead(true);
        if (verb === 'start' || verb === 'restart' || verb === 'unpause') { followLogs(); if (st.detail.tab === 'shell') openShell(); }
      } else {
        await load(false);
      }
      if (btn) btn.disabled = false;
    }

    // ── Container detail ──

    async function openDetail(id, tab) {
      closeDetail();
      st.detail = { id: id, tab: tab || 'overview', data: null, samples: [], logRef: null, shellRef: null, term: null, fit: null, lines: [], partial: {} };
      drawSeg();
      filterEl.style.display = 'none';
      body.innerHTML = '<div class="dk-quiet">Loading…</div>';
      var d = await getJson('/api/docker/containers/detail?id=' + encodeURIComponent(id));
      if (!st.detail || st.detail.id !== id) return;
      if (!d.ok) { body.innerHTML = '<div class="dk-quiet">' + esc(d.error || 'Could not open it') + '</div>'; return; }
      st.detail.data = d;
      st.detail.id = d.container.id;
      drawDetail();
      selectDetailTab(st.detail.tab);
      loadForwards(false);   // its ports show their forwards; redrawn when they arrive
      schedule();
    }

    function closeDetail() {
      var dt = st.detail;
      if (!dt) return;
      stopLogs();
      closeShell();
      if (dt.statsTimer) clearInterval(dt.statsTimer);
      if (dt.ro) { try { dt.ro.disconnect(); } catch (e) { /* gone */ } }
      st.detail = null;
      body.style.padding = '';
    }

    function detailActions(c) {
      var b = function (verb, label, ic, arm, cls) {
        return '<button class="dk-btn ' + (cls || '') + '" data-a="ctr" data-verb="' + verb + '" data-id="' + esc(c.id) + '"' + (arm ? ' data-arm="' + esc(arm) + '"' : '') + '>' + icon(ic) + label + '</button>';
      };
      var h = '';
      if (c.state === 'running') {
        h += b('stop', 'Stop', 'stop', 'Stop ' + c.name + '?', 'danger') + b('restart', 'Restart', 'rotate-ccw') + b('pause', 'Pause', 'pause');
        h += b('kill', 'Kill', 'zap', 'Kill it now?', 'danger');
      } else if (c.state === 'paused') {
        h += b('unpause', 'Resume', 'play') + b('stop', 'Stop', 'stop', 'Stop ' + c.name + '?', 'danger');
      } else {
        h += b('start', 'Start', 'play', '', 'primary');
      }
      h += '<button class="dk-btn" data-a="rename-open">' + icon('pencil') + 'Rename</button>';
      h += '<button class="dk-btn danger" data-a="remove-open">' + icon('trash') + 'Remove</button>';
      return h;
    }

    function headHtml(c) {
      return '<button class="dk-btn icon" data-a="back" title="Back to the list">' + icon('chevron-left') + '</button>'
        + '<div class="dk-dtitle"><div class="dk-name"><span class="dk-dot dk-s-' + stateKey(c) + '"></span>' + esc(c.name) + '</div>'
        + '<div class="dk-sub">' + esc(c.image) + ' · ' + esc(c.status) + (c.project ? ' · ' + esc(c.project) : '') + '</div></div>'
        + '<div class="dk-btns" data-r="acts">' + detailActions(c) + '</div>';
    }

    function drawDetail() {
      var dt = st.detail;
      var c = dt.data.container;
      body.style.padding = '0';
      body.innerHTML = '<div class="dk-detail">'
        + '<div class="dk-dhead" data-r="dhead">' + headHtml(c) + '</div>'
        + '<div data-r="prompt"></div>'
        + '<div class="dk-dtabs">' + ['overview', 'logs', 'stats', 'shell', 'inspect'].map(function (t) {
          return '<button data-a="dtab" data-t="' + t + '">' + t.charAt(0).toUpperCase() + t.slice(1) + '</button>';
        }).join('') + '</div>'
        + '<div class="dk-pane scroll" data-p="overview">' + overviewHtml(dt.data.overview) + '</div>'
        + '<div class="dk-pane" data-p="logs">'
        + '<div class="dk-logbar"><input class="dk-filter" data-r="logfilter" type="search" placeholder="Filter lines" style="flex:1;max-width:280px">'
        + '<label><input type="checkbox" data-r="logfollow" checked> Follow</label>'
        + '<label><input type="checkbox" data-r="logts"> Times</label>'
        + '<label><input type="checkbox" data-r="logwrap" checked> Wrap</label>'
        + '<span class="dk-spacer"></span>'
        + '<button class="dk-btn icon" data-a="log-copy" title="Copy">' + icon('copy') + '</button>'
        + '<button class="dk-btn icon" data-a="log-clear" title="Clear">' + icon('x') + '</button></div>'
        + '<div class="dk-logs wrap nots" data-r="logs"></div></div>'
        + '<div class="dk-pane scroll" data-p="stats"><div data-r="stats"><div class="dk-quiet">Measuring…</div></div></div>'
        + '<div class="dk-pane" data-p="shell"><div class="dk-shellbar" data-r="shellbar"></div><div class="dk-shell" data-r="shell"></div></div>'
        + '<div class="dk-pane" data-p="inspect"><div class="dk-logbar"><span class="dk-spacer"></span><button class="dk-btn" data-a="inspect-copy">' + icon('copy') + 'Copy JSON</button></div>'
        + '<pre class="dk-pre" data-r="inspect"></pre></div>'
        + '</div>';
      R2('inspect').textContent = JSON.stringify(dt.data.inspect, null, 2);
    }
    var R2 = function (k) { return body.querySelector('[data-r="' + k + '"]'); };

    function overviewHtml(o) {
      var kv = function (rows) {
        return '<div class="dk-kv">' + rows.filter(function (r) { return r && r[1] !== '' && r[1] != null; }).map(function (r) {
          return '<div class="k">' + esc(r[0]) + '</div><div class="v' + (r[2] ? ' hid' : '') + '">' + esc(r[1]) + '</div>';
        }).join('') + '</div>';
      };
      var h = '<div class="dk-cols">';
      h += '<div class="dk-sec"><h5>State</h5>' + kv([
        ['Status', o.state + (o.health ? ' (' + o.health + ')' : '')],
        ['Started', o.state === 'running' || o.state === 'paused' ? when(o.startedAt) : ''],
        ['Finished', o.state !== 'running' && o.state !== 'paused' ? when(o.finishedAt) : ''],
        ['Exit code', o.state === 'exited' ? String(o.exitCode) + (o.oomKilled ? ' (out of memory)' : '') : ''],
        ['Error', o.error],
        ['Restarts', String(o.restartCount)],
        ['Restart policy', o.restartPolicy || 'no'],
        ['Created', when(o.created)],
      ]) + '</div>';
      h += '<div class="dk-sec"><h5>Runs</h5>' + kv([
        ['Image', o.image],
        ['Image id', (o.imageId || '').replace('sha256:', '').slice(0, 12)],
        ['Entrypoint', o.entrypoint],
        ['Command', o.cmd],
        ['Working dir', o.workingDir],
        ['User', o.user],
        ['Hostname', o.hostname],
        ['Compose', o.project ? o.project + (o.service ? ' / ' + o.service : '') : ''],
        ['Id', o.id],
      ]) + '</div>';
      h += '</div><div style="height:12px"></div><div class="dk-cols">';
      h += '<div class="dk-sec"><h5>Ports</h5><div data-r="ports">' + portsHtml(o) + '</div></div>';
      h += '<div class="dk-sec"><h5>Networks</h5>' + (o.networks.length
        ? kv(o.networks.map(function (n) { return [n.name, (n.ip || 'no address') + (n.gateway ? '  via ' + n.gateway : '')]; }))
        : '<div class="dk-quiet">Not on any network.</div>') + '</div>';
      h += '</div><div style="height:12px"></div>';
      h += '<div class="dk-sec"><h5>Mounts</h5>' + (o.mounts.length
        ? kv(o.mounts.map(function (m) { return [m.destination, (m.type === 'volume' ? 'volume ' + (m.name.length > 40 ? m.name.slice(0, 12) + '…' : m.name) : m.source) + (m.rw ? '' : '  (read-only)')]; }))
        : '<div class="dk-quiet">No mounts.</div>') + '</div>';
      var hidden = o.env.some(function (e) { return e.hidden; });
      h += '<div class="dk-sec"><h5>Environment</h5>' + (o.env.length
        ? kv(o.env.map(function (e) { return [e.key, e.hidden ? 'hidden' : e.value, e.hidden]; }))
          + (hidden ? '<div class="dk-note">Values that look like secrets are hidden here. Inspect shows everything.</div>' : '')
        : '<div class="dk-quiet">None.</div>') + '</div>';
      var labels = Object.keys(o.labels || {});
      if (labels.length) {
        h += '<div class="dk-sec"><details><summary>Labels (' + labels.length + ')</summary><div style="height:8px"></div>'
          + kv(labels.sort().map(function (k) { return [k, o.labels[k]]; })) + '</details></div>';
      }
      return h;
    }

    function selectDetailTab(t) {
      var dt = st.detail;
      if (!dt) return;
      dt.tab = t;
      body.querySelectorAll('.dk-dtabs button').forEach(function (b) { b.classList.toggle('on', b.dataset.t === t); });
      body.querySelectorAll('.dk-pane').forEach(function (p) { p.classList.toggle('on', p.dataset.p === t); });
      if (t === 'logs' && !dt.logRef) followLogs();
      if (t === 'stats') startStats(); else stopStats();
      if (t === 'shell') {
        if (!dt.shellRef) openShell();
        else setTimeout(fitShell, 30);
        if (dt.term) dt.term.focus();
      }
    }

    async function refreshDetailHead(force) {
      var dt = st.detail;
      if (!dt || !dt.data) return;
      if (!force && R2('prompt') && R2('prompt').innerHTML) return;     // mid-rename/remove
      if (!force && body.querySelector('.dk-dhead .dk-btn.armed')) return;
      var d = await getJson('/api/docker/containers');
      if (!st.detail || st.detail !== dt) return;
      if (!d.ok) return;
      var c = d.containers.filter(function (x) { return x.id === dt.id; })[0];
      if (!c) {
        // Removed from elsewhere (or by us): back to the list.
        toast((dt.data.container.name || 'The container') + ' is gone', '');
        setView('containers');
        return;
      }
      var wasRunning = dt.data.container.running;
      dt.data.container = c;
      var head = R2('dhead');
      if (head) head.innerHTML = headHtml(c);
      if (!wasRunning && c.running && dt.tab === 'shell' && !dt.shellRef) openShell();
      if (wasRunning !== c.running && dt.tab === 'shell') drawShellBar();
    }

    // Rename / remove: an inline prompt under the header, not a dialog (an
    // Android web view shows no prompt() or confirm()).
    function openPrompt(kind) {
      var dt = st.detail;
      if (!dt) return;
      var c = dt.data.container;
      var box = R2('prompt');
      if (kind === 'rename') {
        box.innerHTML = '<div class="dk-prompt"><span>New name</span><input type="text" data-r="newname" value="' + esc(c.name) + '" spellcheck="false" autocomplete="off">'
          + '<button class="dk-btn primary" data-a="rename-do">Rename</button><button class="dk-btn" data-a="prompt-close">Cancel</button></div>';
        var inp = R2('newname'); inp.focus(); inp.select();
        inp.addEventListener('keydown', function (e) { if (e.key === 'Enter') doRename(); if (e.key === 'Escape') box.innerHTML = ''; });
      } else {
        box.innerHTML = '<div class="dk-prompt"><span>Remove <b>' + esc(c.name) + '</b>? This cannot be undone.</span>'
          + '<label><input type="checkbox" data-r="rmvol"> Also its anonymous volumes</label>'
          + (c.running || c.state === 'paused' ? '<label><input type="checkbox" data-r="rmforce" checked> Stop it first</label>' : '')
          + '<span class="dk-spacer"></span><button class="dk-btn danger armed" data-a="remove-do">' + icon('trash') + 'Remove</button><button class="dk-btn" data-a="prompt-close">Cancel</button></div>';
      }
    }

    async function doRename() {
      var dt = st.detail, inp = R2('newname');
      if (!dt || !inp) return;
      var d = await post('/api/docker/containers/rename', { id: dt.id, name: inp.value.trim() });
      if (!d.ok) { toast(d.error || 'Could not rename it', 'error'); return; }
      toast('Renamed to ' + d.name, 'success');
      R2('prompt').innerHTML = '';
      refreshDetailHead(true);
    }

    async function doRemove() {
      var dt = st.detail;
      if (!dt) return;
      var vol = R2('rmvol'), force = R2('rmforce');
      var d = await post('/api/docker/containers/remove', { id: dt.id, volumes: !!(vol && vol.checked), force: !!(force && force.checked) });
      if (!d.ok) { toast(d.error || 'Could not remove it', 'error'); return; }
      toast('Removed ' + d.name, 'success');
      setView('containers');
    }

    // ── Logs ──

    var MAX_LINES = 5000;

    function followLogs() {
      var dt = st.detail;
      if (!dt) return;
      stopLogs();
      dt.lines = [];
      dt.partial = {};
      var box = R2('logs');
      if (box) box.innerHTML = '';
      dt.logRef = 'l' + (++refSeq);
      if (!send({ type: 'docker-logs-follow', ref: dt.logRef, id: dt.id, tail: 500 })) {
        // No live socket: fall back to a one-off read so the tab is not blank.
        dt.logRef = null;
        getJson('/api/docker/containers/logs?id=' + encodeURIComponent(dt.id) + '&tail=500').then(function (d) {
          if (!st.detail || st.detail !== dt) return;
          if (!d.ok) { sysLine(d.error || 'Could not read the log'); return; }
          addFrames([{ stream: 'stdout', text: d.text }]);
          sysLine('Not live: the connection to the server is down.');
        });
      }
    }

    function stopLogs() {
      var dt = st.detail;
      if (dt && dt.logRef) { send({ type: 'docker-logs-stop', ref: dt.logRef }); dt.logRef = null; }
    }

    function sysLine(text) {
      var box = R2('logs');
      if (!box) return;
      var div = document.createElement('div');
      div.className = 'ln sys';
      div.textContent = '— ' + text;
      box.appendChild(div);
      if (R2('logfollow') && R2('logfollow').checked) box.scrollTop = box.scrollHeight;
    }

    function addFrames(frames) {
      var dt = st.detail;
      if (!dt) return;
      var fresh = [];
      frames.forEach(function (f) {
        var key = f.stream === 'stderr' ? 'e' : 'o';
        var text = (dt.partial[key] || '') + f.text;
        var parts = text.split('\n');
        dt.partial[key] = parts.pop();
        parts.forEach(function (line) {
          if (line.charAt(line.length - 1) === '\r') line = line.slice(0, -1);
          var m = /^(\d{4}-\d\d-\d\dT[\d:.]+Z) ?/.exec(line);
          var rec = { ts: m ? m[1] : '', text: m ? line.slice(m[0].length) : line, err: key === 'e' };
          dt.lines.push(rec);
          fresh.push(rec);
        });
      });
      var dropped = 0;
      if (dt.lines.length > MAX_LINES) { dropped = dt.lines.length - MAX_LINES; dt.lines.splice(0, dropped); }
      var box = R2('logs');
      if (!box) return;
      var filter = (R2('logfilter') && R2('logfilter').value.trim()) || '';
      var frag = document.createDocumentFragment();
      fresh.forEach(function (rec) { var n = lineNode(rec, filter); if (n) frag.appendChild(n); });
      box.appendChild(frag);
      while (dropped-- > 0 && box.firstChild) box.removeChild(box.firstChild);
      if (R2('logfollow') && R2('logfollow').checked) box.scrollTop = box.scrollHeight;
    }

    function lineNode(rec, filter) {
      if (filter && stripAnsi(rec.text).toLowerCase().indexOf(filter.toLowerCase()) < 0) return null;
      var div = document.createElement('div');
      div.className = 'ln' + (rec.err ? ' err' : '');
      div.innerHTML = (rec.ts ? '<span class="ts">' + esc(rec.ts.replace('T', ' ').slice(0, 23)) + '</span>' : '') + ansiHtml(rec.text, filter);
      return div;
    }

    function redrawLogs() {
      var dt = st.detail, box = R2('logs');
      if (!dt || !box) return;
      var filter = (R2('logfilter') && R2('logfilter').value.trim()) || '';
      var frag = document.createDocumentFragment();
      dt.lines.forEach(function (rec) { var n = lineNode(rec, filter); if (n) frag.appendChild(n); });
      box.innerHTML = '';
      box.appendChild(frag);
      box.scrollTop = box.scrollHeight;
    }

    // ── Stats ──

    function startStats() {
      var dt = st.detail;
      if (!dt || dt.statsTimer) return;
      var tick = async function () {
        if (!st.visible || document.hidden) return;
        var d = await getJson('/api/docker/containers/stats?id=' + encodeURIComponent(dt.id));
        if (!st.detail || st.detail !== dt) return;
        if (!d.ok) { R2('stats').innerHTML = '<div class="dk-quiet">' + esc(d.error || 'No numbers') + '</div>'; return; }
        if (!d.running) { R2('stats').innerHTML = '<div class="dk-quiet">The container is not running, so there is nothing to measure.</div>'; dt.samples = []; return; }
        if (d.stats) { dt.samples.push(d.stats); if (dt.samples.length > 120) dt.samples.shift(); }
        drawStats();
      };
      tick();
      dt.statsTimer = setInterval(tick, 2000);
    }

    function stopStats() {
      var dt = st.detail;
      if (dt && dt.statsTimer) { clearInterval(dt.statsTimer); dt.statsTimer = null; }
    }

    function rates(samples, key) {
      var out = [];
      for (var i = 1; i < samples.length; i++) {
        var dtSec = (samples[i].at - samples[i - 1].at) / 1000;
        out.push(dtSec > 0 ? Math.max(0, (samples[i][key] - samples[i - 1][key]) / dtSec) : 0);
      }
      return out;
    }

    function drawStats() {
      var dt = st.detail, s = dt.samples;
      var box = R2('stats');
      if (!box || !s.length) return;
      var last = s[s.length - 1];
      var cpu = s.map(function (x) { return x.cpuPct; });
      var mem = s.map(function (x) { return x.memory; });
      var rx = rates(s, 'netRx'), tx = rates(s, 'netTx'), br = rates(s, 'blkRead'), bw = rates(s, 'blkWrite');
      var lastOf = function (a) { return a.length ? a[a.length - 1] : null; };
      var card = function (title, value, sub, svg, legend) {
        return '<div class="dk-chart"><div class="h"><span class="t">' + esc(title) + '</span><span class="v">' + esc(value) + (sub ? '<small>' + esc(sub) + '</small>' : '') + '</span></div>'
          + svg + (legend || '') + '</div>';
      };
      var leg = function (a, ca, b, cb) {
        return '<div class="leg"><span><i style="background:' + ca + '"></i>' + esc(a) + '</span><span><i style="background:' + cb + '"></i>' + esc(b) + '</span></div>';
      };
      var memPct = last.memoryLimit ? (last.memory / last.memoryLimit) * 100 : null;
      box.innerHTML = '<div class="dk-charts">'
        + card('CPU', pct(last.cpuPct), last.cpus ? 'of one core · ' + last.cpus + ' available' : '', chartSvg([{ values: cpu, color: 'var(--accent)' }], 100))
        + card('Memory', bytes(last.memory), last.memoryLimit ? 'of ' + bytes(last.memoryLimit) + (memPct != null ? ' (' + pct(memPct) + ')' : '') : '', chartSvg([{ values: mem, color: 'var(--green)' }]))
        + card('Network', bytes(lastOf(rx)) + '/s', 'in', chartSvg([{ values: rx, color: '#22d3ee' }, { values: tx, color: '#c084fc' }]),
          leg('in ' + bytes(lastOf(rx)) + '/s · ' + bytes(last.netRx) + ' total', '#22d3ee', 'out ' + bytes(lastOf(tx)) + '/s · ' + bytes(last.netTx) + ' total', '#c084fc'))
        + card('Disk', bytes(lastOf(br)) + '/s', 'read', chartSvg([{ values: br, color: '#fbbf24' }, { values: bw, color: '#f87171' }]),
          leg('read ' + bytes(last.blkRead) + ' total', '#fbbf24', 'write ' + bytes(last.blkWrite) + ' total', '#f87171'))
        + '</div><div class="dk-note">' + last.pids + ' processes · a sample every 2 seconds, the last ' + Math.round(s.length * 2 / 60 * 10) / 10 + ' minutes</div>';
    }

    // ── Shell ──

    function drawShellBar(text) {
      var dt = st.detail, bar = R2('shellbar');
      if (!dt || !bar) return;
      var c = dt.data.container;
      if (text) { bar.innerHTML = '<span style="flex:1">' + esc(text) + '</span>' + (c.running ? '<button class="dk-btn" data-a="shell-again">' + icon('terminal') + 'Open again</button>' : ''); return; }
      if (!c.running) { bar.innerHTML = '<span>Start the container to open a shell in it.</span>'; return; }
      bar.innerHTML = '<span style="flex:1">A shell inside <b>' + esc(c.name) + '</b> (bash when the image has it, else sh)</span>'
        + '<button class="dk-btn" data-a="shell-again" title="Close this shell and open a fresh one">' + icon('rotate-ccw') + 'New shell</button>';
    }

    function openShell() {
      var dt = st.detail;
      if (!dt) return;
      closeShell();
      var c = dt.data.container;
      drawShellBar();
      if (!c.running) return;
      if (!opts.Terminal) { drawShellBar('The terminal component did not load on this page.'); return; }
      var mountEl = R2('shell');
      mountEl.innerHTML = '';
      var term = new opts.Terminal({
        cursorBlink: true, fontSize: 13, fontFamily: opts.termFont, theme: opts.termTheme,
        scrollback: 5000, convertEol: false, allowProposedApi: true,
      });
      var fit = opts.FitAddon ? new opts.FitAddon.FitAddon() : null;
      if (fit) term.loadAddon(fit);
      term.open(mountEl);
      dt.term = term; dt.fit = fit;
      try { fit && fit.fit(); } catch (e) { /* not laid out yet */ }
      var ref = 's' + (++refSeq);
      dt.shellRef = ref;
      term.onData(function (data) { send({ type: 'docker-shell-input', ref: ref, data: data }); });
      term.onResize(function (sz) { send({ type: 'docker-shell-resize', ref: ref, cols: sz.cols, rows: sz.rows }); });
      if (!send({ type: 'docker-shell-open', ref: ref, id: dt.id, cols: term.cols, rows: term.rows })) {
        drawShellBar('No connection to the server right now.');
        dt.shellRef = null;
        return;
      }
      if (window.ResizeObserver && !dt.ro) {
        dt.ro = new ResizeObserver(function () { fitShell(); });
        dt.ro.observe(mountEl);
      }
      setTimeout(function () { fitShell(); term.focus(); }, 40);
    }

    function fitShell() {
      var dt = st.detail;
      if (!dt || !dt.fit || !dt.term || dt.tab !== 'shell') return;
      try { dt.fit.fit(); } catch (e) { /* hidden */ }
    }

    function closeShell() {
      var dt = st.detail;
      if (!dt) return;
      if (dt.shellRef) { send({ type: 'docker-shell-close', ref: dt.shellRef }); dt.shellRef = null; }
      if (dt.term) { try { dt.term.dispose(); } catch (e) { /* gone */ } dt.term = null; dt.fit = null; }
    }

    // ── Stacks ──

    function drawStacks() {
      var list = st.data.stacks || [];
      if (!list.length || !list[0].containers) {
        body.innerHTML = '<div class="dk-empty">' + icon('layers') + '<b>No Compose projects</b><span>Projects started with <code>docker compose</code> show here, with their services.</span></div>';
        return;
      }
      var f = st.filter.stacks || '';
      var shown = list.filter(function (s) { return matches(f, [s.name, s.directory].concat(s.containers.map(function (c) { return c.service + ' ' + c.name; }))); });
      if (!shown.length) { body.innerHTML = '<div class="dk-quiet">Nothing matches the filter.</div>'; return; }
      body.innerHTML = shown.map(function (s) {
        var state = s.live === s.total ? 'ok' : s.live === 0 ? '' : 'warn';
        var where = s.files.length ? s.files.join(', ') : 'compose file unknown';
        if (s.files.length && !s.filesPresent) where += '  (not found on this machine)';
        var noFile = s.filesPresent ? '' : ' disabled title="Needs the compose file, which was not found"';
        var b = function (action, label, ic, cls, extra) {
          return '<button class="dk-btn ' + (cls || '') + '" data-a="stack" data-name="' + esc(s.name) + '" data-act="' + action + '"' + (extra || '') + '>' + icon(ic) + label + '</button>';
        };
        var btns = b('up', 'Up', 'arrow-up', 'primary', noFile) + b('pull', 'Pull', 'download', '', noFile);
        if (s.live) btns += b('restart', 'Restart', 'rotate-ccw') + b('stop', 'Stop', 'stop', 'danger', ' data-arm="Stop ' + esc(s.name) + '?"');
        else btns += b('start', 'Start', 'play');
        btns += b('down', 'Down', 'trash', 'danger', ' data-arm="Down: remove its containers?"');
        return '<div class="dk-stack"><div class="dk-stack-head"><div class="dk-main">'
          + '<div class="dk-stack-name">' + esc(s.name) + '<span class="dk-chip ' + state + '">' + s.live + '/' + s.total + ' running</span></div>'
          + '<div class="dk-sub" title="' + esc(where) + '">' + esc(where) + '</div></div>'
          + '<div class="dk-btns">' + btns + '</div></div>'
          + s.containers.map(function (c) {
            return '<div class="dk-svc" data-a="open" data-id="' + esc(c.id) + '"><span class="dk-dot dk-s-' + stateKey(c) + '"></span>'
              + '<div class="dk-main"><div class="dk-name">' + esc(c.service || c.name) + '</div><div class="dk-sub">' + esc(c.name) + '</div></div>'
              + '<div class="dk-sub img">' + esc(c.image) + (c.ports.filter(function (p) { return p.public; }).length ? ' · ' + esc(c.ports.filter(function (p) { return p.public; }).slice(0, 2).map(portText).join('  ')) : '') + '</div>'
              + '<div class="dk-sub">' + esc(c.status) + '</div></div>';
          }).join('')
          + '</div>';
      }).join('');
    }

    async function stackAction(name, action) {
      var d = await post('/api/docker/stacks/action', { name: name, action: action });
      if (!d.ok) { toast(d.error || 'Could not run it', 'error'); return; }
      watchJob(d.job, name + ': compose ' + action);
    }

    // ── Images ──

    function drawImages() {
      var list = st.data.images || [];
      var f = st.filter.images || '';
      var used = list.filter(function (i) { return i.usedBy.length; }).length;
      var total = list.reduce(function (a, i) { return a + i.size; }, 0);
      var html = '<div class="dk-toolbar">'
        + '<input class="dk-filter" data-r="pullref" type="text" placeholder="Pull an image, e.g. nginx:alpine" autocomplete="off" spellcheck="false" style="max-width:300px">'
        + '<button class="dk-btn primary" data-a="pull">' + icon('download') + 'Pull</button><span class="dk-spacer"></span>'
        + '<button class="dk-btn danger" data-a="prune" data-kind="images" data-arm="Remove untagged images?">' + icon('trash') + 'Clean untagged</button>'
        + '<button class="dk-btn danger" data-a="prune" data-kind="images-all" data-arm="Remove every unused image?">' + icon('trash') + 'Clean unused</button>'
        + '</div>'
        + '<div class="dk-quiet" style="margin-bottom:8px">' + list.length + ' images, ' + used + ' in use · ' + bytes(total) + '</div>';
      var shown = list.filter(function (i) { return matches(f, [i.name, i.tags.join(' '), i.id, i.usedBy.join(' ')]); });
      if (!list.length) html += '<div class="dk-empty">' + icon('archive') + '<b>No images</b></div>';
      else if (!shown.length) html += '<div class="dk-quiet">Nothing matches the filter.</div>';
      else html += '<div class="dk-list">' + shown.map(function (i) {
        var extra = i.tags.slice(1);
        return '<div class="dk-row plain"><div class="dk-main"><div class="dk-name">' + esc(i.name) + '</div>'
          + '<div class="dk-sub">' + esc(i.id) + ' · ' + bytes(i.size) + ' · ' + ago(i.created) + (extra.length ? ' · also ' + esc(extra.join(', ')) : '') + '</div></div>'
          + '<div class="dk-mid">' + (i.usedBy.length ? i.usedBy.slice(0, 4).map(function (n) { return '<span class="dk-chip ok link" data-a="open" data-id="' + esc(n) + '">' + esc(n) + '</span>'; }).join('') + (i.usedBy.length > 4 ? '<span class="dk-chip">+' + (i.usedBy.length - 4) + '</span>' : '') : (i.dangling ? '<span class="dk-chip warn">untagged</span>' : '<span class="dk-chip">unused</span>')) + '</div>'
          + '<div class="dk-btns">' + (i.usedBy.length
            ? '<button class="dk-btn icon" disabled title="In use: remove its containers first">' + icon('trash') + '</button>'
            : '<button class="dk-btn icon danger" data-a="rm-image" data-id="' + esc(i.fullId) + '" data-arm="Remove?" title="Remove">' + icon('trash') + '</button>') + '</div>'
          + '</div>';
      }).join('') + '</div>';
      body.innerHTML = html;
      var inp = R2('pullref');
      if (inp) inp.addEventListener('keydown', function (e) { if (e.key === 'Enter') pull(); });
    }

    async function pull() {
      var inp = R2('pullref');
      var ref = inp ? inp.value.trim() : '';
      if (!ref) { if (inp) inp.focus(); return; }
      var d = await post('/api/docker/images/pull', { reference: ref });
      if (!d.ok) { toast(d.error || 'Could not pull it', 'error'); return; }
      if (inp) inp.value = '';
      watchJob(d.job, 'Pull ' + ref);
    }

    // ── Volumes ──

    function drawVolumes() {
      var list = st.data.volumes || [];
      var f = st.filter.volumes || '';
      var unused = list.filter(function (v) { return !v.usedBy.length; }).length;
      var html = '<div class="dk-toolbar"><span class="dk-quiet">' + list.length + ' volumes, ' + unused + ' not used by any container. Sizes are under System.</span><span class="dk-spacer"></span>'
        + '<button class="dk-btn danger" data-a="prune" data-kind="volumes" data-arm="Delete every unused volume and its data?">' + icon('trash') + 'Clean unused</button></div>';
      var shown = list.filter(function (v) { return matches(f, [v.name, v.project, v.driver, v.usedBy.join(' ')]); });
      if (!list.length) html += '<div class="dk-empty">' + icon('hard-drive') + '<b>No volumes</b></div>';
      else if (!shown.length) html += '<div class="dk-quiet">Nothing matches the filter.</div>';
      else html += '<div class="dk-list">' + shown.map(function (v) {
        var name = v.anonymous ? 'anonymous ' + v.name.slice(0, 12) : v.name;
        return '<div class="dk-row plain"><div class="dk-main"><div class="dk-name" title="' + esc(v.name) + '">' + esc(name) + '</div>'
          + '<div class="dk-sub" title="' + esc(v.mountpoint) + '">' + esc(v.driver) + (v.project ? ' · ' + esc(v.project) : '') + ' · ' + ago(v.created) + '</div></div>'
          + '<div class="dk-mid">' + (v.usedBy.length ? v.usedBy.map(function (n) { return '<span class="dk-chip ok link" data-a="open" data-id="' + esc(n) + '">' + esc(n) + '</span>'; }).join('') : '<span class="dk-chip">unused</span>') + '</div>'
          + '<div class="dk-btns">' + (v.usedBy.length
            ? '<button class="dk-btn icon" disabled title="In use">' + icon('trash') + '</button>'
            : '<button class="dk-btn icon danger" data-a="rm-volume" data-id="' + esc(v.name) + '" data-arm="Delete its data?" title="Remove">' + icon('trash') + '</button>') + '</div></div>';
      }).join('') + '</div>';
      body.innerHTML = html;
    }

    // ── Forwards ──
    //
    // Crundi's own forwards (name.<domain> or /tunnel/name/ to 127.0.0.1:port),
    // matched to containers by the host port they publish. The server says
    // what is behind each forward's port: a running container, a stopped one,
    // some other program, or nothing at all, which makes the forward an orphan.

    function takeForwards(list) {
      st.fwd.list = list;
      st.fwd.at = Date.now();
      var by = {};
      list.forEach(function (fw) { (by[fw.port] = by[fw.port] || []).push(fw); });
      st.fwd.byPort = by;
    }

    async function loadForwards(force) {
      if (!force && st.fwd.list && Date.now() - st.fwd.at < 15000) return;
      st.fwd.at = Date.now();   // one request at a time, not one per poll
      var d = await getJson('/api/docker/forwards').catch(function () { return null; });
      if (!d || !d.ok) return;
      takeForwards(d.forwards || []);
      drawSeg();
      if (st.detail) drawPortsNow();
      else if (st.view === 'containers' || st.view === 'forwards') draw();
    }

    function fwdChip(fw) {
      return '<span class="dk-chip fwd link" data-a="open-url" data-url="' + esc(fw.url) + '" title="Forward: ' + esc(fw.url) + (fw.public ? ' (public)' : '') + '">'
        + icon('external-link') + esc(fw.host) + '</span>';
    }

    function slug(text) {
      return String(text || '').toLowerCase().replace(/[^a-z0-9-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 32).replace(/-+$/, '') || 'app';
    }

    function portsHtml(o) {
      if (!o.ports.length) return '<div class="dk-quiet">No ports.</div>';
      return o.ports.map(function (p) {
        var where = p.public ? (p.ip || '0.0.0.0') + ':' + p.public : 'not published';
        var right = '';
        if (p.public) {
          var fws = st.fwd.byPort[p.public] || [];
          right = fws.map(fwdChip).join('')
            + '<button class="dk-btn" data-a="fwd-add" data-port="' + p.public + '" title="Reach this port through Crundi, by name">' + icon('plus') + (fws.length ? 'Another' : 'Add forward') + '</button>';
        }
        return '<div class="dk-port"><span class="k">' + esc(p.private + '/' + p.type) + '</span><span class="v">' + esc(where) + '</span>' + right + '</div>'
          + '<div data-r="fwdform-' + (p.public || 'x') + '"></div>';
      }).join('');
    }

    function drawPortsNow() {
      var dt = st.detail, box = R2('ports');
      if (!dt || !dt.data || !box || box.querySelector('.dk-fwdform')) return;   // not under someone typing
      box.innerHTML = portsHtml(dt.data.overview);
    }

    async function openFwdForm(port) {
      var dt = st.detail;
      var blind = port === 'any';   // from the Forwards view: any port, no container
      if (!dt && !blind) return;
      var box = R2('fwdform-' + port);
      if (!box) return;
      if (!st.fwd.options) {
        var o = await getJson('/api/forwards/options').catch(function () { return null; });
        st.fwd.options = o && o.ok ? o : { choices: [] };
      }
      var sub = (st.fwd.options.choices || []).filter(function (c) { return c.id === 'subdomain' && c.available; })[0];
      var c = blind ? null : dt.data.container;
      box.innerHTML = '<div class="dk-fwdform">'
        + (blind ? '<input type="text" data-r="fwdport" inputmode="numeric" placeholder="Port" style="flex:0 0 90px;min-width:80px" autocomplete="off">' : '')
        + '<input type="text" data-r="fwdname" value="' + esc(blind ? '' : slug(c.service || c.name)) + '" placeholder="name" spellcheck="false" autocomplete="off" title="Lowercase letters, digits and hyphens">'
        + '<select data-r="fwdmode">' + (sub ? '<option value="subdomain">' + esc('name.' + (sub.suggestedDomain || st.fwd.options.domain || '')) + '</option>' : '')
        + '<option value="path">/tunnel/name/ on this server</option></select>'
        + '<label title="Reachable without signing in to Crundi. Leave off unless it has its own sign-in or must take webhooks."><input type="checkbox" data-r="fwdpublic"> Public</label>'
        + '<label title="The way a service warns when it stops: when its container stops or fails its Docker health check, or, with no container behind it, when nothing answers on the port"><input type="checkbox" data-r="fwdwatch"> Warn if it goes down</label>'
        + '<button class="dk-btn primary" data-a="fwd-create" data-port="' + port + '">' + icon('check') + 'Create</button>'
        + '<button class="dk-btn" data-a="fwd-cancel" data-port="' + port + '">Cancel</button>'
        + '</div><div class="dk-help">' + (blind ? 'Forwards 127.0.0.1 on the port you give: a container\u2019s published port, a dev server, anything listening on this machine.' : 'Forwards 127.0.0.1:' + port + '.')
        + ' Private unless Public is ticked: only people signed in to Crundi can open it.</div>';
      var inp = R2(blind ? 'fwdport' : 'fwdname');
      [R2('fwdport'), R2('fwdname')].forEach(function (el) {
        if (el) el.addEventListener('keydown', function (e) { if (e.key === 'Enter') createForward(port); if (e.key === 'Escape') { box.innerHTML = ''; } });
      });
      if (inp) { inp.focus(); inp.select(); }
    }

    async function createForward(port) {
      var dt = st.detail;
      var blind = port === 'any';
      var name = R2('fwdname'), mode = R2('fwdmode'), pub = R2('fwdpublic');
      if ((!dt && !blind) || !name) return;
      var target = blind ? Number((R2('fwdport') || {}).value) : Number(port);
      if (!(target >= 1 && target <= 65535)) { toast('Give the port to forward, 1 to 65535', 'error'); var pi = R2('fwdport'); if (pi) pi.focus(); return; }
      var d = await post('/api/forwards', {
        name: name.value.trim(), port: target, mode: mode ? mode.value : 'path', public: !!(pub && pub.checked),
        project: blind ? '' : (dt.data.overview.project || ''),
        description: blind ? '' : 'Container ' + dt.data.container.name,
      });
      if (!d.ok) { toast(d.error || 'Could not create the forward', 'error'); return; }
      var w = R2('fwdwatch');
      if (w && w.checked && d.forward) await setForwardWatch(d.forward.host, true, true);
      toast('Forward ready: ' + (d.forward && d.forward.url || ''), 'success');
      var box = R2('fwdform-' + port);
      if (box) box.innerHTML = '';
      await loadForwards(true);
      if (blind && st.view === 'forwards' && !st.detail) load(true);
    }

    var FWD_GROUPS = [
      { state: 'orphaned', title: 'Nothing behind it', help: 'No container publishes this port and nothing answers on it. Left over from something that is gone, most likely.' },
      { state: 'stopped', title: 'Container stopped', help: 'The container that publishes this port is not running.' },
      { state: 'container', title: 'Running container' },
      { state: 'other', title: 'Another program', help: 'Something on this machine answers on the port, but not a container.' },
    ];

    function drawForwards() {
      var list = st.fwd.list || st.data.forwards || [];
      var f = st.filter.forwards || '';
      if (!list.length) {
        body.innerHTML = '<div class="dk-empty">' + icon('globe') + '<b>No forwards</b><span>A forward reaches a port on this machine through Crundi, by name: for a container, or for anything else listening here.</span>'
          + '<button class="dk-btn primary" data-a="fwd-add" data-port="any">' + icon('plus') + 'Add forward</button><div data-r="fwdform-any" style="width:100%;max-width:640px;text-align:left"></div></div>';
        return;
      }
      var shown = list.filter(function (fw) { return matches(f, [fw.host, fw.url, String(fw.port), fw.project, fw.container ? fw.container.name : '', fw.state]); });
      var orphans = list.filter(function (fw) { return fw.state === 'orphaned'; });
      var html = '<div class="dk-toolbar"><span class="dk-quiet">' + list.length + ' forwards' + (orphans.length ? ', ' + orphans.length + ' with nothing behind them' : '') + '</span><span class="dk-spacer"></span>'
        + '<button class="dk-btn primary" data-a="fwd-add" data-port="any">' + icon('plus') + 'Add forward</button>'
        + (orphans.length ? '<button class="dk-btn danger" data-a="rm-orphans" data-arm="Remove ' + orphans.length + ' orphaned?">' + icon('trash') + 'Remove orphaned</button>' : '') + '</div>'
        + '<div data-r="fwdform-any"></div>';
      FWD_GROUPS.forEach(function (g) {
        var rows = shown.filter(function (fw) { return fw.state === g.state; });
        if (!rows.length) return;
        html += '<div class="dk-group">' + esc(g.title) + ' \u00b7 ' + rows.length + '</div>' + (g.help ? '<div class="dk-help">' + esc(g.help) + '</div>' : '');
        html += '<div class="dk-list">' + rows.map(function (fw) {
          var hc = fw.container ? (fw.container.health === 'unhealthy' ? 'bad' : fw.container.running ? 'ok' : '') : '';
          var ctr = fw.container ? '<span class="dk-chip ' + hc + ' link" data-a="open" data-id="' + esc(fw.container.name) + '" title="' + esc(fw.container.running ? 'Running' + (fw.container.health ? ', ' + fw.container.health : '') : 'Stopped') + '">' + esc(fw.container.name) + (fw.container.health === 'unhealthy' ? ' \u00b7 unhealthy' : '') + '</span>' : '';
          return '<div class="dk-row plain fwdrow"><div class="dk-main"><div class="dk-name">' + esc(fw.host) + (fw.public ? ' <span class="dk-chip warn">public</span>' : '') + (fw.watch ? ' <span class="dk-chip ok" title="You are warned when this goes down">' + icon('bell') + 'watched</span>' : '') + '</div>'
            + '<div class="dk-sub">port ' + esc(fw.port) + ' \u00b7 ' + (fw.mode === 'path' ? 'path' : 'subdomain') + (fw.project ? ' \u00b7 ' + esc(fw.project) : '') + (fw.description ? ' \u00b7 ' + esc(fw.description) : '') + '</div></div>'
            + '<div class="dk-mid">' + ctr + '<span class="dk-chip fwd link" data-a="open-url" data-url="' + esc(fw.url) + '">' + icon('external-link') + 'Open</span></div>'
            + '<div class="dk-btns"><button class="dk-btn icon' + (fw.watch ? ' primary' : '') + '" data-a="fwd-watch" data-host="' + esc(fw.host) + '" data-on="' + (fw.watch ? '1' : '') + '" title="' + (fw.watch ? 'Stop warning when this goes down' : 'Warn me when this goes down, the way a service warns when it stops: its container stops or fails its Docker health check, or, with no container behind it, nothing answers on the port') + '">' + icon('bell') + '</button>'
            + '<button class="dk-btn icon danger" data-a="rm-fwd" data-host="' + esc(fw.host) + '" data-arm="Remove?" title="Remove this forward">' + icon('trash') + '</button></div></div>';
        }).join('') + '</div>';
      });
      if (!shown.length) html += '<div class="dk-quiet">Nothing matches the filter.</div>';
      body.innerHTML = html;
    }

    async function removeForward(host, quiet) {
      var res = await api('/api/forwards/' + encodeURIComponent(host), { method: 'DELETE' });
      var d = await res.json().catch(function () { return { ok: res.ok }; });
      if (!d.ok && !quiet) toast(d.error || 'Could not remove ' + host, 'error');
      return !!d.ok;
    }

    async function setForwardWatch(host, on, quiet) {
      var res = await api('/api/forwards/' + encodeURIComponent(host), { method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ watch: on }) });
      var d = await res.json().catch(function () { return { ok: res.ok }; });
      if (!d.ok) { toast(d.error || 'Could not change that', 'error'); return false; }
      if (!quiet) toast(on ? 'You will be warned when ' + host + ' goes down' : 'No longer watching ' + host, 'success');
      return true;
    }

    async function removeOrphans() {
      var orphans = (st.fwd.list || []).filter(function (fw) { return fw.state === 'orphaned'; });
      var n = 0;
      for (var i = 0; i < orphans.length; i++) if (await removeForward(orphans[i].host, true)) n++;
      toast('Removed ' + n + ' orphaned forward' + (n === 1 ? '' : 's'), 'success');
      st.fwd.at = 0;
      load(true);
    }

    // ── Networks ──

    function drawNetworks() {
      var list = st.data.networks || [];
      var f = st.filter.networks || '';
      var html = '<div class="dk-toolbar"><span class="dk-quiet">' + list.length + ' networks</span><span class="dk-spacer"></span>'
        + '<button class="dk-btn danger" data-a="prune" data-kind="networks" data-arm="Remove unused networks?">' + icon('trash') + 'Clean unused</button></div>';
      var shown = list.filter(function (n) { return matches(f, [n.name, n.driver, n.subnet, n.project, n.usedBy.join(' ')]); });
      if (!shown.length) html += '<div class="dk-quiet">Nothing matches the filter.</div>';
      else html += '<div class="dk-list">' + shown.map(function (n) {
        var can = !n.builtin && !n.usedBy.length;
        return '<div class="dk-row plain"><div class="dk-main"><div class="dk-name">' + esc(n.name) + (n.builtin ? ' <span class="dk-chip">built in</span>' : '') + (n.internal ? ' <span class="dk-chip">internal</span>' : '') + '</div>'
          + '<div class="dk-sub">' + esc(n.driver) + ' · ' + esc(n.scope) + (n.subnet ? ' · ' + esc(n.subnet) : '') + (n.gateway ? ' via ' + esc(n.gateway) : '') + '</div></div>'
          + '<div class="dk-mid">' + n.usedBy.slice(0, 6).map(function (c) { return '<span class="dk-chip ok link" data-a="open" data-id="' + esc(c) + '">' + esc(c) + '</span>'; }).join('') + (n.usedBy.length > 6 ? '<span class="dk-chip">+' + (n.usedBy.length - 6) + '</span>' : '') + '</div>'
          + '<div class="dk-btns">' + (can
            ? '<button class="dk-btn icon danger" data-a="rm-network" data-id="' + esc(n.fullId) + '" data-arm="Remove?" title="Remove">' + icon('trash') + '</button>'
            : '<button class="dk-btn icon" disabled title="' + (n.builtin ? 'Docker’s own' : 'In use') + '">' + icon('trash') + '</button>') + '</div></div>';
      }).join('') + '</div>';
      body.innerHTML = html;
    }

    // ── System ──

    var diskLoading = false;
    async function loadDisk() {
      if (diskLoading) return;
      diskLoading = true;
      try {
        var d = await getJson('/api/docker/system/df');
        st.data.disk = d.ok ? d.disk : { error: d.error || 'Could not measure' };
      } catch (e) {
        st.data.disk = { error: e.message };
      } finally { diskLoading = false; }
      if (st.view === 'system' && !st.detail) draw();
    }

    function drawSystem() {
      var d = st.data.system;
      if (!d || !d.engine) { body.innerHTML = '<div class="dk-quiet">Loading…</div>'; return; }
      var e = d.engine, k = st.data.disk || null;
      var kv = function (rows) {
        return '<div class="dk-kv">' + rows.filter(function (r) { return r[1]; }).map(function (r) { return '<div class="k">' + esc(r[0]) + '</div><div class="v">' + esc(r[1]) + '</div>'; }).join('') + '</div>';
      };
      body.innerHTML = '<div class="dk-cols">'
        + '<div class="dk-sec"><h5>Engine</h5>' + kv([
          ['Docker', e.version + ' (API ' + e.apiVersion + ')'],
          ['Compose', e.compose || 'not installed'],
          ['System', e.os],
          ['Kernel', e.kernel],
          ['Machine', e.cpus + ' CPUs · ' + bytes(e.memory) + ' · ' + e.arch],
          ['Storage', e.storageDriver],
          ['Cgroups', e.cgroup],
          ['Data', e.rootDir],
        ]) + '</div>'
        + '<div class="dk-sec"><h5>Contents</h5>' + kv([
          ['Containers', e.containers + ' (' + e.running + ' running, ' + e.paused + ' paused, ' + e.stopped + ' stopped)'],
          ['Images', String(e.images)],
        ]) + (e.warnings.length ? '<div style="height:8px"></div>' + e.warnings.map(function (w) { return '<div class="dk-warn">' + esc(w) + '</div>'; }).join('') : '') + '</div>'
        + '</div><div style="height:12px"></div>'
        + '<div class="dk-sec"><h5>Disk use</h5>' + (!k ? '<div class="dk-quiet">Measuring\u2026 Docker walks every layer for this, which can take a while.</div></div>'
          : k.error ? '<div class="dk-quiet">' + esc(k.error) + '</div></div>' : diskTable(k));
    }

    function diskTable(k) {
      var row = function (label, x, kind, arm, note) {
        if (!x) return '';
        return '<tr><td>' + esc(label) + (note ? '<div class="dk-sub">' + esc(note) + '</div>' : '') + '</td><td class="n hide-m">' + x.count + '</td><td class="n">' + bytes(x.size) + '</td><td class="n">' + bytes(x.reclaimable) + '</td>'
          + '<td class="n"><button class="dk-btn danger" data-a="prune" data-kind="' + kind + '" data-arm="' + esc(arm) + '"' + (x.reclaimable || kind === 'containers' ? '' : ' disabled') + '>' + icon('trash') + 'Clean</button></td></tr>';
      };
      return '<table class="dk-disk"><thead><tr><th>What</th><th class="n hide-m">Count</th><th class="n">Size</th><th class="n">Reclaimable</th><th></th></tr></thead><tbody>'
        + row('Images', k.images, 'images-all', 'Remove every image no container uses?', 'Cleaning removes every image no container uses')
        + row('Stopped containers', k.containers, 'containers', 'Remove every stopped container?', 'Their writable layers; running ones are kept')
        + row('Volumes', k.volumes, 'volumes', 'Delete every unused volume and its data?', 'Data in volumes no container uses. Deleted for good.')
        + row('Build cache', k.build, 'build', 'Clear the whole build cache?', 'Rebuilt on the next build')
        + '</tbody></table>'
        + '<div class="dk-note">Docker is shared by every project on this machine. A stopped container may be one someone means to start again.</div></div>';
    }

    async function prune(kind) {
      var d = await post('/api/docker/prune', { kind: kind });
      if (!d.ok) { toast(d.error || 'Clean-up failed', 'error'); return; }
      toast('Removed ' + d.removed + (d.reclaimed ? ', freed ' + bytes(d.reclaimed) : ''), 'success');
      st.data.disk = null;
      load(true);
    }

    async function removeThing(path, payload, what) {
      var d = await post(path, payload);
      if (!d.ok) { toast(d.error || ('Could not remove the ' + what), 'error'); }
      else toast('Removed ' + d.name, 'success');
      load(false);
    }

    // ── Jobs (compose and pull output) ──

    function watchJob(id, title) {
      st.job = { id: id, title: title, text: '', next: 0, done: false, code: null };
      drawJob();
      pollJob(st.job);
    }

    async function pollJob(job) {
      while (st.job === job && !job.done) {
        var d = await getJson('/api/docker/job?id=' + encodeURIComponent(job.id) + '&from=' + job.next).catch(function () { return null; });
        if (st.job !== job) return;
        if (!d || !d.ok) { job.done = true; job.code = -1; job.text += '\n(lost track of this job)'; drawJob(); break; }
        job.text += d.text; job.next = d.next; job.done = d.done; job.code = d.code;
        if (job.text.length > 400000) job.text = job.text.slice(-300000);
        drawJob();
        if (!job.done) await new Promise(function (r) { setTimeout(r, 700); });
      }
      if (st.job === job) {
        toast(job.title + (job.code === 0 ? ' finished' : ' failed (exit ' + job.code + ')'), job.code === 0 ? 'success' : 'error');
        load(false);
      }
    }

    function drawJob() {
      var host = R('job');
      var job = st.job;
      if (!job) { host.innerHTML = ''; return; }
      var status = job.done
        ? (job.code === 0 ? '<span class="dk-chip ok">done</span>' : '<span class="dk-chip bad">exit ' + esc(job.code) + '</span>')
        : '<span class="dk-spin"></span>';
      if (!host.firstChild) {
        host.innerHTML = '<div class="dk-job"><div class="h">' + icon('terminal') + '<b></b><span data-r="jstat"></span>'
          + '<button class="dk-btn icon" data-a="job-close" title="Hide (it keeps running)">' + icon('x') + '</button></div><pre></pre></div>';
      }
      host.querySelector('.h b').textContent = job.title;
      host.querySelector('[data-r="jstat"]').innerHTML = status;
      var pre = host.querySelector('pre');
      var atEnd = pre.scrollTop + pre.clientHeight >= pre.scrollHeight - 20;
      pre.textContent = job.text;
      if (atEnd) pre.scrollTop = pre.scrollHeight;
    }

    // ── Confirm-by-second-click ──
    // The daemon is shared: a stop or a remove here can be someone else's.
    // Destructive buttons carry data-arm; the first click arms, the second acts.

    function armed(btn) {
      if (!btn.dataset.arm) return true;
      if (btn.classList.contains('armed')) {
        clearTimeout(btn._armT);
        btn.classList.remove('armed');
        return true;
      }
      btn._label = btn.innerHTML;
      btn.classList.add('armed');
      btn.innerHTML = icon('check') + esc(btn.dataset.arm);
      btn._armT = setTimeout(function () {
        btn.classList.remove('armed');
        btn.innerHTML = btn._label;
      }, 5000);
      return false;
    }

    // ── Events ──

    root.addEventListener('click', function (e) {
      var t = e.target.closest('[data-a]');
      if (!t || !root.contains(t)) return;
      var a = t.dataset.a;
      // A button inside a clickable row acts, it does not open the row.
      if (a === 'open' && e.target.closest('button') && e.target.closest('button') !== t) return;
      if (t.tagName === 'BUTTON' && t.disabled) return;
      if (t.tagName === 'BUTTON') e.stopPropagation();
      switch (a) {
        case 'view': setView(t.dataset.v); break;
        case 'refresh': if (st.detail) refreshDetailHead(true); else load(false); break;
        case 'open': openDetail(t.dataset.id); break;
        case 'back': setView(st.view); break;
        case 'dtab': selectDetailTab(t.dataset.t); break;
        case 'ctr': if (armed(t)) containerVerb(t.dataset.id, t.dataset.verb, t); break;
        case 'rename-open': openPrompt('rename'); break;
        case 'remove-open': openPrompt('remove'); break;
        case 'prompt-close': R2('prompt').innerHTML = ''; break;
        case 'rename-do': doRename(); break;
        case 'remove-do': doRemove(); break;
        case 'log-clear': if (st.detail) { st.detail.lines = []; R2('logs').innerHTML = ''; } break;
        case 'log-copy': copyText(st.detail ? st.detail.lines.map(function (l) { return (R2('logts').checked && l.ts ? l.ts + ' ' : '') + stripAnsi(l.text); }).join('\n') : ''); break;
        case 'inspect-copy': copyText(R2('inspect').textContent); break;
        case 'shell-again': openShell(); break;
        case 'stack': if (armed(t)) stackAction(t.dataset.name, t.dataset.act); break;
        case 'pull': pull(); break;
        case 'prune': if (armed(t)) prune(t.dataset.kind); break;
        case 'rm-image': if (armed(t)) removeThing('/api/docker/images/remove', { id: t.dataset.id }, 'image'); break;
        case 'rm-volume': if (armed(t)) removeThing('/api/docker/volumes/remove', { name: t.dataset.id }, 'volume'); break;
        case 'rm-network': if (armed(t)) removeThing('/api/docker/networks/remove', { id: t.dataset.id }, 'network'); break;
        case 'open-url': if (t.dataset.url) window.open(t.dataset.url, '_blank', 'noopener'); break;
        case 'fwd-add': openFwdForm(t.dataset.port); break;
        case 'fwd-create': createForward(t.dataset.port); break;
        case 'fwd-cancel': { var fb = R2('fwdform-' + t.dataset.port); if (fb) fb.innerHTML = ''; } break;
        case 'rm-fwd': if (armed(t)) removeForward(t.dataset.host).then(function (ok) { if (ok) toast('Removed ' + t.dataset.host, 'success'); st.fwd.at = 0; load(true); }); break;
        case 'rm-orphans': if (armed(t)) removeOrphans(); break;
        case 'fwd-watch': setForwardWatch(t.dataset.host, !t.dataset.on).then(function () { st.fwd.at = 0; load(true); }); break;
        case 'job-close': st.job = null; drawJob(); break;
      }
    });

    root.addEventListener('input', function (e) {
      var t = e.target;
      if (t === filterEl) { st.filter[st.view] = t.value; draw(); return; }
      if (t.dataset.r === 'logfilter') { clearTimeout(t._t); t._t = setTimeout(redrawLogs, 150); }
    });
    root.addEventListener('change', function (e) {
      var t = e.target, box = R2('logs');
      if (!box) return;
      if (t.dataset.r === 'logts') box.classList.toggle('nots', !t.checked);
      if (t.dataset.r === 'logwrap') box.classList.toggle('wrap', t.checked);
      if (t.dataset.r === 'logfollow' && t.checked) box.scrollTop = box.scrollHeight;
    });

    function copyText(s) {
      if (!s) return;
      var done = function () { toast('Copied'); };
      if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(s).then(done, fallback);
      else fallback();
      function fallback() {
        var ta = document.createElement('textarea');
        ta.value = s; ta.style.position = 'fixed'; ta.style.opacity = '0';
        document.body.appendChild(ta); ta.select();
        try { document.execCommand('copy'); done(); } catch (e) { toast('Could not copy', 'error'); }
        ta.remove();
      }
    }

    // ── Status gate ──

    async function checkStatus() {
      var s = await getJson('/api/docker/status');
      st.status = s;
      if (!s.installed) {
        body.innerHTML = '<div class="dk-empty">' + icon('docker') + '<b>Docker is not installed</b><span>Install Docker Engine on this machine and this tab will manage it.</span></div>';
        nav.style.display = 'none';
        return false;
      }
      if (!s.reachable) {
        body.innerHTML = '<div class="dk-empty">' + icon('docker') + '<b>Docker is installed, but Crundi cannot reach it</b><span>' + esc(s.error || '') + '</span>'
          + '<button class="dk-btn" data-a="recheck">' + icon('refresh') + 'Try again</button></div>';
        nav.style.display = 'none';
        var b = body.querySelector('[data-a="recheck"]');
        if (b) b.onclick = function () { getJson('/api/docker/status?force=1').then(function () { view.show(); }); };
        return false;
      }
      nav.style.display = '';
      return true;
    }

    // ── Socket messages ──

    function onMessage(msg) {
      var dt = st.detail;
      if (!dt) return;
      switch (msg.type) {
        case 'docker-logs': if (msg.ref === dt.logRef) addFrames(msg.frames || []); break;
        case 'docker-logs-end': if (msg.ref === dt.logRef) { dt.logRef = null; flushPartial(); sysLine(dt.data.container.running ? 'The log stream ended.' : 'The container stopped. Start it to follow the log again.'); } break;
        case 'docker-logs-error': if (msg.ref === dt.logRef) { dt.logRef = null; sysLine(msg.error || 'Could not follow the log'); } break;
        case 'docker-shell-data': if (msg.ref === dt.shellRef && dt.term) dt.term.write(msg.data); break;
        case 'docker-shell-exit':
          if (msg.ref === dt.shellRef) {
            dt.shellRef = null;
            drawShellBar(msg.code === 126 || msg.code === 127 ? 'This image may not include a shell (exit ' + msg.code + ').' : 'The shell ended (exit ' + msg.code + ').');
          }
          break;
        case 'docker-shell-error': if (msg.ref === dt.shellRef) { dt.shellRef = null; drawShellBar(msg.error || 'Could not open a shell'); } break;
      }
    }

    function flushPartial() {
      var dt = st.detail;
      if (!dt) return;
      var rest = [];
      ['o', 'e'].forEach(function (k) { if (dt.partial[k]) { rest.push({ stream: k === 'e' ? 'stderr' : 'stdout', text: dt.partial[k] + '\n' }); dt.partial[k] = ''; } });
      if (rest.length) addFrames(rest);
    }

    function onReconnect() {
      var dt = st.detail;
      if (!dt || !st.visible) return;
      // The server dropped our streams with the old socket.
      if (dt.logRef || dt.tab === 'logs') followLogs();
      if (dt.shellRef) { dt.shellRef = null; drawShellBar('The connection dropped, which ended the shell.'); }
    }

    var view = {
      show: async function () {
        st.visible = true;
        if (!await checkStatus()) return;
        if (st.detail) {
          selectDetailTab(st.detail.tab);
          refreshDetailHead(true);
          if (st.detail && !st.detail.logRef && st.detail.tab === 'logs') followLogs();
        } else {
          drawSeg();
          if (!st.data[st.view]) body.innerHTML = '<div class="dk-quiet">Loading…</div>';
          else draw();
          await load(true);
        }
        schedule();
      },
      hide: function () {
        st.visible = false;
        clearTimeout(st.timer);
        stopStats();
        // Logs stop following while nobody looks; a shell is left alone, since
        // whatever runs in it would die with it.
        stopLogs();
      },
      onMessage: onMessage,
      onReconnect: onReconnect,
    };
    active = view;
    return view;
  }

  window.CrundiDocker = {
    mount: mount,
    handleMessage: function (msg) { if (active) active.onMessage(msg); },
    onReconnect: function () { if (active) active.onReconnect(); },
  };
})();
