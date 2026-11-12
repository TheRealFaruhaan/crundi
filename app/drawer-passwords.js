/**
 * drawer-passwords.js — saved passwords for the browser drawer, the way
 * Chrome keeps them: on this computer only, each password encrypted with the
 * operating system's protection for this user (Electron safeStorage: DPAPI on
 * Windows, the Keychain on macOS, the desktop keyring on Linux). Filling needs
 * no prompt; showing a password in the list asks for the owner's PIN (the
 * drawer page checks it with the server, /api/lock/check).
 *
 * Nothing here reaches the server, and no chat can read the list, fill a
 * login, or see a password through the browser tools (browser-drawer.js
 * redacts a filled password from what a chat reads back).
 *
 * Also the two scripts that run in the drawer's pages, in an isolated world of
 * their own (the page's scripts cannot see or call into it):
 *   captureScript() notices a login form and a login being submitted, and
 *                   reports them as console messages with a prefix that is
 *                   random for each run of the app (the page cannot see this
 *                   world's code, so cannot forge one); the desktop app takes
 *                   those out of the tab's console before anything reads it.
 *                   Not a CDP binding: that needs Runtime.enable, which
 *                   anti-bot scripts detect.
 *   fillScript()    fills a saved login into the page, if it is still on the
 *                   saved site
 *
 * Pure apart from the file and the two crypto callbacks: the e2e harness and
 * the unit test run the same code.
 */

import { readFileSync, writeFileSync, renameSync } from 'node:fs';
import { randomUUID } from 'node:crypto';

export const WORLD = 'crundi-pw';

/** The site a login belongs to: scheme, host and port (exactly, as Chrome does). */
export function originOf(url) {
  try {
    const u = new URL(String(url || ''));
    if (u.protocol !== 'https:' && u.protocol !== 'http:') return null;
    return u.origin;
  } catch { return null; }
}

/**
 * store({ file, encrypt(str) → Buffer, decrypt(Buffer) → str, backend() → { ok, reason } })
 */
export function createPasswordStore({ file, encrypt, decrypt, backend }) {
  let data = null;
  function load() {
    if (data) return data;
    try {
      const d = JSON.parse(readFileSync(file, 'utf-8'));
      data = { logins: Array.isArray(d.logins) ? d.logins : [], never: Array.isArray(d.never) ? d.never : [] };
    } catch { data = { logins: [], never: [] }; }
    return data;
  }
  function persist() {
    const tmp = file + '.tmp';
    writeFileSync(tmp, JSON.stringify({ v: 1, logins: data.logins, never: data.never }), { mode: 0o600 });
    renameSync(tmp, file);
  }
  const pub = (l) => ({ id: l.id, origin: l.origin, username: l.username, created: l.created, updated: l.updated, used: l.used || 0 });
  const status = () => { const b = backend(); return { available: !!b.ok, reason: b.ok ? '' : b.reason }; };
  function need() { const b = backend(); if (!b.ok) throw new Error(b.reason); }

  return {
    status,
    /** Every saved login (no passwords), or one site's. */
    list(origin) {
      const all = load().logins;
      return (origin ? all.filter((l) => l.origin === origin) : all).map(pub)
        .sort((a, b) => a.origin.localeCompare(b.origin) || a.username.localeCompare(b.username));
    },
    count(origin) { return origin ? load().logins.filter((l) => l.origin === origin).length : 0; },
    /** What saving this would do: 'new', 'update' (another password for that user), 'same' or 'never'. */
    check(origin, username, password) {
      const d = load();
      if (d.never.includes(origin)) return 'never';
      const l = d.logins.find((x) => x.origin === origin && x.username === username);
      if (!l) return 'new';
      try { return decrypt(Buffer.from(l.pw, 'base64')) === password ? 'same' : 'update'; } catch { return 'update'; }
    },
    save({ origin, username, password }) {
      need();
      if (!originOf(origin) || originOf(origin) !== origin) throw new Error('not a site');
      if (typeof password !== 'string' || !password) throw new Error('no password');
      const d = load();
      const pw = encrypt(password).toString('base64');
      const now = Date.now();
      const l = d.logins.find((x) => x.origin === origin && x.username === String(username || ''));
      if (l) { l.pw = pw; l.updated = now; }
      else d.logins.push({ id: randomUUID(), origin, username: String(username || ''), pw, created: now, updated: now, used: 0 });
      d.never = d.never.filter((o) => o !== origin);
      persist();
      return pub(l || d.logins[d.logins.length - 1]);
    },
    /** The password itself: to fill it, or to show it once the PIN was given. */
    password(id) {
      need();
      const l = load().logins.find((x) => x.id === id);
      if (!l) throw new Error('No such saved login');
      return { origin: l.origin, username: l.username, password: decrypt(Buffer.from(l.pw, 'base64')) };
    },
    used(id) { const l = load().logins.find((x) => x.id === id); if (l) { l.used = Date.now(); persist(); } },
    update(id, { username, password } = {}) {
      need();
      const d = load(); const l = d.logins.find((x) => x.id === id);
      if (!l) throw new Error('No such saved login');
      if (typeof username === 'string') {
        if (d.logins.some((x) => x !== l && x.origin === l.origin && x.username === username)) throw new Error('That site already has a login with this username');
        l.username = username;
      }
      if (typeof password === 'string' && password) l.pw = encrypt(password).toString('base64');
      l.updated = Date.now();
      persist();
      return pub(l);
    },
    remove(id) { const d = load(); const n = d.logins.length; d.logins = d.logins.filter((x) => x.id !== id); if (d.logins.length !== n) persist(); return d.logins.length !== n; },
    never(origin) { const d = load(); if (!d.never.includes(origin)) { d.never.push(origin); persist(); } },
    neverList() { return load().never.slice().sort(); },
    unnever(origin) { const d = load(); d.never = d.never.filter((o) => o !== origin); persist(); },
    /** Everything gone (Clear browsing data, "Passwords"). */
    clear() { data = { logins: [], never: [] }; persist(); },
  };
}

