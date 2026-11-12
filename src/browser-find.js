/**
 * browser-find.js — find an element on a page the way a person would point at
 * it (its visible text, its role, inside the dialog that is open) and describe
 * what is there, for browser_click and Cru's browser tools.
 *
 * Everything here is a script run through browser_eval, so it works on every
 * desktop app version and the headless server browser alike.
 *
 *   findScript(opts)      → JSON { ok, x, y, tag, text, role, matches, inDialog, selector? } | { ok: false, error, near? }
 *   pointScript(x, y)     → JSON describing the element at that point
 *   clickablesScript(max) → JSON [{ text, tag, role, x, y }] of what can be clicked, on screen first
 *
 * opts: { selector?, text?, role?, within?, exact?, nth?, click? }
 *   text     matched against the element's visible text, aria-label, value,
 *            title or alt (case and spacing ignored); an exact match beats a
 *            partial one
 *   role     button | link | checkbox | radio | tab | menuitem | option |
 *            textbox | combobox | switch (or a tag name)
 *   within   a CSS selector to search inside; without one, an open dialog is
 *            searched first (where a person would look), then the page
 *   nth      which match (0 = the first), when several are equally good
 *   click    el.click() it right away (for a selector or text match), after
 *            scrolling it into view
 */

const ROLES = ['button', 'link', 'checkbox', 'radio', 'tab', 'menuitem', 'menuitemcheckbox', 'menuitemradio', 'option', 'switch', 'combobox', 'textbox', 'searchbox', 'slider', 'treeitem'];
const CLICKABLE = 'a[href],button,input:not([type=hidden]),select,textarea,summary,label,option,[onclick],[tabindex]:not([tabindex="-1"]),[contenteditable=""],[contenteditable=true],'
  + ROLES.map((r) => `[role=${r}]`).join(',');
const DIALOG = 'dialog[open],[role=dialog],[role=alertdialog],[aria-modal=true],.modal.show,.modal.open,.modal.is-open';

// Shared in-page helpers (plain ES5 so any page runs it).
const HELPERS = `
  function shown(el) {
    if (!el || !el.getClientRects || !el.getClientRects().length) return false;
    var s = getComputedStyle(el);
    if (s.visibility === 'hidden' || s.display === 'none' || Number(s.opacity) === 0) return false;
    var r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0;
  }
  function norm(s) { return String(s || '').replace(/\\s+/g, ' ').trim().toLowerCase(); }
  // Everything a person might call it: its visible text, aria-label, title, value, alt, placeholder.
  function names(el) {
    var out = [el.innerText || el.textContent || '', el.getAttribute('aria-label'), el.getAttribute('title'), el.value, el.getAttribute('alt'), el.getAttribute('placeholder')];
    var img = el.querySelector && el.querySelector('img[alt],svg[aria-label]'); if (img) out.push(img.getAttribute('alt') || img.getAttribute('aria-label'));
    return out.map(norm).filter(function (x) { return x; });
  }
  function matches(el, want, exact) { var n = names(el); for (var i = 0; i < n.length; i++) { if (exact ? n[i] === want : n[i].indexOf(want) >= 0) return true; } return false; }
  function exactly(el, want) { return names(el).indexOf(want) >= 0; }
  function label(el) {
    var t = el.innerText || el.textContent || '';
    if (!norm(t)) t = el.getAttribute('aria-label') || el.value || el.getAttribute('title') || el.getAttribute('alt') || el.getAttribute('placeholder') || '';
    if (!norm(t)) { var img = el.querySelector && el.querySelector('img[alt],svg[aria-label]'); if (img) t = img.getAttribute('alt') || img.getAttribute('aria-label') || ''; }
    return String(t).replace(/\\s+/g, ' ').trim();
  }
  function roleOf(el) {
    var r = el.getAttribute('role'); if (r) return r;
    var t = el.tagName.toLowerCase(), ty = (el.getAttribute('type') || '').toLowerCase();
    if (t === 'a' && el.hasAttribute('href')) return 'link';
    if (t === 'button' || t === 'summary' || (t === 'input' && /^(button|submit|reset|image)$/.test(ty))) return 'button';
    if (t === 'input' && ty === 'checkbox') return 'checkbox';
    if (t === 'input' && ty === 'radio') return 'radio';
    if (t === 'select') return 'combobox';
    if (t === 'textarea' || t === 'input') return 'textbox';
    if (t === 'option') return 'option';
    return t;
  }
  function describe(el, x, y) {
    var r = el.getBoundingClientRect();
    var o = { tag: el.tagName.toLowerCase(), role: roleOf(el), text: label(el).slice(0, 120),
      x: Math.round(x != null ? x : r.left + r.width / 2), y: Math.round(y != null ? y : r.top + r.height / 2) };
    if (el.id) o.id = el.id;
    var href = el.getAttribute && el.getAttribute('href'); if (href) o.href = href.slice(0, 200);
    var d = el.closest && el.closest(${JSON.stringify(DIALOG)}); if (d) o.inDialog = true;
    return o;
  }
  // The element a person would say they clicked: the nearest clickable around the target.
  function clickableOf(el) { return (el && el.closest && el.closest(${JSON.stringify(CLICKABLE)})) || el; }
  function dialogs() { return Array.prototype.filter.call(document.querySelectorAll(${JSON.stringify(DIALOG)}), shown); }
`;

