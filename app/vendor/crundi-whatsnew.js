/* What's new: a full-window stepper shown once after an install or an update.
 *
 * The steps come from the server (GET /api/whatsnew, see src/whatsnew.js): the
 * text, and for each a small animation written as markup and styles for a
 * 640 x 400 stage. This file is only the frame around them: one step at a
 * time, Back and Next, dots, Skip, and a recap at the end.
 *
 * Wide window: words on the left, the animation on the right.
 * Narrow window or a phone: the animation on top, the words under it, and the
 * buttons at the bottom within reach of a thumb. Swiping moves between steps.
 */
(function () {
  'use strict';
  var STAGE_W = 640, STAGE_H = 400;
  var styled = false;

  function css() {
    if (styled) return; styled = true;
    var s = document.createElement('style');
    s.textContent = [
      '.wn-root{position:fixed;inset:0;z-index:5000;display:grid;grid-template-columns:minmax(340px,42%) 1fr;grid-template-rows:1fr;background:radial-gradient(1200px 700px at 78% 40%,rgba(99,102,241,.14),transparent 60%),var(--bg-primary,#0b0b11);color:var(--text-primary,#ececf3);font-family:inherit;opacity:0;transition:opacity .25s ease;overflow:hidden;-webkit-app-region:no-drag}',
      '.wn-root.in{opacity:1}',
      '.wn-top{position:absolute;left:0;right:0;top:0;display:flex;align-items:center;gap:10px;padding:max(16px,env(safe-area-inset-top)) max(22px,env(safe-area-inset-right)) 0 max(22px,env(safe-area-inset-left));z-index:3}',
      '.wn-brand{font-weight:700;font-size:15px;letter-spacing:.01em;display:flex;align-items:center;gap:9px}',
      '.wn-logo{width:26px;height:26px;border-radius:8px;background:linear-gradient(135deg,#6366f1,#22d3ee);display:grid;place-items:center}',
      '.wn-logo svg{width:15px;height:15px}',
      '.wn-ver{font:600 11.5px var(--mono,ui-monospace,monospace);color:var(--accent-hover,#8b8dfb);background:var(--accent-dim,rgba(99,102,241,.16));border:1px solid rgba(99,102,241,.35);padding:3px 8px;border-radius:99px}',
      '.wn-sp{flex:1}',
      '.wn-skip{appearance:none;background:transparent;border:1px solid var(--border,#262636);color:var(--text-secondary,#8d8da3);border-radius:99px;padding:8px 14px;font:inherit;font-size:13px;cursor:pointer;display:inline-flex;align-items:center;gap:6px}',
      '.wn-skip:hover{color:var(--text-primary,#fff);border-color:var(--accent,#6366f1)}',
      '.wn-skip svg{width:13px;height:13px}',
      '.wn-copy{display:flex;flex-direction:column;justify-content:center;padding:90px 48px 116px 64px;min-width:0;min-height:0}',
      '.wn-copy-in{animation:wn-in .32s ease both}',
      '@keyframes wn-in{from{opacity:0;transform:translateY(10px)}to{opacity:1;transform:none}}',
      '.wn-eyebrow{font-size:12px;letter-spacing:.14em;text-transform:uppercase;color:var(--accent-hover,#8b8dfb);font-weight:700;margin-bottom:14px;display:flex;align-items:baseline;gap:10px;flex-wrap:wrap}',
      '.wn-eyebrow i{font-style:normal;color:var(--text-muted,#5d5d72);letter-spacing:.04em}',
      '.wn-title{font-size:clamp(28px,3.1vw,42px);line-height:1.08;margin:0 0 16px;letter-spacing:-.02em;font-weight:750}',
      '.wn-body{margin:0 0 20px;color:#b9b9cc;font-size:16.5px;line-height:1.5;max-width:31em}',
      '.wn-keys{display:flex;flex-direction:column;gap:9px}',
      '.wn-krow{display:flex;align-items:center;gap:12px;font-size:14px;color:#c9c9da}',
      '.wn-k{display:inline-flex;gap:4px;flex:0 0 150px;flex-wrap:wrap}',
      '.wn-k kbd{font:600 11.5px var(--mono,ui-monospace,monospace);padding:3px 7px;border-radius:6px;border:1px solid var(--border,#262636);border-bottom-width:2px;background:var(--bg-secondary,#12121a);color:var(--text-primary,#fff);white-space:nowrap}',
      '.wn-art{position:relative;display:grid;place-items:center;padding:86px 56px 116px 8px;min-width:0;min-height:0}',
      '.wn-frame{width:min(100%,760px);background:var(--bg-secondary,#12121a);border:1px solid var(--border,#262636);border-radius:18px;box-shadow:0 30px 80px rgba(0,0,0,.55);overflow:hidden;display:flex;flex-direction:column;max-height:100%}',
      '.wn-fbar{height:34px;flex:none;border-bottom:1px solid var(--border,#262636);display:flex;align-items:center;gap:8px;padding:0 14px;color:var(--text-secondary,#8d8da3);font-size:12px;font-weight:600}',
      '.wn-fbar s{width:8px;height:8px;border-radius:50%;background:#2b2b3b;display:block}',
      // The stage is always 640 x 400 and is scaled to whatever room there is.
      '.wn-stagebox{position:relative;width:100%;aspect-ratio:' + STAGE_W + ' / ' + STAGE_H + ';overflow:hidden;flex:0 1 auto;min-height:0}',
      '.wn-stage{position:absolute;left:0;top:0;width:' + STAGE_W + 'px;height:' + STAGE_H + 'px;transform-origin:0 0}',
      '.wn-stage .wna{position:absolute;inset:0;overflow:hidden}',
      '.wn-recap{padding:18px 20px;display:flex;flex-direction:column;gap:9px;overflow-y:auto}',
      '.wn-li{appearance:none;text-align:left;font:inherit;color:inherit;cursor:pointer;display:flex;gap:12px;align-items:flex-start;padding:10px 13px;border:1px solid var(--border,#262636);border-radius:12px;background:var(--bg-card,#181822)}',
      '.wn-li:hover{border-color:var(--accent,#6366f1)}',
      '.wn-li svg{width:17px;height:17px;color:var(--green,#10b981);flex:none;margin-top:2px}',
      '.wn-li b{display:block;font-size:14px;font-weight:650}',
      '.wn-li span{font-size:12px;color:var(--text-secondary,#8d8da3)}',
      '.wn-foot{position:absolute;left:0;right:0;bottom:0;display:flex;align-items:center;gap:16px;padding:20px max(56px,env(safe-area-inset-right)) max(26px,env(safe-area-inset-bottom)) max(64px,env(safe-area-inset-left));z-index:3}',
      '.wn-dots{display:flex;gap:7px;align-items:center}',
      '.wn-dots b{width:7px;height:7px;border-radius:99px;background:#2c2c3e;display:block;transition:width .2s ease,background .2s ease;cursor:pointer}',
      '.wn-dots b.on{width:26px;background:var(--accent,#6366f1)}',
      '.wn-dots b.done{background:#4b4d8f}',
      '.wn-count{font:600 12px var(--mono,ui-monospace,monospace);color:var(--text-muted,#5d5d72)}',
      '.wn-hint{font-size:12px;color:var(--text-muted,#5d5d72)}',
      '.wn-btn{appearance:none;border:1px solid var(--border,#262636);background:var(--bg-card,#181822);color:var(--text-primary,#fff);border-radius:12px;padding:11px 18px;font:inherit;font-weight:650;font-size:14.5px;cursor:pointer;display:inline-flex;align-items:center;gap:8px}',
      '.wn-btn svg{width:15px;height:15px}',
      '.wn-btn.primary{background:var(--accent,#6366f1);border-color:var(--accent,#6366f1);box-shadow:0 8px 24px rgba(99,102,241,.35)}',
      '.wn-btn[disabled]{opacity:0;pointer-events:none}',
      // Narrow window, a phone, or a short one held sideways.
      '@media (max-width:820px){',
      '.wn-root{grid-template-columns:1fr;grid-template-rows:minmax(0,43%) minmax(0,1fr)}',
      '.wn-art{order:1;padding:calc(max(12px,env(safe-area-inset-top)) + 46px) 14px 4px}',
      '.wn-frame{border-radius:14px;height:100%;width:auto;max-width:100%;aspect-ratio:' + STAGE_W + ' / ' + (STAGE_H + 34) + '}',
      '.wn-copy{order:2;padding:14px 22px 132px;justify-content:flex-start;overflow-y:auto}',
      '.wn-title{font-size:27px;margin-bottom:9px}.wn-body{font-size:15px;margin-bottom:14px}',
      '.wn-eyebrow{margin-bottom:8px;font-size:11px}',
      '.wn-k{flex-basis:132px}.wn-krow{font-size:13px;align-items:flex-start}',
      '.wn-top{padding-left:14px;padding-right:14px}',
      '.wn-foot{padding:10px 16px max(18px,env(safe-area-inset-bottom));flex-wrap:wrap;gap:10px;background:linear-gradient(to top,var(--bg-primary,#0b0b11) 62%,transparent)}',
      '.wn-foot .wn-dots{order:-1;flex-basis:100%;justify-content:center;margin-bottom:2px}',
      '.wn-foot .wn-btn{padding:14px 18px}',
      '.wn-foot .wn-btn.primary{flex:1;justify-content:center}',
      '.wn-count,.wn-hint,.wn-foot .wn-sp{display:none}',
      '}',
      // A phone held sideways: side by side again, but tight.
      '@media (max-height:520px) and (min-width:600px){',
      '.wn-root{grid-template-columns:46% 1fr;grid-template-rows:1fr}',
      '.wn-art{order:2;padding:54px 16px 70px 6px}.wn-copy{order:1;padding:54px 14px 78px 22px;justify-content:flex-start;overflow-y:auto}',
      '.wn-frame{height:auto;width:100%;aspect-ratio:auto}',
      '.wn-title{font-size:22px;margin-bottom:6px}.wn-body{font-size:13.5px;margin-bottom:8px}.wn-keys{gap:5px}',
      '.wn-foot{flex-wrap:nowrap;padding:8px 18px max(10px,env(safe-area-inset-bottom));background:none}',
      '.wn-foot .wn-dots{order:0;flex-basis:auto;margin:0}.wn-foot .wn-btn{padding:9px 14px}.wn-foot .wn-btn.primary{flex:0 0 auto}.wn-foot .wn-sp{display:block}',
      '}',
      '@media (prefers-reduced-motion:reduce){.wn-copy-in{animation:none}}'
    ].join('\n');
    document.head.appendChild(s);
  }

  var IC = {
    x: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>',
    next: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>',
    back: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>',
    check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>',
    logo: '<svg viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg>'
  };
  function el(tag, cls, html) { var e = document.createElement(tag); if (cls) e.className = cls; if (html != null) e.innerHTML = html; return e; }
  function txt(tag, cls, t) { var e = document.createElement(tag); if (cls) e.className = cls; e.textContent = t == null ? '' : String(t); return e; }

  var current = null;

  /**
   * @param {{version:string, steps:object[]}} data  as /api/whatsnew returns it
   * @param {{onClose?:function(string)}} opts       onClose('done' | 'skip')
   */
  function open(data, opts) {
    opts = opts || {};
    var steps = (data && data.steps) || [];
    if (!steps.length) return null;
    if (current) current.close('replaced');
    css();
    var N = steps.length + 1;          // the steps, then the recap
    var at = 0;

    var root = el('div', 'wn-root'); root.setAttribute('role', 'dialog'); root.setAttribute('aria-modal', 'true'); root.setAttribute('aria-label', "What's new");
    var top = el('div', 'wn-top');
    var brand = el('div', 'wn-brand', '<span class="wn-logo">' + IC.logo + '</span>'); brand.appendChild(document.createTextNode('Crundi'));
    var skip = el('button', 'wn-skip'); skip.type = 'button'; skip.appendChild(document.createTextNode('Skip')); skip.insertAdjacentHTML('beforeend', IC.x);
    top.appendChild(brand); top.appendChild(txt('span', 'wn-ver', data.version || '')); top.appendChild(el('span', 'wn-sp')); top.appendChild(skip);
    var copy = el('div', 'wn-copy'), art = el('div', 'wn-art');
    var frame = el('div', 'wn-frame'); art.appendChild(frame);
    var foot = el('div', 'wn-foot');
    var back = el('button', 'wn-btn'); back.type = 'button'; back.innerHTML = IC.back; back.appendChild(document.createTextNode('Back'));
    var dots = el('div', 'wn-dots'), count = el('span', 'wn-count');
    var next = el('button', 'wn-btn primary'); next.type = 'button';
    foot.appendChild(back); foot.appendChild(dots); foot.appendChild(count); foot.appendChild(el('span', 'wn-sp'));
    foot.appendChild(txt('span', 'wn-hint', 'Arrow keys, or swipe')); foot.appendChild(next);
    root.appendChild(top); root.appendChild(copy); root.appendChild(art); root.appendChild(foot);

    function fit() {
      var box = frame.querySelector('.wn-stagebox'), stage = frame.querySelector('.wn-stage');
      if (!box || !stage) return;
      var s = Math.min(box.clientWidth / STAGE_W, box.clientHeight / STAGE_H) || 1;
      stage.style.transform = 'translate(' + Math.max(0, (box.clientWidth - STAGE_W * s) / 2) + 'px,' + Math.max(0, (box.clientHeight - STAGE_H * s) / 2) + 'px) scale(' + s + ')';
    }

    function draw() {
      var last = at === N - 1, s = steps[at];
      dots.innerHTML = '';
      for (var i = 0; i < N; i++) (function (i) { var d = el('b', i === at ? 'on' : (i < at ? 'done' : '')); d.addEventListener('click', function () { go(i); }); dots.appendChild(d); })(i);
      count.textContent = (at + 1) + ' / ' + N;
      back.disabled = at === 0;
      next.innerHTML = '';
      if (last) { next.innerHTML = IC.check; next.appendChild(document.createTextNode('Done')); }
      else { next.appendChild(document.createTextNode('Next')); next.insertAdjacentHTML('beforeend', IC.next); }
      skip.style.visibility = last ? 'hidden' : '';

      var inner = el('div', 'wn-copy-in');
      frame.innerHTML = '';
      var bar = el('div', 'wn-fbar', '<s></s><s></s><s></s>');
      if (last) {
        var eb = el('div', 'wn-eyebrow'); eb.appendChild(document.createTextNode('That is everything'));
        inner.appendChild(eb);
        inner.appendChild(txt('h2', 'wn-title', 'You are up to date'));
        inner.appendChild(txt('p', 'wn-body', "This shows once after each update. You can open it again any time from Settings, under What's new."));
        bar.appendChild(document.createTextNode('  In this update')); frame.appendChild(bar);
        var list = el('div', 'wn-recap');
        steps.forEach(function (st, i) {
          var b = el('button', 'wn-li', IC.check); b.type = 'button';
          var d = el('div'); d.appendChild(txt('b', '', st.title)); d.appendChild(txt('span', '', st.tag || '')); b.appendChild(d);
          b.addEventListener('click', function () { go(i); });
          list.appendChild(b);
        });
        frame.appendChild(list);
      } else {
        var e2 = el('div', 'wn-eyebrow'); e2.appendChild(document.createTextNode(s.tag || 'New'));
        if (s.version) e2.appendChild(txt('i', '', 'new in ' + s.version));
        inner.appendChild(e2);
        inner.appendChild(txt('h2', 'wn-title', s.title));
        inner.appendChild(txt('p', 'wn-body', s.body));
        if (s.keys && s.keys.length) {
          var ks = el('div', 'wn-keys');
          s.keys.forEach(function (k) {
            var row = el('div', 'wn-krow'), kk = el('span', 'wn-k');
            (k.keys || []).forEach(function (x) { kk.appendChild(txt('kbd', '', x)); });
            row.appendChild(kk); row.appendChild(txt('span', '', k.text)); ks.appendChild(row);
          });
          inner.appendChild(ks);
        }
        bar.appendChild(document.createTextNode('  ' + (s.artTitle || s.tag || ''))); frame.appendChild(bar);
        var box = el('div', 'wn-stagebox'), stage = el('div', 'wn-stage');
        // The animation is our own file, shipped with this version: markup and
        // styles only (the tests refuse a script in one).
        stage.innerHTML = s.art || '';
        box.appendChild(stage); frame.appendChild(box);
      }
      copy.innerHTML = ''; copy.appendChild(inner); copy.scrollTop = 0;
      fit(); requestAnimationFrame(fit);
    }
    function go(i) { at = Math.max(0, Math.min(N - 1, i)); draw(); }
    function close(how) {
      if (!root.isConnected) return;
      document.removeEventListener('keydown', onKey, true);
      window.removeEventListener('resize', fit);
      if (current && current.root === root) current = null;
      root.classList.remove('in');
      setTimeout(function () { root.remove(); }, 240);
      if (opts.onClose && how !== 'replaced') { try { opts.onClose(how || 'skip'); } catch (e) { /* the caller's business */ } }
    }
    function onKey(e) {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close('skip'); }
      else if (e.key === 'ArrowRight' || e.key === 'Enter' || e.key === 'PageDown') { e.preventDefault(); e.stopPropagation(); if (at === N - 1) close('done'); else go(at + 1); }
      else if (e.key === 'ArrowLeft' || e.key === 'PageUp') { e.preventDefault(); e.stopPropagation(); go(at - 1); }
      else if (e.key === 'Tab') return;
      else e.stopPropagation();          // the app underneath gets no keys while this is up
    }
    back.addEventListener('click', function () { go(at - 1); });
    next.addEventListener('click', function () { if (at === N - 1) close('done'); else go(at + 1); });
    skip.addEventListener('click', function () { close('skip'); });
    // Swipe left for the next step, right for the one before.
    var sx = 0, sy = 0, st0 = 0;
    root.addEventListener('touchstart', function (e) { if (e.touches.length !== 1) { st0 = 0; return; } sx = e.touches[0].clientX; sy = e.touches[0].clientY; st0 = Date.now(); }, { passive: true });
    root.addEventListener('touchend', function (e) {
      if (!st0) return;
      var t = e.changedTouches[0], dx = t.clientX - sx, dy = t.clientY - sy;
      if (Date.now() - st0 < 700 && Math.abs(dx) > 55 && Math.abs(dx) > Math.abs(dy) * 1.6) { if (dx < 0) { if (at < N - 1) go(at + 1); } else go(at - 1); }
      st0 = 0;
    }, { passive: true });
    document.addEventListener('keydown', onKey, true);
    window.addEventListener('resize', fit);

    document.body.appendChild(root);
    draw();
    requestAnimationFrame(function () { root.classList.add('in'); fit(); next.focus({ preventScroll: true }); });
    current = { root: root, close: close, go: go };
    return current;
  }

  window.CrundiWhatsNew = {
    open: open,
    isOpen: function () { return !!(current && current.root.isConnected); },
    close: function () { if (current) current.close('skip'); }
  };
})();
