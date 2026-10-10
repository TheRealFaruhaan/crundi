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
 *           typing, waves, and eventually falls asleep.
 *
 * It is drawn in code and moved in code. Nothing here is a keyframe animation
 * played from the start: every part of the body (each arm, each leg, the head,
 * the eyes, the lean of the body) is a number on a spring, and a behaviour only
 * ever says where those numbers should go next. So whatever Mini is doing when
 * something changes, it carries on from exactly that pose. It cannot jump from
 * one state to another because there are no states to jump between.
 *
 * Its eyes follow the mouse when the mouse is near. Tap it and it is annoyed,
 * a different way each time: it stamps, shoos you off, turns its back, steps
 * out of reach, or leaves the pane and peers back in. Once it has been
 * bothered it is wary of the cursor too, and backs away when the cursor comes
 * close, or cowers if it has nowhere to go. Keep tapping and it storms off.
 *
 * There is only ever one Mini on the page, however many chats are open.
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
      +   '<rect data-p="legL" x="36" y="77" width="9" height="13" rx="4.2" fill="' + A + '"/>'
      +   '<rect data-p="legR" x="51" y="77" width="9" height="13" rx="4.2" fill="' + A + '"/>'
      +   '<g data-p="up">'
      +     '<rect x="32" y="54" width="32" height="24" rx="9" fill="url(#' + g + ')"/>'
      +     '<circle data-p="light" cx="48" cy="66" r="4.8" fill="' + D + '" opacity=".6"/>'
      +     '<g data-p="back" opacity="0"><path d="M41 62H55M41 67H55M41 72H55" stroke="' + D + '" stroke-opacity=".4" stroke-width="2.2" stroke-linecap="round"/></g>'
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
    plane: { w: 18, h: 14, d: 1100, a: 'cm-plane', s: '<path d="M1 6L17 1L11 13L8 8Z" fill="#e8e8f0"/><path d="M8 8L17 1" stroke="#8888a8" stroke-width="1"/>' }
  };

  function clamp(v, a, b) { return v < a ? a : v > b ? b : v; }
  function rnd(a, b) { return a + Math.random() * (b - a); }
  function pick(list) { return list[Math.floor(Math.random() * list.length)]; }

  // ─── The actor: a body on springs ───
  // [stiffness, damping]. Lower damping is bouncier.
  var SPRING = {
    turn: [130, 21], lean: [150, 22], squash: [330, 13], headRot: [160, 19], headY: [170, 20],
    eyeX: [230, 26], eyeY: [230, 26], lid: [420, 34], armL: [165, 16], armR: [165, 16],
    legL: [260, 24], legR: [260, 24], sit: [95, 17], swing: [50, 13], shake: [70, 15], alpha: [60, 15]
  };
  var REST = { turn: 0, lean: 0, squash: 1, headRot: 0, headY: 0, eyeX: 0, eyeY: 0, lid: 0, armL: 0, armR: 0, legL: 0, legR: 0, sit: 0, swing: 0, shake: 0, alpha: 1 };

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
    var sp = Math.abs(this.vx);
    this.walk += (clamp(sp / 34, 0, 1) - this.walk) * Math.min(1, dt * 10);
    this.phase += sp * dt * 0.21 / this.k * 0.59;

    // falling and hopping
    if (this.air) {
      this.vy += G * dt;
      this.yOff += this.vy * dt;
      if (this.yOff >= 0 && this.vy > 0) {
        var hit = this.vy;
        this.yOff = 0; this.vy = 0; this.air = false;
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

    var legL = v.legL + gait * (30 + run * 16) * w + Math.sin(tm * 5.2) * 24 * sw;
    var legR = v.legR - gait * (30 + run * 16) * w + Math.sin(tm * 5.2 + 2.5) * 24 * sw;
    var armL = v.armL - gait * 24 * w * (1 - clamp(v.shake, 0, 1)) + shake;
    var armR = v.armR + gait * 24 * w * (1 - clamp(v.shake, 0, 1)) - shake;
    var bob = -Math.abs(gait) * (2.4 + run * 2) * w + (this.air ? 0 : Math.sin(tm * 2.1) * 0.5);
    var lean = v.lean + clamp(this.vx / 22, -11, 11);
    var sq = clamp(v.squash, 0.55, 1.4), sx = 1 / Math.sqrt(sq);
    var drop = sit * SIT_DROP;

    this.el.style.transform = 'translate3d(' + (this.x - 48 * k).toFixed(2) + 'px,' + (this.baseY + this.yOff - GROUND * k).toFixed(2) + 'px,0)';
    this.el.style.opacity = clamp(v.alpha, 0, 1).toFixed(3);

    p.root.setAttribute('transform', 'translate(0 ' + drop.toFixed(2) + ') rotate(' + lean.toFixed(2) + ' 48 ' + GROUND + ') translate(48 ' + GROUND + ') scale(' + sx.toFixed(3) + ' ' + sq.toFixed(3) + ') translate(-48 -' + GROUND + ')');
    p.legL.setAttribute('transform', 'rotate(' + legL.toFixed(2) + ' 40.5 78)');
    p.legR.setAttribute('transform', 'rotate(' + legR.toFixed(2) + ' 55.5 78)');
    p.up.setAttribute('transform', 'translate(0 ' + bob.toFixed(2) + ')');
    p.armL.setAttribute('transform', 'rotate(' + armL.toFixed(2) + ' 23.5 58)');
    p.armR.setAttribute('transform', 'rotate(' + (-armR).toFixed(2) + ' 72.5 58)');

    var turn = v.turn, tc = clamp(turn, -1, 1);
    p.head.setAttribute('transform', 'translate(' + (tc * 2.6).toFixed(2) + ' ' + v.headY.toFixed(2) + ') rotate(' + v.headRot.toFixed(2) + ' 48 50)');
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
    p.shadow.setAttribute('opacity', (0.32 * (1 - sit) * (1 - lift * 0.7) * clamp(v.alpha, 0, 1)).toFixed(3));
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
      rest: function (keep) { live(); for (var key in REST) if (key !== 'alpha' && key !== 'sit' && !(keep && keep[key])) a.t[key] = REST[key]; },
      walkTo: function (x, speed) {
        live();
        a.t.sit = 0; a.t.swing = 0;                // nothing walks sitting down
        a.speed = speed || 55; a.tx = x;
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
    s.pose({ armL: -38, armR: -38, headRot: 8, eyeY: -2.5, eyeX: 2 });      // arms folded, eyes to the ceiling
    for (var i = 0; i < 7; i++) { s.pose({ legL: -24 }); await s.wait(140); s.pose({ legL: 0 }); await s.wait(160); }
    s.pose({ armL: 0, armR: 0, headRot: 0, eyeY: 0, eyeX: 0 }); await s.wait(360);
  }
  async function pace(s, turns, speed) {
    var sf = s.surf(); if (!sf) return;
    var l = sf.l + 14, r = sf.r - 14;
    if (r - l < 30) return;
    for (var i = 0; i < turns; i++) {
      s.pose({ armL: -16, armR: -16, headY: 1.5 });                          // hands behind its back
      await s.walkTo(i % 2 ? l + rnd(0, 12) : r - rnd(0, 12), speed);
      s.pose({ headY: 0 });
      await s.wait(rnd(180, 420));
    }
    s.pose({ armL: 0, armR: 0 });
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
    s.pose({ eyeY: -3, headRot: -6 }); await s.wait(520);                     // look up at the conversation
    s.pose({ eyeY: 0, headRot: 0 });
    await knock(s);
    await shrug(s);
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
      function () { return pace(s, 4, 74); },
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
    var order = [9, 0, 10, 1, 2, 3, 5, 8, 6, 4, 7], n = from, was = -1;
    for (;;) {
      var i = n < order.length ? order[n] : Math.floor(Math.random() * acts.length);
      // only what is actually on screen
      if (i === 9 && !(env && env.activity())) i = 1;
      if (i === 10 && !(env && env.badge())) i = 0;
      if (i === was) i = (i + 1) % 9;
      was = i; n++;
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
    for (var n = from; ; n++) {
      var r = n % 6;
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
  async function sceneNudge(s, layerW, aim, mobile, sendAt) {
    var sf = s.surf(); if (!sf) return;
    var spot = mobile ? sf.l + (sf.r - sf.l) * 0.5 : sf.r - 70;
    await enter(s, layerW + 40, spot, 66);
    await s.wait(300);
    await wave(s, 3);
    await nudgeLoop(s, aim, 0, mobile ? null : sendAt);
  }
  async function nudgeLoop(s, aim, from, sendAt) {
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
    var acts = sendAt
      ? [showAndTell, typing, ask, jacks, showAndTell, sitAwhile, function () { return watch(s); }, point, showAndTell, function () { return sigh(s); }]
      : [point, typing, ask, jacks, point, function () { return pace(s, 2, 58); }, sitAwhile, function () { return watch(s); }, point, function () { return sigh(s); }];
    for (var n = from; n < acts.length; n++) {
      await acts[n]();
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

  // There is one Mini. With several chats on screen, whichever needs it first
  // has it, and the others wait until it has left.
  var HOLDER = null;

  // ─── Watching a chat, and deciding when Mini belongs in it ───
  var DELAY = { queue: 12000, sit: 25000, nudge: 90000 };

  function attach(root, o) {
    o = o || {};
    if (REDUCED && !o.force) return { destroy: function () {}, trigger: function () {} };
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
    var since = { queue: 0, sit: 0, idle: 0 }, sawWork = !!o.eager, lastState = '';
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
    function sendAt() { var b = box(send || q('.cc-btn.primary')); return b ? { x: (b.l + b.r) / 2, y: (b.t + b.b) / 2 } : null; }
    function aim() { var b = box(input || q('.cc-input')); return b ? { x: b.l + Math.min(90, (b.r - b.l) * 0.3), y: (b.t + b.b) / 2 } : null; }

    function ensure() {
      if (!actor) actor = new Actor(layer, mobile() ? 50 : 54);
      return actor;
    }
    var me = {};
    function gone() { if (actor) { actor.destroy(); actor = null; } scene = null; leaving = false; if (HOLDER === me) HOLDER = null; }
    function start(name) {
      if (HOLDER && HOLDER !== me) return;      // it is busy in another chat
      HOLDER = me;
      var a = ensure();
      scene = name; leaving = false;
      a.autoBlink = true;
      if (name === 'queue') { a.surf = clearOfBadges(topOf('.cc-queue', 12, 12)); a.play(function (s) { return sceneQueue(s, width(), env); }); }
      else if (name === 'sit') { a.surf = topOf('.cc-sug', 0, 0); a.play(function (s) { return sceneSit(s, width()); }); }
      else { a.surf = clearOfBadges(nudgeSurf); a.play(function (s) { return sceneNudge(s, width(), aim, mobile(), sendAt); }); }
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
      return nudgeLoop(s, aim, asleep ? 99 : 1 + Math.floor(Math.random() * 7), mobile() ? null : sendAt);
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
        a.play(function (s) { return stormOff(s, w); }).then(function () { if (actor === a && leaving) { gone(); since.queue = since.sit = 0; since.idle = now(); sawWork = false; } });
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
        if (st === 'working') sawWork = true;
        if (st === 'idle') since.idle = t;
        lastState = st;
      }
      var hasQ = !!q('.cc-queue'), hasS = !!q('.cc-sug');
      var typed = input && input.value && input.value.trim();
      since.queue = hasQ ? (since.queue || t) : 0;
      since.sit = hasS ? (since.sit || t) : 0;

      if (scene === 'queue' && !hasQ) leave('fall');
      else if (scene === 'sit' && !hasS) leave('fall');
      else if (scene === 'nudge' && (st !== 'idle' || typed || hasQ || hasS)) leave('bye');
      // More was queued while it stood there: it notices.
      var sig = queueSig();
      if (scene === 'queue' && !leaving && actor && lastSig && sig.n && (sig.n > lastSig.n || sig.len > lastSig.len + 2) && !actor.air) {
        var a = actor, cnt = sig.lines;
        a.autoBlink = true;
        a.play(async function (s) { s.rest(); await another(s, cnt); s.rest(); await s.wait(500); await queueLoop(s, 2 + Math.floor(Math.random() * 6), env); });
      }
      lastSig = sig;
      if (scene) return;
      if (HOLDER && HOLDER !== me) return;

      if (hasQ && t - since.queue >= delay.queue) start('queue');
      else if (hasS && st === 'idle' && t - since.sit >= delay.sit) start('sit');
      else if (!hasQ && !hasS && st === 'idle' && sawWork && !typed && since.idle && t - since.idle >= delay.nudge && nudgeSurf()) {
        sawWork = !!o.eager;          // once per finished turn, not on a loop
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
        if (layer.parentNode) layer.parentNode.removeChild(layer);
      },
      // For the demo page and for tests: start a scene now, or poke it.
      trigger: function (name) { if (scene) gone(); lastSig = queueSig(); start(name); },
      scare: function (x) { wary = now(); lastFlee = 0; if (actor) moved({ clientX: layer.getBoundingClientRect().left + (x === undefined ? actor.x + 20 : x), clientY: layer.getBoundingClientRect().top + actor.baseY - 20, pointerType: 'mouse' }); },
      poke: function () { if (actor) tapped({ target: actor.el, preventDefault: function () {}, stopPropagation: function () {} }); },
      scene: function () { return leaving ? 'leaving' : scene; },
      actor: function () { return actor; }
    };
  }

  window.CrundiMini = { attach: attach, Actor: Actor, delays: DELAY };
})();
