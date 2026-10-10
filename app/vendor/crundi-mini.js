/*
 * Mini, alive.
 *
 * The robot from the logo, with legs, as a small character that turns up in a
 * chat when someone is left waiting:
 *
 *   queue   a message has been sent but Claude has not picked it up for a
 *           while. Mini walks in along the top of the waiting message, knocks
 *           on the conversation behind it, paces, checks its watch, taps a
 *           foot, shakes its head.
 *   sit     a suggested reply has been sitting there untouched. Mini sits on
 *           its corner and swings its legs. When the suggestion goes, the seat
 *           goes: it falls, picks itself up and runs off with its arms up.
 *   nudge   Claude finished a while ago and nothing has been typed. Mini
 *           stands by the Send button and points at the message box, mimes
 *           typing, waves, and eventually falls asleep. A suggestion being
 *           there does not prevent this: if it is sitting on one it climbs
 *           down and comes over, and points at the suggestion too.
 *
 * It is drawn in code and moved in code. Nothing here is a keyframe animation
 * played from the start: every part of the body (each arm, each leg, the head,
 * the eyes, the lean of the body) is a number on a spring, and a behaviour only
 * ever says where those numbers should go next. So whatever Mini is doing when
 * something changes, it carries on from exactly that pose. It cannot jump from
 * one state to another because there are no states to jump between.
 *
 * A fall of more than a couple of its own heights is taken by parachute.
 *
 * Its eyes follow the mouse when the mouse is near. Tap it and it is annoyed,
 * a different way each time: it stamps, shoos you off, turns its back, steps
 * out of reach, or leaves the pane and peers back in. Once it has been
 * bothered it is wary of the cursor too, and backs away when the cursor comes
 * close, or cowers if it has nowhere to go. Keep tapping and it storms off.
 *
 * In a sleeping chat it does none of that. If it feels like it, it comes and
 * stands on the Resume button and shows it to you; if the conversation is a
 * long one it frets, and goes and points at "Compact and resume" instead.
 *
 * It is the logo. Before it turns up, the head in the top bar grows a body,
 * arms and legs and walks off the screen; when it has left the chat it comes
 * back along the top bar at the pace it left with and folds away into the logo
 * again. So there is only ever one Mini on the page, however many chats are
 * open, and while it is out the logo's place is empty.
 *
 * Only the robot itself takes a tap; the layer it lives on lets everything
 * else through. It does not appear at all for someone who has asked for
 * reduced motion.
 */
