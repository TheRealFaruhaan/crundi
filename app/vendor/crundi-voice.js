/**
 * crundi-voice.js: the voice assistant in the web workbench, and so in the PC
 * client, which loads the same page (D39: parity with the phone).
 *
 *   window.CrundiVoice.open(opts)   the voice screen over the page
 *   window.CrundiVoice.close()
 *
 * opts: {
 *   apiFetch(path, init) -> Response   the page's signed-in fetch
 *   token() -> string                   for the live stream's WebSocket
 *   cells() -> [{ name, kind, service }]  projects as cells: kind working |
 *                                       waiting | needs | error | running | idle;
 *                                       service true (running) / false / null
 *   usage() -> { week, five }           0..1, or null
 *   host                                 shown at the top
 * }
 *
 * Same design and rules as the phone (android/…/voice/): the Crundi head at
 * the centre of a contour visualizer that follows the live voice, the usage
 * arcs inside its disc, the projects as cells joined by strands, the liquid
 * mic. The microphone streams as 16 kHz PCM to /api/voice/listen (Deepgram
 * through the server); each utterance goes to /api/voice/turn; the answer is
 * spoken in the chosen Deepgram voice (streamed) or the browser's en-GB one.
 * Crundi's own voice never goes to the server. Its speech and cues play
 * through a WebRTC loopback inside the page, so Chrome's echo canceller
 * has them as the far end and takes them out of the microphone; and while
 * it speaks (or a cue plays) nothing at all is sent: the mic runs into a
 * local gate (createGate: louder than Crundi's echo, for a moment, and
 * speech by a real speech detector, createVad: Silero VAD run here, so noise
 * never cuts Crundi off) that decides here whether the owner is talking. When it is him,
 * Crundi stops at once and the stream resumes from a pre-roll of the last
 * second and a bit, so his first words are kept. On a server that knows
 * barge-in ('ready' echoFilter) the server's transcript filter ('saying' /
 * 'said') is the second net; an older server is paused as before and a loud
 * voice (speech, not noise) alone cuts in. The moment something is asked
 * a short cue ("One moment, Sir.") is played from clips loaded when the
 * screen opened, or said with the browser's voice. Yes/no to Crundi's
 * questions by voice; on a PC you may also type.
 */
