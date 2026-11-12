/**
 * crundi-homepool.js — the Homepool tab: the owner's control over the shared
 * exit (docs/homepool.md, src/homepool-service.js).
 *
 *   Networks   the server, the standalone homepool bridge, his own networks
 *              and the nodes he assigned to each (add a node for a friend)
 *   Access     every key: who, which networks, on/off, usage, how to use it
 *   Requests   what chats asked for: approve, change, refuse
 *   Activity   the change log
 *
 * Plain script outside webapp-html.js (that file is one template literal).
 *
 *   window.CrundiHomepool.mount(el, opts) -> view  { show(), hide(), refresh() }
 *
 * opts: apiFetch(path, init), toast(msg, kind), icon(name) -> svg,
 *       projects() -> [{ alias, name }],
 *       desktop: window.api.homepool in the desktop app (this PC as a node), else null
 */
(function () {
  'use strict';

  var STYLE_ID = 'hp-styles';
  var CSS = [
    '.hp-root{display:flex;flex-direction:column;height:100%;min-height:0;color:var(--text-primary)}',
    '.hp-nav{display:flex;align-items:center;gap:8px;padding:10px 14px;border-bottom:1px solid var(--border);flex:none;flex-wrap:wrap}',
    '.hp-seg{display:flex;gap:2px;padding:2px;background:var(--bg-tertiary);border:1px solid var(--border);border-radius:8px;overflow-x:auto;scrollbar-width:none;max-width:100%}',
    '.hp-seg::-webkit-scrollbar{display:none}',
    '.hp-seg button{display:inline-flex;align-items:center;gap:6px;border:none;background:none;color:var(--text-secondary);font-size:.78rem;padding:6px 11px;border-radius:6px;cursor:pointer;white-space:nowrap}',
    '.hp-seg button.on{background:var(--bg-hover);color:var(--text-primary);box-shadow:inset 0 0 0 1px var(--border)}',
    '.hp-seg .n{font-family:var(--mono);font-size:.66rem;color:var(--text-muted)}',
    '.hp-seg .n.alert{color:#fff;background:var(--accent);padding:0 5px;border-radius:999px}',
    '.hp-spacer{flex:1}',
    '.hp-body{flex:1;min-height:0;overflow:auto;padding:14px}',
    '.hp-sum{display:flex;gap:18px;flex-wrap:wrap;margin-bottom:14px}',
    '.hp-stat{display:flex;flex-direction:column;gap:1px}',
    '.hp-stat .v{font-family:var(--mono);font-size:1.05rem;font-variant-numeric:tabular-nums}',
    '.hp-stat .l{font-size:.64rem;text-transform:uppercase;letter-spacing:.07em;color:var(--text-muted)}',
    '.hp-card{border:1px solid var(--border);border-radius:var(--radius-sm,8px);background:var(--bg-secondary);margin-bottom:12px;overflow:hidden}',
    '.hp-card>.hd{display:flex;align-items:center;gap:8px;padding:10px 12px;border-bottom:1px solid var(--border-subtle,var(--border));flex-wrap:wrap}',
    '.hp-card>.hd .t{font-weight:600;font-size:.88rem}',
    '.hp-card>.hd .s{color:var(--text-muted);font-size:.74rem}',
    '.hp-card>.bd{padding:8px 12px}',
    '.hp-row{display:flex;align-items:flex-start;gap:10px;padding:8px 0;border-bottom:1px solid var(--border-subtle,var(--border))}',
    '.hp-row:last-child{border-bottom:none}',
    '.hp-row .main{flex:1;min-width:0}',
    '.hp-row .nm{font-size:.84rem;font-weight:600;display:flex;align-items:center;gap:6px;flex-wrap:wrap}',
    '.hp-row .meta{font-size:.74rem;color:var(--text-muted);line-height:1.5;margin-top:2px;word-break:break-word}',
    '.hp-dot{width:8px;height:8px;border-radius:50%;flex:none;margin-top:6px;background:var(--text-muted)}',
    '.hp-dot.up{background:var(--green);box-shadow:0 0 0 3px var(--green-dim)}',
    '.hp-dot.paused{background:var(--yellow);box-shadow:0 0 0 3px var(--yellow-dim)}',
    '.hp-dot.bad{background:var(--red)}',
    '.hp-chip{display:inline-flex;align-items:center;gap:4px;font-size:.64rem;font-family:var(--mono);padding:1px 7px;border-radius:999px;background:var(--bg-tertiary);color:var(--text-secondary);white-space:nowrap}',
    '.hp-chip.ok{background:var(--green-dim);color:var(--green)}',
    '.hp-chip.warn{background:var(--yellow-dim);color:var(--yellow)}',
    '.hp-chip.bad{background:var(--red-dim);color:var(--red)}',
    '.hp-btn{display:inline-flex;align-items:center;gap:5px;cursor:pointer;padding:5px 10px;border-radius:var(--radius-sm,6px);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-secondary);font-size:.76rem;white-space:nowrap;line-height:1.2}',
    '.hp-btn:hover{border-color:var(--accent);color:var(--text-primary)}',
    '.hp-btn.primary{background:var(--accent);border-color:var(--accent);color:#fff}',
    '.hp-btn.danger:hover{border-color:var(--red);color:var(--red);background:var(--red-dim)}',
    '.hp-btn .ic{width:13px;height:13px}',
    '.hp-btns{display:flex;gap:6px;flex-wrap:wrap;align-items:center;margin-top:6px}',
    '.hp-quiet{color:var(--text-muted);font-size:.8rem;padding:6px 2px;line-height:1.5}',
    '.hp-code{font-family:var(--mono);font-size:.72rem;background:var(--bg-tertiary);border:1px solid var(--border);border-radius:6px;padding:7px 9px;word-break:break-all;white-space:pre-wrap;margin:4px 0;position:relative}',
    '.hp-sw{position:relative;width:34px;height:20px;flex:none;border-radius:999px;background:var(--bg-hover);border:1px solid var(--border);cursor:pointer}',
    '.hp-sw::after{content:"";position:absolute;top:2px;left:2px;width:14px;height:14px;border-radius:50%;background:var(--text-muted);transition:left .15s}',
    '.hp-sw.on{background:var(--green-dim);border-color:var(--green)}',
    '.hp-sw.on::after{left:16px;background:var(--green)}',
    '.hp-sel,.hp-in{padding:5px 8px;border-radius:6px;border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:.78rem;max-width:100%}',
    '.hp-modal{position:fixed;inset:0;background:rgba(0,0,0,.45);z-index:3000;display:flex;align-items:center;justify-content:center;padding:14px}',
    '.hp-dlg{background:var(--bg-secondary);border:1px solid var(--border);border-radius:12px;width:min(560px,100%);max-height:calc(100vh - 28px);overflow:auto;padding:16px}',
    '.hp-dlg h3{margin:0 0 10px;font-size:1rem}',
    '.hp-f{margin-bottom:11px}',
    '.hp-f>label{display:block;font-size:.72rem;color:var(--text-muted);margin-bottom:4px;text-transform:uppercase;letter-spacing:.05em}',
    '.hp-f .hint{font-size:.72rem;color:var(--text-muted);margin-top:3px;line-height:1.45}',
    '.hp-f .hp-in,.hp-f .hp-sel{width:100%;box-sizing:border-box}',
    '.hp-checks{display:flex;flex-wrap:wrap;gap:6px}',
    '.hp-checks label{display:inline-flex;align-items:center;gap:5px;font-size:.78rem;padding:4px 8px;border:1px solid var(--border);border-radius:6px;cursor:pointer;background:var(--bg-tertiary)}',
    '.hp-qr{background:#fff;padding:8px;border-radius:8px;width:180px;height:180px;margin:6px 0}',
    '.hp-qr svg{width:100%;height:100%}',
    '.hp-warn{border:1px solid var(--yellow);background:var(--yellow-dim);color:var(--text-primary);border-radius:8px;padding:8px 10px;font-size:.78rem;margin:6px 0;line-height:1.45}',
    '.hp-log{font-family:var(--mono);font-size:.72rem;line-height:1.7}',
    '.hp-log .at{color:var(--text-muted);margin-right:8px}',
    '.hp-dim{opacity:.6}',
  ].join('\n');

  function esc(s) {
    return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }
  function bytes(b) {
    b = Number(b) || 0;
    if (b >= 1073741824) return (b / 1073741824).toFixed(2) + ' GB';
    if (b >= 1048576) return (b / 1048576).toFixed(1) + ' MB';
    if (b >= 1024) return Math.round(b / 1024) + ' KB';
    return b + ' B';
  }
  function dur(ms) {
    var s = Math.max(0, Math.floor(ms / 1000));
    if (s >= 86400) return Math.floor(s / 86400) + 'd ' + Math.floor((s % 86400) / 3600) + 'h';
    if (s >= 3600) return Math.floor(s / 3600) + 'h ' + Math.floor((s % 3600) / 60) + 'm';
    if (s >= 60) return Math.floor(s / 60) + 'm';
    return s + 's';
  }
  function ago(t) { return t ? dur(Date.now() - t) + ' ago' : 'never'; }
  function day(t) { try { return new Date(t).toLocaleString(); } catch (e) { return ''; } }

  var KIND = { chat: 'Chat', project: 'Project', software: 'Software' };

  function mount(el, opts) {
    if (!document.getElementById(STYLE_ID)) {
      var st = document.createElement('style'); st.id = STYLE_ID; st.textContent = CSS; document.head.appendChild(st);
    }
    var icon = opts.icon || function () { return ''; };
    var S = { tab: 'networks', data: null, timer: null, shown: false, error: '', pc: null };
    var desktop = opts.desktop || null;
    el.innerHTML = '<div class="hp-root"><div class="hp-nav"></div><div class="hp-body"></div></div>';
    var nav = el.querySelector('.hp-nav');
    var body = el.querySelector('.hp-body');

    async function load() {
      try {
        var r = await opts.apiFetch('/api/homepool');
        if (r.status === 404) { S.error = 'This server does not support homepool yet. Update the server to use it.'; render(); return; }
        var d = await r.json();
        if (!d.ok) { S.error = d.error || 'Could not load homepool.'; render(); return; }
        S.error = ''; S.data = d;
        if (desktop) { try { S.pc = await desktop.status(); } catch (e) { S.pc = null; } }
        render();
      } catch (e) { S.error = 'Could not reach the server.'; render(); }
    }

    async function act(payload, okMsg) {
      try {
        var r = await opts.apiFetch('/api/homepool', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
        var d = await r.json().catch(function () { return { ok: false, error: 'Bad reply from the server' }; });
        if (!d.ok) { opts.toast(d.error || 'That did not work', 'error'); return null; }
        if (d.overview) { S.data = Object.assign({ ok: true }, d.overview); render(); }
        if (okMsg) opts.toast(okMsg, 'success');
        return d;
      } catch (e) { opts.toast('Could not reach the server', 'error'); return null; }
    }

    function netName(id) {
      if (!id) return 'No network';
      var n = (S.data.networks || []).find(function (x) { return x.id === id; });
      return n ? n.name : id;
    }

    // ── render ──
    function render() {
      if (S.error && !S.data) { nav.innerHTML = ''; body.innerHTML = '<div class="hp-quiet">' + esc(S.error) + '</div>'; return; }
      if (!S.data) { body.innerHTML = '<div class="hp-quiet">Loading…</div>'; return; }
      var d = S.data;
      var pending = (d.requests || []).filter(function (r) { return r.status === 'pending'; }).length;
      var tabs = [['networks', 'Networks', (d.nodes || []).length], ['access', 'Access', (d.keys || []).length], ['requests', 'Requests', pending], ['activity', 'Activity', '']];
      nav.innerHTML = '<div class="hp-seg">' + tabs.map(function (t) {
        var n = t[2] === '' ? '' : '<span class="n' + (t[0] === 'requests' && pending ? ' alert' : '') + '">' + t[2] + '</span>';
        return '<button data-hp-tab="' + t[0] + '" class="' + (S.tab === t[0] ? 'on' : '') + '">' + t[1] + n + '</button>';
      }).join('') + '</div><div class="hp-spacer"></div>'
        + (S.tab === 'networks' ? '<button class="hp-btn" data-hp="net-add">' + icon('plus') + 'Network</button><button class="hp-btn primary" data-hp="node-add">' + icon('plus') + 'Node</button>' : '')
        + (S.tab === 'access' ? '<button class="hp-btn primary" data-hp="key-add">' + icon('plus') + 'Access</button>' : '');

      var upNodes = (d.nodes || []).filter(function (n) { return n.up && !n.paused; }).length;
      var dataToday = (d.keys || []).reduce(function (a, k) { return a + ((k.usage && k.usage.bytes) || 0); }, 0);
      var sum = '<div class="hp-sum">'
        + stat(upNodes + '/' + (d.nodes || []).length, 'nodes up')
        + stat((d.keys || []).filter(function (k) { return !k.refusal; }).length, 'keys in use')
        + stat(bytes(dataToday), 'data today')
        + stat(pending, 'waiting for you')
        + '</div>';
      var html = sum;
      if (S.tab === 'networks') html += renderNetworks(d);
      else if (S.tab === 'access') html += renderAccess(d);
      else if (S.tab === 'requests') html += renderRequests(d);
      else html += renderActivity(d);
      var keep = body.scrollTop;
      body.innerHTML = html;
      body.scrollTop = keep;
    }
    function stat(v, l) { return '<div class="hp-stat"><span class="v">' + esc(v) + '</span><span class="l">' + esc(l) + '</span></div>'; }

    function renderNetworks(d) {
      var out = renderPc(d) + '<div class="hp-quiet" style="margin:-6px 0 10px">Traffic leaves only through a network a key was given. Put each node in the network it really is on: two phones on one home Wi-Fi share one address.</div>';
      var nodes = d.nodes || [];
      (d.networks || []).forEach(function (n) {
        var members = nodes.filter(function (x) { return x.network === n.id; });
        var hd = '<span class="t">' + esc(n.name) + '</span><span class="hp-chip">' + esc(n.id) + '</span>';
        var bd = '';
        if (n.id === 'server') {
          hd += '<span class="hp-chip ok">always up</span>';
          bd = '<div class="hp-quiet">This server’s own connection. The default for every key.</div>';
        } else if (n.id === 'pool') {
          var p = n.pool || {};
          hd += n.enabled === false ? '<span class="hp-chip">bridge off</span>' : (p.up ? '<span class="hp-chip ok">' + (p.liveCount || 0) + ' node(s) up</span>' : '<span class="hp-chip warn">down</span>');
          hd += '<span class="hp-spacer"></span><span class="hp-sw' + (n.enabled !== false ? ' on' : '') + '" data-hp="pool-toggle" title="Use the standalone homepool"></span>';
          bd = '<div class="hp-quiet">The standalone homepool (homepool-relay) as it runs today. A key on this network keeps the same slots, so the same identities keep the same home lines.'
            + (p.nodes && p.nodes.length ? ' Up: ' + p.nodes.map(function (x) { return esc(x.name || x.id); }).join(', ') + '.' : '')
            + (p.error ? ' <span style="color:var(--red)">' + esc(p.error) + '</span>' : '') + '</div>';
        } else {
          hd += '<span class="hp-chip' + (n.live ? ' ok' : '') + '">' + (n.live || 0) + ' up</span><span class="hp-spacer"></span>'
            + '<button class="hp-btn" data-hp="net-rename" data-id="' + esc(n.id) + '">Rename</button>'
            + '<button class="hp-btn danger" data-hp="net-remove" data-id="' + esc(n.id) + '">Remove</button>';
          bd = members.length ? members.map(nodeRow).join('') : '<div class="hp-quiet">No node in this network yet. Add one, or move one here.</div>';
        }
        out += '<div class="hp-card"><div class="hd">' + hd + '</div><div class="bd">' + bd + '</div></div>';
      });
      var loose = nodes.filter(function (x) { return !x.network || !(d.networks || []).some(function (n) { return n.id === x.network; }); });
      if (loose.length) out += '<div class="hp-card"><div class="hd"><span class="t">Not in a network</span><span class="s">carry nothing until you put them in one</span></div><div class="bd">' + loose.map(nodeRow).join('') + '</div></div>';
      return out;
    }

    // This PC (desktop app only): lend its connection as a node.
    function renderPc(d) {
      if (!desktop || !S.pc) return '';
      var p = S.pc;
      var local = opts.localServer != null ? !!opts.localServer : /^(localhost|127\.0\.0\.1|\[::1\])$/.test(location.hostname);
      var hd = '<span class="t">This PC</span><span class="s">' + esc(p.name || '') + '</span>';
      if (local) return '<div class="hp-card"><div class="hd">' + hd + '</div><div class="bd"><div class="hp-quiet">This PC runs the server, so its connection is already the Server network.</div></div></div>';
      if (!p.configured) {
        var nets = homeNets();
        return '<div class="hp-card"><div class="hd">' + hd + '<span class="hp-chip">not shared</span></div><div class="bd">'
          + '<div class="hp-quiet">Lend this PC\u2019s internet connection to the server as a node. Only keys you give the network you put it in can use it, and it never opens anything on this PC\u2019s own network.</div>'
          + '<div class="hp-btns"><select class="hp-sel" id="hp-pc-net"><option value="">No network yet</option>' + nets.map(function (n) { return '<option value="' + esc(n.id) + '"' + (n.id === 'home' ? ' selected' : '') + '>' + esc(n.name) + '</option>'; }).join('') + '</select>'
          + '<button class="hp-btn primary" data-hp="pc-share">Share this PC\u2019s connection</button></div></div></div>';
      }
      var st = p.state;
      var chip = st === 'connected' ? '<span class="hp-chip ok">connected</span>' : st === 'connecting' ? '<span class="hp-chip warn">connecting</span>' : st === 'refused' ? '<span class="hp-chip bad">the server refused this node key</span>' : '<span class="hp-chip">stopped</span>';
      var me = (d.nodes || []).find(function (n) { return p.node && n.id === p.node.id; });
      var sv = p.serverView || {};
      var meta = [];
      if (p.node) meta.push('node ' + esc(p.node.name) + ' in ' + esc(p.node.networkName || netName(p.node.network)));
      if (p.publicIp) meta.push('IP ' + esc(p.publicIp));
      if (p.connectedAt) meta.push('up ' + dur(Date.now() - p.connectedAt));
      meta.push(p.streams + ' open now');
      meta.push(bytes((p.bytesUp || 0) + (p.bytesDown || 0)) + ' since start');
      if (sv.slots != null) meta.push(sv.slots + ' slot' + (sv.slots === 1 ? '' : 's') + ' carried');
      if (sv.bytesToday != null) meta.push(bytes(sv.bytesToday) + ' / ' + (sv.connsToday || 0) + ' connections today');
      if (sv.keys && sv.keys.length) meta.push('used by ' + sv.keys.map(esc).join(', '));
      var open = S.pcLog;
      return '<div class="hp-card"><div class="hd">' + hd + chip + '<span class="hp-spacer"></span>'
        + (p.enabled && st !== 'refused' ? '<button class="hp-btn" data-hp="pc-stop">Stop</button>' : '<button class="hp-btn primary" data-hp="pc-start">Start</button>')
        + '<button class="hp-btn danger" data-hp="pc-forget" data-id="' + esc((me && me.id) || (p.node && p.node.id) || '') + '">Stop sharing</button></div>'
        + '<div class="bd"><div class="meta" style="font-size:.74rem;color:var(--text-muted);line-height:1.55">' + meta.join(' \u00b7 ') + '</div>'
        + '<div class="hp-btns"><button class="hp-btn" data-hp="pc-log">' + (open ? 'Hide activity' : 'Activity') + '</button></div>'
        + (open ? '<div class="hp-code" style="max-height:200px;overflow:auto">' + esc((p.log || []).join('\n') || 'Nothing yet.') + '</div>' : '')
        + '</div></div>';
    }

    async function pcShare() {
      var sel = el.querySelector('#hp-pc-net');
      var r = await act({ action: 'self-node', deviceId: S.pc.deviceId, name: S.pc.name || 'PC', network: sel ? sel.value : '' });
      if (!r || !r.node) return;
      var d = await desktop.share({ server: location.origin, key: r.node.key });
      if (!d || !d.ok) { opts.toast((d && d.error) || 'The desktop app could not start the node', 'error'); return; }
      S.pc = d.status; render();
      opts.toast('This PC is now a node', 'success');
    }

    function nodeRow(n) {
      var i = n.info || {};
      var dot = n.enabled === false ? '' : (n.up ? (n.paused ? 'paused' : 'up') : '');
      var state = n.enabled === false ? '<span class="hp-chip">switched off</span>' : n.up ? (n.paused ? '<span class="hp-chip warn">paused' + (i.pauseReason ? ': ' + esc(i.pauseReason) : '') + '</span>' : '<span class="hp-chip ok">connected</span>') : '<span class="hp-chip">offline</span>';
      var meta = [];
      meta.push((n.kind === 'crundi-app' ? 'Crundi app' : 'Standalone node') + (i.platform || n.platform ? ' · ' + esc(i.platform || n.platform) : ''));
      if (n.up) {
        if (i.network) meta.push(esc(i.network));
        if (i.publicIp) meta.push('IP ' + esc(i.publicIp));
        meta.push('up ' + dur(Date.now() - n.connectedAt));
        meta.push(n.streams + ' open');
      } else meta.push('last seen ' + ago(n.lastSeenAt));
      meta.push(n.slots + ' slot' + (n.slots === 1 ? '' : 's'));
      meta.push(bytes(n.usage && n.usage.bytes) + ' today');
      if (i.mobileCapMb) meta.push('mobile ' + bytes(i.mobileBytesToday) + ' of ' + i.mobileCapMb + ' MB');
      var opts = '<option value="">No network</option>' + (S.data.networks || []).filter(function (x) { return x.id !== 'server' && x.id !== 'pool'; }).map(function (x) {
        return '<option value="' + esc(x.id) + '"' + (x.id === n.network ? ' selected' : '') + '>' + esc(x.name) + '</option>';
      }).join('');
      return '<div class="hp-row"><span class="hp-dot ' + dot + '"></span><div class="main">'
        + '<div class="nm">' + esc(n.name) + ' ' + state + '</div>'
        + '<div class="meta">' + meta.join(' · ') + '</div>'
        + '<div class="hp-btns"><select class="hp-sel" data-hp="node-move" data-id="' + esc(n.id) + '" title="Network">' + opts + '</select>'
        + '<span class="hp-sw' + (n.enabled !== false ? ' on' : '') + '" data-hp="node-toggle" data-id="' + esc(n.id) + '" title="On / off"></span>'
        + '<button class="hp-btn" data-hp="node-rename" data-id="' + esc(n.id) + '">Rename</button>'
        + '<button class="hp-btn" data-hp="node-reissue" data-id="' + esc(n.id) + '">New key</button>'
        + '<button class="hp-btn danger" data-hp="node-remove" data-id="' + esc(n.id) + '">Remove</button></div>'
        + '</div></div>';
    }

    function whoOf(k) {
      if (k.kind === 'chat') return 'chat ' + esc(String(k.chatId).slice(0, 8)) + (k.project ? ' in ' + esc(k.project) : '');
      if (k.kind === 'project') return 'every chat of ' + esc(k.project);
      return 'software';
    }

    function renderAccess(d) {
      var keys = d.keys || [];
      if (!keys.length) return '<div class="hp-quiet">No access given yet, so nothing can use homepool. Add access for a chat, a project or a piece of software, or approve a request from a chat.</div>';
      return keys.map(function (k) {
        var u = k.usage || {};
        var st = k.status || {};
        var state = k.refusal ? '<span class="hp-chip bad">' + esc(k.refusal) + '</span>' : (st.egressAvailable ? '<span class="hp-chip ok">' + (st.fallbackInUse ? 'leaving from the server (fallback)' : 'exit up') + '</span>' : '<span class="hp-chip warn">no exit up</span>');
        var nets = (k.networks || []).map(function (n) { return '<span class="hp-chip">' + esc(netName(n)) + '</span>'; }).join(' ');
        var meta = [k.kind === 'software' ? (k.portBlock ? 'Software, port block' : 'Software, own login') : KIND[k.kind] + ': ' + whoOf(k), bytes(u.bytes) + ' today', (u.conns || 0) + ' connections', (u.refused || 0) + ' refused', 'used ' + ago(u.lastUsedAt)];
        if (k.open) meta.push(k.open + ' open now');
        if (k.dailyCapMb) meta.push('cap ' + k.dailyCapMb + ' MB/day');
        if (k.expiresAt) meta.push('expires ' + day(k.expiresAt));
        if (k.fallbackToServer) meta.push('falls back to the server');
        if (k.portBlock && k.ports) meta.push('ports ' + k.ports.base + '-' + k.ports.status + ' from ' + (k.allowFrom || []).join(', '));
        var open = S.openKey === k.id;
        var use = '';
        if (open && k.use) {
          use = '<div style="margin-top:8px">'
            + (k.kind === 'software' ? '<div class="hp-quiet">Login: ' + esc(k.use.login) + '</div>'
              + code(k.use.socks) + code(k.use.http) + '<div class="hp-quiet">Status for the software to check before it starts: ' + esc(k.use.status) + '</div>'
              + (k.use.fromDocker ? '<div class="hp-quiet">From a container: ' + esc(k.use.fromDocker) + '</div>' : '') : '<div class="hp-quiet">Chats use it through the MCP tools homepool_proxy and homepool_fetch.</div>')
            + (k.use.block ? '<div class="hp-quiet">Port block (no login; only from ' + esc((k.use.block.from || []).join(', ')) + '): any slot ' + esc(k.use.block.any) + ', slots ' + esc(k.use.block.slots) + '. For docker compose:</div>' + code(k.use.block.compose.join('\n')) : '')
            + '</div>';
        }
        return '<div class="hp-card"><div class="hd"><span class="t">' + esc(k.name) + '</span>' + nets + state
          + '<span class="hp-spacer"></span><span class="hp-sw' + (k.enabled !== false ? ' on' : '') + '" data-hp="key-toggle" data-id="' + esc(k.id) + '" title="On / off: off cuts its connections at once"></span></div>'
          + '<div class="bd"><div class="meta" style="font-size:.74rem;color:var(--text-muted);line-height:1.5">' + meta.join(' · ') + '</div>'
          + '<div class="hp-btns"><button class="hp-btn" data-hp="key-use" data-id="' + esc(k.id) + '">' + (open ? 'Hide' : 'How to use') + '</button>'
          + '<button class="hp-btn" data-hp="key-edit" data-id="' + esc(k.id) + '">' + icon('pencil') + 'Change</button>'
          + (k.kind === 'software' ? '<button class="hp-btn" data-hp="key-reissue" data-id="' + esc(k.id) + '">New secret</button>' : '')
          + '<button class="hp-btn danger" data-hp="key-remove" data-id="' + esc(k.id) + '">' + icon('trash') + 'Revoke</button></div>'
          + use + '</div></div>';
      }).join('');
    }

    function code(s) { return '<div class="hp-code">' + esc(s) + '</div><div class="hp-btns" style="margin-top:0"><button class="hp-btn" data-hp="copy" data-text="' + esc(s) + '">' + icon('copy') + 'Copy</button></div>'; }

    function renderRequests(d) {
      var rs = d.requests || [];
      if (!rs.length) return '<div class="hp-quiet">No requests. A chat asks with the MCP tool homepool_request, and nothing is made until you approve it here.</div>';
      return rs.map(function (r) {
        var pend = r.status === 'pending';
        var nets = (r.networks || []).map(function (n) { return '<span class="hp-chip">' + esc(netName(n)) + '</span>'; }).join(' ');
        return '<div class="hp-card' + (pend ? '' : ' hp-dim') + '"><div class="hd"><span class="t">' + esc(KIND[r.kind] || r.kind) + (r.name ? ' "' + esc(r.name) + '"' : '') + '</span>' + nets
          + '<span class="hp-chip' + (pend ? ' warn' : r.status === 'approved' ? ' ok' : ' bad') + '">' + esc(r.status) + '</span></div>'
          + '<div class="bd"><div class="meta" style="font-size:.76rem;line-height:1.55">From chat ' + esc(String(r.chatId).slice(0, 8)) + (r.project ? ' in ' + esc(r.project) : '') + ', ' + ago(r.createdAt) + '</div>'
          + (r.reason ? '<div style="font-size:.82rem;margin:6px 0;line-height:1.5">' + esc(r.reason) + '</div>' : '')
          + (pend ? '<div class="hp-btns"><button class="hp-btn primary" data-hp="req-approve" data-id="' + esc(r.id) + '">' + icon('check') + 'Approve</button>'
            + '<button class="hp-btn" data-hp="req-change" data-id="' + esc(r.id) + '">Change and approve</button>'
            + '<button class="hp-btn danger" data-hp="req-refuse" data-id="' + esc(r.id) + '">Refuse</button></div>' : '')
          + '</div></div>';
      }).join('');
    }

    var LOG = {
      'network-added': 'added network', 'network-renamed': 'renamed network', 'network-removed': 'removed network',
      'node-added': 'added node', 'node-moved': 'moved node', 'node-enabled': 'switched node on', 'node-disabled': 'switched node off',
      'node-key-reissued': 'gave node a new key', 'node-removed': 'removed node', 'node-up': 'node connected', 'node-down': 'node disconnected',
      'key-added': 'gave access', 'key-changed': 'changed access', 'key-reissued': 'new secret for', 'key-revoked': 'revoked access',
      'request': 'a chat asked for access', 'request-approved': 'approved request', 'request-refused': 'refused request',
      'refused-login': 'refused a login', 'settings-changed': 'changed settings',
    };
    function renderActivity(d) {
      var names = {};
      (d.nodes || []).forEach(function (n) { names[n.id] = n.name; });
      (d.keys || []).forEach(function (k) { names[k.id] = k.name; });
      var lines = (d.log || []).map(function (e) {
        var what = LOG[e.what] || e.what;
        var subj = e.node ? (names[e.node] || e.node) : e.key ? (names[e.key] || e.key) : e.network ? netName(e.network) : e.request ? e.request : e.user ? e.user + (e.from ? ' from ' + e.from : '') : '';
        if (e.what === 'node-moved') subj += ' → ' + netName(e.to);
        return '<div><span class="at">' + esc(day(e.at)) + '</span>' + esc(what) + (subj ? ' <b>' + esc(subj) + '</b>' : '') + '</div>';
      });
      return '<div class="hp-card"><div class="bd hp-log">' + (lines.join('') || '<div class="hp-quiet">Nothing yet.</div>') + '</div></div>';
    }

    // ── dialogs ──
    function dialog(title, inner, onOk, okLabel) {
      var m = document.createElement('div');
      m.className = 'hp-modal';
      m.innerHTML = '<div class="hp-dlg"><h3>' + esc(title) + '</h3><div class="hp-dlg-body">' + inner + '</div>'
        + '<div class="hp-btns" style="justify-content:flex-end;margin-top:12px">' + (onOk ? '<button class="hp-btn" data-x="cancel">Cancel</button><button class="hp-btn primary" data-x="ok">' + esc(okLabel || 'Save') + '</button>' : '<button class="hp-btn primary" data-x="cancel">Done</button>') + '</div></div>';
      document.body.appendChild(m);
      var close = function () { m.remove(); };
      m.addEventListener('click', async function (e) {
        var t = e.target.closest('[data-x],[data-hp="copy"]');
        if (e.target === m) return close();
        if (!t) return;
        if (t.dataset.hp === 'copy') return copy(t.dataset.text);
        if (t.dataset.x === 'cancel') return close();
        if (t.dataset.x === 'ok' && onOk) { t.disabled = true; var done = await onOk(m); t.disabled = false; if (done !== false) close(); }
      });
      return m;
    }

    function copy(text) {
      try { navigator.clipboard.writeText(text).then(function () { opts.toast('Copied', 'success'); }, function () { opts.toast('Copy failed', 'error'); }); }
      catch (e) { opts.toast('Copy failed', 'error'); }
    }

    function homeNets() { return (S.data.networks || []).filter(function (x) { return x.id !== 'server' && x.id !== 'pool'; }); }

    function showNodeSetup(node, title) {
      var inner = '<div class="hp-warn">The key is shown this once. Whoever has it can lend their connection to this server as this node (and nothing else). Give it only to the person running the node.</div>'
        + (node.qr ? '<div class="hp-qr">' + node.qr + '</div>' : '')
        + '<div class="hp-f"><label>Code (scan or paste in the Homepool Node app or the Crundi app)</label>' + code(node.code || node.key) + '</div>'
        + (node.docker ? '<div class="hp-f"><label>Or on any machine with Docker</label>' + code(node.docker) + '</div>' : '')
        + (!node.server ? '<div class="hp-warn">This server has no public address set, so the code has none. Set one (TLS domain or tunnel) for nodes outside this machine.</div>' : '');
      dialog(title || 'Node ready', inner);
    }

    function nodeAddDialog() {
      var nets = homeNets();
      var inner = '<div class="hp-f"><label>Name</label><input class="hp-in" id="hp-n-name" placeholder="e.g. Ali’s PC" /></div>'
        + '<div class="hp-f"><label>Network</label><select class="hp-sel" id="hp-n-net"><option value="">No network yet</option>' + nets.map(function (n) { return '<option value="' + esc(n.id) + '"' + (n.id === 'home' ? ' selected' : '') + '>' + esc(n.name) + '</option>'; }).join('') + '</select>'
        + '<div class="hint">A friend’s line is a network of its own: make one first if it does not exist (Network button).</div></div>';
      dialog('Add a node', inner, async function (m) {
        var name = m.querySelector('#hp-n-name').value.trim();
        if (!name) { opts.toast('Give the node a name', 'error'); return false; }
        var r = await act({ action: 'node-add', name: name, network: m.querySelector('#hp-n-net').value });
        if (!r) return false;
        setTimeout(function () { showNodeSetup(r.node, 'Node "' + name + '" is ready'); }, 0);
      }, 'Make node');
    }

    function keyDialog(k, fromRequest) {
      var edit = !!(k && k.id && !fromRequest);
      k = k || { kind: 'software', networks: ['server'], slots: 8 };
      var projects = (opts.projects ? opts.projects() : []) || [];
      var allNets = (S.data.networks || []);
      var kinds = ['software', 'project', 'chat'].map(function (x) { return '<option value="' + x + '"' + (k.kind === x ? ' selected' : '') + '>' + KIND[x] + '</option>'; }).join('');
      var inner = '<div class="hp-f"><label>For</label><select class="hp-sel" id="hp-k-kind"' + (edit ? ' disabled' : '') + '>' + kinds + '</select>'
        + '<div class="hint">Software: a scraper, script or container, with a login of its own. Project: every chat of a project. Chat: one chat.</div></div>'
        + '<div class="hp-f" data-for="software"><label>Name</label><input class="hp-in" id="hp-k-name" value="' + esc(k.name || '') + '" placeholder="e.g. insta-scraper" /></div>'
        + '<div class="hp-f" data-for="project chat"><label>Project</label><select class="hp-sel" id="hp-k-project"' + (edit || fromRequest ? ' disabled' : '') + '><option value="">—</option>' + projects.map(function (p) { return '<option value="' + esc(p.alias) + '"' + (p.alias === k.project ? ' selected' : '') + '>' + esc(p.name || p.alias) + '</option>'; }).join('') + '</select></div>'
        + '<div class="hp-f" data-for="chat"><label>Chat id</label><input class="hp-in" id="hp-k-chat" value="' + esc(k.chatId || '') + '"' + (edit || fromRequest ? ' disabled' : '') + ' placeholder="the chat’s id" /><div class="hint">Easier: let the chat ask with homepool_request, then approve it.</div></div>'
        + '<div class="hp-f"><label>Networks it may use</label><div class="hp-checks">' + allNets.map(function (n) {
          return '<label><input type="checkbox" value="' + esc(n.id) + '"' + ((k.networks || []).indexOf(n.id) >= 0 ? ' checked' : '') + ' />' + esc(n.name) + '</label>';
        }).join('') + '</div><div class="hint">With more than one, its slots spread over all of them and each slot stays on one exit, as the standalone homepool does. A login can pin one with user@network.</div></div>'
        + '<div class="hp-f"><label><input type="checkbox" id="hp-k-fb"' + (k.fallbackToServer ? ' checked' : '') + ' /> Fall back to the server when none of them is up</label><div class="hint">Off: with no exit up, connections are refused rather than quietly leaving from the server.</div></div>'
        + '<div class="hp-f"><label>Expires</label><input class="hp-in" type="datetime-local" id="hp-k-exp" value="' + (k.expiresAt ? new Date(k.expiresAt - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 16) : '') + '" /><div class="hint">Empty: until you revoke it.</div></div>'
        + '<div class="hp-f"><label>Data cap per day (MB, 0 = none)</label><input class="hp-in" type="number" min="0" id="hp-k-cap" value="' + (k.dailyCapMb || 0) + '" /></div>'
        + '<div class="hp-f"><label>Slots</label><input class="hp-in" type="number" min="0" max="32" id="hp-k-slots" value="' + (k.slots == null ? 8 : k.slots) + '" /><div class="hint">Identities that should each keep one exit (one per account).</div></div>'
        + '<div class="hp-f" data-for="software"><label><input type="checkbox" id="hp-k-block"' + (k.portBlock ? ' checked' : '') + ' /> Port block, for software that cannot send a proxy login (Chromium with SOCKS)</label>'
        + '<input class="hp-in" id="hp-k-from" style="margin-top:6px" value="' + esc((k.allowFrom || ['homepool']).join(', ')) + '" placeholder="homepool, loopback, 172.21.0.0/16" />'
        + '<div class="hint">Who may use the ports: Docker network names, loopback, or address ranges. Nothing else can.</div></div>';
      var title = edit ? 'Change access "' + (k.name || '') + '"' : fromRequest ? 'Approve with changes' : 'Give access';
      var m = dialog(title, inner, async function (mm) {
        var key = readKey(mm);
        if (!key.networks.length) { opts.toast('Pick at least one network', 'error'); return false; }
        var r;
        if (fromRequest) r = await act({ action: 'request-decide', id: fromRequest, approve: true, changes: key }, 'Approved');
        else if (edit) r = await act({ action: 'key-update', id: k.id, key: key }, 'Saved');
        else {
          r = await act({ action: 'key-add', key: key });
          if (r && r.secret) setTimeout(function () { showSecret(r, key.name || 'Access'); }, 0);
        }
        return r ? true : false;
      }, edit ? 'Save' : fromRequest ? 'Approve' : 'Give access');
      var sync = function () {
        var kind = m.querySelector('#hp-k-kind').value;
        m.querySelectorAll('[data-for]').forEach(function (f) { f.style.display = f.dataset.for.split(' ').indexOf(kind) >= 0 ? '' : 'none'; });
      };
      m.querySelector('#hp-k-kind').addEventListener('change', sync);
      sync();
    }

    function readKey(m) {
      var kind = m.querySelector('#hp-k-kind').value;
      var exp = m.querySelector('#hp-k-exp').value;
      var out = {
        kind: kind,
        name: m.querySelector('#hp-k-name').value.trim(),
        project: m.querySelector('#hp-k-project').value,
        chatId: m.querySelector('#hp-k-chat').value.trim(),
        networks: [].slice.call(m.querySelectorAll('.hp-checks input:checked')).map(function (x) { return x.value; }),
        fallbackToServer: m.querySelector('#hp-k-fb').checked,
        expiresAt: exp ? new Date(exp).getTime() : null,
        dailyCapMb: Number(m.querySelector('#hp-k-cap').value) || 0,
        slots: Number(m.querySelector('#hp-k-slots').value) || 0,
        portBlock: kind === 'software' && m.querySelector('#hp-k-block').checked,
        allowFrom: m.querySelector('#hp-k-from').value.split(',').map(function (s) { return s.trim(); }).filter(Boolean),
      };
      if (!out.name) delete out.name;
      return out;
    }

    function showSecret(r, name) {
      var u = r.use || {};
      var inner = '<div class="hp-warn">The secret is shown this once. Keep it with the software (its .env or a Crundi secret), never in a file you commit.</div>'
        + '<div class="hp-f"><label>Login</label>' + code('user: ' + r.key.id + '\npassword: ' + r.secret) + '</div>'
        + (u.socks ? '<div class="hp-f"><label>SOCKS5</label>' + code(u.socks) + '</div>' : '')
        + (u.http ? '<div class="hp-f"><label>HTTP proxy</label>' + code(u.http) + '</div>' : '')
        + (u.block ? '<div class="hp-f"><label>Port block (no login)</label>' + code(u.block.compose.join('\n')) + '</div>' : '');
      dialog('Access "' + name + '"', inner);
    }

    function confirmDlg(title, text, okLabel, fn) {
      dialog(title, '<div style="font-size:.84rem;line-height:1.55">' + text + '</div>', async function () { return (await fn()) !== false; }, okLabel);
    }

    function promptDlg(title, label, value, fn) {
      dialog(title, '<div class="hp-f"><label>' + esc(label) + '</label><input class="hp-in" id="hp-p" value="' + esc(value || '') + '" /></div>', async function (m) {
        var v = m.querySelector('#hp-p').value.trim();
        if (!v) return false;
        return (await fn(v)) !== false;
      });
    }

    // ── events ──
    el.addEventListener('click', async function (e) {
      var t = e.target.closest('[data-hp-tab],[data-hp]');
      if (!t || t.tagName === 'SELECT') return;
      if (t.dataset.hpTab) { S.tab = t.dataset.hpTab; render(); return; }
      var a = t.dataset.hp, id = t.dataset.id;
      var d = S.data || {};
      var node = (d.nodes || []).find(function (n) { return n.id === id; });
      var key = (d.keys || []).find(function (k) { return k.id === id; });
      var req = (d.requests || []).find(function (r) { return r.id === id; });
      switch (a) {
        case 'copy': return copy(t.dataset.text);
        case 'pc-share': return pcShare();
        case 'pc-start': { var a1 = await desktop.start(); S.pc = a1 && a1.status; return render(); }
        case 'pc-stop': { var a2 = await desktop.stop(); S.pc = a2 && a2.status; return render(); }
        case 'pc-log': S.pcLog = !S.pcLog; return render();
        case 'pc-forget': return confirmDlg('Stop sharing this PC?', 'The node stops, its key is removed from this PC and from the server.', 'Stop sharing', async function () {
          var a3 = await desktop.forget(); S.pc = a3 && a3.status;
          if (id) await act({ action: 'node-remove', id: id }, 'This PC no longer shares its connection');
          else render();
        });
        case 'net-add': return promptDlg('Add a network', 'Name (where it is: Home, Ali’s place, Office)', '', function (v) { return act({ action: 'network-add', name: v }, 'Network added').then(Boolean); });
        case 'net-rename': return promptDlg('Rename network', 'Name', netName(id), function (v) { return act({ action: 'network-rename', id: id, name: v }).then(Boolean); });
        case 'net-remove': return confirmDlg('Remove "' + netName(id) + '"?', 'Its nodes stay, but carry nothing until you put them in another network. Keys lose this network.', 'Remove', function () { return act({ action: 'network-remove', id: id }, 'Removed').then(Boolean); });
        case 'pool-toggle': {
          var on = !t.classList.contains('on');
          return act({ action: 'settings', settings: { poolBridge: { enabled: on } } }, on ? 'Standalone homepool on' : 'Standalone homepool off');
        }
        case 'node-add': return nodeAddDialog();
        case 'node-toggle': return act({ action: 'node-update', id: id, enabled: !t.classList.contains('on') });
        case 'node-rename': return promptDlg('Rename node', 'Name', node && node.name, function (v) { return act({ action: 'node-update', id: id, name: v }).then(Boolean); });
        case 'node-reissue': return confirmDlg('New key for "' + (node && node.name) + '"?', 'The old key stops working now and the node disconnects. Set it up again with the new code.', 'New key', async function () {
          var r = await act({ action: 'node-reissue', id: id });
          if (r) setTimeout(function () { showNodeSetup(r.node, 'New key for "' + node.name + '"'); }, 0);
          return !!r;
        });
        case 'node-remove': return confirmDlg('Remove "' + (node && node.name) + '"?', 'It disconnects now and its key stops working for good.', 'Remove', function () { return act({ action: 'node-remove', id: id }, 'Removed').then(Boolean); });
        case 'key-add': return keyDialog(null);
        case 'key-edit': return keyDialog(key);
        case 'key-use': S.openKey = S.openKey === id ? '' : id; return render();
        case 'key-toggle': return act({ action: 'key-update', id: id, key: { enabled: !t.classList.contains('on') } }, t.classList.contains('on') ? 'Switched off: its connections are cut' : 'Switched on');
        case 'key-reissue': return confirmDlg('New secret for "' + (key && key.name) + '"?', 'The old one stops working now.', 'New secret', async function () {
          var r = await act({ action: 'key-reissue', id: id });
          if (r) setTimeout(function () { showSecret(r, key.name); }, 0);
          return !!r;
        });
        case 'key-remove': return confirmDlg('Revoke "' + (key && key.name) + '"?', 'Its open connections end now and it can never be used again.', 'Revoke', function () { return act({ action: 'key-remove', id: id }, 'Revoked').then(Boolean); });
        case 'req-approve': return act({ action: 'request-decide', id: id, approve: true }, 'Approved');
        case 'req-refuse': return act({ action: 'request-decide', id: id, approve: false }, 'Refused');
        case 'req-change': return req && keyDialog({ kind: req.kind, name: req.name, project: req.project, chatId: req.chatId, networks: req.networks, slots: 8 }, id);
      }
    });
    el.addEventListener('change', function (e) {
      var t = e.target.closest('select[data-hp="node-move"]');
      if (t) act({ action: 'node-update', id: t.dataset.id, network: t.value }, 'Moved');
    });

    return {
      show: function () {
        S.shown = true; load();
        clearInterval(S.timer);
        // Live while open; a dialog being filled in is left alone.
        S.timer = setInterval(function () { if (!document.querySelector('.hp-modal')) load(); }, 3000);
      },
      hide: function () { S.shown = false; clearInterval(S.timer); S.timer = null; },
      refresh: load,
    };
  }

  window.CrundiHomepool = { mount: mount };
})();