(function () {
  'use strict';
  if (window.CrundiMini) return;

  var REDUCED = false;
  try { REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches; } catch (e) { /* old browser: animate */ }

  var CANCEL = { cancelled: true };
  var GROUND = 90;        // feet, in the drawing's own units
  var SIT_DROP = 12;      // how far the body sinks to rest on an edge
  var G = 1500;           // gravity, px per second squared
  var uid = 0;

  // ─── Styles (once) ───
  function addStyle() {
    if (document.getElementById('cm-style')) return;
    var st = document.createElement('style');
    st.id = 'cm-style';
    st.textContent = [
      '.cm-top{position:fixed;left:0;top:0;right:0;overflow:hidden;pointer-events:none;z-index:2147483000}','.cm-top .cm{pointer-events:none;cursor:default}',
      '.cm-layer{position:absolute;left:0;top:0;right:0;bottom:0;overflow:hidden;pointer-events:none;z-index:16;contain:layout style}',
      '.cm{position:absolute;left:0;top:0;will-change:transform,opacity;pointer-events:auto;cursor:pointer;-webkit-tap-highlight-color:transparent;touch-action:manipulation}',
      '.cm svg{display:block;overflow:visible}',
      '.cm-fx{position:absolute;left:0;top:0;pointer-events:none;will-change:transform,opacity}',
      '.cm-fx svg{display:block;overflow:visible}',
      '@keyframes cm-knock{0%{opacity:0;transform:scale(.3)}25%{opacity:1}100%{opacity:0;transform:scale(1.5)}}',
      '@keyframes cm-dust{0%{opacity:.55;transform:translate(0,0) scale(.4)}100%{opacity:0;transform:translate(var(--dx,0),-9px) scale(1.5)}}',
      '@keyframes cm-rise{0%{opacity:0;transform:translate(0,4px) scale(.6)}20%{opacity:1}100%{opacity:0;transform:translate(var(--dx,6px),-26px) scale(1.1)}}',
      '@keyframes cm-pop{0%{opacity:0;transform:scale(.2)}18%{opacity:1;transform:scale(1.15)}30%,78%{opacity:1;transform:scale(1)}100%{opacity:0;transform:scale(.8)}}',
      '@keyframes cm-ring{0%{opacity:.9;transform:scale(.3)}100%{opacity:0;transform:scale(1.6)}}',
      '@keyframes cm-drop{0%{opacity:0;transform:translateY(-3px)}20%{opacity:1}100%{opacity:0;transform:translateY(14px)}}',
      '@keyframes cm-fall{0%{opacity:0;transform:translate(0,-6px) rotate(0)}15%{opacity:1}100%{opacity:0;transform:translate(var(--dx,6px),26px) rotate(260deg)}}',
      '@keyframes cm-plane{0%{opacity:0;transform:translate(0,0) rotate(-20deg) scale(.6)}12%{opacity:1}100%{opacity:0;transform:translate(var(--dx,-60px),var(--dy,-110px)) rotate(-34deg) scale(1)}}'
    ].join('\n');
    document.head.appendChild(st);
  }

  // ─── The drawing ───
  function markup(id) {
    var g = 'cmg' + id, c = 'cmc' + id;
    var A = '#fbbf24', D = '#0d0d16';
    return '<svg viewBox="0 0 96 92" aria-hidden="true">'
      + '<defs><linearGradient id="' + g + '" gradientUnits="userSpaceOnUse" x1="14" y1="10" x2="82" y2="84">'
      + '<stop offset="0" stop-color="#818cf8"/><stop offset="1" stop-color="#4f46e5"/></linearGradient>'
      + '<clipPath id="' + c + '"><rect x="23" y="12" width="50" height="37" rx="15"/></clipPath></defs>'
      + '<ellipse data-p="shadow" cx="48" cy="90" rx="21" ry="3.2" fill="#000" opacity=".32"/>'
      + '<g data-p="root">'
      +   '<g data-p="chute" opacity="0">'
      +     '<path d="M12 -8L33 56M36 -6L40 54M60 -6L56 54M84 -8L63 56" stroke="#c7c9e8" stroke-width="1.4" fill="none"/>'
      +     '<path d="M6 -8Q48 -66 90 -8Q78 -17 69 -8Q58 -17 48 -8Q38 -17 27 -8Q18 -17 6 -8Z" fill="' + A + '"/>'
      +     '<path d="M27 -8Q30 -44 48 -51Q38 -30 48 -8Q38 -17 27 -8ZM69 -8Q66 -44 48 -51Q58 -30 48 -8Q58 -17 69 -8Z" fill="#818cf8"/>'
      +   '</g>'
      +   '<rect data-p="legL" x="36" y="77" width="9" height="13" rx="4.2" fill="' + A + '"/>'
      +   '<rect data-p="legR" x="51" y="77" width="9" height="13" rx="4.2" fill="' + A + '"/>'
      +   '<g data-p="up">'
      +     '<g data-p="torso"><rect x="32" y="54" width="32" height="24" rx="9" fill="url(#' + g + ')"/>'
      +     '<circle data-p="light" cx="48" cy="66" r="4.8" fill="' + D + '" opacity=".6"/>'
      +     '<g data-p="back" opacity="0"><path d="M41 62H55M41 67H55M41 72H55" stroke="' + D + '" stroke-opacity=".4" stroke-width="2.2" stroke-linecap="round"/></g></g>'
      +     '<g data-p="head">'
      +       '<path data-p="ant" d="M48 13L48 6" fill="none" stroke="' + A + '" stroke-width="4" stroke-linecap="round"/>'
      +       '<circle data-p="tip" cx="48" cy="5" r="4.5" fill="' + A + '"/>'
      +       '<rect x="23" y="12" width="50" height="37" rx="15" fill="url(#' + g + ')"/>'
      +       '<g clip-path="url(#' + c + ')"><g data-p="eyes">'
      +         '<rect data-p="eyeL" x="36.5" y="24" width="7.5" height="13" rx="3.7" fill="' + D + '"/>'
      +         '<rect data-p="eyeR" x="52" y="24" width="7.5" height="13" rx="3.7" fill="' + D + '"/>'
      +       '</g></g>'
      +     '</g>'
      +     '<rect data-p="armL" x="20" y="56" width="7" height="16" rx="3.5" fill="' + A + '"/>'
      +     '<rect data-p="armR" x="69" y="56" width="7" height="16" rx="3.5" fill="' + A + '"/>'
      +   '</g>'
      + '</g></svg>';
  }

  // Small things that appear around Mini and vanish: all drawn, no fonts.
  var FX = {
    knock: { w: 22, h: 22, d: 420, a: 'cm-knock', s: '<path d="M11 2V7M3 6L7 9M19 6L15 9" stroke="#fbbf24" stroke-width="2.4" stroke-linecap="round" fill="none"/>' },
    dust: { w: 12, h: 12, d: 520, a: 'cm-dust', s: '<circle cx="6" cy="6" r="5" fill="#8888a8"/>' },
    puff: { w: 14, h: 10, d: 900, a: 'cm-rise', s: '<ellipse cx="7" cy="5" rx="6" ry="4" fill="#8888a8" opacity=".7"/>' },
    z: { w: 12, h: 12, d: 1700, a: 'cm-rise', s: '<path d="M2 2H10L2 10H10" stroke="#a5b4fc" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>' },
    note: { w: 12, h: 16, d: 1500, a: 'cm-rise', s: '<path d="M4 12V3L10 1.5V10" stroke="#fbbf24" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" fill="none"/><circle cx="2.6" cy="12" r="2.4" fill="#fbbf24"/><circle cx="8.6" cy="10" r="2.4" fill="#fbbf24"/>' },
    bang: { w: 8, h: 18, d: 900, a: 'cm-pop', s: '<path d="M4 2V10" stroke="#fbbf24" stroke-width="3.4" stroke-linecap="round"/><circle cx="4" cy="15.5" r="2" fill="#fbbf24"/>' },
    ask: { w: 14, h: 20, d: 1500, a: 'cm-pop', s: '<path d="M3 6C3 1 11 1 11 6C11 9 7 9 7 13" stroke="#fbbf24" stroke-width="3" stroke-linecap="round" fill="none"/><circle cx="7" cy="17.6" r="1.9" fill="#fbbf24"/>' },
    dots: { w: 34, h: 22, d: 2300, a: 'cm-pop', s: '<path d="M4 1H30A3 3 0 0 1 33 4V13A3 3 0 0 1 30 16H14L9 21V16H4A3 3 0 0 1 1 13V4A3 3 0 0 1 4 1Z" fill="#1a1a28" stroke="#3a3a55" stroke-width="1.2"/><circle cx="10" cy="8.5" r="2.1" fill="#a5b4fc"/><circle cx="17" cy="8.5" r="2.1" fill="#a5b4fc"/><circle cx="24" cy="8.5" r="2.1" fill="#a5b4fc"/>' },
    ring: { w: 20, h: 20, d: 520, a: 'cm-ring', s: '<circle cx="10" cy="10" r="8" fill="none" stroke="#818cf8" stroke-width="2"/>' },
    sweat: { w: 8, h: 11, d: 700, a: 'cm-drop', s: '<path d="M4 1C6 4 7 5.5 7 7.5A3 3 0 0 1 1 7.5C1 5.5 2 4 4 1Z" fill="#7dd3fc"/>' },
    anger: { w: 16, h: 16, d: 900, a: 'cm-pop', s: '<path d="M6 2V6H2M10 2V6H14M6 14V10H2M10 14V10H14" stroke="#fb7185" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>' },
    spark: { w: 14, h: 14, d: 700, a: 'cm-pop', s: '<path d="M7 0L8.6 5.4L14 7L8.6 8.6L7 14L5.4 8.6L0 7L5.4 5.4Z" fill="#fbbf24"/>' },
    confetti: { w: 6, h: 9, d: 1300, a: 'cm-fall', s: '<rect width="6" height="9" rx="1.5" fill="var(--cf,#818cf8)"/>' },
    plane: { w: 18, h: 14, d: 1100, a: 'cm-plane', s: '<path d="M1 6L17 1L11 13L8 8Z" fill="#e8e8f0"/><path d="M8 8L17 1" stroke="#8888a8" stroke-width="1"/>' }
  };

  function clamp(v, a, b) { return v < a ? a : v > b ? b : v; }
  function rnd(a, b) { return a + Math.random() * (b - a); }
  function pick(list) { return list[Math.floor(Math.random() * list.length)]; }
  // What to do next, without doing the same thing over and over: every choice
  // is dealt from a shuffled pack, each one once before any comes round again,
  // and never the same one twice running when a pack is reshuffled. The packs
  // last as long as the page, so a scene that starts again does not open the
  // way it opened last time.
  var PACKS = {};
  function deal(key, n) {
    var p = PACKS[key];
    if (!p || !p.q.length || p.n !== n) {
      var q = [], last = p && p.n === n ? p.last : -1;
      for (var i = 0; i < n; i++) q.push(i);
      for (var j = n - 1; j > 0; j--) { var r = Math.floor(Math.random() * (j + 1)), t = q[j]; q[j] = q[r]; q[r] = t; }
      if (n > 1 && q[n - 1] === last) { q[n - 1] = q[0]; q[0] = last; }
      p = PACKS[key] = { q: q, last: last, n: n };
    }
    p.last = p.q.pop();
    return p.last;
  }

  // ─── The actor: a body on springs ───
  // [stiffness, damping]. Lower damping is bouncier.
  var SPRING = {
    turn: [130, 21], lean: [150, 22], squash: [330, 13], headRot: [160, 19], headY: [170, 20],
    eyeX: [230, 26], eyeY: [230, 26], lid: [420, 34], armL: [165, 16], armR: [165, 16],
    legL: [260, 24], legR: [260, 24], sit: [95, 17], swing: [50, 13], shake: [70, 15], alpha: [60, 15], armSwing: [90, 18], armsBack: [110, 19], grow: [120, 12], reach: [150, 20], chute: [150, 13]
  };
  var REST = { turn: 0, lean: 0, squash: 1, headRot: 0, headY: 0, eyeX: 0, eyeY: 0, lid: 0, armL: 0, armR: 0, legL: 0, legR: 0, sit: 0, swing: 0, shake: 0, alpha: 1, armSwing: 1, armsBack: 0, grow: 1, reach: 0, chute: 0 };

  function Actor(layer, size) {
    addStyle();
    this.layer = layer;
    this.k = size / 92;
    this.el = document.createElement('div');
    this.el.className = 'cm';
    this.el.style.opacity = '0';
    this.el.innerHTML = markup(++uid);
    var svg = this.el.firstChild;
    svg.setAttribute('width', (96 * this.k).toFixed(2));
    svg.setAttribute('height', (92 * this.k).toFixed(2));
    this.p = {};
    var nodes = this.el.querySelectorAll('[data-p]');
    for (var i = 0; i < nodes.length; i++) this.p[nodes[i].getAttribute('data-p')] = nodes[i];
    layer.appendChild(this.el);

    this.v = {}; this.t = {}; this.vel = {};
    for (var key in REST) { this.v[key] = REST[key]; this.t[key] = REST[key]; this.vel[key] = 0; }
    this.v.alpha = 0; this.t.alpha = 0;
    this.x = 0; this.vx = 0; this.tx = null; this.speed = 60; this.onArrive = null;
    this.baseY = 0; this.yOff = 0; this.vy = 0; this.air = false; this.onLand = null;
    this.surf = null;           // function returning { y, l, r } in layer pixels
    this.clampX = true;
    this.phase = 0; this.walk = 0; this.time = 0; this.ant = 0; this.antV = 0;
    this.nextBlink = 1.5; this.blink = 0; this.autoBlink = true;
    this.lookX = 0; this.lookY = 0; this.lookTX = 0; this.lookTY = 0;   // eyes drawn towards the cursor, on top of everything else
    this.waits = []; this.tok = 0; this.dead = false;
    ACTORS.push(this);
    startLoop();
  }

  Actor.prototype.destroy = function () {
    this.dead = true; this.tok++;
    this.waits.forEach(function (w) { w.rej(CANCEL); });
    this.waits = [];
    if (this.el.parentNode) this.el.parentNode.removeChild(this.el);
    var i = ACTORS.indexOf(this);
    if (i >= 0) ACTORS.splice(i, 1);
  };

  Actor.prototype.fx = function (name, dx, dy, opt) {
    var f = FX[name];
    if (!f || this.dead) return;
    var n = document.createElement('div');
    n.className = 'cm-fx';
    var x = this.x + (dx || 0) * this.k - f.w / 2, y = this.baseY + this.yOff - (GROUND - (dy || 0)) * this.k - f.h / 2;
    n.style.transform = 'translate3d(' + x.toFixed(1) + 'px,' + y.toFixed(1) + 'px,0)';
    n.innerHTML = '<svg width="' + f.w + '" height="' + f.h + '" viewBox="0 0 ' + f.w + ' ' + f.h + '" style="animation:' + f.a + ' ' + f.d + 'ms ease-out both;transform-origin:center' + (opt || '') + '">' + f.s + '</svg>';
    this.layer.appendChild(n);
    setTimeout(function () { if (n.parentNode) n.parentNode.removeChild(n); }, f.d + 60);
  };

  // The same, at a place in the pane rather than a place on the body.
  Actor.prototype.fxAt = function (name, x, y) {
    var k = this.k;
    this.fx(name, 48 + (x - this.x) / k, GROUND + (y - this.baseY - this.yOff) / k);
  };

  Actor.prototype.step = function (dt) {
    var v = this.v, t = this.t, vel = this.vel, key, s;
    this.time += dt;

    // waits
    for (var i = this.waits.length - 1; i >= 0; i--) {
      var w = this.waits[i];
      if (w.tok !== this.tok) { this.waits.splice(i, 1); w.rej(CANCEL); }
      else if (this.time >= w.at) { this.waits.splice(i, 1); w.res(); }
    }

    // springs
    for (key in SPRING) {
      s = SPRING[key];
      vel[key] += ((t[key] - v[key]) * s[0] - vel[key] * s[1]) * dt;
      v[key] += vel[key] * dt;
    }

    // where the ground is
    var sf = this.surf ? this.surf() : null;
    if (sf) {
      // The ground rising (a card growing) carries it up. The ground dropping
      // by more than a step (the card it was on went, another is below) is a
      // fall of exactly that height, not a slide.
      if (!this.air && sf.y - this.baseY > 10 && this.v.alpha > 0.5) { this.yOff = this.baseY - sf.y; this.baseY = sf.y; this.air = true; this.vy = 0; }
      this.baseY += (sf.y - this.baseY) * Math.min(1, dt * 18);
      if (this.clampX && this.tx !== null) this.tx = clamp(this.tx, sf.l, sf.r);
    }

    // walking: ease the speed in and out, stop on the mark
    var want = 0;
    if (this.tx !== null) {
      var d = this.tx - this.x;
      want = clamp(d * 5, -this.speed, this.speed);
      if (Math.abs(d) < 1.2 && Math.abs(this.vx) < 14) {
        this.x = this.tx; this.tx = null; want = 0;
        if (this.onArrive) { var fa = this.onArrive; this.onArrive = null; fa(); }
      }
    }
    this.vx += (want - this.vx) * Math.min(1, dt * 9);
    this.x += this.vx * dt;
    // holding on to something that someone else is moving
    if (this.carry) { var cx = this.carry(); this.vx = 0; this.x = cx; }
    var sp = Math.abs(this.vx);
    this.walk += (clamp(sp / 34, 0, 1) - this.walk) * Math.min(1, dt * 10);
    // How far the legs turn for each pixel travelled. Too little and the feet
    // slide over the ground; this is about one stride to the length of a step.
    this.phase += sp * dt * 0.165 / this.k;

    // falling and hopping
    if (this.air) {
      this.vy += G * dt;
      // A long way down: after a moment of plain falling, the parachute opens
      // and the rest is a slow, swinging descent.
      if (this.vy > 0 && !this.chuting && -this.yOff > 2.3 * 92 * this.k && this.vy > 190) { this.chuting = true; this.t.chute = 1; this.vel.squash += 2.5; this.vy *= 0.35; }
      if (this.chuting) {
        this.vy = Math.min(this.vy, 62 + 26 * Math.sin(this.time * 3.1));
        this.x += Math.sin(this.time * 2.3) * 13 * dt;
        this.t.lean = Math.sin(this.time * 2.3 + 1.2) * 9;
      }
      this.yOff += this.vy * dt;
      if (this.yOff >= 0 && this.vy > 0) {
        var hit = this.vy;
        this.yOff = 0; this.vy = 0; this.air = false;
        if (this.chuting) { this.chuting = false; this.t.chute = 0; this.t.lean = 0; }
        vel.squash -= clamp(hit / 95, 1.2, 7);       // the landing squashes it
        if (hit > 260) { this.fx('dust', 30, GROUND - 3, ';--dx:-9px'); this.fx('dust', 66, GROUND - 3, ';--dx:9px'); }
        if (this.onLand) { var fl = this.onLand; this.onLand = null; fl(); }
      }
    }

    // blinking by itself
    if (this.autoBlink) {
      this.nextBlink -= dt;
      if (this.nextBlink <= 0) { this.blink = 0.13; this.nextBlink = rnd(1.8, 4.6); }
    }
    if (this.blink > 0) this.blink -= dt;

    this.lookX += (this.lookTX - this.lookX) * Math.min(1, dt * 9);
    this.lookY += (this.lookTY - this.lookY) * Math.min(1, dt * 9);

    // the antenna trails behind whatever the head does
    var drive = -this.vx * 0.012 - vel.headRot * 0.006 - vel.turn * 0.5 + (this.air ? -this.vy * 0.0016 : 0);
    this.antV += ((drive - this.ant) * 210 - this.antV * 11) * dt;
    this.ant += this.antV * dt;

    this.draw();
  };

  Actor.prototype.draw = function () {
    var v = this.v, p = this.p, k = this.k;
    var gait = Math.sin(this.phase), w = this.walk;
    var run = clamp((Math.abs(this.vx) - 90) / 80, 0, 1);
    var sit = clamp(v.sit, 0, 1);
    var sw = v.swing, tm = this.time;
    var shake = v.shake * Math.sin(tm * 34) * 15;

    // A step: the leg swings from the hip, the foot going forward is picked up
    // off the ground, the body rises twice a stride and rolls a little from
    // side to side, and the head follows the body a moment late.
    var amp = (30 + run * 14) * w, cosg = Math.cos(this.phase);
    var legL = v.legL + gait * amp + Math.sin(tm * 5.2) * 24 * sw;
    var legR = v.legR - gait * amp + Math.sin(tm * 5.2 + 2.5) * 24 * sw;
    // The foot to pick up is the one travelling forwards, and which way is
    // forwards depends on which way it is going. (Lifting the same foot both
    // ways made it moonwalk to the right.)
    var way = clamp(this.vx / 18, -1, 1);
    var liftL = Math.max(0, -way * cosg) * 3.4 * w, liftR = Math.max(0, way * cosg) * 3.4 * w;
    var swingA = 24 * w * clamp(v.armSwing, 0, 1.4) * (1 - clamp(v.shake, 0, 1));
    var armL = v.armL - gait * swingA + shake;
    var armR = v.armR + gait * swingA - shake;
    // hanging from the lines: both hands up
    var ch = clamp(v.chute, 0, 1.15);
    armL = armL * (1 - Math.min(1, ch)) + 152 * Math.min(1, ch); armR = armR * (1 - Math.min(1, ch)) + 152 * Math.min(1, ch);
    p.chute.setAttribute('opacity', clamp(ch * 3, 0, 1).toFixed(2));
    p.chute.setAttribute('transform', 'translate(48 54) scale(' + Math.max(0.01, ch).toFixed(3) + ') translate(-48 -54)');
    var rise = 0.5 - 0.5 * Math.cos(this.phase * 2);
    var bob = -rise * (2.3 + run * 2) * w + (this.air ? 0 : Math.sin(tm * 2.1) * 0.5);
    var roll = gait * 1.7 * w;
    var headLag = Math.sin(this.phase * 2 - 0.9) * 0.9 * w;
    var lean = v.lean + clamp(this.vx / 22, -11, 11) + roll;
    var sq = clamp(v.squash, 0.55, 1.4), sx = 1 / Math.sqrt(sq);
    var drop = sit * SIT_DROP;

    this.el.style.transform = 'translate3d(' + (this.x - 48 * k).toFixed(2) + 'px,' + (this.baseY + this.yOff - GROUND * k).toFixed(2) + 'px,0)';
    this.el.style.opacity = clamp(v.alpha, 0, 1).toFixed(3);

    p.root.setAttribute('transform', 'translate(0 ' + drop.toFixed(2) + ') rotate(' + lean.toFixed(2) + ' 48 ' + GROUND + ') translate(48 ' + GROUND + ') scale(' + sx.toFixed(3) + ' ' + sq.toFixed(3) + ') translate(-48 -' + GROUND + ')');
    // Growing out of the logo: with grow at 0 there is only the head. The body
    // comes first, then the arms, then the legs, each with a little overshoot.
    var gr = v.grow, gB = clamp(gr * 2.2, 0, 1.12), gA = clamp(gr * 2.2 - 0.6, 0, 1.12), gL = clamp(gr * 2.2 - 1.2, 0, 1.12);
    if (gr > 0.985 && gr < 1.015) { gB = gA = gL = 1; }
    function sc(px, py, f) { return f === 1 ? '' : ' translate(' + px + ' ' + py + ') scale(' + Math.max(0, f).toFixed(3) + ') translate(' + (-px) + ' ' + (-py) + ')'; }
    p.torso.setAttribute('transform', sc(48, 51, gB).trim());
    p.legL.setAttribute('transform', 'translate(0 ' + (-liftL).toFixed(2) + ') rotate(' + legL.toFixed(2) + ' 40.5 78)' + sc(40.5, 78, gL));
    p.legR.setAttribute('transform', 'translate(0 ' + (-liftR).toFixed(2) + ') rotate(' + legR.toFixed(2) + ' 55.5 78)' + sc(55.5, 78, gL));
    p.up.setAttribute('transform', 'translate(0 ' + bob.toFixed(2) + ')');
    // Hands behind the back: the arms slip round behind the body and tuck in,
    // so only the elbows show at the sides. They change places with the body
    // in the drawing while they are still clear of it, so nothing pops.
    var ab = clamp(v.armsBack, 0, 1), behind = ab > 0.1;
    if (behind !== !!this.armsBehind) {
      this.armsBehind = behind;
      if (behind) { p.up.insertBefore(p.armR, p.up.firstChild); p.up.insertBefore(p.armL, p.up.firstChild); }
      else { p.up.appendChild(p.armL); p.up.appendChild(p.armR); }
    }
    // Both hands on something to one side: seen side-on, the two shoulders are
    // one in front of the other, so both arms start from the same place and
    // reach the same way, one hand a little under the other.
    var rcA = clamp(Math.abs(v.reach), 0, 1), rcX = 48 + (v.reach < 0 ? -9 : 9);
    var rLx = rcA * (rcX - 23.5), rRx = rcA * (rcX - 72.5), rRy = rcA * 5;
    p.armL.setAttribute('transform', 'translate(' + (ab * 7.5 + rLx).toFixed(2) + ' ' + (ab * 2.5).toFixed(2) + ') rotate(' + (armL - ab * 24).toFixed(2) + ' 23.5 58)' + sc(23.5, 58, gA));
    p.armR.setAttribute('transform', 'translate(' + (-ab * 7.5 + rRx).toFixed(2) + ' ' + (ab * 2.5 + rRy).toFixed(2) + ') rotate(' + (-(armR - ab * 24)).toFixed(2) + ' 72.5 58)' + sc(72.5, 58, gA));

    var turn = v.turn, tc = clamp(turn, -1, 1);
    p.head.setAttribute('transform', 'translate(' + (tc * 2.6).toFixed(2) + ' ' + (v.headY + headLag).toFixed(2) + ') rotate(' + v.headRot.toFixed(2) + ' 48 50)');
    p.eyes.setAttribute('transform', 'translate(' + (turn * 15 + v.eyeX + this.lookX).toFixed(2) + ' ' + (v.eyeY + this.lookY).toFixed(2) + ')');
    var lid = clamp(v.lid + (this.blink > 0 ? 1 : 0), -0.35, 0.94), open = 1 - lid;
    var nearL = 1 - clamp(-tc, 0, 1) * 0.4, nearR = 1 - clamp(tc, 0, 1) * 0.4;
    p.eyeL.setAttribute('transform', 'translate(40.25 30.5) scale(' + nearL.toFixed(3) + ' ' + open.toFixed(3) + ') translate(-40.25 -30.5)');
    p.eyeR.setAttribute('transform', 'translate(55.75 30.5) scale(' + nearR.toFixed(3) + ' ' + open.toFixed(3) + ') translate(-55.75 -30.5)');
    p.back.setAttribute('opacity', clamp((Math.abs(turn) - 1.25) / 0.5, 0, 1).toFixed(2));
    p.light.setAttribute('opacity', (0.6 * (1 - clamp((Math.abs(turn) - 1.1) / 0.4, 0, 1))).toFixed(2));

    var ax = 48 + clamp(this.ant, -1.3, 1.3) * 7, ay = 5 + Math.abs(clamp(this.ant, -1.3, 1.3)) * 1.6;
    p.ant.setAttribute('d', 'M48 13Q48 9 ' + ax.toFixed(2) + ' ' + (ay + 1).toFixed(2));
    p.tip.setAttribute('cx', ax.toFixed(2)); p.tip.setAttribute('cy', ay.toFixed(2));

    var lift = clamp(-this.yOff / 40, 0, 1);
    p.shadow.setAttribute('opacity', (0.32 * (1 - sit) * (1 - lift * 0.7) * clamp(v.alpha, 0, 1) * clamp(gL, 0, 1)).toFixed(3));
    p.shadow.setAttribute('transform', 'translate(0 ' + (-this.yOff / k).toFixed(2) + ') translate(48 90) scale(' + (1 - lift * 0.4).toFixed(3) + ') translate(-48 -90)');
  };

  // What a behaviour is given to work with. Everything waits on the actor's
  // own clock and is dropped the moment a newer behaviour takes over.
  Actor.prototype.play = function (fn) {
    var a = this, tok = ++this.tok;
    this.onArrive = null; this.onLand = null;
    function live() { if (a.dead || tok !== a.tok) throw CANCEL; }
    var api = {
      a: a,
      wait: function (ms) { live(); return new Promise(function (res, rej) { a.waits.push({ at: a.time + ms / 1000, tok: tok, res: res, rej: rej }); }); },
      pose: function (o) { live(); for (var key in o) a.t[key] = o[key]; },
      rest: function (keep) { live(); for (var key in REST) if (key !== 'alpha' && key !== 'sit' && key !== 'grow' && key !== 'chute' && !(keep && keep[key])) a.t[key] = REST[key]; },
      walkTo: function (x, speed) {
        live();
        a.t.sit = 0; a.t.swing = 0;                // nothing walks sitting down
        a.speed = speed || 55; a.tx = x; a.lastSpeed = a.speed;
        a.t.turn = x > a.x ? 0.8 : -0.8;
        return new Promise(function (res, rej) {
          a.onArrive = function () { if (tok !== a.tok) return rej(CANCEL); a.t.turn = 0; res(); };
        });
      },
      hop: function (h) {
        live();
        a.air = true; a.vy = -Math.sqrt(2 * G * (h || 12));
        a.vel.squash += 2.2;
        return new Promise(function (res, rej) { a.onLand = function () { if (tok !== a.tok) return rej(CANCEL); res(); }; });
      },
      // The ground has moved to somewhere lower: keep the body where it is and
      // let it drop.
      fallTo: function (surf) {
        live();
        var was = a.baseY + a.yOff, sf = surf();
        a.surf = surf; a.baseY = sf ? sf.y : was; a.yOff = Math.min(0, was - a.baseY); a.air = true; a.vy = Math.max(a.vy, 0);
        return new Promise(function (res, rej) { a.onLand = function () { if (tok !== a.tok) return rej(CANCEL); res(); }; });
      },
      // The ground has moved to somewhere higher: jump up onto it.
      jumpTo: function (surf) {
        live();
        var was = a.baseY + a.yOff, sf = surf();
        a.surf = surf; a.baseY = sf ? sf.y : was; a.yOff = Math.max(0, was - a.baseY); a.air = true;
        a.vy = -Math.sqrt(2 * G * (a.yOff + 9)); a.vel.squash += 2.4;
        return new Promise(function (res, rej) { a.onLand = function () { if (tok !== a.tok) return rej(CANCEL); res(); }; });
      },
      fx: function (n, dx, dy, o) { live(); a.fx(n, dx, dy, o); },
      fxAt: function (n, x, y) { live(); a.fxAt(n, x, y); },
      surf: function () { return a.surf ? a.surf() : null; },
      live: live
    };
    return Promise.resolve().then(function () { return fn(api); }).catch(function (e) { if (e !== CANCEL) { try { console.error('[mini]', e); } catch (x) { /* nothing */ } } });
  };

  // ─── One clock for every actor ───
  var ACTORS = [], raf = 0, last = 0;
  function frame(now) {
    raf = 0;
    var dt = Math.min(0.05, (now - last) / 1000 || 0.016);
    last = now;
    for (var i = ACTORS.length - 1; i >= 0; i--) ACTORS[i].step(dt);
    if (ACTORS.length) raf = requestAnimationFrame(frame);
  }
  function startLoop() { if (!raf) { last = performance.now(); raf = requestAnimationFrame(frame); } }

  // ─── Small moves, shared by the scenes ───
  async function knock(s) {
    s.pose({ turn: 2.1, armR: 0, armL: 0, lean: 0 });           // turn its back on us
    await s.wait(620);
    s.pose({ armR: 152 });
    await s.wait(300);
    for (var i = 0; i < 3; i++) {
      s.pose({ armR: 118 }); await s.wait(95);
      s.fx('knock', 79, 20);
      s.pose({ armR: 156 }); await s.wait(150);
    }
    s.pose({ armR: 30, headRot: 13 });                           // listen
    await s.wait(1000);
    s.pose({ headRot: 0, armR: 0, turn: 0 });
    await s.wait(560);
  }
  async function shrug(s) {
    s.pose({ armL: 48, armR: 48, headY: 2.5, headRot: -7, eyeY: -2 });
    await s.wait(620);
    s.pose({ armL: 0, armR: 0, headY: 0, headRot: 0, eyeY: 0 });
    await s.wait(380);
  }
  async function watch(s) {
    s.pose({ armL: -112, headRot: -11, headY: 2, eyeX: -3.5, eyeY: 3.5, turn: -0.25 });   // wrist up, look at it
    await s.wait(700);
    for (var i = 0; i < 4; i++) { s.pose({ legR: -22 }); await s.wait(150); s.pose({ legR: 0 }); await s.wait(170); }
    s.pose({ lid: 0.45 }); await s.wait(260);
    s.pose({ armL: 0, headRot: 0, headY: 0, eyeX: 0, eyeY: 0, turn: 0, lid: 0 });
    await s.wait(420);
  }
  async function shakeHead(s) {
    s.pose({ lid: 0.55, headY: 1.5 });
    for (var i = 0; i < 5; i++) { s.pose({ turn: i % 2 ? 0.55 : -0.55 }); await s.wait(190); }
    s.pose({ turn: 0, lid: 0, headY: 0 });
    await s.wait(360);
  }
  async function sigh(s) {
    s.pose({ squash: 1.07, headY: -1.5, armL: 14, armR: 14 }); await s.wait(430);
    s.pose({ squash: 0.9, headY: 4, headRot: 6, lid: 0.6, armL: -4, armR: -4 });
    s.fx('puff', 64, 38);
    await s.wait(820);
    s.pose({ squash: 1, headY: 0, headRot: 0, lid: 0, armL: 0, armR: 0 }); await s.wait(380);
  }
  async function tapFoot(s) {
    if (Math.random() < 0.45) s.pose({ armsBack: 1, headRot: 8, eyeY: -2.5, eyeX: 2 });   // hands behind its back, rocking
    else s.pose({ armL: -38, armR: -38, headRot: 8, eyeY: -2.5, eyeX: 2 });  // or arms folded; eyes to the ceiling either way
    for (var i = 0; i < 7; i++) { s.pose({ legL: -24 }); await s.wait(140); s.pose({ legL: 0 }); await s.wait(160); }
    s.pose({ armsBack: 0, armL: 0, armR: 0, headRot: 0, eyeY: 0, eyeX: 0 }); await s.wait(360);
  }
  // Pacing, the way people do outside a hospital room: not hurrying anywhere,
  // because there is nowhere to go. Slow, heavy lengths of the same stretch,
  // head down, hands clasped behind the back or wrung in front. At each end a
  // stop, a look up at the door (the conversation) in case, nothing, and
  // round again. Now and then the watch, a hand to the forehead, a long
  // breath, or sitting down for a moment and not being able to stay sat.
  async function pace(s, turns, speed) {
    var a = s.a, sf = s.surf(); if (!sf) return;
    if (sf.r - sf.l < 58) return;
    var dir = a.x - sf.l > sf.r - a.x ? -1 : 1;                              // start towards the longer side
    var clasp = Math.random() < 0.65;
    function brood(d) {
      if (clasp) s.pose({ armsBack: 1, armL: 0, armR: 0, shake: 0, armSwing: 0.08, headY: 2.6, headRot: d * 8, eyeY: 3.6, lid: 0.26, lean: d * 1.5, squash: 0.985 });
      else s.pose({ armsBack: 0, armL: -30, armR: -30, shake: 0.1, armSwing: 0, headY: 2.6, headRot: d * 8, eyeY: 3.6, lid: 0.26, lean: d * 1.5, squash: 0.985 });   // wringing its hands
    }
    async function lookUp() {                                                  // anything? no.
      s.pose({ turn: 0, headRot: -6, headY: -0.5, eyeY: -4.2, lid: -0.12, lean: 0, shake: 0, squash: 1 });
      await s.wait(rnd(900, 1500));
      s.pose({ lid: 0.55, headY: 2.5, eyeY: 2 });
      await s.wait(rnd(380, 620));
    }
    for (var i = 0; i < turns; i++) {
      sf = s.surf(); if (!sf) return;
      var to = dir > 0 ? sf.r - 8 - rnd(0, 14) : sf.l + 8 + rnd(0, 14);
      var kind = i === 0 ? 0 : Math.floor(Math.random() * 6);
      if (i && Math.random() < 0.3) clasp = !clasp;
      brood(dir);
      var going = s.walkTo(to, kind === 3 ? speed * 1.4 : speed);
      if (kind === 1) {                                                        // the watch, without stopping
        await s.wait(520);
        s.pose({ armsBack: 0, shake: 0, armSwing: 0.08, armL: -112, armR: 0, headRot: -9, headY: 3, eyeX: -3.2, eyeY: 3.6, lid: 0.1 });
        await s.wait(1000);
        if (a.tx !== null) brood(dir);
      } else if (kind === 2) {                                                 // stop dead, a hand to the forehead
        await s.wait(rnd(500, 900));
        if (a.tx !== null) {
          a.tx = null;
          s.pose({ turn: 0, armsBack: 0, shake: 0, armR: 0, armL: -122, headY: 4, headRot: -5, lid: 0.9, lean: 0 });
          await s.wait(1300);
          s.pose({ armL: 0, lid: 0.3, headY: 2 }); await s.wait(350);
          brood(dir);
          going = s.walkTo(to, speed);
        }
      } else if (kind === 4) {                                                 // a long breath on the way
        await s.wait(rnd(400, 800));
        if (a.tx !== null) { a.tx = null; s.pose({ turn: 0, lean: 0 }); await s.wait(200); await sigh(s); brood(dir); going = s.walkTo(to, speed); }
      }
      await going;
      // the end of the length: stop, look up at the door, turn
      s.pose({ squash: 0.97, lean: 0, headRot: 0 });
      await s.wait(rnd(260, 520));
      if (i === turns - 1 || Math.random() < 0.6) await lookUp();
      if (kind === 5) {                                                        // sit; cannot stay sat
        s.pose({ armsBack: 0, shake: 0, sit: 1, swing: 0.25, armL: -118, armR: -118, headY: 5, lid: 0.9, turn: 0, headRot: 0 });   // head in its hands
        await s.wait(rnd(2200, 3200));
        s.pose({ sit: 0, swing: 0, armL: 0, armR: 0, headY: 0, lid: 0.2 });
        await s.wait(650);
      }
      s.pose({ squash: 1 });
      dir = -dir;
    }
    s.pose({ armsBack: 0, shake: 0, armL: 0, armR: 0, armSwing: 1, headY: 0, headRot: 0, eyeY: 0, eyeX: 0, lid: 0, lean: 0, turn: 0, squash: 1 });
    await s.wait(420);
  }
  async function peekDown(s) {
    s.pose({ lean: 0, headY: 6, headRot: 0, eyeY: 4.5, squash: 0.93, armL: 22, armR: 22 });     // read what is underfoot
    await s.wait(900);
    s.pose({ squash: 1, headY: 0, eyeY: 0, armL: 0, armR: 0 });
    await s.wait(250);
    await s.hop(7); await s.hop(7);                                           // stamp on it
    await s.wait(300);
  }
  async function scan(s) {
    s.pose({ armR: 148, turn: 0.85, eyeX: 2 }); await s.wait(820);            // hand over the eyes, look far off
    s.pose({ armR: 0, armL: 148, turn: -0.85, eyeX: -2 }); await s.wait(900);
    s.pose({ armL: 0, turn: 0, eyeX: 0 }); await s.wait(360);
  }
  async function paperPlane(s) {
    s.pose({ armR: 70, turn: 0.4, lean: -5 }); await s.wait(330);             // wind up
    s.pose({ armR: 160, lean: 6 });
    s.fx('plane', 76, 30, ';--dx:' + Math.round(rnd(-70, -20)) + 'px;--dy:-120px');
    await s.wait(520);
    s.pose({ armR: 0, lean: 0, turn: 0, headRot: -9, eyeY: -4 });             // watch it go
    await s.wait(900);
    s.pose({ headRot: 0, eyeY: 0 }); await s.wait(260);
  }
  async function wave(s, n) {
    s.pose({ armR: 150, headRot: 6 }); await s.wait(260);
    for (var i = 0; i < (n || 4); i++) { s.pose({ armR: 126 }); await s.wait(130); s.pose({ armR: 164 }); await s.wait(130); }
    s.pose({ armR: 0, headRot: 0 }); await s.wait(340);
  }
  async function enter(s, from, to, speed) {
    var a = s.a, sf = s.surf();
    a.clampX = false;
    a.x = from; a.vx = 0; a.baseY = sf ? sf.y : a.baseY; a.yOff = 0; a.air = false;
    for (var key in REST) { a.v[key] = REST[key]; a.t[key] = REST[key]; a.vel[key] = 0; }
    a.v.alpha = 0; s.pose({ alpha: 1 });
    await s.walkTo(to, speed);
    a.clampX = true;
  }
  // Leave in a hurry: arms in the air, flapping, dust at the heels.
  async function runOff(s, layerW) {
    var a = s.a, to = a.x < layerW / 2 ? -50 : layerW + 50;
    a.clampX = false;
    s.pose({ armL: 162, armR: 162, shake: 1, lid: -0.3, sit: 0, swing: 0, headY: -1, lean: 0, headRot: 0, eyeX: 0, eyeY: 0, squash: 1, legL: 0, legR: 0 });
    var done = s.walkTo(to, 215);
    for (var i = 0; i < 12 && a.tx !== null; i++) {
      s.fx('dust', a.vx > 0 ? 30 : 66, GROUND - 3, ';--dx:' + (a.vx > 0 ? -12 : 12) + 'px');
      await s.wait(105);
    }
    await done;
    s.pose({ alpha: 0 });
    await s.wait(200);
  }

  // ─── Scenes ───
  // A message is waiting to be read.
  async function sceneQueue(s, layerW, env) {
    var sf = s.surf(); if (!sf) return;
    // come in on the side that does not mean walking across the badges
    var side = env && env.badge() ? -1 : (Math.random() < 0.5 ? -1 : 1);
    var spot = sf.l + (sf.r - sf.l) * rnd(0.55, 0.72);
    await enter(s, side < 0 ? -40 : layerW + 40, spot, 62);
    await s.wait(380);
    // It does not always open the same way.
    var open = deal('queue-open', 4);
    if (open === 0) { s.pose({ eyeY: -3, headRot: -6 }); await s.wait(520); s.pose({ eyeY: 0, headRot: 0 }); await knock(s); await shrug(s); }   // look up, knock, nothing
    else if (open === 1) { await watch(s); await shakeHead(s); }               // arrives already looking at the time
    else if (open === 2) { await peekDown(s); await knock(s); }                // what is this it is standing on
    else { await scan(s); await sigh(s); }                                     // anyone? no
    await queueLoop(s, 0, env);
  }
  // "Working…" is right there. Go over, look up at it, point, tap the watch.
  async function nagWorking(s, env) {
    var a = s.a, sf = s.surf(), w = env && env.activity(); if (!sf || !w) return;
    await s.walkTo(clamp(w.r + 30, sf.l, sf.r - 4), 70);
    var left = w.x < a.x, arm = left ? 'armL' : 'armR', o = { turn: left ? -0.6 : 0.6, eyeY: -4, headRot: left ? -9 : 9 };
    s.pose(o); await s.wait(700);                                             // look up at it
    var up = w.y < a.baseY - 46 * a.k ? 128 : 96;                             // above its shoulder, or level with it
    o[arm] = up; s.pose(o); await s.wait(260);
    for (var i = 0; i < 3; i++) { o[arm] = up - 14; s.pose(o); await s.wait(140); var t = env.activity(); if (t) s.fxAt('ring', t.x, t.y); o[arm] = up + 6; s.pose(o); await s.wait(170); }
    o = { turn: 0, eyeY: 0, headRot: 0 }; o[arm] = 0; s.pose(o); await s.wait(300);
    await watch(s);                                                           // and how long has it been
  }
  // A background command is running in the corner. Go and see what it is.
  async function checkBadge(s, env) {
    var sf = s.surf(), b = env && env.badge(); if (!sf || !b) return;
    await s.walkTo(sf.r - 2, 60);
    s.pose({ turn: 0.75, lean: 7, headRot: 8, eyeY: 2 }); await s.wait(900);  // lean in and read it
    s.pose({ armR: 96 });
    for (var i = 0; i < 2; i++) { s.pose({ armR: 84 }); await s.wait(130); var t = env.badge(); if (t) s.fxAt('ring', t.l + 6, t.y); s.pose({ armR: 100 }); await s.wait(170); }
    s.pose({ armR: 0, lean: 0, headRot: 0, eyeY: 0, turn: 0 }); await s.wait(300);
    await shrug(s);                                                           // no idea either
  }
  async function queueLoop(s, from, env) {
    var acts = [
      function () { return pace(s, 6, 33); },
      function () { return watch(s); },
      function () { return shakeHead(s); },
      function () { return tapFoot(s); },
      function () { return knock(s); },
      function () { return peekDown(s); },
      function () { return sigh(s); },
      function () { return scan(s); },
      function () { return paperPlane(s); },
      function () { return nagWorking(s, env); },
      function () { return checkBadge(s, env); }
    ];
    for (;;) {
      var i = deal('queue', acts.length);
      // only what is actually on screen: otherwise deal again
      if (i === 9 && !(env && env.activity())) i = deal('queue', acts.length);
      if (i === 10 && !(env && env.badge())) i = deal('queue', acts.length);
      if ((i === 9 && !(env && env.activity())) || (i === 10 && !(env && env.badge()))) i = deal('queue-plain', 9);
      await acts[i]();
      s.rest();
      await s.wait(rnd(500, 1300));
    }
  }

  // Another message has landed on the pile under its feet.
  async function another(s, count) {
    var a = s.a;
    a.tx = null;
    s.pose({ lid: -0.35, headY: -2, armL: 30, armR: 30 }); s.fx('bang', 48, -12);       // what was that
    await s.wait(360);
    s.pose({ lid: 0, headY: 6, eyeY: 4.5, squash: 0.93, armL: 22, armR: 22 });          // look down at it
    await s.wait(820);
    s.pose({ headY: 0, eyeY: 0, squash: 1, armL: 0, armR: 0 });
    await s.wait(200);
    if (count >= 3) {                                                                   // this is getting silly
      s.pose({ armL: 150, armR: 150, headRot: -8, eyeY: -4 }); await s.wait(760);
      s.pose({ armL: 0, armR: 0, headRot: 0, eyeY: 0 });
      await sigh(s);
    } else {
      s.pose({ armL: -118, headY: 3, lid: 0.9, headRot: -6 }); await s.wait(900);       // hand over its face
      s.pose({ armL: 0, headY: 0, lid: 0, headRot: 0 }); await s.wait(300);
      await shakeHead(s);
    }
    await knock(s);                                                                     // and knock again, harder
  }

  // A suggested reply is waiting. Sit on its corner.
  async function sceneSit(s, layerW) {
    var sf = s.surf(); if (!sf) return;
    await enter(s, layerW + 40, Math.min(sf.r - 8, sf.l + 60), 70);
    await takeSeat(s);
    await sitLoop(s, 0);
  }
  async function takeSeat(s) {
    var sf = s.surf(); if (!sf) return;
    await s.walkTo(sf.l + 15, 46);
    await s.wait(200);
    s.pose({ eyeY: 4, headY: 3 }); await s.wait(520);                         // is it safe to sit on?
    s.pose({ eyeY: 0, headY: 0 });
    await s.hop(8);
    s.pose({ sit: 1, swing: 1, armL: 10, armR: 10 });
    await s.wait(1300);
  }
  async function sitLoop(s, from) {
    for (;;) {
      var r = deal('sit', 6);
      if (r === 0) { s.pose({ turn: 0.7, eyeY: 3, headRot: 7 }); await s.wait(1200); s.pose({ turn: 0, eyeY: 0, headRot: 0 }); }        // look at the suggestion beside it
      else if (r === 1) { s.pose({ armR: 62 }); for (var i = 0; i < 3; i++) { s.pose({ armR: 40, turn: 0.5, eyeY: 3 }); await s.wait(170); s.fx('ring', 84, 78); s.pose({ armR: 62 }); await s.wait(210); } s.pose({ armR: 10, turn: 0, eyeY: 0 }); }   // pat it: this one
      else if (r === 2) { s.pose({ armL: 34, armR: 34, headRot: -9, lean: -5, lid: 0.5 }); for (var j = 0; j < 3; j++) { s.fx('note', 70 + j * 4, 6, ';--dx:' + (8 + j * 5) + 'px'); await s.wait(760); } s.pose({ armL: 10, armR: 10, headRot: 0, lean: 0, lid: 0 }); }   // lean back and hum
      else if (r === 3) { await wave(s, 3); s.pose({ armR: 10 }); }
      else if (r === 4) { s.pose({ swing: 2.1 }); await s.wait(1300); s.pose({ swing: 1 }); }                                           // kick
      else { s.pose({ headRot: -10, eyeY: -3.5, eyeX: -2 }); await s.wait(1500); s.pose({ headRot: 0, eyeY: 0, eyeX: 0 }); }            // stare at the ceiling
      await s.wait(rnd(1400, 3000));
    }
  }

  // The seat has gone. Hang for a moment, fall, get up, leave.
  async function sceneFall(s, layerW, ground) {
    var a = s.a;
    a.tx = null;
    s.pose({ lid: -0.35, armL: 150, armR: 150, swing: 0, headY: -2, turn: 0, headRot: 0, lean: 0 });
    s.fx('bang', 48, -12);
    await s.wait(330);                                                        // the cartoon pause before gravity notices
    s.pose({ sit: 0, shake: 0.8, legL: 24, legR: -24 });
    await s.fallTo(ground);
    s.pose({ shake: 0, armL: 62, armR: 62, lid: 0.9, headY: 5, legL: 0, legR: 0 });
    await s.wait(520);
    for (var i = 0; i < 4; i++) { s.pose({ headRot: i % 2 ? 9 : -9 }); await s.wait(150); }       // shake it off
    s.pose({ headRot: 0, lid: 0, headY: 0, armL: 0, armR: 0 });
    await s.wait(260);
    s.pose({ eyeY: -4.5, headRot: -8 }); await s.wait(620);                   // look up at where the seat was
    s.pose({ eyeY: 0, headRot: 0, lid: -0.3 });
    s.fx('sweat', 70, 14);
    await s.wait(260);
    await runOff(s, layerW);
  }

  // Nothing typed for a long time. Stand by Send and make a case for the box.
  async function sceneNudge(s, layerW, aim, mobile, sendAt, sugAt) {
    var sf = s.surf(); if (!sf) return;
    var spot = mobile ? sf.l + (sf.r - sf.l) * 0.5 : sf.r - 70;
    await enter(s, layerW + 40, spot, 66);
    await s.wait(300);
    await wave(s, 3);
    await nudgeLoop(s, aim, 0, mobile ? null : sendAt, sugAt);
  }
  async function nudgeLoop(s, aim, from, sendAt, sugAt) {
    var a = s.a;
    function pointAt(where) {
      var t = (where || aim)(); if (!t) return null;
      var dx = t.x - a.x, dy = t.y - (a.baseY - 34 * a.k);
      var left = dx < 0, ang = Math.atan2(Math.abs(dx), dy) * 180 / Math.PI;
      return { left: left, ang: clamp(ang, 18, 172), dx: dx, dy: dy };
    }
    async function point() {
      var p = pointAt(); if (!p) return;
      var arm = p.left ? 'armL' : 'armR', o = {};
      o[arm] = p.ang; o.turn = p.left ? -0.55 : 0.55; o.eyeY = p.dy < 0 ? -3.5 : 3.5; o.headRot = p.left ? -6 : 6;
      s.pose(o); await s.wait(520);
      for (var i = 0; i < 3; i++) { o[arm] = p.ang - 16; s.pose(o); await s.wait(150); o[arm] = p.ang + 4; s.pose(o); await s.wait(170); }
      s.pose({ turn: 0, eyeY: 0, headRot: 0 }); await s.wait(520);            // look back at you, still pointing
      o = {}; o[arm] = 0; s.pose(o); await s.wait(320);
    }
    async function typing() {
      s.pose({ armL: -52, armR: -52, headY: 2.5, eyeY: 3.5 });
      for (var i = 0; i < 9; i++) { s.pose(i % 2 ? { armL: -66, armR: -40 } : { armL: -40, armR: -66 }); await s.wait(110); }
      s.pose({ armL: 0, armR: 0, headY: 0, eyeY: 0 }); await s.wait(260);
      s.fx('dots', 48, -18);
      await s.wait(1500);
    }
    async function jacks() {
      for (var i = 0; i < 3; i++) { s.pose({ armL: 150, armR: 150, legL: 16, legR: -16 }); await s.hop(11); s.pose({ armL: 10, armR: 10, legL: 0, legR: 0 }); await s.wait(170); }
      s.pose({ armL: 0, armR: 0 });
    }
    async function ask() { s.pose({ headRot: 11, armL: 44, armR: 44, eyeY: -1 }); s.fx('ask', 48, -16); await s.wait(1500); s.pose({ headRot: 0, armL: 0, armR: 0, eyeY: 0 }); await s.wait(300); }
    // Wide panes only: walk to the start of the message box and point at it,
    // walk to Send and point at that, and back. Here, then there.
    async function showAndTell() {
      var sf = s.surf(), i0 = aim(), s0 = sendAt && sendAt(); if (!sf || !i0 || !s0) return;
      async function at(where, x) {
        await s.walkTo(clamp(x, sf.l + 4, sf.r - 4), 96);
        var p = pointAt(where); if (!p) return;
        var arm = p.left ? 'armL' : 'armR', o = {};
        o[arm] = p.ang; o.eyeY = 4; o.headY = 2.5; s.pose(o); await s.wait(300);
        for (var i = 0; i < 2; i++) { o[arm] = p.ang + 14; s.pose(o); await s.wait(140); var t = where(); if (t) s.fxAt('ring', t.x, t.y); o[arm] = p.ang - 4; s.pose(o); await s.wait(190); }
        s.pose({ eyeY: 0, headY: 0 }); await s.wait(420);                    // and look at you
        o = {}; o[arm] = 0; s.pose(o); await s.wait(160);
      }
      for (var r = 0; r < 2; r++) { await at(aim, i0.x + 26); await at(sendAt, s0.x - 26); }
      s.pose({ armL: 48, armR: 48, headRot: 8 }); await s.wait(700);         // well?
      s.pose({ armL: 0, armR: 0, headRot: 0 }); await s.wait(260);
    }
    async function sitAwhile() {
      await s.hop(6);
      s.pose({ sit: 1, swing: 1, armL: 10, armR: 10 });
      await s.wait(4200);
      s.pose({ sit: 0, swing: 0, armL: 0, armR: 0 });
      await s.wait(620);
    }
    // There is a suggested reply right there: or just tap that.
    async function orThat() {
      var t = sugAt && sugAt(); if (!t) return point();
      var p = pointAt(sugAt); if (!p) return;
      var arm = p.left ? 'armL' : 'armR', o = {};
      o[arm] = p.ang; o.turn = p.left ? -0.55 : 0.55; o.eyeY = -3.5; o.headRot = p.left ? -7 : 7;
      s.pose(o); await s.wait(520);
      for (var i = 0; i < 3; i++) { o[arm] = p.ang - 14; s.pose(o); await s.wait(150); var u = sugAt(); if (u) s.fxAt('ring', u.x, u.y); o[arm] = p.ang + 4; s.pose(o); await s.wait(180); }
      s.pose({ turn: 0, eyeY: 0, headRot: 0 }); await s.wait(600);            // look at you: well, that one then
      o = {}; o[arm] = 0; s.pose(o); await s.wait(320);
    }
    var acts = sendAt
      ? [showAndTell, typing, orThat, ask, jacks, showAndTell, sitAwhile, function () { return watch(s); }, orThat, showAndTell, function () { return sigh(s); }]
      : [point, typing, orThat, ask, jacks, point, function () { return pace(s, 2, 36); }, sitAwhile, function () { return watch(s); }, orThat, function () { return sigh(s); }];
    // The first thing is always the point of it; after that, a different
    // run of things each time, and not all of them.
    var turns = from >= 99 ? 0 : 6 + Math.floor(Math.random() * 3), key = sendAt ? 'nudge-wide' : 'nudge';
    for (var n = 0; n < turns; n++) {
      if (n === 0 && from === 0) await acts[0](); else await acts[deal(key, acts.length)]();
      s.rest();
      await s.wait(rnd(900, 2000) + n * 220);
    }
    // It has tried everything. Sit down and nod off until someone types.
    if (a.t.sit < 0.5) await s.hop(5);
    s.pose({ sit: 1, swing: 0.5, armL: 8, armR: 8 });
    await s.wait(1700);
    s.pose({ lid: 0.5, headY: 2 }); await s.wait(900);
    s.pose({ lid: 0.1, headY: 0 }); await s.wait(500);
    a.autoBlink = false;
    s.pose({ lid: 0.94, headRot: 13, headY: 4, swing: 0 });
    for (;;) { s.fx('z', 66, 2, ';--dx:10px'); await s.wait(1500); }
  }

  // ─── Being tapped ───
  // Each returns when Mini has made its feelings known. `seated` ones keep it
  // on its seat; the others are for when it is on its feet.
  function steam(s) { s.fx('puff', 22, 16, ';--dx:-10px'); s.fx('puff', 74, 16, ';--dx:10px'); }
  var ANNOYED = [
    async function stamp(s) {
      s.pose({ lid: 0.5, armL: -34, armR: -34, headY: 1 }); s.fx('anger', 76, 6);
      await s.wait(260);
      await s.hop(6); await s.hop(6); await s.hop(6);
      steam(s); await s.wait(700);
    },
    async function shoo(s) {
      s.pose({ lid: 0.45, turn: 0.35, headRot: 5 });
      for (var i = 0; i < 4; i++) { s.pose({ armR: 112 }); await s.wait(120); s.pose({ armR: 66 }); await s.wait(130); }
      s.pose({ armR: 0 }); await s.wait(360);
    },
    async function turnAway(s) {
      s.fx('anger', 76, 6);
      s.pose({ turn: 2.1, armL: -36, armR: -36 }); await s.wait(700);
      for (var i = 0; i < 4; i++) { s.pose({ legL: -22 }); await s.wait(140); s.pose({ legL: 0 }); await s.wait(160); }
      s.pose({ turn: 1.05, headRot: 6 }); await s.wait(820);                 // one eye over the shoulder: still there?
      s.pose({ turn: 2.1, headRot: 0 }); await s.wait(620);
      s.pose({ turn: 0, armL: 0, armR: 0 }); await s.wait(460);
    },
    async function fist(s) {
      s.pose({ lid: 0.5, lean: 5, armR: 158, shake: 0.55, headRot: 4 }); s.fx('anger', 20, 8);
      await s.wait(1100);
      s.pose({ shake: 0, armR: 0, lean: 0, headRot: 0 }); await s.wait(360);
    },
    async function stepAside(s) {
      var a = s.a, sf = s.surf(); if (!sf) return;
      s.pose({ lid: -0.35 }); s.fx('bang', 48, -12); await s.wait(200);
      var room = a.x - sf.l > sf.r - a.x ? -1 : 1, to = clamp(a.x + room * rnd(46, 80), sf.l + 6, sf.r - 6);
      s.pose({ lid: 0 });
      await s.walkTo(to, 150);                                                // out of reach
      s.pose({ turn: -room * 0.7, lid: 0.5, armL: -34, armR: -34 }); await s.wait(1100);     // and glare back
      await shakeHead(s);
    },
    async function leaveAndPeek(s, layerW) {
      var a = s.a, sf = s.surf(); if (!sf) return;
      var home = a.x, right = a.x > layerW / 2, edge = right ? layerW : 0;
      s.pose({ lid: 0.5 }); s.fx('anger', 76, 6); await s.wait(260);
      a.clampX = false;
      await s.walkTo(right ? layerW + 46 : -46, 170);                         // right out of the pane
      await s.wait(rnd(1300, 2400));
      s.pose({ lid: 0 });
      await s.walkTo(edge + (right ? 9 : -9), 26);                            // one eye back round the corner
      s.pose({ turn: right ? -0.6 : 0.6, headRot: right ? -8 : 8 });
      await s.wait(1500);
      s.pose({ headRot: 0 });
      await s.walkTo(clamp(home, sf.l + 6, sf.r - 6), 50);
      a.clampX = true;
      await s.wait(260);
    }
  ];
  var ANNOYED_SEATED = [
    async function glareKick(s) { s.pose({ lid: 0.5, swing: 2.4, armL: -30, armR: -30 }); s.fx('anger', 76, 6); await s.wait(1500); s.pose({ lid: 0, swing: 1, armL: 10, armR: 10 }); },
    async function shooSeated(s) { s.pose({ lid: 0.45, turn: 0.35 }); for (var i = 0; i < 4; i++) { s.pose({ armR: 112 }); await s.wait(120); s.pose({ armR: 66 }); await s.wait(130); } s.pose({ armR: 10, lid: 0, turn: 0 }); await s.wait(300); },
    async function humph(s) { s.pose({ turn: 1.7, armL: -34, armR: -34, swing: 0.2 }); await s.wait(1500); s.pose({ turn: 1.0 }); await s.wait(700); s.pose({ turn: 0, armL: 10, armR: 10, swing: 1 }); await s.wait(400); },
    async function hopDown(s) {
      var a = s.a, sf = s.surf(); if (!sf) return;
      s.pose({ sit: 0, swing: 0, lid: 0.5 }); s.fx('anger', 76, 6); await s.wait(380);
      await s.walkTo(clamp(a.x + 56, sf.l + 6, sf.r - 6), 110);
      s.pose({ turn: -0.7, armL: -34, armR: -34 }); await s.wait(1000);
      s.pose({ turn: 0, armL: 0, armR: 0, lid: 0 });
      await takeSeat(s);
    }
  ];
  // Prodded awake.
  async function grumpyWake(s) {
    s.pose({ lid: -0.35, headRot: 0, headY: -2 }); s.fx('bang', 48, -12); await s.wait(420);
    s.pose({ lid: 0.5, headY: 0 }); s.fx('anger', 76, 6); await s.wait(500);
    await shakeHead(s);
  }
  // The cursor is coming and it remembers what the cursor did. Back away from
  // it; with nowhere to go, duck and cover.
  async function flee(s, fromX, panic) {
    var a = s.a, sf = s.surf(); if (!sf) return;
    var dir = a.x >= fromX ? 1 : -1;
    var room = dir > 0 ? sf.r - 6 - a.x : a.x - (sf.l + 6);
    a.tx = null;
    if (a.t.sit > 0.5 || room < 26) {                                          // cornered, or sitting: duck and cover
      s.pose({ squash: 0.8, armL: 156, armR: 156, lid: 0.9, shake: 0.45, headY: 5, lean: dir * 9, swing: 0, turn: 0 });
      s.fx('sweat', dir > 0 ? 24 : 72, 14);
      await s.wait(1300);
      s.pose({ squash: 1, shake: 0, lid: 0.35, headY: 0, lean: 0, armL: 40, armR: 40 });
      await s.wait(520);                                                       // peep out
      return;
    }
    if (panic) {
      s.pose({ lid: -0.35, armL: 150, armR: 150, shake: 0.7, headY: -2 });
      s.fx('sweat', dir > 0 ? 24 : 72, 14);
      await s.wait(150);
      await s.walkTo(a.x + dir * Math.min(room, rnd(80, 130)), 190);
      s.pose({ shake: 0.25, armL: 60, armR: 60, turn: -dir * 0.75, lid: -0.2 });  // look back, still trembling
      await s.wait(800);
      s.pose({ shake: 0, armL: 0, armR: 0, lid: 0 });
      return;
    }
    // Not running: backing off, without taking its eyes off the thing.
    s.pose({ lid: 0.35, armL: 34, armR: 34, lean: dir * 4 });
    var going = s.walkTo(a.x + dir * Math.min(room, rnd(46, 74)), 78);
    a.t.turn = -dir * 0.7;                                                     // walking backwards
    await going;
    s.pose({ turn: -dir * 0.7, lean: 0, armL: 0, armR: 0 });
    await s.wait(260);
  }
  // Something has it on edge and now the cursor is hanging about. Watch it:
  // look, look away, look again. If it will not leave, put some distance in.
  async function eyeIt(s, mouse) {
    var a = s.a, t0 = a.time, moves = 0;
    for (;;) {
      var m = mouse(); if (!m || m.d > 125) break;
      var dir = m.x > a.x ? 1 : -1;
      s.pose({ lid: 0.42, turn: dir * 0.6, headRot: dir * 5, armL: -22, armR: -22, headY: 0 });     // stare at it
      await s.wait(rnd(480, 760));
      m = mouse(); if (!m || m.d > 125) break;
      s.pose({ turn: -dir * 0.35, headRot: 0 });                                                    // look away; the eyes stay on it
      await s.wait(rnd(300, 520));
      m = mouse(); if (!m || m.d > 125) break;
      if (a.time - t0 > 2.0 || m.d < 42) { moves++; await flee(s, m.x, moves >= 3 || m.d < 30); t0 = a.time; }
    }
    s.pose({ lid: 0, turn: 0, headRot: 0, armL: 0, armR: 0 });
    await s.wait(360);
  }
  // Enough. Shake a fist, stamp off.
  async function stormOff(s, layerW) {
    var a = s.a;
    a.tx = null; a.autoBlink = true;
    s.pose({ sit: 0, swing: 0, lid: 0.5, turn: 0, headRot: 0, headY: 0, armL: 0, armR: 158, shake: 0.6, lean: 5 });
    s.fx('anger', 76, 6); s.fx('anger', 22, 10);
    await s.wait(900);
    s.pose({ shake: 0, armR: -34, armL: -34, lean: 0 });
    steam(s);
    a.clampX = false;
    await s.walkTo(a.x < layerW / 2 ? -50 : layerW + 50, 105);
    s.pose({ alpha: 0 }); await s.wait(200);
  }

  // Someone started typing: wake if asleep, salute, go.
  async function sceneBye(s, layerW) {
    var a = s.a;
    a.tx = null; a.autoBlink = true;
    if (a.t.lid > 0.8) { s.pose({ lid: -0.35, headRot: 0, headY: -2 }); s.fx('bang', 48, -12); await s.wait(360); }
    s.pose({ sit: 0, swing: 0, lid: 0, headRot: 0, headY: 0, eyeX: 0, eyeY: 0, turn: 0, armL: 0, armR: 0, legL: 0, legR: 0 });
    await s.wait(260);
    s.pose({ armR: 148, headRot: 5 }); await s.wait(300);                     // salute
    s.pose({ armR: 0, headRot: 0 });
    await s.hop(10);
    var to = a.x < layerW / 2 ? -50 : layerW + 50;
    a.clampX = false;
    await s.walkTo(to, 150);
    s.pose({ alpha: 0 }); await s.wait(200);
  }

  // ─── It is the logo ───
  // Mini in a chat is the head in the top bar, gone for a walk. Before it turns
  // up anywhere, the logo grows a body, arms and legs where it sits, and walks
  // off the edge of the screen. When it has left the chat it comes back along
  // the top bar at the pace it left with, stands in its place, and folds itself
  // away into a logo again. While it is out, the logo's place is empty.
  var LOGO = { chain: Promise.resolve(), away: false };
  function logoMark() {
    var n = document.querySelector('.topbar .logo .cb-headmark');
    return n && n.offsetParent !== null ? n : null;
  }
  function logoStage(mark) {
    var r = mark.getBoundingClientRect();
    if (r.width < 8) return null;
    var k = (r.width * 74 / 96) / 50;                     // so its head is exactly the size of the logo's
    var layer = document.createElement('div');
    layer.className = 'cm-top';
    var tabs = document.getElementById('tab-bar'), tb = tabs && tabs.offsetParent !== null ? tabs.getBoundingClientRect().bottom : 0;
    layer.style.height = Math.ceil(Math.max(r.bottom + 30, tb + 26)) + 'px';
    document.body.appendChild(layer);
    var actor = new Actor(layer, 92 * k);
    var home = { x: r.left + r.width * 11 / 96 + 25 * k, y: r.top + r.height * 26 / 96 + 78 * k };
    var edge = home.x < window.innerWidth / 2 ? -46 : window.innerWidth + 46;
    actor.clampX = false;
    actor.surf = function () { return { y: home.y, l: -200, r: window.innerWidth + 200 }; };
    actor.baseY = home.y;
    return { layer: layer, actor: actor, home: home, edge: edge, done: function () { actor.destroy(); if (layer.parentNode) layer.parentNode.removeChild(layer); } };
  }
  function logoLeave() {
    LOGO.chain = LOGO.chain.then(function () {
      var mark = logoMark();
      if (!mark || LOGO.away) return;
      var st = logoStage(mark); if (!st) return;
      var a = st.actor;
      a.x = st.home.x; a.v.grow = 0; a.t.grow = 0; a.v.alpha = 1; a.t.alpha = 1;
      a.draw();
      mark.style.visibility = 'hidden';
      LOGO.away = true;
      return a.play(async function (s) {
        await s.wait(260);
        s.pose({ eyeX: -3 }); await s.wait(300); s.pose({ eyeX: 3 }); await s.wait(300); s.pose({ eyeX: 0 });   // is anyone looking
        await s.wait(160);
        s.pose({ grow: 1 });                                                 // body, arms, legs
        await s.wait(950);
        s.pose({ armL: 150, armR: 150, headY: -1.5 }); await s.wait(380);     // a stretch
        s.pose({ armL: 0, armR: 0, headY: 0 }); await s.wait(260);
        await s.walkTo(st.edge, 62);
      }).then(st.done, st.done);
    });
    return LOGO.chain;
  }
  function logoReturn(speed) {
    LOGO.chain = LOGO.chain.then(function () {
      if (!LOGO.away) return;
      var mark = document.querySelector('.topbar .logo .cb-headmark');
      var shown = mark && mark.offsetParent !== null;
      function home() { if (mark) mark.style.visibility = ''; LOGO.away = false; }
      var st = shown ? logoStage(mark) : null;
      if (!st) { home(); return; }
      var a = st.actor, sp = clamp(speed || 62, 40, 230);
      a.x = st.edge; a.v.alpha = 1; a.t.alpha = 1;
      return a.play(async function (s) {
        if (sp > 170) s.pose({ armL: 162, armR: 162, shake: 1, lid: -0.3 });   // it left running: it comes back running
        else if (sp > 95 && sp < 125) s.pose({ lid: 0.5, armL: -34, armR: -34 }); // it stormed off: still cross
        await s.walkTo(st.home.x, sp);
        s.pose({ shake: 0, armL: 0, armR: 0, lid: 0, turn: 0 });
        await s.wait(sp > 170 ? 420 : 240);
        if (sp > 170) { await sigh(s); }                                       // out of breath
        s.pose({ eyeX: -3 }); await s.wait(240); s.pose({ eyeX: 3 }); await s.wait(240); s.pose({ eyeX: 0 });
        s.pose({ grow: 0, squash: 1, headY: 0, headRot: 0, lean: 0 });         // fold away
        await s.wait(820);
      }).then(function () { home(); st.done(); }, function () { home(); st.done(); });
    });
    return LOGO.chain;
  }


  // ─── A life in the top bar ───
  // Mini does not only leave the top bar for a chat. Things up there bother it
  // too, and it deals with them at the size it is as a logo:
  //
  //   five    the five-hour limit is over 90%. Panic. It runs to the end of
  //           the bar and tries to drag it back, or gets round the far side
  //           and shoves, with its hands or with its back. The bar does not
  //           move. It wears itself out and trudges home.
  //   week    the weekly limit is over 90%. That bar is in the top half, out
  //           of reach, so it jumps for it. And jumps. And cannot.
  //   notify  something is waiting for the person and the bell is lit. Some of
  //           the time, it walks over and points at it.
  //   project where the sidebar is beside the work, instead of the bell it may
  //           go to the project that is waiting: off the screen, in again
  //           along the first project in the list, and down the list one
  //           project at a time, by a climb or a jump.
  //   party   a limit has reset. It jumps for joy, or runs a lap of honour
  //           with its arms up, or dances, or spins; more of it for the
  //           weekly one.
  //   update  there is an update. It jumps down onto the tabs, walks along to
  //           Settings and points at it. If Settings is off the side of a
  //           narrow screen it hauls the row of tabs along, a little at a time,
  //           until it is there. Once when the update turns up, then seldom.
  //
  // Which variant, how long it keeps at it and what it does when it gives up
  // are picked at random each time. If a chat needs it while it is out on one
  // of these, it drops what it is doing and walks off the screen to go there.
  var TOPKEY = { top: true };
  var TOP = { busy: false, cut: null, next: { five: 0, week: 0, update: 0 }, seen: {}, timer: 0 };
  var UPDATE_EVERY = 4 * 3600000;                         // a reminder, not a nag
  function vis(n) { return n && n.offsetParent !== null ? n : null; }
  function barEnd(id) {
    var b = vis(document.getElementById(id)); if (!b || !b.parentNode) return null;
    var r = b.getBoundingClientRect(), pr = b.parentNode.getBoundingClientRect();
    if (pr.width < 40 || pr.height < 4) return null;
    return { x: r.right, pct: r.width / pr.width * 100, top: pr.top, bottom: pr.bottom, left: pr.left, right: pr.right };
  }
  // A project row in the sidebar that is waiting for an answer, on screen, on a
  // layout wide enough to have the sidebar beside the work (folded or not).
  function waitingProject() {
    if (window.innerWidth < 900 && !TOP.anyWidth) return null;
    var list = document.querySelectorAll('.sidebar .sidebar-item.ts-input');
    for (var i = 0; i < list.length; i++) {
      var n = list[i]; if (n.offsetParent === null) continue;
      var b = n.getBoundingClientRect();
      if (b.width > 20 && b.top > 40 && b.bottom < window.innerHeight - 8) return n;
    }
    return null;
  }
  function topRead() {
    var badge = document.getElementById('update-badge');
    return {
      five: barEnd('fh-usage'), week: barEnd('wk-usage'),
      update: !!(badge && badge.offsetParent !== null && badge.style.display !== 'none'),
      tabs: vis(document.getElementById('tab-bar')),
      settings: vis(document.querySelector('#tab-bar .tab-btn[data-tab="settings"]')),
      bell: vis(document.getElementById('approvals-btn')),
      project: waitingProject(),
      bells: parseInt((document.getElementById('approvals-count') || {}).textContent, 10) || 0
    };
  }
  // Straining at something that will not give: feet slipping, breath going.
  async function strain(s, lean, ms, dustDx) {
    var a = s.a, end = a.time + ms / 1000, i = 0;
    while (a.time < end) {
      s.pose({ legL: i % 2 ? 26 : -22, legR: i % 2 ? -22 : 26, lean: lean * (i % 2 ? 1 : 0.72), squash: i % 2 ? 0.93 : 0.97, lid: 0.86, headY: 2 });
      if (i % 3 === 0) s.fx('dust', 48 + dustDx, GROUND - 3, ';--dx:' + (dustDx > 0 ? 9 : -9) + 'px');
      if (i % 4 === 1) s.fx('sweat', 48 + dustDx * 0.5, 10);
      await s.wait(150); i++;
    }
    s.pose({ legL: 0, legR: 0 });
  }
  async function worn(s) {                                                    // spent
    s.pose({ reach: 0, armL: 0, armR: 0, lean: 0, squash: 0.9, headY: 5, lid: 0.8, turn: 0, headRot: 0 });
    s.fx('puff', 64, 34); await s.wait(700);
    s.fx('puff', 30, 34); await s.wait(600);
    if (Math.random() < 0.6) { s.pose({ sit: 1, swing: 0, armL: 20, armR: 20, squash: 1 }); await s.wait(rnd(1500, 2400)); s.pose({ sit: 0 }); await s.wait(600); }
    s.pose({ squash: 1, headY: 0, lid: 0.2 });
    s.pose({ armR: -118 }); await s.wait(700); s.pose({ armR: 0 }); await s.wait(250);   // wipe its brow
  }
  async function sceneFive(s, st, r) {
    var a = s.a, k = a.k, end = r.five.x, W = window.innerWidth;
    s.pose({ turn: end > a.x ? 0.8 : -0.8, eyeX: 0 }); await s.wait(500);     // it has seen the number
    s.pose({ lid: -0.35, headY: -2, armL: 150, armR: 150, shake: 0.9 }); s.fx('bang', 48, -14);
    await s.wait(620);
    var roomRight = Math.min(W, r.five.right) - end;
    var far = roomRight > 44 * k + 8;                                         // is there room to get round the other side
    var ways = far ? ['pull', 'push', 'back', 'charge', 'plead'] : ['pull', 'plead'];
    var rounds = 1 + Math.floor(Math.random() * 2);
    for (var n = 0; n < rounds; n++) {
      var way = TOP.force || ways[deal(far ? 'five-far' : 'five-near', ways.length)];
      var heaves = 1 + Math.floor(Math.random() * 3);
      if (way === 'pull') {                                                   // get hold of the end and heave it back
        // Hands on the end of the bar, feet nearer it than the hands, leaning
        // away: where it stands is worked out from where the hands must be.
        // Feet planted almost at the end of the bar, body thrown right back,
        // arms out straight to the edge: the whole of it is one line pulling.
        await s.walkTo(clamp(end - 5 * k, 12, W - 12), 150);
        s.pose({ shake: 0, lid: 0, turn: 0.8, reach: 1, armR: 100, armL: -92, headY: 0 }); await s.wait(360);   // take hold
        for (var i = 0; i < heaves; i++) {
          s.pose({ lean: -34, squash: 0.94, armR: 124, armL: -114, headRot: -8 });                               // and lean
          await strain(s, -34, rnd(800, 1700), 14);
          s.pose({ lean: -12, squash: 1, lid: 0.3, armR: 104, armL: -96, headRot: 0 }); await s.wait(rnd(200, 420));
        }
      } else if (way === 'push') {                                            // round the far side, hands on it, shove
        // Hands flat on the end of the bar, feet well back, leaning into it.
        await s.walkTo(clamp(end + 33 * k, 12, W - 12), 150);
        s.pose({ shake: 0, lid: 0, turn: -0.8, reach: -1, armL: 90, armR: -80, headY: 0 }); await s.wait(360);
        for (var j = 0; j < heaves; j++) { await strain(s, -19, rnd(700, 1600), 14); s.pose({ lean: -5, squash: 1, lid: 0.3 }); await s.wait(rnd(200, 420)); }
      } else if (way === 'back') {                                            // put its back into it
        // Its back against the end of the bar, feet out in front.
        await s.walkTo(clamp(end + 23 * k, 12, W - 12), 150);
        s.pose({ shake: 0, lid: 0, turn: 0.8, armL: 26, armR: 26, headRot: -9, headY: 0 }); await s.wait(360);
        for (var m = 0; m < heaves; m++) { await strain(s, -17, rnd(800, 1700), 14); s.pose({ lean: -6, squash: 1, lid: 0.3 }); await s.wait(rnd(200, 420)); }
      } else if (way === 'charge') {                                          // a run-up. this will do it.
        await s.walkTo(clamp(end + 30 * k + rnd(46, 70), 12, W - 12), 120);
        s.pose({ shake: 0, lid: 0.5, turn: -0.8, lean: -6, headY: 1 });
        for (var c = 0; c < 3; c++) { s.pose({ legR: 34 }); await s.wait(150); s.fx('dust', 66, GROUND - 3, ';--dx:10px'); s.pose({ legR: 0 }); await s.wait(170); }   // paw the ground
        s.pose({ lean: -12, armL: 40, armR: 40 });
        await s.walkTo(clamp(end + 20 * k, 12, W - 12), 230);
        s.fxAt('ring', end, a.baseY - 12 * k); s.fx('bang', 48, -14);
        a.vx = 150; a.vel.squash -= 3;                                        // and straight back off it
        s.pose({ lid: 0.94, lean: 14, armL: 150, armR: 150, turn: 0 });
        await s.hop(7);
        s.pose({ sit: 1, swing: 0, lean: 0, armL: 30, armR: 30, headY: 4 });
        for (var d2 = 0; d2 < 5; d2++) { s.pose({ headRot: d2 % 2 ? 10 : -10 }); await s.wait(170); }       // stars
        s.pose({ headRot: 0, lid: 0.3, sit: 0, headY: 0, armL: 0, armR: 0 }); await s.wait(650);
      } else {                                                                // ask it nicely
        await s.walkTo(clamp(end + (far ? 30 : -30) * k, 12, W - 12), 110);
        var fd = far ? -1 : 1;
        s.pose({ shake: 0, lid: 0, turn: fd * 0.8, squash: 0.86, headY: 3, reach: fd, armL: fd < 0 ? 62 : -52, armR: fd < 0 ? -52 : 62 }); await s.wait(1300);   // down on its knees to it
        s.pose({ turn: 0, eyeY: -3, headRot: 6 }); await s.wait(900);          // then to you
        s.pose({ turn: fd * 0.8, eyeY: 0, headRot: 0 }); await s.wait(900);
        s.pose({ squash: 1, headY: 0, reach: 0, armL: 0, armR: 0 }); await s.wait(350);
      }
      if (way !== 'plead' || Math.random() < 0.4) await worn(s); else { s.pose({ turn: 0 }); await sigh(s); }
      if (n < rounds - 1) { s.pose({ turn: end > a.x ? 0.8 : -0.8, lid: 0.45 }); await s.wait(700); }   // look at it. right. something else.
    }
    var after = deal('five-after', 3) / 3;
    if (after < 0.34) {                                                       // kick it, regret it
      s.pose({ turn: end > a.x ? 0.7 : -0.7, lid: 0.5 }); await s.wait(300);
      s.pose({ legR: end > a.x ? -70 : 70 }); await s.wait(170); s.fxAt('ring', end, a.baseY - 8 * k);
      s.pose({ legR: 0, lid: -0.35 }); s.fx('bang', 48, -14);
      for (var h = 0; h < 4; h++) await s.hop(5);
    } else if (after < 0.67) { s.pose({ turn: 0 }); await shrug(s); }         // to you: I tried
    else { s.pose({ turn: end > a.x ? 0.7 : -0.7 }); await shakeHead(s); }
    return 'tired';
  }
  async function sceneWeek(s, st, r) {
    var a = s.a, k = a.k, end = r.week.x, W = window.innerWidth;
    s.pose({ eyeY: -4.5, headRot: -7 }); await s.wait(700);                   // up there
    s.pose({ lid: -0.3 }); s.fx('bang', 48, -14); await s.wait(420);
    s.pose({ lid: 0, eyeY: 0, headRot: 0 });
    await s.walkTo(clamp(end - 6 * k, 12, W - 12), 120);
    var tries = 3 + Math.floor(Math.random() * 3);
    for (var i = 0; i < tries; i++) {
      s.pose({ squash: 0.86, armL: 30, armR: 30, eyeY: -4.5 }); await s.wait(220);        // crouch
      s.pose({ squash: 1, armL: 170, armR: 170 });
      await s.hop(7 + i * 1.2);                                               // not high enough. never high enough.
      s.pose({ armL: 0, armR: 0 }); await s.wait(rnd(160, 320));
    }
    var after = deal('week-after', 3) * 0.34;
    if (after < 0.4) {                                                        // on tiptoe, trembling
      s.pose({ squash: 1.09, armR: 172, headY: -3, shake: 0.3, eyeY: -4.5 }); await s.wait(1500);
      s.pose({ squash: 1, armR: 0, headY: 0, shake: 0, eyeY: 0 });
    } else if (after < 0.7) {                                                 // shake a fist at it
      s.pose({ armR: 158, shake: 0.55, lid: 0.5, eyeY: -4 }); s.fx('anger', 76, 6); await s.wait(1100);
      s.pose({ armR: 0, shake: 0, lid: 0, eyeY: 0 });
    }
    await worn(s);
    return 'tired';
  }
  async function sceneUpdate(s, st, r) {
    var a = s.a, k = a.k, W = window.innerWidth, tabs = r.tabs, btn = r.settings;
    if (!tabs || !btn) return 'calm';
    function floor() { var b = tabs.getBoundingClientRect(); return { y: b.bottom - 1, l: -200, r: W + 200 }; }
    function seen() { var b = btn.getBoundingClientRect(), t = tabs.getBoundingClientRect(); return b.left >= t.left - 2 && b.right <= Math.min(t.right, W) + 2; }
    s.pose({ eyeY: 4, headY: 3 }); await s.wait(600);                         // what is that down there
    s.pose({ eyeY: 0, headY: 0, armL: 120, armR: 120 }); await s.wait(200);
    await s.fallTo(floor);
    s.pose({ armL: 0, armR: 0 }); await s.wait(380);
    // Settings is off the side: haul the tabs along until it is not.
    for (var n = 0; n < 9 && !seen(); n++) {
      var t = tabs.getBoundingClientRect(), from = clamp(Math.min(t.right, W) - 26, 30, W - 14);
      await s.walkTo(from, 70);
      s.pose({ turn: 0.75, reach: 1, armR: 90, armL: -80 }); await s.wait(300);  // get a grip on it
      var x0 = a.x, base = tabs.scrollLeft, going = s.walkTo(from - rnd(42, 58), 13);
      a.t.turn = 0.75;                                                        // backwards, leaning into it
      var i = 0, set = tabs.scrollLeft, fate = null;
      while (a.tx !== null) {
        // Somebody else has hold of the tabs: the person is dragging them.
        if (Math.abs(tabs.scrollLeft - set) > 2) { fate = await carried(set); break; }
        set = tabs.scrollLeft = base + (x0 - a.x);
        set = tabs.scrollLeft;                                                // what it actually became (it stops at the end)
        s.pose({ lean: i % 2 ? -19 : -13, squash: i % 2 ? 0.93 : 0.97, lid: 0.85, headY: 2 });
        if (i % 5 === 0) s.fx('sweat', 40, 10);
        if (i % 4 === 0) s.fx('dust', 62, GROUND - 3, ';--dx:9px');
        await s.wait(120); i++;
      }
      if (fate === 'flung') return await stompBack();
      if (fate === null) { await going; }
      s.pose({ reach: 0, lean: 0, squash: 1, lid: 0.3, armL: 0, armR: 0, headY: 0, turn: 0, shake: 0, legL: 0, legR: 0 }); await s.wait(260);
      if (fate === 'dropped') { await shakeHead(s); continue; }               // let go of. dizzy. right, where was it
      if (!seen()) { s.fx('puff', 64, 34); s.pose({ squash: 0.92, headY: 4, lid: 0.7 }); await s.wait(rnd(600, 1000)); s.pose({ squash: 1, headY: 0, lid: 0 }); }
    }
    // It was holding the tabs and the person dragged them. It goes where they
    // go, hanging on. Dragged far enough, or flicked hard enough, it leaves the
    // screen with them.
    async function carried(gripScroll) {
      var gripX = a.x, prev = tabs.scrollLeft, still = 0, v = 0;
      a.tx = null; a.onArrive = null;
      a.carry = function () { return gripX - (tabs.scrollLeft - gripScroll); };
      s.pose({ lid: -0.35, reach: 1, armR: 90, armL: -80, turn: 0.75, shake: 0.35, squash: 1, headY: -1 }); s.fx('bang', 48, -14);
      try {
        while (still < 6) {
          await s.wait(70);
          var cur = tabs.scrollLeft, d = cur - prev;
          if (d) { v = -d / 0.07; still = 0; s.pose({ lean: clamp(d * 2.2, -34, 34), legL: clamp(d * 3, -50, 50), legR: clamp(d * 3 + 14, -50, 50) }); }
          else still++;
          prev = cur;
          if (a.x < -30 || a.x > W + 30) return 'flung';
          // the tabs hit the end of their travel while it was still going fast
          if (!d && Math.abs(v) > 260) {
            a.carry = null;
            s.pose({ armL: 160, armR: 160, shake: 1, lean: v > 0 ? 30 : -30 });
            await s.walkTo(v > 0 ? W + 60 : -60, clamp(Math.abs(v), 300, 700));
            return 'flung';
          }
          if (still > 2) v = 0;
        }
      } finally { a.carry = null; }
      return 'dropped';
    }
    // Thrown off the screen. It comes back along the top bar, not pleased, has
    // a word, and goes home: the person is busy, the update can wait.
    async function stompBack() {
      var from = a.x < W / 2 ? -46 : W + 46;
      s.rest(); s.pose({ alpha: 0 }); await s.wait(rnd(1100, 1900));
      a.surf = a.homeSurf; a.baseY = st.home.y; a.yOff = 0; a.air = false; a.x = from; a.vx = 0;
      s.pose({ alpha: 1, lid: 0.5, armL: -34, armR: -34 });
      var going2 = s.walkTo(st.home.x + (from < 0 ? 0 : 0), 46), n2 = 0;
      while (a.tx !== null) { if (n2 % 6 === 0) s.fx('anger', 76, 4); n2++; await s.wait(160); }
      await going2;
      s.pose({ turn: 0 }); await s.wait(500);                                 // at you
      var w2 = Math.random();
      if (w2 < 0.4) { s.pose({ armL: 0, armR: 158, shake: 0.55, lean: 4 }); await s.wait(1000); s.pose({ shake: 0, armR: 0, lean: 0 }); }
      else if (w2 < 0.75) { await shakeHead(s); }
      else { steam(s); await s.hop(5); await s.hop(5); }
      s.pose({ armL: 0, armR: 0, lid: 0 }); await s.wait(300);
      return 'calm';
    }
    var b = btn.getBoundingClientRect(), bx = (b.left + b.right) / 2, by = (b.top + b.bottom) / 2;
    var side = bx - 22 * k > 12 ? -1 : 1;
    await s.walkTo(clamp(bx + side * 22 * k, 10, W - 10), 66);
    var arm = side < 0 ? 'armR' : 'armL', o = { turn: side < 0 ? 0.6 : -0.6, eyeY: -1 };
    for (var rep2 = 0; rep2 < 2; rep2++) {
      o[arm] = 104; s.pose(o); await s.wait(320);
      for (var j = 0; j < 3; j++) { o[arm] = 92; s.pose(o); await s.wait(130); s.fxAt('ring', bx, by); o[arm] = 110; s.pose(o); await s.wait(180); }
      s.pose({ turn: 0, eyeY: 0 }); await s.wait(700);                        // look at you. this. here.
      if (rep2 === 0) { await s.hop(6); await s.hop(6); }
    }
    o = {}; o[arm] = 0; s.pose(o); await s.wait(300);
    // and back upstairs
    await s.jumpTo(st.actor.homeSurf);
    await s.wait(260);
    return 'calm';
  }
  // Something is waiting for the person: the bell in the top bar is lit.
  async function sceneNotify(s, st, r) {
    var a = s.a, k = a.k, W = window.innerWidth, btn = r.bell; if (!btn) return 'calm';
    function at() { var b = btn.getBoundingClientRect(); return b.width ? { x: (b.left + b.right) / 2, y: (b.top + b.bottom) / 2, l: b.left, r: b.right } : null; }
    var t = at(); if (!t) return 'calm';
    s.pose({ turn: t.x > a.x ? 0.8 : -0.8, lid: -0.2 }); await s.wait(650);   // oh, that is new
    var side = t.l - 24 * k > 12 ? -1 : 1;
    await s.walkTo(clamp((side < 0 ? t.l : t.r) + side * 20 * k, 10, W - 10), 96);
    var arm = side < 0 ? 'armR' : 'armL', o = { turn: side < 0 ? 0.6 : -0.6 };
    var how = Math.random();
    for (var n = 0; n < 2; n++) {
      o[arm] = 100; s.pose(o); await s.wait(300);
      for (var i = 0; i < 3; i++) { o[arm] = 88; s.pose(o); await s.wait(130); var u = at(); if (u) s.fxAt('ring', u.x, u.y); o[arm] = 106; s.pose(o); await s.wait(170); }
      s.pose({ turn: 0 }); await s.wait(650);                                 // you. look.
      if (n === 0) {
        if (how < 0.35) { await s.hop(6); await s.hop(6); }
        else if (how < 0.7) { s.pose({ armL: -118, armR: -118, squash: 1.06, headY: -2 }); await s.wait(700); s.pose({ armL: 0, armR: 0, squash: 1, headY: 0 }); }   // hands round its mouth: oi
        else await wave(s, 3);
        o.turn = side < 0 ? 0.6 : -0.6;
      }
    }
    o = {}; o[arm] = 0; s.pose(o); await s.wait(300);
    return 'calm';
  }
  // Hand over hand, not falling: to a ledge lower down or one higher up, with
  // its back to you and not enjoying it.
  async function climbTo(s, surf, speed) {
    var a = s.a, was = a.baseY + a.yOff, sf = surf(); if (!sf) return;
    a.onLand = null; a.air = false; a.vy = 0;
    a.surf = surf; a.baseY = sf.y; a.yOff = was - sf.y;
    var down = a.yOff < 0, i = 0, pauseAt = Math.abs(a.yOff) * rnd(0.35, 0.6), looked = Math.abs(a.yOff) < 60;   // a short step down is not worth a look
    s.pose({ turn: 2.1, shake: 0.22, lid: 0.3 });
    await s.wait(500);
    while (Math.abs(a.yOff) > 1.5) {
      var step = Math.min(Math.abs(a.yOff), (speed || 34) * 0.06);
      a.yOff += down ? step : -step;
      s.pose(i % 2 ? { armL: 158, armR: 112, legL: -14, legR: 12 } : { armL: 112, armR: 158, legL: 12, legR: -14 });
      if (!looked && Math.abs(a.yOff) < pauseAt) {                            // do not look down. it looked down.
        looked = true;
        s.pose({ turn: 1.15, headY: 3, eyeY: 4, shake: 0.5, lid: -0.3 }); s.fx('sweat', 72, 14);
        await s.wait(rnd(800, 1300));
        s.pose({ turn: 2.1, headY: 0, eyeY: 0, shake: 0.22, lid: 0.3 });
        await s.wait(300);
      }
      await s.wait(i % 2 ? 130 : 60); i++;
    }
    a.yOff = 0;
    s.pose({ turn: 0, shake: 0, lid: 0, armL: 0, armR: 0, legL: 0, legR: 0 });
    await s.wait(300);
    s.fx('puff', 64, 34); s.pose({ squash: 0.93, headY: 3 }); await s.wait(500); s.pose({ squash: 1, headY: 0 });   // made it
  }
  // A project in the sidebar needs an answer. It does not come down out of the
  // top bar through thin air: it leaves the screen, comes back in from the side
  // along the first project in the list, and makes its way down the list one
  // project at a time, each a careful climb or a jump, to the one that is
  // waiting. It goes off the side again when it has made its point.
  async function sceneProject(s, st, r) {
    var a = s.a, k = a.k, W = window.innerWidth, row = r.project; if (!row) return 'calm';
    function boxOf(n) { var b = n && n.isConnected && n.offsetParent !== null ? n.getBoundingClientRect() : null; return b && b.width ? b : null; }
    var all = document.querySelectorAll('.sidebar .sidebar-item'), rows = [];
    for (var i = 0; i < all.length; i++) { var bb = boxOf(all[i]); if (bb && bb.top > 40 && bb.bottom < window.innerHeight - 8) rows.push(all[i]); }
    var ti = rows.indexOf(row); if (ti < 0) return 'calm';
    var b0 = boxOf(row);
    st.layer.style.height = '100%';
    var narrow = b0.width < 90;                                               // the sidebar is folded to icons
    function spotX(n) { var b = boxOf(n) || b0; return narrow ? b.right + 12 * k : clamp(b.right - 20 * k, b.left + 40, W - 12); }
    function ledge(n) { return function () { var b = boxOf(n) || b0; return { y: b.bottom - 1, l: -300, r: W + 300 }; }; }
    var sideX = Math.min(b0.left, 0) - 46;                                    // off the side the list is on
    s.pose({ eyeY: 4, headY: 3, turn: -0.4 }); await s.wait(700);             // someone down there wants something
    s.pose({ eyeY: 0, headY: 0, turn: 0 });
    await s.walkTo(st.edge, 70);                                              // off the top bar
    await s.wait(rnd(500, 1000));
    // and in again, along the first project
    var f0 = ledge(rows[0])();
    a.surf = ledge(rows[0]); a.baseY = f0.y; a.yOff = 0; a.air = false; a.x = sideX; a.vx = 0;
    await s.walkTo(spotX(rows[0]), 62);
    await s.wait(300);
    var careful = 0;
    for (var n = 1; n <= ti; n++) {
      // sometimes two at a time, when there is a way to go
      if (ti - n >= 2 && Math.random() < 0.25) n++;
      s.pose({ headY: 4, eyeY: 4.5 }); await s.wait(rnd(260, 520));           // look at the next one down
      s.pose({ headY: 0, eyeY: 0 });
      if (Math.random() < 0.5) { careful++; await climbTo(s, ledge(rows[n]), 38); }
      else {
        s.pose({ squash: 0.9, armL: 30, armR: 30 }); await s.wait(200);
        s.pose({ squash: 1, armL: 140, armR: 140 });
        await s.fallTo(ledge(rows[n]));
        s.pose({ armL: 0, armR: 0 }); await s.wait(rnd(200, 380));
      }
      var want = spotX(rows[n]); if (Math.abs(want - a.x) > 4) await s.walkTo(want, 50);
    }
    function target() { var b = boxOf(row); if (!b) return null; var d = row.querySelector('.dot'), db = d && d.offsetParent !== null ? d.getBoundingClientRect() : null; return db && db.width ? { x: db.left + db.width / 2, y: db.top + db.height / 2 } : { x: b.left + Math.min(20, b.width / 2), y: (b.top + b.bottom) / 2 }; }
    for (var m = 0; m < 2; m++) {
      s.pose({ turn: -0.6, armL: 98 }); await s.wait(320);
      for (var j = 0; j < 3; j++) { s.pose({ armL: 86 }); await s.wait(130); var t = target(); if (t) s.fxAt('ring', t.x, t.y); s.pose({ armL: 104 }); await s.wait(170); }
      s.pose({ turn: 0 }); await s.wait(700);                                 // this one. it is waiting for you.
      if (m === 0) { if (Math.random() < 0.5) { await s.hop(6); await s.hop(6); } else { s.pose({ armL: 0 }); await wave(s, 3); } }
    }
    s.pose({ armL: 0 }); await s.wait(300);
    // off the side, and home along the top bar
    await s.walkTo(sideX, 70);
    await s.wait(rnd(400, 800));
    a.surf = a.homeSurf; a.baseY = st.home.y; a.yOff = 0; a.air = false; a.x = st.edge; a.vx = 0;
    return careful > 1 ? 'tired' : 'calm';
  }
  // A limit has reset. Everything it was worried about is gone.
  async function sceneParty(s, st, r) {
    var a = s.a, k = a.k, W = window.innerWidth, big = TOP.partyFor === 'week';
    var COL = ['#818cf8', '#fbbf24', '#34d399', '#fb7185', '#38bdf8'];
    function throwUp() { for (var i = 0; i < 4; i++) s.fx('confetti', rnd(10, 86), rnd(-26, -4), ';--dx:' + Math.round(rnd(-12, 12)) + 'px;--cf:' + pick(COL)); }
    s.pose({ eyeY: -3, lid: -0.3 }); await s.wait(600);                       // it looks. it looks again.
    s.pose({ eyeY: 0, lid: 0 }); await s.wait(200);
    s.pose({ eyeY: -3, lid: -0.35 }); s.fx('bang', 48, -14); await s.wait(500);
    s.pose({ eyeY: 0 });
    var acts = [
      async function jumps() {                                                // up and down on the spot
        var n = 4 + Math.floor(Math.random() * 3);
        for (var i = 0; i < n; i++) { s.pose({ armL: 162, armR: 162, lid: 0.5 }); s.fx('spark', i % 2 ? 78 : 18, 4); if (i % 2 === 0) throwUp(); await s.hop(rnd(8, 13)); s.pose({ armL: 120, armR: 120 }); await s.wait(90); }
        s.pose({ armL: 0, armR: 0, lid: 0 });
      },
      async function lap() {                                                  // a lap of honour, arms in the air
        var far = clamp(a.x + rnd(0.3, 0.55) * W, 20, W - 20);
        s.pose({ armL: 162, armR: 162, shake: 0.9, lid: 0.5 });
        for (var i = 0; i < 2; i++) {
          var g = s.walkTo(i % 2 ? clamp(st.home.x + 30, 20, W - 20) : far, 185);
          while (a.tx !== null) { s.fx('spark', 48, -6); await s.wait(170); }
          await g; throwUp();
        }
        s.pose({ shake: 0, armL: 0, armR: 0, lid: 0, turn: 0 });
      },
      async function dance() {                                                // a little dance
        for (var i = 0; i < 8; i++) {
          s.pose(i % 2 ? { armL: 150, armR: 24, lean: 7, headRot: 8, legL: 0, legR: -18, lid: 0.5 } : { armL: 24, armR: 150, lean: -7, headRot: -8, legL: 18, legR: 0, lid: 0.5 });
          if (i % 2) s.fx('note', 74, 2, ';--dx:10px');
          await s.wait(230);
        }
        s.pose({ armL: 0, armR: 0, lean: 0, headRot: 0, legL: 0, legR: 0, lid: 0 });
      },
      async function spin() {                                                 // round and round, then a pump of the fist
        for (var i = 0; i < 2; i++) { s.pose({ turn: 2.1, armL: 60, armR: 60 }); await s.wait(260); s.pose({ turn: 0 }); await s.wait(260); }
        s.pose({ squash: 0.85, armR: 30 }); await s.wait(200);
        s.pose({ squash: 1, armR: 168, armL: 0 }); throwUp(); await s.hop(12);
        s.pose({ armR: 0 });
      }
    ];
    var count = big ? 3 : 1 + Math.floor(Math.random() * 2);
    for (var n = 0; n < count; n++) { await acts[deal('party', acts.length)](); await s.wait(rnd(250, 500)); }
    s.pose({ turn: 0 }); await wave(s, 3);                                    // to you: did you see that
    return 'calm';
  }
  var TOPSCENES = { five: sceneFive, week: sceneWeek, update: sceneUpdate, notify: sceneNotify, project: sceneProject, party: sceneParty };
  function topOuting(kind, r) {
    if (HOLDER || LOGO.away || TOP.busy) return false;
    var mark = logoMark(); if (!mark) return false;
    var st = logoStage(mark); if (!st) return false;
    var a = st.actor, cut = false;
    a.homeSurf = a.surf;
    HOLDER = TOPKEY; TOP.busy = true; LOGO.away = true;
    a.x = st.home.x; a.v.grow = 0; a.t.grow = 0; a.v.alpha = 1; a.t.alpha = 1; a.draw();
    mark.style.visibility = 'hidden';
    function free() { st.done(); TOP.busy = false; TOP.cut = null; if (HOLDER === TOPKEY) HOLDER = null; }
    // A chat wants it: leave from wherever it is, by the nearest edge. The
    // logo stays out; the chat brings it home.
    TOP.cut = function () {
      if (cut) return; cut = true;
      a.play(async function (s) {
        s.rest(); s.pose({ sit: 0, swing: 0, grow: 1 });
        if (a.air) await new Promise(function (res) { a.onLand = res; });
        s.pose({ lid: -0.3 }); s.fx('bang', 48, -14); await s.wait(300);      // oh. needed elsewhere
        s.pose({ lid: 0 });
        await s.walkTo(a.x < window.innerWidth / 2 ? -46 : window.innerWidth + 46, 120);
      }).then(free, free);
    };
    a.play(async function (s) {
      await s.wait(260);
      s.pose({ grow: 1 }); await s.wait(950);
      var mood = await TOPSCENES[kind](s, st, r);
      // home: slowly if it is worn out, head down
      s.rest();
      if (mood === 'tired') s.pose({ headY: 2.6, eyeY: 3.4, lid: 0.3, armSwing: 0.3 });
      await s.walkTo(st.home.x, mood === 'tired' ? 30 : 62);
      s.pose({ headY: 0, eyeY: 0, lid: 0, armSwing: 1, turn: 0 });
      await s.wait(300);
      if (mood === 'tired') await sigh(s);
      s.pose({ grow: 0, squash: 1, headRot: 0, lean: 0 });
      await s.wait(820);
    }).then(function () {
      if (cut) return;
      mark.style.visibility = ''; LOGO.away = false; free();
    });
    return true;
  }
  function topTick() {
    if (document.hidden || HOLDER || LOGO.away || TOP.busy || REDUCED) return;
    if (!logoMark()) return;
    var r = topRead(), t = Date.now(), ready = [];
    function due(kind, on, first, again) {
      if (!on) { TOP.seen[kind] = false; return; }
      if (!TOP.seen[kind]) { TOP.seen[kind] = true; if (TOP.next[kind] < t + first[0]) TOP.next[kind] = t + rnd(first[0], first[1]); }
      if (t >= TOP.next[kind]) ready.push([kind, again]);
    }
    // A limit that was well up and is suddenly near nothing has reset. What it
    // last saw is kept across reloads, so a reset that happened while the page
    // was shut is still noticed.
    var prev = TOP.prev;
    if (!prev) { prev = TOP.prev = {}; try { prev = TOP.prev = JSON.parse(localStorage.getItem('crundi_mini_pct') || '{}') || {}; } catch (e) { /* private mode */ } }
    ['five', 'week'].forEach(function (kk) {
      var now = r[kk] ? r[kk].pct : null; if (now === null) return;
      if (typeof prev[kk] === 'number' && prev[kk] >= 35 && prev[kk] - now >= 30) { TOP.partyFor = kk; TOP.partyAt = t + rnd(3000, 9000); }
      if (typeof prev[kk] !== 'number' || Math.abs(prev[kk] - now) >= 1) { prev[kk] = now; try { localStorage.setItem('crundi_mini_pct', JSON.stringify(prev)); } catch (e) { /* private mode */ } }
    });
    if (TOP.partyAt && t >= TOP.partyAt) { if (topOuting('party', r)) TOP.partyAt = 0; return; }
    due('five', r.five && r.five.pct >= 90, [6000, 20000], [7 * 60000, 16 * 60000]);
    due('week', r.week && r.week.pct >= 90, [15000, 45000], [12 * 60000, 25 * 60000]);
    // The update reminder is remembered across reloads, so reloading the page
    // does not bring it straight back.
    var last = 0; try { last = +localStorage.getItem('crundi_mini_update_at') || 0; } catch (e) { /* private mode */ }
    if (r.update && r.tabs && r.settings && t - last >= UPDATE_EVERY) due('update', true, [20000, 45000], [UPDATE_EVERY, UPDATE_EVERY * 1.5]);
    else TOP.seen.update = false;
    // A notification: only sometimes, and only when one has just arrived.
    var nb = (r.bell ? Math.max(1, r.bells) : 0) + (r.project ? document.querySelectorAll('.sidebar .sidebar-item.ts-input').length : 0);
    if (nb > (TOP.bells || 0) && t >= (TOP.next.notify || 0)) {
      if (Math.random() < 0.55) { TOP.seen.notify = true; TOP.next.notify = t + rnd(8000, 30000); TOP.bellDue = true; }
      else TOP.next.notify = t + rnd(10 * 60000, 20 * 60000);                 // not this time
    }
    TOP.bells = nb;
    if (TOP.bellDue && !r.bell && !r.project) TOP.bellDue = false;
    // the bell, or (where there is a sidebar) the project itself: either
    if (TOP.bellDue && t >= TOP.next.notify) ready.push([r.project && (!r.bell || Math.random() < 0.6) ? 'project' : 'notify', [20 * 60000, 40 * 60000]]);
    if (!ready.length) return;
    var go = pick(ready);
    if (topOuting(go[0], r)) {
      TOP.next[go[0]] = t + rnd(go[1][0], go[1][1]);
      if (go[0] === 'notify' || go[0] === 'project') { TOP.bellDue = false; TOP.next.notify = TOP.next[go[0]]; }
      if (go[0] === 'update') { try { localStorage.setItem('crundi_mini_update_at', String(t)); } catch (e) { /* private mode */ } }
    }
  }
  function topStart() { if (!TOP.timer && !REDUCED) TOP.timer = setInterval(topTick, 3000); }

  // There is one Mini. With several chats on screen, whichever needs it first
  // has it, and the others wait until it has left.
  var HOLDER = null;


  // ─── A sleeping chat ───
  // A chat that has been closed and is waiting to be resumed is drawn dimmed
  // behind a card with a Resume button. There is no message box to point at
  // there, and nothing is waiting, so none of the usual scenes belong in it.
  // What Mini does instead, when it feels like it: comes and stands on the
  // Resume button and shows it to you. If the card warns that the conversation
  // is a long one, that worries it, and it climbs down to "Compact and resume"
  // and makes a case for that instead.
  async function sceneParked(s, layerW, get) {
    var a = s.a, sf = s.surf(); if (!sf) return;
    await enter(s, Math.random() < 0.5 ? -40 : layerW + 40, (sf.l + sf.r) / 2, 62);
    await s.wait(300);
    s.pose({ eyeX: -3, eyeY: -2 }); await s.wait(600); s.pose({ eyeX: 3 }); await s.wait(600);   // dark in here
    s.pose({ eyeX: 0, eyeY: 0 });
    if (Math.random() < 0.5) { s.pose({ lid: 0.7, armL: 150, armR: 150, headY: -2 }); await s.wait(900); s.pose({ lid: 0, armL: 0, armR: 0, headY: 0 }); }   // a yawn: it is catching
    async function press(where) {                                             // this one. press this.
      var t = where(); if (!t) return;
      s.pose({ headY: 4, eyeY: 4.5, armL: 26, armR: 26 }); await s.wait(500);
      for (var i = 0; i < 2; i++) { await s.hop(6); var u = where(); if (u) s.fxAt('ring', u.x, u.y); }
      s.pose({ headY: 0, eyeY: 0, armL: 0, armR: 0 }); await s.wait(300);
      var arm = Math.random() < 0.5 ? 'armL' : 'armR', o = {}; o[arm] = 24; s.pose(o); await s.wait(250);
      for (var j = 0; j < 3; j++) { o[arm] = 14; s.pose(o); await s.wait(140); o[arm] = 30; s.pose(o); await s.wait(160); }
      o[arm] = 0; s.pose(o); await s.wait(500);
    }
    await press(get.resume);
    // A long conversation: that warning is right under its feet.
    if (get.compact()) {
      s.pose({ headY: 6, eyeY: 4.5, squash: 0.93 }); await s.wait(1100);      // read it
      s.pose({ headY: -2, eyeY: 0, squash: 1, lid: -0.35 }); s.fx('bang', 48, -12); await s.wait(500);
      s.fx('sweat', 72, 14);
      s.pose({ armL: -118, armR: -118, lid: 0.3, headY: 3 }); await s.wait(900);   // hands to its face
      s.pose({ armL: 0, armR: 0, headY: 0, lid: 0 });
      await pace(s, 2, 40);
      var cs = get.compactSurf();
      if (cs) {
        s.pose({ headY: 5, eyeY: 4.5 }); await s.wait(500);
        s.pose({ headY: 0, eyeY: 0, armL: 120, armR: 120 }); await s.wait(200);
        var held = { y: cs.y, l: -100, r: layerW + 100 };
        await s.fallTo(function () { var f = get.compactSurf(); return f ? { y: f.y, l: -100, r: layerW + 100 } : held; });
        a.surf = get.compactSurf;
        s.pose({ armL: 0, armR: 0 }); await s.wait(300);
        var c2 = get.compactSurf(); if (c2) await s.walkTo((c2.l + c2.r) / 2, 50);
        for (;;) {
          await press(get.compact);
          s.pose({ armL: -60, armR: -60, headRot: 7, eyeY: -2 }); await s.wait(1300);     // please
          s.pose({ armL: 0, armR: 0, headRot: 0, eyeY: 0 });
          await s.wait(rnd(2500, 5000));
          if (Math.random() < 0.4) await watch(s);
        }
      }
    }
    // Nothing to worry about: keep it company. Point now and then, and in the
    // end sit down on the button and doze off along with the chat.
    for (var n = 0; n < 3; n++) {
      await s.wait(rnd(2200, 4200));
      if (n === 1) await wave(s, 3); else await press(get.resume);
    }
    await s.hop(5);
    s.pose({ sit: 1, swing: 0.6, armL: 8, armR: 8 }); await s.wait(2600);
    s.pose({ lid: 0.5, headY: 2 }); await s.wait(900);
    a.autoBlink = false;
    s.pose({ lid: 0.94, headRot: 13, headY: 4, swing: 0 });
    for (;;) { s.fx('z', 66, 2, ';--dx:10px'); await s.wait(1500); }
  }
  function attachParked(veil, o) {
    addStyle();
    var layer = document.createElement('div');
    layer.className = 'cm-layer';
    veil.appendChild(layer);
    var me = {}, actor = null, scene = null, leaving = false, dead = false;
    var cellEl = veil.parentNode && veil.parentNode.closest ? (veil.closest('.term-cell') || veil.closest('.parked-cell') || veil.parentNode) : null;
    var since = Date.now(), wantAt = since + rnd(35000, 110000);              // if it feels like it, and not at once
    function width() { return layer.clientWidth || 300; }
    function box(node) {
      if (!node || !node.isConnected || node.offsetParent === null) return null;
      var r = node.getBoundingClientRect(), b = layer.getBoundingClientRect();
      return r.width ? { l: r.left - b.left, r: r.right - b.left, t: r.top - b.top, b: r.bottom - b.top } : null;
    }
    function q(sel) { return veil.querySelector(sel); }
    function topOf(sel) { return function () { var b = box(q(sel)); return b ? { y: b.t, l: b.l + 6, r: b.r - 6 } : null; }; }
    function mid(sel) { return function () { var b = box(q(sel)); return b ? { x: (b.l + b.r) / 2, y: (b.t + b.b) / 2 } : null; }; }
    var CMP = '.pk-alt[data-mode="compact"]';
    var get = { resume: mid('.pk-resume'), compact: mid(CMP), compactSurf: topOf(CMP) };
    function gone() {
      var sp = actor ? actor.lastSpeed : 0;
      if (actor) { actor.destroy(); actor = null; }
      scene = null; leaving = false;
      if (HOLDER !== me || handing) return;
      logoReturn(sp).then(function () { if (HOLDER === me && !scene) HOLDER = null; });
    }
    // Resumed. The card it was standing on is taken away with everything in
    // it, this layer included, so it cannot leave from here: it would just
    // vanish. It is picked up exactly where it was last seen and put on a
    // layer of the page itself, to be glad about it and run off properly.
    var handing = false, lastAt = null;
    function handOver() {
      if (handing || !actor || !lastAt) { gone(); return; }
      handing = true;
      var old = actor, top = document.createElement('div');
      top.className = 'cm-top'; top.style.height = '100%';
      document.body.appendChild(top);
      var a = new Actor(top, 50), key;
      for (key in old.v) { a.v[key] = old.v[key]; a.t[key] = old.t[key]; }
      a.x = lastAt.x; a.baseY = lastAt.y; a.clampX = false; a.el.style.pointerEvents = 'none';
      var floor = { y: lastAt.y, l: -200, r: window.innerWidth + 200 };
      a.surf = function () { return floor; };
      a.v.alpha = 1; a.t.alpha = 1; a.autoBlink = true;
      old.destroy(); actor = null; scene = null; leaving = false;
      function done() {
        a.destroy(); if (top.parentNode) top.parentNode.removeChild(top);
        handing = false;
        logoReturn(215).then(function () { if (HOLDER === me) HOLDER = null; });
      }
      // What it was standing on has gone, so after the moment it takes to
      // notice, it comes down: onto the message box of the chat that has just
      // woken, or the bottom of its pane. From up there, that is a parachute.
      function ground() {
        var c = cellEl && cellEl.isConnected ? cellEl.querySelector('.cc-composer') : null;
        var b = c && c.offsetParent !== null ? c.getBoundingClientRect() : null, y;
        if (b && b.height) y = b.top;
        else if (cellEl && cellEl.isConnected && cellEl.getBoundingClientRect().height) y = cellEl.getBoundingClientRect().bottom - 4;
        else y = window.innerHeight - 6;
        return { y: Math.max(y, floor.y), l: -200, r: window.innerWidth + 200 };
      }
      a.play(async function (s) {
        s.rest(); s.pose({ sit: 0, swing: 0, lid: -0.35, headY: -2, armL: 150, armR: 150 }); s.fx('bang', 48, -12);   // it is awake! and there is no floor
        await s.wait(380);
        s.pose({ legL: 22, legR: -22 });
        await s.fallTo(ground);
        s.pose({ legL: 0, legR: 0, lid: 0.5, armL: 160, armR: 160, headY: 0 });
        await s.wait(260);
        await s.hop(11); s.fx('spark', 20, 2); await s.hop(11); s.fx('spark', 76, 2);
        s.pose({ armL: 0, armR: 0, lid: 0 }); await s.wait(200);
        await runOff(s, window.innerWidth);
      }).then(done, done);
    }
    function leave() {
      if (!actor || leaving) return;
      leaving = true;
      var a = actor, held = { y: a.baseY, l: -100, r: width() + 100 };
      a.surf = function () { return held; };
      a.play(function (s) { return sceneBye(s, width()); }).then(function () { if (actor === a) gone(); });
    }
    function onMini(e) { return !!(actor && e.target && actor.el.contains(e.target)); }
    function acted(e) { if (e && onMini(e)) return; since = Date.now(); wantAt = since + rnd(60000, 180000); if (scene === 'parked') leave(); }
    function tapped(e) {
      if (!onMini(e) || leaving || scene !== 'parked') return;
      e.preventDefault(); e.stopPropagation();
      var a = actor, w = width(), list = [ANNOYED[0], ANNOYED[1], ANNOYED[3]], f = pick(list);
      a.tx = null; a.autoBlink = true;
      a.play(async function (s) { s.rest(); s.pose({ sit: 0, swing: 0 }); await s.wait(300); await f(s, w); s.rest(); await s.wait(600); leaving = true; await sceneBye(s, w); }).then(function () { if (actor === a) gone(); });
    }
    veil.addEventListener('pointerdown', acted, { passive: true, capture: true });
    layer.addEventListener('pointerdown', tapped);
    layer.addEventListener('click', function (e) { if (onMini(e)) { e.preventDefault(); e.stopPropagation(); } });
    function tick() {
      if (dead || document.hidden) return;
      var btn = q('.pk-resume'), ok = btn && btn.offsetParent !== null && !btn.disabled;
      if (actor && layer.isConnected && layer.clientWidth) { var lb = layer.getBoundingClientRect(); lastAt = { x: lb.left + actor.x, y: lb.top + actor.baseY + actor.yOff }; }
      if (scene === 'parked' && !leaving && (!ok || !veil.isConnected)) { handOver(); return; }
      if (scene || !ok || HOLDER || Date.now() < wantAt) return;
      HOLDER = me; scene = 'coming';
      logoLeave().then(function () {
        if (dead) return;
        var b = q('.pk-resume');
        if (!b || b.offsetParent === null) { scene = null; gone(); return; }
        actor = new Actor(layer, 50);
        scene = 'parked'; leaving = false;
        actor.surf = topOf('.pk-resume');
        actor.play(function (s) { return sceneParked(s, width(), get); });
        wantAt = Date.now() + 15 * 60000;                                     // not again for a good while
      });
    }
    var timer = setInterval(tick, 500);
    return {
      destroy: function () {
        dead = true; clearInterval(timer); veil.removeEventListener('pointerdown', acted, { capture: true });
        // being destroyed while it stands there is what resuming looks like from in here
        if (actor && scene === 'parked' && !leaving) handOver(); else { gone(); if (HOLDER === me && !handing) HOLDER = null; }
        if (layer.parentNode) layer.parentNode.removeChild(layer);
      },
      trigger: function () { wantAt = 0; tick(); },
      poke: function () { if (actor) tapped({ target: actor.el, preventDefault: function () {}, stopPropagation: function () {} }); },
      scene: function () { return leaving ? 'leaving' : scene; },
      actor: function () { return actor; }
    };
  }

  // ─── Watching a chat, and deciding when Mini belongs in it ───
  // nudge counts from when Claude finished (or the last thing the person did),
  // suggestion or no suggestion
  var DELAY = { queue: 12000, sit: 25000, nudge: 90000 };

  function attach(root, o) {
    o = o || {};
    if (REDUCED && !o.force) return { destroy: function () {}, trigger: function () {} };
    // A sleeping chat is not a chat to wait in: it has its own, smaller part.
    var cell = root.closest ? root.closest('.parked-cell') : null, veil = cell && cell.querySelector('.pk-veil');
    if (cell) return veil ? attachParked(veil, o) : { destroy: function () {}, trigger: function () {} };
    addStyle();
    // The layer fills the chat. That needs the chat to be the box it is measured
    // against, which it is not unless it is positioned.
    try { if (getComputedStyle(root).position === 'static') root.style.position = 'relative'; } catch (e) { /* leave it */ }
    var layer = document.createElement('div');
    layer.className = 'cm-layer';
    root.appendChild(layer);
    var delay = { queue: DELAY.queue, sit: DELAY.sit, nudge: DELAY.nudge };
    if (o.delays) for (var dk in o.delays) delay[dk] = o.delays[dk];

    var actor = null, scene = null, leaving = false, dead = false;
    var since = { queue: 0, sit: 0, idle: 0 }, lastState = '';
    // The nudge needs only an open chat and nobody typing; it does not need
    // Claude to have just finished something. What stops it coming round and
    // round is a rest after each visit, ended early by a new finished turn.
    var NUDGE_REST = 10 * 60000, nudgeAfter = 0;
    function mayNudge(t) { return !!o.eager || t >= nudgeAfter; }
    var now = function () { return Date.now(); };

    function box(node) {
      if (!node || !node.isConnected) return null;
      var r = node.getBoundingClientRect(), b = layer.getBoundingClientRect();
      if (!r.width && !r.height) return null;
      return { l: r.left - b.left, r: r.right - b.left, t: r.top - b.top, b: r.bottom - b.top, w: b.width };
    }
    function q(sel) { var n = root.querySelector(sel); return n && n.offsetParent !== null ? n : null; }
    function width() { return layer.clientWidth || 300; }
    function mobile() { return root.classList.contains('cc-narrow') || width() < 520; }
    // Things in the chat it should know about: the "Working…" line, and the
    // badges for background commands and subagents in the corner. It keeps off
    // the badges (they are buttons) and goes and looks at both.
    function badgeBox() {
      var list = root.querySelectorAll('.cc-agbadge'), l = Infinity, r = -Infinity, t = 0, bt = 0, any = false;
      for (var i = 0; i < list.length; i++) {
        var b = list[i].offsetParent !== null ? box(list[i]) : null;
        if (!b) continue;
        any = true; if (b.l < l) { l = b.l; } if (b.r > r) r = b.r; t = b.t; bt = b.b;
      }
      return any ? { l: l, r: r, x: (l + r) / 2, y: (t + bt) / 2 } : null;
    }
    var env = {
      // the row is as wide as the pane; its words are not
      activity: function () {
        var row = q('.cc-activity'), b = box(row); if (!b) return null;
        var last = row.lastElementChild, t = last ? box(last) : null;
        return { x: b.l + 8, r: t ? t.r : b.l + 90, y: (b.t + b.b) / 2, t: b.t };
      },
      badge: badgeBox
    };
    function clearOfBadges(fn) {
      return function () {
        var sf = fn(); if (!sf) return sf;
        var b = badgeBox(), w = env.activity();
        if (b && b.l - 12 > sf.l + 40) sf.r = Math.min(sf.r, b.l - 12);
        // Not on top of the words "Working…" either, when they are down at its
        // level and there is room beside them.
        if (w && w.t > sf.y - 60 && sf.r - (w.r + 26) > 70) sf.l = Math.max(sf.l, w.r + 26);
        return sf;
      };
    }
    function topOf(sel, padL, padR) {
      return function () {
        var b = box(typeof sel === 'string' ? q(sel) : sel);
        return b ? { y: b.t, l: b.l + (padL || 0), r: b.r - (padR || 0) } : null;
      };
    }
    var send = o.sendBtn || null, input = o.input || null, composer = o.composer || null;
    function nudgeSurf() {
      var b;
      if (mobile()) { b = box(send || q('.cc-btn.primary')); return b ? { y: b.t, l: b.l + 14, r: b.r - 14 } : null; }
      b = box(composer || q('.cc-composer'));
      return b ? { y: b.t, l: b.l + 16, r: b.r - 16 } : null;
    }
    function composerTop() { var b = box(composer || q('.cc-composer')); return b ? { y: b.t, l: b.l + 10, r: b.r - 10 } : null; }
    function inputTop() { var b = box(q('.cc-inrow') || composer); return b ? { y: b.t, l: b.l + 10, r: b.r - 10 } : null; }
    function sugAt() { var b = box(q('.cc-sug')); return b ? { x: b.l + Math.min(40, (b.r - b.l) / 2), y: (b.t + b.b) / 2 } : null; }
    function sendAt() { var b = box(send || q('.cc-btn.primary')); return b ? { x: (b.l + b.r) / 2, y: (b.t + b.b) / 2 } : null; }
    function aim() { var b = box(input || q('.cc-input')); return b ? { x: b.l + Math.min(90, (b.r - b.l) * 0.3), y: (b.t + b.b) / 2 } : null; }

    function ensure() {
      if (!actor) actor = new Actor(layer, mobile() ? 50 : 54);
      return actor;
    }
    var me = {};
    // When it has left the chat it goes home to the top bar, at the pace it
    // left with. Nobody else gets it until it is back in its place.
    function gone(quiet) {
      var sp = actor ? actor.lastSpeed : 0;
      if (actor) { actor.destroy(); actor = null; }
      scene = null; leaving = false; coming = null;
      if (HOLDER !== me) return;
      if (quiet) { HOLDER = null; return; }
      logoReturn(sp).then(function () { if (HOLDER === me && !scene) HOLDER = null; });
    }
    var coming = null;
    function start(name) {
      if (HOLDER === TOPKEY) { if (TOP.cut) TOP.cut(); return; }   // it is busy in the top bar: call it away
      if (HOLDER && HOLDER !== me) return;      // it is busy in another chat
      HOLDER = me;
      // First it has to get here: out of the logo and off the top bar.
      if (!actor && coming !== name) {
        coming = name; scene = 'coming';
        logoLeave().then(function () {
          if (dead || coming !== name) return;
          coming = null; scene = null;
          if (!wanted(name)) { gone(); return; }  // what it was coming for has gone: turn round
          start2(name);
        });
        return;
      }
      start2(name);
    }
    // From the seat on the suggestion to the floor by the message box, without
    // leaving: stand, look down, step off, land, walk over, carry on.
    function climbDown() {
      var a = actor, w = width(), mob = mobile();
      scene = 'nudge';
      var held = { y: a.baseY, l: -100, r: w + 100 };
      a.surf = function () { return held; };
      a.tx = null; a.autoBlink = true;
      a.play(async function (s) {
        s.rest();
        s.pose({ sit: 0, swing: 0 }); await s.wait(650);
        s.pose({ headY: 5, eyeY: 4.5 }); await s.wait(600);                   // a long way down
        s.pose({ headY: 0, eyeY: 0, armL: 120, armR: 120 });
        await s.wait(220);
        var surf = clearOfBadges(nudgeSurf);
        await s.fallTo(function () { var f = surf(); return f ? { y: f.y, l: -100, r: w + 100 } : held; });
        s.pose({ armL: 0, armR: 0 }); await s.wait(360);
        a.surf = surf;
        var sf = surf();
        if (sf) await s.walkTo(mob ? sf.l + (sf.r - sf.l) * 0.5 : sf.r - 70, 60);
        await wave(s, 2);
        await nudgeLoop(s, aim, 0, mob ? null : sendAt, sugAt);
      });
    }
    function wanted(name) {
      if (o.eager) return true;
      if (name === 'queue') return !!q('.cc-queue');
      if (name === 'sit') return !!q('.cc-sug');
      return (o.state ? o.state() : 'idle') === 'idle' && !(input && input.value && input.value.trim());
    }
    var asked = null;
    function start2(name) {
      var a = ensure();
      asked = null;
      scene = name; leaving = false;
      a.autoBlink = true;
      if (name === 'queue') { a.surf = clearOfBadges(topOf('.cc-queue', 12, 12)); a.play(function (s) { return sceneQueue(s, width(), env); }); }
      else if (name === 'sit') { a.surf = topOf('.cc-sug', 0, 0); a.play(function (s) { return sceneSit(s, width()); }); }
      else { a.surf = clearOfBadges(nudgeSurf); a.play(function (s) { return sceneNudge(s, width(), aim, mobile(), sendAt, sugAt); }); }
    }
    function leave(how) {
      if (!actor || leaving) return;
      leaving = true;
      var a = actor, was = scene;
      // Hold the ground where it last was while the exit plays: the thing it
      // was standing on is no longer there to be measured.
      var held = { y: a.baseY, l: -100, r: width() + 100 };
      if (how === 'fall') {
        var ground = was === 'sit' ? composerTop : inputTop;
        a.surf = function () { return held; };
        a.play(function (s) { return sceneFall(s, width(), function () { return ground() || held; }); }).then(function () { if (actor === a && leaving) gone(); });
      } else {
        a.surf = (function (f) { return function () { return f() || held; }; })(a.surf || function () { return held; });
        a.play(function (s) { return sceneBye(s, width()); }).then(function () { if (actor === a && leaving) gone(); });
      }
    }

    // A tap on the robot is not the person getting on with things: it must not
    // send the nudge away, and it is not passed on to whatever is behind.
    function onMini(e) { return !!(actor && e.target && actor.el.contains(e.target)); }
    function userActed(e) { if (e && onMini(e)) return; since.idle = now(); if (scene === 'nudge') leave('bye'); }
    var taps = [], lastKind = -1;
    var wary = 0, lastFlee = 0;                    // goes up with every tap, wears off with time
    // Back to what it was doing. Anything but the sitting scene is done on its
    // feet, so if it was annoyed while sitting it stands up first; otherwise the
    // next act would be played from the seat, a little lower than it walks.
    function resume(s, was, asleep) {
      if (was !== 'sit' && !asleep && actor && actor.t.sit > 0.5) {
        s.pose({ sit: 0, swing: 0, armL: 0, armR: 0 });
        return s.wait(650).then(function () { return resume(s, was, false); });
      }
      if (was === 'queue') return queueLoop(s, 2 + Math.floor(Math.random() * 6), env);
      if (was === 'sit') return (actor.t.sit < 0.5 ? takeSeat(s) : Promise.resolve()).then(function () { s.pose({ sit: 1, swing: 1, armL: 10, armR: 10 }); return sitLoop(s, Math.floor(Math.random() * 6)); });
      return nudgeLoop(s, aim, asleep ? 99 : 1 + Math.floor(Math.random() * 7), mobile() ? null : sendAt, sugAt);
    }
    function moved(e) {
      if (!actor || !scene || e.pointerType === 'touch') return;
      var a = actor, b = layer.getBoundingClientRect();
      var mx = e.clientX - b.left, my = e.clientY - b.top;
      var cx = a.x, cy = a.baseY + a.yOff - 30 * a.k;
      var dx = mx - cx, dy = my - cy, d = Math.sqrt(dx * dx + dy * dy);
      // eyes follow a cursor that is near
      var near = clamp(1 - (d - 60) / 220, 0, 1), n = d || 1;
      a.lookTX = dx / n * 3.2 * near; a.lookTY = dy / n * 3 * near;
      mouseAt = { x: mx, y: my };
      if (leaving || a.air || eyeing) return;
      var t = now();
      if (wary > 0 && t - wary > 60000) wary = 0;
      if (!wary || d > 105 || t - lastFlee < 900 || a.t.lid > 0.8) return;
      var was = scene;
      eyeing = true; a.autoBlink = true;
      a.play(async function (s) {
        // Whatever ends the watching (the cursor leaving, a tap, the scene
        // ending), it is free to notice the cursor again afterwards.
        try { s.rest({ sit: 1 }); await eyeIt(s, mouse); } finally { eyeing = false; lastFlee = now(); }
        s.rest();
        await s.wait(400);
        await resume(s, was, false);
      });
    }
    var mouseAt = null, eyeing = false;
    function mouse() {
      if (!mouseAt || !actor) return null;
      var a = actor, dx = mouseAt.x - a.x, dy = mouseAt.y - (a.baseY + a.yOff - 30 * a.k);
      return { x: mouseAt.x, d: Math.sqrt(dx * dx + dy * dy) };
    }
    root.addEventListener('pointermove', moved, { passive: true });
    root.addEventListener('pointerleave', function () { mouseAt = null; if (actor) { actor.lookTX = 0; actor.lookTY = 0; } }, { passive: true });
    function tapped(e) {
      if (!onMini(e) || leaving || !scene) return;
      e.preventDefault(); e.stopPropagation();
      var a = actor, was = scene, t = now(), w = width();
      taps = taps.filter(function (x) { return t - x < 16000; }); taps.push(t);
      wary = t; lastFlee = t;
      if (taps.length >= 4) {                       // it has had enough of you
        taps = []; leaving = true;
        a.surf = (function (f, held) { return function () { return f() || held; }; })(a.surf, { y: a.baseY, l: -100, r: w + 100 });
        a.play(function (s) { return stormOff(s, w); }).then(function () { if (actor === a && leaving) { gone(); since.queue = since.sit = 0; since.idle = now(); nudgeAfter = now() + NUDGE_REST; } });
        return;
      }
      var asleep = a.t.lid > 0.8, seated = a.t.sit > 0.5;
      var list = seated ? ANNOYED_SEATED : ANNOYED, k = Math.floor(Math.random() * list.length);
      if (k === lastKind) k = (k + 1) % list.length;
      lastKind = k;
      a.tx = null; a.autoBlink = true;
      a.play(async function (s) {
        s.rest({ lid: 1 });
        if (asleep) await grumpyWake(s);
        else await list[k](s, w);
        // However it took it, it gets to its feet afterwards: nobody stays
        // sitting comfortably next to whoever just poked them.
        if (a.t.sit > 0.5) {
          s.pose({ sit: 0, swing: 0, lid: 0.4, armL: -34, armR: -34, turn: 0, headRot: 0 });
          await s.wait(700);
          await tapFoot(s);
        }
        s.rest();
        await s.wait(500);
        // then back to what it was doing, picking up somewhere in the middle
        // (on a suggestion that means sitting down again, in its own time)
        await resume(s, was, false);
      });
    }
    layer.addEventListener('pointerdown', tapped);
    layer.addEventListener('click', function (e) { if (onMini(e)) { e.preventDefault(); e.stopPropagation(); } });
    var evs = ['keydown', 'pointerdown', 'input', 'wheel', 'touchstart'];
    evs.forEach(function (e) { root.addEventListener(e, userActed, { passive: true, capture: true }); });

    function tick() {
      if (dead || document.hidden) return;
      var t = now();
      var st = o.state ? o.state() : 'idle';
      if (st !== lastState) {
        if (st === 'working') nudgeAfter = 0;
        if (st === 'idle') since.idle = t;
        lastState = st;
      }
      var hasQ = !!q('.cc-queue'), hasS = !!q('.cc-sug');
      var typed = input && input.value && input.value.trim();
      since.queue = hasQ ? (since.queue || t) : 0;
      since.sit = hasS ? (since.sit || t) : 0;

      if (scene === 'queue' && !hasQ) leave('fall');
      else if (scene === 'sit' && !hasS) {
        // Its seat was taken away, not the reason to be here: if nothing is
        // typed it is back before long to make the case for the message box.
        if (!leaving && st === 'idle') since.idle = Math.min(since.idle, t - delay.nudge + Math.min(delay.nudge, 30000));
        leave('fall');
      }
      // A suggestion being there does not send the nudge away: it points at it.
      else if (scene === 'nudge' && (st !== 'idle' || typed || hasQ)) leave('bye');
      // It has sat on the suggestion long enough and still nothing has been
      // typed: get down and go and stand by the message box instead.
      else if (scene === 'sit' && !leaving && actor && !actor.air && st === 'idle' && mayNudge(t) && !typed && since.idle && t - since.idle >= delay.nudge && nudgeSurf()) {
        nudgeAfter = t + NUDGE_REST;
        climbDown();
      }
      // More was queued while it stood there: it notices.
      var sig = queueSig();
      if (scene === 'queue' && !leaving && actor && lastSig && sig.n && (sig.n > lastSig.n || sig.len > lastSig.len + 2) && !actor.air) {
        var a = actor, cnt = sig.lines;
        a.autoBlink = true;
        a.play(async function (s) { s.rest(); await another(s, cnt); s.rest(); await s.wait(500); await queueLoop(s, 2 + Math.floor(Math.random() * 6), env); });
      }
      lastSig = sig;
      if (scene) return;
      if (HOLDER && HOLDER !== me && HOLDER !== TOPKEY) return;
      if (asked) { start(asked); return; }

      if (hasQ && t - since.queue >= delay.queue) start('queue');
      else if (hasS && st === 'idle' && t - since.sit >= delay.sit) start('sit');
      else if (!hasQ && st === 'idle' && mayNudge(t) && !typed && since.idle && t - since.idle >= delay.nudge && nudgeSurf()) {
        nudgeAfter = t + NUDGE_REST;  // then leave them be for a while
        start('nudge');
      }
    }
    var lastSig = null;
    function queueSig() {
      var list = root.querySelectorAll('.cc-queue'), n = 0, len = 0, lines = 0;
      for (var i = 0; i < list.length; i++) {
        if (list[i].offsetParent === null) continue;
        n++;
        var b = list[i].querySelector('.cc-queue-body'), txt = b ? b.textContent : '';
        len += txt.length; lines += txt ? txt.split('\n').length : 0;
      }
      return { n: n, len: len, lines: lines };
    }
    var timer = setInterval(tick, 400);
    since.idle = now();

    return {
      destroy: function () {
        dead = true; clearInterval(timer);
        evs.forEach(function (e) { root.removeEventListener(e, userActed, { capture: true }); });
        root.removeEventListener('pointermove', moved);
        gone();
        if (HOLDER === me) HOLDER = null;
        if (layer.parentNode) layer.parentNode.removeChild(layer);
      },
      // For the demo page and for tests: start a scene now, or poke it.
      // If it is busy in the top bar it has to be called away first, so the
      // request is kept and tried again until it has arrived.
      trigger: function (name) { if (scene) gone(true); lastSig = queueSig(); asked = name; start(name); },
      scare: function (x) { wary = now(); lastFlee = 0; if (actor) moved({ clientX: layer.getBoundingClientRect().left + (x === undefined ? actor.x + 20 : x), clientY: layer.getBoundingClientRect().top + actor.baseY - 20, pointerType: 'mouse' }); },
      poke: function () { if (actor) tapped({ target: actor.el, preventDefault: function () {}, stopPropagation: function () {} }); },
      scene: function () { return leaving ? 'leaving' : scene; },
      actor: function () { return actor; }
    };
  }

  window.CrundiMini = { attach: attach, Actor: Actor, delays: DELAY, _acts: { pace: pace },
    // the top bar: starts by itself; `go` is for the demo page and for tests
    top: { start: topStart, go: function (kind, force) { TOP.force = force || null; TOP.anyWidth = true; if (kind === 'party') TOP.partyFor = force || 'five'; var ok = topOuting(kind, topRead()); if (ok) { TOP.seen[kind] = true; TOP.next[kind] = Date.now() + 10 * 60000; } return ok; }, busy: function () { return TOP.busy; } } };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', topStart); else topStart();
})();
