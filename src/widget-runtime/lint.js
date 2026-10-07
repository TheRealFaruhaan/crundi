/* eslint-disable */
/**
 * Crundi widget lint. Runs inside the frame, only when rendered by the test
 * harness, and answers the questions a screenshot leaves open: does anything
 * spill sideways, is text cut off, can a finger hit the buttons, is the text
 * readable against what is behind it.
 *
 * Browser code, plain script. Sets window.__crundiLint(crundi) -> report.
 */
(function () {
  'use strict';

  function sel(el) {
    if (!el || el === document.body) return 'body';
    var s = el.tagName.toLowerCase();
    if (el.id) return s + '#' + el.id;
    var c = (el.getAttribute('class') || '').trim().split(/\s+/).filter(Boolean).slice(0, 2).join('.');
    if (c) s += '.' + c;
    var t = (el.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 24);
    return t ? s + ' "' + t + '"' : s;
  }

  function visible(el) {
    var r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) return false;
    var cs = getComputedStyle(el);
    return cs.visibility !== 'hidden' && cs.display !== 'none' && Number(cs.opacity) > 0.05;
  }

  function hasOwnText(el) {
    for (var n = el.firstChild; n; n = n.nextSibling) if (n.nodeType === 3 && n.nodeValue.trim().length > 1) return true;
    return false;
  }

  function parseColor(str) {
    var m = /rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:[,\/\s]+([\d.]+%?))?\s*\)/.exec(str || '');
    if (!m) return null;
    var a = m[4] === undefined ? 1 : (m[4].slice(-1) === '%' ? parseFloat(m[4]) / 100 : parseFloat(m[4]));
    return [Number(m[1]), Number(m[2]), Number(m[3]), a];
  }
  function over(top, base) {
    var a = top[3];
    return [top[0] * a + base[0] * (1 - a), top[1] * a + base[1] * (1 - a), top[2] * a + base[2] * (1 - a), 1];
  }
  function lum(c) {
    var f = function (v) { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]);
  }
  function ratio(a, b) { var x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); }

  /** The colour actually behind an element, or null if a gradient/image makes it unknowable. */
  function backdrop(el) {
    var stack = [];
    for (var n = el; n && n.nodeType === 1; n = n.parentElement) {
      var cs = getComputedStyle(n);
      if (cs.backgroundImage && cs.backgroundImage !== 'none') return null;
      var c = parseColor(cs.backgroundColor);
      if (c && c[3] > 0) { stack.push(c); if (c[3] >= 0.99) break; }
    }
    // The host paints the frame's surround; the widget's own page colour is the floor.
    var base = parseColor(getComputedStyle(document.documentElement).getPropertyValue('--bg-primary-rgb')) || [10, 10, 15, 1];
    for (var i = stack.length - 1; i >= 0; i--) base = over(stack[i], base);
    return base;
  }

  window.__crundiLint = function (crundi) {
    var issues = [];
    var add = function (level, rule, message, nodes) { issues.push({ level: level, rule: rule, message: message, nodes: (nodes || []).slice(0, 5) }); };
    var doc = document.documentElement, body = document.body;
    var vw = doc.clientWidth, vh = window.innerHeight;
    var mobile = (crundi.context || {}).platform === 'mobile';
    var all = Array.prototype.slice.call(body.querySelectorAll('*')).filter(function (el) {
      var t = el.tagName; return t !== 'SCRIPT' && t !== 'STYLE' && t !== 'TEMPLATE';
    });
    var shown = all.filter(visible);
    // Text drawn inside an SVG is sized in the drawing's own units, not CSS pixels.
    var htmlText = shown.filter(function (el) { return !el.closest('svg') && hasOwnText(el); });

    // 1. Nothing drawn.
    var text = (body.innerText || '').trim();
    var hasGraphic = !!body.querySelector('svg, canvas, img');
    if (!text && !hasGraphic) add('error', 'empty', 'The widget rendered nothing visible. If it waits for data, draw an empty state instead of a blank frame.');

    // 2. Sideways overflow: the commonest way a widget breaks in a narrow slot.
    if (doc.scrollWidth > vw + 1 || body.scrollWidth > vw + 1) {
      var wide = shown.filter(function (el) {
        var r = el.getBoundingClientRect();
        if (r.right <= vw + 1) return false;
        // Inside something that scrolls on purpose is fine.
        for (var p = el.parentElement; p && p !== body; p = p.parentElement) {
          var o = getComputedStyle(p).overflowX;
          if (o === 'auto' || o === 'scroll' || o === 'hidden') return false;
        }
        return true;
      });
      add('error', 'overflow-x', 'Content is ' + Math.max(doc.scrollWidth, body.scrollWidth) + 'px wide in a ' + vw + 'px frame, so it spills sideways. Let it wrap, shrink (min-width: 0 on flex children), or put wide tables in .c-table-wrap.', wide.map(sel));
    }

    // 3. Text cut off without an ellipsis.
    var clipped = shown.filter(function (el) {
      if (!hasOwnText(el)) return false;
      var cs = getComputedStyle(el);
      if (cs.overflowX !== 'hidden' && cs.overflow !== 'hidden') return false;
      if (cs.textOverflow === 'ellipsis') return false;
      return el.scrollWidth > el.clientWidth + 1;
    });
    if (clipped.length) add('warn', 'clipped-text', clipped.length + ' element(s) have text cut off with no ellipsis.', clipped.map(sel));

    // 4. Text too small to read.
    var tiny = htmlText.filter(function (el) { return parseFloat(getComputedStyle(el).fontSize) < 11; });
    if (tiny.length) add('warn', 'small-text', tiny.length + ' element(s) use text under 11px.', tiny.map(sel));

    // 5. Tap targets, where a finger is the pointer.
    if (mobile) {
      var targets = shown.filter(function (el) {
        var t = el.tagName;
        if (!(t === 'BUTTON' || t === 'SELECT' || t === 'INPUT' || t === 'TEXTAREA' || el.getAttribute('role') === 'button' || el.hasAttribute('onclick') || (t === 'A' && getComputedStyle(el).display !== 'inline'))) return false;
        var r = el.getBoundingClientRect();
        return r.height < 40 || r.width < 40;
      });
      if (targets.length) add('warn', 'tap-target', targets.length + ' control(s) are smaller than 40px on a touch screen. .c-btn grows on mobile by itself; custom controls need min-height: 44px.', targets.map(function (el) { var r = el.getBoundingClientRect(); return sel(el) + ' (' + Math.round(r.width) + 'x' + Math.round(r.height) + ')'; }));
    }

    // 6. Contrast.
    var low = [];
    htmlText.forEach(function (el) {
      if (low.length > 20) return;
      var cs = getComputedStyle(el);
      var fg = parseColor(cs.color); if (!fg) return;
      var bg = backdrop(el); if (!bg) return;
      var c = ratio(over(fg, bg), bg);
      var size = parseFloat(cs.fontSize), bold = parseInt(cs.fontWeight, 10) >= 600;
      var need = size >= 18 || (size >= 14 && bold) ? 3 : 4.5;
      if (c < need - 0.05) low.push(sel(el) + ' (' + c.toFixed(1) + ':1, needs ' + need + ')');
    });
    if (low.length) add('warn', 'contrast', low.length + ' text element(s) are hard to read against their background (--text-muted is for decoration, not for words that matter).', low);

    // 7. Colours typed in by hand instead of taken from the tokens.
    var raw = [];
    var styles = document.querySelectorAll('style:not([data-crundi])');
    for (var i = 0; i < styles.length; i++) {
      var css = styles[i].textContent.replace(/\/\*[\s\S]*?\*\//g, '');
      var m = css.match(/#[0-9a-fA-F]{3,8}\b|rgba?\([^)]*\)|hsla?\([^)]*\)/g);
      if (m) raw = raw.concat(m);
    }
    shown.forEach(function (el) {
      var st = el.getAttribute('style');
      if (!st) return;
      var m = st.match(/#[0-9a-fA-F]{3,8}\b|rgba?\([^)]*\)|hsla?\([^)]*\)/g);
      if (m) raw = raw.concat(m);
    });
    raw = raw.filter(function (c) { return !/^#(fff|ffffff|000|000000)$/i.test(c); });
    if (raw.length) {
      var uniq = raw.filter(function (c, i) { return raw.indexOf(c) === i; });
      add('info', 'raw-colors', uniq.length + ' colour(s) are hard-coded (' + uniq.slice(0, 4).join(', ') + '). Use the tokens (var(--accent), var(--green), var(--text-secondary)…) so the widget matches Crundi.');
    }

    // 8. Data that failed to arrive.
    var errs = crundi.errors || {};
    for (var name in errs) {
      if (!Object.prototype.hasOwnProperty.call(errs, name)) continue;
      add(errs[name] === 'needs-approval' ? 'info' : 'error', 'source', 'Source "' + name + '": ' + (errs[name] === 'needs-approval' ? 'waiting for the owner to approve it (shown on the widget).' : errs[name]));
    }

    // 9. Height, as information: tall content scrolls in a cell, and that is often fine.
    var docH = Math.max(body.scrollHeight, doc.scrollHeight);
    if (docH > vh * 2.5 && vh > 0) add('info', 'tall', 'Content is ' + docH + 'px tall in a ' + vh + 'px frame (it scrolls). Put what matters most at the top.');

    return {
      issues: issues,
      metrics: { frameWidth: vw, frameHeight: vh, contentWidth: Math.max(doc.scrollWidth, body.scrollWidth), contentHeight: docH, elements: shown.length, textLength: text.length }
    };
  };
})();
