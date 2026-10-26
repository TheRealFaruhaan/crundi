/**
 * crundi-secret-lock.js — how a secret's lock is typed, re-locked and edited,
 * in the browser.
 *
 * Shared by the workbench (src/webapp-html.js), the compact chat page
 * (/chat/<id>, crundi-chat-page.js), the chat pre-authorise dialog
 * (crundi-chat-secrets.js) and the factory start prompts
 * (crundi-factories.js), so every place that asks for a lock asks the same way:
 *
 *   - field(input, lock): a 6-digit PIN field (digits only, as always) for an
 *     old secret, a plain password field for a passphrase;
 *   - lockOk(lock, text): enough to send;
 *   - passphraseOk(text) / RULE: what a NEW lock must be (8+ characters, not
 *     digits only), the server's rule (src/secrets-store.js);
 *   - upgrade({...}): after an old lock was accepted, "set a passphrase" right
 *     there, using the lock just typed (kept in this call only, never stored);
 *   - edit({...}): change a secret's lock, value, name or description, proven
 *     by its current lock.
 *
 * Capabilities: 'secret-passphrase' (lock kinds), 'secret-edit' (action
 * 'edit') and 'secret-upgrade' (needsUpgrade). supports(cap) asks the
 * handshake once. A server without them has 6-digit PINs only, and the
 * upgrade and edit are not offered.
 *
 * No prompt/confirm/alert: a web view inside the phone app shows none.
 */
