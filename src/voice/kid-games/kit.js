/* eslint-disable */
/**
 * kid-games/kit.js — put in front of every kid game on Cru's live screen
 * (src/voice/kid-games.js gameDoc). A browser script, run inside the
 * sandboxed frame (no network, no storage, no same-origin).
 *
 *   kid.opts                 the options start_game was given
 *   kid.init                 what the child knows: { levels, mastery, focus, avoid } per skill
 *   kid.level(skill)         1..5 (1 when unknown)
 *   kid.ask(skill, item)     a question starts: the first answer to it is its first-try result
 *   kid.hint()               a hint was shown (the question no longer counts as first try)
 *   kid.say(text)            the voice screen says it aloud now (no turn)
 *   kid.tell(text)           Cru hears "[Kid tapped: text]" and answers
 *   kid.event(kind, detail)  for the play log: tap | correct | wrong | level | finished
 *   kid.correct(what)        event + happy sound + confetti + a cheer; kid.wrong(what) event + soft sound + a gentle "try again"
 *   kid.sound(kind)          tap | yes | miss | done, soft, at Cru's volume (taps on buttons play 'tap' by themselves)
 *   kid.done(summary)        finished: Cru is told, and cheers
 *   kid.pic(word, px)        a simple drawn picture (SVG) for a word in kid.WORDS
 *   kid.shape(name, colour, px)   circle | square | triangle | star | heart | diamond
 *   kid.LETTERS, kid.WORDS (A: 'apple' …), kid.SOUNDS (A: 'ah' …), kid.COLOURS
 *   kid.shuffle(a), kid.pick(a), kid.sample(a, n), kid.el(tag, cls, html), kid.confetti()
 *
 * Messages to the parent page: { cru: 'say'|'tell'|'event', text?, kind?, detail? }.
 */
