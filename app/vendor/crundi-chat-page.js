/**
 * crundi-chat-page.js — the compact chat at /chat/<id> (src/chat-page.js).
 *
 * One conversation, full height, with a small header: back, the project's
 * mark, the chat's title, the project under it, a menu. It is what a chat
 * bubble on a phone shows.
 *
 * The conversation itself is not drawn here. window.CrundiChat (claude-chat.js)
 * does that, exactly as it does in the workbench; this file is the host it
 * expects: the signed-in fetch, the socket, and the few things round the edge.
 *
 * Inside the phone app there is a bridge, window.CrundiNative. Everything it
 * offers is optional and asked for by name, so this page is the same page in
 * an ordinary browser, where the bridge is simply not there:
 *
 *   CrundiNative.session()        -> '{"token":"…","refreshToken":"…"}' the app's page login
 *   CrundiNative.saveSession(json)   the page renewed it; keep the new one
 *   CrundiNative.close()             the back arrow: put the bubble away
 *   CrundiNative.openMain(path)      "Open in Crundi": show this in the full app
 *   CrundiNative.chatState(json)     title, project, state: for the bubble's own label
 *   CrundiNative.download(url, name) save a file of the server's to the phone
 *   CrundiNative.seen(chatId)        this chat is on screen: its notification can go
 */