export function findScript(opts = {}) {
  return `(function (o) {${HELPERS}
  var scopes = [];
  if (o.within) {
    var w = Array.prototype.filter.call(document.querySelectorAll(o.within), shown);
    if (!w.length) return JSON.stringify({ ok: false, error: 'Nothing on the page matches within: ' + o.within });
    scopes.push(w);
  } else {
    var ds = dialogs(); if (ds.length) scopes.push([ds[ds.length - 1]]);
    scopes.push([document]);
  }
  var want = norm(o.text), role = norm(o.role);
  var best = null, lastScope = null;
  for (var si = 0; si < scopes.length && !best; si++) {
    var pool = [];
    scopes[si].forEach(function (root) {
      var list = o.selector ? root.querySelectorAll(o.selector) : root.querySelectorAll(${JSON.stringify(CLICKABLE)});
      Array.prototype.forEach.call(list, function (el) { if (pool.indexOf(el) < 0) pool.push(el); });
    });
    // Text with no clickable around it (a styled div): fall back to any element with that text.
    var cands = pool.filter(function (el) {
      if (!shown(el)) return false;
      if (role && roleOf(el) !== role && el.tagName.toLowerCase() !== role) return false;
      if (!want) return true;
      return matches(el, want, o.exact);
    });
    if (!cands.length && want && !o.selector) {
      scopes[si].forEach(function (root) {
        var all = root.querySelectorAll('*');
        Array.prototype.forEach.call(all, function (el) {
          if (el.children.length || !shown(el)) return;
          var l = norm(el.textContent); if (o.exact ? l === want : l.indexOf(want) >= 0) { var c = clickableOf(el); if (cands.indexOf(c) < 0) cands.push(c); }
        });
      });
    }
    if (want) {
      // Exact text first, then the shortest label (the most specific element).
      cands.sort(function (a, b) {
        var ea = exactly(a, want) ? 0 : 1, eb = exactly(b, want) ? 0 : 1;
        return ea - eb || label(a).length - label(b).length;
      });
    }
    if (cands.length) { best = cands; lastScope = si; }
  }
  if (!best) {
    var near = [];
    var all2 = Array.prototype.filter.call(document.querySelectorAll(${JSON.stringify(CLICKABLE)}), shown).slice(0, 400);
    all2.forEach(function (el) { var l = label(el); if (l && near.length < 12 && near.indexOf(l.slice(0, 60)) < 0) near.push(l.slice(0, 60)); });
    return JSON.stringify({ ok: false, error: 'Nothing visible matches' + (o.text ? ' the text "' + o.text + '"' : '') + (o.role ? ' as a ' + o.role : '') + (o.selector ? ' ' + o.selector : ''), near: near });
  }
  var n = Math.max(0, Math.min(best.length - 1, Number(o.nth) || 0));
  var el = best[n];
  try { el.scrollIntoView({ block: 'center', inline: 'center' }); } catch (e) { /* old */ }
  var out = describe(el);
  out.ok = true; out.matches = best.length; out.inDialog = !!out.inDialog; out.searched = (o.within ? 'within ' + o.within : lastScope === 0 && scopes.length > 1 ? 'the open dialog' : 'the page');
  // What a person's click there would actually land on (an overlay shows up here).
  var top = document.elementFromPoint(out.x, out.y);
  if (top && top !== el && !el.contains(top) && !top.contains(el)) { out.covered = describe(clickableOf(top)); }
  if (o.click) { el.click(); out.clicked = true; }
  return JSON.stringify(out);
})(${JSON.stringify({
    selector: opts.selector || null, text: opts.text || null, role: opts.role || null, within: opts.within || null,
    exact: !!opts.exact, nth: Number.isFinite(Number(opts.nth)) ? Number(opts.nth) : 0, click: !!opts.click,
  })})`;
}

