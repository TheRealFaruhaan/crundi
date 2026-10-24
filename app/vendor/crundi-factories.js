/**
 * crundi-factories.js — the Factories tab and the project Factory panel.
 *
 * Lives outside webapp-html.js on purpose (that file is one template literal).
 * The editor and the live graph come from factory-flow.js (React Flow), loaded
 * only when one is opened.
 *
 *   window.CrundiFactories.mountTab(el, ctx)    -> { show(), hide(), openRun(id), refresh() }
 *   window.CrundiFactories.mountPanel(el, ctx)  -> { load(project) }
 *   window.CrundiFactories.runEvent(d)          the host forwards its factoryRun / factories events
 *
 * ctx: { apiFetch, toast, icon, currentProject(), projects(), flowJs, flowCss,
 *        openManagerChat(chatId) -> bool, openTab(runId) }
 */
(function () {
  'use strict';

  var CSS = [
    '.fx{display:flex;flex-direction:column;height:100%;min-height:0;color:var(--text-primary)}',
    '.fx-head{display:flex;align-items:center;gap:8px;padding:10px 14px;border-bottom:1px solid var(--border);flex-wrap:wrap}',
    '.fx-title{font-weight:650;font-size:15px;margin-right:6px}',
    '.fx-seg{display:flex;gap:2px;background:var(--bg-tertiary);border-radius:8px;padding:2px}',
    '.fx-seg button{background:none;border:0;color:var(--text-secondary);padding:5px 12px;border-radius:6px;cursor:pointer;font:inherit;font-size:12.5px}',
    '.fx-seg button.on{background:var(--bg-hover);color:var(--text-primary)}',
    '.fx-sp{flex:1}',
    '.fx-body{flex:1;overflow-y:auto;min-height:0;padding:14px}',
    '.fx-full{flex:1;min-height:0;display:flex;flex-direction:column}',
    '.fx-btn{display:inline-flex;align-items:center;gap:6px;background:var(--bg-card);color:var(--text-primary);border:1px solid var(--border);border-radius:6px;padding:6px 11px;font:inherit;font-size:12.5px;cursor:pointer;white-space:nowrap}',
    '.fx-btn:hover:not(:disabled){background:var(--bg-hover)}',
    '.fx-btn:disabled{opacity:.45;cursor:default}',
    '.fx-btn.primary{background:var(--accent);border-color:var(--accent);color:#fff}',
    '.fx-btn.ghost{background:none}',
    '.fx-btn.danger{color:var(--red)}',
    '.fx-btn .ic{width:14px;height:14px}',
    '.fx-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(280px,1fr));gap:12px}',
    '.fx-card{background:var(--bg-card);border:1px solid var(--border);border-radius:10px;padding:12px 14px;display:flex;flex-direction:column;gap:8px}',
    '.fx-card-head{display:flex;align-items:center;gap:8px}',
    '.fx-card-name{font-weight:650;font-size:14px;flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.fx-card-desc{font-size:12.5px;color:var(--text-secondary);line-height:1.45;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;min-height:1.45em}',
    '.fx-meta{font-size:11.5px;color:var(--text-muted)}',
    '.fx-row-btns{display:flex;flex-wrap:wrap;gap:6px}',
    '.fx-chip{font-size:11px;padding:1px 8px;border-radius:99px;border:1px solid var(--border);color:var(--text-secondary);white-space:nowrap}',
    '.fx-chip.ok{color:var(--green);border-color:rgba(16,185,129,.4)}',
    '.fx-chip.draft,.fx-chip.waiting,.fx-chip.paused{color:var(--yellow);border-color:rgba(245,158,11,.4)}',
    '.fx-chip.running{color:var(--green);border-color:rgba(16,185,129,.4)}',
    '.fx-chip.crashed,.fx-chip.stopped{color:var(--red);border-color:rgba(239,68,68,.4)}',
    '.fx-chip.finishing,.fx-chip.finished{color:var(--sky);border-color:rgba(56,189,248,.4)}',
    '.fx-empty{color:var(--text-muted);font-size:13px;padding:24px 4px;line-height:1.6}',
    '.fx-list{display:flex;flex-direction:column;gap:8px}',
    '.fx-run{background:var(--bg-card);border:1px solid var(--border);border-radius:10px;padding:10px 12px;display:flex;flex-direction:column;gap:6px;cursor:pointer}',
    '.fx-run:hover{border-color:var(--accent)}',
    '.fx-run-top{display:flex;align-items:center;gap:8px}',
    '.fx-run-name{font-weight:600;flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.fx-run-task{font-size:12.5px;color:var(--text-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.fx-note{font-size:12px;color:var(--text-muted);line-height:1.5}',
    '.fx-warn{font-size:12.5px;line-height:1.45;padding:8px 10px;border-radius:7px;background:rgba(245,158,11,.1);color:#fcd34d}',
    '.fx-err{font-size:12.5px;line-height:1.45;padding:8px 10px;border-radius:7px;background:rgba(239,68,68,.1);color:#fca5a5}',
    '.fx-field{display:flex;flex-direction:column;gap:4px}',
    '.fx-field>span{font-size:12px;color:var(--text-secondary)}',
    '.fx-input{width:100%;box-sizing:border-box;background:var(--bg-primary);color:var(--text-primary);border:1px solid var(--border);border-radius:6px;padding:7px 9px;font:inherit;font-size:13px}',
    '.fx-input:focus{outline:none;border-color:var(--accent)}',
    'textarea.fx-input{resize:vertical;line-height:1.45}',
    '.fx-modal-bg{position:fixed;inset:0;background:rgba(0,0,0,.6);z-index:900;display:flex;align-items:center;justify-content:center;padding:16px}',
    '.fx-modal{background:var(--bg-secondary);border:1px solid var(--border);border-radius:12px;width:min(560px,100%);max-height:calc(100vh - 32px);overflow-y:auto;padding:16px;display:flex;flex-direction:column;gap:12px;box-shadow:0 24px 60px rgba(0,0,0,.6)}',
    '.fx-modal h3{margin:0;font-size:15px}',
    '.fx-modal-actions{display:flex;justify-content:flex-end;gap:8px;flex-wrap:wrap}',
    '.fx-modal.fx-modal-wide{width:min(680px,100%)}',
    // The PIN prompt at the start of a run, like the page's own PIN dialog, above the start form.
    '.fx-pin-bg{position:fixed;inset:0;background:rgba(0,0,0,.6);z-index:950;display:flex;align-items:center;justify-content:center;padding:20px}',
    '.fx-pin{background:var(--bg-secondary);border:1px solid var(--border);border-radius:12px;padding:22px;width:100%;max-width:340px;text-align:center;display:flex;flex-direction:column;gap:10px;box-shadow:0 24px 60px rgba(0,0,0,.6)}',
    '.fx-pin h3{margin:0;font-size:15px;display:flex;align-items:center;justify-content:center;gap:6px}',
    '.fx-pin h3 .ic{width:16px;height:16px}',
    '.fx-pin p{margin:0;font-size:12.5px;color:var(--text-secondary);line-height:1.45;word-break:break-word}',
    '.fx-pin input{width:100%;box-sizing:border-box;text-align:center;letter-spacing:.4em;font-size:18px;padding:9px;background:var(--bg-primary);color:var(--text-primary);border:1px solid var(--border);border-radius:6px}',
    '.fx-pin input:focus{outline:none;border-color:var(--accent)}',
    '.fx-pin-err{color:var(--red);font-size:12px;min-height:16px}',
    '.fx-pin-step{font-size:11.5px;color:var(--text-muted)}',
    '.fx-pin-btns{display:flex;gap:8px}',
    '.fx-pin-btns .fx-btn{flex:1;justify-content:center}',
    '.fx-panel-live{position:absolute;inset:0;display:flex;flex-direction:column;min-height:0}',
    '.fx-section{display:flex;flex-direction:column;gap:2px;margin:18px 2px 10px;font-size:14px}',
    '.fx-section span{font-size:12.5px;color:var(--text-secondary)}',
    '.fx-tpl .fx-card-desc{-webkit-line-clamp:3}',
    '.fx-days{display:flex;gap:4px;flex-wrap:wrap}',
    '.fx-day{background:var(--bg-card);border:1px solid var(--border);color:var(--text-secondary);border-radius:6px;padding:5px 9px;font:inherit;font-size:12px;cursor:pointer}',
    '.fx-day.on{background:var(--accent);border-color:var(--accent);color:#fff}',
    '.fx-check{display:flex;align-items:center;gap:8px;font-size:13px}',
    '.fx-chip-s{font-size:11px;color:var(--text-secondary);border:1px solid var(--border);border-radius:999px;padding:1px 7px;margin-left:6px}',
    '.fx-run.off{opacity:.6}',
    '.fx-triggers{margin-top:12px}',
    '.fx-cr-view{font-size:12.5px;margin:4px 0}',
    '.fx-cr-view summary{cursor:pointer;color:var(--text-secondary)}',
    '.fx-cr-view-k{font-weight:600;margin-top:6px;font-size:12px}',
    '.fx-cr-view-v{white-space:pre-wrap;color:var(--text-secondary);line-height:1.45}',
    '.fx-modal-actions .fx-sp{flex:1}',
    '.fx-cr-row{display:grid;grid-template-columns:1fr 1fr;gap:10px}',
    '.fx-cr{display:flex;flex-direction:column;gap:10px;border:1px solid var(--border);border-radius:10px;padding:12px;background:var(--bg-primary)}',
    '.fx-cr.drop{border-color:var(--accent);background:var(--bg-hover)}',
    '.fx-cr-head{display:flex;flex-direction:column;gap:2px;font-size:13px}',
    '.fx-cr-head span{color:var(--text-secondary);font-size:12px}',
    '.fx-field i{font-style:normal;color:var(--text-muted,var(--text-secondary));font-size:11px;margin-left:4px;font-weight:400}',
    '.fx-cr textarea.fx-input{resize:vertical;min-height:44px;line-height:1.45}',
    '.fx-bad{border-color:var(--error,#e5484d)!important;box-shadow:0 0 0 2px rgba(229,72,77,.25)}',
    '.fx-cr-att{display:flex;flex-wrap:wrap;align-items:center;gap:6px}',
    '.fx-cr-att-label{font-size:12px;color:var(--text-secondary);margin-right:4px}',
    '.fx-cr-chips{display:contents}',
    '.fx-cr-hint{flex-basis:100%;font-size:11.5px;color:var(--text-secondary)}',
    '.fx-chip{display:inline-flex;align-items:center;gap:5px;max-width:220px;background:var(--bg-tertiary);border:1px solid var(--border);border-radius:999px;padding:3px 4px 3px 9px;font-size:12px}',
    '.fx-chip span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.fx-chip svg{width:13px;height:13px;flex:none}',
    '.fx-chip-x{background:none;border:0;color:var(--text-secondary);cursor:pointer;padding:2px;display:inline-flex;border-radius:50%}',
    '.fx-chip-x:hover{color:var(--text-primary);background:var(--bg-hover)}',
    '@media (max-width:520px){.fx-cr-row{grid-template-columns:1fr}}',
    '.fx-chatwrap{position:fixed;inset:0;z-index:900;background:rgba(0,0,0,.6);display:flex;align-items:stretch;justify-content:flex-end}',
    '.fx-chatbox{width:min(560px,100%);background:var(--bg-primary);border-left:1px solid var(--border);display:flex;flex-direction:column}',
    '.fx-chatbar{display:flex;align-items:center;gap:8px;padding:8px 10px;border-bottom:1px solid var(--border)}',
    '.fx-chatbar b{flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
    '.fx-chatbox iframe{flex:1;border:0;width:100%;background:var(--bg-primary)}',
    '.fx-table{width:100%;border-collapse:collapse;font-size:12.5px}',
    '.fx-table td,.fx-table th{padding:8px 6px;border-bottom:1px solid var(--border);text-align:left;vertical-align:top}',
    '.fx-table th{color:var(--text-muted);font-weight:500;font-size:11.5px}',
    '.fx-panel{padding:12px;display:flex;flex-direction:column;gap:10px;font-size:13px}',
    '.fx-steps{display:flex;flex-direction:column;gap:3px}',
    '.fx-step{display:flex;gap:6px;align-items:center;font-size:12.5px}',
    '.fx-dot{width:7px;height:7px;border-radius:50%;background:var(--green);flex:none}',
    '.fx-dot.answer{background:var(--sky)}.fx-dot.ask{background:#c084fc}.fx-dot.join{background:var(--text-muted)}',
    '.fx-kv{display:grid;grid-template-columns:auto 1fr;gap:3px 10px;font-size:12.5px}',
    '.fx-kv>:nth-child(odd){color:var(--text-muted)}',
    '@media (max-width:760px){.fx-modal{width:100%;max-height:100%;border-radius:12px 12px 0 0;align-self:flex-end}.fx-modal-bg{padding:0;align-items:flex-end}.fx-chatbox{width:100%}.fx-head{padding:8px 10px}.fx-body{padding:10px}}',
  ].join('\n');

  function injectCss() {
    if (document.getElementById('fx-styles')) return;
    var s = document.createElement('style');
    s.id = 'fx-styles';
    s.textContent = CSS;
    document.head.appendChild(s);
  }
  function el(tag, cls, html) { var e = document.createElement(tag); if (cls) e.className = cls; if (html != null) e.innerHTML = html; return e; }
  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]; }); }
  function ago(iso) {
    var t = Date.parse(iso || ''); if (!t) return '';
    var s = Math.max(0, (Date.now() - t) / 1000);
    if (s < 60) return 'just now'; if (s < 3600) return Math.floor(s / 60) + 'm ago'; if (s < 86400) return Math.floor(s / 3600) + 'h ago';
    return Math.floor(s / 86400) + 'd ago';
  }
  function dur(ms) { var m = Math.floor(ms / 60000); if (m < 60) return m + ' min'; return Math.floor(m / 60) + ' h ' + (m % 60) + ' min'; }
  function size(b) { if (!b) return '0'; var u = ['B', 'KB', 'MB', 'GB']; var i = Math.min(3, Math.floor(Math.log(b) / Math.log(1024))); return (b / Math.pow(1024, i)).toFixed(i ? 1 : 0) + ' ' + u[i]; }
  function btn(label, cls, icon, ctx) { var b = el('button', 'fx-btn ' + (cls || ''), (icon && ctx ? ctx.icon(icon) : '') + '<span>' + esc(label) + '</span>'); return b; }
  async function getJson(ctx, path) { var r = await ctx.apiFetch(path); return r.json(); }
  async function post(ctx, path, body) { var r = await ctx.apiFetch(path, { method: 'POST', body: JSON.stringify(body) }); return r.json(); }

  var STATUS = { starting: 'Starting', running: 'Running', waiting: 'Waiting on you', paused: 'Paused', crashed: 'Crashed', finishing: 'Done: your decision', finished: 'Finished', stopped: 'Stopped' };
  var WAITING = { ask: 'A question for you', cap: 'Reached its limit', manager: 'The Manager is talking to you', paused: 'Paused', usage: 'Paused: usage limit' };
  function statusChip(run) {
    var label = STATUS[run.status] || run.status;
    if (run.status === 'running' && run.waitingOn) label = WAITING[run.waitingOn] || label;
    var cls = run.status === 'running' && run.waitingOn ? 'waiting' : run.status;
    return '<span class="fx-chip ' + esc(cls) + '">' + esc(label) + '</span>';
  }

  // ─── Events from the host (factoryRun / factories SSE) ───
  var listeners = [];
  function runEvent(d) { listeners.slice().forEach(function (fn) { try { fn(d || {}); } catch (e) { /* a view's problem */ } }); }
  function onEvent(fn) { listeners.push(fn); return function () { listeners = listeners.filter(function (x) { return x !== fn; }); }; }

  // ─── factory-flow.js, loaded on first use ───
  var flowLoading = null;
  function loadFlow(ctx) {
    if (window.CrundiFactoryFlow) return Promise.resolve(window.CrundiFactoryFlow);
    if (flowLoading) return flowLoading;
    flowLoading = new Promise(function (resolve, reject) {
      var link = document.createElement('link'); link.rel = 'stylesheet'; link.href = ctx.flowCss; document.head.appendChild(link);
      var s = document.createElement('script'); s.src = ctx.flowJs;
      s.onload = function () { window.CrundiFactoryFlow ? resolve(window.CrundiFactoryFlow) : reject(new Error('The factory editor did not load.')); };
      s.onerror = function () { flowLoading = null; reject(new Error('The factory editor did not load. Check the connection and try again.')); };
      document.head.appendChild(s);
    });
    return flowLoading;
  }

  // ─── A person's chat ───
  // The Manager's chat is in the workbench: go there. An employee's chat is a
  // background chat, opened as the compact chat page beside whatever is open.
  function openChat(ctx, chatId, name) {
    if (!chatId) return;
    if (ctx.openManagerChat && ctx.openManagerChat(chatId)) return;
    var wrap = el('div', 'fx-chatwrap');
    var box = el('div', 'fx-chatbox');
    var bar = el('div', 'fx-chatbar', '<b>' + esc(name || 'Chat') + '</b>');
    var tab = el('a', 'fx-btn ghost', 'Open in a new tab'); tab.href = '/chat/' + encodeURIComponent(chatId); tab.target = '_blank'; tab.rel = 'noopener';
    var close = btn('Close', 'ghost', 'x', ctx);
    bar.appendChild(tab); bar.appendChild(close);
    var frame = el('iframe'); frame.src = '/chat/' + encodeURIComponent(chatId); frame.title = name || 'Chat';
    box.appendChild(bar); box.appendChild(frame); wrap.appendChild(box);
    var done = function () { wrap.remove(); document.removeEventListener('keydown', key); window.removeEventListener('message', msg); };
    var key = function (e) { if (e.key === 'Escape') done(); };
    // The chat page's own back arrow, inside the frame, asks to be closed.
    var msg = function (e) { if (e.origin === location.origin && e.data && e.data.type === 'crundi-chat-close') done(); };
    window.addEventListener('message', msg);
    close.onclick = done;
    wrap.addEventListener('click', function (e) { if (e.target === wrap) done(); });
    document.addEventListener('keydown', key);
    document.body.appendChild(wrap);
  }

  function modal(title) {
    var bg = el('div', 'fx-modal-bg');
    var m = el('div', 'fx-modal');
    m.appendChild(el('h3', '', esc(title)));
    bg.appendChild(m);
    var close = function () { bg.remove(); document.removeEventListener('keydown', key); };
    var key = function (e) { if (e.key === 'Escape') close(); };
    bg.addEventListener('click', function (e) { if (e.target === bg) close(); });
    document.addEventListener('keydown', key);
    document.body.appendChild(bg);
    return { box: m, close: close };
  }

  // ─── The factory's secrets: their PINs, asked at the start of a run ───
  //
  // One secret at a time. Each PIN is checked by the server (action 'check',
  // which never returns the value) before the next is asked; Skip leaves that
  // secret locked, and the person is asked when the team first uses it.
  // Resolves { pins: { id: PIN } }, or null when the person cancels.
  function askSecretPins(ctx, list) {
    return new Promise(function (resolve) {
      var pins = {}, i = 0;
      var bg = el('div', 'fx-pin-bg');
      var box = el('div', 'fx-pin'); bg.appendChild(box);
      var h = el('h3'), stepEl = el('div', 'fx-pin-step'), sub = el('p');
      var inp = el('input'); inp.type = 'password'; inp.inputMode = 'numeric'; inp.maxLength = 6; inp.autocomplete = 'off'; inp.placeholder = '••••••';
      var err = el('div', 'fx-pin-err');
      var row = el('div', 'fx-pin-btns');
      var cancel = btn('Cancel', 'ghost', '', ctx), skip = btn('Skip', '', '', ctx), unlock = btn('Unlock', 'primary', 'lock', ctx);
      row.appendChild(cancel); row.appendChild(skip); row.appendChild(unlock);
      [h, stepEl, sub, inp, err, row].forEach(function (x) { box.appendChild(x); });
      function done(v) { bg.remove(); window.removeEventListener('keydown', key, true); resolve(v); }
      function show() {
        if (i >= list.length) return done({ pins: pins });
        var x = list[i];
        h.innerHTML = ctx.icon('lock') + '<span>' + esc(x.name) + '</span>';
        stepEl.textContent = list.length > 1 ? 'Secret ' + (i + 1) + ' of ' + list.length : '';
        sub.textContent = 'This factory may use this secret. Its PIN is kept in memory for this run only, so the team does not stop to ask; the value is never shown. Skip to be asked when it is first used.';
        inp.value = ''; err.textContent = '';
        setTimeout(function () { inp.focus(); }, 30);
      }
      async function submit() {
        var pin = inp.value.trim();
        if (!/^[0-9]{6}$/.test(pin)) { err.textContent = 'Enter a 6-digit PIN'; return; }
        unlock.disabled = true;
        var r = await post(ctx, '/api/secrets', { action: 'check', id: list[i].id, pin: pin }).catch(function (e) { return { ok: false, error: e.message }; });
        unlock.disabled = false;
        if (!r || !r.ok) { err.textContent = (r && r.error) || 'Failed'; inp.select(); inp.focus(); return; }
        pins[list[i].id] = pin; i++; show();
      }
      // Captured before the start form's own Escape, which would close it too.
      function key(e) {
        if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); done(null); }
        else if (e.key === 'Enter' && document.activeElement === inp) { e.preventDefault(); e.stopPropagation(); submit(); }
      }
      cancel.onclick = function () { done(null); };
      skip.onclick = function () { i++; show(); };
      unlock.onclick = submit;
      window.addEventListener('keydown', key, true);
      document.body.appendChild(bg);
      show();
    });
  }

  /** The factory's secrets by name, when the server can pre-authorise them; [] otherwise. */
  async function secretsToUnlock(ctx, f) {
    var ids = (f && f.secrets) || [];
    if (!ids.length) return [];
    var caps = await serverCaps(ctx);
    if (caps.indexOf('factory-secrets') < 0) return [];
    var d = await getJson(ctx, '/api/secrets').catch(function () { return null; });
    var known = (d && d.secrets) || [];
    return ids.map(function (id) { return known.find(function (x) { return x.id === id; }); }).filter(Boolean).map(function (x) { return { id: x.id, name: x.name }; });
  }

  // ─── Start a factory: a change request ───
  //
  // The run starts from a change request (CR) built from the factory's CR
  // template: a title, the template's sections (goal, situation, expected
  // result, done when…) and attachments. The draft is kept per factory in this
  // browser, so closing the dialog loses nothing. Attachments upload to the
  // project's crundi_attachments folder at Start (like the chat's paperclip);
  // the server copies them into the run's shared folder.
  var CR_DRAFT = 'crundi.factoryCr.';
  var MAX_ATTACH = 10, MAX_BYTES = 25 * 1024 * 1024;
  function readDraft(fid) { try { return JSON.parse(localStorage.getItem(CR_DRAFT + fid) || 'null'); } catch (e) { return null; } }
  function saveDraft(fid, d) { try { localStorage.setItem(CR_DRAFT + fid, JSON.stringify(d)); } catch (e) { /* full or blocked */ } }
  function dropDraft(fid) { try { localStorage.removeItem(CR_DRAFT + fid); } catch (e) { /* blocked */ } }
  function fileB64(file) {
    return new Promise(function (res, rej) {
      var r = new FileReader();
      r.onload = function () { var s = String(r.result || ''); res(s.slice(s.indexOf(',') + 1)); };
      r.onerror = function () { rej(new Error('Could not read ' + file.name)); };
      r.readAsDataURL(file);
    });
  }
  function grow(t) { t.style.height = 'auto'; t.style.height = Math.min(t.scrollHeight + 2, 320) + 'px'; }

  async function startDialog(ctx, opts) {
    opts = opts || {};
    var md = modal('Start a factory');
    md.box.classList.add('fx-modal-wide');
    var b = md.box;
    var body = el('div', 'fx-list'); b.appendChild(body);
    body.appendChild(el('div', 'fx-note', 'Loading…'));
    var projects = (ctx.projects() || []);
    var project = opts.project || ctx.currentProject() || (projects[0] && projects[0].alias) || '';
    var lib = await getJson(ctx, '/api/factories').catch(function () { return { factories: [] }; });
    var factories = lib.factories || [];
    body.innerHTML = '';
    if (!factories.length) {
      body.appendChild(el('div', 'fx-empty', 'There are no factories yet. Create one in the Factories tab first.'));
      var a0 = el('div', 'fx-modal-actions'); var c0 = btn('Close', 'ghost'); c0.onclick = md.close; a0.appendChild(c0); b.appendChild(a0); return;
    }
    var projSel = el('select', 'fx-input');
    projects.forEach(function (p) { var o = el('option', '', esc(p.name || p.alias)); o.value = p.alias; if (p.alias === project) o.selected = true; projSel.appendChild(o); });
    var fSel = el('select', 'fx-input');
    factories.forEach(function (f) {
      var o = el('option', '', esc(f.name) + (f.runnable ? '' : ' (draft: ' + f.errorCount + ' to fix)'));
      o.value = f.id; o.disabled = !f.runnable;
      if (opts.factoryId === f.id || (!opts.factoryId && f.runnable && !fSel.value)) o.selected = true;
      fSel.appendChild(o);
    });
    var mm = el('select', 'fx-input');
    [['', 'As the factory says'], ['ask', 'Show me first, I decide'], ['auto', 'Merge automatically (with Undo)']].forEach(function (x) { var o = el('option', '', x[1]); o.value = x[0]; mm.appendChild(o); });
    var info = el('div', 'fx-list');
    var field = function (label, input, cls) { var f = el('label', 'fx-field' + (cls ? ' ' + cls : '')); f.appendChild(el('span', '', label)); f.appendChild(input); return f; };
    var top = el('div', 'fx-cr-row');
    top.appendChild(field('Project', projSel));
    top.appendChild(field('Factory', fSel));
    body.appendChild(top);

    // The change request.
    var crBox = el('div', 'fx-cr');
    crBox.appendChild(el('div', 'fx-cr-head', '<b>Change request</b><span>The Manager gets all of it; every step gets the title, goal and the checks for done, and can open the rest.</span>'));
    var title = el('input', 'fx-input'); title.placeholder = 'One line: what should change';  title.maxLength = 120;
    crBox.appendChild(field('Title <i>required</i>', title));
    var secWrap = el('div', 'fx-list'); crBox.appendChild(secWrap);
    var inputs = {};          // section id -> textarea
    var template = [];
    var files = [];           // File objects, uploaded at Start
    var attWrap = el('div', 'fx-cr-att');
    var attList = el('div', 'fx-cr-chips');
    var attBtn = btn('Attach', '', 'paperclip', ctx);
    var fileIn = el('input'); fileIn.type = 'file'; fileIn.multiple = true; fileIn.style.display = 'none';
    attWrap.appendChild(el('span', 'fx-cr-att-label', 'Attachments'));
    attWrap.appendChild(attList); attWrap.appendChild(attBtn); attWrap.appendChild(fileIn);
    attWrap.appendChild(el('div', 'fx-cr-hint', 'Screenshots, mockups, specs, logs. Drop or paste them here too.'));
    crBox.appendChild(attWrap);
    body.appendChild(crBox);
    body.appendChild(field('When the work is done', mm));
    body.appendChild(info);
    var actions = el('div', 'fx-modal-actions');
    var clear = btn('Clear form', 'ghost');
    var cancel = btn('Cancel', 'ghost'); cancel.onclick = md.close;
    var go = btn('Start', 'primary', 'play', ctx);
    actions.appendChild(clear); actions.appendChild(el('span', 'fx-sp')); actions.appendChild(cancel); actions.appendChild(go); b.appendChild(actions);

    function current() { return factories.find(function (f) { return f.id === fSel.value; }) || null; }
    function values() {
      var v = { title: title.value, sections: {} };
      Object.keys(inputs).forEach(function (k) { v.sections[k] = inputs[k].value; });
      return v;
    }
    var saveT = null;
    function save() { clearTimeout(saveT); saveT = setTimeout(function () { if (fSel.value) saveDraft(fSel.value, values()); }, 250); }
    function renderSections(keep) {
      var f = current();
      template = (f && f.crTemplate) || [];
      var d = keep || readDraft(fSel.value) || { title: '', sections: {} };
      if (!keep) title.value = d.title || title.value || '';
      secWrap.innerHTML = ''; var old = inputs; inputs = {};
      template.forEach(function (sec) {
        var t = el('textarea', 'fx-input'); t.rows = sec.list ? 3 : 2;
        t.placeholder = sec.hint + (sec.list && !/per line/i.test(sec.hint) ? '\nOne per line.' : '');
        t.value = (d.sections && d.sections[sec.id]) || (old[sec.id] ? old[sec.id].value : '');
        t.addEventListener('input', function () { grow(t); save(); });
        inputs[sec.id] = t;
        secWrap.appendChild(field(esc(sec.label) + (sec.required ? ' <i>required</i>' : ''), t));
        setTimeout(function () { grow(t); }, 0);
      });
    }
    title.addEventListener('input', save);
    function renderFiles() {
      attList.innerHTML = '';
      files.forEach(function (f, i) {
        var chip = el('span', 'fx-chip', ctx.icon(/^image\//.test(f.type) ? 'image' : 'file') + '<span>' + esc(f.name) + '</span>');
        var x = el('button', 'fx-chip-x', ctx.icon('x')); x.title = 'Remove'; x.onclick = function (e) { e.preventDefault(); files.splice(i, 1); renderFiles(); };
        chip.appendChild(x); attList.appendChild(chip);
      });
    }
    function addFiles(list) {
      for (var i = 0; i < list.length; i++) {
        var f = list[i];
        if (files.length >= MAX_ATTACH) { ctx.toast('Up to ' + MAX_ATTACH + ' attachments.', 'error'); break; }
        if (f.size > MAX_BYTES) { ctx.toast(f.name + ' is over 25 MB.', 'error'); continue; }
        files.push(f);
      }
      renderFiles();
    }
    attBtn.onclick = function (e) { e.preventDefault(); fileIn.click(); };
    fileIn.onchange = function () { addFiles(fileIn.files); fileIn.value = ''; };
    crBox.addEventListener('dragover', function (e) { if (e.dataTransfer && Array.prototype.indexOf.call(e.dataTransfer.types || [], 'Files') >= 0) { e.preventDefault(); crBox.classList.add('drop'); } });
    crBox.addEventListener('dragleave', function (e) { if (e.target === crBox) crBox.classList.remove('drop'); });
    crBox.addEventListener('drop', function (e) { if (!e.dataTransfer || !e.dataTransfer.files.length) return; e.preventDefault(); crBox.classList.remove('drop'); addFiles(e.dataTransfer.files); });
    crBox.addEventListener('paste', function (e) {
      var items = (e.clipboardData && e.clipboardData.files) || [];
      if (!items.length) return;
      e.preventDefault();
      var named = Array.prototype.map.call(items, function (f, i) { return f.name && f.name !== 'image.png' ? f : new File([f], 'pasted-' + Date.now() + '-' + i + '.png', { type: f.type }); });
      addFiles(named);
    });
    clear.onclick = function () {
      if ((title.value || Object.keys(inputs).some(function (k) { return inputs[k].value; })) && !window.confirm('Clear the change request?')) return;
      title.value = ''; Object.keys(inputs).forEach(function (k) { inputs[k].value = ''; grow(inputs[k]); });
      files = []; renderFiles(); dropDraft(fSel.value); title.focus();
    };

    var pre = null;
    async function check() {
      info.innerHTML = '';
      go.disabled = true;
      pre = await getJson(ctx, '/api/factory-runs/precheck?project=' + encodeURIComponent(projSel.value)).catch(function () { return { ok: false, error: 'Could not check the project.' }; });
      if (pre.active) {
        info.appendChild(el('div', 'fx-err', 'A factory is already ' + (pre.active.status === 'crashed' ? 'waiting to be recovered' : 'running') + ' here ("' + esc(pre.active.factoryName) + '"). One at a time per project.'));
        var open = btn('Open it', '', 'route', ctx); open.onclick = function () { md.close(); ctx.openTab(pre.active.id); }; info.appendChild(open);
        return;
      }
      if (!pre.ok) { info.appendChild(el('div', 'fx-err', esc(pre.error))); return; }
      info.appendChild(el('div', 'fx-note', 'The team works in its own worktree, on a new branch cut from <b>' + esc(pre.branch) + '</b>. Your checkout is not touched.'));
      if (pre.dirty) info.appendChild(el('div', 'fx-warn', 'You have uncommitted changes. The run starts from the last commit on ' + esc(pre.branch) + ', so it will not see them.'));
      go.disabled = !fSel.value;
    }
    projSel.onchange = check;
    var lastF = fSel.value;
    fSel.onchange = function () {
      if (lastF) saveDraft(lastF, values());
      lastF = fSel.value;
      // Another factory may ask different questions: keep what matches, by section.
      var mine = readDraft(fSel.value);
      renderSections(mine ? null : values());
      go.disabled = !fSel.value || !pre || !pre.ok || !!pre.active;
    };
    renderSections(null);
    await check();
    setTimeout(function () { (title.value ? (inputs[template[0] && template[0].id] || title) : title).focus(); }, 50);

    function fieldError(id, msg) {
      var t = id === 'title' ? title : inputs[id];
      ctx.toast(msg, 'error');
      if (t) { t.classList.add('fx-bad'); t.focus(); setTimeout(function () { t.classList.remove('fx-bad'); }, 2500); }
    }
    var heldPins = null;
    async function start(ignoreMissingSkills) {
      if (!title.value.trim()) return fieldError('title', 'Give the change request a title.');
      for (var i = 0; i < template.length; i++) {
        var sec = template[i];
        if (sec.required && !inputs[sec.id].value.trim()) return fieldError(sec.id, 'Fill in "' + sec.label + '".');
      }
      // The factory's secrets: their PINs now, while the person is here.
      // "Start anyway" right after keeps the PINs just typed.
      var secretPins = ignoreMissingSkills && heldPins ? heldPins : null;
      var toUnlock = secretPins ? [] : await secretsToUnlock(ctx, current());
      if (toUnlock.length) {
        var got = await askSecretPins(ctx, toUnlock);
        if (!got) return;
        secretPins = heldPins = got.pins;
      }
      go.disabled = true; var lbl = go.querySelector('span');
      var attachments = [];
      for (var j = 0; j < files.length; j++) {
        lbl.textContent = 'Uploading ' + (j + 1) + '/' + files.length + '…';
        var up = await fileB64(files[j]).then(function (data) { return post(ctx, '/api/attachments/upload', { project: projSel.value, name: files[j].name, data: data }); }).catch(function (e) { return { ok: false, error: e.message }; });
        if (!up.ok) { lbl.textContent = 'Start'; go.disabled = false; ctx.toast('Could not upload ' + files[j].name + ': ' + (up.error || 'failed'), 'error'); return; }
        attachments.push({ path: up.path, name: files[j].name });
      }
      lbl.textContent = 'Starting…';
      var v = values();
      var r = await post(ctx, '/api/factory-runs', { action: 'start', project: projSel.value, factoryId: fSel.value, cr: { title: v.title, sections: v.sections, attachments: attachments }, mergeMode: mm.value || undefined, ignoreMissingSkills: !!ignoreMissingSkills, secretPins: secretPins || undefined }).catch(function (e) { return { ok: false, error: e.message }; });
      lbl.textContent = 'Start'; go.disabled = false;
      if (r.ok || r.field === 'secrets') heldPins = null;
      if (r.ok) { dropDraft(fSel.value); md.close(); ctx.toast('Factory started.', 'success'); if (opts.onStarted) opts.onStarted(r.run); return; }
      if (r.field === 'secrets') { info.innerHTML = ''; info.appendChild(el('div', 'fx-err', esc(r.error || 'A secret could not be unlocked.') + ' Press Start to type it again.')); return; }
      if (r.field && r.field !== 'attachments') return fieldError(r.field, r.error);
      if (r.missingSkills) {
        info.innerHTML = '';
        info.appendChild(el('div', 'fx-warn', 'This factory names skill' + (r.missingSkills.length === 1 ? '' : 's') + ' the project does not have: <b>' + r.missingSkills.map(function (n) { return '/' + esc(n); }).join(', ') + '</b>. You can install ' + (r.missingSkills.length === 1 ? 'it' : 'them') + ' in Settings, under Skills, or start without.'));
        var anyway = btn('Start anyway', '', 'play', ctx); anyway.onclick = function () { start(true); };
        info.appendChild(anyway);
        return;
      }
      info.innerHTML = '';
      info.appendChild(el('div', 'fx-err', esc(r.error || 'It could not start.')));
      if (r.errors) r.errors.slice(0, 5).forEach(function (e) { info.appendChild(el('div', 'fx-note', '· ' + esc(e.message))); });
    }
    go.onclick = function () { start(false); };
  }

  /** The run's change request, folded: what the team was asked for. */
  function crDetails(cr) {
    var d = el('details', 'fx-cr-view');
    d.appendChild(el('summary', '', 'Change request' + (cr.attachments && cr.attachments.length ? ' · ' + cr.attachments.length + ' attachment' + (cr.attachments.length === 1 ? '' : 's') : '')));
    (cr.sections || []).forEach(function (x) {
      d.appendChild(el('div', 'fx-cr-view-k', esc(x.label)));
      d.appendChild(el('div', 'fx-cr-view-v', esc(x.text)));
    });
    if (cr.attachments && cr.attachments.length) d.appendChild(el('div', 'fx-meta', esc(cr.attachments.map(function (a) { return a.name; }).join(', '))));
    return d;
  }

  // What the server can do (its handshake), asked once. A server that does not
  // list 'factory-pause-options' gets a plain Pause, as before.
  var capsAsked = null;
  function serverCaps(ctx) {
    if (!capsAsked) capsAsked = getJson(ctx, '/api/handshake').then(function (hs) { return (hs && hs.server && hs.server.capabilities) || []; }).catch(function () { capsAsked = null; return []; });
    return capsAsked;
  }

  /** What a paused run says about itself: who paused it, and whether it carries on by itself. */
  function pausedText(p) {
    if (p.by === 'usage') return 'Paused: usage reached the limit set for this run; it carries on by itself after the window resets.';
    var head = p.by === 'manager' ? 'Paused by the Manager' + (p.reason ? ': ' + p.reason : '') : 'Paused';
    return p.resumeOnReset ? head + '; it carries on by itself when the usage window resets.' : p.by === 'manager' && p.reason ? head : head + '.';
  }

  /**
   * Pause, with its choices when the server has them: let the steps in
   * progress finish their turn or stop them now, and optionally carry on by
   * itself when the usage window resets. A small card under the buttons.
   */
  function pauseChoice(wrap, act) {
    var old = wrap.querySelector('.fx-pause-choice');
    if (old) { old.remove(); return; }
    var box = el('div', 'fx-card fx-pause-choice'); box.style.flexBasis = '100%';
    box.appendChild(el('div', 'fx-note', 'Pause: nobody new starts a step. Steps in progress can finish their turn, or stop now and carry on at Resume.'));
    var lab = el('label', 'fx-check'); var reset = el('input'); reset.type = 'checkbox'; lab.appendChild(reset); lab.appendChild(el('span', '', 'Carry on by itself when usage resets')); box.appendChild(lab);
    var row = el('div', 'fx-row-btns');
    var fin = btn('Let current steps finish', ''); fin.onclick = function () { act({ action: 'pause', mode: 'finish', resumeOnReset: reset.checked }); };
    var now = btn('Stop them now', 'ghost danger'); now.onclick = function () { act({ action: 'pause', mode: 'now', resumeOnReset: reset.checked }); };
    var no = btn('Cancel', 'ghost'); no.onclick = function () { box.remove(); };
    row.appendChild(fin); row.appendChild(now); row.appendChild(no); box.appendChild(row);
    wrap.appendChild(box);
  }

  // ─── The run's controls, shared by the panel ───
  function runControls(ctx, run, refresh) {
    var wrap = el('div', 'fx-row-btns');
    var act = async function (body, confirmText) {
      if (confirmText && !window.confirm(confirmText)) return;
      var r = await post(ctx, '/api/factory-runs', Object.assign({ id: run.id }, body)).catch(function (e) { return { ok: false, error: e.message }; });
      if (!r.ok) ctx.toast(r.error || 'That did not work.', 'error');
      refresh();
    };
    var graph = btn('Live graph', 'primary', 'git-branch', ctx); graph.onclick = function () { ctx.openTab(run.id); }; wrap.appendChild(graph);
    if (run.managerChatId) { var mc = btn('Manager\'s chat', '', 'message-square', ctx); mc.onclick = function () { openChat(ctx, run.managerChatId, 'Manager'); }; wrap.appendChild(mc); }
    if ((run.status === 'running' || run.status === 'waiting') && !run.paused) {
      var p = btn('Pause', 'ghost');
      p.onclick = async function () {
        var caps = await serverCaps(ctx);
        if (caps.indexOf('factory-pause-options') >= 0) pauseChoice(wrap, act); else act({ action: 'pause' });
      };
      wrap.appendChild(p);
    }
    if (run.paused || run.capHit) { var rs = btn(run.capHit ? 'Let it carry on' : 'Resume', ''); rs.onclick = function () { act({ action: 'resume' }); }; wrap.appendChild(rs); }
    if (['starting', 'running', 'waiting', 'paused', 'crashed', 'finishing'].indexOf(run.status) >= 0) {
      var st = btn('Stop', 'ghost danger'); st.onclick = function () { act({ action: 'stop' }, 'Stop this factory? Its team stops and the Manager\'s chat stays as an ordinary chat. The worktree is kept (see Cleanup).'); }; wrap.appendChild(st);
    }
    return wrap;
  }

  function runSteps(run) {
    var box = el('div', 'fx-steps');
    (run.tokens || []).forEach(function (t) {
      var what = { work: 'working on', answer: 'deciding', ask: 'You are asked', join: 'waiting at', finished: 'finished', delegated: 'waiting for a job it handed out at' }[t.status] || t.status;
      var line = t.status === 'ask' ? 'You are asked a question' : t.status === 'join' ? 'A branch is waiting at ' + t.label : t.status === 'wait' ? 'Waiting at ' + t.label
        : t.job ? (t.employee || 'Someone') + ' · a job from the Manager: ' + t.job.slice(0, 120)
        : (t.employee || 'Someone') + ' · ' + what + ' ' + t.label;
      if (t.blocked) line += ' (waiting for a permission answer)';
      box.appendChild(el('div', 'fx-step', '<span class="fx-dot ' + esc(t.status) + '"></span><span>' + esc(line) + '</span>'));
    });
    return box;
  }

  /** The review findings so far: what reviewers asked for and where each stands. */
  function findingsView(items) {
    var open = items.filter(function (i) { return i.status === 'open' || i.status === 'claimed'; });
    var d = el('details', 'fx-cr-view');
    d.appendChild(el('summary', '', 'Review findings: ' + open.length + ' open of ' + items.length));
    items.forEach(function (i) {
      var st = { open: 'open', claimed: 'said fixed', closed: 'verified', dismissed: 'dismissed' }[i.status] || i.status;
      d.appendChild(el('div', 'fx-meta', '<b>' + esc(i.id) + '</b> ' + esc(st) + (i.blocking ? ' · blocking' : ' · nit') + (i.reopened ? ' · reopened ' + i.reopened + 'x' : '') + ': ' + esc(i.text) + (i.where ? ' (' + esc(i.where) + ')' : '')));
    });
    return d;
  }

  // ─── The project's Factory panel ───
  function mountPanel(root, ctx) {
    injectCss();
    var project = '';
    var off = onEvent(function (d) { if (!d.project || d.project === project) load(project); });
    var busy = false;
    var live = null;   // { runId, view, host }: the live graph filling the panel while a run is going
    function dropLive() { if (live) { try { live.view.destroy(); } catch (e) { /* gone */ } live = null; } }
    async function load(p) {
      var next = String(p || ctx.currentProject() || '').toLowerCase();
      if (next !== project) dropLive();
      project = next;
      if (!project || busy) return;
      busy = true;
      var j = await getJson(ctx, '/api/factory-runs?project=' + encodeURIComponent(project)).catch(function () { return { runs: [] }; });
      busy = false;
      var runs = j.runs || [];
      var active = runs.filter(function (r) { return ['starting', 'running', 'waiting', 'paused', 'crashed', 'finishing'].indexOf(r.status) >= 0; })[0];
      // A run is going: the panel is its live graph, the god's-eye view. It
      // keeps itself up to date; only a different run (or none) replaces it.
      if (active && live && live.runId === active.id) return;
      dropLive();
      // The live graph fills the panel; the list scrolls as before.
      root.style.position = active ? 'relative' : '';
      root.style.overflow = active ? 'hidden' : '';
      // The pane gives its body a height, not the panel: take all of it.
      root.style.height = active ? '100%' : '';
      root.style.minHeight = active ? '0' : '';
      if (active) {
        root.innerHTML = '';
        var host = el('div', 'fx-panel-live');
        root.appendChild(host);
        live = { runId: active.id, host: host, view: { destroy: function () {} } };
        var mine = live;
        try {
          var F = await loadFlow(ctx);
          if (live !== mine) return;
          live.view = F.mountViewer(host, { apiFetch: ctx.apiFetch, runId: active.id, subscribe: onEvent, mode: 'panel', openChat: function (chatId, name) { openChat(ctx, chatId, name); } });
        } catch (err) {
          host.innerHTML = '<div class="fx-err">' + esc(err.message || 'The live graph did not load.') + '</div>';
        }
        return;
      }
      var past = runs.filter(function (r) { return r !== active; }).slice(0, 8);
      root.innerHTML = '';
      var box = el('div', 'fx-panel');
      if (active) {
        var full = active.id ? await getJson(ctx, '/api/factory-runs/get?id=' + encodeURIComponent(active.id)).catch(function () { return {}; }) : {};
        var run = full.run || active;
        var top = el('div', 'fx-run-top', '<span class="fx-run-name">' + esc(run.factoryName) + '</span>' + statusChip(run));
        box.appendChild(top);
        box.appendChild(el('div', 'fx-run-task', esc(run.task)));
        if (run.cr) box.appendChild(crDetails(run.cr));
        var kv = el('div', 'fx-kv');
        kv.innerHTML = '<span>Running for</span><span>' + esc(dur(Date.now() - Date.parse(run.createdAt))) + '</span>'
          + '<span>Handoffs</span><span>' + esc(run.handoffs) + '</span>'
          + '<span>Sessions</span><span>' + esc(run.liveSessions || 0) + ' live</span>'
          + '<span>Branch</span><span>' + esc(run.branch || '') + '</span>'
          + (run.usage && run.usage.turns ? '<span>So far</span><span>' + esc((run.usage.costUsd ? '$' + run.usage.costUsd.toFixed(2) + ' · ' : '') + run.usage.turns + ' turns') + '</span>' : '');
        box.appendChild(kv);
        if (run.tokens && run.tokens.length) box.appendChild(runSteps(run));
        if (run.findings && run.findings.length) box.appendChild(findingsView(run.findings));
        if (run.status === 'crashed') {
          box.appendChild(el('div', 'fx-warn', 'It stopped: ' + esc(run.crash && run.crash.reason || 'it was interrupted') + '. Recover picks it up where it was.'));
          var note = el('textarea', 'fx-input'); note.rows = 2; note.placeholder = 'Anything the team should know (optional)'; box.appendChild(note);
          var rec = btn('Recover', 'primary', 'rotate-ccw', ctx);
          rec.onclick = async function () { rec.disabled = true; var r = await post(ctx, '/api/factory-runs', { action: 'recover', id: run.id, note: note.value }); if (!r.ok) ctx.toast(r.error, 'error'); load(project); };
          box.appendChild(rec);
        }
        (run.asks || []).forEach(function (a) {
          var q = el('div', 'fx-card'); q.appendChild(el('div', '', '<b>' + esc(a.prompt) + '</b>'));
          var n2 = el('textarea', 'fx-input'); n2.rows = 2; n2.placeholder = 'Optional note'; q.appendChild(n2);
          var rb = el('div', 'fx-row-btns');
          a.replies.forEach(function (rp) { var x = btn(rp.label, ''); x.onclick = async function () { var r = await post(ctx, '/api/factory-runs', { action: 'ask', id: run.id, tokenId: a.tokenId, replyId: rp.id, note: n2.value }); if (!r.ok) ctx.toast(r.error, 'error'); load(project); }; rb.appendChild(x); });
          q.appendChild(rb); box.appendChild(q);
        });
        (run.tokens || []).filter(function (t) { return t.wait; }).forEach(function (t) { box.appendChild(waitCard(ctx, run, t, function () { load(project); })); });
        if (run.status === 'finishing') box.appendChild(el('div', 'fx-note', 'The team is done. The result is waiting for you in the Manager\'s chat: merge it, ask for changes, keep the branch, or discard it.'));
        if (run.waitingOn === 'manager') box.appendChild(el('div', 'fx-note', 'The Manager is talking to you in its chat.'));
        if (run.paused) box.appendChild(el('div', 'fx-note', esc(pausedText(run.paused))));
        if (run.capHit) box.appendChild(el('div', 'fx-warn', 'It reached its limit (' + esc(run.capHit.which === 'maxMinutes' ? run.capHit.limit + ' minutes' : run.capHit.limit + ' handoffs') + ') and is waiting for you.'));
        box.appendChild(runControls(ctx, run, function () { load(project); }));
      } else {
        box.appendChild(el('div', 'fx-note', 'A factory is a team of Claude employees, run by a Manager, that takes a large piece of work from start to finish in its own worktree. One runs at a time per project.'));
        var start = btn('Start a factory', 'primary', 'play', ctx);
        start.onclick = function () { startDialog(ctx, { project: project, onStarted: function () { load(project); } }); };
        box.appendChild(start);
      }
      if (past.length) {
        var det = el('details', '');
        det.appendChild(el('summary', 'fx-meta', 'Earlier runs (' + past.length + ')'));
        var list = el('div', 'fx-list'); list.style.marginTop = '8px';
        past.forEach(function (r) {
          var row = el('div', 'fx-run', '<div class="fx-run-top"><span class="fx-run-name">' + esc(r.factoryName) + '</span>' + statusChip(r) + '</div><div class="fx-run-task">' + esc(r.task) + '</div><div class="fx-meta">' + esc(ago(r.updatedAt)) + (r.outcome && r.outcome.how ? ' · ' + esc(r.outcome.how) : '') + '</div>');
          row.onclick = function () { ctx.openTab(r.id); };
          list.appendChild(row);
        });
        det.appendChild(list);
        box.appendChild(det);
      }
      box.appendChild(await triggersSection(ctx, project, function () { load(project); }));
      root.appendChild(box);
    }
    return { load: load, destroy: function () { dropLive(); off(); } };
  }

  // ─── Waits ───
  function waitCard(ctx, run, t, refresh) {
    var w = t.wait;
    var left = function (iso) { var ms = Date.parse(iso) - Date.now(); if (!(ms > 0)) return 'any moment'; var m = Math.floor(ms / 60000); return m >= 60 ? Math.floor(m / 60) + ' h ' + (m % 60) + ' min' : m >= 1 ? m + ' min' : Math.ceil(ms / 1000) + ' s'; };
    var two = w.kind === 'command' || w.kind === 'signal';
    var c = el('div', 'fx-card');
    var head = w.kind === 'time' || w.kind === 'until' ? left(w.deadline) + ' left' : w.kind === 'signal' ? (w.what || 'waiting for a signal') : 'checking (' + w.tries + ' tr' + (w.tries === 1 ? 'y' : 'ies') + ' so far)';
    c.appendChild(el('div', '', ctx.icon('clock') + ' <b>' + esc(t.label) + '</b>: ' + esc(head)));
    if (two) c.appendChild(el('div', 'fx-meta', 'Gives up in ' + esc(left(w.timeoutAt)) + '.'));
    if (w.signalPath) {
      var u = el('input', 'fx-input mono'); u.readOnly = true; u.value = location.origin + w.signalPath; u.onfocus = function () { u.select(); };
      c.appendChild(el('div', 'fx-meta', 'POST to this URL to end it (body optional: {"note": "…"}, or {"outcome": "failed"}):'));
      c.appendChild(u);
    }
    var rb = el('div', 'fx-row-btns');
    var end = function (outcome) { return async function () { var r = await post(ctx, '/api/factory-runs', { action: 'endWait', id: run.id, tokenId: t.id, outcome: outcome }); if (!r.ok) ctx.toast(r.error, 'error'); refresh(); }; };
    var e1 = btn('End the wait now', ''); e1.onclick = end('done'); rb.appendChild(e1);
    if (two) { var e2 = btn('Treat as timed out', 'ghost'); e2.onclick = end('timeout'); rb.appendChild(e2); }
    c.appendChild(rb);
    return c;
  }

  // ─── Triggers: what starts a run by itself ───
  var DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  var COLUMNS = [['backlog', 'Backlog'], ['todo', 'To do'], ['in_progress', 'In progress'], ['done', 'Done']];
  function triggerWhen(t) {
    if (t.kind === 'schedule') return 'At ' + t.schedule.at + (t.schedule.days.length && t.schedule.days.length < 7 ? ' on ' + t.schedule.days.map(function (d) { return DAYS[d]; }).join(', ') : ' every day');
    if (t.kind === 'kanban') return 'When a card enters ' + (COLUMNS.find(function (c) { return c[0] === t.kanban.status; }) || ['', t.kanban.status])[1];
    return 'When its webhook is called';
  }
  async function triggersSection(ctx, project, refresh) {
    var j = await getJson(ctx, '/api/factory-triggers?project=' + encodeURIComponent(project)).catch(function () { return { triggers: [] }; });
    var list = j.triggers || [];
    var det = el('details', 'fx-triggers');
    if (list.length) det.open = true;
    det.appendChild(el('summary', 'fx-meta', 'Triggers' + (list.length ? ' (' + list.length + ')' : '') + ': start a run by itself'));
    var body = el('div', 'fx-list'); body.style.marginTop = '8px';
    if (!list.length) body.appendChild(el('div', 'fx-note', 'Start a factory at a time of day, when a kanban card enters a column, or when a webhook is called (CI, an issue tracker). The change request is filled in from the card or the call.'));
    list.forEach(function (t) {
      var last = (t.history || [])[0];
      var row = el('div', 'fx-run' + (t.enabled ? '' : ' off'), '<div class="fx-run-top"><span class="fx-run-name">' + esc(t.name) + '</span>' + (t.enabled ? '' : '<span class="fx-chip-s">off</span>') + (t.queue && t.queue.length ? '<span class="fx-chip-s">' + t.queue.length + ' waiting</span>' : '') + '</div><div class="fx-run-task">' + esc(triggerWhen(t)) + '</div>' + (last ? '<div class="fx-meta">' + esc(ago(last.at)) + ': ' + esc(last.outcome) + (last.note ? ' (' + esc(last.note) + ')' : '') + '</div>' : ''));
      row.onclick = function () { triggerDialog(ctx, project, t, refresh); };
      body.appendChild(row);
    });
    var add = btn('Add a trigger', '', 'plus', ctx); add.onclick = function () { triggerDialog(ctx, project, null, refresh); };
    body.appendChild(add);
    det.appendChild(body);
    return det;
  }

  async function triggerDialog(ctx, project, t, refresh) {
    var md = modal(t ? 'Trigger' : 'New trigger');
    md.box.classList.add('fx-modal-wide');
    var b = md.box;
    var lib = await getJson(ctx, '/api/factories').catch(function () { return { factories: [] }; });
    var factories = (lib.factories || []).filter(function (f) { return f.runnable || (t && f.id === t.factoryId); });
    if (!factories.length) { b.appendChild(el('div', 'fx-empty', 'There are no runnable factories yet. Create one in the Factories tab first.')); var c0 = btn('Close', 'ghost'); c0.onclick = md.close; b.appendChild(c0); return; }
    var cur = t || { name: '', enabled: true, kind: 'kanban', factoryId: factories[0].id, schedule: { at: '09:00', days: [] }, kanban: { status: 'todo' }, cr: { title: '', sections: {} }, mergeMode: '', ifBusy: 'queue' };
    var field = function (label, input) { var f = el('label', 'fx-field'); f.appendChild(el('span', '', label)); f.appendChild(input); return f; };
    var sel = function (opts, v) { var s2 = el('select', 'fx-input'); opts.forEach(function (o) { var x = el('option', '', esc(o[1])); x.value = o[0]; if (o[0] === v) x.selected = true; s2.appendChild(x); }); return s2; };
    var name = el('input', 'fx-input'); name.value = cur.name; name.placeholder = 'Cards to the team';
    var fSel = sel(factories.map(function (f) { return [f.id, f.name]; }), cur.factoryId);
    var kind = sel([['kanban', 'A kanban card enters a column'], ['schedule', 'A time of day'], ['webhook', 'A webhook is called']], cur.kind);
    var row = el('div', 'fx-cr-row'); row.appendChild(field('Name', name)); row.appendChild(field('Factory', fSel)); b.appendChild(row);
    b.appendChild(field('Start when', kind));
    var kindBox = el('div', 'fx-list'); b.appendChild(kindBox);
    var at = el('input', 'fx-input'); at.type = 'time'; at.value = cur.schedule.at;
    var days = cur.schedule.days.slice();
    var col = sel(COLUMNS, cur.kanban.status);
    var crBox = el('div', 'fx-cr'); b.appendChild(crBox);
    var title = el('input', 'fx-input'); title.value = cur.cr.title;
    var inputs = {};
    var mm = sel([['', 'As the factory says'], ['ask', 'Show me first, I decide'], ['auto', 'Merge automatically (with Undo)']], cur.mergeMode);
    var busy = sel([['manager', 'Tell its Manager (a run of the same factory)'], ['queue', 'Wait its turn, then start'], ['skip', 'Skip it']], cur.ifBusy);
    var row2 = el('div', 'fx-cr-row'); row2.appendChild(field('When the work is done', mm)); row2.appendChild(field('If a run is already going', busy)); b.appendChild(row2);
    var en = el('label', 'fx-check'); var enI = el('input'); enI.type = 'checkbox'; enI.checked = cur.enabled !== false; en.appendChild(enI); en.appendChild(el('span', '', 'On')); b.appendChild(en);
    // Webhook settings: how calls prove who they are from, and what makes a repeat.
    var wh = cur.webhook || {};
    var whBox = el('div', 'fx-list'); b.appendChild(whBox);
    var auth = sel([['key', 'A key in the URL (or X-Crundi-Key / Bearer header)'], ['signature', 'Signed with a secret (HMAC-SHA256)']], wh.auth || 'key');
    var sigH = el('input', 'fx-input mono'); sigH.value = wh.signatureHeader || 'x-crundi-signature'; sigH.placeholder = 'x-crundi-signature';
    var tsH = el('input', 'fx-input mono'); tsH.value = wh.timestampHeader || ''; tsH.placeholder = 'None (Grafana: X-Grafana-Alerting-Signature-Timestamp)';
    var idemI = el('input', 'fx-input mono'); idemI.value = wh.idempotency || ''; idemI.placeholder = '{{payload.messageId}}';
    var sample = el('textarea', 'fx-input mono'); sample.rows = 4; sample.placeholder = '{"messageId": "a1", "message": "CPU above 90% on web-1"}';
    var prevOut = el('div', 'fx-list');
    function renderWebhookSettings() {
      whBox.innerHTML = '';
      if (kind.value !== 'webhook') return;
      whBox.appendChild(el('div', 'fx-cr-head', '<b>Webhook</b><span>Public URL, so every call must prove it is from you: the key, or a signature made with the secret. Calls are answered at once; the change request above is filled from the call\'s JSON.</span>'));
      whBox.appendChild(field('Security', auth));
      var sigRow = el('div', 'fx-cr-row');
      sigRow.appendChild(field('Signature header', sigH)); sigRow.appendChild(field('Timestamp header (optional)', tsH));
      if (auth.value === 'signature') whBox.appendChild(sigRow);
      whBox.appendChild(field('Idempotency key <i>a call with a key already seen is not processed again</i>', idemI));
      var pd = el('details', ''); pd.appendChild(el('summary', 'fx-meta', 'Try it with a sample payload'));
      pd.appendChild(sample);
      var pb = btn('Preview', 'ghost'); pb.onclick = previewSample;
      var pr = el('div', 'fx-row-btns'); pr.appendChild(pb); pd.appendChild(pr); pd.appendChild(prevOut);
      whBox.appendChild(pd);
    }
    async function previewSample() {
      var body;
      try { body = sample.value.trim() ? JSON.parse(sample.value) : {}; } catch (e) { prevOut.innerHTML = ''; prevOut.appendChild(el('div', 'fx-note', 'That is not valid JSON: ' + esc(e.message))); return; }
      var r = await post(ctx, '/api/factory-triggers', { action: 'preview', trigger: collect(), sample: { body: body } }).catch(function (e) { return { ok: false, error: e.message }; });
      prevOut.innerHTML = '';
      if (!r.ok || !r.cr) { prevOut.appendChild(el('div', 'fx-note', esc(r.error || 'No preview.'))); return; }
      var c = r.cr;
      var html = '<div><b>' + esc(c.title) + '</b></div>';
      Object.keys(c.sections || {}).forEach(function (k) { html += '<div class="fx-meta"><b>' + esc(k) + ':</b> ' + esc(c.sections[k]) + '</div>'; });
      html += '<div class="fx-meta">Idempotency key: ' + (c.idem ? '<code>' + esc(c.idem) + '</code>' : '<i>none in this call' + (idemI.value.trim() ? ', so repeats cannot be told apart' : '') + '</i>') + '</div>';
      var miss = (c.missing || []).concat(c.idemMissing || []);
      if (miss.length) html += '<div class="fx-meta">Not in this payload (left empty, never an error): ' + miss.map(function (m) { return '<code>{{' + esc(m) + '}}</code>'; }).join(' ') + '</div>';
      html += '<div class="fx-meta">The Manager also gets the whole request (headers, query, body) as a file.</div>';
      prevOut.appendChild(el('div', 'fx-note', html));
    }
    auth.onchange = renderWebhookSettings;
    var hookBox = el('div', 'fx-list'); b.appendChild(hookBox);
    var hist = el('div', 'fx-list'); b.appendChild(hist);

    function placeholders() {
      var k = kind.value;
      return k === 'kanban' ? '{{card.title}}, {{card.description}}, {{card.todos}} (one per line), {{date}}'
        : k === 'webhook' ? '{{payload.some.field}}, {{payload.alerts.0.labels.alertname}} (arrays by index), {{payload}} (all of it), {{headers.name}}, {{date}}' : '{{date}}, {{time}}';
    }
    function renderKind() {
      kindBox.innerHTML = '';
      if (kind.value === 'schedule') {
        kindBox.appendChild(field('At (this server\'s time)', at));
        var dw = el('div', 'fx-days');
        DAYS.forEach(function (d, i) { var x = el('button', 'fx-day' + (days.indexOf(i) >= 0 ? ' on' : ''), d); x.type = 'button'; x.onclick = function () { var k = days.indexOf(i); if (k >= 0) days.splice(k, 1); else days.push(i); x.classList.toggle('on'); }; dw.appendChild(x); });
        kindBox.appendChild(field('On (none = every day)', dw));
      }
      if (kind.value === 'kanban') kindBox.appendChild(field('Column', col));
      // New webhook triggers default to telling the running Manager.
      if (kind.value === 'webhook' && !t && busy.value === 'queue') busy.value = 'manager';
      renderWebhookSettings();
      renderCr();
    }
    function renderCr() {
      var f = factories.find(function (x) { return x.id === fSel.value; });
      var tpl = (f && f.crTemplate) || [];
      var keep = {}; Object.keys(inputs).forEach(function (k) { keep[k] = inputs[k].value; });
      // Switched away from kanban: card placeholders would come out empty.
      if (kind.value !== 'kanban') { Object.keys(keep).forEach(function (k) { if (/\{\{\s*card\./.test(keep[k])) keep[k] = ''; }); if (/\{\{\s*card\./.test(title.value)) title.value = ''; }
      crBox.innerHTML = '';
      crBox.appendChild(el('div', 'fx-cr-head', '<b>Change request</b><span>Filled in when it fires. Placeholders: ' + esc(placeholders()) + '. ' + (kind.value === 'webhook' ? 'Anything a call does not have comes out empty and never stops it; the Manager gets the full request as well.' : 'Only the title has to come out filled; the Manager asks you about anything missing.') + '</span>'));
      if (!title.value && kind.value === 'kanban') title.value = '{{card.title}}';
      crBox.appendChild(field('Title <i>required</i>', title));
      inputs = {};
      tpl.forEach(function (sec) {
        var ta = el('textarea', 'fx-input'); ta.rows = 2; ta.placeholder = sec.hint;
        ta.value = keep[sec.id] != null ? keep[sec.id] : (cur.cr.sections[sec.id] || (kind.value === 'kanban' && !t ? ({ goal: '{{card.description}}', done: '{{card.todos}}' })[sec.id] || '' : ''));
        ta.addEventListener('input', function () { grow(ta); });
        inputs[sec.id] = ta;
        crBox.appendChild(field(esc(sec.label), ta));
        setTimeout(function () { grow(ta); }, 0);
      });
    }
    function renderHook(tr) {
      hookBox.innerHTML = '';
      if (!tr || tr.kind !== 'webhook') return;
      var signed = tr.webhook.auth === 'signature';
      var u = location.origin + '/api/factory-hook/trigger/' + tr.id + (signed ? '' : '?key=' + tr.webhook.key);
      var ro = function (v) { var i = el('input', 'fx-input mono'); i.readOnly = true; i.value = v; i.onfocus = function () { i.select(); }; return i; };
      hookBox.appendChild(field('Webhook URL (POST, JSON body)', ro(u)));
      if (signed) {
        hookBox.appendChild(field('Secret (sign the raw body with HMAC-SHA256)', ro(tr.webhook.secret)));
        var ts = tr.webhook.timestampHeader;
        hookBox.appendChild(el('div', 'fx-meta', esc('Send the signature in ' + tr.webhook.signatureHeader + ' as hex (or sha256=<hex>)' + (ts ? ', and the time in ' + ts + '; sign "<time>:<body>"' : '') + '. Grafana: in the webhook contact point set HMAC Signature with this secret' + (ts ? ' and this timestamp header' : '') + '.')));
        var sh = "body='{\"messageId\": \"a1\", \"message\": \"test\"}'; sig=$(printf '%s' " + (ts ? '"$(date +%s):$body"' : '"$body"') + " | openssl dgst -sha256 -hmac '" + tr.webhook.secret + "' -hex | sed 's/.* //'); curl -X POST -H 'Content-Type: application/json' -H \"" + tr.webhook.signatureHeader + ": $sig\" -d \"$body\" '" + u + "'";
        if (!ts) hookBox.appendChild(el('div', 'fx-meta mono', esc(sh)));
      } else {
        hookBox.appendChild(el('div', 'fx-meta mono', esc("curl -X POST -H 'Content-Type: application/json' -d '{\"messageId\": \"a1\", \"message\": \"test\"}' '" + u + "'")));
      }
      if (tr.seenCount) hookBox.appendChild(el('div', 'fx-meta', esc(tr.seenCount + ' idempotency key' + (tr.seenCount === 1 ? '' : 's') + ' remembered (30 days).')));
      var which = signed ? 'secret' : 'key';
      var rot = btn(signed ? 'New secret' : 'New key', 'ghost'); rot.onclick = async function () { if (!window.confirm(signed ? 'Calls signed with the old secret stop working. Make a new one?' : 'The old URL stops working. Make a new key?')) return; var r = await post(ctx, '/api/factory-triggers', { action: 'rotate', id: tr.id, which: which }); if (r.ok) renderHook(r.trigger); };
      var rr = el('div', 'fx-row-btns'); rr.appendChild(rot); hookBox.appendChild(rr);
    }
    function renderHistory(tr) {
      hist.innerHTML = '';
      if (!tr || !(tr.history || []).length) return;
      var d = el('details', ''); d.appendChild(el('summary', 'fx-meta', 'What it did (' + tr.history.length + ')'));
      tr.history.forEach(function (h) { d.appendChild(el('div', 'fx-meta', esc(ago(h.at)) + ' · ' + esc(h.outcome) + ' · ' + esc(h.source || '') + (h.title ? ': ' + esc(h.title) : '') + (h.note ? ' (' + esc(h.note) + ')' : ''))); });
      hist.appendChild(d);
    }
    kind.onchange = renderKind; fSel.onchange = renderCr;
    renderKind(); renderHook(t); renderHistory(t);

    var actions = el('div', 'fx-modal-actions');
    var saved = t;
    if (t) {
      var del = btn('Delete', 'ghost', 'trash', ctx);
      del.onclick = async function () { if (!window.confirm('Delete this trigger?')) return; var r = await post(ctx, '/api/factory-triggers', { action: 'delete', id: t.id }); if (r.ok) { md.close(); refresh(); } else ctx.toast(r.error, 'error'); };
      actions.appendChild(del);
      var now = btn('Run now', '', 'play', ctx);
      now.onclick = async function () { var r = await post(ctx, '/api/factory-triggers', { action: 'run', id: t.id }); ctx.toast(r.ok ? (r.queued ? 'Queued: a run is going in this project.' : 'Started.') : (r.error || 'It could not start.'), r.ok ? 'success' : 'error'); refresh(); };
      actions.appendChild(now);
    }
    actions.appendChild(el('span', 'fx-sp'));
    var cancel = btn('Close', 'ghost'); cancel.onclick = md.close; actions.appendChild(cancel);
    var save = btn('Save', 'primary', 'check', ctx); actions.appendChild(save);
    b.appendChild(actions);
    function collect() {
      var sections = {}; Object.keys(inputs).forEach(function (k) { if (inputs[k].value.trim()) sections[k] = inputs[k].value; });
      return { id: saved ? saved.id : undefined, name: name.value, enabled: enI.checked, project: project, factoryId: fSel.value, kind: kind.value, schedule: { at: at.value || '09:00', days: days }, kanban: { status: col.value }, cr: { title: title.value, sections: sections }, mergeMode: mm.value, ifBusy: busy.value,
        webhook: { auth: auth.value, signatureHeader: sigH.value, timestampHeader: tsH.value, idempotency: idemI.value } };
    }
    save.onclick = async function () {
      var tr = collect();
      var r = await post(ctx, '/api/factory-triggers', { action: 'save', trigger: tr }).catch(function (e) { return { ok: false, error: e.message }; });
      if (!r.ok) { ctx.toast(r.error || 'Could not save.', 'error'); return; }
      saved = r.trigger;
      ctx.toast('Saved.', 'success');
      refresh();
      if (r.trigger.kind === 'webhook') { renderHook(r.trigger); md.box.querySelector('h3').textContent = 'Trigger'; } else md.close();
    };
  }

  // ─── The Factories tab ───
  function mountTab(root, ctx) {
    injectCss();
    root.innerHTML = '';
    var wrap = el('div', 'fx'); root.appendChild(wrap);
    var head = el('div', 'fx-head');
    head.appendChild(el('span', 'fx-title', 'Factories'));
    var seg = el('div', 'fx-seg');
    var views = [['library', 'Library'], ['runs', 'Runs'], ['cleanup', 'Cleanup']];
    var segBtns = {};
    views.forEach(function (v) { var b = el('button', '', esc(v[1])); b.onclick = function () { show(v[0]); }; segBtns[v[0]] = b; seg.appendChild(b); });
    head.appendChild(seg);
    head.appendChild(el('span', 'fx-sp'));
    var headActions = el('div', 'fx-row-btns'); head.appendChild(headActions);
    wrap.appendChild(head);
    var body = el('div', 'fx-body'); wrap.appendChild(body);
    var full = el('div', 'fx-full'); full.style.display = 'none'; wrap.appendChild(full);
    var current = 'library';
    var sub = null;   // a mounted editor / viewer
    var visible = false;

    onEvent(function (d) {
      if (!visible || sub) return;
      if (current === 'runs' || (current === 'library' && d.kind === 'factories')) show(current);
    });

    function closeSub() {
      if (sub) { try { sub.destroy(); } catch (e) { /* gone */ } sub = null; }
      full.style.display = 'none'; full.innerHTML = '';
      head.style.display = ''; body.style.display = '';
    }
    async function openSub(kind, id) {
      closeSub();
      head.style.display = 'none'; body.style.display = 'none';
      full.style.display = ''; full.innerHTML = '<div class="fx-empty">Loading…</div>';
      try {
        var F = await loadFlow(ctx);
        full.innerHTML = '';
        var host = el('div', ''); host.style.height = '100%'; host.style.minHeight = '0'; host.style.flex = '1'; full.appendChild(host);
        if (kind === 'editor') sub = F.mountEditor(host, { apiFetch: ctx.apiFetch, factoryId: id, onClose: function () { closeSub(); show('library'); } });
        else sub = F.mountViewer(host, { apiFetch: ctx.apiFetch, runId: id, subscribe: onEvent, openChat: function (chatId, name) { openChat(ctx, chatId, name); }, onClose: function () { closeSub(); show(current); } });
      } catch (e) {
        full.innerHTML = '';
        full.appendChild(el('div', 'fx-err', esc(e.message)));
        var back = btn('Back', ''); back.onclick = function () { closeSub(); show(current); }; full.appendChild(back);
      }
    }

    async function show(view) {
      current = view || current;
      closeSub();
      Object.keys(segBtns).forEach(function (k) { segBtns[k].classList.toggle('on', k === current); });
      headActions.innerHTML = '';
      if (current === 'library') return library();
      if (current === 'runs') return runsView();
      if (current === 'cleanup') return cleanupView();
    }

    async function library() {
      var newB = btn('New factory', 'primary', 'plus', ctx);
      var imp = btn('Import', 'ghost', 'upload', ctx);
      var startB = btn('Start one', '', 'play', ctx);
      headActions.appendChild(startB); headActions.appendChild(imp); headActions.appendChild(newB);
      startB.onclick = function () { startDialog(ctx, { onStarted: function (run) { openSub('viewer', run.id); } }); };
      newB.onclick = async function () {
        var name = window.prompt('Name the new factory', 'New factory');
        if (name == null) return;
        var r = await post(ctx, '/api/factories', { action: 'create', name: name || 'New factory' });
        if (!r.ok) return ctx.toast(r.error || 'Could not create it.', 'error');
        openSub('editor', r.factory.id);
      };
      imp.onclick = function () {
        var input = el('input'); input.type = 'file'; input.accept = '.json,application/json';
        input.onchange = async function () {
          var f = input.files[0]; if (!f) return;
          var text = await f.text();
          var r = await post(ctx, '/api/factories', { action: 'import', data: text });
          if (!r.ok) return ctx.toast(r.error || 'Could not import it.', 'error');
          ctx.toast('Imported "' + r.factory.name + '".' + (r.validation.ok ? '' : ' It is a draft: open it to fix ' + r.validation.errors.length + ' problem' + (r.validation.errors.length === 1 ? '' : 's') + '.'), 'success');
          library();
        };
        input.click();
      };
      body.innerHTML = '<div class="fx-empty">Loading…</div>';
      var both = await Promise.all([
        getJson(ctx, '/api/factories').catch(function () { return { factories: [] }; }),
        getJson(ctx, '/api/factory-templates').catch(function () { return { templates: [] }; }),
      ]);
      var j = both[0], tpls = both[1].templates || [];
      if (current !== 'library' || sub) return;
      body.innerHTML = '';
      var list = j.factories || [];
      if (!list.length) body.appendChild(el('div', 'fx-empty', 'No factories of your own yet. A factory is a team: a Manager who owns the change request and employees with personas, wired together as a flow of steps and decisions. Start from one of the templates below, build your own with New factory, or ask any Claude chat to build one for you.'));
      var grid = el('div', 'fx-grid');
      list.forEach(function (f) {
        var card = el('div', 'fx-card');
        card.appendChild(el('div', 'fx-card-head', '<span class="fx-card-name">' + esc(f.name) + '</span>' + (f.runnable ? '<span class="fx-chip ok">Ready</span>' : '<span class="fx-chip draft">Draft · ' + f.errorCount + ' to fix</span>')));
        card.appendChild(el('div', 'fx-card-desc', esc(f.description || 'No description.')));
        card.appendChild(el('div', 'fx-meta', f.employeeCount + ' people · ' + f.nodeCount + ' steps · edited ' + esc(ago(f.updatedAt))));
        var row = el('div', 'fx-row-btns');
        var edit = btn('Edit', 'primary', 'pencil', ctx); edit.onclick = function () { openSub('editor', f.id); }; row.appendChild(edit);
        var st = btn('Start', '', 'play', ctx); st.disabled = !f.runnable; st.title = f.runnable ? '' : 'Fix it in the editor first';
        st.onclick = function () { startDialog(ctx, { factoryId: f.id, onStarted: function (run) { openSub('viewer', run.id); } }); }; row.appendChild(st);
        var dup = btn('Copy', 'ghost', 'copy', ctx);
        dup.onclick = async function () { var r = await post(ctx, '/api/factories', { action: 'duplicate', id: f.id }); if (!r.ok) ctx.toast(r.error, 'error'); library(); }; row.appendChild(dup);
        var ex = btn('Export', 'ghost', 'download', ctx);
        ex.onclick = async function () {
          var r = await getJson(ctx, '/api/factories/export?id=' + encodeURIComponent(f.id));
          if (!r.ok) return ctx.toast(r.error, 'error');
          var blob = new Blob([JSON.stringify(r.data, null, 2)], { type: 'application/json' });
          var a = el('a'); a.href = URL.createObjectURL(blob); a.download = (f.name || 'factory').replace(/[^a-z0-9-]+/gi, '-').toLowerCase() + '.factory.json'; a.click();
          setTimeout(function () { URL.revokeObjectURL(a.href); }, 2000);
        };
        row.appendChild(ex);
        var del = btn('Delete', 'ghost danger', 'trash', ctx);
        del.onclick = async function () {
          if (!window.confirm('Delete "' + f.name + '"? Runs that already started keep their own copy.')) return;
          var r = await post(ctx, '/api/factories', { action: 'delete', id: f.id }); if (!r.ok) ctx.toast(r.error, 'error'); library();
        };
        row.appendChild(del);
        card.appendChild(row);
        grid.appendChild(card);
      });
      if (list.length) body.appendChild(grid);
      // The provided templates: read-only starting points, copied on use.
      if (tpls.length) {
        body.appendChild(el('div', 'fx-section', '<b>Templates</b><span>Modelled on how real teams work. Use one as it is, copy it and change it to fit how your team works, or build your own from scratch.</span>'));
        var tg = el('div', 'fx-grid');
        tpls.forEach(function (t) {
          var card = el('div', 'fx-card fx-tpl');
          card.appendChild(el('div', 'fx-card-head', '<span class="fx-card-name">' + esc(t.name) + '</span><span class="fx-chip">Template</span>'));
          card.appendChild(el('div', 'fx-card-desc', esc(t.goodFor)));
          card.appendChild(el('div', 'fx-meta', esc(t.description)));
          card.appendChild(el('div', 'fx-meta', '<b>Team:</b> Manager, ' + esc(t.team.join(', '))));
          var row = el('div', 'fx-row-btns');
          var use = btn('Use this', 'primary', 'copy', ctx);
          use.onclick = async function () {
            use.disabled = true;
            var r = await post(ctx, '/api/factories', { action: 'fromTemplate', templateId: t.id });
            use.disabled = false;
            if (!r.ok) return ctx.toast(r.error || 'Could not copy it.', 'error');
            ctx.toast('"' + t.name + '" is in your factories now. Change it however you like.', 'success');
            openSub('editor', r.factory.id);
          };
          row.appendChild(use);
          card.appendChild(row);
          tg.appendChild(card);
        });
        body.appendChild(tg);
      }
    }

    async function runsView() {
      var j = await getJson(ctx, '/api/factory-runs').catch(function () { return { runs: [] }; });
      if (current !== 'runs' || sub) return;
      body.innerHTML = '';
      var runs = j.runs || [];
      if (!runs.length) { body.appendChild(el('div', 'fx-empty', 'No runs yet. Start a factory from the Library, or from a project\'s Factory panel.')); return; }
      var list = el('div', 'fx-list');
      runs.forEach(function (r) {
        var row = el('div', 'fx-run');
        row.innerHTML = '<div class="fx-run-top"><span class="fx-run-name">' + esc(r.factoryName) + ' <span class="fx-meta">in ' + esc(r.project) + '</span></span>' + statusChip(r) + '</div>'
          + '<div class="fx-run-task">' + esc(r.task) + '</div>'
          + '<div class="fx-meta">' + esc(r.handoffs) + ' handoffs · ' + esc(r.liveSessions || 0) + ' live sessions · ' + esc(ago(r.updatedAt)) + (r.outcome && r.outcome.how ? ' · ' + esc(r.outcome.how) : '') + '</div>';
        row.onclick = function () { openSub('viewer', r.id); };
        list.appendChild(row);
      });
      body.appendChild(list);
    }

    async function cleanupView() {
      body.innerHTML = '<div class="fx-empty">Looking for leftovers…</div>';
      var j = await getJson(ctx, '/api/factory-cleanup').catch(function () { return { items: [], settings: {} }; });
      if (current !== 'cleanup' || sub) return;
      body.innerHTML = '';
      var items = j.items || [];
      body.appendChild(el('div', 'fx-note', 'Merging and discarding clean up after themselves. What is listed here was kept: stopped runs, kept branches, and folders no run knows about. Nothing is deleted unless you choose to.'));
      var auto = el('div', 'fx-row-btns'); auto.style.alignItems = 'center'; auto.style.margin = '10px 0';
      var days = el('input', 'fx-input'); days.type = 'number'; days.min = '0'; days.max = '365'; days.style.width = '80px'; days.value = (j.settings && j.settings.autoDeleteDays) || 0;
      auto.appendChild(el('span', 'fx-note', 'Delete leftovers older than'));
      auto.appendChild(days);
      auto.appendChild(el('span', 'fx-note', 'days automatically (0 = never; never one with uncommitted work, never a kept branch).'));
      days.onchange = async function () { var r = await post(ctx, '/api/factory-cleanup', { action: 'settings', autoDeleteDays: +days.value || 0 }); ctx.toast(r.ok ? 'Saved.' : (r.error || 'Not saved'), r.ok ? 'success' : 'error'); };
      body.appendChild(auto);
      if (!items.length) { body.appendChild(el('div', 'fx-empty', 'Nothing to clean up.')); return; }
      var table = el('table', 'fx-table');
      table.innerHTML = '<thead><tr><th></th><th>What</th><th>Size</th><th>Age</th></tr></thead>';
      var tb = el('tbody');
      var picks = {};
      items.forEach(function (it) {
        var tr = el('tr');
        var cb = el('input'); cb.type = 'checkbox'; cb.onchange = function () { picks[it.id] = cb.checked; };
        var td0 = el('td'); td0.appendChild(cb); tr.appendChild(td0);
        tr.appendChild(el('td', '', '<b>' + esc(it.kind === 'kept-branch' ? 'Kept branch ' + it.branch : it.kind === 'orphan' ? 'Unknown worktree folder' : 'Worktree of a ' + it.status + ' run') + '</b>'
          + '<div class="fx-meta">' + esc(it.project) + (it.factoryName ? ' · ' + esc(it.factoryName) : '') + (it.path ? ' · ' + esc(it.path) : '') + '</div>'
          + '<div class="fx-meta">' + esc(it.why) + '</div>'
          + (it.uncommitted ? '<div class="fx-warn" style="margin-top:4px">' + it.uncommitted + ' uncommitted change' + (it.uncommitted === 1 ? '' : 's') + ' in it: deleting loses them.</div>' : '')));
        tr.appendChild(el('td', '', esc(it.kind === 'kept-branch' ? '-' : size(it.bytes) + (it.sizeCapped ? '+' : ''))));
        tr.appendChild(el('td', '', esc(Math.floor(it.ageMs / 86400000) + 'd')));
        tb.appendChild(tr);
      });
      table.appendChild(tb);
      body.appendChild(table);
      var rm = btn('Delete selected', 'danger', 'trash', ctx); rm.style.marginTop = '10px';
      rm.onclick = async function () {
        var ids = Object.keys(picks).filter(function (k) { return picks[k]; });
        if (!ids.length) return ctx.toast('Choose what to delete first.', '');
        var lose = items.filter(function (i) { return picks[i.id] && (i.uncommitted || i.kind === 'kept-branch'); }).length;
        if (!window.confirm('Delete ' + ids.length + ' item' + (ids.length === 1 ? '' : 's') + '?' + (lose ? ' ' + lose + ' of them hold work that will be lost.' : ''))) return;
        rm.disabled = true;
        var r = await post(ctx, '/api/factory-cleanup', { action: 'remove', ids: ids });
        ctx.toast(r.ok ? 'Deleted.' : (r.error || 'Some could not be deleted.'), r.ok ? 'success' : 'error');
        cleanupView();
      };
      body.appendChild(rm);
    }

    return {
      show: function () { visible = true; if (!sub) show(current); },
      hide: function () { visible = false; },
      openRun: function (id) { visible = true; current = 'runs'; Object.keys(segBtns).forEach(function (k) { segBtns[k].classList.toggle('on', k === current); }); openSub('viewer', id); },
      refresh: function () { if (!sub) show(current); },
    };
  }

  window.CrundiFactories = { mountTab: mountTab, mountPanel: mountPanel, runEvent: runEvent, startDialog: startDialog, openChat: openChat };
})();
