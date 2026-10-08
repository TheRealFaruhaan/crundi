/* eslint-disable */
/**
 * Crundi widget runtime. Injected ahead of a widget's own markup, inside a
 * sandboxed frame with an opaque origin and no network. Everything the widget
 * knows about the outside world arrives through window.crundi.
 *
 * Browser code: plain script, no imports, no build step. Kept in its own file
 * so `node --check` reads it; the server inlines it into each frame.
 */
(function () {
  'use strict';
  var BOOT = window.__CRUNDI_BOOT__ || {};
  // The top bar chip is the same panel in a frame 24px tall, showing only the
  // element marked data-chip. Which copy this is has to be known before the
  // body is parsed, or the whole panel flashes into that sliver, so the server
  // builds the chip's document saying so (it also sets data-frame on <html>).
  // The frame's name was tried first and was not reliable: a sandboxed frame
  // does not always keep it across the load.
  var IS_CHIP = !!BOOT.chip;
  if (IS_CHIP) document.documentElement.setAttribute('data-frame', 'chipbar');
  var host = window.parent;
  var seq = 0;
  var waiting = {};
  var handlers = { data: [], context: [], ready: [], store: [] };
  var started = false;
  var faultCount = 0;

  function post(msg) {
    msg.crundi = 1;
    try { host.postMessage(msg, '*'); } catch (e) { /* host gone */ }
  }

  function call(op, args) {
    return new Promise(function (resolve, reject) {
      var id = ++seq;
      waiting[id] = { resolve: resolve, reject: reject };
      post({ t: 'call', seq: id, op: op, args: args === undefined ? null : args });
      setTimeout(function () {
        if (!waiting[id]) return;
        delete waiting[id];
        reject(new Error('Crundi did not answer "' + op + '" in time'));
      }, 30000);
    });
  }

  function fault(message, where) {
    if (faultCount++ > 20) return;
    post({ t: 'fault', message: String(message || 'Unknown error').slice(0, 500), where: String(where || '').slice(0, 120) });
  }

  window.addEventListener('error', function (e) {
    fault(e.message || (e.error && e.error.message), (e.lineno ? 'line ' + e.lineno + ':' + e.colno : ''));
  });
  window.addEventListener('unhandledrejection', function (e) {
    var r = e.reason;
    fault('Unhandled promise rejection: ' + (r && r.message ? r.message : String(r)));
  });

  // ── small helpers ──

  function get(obj, path, dflt) {
    if (path == null || path === '') return obj === undefined ? dflt : obj;
    var parts = String(path).split('.');
    var cur = obj;
    for (var i = 0; i < parts.length; i++) {
      if (cur == null) return dflt;
      cur = cur[parts[i]];
    }
    return cur === undefined || cur === null ? dflt : cur;
  }

  function esc(s) {
    return String(s == null ? '' : s).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }

  /** h('div', {class:'c-card', onclick: fn}, 'text', node, [nodes]) */
  function h(tag, attrs) {
    var el = document.createElement(tag);
    if (attrs && typeof attrs === 'object' && !attrs.nodeType && !Array.isArray(attrs)) {
      for (var k in attrs) {
        if (!Object.prototype.hasOwnProperty.call(attrs, k)) continue;
        var v = attrs[k];
        if (v == null || v === false) continue;
        if (k === 'class') el.className = v;
        else if (k === 'style' && typeof v === 'object') for (var s in v) el.style.setProperty(s, v[s]);
        else if (k === 'html') el.innerHTML = v;
        else if (k.slice(0, 2) === 'on' && typeof v === 'function') el.addEventListener(k.slice(2).toLowerCase(), v);
        else el.setAttribute(k, v === true ? '' : v);
      }
    }
    var start = attrs && typeof attrs === 'object' && !attrs.nodeType && !Array.isArray(attrs) ? 2 : 1;
    var add = function (c) {
      if (c == null || c === false) return;
      if (Array.isArray(c)) return c.forEach(add);
      el.appendChild(c.nodeType ? c : document.createTextNode(String(c)));
    };
    for (var i = start; i < arguments.length; i++) add(arguments[i]);
    return el;
  }

  var fmt = {
    num: function (n, digits) {
      var v = Number(n); if (!isFinite(v)) return '–';
      return v.toLocaleString('en-US', { maximumFractionDigits: digits == null ? 2 : digits });
    },
    compact: function (n) {
      var v = Number(n); if (!isFinite(v)) return '–';
      var a = Math.abs(v);
      if (a >= 1e9) return (v / 1e9).toFixed(1).replace(/\.0$/, '') + 'B';
      if (a >= 1e6) return (v / 1e6).toFixed(1).replace(/\.0$/, '') + 'M';
      if (a >= 1e3) return (v / 1e3).toFixed(1).replace(/\.0$/, '') + 'k';
      return String(Math.round(v * 100) / 100);
    },
    pct: function (n, digits) { var v = Number(n); return isFinite(v) ? v.toFixed(digits == null ? 0 : digits) + '%' : '–'; },
    money: function (n, cur) { var v = Number(n); return isFinite(v) ? (cur || '$') + v.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) : '–'; },
    bytes: function (n) {
      var v = Number(n); if (!isFinite(v)) return '–';
      var u = ['B', 'KB', 'MB', 'GB', 'TB'], i = 0;
      while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
      return (i ? v.toFixed(1) : String(Math.round(v))) + ' ' + u[i];
    },
    duration: function (ms) {
      var s = Math.max(0, Math.round(Number(ms) / 1000)); if (!isFinite(s)) return '–';
      if (s < 60) return s + 's';
      var m = Math.floor(s / 60); if (m < 60) return m + 'm ' + (s % 60) + 's';
      var hr = Math.floor(m / 60); if (hr < 24) return hr + 'h ' + (m % 60) + 'm';
      return Math.floor(hr / 24) + 'd ' + (hr % 24) + 'h';
    },
    ago: function (t) {
      var d = typeof t === 'number' ? t : Date.parse(t); if (!isFinite(d)) return '–';
      var s = Math.round((Date.now() - d) / 1000);
      if (s < 5) return 'just now';
      if (s < 60) return s + 's ago';
      if (s < 3600) return Math.floor(s / 60) + 'm ago';
      if (s < 86400) return Math.floor(s / 3600) + 'h ago';
      return Math.floor(s / 86400) + 'd ago';
    },
    time: function (t) {
      var d = new Date(typeof t === 'number' ? t : Date.parse(t));
      return isNaN(d) ? '–' : d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    },
    date: function (t) {
      var d = new Date(typeof t === 'number' ? t : Date.parse(t));
      return isNaN(d) ? '–' : d.toLocaleDateString([], { month: 'short', day: 'numeric' });
    }
  };

  // ── icons (stroke, 24x24, currentColor) ──

  var ICONS = {
    check: '<polyline points="20 6 9 17 4 12"/>',
    x: '<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>',
    plus: '<line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>',
    minus: '<line x1="5" y1="12" x2="19" y2="12"/>',
    play: '<polygon points="6 4 20 12 6 20 6 4"/>',
    pause: '<rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/>',
    stop: '<rect x="5" y="5" width="14" height="14" rx="2"/>',
    refresh: '<polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.5 9a9 9 0 0 1 14.9-3.4L23 10M1 14l4.6 4.4A9 9 0 0 0 20.5 15"/>',
    clock: '<circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 16 14"/>',
    alert: '<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>',
    info: '<circle cx="12" cy="12" r="10"/><line x1="12" y1="11" x2="12" y2="16"/><line x1="12" y1="8" x2="12.01" y2="8"/>',
    file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
    folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    terminal: '<polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/>',
    code: '<polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/>',
    edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/>',
    search: '<circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.6" y2="16.6"/>',
    database: '<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 1.7 4 3 9 3s9-1.3 9-3V5"/><path d="M3 12c0 1.7 4 3 9 3s9-1.3 9-3"/>',
    server: '<rect x="2" y="3" width="20" height="7" rx="1.5"/><rect x="2" y="14" width="20" height="7" rx="1.5"/><line x1="6" y1="6.5" x2="6.01" y2="6.5"/><line x1="6" y1="17.5" x2="6.01" y2="17.5"/>',
    activity: '<polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/>',
    chart: '<line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/>',
    trend: '<polyline points="23 6 13.5 15.5 8.5 10.5 1 18"/><polyline points="17 6 23 6 23 12"/>',
    'trend-down': '<polyline points="23 18 13.5 8.5 8.5 13.5 1 6"/><polyline points="17 18 23 18 23 12"/>',
    'arrow-up': '<line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/>',
    'arrow-down': '<line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/>',
    'arrow-right': '<line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>',
    'chevron-right': '<polyline points="9 18 15 12 9 6"/>',
    'chevron-down': '<polyline points="6 9 12 15 18 9"/>',
    external: '<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/>',
    git: '<line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/>',
    box: '<path d="M21 16V8a2 2 0 0 0-1-1.7l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.7l7 4a2 2 0 0 0 2 0l7-4a2 2 0 0 0 1-1.7z"/><polyline points="3.3 7 12 12 20.7 7"/><line x1="12" y1="22" x2="12" y2="12"/>',
    zap: '<polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>',
    user: '<path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>',
    users: '<path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.9"/><path d="M16 3.1a4 4 0 0 1 0 7.8"/>',
    cart: '<circle cx="9" cy="21" r="1"/><circle cx="20" cy="21" r="1"/><path d="M1 1h4l2.7 13.4a2 2 0 0 0 2 1.6h9.7a2 2 0 0 0 2-1.6L23 6H6"/>',
    dollar: '<line x1="12" y1="1" x2="12" y2="23"/><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>',
    globe: '<circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>',
    cpu: '<rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6"/><line x1="9" y1="1" x2="9" y2="4"/><line x1="15" y1="1" x2="15" y2="4"/><line x1="9" y1="20" x2="9" y2="23"/><line x1="15" y1="20" x2="15" y2="23"/><line x1="20" y1="9" x2="23" y2="9"/><line x1="20" y1="14" x2="23" y2="14"/><line x1="1" y1="9" x2="4" y2="9"/><line x1="1" y1="14" x2="4" y2="14"/>',
    list: '<line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/>',
    grid: '<rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/>',
    bell: '<path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.7 21a2 2 0 0 1-3.4 0"/>',
    star: '<polygon points="12 2 15.1 8.3 22 9.3 17 14.1 18.2 21 12 17.8 5.8 21 7 14.1 2 9.3 8.9 8.3 12 2"/>',
    flag: '<path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z"/><line x1="4" y1="22" x2="4" y2="15"/>',
    circle: '<circle cx="12" cy="12" r="9"/>',
    loader: '<line x1="12" y1="2" x2="12" y2="6"/><line x1="12" y1="18" x2="12" y2="22"/><line x1="4.9" y1="4.9" x2="7.8" y2="7.8"/><line x1="16.2" y1="16.2" x2="19.1" y2="19.1"/><line x1="2" y1="12" x2="6" y2="12"/><line x1="18" y1="12" x2="22" y2="12"/><line x1="4.9" y1="19.1" x2="7.8" y2="16.2"/><line x1="16.2" y1="7.8" x2="19.1" y2="4.9"/>',
    send: '<line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/>',
    trash: '<polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M10 11v6M14 11v6"/><path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>',
    download: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>',
    eye: '<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>',
    lock: '<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
    bug: '<rect x="8" y="6" width="8" height="14" rx="4"/><path d="M19 7l-3 2M5 7l3 2M19 19l-3-2M5 19l3-2M20 13h-4M4 13h4M12 6V3"/>',
    layers: '<polygon points="12 2 2 7 12 12 22 7 12 2"/><polyline points="2 17 12 22 22 17"/><polyline points="2 12 12 17 22 12"/>'
  };

  function icon(name, size) {
    var body = ICONS[name] || ICONS.circle;
    var s = size ? ' style="width:' + size + 'px;height:' + size + 'px"' : '';
    return '<svg class="c-ic"' + s + ' viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + body + '</svg>';
  }

  // ── charts: SVG strings sized by viewBox, so they scale with their box ──

  var SERIES = ['var(--accent)', 'var(--green)', 'var(--yellow)', 'var(--sky)', 'var(--red)', 'var(--accent-hover)'];

  function nums(values) {
    return (values || []).map(function (v) { return typeof v === 'object' && v ? Number(v.value) : Number(v); }).filter(isFinite);
  }

  var chart = {
    /** A line with no axes. spark([3,5,2,8], {color, fill, width, height}) */
    spark: function (values, o) {
      o = o || {};
      var v = nums(values), W = o.width || 120, H = o.height || 32;
      if (v.length < 2) return '<svg class="c-chart" viewBox="0 0 ' + W + ' ' + H + '" preserveAspectRatio="none"></svg>';
      var min = Math.min.apply(null, v), max = Math.max.apply(null, v), span = max - min || 1;
      var pts = v.map(function (y, i) { return [(i / (v.length - 1)) * W, H - 2 - ((y - min) / span) * (H - 4)]; });
      var d = pts.map(function (p, i) { return (i ? 'L' : 'M') + p[0].toFixed(1) + ' ' + p[1].toFixed(1); }).join(' ');
      var color = o.color || 'var(--accent)';
      return '<svg class="c-chart" viewBox="0 0 ' + W + ' ' + H + '" preserveAspectRatio="none" role="img" aria-label="' + esc(o.label || 'trend') + '">'
        + (o.fill === false ? '' : '<path d="' + d + ' L' + W + ' ' + H + ' L0 ' + H + ' Z" fill="' + color + '" opacity="0.14"/>')
        + '<path d="' + d + '" fill="none" stroke="' + color + '" stroke-width="1.6" vector-effect="non-scaling-stroke" stroke-linejoin="round"/></svg>';
    },
    /** bars([{label,value,color?}], {height, horizontal}) */
    bars: function (items, o) {
      o = o || {};
      var list = (items || []).map(function (it, i) { return typeof it === 'object' && it ? it : { label: String(i + 1), value: it }; });
      var max = Math.max.apply(null, list.map(function (it) { return Number(it.value) || 0; }).concat([0])) || 1;
      if (o.horizontal) {
        return '<div class="c-hbars">' + list.map(function (it, i) {
          var w = Math.max(0, (Number(it.value) || 0) / max) * 100;
          return '<div class="c-hbar"><span class="c-hbar-l">' + esc(it.label) + '</span><span class="c-hbar-t"><span style="width:' + w.toFixed(1) + '%;background:' + (it.color || SERIES[0]) + '"></span></span><span class="c-hbar-v">' + esc(it.display != null ? it.display : fmt.compact(it.value)) + '</span></div>';
        }).join('') + '</div>';
      }
      var W = 100, H = o.height || 60, n = list.length || 1, gap = n > 30 ? 0.4 : 1.2, bw = (W - gap * (n - 1)) / n;
      return '<svg class="c-chart" viewBox="0 0 ' + W + ' ' + H + '" preserveAspectRatio="none" role="img" aria-label="' + esc(o.label || 'bars') + '">'
        + list.map(function (it, i) {
          var bh = Math.max(0.6, ((Number(it.value) || 0) / max) * (H - 2));
          return '<rect x="' + (i * (bw + gap)).toFixed(2) + '" y="' + (H - bh).toFixed(2) + '" width="' + bw.toFixed(2) + '" height="' + bh.toFixed(2) + '" rx="0.8" fill="' + (it.color || o.color || SERIES[0]) + '"><title>' + esc(it.label + ': ' + it.value) + '</title></rect>';
        }).join('') + '</svg>';
    },
    /** line([{name, values:[..], color?}], {height, labels}) with a light grid */
    line: function (series, o) {
      o = o || {};
      var list = Array.isArray(series) && series.length && typeof series[0] === 'object' && series[0] && series[0].values ? series : [{ name: '', values: series }];
      var W = 300, H = o.height || 120, pad = 4;
      var all = [];
      list.forEach(function (s) { all = all.concat(nums(s.values)); });
      if (!all.length) return '<svg class="c-chart" viewBox="0 0 ' + W + ' ' + H + '"></svg>';
      var min = o.min != null ? o.min : Math.min.apply(null, all), max = o.max != null ? o.max : Math.max.apply(null, all), span = max - min || 1;
      var out = '<svg class="c-chart" viewBox="0 0 ' + W + ' ' + H + '" preserveAspectRatio="none" role="img" aria-label="' + esc(o.label || 'chart') + '">';
      for (var g = 0; g <= 3; g++) { var gy = pad + (g / 3) * (H - pad * 2); out += '<line x1="0" x2="' + W + '" y1="' + gy.toFixed(1) + '" y2="' + gy.toFixed(1) + '" stroke="var(--border)" stroke-width="1" vector-effect="non-scaling-stroke" opacity="0.6"/>'; }
      list.forEach(function (s, si) {
        var v = nums(s.values); if (v.length < 2) return;
        var d = v.map(function (y, i) { return (i ? 'L' : 'M') + ((i / (v.length - 1)) * W).toFixed(1) + ' ' + (H - pad - ((y - min) / span) * (H - pad * 2)).toFixed(1); }).join(' ');
        out += '<path d="' + d + '" fill="none" stroke="' + (s.color || SERIES[si % SERIES.length]) + '" stroke-width="1.8" vector-effect="non-scaling-stroke" stroke-linejoin="round"><title>' + esc(s.name || '') + '</title></path>';
      });
      return out + '</svg>';
    },
    /** donut([{label,value,color?}], {size, thickness, center}) */
    donut: function (parts, o) {
      o = o || {};
      var list = (parts || []).filter(function (p) { return Number(p.value) > 0; });
      var total = list.reduce(function (a, p) { return a + Number(p.value); }, 0) || 1;
      var R = 16, C = 2 * Math.PI * R, off = 0, th = o.thickness || 5;
      var out = '<svg class="c-chart c-donut" viewBox="0 0 40 40" role="img" aria-label="' + esc(o.label || 'breakdown') + '"' + (o.size ? ' style="width:' + o.size + 'px;height:' + o.size + 'px"' : '') + '>'
        + '<circle cx="20" cy="20" r="' + R + '" fill="none" stroke="var(--bg-tertiary)" stroke-width="' + th + '"/>';
      list.forEach(function (p, i) {
        var len = (Number(p.value) / total) * C;
        out += '<circle cx="20" cy="20" r="' + R + '" fill="none" stroke="' + (p.color || SERIES[i % SERIES.length]) + '" stroke-width="' + th + '" stroke-dasharray="' + len.toFixed(2) + ' ' + (C - len).toFixed(2) + '" stroke-dashoffset="' + (-off).toFixed(2) + '" transform="rotate(-90 20 20)"><title>' + esc(p.label + ': ' + p.value) + '</title></circle>';
        off += len;
      });
      if (o.center != null) out += '<text x="20" y="20" text-anchor="middle" dominant-baseline="central" fill="var(--text-primary)" font-size="8" font-weight="600">' + esc(o.center) + '</text>';
      return out + '</svg>';
    },
    colors: SERIES
  };

  // ── declarative bindings: data-bind="source.path" data-fmt="ago" ──

  function applyBindings() {
    var nodes = document.querySelectorAll('[data-bind]');
    for (var i = 0; i < nodes.length; i++) {
      var el = nodes[i];
      var v = get(crundi.data, el.getAttribute('data-bind'), undefined);
      var f = el.getAttribute('data-fmt');
      var text = v === undefined ? (el.getAttribute('data-empty') || '–') : (f && fmt[f] ? fmt[f](v) : (typeof v === 'object' ? JSON.stringify(v) : String(v)));
      if (el.textContent !== text) el.textContent = text;
    }
    var shown = document.querySelectorAll('[data-show]');
    for (var j = 0; j < shown.length; j++) {
      var s = shown[j];
      var val = get(crundi.data, s.getAttribute('data-show'), undefined);
      var on = Array.isArray(val) ? val.length > 0 : !!val;
      s.hidden = !on;
    }
  }

  function fire(kind, a, b) {
    var list = handlers[kind] || [];
    for (var i = 0; i < list.length; i++) {
      try { list[i](a, b); } catch (e) { fault(e && e.message ? e.message : String(e), 'in a ' + kind + ' handler'); }
    }
  }

  function dataChanged(name) {
    applyBindings();
    fire('data', crundi.data, name);
    // New data usually means a new size. Measured now, not left to the next paint.
    if (typeof reportHeight === 'function') reportHeight();
  }

  var crundi = {
    id: BOOT.id || '',
    title: BOOT.title || '',
    /** { sourceName: value } — always the latest. */
    data: {},
    /** { sourceName: 'why it failed' } for sources that could not be read. */
    errors: {},
    /** frame ('dock'|'cell'|'tab'|'inline'|'chip'), platform ('desktop'|'mobile'), width, height, touch */
    context: {},
    isHarness: !!BOOT.harness,
    /** True when this copy of the panel is the top bar chip, showing only [data-chip]. */
    isChip: IS_CHIP,

    /** Run fn(data, changedSourceName) now if data has arrived, and on every change. */
    onData: function (fn) {
      handlers.data.push(fn);
      if (started) { try { fn(crundi.data, null); } catch (e) { fault(e.message, 'in onData'); } }
      return crundi;
    },
    /** Run fn(context) when the frame is resized or moved to another slot. */
    onContext: function (fn) { handlers.context.push(fn); if (started) { try { fn(crundi.context); } catch (e) { fault(e.message, 'in onContext'); } } return crundi; },
    /** Run fn() once the first data has arrived. */
    ready: function (fn) { if (started) { try { fn(); } catch (e) { fault(e.message, 'in ready'); } } else handlers.ready.push(fn); return crundi; },

    get: get, esc: esc, h: h, fmt: fmt, icon: icon, chart: chart,

    /** A small key/value store kept by Crundi for this widget. */
    store: {
      _all: {},
      get: function (key, dflt) { var v = crundi.store._all[key]; return v === undefined ? dflt : v; },
      set: function (key, value) { if (value == null) delete crundi.store._all[key]; else crundi.store._all[key] = value; return call('store.set', { key: key, value: value }); },
      all: function () { return crundi.store._all; },
      /**
       * Run fn(all) when the store changes, including from ANOTHER copy of
       * this panel. The chip in the top bar, the panel it opens and a pane in
       * the workbench are separate frames of the same panel; this is how a
       * button pressed in one reaches the others.
       */
      onChange: function (fn) { handlers.store.push(fn); return crundi.store; }
    },
    /** Run an action the manifest declares. Resolves with its result. */
    action: function (name, params) { return call('action', { name: name, params: params || {} }); },
    /** Leave a note for Claude; it reads these with widget_get. No approval needed. */
    emit: function (name, payload) { return call('emit', { name: name, payload: payload === undefined ? null : payload }); },
    /** Send a message into the chat that owns this widget (manifest: "allow": ["prompt"]). */
    prompt: function (text) { return call('prompt', { text: String(text || '') }); },
    openLink: function (url) { return call('openLink', { url: String(url || '') }); },
    toast: function (text, kind) { return call('toast', { text: String(text || ''), kind: kind || '' }); },
    /** Ask for a specific height (inline and dock frames size to content anyway). */
    resize: function (height) { post({ t: 'height', h: Math.round(Number(height) || 0), fixed: true }); }
  };
  window.crundi = crundi;

  // ── host messages ──

  function take(entries) {
    for (var name in entries) {
      if (!Object.prototype.hasOwnProperty.call(entries, name)) continue;
      var e = entries[name] || {};
      crundi.data[name] = e.value === undefined ? null : e.value;
      if (e.error) crundi.errors[name] = e.error; else delete crundi.errors[name];
    }
  }

  function setContext(ctx) {
    crundi.context = ctx || {};
    var root = document.documentElement;
    root.setAttribute('data-frame', IS_CHIP ? 'chipbar' : (crundi.context.frame || ''));
    root.setAttribute('data-platform', crundi.context.platform || 'desktop');
  }

  window.addEventListener('message', function (ev) {
    if (ev.source !== host) return;
    var m = ev.data;
    if (!m || m.crundi !== 1) return;
    if (m.t === 'init') {
      take(m.data || {});
      crundi.store._all = m.store || {};
      setContext(m.context);
      var first = !started;
      started = true;
      if (first) fire('ready');
      fire('context', crundi.context);
      dataChanged(null);
    } else if (m.t === 'data') {
      var one = {}; one[m.source] = { value: m.value, error: m.error };
      take(one);
      if (started) dataChanged(m.source);
    } else if (m.t === 'context') {
      setContext(m.context);
      fire('context', crundi.context);
    } else if (m.t === 'result') {
      var w = waiting[m.seq];
      if (!w) return;
      delete waiting[m.seq];
      if (m.ok) w.resolve(m.value); else w.reject(new Error(m.error || 'Failed'));
    } else if (m.t === 'store') {
      crundi.store._all = m.store || {};
      fire('store', crundi.store._all);
    } else if (m.t === 'measure') {
      // The host asks for the size again, outright. Size normally travels on
      // a ResizeObserver, which waits for the browser to paint; a frame that is
      // not being painted yet (headless, a background tab) would stay unsized.
      lastH = 0; lastW = -1;
      reportHeight();
    } else if (m.t === 'lint') {
      var report;
      try { report = window.__crundiLint ? window.__crundiLint(crundi) : { issues: [], metrics: {} }; }
      catch (e) { report = { issues: [{ level: 'warn', rule: 'lint', message: 'Lint could not run: ' + e.message }], metrics: {} }; }
      post({ t: 'lint', seq: m.seq, report: report });
    }
  });

  // ── content height, for frames that size to their content ──

  var lastH = 0, lastW = -1;
  function reportHeight() {
    var b = document.body; if (!b) return;
    var hgt = Math.ceil(Math.max(b.scrollHeight, b.offsetHeight));
    if (IS_CHIP) {
      // A chip sizes sideways: as wide as its face, whatever the frame is now.
      var face = document.querySelector('body > [data-chip]');
      // scrollWidth as well: the content's width even if the box was squeezed.
      var wid = face ? Math.ceil(Math.max(face.getBoundingClientRect().width, face.scrollWidth)) : 0;
      if (Math.abs(wid - lastW) < 1) return;
      lastW = wid;
      post({ t: 'height', h: hgt, w: wid });
      return;
    }
    if (Math.abs(hgt - lastH) < 2) return;
    lastH = hgt;
    post({ t: 'height', h: hgt });
  }

  function boot() {
    if (window.ResizeObserver && document.body) {
      try {
        var ro = new ResizeObserver(reportHeight);
        ro.observe(document.body);
        var face = IS_CHIP && document.querySelector('body > [data-chip]');
        if (face) ro.observe(face);
      } catch (e) { /* older engine */ }
    }
    post({ t: 'ready' });
    reportHeight();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