(function () {
  'use strict';
  if (window.CrundiVoice) return;

  var C = { sky: '#38bdf8', accent: '#6366f1', hover: '#818cf8', deep: '#4f46e5', ask: '#c084fc', wait: '#d946ef', green: '#10b981', yellow: '#f59e0b', red: '#ef4444', muted: '#5a5a78', text: '#e8e8f0', text2: '#8888a8', border: '#2a2a3d', bg: '#0a0a0f', card: '#12121a' };
  var KIND = { working: C.yellow, waiting: C.sky, needs: C.hover, error: C.red, running: C.green, idle: C.muted };
  var PALETTE = { listen: [C.sky, C.accent], think: [C.ask, C.deep], speak: [C.hover, C.ask], start: [C.accent, C.deep], ended: [C.muted, '#22223a'] };
  var LOGO = 'data:image/svg+xml;utf8,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="14" y1="14" x2="82" y2="82"><stop offset="0" stop-color="#818cf8"/><stop offset="1" stop-color="#4f46e5"/></linearGradient></defs><mask id="eh"><rect width="96" height="96" fill="#fff"/><rect x="32" y="43" width="10.5" height="18" rx="5.2" fill="#000"/><rect x="53.5" y="43" width="10.5" height="18" rx="5.2" fill="#000"/></mask><path d="M48 27V17" fill="none" stroke-linecap="round" stroke="#fbbf24" stroke-width="5.5"/><circle cx="48" cy="13" r="6.5" fill="#fbbf24"/><rect x="11" y="26" width="74" height="57" rx="23" fill="url(#g)" mask="url(#eh)"/></svg>');
  // Kid mode's big, bright cursor (dark outline: seen on any background), and a star over what can be tapped. The games' kit uses the same.
  var KID_CURSOR = 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 48 48"><path d="M5 3l34 18-15 4-6 15z" fill="#facc15" stroke="#111" stroke-width="3" stroke-linejoin="round"/></svg>');
  var KID_POINTER = 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="56" height="56" viewBox="0 0 56 56"><path d="M28 3l7 16 17 2-13 12 4 17-15-9-15 9 4-17L4 21l17-2z" fill="#f472b6" stroke="#111" stroke-width="3" stroke-linejoin="round"/><circle cx="28" cy="28" r="4" fill="#fff"/></svg>');
  var KID_PALETTE = { listen: ['#facc15', '#f472b6'], think: ['#a3e635', '#22d3ee'], speak: ['#f472b6', '#facc15'], start: ['#22d3ee', '#a78bfa'], ended: [C.muted, '#22223a'] };
  var PAUSE_AFTER_SPEECH_MS = 450, SILENCE_MS = 30000, ECHO_MS = 5000;
  /** A cue this close to its end is let finish before the reply; any further off, the reply cuts it. */
  var CUE_FINISH_MS = 300;
  /** After the level detector hears a voice over Crundi, one new word is enough to cut in, for this long. */
  var HINT_MS = 1500;
  /** What is kept of the microphone while Crundi speaks, to send when the owner cuts in. */
  var PREROLL_MS = 1300;
  /** Sent from this long before the owner's voice rose over Cru's (a soft first syllable). */
  var LEAD_MS = 600;
  /** The owner's voice is looked at over this much of the latest microphone (16 kHz samples). */
  var VOICE_WINDOW = 6400;

  function hex(c, a) { var n = parseInt(c.slice(1), 16); return 'rgba(' + (n >> 16) + ',' + (n >> 8 & 255) + ',' + (n & 255) + ',' + a + ')'; }
  function el(tag, cls, html) { var e = document.createElement(tag); if (cls) e.className = cls; if (html != null) e.innerHTML = html; return e; }
  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"]/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]; }); }
  function words(t) { return String(t || '').toLowerCase().split(/[^\p{L}\p{N}']+/u).filter(Boolean); }
  /** Heard back: mostly Crundi's own words (the same rule as the phone's Speech.isEcho). */
  function isEcho(heard, said) {
    var h = words(heard), s = words(said);
    if (h.length < 2) return h.length === 1 && s.indexOf(h[0]) >= 0 && s.length <= 3;
    var set = {}; s.forEach(function (w) { set[w] = 1; });
    return h.filter(function (w) { return set[w]; }).length / h.length >= 0.6;
  }
  var FILLER = { um: 1, uh: 1, erm: 1, er: 1, hmm: 1, mm: 1, mhm: 1, ah: 1, oh: 1 };
  var STOP_WORDS = { stop: 1, wait: 1, cancel: 1, enough: 1, quiet: 1, pause: 1, hold: 1, no: 1, cru: 1, crew: 1, sorry: 1, actually: 1, hang: 1 };
  /**
   * [heard] while Crundi says [said] is the owner talking over it (the phone's
   * Speech.talksOver): not an echo, at least [min] (default 2) words that
   * aren't "um", one of them not Crundi's own; or one word like "stop" or "wait".
   */
  function talksOver(heard, said, min) {
    var h = words(heard).filter(function (w) { return !FILLER[w]; });
    if (!h.length || (said && isEcho(heard, said))) return false;
    var s = {}; words(said).forEach(function (w) { s[w] = 1; });
    var fresh = h.filter(function (w) { return !s[w]; });
    return (h.length >= (min || 2) && fresh.length > 0) || fresh.some(function (w) { return STOP_WORDS[w]; });
  }
  /** Only, or ending with, "scratch that" / "never mind" / "ignore that" (not "cancel": that answers Cru's questions). The phone's Speech.isDiscard. */
  var DISCARD = [['scratch', 'that'], ['never', 'mind'], ['nevermind'], ['ignore', 'that']];
  function isDiscard(t) {
    var w = words(t);
    return DISCARD.some(function (p) { return w.length >= p.length && p.every(function (x, i) { return w[w.length - p.length + i] === x; }); });
  }
  /** What a cancelled turn says: [acted] what it had already done ([{ what }] or strings). */
  function cancelledNotice(acted) {
    var a = (acted || []).map(function (x) { return typeof x === 'string' ? x : (x && x.what) || ''; }).filter(Boolean);
    return a.length ? 'Cancelled. Already done: ' + a.join('; ') + '.' : 'Cancelled.';
  }
  /** The cues when the server has none to offer (an older server): said with the browser's voice only. */
  var CUE_TEXTS = ['One moment, Sir.', 'Let me check.', 'On it, Sir.', 'Just a moment.', 'Looking now, Sir.', 'Right away, Sir.', 'Bear with me, Sir.', 'Let me see.'];
  /** One of [n] at random, never [last] (when there is a choice). */
  function pickCue(n, last, rnd) {
    if (n <= 0) return -1;
    if (n === 1) return 0;
    var i = Math.floor((rnd || Math.random)() * (n - 1));
    return last >= 0 && i >= last ? i + 1 : i;
  }

  /**
   * The share of a clip's loud 30 ms frames that are voiced: a clear pitch
   * between 70 and 400 Hz at 16 kHz (app/voice-wake/enroll.js voicedShare,
   * the phone's CruWake.voicedShare). Speech has it; a cough or a clap hardly.
   */
  function voicedShare(clip) {
    var N = 480, H = 240, peak = 0, i, k;
    for (i = 0; i + N <= clip.length; i += H) { var e0 = 0; for (k = 0; k < N; k++) e0 += clip[i + k] * clip[i + k]; peak = Math.max(peak, e0); }
    if (!peak) return 0;
    var loud = 0, voiced = 0;
    for (i = 0; i + N <= clip.length; i += H) {
      var e = 0; for (k = 0; k < N; k++) e += clip[i + k] * clip[i + k];
      if (e < peak * 0.05) continue;
      loud++;
      var best = 0;
      for (var lag = 40; lag <= 228; lag++) {
        var c = 0, e1 = 0, e2 = 0;
        for (k = 0; k + lag < N; k++) { var a = clip[i + k], b = clip[i + k + lag]; c += a * b; e1 += a * a; e2 += b * b; }
        var r = c / Math.sqrt((e1 * e2) || 1);
        if (r > best) best = r;
      }
      if (best > 0.45) voiced++;
    }
    return loud ? voiced / loud : 0;
  }

  /**
   * Is it the owner talking over Crundi? Decided here, from the microphone
   * alone (the phone's OwnerGate has the same rule). Levels are 0..1 on a
   * square-root scale (the mic's and the playback's alike).
   *
   * With Crundi's playback level known (its streamed voice, a cue clip),
   * how much of it comes back through the mic is learnt as it plays (falls
   * fast, rises slowly: the owner's voice hardly moves it), and only a voice
   * [overEcho] times louder than that echo counts. With it unknown (the
   * browser's own voice) the bar is the loudest of the last two seconds,
   * and high until there are some. Either way: held for [holdMs], and speech
   * (the speech detector, createVad: a door, a cough, music or a fan is not).
   */
  function createGate(o) {
    o = o || {};
    var holdMs = o.holdMs || 300, floor = 0.03, overNoise = 3.2, overEcho = 2, unknownOver = 1.6, unknownStart = 0.45;
    var est = 0.6, noise = 0.01, loud = 0, recent = [0, 0, 0, 0], hist = [], run = [];
    function pct90(a) { if (!a.length) return 0; var b = a.slice().sort(function (x, y) { return x - y; }); return b[Math.min(b.length - 1, Math.floor(b.length * 0.9))]; }
    return {
      /** Listening (nothing playing): how loud the room is. */
      learn: function (level) { noise = Math.min(0.2, Math.max(0.002, noise * 0.95 + level * 0.05)); },
      /** Crundi starts to play: count afresh (the echo learnt is kept). */
      start: function () { loud = 0; recent = [0, 0, 0, 0]; hist = []; run = []; },
      /** One stretch of microphone: [play] Crundi's level now (null: unknown); speech() true when it is speech (heardSpeech). True: it is him. */
      hear: function (level, play, ms, speech) {
        var gate;
        if (play != null) {
          recent.shift(); recent.push(play);
          var P = Math.max.apply(null, recent);
          // Learnt only while Crundi plays steadily (the mic lags the playback a little).
          if (P > 0.05 && Math.min.apply(null, recent) >= P * 0.5) { var r = level / P; est = Math.min(3, Math.max(0.02, r < est ? est + (r - est) * 0.3 : est + (r - est) * 0.01)); }
          gate = Math.max(floor, noise * overNoise, est * Math.max(P, 0.05) * overEcho + 0.03);
        } else {
          gate = Math.max(floor, noise * overNoise, pct90(hist) * unknownOver);
          if (hist.length < 6) gate = Math.max(gate, unknownStart);
        }
        if (level >= gate) { loud += ms; run.push(level); }
        else {
          // A short burst (a loud syllable of Crundi's) joins what the echo sounds like.
          if (play == null) { hist = hist.concat(run, [level]).slice(-24); }
          loud = 0; run = [];
        }
        if (loud < holdMs) return false;
        return !speech || !!speech();
      },
      /** How long it has been loud: his words began about then. */
      get loudMs() { return loud; },
      get echo() { return est; },
      get noise() { return noise; },
    };
  }

  // ─── The speech detector: Silero VAD v5 (MIT), run here ───
  /** Speech starts at this probability and lasts until it falls below the second (Silero's own defaults). */
  var VAD_ON = 0.5, VAD_OFF = 0.35;
  /** Barge-in needs this much unbroken speech: a door, a cough, music or a fan never gets there. */
  var SPEECH_MS = 250;
  /** Frames (32 ms) it runs before it is trusted: a sound out of nothing reads as speech for a moment. */
  var VAD_WARM = 30;
  /**
   * Speech rises and falls syllable by syllable; a hum or a held chord that
   * the detector takes for speech as it starts does not. Within the run of
   * speech (its first VAD_SKIP frames aside: the sound's own onset) the
   * loudest and quietest frames must be this far apart.
   */
  var SPEECH_SWING_DB = 10, VAD_SKIP = 3;
  /** One frame: 512 new samples at 16 kHz (32 ms), after 64 of the last frame's. */
  var VAD_FRAME = 512, VAD_CTX = 64;
  /** How many weights the file holds. */
  var VAD_WEIGHTS = 243073;

  /**
   * The weights file (app/vendor/silero-vad-16k.bin, made by
   * scripts/make-vad-weights.py): "SVAD", version 1, float16 weights.
   * The phone's SpeechDetector.kt reads the same file. Null when it is not that.
   */
  function parseVad(buf) {
    var dv = new DataView(buf);
    if (buf.byteLength !== 8 + 2 * VAD_WEIGHTS || dv.getUint32(0, false) !== 0x53564144 || dv.getUint32(4, true) !== 1) return null;
    var at = 8;
    function take(n) {
      var out = new Float32Array(n);
      for (var i = 0; i < n; i++, at += 2) {
        var h = dv.getUint16(at, true), s = h & 0x8000 ? -1 : 1, e = (h >> 10) & 31, f = h & 1023;
        out[i] = e === 0 ? s * f * 5.960464477539063e-8 : e === 31 ? (f ? NaN : s * Infinity) : s * Math.pow(2, e - 15) * (1 + f / 1024);
      }
      return out;
    }
    var shapes = [[128, 129], [64, 128], [64, 64], [128, 64]], enc = [];
    for (var k = 0; k < 4; k++) enc.push({ o: shapes[k][0], c: shapes[k][1], s: k === 1 || k === 2 ? 2 : 1, w: take(shapes[k][0] * shapes[k][1] * 3), b: take(shapes[k][0]) });
    var W = { enc: enc, wih: take(512 * 128), whh: take(512 * 128), bl: take(512), wo: take(128), bo: take(1)[0] };
    return W;
  }

  var VAD_COS = null, VAD_SIN = null;
  /** The STFT of the model: a periodic Hann window, 256 points, bins 0..128. */
  function vadTables() {
    if (VAD_COS) return;
    VAD_COS = new Float32Array(129 * 256); VAD_SIN = new Float32Array(129 * 256);
    for (var k = 0; k < 129; k++) for (var n = 0; n < 256; n++) {
      var w = 0.5 - 0.5 * Math.cos(2 * Math.PI * n / 256), a = 2 * Math.PI * k * n / 256;
      VAD_COS[k * 256 + n] = Math.cos(a) * w; VAD_SIN[k * 256 + n] = Math.sin(a) * w;
    }
  }
  function sigm(x) { return 1 / (1 + Math.exp(-x)); }
  /** 1-D convolution, kernel 3, padding 1: [x] is c*T (channel-major); returns o*To. */
  function vadConv(L, x, T) {
    var To = Math.floor((T - 1) / L.s) + 1, out = new Float32Array(L.o * To);
    for (var o = 0; o < L.o; o++) for (var t = 0; t < To; t++) {
      var sum = L.b[o], t0 = t * L.s - 1;
      for (var c = 0; c < L.c; c++) {
        var wi = (o * L.c + c) * 3, xi = c * T;
        for (var k = 0; k < 3; k++) { var tt = t0 + k; if (tt >= 0 && tt < T) sum += L.w[wi + k] * x[xi + tt]; }
      }
      out[o * To + t] = sum > 0 ? sum : 0;
    }
    return { x: out, T: To };
  }

  /**
   * A streaming speech detector over 16 kHz audio. push() it the microphone
   * as it comes; [prob] is the latest frame's speech probability, [speech]
   * whether it is speech now (with hysteresis), [speechMs] how long it has
   * been speech without a break, [talking] true once that is someone
   * talking (SPEECH_MS of it, with speech's rise and fall). It hears any
   * voice: Cru's own echo is kept out by the gate's level rule, not here.
   */
  function createVad(W) {
    vadTables();
    var buf = new Float32Array(VAD_CTX + VAD_FRAME), fill = VAD_CTX, h = new Float32Array(128), c = new Float32Array(128);
    var xp = new Float32Array(640), mag = new Float32Array(129 * 4), g = new Float32Array(512);
    var me = { prob: 0, speech: false, speechMs: 0, frames: 0, talking: false }, db = new Float32Array(64);
    function frame() {
      var en = 0;
      for (var a = VAD_CTX; a < buf.length; a++) en += buf[a] * buf[a];
      db[me.frames % 64] = 10 * Math.log10(en / VAD_FRAME + 1e-10);
      for (var i = 0; i < 576; i++) xp[i] = buf[i];
      for (var j = 0; j < 64; j++) xp[576 + j] = buf[574 - j];   // reflect
      for (var t = 0; t < 4; t++) for (var k = 0; k < 129; k++) {
        var re = 0, im = 0, base = k * 256, off = t * 128;
        for (var n = 0; n < 256; n++) { var v = xp[off + n]; re += VAD_COS[base + n] * v; im += VAD_SIN[base + n] * v; }
        mag[k * 4 + t] = Math.sqrt(re * re + im * im);
      }
      var e = { x: mag, T: 4 };
      for (var l = 0; l < 4; l++) e = vadConv(W.enc[l], e.x, e.T);
      var x = e.x;   // 128 x 1
      for (var r = 0; r < 512; r++) {
        var s = W.bl[r], wi = r * 128;
        for (var q = 0; q < 128; q++) s += W.wih[wi + q] * x[q] + W.whh[wi + q] * h[q];
        g[r] = s;
      }
      var out = W.bo;
      for (var u = 0; u < 128; u++) {
        c[u] = sigm(g[128 + u]) * c[u] + sigm(g[u]) * Math.tanh(g[256 + u]);
        h[u] = sigm(g[384 + u]) * Math.tanh(c[u]);
        if (h[u] > 0) out += h[u] * W.wo[u];
      }
      var p = sigm(out);
      me.prob = p; me.frames++;
      me.speech = me.speech ? p >= VAD_OFF : p >= VAD_ON;
      me.speechMs = me.speech ? me.speechMs + 32 : 0;
      // Talking: speech long enough, and rising and falling as speech does.
      var run = Math.min(64, me.speechMs / 32), lo = Infinity, hi = -Infinity;
      for (var y = 0; y < run - VAD_SKIP; y++) { var d = db[(me.frames - 1 - y + 64) % 64]; if (d < lo) lo = d; if (d > hi) hi = d; }
      me.talking = me.speechMs >= SPEECH_MS && run - VAD_SKIP >= 5 && hi - lo >= SPEECH_SWING_DB;
      for (var z = 0; z < VAD_CTX; z++) buf[z] = buf[VAD_FRAME + z];
      fill = VAD_CTX;
    }
    /** [pcm]: 16 kHz samples, Int16Array or Float32Array (-1..1). */
    me.push = function (pcm) {
      var scale = pcm.BYTES_PER_ELEMENT === 2 ? 1 / 32768 : 1;
      for (var i = 0; i < pcm.length; i++) { buf[fill++] = pcm[i] * scale; if (fill === buf.length) frame(); }
    };
    me.reset = function () { buf.fill(0); fill = VAD_CTX; h.fill(0); c.fill(0); db.fill(0); me.prob = 0; me.speech = false; me.speechMs = 0; me.talking = false; };
    return me;
  }

  /**
   * The microphone level to draw: [level] only while the speech detector
   * hears someone talking (its [talking], as barge-in), and SHOWN_TAIL_MS after (a word's end is not cut); 0 for
   * noise and until the detector has warmed up. With no detector to be had
   * ([none]: the weights could not load) the level as it is. [st] keeps when
   * speech was last heard. The phone's SpeechCheck.shown has the same rule.
   */
  var SHOWN_TAIL_MS = 300;
  function shownLevel(vad, none, level, now, st) {
    if (!vad) return none ? level : 0;
    if (vad.frames < VAD_WARM) return 0;
    // Someone talking, as barge-in decides it (speech that rises and falls): a hum or music read as speech for a moment never counts.
    if (vad.talking) st.speechAt = now;
    return now - (st.speechAt || -1e12) < SHOWN_TAIL_MS ? level : 0;
  }

  /** The weights, fetched once per page. Resolves to null when they cannot be had (the voicing check stands in). */
  var vadWeights = null;
  function loadVad() {
    if (!vadWeights) {
      vadWeights = (typeof fetch === 'function' ? fetch('/vendor/silero-vad-16k.bin') : Promise.reject(new Error('no fetch')))
        .then(function (r) { if (!r.ok) throw new Error('HTTP ' + r.status); return r.arrayBuffer(); })
        .then(parseVad)
        .catch(function () { vadWeights = null; return null; });
    }
    return vadWeights;
  }

  /** The last [maxMs] of microphone chunks, to send when the owner cuts in. */
  function createPreroll(maxMs) {
    var q = [], total = 0;
    return {
      push: function (chunk, ms) { q.push({ c: chunk, ms: ms }); total += ms; while (q.length > 1 && total - q[0].ms >= maxMs) total -= q.shift().ms; },
      /** Oldest first; [lastMs]: only about that much of the latest (what came before was not him). */
      drain: function (lastMs) {
        var keep = q;
        if (lastMs != null) { var t = 0, i = q.length; while (i > 0 && t < lastMs) { i--; t += q[i].ms; } keep = q.slice(i); }
        var out = keep.map(function (x) { return x.c; }); q = []; total = 0; return out;
      },
      clear: function () { q = []; total = 0; },
      get ms() { return total; },
    };
  }

  /** Opus at a speech-friendly bitrate for the loopback (it never leaves the machine). */
  function richOpus(sdp) {
    var m = /a=rtpmap:(\d+) opus\/48000/i.exec(sdp || '');
    if (!m) return sdp;
    var re = new RegExp('(a=fmtp:' + m[1] + ' [^\r\n]*)');
    return re.test(sdp) ? sdp.replace(re, function (l) { return /maxaveragebitrate/.test(l) ? l : l + ';maxaveragebitrate=96000'; }) : sdp;
  }

  /**
   * Crundi's voice through WebRTC inside the page: two connections joined to
   * each other here (no server, no STUN), the sound played from the second's
   * remote stream in an <audio>. That is what Chrome's echo canceller listens
   * to as the far end, so it takes Crundi out of the microphone. Resolves to
   * { dest, close } with dest the node to play into; rejects (play directly
   * instead) without WebRTC, or when it isn't connected in [timeoutMs].
   */
  function makeLoopback(ctx, W, timeoutMs) {
    W = W || window;
    return new Promise(function (resolve, reject) {
      if (!W.RTCPeerConnection || !ctx || !ctx.createMediaStreamDestination) { reject(new Error('no WebRTC here')); return; }
      var a = null, b = null, el = null, done = false, playing = false, joined = false, dest = null;
      var lb = { close: function () { try { a && a.close(); } catch (e) { /* gone */ } try { b && b.close(); } catch (e) { /* gone */ } try { if (el) { el.pause(); el.srcObject = null; } } catch (e) { /* gone */ } }, onlost: null };
      var timer = setTimeout(function () { fail(new Error('the loopback did not connect')); }, timeoutMs || 3000);
      function fail(e) { if (done) { if (lb.onlost) lb.onlost(e); return; } done = true; clearTimeout(timer); lb.close(); reject(e); }
      function ready() { if (done || !playing || !joined) return; done = true; clearTimeout(timer); lb.dest = dest; resolve(lb); }
      try {
        dest = ctx.createMediaStreamDestination();
        a = new W.RTCPeerConnection({ iceServers: [] }); b = new W.RTCPeerConnection({ iceServers: [] });
        a.onicecandidate = function (e) { if (e.candidate) Promise.resolve(b.addIceCandidate(e.candidate)).catch(function () { /* the next one */ }); };
        b.onicecandidate = function (e) { if (e.candidate) Promise.resolve(a.addIceCandidate(e.candidate)).catch(function () { /* the next one */ }); };
        b.oniceconnectionstatechange = function () {
          var st = b.iceConnectionState;
          if (st === 'connected' || st === 'completed') { joined = true; ready(); }
          else if (st === 'failed' || st === 'closed' || st === 'disconnected') fail(new Error('the loopback ' + st));
        };
        b.ontrack = function (e) {
          el = new W.Audio(); el.autoplay = true;
          el.srcObject = (e.streams && e.streams[0]) || new W.MediaStream([e.track]);
          Promise.resolve(el.play()).then(function () { playing = true; ready(); }, fail);
        };
        dest.stream.getAudioTracks().forEach(function (t) { a.addTrack(t, dest.stream); });
        Promise.resolve(a.createOffer())
          .then(function (o) { return a.setLocalDescription({ type: o.type, sdp: richOpus(o.sdp) }); })
          .then(function () { return b.setRemoteDescription(a.localDescription); })
          .then(function () { return b.createAnswer(); })
          .then(function (ans) { return b.setLocalDescription({ type: ans.type, sdp: richOpus(ans.sdp) }); })
          .then(function () { return a.setRemoteDescription(b.localDescription); })
          .catch(fail);
      } catch (e) { fail(e); }
    });
  }

  var CSS = '' +
    '.cv-ov{position:fixed;inset:0;z-index:9000;background:' + C.bg + ';display:flex;flex-direction:column;align-items:center;font-family:system-ui,-apple-system,sans-serif;color:' + C.text + '}' +
    '.cv-top{width:100%;max-width:820px;display:flex;flex-wrap:wrap;align-items:center;gap:6px 8px;padding:16px 20px 0;color:' + C.text2 + ';font-size:12px;box-sizing:border-box}' +
    '.cv-wm{display:flex;align-items:center}.cv-wm .cru-ic .cru-b1,.cv-wm .cru-ic .cru-b2{transform-box:fill-box;transform-origin:50% 50%}.cv-wm .cru-ic.cru-live .cru-b1{animation:cru-p .9s ease-in-out infinite}.cv-wm .cru-ic.cru-live .cru-b2{animation:cru-p .9s ease-in-out .15s infinite}@keyframes cru-p{0%,100%{transform:scaleY(1)}50%{transform:scaleY(.45)}}' +
    '.cv-pill{margin-left:auto;display:flex;align-items:center;gap:7px;padding:5px 12px;border-radius:14px;font-size:12px;font-weight:500;color:#d6d6f0;transition:background .4s,border-color .4s}' +
    '.cv-pill i{width:7px;height:7px;border-radius:50%;animation:cvp 1s ease-in-out infinite alternate}@keyframes cvp{from{opacity:.35}to{opacity:1}}' +
    '.cv-admin{margin-left:8px;padding:2px 8px;border-radius:999px;font:700 10px/16px ui-monospace,monospace;letter-spacing:.08em;color:#fecaca;background:rgba(239,68,68,.18);border:1px solid rgba(239,68,68,.55)}' +
    '.cv-chat{width:100%;max-width:680px;flex:1;overflow-y:auto;padding:10px 14px;box-sizing:border-box;display:none;flex-direction:column}.cv-msgs{display:flex;flex-direction:column;gap:8px;margin-top:auto}' +
    '.cv-m{max-width:82%;padding:8px 12px;border-radius:16px;font-size:14px;line-height:20px;white-space:pre-wrap;word-wrap:break-word}.cv-m.me{align-self:flex-end;background:#2b2b55;border-bottom-right-radius:5px}.cv-m.cru{align-self:flex-start;background:' + C.card + ';border:1px solid ' + C.border + ';border-bottom-left-radius:5px}.cv-m.live{opacity:.6;font-style:italic}' +
    '.cv-m .d{display:block;margin-top:6px;padding-top:6px;border-top:1px solid ' + C.border + ';font-size:13px;color:' + C.text2 + '}.cv-m time{display:block;font-size:10px;color:' + C.muted + ';margin-top:3px}' +
    '.cv-m.cru{position:relative;overflow:visible}.cv-m.cru.say{margin-right:40px}.cv-say{position:absolute;right:-40px;bottom:0;width:34px;height:34px;border:0;border-radius:50%;background:transparent;color:' + C.muted + ';cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0}.cv-say:hover{color:' + C.text + ';background:rgba(255,255,255,.08)}' +
    '.cv-m.cru.cv-typing{display:flex;gap:6px;align-items:center;padding:14px 16px;min-height:0}.cv-typing i{width:8px;height:8px;border-radius:50%;background:' + C.text2 + ';opacity:.35;animation:cvdot 1.2s ease-in-out infinite}.cv-typing i:nth-child(2){animation-delay:.2s}.cv-typing i:nth-child(3){animation-delay:.4s}' +
    '@keyframes cvdot{0%,60%,100%{opacity:.35;transform:none}30%{opacity:1;transform:translateY(-3px)}}' +
    '.cv-say.on{color:' + C.accent + '}.cv-say.on .w1{animation:cvwave 1.1s ease-in-out infinite}.cv-say.on .w2{animation:cvwave 1.1s ease-in-out .33s infinite}' +
    '@keyframes cvwave{0%,100%{opacity:.25}35%{opacity:1}}' +
    '.cv-more{align-self:center;font-size:12px;color:' + C.text2 + ';cursor:pointer;padding:4px 10px;margin-bottom:6px;border-radius:12px;background:#1a1a28}' +
    '.cv-x.on{background:' + C.accent + ';color:#fff}.cv-chatb{position:relative}.cv-chatb.cru-unread:after{content:"";position:absolute;top:2px;right:2px;width:8px;height:8px;border-radius:50%;background:#ef4444}' +
    // The bottom row as on the phone: the voice button in the middle, the keyboard on its left;
    // the text box (with its send button) only in keyboard mode, where it takes the voice button's place.
    '.cv-bot{position:relative}.cv-bot .cv-kbb{margin-left:0;flex:none;position:absolute;left:24px;top:50%;transform:translateY(-50%)}.cv-ov.kb .cv-bot .cv-kbb{position:static;transform:none}' +
    '.cv-bot .cv-attb{position:absolute;left:72px;top:50%;transform:translateY(-50%);margin:0}.cv-ov.kb .cv-bot .cv-attb{position:static;transform:none}.cv-ov.kid .cv-attb,.cv-ov.kid .cv-att{display:none!important}' +
    '.cv-att{display:flex;flex-wrap:wrap;gap:6px;padding:4px 16px 0;justify-content:center}.cv-att[hidden]{display:none}.cv-chip{display:inline-flex;align-items:center;gap:6px;max-width:220px;padding:4px 6px 4px 8px;border-radius:12px;background:#1a1a28;border:1px solid ' + C.border + ';color:' + C.text + ';font-size:12px}.cv-chip img{width:28px;height:28px;object-fit:cover;border-radius:6px}.cv-chip span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.cv-chip.up{opacity:.6}.cv-chip.bad{border-color:#f87171}.cv-chip button{border:0;background:none;color:' + C.muted + ';cursor:pointer;padding:0 2px;font-size:14px;line-height:1}' +
    '.cv-ov.drop{outline:3px dashed ' + C.accent + ';outline-offset:-8px}.cv-m .att{display:inline-flex;align-items:center;gap:6px;margin:2px 0;padding:3px 8px;border-radius:10px;background:rgba(255,255,255,.08);font-size:12px}' +
    // Kid mode: a big keyboard button (left) and a mute button (right); typing in a big friendly box.
    '.cv-ov:not(.kid) .cv-mute{display:none!important}.cv-ov.kid .cv-mute{display:flex;position:absolute;right:24px;top:50%;transform:translateY(-50%)}.cv-mute .sl{display:none}.cv-ov.muted .cv-mute .sl{display:inline}.cv-ov.muted .cv-mute{background:#fee2e2!important;color:#b91c1c!important}' +
    '.cv-ov.kid .cv-kbb,.cv-ov.kid .cv-mute{width:64px;height:64px;border-radius:50%;background:#fff;color:#4f46e5;box-shadow:0 5px 0 rgba(49,46,129,.18)}.cv-ov.kid .cv-kbb svg{width:28px;height:28px}.cv-ov.kid .cv-kbb.on{background:#4f46e5;color:#fff}' +
    '.cv-ov.kid.kb .cv-type{font:700 22px "Comic Sans MS","Chalkboard SE",system-ui,sans-serif;padding:14px 20px;border-radius:28px;background:#fff;color:#312e81;border:3px solid #a5b4fc}.cv-ov.kid.kb .cv-send{width:64px;height:64px;border-radius:50%}' +
    '.cv-ov.locked .cv-chatb,.cv-ov.locked .cv-attb,.cv-ov.locked .cv-chat,.cv-ov.locked .cv-att{display:none!important}' +
    '.cv-ov.muted .cv-mic{opacity:.35;filter:grayscale(1)}.cv-ov.kid.kb .cv-mute{display:none!important}' +
    '.cv-bot .cv-type,.cv-bot .cv-send{display:none}.cv-ov.kb .cv-bot .cv-type{display:block}.cv-ov.kb .cv-bot .cv-send{display:flex;margin-left:0;background:' + C.accent + ';color:#fff}' +
    '.cv-ov.kb .cv-mic,.cv-ov.kb .cv-ptt{display:none!important}' +
    '.cv-confirm{width:100%;max-width:680px;box-sizing:border-box;margin:8px auto 0;padding:10px 14px;border-radius:12px;background:' + C.card + ';border:1px solid ' + C.border + ';font-size:13px;color:' + C.text + ';display:flex;flex-wrap:wrap;gap:8px;align-items:center}.cv-confirm[hidden]{display:none}.cv-confirm span{flex:1 1 200px}.cv-confirm .yes{border-color:' + C.accent + '}' +
    '.cv-ov.chat .cv-field,.cv-ov.chat .cv-words{display:none}.cv-ov.chat .cv-chat{display:flex}.cv-ov.chat .cv-mic{width:64px;height:64px}.cv-ov.chat .cv-mic canvas{left:-104px;top:-44px}' +
    '.cv-x{margin-left:10px;width:34px;height:34px;border-radius:50%;border:0;background:#1a1a28;color:' + C.text2 + ';cursor:pointer;font-size:0;display:flex;align-items:center;justify-content:center}' +
    '.cv-field{position:relative;width:100%;max-width:820px;flex:1.5;min-height:300px}.cv-field canvas{position:absolute;inset:0;width:100%;height:100%}' +
    '.cv-leg{order:9;flex:1 1 100%;min-width:0;display:flex;flex-wrap:wrap;align-items:center;gap:2px 12px;font:10.5px ui-monospace,monospace;padding:2px 2px 0;line-height:15px}.cv-leg span{white-space:nowrap}.cv-leg span:before{content:"";display:inline-block;width:7px;height:7px;border-radius:50%;margin-right:5px;background:currentColor}' +
    '.cv-words{width:100%;max-width:680px;flex:1;overflow:auto;text-align:center;padding:10px 24px;box-sizing:border-box}' +
    '.cv-heard{color:' + C.text2 + ';font-size:16px;line-height:23px;min-height:23px;display:flex;align-items:center;justify-content:center;gap:6px}' +
    '.cv-cx{flex:none;width:26px;height:26px;border-radius:50%;border:0;background:#1a1a28;color:' + C.text2 + ';cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0}.cv-cx:hover{color:' + C.text + ';background:#24243a}.cv-cx[hidden]{display:none}' +
    '.cv-q{margin-top:6px;font-size:13px;line-height:18px;color:' + C.hover + '}.cv-q[hidden]{display:none}' +
    '.cv-ht{min-width:0}.cv-reply{font-size:19px;line-height:27px;font-weight:500;margin-top:12px}' +
    '.cv-detail{margin-top:12px;text-align:left;background:' + C.card + ';border:1px solid ' + C.border + ';border-radius:14px;padding:12px 14px;font-size:14px;line-height:20px;white-space:pre-wrap}' +
    '.cv-note{margin-top:10px;font-size:13px;color:' + C.yellow + '}.cv-yesno{margin-top:8px;font-size:13px;color:' + C.hover + ';display:flex;gap:8px;align-items:center;justify-content:center}.cv-yesno[hidden]{display:none}.cv-yn{border:1px solid ' + C.border + ';background:#1a1a28;color:' + C.text + ';border-radius:16px;padding:6px 16px;font-size:13px;cursor:pointer}.cv-yn:hover{border-color:' + C.accent + '}' +
    '.cv-bot{width:100%;max-width:680px;display:flex;align-items:center;justify-content:center;gap:16px;padding:8px 24px 22px;box-sizing:border-box}' +
    '.cv-type{flex:1;min-width:0;background:#1a1a28;border:1px solid ' + C.border + ';border-radius:20px;color:' + C.text + ';padding:10px 14px;font-size:14px;outline:none}.cv-type:focus{border-color:' + C.accent + '}' +
    '.cv-mic{width:112px;height:112px;flex:none;cursor:pointer;position:relative}.cv-mic canvas{position:absolute;left:-80px;top:-20px;width:272px;height:152px;pointer-events:none}' +
    '.cv-ptt{display:none;font-size:11px;color:' + C.muted + ';text-align:center;padding-bottom:6px}' +
    // A wide screen (a PC): a dropdown under the top bar's mic button, the work in view around it.
    '@media (min-width:900px){.cv-ov{left:auto;right:var(--cv-right,14px);top:var(--cv-top,52px);bottom:auto;width:400px;height:min(600px,calc(100vh - 70px));border:1px solid ' + C.border + ';border-radius:16px;box-shadow:0 18px 60px rgba(0,0,0,.6);animation:cvdrop .18s ease-out}.cv-field{min-height:230px}.cv-reply{font-size:16px;line-height:23px}.cv-heard{font-size:14px}.cv-top{padding:12px 14px 0}.cv-bot{padding:4px 14px 12px}.cv-ptt{display:block}}' +
    // With Cru's panel up, the dropdown is wider and taller: the panel needs the room.
    '@media (min-width:900px){.cv-ov.has-scr:not(.kid){width:min(580px,calc(100vw - 40px));height:min(760px,calc(100vh - 70px))}}' +
    '@keyframes cvdrop{from{opacity:0;transform:translateY(-8px)}to{opacity:1;transform:none}}' +

    '.cv-mic svg{position:absolute;left:44px;top:44px;width:24px;height:24px;stroke:#fff;fill:none;stroke-width:2;stroke-linecap:round;stroke-linejoin:round;pointer-events:none}' +
    // The live screen: Cru's panel. In the voice view it takes the field's place; in the chat view it sits above the messages.
    '.cv-scr{display:none;position:relative;width:100%;max-width:820px;flex:1.5;min-height:200px;flex-direction:column;box-sizing:border-box;padding:8px 14px 0}.cv-ov.has-scr .cv-scr{display:flex}.cv-ov.has-scr .cv-field{display:none}' +
    '.cv-ov.chat .cv-scr{flex:none;max-height:42vh;min-height:0}.cv-ov.chat.scr-min .cv-scr-b{display:none}' +
    '.cv-scr-h{display:flex;align-items:center;gap:6px;padding:0 2px 6px}.cv-scr-t{flex:1;min-width:0;font-weight:600;font-size:14px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}' +
    '.cv-scr-h .cv-x{width:28px;height:28px;margin-left:0}.cv-ov:not(.chat) .cv-scr-min{display:none}' +
    '.cv-scr-b{flex:1;min-height:0;overflow:auto;background:' + C.card + ';border:1px solid ' + C.border + ';border-radius:14px;padding:12px 14px;display:flex;flex-direction:column;gap:10px;font-size:14px;line-height:20px}' +
    '.cv-scr-b.frame{padding:0;overflow:hidden}.cv-scr-b iframe{flex:1;width:100%;min-height:240px;border:0;border-radius:14px;background:transparent}' +
    '.cv-b-h{font-size:16px;font-weight:600}.cv-b-t{white-space:pre-wrap;color:' + C.text + '}.cv-b-l{margin:0;padding-left:20px}.cv-b-l li{margin:2px 0}' +
    '.cv-b-c{list-style:none;margin:0;padding:0}.cv-b-c li{display:flex;gap:8px;align-items:flex-start;margin:3px 0}.cv-b-c i{flex:none;width:14px;height:14px;margin-top:3px;border-radius:4px;border:1.5px solid ' + C.muted + '}.cv-b-c li.d i{background:' + C.green + ';border-color:' + C.green + '}.cv-b-c li.d span{color:' + C.text2 + ';text-decoration:line-through}' +
    '.cv-b-tb{border-collapse:collapse;width:100%}.cv-b-tb td,.cv-b-tb th{padding:4px 8px;border-bottom:1px solid ' + C.border + ';text-align:left;vertical-align:top}.cv-b-tb th{color:' + C.text2 + ';font-weight:500}.cv-b-tb td:first-child{color:' + C.text2 + '}' +
    '.cv-b-p{display:flex;flex-direction:column;gap:4px}.cv-b-p span{font-size:12px;color:' + C.text2 + '}.cv-b-p div{height:8px;border-radius:4px;background:#1a1a28;overflow:hidden}.cv-b-p b{display:block;height:100%;background:linear-gradient(90deg,' + C.sky + ',' + C.accent + ')}' +
    // Cru's UI over the whole content area (screen.full): the orb, the words and the chat make way;
    // the bottom bar stays, and the top bar's "Normal view" button is always there to go back.
    '.cv-fullx{display:none!important}.cv-ov.full .cv-fullx{display:flex!important;width:auto;padding:0 12px 0 10px;gap:6px;border-radius:17px;font-size:12px;font-weight:600;color:#fff;background:' + C.accent + '}.cv-fullx span{white-space:nowrap}' +
    '.cv-ov.full .cv-field,.cv-ov.full .cv-chat,.cv-ov.full .cv-scr-h,.cv-ov.full .cv-heard,.cv-ov.full .cv-detail,.cv-ov.full .cv-yesno{display:none!important}' +
    '.cv-ov.full .cv-scr{display:flex;flex:1 1 auto;max-width:none;max-height:none;min-height:0;padding:10px 12px 0}.cv-ov.full.scr-min .cv-scr-b{display:flex}' +
    '.cv-ov.full .cv-scr-b.frame{border:0;border-radius:0;background:transparent}.cv-ov.full .cv-scr-b iframe{border-radius:0;min-height:0}' +
    // Cru's latest words, a short line under its UI (a typed reply is read, not said).
    '.cv-ov.full .cv-words{display:block;flex:none;max-height:66px;padding:4px 18px 0}.cv-ov.full .cv-reply{margin-top:2px;font-size:14px;line-height:20px;font-weight:400;color:' + C.text2 + '}' +
    // A phone: room on the top row for the way back (Compact is a rare thing, it waits for the normal view); the chat view's button has nothing to show under Cru's UI.
    '@media (max-width:480px){.cv-ov.full .cv-cmp{display:none}}.cv-ov.full .cv-chatb{display:none}' +
    '@media (min-width:900px){.cv-ov.full:not(.kid){width:min(1000px,calc(100vw - 40px));height:calc(100vh - 70px)}.cv-ov.full .cv-top{max-width:none}}' +
    '.cv-b-i{max-width:100%;border-radius:10px}.cv-b-ch{display:flex;flex-wrap:wrap;gap:6px}.cv-b-ch span{padding:2px 10px;border-radius:999px;font-size:12px;border:1px solid}' +
    // Kid mode: the whole window, bright and friendly, a big cursor, nothing to leave by.
    '.cv-ov.kid{inset:0!important;left:0!important;top:0!important;right:0!important;bottom:0!important;width:100%!important;height:100%!important;max-height:none!important;border:0!important;border-radius:0!important;z-index:2147483000;background:radial-gradient(circle at 50% 30%,#1e1b4b,#0a0a0f 70%)}' +
    '.cv-ov.kid .cv-cmp,.cv-ov.kid .cv-chatb,.cv-ov.kid .cv-close,.cv-ov.kid .cv-confirm,.cv-ov.kid .cv-leg,.cv-ov.kid .cv-ptt,.cv-ov.kid .cv-admin,.cv-ov.kid .cv-scr-x{display:none!important}' +
    '.cv-ov.kid .cv-top,.cv-ov.kid .cv-scr,.cv-ov.kid .cv-field{max-width:none}.cv-ov.kid .cv-reply{font-size:24px;line-height:32px}.cv-ov.kid .cv-heard{font-size:18px}.cv-ov.kid .cv-mic{width:150px;height:150px}.cv-ov.kid .cv-mic svg{left:63px;top:63px}' +
    '.cv-ov.kid.has-scr .cv-scr{flex:4}.cv-ov.kid.has-scr .cv-words{flex:none;max-height:24vh}.cv-ov.kid .cv-scr-b iframe{height:100%}' +
    // Kid mode's own look (9 Nov, the owner: "doesn't look good, doesn't blend"): one bright, playful
    // backdrop the game sits on with no box around it, big everything, Cru's words in a speech bubble.
    // Light and sunny (the owner: "it's dark"): sky to sunshine, soft colourful shapes, dark friendly text.
    '.cv-ov.kid{color:#312e81!important;background:radial-gradient(circle at 10% 16%,rgba(251,191,36,.45) 0 8%,transparent 8.5%),radial-gradient(circle at 90% 22%,rgba(244,114,182,.38) 0 6%,transparent 6.5%),radial-gradient(circle at 82% 88%,rgba(52,211,153,.40) 0 7%,transparent 7.5%),radial-gradient(circle at 16% 86%,rgba(167,139,250,.40) 0 5.5%,transparent 6%),linear-gradient(175deg,#bae6fd 0%,#e0f2fe 45%,#fef9c3 100%)!important}' +
    '.cv-ov.kid .cv-top{padding:18px 28px 0}.cv-ov.kid .cv-wm .cru-ic{width:86px!important;height:52px!important}.cv-ov.kid .cv-pill{display:none!important}' +
    '.cv-ov.kid .cv-scr{max-width:none;padding:0 2vw}.cv-ov.kid .cv-scr-h,.cv-ov.kid .cv-scr-t{display:none!important}.cv-ov.kid.has-scr .cv-scr{flex:1 1 auto;min-height:0}' +
    // The child sees they were heard: their words in their own bubble, and Cru's dots while it thinks.
    '.cv-ov.kid .cv-heard{justify-content:flex-end;max-width:min(1100px,92vw);margin:0 auto;padding:0 24px;min-height:0}.cv-ov.kid .cv-heard .cv-ht{display:inline-block;background:#dbeafe;color:#1e3a8a;border-radius:24px;padding:10px 20px;font:700 clamp(17px,2vmin,26px)/1.3 "Comic Sans MS","Chalkboard SE",system-ui,sans-serif;box-shadow:0 4px 0 rgba(30,58,138,.12)}.cv-ov.kid .cv-heard .cv-ht:empty{display:none}.cv-ov.kid .cv-heard .cv-cx{display:none!important}' +
    '.cv-ov.kid .cv-reply.think{display:inline-block!important;min-width:90px;min-height:1em}.cv-ov.kid .cv-reply.think::after{content:"";display:inline-block;width:72px;height:18px;background:radial-gradient(circle,#6366f1 7px,transparent 8px) 0 50%/24px 18px repeat-x;animation:cvkdots 1s ease-in-out infinite}' +
    '.cv-ov.kid.kwait .cv-reply::before{content:"Wait for me!";display:block;font-size:.6em;color:#b45309;margin-bottom:4px}.cv-ov.kid.kwait .cv-reply.think::before{content:none}' +
    '@keyframes cvkdots{0%,100%{opacity:.35;transform:translateY(0)}50%{opacity:1;transform:translateY(-5px)}}' +
    '.cv-ov.kid .cv-words{max-width:min(1100px,92vw);padding:4px 24px 18px;overflow:visible!important;flex:none}' +
    '.cv-ov.kid .cv-reply{display:inline-block;position:relative;background:#fff;color:#312e81;border-radius:28px;padding:16px 26px;margin-top:6px;font:700 clamp(20px,2.4vmin,34px)/1.3 "Comic Sans MS","Chalkboard SE",system-ui,sans-serif;box-shadow:0 6px 0 rgba(49,46,129,.15),0 10px 30px rgba(49,46,129,.12)}.cv-ov.kid .cv-lock{color:#6366f1}.cv-ov.kid .cv-note{color:#b45309}.cv-ov.kid .cv-reply:empty{display:none}' +
    '.cv-ov.kid .cv-reply:after{content:"";position:absolute;left:50%;bottom:-14px;margin-left:-14px;border:14px solid transparent;border-top-color:#fff;border-bottom:0}' +
    '.cv-ov.kid .cv-detail{display:none!important}.cv-ov.kid .cv-bot{padding:6px 24px 20px}.cv-ov.kid .cv-mic{width:clamp(110px,15vmin,170px);height:clamp(110px,15vmin,170px)}' +
    '.cv-ov.kid .cv-scr-b{border:0!important;background:transparent!important;box-shadow:none!important}.cv-ov.kid .cv-wm .cru-ic .cru-b1{animation:cru-p .5s ease-in-out infinite}.cv-ov.kid .cv-wm .cru-ic .cru-b2{animation:cru-p .5s ease-in-out .12s infinite}' +
    '.cv-ov.kid,.cv-ov.kid *{cursor:url("' + KID_CURSOR + '") 5 3,auto}.cv-ov.kid button,.cv-ov.kid .cv-mic,.cv-ov.kid [role=button]{cursor:url("' + KID_POINTER + '") 28 28,pointer}' +
    '.cv-lock{display:none;width:30px;height:30px;border-radius:50%;border:0;background:transparent;color:' + C.muted + ';opacity:.45;cursor:pointer;margin-left:8px;align-items:center;justify-content:center}.cv-ov.kid .cv-lock{display:flex}.cv-lock:hover{opacity:.9}' +
    '.cv-talk{display:none;position:absolute;left:50%;bottom:14px;transform:translateX(-50%);padding:14px 26px;border-radius:999px;border:0;background:#f59e0b;color:#111;font:700 20px system-ui,sans-serif;box-shadow:0 6px 0 rgba(0,0,0,.3);z-index:3}.cv-ov.kid.vid .cv-talk{display:block}' +
    '.cv-fs{display:none;margin:6px auto 0;padding:8px 16px;border-radius:999px;border:1px solid ' + C.border + ';background:#1a1a28;color:' + C.text + ';font-size:14px}.cv-ov.kid.nofs .cv-fs{display:block}' +
    '.cv-pin{position:absolute;inset:0;background:rgba(10,10,15,.94);display:flex;align-items:center;justify-content:center;z-index:5}.cv-pin-box{display:flex;flex-direction:column;align-items:center;gap:12px;color:' + C.text + '}' +
    '.cv-pin-box p{margin:0;font-size:14px;color:' + C.text2 + '}.cv-pin-in{font:600 26px ui-monospace,monospace;letter-spacing:.4em;text-align:center;width:220px;padding:8px;border-radius:10px;border:1px solid ' + C.border + ';background:#1a1a28;color:' + C.text + '}' +
    '.cv-pin-k{display:grid;grid-template-columns:repeat(3,64px);gap:10px}.cv-pin-k button{height:56px;border-radius:12px;border:1px solid ' + C.border + ';background:#1a1a28;color:' + C.text + ';font-size:20px}.cv-pin-e{min-height:18px;font-size:13px;color:' + C.red + '}';

  var S = null;   // the open session

  /**
   * Hand-over with a chat's voice mode (claude-chat.js): 'crundi-voice-open'
   * on window as the screen opens (before it asks for the microphone), so a
   * chat listening lets go of it and pauses; 'crundi-voice-close' when it
   * closes, so that chat carries on.
   */
  function announce(name, detail) {
    try { window.dispatchEvent(new CustomEvent(name, { detail: detail || {} })); } catch (e) { /* an old browser: nobody listens */ }
  }

  function open(opts) {
    if (S) return;
    opts = opts || {};
    announce('crundi-voice-open', { how: opts.how || '' });
    if (!document.getElementById('cv-css')) { var st = el('style'); st.id = 'cv-css'; st.textContent = CSS; document.head.appendChild(st); }
    var ov = el('div', 'cv-ov');
    ov.innerHTML =
      '<div class="cv-top"><span class="cv-wm" aria-label="Cru">' + cruIcon(22) + '</span><div class="cv-leg"></div>' +
      '<div class="cv-pill"><i></i><span class="cv-st">Waking</span></div>' +
      '<span class="cv-admin" hidden title="Admin mode is on: Cru can run any command. Turn it off in Settings, or say: admin mode off">ADMIN</span>' +
      '<button class="cv-x cv-fullx" title="Back to the normal view" aria-label="Back to the normal view"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg><span>Normal view</span></button>' +
      '<button class="cv-x cv-cmp" title="Compact the conversation (Cru keeps a summary and its notebook)" aria-label="Compact the conversation"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7"/></svg></button>' +
      '<button class="cv-x cv-chatb" title="Chat view: the whole conversation as messages" aria-label="Chat view"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a8 8 0 0 1-11.6 7.1L4 20l1.1-4.6A8 8 0 1 1 21 12z"/></svg></button>' +
      '<button class="cv-x cv-close" title="Close (Esc)" aria-label="Close"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg></button>' +
      '<button class="cv-lock" title="Grown-ups" aria-label="Grown-ups"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg></button></div>' +
      '<div class="cv-confirm" hidden><span>Compact the conversation? Cru carries on from a summary; its notebook and memory search keep everything.</span><button class="cv-yn yes">Compact</button><button class="cv-yn no">Cancel</button></div>' +
      '<div class="cv-field"><canvas></canvas></div>' +
      '<div class="cv-scr"><div class="cv-scr-h"><span class="cv-scr-t"></span>' +
      '<button class="cv-x cv-scr-min" title="Fold the panel" aria-label="Fold the panel"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M6 15l6-6 6 6"/></svg></button>' +
      '<button class="cv-x cv-scr-x" title="Close the panel" aria-label="Close the panel"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg></button></div>' +
      '<div class="cv-scr-b"></div><button class="cv-talk">Talk to Cru</button></div>' +
      '<div class="cv-chat" hidden><div class="cv-more" hidden>Earlier messages</div><div class="cv-msgs"></div></div>' +
      '<div class="cv-words"><div class="cv-heard"><span class="cv-ht"></span><button class="cv-cx" hidden title="Cancel what I said" aria-label="Cancel what I said"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg></button></div><div class="cv-q" hidden></div><div class="cv-reply"></div><div class="cv-detail" hidden></div><div class="cv-yesno" hidden><span>Say it, or tap:</span><button class="cv-yn" data-yn="Yes, go ahead.">Yes</button><button class="cv-yn" data-yn="No, don\'t.">No</button></div><div class="cv-note" hidden></div></div>' +
      '<div class="cv-ptt">Hold ' + esc(label()) + ' to talk · click the mic to keep this open</div><button class="cv-fs">Full screen</button>' +
      '<div class="cv-att" hidden></div><input class="cv-file" type="file" multiple hidden>' +
      '<div class="cv-bot"><button class="cv-x cv-attb" title="Attach a file, photo or video (or drop it here)" aria-label="Attach"><svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg></button><button class="cv-x cv-kbb" title="Keyboard mode: type to Cru, no microphone" aria-label="Keyboard mode"><svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="5" width="20" height="14" rx="2"/><path d="M6 9h.01M10 9h.01M14 9h.01M18 9h.01M6 13h.01M18 13h.01M8 16h8M10 13h4"/></svg></button><input class="cv-type" placeholder="Type to Cru…" aria-label="Type to Cru"><button class="cv-x cv-send" title="Send" aria-label="Send"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m22 2-7 20-4-9-9-4z"/><path d="M22 2 11 13"/></svg></button>' +
      '<div class="cv-mic" role="button" aria-label="Talk"><canvas></canvas></div><button class="cv-x cv-mute" title="Mute the microphone" aria-label="Mute the microphone"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><path d="M12 19v3"/><path class="sl" d="m2 2 20 20"/></svg></button></div>';
    document.body.appendChild(ov);
    var q = function (s) { return ov.querySelector(s); };
    // Under the button that opened it (a wide screen).
    var rc = opts.anchor && opts.anchor.getBoundingClientRect ? opts.anchor.getBoundingClientRect() : null;
    if (rc && rc.width) {
      ov.style.setProperty('--cv-top', Math.round(rc.bottom + 8) + 'px');
      ov.style.setProperty('--cv-right', Math.max(8, Math.round(window.innerWidth - rc.right - 8)) + 'px');
    }
    S = {
      opts: opts, ov: ov, phase: 'start', heard: '', asked: '', reply: '', detail: '', notice: '', question: '', pending: '', reports: [],
      lastSaid: '', saidUntil: 0, quietSince: Date.now(), streaming: true, speaking: false,
      // live: the server drops Crundi's own words, so the stream stays on while it speaks (barge-in).
      // cancellable: the × by his words (the server's 'voice-cancel'; this page comes from that server). turn: numbers each question.
      cancellable: !(opts.capabilities && opts.capabilities.indexOf && opts.capabilities.indexOf('voice-cancel') < 0), turn: 0,
      live: false, hintAt: 0, cues: [], cueDevice: false, cueLast: -1, cue: null,
      // held: Crundi is talking (or just was): the mic goes to the local gate and the pre-roll, not the server.
      held: false, gate: createGate(), preroll: createPreroll(PREROLL_MS), ring: new Float32Array(VOICE_WINDOW), ringAt: 0, unknownVoice: false, route: 'direct',
      mic: 0, voice: 0, lvl: 0, peak: 0.12, wave: new Float32Array(96), lastShift: 0,
      liq: { m: 0, mv: 0, lv: 0, peak: 0.1, hist: new Float32Array(64), drops: [], ripples: [], until: 0, lastDrop: 0, lastT: 0 },
      noise: 0.01, echo: 0.6, loudFor: 0, spokeFor: 0, recent: [0, 0, 0, 0], settings: null, caps: {},
      cells: [], usage: {}, raf: 0, timer: 0,
      // How it was opened: 'click' (stays), 'hover' (goes when the pointer leaves, if idle), 'ptt' (push to talk).
      how: opts.how || 'click', ptt: opts.how === 'ptt',
      // Kid mode: the child's screen (locked; only the PIN leaves it). screen: Cru's live panel.
      kid: !!opts.kid, screen: null, video: createVideoGate(), sayAt: 0,
    };
    if (S.kid) ov.classList.add('kid');
    watchChatScroll(S);
    ov.addEventListener('mouseenter', function () { if (S) clearTimeout(S.leaveTimer); });
    ov.addEventListener('mouseleave', function () { if (S) maybeLeave(); });
    ov.addEventListener('pointerdown', function () { if (S && S.how === 'hover') S.how = 'click'; });
    q('.cv-close').onclick = close;
    q('.cv-scr-x').onclick = function (e) { e.stopPropagation(); closeScreen(); };
    q('.cv-fullx').onclick = function (e) { e.stopPropagation(); normalView(); };
    q('.cv-scr-min').onclick = function (e) { e.stopPropagation(); S.ov.classList.toggle('scr-min'); };
    q('.cv-lock').onclick = function (e) { e.stopPropagation(); pinPad(); };
    q('.cv-talk').onclick = function (e) { e.stopPropagation(); talkOverVideo(); };
    q('.cv-fs').onclick = function (e) { e.stopPropagation(); kidFullscreen(); };
    // A tap on Yes or No is the signed-in owner answering: typed, so no voice check applies.
    Array.prototype.forEach.call(ov.querySelectorAll('.cv-yn'), function (b) { b.onclick = function (e) { e.stopPropagation(); if (S && S.question) utter(b.getAttribute('data-yn'), true); }; });
    q('.cv-chatb').onclick = function (e) { e.stopPropagation(); chatView(!(S && S.chat)); };
    // The owner scrolling Cru's words himself: they stop following the speech until the next reply.
    ['wheel', 'touchstart', 'mousedown'].forEach(function (ev) { q('.cv-words').addEventListener(ev, function () { if (S) S.ownScroll = true; }, { passive: true }); });
    q('.cv-send').onclick = function (e) { e.stopPropagation(); var inp = q('.cv-type'); var v = inp.value.trim(); if (S && (v || attReady())) { inp.value = ''; if (v) utter(v, true); else ask('', false, true); inp.focus(); } };
    // Files for Cru (capability 'voice-attach'): the paperclip, a drop anywhere on Cru, a pasted picture.
    q('.cv-attb').onclick = function (e) { e.stopPropagation(); q('.cv-file').click(); };
    q('.cv-file').onchange = function () { var f = q('.cv-file'); attachFiles(f.files); f.value = ''; };
    ov.addEventListener('dragover', function (e) { if (!S || S.kid) return; e.preventDefault(); e.stopPropagation(); ov.classList.add('drop'); });
    ov.addEventListener('dragleave', function (e) { if (e.target === ov) ov.classList.remove('drop'); });
    ov.addEventListener('drop', function (e) {
      if (!S || S.kid) return;
      e.preventDefault(); e.stopPropagation(); ov.classList.remove('drop');
      var dt = e.dataTransfer;
      if (dt && dt.files && dt.files.length) { attachFiles(dt.files); return; }
      // A kanban card, a file row, a mindmap idea... dragged from Crundi: its reference ("[Kanban Task ID: …]").
      var t = dt && dt.getData('text/plain');
      if (t) addRef(t);
    });
    q('.cv-type').addEventListener('paste', function (e) {
      var items = (e.clipboardData && e.clipboardData.files) || [];
      if (items.length) { e.preventDefault(); attachFiles(items); }
    });
    q('.cv-kbb').onclick = function (e) { e.stopPropagation(); if (S && S.kid) { kidMute(false, true); keyboard(!S.kb, false); return; } keyboard(!(S && S.kb), true); };
    // Kid mode: the microphone off (Cru still talks, taps and games still work); tap again to listen.
    q('.cv-mute').onclick = function (e) { e.stopPropagation(); if (S) kidMute(!S.muted); };
    q('.cv-more').onclick = function (e) { e.stopPropagation(); loadChat(true); };
    q('.cv-msgs').onclick = function (e) { var b = e.target && e.target.closest && e.target.closest('.cv-say'); if (!b) return; e.stopPropagation(); sayAgain(Number(b.getAttribute('data-say'))); };
    // The view the owner chose last time, on this device: the chat view unless he
    // went back to the voice view (the default since 10 Nov). Kid mode keeps its own screen.
    if (chatPref() && !S.kid) setTimeout(function () { if (S) chatView(true); }, 0);
    setUnread(unread);
    // Compact asks first: a small question under the top row, Compact or Cancel.
    q('.cv-cmp').onclick = function (e) {
      e.stopPropagation();
      if (!S) return;
      var box = q('.cv-confirm');
      box.hidden = !box.hidden;
    };
    q('.cv-confirm .no').onclick = function (e) { e.stopPropagation(); q('.cv-confirm').hidden = true; };
    q('.cv-confirm .yes').onclick = function (e) {
      e.stopPropagation();
      if (!S) return;
      q('.cv-confirm').hidden = true;
      S.notice = 'Compacting the conversation…'; render();
      post('/api/voice/compact').then(function (r) {
        if (!S) return;
        S.notice = r && r.ok ? 'Compacted: Cru carries on from a summary, its notebook kept.' : ((r && (r.error || r.skipped)) ? 'Not compacted: ' + (r.error || r.skipped) : 'Not compacted');
        render();
      }, function () { if (S) { S.notice = 'Not compacted'; render(); } });
    };
    q('.cv-cx').onclick = function (e) { e.stopPropagation(); cancelWords(); };
    q('.cv-mic').onclick = tapMic;
    // Keys go to the voice screen first while it is open (window, capture):
    // Enter sends what was typed, Escape closes, and the page's own shortcuts
    // don't fire underneath.
    S.onKey = function (e) {
      if (!S) return;
      // Kid mode: no key does anything but typing the PIN.
      if (S.kid) {
        if (e.target && e.target.classList && e.target.classList.contains('cv-pin-in')) { e.stopPropagation(); if (e.key === 'Enter') { e.preventDefault(); pinTry(); } return; }
        e.stopPropagation(); e.preventDefault(); return;
      }
      // Docked beside the page (a wide screen), keys elsewhere are the page's.
      var docked = window.innerWidth >= 900, inside = S.ov.contains(e.target);
      if (docked && !inside) return;
      if (e.key === 'Escape') { e.stopPropagation(); e.preventDefault(); close(); return; }
      var inp = S.ov.querySelector('.cv-type');
      if (e.target === inp) {
        e.stopPropagation(); S.quietSince = Date.now();
        if (e.key === 'Enter' && (inp.value.trim() || attReady())) { e.preventDefault(); var t = inp.value.trim(); inp.value = ''; if (t) utter(t, true); else ask('', false, true); }
      } else if (S.ov.contains(e.target) || e.target === document.body) e.stopPropagation();
    };
    window.addEventListener('keydown', S.onKey, true);
    S.onMsg = onFrameMessage;
    window.addEventListener('message', S.onMsg);
    if (S.kid) kidSetup();
    loadScreen();
    S.logo = new Image(); S.logo.src = LOGO;
    refreshData();
    S.timer = setInterval(function () { refreshData(); tick(); }, 1000);
    S.raf = requestAnimationFrame(frame);
    keepAwake();
    start();
  }

  /** Opened by hovering: close again once the pointer has gone, unless the owner is talking or it is busy. */
  function maybeLeave() {
    if (!S || S.how !== 'hover') return;
    clearTimeout(S.leaveTimer);
    S.leaveTimer = setTimeout(function () {
      if (!S || S.how !== 'hover') return;
      var busy = S.phase === 'think' || S.phase === 'speak' || S.heard || S.ov.querySelector('.cv-type').value;
      if (!busy) close();
    }, 700);
  }

  /** Push to talk: the key was let go. Send what was heard now, without waiting for a pause. */
  function pttRelease() {
    if (!S || !S.ptt) return;
    S.ptt = false; S.how = 'click';
    setTimeout(function () {
      if (!S) return;
      if (S.phase === 'listen' && S.heard && !S.pttSent) utter(S.heard, false);
      S.pttSent = false;
    }, 700);
  }

  /** The screen stays on while voice is open (Screen Wake Lock), and again after the page comes back. */
  function keepAwake() {
    if (!S || !navigator.wakeLock || !navigator.wakeLock.request) return;
    navigator.wakeLock.request('screen').then(function (l) { if (S) S.wake = l; else l.release(); }).catch(function () { /* not allowed here */ });
    if (!S.onVis) { S.onVis = function () { if (S && document.visibilityState === 'visible' && (!S.wake || S.wake.released)) keepAwake(); }; document.addEventListener('visibilitychange', S.onVis); }
  }

  function close(force) {
    if (!S) return;
    if (S.kid && force !== 'kid-exit') return;
    var s = S; S = null;
    announce('crundi-voice-close', {});
    try { if (s.wake) s.wake.release(); } catch (e) { /* gone */ }
    if (s.onVis) document.removeEventListener('visibilitychange', s.onVis);
    cancelAnimationFrame(s.raf); clearInterval(s.timer);
    window.removeEventListener('keydown', s.onKey, true);
    if (s.onMsg) window.removeEventListener('message', s.onMsg);
    if (s.onFs) document.removeEventListener('fullscreenchange', s.onFs);
    stopSpeaking(s);
    clearTimeout(s.releaseTimer);
    try { if (s.loop) s.loop.close(); } catch (e) { /* gone */ }
    try { if (s.ws) { s.ws.send(JSON.stringify({ type: 'stop' })); s.ws.close(); } } catch (e) { /* gone */ }
    try { if (s.proc) s.proc.disconnect(); if (s.stream) s.stream.getTracks().forEach(function (t) { t.stop(); }); if (s.ctx) s.ctx.close(); } catch (e) { /* gone */ }
    var vvp = window.visualViewport;
    if (s.onVv && vvp) { vvp.removeEventListener('resize', s.onVv); vvp.removeEventListener('scroll', s.onVv); }
    if (s.chatRo) s.chatRo.disconnect();
    s.ov.remove();
    liveMark();
  }

  /**
   * The chat view keeps the newest message in view, as a Claude chat's log
   * does. It follows while the owner is at the bottom (chatStick, set by his
   * scrolling); the keyboard opening keeps the bottom of the conversation
   * where it was instead of hiding it; a send goes to it (ask). On a phone the
   * screen is fitted to the visual viewport, so on iOS too the keyboard makes
   * it shorter rather than covering its bottom.
   */
  function watchChatScroll(s) {
    var ov = s.ov, box = ov.querySelector('.cv-chat');
    s.chatStick = true; s.chatH = 0;
    var toBottom = function () { box.scrollTop = box.scrollHeight; };
    box.addEventListener('scroll', function () {
      if (S !== s || !box.clientHeight) return;
      // Scrolled by its own change of height (the keyboard): the observer decides, not this.
      if (s.chatH && box.clientHeight !== s.chatH) return;
      s.chatStick = box.scrollHeight - box.scrollTop - box.clientHeight < 60;
    });
    if (window.ResizeObserver) {
      s.chatRo = new ResizeObserver(function () {
        if (S !== s) return;
        var h = box.clientHeight;
        if (h && h !== s.chatH) {
          if (s.chatStick) toBottom();
          // Reading up: what was at the bottom of the screen stays at the bottom.
          else if (s.chatH && h < s.chatH) box.scrollTop += s.chatH - h;
        }
        s.chatH = h;
      });
      try { s.chatRo.observe(box); } catch (e) { s.chatRo = null; }
    }
    var vv = window.visualViewport;
    if (!vv || s.kid) return;
    s.onVv = function () {
      if (S !== s) return;
      if (window.matchMedia('(max-width: 899px)').matches) {
        ov.style.bottom = 'auto'; ov.style.top = Math.round(vv.offsetTop) + 'px'; ov.style.height = Math.round(vv.height) + 'px';
      } else if (ov.style.height) { ov.style.bottom = ''; ov.style.top = ''; ov.style.height = ''; }
      if (s.chat && s.chatStick) toBottom();
    };
    vv.addEventListener('resize', s.onVv); vv.addEventListener('scroll', s.onVv);
    s.onVv();
  }

  // ─── the server ───
  function api(path, init) { return S.opts.apiFetch(path, init).then(function (r) { return r.json().catch(function () { return { ok: false, error: 'HTTP ' + r.status }; }); }); }
  function post(path, body) { return api(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body || {}) }); }

  function refreshData() {
    if (!S) return;
    try { S.cells = (S.opts.cells ? S.opts.cells() : []).slice(0, 12); } catch (e) { S.cells = []; }
    try { S.usage = S.opts.usage ? (S.opts.usage() || {}) : {}; } catch (e) { S.usage = {}; }
    var leg = S.ov.querySelector('.cv-leg'), u = S.usage, sp = S.settings && S.settings.spend;
    var items = [];
    if (u.week != null) items.push([C.hover, 'wk ' + Math.round(u.week * 100) + '%']);
    if (u.five != null) items.push([C.sky, '5h ' + Math.round(u.five * 100) + '%']);
    // The Claude API's spend means nothing while Cru answers through Claude Code.
    if (sp && (S.settings.brain || 'api') === 'api') items.push([C.ask, 'Claude $' + (sp.claudeUsd || 0).toFixed(2) + ' / $' + S.settings.monthlyLimitUsd]);
    if (sp) { items.push([C.green, 'Deepgram $' + (sp.deepgramUsd || 0).toFixed(2) + ' / $' + (S.settings.deepgramLimitUsd || 200)]); }
    leg.innerHTML = items.map(function (i) { return '<span style="color:' + i[0] + '">' + esc(i[1]) + '</span>'; }).join('');
  }

  function start() {
    post('/api/voice/warm');
    api('/api/voice/settings').then(function (s) {
      if (!S) return;
      if (!s || s.ok === false) { block(s && s.error ? s.error : 'The voice assistant is not available on this server.'); return; }
      S.settings = s; applyVolume(); refreshData();
      // The Claude Code brain needs no key: it runs on the subscription.
      if (!s.anthropicKeySet && s.brain !== 'claude-code') { block('There is no Claude API key on the server yet. Add one in Settings, Voice assistant.'); return; }
      // Keyboard mode when voice can't work here, or when the owner chose it last time on this device.
      if (!voiceUsable(s) || (kbPref() && !S.kid)) keyboard(true, false); else openMic();
      if (S.kid) setTimeout(function () { if (S && S.kid && !S.greeted) { S.greeted = true; ask('[Your friend is here: the kid screen just opened, fresh: whatever was on it before (a game, a drawing, a video) is over. Say hello.]', false, true, true); } }, 900);
    }).catch(function (e) { block(e.message || 'Could not reach the server'); });
  }

  function block(why) { if (!S) return; S.phase = 'ended'; S.notice = why; render(); }

  // ─── the microphone: 16 kHz PCM to the server, and the level ───
  function openMic() {
    navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } }).then(function (stream) {
      // Closed, or switched to typing (or muted) while the microphone was starting: let it go at once.
      if (!S || S.kb || S.muted) { stream.getTracks().forEach(function (t) { t.stop(); }); return; }
      S.stream = stream;
      var ctx = new (window.AudioContext || window.webkitAudioContext)();
      S.ctx = ctx;
      outNode();
      loadCues();
      var src = ctx.createMediaStreamSource(stream);
      S.src = src;
      var proc = ctx.createScriptProcessor(4096, 1, 1);
      S.proc = proc;
      var ratio = ctx.sampleRate / 16000;
      proc.onaudioprocess = function (ev) {
        if (!S) return;
        var x = ev.inputBuffer.getChannelData(0);
        var n = Math.floor(x.length / ratio), out = new Int16Array(n), sum = 0;
        for (var i = 0; i < n; i++) {
          var a = 0, from = Math.floor(i * ratio), to = Math.floor((i + 1) * ratio);
          for (var j = from; j < to; j++) a += x[j];
          var v = Math.max(-1, Math.min(1, a / Math.max(1, to - from)));
          out[i] = v * 32767; sum += v * v;
        }
        var level = Math.sqrt(Math.sqrt(sum / Math.max(1, n))), ms = x.length / ctx.sampleRate * 1000;
        S.mic = level;
        if (S.vad) S.vad.push(out); else if (!S.vadAsked) { S.vadAsked = true; var s0 = S; loadVad().then(function (W) { if (W && S === s0) s0.vad = createVad(W); }); }
        // What is drawn moves only for speech (10 Nov: it moved with the room's noise); the gates still get the raw level.
        S.micShown = shownLevel(S.vad, S.vadAsked && !S.vad && vadWeights === null, level, Date.now(), S);
        var nowMs = Date.now(); if (level >= (S.micPeak || 0) || nowMs - (S.micPeakAt || 0) > 2000) { S.micPeak = level; S.micPeakAt = nowMs; }
        if (S.live) {
          if (S.held) {
            // Crundi is talking: nothing goes out; is it him?
            S.preroll.push(out.buffer, ms);
            keepVoice(out);
            if (S.gate.hear(level, S.unknownVoice ? null : playLevel(), ms, heardSpeech)) ownerSpoke();
            return;
          }
          if (!S.speaking && !S.cue) S.gate.learn(level);
        } else onLevel(level, ms);
        if (S.streaming && S.ws && S.ws.readyState === 1) S.ws.send(out.buffer);
      };
      src.connect(proc); proc.connect(ctx.destination);
      var proto = location.protocol === 'https:' ? 'wss' : 'ws';
      var ws = new WebSocket(proto + '://' + location.host + '/api/voice/listen?encoding=linear16&sample_rate=16000&token=' + encodeURIComponent(S.opts.token ? S.opts.token() : ''));
      ws.binaryType = 'arraybuffer';
      S.ws = ws;
      ws.onmessage = function (m) { var e; try { e = JSON.parse(m.data); } catch (x) { return; } heard(e); };
      ws.onclose = function () {
        if (!S || S.ws !== ws || S.phase === 'ended') return;
        // Kid mode: never a bare error; a friendly line, and listening again in a moment.
        if (S.kid) { S.notice = 'Hold on, I am getting ready again!'; render(); var s = S; setTimeout(function () { if (S === s && S.kid && !S.paused) { S.notice = ''; stopMic(); S.phase = 'start'; openMic(); } }, 3000); return; }
        S.notice = 'Live listening stopped. Type instead, or close and open again.'; render();
      };
    }).catch(function () { if (!S) return; keyboard(true, false); S.notice = 'The microphone is not allowed here, so it is keyboard mode. Allow the microphone for this site to talk.'; render(); });
  }

  // ─── Keyboard mode: Cru without the microphone (and without Deepgram) ───
  var KB_KEY = 'crundi_cru_keyboard';
  function kbPref() { try { return localStorage.getItem(KB_KEY) === '1'; } catch (e) { return false; } }
  function voiceUsable(s) { return !!(s && s.deepgramKeySet && !s.deepgramLimitReached); }
  function kidMute(on, quiet) {
    if (!S || !!S.muted === !!on) return;
    S.muted = !!on;
    S.ov.classList.toggle('muted', S.muted);
    var b = S.ov.querySelector('.cv-mute'); if (b) { b.title = S.muted ? 'Turn the microphone back on' : 'Mute the microphone'; b.setAttribute('aria-label', b.title); }
    if (quiet) { render(); return; }
    if (S.muted) {
      if (!S.kb) {
        stopMic();
        if (!S.ctx) { try { S.ctx = new (window.AudioContext || window.webkitAudioContext)(); outNode(); } catch (e) { /* silent then */ } }
        reportSocket();
        if (S.phase !== 'think' && S.phase !== 'speak') S.phase = 'listen';
      }
    } else if (!S.kb) { closeSocket(); S.phase = 'start'; openMic(); }
    render();
  }
  function keyboard(on, chosen) {
    if (!S) return;
    if (!on && !voiceUsable(S.settings)) { S.notice = S.settings && S.settings.deepgramLimitReached ? 'Voice is off: this month\'s Deepgram limit is reached.' : 'Voice needs a Deepgram key: add one in Settings, Voice assistant.'; render(); return; }
    if (chosen) { try { localStorage.setItem(KB_KEY, on ? '1' : '0'); } catch (e) { /* private mode */ } }
    S.kb = !!on;
    S.ov.classList.toggle('kb', S.kb);
    var b = S.ov.querySelector('.cv-kbb'); if (b) { b.classList.toggle('on', S.kb); b.title = S.kb ? 'Back to voice' : 'Keyboard mode: type to Cru, no microphone'; }
    var input = S.ov.querySelector('.cv-type');
    if (S.kb) {
      stopMic();
      // Cru still talks back: the speakers need their own audio context.
      if (!S.ctx) { try { S.ctx = new (window.AudioContext || window.webkitAudioContext)(); outNode(); loadCues(); } catch (e) { /* silent then */ } }
      reportSocket();
      S.phase = 'listen'; S.heard = ''; S.quietSince = Date.now();
      if (input) setTimeout(function () { try { input.focus(); } catch (e) { /* gone */ } }, 0);
    } else {
      closeSocket();
      S.phase = 'start';
      openMic();
    }
    render();
  }
  /** Stop listening: the microphone, its processing and the stream to the server. */
  function stopMic() {
    closeSocket();
    try { if (S.proc) { S.proc.onaudioprocess = null; S.proc.disconnect(); } if (S.src) S.src.disconnect(); } catch (e) { /* gone */ }
    try { if (S.stream) S.stream.getTracks().forEach(function (t) { t.stop(); }); } catch (e) { /* gone */ }
    S.proc = null; S.src = null; S.stream = null; S.live = false; S.streaming = false; S.held = false; S.mic = 0; S.micShown = 0;
  }
  function closeSocket() {
    var ws = S.ws; S.ws = null;
    try { if (ws) { ws.send(JSON.stringify({ type: 'stop' })); ws.close(); } } catch (e) { /* gone */ }
  }
  /** Keyboard mode's line to the server: Cru's reports and settings changes, no audio. */
  function reportSocket() {
    var proto = location.protocol === 'https:' ? 'wss' : 'ws';
    var ws = new WebSocket(proto + '://' + location.host + '/api/voice/listen?keyboard=1&token=' + encodeURIComponent(S.opts.token ? S.opts.token() : ''));
    S.ws = ws;
    ws.onmessage = function (m) { var e; try { e = JSON.parse(m.data); } catch (x) { return; } if (e.type === 'ready') return; heard(e); };
  }

  function heard(e) {
    if (!S) return;
    if (e.type === 'ready') { S.live = !!e.echoFilter; if (S.phase === 'start') { tone('listen'); S.phase = 'listen'; S.quietSince = Date.now(); if (S.reports.length) { var q = S.reports.shift(); tell(q.speak, q.show); } } }
    else if (e.type === 'report') { if (e.kid && typeof e.expressivity === 'number') S.nextExpr = e.expressivity; tell(e.speak || '', e.show || ''); }
    // Cru changed the live screen; kid mode switched; a command for the game on screen.
    else if (e.type === 'screen') { showScreen(e.screen || null); return; }
    else if (e.type === 'kid') { if (kidHook) kidHook(e.kid || {}); return; }
    else if (e.type === 'game-command') { gameCommand(e.command || {}); return; }
    // An instant line from the server (a game being made, waiting for the parent): said if Cru is free.
    else if (e.type === 'say') { if (S.kid && e.text && S.phase === 'listen' && !S.heard && !S.video.playing && !S.paused) { S.reply = String(e.text); speak(String(e.text), { reflex: true, expressivity: typeof e.expressivity === 'number' ? e.expressivity : 1 }); render(); } return; }
    // The server dropped what he said ("scratch that"): clear it, or cancel the turn it started.
    else if (e.type === 'discard') takeBack(true);
    else if ((e.type === 'interim' || e.type === 'utterance') && S.paused) return;
    else if ((e.type === 'interim' || e.type === 'utterance') && S.video.playing) {
      if (e.type === 'utterance' && S.video.wake(e.text, recentMicPeak())) { talkOverVideo(); }
      return;
    }
    else if (e.type === 'interim') {
      // Talking over Crundi: cut it off the moment his words come.
      // Kid mode: a child talks over Cru all the time; Cru finishes, then answers (utter keeps the words).
      if (S.phase === 'speak') { if (S.kid) return; if (S.live && talksOver(e.text, S.lastSaid, Date.now() - S.hintAt <= HINT_MS ? 1 : 2)) { cutIn(); S.heard = e.text; } else return; }
      else if (echo(e.text)) return;
      else if (S.phase === 'listen' || S.phase === 'start') { S.phase = 'listen'; S.heard = e.text; S.quietSince = Date.now(); }
      else if (S.phase === 'think') { if (S.kid) return; if (S.cue && talksOver(e.text, S.cue.text)) stopCue(); S.heard = (S.pending + ' ' + e.text).trim(); }
    }
    else if (e.type === 'utterance') { if (S.phase === 'listen') S.pttSent = true; utter(e.text, false); }
    else if (e.type === 'error') { S.notice = e.error; }
    // A setting changed on the server (Cru's volume, say): picked up at once.
    else if (e.type === 'settings') { api('/api/voice/settings').then(function (s) { if (S && s && s.ok !== false) { S.settings = s; applyVolume(); render(); } }); return; }
    render();
  }

  /** Crundi's own words heard back: while it speaks, or for ECHO_MS after. */
  function echo(t) { return S.lastSaid && (S.speaking || Date.now() - S.saidUntil <= ECHO_MS) && isEcho(t, S.lastSaid); }

  function utter(text, typed) {
    var t = String(text || '').trim();
    if (!t || !S) return;
    if (!typed && echo(t)) { S.heard = ''; render(); return; }
    // "Scratch that" alone (an older server passes it on): take back, never ask it.
    if (!typed && isDiscard(t)) { if (S.phase === 'speak') { if (S.live) cutIn(); } else takeBack(true); render(); return; }
    // Kid mode: one thing at a time. Said while Cru thinks or talks: not saved up;
    // Cru says "wait a moment" (thinking) or shows its wait sign (talking), and
    // listens again once it has answered.
    if (S.kid && (S.phase === 'think' || S.phase === 'speak')) { kidWait(); return; }
    if (S.phase === 'think') { S.pending = (S.pending + ' ' + t).trim(); S.heard = S.pending; render(); return; }
    // Said over Crundi (the stream stayed on): cut it off and ask that now.
    if (S.phase === 'speak') { if (!typed && !(S.live && talksOver(t, S.lastSaid, 1))) return; cutIn(); }
    if (S.phase === 'ended' && !typed) return;
    // "Hey Cru" while already awake: here at once, without asking Claude.
    if (isCalling(t)) { var h = hereLine(); S.asked = t; S.heard = ''; S.reply = h; S.detail = ''; S.notice = ''; S.quietSince = Date.now(); speak(h); render(); return; }
    ask(t, false, typed);
  }

  /** News, unasked (a watched chat finished or waits on the owner): said now if Cru is just listening, else once it is free. */
  // ─── Chat view: the conversation as message bubbles, remembered on this device ───
  /** How long a screen Cru opened for its own news keeps listening for an answer, after saying it. */
  var REPORT_LINGER_MS = 8000;
  var CHAT_KEY = 'crundi_cru_chat_view';
  /** A speaking head in profile, sound coming from its mouth (the owner's drawing): tap to hear a reply; it is also the Speaking sign. */
  var SPEAKER_SVG = '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 21v-4.4C2.2 15.2 1.5 13 1.5 11 1.5 6.6 4.9 3.5 9 3.5c3.6 0 5.8 2.5 6 5.5l1.5 3-1.5.5v2c0 .8-.6 1.5-1.5 1.5h-2V21"/><path class="w1" d="M18.5 10.5a4 4 0 0 1 0 5"/><path class="w2" d="M21 8.5a7 7 0 0 1 0 9"/></svg>';
  function chatPref() { try { return localStorage.getItem(CHAT_KEY) !== '0'; } catch (e) { return true; } }
  function chatView(on) {
    if (!S) return;
    S.chat = !!on;
    try { localStorage.setItem(CHAT_KEY, on ? '1' : '0'); } catch (e) { /* private mode */ }
    S.ov.classList.toggle('chat', S.chat);
    var b = S.ov.querySelector('.cv-chatb'); if (b) { b.classList.toggle('on', S.chat); b.title = S.chat ? 'Back to the voice view' : 'Chat view: the whole conversation as messages'; }
    if (S.chat && !S.msgs) loadChat(false); else if (S.chat && unread) loadChat(false); else renderChat(true);
    if (S.chat) markSeen(); else setUnread(unread);
  }
  function loadChat(older) {
    if (!S) return;
    var s = S, before = older && s.msgs && s.msgs.length ? s.msgs[0].at : 0;
    api('/api/voice/history?limit=40' + (before ? '&before=' + before : '')).then(function (r) {
      if (S !== s || !r || r.ok === false) return;
      var got = r.turns || [];
      var recent = got.slice(-3).map(function (m) { return String(m.said || '').trim(); });
      s.msgs = older ? got.concat(s.msgs || []) : got.concat((s.msgs || []).filter(function (m) { return m.live && !(!m.speak && !m.show && recent.indexOf(String(m.said || '').trim()) >= 0); }));
      s.moreChat = !!r.more;
      renderChat(!older);
    });
  }
  /** A finished exchange (or Cru speaking up) joins the chat view as it happens. */
  function chatAdd(said, speak, show) {
    if (!S || (!said && !speak && !show)) return;
    if (!S.msgs) S.msgs = [];
    S.msgs.push({ at: Date.now(), said: said || '', speak: speak || '', show: show || '', live: true });
    if (S.chat) renderChat(true);
  }
  /**
   * While Cru answers: what waits to be asked next (said or typed meanwhile),
   * shown as queued and asked as soon as the answer comes; and the words still
   * coming in (what was asked is in the conversation already). As the
   * phone's CruChat.queued and CruChat.hearing.
   */
  function queuedOf(st) { return st && st.phase === 'think' ? String(st.pending || '').trim() : ''; }
  function hearingOf(st) {
    var h = String((st && st.heard) || '').trim();
    if (!h) return '';
    if (st.phase === 'listen') return h;
    if (st.phase !== 'think') return '';
    var p = String(st.pending || '').trim(), rest = p && h.indexOf(p) === 0 ? h.slice(p.length).trim() : h;
    return rest === String(st.asked || '').trim() || rest === p ? '' : rest;
  }
  function renderChat(toEnd) {
    if (!S || !S.chat) return;
    var box = S.ov.querySelector('.cv-chat'), list = S.ov.querySelector('.cv-msgs');
    var when = function (at) { var d = new Date(at); return d.toLocaleString([], { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }); };
    var h = '';
    // Thinking and Speaking in the conversation, as other chat apps show them (the pill stays too):
    // the speaking head by the reply being said moves, typing dots where the next reply will appear.
    var sayingAt = speakingIndex(S.msgs || [], S.phase, S.lastSaid);
    (S.msgs || []).forEach(function (m, i) {
      if (m.said) h += '<div class="cv-m me">' + saidHtml(m.said) + '<time>' + esc(when(m.at)) + '</time></div>';
      // The speaking head outside the bubble, at its bottom right: says this reply aloud on a tap (a typed
      // turn's reply is not said on its own), and moves while this reply is being said (tap again: stop).
      if (m.speak || m.show) h += '<div class="cv-m cru' + (m.speak ? ' say' : '') + '">' + esc(m.speak || '') + (m.show ? '<span class="d">' + esc(m.show) + '</span>' : '') + (m.said ? '' : '<time>' + esc(when(m.at)) + '</time>')
        + (m.speak ? (i === sayingAt
          ? '<button class="cv-say on" data-say="' + i + '" title="Cru is saying this: tap to stop" aria-label="Cru is saying this: tap to stop">'
          : '<button class="cv-say" data-say="' + i + '" title="Say it aloud" aria-label="Say it aloud">') + SPEAKER_SVG + '</button>' : '') + '</div>';
    });
    // Cru thinking, then what is queued for next, then the words being heard right now.
    if (S.phase === 'think') h += '<div class="cv-m cru cv-typing" role="img" aria-label="Cru is thinking"><i></i><i></i><i></i></div>';
    var queued = queuedOf(S), hearing = hearingOf(S);
    if (queued) h += '<div class="cv-m me live">' + esc(queued) + '<time>Queued: Cru answers it next</time></div>';
    if (hearing) h += '<div class="cv-m me live">' + esc(hearing) + '</div>';
    // Rebuilt only when it changed: render() runs on every word heard, and a long history
    // rebuilt each time starved the animation (the mic froze in the chat view).
    if (h === S.chatHtml && !toEnd) return;
    S.chatHtml = h;
    var nearEnd = box.scrollHeight - box.scrollTop - box.clientHeight < 60;
    list.innerHTML = h || '<div style="text-align:center;color:' + C.muted + ';font-size:13px">No messages yet.</div>';
    S.ov.querySelector('.cv-more').hidden = !S.moreChat;
    if (toEnd || nearEnd || S.chatStick) { box.scrollTop = box.scrollHeight; S.chatStick = true; }
  }

  /** The message whose reply is being said now (the last with those words), or -1 (as the phone's CruChat.speakingIndex). */
  function speakingIndex(msgs, phase, lastSaid) {
    if (phase !== 'speak') return -1;
    var t = String(lastSaid || '').trim();
    if (!t) return -1;
    for (var i = msgs.length - 1; i >= 0; i--) if (String(msgs[i].speak || '').trim() === t) return i;
    return -1;
  }
  /** The speaker on a reply: said now, when Cru is just listening (not while it thinks, talks or hears him). */
  function sayAgain(i) {
    var m = S && S.msgs && S.msgs[i];
    if (!m || !m.speak) return;
    // Tapped while it says this one: stop, and listen (as tapping over Cru does).
    if (S.phase === 'speak' && i === speakingIndex(S.msgs, S.phase, S.lastSaid)) { cutIn(); render(); return; }
    if (S.phase !== 'listen' || S.heard || S.pending) return;
    S.quietSince = Date.now();
    speak(m.speak);
    render();
  }

  function tell(say, show) {
    if (!S) return;
    if (S.phase !== 'listen' || S.pending || S.heard) { S.reports.push({ speak: say, show: show }); return; }
    S.reply = say; S.detail = show; S.heard = ''; S.notice = ''; S.question = ''; S.quietSince = Date.now();
    chatAdd('', say, show);
    if (say) speak(say);
    render();
  }

  /** What Cru says when called while it is already listening, one at random, never the same twice running (as the phone's Speech.HERE). */
  var HERE = ['Sir?', 'Yes?', 'Yes, Sir?', "I'm here, Sir.", 'Listening, Sir.', 'Here, Sir.', 'At your service, Sir.', "Yes, Sir. I'm listening.", 'Go ahead, Sir.'];
  var lastHere = '';
  function hereLine() { var c = HERE.filter(function (x) { return x !== lastHere; }); lastHere = c[Math.floor(Math.random() * c.length)]; return lastHere; }
  /** Just "Hey Cru", or the name alone (the phone's Speech.isCalling). */
  function isCalling(t) {
    var w = String(t).toLowerCase().split(/[^\p{L}\p{N}']+/u).filter(Boolean);
    while (w.length && /^(hey|hi|hello|ok|okay|oi|yo)$/.test(w[0])) w.shift();
    return w.length > 0 && w.length <= 2 && w.every(function (x) { return /^(cru|crew|kru|krew|crue|crews)$/.test(x); });
  }

  /**
   * [keep]: the last answer stays on screen (it was cut short by what the owner said meanwhile).
   * [typed]: typed here, not spoken: the server's voice check does not apply (capability 'voice-speaker').
   */
  // ─── Files and dropped items for Cru ───
  var ATT_MAX = 50 * 1024 * 1024;
  function attReady() { return !!(S && S.att && S.att.some(function (a) { return a.id || a.ref; })); }
  /** What goes with the next turn: the uploaded files and the dropped references; the tray empties. */
  function takeAtt() {
    var list = (S && S.att) || [];
    var ready = list.filter(function (a) { return a.id || a.ref; });
    S.att = list.filter(function (a) { return a.up; });
    renderAtt();
    return {
      attachments: ready.filter(function (a) { return a.id; }).map(function (a) { return a.id; }),
      refs: ready.filter(function (a) { return a.ref; }).map(function (a) { return a.ref; }),
      label: ready.map(function (a) { return a.ref ? '[The owner dropped in: ' + a.ref + ']' : '[The owner attached: "' + a.name + '" (' + (a.kind || 'file') + '), id ' + a.id + '.]'; }).join('\n'),
    };
  }
  function addRef(t) {
    if (!S) return;
    t = String(t).replace(/\s+/g, ' ').trim().slice(0, 400);
    if (!t) return;
    (S.att = S.att || []).push({ ref: t, name: t });
    renderAtt(); S.quietSince = Date.now();
  }
  /** A smaller JPEG of a picture, for Cru to look at (big photos stay whole for passing on). */
  function previewOf(file) {
    return new Promise(function (done) {
      if (!/^image\/(png|jpeg|webp|gif)$/.test(file.type)) { done(null); return; }
      var url = URL.createObjectURL(file), img = new Image();
      img.onload = function () {
        var k = Math.min(1, 1568 / Math.max(img.width, img.height)), c = document.createElement('canvas');
        c.width = Math.max(1, Math.round(img.width * k)); c.height = Math.max(1, Math.round(img.height * k));
        var g = c.getContext('2d'); g.fillStyle = '#fff'; g.fillRect(0, 0, c.width, c.height); g.drawImage(img, 0, 0, c.width, c.height);
        URL.revokeObjectURL(url);
        var d = c.toDataURL('image/jpeg', 0.85); done({ b64: d.slice(d.indexOf(',') + 1), thumb: d });
      };
      img.onerror = function () { URL.revokeObjectURL(url); done(null); };
      img.src = url;
    });
  }
  function readB64(file) {
    return new Promise(function (done, fail) { var r = new FileReader(); r.onload = function () { var d = String(r.result); done(d.slice(d.indexOf(',') + 1)); }; r.onerror = function () { fail(new Error('Could not read the file')); }; r.readAsDataURL(file); });
  }
  function attachFiles(files) {
    if (!S || S.kid) return;
    Array.prototype.slice.call(files || [], 0, 10).forEach(function (file) {
      var a = { name: file.name || 'file', up: true }, s = S;
      (S.att = S.att || []).push(a); renderAtt();
      if (file.size > ATT_MAX) { a.up = false; a.bad = 'Too large (50 MB at most)'; renderAtt(); return; }
      previewOf(file).then(function (pv) {
        if (pv) { a.thumb = pv.thumb; renderAtt(); }
        return readB64(file).then(function (b64) {
          return post('/api/voice/attach', { name: file.name, type: file.type, data: b64, preview: pv ? pv.b64 : undefined, from: 'web' });
        });
      }).then(function (r) {
        if (S !== s) return;
        a.up = false;
        if (r && r.ok) { a.id = r.id; a.kind = r.kind; a.name = r.name; }
        else a.bad = (r && /404|not found/i.test(r.error || '')) ? 'This server does not take files for Cru yet: update it' : ((r && r.error) || 'Could not attach');
        renderAtt();
      }).catch(function (e) { if (S === s) { a.up = false; a.bad = e.message || 'Could not attach'; renderAtt(); } });
    });
    S.quietSince = Date.now();
  }
  function renderAtt() {
    if (!S) return;
    var box = S.ov.querySelector('.cv-att'), list = S.att || [];
    box.hidden = !list.length;
    box.innerHTML = list.map(function (a, i) {
      return '<span class="cv-chip' + (a.up ? ' up' : '') + (a.bad ? ' bad' : '') + '" title="' + esc(a.bad || a.name) + '">' + (a.thumb ? '<img src="' + a.thumb + '" alt="">' : '') + '<span>' + esc(a.bad ? a.name + ': ' + a.bad : a.name) + '</span><button data-i="' + i + '" aria-label="Remove">\u00d7</button></span>';
    }).join('');
    Array.prototype.forEach.call(box.querySelectorAll('button'), function (b) { b.onclick = function (e) { e.stopPropagation(); S.att.splice(Number(b.getAttribute('data-i')), 1); renderAtt(); }; });
  }
  /** A message of the owner's in the chat view: attached files and dropped items as chips. */
  function saidHtml(t) {
    return String(t || '').split('\n').map(function (l) {
      var m = l.match(/^\[The owner attached: "([^"]*)" \(([a-z]+)[^)]*\)/);
      if (m) return '<span class="att">' + esc(m[1]) + ' · ' + esc(m[2]) + '</span>';
      var r = l.match(/^\[The owner dropped in: (.*)\]$/);
      if (r) return '<span class="att">' + esc(r[1]) + '</span>';
      return esc(l);
    }).join('<br>');
  }

  function ask(t, keep, typed, hidden) {
    S.waitSaid = false; if (S.kid) loadKidWait();
    if (!hidden) { S.reportOnly = false; S.reportCloseAt = 0; }
    // Sent: the chat view goes to it, wherever it was scrolled (renderChat, from render below).
    if (!hidden) S.chatStick = true;
    S.phase = 'think'; S.asked = hidden ? '' : t; S.heard = hidden ? '' : t; if (!keep) { S.reply = ''; S.detail = ''; } S.question = ''; S.notice = ''; S.quietSince = Date.now();
    var turn = ++S.turn;
    // A soft "sent", then, while Cru thinks, a calm pulse every few seconds (no spoken "let me check").
    tone('sent');
    clearTimeout(S.thinkTimer);
    var s0 = S, pulse = function () { if (S !== s0 || S.phase !== 'think' || S.turn !== turn) return; tone('think'); S.thinkTimer = setTimeout(pulse, 4000); };
    S.thinkTimer = setTimeout(pulse, 1400);
    render();
    var att = hidden ? { attachments: [], refs: [] } : takeAtt();
    if (att.attachments.length || att.refs.length) { S.asked = att.label + (t ? '\n' + t : ''); S.heard = S.asked; render(); }
    // What he said or typed is in the conversation at once, not only once answered (10 Nov: a typed
    // "Yes" never appeared). The id makes a resend of this request get this turn's answer (voice/turns.js).
    var requestId = 'w' + Date.now().toString(36) + Math.random().toString(36).slice(2, 10), mine = null;
    if (!hidden && S.asked) { mine = { at: Date.now(), said: S.asked, speak: '', show: '', live: true, key: requestId }; (S.msgs = S.msgs || []).push(mine); if (S.chat) renderChat(true); }
    post('/api/voice/turn', { text: t, where: 'web' + (S.opts.host ? ' (' + S.opts.host + ')' : ''), locked: false, typed: !!typed, attachments: att.attachments, refs: att.refs, requestId: requestId }).then(function (r) {
      if (!S) return;
      // Cancelled: never said or shown as an error, just what it had already done; the message goes.
      if (r && r.cancelled) { if (mine && S.msgs) { S.msgs = S.msgs.filter(function (m) { return m !== mine; }); renderChat(); } if (S.phase === 'listen' && !S.heard) { S.notice = cancelledNotice(r.acted); render(); } return; }
      // Taken back meanwhile (or a newer question): not this answer's place.
      if (S.turn !== turn || S.phase !== 'think') return;
      var ok = r && r.ok;
      S.reply = ok ? (r.speak || '') : ''; S.detail = ok ? (r.show || '') : ''; S.notice = ok ? '' : (r && r.error || 'Something went wrong');
      // The kid screen never shows a grown-up's error.
      if (!ok && S.kid) S.notice = 'Oops! I need a grown-up to help me. Let\'s try again soon!';
      S.question = ok && r.pending && r.pending.length ? r.pending[0].question : '';
      // The owner is done: close once the goodbye has been said.
      S.closing = !!(ok && r.close);
      if (ok && r.phoneActions && r.phoneActions.length) S.notice = 'Background listening is for the phone: turn it on from the Crundi app.';
      // Another voice ("Only act on my voice"): answered, but nothing was changed.
      if (ok && r.speaker && r.speaker.unrecognised) S.notice = 'Cru could not recognise who was speaking, so nothing was changed.';
      // Cru may have just turned itself up or down: the reply says so in the new volume.
      if (ok && typeof r.volume === 'number' && S.settings) { S.settings.volume = r.volume; applyVolume(); }
      // Kid Cru's mood: how lively its voice is.
      S.expr = ok && typeof r.expressivity === 'number' ? r.expressivity : null;
      var say = ok ? S.reply : S.notice;
      if (ok) { S.lastChatAsked = S.asked; if (mine && S.msgs && S.msgs.indexOf(mine) >= 0) { mine.speak = S.reply; mine.show = S.detail; if (S.chat) renderChat(true); } else chatAdd(S.asked, S.reply, S.detail); }
      api('/api/voice/settings').then(function (s) { if (S && s && s.ok !== false) { var v = S.settings && S.settings.ttsVoice, r = S.settings && S.settings.speechRate; S.settings = s; applyVolume(); refreshData(); if (s.ttsVoice !== v || s.speechRate !== r || s.deepgramLimitReached) loadCues(); } });
      // The owner spoke again while Crundi thought: that goes now, the answer stays on screen unsaid.
      if (S.pending) { var p = S.pending; S.pending = ''; S.closing = false; stopCue(); ask(p, true); render(); return; }
      // Typed: the answer is read, not said (no speech is made for it, so Cru works
      // without Deepgram); the speaker on its bubble says it. Kid mode always talks.
      if (say && !(typed && !S.kid)) speak(say); else { stopCue(); afterSpeech(); }
      render();
    }).catch(function (e) { if (!S || S.turn !== turn || S.phase !== 'think') return; S.notice = e.message || 'Could not reach the server'; afterSpeech(); render(); });
  }

  // ─── taking back what was said ───
  /** The × by his words: until the answer comes, while there are words to take back. */
  function canCancel(s) { return !!(s && s.cancellable && s.heard && (s.phase === 'listen' || s.phase === 'think')); }

  function cancelWords() { if (canCancel(S)) { takeBack(false); render(); } }

  /**
   * Take back what the owner said. Listening: the words go (the stream is
   * told, unless it was the server that dropped them). Thinking: the turn is
   * cancelled on the server, with what he said meanwhile; its cue stops and
   * its answer is neither said nor shown.
   */
  function takeBack(fromServer) {
    if (!S) return;
    if (!fromServer) wsSend({ type: 'discard' });
    if (S.phase === 'listen' || S.phase === 'start') { S.heard = ''; S.quietSince = Date.now(); render(); return; }
    if (S.phase !== 'think') return;
    S.turn++; S.phase = 'listen'; S.heard = ''; S.pending = ''; S.notice = 'Cancelled.'; S.quietSince = Date.now();
    if (S.msgs && S.msgs.length) { var lastM = S.msgs[S.msgs.length - 1]; if (lastM.live && lastM.key && !lastM.speak && !lastM.show) S.msgs.pop(); }
    stopCue();
    var s = S;
    post('/api/voice/cancel', {}).then(function (r) {
      if (S !== s) return;
      if (r && r.ok === false && /HTTP 404/.test(r.error || '')) { s.cancellable = false; return; }
      if (r && r.acted && r.acted.length && s.phase === 'listen' && !s.heard) { s.notice = cancelledNotice(r.acted); render(); }
    }).catch(function () { /* the answer will say */ });
    render();
  }

  function afterSpeech() {
    if (!S) return;
    if (S.closing) { close(); return; }
    // News that came while Cru was busy.
    if (S.reports.length && !S.pending) { var q = S.reports.shift(); S.speaking = false; S.saidUntil = Date.now(); tell(q.speak, q.show); return; }
    S.speaking = false; S.saidUntil = Date.now(); S.unknownVoice = false;
    S.phase = 'listen'; S.heard = ''; S.quietSince = Date.now();
    if (S.reportOnly && !S.reports.length) S.reportCloseAt = Date.now() + REPORT_LINGER_MS;
    if (S.live) { said(); releaseLater(); }
    else setTimeout(function () { if (S && (S.phase === 'listen' || S.phase === 'think')) { S.streaming = true; try { S.ws && S.ws.send(JSON.stringify({ type: 'resume' })); } catch (e) { /* gone */ } } }, PAUSE_AFTER_SPEECH_MS);
    if (S.pending) { var p = S.pending; S.pending = ''; ask(p); }
    render();
  }

  // ─── the local gate: Crundi's voice never goes to the server ───
  /** Crundi starts to talk (an answer or a cue): the mic stays local until the owner talks or it is done. */
  function hold() {
    if (!S) return;
    clearTimeout(S.releaseTimer);
    if (!S.held) { S.held = true; S.gate.start(); S.preroll.clear(); }
    S.streaming = false;
    wsSend({ type: 'pause' });
  }

  /** Back to the server: [withPreroll] the owner cut in, so what he said just before goes first. */
  function release(withPreroll) {
    if (!S) return;
    clearTimeout(S.releaseTimer);
    // From a little before he got loud (the start of his first word), never more than the pre-roll.
    var chunks = S.preroll.drain(Math.min(PREROLL_MS, S.gate.loudMs + LEAD_MS));
    S.held = false; S.streaming = true;
    wsSend({ type: 'resume' });
    if (withPreroll && S.ws && S.ws.readyState === 1) chunks.forEach(function (c) { try { S.ws.send(c); } catch (e) { /* gone */ } });
  }

  /** Crundi is done: the gate stays a moment (the room's echo), then the mic goes back to the server. */
  function releaseLater() {
    if (!S) return;
    clearTimeout(S.releaseTimer);
    S.releaseTimer = setTimeout(function () { if (S && S.held && !S.speaking && !S.cue) release(false); }, PAUSE_AFTER_SPEECH_MS);
  }

  /** The gate heard the owner: stop Crundi now, here, and send what he is saying from its start. */
  function ownerSpoke() {
    if (!S) return;
    if (S.phase === 'speak') { stopSpeaking(S); S.saidUntil = Date.now(); S.phase = 'listen'; S.heard = ''; S.quietSince = Date.now(); said(); }
    else if (S.cue) stopCue();
    release(true);
    render();
  }

  function keepVoice(pcm) {
    var r = S.ring, at = S.ringAt;
    for (var i = 0; i < pcm.length; i++) { r[at] = pcm[i] / 32768; at = (at + 1) % r.length; }
    S.ringAt = at;
  }
  /**
   * Is the owner speaking, not making a noise? The speech detector, once it
   * has run a moment: at least SPEECH_MS of unbroken speech. Without it (the
   * weights not loaded yet, or not to be had) the old voicing check stands in.
   */
  function heardSpeech() {
    if (S.vad && S.vad.frames >= VAD_WARM) return S.vad.talking;
    return recentVoicing() >= 0.3;
  }
  function recentVoicing() {
    var r = S.ring, n = r.length, c = new Float32Array(n);
    for (var i = 0; i < n; i++) c[i] = r[(S.ringAt + i) % n];
    return voicedShare(c);
  }

  /** Where Crundi's voice is played into: an analyser (its level), then the loopback or the speakers. */
  /** How loud Cru is (Settings, or Cru's own set_volume), 0.1 to 1. */
  function volume() {
    var v = S && S.settings && Number(S.settings.volume);
    return v >= 10 && v <= 100 ? v / 100 : 1;
  }
  function applyVolume() {
    if (S && S.outGain) { try { S.outGain.gain.value = volume(); } catch (e) { /* closing */ } }
    sendVolume();
  }

  function outNode() {
    if (S.out || !S.ctx) return S.outGain || S.out;
    // Cru's voice and cues -> its volume -> the level meter (the echo gate
    // measures what really plays) -> the speakers or the loopback.
    var ctx = S.ctx, an = ctx.createAnalyser(); an.fftSize = 512; an.connect(ctx.destination);
    var g = ctx.createGain(); g.gain.value = volume(); g.connect(an);
    S.out = an; S.outGain = g; S.outBuf = new Float32Array(an.fftSize);
    var s = S;
    makeLoopback(ctx, window, 3000).then(function (lb) {
      if (S !== s) { lb.close(); return; }
      an.disconnect(); an.connect(lb.dest); s.loop = lb; s.route = 'loopback';
      // Lost later: straight to the speakers again.
      lb.onlost = function () { if (S !== s || s.route !== 'loopback') return; try { an.disconnect(); } catch (e) { /* none */ } an.connect(ctx.destination); s.route = 'direct'; };
    }).catch(function () { /* played directly: the gate does the work */ });
    return g;
  }

  /** Crundi's level now, as the microphone's scale. */
  function playLevel() {
    if (!S.out) return 0;
    S.out.getFloatTimeDomainData(S.outBuf);
    var sum = 0, b = S.outBuf;
    for (var i = 0; i < b.length; i++) sum += b[i] * b[i];
    return Math.min(1, Math.sqrt(Math.sqrt(sum / b.length)) * 1.4);
  }

  // ─── speaking ───
  function wsSend(m) { try { if (S && S.ws && S.ws.readyState === 1) S.ws.send(JSON.stringify(m)); } catch (e) { /* gone */ } }
  /** Crundi stopped talking (live stream): the server keeps dropping its words a moment longer. */
  function said() { wsSend({ type: 'said' }); }

  function speak(text, opts) {
    opts = opts || {};
    if (S.video.playing) videoPause();
    S.speakOpts = opts;
    // A line that never reports its end (a device with no voice of its own, a stream that stalls) must not
    // leave the screen on "Speaking": past about twice its expected length, listening carries on (tick()).
    S.speakDeadline = Date.now() + (opts.reflex ? 2500 : 6000) + String(text || '').length * 140;
    S.phase = 'speak'; S.speaking = true; S.lastSaid = text; S.loudFor = 0; S.spokeFor = 0; S.hintAt = 0; S.unknownVoice = false;
    // The reply scrolls along as it is said (about 14 characters a second at normal speed).
    S.sayFrom = Date.now(); S.sayFor = Math.max(1500, String(text || '').length / (14 * ((S.settings && S.settings.speechRate) || 1)) * 1000); S.ownScroll = false;
    // Nothing goes to the server while Crundi talks: the gate here listens for the owner.
    // The server is still told what it says (its transcript filter is the second net).
    // An older server is paused, as before.
    if (S.live) { wsSend({ type: 'saying', text: text }); hold(); }
    else { S.streaming = false; wsSend({ type: 'pause' }); }
    // A cue nearly done is let finish; otherwise the answer takes over now.
    if (!S.cue || !S.cue.node || cueLeft() > CUE_FINISH_MS) stopCue();
    var voice = S.settings && S.settings.ttsVoice;
    if (voice) streamSpeech(text).catch(function () { browserSpeech(text); });
    else browserSpeech(text);
  }

  function streamSpeech(text) {
    var token = S.speakToken = {};
    var ctx = S.ctx || (S.ctx = new (window.AudioContext || window.webkitAudioContext)());
    var an = outNode();
    // The level meter is S.out, behind the volume node outNode() hands back (the voice plays into that).
    // Taking the volume node for the meter threw on every frame while Cru talked: the mic froze (10 Nov).
    S.outAnalyser = S.out && S.out.getByteTimeDomainData ? S.out : null;
    var at = Math.max(ctx.currentTime + 0.05, S.cue && S.cue.ends ? S.cue.ends : 0), rate = 24000, carry = null, any = false;
    // Kid mode: how lively (the mood of Cru's reply, or of an instant line); a line not ready in 400 ms is said by the device instead.
    var o = S.speakOpts || {}, ex = typeof o.expressivity === 'number' ? o.expressivity : (typeof S.nextExpr === 'number' ? S.nextExpr : (typeof S.expr === 'number' ? S.expr : null));
    S.nextExpr = null;
    var late = null, s0 = S;
    var hasDeviceVoice = !!(window.speechSynthesis && speechSynthesis.getVoices && speechSynthesis.getVoices().length);
    if (o.reflex && hasDeviceVoice) late = setTimeout(function () { if (S === s0 && S.speakToken === token && !any) { S.speakToken = {}; browserSpeech(text); } }, 1500);
    return S.opts.apiFetch('/api/voice/speak?stream=1', { method: 'POST', headers: { 'Content-Type': 'application/json', Accept: 'audio/L16' }, body: JSON.stringify(ex == null ? { text: text } : { text: text, expressivity: ex }) }).then(function (res) {
      clearTimeout(late);
      if (!res.ok || !res.body || !/L16/i.test(res.headers.get('Content-Type') || '')) throw new Error('no stream');
      var reader = res.body.getReader();
      function pump() {
        return reader.read().then(function (r) {
          if (S && S.speakToken !== token) { try { reader.cancel(); } catch (e) { /* done */ } return; }
          if (r.done) { var ends = Math.max(0, (at - ctx.currentTime) * 1000); setTimeout(function () { if (S && S.speakToken === token) { S.outAnalyser = null; afterSpeech(); } }, ends + 60); return; }
          var bytes = r.value;
          if (carry) { var m = new Uint8Array(carry.length + bytes.length); m.set(carry); m.set(bytes, carry.length); bytes = m; carry = null; }
          if (bytes.length % 2) { carry = bytes.slice(bytes.length - 1); bytes = bytes.slice(0, bytes.length - 1); }
          var n = bytes.length / 2; if (!n) return pump();
          var view = new DataView(bytes.buffer, bytes.byteOffset, bytes.length);
          var buf = ctx.createBuffer(1, n, rate), ch = buf.getChannelData(0);
          for (var i = 0; i < n; i++) ch[i] = view.getInt16(i * 2, true) / 32768;
          var node = ctx.createBufferSource(); node.buffer = buf; node.connect(an);
          at = Math.max(at, ctx.currentTime + 0.02); node.start(at); at += buf.duration; any = true;
          (S.nodes = S.nodes || []).push(node);
          return pump();
        });
      }
      return pump();
    }).catch(function (e) { if (any) return; throw e; });
  }

  function browserSpeech(text) {
    if (!window.speechSynthesis) { afterSpeech(); return; }
    var u = new SpeechSynthesisUtterance(text);
    var vs = speechSynthesis.getVoices();
    u.voice = vs.find(function (v) { return /en-GB/i.test(v.lang) && /male|daniel|george|arthur|ryan/i.test(v.name); }) || vs.find(function (v) { return /en-GB/i.test(v.lang); }) || null;
    u.lang = 'en-GB';
    u.rate = (S && S.settings && S.settings.speechRate) || 1;   // the talking speed, as Deepgram's
    u.volume = volume();
    u.onboundary = function () { if (S) S.voiceBump = 0.6 + Math.random() * 0.35; };
    u.onend = function () { if (S && S.speaking) afterSpeech(); };
    u.onerror = function () { if (S && S.speaking) afterSpeech(); };
    stopCue();
    // The browser's voice can't go through the loopback: the gate has only the mic to go by.
    if (S) S.unknownVoice = true;
    speechSynthesis.cancel(); speechSynthesis.speak(u);
  }

  /** The browser's own voice for [text] (British where it has one). */
  function deviceUtterance(text) {
    var u = new SpeechSynthesisUtterance(text);
    var vs = speechSynthesis.getVoices();
    u.voice = vs.find(function (v) { return /en-GB/i.test(v.lang) && /male|daniel|george|arthur|ryan/i.test(v.name); }) || vs.find(function (v) { return /en-GB/i.test(v.lang); }) || null;
    u.lang = 'en-GB';
    u.rate = (S && S.settings && S.settings.speechRate) || 1;
    u.volume = volume();
    return u;
  }

  // ─── the "one moment, Sir" cue ───
  /**
   * The cues for the voice in use: the server's clips (decoded now, so one
   * plays with no network wait) or, with the browser's own voice, the phrases
   * to say. An older server has no list: the browser's voice says ours, a
   * Deepgram voice plays none (it would not match).
   */
  /**
   * Cru's tones (9 Nov, replacing the spoken "one moment" cues): soft sine
   * notes through Cru's own volume. 'listen': two notes up (awake and
   * listening); 'sent': two notes down (what you said has gone); 'think': a
   * slow, quiet bloom while Cru works. The app plays the same (Tones.kt).
   */
  var TONES = {
    listen: [[659.25, 0, 0.11, 0.16], [880, 0.09, 0.16, 0.16]],
    sent: [[880, 0, 0.08, 0.13], [659.25, 0.07, 0.14, 0.13]],
    think: [[523.25, 0, 0.9, 0.07, 0.18], [783.99, 0, 0.9, 0.05, 0.18]],
  };
  var toneCtx = null;
  /** Cru's tone [kind] (see TONES); with no voice screen open (a chat in voice mode) on its own context, at [vol] 0..1. */
  function tone(kind, vol) {
    var notes = TONES[kind]; if (!notes) return;
    try {
      var ctx, out;
      if (S) { ctx = S.ctx || (S.ctx = new (window.AudioContext || window.webkitAudioContext)()); out = outNode() || ctx.destination; }
      else {
        ctx = toneCtx || (toneCtx = new (window.AudioContext || window.webkitAudioContext)());
        if (ctx.state === 'suspended' && ctx.resume) ctx.resume();
        out = ctx.createGain(); out.gain.value = vol >= 0 && vol <= 1 ? vol : 1; out.connect(ctx.destination);
      }
      var t0 = ctx.currentTime + 0.02;
      notes.forEach(function (n) {
        var o = ctx.createOscillator(), g = ctx.createGain(), at = t0 + n[1], dur = n[2], peak = n[3], rise = n[4] || 0.012;
        o.type = 'sine'; o.frequency.value = n[0];
        g.gain.setValueAtTime(0.0001, at);
        g.gain.exponentialRampToValueAtTime(peak, at + rise);
        g.gain.exponentialRampToValueAtTime(0.0001, at + dur);
        o.connect(g); g.connect(out); o.start(at); o.stop(at + dur + 0.05);
      });
    } catch (e) { /* no audio here */ }
  }

  // The spoken cues are retired: tones say the same without words.
  function loadCues() { if (S) S.cues = []; }
  function loadCuesSpoken() {
    if (!S) return;
    var s = S, gen = s.cueGen = (s.cueGen || 0) + 1;
    var deviceVoice = !(s.settings && s.settings.ttsVoice) || (s.settings && s.settings.deepgramLimitReached);
    api('/api/voice/cues').then(function (r) {
      if (S !== s || s.cueGen !== gen) return;
      if (!r || r.ok === false || !Array.isArray(r.cues)) { s.cueDevice = deviceVoice; s.cues = deviceVoice ? CUE_TEXTS.map(function (t) { return { text: t }; }) : []; return; }
      if (r.device) { s.cueDevice = true; s.cues = r.cues.map(function (c) { return { text: c.text }; }); return; }
      s.cueDevice = false; s.cues = [];
      r.cues.filter(function (c) { return c.url; }).forEach(function (c) {
        s.opts.apiFetch(c.url).then(function (res) { if (!res.ok) throw new Error('HTTP ' + res.status); return res.arrayBuffer(); })
          .then(function (ab) { return new Promise(function (ok, no) { s.ctx.decodeAudioData(ab, ok, no); }); })
          .then(function (buf) { if (S === s && s.cueGen === gen) s.cues.push({ text: c.text, buf: buf }); })
          .catch(function () { /* that one is left out */ });
      });
      // Made for the first time just now: ask again in a while.
      if (r.cues.some(function (c) { return !c.url; })) setTimeout(function () { if (S === s && s.cueGen === gen) loadCues(); }, 20000);
    }).catch(function () { /* no cues */ });
  }

  /**
   * Kid mode: the child talked while Cru was busy. Thinking: a short "wait a
   * moment" line, at most once per answer (picked at random, never twice the
   * same in a row). Talking: Cru finishes; a wait sign shows a moment.
   */
  var KID_WAIT_LINES = ['Wait a moment!', 'Hold on, I\'m thinking!', 'One second, friend!', 'Just a moment, let me think!', 'Ooh, wait for me!'];
  function kidWait() {
    if (!S || !S.kid) return;
    S.waitAt = Date.now();
    setTimeout(render, 2600);
    if (S.phase === 'think' && !S.waitSaid && S.kidWaitCues && S.kidWaitCues.length) { S.waitSaid = true; playCue(S.kidWaitCues, 'kidWaitLast'); }
    render();
  }
  /** The wait lines, made once in kid Cru's voice (instant later, like the cues). */
  function loadKidWait() {
    if (!S || !S.kid || S.kidWaitCues) return;
    var s = S; s.kidWaitCues = [];
    var deviceVoice = !(s.settings && s.settings.ttsVoice) || (s.settings && s.settings.deepgramLimitReached);
    if (deviceVoice) { s.kidWaitDevice = true; s.kidWaitCues = KID_WAIT_LINES.map(function (t) { return { text: t }; }); return; }
    KID_WAIT_LINES.forEach(function (t) {
      S.opts.apiFetch('/api/voice/speak?stream=1', { method: 'POST', headers: { 'Content-Type': 'application/json', Accept: 'audio/L16' }, body: JSON.stringify({ text: t, expressivity: 1 }) })
        .then(function (res) { if (!res.ok || !/L16/i.test(res.headers.get('Content-Type') || '')) throw new Error('no audio'); return res.arrayBuffer(); })
        .then(function (ab) {
          if (S !== s) return;
          var ctx = s.ctx || (s.ctx = new (window.AudioContext || window.webkitAudioContext)());
          var n = Math.floor(ab.byteLength / 2); if (!n) return;
          var view = new DataView(ab), buf = ctx.createBuffer(1, n, 24000), ch = buf.getChannelData(0);
          for (var i = 0; i < n; i++) ch[i] = view.getInt16(i * 2, true) / 32768;
          s.kidWaitCues.push({ text: t, buf: buf });
        }).catch(function () { /* that line is left out */ });
    });
  }

  function playCue(list, lastKey) {
    list = list || (S && S.cues); lastKey = lastKey || 'cueLast';
    if (!S || !list || !list.length) return;
    var i = pickCue(list.length, S[lastKey], Math.random), c = list[i];
    if (!c) return;
    S[lastKey] = i;
    stopCue();
    var s = S, cue = { text: c.text, ends: 0 };
    var done = function () { if (S === s && s.cue === cue) { s.cue = null; if (s.live && !s.speaking) { said(); s.unknownVoice = false; releaseLater(); } } };
    if (c.buf && s.ctx) {
      var node = s.ctx.createBufferSource(); node.buffer = c.buf; node.connect(outNode() || s.ctx.destination);
      cue.node = node; cue.ends = s.ctx.currentTime + 0.01 + c.buf.duration;
      node.onended = done;
      node.start(s.ctx.currentTime + 0.01);
    } else if ((s.cueDevice || (list === s.kidWaitCues && s.kidWaitDevice)) && window.speechSynthesis) {
      var u = deviceUtterance(c.text);
      u.onend = done; u.onerror = done;
      cue.utter = u;
      s.unknownVoice = true;
      speechSynthesis.cancel(); speechSynthesis.speak(u);
    } else return;
    s.cue = cue;
    // Its own words coming back are not the owner's: and they are not sent at all.
    s.lastSaid = c.text; s.saidUntil = Date.now() + (c.buf ? c.buf.duration * 1000 : 1500);
    if (s.live) { wsSend({ type: 'saying', text: c.text }); hold(); }
  }

  /** How long the cue has still to play, ms (0 when none, or the browser's voice: unknown). */
  function cueLeft() { return S && S.cue && S.cue.node && S.ctx ? Math.max(0, (S.cue.ends - S.ctx.currentTime) * 1000) : 0; }

  function stopCue(s) {
    s = s || S;
    if (!s || !s.cue) return;
    var c = s.cue; s.cue = null;
    try { if (c.node) c.node.stop(); } catch (e) { /* done */ }
    try { if (c.utter && window.speechSynthesis) speechSynthesis.cancel(); } catch (e) { /* none */ }
    // Cut short while thinking: the server may stop dropping its words, and the mic goes back.
    if (s.live && !s.speaking) {
      try { if (s.ws && s.ws.readyState === 1) s.ws.send(JSON.stringify({ type: 'said' })); } catch (e) { /* gone */ }
      if (S === s) { s.unknownVoice = false; releaseLater(); }
    }
  }

  function stopSpeaking(s) {
    s.speakToken = {};
    (s.nodes || []).forEach(function (n) { try { n.stop(); } catch (e) { /* done */ } }); s.nodes = [];
    s.outAnalyser = null;
    try { window.speechSynthesis && speechSynthesis.cancel(); } catch (e) { /* none */ }
    stopCue(s);
    s.speaking = false;
  }

  function tapMic() {
    if (!S) return;
    S.reportOnly = false; S.reportCloseAt = 0;
    if (S.phase === 'speak') { bargeIn(); return; }
    if (S.phase === 'ended' && S.micOff) { wakeMic(); return; }
    if (S.phase === 'ended' && !S.notice) { S.phase = 'listen'; S.quietSince = Date.now(); render(); }
  }

  /**
   * Asleep: the microphone is let go (nothing is streamed, and on the PC its
   * own "Hey Cru" listener takes the microphone and wakes this screen). The
   * voice stays, so a report can still be said.
   */
  function sleepMic() {
    if (!S || S.micOff) return;
    S.micOff = true;
    try { if (S.ws) { S.ws.send(JSON.stringify({ type: 'stop' })); S.ws.close(); } } catch (e) { /* gone */ }
    S.ws = null; S.live = false; S.streaming = false;
    try { if (S.src) S.src.disconnect(); if (S.proc) S.proc.disconnect(); } catch (e) { /* gone */ }
    try { if (S.stream) S.stream.getTracks().forEach(function (t) { t.stop(); }); } catch (e) { /* gone */ }
    S.stream = null; S.src = null; S.proc = null;
  }

  /** "Hey Cru" on the PC, a tap, the shortcut: listening again, on the same screen. */
  function wakeMic() {
    if (!S || !S.micOff) return;
    S.micOff = false;
    try { if (S.loop) S.loop.close(); } catch (e) { /* gone */ }
    try { if (S.ctx) S.ctx.close(); } catch (e) { /* gone */ }
    S.loop = null; S.ctx = null;
    S.phase = 'start'; S.notice = ''; S.heard = ''; S.streaming = true; S.quietSince = Date.now();
    render();
    openMic();
  }

  function bargeIn() {
    cutIn();
    render();
  }

  /** Crundi stops talking now and listens: the owner talked over it (or tapped). */
  function cutIn() {
    stopSpeaking(S);
    S.saidUntil = Date.now(); S.phase = 'listen'; S.heard = ''; S.quietSince = Date.now(); S.streaming = true; S.hintAt = 0;
    // Tapped: straight back to the server. Not the pre-roll: that is Cru's own voice.
    if (S.live) { said(); release(false); } else wsSend({ type: 'resume' });
  }

  /** The same echo-aware rule as the phone's BargeInDetector. */
  function onLevel(level, ms) {
    if (!S.speaking) { S.noise = Math.min(0.2, Math.max(0.002, S.noise * 0.95 + level * 0.05)); return; }
    S.spokeFor += ms;
    var v = S.voice; S.recent.shift(); S.recent.push(v);
    var peak = Math.max.apply(null, S.recent);
    if (S.spokeFor <= 700 && peak > 0.05) S.echo = Math.min(3, Math.max(0.05, Math.max(S.echo * 0.85, level / peak)));
    var gate = S.outAnalyser ? Math.max(0.03, S.noise * 3.2, S.echo * peak * 2.2 + 0.04) : Math.max(0.45, S.noise * 3.2);
    if (S.spokeFor <= 700) { S.loudFor = 0; return; }
    S.loudFor = level >= gate ? S.loudFor + ms : 0;
    // Speech, by the detector (its ring of voice is only kept for the live gate, so without it: the level alone, as before).
    if (S.loudFor < 400 || (S.vad && S.vad.frames >= VAD_WARM && !S.vad.talking)) return;
    // With the stream on, a voice that loud only lowers the bar for his words
    // (a cough or a door is not an interruption); an older server has only this.
    if (S.live) { S.hintAt = Date.now(); S.loudFor = 0; } else bargeIn();
  }

  function tick() {
    if (!S) return;
    // Stuck on "Speaking" with nothing playing: carry on listening.
    // (The device's own voice can report "speaking" for ever, on Linux in particular: past the deadline plus 3 s it is stopped anyway.)
    if (S.phase === 'speak' && S.speakDeadline && Date.now() > S.speakDeadline && (!(window.speechSynthesis && speechSynthesis.speaking) || Date.now() > S.speakDeadline + 3000)) {
      var playing = false;
      try { playing = !!(S.ctx && (S.nodes || []).length && S.outAnalyser); } catch (e) { playing = false; }
      if (!playing || Date.now() > S.speakDeadline + 8000) { S.speakDeadline = 0; stopSpeaking(S); afterSpeech(); }
    }
    // After a short silence (Settings, default a minute) the voice screen closes itself; "Hey Cru" opens it again.
    var idleMs = ((S.settings && S.settings.screenCloseSeconds) || 60) * 1000;
    // Kid mode stays, listening; a panel on screen keeps the screen open (the mic still sleeps).
    if (S.kid) return;
    // Opened by Cru's own news and said: closed again, unless the owner started talking.
    if (S.reportOnly && S.reportCloseAt && Date.now() > S.reportCloseAt && S.phase === 'listen' && !S.heard && !S.pending) { close(); return; }
    if ((S.phase === 'ended' || S.phase === 'listen') && !S.pending && !S.screen && Date.now() - S.quietSince > idleMs) { close(); return; }
    // (Typing: there is no microphone to put to sleep; its socket brings Cru's news.)
    if (S.phase === 'listen' && !S.kb && !S.pending && Date.now() - S.quietSince > SILENCE_MS) { S.phase = 'ended'; sleepMic(); render(); }
  }


  // ─── The live screen (capability 'voice-screen'): Cru's own panel ───
  function loadScreen() {
    var s = S;
    api('/api/voice/screen').then(function (r) { if (S === s && r && r.ok !== false && r.screen !== undefined) showScreen(r.screen); }).catch(function () { /* an older server: no panel */ });
  }
  var TONE_COLOURS = { ok: C.green, warn: C.yellow, bad: C.red, info: C.sky, muted: C.muted };
  /** Cru's blocks, drawn here in Crundi's style; every text escaped. */
  function blocksHtml(blocks) {
    return (blocks || []).map(function (b) {
      if (b.type === 'heading') return '<div class="cv-b-h">' + esc(b.text) + '</div>';
      if (b.type === 'text') return '<div class="cv-b-t">' + esc(b.text) + '</div>';
      if (b.type === 'list') { var tag = b.ordered ? 'ol' : 'ul'; return '<' + tag + ' class="cv-b-l">' + (b.items || []).map(function (i) { return '<li>' + esc(i) + '</li>'; }).join('') + '</' + tag + '>'; }
      if (b.type === 'checklist') return '<ul class="cv-b-c">' + (b.items || []).map(function (i) { return '<li' + (i.done ? ' class="d"' : '') + '><i></i><span>' + esc(i.text) + '</span></li>'; }).join('') + '</ul>';
      if (b.type === 'table') return '<table class="cv-b-tb">' + (b.headers ? '<tr>' + b.headers.map(function (h) { return '<th>' + esc(h) + '</th>'; }).join('') + '</tr>' : '') + (b.rows || []).map(function (r) { return '<tr>' + r.map(function (c) { return '<td>' + esc(c) + '</td>'; }).join('') + '</tr>'; }).join('') + '</table>';
      if (b.type === 'progress') return '<div class="cv-b-p"><span>' + esc(b.label || '') + ' ' + esc(b.value) + '%</span><div><b style="width:' + Math.max(0, Math.min(100, Number(b.value) || 0)) + '%"></b></div></div>';
      if (b.type === 'image' && /^(https:|data:image\/)/i.test(b.url || '')) return '<img class="cv-b-i" alt="' + esc(b.alt || '') + '" src="' + esc(b.url) + '" referrerpolicy="no-referrer">';
      if (b.type === 'chips') return '<div class="cv-b-ch">' + (b.items || []).map(function (c) { var col = TONE_COLOURS[c.tone] || C.sky; return '<span style="color:' + col + ';border-color:' + hex(col, 0.45) + ';background:' + hex(col, 0.12) + '">' + esc(c.text) + '</span>'; }).join('') + '</div>';
      return '';
    }).join('');
  }
  /** What Cru put up (null: nothing). A video or a page goes in a frame; a page's frame is kept when only its blocks change. */
  function showScreen(scr) {
    if (!S) return;
    var was = S.screen;
    S.screen = scr || null;
    S.ov.classList.toggle('has-scr', !!scr);
    S.ov.classList.toggle('full', !!(scr && scr.full) && !S.kid);
    var body = S.ov.querySelector('.cv-scr-b');
    if (!scr) { body.innerHTML = ''; S.frame = null; S.vidFrame = null; S.video.reset(); S.ov.classList.remove('vid'); render(); return; }
    S.ov.querySelector('.cv-scr-t').textContent = scr.title || '';
    var same = was && scr && was.doc && scr.doc === was.doc && S.frame;
    if (scr.video && scr.video.embed) {
      if (!(was && was.video && was.video.embed === scr.video.embed && S.vidFrame)) {
        body.className = 'cv-scr-b frame'; body.innerHTML = '';
        var v = document.createElement('iframe');
        // youtube-nocookie only, no related videos, the player's API on (to know when it plays).
        var src = scr.video.embed;
        if (!/^https:\/\/www\.youtube-nocookie\.com\/embed\//.test(src)) return;
        v.src = src + (src.indexOf('?') < 0 ? '?' : '&') + 'enablejsapi=1&origin=' + encodeURIComponent(location.origin);
        v.setAttribute('sandbox', 'allow-scripts allow-same-origin allow-presentation');
        v.setAttribute('allow', 'autoplay; encrypted-media');
        v.setAttribute('referrerpolicy', 'strict-origin-when-cross-origin');
        v.title = scr.title || 'Video';
        v.onload = function () { try { v.contentWindow.postMessage(JSON.stringify({ event: 'listening', id: 'cru' }), '*'); } catch (e) { /* gone */ } };
        body.appendChild(v); S.vidFrame = v; S.frame = null;
        S.video.loaded();
      }
    } else if (scr.doc) {
      if (!same) {
        body.className = 'cv-scr-b frame'; body.innerHTML = (scr.blocks && scr.blocks.length ? '' : '');
        var f = document.createElement('iframe');
        // Somebody else's page: an opaque origin, no network (its own CSP), never same-origin.
        f.setAttribute('sandbox', 'allow-scripts');
        f.setAttribute('allow', 'autoplay');
        f.setAttribute('referrerpolicy', 'no-referrer');
        f.title = scr.title || 'Cru';
        f.onload = function () { sendVolume(); };
        f.srcdoc = scr.doc;
        body.appendChild(f); S.frame = f; S.vidFrame = null; S.video.reset();
      }
    } else {
      body.className = 'cv-scr-b'; body.innerHTML = blocksHtml(scr.blocks); S.frame = null; S.vidFrame = null; S.video.reset();
    }
    // A page with blocks too: the blocks above the frame.
    if (scr.doc && scr.blocks && scr.blocks.length && !same) { var d = el('div', '', blocksHtml(scr.blocks)); d.style.padding = '10px 12px'; body.insertBefore(d, body.firstChild); }
    S.ov.classList.toggle('vid', !!S.video.playing);
    render();
  }
  function closeScreen() {
    if (!S || S.kid) return;
    showScreen(null);
    post('/api/voice/screen/close').catch(function () { /* the next push says */ });
  }
  /** The top bar's "Normal view": out of Cru's full UI at once, here and (from the server's push) on every screen; the content stays as the panel. */
  function normalView() {
    if (!S || !S.screen || !S.screen.full) return;
    var scr = {}; for (var k in S.screen) if (k !== 'full') scr[k] = S.screen[k];
    showScreen(scr);
    post('/api/voice/screen/normal').catch(function () { /* the next push says */ });
  }
  /** The game's sounds follow Cru's volume, and are muted while a video plays. */
  function sendVolume() {
    if (!S || !S.frame || !S.frame.contentWindow) return;
    try { S.frame.contentWindow.postMessage({ cru: 'volume', value: volume(), muted: !!S.video.playing }, '*'); } catch (e) { /* gone */ }
  }
  /** Cru tells the game to do something (game_command): next, easier, harder, hint, pause, resume, end. */
  function gameCommand(c) {
    if (!S || !S.frame || !S.frame.contentWindow || !c || !c.action) return;
    try { S.frame.contentWindow.postMessage({ type: 'command', action: String(c.action) }, '*'); } catch (e) { /* gone */ }
  }
  /** Messages from Cru's page or game (only from the frame this screen made) and from the video player. */
  function onFrameMessage(e) {
    if (!S) return;
    if (S.vidFrame && e.source === S.vidFrame.contentWindow) { videoMessage(e.data); return; }
    if (!S.frame || e.source !== S.frame.contentWindow) return;
    var m = e.data || {};
    if (m.cru === 'say') sayLocal(String(m.text || '').slice(0, 300));
    else if (m.cru === 'tell') screenTell(String(m.text || '').slice(0, 300));
    else if (m.cru === 'picture') showPicture(String(m.data || ''), String(m.label || ''));
    else if (m.type === 'game') gameEvent({ event: m.event, item: m.item, detail: m.detail, streak: m.streak });
    else if (m.cru === 'event') gameEvent(m.kind === 'result' ? { event: 'result', skill: m.skill, item: m.item, ok: m.ok, ms: m.ms } : { event: m.kind === 'wrong' ? 'miss' : m.kind === 'level' ? 'level-up' : m.kind, item: m.detail, detail: m.detail });
  }
  /** A game says a word (a letter, a picture's name): now, unless Cru is talking. */
  function sayLocal(t) {
    if (!t || !S || S.phase === 'speak' || S.phase === 'think' || S.video.playing) return;
    if (Date.now() - S.sayAt < 600) return;
    S.sayAt = Date.now();
    speak(t, { reflex: true, expressivity: 1 });
  }
  /** The child shows Cru a picture (the drawing pad's camera): kept on the server, then Cru looks. */
  function showPicture(data, label) {
    if (!S || !S.kid || !/^data:image\/(png|jpeg|webp);base64,/.test(data) || data.length > 2500000) return;
    var s = S;
    post('/api/voice/kid/picture', { data: data, label: label }).then(function (r) {
      if (S !== s || !(r && r.ok)) return;
      // Busy talking or thinking: as soon as Cru is free.
      var go = function (n) { if (S !== s) return; if ((S.phase === 'speak' || S.phase === 'think') && n < 40) { setTimeout(function () { go(n + 1); }, 500); return; } screenTell('[The child tapped "Show Cru" to show you ' + (label || 'their drawing') + ': call look_at_picture, then tell them what you see.]'); };
      go(0);
    }).catch(function () { /* an older server */ });
  }
  /** Something tapped that Cru should hear: answered like something said. */
  function screenTell(t) {
    if (!t || !S || S.phase === 'think') return;
    S.phase = 'think'; render();
    var s = S;
    post('/api/voice/screen/event', { text: t }).then(function (r) {
      if (S !== s) return;
      if (S.phase === 'think') S.phase = 'listen';
      if (r && r.ok && (r.speak || r.show)) { if (typeof r.expressivity === 'number') S.nextExpr = r.expressivity; tell(r.speak || '', r.show || ''); }
      render();
    }).catch(function () { if (S === s && S.phase === 'think') { S.phase = 'listen'; render(); } });
  }
  /** A game event: the play log and levels, and the instant line to say (kid mode). */
  function gameEvent(ev) {
    if (!S || !ev || !ev.event || S.video.playing) return;
    var s = S;
    ev.speaking = S.phase === 'speak' || S.phase === 'think';
    post('/api/voice/kid/play', ev).then(function (r) {
      if (S !== s || !r || !r.reflex || !r.reflex.text) return;
      if (S.phase === 'speak' || S.phase === 'think') return;
      // After the game's own sound.
      setTimeout(function () { if (S === s && S.phase !== 'speak' && S.phase !== 'think') { S.reply = r.reflex.text; speak(r.reflex.text, { reflex: true, expressivity: r.reflex.expressivity }); render(); } }, 250);
    }).catch(function () { /* an older server */ });
  }

  // ─── A video on the live screen: speech does not reach Cru while it plays ───
  /**
   * The YouTube player tells its state (enablejsapi); while it plays, what
   * the microphone hears is the video, so nothing is a turn, no game event or
   * instant line fires, and only a tap on "Talk to Cru" (or a loud, clear
   * "Hey Cru" alone, over the video's level) gets Cru back: the video pauses.
   */
  function createVideoGate() {
    var st = 'none';
    return {
      get playing() { return st === 'playing'; },
      get state() { return st; },
      loaded: function () { st = 'loading'; },
      reset: function () { st = 'none'; },
      /** The player's state: 1 playing, 2 paused, 0 ended. Returns 'resume' when listening should come back. */
      player: function (code) {
        var was = st;
        st = code === 1 ? 'playing' : code === 2 ? 'paused' : code === 0 ? 'ended' : code === 3 ? (was === 'playing' ? 'playing' : 'buffering') : st;
        return was === 'playing' && st !== 'playing' ? 'resume' : st === 'playing' && was !== 'playing' ? 'mute' : '';
      },
      /** While it plays: only "Hey Cru" (or the name) alone, said loudly over it. */
      wake: function (text, micPeak) { return st === 'playing' && isCalling(text) && (micPeak || 0) >= 0.55; },
    };
  }
  function recentMicPeak() { return S && Date.now() - (S.micPeakAt || 0) < 2500 ? S.micPeak || 0 : 0; }
  function videoMessage(data) {
    var d = data;
    if (typeof d === 'string') { try { d = JSON.parse(d); } catch (e) { return; } }
    if (!d || !S) return;
    var code = d.event === 'onStateChange' ? d.info : d.event === 'infoDelivery' && d.info && typeof d.info.playerState === 'number' ? d.info.playerState : null;
    if (code == null) return;
    var act = S.video.player(code);
    S.ov.classList.toggle('vid', S.video.playing);
    if (act === 'mute') { wsSend({ type: 'pause' }); S.streaming = false; S.heard = ''; }
    if (act === 'resume') { S.streaming = true; wsSend({ type: 'resume' }); S.phase = 'listen'; S.quietSince = Date.now(); tone('listen'); }
    render();
  }
  function videoPause() {
    if (!S || !S.vidFrame) return;
    try { S.vidFrame.contentWindow.postMessage(JSON.stringify({ event: 'command', func: 'pauseVideo', args: [] }), '*'); } catch (e) { /* gone */ }
  }
  /** "Talk to Cru" over a video: it pauses, and Cru listens. */
  function talkOverVideo() {
    if (!S) return;
    videoPause();
    S.video.player(2);
    S.ov.classList.remove('vid');
    S.streaming = true; wsSend({ type: 'resume' });
    S.phase = 'listen'; S.heard = ''; S.quietSince = Date.now();
    tone('listen'); render();
  }

  // ─── Kid mode (capability 'voice-kid'): the child's screen, left only with the PIN ───
  var kidHook = null;
  function kidSetup() {
    // The PC app locks its window (kiosk); a browser goes full screen and holds the keys it can.
    try { if (window.api && window.api.kidLock) window.api.kidLock(true); } catch (e) { /* an older app */ }
    S.onFs = function () { if (S && S.kid) S.ov.classList.toggle('nofs', !document.fullscreenElement && !(window.api && window.api.kidLock)); };
    document.addEventListener('fullscreenchange', S.onFs);
    kidFullscreen();
  }
  function kidFullscreen() {
    if (!S || !S.kid || (window.api && window.api.kidLock)) return;
    var d = document.documentElement;
    var p = d.requestFullscreen ? d.requestFullscreen({ navigationUI: 'hide' }) : null;
    if (p && p.then) p.then(function () { try { if (navigator.keyboard && navigator.keyboard.lock) navigator.keyboard.lock(); } catch (e) { /* not here */ } if (S) S.ov.classList.remove('nofs'); }, function () { if (S) S.ov.classList.add('nofs'); });
    else S.ov.classList.add('nofs');
  }
  /** The grown-ups' lock: a PIN pad. */
  function pinPad() {
    if (!S || !S.kid || S.ov.querySelector('.cv-pin')) return;
    var box = el('div', 'cv-pin', '<div class="cv-pin-box"><p>Grown-ups: enter the PIN to leave kid mode</p><input class="cv-pin-in" type="password" inputmode="numeric" autocomplete="off" maxlength="8"><div class="cv-pin-k"></div><div class="cv-pin-e"></div></div>');
    var keys = box.querySelector('.cv-pin-k'), input = box.querySelector('.cv-pin-in');
    ['1', '2', '3', '4', '5', '6', '7', '8', '9', 'Back', '0', 'OK'].forEach(function (k) {
      var b = el('button', '', k === 'Back' ? '&larr;' : k);
      b.onclick = function (e) { e.stopPropagation(); if (k === 'OK') pinTry(); else if (k === 'Back') input.value = input.value.slice(0, -1); else if (input.value.length < 8) input.value += k; };
      keys.appendChild(b);
    });
    box.onclick = function (e) { if (e.target === box) box.remove(); };
    S.ov.appendChild(box);
    setTimeout(function () { try { input.focus(); } catch (e) { /* gone */ } }, 0);
    S.pinTimer = setTimeout(function () { if (box.parentNode) box.remove(); }, 60000);
  }
  function pinTry() {
    var box = S && S.ov.querySelector('.cv-pin');
    if (!box) return;
    var input = box.querySelector('.cv-pin-in'), err = box.querySelector('.cv-pin-e');
    var pin = String(input.value || '').replace(/\D/g, '');
    if (!pin) return;
    post('/api/voice/kid/exit', { pin: pin }).then(function (r) {
      if (r && r.ok) { kidExit(); return; }
      input.value = '';
      err.textContent = (r && r.error) || 'That is not the PIN';
    }).catch(function () { err.textContent = 'Could not reach the server'; });
  }
  /** Kid mode is over (the PIN, or another screen ended it): unlock and close. */
  function kidExit() {
    try { if (window.api && window.api.kidLock) window.api.kidLock(false); } catch (e) { /* an older app */ }
    try { if (navigator.keyboard && navigator.keyboard.unlock) navigator.keyboard.unlock(); } catch (e) { /* none */ }
    try { if (document.fullscreenElement && document.exitFullscreen) document.exitFullscreen(); } catch (e) { /* none */ }
    if (S && S.kid) close('kid-exit');
  }
  /**
   * The page tells the voice screen what kid mode is (at load, and on the
   * server's 'voice-kid' event): on, the kid screen opens over everything;
   * off, it goes.
   */
  function kidMode(state, opts) {
    var on = !!(state && state.on);
    if (on && !(S && S.kid)) { if (S) close(); open(Object.assign({}, opts || {}, { kid: true, how: 'click' })); }
    else if (!on && S && S.kid) kidExit();
  }
  kidHook = function (st) { if (S && S.kid && !st.on) kidExit(); else if (S && S.kid) kidPaused(!!st.paused); };
  /** A grown-up paused the kid screen from another device: a break scene, the microphone let go. */
  function kidPaused(on) {
    if (!S || !S.kid || !!S.paused === !!on) return;
    S.paused = !!on;
    var v = S.ov.querySelector('.cv-break');
    if (on) {
      stopSpeaking(S); videoPause(); sleepMic();
      if (!v) {
        v = el('div', 'cv-break', '<svg width="160" height="120" viewBox="0 0 160 120" aria-hidden="true"><circle cx="80" cy="60" r="36" fill="#facc15"><animate attributeName="r" values="34;38;34" dur="3s" repeatCount="indefinite"/></circle><path d="M66 56q4-4 8 0M86 56q4-4 8 0" stroke="#111" stroke-width="3" fill="none" stroke-linecap="round"/><path d="M68 70q12 8 24 0" stroke="#111" stroke-width="3" fill="none" stroke-linecap="round"/></svg><div>Break time! See you soon.</div>');
        v.style.cssText = 'position:absolute;inset:0;z-index:4;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:12px;background:rgba(10,10,15,.92);font:700 26px system-ui,sans-serif;color:#fde68a';
        S.ov.appendChild(v);
      }
    } else {
      if (v) v.remove();
      if (S.micOff) wakeMic();
      tone('listen');
    }
  }

  // ─── the screen ───
  function render() {
    liveMark();
    if (!S) return;
    if (S.chat) renderChat(false);
    var q = function (s) { return S.ov.querySelector(s); };
    var ph = S.phase, P = S.kid ? KID_PALETTE : PALETTE, pal = P[ph] || P.listen;
    // Admin mode on: always in view while it is.
    var adm = q('.cv-admin'); if (adm) adm.hidden = !(S.settings && S.settings.adminMode === true);
    // The PC lock: Cru works as always (your voice decides what it changes); its history and files stay out of view.
    var lockedNow = !!(S.opts.isLocked && S.opts.isLocked()); S.ov.classList.toggle('locked', lockedNow); if (lockedNow && S.chat) { S.chat = false; S.ov.classList.remove('chat'); }
    q('.cv-st').textContent = ph === 'listen' ? 'Listening' : ph === 'think' ? 'Thinking' : ph === 'speak' ? 'Speaking' : ph === 'start' ? 'Waking' : 'Asleep';
    q('.cv-pill').style.background = hex(pal[0], 0.14); q('.cv-pill').style.border = '1px solid ' + hex(pal[0], 0.35); q('.cv-pill i').style.background = pal[0];
    // Listening: the words as they come; thinking: what was asked, and what is queued for next under it.
    var h = ph === 'listen' ? S.heard : S.asked;
    var qd = S.kid ? '' : [queuedOf(S), hearingOf(S)].filter(Boolean).join(' '), qe = q('.cv-q');
    if (qe) { qe.hidden = !qd; qe.textContent = qd ? 'Queued, answered next: “' + qd + '”' : ''; }
    // Kid mode: what the child said, while Cru thinks about it or while it waits for Cru to finish talking.
    if (S.kid) h = ph === 'speak' ? '' : ph === 'think' ? (S.heard || S.asked) : S.heard;
    S.ov.classList.toggle('kwait', !!(S.kid && S.waitAt && Date.now() - S.waitAt < 2500 && (ph === 'think' || ph === 'speak')));
    q('.cv-ht').textContent = h ? (S.kid ? h : '“' + h + '”') : (ph === 'listen' && !S.reply && !S.kid ? 'Ask me anything, Sir.' : '');
    q('.cv-cx').hidden = !canCancel(S);
    q('.cv-reply').textContent = S.kid && ph === 'think' ? '' : S.reply;
    q('.cv-reply').classList.toggle('think', !!(S.kid && ph === 'think'));
    var d = q('.cv-detail'); d.hidden = !S.detail; d.textContent = S.detail;
    q('.cv-yesno').hidden = !(S.question && ph !== 'think');
    var n = q('.cv-note'); n.hidden = !S.notice; n.textContent = S.notice;
    q('.cv-mic').setAttribute('aria-label', ph === 'speak' ? 'Stop talking' : 'Talk');
  }

  /** While Cru talks, keep the part being said in view (unless the owner scrolled himself). */
  function followSpeech() {
    if (!S || S.phase !== 'speak' || S.ownScroll || S.chat) return;
    var w = S.ov.querySelector('.cv-words'), r = S.ov.querySelector('.cv-reply');
    if (!w || !r || w.scrollHeight - w.clientHeight < 4) return;
    var frac = Math.min(1, (Date.now() - (S.sayFrom || 0)) / (S.sayFor || 1));
    var top = r.offsetTop - w.offsetTop, target = top + frac * r.offsetHeight - w.clientHeight / 3;
    target = Math.max(0, Math.min(w.scrollHeight - w.clientHeight, target));
    if (Math.abs(target - w.scrollTop) > 1) w.scrollTop += (target - w.scrollTop) * 0.12;
  }

  function frame(ms) {
    if (!S) return;
    S.raf = requestAnimationFrame(frame);
    followSpeech();
    var t = ms / 1000, ph = S.phase;
    // Crundi's level: the output analyser (streamed voice), or a bump on each word (browser voice).
    if (S.outAnalyser && S.outAnalyser.getByteTimeDomainData) { var b = new Uint8Array(S.outAnalyser.fftSize); S.outAnalyser.getByteTimeDomainData(b); var sum = 0; for (var i = 0; i < b.length; i++) { var x = (b[i] - 128) / 128; sum += x * x; } S.voice = Math.min(1, Math.sqrt(Math.sqrt(sum / b.length)) * 1.4); }
    else if (S.speaking) { S.voiceBump = (S.voiceBump || 0.4) * 0.94; S.voice = 0.3 + S.voiceBump * 0.6 + 0.1 * Math.abs(Math.sin(t * 9)); }
    else S.voice = 0;
    var target = ph === 'listen' ? (S.micShown || 0) : ph === 'speak' ? S.voice : ph === 'think' ? 0.08 + 0.05 * Math.sin(t * 3) : 0.03;
    S.peak = Math.max(S.peak * 0.996, Math.max(0.08, target));
    var norm = (ph === 'listen' || ph === 'speak') ? Math.min(1, target / S.peak) : target;
    S.lvl += (norm - S.lvl) * (norm > S.lvl ? 0.45 : 0.15);
    if (ms - S.lastShift > 16) { S.wave.copyWithin(1, 0); S.wave[0] = S.lvl; S.lastShift = ms; }
    // The field is hidden in the chat view: drawn at no size it throws, and the mic below stopped moving.
    var fc = S.ov.querySelector('.cv-field canvas');
    if (fc && fc.clientWidth > 40 && fc.clientHeight > 40) { try { drawField(t, ph); } catch (e) { /* the mic still moves */ } }
    try { drawMic(t, ms, ph); } catch (e) { /* the next frame draws again */ }
  }

  function fit(cv) {
    var d = window.devicePixelRatio || 1, w = cv.clientWidth, h = cv.clientHeight;
    if (cv.width !== Math.round(w * d) || cv.height !== Math.round(h * d)) { cv.width = Math.round(w * d); cv.height = Math.round(h * d); }
    var g = cv.getContext('2d'); g.setTransform(d, 0, 0, d, 0, 0); return { g: g, w: w, h: h };
  }

  function blob(g, x, y, r, t, s, w) {
    var n = 26, p = [];
    for (var k = 0; k < n; k++) { var a = k / n * 6.283, rr = r * (1 + w * (0.6 * Math.sin(3 * a + t * 1.3 + s) + 0.4 * Math.sin(5 * a - t * 0.9 + s * 2))); p.push([x + Math.cos(a) * rr, y + Math.sin(a) * rr]); }
    g.beginPath();
    for (var k2 = 0; k2 < n; k2++) { var a1 = p[k2], b1 = p[(k2 + 1) % n]; if (!k2) { var z = p[n - 1]; g.moveTo((z[0] + a1[0]) / 2, (z[1] + a1[1]) / 2); } g.quadraticCurveTo(a1[0], a1[1], (a1[0] + b1[0]) / 2, (a1[1] + b1[1]) / 2); }
    g.closePath();
  }

  function drawField(t, ph) {
    var f = fit(S.ov.querySelector('.cv-field canvas')), g = f.g, W = f.w, H = f.h;
    var P = S.kid ? KID_PALETTE : PALETTE, pal = P[ph] || P.listen, a = pal[0], b = pal[1];
    g.clearRect(0, 0, W, H);
    var cx = W / 2, cy = H / 2, L = Math.min(38, Math.min(W, H * 0.8) / 390 * 40), lvl = S.lvl;
    var glow = g.createRadialGradient(cx, cy, 0, cx, cy, Math.min(W, H) * 0.5); glow.addColorStop(0, hex(a, 0.16 + 0.12 * lvl)); glow.addColorStop(0.7, hex(a, 0.04)); glow.addColorStop(1, 'rgba(0,0,0,0)');
    g.fillStyle = glow; g.beginPath(); g.arc(cx, cy, Math.min(W, H) * 0.5, 0, 7); g.fill();
    // cells
    var cells = S.cells, n = cells.length, rx = W / 2 - 46, ry = H / 2 - 34, placed = [];
    for (var i = 0; i < n; i++) {
      var an = -Math.PI / 2 + i / Math.max(1, n) * 6.283 + t * 0.025 + 0.06 * Math.sin(t * 0.4 + i);
      placed.push({ c: cells[i], i: i, x: cx + Math.cos(an) * (rx + 6 * Math.sin(t * 0.5 + i * 1.7)), y: cy + Math.sin(an) * (ry + 6 * Math.cos(t * 0.45 + i)), r: (cells[i].kind === 'idle' ? 13 : 17) + (cells[i].service != null ? 1.5 : 0) });
    }
    var disc = L * 1.38, lines = 22, gap = 3.2, r0 = disc + 5 + lines * gap - 4;
    placed.forEach(function (p) {
      var d = Math.hypot(p.x - cx, p.y - cy); if (d < 1) return;
      var dx = (p.x - cx) / d, dy = (p.y - cy) / d, w = 14 * Math.sin(t * 0.8 + p.i);
      if (d - p.r <= r0) return;
      var p0 = [cx + dx * r0, cy + dy * r0], p3 = [cx + dx * (d - p.r), cy + dy * (d - p.r)], col = KIND[p.c.kind] || C.muted, idle = p.c.kind === 'idle';
      var p1 = [p0[0] + dx * 28 - dy * w, p0[1] + dy * 28 + dx * w], p2 = [p3[0] - dx * 28 - dy * w, p3[1] - dy * 28 + dx * w];
      g.beginPath(); g.moveTo(p0[0], p0[1]); g.bezierCurveTo(p1[0], p1[1], p2[0], p2[1], p3[0], p3[1]);
      g.strokeStyle = hex(col, idle ? 0.12 : 0.22); g.lineWidth = 4; g.stroke(); g.strokeStyle = hex(col, idle ? 0.25 : 0.55); g.lineWidth = 1.2; g.stroke();
      if (!idle && p.c.kind !== 'running' && p.c.kind !== 'error') for (var j = 0; j < 3; j++) {
        var u = (t * 0.45 + j / 3 + p.i * 0.13) % 1, v = 1 - u;
        var bx = v * v * v * p3[0] + 3 * v * v * u * p2[0] + 3 * v * u * u * p1[0] + u * u * u * p0[0], by = v * v * v * p3[1] + 3 * v * v * u * p2[1] + 3 * v * u * u * p1[1] + u * u * u * p0[1];
        g.fillStyle = hex(col, 0.9 * (1 - u * 0.6)); g.beginPath(); g.arc(bx, by, 2.2, 0, 7); g.fill();
      }
    });
    placed.forEach(function (p) {
      var col = KIND[p.c.kind] || C.muted, idle = p.c.kind === 'idle';
      var r = p.r * (p.c.kind === 'working' ? 1 + 0.08 * Math.sin(t * 4 + p.i) : p.c.kind === 'error' ? 1 + 0.05 * Math.sin(t * 8) : 1);
      if (!idle) { g.fillStyle = hex(col, 0.1); g.beginPath(); g.arc(p.x, p.y, r * 1.9, 0, 7); g.fill(); }
      blob(g, p.x, p.y, r, t, p.i * 2.1, idle ? 0.05 : 0.09);
      var fg = g.createRadialGradient(p.x - r * 0.3, p.y - r * 0.3, 1, p.x, p.y, r * 1.2); fg.addColorStop(0, hex(col, idle ? 0.22 : 0.45)); fg.addColorStop(1, hex(col, 0.05));
      g.fillStyle = fg; g.fill(); g.strokeStyle = hex(col, idle ? 0.45 : 0.9); g.lineWidth = 1.4; g.stroke();
      g.fillStyle = hex(col, 0.95); g.beginPath(); g.arc(p.x + Math.sin(t * 0.7 + p.i) * 2, p.y + Math.cos(t * 0.6 + p.i) * 2, r * 0.28, 0, 7); g.fill();
      if (p.c.service != null) { var sa = t * 1.2 + p.i; g.fillStyle = p.c.service ? C.green : C.muted; g.beginPath(); g.arc(p.x + Math.cos(sa) * r * 0.62, p.y + Math.sin(sa) * r * 0.62, 2.2, 0, 7); g.fill(); }
      g.font = '500 10.5px system-ui'; g.textAlign = 'center'; g.fillStyle = idle ? C.muted : '#b8b8d0';
      var nm = p.c.name.length > 14 ? p.c.name.slice(0, 13) + '…' : p.c.name; g.fillText(nm, p.x, p.y + r + 14);
    });
    // contours
    var spin = (ph === 'think' ? t * 60 : t * 6) * Math.PI / 180, swell = 8 + 38 * lvl;
    var sw = g.createConicGradient ? g.createConicGradient(spin, cx, cy) : null;
    if (sw) [[C.sky, 0], [C.accent, 0.18], [C.ask, 0.36], [C.wait, 0.5], [C.ask, 0.64], [C.accent, 0.82], [C.sky, 1]].forEach(function (s) { sw.addColorStop(s[1], s[0]); });
    for (var li = 0; li < lines; li++) {
      var fr = (li + 1) / lines, base = disc + 5 + li * gap, amp = swell * fr * fr + 2.5 * fr;
      g.beginPath();
      for (var s2 = 0; s2 <= 144; s2++) {
        var an2 = s2 / 144 * 6.283, fx = Math.abs(s2 / 144 * 2 - 1);
        var wv = S.wave[Math.min(95, Math.floor((1 - fx) * (96 - 25)) + Math.floor(li / 3))];
        var sh = 0.55 * Math.sin(an2 * 3 + t * 0.6 + fr * 1.2) + 0.3 * Math.sin(an2 * 5 - t * 0.45 + fr * 2.1) + 0.15 * Math.sin(an2 * 2 + t * 0.8);
        var rr = base + amp * (0.35 + 0.65 * wv) * (0.6 + 0.4 * sh) + amp * 0.5 * sh;
        var x2 = cx + Math.cos(an2 - Math.PI / 2) * rr, y2 = cy + Math.sin(an2 - Math.PI / 2) * rr;
        if (s2) g.lineTo(x2, y2); else g.moveTo(x2, y2);
      }
      g.closePath(); g.strokeStyle = sw || a; g.globalAlpha = Math.max(0.12, (1 - fr * 0.7) * (0.7 + 0.3 * lvl)); g.lineWidth = 1; g.stroke(); g.globalAlpha = 1;
    }
    // the disc, its rim, the usage arcs, the head
    var gl2 = g.createRadialGradient(cx, cy, 0, cx, cy, disc * 1.6); gl2.addColorStop(0, hex(a, 0.35 + 0.4 * lvl)); gl2.addColorStop(1, 'rgba(0,0,0,0)');
    g.fillStyle = gl2; g.beginPath(); g.arc(cx, cy, disc * 1.6, 0, 7); g.fill();
    g.fillStyle = '#05050a'; g.beginPath(); g.arc(cx, cy, disc, 0, 7); g.fill();
    g.strokeStyle = sw || a; g.lineWidth = 2 + 1.5 * lvl; g.beginPath(); g.arc(cx, cy, disc, 0, 7); g.stroke();
    var u = S.usage || {}, sp = S.settings && S.settings.spend, arcs = [];
    if (u.week != null) arcs.push([u.week, C.hover]);
    if (u.five != null) arcs.push([u.five, C.sky]);
    if (sp) { arcs.push([S.settings.monthlyLimitUsd > 0 ? sp.claudeUsd / S.settings.monthlyLimitUsd : 1, C.ask]); arcs.push([(S.settings.deepgramLimitUsd || 200) > 0 ? sp.deepgramUsd / (S.settings.deepgramLimitUsd || 200) : 1, C.green]); }
    arcs.forEach(function (ar, k) {
      var rad = disc - (6 + k * 4.5);
      g.lineCap = 'round'; g.strokeStyle = hex(C.border, 0.6); g.lineWidth = 2.2; g.beginPath(); g.arc(cx, cy, rad, 0, 7); g.stroke();
      g.strokeStyle = ar[1]; g.lineWidth = 2.6; g.beginPath(); g.arc(cx, cy, rad, -Math.PI / 2, -Math.PI / 2 + Math.max(0.05, Math.min(1, ar[0])) * 6.283); g.stroke();
    });
    var sz = L * 1.45 * (1 + 0.06 * lvl);
    if (S.logo.complete) g.drawImage(S.logo, cx - sz / 2, cy - sz / 2 - 2, sz, sz);
  }

  /** The liquid mic (as the phone's FluidMic): a sphere that spreads into water while the owner talks. */
  function drawMic(t, ms, ph) {
    var f = fit(S.ov.querySelector('.cv-mic canvas')), g = f.g, W = f.w, H = f.h, Lq = S.liq;
    var dt = Lq.lastT ? Math.min(0.05, (ms - Lq.lastT) / 1000) : 0.016; Lq.lastT = ms;
    var raw = ph === 'listen' ? (S.micShown || 0) : 0;
    Lq.peak = Math.max(Lq.peak * 0.995, Math.max(0.1, raw));
    var level = Math.min(1, raw / Lq.peak); Lq.lv += (level - Lq.lv) * (level > Lq.lv ? 0.45 : 0.12);
    Lq.hist.copyWithin(1, 0); Lq.hist[0] = Lq.lv;
    if (level > 0.22) Lq.until = ms + 450;
    var talking = ph === 'listen' && ms < Lq.until, target = talking ? 1 : 0;
    Lq.mv += ((target - Lq.m) * (talking ? 28 : 42) - Lq.mv * (talking ? 9 : 5.5)) * dt; Lq.m += Lq.mv * dt;
    if (talking && Lq.lv > 0.55 && ms - Lq.lastDrop > 140) { Lq.drops.push({ x: (Math.random() - 0.5) * 3, vx: (Math.random() - 0.5) * 60, vy: -(90 + Lq.lv * 120), b: ms }); Lq.lastDrop = ms; }
    if (talking && Lq.lv > 0.3 && Math.random() < 0.08) Lq.ripples.push({ x: (Math.random() - 0.5) * 2.6, b: ms });
    Lq.drops = Lq.drops.filter(function (d) { return ms - d.b < 900; }); Lq.ripples = Lq.ripples.filter(function (r) { return ms - r.b < 900; });
    g.clearRect(0, 0, W, H);
    var cx = W / 2, cy = H / 2, R = 30, m = Lq.m, pal = (S.kid ? KID_PALETTE : PALETTE)[ph] || PALETTE.listen;
    var gl = g.createRadialGradient(cx, cy, 0, cx, cy, R * 3); gl.addColorStop(0, hex(C.sky, 0.18 + 0.2 * Lq.lv)); gl.addColorStop(1, 'rgba(0,0,0,0)'); g.fillStyle = gl; g.fillRect(0, 0, W, H);
    Lq.ripples.forEach(function (r) { var a = (ms - r.b) / 900; g.strokeStyle = 'rgba(125,211,252,' + (0.5 * (1 - a) * m) + ')'; g.lineWidth = 1.2; g.beginPath(); g.ellipse(cx + r.x * R, cy - R * (1 - 0.62 * m) * 0.35, 6 + 40 * a, (6 + 40 * a) * 0.22, 0, 0, 7); g.stroke(); });
    function shape(tt, layer) {
      var pts = [], halfW = R * (1 + 1.9 * m), band = R * (1 - 0.62 * m);
      for (var i = 0; i <= 120; i++) {
        var a = i / 120 * Math.PI * 2, wob = R * (0.03 * Math.sin(a * 3 + tt * 2 + layer) + 0.02 * Math.sin(a * 5 - tt * 3 + layer * 2)) * (1 - m) + R * 0.06 * (1 - m) * Math.sin(a * 2 + tt * 1.3);
        var sx = Math.cos(a) * (R + wob), sy = Math.sin(a) * (R + wob), uu = Math.cos(a), k = Math.floor((uu * 0.5 + 0.5) * 63);
        var wave = Lq.hist[Math.min(63, Math.abs(k - 32))] * R * 0.9;
        var surf = -band * 0.35 - wave * (0.6 + 0.4 * Math.sin(uu * 9 + tt * 8 + layer)) - R * 0.05 * Math.sin(uu * 14 - tt * 10 + layer);
        var bx = uu * halfW, by = Math.sin(a) < 0 ? surf * (1 - Math.pow(Math.abs(uu), 6)) : band * 0.55 * (1 - Math.pow(Math.abs(uu), 4));
        pts.push([cx + sx + (bx - sx) * m, cy + sy + (by - sy) * m]);
      }
      g.beginPath(); pts.forEach(function (p, i) { if (i) g.lineTo(p[0], p[1]); else g.moveTo(p[0], p[1]); }); g.closePath();
    }
    g.globalCompositeOperation = 'lighter';
    [[pal[0], '#1e3a8a'], [C.accent, '#312e81'], [C.ask, '#4c1d95']].forEach(function (c, l) {
      shape(t + l * 0.7, l * 1.3); var gr = g.createLinearGradient(cx - R * 2, cy - R, cx + R * 2, cy + R); gr.addColorStop(0, c[0]); gr.addColorStop(1, c[1]); g.fillStyle = gr; g.globalAlpha = 0.55; g.fill();
    });
    g.globalAlpha = 1; g.globalCompositeOperation = 'source-over';
    shape(t, 0); g.strokeStyle = 'rgba(255,255,255,.22)'; g.lineWidth = 1; g.stroke();
    g.fillStyle = 'rgba(255,255,255,' + (0.22 * (1 - 0.5 * m)) + ')'; g.beginPath(); g.ellipse(cx - R * 0.25 * (1 - m) - R * 0.6 * m, cy - R * 0.55 * (1 - m) - R * 0.38 * m, R * (0.34 + 0.5 * m), R * Math.max(0.02, 0.13 - 0.06 * m), -0.2 * (1 - m), 0, 7); g.fill();
    Lq.drops.forEach(function (d) { var a = (ms - d.b) / 1000; g.fillStyle = 'rgba(125,211,252,' + (0.8 * (1 - a)) + ')'; g.beginPath(); g.arc(cx + d.x * R + d.vx * a, cy - R * 0.4 + d.vy * a + 260 * a * a, 2.4 * (1 - a * 0.5), 0, 7); g.fill(); });
  }

  /**
   * Wire a top-bar button on a wide screen: click toggles (and keeps it open),
   * hovering opens it (and it goes again when left idle), and holding
   * Ctrl+Space anywhere is push to talk.
   */
  /**
   * Cru's mark: the voice screen's contours (the same formula as the field,
   * frozen at one moment of speech), fourteen hairlines round a dark centre,
   * sky to violet to pink. An SVG string; [px] sets its size.
   */
  var iconSeq = 0;
  /**
   * Cru's mark (the owner's sketch, 9 Nov): three bars centred like a sound
   * wave, the tall one running into a joined "Cru", all one stroke with a
   * dark layer behind and a light edge on top. [px] is its height. While Cru
   * listens or talks the two short bars move (.cru-live, see liveMark).
   * The app draws the same (CruIcon.kt).
   */
  var CRU_WORD = 'M76.6 24C70.2 12 54.2 14 51 38C48.6 60 57.4 74 68.6 72C76.6 71 81.4 64 84.6 58C87.8 51 90.2 46 92.6 42L92.6 72L92.6 58C95.8 48 100.6 42 107 42C105.4 56 105.4 71 114.2 72C123 73 127.8 60 129.4 42C127.8 58 127.8 72 135.8 72C140.6 72 143.8 67 146.2 62';
  function cruIcon(px) {
    var n = ++iconSeq, g = 'cru-g' + n, d = 'cru-d' + n, w = Math.round(px * 150 / 90);
    var stops = function (cs) { return cs.map(function (c) { return '<stop offset="' + c[0] + '" stop-color="' + c[1] + '"/>'; }).join(''); };
    var lines = '<path class="cru-b1" d="M15 35V55"/><path class="cru-b2" d="M28 25V65"/><path d="M41 10V80"/><path d="' + CRU_WORD + '"/>';
    return '<svg class="cru-ic" viewBox="6 0 150 90" width="' + w + '" height="' + px + '" fill="none" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
      '<defs><linearGradient id="' + g + '" gradientUnits="userSpaceOnUse" x1="8" y1="90" x2="155" y2="0">' + stops([[0, '#38bdf8'], [0.4, '#6366f1'], [0.72, '#a78bfa'], [1, '#e879f9']]) + '</linearGradient>' +
      '<linearGradient id="' + d + '" gradientUnits="userSpaceOnUse" x1="8" y1="90" x2="155" y2="0">' + stops([[0, '#0c4a6e'], [0.45, '#312e81'], [1, '#701a75']]) + '</linearGradient></defs>' +
      '<g stroke="url(#' + d + ')" stroke-width="9" transform="translate(1.6 2.4)">' + lines + '</g>' +
      '<g stroke="url(#' + g + ')" stroke-width="9">' + lines + '</g>' +
      '<g stroke="#fff" stroke-opacity=".28" stroke-width="2.4" transform="translate(-1.2 -1.4)">' + lines + '</g></svg>';
  }
  // Cru's unread messages (reports nobody heard): a dot on its mark and on the chat button.
  var unread = 0, cruButtons = [];
  function setUnread(n) {
    unread = Math.max(0, Number(n) || 0);
    cruButtons.forEach(function (b) { b.classList.toggle('cru-unread', unread > 0); b.title = unread ? 'Talk to Cru (' + unread + ' unread)' : 'Talk to Cru (voice assistant)'; });
    if (S) { var c = S.ov.querySelector('.cv-chatb'); if (c) c.classList.toggle('cru-unread', unread > 0 && !S.chat); }
  }
  function markSeen() {
    if (!unread || !S) return;
    setUnread(0);
    post('/api/voice/seen').catch(function () { /* next time */ });
  }

  /** The marks move while a voice screen is open and Cru listens, thinks or talks. */
  function liveMark() {
    var on = !!(S && S.phase && S.phase !== 'ended');
    var list = document.querySelectorAll('.cru-ic');
    for (var i = 0; i < list.length; i++) list[i].classList.toggle('cru-live', on);
  }

  function attach(button, getOpts) {
    if (!button || button._cvAttached) return;
    button._cvAttached = true;
    // The top-bar button wears Cru's mark: no outline, no background.
    if (!document.getElementById('cru-btn-css')) {
      var bs = el('style'); bs.id = 'cru-btn-css';
      bs.textContent = '.cru-btn{position:relative}.cru-unread:after{content:"";position:absolute;top:1px;right:1px;width:8px;height:8px;border-radius:50%;background:#ef4444;box-shadow:0 0 0 2px var(--bg-primary,#05050a)}' + '.cru-btn,.cru-btn:hover{border:none!important;background:transparent!important;padding:2px!important;width:auto!important}.cru-btn .cru-ic{display:block;transition:transform .2s}.cru-btn:hover .cru-ic{transform:scale(1.08)}' +
        '.cru-ic .cru-b1,.cru-ic .cru-b2{transform-box:fill-box;transform-origin:50% 50%}.cru-ic.cru-live .cru-b1{animation:cru-p .9s ease-in-out infinite}.cru-ic.cru-live .cru-b2{animation:cru-p .9s ease-in-out .15s infinite}@keyframes cru-p{0%,100%{transform:scaleY(1)}50%{transform:scaleY(.45)}}';
      document.head.appendChild(bs);
    }
    button.classList.add('cru-btn');
    cruButtons.push(button); setUnread(unread);
    button.innerHTML = cruIcon(24);
    var hoverTimer = null;
    var openWith = function (how) { var o = getOpts() || {}; o.anchor = button; o.how = how; open(o); };
    button.addEventListener('click', function (e) { e.stopPropagation(); clearTimeout(hoverTimer); if (S) { if (S.how === 'hover') S.how = 'click'; else close(); } else openWith('click'); });
    // Hovering is not asking: while a chat is in voice mode it keeps the microphone.
    button.addEventListener('mouseenter', function () { if (window.innerWidth < 900 || S || (window.CrundiChat && window.CrundiChat.voiceActive === true)) return; hoverTimer = setTimeout(function () { if (!S) openWith('hover'); }, 350); });
    button.addEventListener('mouseleave', function () { clearTimeout(hoverTimer); if (S) maybeLeave(); });
    var held = false;
    window.addEventListener('keydown', function (e) {
      if (recording || held || e.repeat) return;
      var k = shortcut();
      if (!matches(e, k)) return;
      e.preventDefault(); held = true;
      if (!S) openWith('ptt'); else { S.ptt = true; if (S.phase === 'speak') bargeIn(); }
    }, true);
    window.addEventListener('keyup', function (e) {
      if (!held) return;
      var k = shortcut();
      // Let go of the key, or of any modifier the shortcut needs.
      if (e.code === k.code || (k.ctrl && e.key === 'Control') || (k.alt && e.key === 'Alt') || (k.shift && e.key === 'Shift') || (k.meta && e.key === 'Meta')) { held = false; pttRelease(); }
    }, true);
  }

  // ─── the push-to-talk shortcut: bindable, kept per device (Settings › Voice assistant) ───
  var SHORTCUT_KEY = 'crundi_voice_ptt';
  var DEFAULT_SHORTCUT = { ctrl: true, alt: false, shift: false, meta: false, code: 'Space' };
  var recording = false;
  function shortcut() { try { var k = JSON.parse(localStorage.getItem(SHORTCUT_KEY) || 'null'); if (k && k.code) return k; } catch (e) { /* default */ } return DEFAULT_SHORTCUT; }
  function matches(e, k) { return e.code === k.code && !!e.ctrlKey === !!k.ctrl && !!e.altKey === !!k.alt && !!e.shiftKey === !!k.shift && !!e.metaKey === !!k.meta; }
  function label(k) {
    k = k || shortcut();
    var key = k.code.replace(/^Key/, '').replace(/^Digit/, '').replace(/^Arrow/, '').replace(/^Numpad/, 'Num ');
    return [k.ctrl && 'Ctrl', k.alt && 'Alt', k.shift && 'Shift', k.meta && (navigator.platform.indexOf('Mac') >= 0 ? 'Cmd' : 'Win'), key].filter(Boolean).join('+');
  }
  /** Wait for the next key combination and save it as the shortcut. done(label) when set; Escape cancels. */
  function recordShortcut(done) {
    recording = true;
    var on = function (e) {
      if (['Control', 'Alt', 'Shift', 'Meta'].indexOf(e.key) >= 0) return;   // a modifier alone is not a shortcut
      e.preventDefault(); e.stopPropagation();
      window.removeEventListener('keydown', on, true);
      recording = false;
      if (e.key === 'Escape') { done && done(null); return; }
      var k = { ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey, code: e.code };
      try { localStorage.setItem(SHORTCUT_KEY, JSON.stringify(k)); } catch (x) { /* not kept */ }
      done && done(label(k));
    };
    window.addEventListener('keydown', on, true);
  }
  function resetShortcut() { try { localStorage.removeItem(SHORTCUT_KEY); } catch (e) { /* none */ } return label(DEFAULT_SHORTCUT); }

  /** The PC: news while the dropdown is shut opens it and says it (src/webapp.js voiceReport). */
  function report(say, show, opts) {
    // Opened by Cru itself (news, a wake-up): it says it, and closes again unless the owner answers.
    if (!S) { open(opts || {}); if (S) { S.reportOnly = true; S.reports.push({ speak: say, show: show }); } return; }
    tell(say, show);
  }

  window.CrundiVoice = { _test: { isEcho: isEcho, talksOver: talksOver, pickCue: pickCue, CUE_TEXTS: CUE_TEXTS, voicedShare: voicedShare, createGate: createGate, createPreroll: createPreroll, createVad: createVad, parseVad: parseVad, shownLevel: shownLevel, SPEECH_MS: SPEECH_MS, VAD_WARM: VAD_WARM, makeLoopback: makeLoopback, richOpus: richOpus, isDiscard: isDiscard, cancelledNotice: cancelledNotice, canCancel: canCancel, createVideoGate: createVideoGate, blocksHtml: blocksHtml, isCalling: isCalling, speakingIndex: speakingIndex, queuedOf: queuedOf, hearingOf: hearingOf }, open: open, close: function () { close(); },
    /** Kid mode from the page: { on, ... } (GET /api/voice/kid, SSE 'voice-kid'); opts as for open(). */
    kidMode: kidMode, get isKid() { return !!(S && S.kid); }, kidPaused: function (on) { kidPaused(on); }, attach: attach, icon: cruIcon, tone: tone, setUnread: setUnread, get unread() { return unread; }, report: report,
    /** A reference dragged from Crundi (a kanban card, a file row...): into the typing box when typing, else a chip for the next turn. */
    dropRef: function (ref) {
      if (!S || S.kid || !ref) return false;
      var inp = S.ov.querySelector('.cv-type');
      if (S.kb && inp) { inp.value = (inp.value ? inp.value.replace(/\s*$/, ' ') : '') + ref + ' '; try { inp.focus(); } catch (e) { /* gone */ } S.quietSince = Date.now(); return true; }
      addRef(ref); return true;
    },
    shortcutLabel: function () { return label(); }, recordShortcut: recordShortcut, resetShortcut: resetShortcut, get isOpen() { return !!S; },
    // The own-voice tools, shared with a chat's voice mode (claude-chat.js), which talks over its replies the same way.
    kit: { createGate: createGate, createPreroll: createPreroll, voicedShare: voicedShare, makeLoopback: makeLoopback },
    // The PC's "Hey Cru" (app/voice-wake/controller.js): an asleep screen has let the microphone go, and is woken where it is.
    // Typing (or muted on the kid screen) counts as holding it: the PC's own "Hey Cru" listener stays off too, so no microphone is open at all.
    get holdsMic() { return !!S && (!S.micOff || !!S.kb || !!S.muted); }, wake: function () { if (S && (S.kb || S.muted)) return true; if (S && S.micOff) { wakeMic(); return true; } return false; } };
})();