(function () {
  'use strict';
  var P = window.parent;
  var opts = window.CRU_OPTIONS || {};
  // What the child knows (kid-skills.js initFor): { type:'init', levels:{skill:1-5}, mastery:{skill:{item:0-1}}, focus:{skill:[items]}, avoid:{skill:[items]} }.
  var init = window.CRU_INIT || { type: 'init', levels: {}, mastery: {}, focus: {}, avoid: {} };
  function post(m) { try { P.postMessage(m, '*'); } catch (e) { /* gone */ } }

  var css = document.createElement('style');
  css.textContent = 'html,body{height:100%}body{margin:0;padding:10px;box-sizing:border-box;display:flex;flex-direction:column;justify-content:center;font:22px/1.3 "Comic Sans MS","Chalkboard SE",system-ui,sans-serif;color:#312e81;background:transparent;user-select:none;-webkit-user-select:none;touch-action:manipulation;overflow-x:hidden}'
    + '.k-title{font-size:30px;font-weight:800;text-align:center;margin:4px 0 10px;letter-spacing:.02em}'
    + '.k-grid{display:flex;flex-wrap:wrap;gap:14px;justify-content:center;align-items:center}'
    + '.k-btn{min-width:96px;min-height:96px;border:0;border-radius:24px;font-family:inherit;font-weight:800;font-size:52px;line-height:1;color:#fff;cursor:pointer;display:flex;align-items:center;justify-content:center;box-shadow:0 6px 0 rgba(0,0,0,.25);transition:transform .12s;padding:10px}'
    + '.k-btn:active{transform:translateY(4px) scale(.97);box-shadow:0 2px 0 rgba(0,0,0,.25)}'
    + '.k-big{font-size:120px;font-weight:900;text-align:center;line-height:1}'
    + '.k-pop{animation:kpop .45s ease-out}@keyframes kpop{0%{transform:scale(.6)}60%{transform:scale(1.15)}100%{transform:scale(1)}}'
    + '.k-shake{animation:kshake .4s}@keyframes kshake{0%,100%{transform:translateX(0)}25%{transform:translateX(-10px)}75%{transform:translateX(10px)}}'
    + '.k-dim{opacity:.35}.k-row{display:flex;gap:12px;justify-content:center;align-items:center;flex-wrap:wrap}'
    + '.k-stars{text-align:center;font-size:18px;opacity:.85;margin-top:8px}'
    + '.k-conf{position:fixed;width:12px;height:12px;border-radius:3px;pointer-events:none;animation:kfall 1.4s ease-in forwards}@keyframes kfall{to{transform:translateY(110vh) rotate(540deg);opacity:.2}}';
  // On the light kid screen, white things (a cow, a zebra, a sheep) need an edge to be seen.
  css.textContent += 'svg{filter:drop-shadow(0 0 1.5px rgba(49,46,129,.55)) drop-shadow(0 3px 0 rgba(49,46,129,.16))}';
  document.head.appendChild(css);
  // Games are laid out for about 760 x 500; on a big screen everything grows with it
  // (the kid screen fills a PC monitor), never smaller than designed.
  function fit() {
    var z = Math.max(1, Math.min(2.4, Math.min(window.innerWidth / 760, window.innerHeight / 500)));
    document.documentElement.style.zoom = String(z);
    if (document.body) document.body.style.minHeight = Math.floor(window.innerHeight / z) + 'px';
  }
  fit();
  window.addEventListener('resize', fit);
  document.addEventListener('DOMContentLoaded', fit);
  window.addEventListener('load', fit);
  // The frame may get its real size only after it is placed: look again a few times.
  [50, 200, 600, 1500].forEach(function (ms) { setTimeout(fit, ms); });
  try { new ResizeObserver(fit).observe(document.documentElement); } catch (e) { /* older engines: resize covers it */ }
  // A big, bright cursor (dark outline: visible on any background), a star over what can be tapped, a glow on hover,
  // and a ripple where a finger lands. The kid screen around the game uses the same (crundi-voice.js KID_CURSOR).
  var CUR_ARROW = 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 48 48"><path d="M5 3l34 18-15 4-6 15z" fill="#facc15" stroke="#111" stroke-width="3" stroke-linejoin="round"/><path d="M12 11l18 10" stroke="#fff" stroke-width="2.5" stroke-linecap="round" opacity=".8"/></svg>');
  var CUR_STAR = 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="56" height="56" viewBox="0 0 56 56"><path d="M28 3l7 16 17 2-13 12 4 17-15-9-15 9 4-17L4 21l17-2z" fill="#f472b6" stroke="#111" stroke-width="3" stroke-linejoin="round"/><circle cx="28" cy="28" r="4" fill="#fff"/></svg>');
  css.textContent += 'html,body{cursor:url("' + CUR_ARROW + '") 5 3,auto}button,.k-btn,a,[role=button],canvas,[data-tap]{cursor:url("' + CUR_STAR + '") 28 28,pointer}'
    + '.k-btn:hover,[data-tap]:hover{box-shadow:0 0 0 5px rgba(255,255,255,.4),0 0 28px 8px rgba(250,204,21,.55),0 6px 0 rgba(0,0,0,.25)}'
    + '.k-rip{position:fixed;width:20px;height:20px;margin:-10px 0 0 -10px;border-radius:50%;border:5px solid #facc15;pointer-events:none;animation:krip .6s ease-out forwards;z-index:99}@keyframes krip{to{transform:scale(5);opacity:0;border-color:#f472b6}}';
  // Less motion when the device asks for it.
  css.textContent += '@media (prefers-reduced-motion: reduce){.k-pop,.k-shake,.k-conf,.k-rip{animation:none!important}.k-conf{display:none}*{transition:none!important}}';

  // Soft sounds made here (Web Audio: no files, no network), at Cru's volume;
  // the voice screen sends { cru: 'volume', value: 0..1, muted } to change it.
  var actx = null, vol = 0.8, muted = false;
  window.addEventListener('message', function (e) {
    if (e.source !== P || !e.data || e.data.cru !== 'volume') return;
    vol = Math.max(0, Math.min(1, Number(e.data.value) || 0)); muted = !!e.data.muted;
  });
  var SOUNDS_FX = {
    tap: [[660, 0, 0.06, 0.05]],
    yes: [[523.25, 0, 0.12, 0.12], [659.25, 0.1, 0.12, 0.12], [783.99, 0.2, 0.22, 0.13]],
    miss: [[392, 0, 0.16, 0.07], [349.23, 0.14, 0.22, 0.06]],
    done: [[523.25, 0, 0.14, 0.12], [659.25, 0.13, 0.14, 0.12], [783.99, 0.26, 0.14, 0.12], [1046.5, 0.39, 0.4, 0.14]],
  };
  function sound(kind) {
    var notes = SOUNDS_FX[kind]; if (!notes || muted || vol <= 0) return;
    try {
      actx = actx || new (window.AudioContext || window.webkitAudioContext)();
      if (actx.state === 'suspended' && actx.resume) actx.resume();
      var t0 = actx.currentTime + 0.01;
      notes.forEach(function (n) {
        var o = actx.createOscillator(), g = actx.createGain(), at = t0 + n[1];
        o.type = 'sine'; o.frequency.value = n[0];
        g.gain.setValueAtTime(0.0001, at); g.gain.exponentialRampToValueAtTime(Math.max(0.0002, n[3] * vol), at + 0.015); g.gain.exponentialRampToValueAtTime(0.0001, at + n[2]);
        o.connect(g); g.connect(actx.destination); o.start(at); o.stop(at + n[2] + 0.05);
      });
    } catch (e) { /* no audio here */ }
  }
  document.addEventListener('pointerdown', function (e) {
    if (e.target && e.target.closest && e.target.closest('button,.k-btn,[data-tap]')) sound('tap');
    if (e.pointerType === 'mouse') return;
    var r = document.createElement('div'); r.className = 'k-rip'; r.style.left = e.clientX + 'px'; r.style.top = e.clientY + 'px';
    document.body.appendChild(r); setTimeout(function () { r.remove(); }, 700);
  }, true);

  var BRIGHT = ['#ef4444', '#f59e0b', '#10b981', '#3b82f6', '#8b5cf6', '#ec4899', '#14b8a6', '#f97316'];
  var LETTERS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('');
  var WORDS = { A: 'apple', B: 'ball', C: 'cake', D: 'drum', E: 'egg', F: 'fish', G: 'grapes', H: 'hat', I: 'igloo', J: 'jar', K: 'kite', L: 'leaf', M: 'moon', N: 'nest', O: 'orange', P: 'pear', Q: 'queen', R: 'rainbow', S: 'sun', T: 'tree', U: 'umbrella', V: 'van', W: 'watermelon', X: 'box', Y: 'yo-yo', Z: 'zebra' };
  var SOUNDS = { A: 'ah', B: 'buh', C: 'kuh', D: 'duh', E: 'eh', F: 'fff', G: 'guh', H: 'hhh', I: 'ih', J: 'juh', K: 'kuh', L: 'lll', M: 'mmm', N: 'nnn', O: 'oh', P: 'puh', Q: 'kwuh', R: 'rrr', S: 'sss', T: 'tuh', U: 'uh', V: 'vvv', W: 'wuh', X: 'ks', Y: 'yuh', Z: 'zzz' };
  var COLOURS = [{ name: 'red', hex: '#ef4444' }, { name: 'orange', hex: '#f97316' }, { name: 'yellow', hex: '#facc15' }, { name: 'green', hex: '#22c55e' }, { name: 'blue', hex: '#3b82f6' }, { name: 'purple', hex: '#8b5cf6' }, { name: 'pink', hex: '#ec4899' }];

  // Simple drawn pictures, 100x100.
  var S = function (body) { return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">' + body + '</svg>'; };
  var PICS = {
    apple: S('<path d="M50 30c-14-10-36-4-36 22 0 22 16 38 26 38 5 0 7-3 10-3s5 3 10 3c10 0 26-16 26-38 0-26-22-32-36-22z" fill="#ef4444"/><path d="M50 30c0-8 2-14 8-20" stroke="#7c4a1e" stroke-width="5" fill="none" stroke-linecap="round"/><ellipse cx="64" cy="18" rx="10" ry="5" fill="#22c55e" transform="rotate(-25 64 18)"/>'),
    ball: S('<circle cx="50" cy="50" r="38" fill="#3b82f6"/><path d="M12 50h76M50 12c-16 14-16 62 0 76M50 12c16 14 16 62 0 76" stroke="#fff" stroke-width="5" fill="none"/>'),
    cake: S('<rect x="18" y="48" width="64" height="38" rx="6" fill="#f9a8d4"/><rect x="18" y="48" width="64" height="12" fill="#fff"/><rect x="47" y="26" width="6" height="22" fill="#60a5fa"/><path d="M50 12c5 6 5 10 0 13-5-3-5-7 0-13z" fill="#f59e0b"/>'),
    drum: S('<ellipse cx="50" cy="34" rx="34" ry="12" fill="#fde68a"/><path d="M16 34v34c0 7 15 12 34 12s34-5 34-12V34c0 7-15 12-34 12S16 41 16 34z" fill="#ef4444"/><path d="M16 40l17 38M84 40l-17 38M50 46v34" stroke="#fde68a" stroke-width="3"/><path d="M60 8l-16 22M78 10L56 28" stroke="#7c4a1e" stroke-width="4" stroke-linecap="round"/>'),
    egg: S('<ellipse cx="50" cy="55" rx="28" ry="36" fill="#fef3c7" stroke="#d6b36a" stroke-width="3"/>'),
    fish: S('<ellipse cx="45" cy="50" rx="30" ry="20" fill="#f97316"/><path d="M72 50l18-16v32z" fill="#f97316"/><circle cx="32" cy="45" r="4" fill="#111"/>'),
    grapes: S('<g fill="#8b5cf6"><circle cx="40" cy="38" r="10"/><circle cx="60" cy="38" r="10"/><circle cx="50" cy="54" r="10"/><circle cx="30" cy="54" r="10"/><circle cx="70" cy="54" r="10"/><circle cx="40" cy="70" r="10"/><circle cx="60" cy="70" r="10"/><circle cx="50" cy="86" r="9"/></g><path d="M50 28V12" stroke="#7c4a1e" stroke-width="4"/>'),
    hat: S('<rect x="30" y="20" width="40" height="46" rx="4" fill="#1f2937"/><rect x="14" y="62" width="72" height="12" rx="6" fill="#1f2937"/><rect x="30" y="52" width="40" height="8" fill="#ef4444"/>'),
    igloo: S('<path d="M12 80a38 38 0 0 1 76 0z" fill="#e0f2fe" stroke="#7dd3fc" stroke-width="3"/><path d="M40 80a10 12 0 0 1 20 0z" fill="#334155"/><path d="M20 62h60M28 48h44M50 42v20" stroke="#7dd3fc" stroke-width="2"/>'),
    jar: S('<rect x="26" y="30" width="48" height="56" rx="10" fill="#fca5a5"/><rect x="30" y="18" width="40" height="14" rx="4" fill="#f59e0b"/><rect x="34" y="48" width="32" height="20" rx="4" fill="#fff"/>'),
    kite: S('<path d="M50 8L80 40 50 72 20 40z" fill="#ec4899"/><path d="M50 8v64M20 40h60" stroke="#fff" stroke-width="2"/><path d="M50 72c-6 6 6 10 0 16s6 8 0 10" stroke="#3b82f6" stroke-width="3" fill="none"/>'),
    leaf: S('<path d="M20 80C20 40 50 15 85 15c0 40-25 65-65 65z" fill="#22c55e"/><path d="M20 80L70 30" stroke="#166534" stroke-width="3"/>'),
    moon: S('<path d="M62 10a40 40 0 1 0 28 64A32 32 0 1 1 62 10z" fill="#fde047"/>'),
    nest: S('<ellipse cx="50" cy="66" rx="38" ry="18" fill="#a16207"/><ellipse cx="38" cy="56" rx="9" ry="11" fill="#bfdbfe"/><ellipse cx="54" cy="54" rx="9" ry="11" fill="#bfdbfe"/><ellipse cx="68" cy="58" rx="8" ry="10" fill="#bfdbfe"/><path d="M14 64c20 8 52 8 72 0" stroke="#713f12" stroke-width="3" fill="none"/>'),
    orange: S('<circle cx="50" cy="55" r="34" fill="#f97316"/><path d="M50 21c2-6 8-10 14-10" stroke="#166534" stroke-width="4" fill="none"/>'),
    pear: S('<path d="M50 20c-8 0-10 12-10 20-12 8-18 18-18 28 0 14 12 22 28 22s28-8 28-22c0-10-6-20-18-28 0-8-2-20-10-20z" fill="#a3e635"/><path d="M50 20V8" stroke="#7c4a1e" stroke-width="4"/>'),
    queen: S('<path d="M14 72l8-44 18 22 10-30 10 30 18-22 8 44z" fill="#facc15"/><rect x="14" y="72" width="72" height="12" rx="3" fill="#eab308"/><circle cx="50" cy="60" r="5" fill="#ef4444"/>'),
    rainbow: S('<g fill="none" stroke-width="7"><path d="M10 80a40 40 0 0 1 80 0" stroke="#ef4444"/><path d="M17 80a33 33 0 0 1 66 0" stroke="#f97316"/><path d="M24 80a26 26 0 0 1 52 0" stroke="#facc15"/><path d="M31 80a19 19 0 0 1 38 0" stroke="#22c55e"/><path d="M38 80a12 12 0 0 1 24 0" stroke="#3b82f6"/></g>'),
    sun: S('<circle cx="50" cy="50" r="20" fill="#facc15"/><g stroke="#facc15" stroke-width="6" stroke-linecap="round"><path d="M50 10v12M50 78v12M10 50h12M78 50h12M22 22l8 8M70 70l8 8M22 78l8-8M70 30l8-8"/></g>'),
    tree: S('<rect x="44" y="56" width="12" height="32" fill="#7c4a1e"/><circle cx="50" cy="40" r="28" fill="#22c55e"/>'),
    umbrella: S('<path d="M10 50a40 34 0 0 1 80 0z" fill="#8b5cf6"/><path d="M50 50v30c0 8-12 8-12 0" stroke="#334155" stroke-width="5" fill="none" stroke-linecap="round"/>'),
    van: S('<rect x="8" y="34" width="70" height="36" rx="6" fill="#3b82f6"/><path d="M78 44h8l8 12v14H78z" fill="#3b82f6"/><rect x="16" y="40" width="20" height="14" fill="#bfdbfe"/><circle cx="26" cy="74" r="9" fill="#111"/><circle cx="74" cy="74" r="9" fill="#111"/>'),
    watermelon: S('<path d="M8 40a42 42 0 0 0 84 0z" fill="#22c55e"/><path d="M16 40a34 34 0 0 0 68 0z" fill="#f87171"/><g fill="#111"><circle cx="38" cy="52" r="3"/><circle cx="50" cy="60" r="3"/><circle cx="62" cy="52" r="3"/></g>'),
    box: S('<path d="M14 36l36-16 36 16-36 16z" fill="#fbbf24"/><path d="M14 36v40l36 16V52z" fill="#d97706"/><path d="M86 36v40L50 92V52z" fill="#f59e0b"/>'),
    'yo-yo': S('<circle cx="50" cy="60" r="28" fill="#ef4444"/><circle cx="50" cy="60" r="8" fill="#fca5a5"/><path d="M50 32V6" stroke="#334155" stroke-width="3"/>'),
    zebra: S('<ellipse cx="50" cy="56" rx="34" ry="22" fill="#fff"/><g stroke="#111" stroke-width="6"><path d="M30 38v36M44 34v44M58 34v44M72 38v34"/></g><circle cx="86" cy="40" r="10" fill="#fff"/><circle cx="88" cy="38" r="2" fill="#111"/>'),
    // Animals, the body, the weather, everyday things (the trivia game's pictures).
    cat: S('<path d="M22 30l8-20 14 14h12l14-14 8 20v26c0 18-14 30-28 30S22 74 22 56z" fill="#f59e0b"/><circle cx="38" cy="48" r="5" fill="#111"/><circle cx="62" cy="48" r="5" fill="#111"/><path d="M46 60l4 4 4-4M30 62H12M30 68l-16 4M70 62h18M70 68l16 4" stroke="#111" stroke-width="2.5" fill="none"/>'),
    dog: S('<ellipse cx="50" cy="54" rx="28" ry="30" fill="#b45309"/><ellipse cx="22" cy="46" rx="10" ry="22" fill="#78350f"/><ellipse cx="78" cy="46" rx="10" ry="22" fill="#78350f"/><circle cx="40" cy="46" r="5" fill="#111"/><circle cx="60" cy="46" r="5" fill="#111"/><ellipse cx="50" cy="64" rx="8" ry="6" fill="#111"/><path d="M50 70v8" stroke="#111" stroke-width="3"/>'),
    cow: S('<ellipse cx="50" cy="52" rx="30" ry="30" fill="#fff"/><path d="M26 34l-12-10M74 34l12-10" stroke="#d6d3d1" stroke-width="7" stroke-linecap="round"/><ellipse cx="50" cy="70" rx="20" ry="13" fill="#f9a8d4"/><circle cx="43" cy="70" r="3" fill="#111"/><circle cx="57" cy="70" r="3" fill="#111"/><circle cx="38" cy="44" r="5" fill="#111"/><circle cx="62" cy="44" r="5" fill="#111"/><ellipse cx="68" cy="30" rx="9" ry="6" fill="#111"/>'),
    duck: S('<ellipse cx="46" cy="66" rx="34" ry="20" fill="#facc15"/><circle cx="66" cy="36" r="18" fill="#facc15"/><path d="M80 38l16 4-16 6z" fill="#f97316"/><circle cx="70" cy="32" r="3.5" fill="#111"/>'),
    pig: S('<circle cx="50" cy="52" r="34" fill="#f9a8d4"/><path d="M24 28l6 14M76 28l-6 14" stroke="#f472b6" stroke-width="8" stroke-linecap="round"/><ellipse cx="50" cy="62" rx="14" ry="10" fill="#f472b6"/><circle cx="45" cy="62" r="3" fill="#831843"/><circle cx="55" cy="62" r="3" fill="#831843"/><circle cx="38" cy="44" r="4" fill="#111"/><circle cx="62" cy="44" r="4" fill="#111"/>'),
    sheep: S('<g fill="#f5f5f4"><circle cx="30" cy="40" r="14"/><circle cx="50" cy="32" r="14"/><circle cx="70" cy="40" r="14"/><circle cx="30" cy="62" r="14"/><circle cx="70" cy="62" r="14"/><circle cx="50" cy="68" r="14"/></g><ellipse cx="50" cy="52" rx="14" ry="17" fill="#44403c"/><circle cx="45" cy="48" r="3" fill="#fff"/><circle cx="55" cy="48" r="3" fill="#fff"/>'),
    horse: S('<path d="M30 90V46c0-20 14-34 34-34l8 6-4 8 14 22c2 6-4 10-10 8L62 44v46z" fill="#92400e"/><path d="M42 14c-8 6-12 18-12 30" stroke="#451a03" stroke-width="8" fill="none"/><circle cx="64" cy="26" r="3.5" fill="#111"/>'),
    frog: S('<ellipse cx="50" cy="62" rx="36" ry="24" fill="#22c55e"/><circle cx="32" cy="36" r="13" fill="#22c55e"/><circle cx="68" cy="36" r="13" fill="#22c55e"/><circle cx="32" cy="36" r="6" fill="#111"/><circle cx="68" cy="36" r="6" fill="#111"/><path d="M30 66c10 8 30 8 40 0" stroke="#14532d" stroke-width="3" fill="none"/>'),
    lion: S('<circle cx="50" cy="50" r="40" fill="#b45309"/><circle cx="50" cy="52" r="26" fill="#facc15"/><circle cx="41" cy="46" r="4" fill="#111"/><circle cx="59" cy="46" r="4" fill="#111"/><path d="M44 58l6 5 6-5" stroke="#111" stroke-width="3" fill="none"/>'),
    bird: S('<ellipse cx="46" cy="56" rx="30" ry="22" fill="#3b82f6"/><circle cx="70" cy="40" r="14" fill="#3b82f6"/><path d="M82 40l12 4-12 4z" fill="#f59e0b"/><circle cx="73" cy="37" r="3" fill="#111"/><path d="M26 52c10-4 24-2 30 8" stroke="#1d4ed8" stroke-width="5" fill="none"/>'),
    bee: S('<ellipse cx="50" cy="58" rx="30" ry="22" fill="#facc15"/><path d="M40 37v42M56 36v44" stroke="#111" stroke-width="8"/><ellipse cx="40" cy="30" rx="12" ry="16" fill="#bae6fd" opacity=".9"/><ellipse cx="60" cy="30" rx="12" ry="16" fill="#bae6fd" opacity=".9"/><circle cx="78" cy="54" r="3" fill="#111"/>'),
    owl: S('<ellipse cx="50" cy="56" rx="32" ry="36" fill="#a16207"/><circle cx="37" cy="44" r="12" fill="#fff"/><circle cx="63" cy="44" r="12" fill="#fff"/><circle cx="37" cy="44" r="6" fill="#111"/><circle cx="63" cy="44" r="6" fill="#111"/><path d="M45 56l5 8 5-8z" fill="#f59e0b"/><path d="M24 22l10 10M76 22L66 32" stroke="#a16207" stroke-width="6"/>'),
    elephant: S('<ellipse cx="54" cy="50" rx="32" ry="28" fill="#94a3b8"/><ellipse cx="28" cy="44" rx="16" ry="20" fill="#64748b"/><path d="M70 60c4 14 0 26-10 30" stroke="#94a3b8" stroke-width="12" fill="none" stroke-linecap="round"/><circle cx="62" cy="42" r="4" fill="#111"/>'),
    mouse: S('<ellipse cx="52" cy="62" rx="30" ry="20" fill="#a8a29e"/><circle cx="30" cy="38" r="12" fill="#d6d3d1"/><circle cx="56" cy="38" r="12" fill="#d6d3d1"/><circle cx="34" cy="58" r="3" fill="#111"/><circle cx="22" cy="64" r="3" fill="#f472b6"/><path d="M82 64c10 0 12 14 2 18" stroke="#a8a29e" stroke-width="3" fill="none"/>'),
    chicken: S('<ellipse cx="50" cy="62" rx="30" ry="24" fill="#fff"/><circle cx="62" cy="36" r="14" fill="#fff"/><path d="M58 20c2-6 8-6 8 0 2-6 8-4 6 2" fill="#ef4444"/><path d="M76 36l10 3-10 4z" fill="#f59e0b"/><circle cx="64" cy="33" r="3" fill="#111"/>'),
    monkey: S('<circle cx="50" cy="48" r="30" fill="#92400e"/><circle cx="20" cy="46" r="10" fill="#92400e"/><circle cx="80" cy="46" r="10" fill="#92400e"/><ellipse cx="50" cy="56" rx="20" ry="16" fill="#fcd34d"/><circle cx="42" cy="44" r="4" fill="#111"/><circle cx="58" cy="44" r="4" fill="#111"/><path d="M42 62c4 4 12 4 16 0" stroke="#111" stroke-width="3" fill="none"/>'),
    octopus: S('<ellipse cx="50" cy="38" rx="28" ry="26" fill="#a855f7"/><g stroke="#a855f7" stroke-width="7" fill="none" stroke-linecap="round"><path d="M28 56c-6 10-2 20-10 26M40 60c-2 12 2 18-2 28M60 60c2 12-2 18 2 28M72 56c6 10 2 20 10 26"/></g><circle cx="40" cy="36" r="5" fill="#111"/><circle cx="60" cy="36" r="5" fill="#111"/>'),
    turtle: S('<ellipse cx="50" cy="58" rx="32" ry="20" fill="#16a34a"/><path d="M30 52h40M40 42l-6 30M60 42l6 30" stroke="#14532d" stroke-width="3"/><circle cx="88" cy="58" r="8" fill="#86efac"/><circle cx="90" cy="56" r="2" fill="#111"/><g fill="#86efac"><circle cx="28" cy="78" r="6"/><circle cx="72" cy="78" r="6"/></g>'),
    giraffe: S('<path d="M40 92V54h20v38zM50 54V18" stroke="#facc15" stroke-width="14"/><circle cx="56" cy="16" r="10" fill="#facc15"/><g fill="#b45309"><circle cx="46" cy="40" r="3"/><circle cx="52" cy="28" r="3"/><circle cx="46" cy="66" r="4"/><circle cx="54" cy="80" r="4"/></g><circle cx="59" cy="14" r="2" fill="#111"/>'),
    rabbit: S('<ellipse cx="38" cy="22" rx="8" ry="20" fill="#e7e5e4"/><ellipse cx="62" cy="22" rx="8" ry="20" fill="#e7e5e4"/><circle cx="50" cy="56" r="26" fill="#e7e5e4"/><circle cx="41" cy="52" r="4" fill="#111"/><circle cx="59" cy="52" r="4" fill="#111"/><circle cx="50" cy="62" r="4" fill="#f472b6"/>'),
    eye: S('<path d="M8 50c20-26 64-26 84 0-20 26-64 26-84 0z" fill="#fff"/><circle cx="50" cy="50" r="16" fill="#3b82f6"/><circle cx="50" cy="50" r="7" fill="#111"/>'),
    ear: S('<path d="M60 14c-22 0-34 16-34 36 0 14 8 18 8 30 0 8 6 12 12 12 10 0 12-10 18-18 8-10 18-16 18-30 0-18-10-30-22-30z" fill="#fdba74"/><path d="M46 46c0-12 18-12 18 0 0 8-10 8-10 18" stroke="#c2410c" stroke-width="4" fill="none"/>'),
    nose: S('<path d="M50 14c-4 20-18 40-18 52 0 10 8 14 18 14s18-4 18-14c0-12-14-32-18-52z" fill="#fdba74"/><ellipse cx="42" cy="70" rx="5" ry="3" fill="#9a3412"/><ellipse cx="58" cy="70" rx="5" ry="3" fill="#9a3412"/>'),
    hand: S('<path d="M30 90V48c0-6 8-6 8 0V18c0-6 8-6 8 0v28-34c0-6 8-6 8 0v34-28c0-6 8-6 8 0v32-18c0-6 8-6 8 0v34c0 16-10 24-24 24H40c-6 0-10-4-10-10z" fill="#fdba74"/>'),
    foot: S('<path d="M30 20c12-4 24 4 24 18v24c0 6 6 8 14 10 10 2 16 6 16 12 0 6-6 8-14 8H30c-6 0-8-4-8-10V34c0-6 2-12 8-14z" fill="#fdba74"/>'),
    mouth: S('<path d="M14 44c12 8 60 8 72 0-6 26-30 34-36 34s-30-8-36-34z" fill="#ef4444"/><path d="M22 46c10 4 46 4 56 0v6c-10 4-46 4-56 0z" fill="#fff"/>'),
    cloud: S('<g fill="#e2e8f0"><circle cx="34" cy="52" r="18"/><circle cx="54" cy="42" r="22"/><circle cx="72" cy="54" r="16"/><rect x="20" y="52" width="66" height="18" rx="9"/></g>'),
    rain: S('<g fill="#94a3b8"><circle cx="34" cy="38" r="16"/><circle cx="54" cy="30" r="20"/><circle cx="72" cy="40" r="14"/><rect x="20" y="38" width="66" height="16" rx="8"/></g><g stroke="#3b82f6" stroke-width="5" stroke-linecap="round"><path d="M32 64l-4 12M50 64l-4 14M68 64l-4 12"/></g>'),
    snow: S('<g stroke="#bae6fd" stroke-width="6" stroke-linecap="round"><path d="M50 10v80M15 30l70 40M85 30L15 70"/></g><circle cx="50" cy="50" r="8" fill="#fff"/>'),
    wind: S('<g stroke="#cbd5e1" stroke-width="6" fill="none" stroke-linecap="round"><path d="M10 36h52c10 0 12-16 2-16"/><path d="M10 52h70c10 0 12 16 2 16"/><path d="M10 68h36"/></g>'),
    flower: S('<path d="M50 56v38" stroke="#16a34a" stroke-width="5"/><g fill="#ec4899"><circle cx="50" cy="26" r="12"/><circle cx="68" cy="40" r="12"/><circle cx="62" cy="60" r="12"/><circle cx="38" cy="60" r="12"/><circle cx="32" cy="40" r="12"/></g><circle cx="50" cy="44" r="10" fill="#facc15"/>'),
    star: S('<path d="M50 6l12 30 32 2-25 20 9 32-28-18-28 18 9-32L6 38l32-2z" fill="#fde047"/>'),
    cup: S('<path d="M20 30h50l-6 52H26z" fill="#38bdf8"/><path d="M70 40c16 0 16 26 0 26" stroke="#38bdf8" stroke-width="7" fill="none"/>'),
    spoon: S('<ellipse cx="50" cy="28" rx="14" ry="20" fill="#cbd5e1"/><rect x="46" y="44" width="8" height="48" rx="4" fill="#cbd5e1"/>'),
    shoe: S('<path d="M10 70V44c10 0 16-8 26-8 6 14 20 18 34 20 12 2 20 6 20 14z" fill="#ef4444"/><rect x="10" y="70" width="80" height="10" rx="4" fill="#fff"/>'),
    bed: S('<rect x="10" y="50" width="80" height="22" rx="4" fill="#8b5cf6"/><rect x="10" y="34" width="12" height="52" fill="#7c2d12"/><rect x="78" y="44" width="12" height="42" fill="#7c2d12"/><rect x="24" y="40" width="22" height="12" rx="6" fill="#fff"/>'),
    toothbrush: S('<rect x="12" y="56" width="62" height="10" rx="5" fill="#38bdf8" transform="rotate(-20 50 60)"/><rect x="62" y="30" width="22" height="14" rx="3" fill="#fff" transform="rotate(-20 50 60)"/>'),
    book: S('<path d="M50 24c-12-8-28-8-38-4v60c10-4 26-4 38 4 12-8 28-8 38-4V20c-10-4-26-4-38 4z" fill="#3b82f6"/><path d="M50 24v60" stroke="#1e3a8a" stroke-width="3"/>'),
    car: S('<path d="M10 64V50l14-4 12-14h28l14 14 12 4v14z" fill="#ef4444"/><rect x="38" y="34" width="22" height="12" fill="#bfdbfe"/><circle cx="28" cy="68" r="10" fill="#111"/><circle cx="72" cy="68" r="10" fill="#111"/>'),
    house: S('<path d="M50 12L10 46h12v42h56V46h12z" fill="#f97316"/><rect x="42" y="60" width="16" height="28" fill="#7c2d12"/><rect x="26" y="52" width="12" height="12" fill="#bfdbfe"/><rect x="62" y="52" width="12" height="12" fill="#bfdbfe"/>'),
    boat: S('<path d="M10 62h80l-12 22H22z" fill="#7c2d12"/><path d="M50 10v50M50 14l30 40H50z" fill="#fff" stroke="#e2e8f0" stroke-width="3"/><path d="M0 90c16-6 34 6 50 0s34 6 50 0" stroke="#3b82f6" stroke-width="5" fill="none"/>'),
    train: S('<rect x="14" y="30" width="54" height="40" rx="6" fill="#22c55e"/><rect x="68" y="44" width="20" height="26" rx="4" fill="#16a34a"/><rect x="22" y="38" width="16" height="14" fill="#bfdbfe"/><rect x="44" y="38" width="16" height="14" fill="#bfdbfe"/><circle cx="28" cy="76" r="8" fill="#111"/><circle cx="56" cy="76" r="8" fill="#111"/><circle cx="80" cy="76" r="8" fill="#111"/>'),
    plane: S('<path d="M8 54l84-12-6 10-40 8-10 22H28l4-20-24 4z" fill="#e2e8f0"/><path d="M40 52L30 24h10l20 26z" fill="#94a3b8"/>'),
    mountain: S('<path d="M4 88l34-60 18 30 12-16 28 46z" fill="#64748b"/><path d="M38 28l-10 18 10-4 8 6z" fill="#fff"/>'),
    beach: S('<rect x="0" y="62" width="100" height="38" fill="#fde68a"/><path d="M0 62c16-6 34 6 50 0s34 6 50 0V40H0z" fill="#38bdf8"/><circle cx="80" cy="20" r="10" fill="#facc15"/>'),
    farm: S('<path d="M20 40l30-24 30 24v48H20z" fill="#dc2626"/><rect x="40" y="56" width="20" height="32" fill="#fff"/><path d="M40 56l20 32M60 56L40 88" stroke="#dc2626" stroke-width="3"/><rect x="0" y="88" width="100" height="12" fill="#22c55e"/>'),
    snail: S('<circle cx="56" cy="50" r="24" fill="#f59e0b"/><path d="M56 50m-12 0a12 12 0 1 1 12 12" stroke="#92400e" stroke-width="4" fill="none"/><path d="M10 78h70c4 0 6-4 4-6" fill="#a3e635" stroke="#a3e635" stroke-width="10" stroke-linecap="round"/><path d="M18 74V56M26 74V58" stroke="#a3e635" stroke-width="4"/>'),
    butterfly: S('<g fill="#ec4899"><ellipse cx="30" cy="36" rx="20" ry="16"/><ellipse cx="70" cy="36" rx="20" ry="16"/></g><g fill="#a855f7"><ellipse cx="32" cy="66" rx="14" ry="12"/><ellipse cx="68" cy="66" rx="14" ry="12"/></g><rect x="46" y="24" width="8" height="56" rx="4" fill="#111"/>'),
    whale: S('<path d="M8 54c0-18 22-28 46-28 22 0 34 12 34 28 0 10-8 18-22 18H28C16 72 8 64 8 54z" fill="#3b82f6"/><path d="M84 54l12-14v28z" fill="#3b82f6"/><circle cx="30" cy="50" r="4" fill="#111"/><path d="M50 26c0-10-6-14-6-14M50 26c0-10 6-14 6-14" stroke="#7dd3fc" stroke-width="3" fill="none"/>'),
    penguin: S('<ellipse cx="50" cy="54" rx="26" ry="36" fill="#111827"/><ellipse cx="50" cy="60" rx="17" ry="27" fill="#fff"/><circle cx="43" cy="34" r="3.5" fill="#fff"/><circle cx="57" cy="34" r="3.5" fill="#fff"/><path d="M45 42l5 6 5-6z" fill="#f59e0b"/>'),
  };
  function pic(word, px) {
    var s = PICS[String(word || '').toLowerCase()] || PICS.ball;
    px = px || 90;
    return s.replace('<svg ', '<svg width="' + px + '" height="' + px + '" ');
  }
  function shape(name, colour, px) {
    px = px || 90;
    var f = colour || '#3b82f6', b;
    if (name === 'square') b = '<rect x="14" y="14" width="72" height="72" rx="6" fill="' + f + '"/>';
    else if (name === 'triangle') b = '<path d="M50 10L90 86H10z" fill="' + f + '"/>';
    else if (name === 'star') b = '<path d="M50 6l12 30 32 2-25 20 9 32-28-18-28 18 9-32L6 38l32-2z" fill="' + f + '"/>';
    else if (name === 'heart') b = '<path d="M50 88S10 62 10 36c0-14 10-24 22-24 9 0 15 5 18 12 3-7 9-12 18-12 12 0 22 10 22 24 0 26-40 52-40 52z" fill="' + f + '"/>';
    else if (name === 'diamond') b = '<path d="M50 6l38 44-38 44-38-44z" fill="' + f + '"/>';
    else b = '<circle cx="50" cy="50" r="40" fill="' + f + '"/>';
    return '<svg xmlns="http://www.w3.org/2000/svg" width="' + px + '" height="' + px + '" viewBox="0 0 100 100">' + b + '</svg>';
  }
  function shuffle(a) { a = a.slice(); for (var i = a.length - 1; i > 0; i--) { var j = Math.floor(Math.random() * (i + 1)); var t = a[i]; a[i] = a[j]; a[j] = t; } return a; }
  function pick(a) { return a[Math.floor(Math.random() * a.length)]; }
  function sample(a, n) { return shuffle(a).slice(0, n); }
  function el(tag, cls, html) { var e = document.createElement(tag); if (cls) e.className = cls; if (html != null) e.innerHTML = html; return e; }
  var PRAISE = ['Great job!', 'You did it!', 'Wow, well done!', 'Amazing!', 'Super!', 'Brilliant!', 'Yay!', 'Fantastic!'];
  var AGAIN = ['Oops, try again!', 'Almost! Try again.', 'Good try! Have another go.'];
  function confetti() {
    for (var i = 0; i < 30; i++) {
      var c = el('div', 'k-conf');
      c.style.left = Math.random() * 100 + 'vw'; c.style.top = '-20px';
      c.style.background = pick(BRIGHT); c.style.animationDelay = Math.random() * 0.3 + 's';
      document.body.appendChild(c);
      (function (c) { setTimeout(function () { c.remove(); }, 1800); })(c);
    }
  }
  function level(skill) { var v = Number(init.levels && init.levels[skill]); return v >= 1 && v <= 5 ? v : 1; }
  /**
   * The letters to use: opts.letters ("ABC" or ["A","B"]); else, for [skill],
   * the ones being learnt (focus) with a couple already known; else as many
   * from A as the level allows.
   */
  function letters(dflt, skill) {
    var l = opts.letters;
    if (typeof l === 'string') l = l.toUpperCase().replace(/[^A-Z]/g, '').split('');
    if (Array.isArray(l)) l = l.map(function (x) { return String(x).toUpperCase(); }).filter(function (x) { return /^[A-Z]$/.test(x); });
    if (l && l.length) return l;
    if (dflt) return dflt;
    skill = skill || 'letters';
    var isL = function (x) { return /^[A-Z]$/.test(x); };
    var focus = ((init.focus && init.focus[skill]) || []).filter(isL).slice(0, 4);
    var known = ((init.avoid && init.avoid[skill]) || []).filter(isL);
    if (focus.length) { var set = focus.concat(sample(known, 2)); if (set.length < 3) set = set.concat(sample(LETTERS.filter(function (x) { return set.indexOf(x) < 0; }), 3 - set.length)); return set; }
    return LETTERS.slice(0, [0, 6, 10, 16, 21, 26][level(skill)]);
  }
  // The question being answered: its first answer is what the skill level learns from.
  var q = null;
  function result(ok) {
    if (!q) return;
    post({ cru: 'event', kind: 'result', skill: q.skill, item: q.item, ok: !!ok, ms: Date.now() - q.at, detail: q.item });
    q = null;
  }
  var lastSay = 0;
  // Game events for the host: { type: 'game', event, item?, detail?, streak? } (README "Messages").
  var EVENT_OF = { wrong: 'miss', level: 'level-up' };
  var streak = 0, lastTouch = Date.now(), lastProgress = Date.now(), idleSent = false, stuckSent = false, touched = false;
  function gameEvent(kind, item, detail) {
    var ev = EVENT_OF[kind] || kind;
    if (ev === 'correct') { streak++; lastProgress = Date.now(); stuckSent = false; } else if (ev === 'miss') streak = 0;
    post({ type: 'game', event: ev, item: String(item == null ? '' : item).slice(0, 40), detail: String(detail == null ? '' : detail).slice(0, 200), streak: ev === 'correct' ? streak : 0 });
  }
  document.addEventListener('pointerdown', function () { lastTouch = Date.now(); idleSent = false; touched = true; }, true);
  // Idle: nothing touched for 30 s. Stuck: touching, but nothing right for 20 s.
  setInterval(function () {
    var t = Date.now();
    if (paused || waiting) return;
    if (!idleSent && t - lastTouch > 30000) { idleSent = true; gameEvent('idle'); }
    else if (touched && !stuckSent && t - lastProgress > 20000 && t - lastTouch < 8000) { stuckSent = true; gameEvent('stuck', q ? q.item : ''); }
  }, 2000);
  // Commands from the host (Cru's game_command): { type: 'command', action }.
  var handlers = {}, paused = false, veil = null;
  window.addEventListener('message', function (e) {
    if (e.source !== P || !e.data || e.data.type !== 'command') return;
    var a = String(e.data.action || '');
    if (handlers[a]) { try { handlers[a](e.data); } catch (x) { /* the game's own */ } return; }
    if (a === 'pause' && !veil) { paused = true; veil = el('div', '', ''); veil.style.cssText = 'position:fixed;inset:0;background:rgba(10,10,15,.6);z-index:50'; document.body.appendChild(veil); }
    else if (a === 'resume' && veil) { paused = false; veil.remove(); veil = null; lastTouch = lastProgress = Date.now(); }
    else if (a === 'end') window.kid.done('ended by Cru');
  });
  var waiting = !!window.CRU_WAITING;
  if (!waiting) setTimeout(function () { gameEvent('start', '', document.title || ''); }, 50);
  window.kid = {
    opts: opts,
    say: function (t) { lastSay = Date.now(); post({ cru: 'say', text: String(t || '').slice(0, 300) }); },
    tell: function (t) { post({ cru: 'tell', text: String(t || '').slice(0, 300) }); },
    event: function (kind, detail, item) { gameEvent(String(kind || 'tap'), item == null ? detail : item, detail); },
    /** A game's answer to a command from Cru: kid.on('easier', fn); next, easier, harder, hint, pause, resume, end. */
    on: function (action, fn) { handlers[String(action)] = fn; },
    get streak() { return streak; },
    init: init, level: level,
    ask: function (skill, item) { q = { skill: String(skill || ''), item: String(item == null ? '' : item).slice(0, 40), at: Date.now(), missed: false }; },
    hint: function () { if (q) q.missed = true; gameEvent('hint', q ? q.item : ''); },
    correct: function (what, line) { if (q) result(!q.missed); this.event('correct', what); sound('yes'); confetti(); this.say(line || pick(PRAISE)); },
    wrong: function (what, line) { if (q && !q.missed) { q.missed = true; } this.event('wrong', what); sound('miss'); this.say(line || pick(AGAIN)); },
    sound: sound,
    done: function (summary) { this.event('finished', summary || ''); sound('done'); confetti(); setTimeout(confetti, 500); this.tell('[Game: finished] ' + (summary || '')); },
    pic: pic, shape: shape, shuffle: shuffle, pick: pick, sample: sample, el: el, confetti: confetti, letters: letters,
    LETTERS: LETTERS, WORDS: WORDS, SOUNDS: SOUNDS, COLOURS: COLOURS, BRIGHT: BRIGHT, PRAISE: PRAISE,
  };
})();