/** Is this safeStorage good enough to keep passwords with? */
export function safeStorageBackend(safeStorage, platform = process.platform) {
  try {
    if (!safeStorage || !safeStorage.isEncryptionAvailable()) return { ok: false, reason: 'This computer offers no encrypted storage for this app, so passwords cannot be saved.' };
    if (platform === 'linux' && typeof safeStorage.getSelectedStorageBackend === 'function') {
      const b = safeStorage.getSelectedStorageBackend();
      if (b === 'basic_text' || b === 'unknown') return { ok: false, reason: 'No desktop keyring (GNOME Keyring or KWallet) is running, so passwords could not be kept safely and are not saved.' };
    }
    return { ok: true };
  } catch (err) { return { ok: false, reason: err.message }; }
}

// ─── In the page (an isolated world; see the header) ───

/**
 * Reports, as console.debug(prefix + JSON), with the frame's origin and
 * whether it is the top frame:
 *   { t: 'page', hasLogin }     whether a login form is on the page, at load
 *                               and whenever that changes (a form shown late)
 *   { t: 'submit', username, password }
 *                               a login being submitted: a form submit, a click
 *                               on a button while a password is filled, or
 *                               Enter in a password field. A sign-up or
 *                               change-password form (two or three password
 *                               fields) gives the last one, the new password.
 *                               The username is the nearest filled text, email
 *                               or phone field before the password field.
 *   { t: 'after', stillForm }   up to 8 s after a submit on the same page:
 *                               did the login form go away (it worked) or is
 *                               it still there (most likely a wrong password)?
 */
