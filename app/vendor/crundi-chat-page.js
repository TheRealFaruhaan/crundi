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
 *
 * And, with a server that offers it, window.CrundiLogin: a message channel
 * only this server's own frames can see, over which the app hands out the
 * page login it keeps and renews for every page (PageLogin in the app). The
 * page then holds the access token only, in memory, and never a refresh token.
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
    // "undefined" has to mean one thing only: the app has no such call, so
    // the page must do it itself. Most of the app's calls return nothing,
    // which is undefined too, and read that way a call that WORKED was done
    // a second time by the page: a file saved twice, once by each.
    try { var r = native[name].apply(native, Array.prototype.slice.call(arguments, 1)); return r === undefined ? true : r; } catch (e) { return undefined; }
  };

  // ─── Sign-in ───
  // The same two tokens the main page keeps, in the same place, so a browser
  // that is signed in to Crundi is signed in here. In the app they come from
  // the bridge, which holds them outside the web view.
  var token = '', refreshToken = '';
  // See window.CrundiLogin above. The same rule as the main page.
  // Embedded in Crundi's own page (the live graph): use its sign-in, which
  // works wherever that page does, the phone app included.
  var FRAME_LOGIN = (function () { try { return window.top !== window.self && window.top.location.origin === location.origin && window.top.crundiFrameSession ? window.top.crundiFrameSession : null; } catch (e) { return null; } })();
  var APP_LOGIN = !FRAME_LOGIN && !!window.CrundiLogin;
  var appSeq = 0, appWait = {};
  if (APP_LOGIN) {
    window.CrundiLogin.onmessage = function (e) {
      var m = null;
      try { m = JSON.parse(e.data); } catch (err) { return; }
      var done = m && appWait[m.id];
      if (!done) return;
      delete appWait[m.id];
      done(m);
    };
  }
  function askAppLogin(op) {
    return new Promise(function (resolve) {
      var id = 'l' + (++appSeq);
      var timer = setTimeout(function () { delete appWait[id]; resolve(null); }, 30000);
      appWait[id] = function (m) { clearTimeout(timer); resolve(m); };
      try { window.CrundiLogin.postMessage(JSON.stringify({ id: id, op: op, stale: token || '' })); }
      catch (err) { clearTimeout(timer); delete appWait[id]; resolve(null); }
    });
  }
  function takeAppLogin(m) {
    if (!m || !m.token) return false;
    token = m.token;
    return true;
  }
  function readSession() {
    if (FRAME_LOGIN) { token = FRAME_LOGIN.token(); return; }
    if (APP_LOGIN) return;   // asked for at start, over the channel
    var fromApp = bridge('session');
    if (typeof fromApp === 'string' && fromApp) {
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
    if (FRAME_LOGIN) {
      refreshing = Promise.resolve(FRAME_LOGIN.renew()).then(function (ok) { token = FRAME_LOGIN.token(); refreshing = null; return !!ok && !!token; }, function () { refreshing = null; return false; });
      return refreshing;
    }
    if (APP_LOGIN) {
      refreshing = askAppLogin('renew').then(takeAppLogin).then(function (ok) { refreshing = null; return ok; });
      return refreshing;
    }
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

  /**
   * Before connecting again. Inside the app, check the token rather than
   * renew it: a dropped connection is usually the network changing hands,
   * and apiFetch renews by itself if the check is refused. Elsewhere, renew
   * as before.
   */
  function reconnectAuth() {
    if (!APP_LOGIN) return refreshSession();
    return apiFetch('/api/session-check').then(function () { return true; }, function () { return false; });
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
  // ─── A file path in the chat ───
  // The workbench opens one in its editor. There is no editor here, so it is
  // shown: a picture as a picture, text as text, anything else as something
  // to download. Where the path really is is the server's to say: Claude
  // writes them whole, from the project, and from the project with a slash
  // in front, and only the server can tell which of those exists.
  var FILE_PICS = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml', ico: 'image/x-icon', bmp: 'image/bmp', avif: 'image/avif' };
  var fileShown = null, filePic = '';
  function closeFile() {
    $('cp-file').className = '';
    $('cp-file-body').textContent = '';
    if (filePic) { try { URL.revokeObjectURL(filePic); } catch (e) { /* already gone */ } filePic = ''; }
    fileShown = null;
  }
  function fileMsg(text) { var d = document.createElement('div'); d.className = 'msg'; d.textContent = text; $('cp-file-body').textContent = ''; $('cp-file-body').appendChild(d); }
  function showFile(file, size) {
    closeFile();
    var q = 'project=' + encodeURIComponent(meta.project || '') + '&file=' + encodeURIComponent(file);
    var name = file.split('/').pop() || 'file';
    var mine = fileShown = { url: '/api/files/download?' + q, name: name };
    $('cp-file-name').textContent = file;
    $('cp-file').className = 'on';
    fileMsg('Loading…');
    var ext = (name.indexOf('.') >= 0 ? name.slice(name.lastIndexOf('.') + 1) : '').toLowerCase();
    if (FILE_PICS[ext]) {
      apiFetch('/api/files/download?' + q).then(function (r) { return r.ok ? r.blob() : null; }).then(function (b) {
        if (fileShown !== mine) return;
        if (!b) { fileMsg('This file could not be read.'); return; }
        filePic = URL.createObjectURL(new Blob([b], { type: FILE_PICS[ext] }));
        var img = document.createElement('img'); img.alt = name; img.src = filePic;
        $('cp-file-body').textContent = ''; $('cp-file-body').appendChild(img);
      }).catch(function () { if (fileShown === mine) fileMsg('This file could not be read.'); });
      return;
    }
    if (Number(size) > 1024 * 1024) { fileMsg('This file is too large to show here. It can be downloaded.'); return; }
    apiFetch('/api/files/read?' + q).then(function (r) { return r.json(); }).then(function (d) {
      if (fileShown !== mine) return;
      if (!d || !d.ok) { fileMsg((d && d.error) || 'This file could not be read.'); return; }
      // Not text after all: say so, where a page of nonsense would be shown.
      if (/\u0000/.test(String(d.content).slice(0, 4000))) { fileMsg('This is not a text file. It can be downloaded.'); return; }
      var pre = document.createElement('pre'); pre.textContent = d.content;
      $('cp-file-body').textContent = ''; $('cp-file-body').appendChild(pre);
    }).catch(function () { if (fileShown === mine) fileMsg('This file could not be read.'); });
  }
  function openPath(raw) {
    var p = String(raw || '').trim();
    if (!p) return;
    apiFetch('/api/files/resolve?project=' + encodeURIComponent(meta.project || '') + '&path=' + encodeURIComponent(p))
      .then(function (r) { return r.json(); })
      .then(function (d) {
        if (!d || !d.ok) { toast('File not found: ' + p, 'error'); return; }
        if (d.dir) { toast('That is a folder: ' + p); return; }
        showFile(d.file, d.size);
      })
      .catch(function () { toast('Could not open ' + p, 'error'); });
  }
  $('cp-file-close').onclick = closeFile;
  $('cp-file-save').onclick = function () { if (fileShown) download(fileShown); };

  function toolExtra(entry) {
    var W = window.CrundiWidgets;
    if (!W || !entry) return null;
    try {
      var media = W.toolMedia ? W.toolMedia(sid, entry) : null;
      if (media) return media;
      // A panel Claude built and placed in the chat, or the pictures it took
      // of one: the workbench draws these under the tool call, so this does.
      if (entry.status !== 'done' || entry.isError) return null;
      var id = String((entry.input && entry.input.id) || '').toLowerCase();
      var name = String(entry.name || '');
      if (!id || !meta.project) return null;
      if (/widget_render$/.test(name) && W.shotsNode) return W.shotsNode(meta.project, id);
      if (/widget_open$/.test(name) && entry.input.slot === 'inline' && W.inlineNode) return W.inlineNode(meta.project, id);
    } catch (e) { /* draw the tool call without its extra */ }
    return null;
  }

  // ─── A secret Claude is asking for ───
  // The main page shows these wherever you are. This page had no such thing,
  // so a request raised from a chat open in a bubble was simply invisible: it
  // sat for three minutes and failed. Requests from this chat (and ones no
  // chat is named for) are shown over the top of it, with the same two
  // answers: approve with the PIN, or deny.
  var asks = [], askArmed = '', askError = '', askShown = '', askAuto = false;
  var askAllow = { on: false, duration: '1h' };   // "Allow for this chat" on the open card
  var CS = window.CrundiChatSecrets || null;
  function canAllow(r) { return !!(CS && CS.isSupported() && r && r.kind === 'run' && r.sessionId === sid); }
  function loadAsks() {
    if (stopped || !view || document.visibilityState !== 'visible') return;
    apiFetch('/api/secrets').then(function (r) { return r.ok ? r.json() : null; }).then(function (d) {
      if (!d) return;
      asks = (d.requests || []).filter(function (r) { return r.sessionId ? r.sessionId === sid : (!r.project || r.project === meta.project); });
      drawAsks();
    }).catch(function () { /* the next look will do */ });
  }
  function askPost(body) {
    return apiFetch('/api/secrets', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
      .then(function (r) { return r.json(); }).catch(function (e) { return { ok: false, error: e.message }; });
  }
  function drawAsks() {
    var host = $('cp-ask');
    if (askArmed && !asks.some(function (r) { return r.id === askArmed; })) { askArmed = ''; askError = ''; }
    // The PIN box is on the card from the start: typing the PIN is the
    // approval. The first request opens by itself (without taking the
    // keyboard); Approve on another card moves the box there.
    if (!askArmed && asks.length) { askArmed = asks[0].id; askAuto = true; }
    // Rebuilt only when something changed: rebuilding under someone who is
    // half-way through a PIN would eat the digits.
    var sig = asks.map(function (r) { return r.id; }).join(',') + '|' + askArmed + '|' + askError;
    if (sig === askShown) return;
    askShown = sig;
    host.textContent = '';
    host.className = asks.length ? 'on' : '';
    asks.forEach(function (r) {
      var run = r.kind === 'run';
      var card = document.createElement('div'); card.className = 'cp-ask-card';
      var add = function (cls, text) { var n = document.createElement('div'); n.className = cls; n.textContent = text; card.appendChild(n); return n; };
      add('cp-ask-head', (run ? 'Run a command with ' : 'Release ') + r.secretName);
      if (r.reason) add('cp-ask-why', r.reason);
      if (run) { add('cp-ask-cmd', r.command); add('cp-ask-why', 'The value is bound to $' + r.envName + ' and never shown to Claude.'); }
      else add('cp-ask-why', 'Claude will be able to read this value.');
      var row = document.createElement('div'); row.className = 'cp-ask-row';
      var btn = function (text, cls, fn) { var b = document.createElement('button'); b.type = 'button'; b.textContent = text; if (cls) b.className = cls; b.onclick = fn; row.appendChild(b); return b; };
      if (askArmed === r.id) {
        var pin = document.createElement('input');
        pin.setAttribute('inputmode', 'numeric'); pin.setAttribute('maxlength', '6'); pin.setAttribute('autocomplete', 'off');
        pin.placeholder = 'PIN'; pin.setAttribute('aria-label', '6-digit PIN');
        row.appendChild(pin);
        var unlock = function () {
          var v = String(pin.value || '').trim();
          if (!/^[0-9]{6}$/.test(v)) { askError = 'Enter a 6-digit PIN'; drawAsks(); return; }
          var allow = canAllow(r) ? CS.allowForChat(askAllow) : null;
          var body = { action: 'approve', reqId: r.id, pin: v };
          if (allow) body.allowForChat = allow;
          askPost(body).then(function (d) {
            // A wrong PIN leaves the request waiting on purpose, so the card
            // stays and says why.
            if (!d || !d.ok) { askError = (d && d.error) || 'Wrong PIN'; askShown = ''; drawAsks(); return; }
            askArmed = ''; askError = ''; askAllow = { on: false, duration: askAllow.duration };
            toast(allow && d.preauthorised ? 'Approved, and allowed for this chat' : 'Approved'); loadAsks();
          });
        };
        pin.addEventListener('keydown', function (e) { if (e.key === 'Enter') { e.preventDefault(); unlock(); } });
        btn('Approve', 'go', unlock);
        btn('Reject', '', function () { askArmed = ''; askError = ''; askPost({ action: 'deny', reqId: r.id }).then(loadAsks); });
        if (!askAuto) setTimeout(function () { try { pin.focus(); } catch (e) { /* not focusable yet */ } }, 30);
      } else {
        btn('Approve', 'go', function () { askArmed = r.id; askAuto = false; askError = ''; askAllow = { on: false, duration: askAllow.duration }; drawAsks(); });
        btn('Reject', '', function () { askPost({ action: 'deny', reqId: r.id }).then(loadAsks); });
      }
      card.appendChild(row);
      if (askArmed === r.id && canAllow(r)) card.appendChild(CS.allowRow(askAllow));
      if (askArmed === r.id && askError) add('cp-ask-err', askError);
      host.appendChild(card);
    });
  }
  setInterval(loadAsks, 2500);
  document.addEventListener('visibilitychange', loadAsks);

  // ─── Panels docked to this chat, and what the server announces ───
  // A panel Claude docks to a chat sits above the conversation. The widgets
  // file draws it; it has to be told when the list of panels changes and
  // when one has new data, which the server announces on its event stream.
  function dock() {
    if (!view || !window.CrundiWidgets || !window.CrundiWidgets.syncDock) return;
    try { window.CrundiWidgets.syncDock($('cp-body'), sid); } catch (e) { /* the chat still works without its panel */ }
  }
  var es = null;
  function listen() {
    if (es) { try { es.close(); } catch (e) { /* already closed */ } es = null; }
    if (stopped || !window.EventSource) return;
    es = new EventSource('/api/events?token=' + encodeURIComponent(token));
    es.addEventListener('widget', function (e) {
      // Not followed by dock(): the widgets file fetches the panels again
      // and calls onChange or onChrome when it has them, which docks. Docking
      // here as well ran on what was known BEFORE the change, so a panel just
      // collapsed was drawn open again and then collapsed a second time.
      try { var d = JSON.parse(e.data); if (window.CrundiWidgets) window.CrundiWidgets.handleEvent(d); } catch (x) { /* a malformed frame must not stop the stream */ }
    });
    // A secret request the moment it is raised, not at the next look.
    es.addEventListener('secret-requests', function () { loadAsks(); });
    // This chat's pre-authorised secrets, the moment they change.
    es.addEventListener('chat-secrets', function (e) { try { if (CS) CS.handleEvent(JSON.parse(e.data)); } catch (x) { /* a malformed frame must not stop the stream */ } });
    // The whole workbench, every few seconds: what the server says this
    // chat is doing (Catching up). Read only while a turn shows at work.
    es.addEventListener('state', function (e) {
      if (meta.state !== 'working') { serverState = null; return; }
      try {
        var d = JSON.parse(e.data), t = null, list = (d && d.terminals) || [];
        for (var i = 0; i < list.length; i++) if (list[i] && list[i].id === sid) { t = list[i]; break; }
        serverState = t ? (t.status === 'exited' ? 'exited' : (t.agentState || '')) : null;
      } catch (x) { /* a malformed frame must not stop the stream */ }
    });
    es.onerror = function () {
      // The stream is checked at connect, with a token that lasts fifteen
      // minutes. A browser retries with the same address for ever, so it is
      // closed here and opened again with a fresh token.
      try { es.close(); } catch (x) { /* already closed */ }
      es = null;
      setTimeout(function () { if (!stopped) reconnectAuth().then(listen); }, 5000);
    };
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
    connLine('');
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
  // Shown inside the main page (a factory employee's chat opens in a frame):
  // "back" and "open in Crundi" act on the main page, not inside the frame.
  var framed = (function () { try { return window.top !== window.self && window.top.location.origin === location.origin; } catch (e) { return false; } })();
  function openMain(path) {
    if (bridge('openMain', path) !== undefined) return;
    if (framed) { window.top.location.href = path; return; }
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
      var mark = $('cp-mark'), pic = m.icon ? markIcon(m.icon) : '';
      if (pic) {
        // The project's own icon. Only touched when it is a different one:
        // this runs on every change of state.
        if (mark.getAttribute('data-icon') !== pic) {
          mark.textContent = '';
          var img = document.createElement('img');
          img.alt = ''; img.src = pic;
          mark.appendChild(img);
          mark.setAttribute('data-icon', pic);
          mark.style.background = 'var(--bg-primary)';
        }
      } else {
        mark.removeAttribute('data-icon');
        mark.textContent = m.initials;
        mark.style.background = 'linear-gradient(135deg,' + m.color + ',' + m.colorDeep + ')';
      }
    }
    bridge('chatState', JSON.stringify({ id: sid, title: meta.title, project: meta.project, state: meta.state, mark: meta.mark || null }));
  }
  // The project's icon, fetched once per address as this page's login. The
  // address changes when the file does. Until it arrives, and for good if it
  // cannot be had (the file was removed, the server is older), the letters.
  var markIcons = {};
  function markIcon(u) {
    if (Object.prototype.hasOwnProperty.call(markIcons, u)) return markIcons[u];
    markIcons[u] = '';
    apiFetch(u).then(function (r) { return r && r.ok ? r.blob() : null; }).then(function (b) {
      if (!b || !b.size) return;
      markIcons[u] = URL.createObjectURL(b);
      paintHead();
    }).catch(function () { /* the letters stay */ });
    return '';
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
  // The reconnecting line under the header (see #cp-conn in chat-page.js).
  // 'on' shows it after a short wait, 'back' flashes green if it was showing.
  var connShowing = false, connBackTimer = null;
  function connLine(state) {
    var el = $('cp-conn');
    if (!el) return;
    var say = el.querySelector('span');
    clearTimeout(connBackTimer);
    if (state === 'on') {
      var offline = navigator.onLine === false;
      el.className = offline ? 'on offline' : 'on';
      connShowing = true;
      if (say) say.textContent = offline ? 'This device is offline.' : 'Reconnecting';
      return;
    }
    // Shown only once the CSS delay has passed: a blip never painted it.
    var painted = connShowing && getComputedStyle(el).opacity !== '0';
    connShowing = false;
    if (state === 'back' && painted) {
      el.className = 'back';
      if (say) say.textContent = 'Connected again.';
      connBackTimer = setTimeout(function () { el.className = ''; }, 1700);
    } else {
      el.className = '';
    }
  }
  window.addEventListener('offline', function () { if (connShowing) connLine('on'); });
  window.addEventListener('online', function () { if (connShowing) connLine('on'); });

  function connect() {
    clearTimeout(retryTimer);
    if (stopped) return;
    if (ws) { ws.onopen = ws.onmessage = ws.onerror = ws.onclose = null; try { ws.close(); } catch (e) { /* ignore */ } }
    var url = (location.protocol === 'https:' ? 'wss:' : 'ws:') + '//' + location.host + '/ws?token=' + encodeURIComponent(token) + (VERSION ? '&web=' + encodeURIComponent(VERSION) : '');
    ws = new WebSocket(url);
    ws.onopen = function () {
      everOpen = true; retry = 0;
      connLine('back');
      // The server replays the conversation on subscribe, which is also how a
      // dropped connection catches up on what it missed.
      if (view) view.resubscribe();
      present();
    };
    ws.onmessage = function (e) {
      var msg; try { msg = JSON.parse(e.data); } catch (err) { return; }
      if (!view || msg.id !== sid) return;
      heardAt = Date.now();
      if (msg.type === 'ui-history') {
        view.applyHistory(msg.session);
        // The Manager of a factory run: a way to its live graph.
        var fb = $('cp-factory');
        // Not when shown inside the live graph itself (?in=graph): it is right there.
        if (fb && !/[?&]in=graph(&|$)/.test(location.search) && msg.session && msg.session.factory && msg.session.factory.runId) {
          var runId = msg.session.factory.runId;
          fb.hidden = false;
          fb.onclick = function () { openMain('/#factory-run=' + encodeURIComponent(runId)); };
        }
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
      connLine('on');
      retry++;
      // A socket is checked at connect only, and an access token lasts fifteen
      // minutes: a refusal here is usually just a token that has aged out.
      var again = function () { retryTimer = setTimeout(connect, Math.min(15000, 600 * Math.pow(1.7, retry))); };
      if (!everOpen || retry % 3 === 0) reconnectAuth().then(again); else again();
    };
    ws.onerror = function () { /* onclose follows and does the work */ };
  }
  document.addEventListener('visibilitychange', function () {
    present();
    if (document.visibilityState !== 'visible') return;
    if (!ws || ws.readyState > 1) { retry = 0; connect(); }
    // Open, but the page was away (a bubble folded, the phone asleep) and a
    // reply may have ended meanwhile: ask for the conversation again, as a
    // reconnect does. It replaces what is drawn.
    else if (ws.readyState === 1 && view) { heardAt = Date.now(); view.resubscribe(); }
  });
  setInterval(present, 60000);

  // ─── Catching up ───
  // A bubble that stopped mid-sentence and said "working" for ever: the
  // socket had gone quiet without closing, which a browser cannot see (it
  // has no way to ping). So while the chat shows a turn at work, two signs
  // that it has fallen behind, and a new socket for either (it replays the
  // conversation on open): the server's own list, on the event stream, has
  // said for a while that the chat is not working; or nothing at all has come
  // for it in two minutes (a long tool run is quiet too, so not sooner).
  var heardAt = Date.now(), serverState = null, disagreeSince = 0, caughtUpAt = 0;
  function catchUp() { caughtUpAt = Date.now(); disagreeSince = 0; heardAt = caughtUpAt; retry = 0; connect(); }
  setInterval(function () {
    if (stopped || !view || document.visibilityState !== 'visible') return;
    if (meta.state !== 'working') { disagreeSince = 0; return; }
    var now = Date.now();
    if (serverState !== null && serverState !== 'working') {
      if (!disagreeSince) disagreeSince = now;
      if (now - disagreeSince >= 8000 && (!caughtUpAt || now - caughtUpAt >= 30000)) { catchUp(); return; }
    } else disagreeSince = 0;
    if (now - Math.max(heardAt, caughtUpAt) >= 120000) catchUp();
  }, 2000);

  // ─── Buttons ───
  $('cp-back').onclick = function () {
    // A file being shown is what is in front: back puts that away first.
    if (fileShown) { closeFile(); return; }
    if (bridge('close') !== undefined) return;
    // In a frame, the main page has its own Close; history.back() here would
    // move the main page through its history instead.
    if (framed) { try { window.top.postMessage({ type: 'crundi-chat-close' }, location.origin); } catch (e) { /* fine */ } return; }
    if (history.length > 1) history.back(); else location.href = '/';
  };
  // Renaming is done in the header itself: the title becomes a box to type
  // in. A dialog would be the obvious way, and a web view inside an app
  // shows none unless the app builds one, so the obvious way does nothing.
  function renameHere() {
    var t = $('cp-title');
    if (t.querySelector('input')) return;
    var box = document.createElement('input');
    box.value = meta.title || '';
    box.setAttribute('aria-label', 'Chat name');
    box.setAttribute('maxlength', '80');
    box.style.cssText = 'width:100%;font:inherit;font-weight:650;color:var(--text-primary);background:var(--bg-primary);border:1px solid var(--accent);border-radius:6px;padding:2px 6px;';
    t.textContent = '';
    t.appendChild(box);
    var done = false;
    var finish = function (save) {
      if (done) return;
      done = true;
      var next = String(box.value || '').trim();
      paintHead();
      if (!save || !next || next === meta.title) return;
      apiFetch('/api/ui-sessions/' + encodeURIComponent(sid) + '/rename', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ title: next }) })
        .then(function (r) { return r.json(); })
        .then(function (d) { if (d && d.ok === false) toast(d.error || 'Could not rename it', 'error'); else loadMeta(); })
        .catch(function () { toast('Could not rename it', 'error'); });
    };
    box.addEventListener('keydown', function (e) { if (e.key === 'Enter') { e.preventDefault(); finish(true); } if (e.key === 'Escape') finish(false); });
    box.addEventListener('blur', function () { finish(true); });
    setTimeout(function () { try { box.focus(); box.select(); } catch (e) { /* not focusable yet */ } }, 30);
  }

  var menu = $('cp-menu'), hasPlan = false;
  $('cp-more').onclick = function (e) {
    e.stopPropagation();
    menu.className = menu.className === 'on' ? '' : 'on';
    menu.querySelector('[data-do="stop"]').disabled = meta.state !== 'working';
    menu.querySelector('[data-do="plan"]').style.display = hasPlan ? '' : 'none';
  };
  document.addEventListener('click', function () { menu.className = ''; });
  menu.onclick = function (e) {
    var act = e.target && e.target.getAttribute && e.target.getAttribute('data-do');
    if (act === 'open') openMain('/chat/' + encodeURIComponent(sid));
    if (act === 'plan' && view && view.togglePlan) view.togglePlan();
    if (act === 'rename') renameHere();
    if (act === 'secrets' && CS) CS.open(sid);
  };
  // ─── Secrets pre-authorised for this chat ───
  // The lock in the header (wide) and the menu line (narrow) open the shared
  // dialog. Shown only when the server offers it.
  function paintLock() {
    if (!CS) return;
    var ok = CS.isSupported();
    var n = ok ? CS.grants(sid).length : 0;
    var lock = $('cp-lock');
    lock.className = 'cp-btn' + (ok ? ' ok' : '') + (n ? ' on' : '');
    lock.querySelector('b').textContent = n ? String(n) : '';
    lock.title = n ? n + ' secret' + (n === 1 ? '' : 's') + ' pre-authorised for this chat' : 'Pre-authorise secrets for this chat';
    var item = menu.querySelector('[data-do="secrets"]');
    item.style.display = ok ? '' : 'none';
    item.textContent = n ? 'Secrets for this chat (' + n + ')' : 'Secrets for this chat';
  }
  $('cp-lock').onclick = function (e) { e.stopPropagation(); if (CS) CS.open(sid); };
  function startSecrets() {
    if (!CS) return;
    CS.configure({ apiFetch: apiFetch, toast: toast });
    CS.onChange(function (which) { if (!which || which === sid) paintLock(); });
    CS.supported().then(function (yes) { if (yes) CS.load(sid); paintLock(); drawAsksAgain(); });
  }
  function drawAsksAgain() { askShown = ''; drawAsks(); }
  // ─── Start ───
  function start() {
    if (!sid || !/^[A-Za-z0-9_-]{1,80}$/.test(sid)) { note('Not a chat', 'This address does not name a chat.', 'Open Crundi', function () { openMain('/'); }); return; }
    if (!window.CrundiChat) { note('Could not load', 'The chat could not be loaded. Check the connection and try again.', 'Try again', function () { location.reload(); }); return; }
    readSession();
    if (FRAME_LOGIN) token = FRAME_LOGIN.token();
    if (!APP_LOGIN && !token && !refreshToken) { note('Signed out', 'Sign in to Crundi to see this chat.', 'Sign in', function () { openMain('/'); }); return; }
    var ready = APP_LOGIN
      ? askAppLogin('get').then(function (m) { return takeAppLogin(m) || refreshSession(); })
      : (token ? Promise.resolve(true) : refreshSession());
    ready.then(function () { return loadMeta(); }).then(function (found) {
      if (!found || stopped) return;
      if (window.CrundiWidgets) {
        window.CrundiWidgets.configure({
          apiFetch: apiFetch, toast: toast, isMobile: function () { return true; },
          mediaUrl: mediaUrl, refreshAuth: refreshSession, download: download,
          liveChats: function () { return [sid]; },
          onChange: function () { dock(); }, onChrome: function () { dock(); },
        });
        try { Promise.resolve(window.CrundiWidgets.setProject(meta.project || '')).then(dock); } catch (e) { /* panels then simply do not draw */ }
      }
      view = window.CrundiChat.mount($('cp-chat'), {
        sessionId: sid,
        project: meta.project,
        apiFetch: apiFetch,
        apiUpload: apiUpload,
        toast: toast,
        wsSend: wsSend,
        toolExtra: toolExtra,
        openPath: openPath,
        onChatMeta: function (patch) {
          if (!patch) return;
          if (typeof patch.title === 'string' && patch.title) { meta.title = patch.title; paintHead(); }
          // Whether there is a plan to show. The workbench has a pill for it
          // in the chat's header; here it is a line in the menu.
          if ('plan' in patch) hasPlan = !!patch.plan;
        },
      });
      connect();
      listen();
      startSecrets();
    });
  }
  start();
})();