(function () {
  'use strict';

  var VERSION = document.body.getAttribute('data-version') || '';
  var sid = '';
  try { sid = decodeURIComponent(location.pathname.replace(/^\/chat\//, '').replace(/\/+$/, '')); } catch (e) { sid = ''; }
  var native = window.CrundiNative || null;
  var $ = function (id) { return document.getElementById(id); };
  var bridge = function (name) {
    if (!native || typeof native[name] !== 'function') return undefined;
    try { return native[name].apply(native, Array.prototype.slice.call(arguments, 1)); } catch (e) { return undefined; }
  };

  // ─── Sign-in ───
  // The same two tokens the main page keeps, in the same place, so a browser
  // that is signed in to Crundi is signed in here. In the app they come from
  // the bridge, which holds them outside the web view.
  var token = '', refreshToken = '';
  function readSession() {
    var fromApp = bridge('session');
    if (fromApp) {
      try { var s = JSON.parse(fromApp); token = s.token || ''; refreshToken = s.refreshToken || ''; return; } catch (e) { /* fall through */ }
    }
    try { token = localStorage.getItem('crundi_token') || ''; refreshToken = localStorage.getItem('crundi_refresh') || ''; } catch (e) { /* no storage */ }
  }
  function storeSession(d) {
    if (!d || !d.token) return false;
    token = d.token;
    if (d.refreshToken) refreshToken = d.refreshToken;
    try { localStorage.setItem('crundi_token', token); if (d.refreshToken) localStorage.setItem('crundi_refresh', refreshToken); } catch (e) { /* ignore */ }
    bridge('saveSession', JSON.stringify({ token: token, refreshToken: refreshToken }));
    return true;
  }
  var refreshing = null;
  function refreshSession() {
    if (refreshing) return refreshing;
    // Another tab, or the main page, may have renewed it since this page read
    // it. Sending the spent one would look like a stolen token to the server.
    try { var stored = localStorage.getItem('crundi_refresh'); if (stored && stored !== refreshToken) refreshToken = stored; } catch (e) { /* ignore */ }
    if (!refreshToken) return Promise.resolve(false);
    refreshing = fetch('/api/auth/refresh', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ refreshToken: refreshToken }) })
      .then(function (r) { return r.json(); })
      .then(function (d) { return d && d.ok ? storeSession(d) : false; })
      .catch(function () { return false; })
      .then(function (ok) { refreshing = null; return ok; });
    return refreshing;
  }

  var stopped = false;       // a state nothing on this page can recover from
  function apiFetch(path, opts, retried) {
    opts = opts || {};
    opts.headers = opts.headers || {};
    if (token) opts.headers.Authorization = 'Bearer ' + token;
    if (VERSION) opts.headers['X-Crundi-Web'] = VERSION;
    return fetch(path, opts).then(function (res) {
      if (res.status === 426) { res.clone().json().then(onRefused).catch(function () {}); return res; }
      if (res.status !== 401 || retried || path.indexOf('/api/auth/') === 0) return res;
      return refreshSession().then(function (ok) {
        if (ok) return apiFetch(path, opts, true);
        note('Signed out', 'Sign in to Crundi to see this chat.', 'Sign in', function () { openMain('/'); });
        return res;
      });
    });
  }
  // fetch cannot report how much of an upload has gone; attachments use XHR.
  function apiUpload(path, payload, onProgress) {
    return new Promise(function (resolve, reject) {
      var xhr = new XMLHttpRequest();
      xhr.open('POST', path);
      xhr.setRequestHeader('Content-Type', 'application/json');
      if (token) xhr.setRequestHeader('Authorization', 'Bearer ' + token);
      if (VERSION) xhr.setRequestHeader('X-Crundi-Web', VERSION);
      xhr.upload.onprogress = function (ev) { if (ev.lengthComputable && onProgress) onProgress(ev.loaded / ev.total); };
      xhr.upload.onload = function () { if (onProgress) onProgress(1); };
      xhr.onload = function () {
        try { resolve(JSON.parse(xhr.responseText)); } catch (e) {
          // Not JSON, so something between here and the server answered instead.
          var said = String(xhr.responseText || '').replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 80);
          reject(new Error('The upload was answered with HTTP ' + xhr.status + (said ? ' (' + said + ')' : ', and nothing else') + ', not by Crundi. Something between this device and the server may have blocked it.'));
        }
      };
      xhr.onerror = function () { reject(new Error('Network error')); };
      xhr.onabort = function () { reject(new Error('Upload cancelled')); };
      xhr.send(JSON.stringify(payload));
    });
  }

  // ─── What Claude shows in the chat ───
  // Pictures, video, sound, files to download and live frames are drawn by
  // the widgets file, the same one the workbench uses. It needs a host for
  // three things a page has to do for it.
  function mediaUrl(u) { return u + (u.indexOf('?') < 0 ? '?' : '&') + 'token=' + encodeURIComponent(token || ''); }
  function download(o) {
    var abs = new URL(mediaUrl(o.url), location.href).href;
    // In the app, the phone's own downloader saves it to Downloads. In a
    // browser, an ordinary download link does.
    if (bridge('download', abs, String(o.name || 'download')) !== undefined) { toast('Downloading ' + (o.name || 'file')); return; }
    var a = document.createElement('a');
    a.href = abs; a.download = String(o.name || 'download'); a.rel = 'noopener';
    document.body.appendChild(a); a.click(); a.remove();
  }
  function toolExtra(entry) {
    if (!window.CrundiWidgets || !window.CrundiWidgets.toolMedia) return null;
    try { return window.CrundiWidgets.toolMedia(sid, entry); } catch (e) { return null; }
  }

  // ─── The edges ───
  var toastTimer = 0;
  function toast(msg, type) {
    var el = $('cp-toast');
    el.textContent = String(msg || '');
    el.className = 'on' + (type === 'error' ? ' error' : '');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () { el.className = ''; }, 3200);
  }
  /** Cover the chat with a plain statement, and at most one thing to do about it. */
  function note(title, text, action, onAction) {
    stopped = true;
    var el = $('cp-note');
    el.textContent = '';
    var b = document.createElement('b'); b.textContent = title; el.appendChild(b);
    var p = document.createElement('div'); p.textContent = text; el.appendChild(p);
    if (action) { var btn = document.createElement('button'); btn.type = 'button'; btn.textContent = action; btn.onclick = onAction; el.appendChild(btn); }
    el.className = 'on';
    $('cp-conn').className = '';
  }
  function onRefused(body) {
    if (!body || body.error !== 'client_too_old') return;
    var kind = (body.client && body.client.kind) || '';
    if (kind === 'web') {
      // The page is behind the server: loading it again is the update. Once.
      var tried = true;
      try { tried = sessionStorage.getItem('crundi_gate_reload') === String(body.minimum); sessionStorage.setItem('crundi_gate_reload', String(body.minimum)); } catch (e) { /* do not risk a loop */ }
      if (!tried) { location.reload(); return; }
    }
    note('Update required', (kind === 'web' ? 'This page' : 'This app') + ' is older than this server accepts. Version ' + body.minimum + ' or newer is needed.');
  }
  function openMain(path) {
    if (bridge('openMain', path) !== undefined) return;
    location.href = path;
  }

  // ─── Header ───
  var meta = { title: '', project: '', state: '' };
  var STATE_WORDS = { working: 'Working', 'needs-input': 'Needs you', waiting: 'Waiting', idle: 'Ready' };
  function paintHead() {
    $('cp-title').textContent = meta.title || 'Chat';
    document.title = (meta.title || 'Chat') + ' · Crundi';
    var word = STATE_WORDS[meta.state] || '';
    $('cp-proj').textContent = meta.project + (word && meta.project ? ' · ' : '') + word;
    $('cp-dot').className = meta.state || '';
    var m = meta.mark;
    if (m) {
      $('cp-mark').textContent = m.initials;
      $('cp-mark').style.background = 'linear-gradient(135deg,' + m.color + ',' + m.colorDeep + ')';
    }
    bridge('chatState', JSON.stringify({ id: sid, title: meta.title, project: meta.project, state: meta.state, mark: meta.mark || null }));
  }
  function loadMeta() {
    return apiFetch('/api/ui-sessions').then(function (r) { return r.ok ? r.json() : null; }).then(function (d) {
      if (!d) return false;
      var s = (d.sessions || []).filter(function (x) { return x.id === sid; })[0];
      if (!s) { note('Chat closed', 'This chat is no longer open on the server.', 'Open Crundi', function () { openMain('/'); }); return false; }
      meta.title = s.longTitle || s.title || 'Chat';
      meta.project = s.project || '';
      meta.state = s.status === 'exited' ? '' : (s.agentState || '');
      meta.mark = s.mark || null;
      paintHead();
      return true;
    }).catch(function () { return false; });
  }

  // ─── Socket ───
  var ws = null, view = null, retry = 0, retryTimer = 0, everOpen = false;
  function wsSend(obj) { if (ws && ws.readyState === 1) ws.send(JSON.stringify(obj)); }
  function present() {
    var active = document.visibilityState === 'visible';
    // This chat is being looked at: its notification can come down.
    if (active && view) bridge('seen', sid);
    wsSend({ type: 'presence', active: active, engaged: active, project: meta.project || '' });
  }
  function connect() {
    clearTimeout(retryTimer);
    if (stopped) return;
    if (ws) { ws.onopen = ws.onmessage = ws.onerror = ws.onclose = null; try { ws.close(); } catch (e) { /* ignore */ } }
    var url = (location.protocol === 'https:' ? 'wss:' : 'ws:') + '//' + location.host + '/ws?token=' + encodeURIComponent(token) + (VERSION ? '&web=' + encodeURIComponent(VERSION) : '');
    ws = new WebSocket(url);
    ws.onopen = function () {
      everOpen = true; retry = 0;
      $('cp-conn').className = '';
      // The server replays the conversation on subscribe, which is also how a
      // dropped connection catches up on what it missed.
      if (view) view.resubscribe();
      present();
    };
    ws.onmessage = function (e) {
      var msg; try { msg = JSON.parse(e.data); } catch (err) { return; }
      if (!view || msg.id !== sid) return;
      if (msg.type === 'ui-history') {
        view.applyHistory(msg.session);
        if (msg.session) { meta.state = msg.session.status === 'exited' ? '' : (msg.session.state || ''); if (msg.session.title && !meta.title) meta.title = msg.session.title; paintHead(); }
      }
      if (msg.type === 'ui-event') {
        view.applyEvent(msg.event);
        var ev = msg.event || {};
        if (ev.type === 'state' && typeof ev.state === 'string') { meta.state = ev.state; paintHead(); }
        if (ev.type === 'exit') { meta.state = ''; paintHead(); }
        // A new title arrives as a change to the session's details; the list
        // has the whole of it, long form included.
        if (ev.type === 'meta') loadMeta();
      }
    };
    ws.onclose = function () {
      if (stopped) return;
      $('cp-conn').className = 'on';
      retry++;
      // A socket is checked at connect only, and an access token lasts fifteen
      // minutes: a refusal here is usually just a token that has aged out.
      var again = function () { retryTimer = setTimeout(connect, Math.min(15000, 600 * Math.pow(1.7, retry))); };
      if (!everOpen || retry % 3 === 0) refreshSession().then(again); else again();
    };
    ws.onerror = function () { /* onclose follows and does the work */ };
  }
  document.addEventListener('visibilitychange', function () {
    present();
    if (document.visibilityState === 'visible' && (!ws || ws.readyState > 1)) { retry = 0; connect(); }
  });
  setInterval(present, 60000);

  // ─── Buttons ───
  $('cp-back').onclick = function () {
    if (bridge('close') !== undefined) return;
    if (history.length > 1) history.back(); else location.href = '/';
  };
  var menu = $('cp-menu');
  $('cp-more').onclick = function (e) { e.stopPropagation(); menu.className = menu.className === 'on' ? '' : 'on'; menu.querySelector('[data-do="stop"]').disabled = meta.state !== 'working'; };
  document.addEventListener('click', function () { menu.className = ''; });
  menu.onclick = function (e) {
    var act = e.target && e.target.getAttribute && e.target.getAttribute('data-do');
    if (act === 'open') openMain('/?chat=' + encodeURIComponent(sid));
    if (act === 'stop') apiFetch('/api/ui-sessions/' + encodeURIComponent(sid) + '/interrupt', { method: 'POST' }).then(function (r) { if (!r.ok) toast('Could not stop it', 'error'); }).catch(function () { toast('Could not stop it', 'error'); });
  };
  // ─── Start ───
  function start() {
    if (!sid || !/^[A-Za-z0-9_-]{1,80}$/.test(sid)) { note('Not a chat', 'This address does not name a chat.', 'Open Crundi', function () { openMain('/'); }); return; }
    if (!window.CrundiChat) { note('Could not load', 'The chat could not be loaded. Check the connection and try again.', 'Try again', function () { location.reload(); }); return; }
    readSession();
    if (!token && !refreshToken) { note('Signed out', 'Sign in to Crundi to see this chat.', 'Sign in', function () { openMain('/'); }); return; }
    (token ? Promise.resolve(true) : refreshSession()).then(function () { return loadMeta(); }).then(function (found) {
      if (!found || stopped) return;
      if (window.CrundiWidgets) {
        window.CrundiWidgets.configure({
          apiFetch: apiFetch, toast: toast, isMobile: function () { return true; },
          mediaUrl: mediaUrl, refreshAuth: refreshSession, download: download,
        });
      }
      view = window.CrundiChat.mount($('cp-chat'), {
        sessionId: sid,
        project: meta.project,
        apiFetch: apiFetch,
        apiUpload: apiUpload,
        toast: toast,
        wsSend: wsSend,
        toolExtra: toolExtra,
        onChatMeta: function (patch) { if (patch && typeof patch.title === 'string' && patch.title) { meta.title = patch.title; paintHead(); } },
      });
      connect();
    });
  }
  start();
})();