export function captureScript(prefix) {
  return `(function () {
  if (window.__crundiPwOn) return; window.__crundiPwOn = true;
  var prefix = ${JSON.stringify(prefix)}, debug = console.debug.bind(console);
  function send(o) { o.origin = location.origin; o.top = window === window.top; try { debug(prefix + JSON.stringify(o)); } catch (e) { /* gone */ } }
  function shown(el) { try { return el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden'; } catch (e) { return false; } }
  function pwFields(root) { return Array.prototype.filter.call(root.querySelectorAll('input[type=password]'), shown); }
  function grab(scope) {
    var root = scope && scope.querySelectorAll ? scope : document;
    var pws = pwFields(root).filter(function (p) { return p.value; });
    if (!pws.length && root !== document) pws = pwFields(document).filter(function (p) { return p.value; });
    if (!pws.length) return null;
    var pw = pws[pws.length - 1];
    var box = pw.form || document;
    var users = Array.prototype.filter.call(box.querySelectorAll('input'), function (i) {
      var t = (i.getAttribute('type') || 'text').toLowerCase();
      return (t === 'text' || t === 'email' || t === 'tel') && i.value && shown(i);
    });
    var user = '';
    users.forEach(function (i) { if (i.compareDocumentPosition(pw) & 4) user = i.value; });
    if (!user && users.length) user = users[0].value;
    return { username: String(user).slice(0, 300), password: String(pw.value).slice(0, 1000) };
  }
  var told = null;
  function look() { var has = pwFields(document).length > 0; if (has !== told) { told = has; send({ t: 'page', hasLogin: has }); } }
  var last = '', watching = null;
  function after() {
    clearInterval(watching); var n = 0;
    watching = setInterval(function () {
      n++; var still = pwFields(document).length > 0;
      if (!still || n >= 16) { clearInterval(watching); watching = null; send({ t: 'after', stillForm: still }); }
    }, 500);
  }
  function maybe(scope) {
    var c = grab(scope); if (!c) return;
    var k = c.username + '\\u0000' + c.password; if (k === last) return; last = k;
    c.t = 'submit'; send(c); after();
  }
  document.addEventListener('submit', function (e) { maybe(e.target); }, true);
  document.addEventListener('click', function (e) {
    var b = e.target && e.target.closest && e.target.closest('button, input[type=submit], input[type=button], [role=button]');
    if (b) setTimeout(function () { maybe(b.form || null); }, 0);
  }, true);
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Enter' && e.target && String(e.target.type).toLowerCase() === 'password') maybe(e.target.form || null);
  }, true);
  var moT = null;
  function watchForms() {
    try {
      new MutationObserver(function () { if (moT) return; moT = setTimeout(function () { moT = null; look(); }, 400); })
        .observe(document.documentElement || document, { childList: true, subtree: true, attributes: true, attributeFilter: ['type', 'style', 'class', 'hidden'] });
    } catch (e) { /* no DOM yet */ }
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', function () { look(); watchForms(); });
  else { look(); watchForms(); }
})();`;
}

/**
 * Fill a login into the page: the password field, and the username field
 * before it (when there is one). Only while the page is still on that site.
 * Sets values the way typing would, so frameworks (React, Vue) see them.
 */
export function fillScript(origin, username, password) {
  return `(function (o, u, p) {
  if (location.origin !== o) return { ok: false, error: 'The page is no longer on that site' };
  function shown(el) { try { return el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden' && !el.disabled && !el.readOnly; } catch (e) { return false; } }
  var pw = Array.prototype.filter.call(document.querySelectorAll('input[type=password]'), shown)[0] || null;
  function set(el, v) {
    var proto = Object.getPrototypeOf(el);
    var d = Object.getOwnPropertyDescriptor(proto, 'value') || Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
    el.focus();
    d.set.call(el, v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }
  var box = (pw && pw.form) || document;
  var users = Array.prototype.filter.call(box.querySelectorAll('input'), function (i) {
    var t = (i.getAttribute('type') || 'text').toLowerCase();
    return (t === 'text' || t === 'email' || t === 'tel') && shown(i);
  });
  var user = null;
  if (pw) users.forEach(function (i) { if (i.compareDocumentPosition(pw) & 4) user = i; });
  if (!user) user = users.filter(function (i) { return /user|email|login|account|name|phone/i.test((i.name || '') + ' ' + (i.id || '') + ' ' + (i.autocomplete || '') + ' ' + (i.placeholder || '')); })[0] || null;
  if (!pw && !user) return { ok: false, error: 'There is no login form on this page' };
  if (user && u) set(user, u);
  if (pw) set(pw, p);
  return { ok: true, user: !!user, password: !!pw };
})(${JSON.stringify(origin)}, ${JSON.stringify(username)}, ${JSON.stringify(password)})`;
}

/**
 * Take every filled password out of what a chat reads back from a tab (eval
 * results, page text, HTML, console, network): replaced by ••••••••. Binary
 * fields (screenshots, PDFs) are left alone; a password field shows dots in a
 * picture anyway.
 */
export function redactSecrets(value, secrets) {
  const list = [...(secrets || [])].filter((s) => typeof s === 'string' && s.length >= 4).sort((a, b) => b.length - a.length);
  if (!list.length) return value;
  const BINARY = new Set(['data', 'image', 'png', 'pdf']);
  const fix = (s) => { let out = s; for (const x of list) if (out.includes(x)) out = out.split(x).join('••••••••'); return out; };
  const walk = (v, key) => {
    if (typeof v === 'string') return BINARY.has(key) ? v : fix(v);
    if (Array.isArray(v)) return v.map((x) => walk(x, ''));
    if (v && typeof v === 'object') { const o = {}; for (const [k, x] of Object.entries(v)) o[k] = walk(x, k); return o; }
    return v;
  };
  return walk(value, '');
}
