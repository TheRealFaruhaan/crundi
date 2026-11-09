# Ready-to-paste snippets

All of these are in `game-template.html` already (which also shows the
`{type:'game'}` events and `{type:'command'}` handling in full). They pass the library validator
(no URLs except the SVG namespace inside `data:`, no storage, no `<meta>`).

## Cursor, hover glow and touch ripple

A yellow outlined arrow everywhere, a pink star (hotspot in its centre) over
tappables, a glow on hover only where a real mouse is, a ripple on touch. Mark
tappable non-buttons with `data-tap`.

```html
<style>
html,body,*{cursor:url(data:image/svg+xml,%3Csvg%20xmlns=%27http://www.w3.org/2000/svg%27%20width=%2756%27%20height=%2756%27%20viewBox=%270%200%2056%2056%27%3E%3Cpath%20d=%27M5%204v42l11-10%208%2017%209-4-8-17h15z%27%20fill=%27%23FFD400%27%20stroke=%27%231F2937%27%20stroke-width=%274%27%20stroke-linejoin=%27round%27/%3E%3C/svg%3E) 6 5,auto}
button,button *,.k-btn,.k-btn *,[data-tap],[data-tap] *{cursor:url(data:image/svg+xml,%3Csvg%20xmlns=%27http://www.w3.org/2000/svg%27%20width=%2764%27%20height=%2764%27%20viewBox=%270%200%2064%2064%27%3E%3Cpath%20d=%27M32%206L39.1%2023.3L57.7%2024.7L43.4%2036.7L47.9%2054.8L32%2045L16.1%2054.8L20.6%2036.7L6.3%2024.7L24.9%2023.3Z%27%20fill=%27%23FF5FA8%27%20stroke=%27%231F2937%27%20stroke-width=%274%27%20stroke-linejoin=%27round%27/%3E%3Ccircle%20cx=%2732%27%20cy=%2734%27%20r=%274%27%20fill=%27%23fff%27/%3E%3C/svg%3E) 32 33,pointer}
@media (hover:hover){button:hover,.k-btn:hover,[data-tap]:hover{filter:brightness(1.12);box-shadow:0 0 0 6px rgba(255,255,255,.55),0 0 28px 8px rgba(255,212,0,.55)}}
.k-ripple{position:fixed;width:90px;height:90px;margin:-45px 0 0 -45px;border-radius:50%;border:6px solid #FFD400;pointer-events:none;animation:kripple .5s ease-out forwards}
@keyframes kripple{from{transform:scale(.3);opacity:.9}to{transform:scale(1.4);opacity:0}}
@media (prefers-reduced-motion:reduce){.k-ripple{animation:none;opacity:.6;transition:opacity .4s}}
</style>
<script>
addEventListener('pointerdown',function(e){if(e.pointerType==='mouse')return;var r=document.createElement('div');r.className='k-ripple';r.style.left=e.clientX+'px';r.style.top=e.clientY+'px';r.style.borderColor=['#FFD400','#FF5FA8','#2F9BFF','#2EE58A'][Math.random()*4|0];document.body.appendChild(r);setTimeout(function(){r.remove()},600)},{passive:true});
</script>
```

## Sounds: tap, success, gentle miss, progress step, fanfare

Made in the page with Web Audio, soft and short; honours the host's mute/volume
(options `sound:false`, `volume:0-1`, or `{cru:'mute',muted}` / `{cru:'volume',value}`
messages — the library README defines the final mechanism; if the kit gains
`kid.sfx`, that is used).

```js
var sfx = window.kid && kid.sfx || (function () {
  var ac = null, vol = 0.7, muted = false, o = {};
  try { o = (window.kid && kid.opts) || window.CRU_OPTIONS || {}; } catch (e) {}
  if (o.sound === false || o.muted) muted = true;
  if (typeof o.volume === 'number') vol = Math.max(0, Math.min(1, o.volume));
  addEventListener('message', function (e) {
    var d = e.data || {}; if (e.source !== window.parent) return;
    if (d.cru === 'mute') muted = !!d.muted;
    if (d.cru === 'volume') vol = Math.max(0, Math.min(1, Number(d.value) || 0));
    if (d.type === 'command' && (d.action === 'mute' || d.action === 'unmute')) muted = d.action === 'mute';
  });
  function note(f, at, len, type, peak) {
    if (muted || vol <= 0) return;
    try {
      ac = ac || new (window.AudioContext || window.webkitAudioContext)();
      if (ac.state === 'suspended') ac.resume();
      var t = ac.currentTime + at, osc = ac.createOscillator(), g = ac.createGain();
      osc.type = type || 'sine'; osc.frequency.setValueAtTime(f, t);
      g.gain.setValueAtTime(0.0001, t);
      g.gain.exponentialRampToValueAtTime(Math.max(0.0002, (peak || 0.12) * vol), t + 0.015);
      g.gain.exponentialRampToValueAtTime(0.0001, t + len);
      osc.connect(g); g.connect(ac.destination); osc.start(t); osc.stop(t + len + 0.05);
    } catch (e) {}
  }
  return {
    tap: function () { note(660, 0, 0.08, 'sine', 0.06); },
    success: function () { note(523, 0, 0.18); note(659, 0.09, 0.18); note(784, 0.18, 0.32); },
    miss: function () { note(392, 0, 0.2, 'triangle', 0.06); note(440, 0.13, 0.25, 'triangle', 0.06); },
    step: function () { note(880, 0, 0.12, 'sine', 0.08); note(1175, 0.07, 0.16, 'sine', 0.08); },
    fanfare: function () { [523, 659, 784, 1047].forEach(function (f, i) { note(f, i * 0.12, 0.25); }); note(1047, 0.62, 0.45, 'sine', 0.14); }
  };
})();
```

Pair each with a visual: tap → ripple/pop; success → sparkle/grow + voice line;
miss → wobble + glow toward the answer + encouraging line; progress → a star added;
finish → party + summary. For 12 m–3 y play `sfx.tap()` on a miss, not `sfx.miss()`.

## Reduced motion

```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation-duration:.01ms !important; animation-iteration-count:1 !important; transition-duration:.01ms !important; }
  .k-conf, .spark { display:none !important; }
  .glow { outline:8px solid #F0C808; outline-offset:6px; }   /* glow becomes a steady outline */
  .yay  { outline:8px solid #009E73; outline-offset:6px; }   /* success becomes a colour ring */
}
```

## Host bridge shim (kit present, Cru's show_screen html, or standalone)

```js
var standalone = window.parent === window;
var kid = window.kid || (function () {
  function post(m) { if (!standalone) { try { window.parent.postMessage(m, '*'); } catch (e) {} } }
  return {
    opts: window.CRU_OPTIONS || {},
    say: function (t) {
      if (!standalone) return post({ cru: 'say', text: String(t || '').slice(0, 300) });
      try { speechSynthesis.cancel(); speechSynthesis.speak(new SpeechSynthesisUtterance(t)); } catch (e) {}
    },
    tell: function (t) { post({ cru: 'tell', text: String(t || '').slice(0, 300) }); },
    event: function (k, d) { post({ cru: 'event', kind: String(k || 'tap'), detail: String(d == null ? '' : d).slice(0, 200) }); },
    done: function (s) { this.event('finished', s || ''); this.tell('[Game: finished] ' + (s || '')); }
  };
})();
```