export function pointScript(x, y) {
  return `(function (x, y) {${HELPERS}
  var el = document.elementFromPoint(x, y);
  if (!el) return JSON.stringify({ ok: false, error: 'Nothing is at ' + x + ',' + y + ' (outside the page?)' });
  if (el.tagName === 'IFRAME') { var r = el.getBoundingClientRect(); return JSON.stringify({ ok: true, tag: 'iframe', role: 'iframe', text: (el.getAttribute('title') || el.src || '').slice(0, 120), x: x, y: y, frame: { src: (el.src || '').slice(0, 200), left: Math.round(r.left), top: Math.round(r.top) } }); }
  var o = describe(clickableOf(el), x, y); o.ok = true; return JSON.stringify(o);
})(${Number(x) || 0}, ${Number(y) || 0})`;
}

export function clickablesScript(max = 60) {
  return `(function (max) {${HELPERS}
  var vw = innerWidth, vh = innerHeight, seen = [], out = [];
  var ds = dialogs(); var root = ds.length ? ds[ds.length - 1] : document;
  Array.prototype.forEach.call(root.querySelectorAll(${JSON.stringify(CLICKABLE)}), function (el) {
    if (!shown(el)) return;
    var d = describe(el);
    if (!d.text && d.role !== 'textbox' && d.role !== 'checkbox' && d.role !== 'radio') return;
    var key = d.role + '|' + d.text; if (seen.indexOf(key) >= 0) return; seen.push(key);
    d.onScreen = d.x >= 0 && d.y >= 0 && d.x <= vw && d.y <= vh;
    out.push(d);
  });
  out.sort(function (a, b) { return (b.onScreen ? 1 : 0) - (a.onScreen ? 1 : 0) || a.y - b.y || a.x - b.x; });
  return JSON.stringify({ dialog: ds.length > 0, viewport: [vw, vh], items: out.slice(0, max) });
})(${Number(max) || 60})`;
}

/** One line per element, for a chat or Cru to read: [button] "Add to Shopping List" at 640,712. */
export function formatClickables(r) {
  if (!r || !Array.isArray(r.items)) return '';
  const lines = r.items.map((d) => `[${d.role}] ${d.text ? JSON.stringify(d.text.slice(0, 70)) : '(no text)'} at ${d.x},${d.y}${d.onScreen ? '' : ' (scroll to it)'}${d.href ? ' -> ' + d.href.slice(0, 60) : ''}`);
  return (r.dialog ? 'A dialog is open; what it holds:\n' : '') + lines.join('\n');
}
