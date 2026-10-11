/**
 * crundi-phone.js — the "Phone app" part of Settings.
 *
 * Two things: the dialog that shows the pairing picture and waits for a phone
 * to scan it, and the box where the push key goes. Kept out of webapp-html.js
 * so it is an ordinary script.
 *
 *   CrundiPhone.connect({ apiFetch, toast })              open the pairing dialog
 *   CrundiPhone.renderPush(el, { apiFetch, toast })       draw push settings into el
 *
 * The picture (the "mark") comes from the server as finished SVG, with its own
 * animation and its own ending built in. This file shows it, asks every
 * second or so what became of the code, and plays the ending when the answer
 * is "paired".
 */
(function () {
  'use strict';

  function el(tag, css, text) {
    var n = document.createElement(tag);
    if (css) n.style.cssText = css;
    if (text !== undefined) n.textContent = text;
    return n;
  }
  function post(apiFetch, path, body) {
    return apiFetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body || {}) })
      .then(function (r) { return r.json().catch(function () { return { ok: false, error: 'Bad answer from the server' }; }); });
  }
  var BTN = 'padding:9px 16px;border-radius:8px;border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font:inherit;font-size:0.86rem;cursor:pointer;';
  var PRIMARY = BTN + 'background:var(--accent);border-color:var(--accent);color:#fff;';
  var LINK = 'background:none;border:0;padding:0;color:var(--accent-hover);font:inherit;font-size:0.8rem;cursor:pointer;text-decoration:underline;text-underline-offset:2px;';

  // ─── Pairing ───
  var open = null;       // only one dialog at a time

  function connect(opts) {
    if (open) return;
    var apiFetch = opts.apiFetch, toast = opts.toast || function () {};
    var code = '', poll = 0, tick = 0, closed = false, paired = false, showingQr = false, current = null;

    var wrap = el('div', 'position:fixed;inset:0;z-index:9000;display:flex;align-items:center;justify-content:center;background:rgba(0,0,0,.72);padding:16px;');
    wrap.setAttribute('role', 'dialog');
    wrap.setAttribute('aria-label', 'Connect a phone');
    var box = el('div', 'background:var(--bg-secondary);border:1px solid var(--border);border-radius:16px;padding:22px;width:100%;max-width:380px;display:flex;flex-direction:column;align-items:center;gap:12px;max-height:94vh;overflow:auto;');
    var title = el('h3', 'margin:0;font-size:1.05rem;color:var(--text-primary);', 'Connect a phone');
    var pic = el('div', 'width:min(100%,300px);aspect-ratio:1;border-radius:22px;overflow:hidden;background:#0a0a0f;display:flex;align-items:center;justify-content:center;color:var(--text-muted);font-size:0.85rem;');
    var line = el('div', 'font-size:0.86rem;color:var(--text-secondary);text-align:center;line-height:1.5;min-height:2.6em;', 'Getting a code…');
    var small = el('div', 'font-size:0.74rem;color:var(--text-muted);text-align:center;min-height:1.2em;');
    var swap = el('button', LINK, '');
    swap.type = 'button';
    var row = el('div', 'display:flex;gap:10px;justify-content:center;flex-wrap:wrap;margin-top:4px;');
    var again = el('button', PRIMARY, 'New code'); again.type = 'button'; again.style.display = 'none';
    var done = el('button', BTN, 'Close'); done.type = 'button';
    row.appendChild(again); row.appendChild(done);
    [title, pic, line, small, swap, row].forEach(function (n) { box.appendChild(n); });
    wrap.appendChild(box);
    document.body.appendChild(wrap);
    open = wrap;

    function stop() { clearInterval(poll); clearInterval(tick); poll = tick = 0; }
    function close() {
      if (closed) return;
      closed = true;
      stop();
      // A code left on a closed dialog is a login nobody is watching. Take it back.
      if (code && !paired) post(apiFetch, '/api/pair/cancel', { code: code }).catch(function () {});
      wrap.remove();
      open = null;
      if (paired && opts.onPaired) { try { opts.onPaired(); } catch (e) { /* host's problem */ } }
    }
    done.onclick = close;
    wrap.addEventListener('click', function (e) { if (e.target === wrap) close(); });
    again.onclick = start;

    function show() {
      if (!current) return;
      // The picture is markup the server built for this purpose (crundi-code.js
      // and qr.js): finished SVG, with no script in it.
      pic.innerHTML = showingQr || !current.mark ? current.qr : current.mark;
      var svg = pic.querySelector('svg');
      if (svg) { svg.style.width = '100%'; svg.style.height = '100%'; svg.style.display = 'block'; }
      if (!current.mark) { swap.style.display = 'none'; small.dataset.note = 'This address is too long for the Crundi code, so this is a QR code.'; }
      else { swap.style.display = ''; swap.textContent = showingQr ? 'Show the Crundi code' : 'Show a QR code instead'; small.dataset.note = ''; }
    }
    swap.onclick = function () { showingQr = !showingQr; show(); };

    function start() {
      stop();
      again.style.display = 'none';
      line.textContent = 'Getting a code…';
      small.textContent = '';
      pic.textContent = '';
      post(apiFetch, '/api/pair/start', { origin: location.origin }).then(function (d) {
        if (closed) return;
        if (!d || !d.ok) { line.textContent = (d && d.error) || 'Could not get a pairing code.'; again.style.display = ''; return; }
        current = d; code = d.code; showingQr = false;
        show();
        line.textContent = 'Open the Crundi app on your phone and point it here.';
        var left = d.expiresIn || 300;
        var warn = d.insecure ? 'This server is not using https. ' : '';
        var paint = function () { small.textContent = (small.dataset.note ? small.dataset.note + ' ' : '') + warn + 'Code expires in ' + Math.floor(left / 60) + ':' + String(left % 60).padStart(2, '0'); };
        paint();
        tick = setInterval(function () { left = Math.max(0, left - 1); paint(); }, 1000);
        poll = setInterval(function () {
          post(apiFetch, '/api/pair/status', { code: code }).then(function (s) {
            if (closed || !s || !s.ok) return;
            if (s.state === 'paired') return succeed(d, s.device || {});
            if (s.state === 'expired') { stop(); pic.textContent = 'Expired'; line.textContent = 'That code ran out before a phone used it.'; small.textContent = ''; swap.style.display = 'none'; again.style.display = ''; code = ''; }
          }).catch(function () { /* the next ask will do */ });
        }, 1500);
      }).catch(function () { if (!closed) { line.textContent = 'Could not reach the server.'; again.style.display = ''; } });
    }

    function succeed(d, device) {
      paired = true;
      stop();
      swap.style.display = 'none';
      small.textContent = '';
      // The ending is in the Crundi code itself; the QR code has none, so
      // bring the Crundi code back to play it on.
      if (d.mark && showingQr) { showingQr = false; show(); }
      var svg = pic.querySelector('svg');
      if (svg && d.mark && d.doneClass) svg.classList.add(d.doneClass);
      line.textContent = '';
      var name = el('b', 'color:var(--green);', (device.name || 'Phone') + ' connected');
      line.appendChild(name);
      toast((device.name || 'Phone') + ' connected', 'success');
      setTimeout(close, (d.doneMs || 1500) + 1800);
    }

    start();
  }

  // ─── Push ───
  function renderPush(host, opts) {
    var apiFetch = opts.apiFetch, toast = opts.toast || function () {};
    host.textContent = '';
    host.appendChild(el('div', 'font-size:0.8rem;color:var(--text-muted);', 'Checking…'));
    apiFetch('/api/push/mobile').then(function (r) { return r.json(); }).then(function (s) {
      if (!s || !s.ok) throw new Error('unavailable');
      draw(s);
    }).catch(function () { host.textContent = ''; host.appendChild(el('div', 'font-size:0.8rem;color:var(--text-muted);', 'Phone notifications are not available on this server.')); });

    function draw(s) {
      host.textContent = '';
      var n = (s.devices || []).length;
      var state = el('div', 'font-size:0.8rem;color:var(--text-secondary);line-height:1.5;');
      if (s.hasKey) state.textContent = 'Push key ' + s.keyHint + ' is set. ' + (n ? n + ' phone' + (n === 1 ? '' : 's') + ' registered.' : 'No phone has registered for notifications yet.');
      else state.textContent = 'Notifications reach the phone app through a relay, which needs a key for this server. ' + (s.keyHelp || '');
      host.appendChild(state);
      if (!s.hasKey && s.contact && s.contact.url) {
        var a = el('a', 'font-size:0.8rem;color:var(--accent-hover);', s.contact.telegram ? 'Message @' + s.contact.telegram : 'Ask for a key');
        a.href = s.contact.url; a.target = '_blank'; a.rel = 'noopener';
        host.appendChild(a);
      }
      if (s.lastError) host.appendChild(el('div', 'font-size:0.78rem;color:var(--yellow);margin-top:6px;', s.lastError));

      var row = el('div', 'display:flex;gap:8px;flex-wrap:wrap;margin-top:10px;align-items:center;');
      var input = el('input', 'flex:1;min-width:180px;padding:8px 10px;border-radius:8px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font:inherit;font-size:0.82rem;');
      input.type = 'password'; input.autocomplete = 'off'; input.spellcheck = false;
      input.placeholder = s.hasKey ? 'Paste a new key to replace it' : 'Paste the push key';
      input.setAttribute('aria-label', 'Push key');
      var save = el('button', PRIMARY, 'Save'); save.type = 'button';
      row.appendChild(input); row.appendChild(save);
      if (s.hasKey) {
        var test = el('button', BTN, 'Test'); test.type = 'button';
        var remove = el('button', BTN, 'Remove'); remove.type = 'button';
        row.appendChild(test); row.appendChild(remove);
        test.onclick = function () {
          test.disabled = true;
          post(apiFetch, '/api/push/mobile/test').then(function (d) {
            test.disabled = false;
            if (d.ok && d.sent) toast('Test notification sent', 'success');
            else if (d.ok) toast(d.note || 'The key is good.', 'success');
            else toast(d.error || 'The test did not go through', 'error');
            renderPush(host, opts);
          }).catch(function () { test.disabled = false; toast('Could not reach the server', 'error'); });
        };
        remove.onclick = function () {
          apiFetch('/api/push/mobile/key', { method: 'DELETE' }).then(function () { toast('Push key removed'); renderPush(host, opts); }).catch(function () { toast('Could not remove the key', 'error'); });
        };
      }
      host.appendChild(row);
      save.onclick = function () {
        var key = input.value.trim();
        if (!key) { input.focus(); return; }
        save.disabled = true;
        post(apiFetch, '/api/push/mobile/key', { key: key }).then(function (d) {
          save.disabled = false;
          if (!d.ok) { toast(d.error || 'Could not save the key', 'error'); return; }
          if (d.accepted) toast('Push key saved' + (d.label ? ' (' + d.label + ')' : ''), 'success');
          else toast(d.error || 'Saved, but the relay did not accept it', 'error');
          renderPush(host, opts);
        }).catch(function () { save.disabled = false; toast('Could not reach the server', 'error'); });
      };

      // A custom relay is for a custom build of the app, which is rare enough
      // to stay folded away.
      var more = el('details', 'margin-top:10px;font-size:0.76rem;color:var(--text-muted);');
      var sum = el('summary', 'cursor:pointer;', 'Relay: ' + s.relayUrl.replace(/^https?:\/\//, '') + (s.customRelay ? ' (custom)' : ''));
      more.appendChild(sum);
      more.appendChild(el('div', 'margin:8px 0;line-height:1.5;', 'Only change this if you use your own build of the app, with its own relay. The published app works with the published relay and no other. Changing the relay removes the key.'));
      var rrow = el('div', 'display:flex;gap:8px;flex-wrap:wrap;');
      var rin = el('input', 'flex:1;min-width:180px;padding:7px 10px;border-radius:8px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font:inherit;font-size:0.8rem;');
      rin.type = 'url'; rin.placeholder = 'https://your-relay.example'; rin.value = s.customRelay ? s.relayUrl : '';
      rin.setAttribute('aria-label', 'Relay address');
      var rsave = el('button', BTN, s.customRelay ? 'Save' : 'Use this relay'); rsave.type = 'button';
      rrow.appendChild(rin); rrow.appendChild(rsave);
      if (s.customRelay) { var back = el('button', BTN, 'Use the published relay'); back.type = 'button'; rrow.appendChild(back); back.onclick = function () { setRelay(''); }; }
      more.appendChild(rrow);
      host.appendChild(more);
      rsave.onclick = function () { setRelay(rin.value.trim()); };
      function setRelay(url) {
        post(apiFetch, '/api/push/mobile/relay', { relayUrl: url }).then(function (d) {
          if (!d.ok) { toast(d.error || 'Could not change the relay', 'error'); return; }
          toast(d.reachable === false ? (d.error || 'Saved, but that relay did not answer') : 'Relay changed', d.reachable === false ? 'error' : 'success');
          renderPush(host, opts);
        }).catch(function () { toast('Could not reach the server', 'error'); });
      }
    }
  }

  window.CrundiPhone = { connect: connect, renderPush: renderPush };
})();