(function () {
  'use strict';

  var MIN = 8;
  var RULE = 'At least ' + MIN + ' characters, and not only digits';
  var UNSUPPORTED = 'This server does not support this yet. Update the server to use it.';
  var LOCK = '<svg class="csl-ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>';

  var host = { apiFetch: null, toast: function () {} };
  var caps = null, capsAsk = null;

  function configure(o) {
    o = o || {};
    if (o.apiFetch) host.apiFetch = o.apiFetch;
    if (o.toast) host.toast = o.toast;
  }

  function fetcher() { return host.apiFetch; }

  /** Does the server list this capability? (Asked once.) */
  function supports(cap) {
    if (caps) return Promise.resolve(caps.indexOf(cap) >= 0);
    var f = fetcher();
    if (!f) return Promise.resolve(false);
    if (!capsAsk) {
      capsAsk = f('/api/handshake').then(function (r) { return r.json(); })
        .then(function (hs) { caps = (hs && hs.server && hs.server.capabilities) || []; return caps; })
        .catch(function () { capsAsk = null; return []; });
    }
    return capsAsk.then(function (c) { return c.indexOf(cap) >= 0; });
  }
  function knows(cap) { return !!caps && caps.indexOf(cap) >= 0; }

  function passphraseOk(t) {
    t = String(t == null ? '' : t);
    return Array.from(t).length >= MIN && /[^0-9]/.test(t);
  }
  function lockOk(lock, t) {
    t = String(t == null ? '' : t);
    return lock === 'passphrase' ? t.length > 0 && t.length <= 1024 : /^[0-9]{6}$/.test(t);
  }
  function kindOf(x) { return x && x.lock === 'passphrase' ? 'passphrase' : 'pin'; }
  function askText(lock) { return lock === 'passphrase' ? 'Enter the passphrase' : 'Enter a 6-digit PIN'; }

  /**
   * Make an input ask for this lock. A PIN keeps the old field exactly
   * (numeric keyboard, 6 digits, anything else dropped as typed); a
   * passphrase takes any text. Safe to call again on the same input.
   */
  function field(input, lock, label) {
    if (!input) return input;
    var pass = lock === 'passphrase';
    input.type = 'password';
    input.setAttribute('autocomplete', 'off');
    input.setAttribute('autocapitalize', 'off');
    input.setAttribute('spellcheck', 'false');
    if (pass) {
      input.removeAttribute('inputmode'); input.removeAttribute('maxlength'); input.removeAttribute('pattern');
      input.placeholder = 'Passphrase';
      input.setAttribute('aria-label', label ? 'Passphrase for ' + label : 'Passphrase');
      input.classList.add('csl-pass');
      input.oninput = null;
    } else {
      input.setAttribute('inputmode', 'numeric'); input.setAttribute('maxlength', '6');
      input.placeholder = 'PIN';
      input.setAttribute('aria-label', label ? 'PIN for ' + label : '6-digit PIN');
      input.classList.remove('csl-pass');
    }
    input.dataset.lock = pass ? 'passphrase' : 'pin';
    return input;
  }
  /** Keep a PIN field to digits (call from the field's own input handler). */
  function filter(input) {
    if (input && input.dataset.lock !== 'passphrase') {
      var v = input.value.replace(/[^0-9]/g, '').slice(0, 6);
      if (v !== input.value) input.value = v;
    }
    return input ? input.value : '';
  }

  function post(body) {
    var f = fetcher();
    if (!f) return Promise.resolve({ ok: false, error: 'Not ready' });
    return f('/api/secrets', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
      .then(function (r) {
        if (r.status === 404) return { ok: false, error: UNSUPPORTED, unsupported: true };
        return r.json().catch(function () { return { ok: false, error: 'The server answered with HTTP ' + r.status + '.' }; });
      })
      .then(function (d) { if (d && !d.ok && /Unknown secrets action/.test(d.error || '')) return { ok: false, error: UNSUPPORTED, unsupported: true }; return d; })
      .catch(function (e) { return { ok: false, error: e.message }; });
  }

  // ─── Dialogs ───
  function el(tag, cls, text) { var n = document.createElement(tag); if (cls) n.className = cls; if (text != null) n.textContent = text; return n; }
  var open = null;    // the dialog showing, one at a time
  var queue = [];

  function closeDialog() {
    if (!open) return;
    open.root.querySelectorAll('input,textarea').forEach(function (i) { i.value = ''; });
    open.root.remove();
    document.removeEventListener('keydown', open.onKey);
    var done = open.done; open = null;
    if (done) try { done(); } catch (e) { /* the caller's */ }
    var next = queue.shift(); if (next) next();
  }

  function frame(title, onKey) {
    injectCss();
    var root = el('div', 'csl-scrim');
    root.setAttribute('role', 'dialog'); root.setAttribute('aria-modal', 'true'); root.setAttribute('aria-label', title);
    var card = el('div', 'csl-card');
    var head = el('div', 'csl-head');
    var ic = el('span', 'csl-lock'); ic.innerHTML = LOCK; head.appendChild(ic);
    head.appendChild(el('div', 'csl-title', title));
    card.appendChild(head);
    root.appendChild(card);
    document.body.appendChild(root);
    return { root: root, card: card };
  }

  function passInput(placeholder) {
    var i = document.createElement('input');
    i.type = 'password'; i.className = 'csl-in csl-pass'; i.placeholder = placeholder;
    i.setAttribute('autocomplete', 'new-password'); i.setAttribute('autocapitalize', 'off'); i.setAttribute('spellcheck', 'false');
    i.setAttribute('aria-label', placeholder);
    return i;
  }

  /** A new passphrase and its confirmation; returns { box, value(), valid(), why() }. */
  function newLockFields(card, onChange) {
    var a = passInput('New passphrase'), b = passInput('Confirm the passphrase');
    var hint = el('div', 'csl-hint', RULE + '.');
    card.appendChild(a); card.appendChild(b); card.appendChild(hint);
    function why() {
      if (!a.value) return '';
      if (!passphraseOk(a.value)) return RULE + '.';
      if (b.value && b.value !== a.value) return 'The two do not match.';
      return '';
    }
    function valid() { return passphraseOk(a.value) && a.value === b.value; }
    function sync() { var w = why(); hint.textContent = w || RULE + '.'; hint.classList.toggle('csl-warn', !!w); if (onChange) onChange(); }
    a.oninput = sync; b.oninput = sync;
    return { a: a, b: b, value: function () { return a.value; }, valid: valid, why: why, clear: function () { a.value = ''; b.value = ''; } };
  }

  /**
   * Offer to re-lock a secret whose old lock was just accepted.
   * o: { id, name, lock ('pin'|'passphrase'), pin (the lock just typed),
   *      onUpdated(newPin), onDone() }
   * Asks only when the server lists 'secret-upgrade'. The old lock lives in
   * this call only and is dropped when the dialog closes.
   */
  function upgrade(o) {
    o = o || {};
    if (!o.id || !o.pin) return;
    supports('secret-upgrade').then(function (yes) {
      if (!yes) return;
      var show = function () { drawUpgrade(o); };
      if (open) queue.push(show); else show();
    });
  }

  function drawUpgrade(o) {
    var oldPin = String(o.pin);
    var f = frame('Update this secret’s lock');
    var card = f.card;
    var old = o.lock !== 'passphrase';
    card.appendChild(el('div', 'csl-name', o.name || 'This secret'));
    card.appendChild(el('div', 'csl-text', old
      ? 'This secret uses an old 6-digit PIN. Set a new passphrase (8+ characters).'
      : 'This secret uses an older, weaker lock. Set a passphrase (8+ characters) to seal it again; it may be the same one.'));
    var go;
    var nf = newLockFields(card, function () { go.disabled = !nf.valid(); });
    var err = el('div', 'csl-err'); card.appendChild(err);
    var foot = el('div', 'csl-right');
    var later = el('button', 'csl-btn', 'Not now'); later.type = 'button'; foot.appendChild(later);
    go = el('button', 'csl-btn csl-go', 'Update'); go.type = 'button'; go.disabled = true; foot.appendChild(go);
    card.appendChild(foot);
    later.onclick = function () { oldPin = ''; closeDialog(); };
    go.onclick = function () {
      if (!nf.valid() || go.disabled) return;
      var np = nf.value();
      go.disabled = true; later.disabled = true; go.textContent = 'Updating…'; err.textContent = '';
      post({ action: 'edit', id: o.id, pin: oldPin, newPin: np }).then(function (d) {
        if (!open) return;
        if (d && d.ok) {
          oldPin = '';
          host.toast('Lock updated', 'success');
          if (o.onUpdated) try { o.onUpdated(np); } catch (e) { /* the caller's */ }
          closeDialog();
          return;
        }
        go.textContent = 'Update'; later.disabled = false; go.disabled = !nf.valid();
        err.textContent = (d && d.error) || 'Could not update it.';
      });
    };
    open = { root: f.root, done: function () { oldPin = ''; if (o.onDone) o.onDone(); }, onKey: function (e) { if (e.key === 'Escape') { oldPin = ''; closeDialog(); } } };
    document.addEventListener('keydown', open.onKey);
    setTimeout(function () { nf.a.focus(); }, 30);
  }

  /**
   * Edit a secret. s: { id, name, description, lock }. o.onSaved(result, newPin?)
   * Three things, any of them: a new passphrase, a new value, a new name or
   * description. The current lock proves it is theirs (and is what the value
   * is re-sealed with when the lock does not change).
   */
  function edit(s, o) {
    o = o || {};
    if (!s || !s.id) return;
    var f = frame('Edit secret');
    var card = f.card;
    var lock = kindOf(s);
    card.appendChild(el('div', 'csl-sec', 'Name and description'));
    var name = document.createElement('input'); name.className = 'csl-in'; name.value = s.name || ''; name.setAttribute('aria-label', 'Name'); name.placeholder = 'Name';
    var desc = document.createElement('textarea'); desc.className = 'csl-in'; desc.value = s.description || ''; desc.setAttribute('aria-label', 'Description'); desc.placeholder = 'Description';
    card.appendChild(name); card.appendChild(desc);
    card.appendChild(el('div', 'csl-sec', 'New value'));
    var val = document.createElement('textarea'); val.className = 'csl-in csl-mono'; val.placeholder = 'Leave empty to keep the value'; val.setAttribute('aria-label', 'New value');
    card.appendChild(val);
    card.appendChild(el('div', 'csl-sec', 'New passphrase'));
    var change = el('label', 'csl-check');
    var cb = document.createElement('input'); cb.type = 'checkbox'; change.appendChild(cb);
    change.appendChild(el('span', '', lock === 'pin' ? 'Replace the 6-digit PIN with a passphrase' : 'Change the passphrase'));
    card.appendChild(change);
    var nfBox = el('div', 'csl-sub'); nfBox.style.display = 'none'; card.appendChild(nfBox);
    var go;
    var nf = newLockFields(nfBox, function () { sync(); });
    card.appendChild(el('div', 'csl-sec', lock === 'pin' ? 'Current PIN' : 'Current passphrase'));
    var cur = field(document.createElement('input'), lock, s.name); cur.className = 'csl-in' + (lock === 'pin' ? ' csl-pinbox' : ' csl-pass');
    cur.addEventListener('input', function () { filter(cur); sync(); });
    card.appendChild(cur);
    card.appendChild(el('div', 'csl-hint', 'Needed for any change to the value or the lock. It is never stored.'));
    var err = el('div', 'csl-err'); card.appendChild(err);
    var foot = el('div', 'csl-right');
    var cancel = el('button', 'csl-btn', 'Cancel'); cancel.type = 'button'; foot.appendChild(cancel);
    go = el('button', 'csl-btn csl-go', 'Save'); go.type = 'button'; foot.appendChild(go);
    card.appendChild(foot);
    function changes() {
      return { relock: cb.checked, revalue: val.value !== '', meta: name.value.trim() !== (s.name || '') || desc.value !== (s.description || '') };
    }
    function sync() {
      var c = changes();
      var needLock = c.relock || c.revalue;
      var okay = (c.relock ? nf.valid() : true) && (!needLock || lockOk(lock, cur.value)) && (c.relock || c.revalue || c.meta) && !!name.value.trim();
      go.disabled = !okay;
    }
    cb.onchange = function () { nfBox.style.display = cb.checked ? '' : 'none'; if (!cb.checked) nf.clear(); sync(); if (cb.checked) nf.a.focus(); };
    name.oninput = sync; desc.oninput = sync; val.oninput = sync;
    cancel.onclick = closeDialog;
    go.onclick = function () {
      if (go.disabled) return;
      var c = changes();
      var body = { action: 'edit', id: s.id };
      if (c.meta) { body.name = name.value.trim(); body.description = desc.value; }
      var done = function (d) {
        if (!open) return;
        if (d && d.ok) {
          host.toast('Secret saved', 'success');
          if (o.onSaved) try { o.onSaved(d, c.relock ? body.newPin : '', c.relock || c.revalue ? cur.value : ''); } catch (e) { /* the caller's */ }
          closeDialog();
          return;
        }
        go.disabled = false; go.textContent = 'Save';
        err.textContent = (d && d.error) || 'Could not save it.';
        if (d && /Incorrect|PIN must|passphrase$/i.test(d.error || '')) { cur.select(); cur.focus(); }
      };
      go.disabled = true; go.textContent = 'Saving…'; err.textContent = '';
      if (!c.relock && !c.revalue) {
        // Only the name or description: no lock needed (action 'updateMeta').
        post({ action: 'updateMeta', id: s.id, name: body.name, description: body.description }).then(done);
        return;
      }
      body.pin = cur.value;
      if (c.relock) body.newPin = nf.value();
      if (c.revalue) body.value = val.value;
      post(body).then(done);
    };
    open = { root: f.root, done: o.onDone || null, onKey: function (e) { if (e.key === 'Escape') closeDialog(); } };
    document.addEventListener('keydown', open.onKey);
    sync();
    setTimeout(function () { name.focus(); }, 30);
  }

  var cssDone = false;
  function injectCss() {
    if (cssDone || !document.head) return;
    cssDone = true;
    var s = document.createElement('style');
    s.textContent = [
      '.csl-scrim{position:fixed;inset:0;z-index:650;background:rgba(0,0,0,0.55);display:flex;align-items:center;justify-content:center;padding:16px}',
      '.csl-card{width:100%;max-width:420px;max-height:calc(100vh - 32px);overflow:auto;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius,10px);box-shadow:var(--shadow-lg,0 10px 30px rgba(0,0,0,.4));padding:16px 18px;display:flex;flex-direction:column;gap:9px;color:var(--text-primary)}',
      '.csl-head{display:flex;align-items:center;gap:8px}',
      '.csl-lock{display:inline-flex;color:var(--accent-hover,var(--accent))}',
      '.csl-ic{width:16px;height:16px;display:block;pointer-events:none}',
      '.csl-title{flex:1;font-weight:650;font-size:15px}',
      '.csl-name{font-family:var(--mono,monospace);font-size:13px;color:var(--accent-hover,var(--accent));word-break:break-word}',
      '.csl-text{font-size:13px;line-height:1.45;color:var(--text-primary)}',
      '.csl-sec{font-size:11.5px;font-weight:650;text-transform:uppercase;letter-spacing:.04em;color:var(--text-secondary);margin-top:4px}',
      '.csl-sub{display:flex;flex-direction:column;gap:9px}',
      '.csl-in{width:100%;box-sizing:border-box;padding:8px 10px;border-radius:var(--radius-sm,6px);border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font:inherit;font-size:14px}',
      'textarea.csl-in{min-height:58px;resize:vertical}',
      '.csl-in:focus{outline:none;border-color:var(--accent)}',
      '.csl-mono{font-family:var(--mono,monospace);font-size:13px}',
      '.csl-pinbox{font-family:var(--mono,monospace);letter-spacing:.3em;text-align:center}',
      '.csl-hint{font-size:12px;color:var(--text-secondary);line-height:1.4}',
      '.csl-hint.csl-warn{color:var(--yellow,#d69e2e)}',
      '.csl-check{display:flex;align-items:center;gap:8px;font-size:13px;cursor:pointer}',
      '.csl-err{font-size:12.5px;color:var(--red)}',
      '.csl-err:empty{display:none}',
      '.csl-right{display:flex;justify-content:flex-end;gap:8px;margin-top:4px}',
      '.csl-btn{padding:7px 14px;border-radius:var(--radius-sm,6px);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:13px;cursor:pointer}',
      '.csl-btn:disabled{opacity:.55;cursor:default}',
      '.csl-go{background:var(--accent);border-color:var(--accent);color:#fff}',
      // A passphrase in a field drawn for 6 spaced-out digits.
      'input.csl-pass{letter-spacing:normal!important;font-family:inherit}',
    ].join('');
    document.head.appendChild(s);
  }

  window.CrundiSecretLock = {
    RULE: RULE, MIN: MIN, UNSUPPORTED: UNSUPPORTED,
    configure: configure, supports: supports, knows: knows,
    passphraseOk: passphraseOk, lockOk: lockOk, kindOf: kindOf, askText: askText,
    field: field, filter: filter,
    upgrade: upgrade, edit: edit, close: closeDialog,
  };
})();
