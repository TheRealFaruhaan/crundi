/**
 * crundi-workers.js — the Workers tab: the owner's PCs (and Linux servers)
 * joined to this server as workers (src/workers.js, specs/workers-protocol.md).
 *
 *   List     each PC: online, paused, what it is (CPU, RAM, GPU, disk), labels
 *   Add a PC a one-time join code and how to use it
 *   A PC     rename, allow chats / commands, pause, labels, tools, its log,
 *            history, a terminal on it, revoke / rejoin / remove
 *
 * Plain script outside webapp-html.js (that file is one template literal).
 *
 *   window.CrundiWorkers.mount(el, opts) -> view { show(), hide(), refresh() }
 *   window.CrundiWorkers.handleMessage(msg)   worker-term-* and {type:"workers"}
 *
 * opts: apiFetch(path, init), toast(msg, kind), icon(name) -> svg,
 *       send(msg) -> bool (the page's websocket), Terminal, FitAddon,
 *       termTheme, termFont, serverUrl() -> string
 */
(function () {
  'use strict';

  var STYLE_ID = 'wk-styles';
  var CSS = [
    '.wk-root{display:flex;flex-direction:column;height:100%;min-height:0;color:var(--text-primary)}',
    '.wk-nav{display:flex;align-items:center;gap:8px;padding:10px 14px;border-bottom:1px solid var(--border);flex:none;flex-wrap:wrap}',
    '.wk-nav .t{font-weight:600;font-size:.92rem}',
    '.wk-nav .s{color:var(--text-muted);font-size:.74rem}',
    '.wk-spacer{flex:1}',
    '.wk-body{flex:1;min-height:0;overflow:auto;padding:14px}',
    '.wk-card{border:1px solid var(--border);border-radius:var(--radius-sm,8px);background:var(--bg-secondary);margin-bottom:12px;overflow:hidden}',
    '.wk-card>.hd{display:flex;align-items:center;gap:8px;padding:10px 12px;border-bottom:1px solid var(--border-subtle,var(--border));flex-wrap:wrap}',
    '.wk-card>.hd .t{font-weight:600;font-size:.88rem}',
    '.wk-card>.bd{padding:10px 12px}',
    '.wk-row{display:flex;align-items:flex-start;gap:10px;padding:10px 12px;border-bottom:1px solid var(--border-subtle,var(--border));cursor:pointer}',
    '.wk-row:last-child{border-bottom:none}',
    '.wk-row:hover{background:var(--bg-hover)}',
    '.wk-row .main{flex:1;min-width:0}',
    '.wk-row .nm{font-size:.86rem;font-weight:600;display:flex;align-items:center;gap:6px;flex-wrap:wrap}',
    '.wk-meta{font-size:.74rem;color:var(--text-muted);line-height:1.55;margin-top:2px;word-break:break-word}',
    '.wk-dot{width:8px;height:8px;border-radius:50%;flex:none;margin-top:6px;background:var(--text-muted)}',
    '.wk-dot.up{background:var(--green);box-shadow:0 0 0 3px var(--green-dim)}',
    '.wk-dot.paused{background:var(--yellow);box-shadow:0 0 0 3px var(--yellow-dim)}',
    '.wk-dot.bad{background:var(--red)}',
    '.wk-chips{display:flex;gap:4px;flex-wrap:wrap;margin-top:5px}',
    '.wk-chip{display:inline-flex;align-items:center;font-size:.64rem;font-family:var(--mono);padding:1px 7px;border-radius:999px;background:var(--bg-tertiary);color:var(--text-secondary);white-space:nowrap}',
    '.wk-chip.own{background:var(--accent-dim);color:var(--accent)}',
    '.wk-chip.ok{background:var(--green-dim);color:var(--green)}',
    '.wk-chip.warn{background:var(--yellow-dim);color:var(--yellow)}',
    '.wk-chip.bad{background:var(--red-dim);color:var(--red)}',
    '.wk-btn{display:inline-flex;align-items:center;gap:5px;cursor:pointer;padding:5px 10px;border-radius:var(--radius-sm,6px);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-secondary);font-size:.76rem;white-space:nowrap;line-height:1.2}',
    '.wk-btn:hover{border-color:var(--accent);color:var(--text-primary)}',
    '.wk-btn.primary{background:var(--accent);border-color:var(--accent);color:#fff}',
    '.wk-btn.danger:hover{border-color:var(--red);color:var(--red);background:var(--red-dim)}',
    '.wk-btn.armed{border-color:var(--red);color:#fff;background:var(--red)}',
    '.wk-btn .ic{width:13px;height:13px}',
    '.wk-btns{display:flex;gap:6px;flex-wrap:wrap}',
    '.wk-code{font-family:var(--mono);font-size:1.6rem;letter-spacing:.18em;font-weight:600;margin:4px 0}',
    '.wk-cmd{display:flex;align-items:center;gap:6px;font-family:var(--mono);font-size:.74rem;background:var(--bg-primary);border:1px solid var(--border);border-radius:6px;padding:6px 8px;margin:4px 0;word-break:break-all}',
    '.wk-cmd span{flex:1}',
    '.wk-hint{font-size:.74rem;color:var(--text-muted);line-height:1.55}',
    '.wk-quiet{color:var(--text-muted);font-size:.82rem;padding:18px 4px;line-height:1.6}',
    '.wk-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:10px}',
    '.wk-stat .v{font-family:var(--mono);font-size:.92rem;font-variant-numeric:tabular-nums;word-break:break-word}',
    '.wk-stat .l{font-size:.62rem;text-transform:uppercase;letter-spacing:.07em;color:var(--text-muted)}',
    '.wk-toggle{display:flex;align-items:center;gap:8px;font-size:.82rem;padding:5px 0;cursor:pointer}',
    '.wk-input{background:var(--bg-primary);border:1px solid var(--border);border-radius:6px;color:var(--text-primary);padding:6px 8px;font-size:.8rem;width:100%;box-sizing:border-box}',
    '.wk-tools{display:grid;grid-template-columns:repeat(auto-fill,minmax(140px,1fr));gap:4px 12px;font-size:.76rem}',
    '.wk-tools .k{color:var(--text-secondary)}.wk-tools .v{font-family:var(--mono);color:var(--text-muted)}',
    '.wk-log{font-family:var(--mono);font-size:.72rem;line-height:1.5;max-height:200px;overflow:auto;white-space:pre-wrap;word-break:break-word}',
    '.wk-log .warn{color:var(--yellow)}.wk-log .error{color:var(--red)}',
    '.wk-term-wrap{border:1px solid var(--border);border-radius:var(--radius-sm,8px);overflow:hidden;background:#0d1117}',
    '.wk-termbar{display:flex;align-items:center;gap:8px;padding:6px 10px;font-size:.74rem;color:var(--text-muted);background:var(--bg-secondary);border-bottom:1px solid var(--border)}',
    '.wk-term{height:360px;padding:4px}',
    '.wk-job{display:flex;gap:8px;align-items:flex-start;padding:7px 0;border-bottom:1px solid var(--border-subtle,var(--border));cursor:pointer}',
    '.wk-job:last-child{border-bottom:none}',
    '.wk-job .cmd{font-family:var(--mono);font-size:.76rem;word-break:break-all}',
    '.wk-joblog{font-family:var(--mono);font-size:.72rem;line-height:1.45;background:#0d1117;color:#d6deeb;border-radius:6px;padding:8px;max-height:360px;overflow:auto;white-space:pre-wrap;word-break:break-word;margin-top:8px}',
    '@media (max-width:640px){.wk-term{height:300px}.wk-code{font-size:1.3rem}}',
  ].join('\n');

  var active = null;

  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]; }); }
  function gb(mb) { return mb == null ? '' : (Math.round(mb / 102.4) / 10) + ' GB'; }
  function ago(iso) {
    var t = Date.parse(iso || ''); if (!t) return 'never';
    var s = Math.max(0, (Date.now() - t) / 1000);
    if (s < 60) return 'just now';
    if (s < 3600) return Math.floor(s / 60) + ' min ago';
    if (s < 86400) return Math.floor(s / 3600) + ' h ago';
    return Math.floor(s / 86400) + ' d ago';
  }
  var KIND = { tray: 'Tray app', headless: 'Headless', electron: 'Crundi PC client', native: 'Crundi PC client' };
  var OS = { windows: 'Windows', macos: 'macOS', linux: 'Linux' };

  function mount(el, opts) {
    if (!document.getElementById(STYLE_ID)) { var s = document.createElement('style'); s.id = STYLE_ID; s.textContent = CSS; document.head.appendChild(s); }
    var icon = opts.icon || function () { return ''; };
    var st = { visible: false, data: null, detail: null, adding: null, armed: '', tick: null, refSeq: 0, term: null };
    el.innerHTML = '<div class="wk-root"><div class="wk-nav" data-r="nav"></div><div class="wk-body" data-r="body"></div></div>';
    var nav = el.querySelector('[data-r="nav"]');
    var body = el.querySelector('[data-r="body"]');

    async function api(method, payload) {
      var r = await opts.apiFetch('/api/workers', method === 'GET' ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
      if (r.status === 404) throw new Error('This server does not support workers yet. Update the server to use them.');
      var d = await r.json().catch(function () { return {}; });
      if (!r.ok || d.ok === false) throw new Error(d.error || ('Failed (' + r.status + ')'));
      return d;
    }
    async function load() {
      try { st.data = await api('GET'); } catch (e) { if (!st.data) body.innerHTML = '<div class="wk-quiet">' + esc(e.message) + '</div>'; return; }
      if (st.detail) {
        try { st.detail.jobs = (await api('POST', { action: 'jobs', id: st.detail.id })).jobs; } catch (e) { st.detail.jobs = st.detail.jobs || []; }
        if (st.detail.openJob) { try { st.detail.jobView = (await api('POST', { action: 'job-log', job: st.detail.openJob, tailBytes: 65536 })).job; } catch (e) { /* gone */ } }
      }
      draw();
    }
    // A running job's output pushes "workers" often: refresh at most once a second.
    var lastLoad = 0, soonTimer = null;
    function soon() {
      if (soonTimer) return;
      var wait = Math.max(0, 1000 - (Date.now() - lastLoad));
      soonTimer = setTimeout(function () { soonTimer = null; lastLoad = Date.now(); load(); }, wait);
    }
    function worker(id) { return st.data && st.data.workers.filter(function (w) { return w.id === id; })[0]; }

    function draw() {
      if (!st.visible) return;
      if (st.detail) return drawDetail();
      var ws = (st.data && st.data.workers) || [];
      var on = ws.filter(function (w) { return w.online; }).length;
      nav.innerHTML = '<span class="t">Workers</span><span class="s">' + ws.length + ' PC' + (ws.length === 1 ? '' : 's') + (ws.length ? ', ' + on + ' online' : '') + '</span><span class="wk-spacer"></span>'
        + '<button class="wk-btn primary" data-a="add">' + icon('plus') + 'Add a PC</button>';
      var h = '';
      if (st.adding) h += addCard();
      if (!ws.length && !st.adding) {
        h += '<div class="wk-quiet">No PCs yet. A worker is a PC (or a Linux server) that this server can use for heavy work: builds, renders, and later whole chats. Add one with <b>Add a PC</b>: the Crundi PC client has a switch for it, and other machines run the small crundi-worker program.</div>';
      }
      if (ws.length) {
        h += '<div class="wk-card">';
        ws.forEach(function (w) { h += row(w); });
        h += '</div>';
      }
      body.innerHTML = h;
    }

    function summary(w) {
      var i = w.info || {}, bits = [];
      if (i.os) bits.push((OS[i.os] || i.os) + (i.arch ? ' ' + i.arch : ''));
      if (i.cpu && i.cpu.threads) bits.push(i.cpu.threads + ' threads');
      if (i.ramMb) bits.push(gb(i.ramMb) + ' RAM');
      (i.gpus || []).forEach(function (g) { if (g.name) bits.push(g.name); });
      if (i.disk && i.disk.freeMb != null) bits.push(gb(i.disk.freeMb) + ' free');
      return bits.join(' · ');
    }
    function stateChip(w) {
      if (w.revoked) return '<span class="wk-chip bad">revoked</span>';
      if (w.paused) return '<span class="wk-chip warn">paused' + (w.pausedBy === 'pc' ? ' on the PC' : '') + '</span>';
      if (w.busy) return '<span class="wk-chip ok">running a job</span>';
      return '';
    }
    // How it is kept up to date: by itself (tray, headless), or with the PC app.
    function updWord(w) { return w.updates === 'self' ? 'updates itself' : w.updates === 'app' ? 'updates with the PC app' : ''; }
    function dotOf(w) { return w.revoked ? 'bad' : !w.online ? '' : w.paused ? 'paused' : 'up'; }
    function row(w) {
      var labels = (w.labels || []).slice(0, 14).map(function (l) { return '<span class="wk-chip' + ((w.customLabels || []).indexOf(l) >= 0 ? ' own' : '') + '">' + esc(l) + '</span>'; }).join('');
      return '<div class="wk-row" data-a="open" data-id="' + esc(w.id) + '"><span class="wk-dot ' + dotOf(w) + '"></span><div class="main">'
        + '<div class="nm">' + esc(w.name) + ' ' + stateChip(w) + '<span class="wk-chip">' + esc(KIND[w.kind] || w.kind || '') + '</span></div>'
        + '<div class="wk-meta">' + (w.online ? 'Online' : 'Offline, last seen ' + esc(ago(w.lastSeenAt))) + (w.version ? ' · v' + esc(w.version) + (updWord(w) ? ', ' + updWord(w) : '') : '') + (summary(w) ? ' · ' + esc(summary(w)) : '') + '</div>'
        + (labels ? '<div class="wk-chips">' + labels + '</div>' : '') + '</div></div>';
    }

    function addCard() {
      var a = st.adding, left = Math.max(0, Math.round((Date.parse(a.expiresAt) - Date.now()) / 1000));
      var url = (opts.serverUrl && opts.serverUrl()) || location.origin;
      var cmd = 'crundi-worker join ' + url + ' ' + a.code;
      return '<div class="wk-card"><div class="hd"><span class="t">Add a PC</span><span class="wk-spacer"></span><button class="wk-btn" data-a="add-close">' + icon('x') + 'Done</button></div><div class="bd">'
        + '<div class="wk-hint">On a PC with the <b>Crundi PC client</b>: Settings, <b>Use this PC as a worker</b>. No code needed.</div>'
        + '<div class="wk-hint" style="margin-top:10px">On any other PC or a Linux server, run the crundi-worker program with this code (the tray app asks for it on its first run):</div>'
        + '<div class="wk-code">' + esc(a.code) + '</div>'
        + '<div class="wk-cmd"><span>' + esc(cmd) + '</span><button class="wk-btn" data-a="copy" data-text="' + esc(cmd) + '">' + icon('copy') + 'Copy</button></div>'
        + '<div class="wk-hint">' + (left > 0 ? 'Works once, for ' + Math.floor(left / 60) + ':' + ('0' + (left % 60)).slice(-2) + ' more.' : 'This code has expired.') + ' On a Linux server, add <span style="font-family:var(--mono)">crundi-worker install-service</span> to keep it running.</div>'
        + '</div></div>';
    }

    // ── One PC ──
    function drawDetail() {
      var w = worker(st.detail.id);
      if (!w) { st.detail = null; closeTerm(); return draw(); }
      var i = w.info || {};
      nav.innerHTML = '<button class="wk-btn" data-a="back">' + icon('chevron-left') + 'Workers</button><span class="wk-dot ' + dotOf(w) + '" style="margin-top:0"></span><span class="t">' + esc(w.name) + '</span>' + stateChip(w) + '<span class="s">' + (w.online ? 'online since ' + esc(ago(w.connectedSince)) : 'offline, last seen ' + esc(ago(w.lastSeenAt))) + '</span>';
      var stats = [
        ['Runs as', (KIND[w.kind] || w.kind || '') + (w.version ? ' ' + w.version : '')],
        ['Updates', updWord(w) ? updWord(w).charAt(0).toUpperCase() + updWord(w).slice(1) : ''],
        ['System', i.os ? (OS[i.os] || i.os) + ' ' + (i.osVersion || '') + ' ' + (i.arch || '') : ''],
        ['CPU', i.cpu ? (i.cpu.model || '') + (i.cpu.threads ? ' (' + i.cpu.threads + ' threads)' : '') : ''],
        ['Memory', i.ramMb ? gb(i.ramMb) + (i.ramFreeMb != null ? ', ' + gb(i.ramFreeMb) + ' free' : '') : ''],
        ['GPU', (i.gpus || []).map(function (g) { return g.name + (g.vramMb ? ' (' + gb(g.vramMb) + ')' : ''); }).join(', ') || 'none found'],
        ['Work folder', i.workDir ? i.workDir + (i.disk && i.disk.freeMb != null ? ' (' + gb(i.disk.freeMb) + ' free)' : '') : ''],
        ['Load', i.load ? i.load.map(function (x) { return x == null ? '-' : x.toFixed(2); }).join(' ') : ''],
        ['Host', i.hostname || ''],
      ].filter(function (x) { return x[1]; });
      var tools = Object.keys(i.tools || {}).sort().map(function (k) { return '<div><span class="k">' + esc(k) + '</span> <span class="v">' + esc(i.tools[k]) + '</span></div>'; }).join('');
      var logs = (w.logs || []).slice(-50).map(function (l) { return '<div class="' + esc(l.level) + '">' + esc(l.at.slice(11, 19)) + '  ' + esc(l.text) + '</div>'; }).join('');
      var h = '';
      h += '<div class="wk-card"><div class="bd"><div class="wk-grid">' + stats.map(function (x) { return '<div class="wk-stat"><div class="l">' + esc(x[0]) + '</div><div class="v">' + esc(x[1]) + '</div></div>'; }).join('') + '</div>'
        + '<div class="wk-chips" style="margin-top:10px">' + (w.labels || []).map(function (l) { return '<span class="wk-chip' + ((w.customLabels || []).indexOf(l) >= 0 ? ' own' : '') + '">' + esc(l) + '</span>'; }).join('') + '</div></div></div>';
      // Terminal
      h += '<div class="wk-card"><div class="hd"><span class="t">Terminal</span><span class="wk-spacer"></span>'
        + (w.online ? '<button class="wk-btn" data-a="term">' + icon('terminal') + (st.term ? 'New terminal' : 'Open a terminal') + '</button>' + (st.term ? '<button class="wk-btn" data-a="term-close">' + icon('x') + 'Close</button>' : '') : '<span class="wk-hint">When the PC is online</span>')
        + '</div><div class="bd" data-r="termhost">' + (st.term ? '' : '<div class="wk-hint">A shell on the PC, in its work folder: sign in to GitHub there (gh auth login), install a tool, look around.</div>') + '</div></div>';
      // Jobs: commands chats ran here (worker_run)
      h += jobsCard(w);
      // Settings
      h += '<div class="wk-card"><div class="hd"><span class="t">Settings</span></div><div class="bd">'
        + '<div style="display:flex;gap:6px;margin-bottom:8px"><input class="wk-input" data-r="name" value="' + esc(w.name) + '" maxlength="60"><button class="wk-btn" data-a="rename">' + icon('check') + 'Rename</button></div>'
        + toggle('allowCommands', w.allowCommands, 'Allow commands', 'Builds, renders and other commands from chats and factories.')
        + toggle('allowChats', w.allowChats, 'Allow chats', 'Whole chats running on this PC.')
        + toggle('paused', w.paused, 'Paused', 'Nothing new starts here. The terminal still works.')
        + toggle('notOnBattery', w.notOnBattery, 'Not on battery', 'Jobs wait while this PC runs on battery.')
        + '<div style="display:flex;gap:6px;align-items:center;margin-top:6px"><span class="wk-hint" style="flex:1">Only when I am away: start jobs only after this PC has been idle for (minutes, 0 for any time)' + (w.info && w.info.idleSec == null ? '. This PC cannot tell when it is idle, so this has no effect on it.' : '') + '</span><input class="wk-input" data-r="away" type="number" min="0" max="240" style="width:80px" value="' + esc(w.onlyWhenAwayMin || 0) + '"><button class="wk-btn" data-a="away">' + icon('check') + 'Save</button></div>'
        + '<div class="wk-hint" style="margin-top:8px">Your labels (as well as the ones made from what the PC has), for picking PCs by what they can do:</div>'
        + '<div style="display:flex;gap:6px;margin-top:4px"><input class="wk-input" data-r="labels" value="' + esc((w.customLabels || []).join(', ')) + '" placeholder="fast, studio"><button class="wk-btn" data-a="labels">' + icon('check') + 'Save</button></div>'
        + '</div></div>';
      if (tools) h += '<div class="wk-card"><div class="hd"><span class="t">Tools found</span></div><div class="bd"><div class="wk-tools">' + tools + '</div></div></div>';
      if (logs) h += '<div class="wk-card"><div class="hd"><span class="t">From the PC</span></div><div class="bd"><div class="wk-log">' + logs + '</div></div></div>';
      h += '<div class="wk-card"><div class="hd"><span class="t">History</span><span class="wk-spacer"></span><button class="wk-btn" data-a="history">' + icon('clock') + (st.detail.history ? 'Refresh' : 'Show') + '</button></div>'
        + (st.detail.history ? '<div class="bd"><div class="wk-log">' + (st.detail.history.length ? st.detail.history.map(function (e) { return '<div>' + esc(e.at.replace('T', ' ').slice(0, 19)) + '  ' + esc(e.type) + (e.by ? ' (' + esc(e.by) + ')' : '') + (e.to ? ': ' + esc(e.to) : '') + '</div>'; }).join('') : 'Nothing yet.') + '</div></div>' : '') + '</div>';
      h += '<div class="wk-card"><div class="bd"><div class="wk-btns">'
        + (w.revoked ? '<button class="wk-btn" data-a="reissue">' + icon('refresh') + 'Let it join again</button>' : '<button class="wk-btn danger" data-a="revoke">' + icon('lock') + 'Revoke its key</button>')
        + '<button class="wk-btn danger" data-a="remove">' + icon('trash') + 'Remove</button></div>'
        + (st.detail.rejoin ? '<div class="wk-hint" style="margin-top:8px">On the PC: <span style="font-family:var(--mono)">crundi-worker join ' + esc((opts.serverUrl && opts.serverUrl()) || location.origin) + ' ' + esc(st.detail.rejoin) + '</span> (works once, 10 minutes)</div>' : '')
        + '<div class="wk-hint" style="margin-top:8px">Revoke cuts it off at once and keeps its record; Remove forgets it.</div></div></div>';
      var keepTerm = st.term && st.term.host;
      var keepScroll = body.scrollTop;
      body.innerHTML = h;
      body.scrollTop = keepScroll;
      var jl = body.querySelector('.wk-joblog'); if (jl) jl.scrollTop = jl.scrollHeight;
      if (keepTerm) body.querySelector('[data-r="termhost"]').appendChild(keepTerm);
    }
    function dur(ms) { if (ms == null) return ''; var s2 = Math.round(ms / 1000); return s2 < 60 ? s2 + ' s' : s2 < 3600 ? Math.floor(s2 / 60) + ' min ' + (s2 % 60) + ' s' : Math.floor(s2 / 3600) + ' h ' + Math.floor((s2 % 3600) / 60) + ' min'; }
    function jobChip(j) {
      var cls = j.state === 'done' ? 'ok' : (j.state === 'failed' || j.state === 'lost' || j.state === 'error' || j.state === 'refused') ? 'bad' : j.running ? 'warn' : '';
      var word = j.running ? (j.offline ? 'PC away' : (j.phase || j.state)) : j.state === 'failed' ? 'failed (exit ' + j.exitCode + ')' : j.state;
      return '<span class="wk-chip ' + cls + '">' + esc(word) + '</span>';
    }
    function jobsCard(w) {
      var list = st.detail.jobs;
      var h2 = '<div class="wk-card"><div class="hd"><span class="t">Jobs</span><span class="wk-hint">' + (w.canRunJobs ? 'Commands chats ran here (worker_run)' : (w.online ? 'This worker is too old to run commands. Update it on the PC.' : '')) + '</span></div><div class="bd">';
      if (!list) h2 += '<div class="wk-hint">Loading…</div>';
      else if (!list.length) h2 += '<div class="wk-hint">None yet. A chat runs one with worker_run, e.g. a release build.</div>';
      else list.slice(0, 15).forEach(function (j) {
        h2 += '<div class="wk-job" data-a="job" data-job="' + esc(j.job) + '"><div style="flex:1;min-width:0"><div class="cmd">' + esc(j.command) + '</div>'
          + '<div class="wk-meta">' + jobChip(j) + ' ' + esc(j.project || '') + (j.branch ? ' · ' + esc(j.branch) + (j.commit ? ' ' + esc(j.commit.slice(0, 7)) : '') : ' · ' + esc(j.source)) + (j.durationMs != null ? ' · ' + esc(dur(j.durationMs)) : '') + ' · ' + esc(ago(j.createdAt)) + (j.artifacts ? ' · ' + j.artifacts.count + ' file' + (j.artifacts.count === 1 ? '' : 's') + ' back (' + esc(gb(j.artifacts.bytes / 1048576)) + ')' : '') + '</div>';
        if (st.detail.openJob === j.job) {
          var v = st.detail.jobView;
          h2 += (j.running ? '<div class="wk-btns" style="margin-top:6px"><button class="wk-btn danger" data-a="job-cancel" data-job="' + esc(j.job) + '">' + icon('stop') + 'Cancel</button></div>' : '')
            + (v && v.error ? '<div class="wk-hint" style="color:var(--red)">' + esc(v.error) + '</div>' : '')
            + (v && v.notes && v.notes.length ? '<div class="wk-hint">' + v.notes.map(esc).join('<br>') + '</div>' : '')
            + '<div class="wk-joblog">' + (v ? esc(v.tail || '(no output yet)') : 'Loading…') + '</div>';
        }
        h2 += '</div></div>';
      });
      return h2 + '</div></div>';
    }
    function toggle(key, on, label, hint) {
      return '<label class="wk-toggle"><input type="checkbox" data-a="set" data-k="' + key + '"' + (on ? ' checked' : '') + '><span>' + esc(label) + '<div class="wk-hint">' + esc(hint) + '</div></span></label>';
    }

    // ── Terminal ──
    function openTerm(id) {
      closeTerm();
      if (!opts.Terminal) { opts.toast('The terminal component did not load on this page.', 'error'); return; }
      var host = document.createElement('div');
      host.className = 'wk-term-wrap';
      host.innerHTML = '<div class="wk-termbar"><span data-r="tstat">Connecting…</span></div><div class="wk-term"></div>';
      var term = new opts.Terminal({ cursorBlink: true, fontSize: 13, fontFamily: opts.termFont, theme: opts.termTheme, scrollback: 5000, allowProposedApi: true });
      var fit = opts.FitAddon ? new opts.FitAddon.FitAddon() : null;
      if (fit) term.loadAddon(fit);
      var ref = 'w' + (++st.refSeq);
      st.term = { ref: ref, id: id, term: term, fit: fit, host: host, ro: null };
      drawDetail();
      term.open(host.querySelector('.wk-term'));
      try { fit && fit.fit(); } catch (e) { /* not laid out */ }
      term.onData(function (data) { opts.send({ type: 'worker-term-input', ref: ref, data: data }); });
      term.onResize(function (sz) { opts.send({ type: 'worker-term-resize', ref: ref, cols: sz.cols, rows: sz.rows }); });
      if (!opts.send({ type: 'worker-term-open', ref: ref, worker: id, cols: term.cols, rows: term.rows })) { termStatus('No connection to the server right now.'); return; }
      termStatus('A shell on the PC');
      if (window.ResizeObserver) { st.term.ro = new ResizeObserver(function () { try { fit && fit.fit(); } catch (e) { /* hidden */ } }); st.term.ro.observe(host); }
      setTimeout(function () { try { fit && fit.fit(); } catch (e) { /* hidden */ } term.focus(); }, 40);
    }
    function termStatus(text) { var t = st.term && st.term.host.querySelector('[data-r="tstat"]'); if (t) t.textContent = text; }
    function closeTerm() {
      if (!st.term) return;
      opts.send({ type: 'worker-term-close', ref: st.term.ref });
      try { st.term.ro && st.term.ro.disconnect(); } catch (e) { /* gone */ }
      try { st.term.term.dispose(); } catch (e) { /* gone */ }
      st.term = null;
    }
    function onMessage(msg) {
      if (msg.type === 'workers') { if (st.visible) soon(); return; }
      if (!st.term || msg.ref !== st.term.ref) return;
      if (msg.type === 'worker-term-data') st.term.term.write(msg.data);
      else if (msg.type === 'worker-term-exit') termStatus(msg.error || ('The shell ended' + (msg.code != null ? ' (exit ' + msg.code + ')' : '') + '.'));
      else if (msg.type === 'worker-term-error') termStatus(msg.error || 'Could not open a terminal.');
    }

    // ── Actions ──
    function arm(btn, key) {
      if (st.armed === key) { st.armed = ''; return true; }
      st.armed = key; btn.classList.add('armed'); var old = btn.innerHTML; btn.innerHTML = 'Sure? Click again';
      setTimeout(function () { if (st.armed === key) { st.armed = ''; btn.classList.remove('armed'); btn.innerHTML = old; } }, 3000);
      return false;
    }
    async function run(fn, okText) {
      try { await fn(); if (okText) opts.toast(okText, 'success'); await load(); } catch (e) { opts.toast(e.message, 'error'); }
    }
    el.addEventListener('click', function (ev) {
      var t = ev.target.closest('[data-a]');
      if (!t || !el.contains(t)) return;
      var a = t.getAttribute('data-a');
      var id = st.detail && st.detail.id;
      if (a === 'set') return;   // change handler
      switch (a) {
        case 'add':
          run(async function () { st.adding = await api('POST', { action: 'code' }); });
          break;
        case 'add-close': st.adding = null; draw(); break;
        case 'copy':
          try { navigator.clipboard.writeText(t.getAttribute('data-text')); opts.toast('Copied', 'success'); } catch (e) { opts.toast('Could not copy', 'error'); }
          break;
        case 'open': st.detail = { id: t.getAttribute('data-id') }; st.adding = null; draw(); load(); break;
        case 'job': st.detail.openJob = st.detail.openJob === t.getAttribute('data-job') ? null : t.getAttribute('data-job'); st.detail.jobView = null; load(); break;
        case 'job-cancel': run(function () { return api('POST', { action: 'job-cancel', job: t.getAttribute('data-job') }); }, 'Cancelling'); break;
        case 'back': closeTerm(); st.detail = null; draw(); break;
        case 'term': openTerm(id); break;
        case 'term-close': closeTerm(); drawDetail(); break;
        case 'rename': run(function () { return api('POST', { action: 'rename', id: id, name: body.querySelector('[data-r="name"]').value }); }, 'Renamed'); break;
        case 'labels': run(function () { return api('POST', { action: 'set', id: id, labels: body.querySelector('[data-r="labels"]').value }); }, 'Labels saved'); break;
        case 'away': run(function () { return api('POST', { action: 'set', id: id, onlyWhenAwayMin: Number(body.querySelector('[data-r="away"]').value) || 0 }); }, 'Saved'); break;
        case 'history': run(async function () { st.detail.history = (await api('POST', { action: 'history', id: id })).history; }); break;
        case 'revoke': if (arm(t, 'revoke')) run(function () { closeTerm(); return api('POST', { action: 'revoke', id: id }); }, 'Revoked'); break;
        case 'reissue': run(async function () { st.detail.rejoin = (await api('POST', { action: 'reissue', id: id })).code; }); break;
        case 'remove': if (arm(t, 'remove')) run(async function () { closeTerm(); await api('POST', { action: 'remove', id: id }); st.detail = null; }, 'Removed'); break;
      }
    });
    el.addEventListener('change', function (ev) {
      var t = ev.target;
      if (t.getAttribute('data-a') !== 'set' || !st.detail) return;
      var p = { action: 'set', id: st.detail.id }; p[t.getAttribute('data-k')] = t.checked;
      run(function () { return api('POST', p); });
    });

    var view = {
      show: function () {
        st.visible = true;
        if (!st.data) body.innerHTML = '<div class="wk-quiet">Loading…</div>';
        load();
        clearInterval(st.tick);
        // The code's countdown, and "last seen" staying true.
        st.tick = setInterval(function () { if (st.visible && (st.adding || !st.detail)) draw(); }, 15000);
      },
      hide: function () { st.visible = false; clearInterval(st.tick); },
      refresh: load,
      onMessage: onMessage,
    };
    active = view;
    return view;
  }

  window.CrundiWorkers = {
    mount: mount,
    handleMessage: function (msg) { if (active) active.onMessage(msg); },
  };
})();
