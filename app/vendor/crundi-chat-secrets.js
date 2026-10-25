/**
 * crundi-chat-secrets.js — secrets pre-authorised for one chat, in the browser.
 *
 * Shared by the workbench (src/webapp-html.js) and the compact chat page
 * (/chat/<id>, crundi-chat-page.js), so both offer the same thing:
 *
 *   - a dialog (open(sid)): what is pre-authorised for this chat (names and
 *     when each runs out, never a value or a PIN), revoke one or all, and
 *     pre-authorise more: pick secrets, a duration, type each PIN once;
 *   - the "Allow for this chat" choice under a PIN prompt (allowRow()),
 *     whose answer goes with the approval as allowForChat.
 *
 * The server keeps the PINs in memory only (src/chat-secrets.js). A
 * pre-authorised secret is used by that chat's secret_run without asking;
 * secret_get still asks every time. Offered only when the server lists the
 * capability 'chat-secret-preauth' (supported()); an older server has no
 * route for it, and a call that meets one says so in plain words.
 *
 * No prompt/confirm/alert: a web view inside the phone app shows none.
 */
(function () {
  'use strict';

  var CAP = 'chat-secret-preauth';
  var PRESETS = [
    { key: '15m', label: '15 minutes' },
    { key: '1h', label: '1 hour' },
    { key: '8h', label: '8 hours' },
    { key: 'chat', label: 'Until this chat ends' },
  ];
  var LOCK = '<svg class="ccs-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>';
  var UNSUPPORTED = 'This server does not support this yet. Update the server to use it.';

  var host = { apiFetch: null, toast: function () {} };
  var cap = null;            // null unknown, then true/false
  var capAsk = null;
  var byChat = {};           // sessionId -> grants
  var listeners = [];

  function configure(o) {
    o = o || {};
    if (o.apiFetch) host.apiFetch = o.apiFetch;
    if (o.toast) host.toast = o.toast;
    injectCss();
  }

  function api(path, opts) {
    if (!host.apiFetch) return Promise.reject(new Error('Not ready'));
    return host.apiFetch(path, opts).then(function (r) {
      if (r.status === 404) return { ok: false, error: UNSUPPORTED, unsupported: true };
      return r.json().catch(function () { return { ok: false, error: 'The server answered with HTTP ' + r.status + '.' }; });
    });
  }
  function post(body) {
    return api('/api/secrets/chat', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
      .catch(function (e) { return { ok: false, error: e.message }; });
  }

  /** Whether the server offers it (asked once, from the handshake). */
  function supported() {
    if (cap !== null) return Promise.resolve(cap);
    if (capAsk) return capAsk;
    capAsk = (host.apiFetch ? host.apiFetch('/api/handshake').then(function (r) { return r.json(); }) : Promise.reject(new Error('Not ready')))
      .then(function (hs) {
        var caps = (hs && hs.server && hs.server.capabilities) || [];
        cap = caps.indexOf(CAP) >= 0;
        if (cap) load();
        notify('');
        return cap;
      })
      .catch(function () { capAsk = null; return false; });
    return capAsk;
  }
  function isSupported() { return cap === true; }

  function setGrants(sid, list) {
    byChat[sid] = Array.isArray(list) ? list : [];
    if (!byChat[sid].length) delete byChat[sid];
    notify(sid);
  }
  function grants(sid) {
    var now = Date.now();
    return (byChat[sid] || []).filter(function (g) { return !g.expiresAt || Date.parse(g.expiresAt) > now; });
  }
  /** Everything known (no sid), or one chat's list, from the server. */
  function load(sid) {
    if (cap !== true) return Promise.resolve(null);
    var q = sid ? '?sessionId=' + encodeURIComponent(sid) : '';
    return api('/api/secrets/chat' + q).then(function (d) {
      if (!d || !d.ok) return d;
      if (sid) setGrants(sid, d.grants);
      else if (d.chats) { byChat = {}; Object.keys(d.chats).forEach(function (k) { byChat[k] = d.chats[k]; }); notify(''); }
      return d;
    }).catch(function () { return null; });
  }
  /** The server's 'chat-secrets' event: { sessionId, grants }. */
  function handleEvent(d) {
    if (!d || !d.sessionId) return;
    setGrants(String(d.sessionId), d.grants);
  }
  function onChange(cb) { if (typeof cb === 'function') listeners.push(cb); }
  function notify(sid) { listeners.forEach(function (cb) { try { cb(sid); } catch (e) { /* one listener must not stop the rest */ } }); if (dlg && (!sid || sid === dlg.sid)) drawList(); }
  // Timed grants run out on the server, which says so; this also redraws the
  // times shown ("in 12 min") and drops anything the clock has passed.
  setInterval(function () { if (Object.keys(byChat).length) notify(''); }, 30000);

  function describe(g) {
    if (!g || !g.expiresAt) return 'until this chat ends';
    var t = Date.parse(g.expiresAt), left = Math.max(0, t - Date.now());
    var mins = Math.round(left / 60000);
    var when = new Date(t);
    var hh = String(when.getHours()).padStart(2, '0') + ':' + String(when.getMinutes()).padStart(2, '0');
    var inWords = mins < 1 ? 'less than a minute' : mins < 60 ? mins + ' min' : Math.floor(mins / 60) + ' h' + (mins % 60 ? ' ' + (mins % 60) + ' min' : '');
    return 'until ' + hh + ' (' + inWords + ' left)';
  }

  // ─── "Allow for this chat", under a PIN prompt ───
  /**
   * state: { on: bool, duration: key }, kept by the caller so a redraw keeps it.
   * Returns the element. The approval then sends allowForChat(state).
   */
  function allowRow(state, onChange) {
    var row = document.createElement('div'); row.className = 'ccs-allow';
    var lab = document.createElement('label');
    var box = document.createElement('input'); box.type = 'checkbox'; box.checked = !!state.on;
    lab.appendChild(box);
    var txt = document.createElement('span'); txt.textContent = 'Allow for this chat'; lab.appendChild(txt);
    row.appendChild(lab);
    var sel = document.createElement('select'); sel.setAttribute('aria-label', 'For how long');
    PRESETS.forEach(function (p) { var o = document.createElement('option'); o.value = p.key; o.textContent = p.label; sel.appendChild(o); });
    sel.value = state.duration || '1h';
    sel.disabled = !state.on;
    row.appendChild(sel);
    var hint = document.createElement('div'); hint.className = 'ccs-hint';
    hint.textContent = 'This chat may then run commands with this secret without asking, until then. Its value is never shown.';
    hint.style.display = state.on ? '' : 'none';
    row.appendChild(hint);
    box.onchange = function () { state.on = box.checked; sel.disabled = !box.checked; hint.style.display = box.checked ? '' : 'none'; if (onChange) onChange(state); };
    sel.onchange = function () { state.duration = sel.value; if (onChange) onChange(state); };
    return row;
  }
  function allowForChat(state) { return state && state.on ? { duration: state.duration || '1h' } : null; }

  // ─── The dialog ───
  var dlg = null;
  function close() {
    if (!dlg) return;
    // The PINs typed are in these inputs only: cleared with them.
    dlg.root.querySelectorAll('input').forEach(function (i) { i.value = ''; });
    dlg.root.remove();
    document.removeEventListener('keydown', dlg.onKey);
    dlg = null;
  }
  function el(tag, cls, text) { var n = document.createElement(tag); if (cls) n.className = cls; if (text != null) n.textContent = text; return n; }

  function drawList() {
    if (!dlg) return;
    var box = dlg.list; box.textContent = '';
    var list = grants(dlg.sid);
    if (!list.length) { box.appendChild(el('div', 'ccs-empty', 'Nothing is pre-authorised for this chat.')); return; }
    list.forEach(function (g) {
      var row = el('div', 'ccs-grant');
      var info = el('div', 'ccs-grant-info');
      info.appendChild(el('div', 'ccs-name', g.name || 'A secret'));
      info.appendChild(el('div', 'ccs-when', describe(g)));
      row.appendChild(info);
      var b = el('button', 'ccs-btn', 'Revoke'); b.type = 'button';
      b.onclick = function () { b.disabled = true; post({ action: 'revoke', sessionId: dlg.sid, secretId: g.secretId }).then(function (d) { if (d && d.ok) { setGrants(dlg.sid, d.grants); host.toast('Revoked ' + (g.name || 'the secret')); } else { b.disabled = false; host.toast((d && d.error) || 'Could not revoke it', 'error'); } }); };
      row.appendChild(b);
      box.appendChild(row);
    });
    if (list.length > 1) {
      var all = el('button', 'ccs-btn ccs-danger', 'Revoke all'); all.type = 'button';
      all.onclick = function () { all.disabled = true; post({ action: 'revokeAll', sessionId: dlg.sid }).then(function (d) { if (d && d.ok) { setGrants(dlg.sid, d.grants); host.toast('Revoked all for this chat'); } else { all.disabled = false; host.toast((d && d.error) || 'Could not revoke them', 'error'); } }); };
      var w = el('div', 'ccs-right'); w.appendChild(all); box.appendChild(w);
    }
  }

  function drawPick() {
    var box = dlg.pick; box.textContent = '';
    if (dlg.secrets === null) { box.appendChild(el('div', 'ccs-empty', 'Loading the secrets…')); return; }
    if (!dlg.secrets.length) { box.appendChild(el('div', 'ccs-empty', 'There are no secrets yet. Add them under Secrets.')); return; }
    dlg.secrets.forEach(function (s) {
      var row = el('div', 'ccs-pick-row');
      var lab = el('label', 'ccs-pick');
      var cb = document.createElement('input'); cb.type = 'checkbox'; cb.checked = !!dlg.chosen[s.id];
      lab.appendChild(cb); lab.appendChild(el('span', 'ccs-name', s.name));
      row.appendChild(lab);
      if (dlg.chosen[s.id]) {
        var pin = document.createElement('input');
        pin.className = 'ccs-pin'; pin.setAttribute('inputmode', 'numeric'); pin.setAttribute('maxlength', '6'); pin.setAttribute('autocomplete', 'off');
        pin.type = 'password'; pin.placeholder = 'PIN'; pin.setAttribute('aria-label', 'PIN for ' + s.name); pin.setAttribute('data-sid', s.id);
        pin.value = dlg.pins[s.id] || '';
        pin.oninput = function () { pin.value = pin.value.replace(/[^0-9]/g, '').slice(0, 6); dlg.pins[s.id] = pin.value; };
        pin.onkeydown = function (e) { if (e.key === 'Enter') { e.preventDefault(); submit(); } };
        row.appendChild(pin);
        if (dlg.bad === s.id) row.classList.add('ccs-bad');
      }
      cb.onchange = function () {
        if (cb.checked) dlg.chosen[s.id] = true; else { delete dlg.chosen[s.id]; delete dlg.pins[s.id]; }
        drawPick();
        if (cb.checked) { var mine = dlg.pick.querySelector('[data-sid="' + s.id + '"]'); if (mine) mine.focus(); }
      };
      box.appendChild(row);
    });
  }

  function submit() {
    if (!dlg || dlg.busy) return;
    var ids = Object.keys(dlg.chosen);
    dlg.err.textContent = '';
    if (!ids.length) { dlg.err.textContent = 'Choose at least one secret.'; return; }
    for (var i = 0; i < ids.length; i++) if (!/^[0-9]{6}$/.test(dlg.pins[ids[i]] || '')) { dlg.bad = ids[i]; drawPick(); dlg.err.textContent = 'Enter the 6-digit PIN of each secret chosen.'; return; }
    dlg.busy = true; dlg.go.disabled = true; dlg.go.textContent = 'Checking…';
    var body = { action: 'grant', sessionId: dlg.sid, duration: dlg.duration, secrets: ids.map(function (id) { return { id: id, pin: dlg.pins[id] }; }) };
    post(body).then(function (d) {
      body.secrets.forEach(function (s) { s.pin = ''; });
      if (!dlg) return;
      dlg.busy = false; dlg.go.disabled = false; dlg.go.textContent = 'Pre-authorise';
      if (!d || !d.ok) {
        dlg.bad = (d && d.secretId) || '';
        if (dlg.bad) dlg.pins[dlg.bad] = '';
        var who = dlg.bad ? (dlg.secrets.filter(function (s) { return s.id === dlg.bad; })[0] || {}).name : '';
        dlg.err.textContent = (who ? who + ': ' : '') + ((d && d.error) || 'Could not pre-authorise them.');
        drawPick();
        return;
      }
      setGrants(dlg.sid, d.grants);
      host.toast('Pre-authorised for this chat');
      dlg.chosen = {}; dlg.pins = {}; dlg.bad = '';
      drawPick();
    });
  }

  /** The dialog for one chat. opts.preselect: a secret id to tick. */
  function open(sid, opts) {
    opts = opts || {};
    close();
    injectCss();
    var root = el('div', 'ccs-scrim');
    root.setAttribute('role', 'dialog'); root.setAttribute('aria-modal', 'true'); root.setAttribute('aria-label', 'Secrets for this chat');
    var card = el('div', 'ccs-card');
    var head = el('div', 'ccs-head');
    var ic = el('span', 'ccs-lock'); ic.innerHTML = LOCK; head.appendChild(ic);
    head.appendChild(el('div', 'ccs-title', 'Secrets for this chat'));
    var x = el('button', 'ccs-x', '×'); x.type = 'button'; x.setAttribute('aria-label', 'Close'); x.onclick = close; head.appendChild(x);
    card.appendChild(head);
    card.appendChild(el('div', 'ccs-hint', 'A pre-authorised secret lets this chat run commands with it without asking for the PIN. Only to run: its value is never shown, and Claude reading a value still asks you every time. The PINs are kept in the server’s memory only, for this chat only.'));
    card.appendChild(el('div', 'ccs-sec', 'Pre-authorised now'));
    var list = el('div', 'ccs-list'); card.appendChild(list);
    card.appendChild(el('div', 'ccs-sec', 'Pre-authorise'));
    var pick = el('div', 'ccs-pickbox'); card.appendChild(pick);
    var durRow = el('div', 'ccs-durs');
    PRESETS.forEach(function (p) {
      var b = el('button', 'ccs-chip', p.label); b.type = 'button'; b.dataset.key = p.key;
      b.onclick = function () { dlg.duration = p.key; durRow.querySelectorAll('.ccs-chip').forEach(function (c) { c.classList.toggle('on', c.dataset.key === p.key); }); };
      durRow.appendChild(b);
    });
    card.appendChild(durRow);
    var err = el('div', 'ccs-err'); card.appendChild(err);
    var foot = el('div', 'ccs-right');
    var cancel = el('button', 'ccs-btn', 'Close'); cancel.type = 'button'; cancel.onclick = close; foot.appendChild(cancel);
    var go = el('button', 'ccs-btn ccs-go', 'Pre-authorise'); go.type = 'button'; go.onclick = submit; foot.appendChild(go);
    card.appendChild(foot);
    root.appendChild(card);
    root.addEventListener('click', function (e) { if (e.target === root) close(); });
    document.body.appendChild(root);
    dlg = { sid: sid, root: root, list: list, pick: pick, err: err, go: go, secrets: null, chosen: {}, pins: {}, bad: '', duration: '1h', busy: false };
    dlg.onKey = function (e) { if (e.key === 'Escape') close(); };
    document.addEventListener('keydown', dlg.onKey);
    durRow.querySelector('[data-key="1h"]').classList.add('on');
    if (opts.preselect) dlg.chosen[opts.preselect] = true;
    drawList();
    drawPick();
    supported().then(function (yes) {
      if (!dlg || dlg.sid !== sid) return;
      if (!yes) { err.textContent = UNSUPPORTED; go.disabled = true; return; }
      load(sid).then(function (d) { if (d && !d.ok && dlg) err.textContent = d.error || ''; });
      api('/api/secrets').then(function (d) {
        if (!dlg || dlg.sid !== sid) return;
        dlg.secrets = d && d.ok ? (d.secrets || []).map(function (s) { return { id: s.id, name: s.name }; }) : [];
        drawPick();
      }).catch(function () { if (dlg) { dlg.secrets = []; drawPick(); } });
    });
  }

  var cssDone = false;
  function injectCss() {
    if (cssDone || !document.head) return;
    cssDone = true;
    var s = document.createElement('style');
    s.textContent = [
      '.ccs-scrim{position:fixed;inset:0;z-index:600;background:rgba(0,0,0,0.55);display:flex;align-items:center;justify-content:center;padding:16px}',
      '.ccs-card{width:100%;max-width:460px;max-height:calc(100vh - 32px);overflow:auto;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius,10px);box-shadow:var(--shadow-lg,0 10px 30px rgba(0,0,0,.4));padding:16px 18px;display:flex;flex-direction:column;gap:10px;color:var(--text-primary)}',
      '.ccs-head{display:flex;align-items:center;gap:8px}',
      '.ccs-lock{display:inline-flex;color:var(--accent-hover,var(--accent))}',
      '.ccs-ic{width:16px;height:16px;display:block;pointer-events:none}',
      '.ccs-title{flex:1;font-weight:650;font-size:15px}',
      '.ccs-x{border:0;background:transparent;color:var(--text-secondary);font-size:20px;line-height:1;cursor:pointer;padding:2px 6px}',
      '.ccs-hint{font-size:12.5px;color:var(--text-secondary);line-height:1.45}',
      '.ccs-sec{font-size:11.5px;font-weight:650;text-transform:uppercase;letter-spacing:.04em;color:var(--text-secondary);margin-top:4px}',
      '.ccs-list,.ccs-pickbox{display:flex;flex-direction:column;gap:6px}',
      '.ccs-pickbox{max-height:240px;overflow:auto}',
      '.ccs-empty{font-size:13px;color:var(--text-muted)}',
      '.ccs-grant{display:flex;align-items:center;gap:10px;padding:8px 10px;border:1px solid var(--border);border-radius:var(--radius-sm,6px);background:var(--bg-primary)}',
      '.ccs-grant-info{flex:1;min-width:0}',
      '.ccs-name{font-family:var(--mono,monospace);font-size:13px;word-break:break-word}',
      '.ccs-when{font-size:12px;color:var(--text-secondary)}',
      '.ccs-pick-row{display:flex;align-items:center;gap:8px;padding:4px 2px;border-radius:var(--radius-sm,6px)}',
      '.ccs-pick-row.ccs-bad .ccs-pin{border-color:var(--red)}',
      '.ccs-pick{flex:1;display:flex;align-items:center;gap:8px;cursor:pointer;min-width:0}',
      '.ccs-pin{width:110px;padding:7px 9px;border-radius:var(--radius-sm,6px);border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font-family:var(--mono,monospace);font-size:15px;letter-spacing:.25em;text-align:center}',
      '.ccs-pin:focus{outline:none;border-color:var(--accent)}',
      '.ccs-durs{display:flex;flex-wrap:wrap;gap:6px}',
      '.ccs-chip{padding:6px 11px;border-radius:99px;border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:12.5px;cursor:pointer}',
      '.ccs-chip.on{background:var(--accent-dim,var(--bg-hover));border-color:var(--accent);color:var(--accent-hover,var(--text-primary))}',
      '.ccs-err{font-size:12.5px;color:var(--red);min-height:0}',
      '.ccs-err:empty{display:none}',
      '.ccs-right{display:flex;justify-content:flex-end;gap:8px;margin-top:2px}',
      '.ccs-btn{padding:7px 14px;border-radius:var(--radius-sm,6px);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:13px;cursor:pointer}',
      '.ccs-btn:disabled{opacity:.6;cursor:default}',
      '.ccs-go{background:var(--accent);border-color:var(--accent);color:#fff}',
      '.ccs-danger{color:var(--red)}',
      '.ccs-allow{display:flex;flex-wrap:wrap;align-items:center;gap:8px;margin-top:6px;font-size:13px}',
      '.ccs-allow label{display:inline-flex;align-items:center;gap:6px;cursor:pointer}',
      '.ccs-allow select{padding:4px 6px;border-radius:var(--radius-sm,6px);border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font:inherit;font-size:12.5px}',
      '.ccs-allow .ccs-hint{flex-basis:100%}',
    ].join('');
    document.head.appendChild(s);
  }

  window.CrundiChatSecrets = {
    CAPABILITY: CAP, PRESETS: PRESETS, LOCK_SVG: LOCK,
    configure: configure, supported: supported, isSupported: isSupported,
    load: load, grants: grants, handleEvent: handleEvent, onChange: onChange,
    open: open, close: close, describe: describe,
    allowRow: allowRow, allowForChat: allowForChat,
  };
})();
