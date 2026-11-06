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
 * voiced) that decides here whether the owner is talking. When it is him,
 * Crundi stops at once and the stream resumes from a pre-roll of the last
 * second and a bit, so his first words are kept. On a server that knows
 * barge-in ('ready' echoFilter) the server's transcript filter ('saying' /
 * 'said') is the second net; an older server is paused as before and a loud
 * voice alone cuts in. The moment something is asked
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
   * and high until there are some. Either way: held for [holdMs], and voiced.
   */
  function createGate(o) {
    o = o || {};
    var holdMs = o.holdMs || 300, floor = 0.03, overNoise = 3.2, overEcho = 2, unknownOver = 1.6, unknownStart = 0.45, minVoiced = 0.3;
    var est = 0.6, noise = 0.01, loud = 0, recent = [0, 0, 0, 0], hist = [], run = [];
    function pct90(a) { if (!a.length) return 0; var b = a.slice().sort(function (x, y) { return x - y; }); return b[Math.min(b.length - 1, Math.floor(b.length * 0.9))]; }
    return {
      /** Listening (nothing playing): how loud the room is. */
      learn: function (level) { noise = Math.min(0.2, Math.max(0.002, noise * 0.95 + level * 0.05)); },
      /** Crundi starts to play: count afresh (the echo learnt is kept). */
      start: function () { loud = 0; recent = [0, 0, 0, 0]; hist = []; run = []; },
      /** One stretch of microphone: [play] Crundi's level now (null: unknown); voiced() the share of voiced frames lately. True: it is him. */
      hear: function (level, play, ms, voiced) {
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
        return !voiced || voiced() >= minVoiced;
      },
      /** How long it has been loud: his words began about then. */
      get loudMs() { return loud; },
      get echo() { return est; },
      get noise() { return noise; },
    };
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
    '.cv-top{width:100%;max-width:820px;display:flex;align-items:center;gap:8px;padding:16px 20px 0;color:' + C.text2 + ';font-size:12px;box-sizing:border-box}' +
    '.cv-wm{font-family:ui-monospace,monospace;font-weight:700;letter-spacing:.06em;font-size:13px}' +
    '.cv-pill{margin-left:auto;display:flex;align-items:center;gap:7px;padding:5px 12px;border-radius:14px;font-size:12px;font-weight:500;color:#d6d6f0;transition:background .4s,border-color .4s}' +
    '.cv-pill i{width:7px;height:7px;border-radius:50%;animation:cvp 1s ease-in-out infinite alternate}@keyframes cvp{from{opacity:.35}to{opacity:1}}' +
    '.cv-x{margin-left:10px;width:34px;height:34px;border-radius:50%;border:0;background:#1a1a28;color:' + C.text2 + ';cursor:pointer;font-size:0;display:flex;align-items:center;justify-content:center}' +
    '.cv-field{position:relative;width:100%;max-width:820px;flex:1.5;min-height:300px}.cv-field canvas{position:absolute;inset:0;width:100%;height:100%}' +
    '.cv-leg{display:flex;flex-wrap:wrap;justify-content:center;gap:4px 14px;font:11px ui-monospace,monospace;padding:4px 16px}.cv-leg span:before{content:"";display:inline-block;width:7px;height:7px;border-radius:50%;margin-right:5px;background:currentColor}' +
    '.cv-words{width:100%;max-width:680px;flex:1;overflow:auto;text-align:center;padding:10px 24px;box-sizing:border-box}' +
    '.cv-heard{color:' + C.text2 + ';font-size:16px;line-height:23px;min-height:23px;display:flex;align-items:center;justify-content:center;gap:6px}' +
    '.cv-cx{flex:none;width:26px;height:26px;border-radius:50%;border:0;background:#1a1a28;color:' + C.text2 + ';cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0}.cv-cx:hover{color:' + C.text + ';background:#24243a}.cv-cx[hidden]{display:none}' +
    '.cv-ht{min-width:0}.cv-reply{font-size:19px;line-height:27px;font-weight:500;margin-top:12px}' +
    '.cv-detail{margin-top:12px;text-align:left;background:' + C.card + ';border:1px solid ' + C.border + ';border-radius:14px;padding:12px 14px;font-size:14px;line-height:20px;white-space:pre-wrap}' +
    '.cv-note{margin-top:10px;font-size:13px;color:' + C.yellow + '}.cv-yesno{margin-top:6px;font-size:13px;color:' + C.hover + '}' +
    '.cv-bot{width:100%;max-width:680px;display:flex;align-items:center;justify-content:center;gap:16px;padding:8px 24px 22px;box-sizing:border-box}' +
    '.cv-type{flex:1;min-width:0;background:#1a1a28;border:1px solid ' + C.border + ';border-radius:20px;color:' + C.text + ';padding:10px 14px;font-size:14px;outline:none}.cv-type:focus{border-color:' + C.accent + '}' +
    '.cv-mic{width:112px;height:112px;flex:none;cursor:pointer;position:relative}.cv-mic canvas{position:absolute;left:-80px;top:-20px;width:272px;height:152px;pointer-events:none}' +
    '.cv-ptt{display:none;font-size:11px;color:' + C.muted + ';text-align:center;padding-bottom:6px}' +
    // A wide screen (a PC): a dropdown under the top bar's mic button, the work in view around it.
    '@media (min-width:900px){.cv-ov{left:auto;right:var(--cv-right,14px);top:var(--cv-top,52px);bottom:auto;width:400px;height:min(600px,calc(100vh - 70px));border:1px solid ' + C.border + ';border-radius:16px;box-shadow:0 18px 60px rgba(0,0,0,.6);animation:cvdrop .18s ease-out}.cv-field{min-height:230px}.cv-reply{font-size:16px;line-height:23px}.cv-heard{font-size:14px}.cv-top{padding:12px 14px 0}.cv-bot{padding:4px 14px 12px}.cv-ptt{display:block}}' +
    '@keyframes cvdrop{from{opacity:0;transform:translateY(-8px)}to{opacity:1;transform:none}}' +

    '.cv-mic svg{position:absolute;left:44px;top:44px;width:24px;height:24px;stroke:#fff;fill:none;stroke-width:2;stroke-linecap:round;stroke-linejoin:round;pointer-events:none}';

  var S = null;   // the open session

  function open(opts) {
    if (S) return;
    opts = opts || {};
    if (!document.getElementById('cv-css')) { var st = el('style'); st.id = 'cv-css'; st.textContent = CSS; document.head.appendChild(st); }
    var ov = el('div', 'cv-ov');
    ov.innerHTML =
      '<div class="cv-top"><span class="cv-wm">CRU</span><span>' + (opts.host ? '· ' + esc(opts.host) : '') + '</span>' +
      '<div class="cv-pill"><i></i><span class="cv-st">Waking</span></div>' +
      '<button class="cv-x" title="Close (Esc)" aria-label="Close"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg></button></div>' +
      '<div class="cv-field"><canvas></canvas></div><div class="cv-leg"></div>' +
      '<div class="cv-words"><div class="cv-heard"><span class="cv-ht"></span><button class="cv-cx" hidden title="Cancel what I said" aria-label="Cancel what I said"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg></button></div><div class="cv-reply"></div><div class="cv-detail" hidden></div><div class="cv-yesno" hidden>Say yes or no</div><div class="cv-note" hidden></div></div>' +
      '<div class="cv-ptt">Hold ' + esc(label()) + ' to talk · click the mic to keep this open</div>' +
      '<div class="cv-bot"><input class="cv-type" placeholder="Or type to Cru…" aria-label="Type to Cru">' +
      '<div class="cv-mic" role="button" aria-label="Talk"><canvas></canvas><svg viewBox="0 0 24 24"><rect x="9" y="2" width="6" height="12" rx="3"/><path d="M5 10v1a7 7 0 0 0 14 0v-1M12 18v4"/></svg></div></div>';
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
    };
    ov.addEventListener('mouseenter', function () { if (S) clearTimeout(S.leaveTimer); });
    ov.addEventListener('mouseleave', function () { if (S) maybeLeave(); });
    ov.addEventListener('pointerdown', function () { if (S && S.how === 'hover') S.how = 'click'; });
    q('.cv-x').onclick = close;
    q('.cv-cx').onclick = function (e) { e.stopPropagation(); cancelWords(); };
    q('.cv-mic').onclick = tapMic;
    // Keys go to the voice screen first while it is open (window, capture):
    // Enter sends what was typed, Escape closes, and the page's own shortcuts
    // don't fire underneath.
    S.onKey = function (e) {
      if (!S) return;
      // Docked beside the page (a wide screen), keys elsewhere are the page's.
      var docked = window.innerWidth >= 900, inside = S.ov.contains(e.target);
      if (docked && !inside) return;
      if (e.key === 'Escape') { e.stopPropagation(); e.preventDefault(); close(); return; }
      var inp = S.ov.querySelector('.cv-type');
      if (e.target === inp) {
        e.stopPropagation();
        if (e.key === 'Enter' && inp.value.trim()) { e.preventDefault(); var t = inp.value.trim(); inp.value = ''; utter(t, true); }
      } else if (S.ov.contains(e.target) || e.target === document.body) e.stopPropagation();
    };
    window.addEventListener('keydown', S.onKey, true);
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

  function close() {
    if (!S) return;
    var s = S; S = null;
    try { if (s.wake) s.wake.release(); } catch (e) { /* gone */ }
    if (s.onVis) document.removeEventListener('visibilitychange', s.onVis);
    cancelAnimationFrame(s.raf); clearInterval(s.timer);
    window.removeEventListener('keydown', s.onKey, true);
    stopSpeaking(s);
    clearTimeout(s.releaseTimer);
    try { if (s.loop) s.loop.close(); } catch (e) { /* gone */ }
    try { if (s.ws) { s.ws.send(JSON.stringify({ type: 'stop' })); s.ws.close(); } } catch (e) { /* gone */ }
    try { if (s.proc) s.proc.disconnect(); if (s.stream) s.stream.getTracks().forEach(function (t) { t.stop(); }); if (s.ctx) s.ctx.close(); } catch (e) { /* gone */ }
    s.ov.remove();
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
    if (sp) { items.push([C.ask, 'Claude $' + (sp.claudeUsd || 0).toFixed(2) + ' / $' + S.settings.monthlyLimitUsd]); items.push([C.green, 'Deepgram $' + (sp.deepgramUsd || 0).toFixed(2) + ' / $' + (S.settings.deepgramLimitUsd || 200)]); }
    leg.innerHTML = items.map(function (i) { return '<span style="color:' + i[0] + '">' + esc(i[1]) + '</span>'; }).join('');
  }

  function start() {
    post('/api/voice/warm');
    api('/api/voice/settings').then(function (s) {
      if (!S) return;
      if (!s || s.ok === false) { block(s && s.error ? s.error : 'The voice assistant is not available on this server.'); return; }
      S.settings = s; refreshData();
      // The Claude Code brain needs no key: it runs on the subscription.
      if (!s.anthropicKeySet && s.brain !== 'claude-code') { block('There is no Claude API key on the server yet. Add one in Settings, Voice assistant.'); return; }
      openMic();
    }).catch(function (e) { block(e.message || 'Could not reach the server'); });
  }

  function block(why) { if (!S) return; S.phase = 'ended'; S.notice = why; render(); }

  // ─── the microphone: 16 kHz PCM to the server, and the level ───
  function openMic() {
    navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } }).then(function (stream) {
      if (!S) { stream.getTracks().forEach(function (t) { t.stop(); }); return; }
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
        if (S.live) {
          if (S.held) {
            // Crundi is talking: nothing goes out; is it him?
            S.preroll.push(out.buffer, ms);
            keepVoice(out);
            if (S.gate.hear(level, S.unknownVoice ? null : playLevel(), ms, recentVoicing)) ownerSpoke();
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
      ws.onclose = function () { if (S && S.ws === ws && S.phase !== 'ended') { S.notice = 'Live listening stopped. Type instead, or close and open again.'; render(); } };
    }).catch(function () { block('The microphone is not allowed. Allow it for this site, or type instead.'); S && (S.phase = 'listen'); render(); });
  }

  function heard(e) {
    if (!S) return;
    if (e.type === 'ready') { S.live = !!e.echoFilter; if (S.phase === 'start') { S.phase = 'listen'; S.quietSince = Date.now(); if (S.reports.length) { var q = S.reports.shift(); tell(q.speak, q.show); } } }
    else if (e.type === 'report') tell(e.speak || '', e.show || '');
    // The server dropped what he said ("scratch that"): clear it, or cancel the turn it started.
    else if (e.type === 'discard') takeBack(true);
    else if (e.type === 'interim') {
      // Talking over Crundi: cut it off the moment his words come.
      if (S.phase === 'speak') { if (S.live && talksOver(e.text, S.lastSaid, Date.now() - S.hintAt <= HINT_MS ? 1 : 2)) { cutIn(); S.heard = e.text; } else return; }
      else if (echo(e.text)) return;
      else if (S.phase === 'listen' || S.phase === 'start') { S.phase = 'listen'; S.heard = e.text; S.quietSince = Date.now(); }
      else if (S.phase === 'think') { if (S.cue && talksOver(e.text, S.cue.text)) stopCue(); S.heard = (S.pending + ' ' + e.text).trim(); }
    }
    else if (e.type === 'utterance') { if (S.phase === 'listen') S.pttSent = true; utter(e.text, false); }
    else if (e.type === 'error') { S.notice = e.error; }
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
    if (S.phase === 'think') { S.pending = (S.pending + ' ' + t).trim(); S.heard = S.pending; render(); return; }
    // Said over Crundi (the stream stayed on): cut it off and ask that now.
    if (S.phase === 'speak') { if (!typed && !(S.live && talksOver(t, S.lastSaid, 1))) return; cutIn(); }
    if (S.phase === 'ended' && !typed) return;
    // "Hey Cru" while already awake: here at once, without asking Claude.
    if (isCalling(t)) { var h = hereLine(); S.asked = t; S.heard = ''; S.reply = h; S.detail = ''; S.notice = ''; S.quietSince = Date.now(); speak(h); render(); return; }
    ask(t);
  }

  /** News, unasked (a watched chat finished or waits on the owner): said now if Cru is just listening, else once it is free. */
  function tell(say, show) {
    if (!S) return;
    if (S.phase !== 'listen' || S.pending || S.heard) { S.reports.push({ speak: say, show: show }); return; }
    S.reply = say; S.detail = show; S.heard = ''; S.notice = ''; S.question = ''; S.quietSince = Date.now();
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

  /** [keep]: the last answer stays on screen (it was cut short by what the owner said meanwhile). */
  function ask(t, keep) {
    S.phase = 'think'; S.asked = t; S.heard = t; if (!keep) { S.reply = ''; S.detail = ''; } S.question = ''; S.notice = ''; S.quietSince = Date.now();
    var turn = ++S.turn;
    playCue();
    render();
    post('/api/voice/turn', { text: t, where: 'web' + (S.opts.host ? ' (' + S.opts.host + ')' : ''), locked: false }).then(function (r) {
      if (!S) return;
      // Cancelled: never said or shown as an error, just what it had already done.
      if (r && r.cancelled) { if (S.phase === 'listen' && !S.heard) { S.notice = cancelledNotice(r.acted); render(); } return; }
      // Taken back meanwhile (or a newer question): not this answer's place.
      if (S.turn !== turn || S.phase !== 'think') return;
      var ok = r && r.ok;
      S.reply = ok ? (r.speak || '') : ''; S.detail = ok ? (r.show || '') : ''; S.notice = ok ? '' : (r && r.error || 'Something went wrong');
      S.question = ok && r.pending && r.pending.length ? r.pending[0].question : '';
      // The owner is done: close once the goodbye has been said.
      S.closing = !!(ok && r.close);
      if (ok && r.phoneActions && r.phoneActions.length) S.notice = 'Background listening is for the phone: turn it on from the Crundi app.';
      var say = ok ? S.reply : S.notice;
      api('/api/voice/settings').then(function (s) { if (S && s && s.ok !== false) { var v = S.settings && S.settings.ttsVoice, r = S.settings && S.settings.speechRate; S.settings = s; refreshData(); if (s.ttsVoice !== v || s.speechRate !== r || s.deepgramLimitReached) loadCues(); } });
      // The owner spoke again while Crundi thought: that goes now, the answer stays on screen unsaid.
      if (S.pending) { var p = S.pending; S.pending = ''; S.closing = false; stopCue(); ask(p, true); render(); return; }
      if (say) speak(say); else { stopCue(); afterSpeech(); }
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
  function recentVoicing() {
    var r = S.ring, n = r.length, c = new Float32Array(n);
    for (var i = 0; i < n; i++) c[i] = r[(S.ringAt + i) % n];
    return voicedShare(c);
  }

  /** Where Crundi's voice is played into: an analyser (its level), then the loopback or the speakers. */
  function outNode() {
    if (S.out || !S.ctx) return S.out;
    var ctx = S.ctx, an = ctx.createAnalyser(); an.fftSize = 512; an.connect(ctx.destination);
    S.out = an; S.outBuf = new Float32Array(an.fftSize);
    var s = S;
    makeLoopback(ctx, window, 3000).then(function (lb) {
      if (S !== s) { lb.close(); return; }
      an.disconnect(); an.connect(lb.dest); s.loop = lb; s.route = 'loopback';
      // Lost later: straight to the speakers again.
      lb.onlost = function () { if (S !== s || s.route !== 'loopback') return; try { an.disconnect(); } catch (e) { /* none */ } an.connect(ctx.destination); s.route = 'direct'; };
    }).catch(function () { /* played directly: the gate does the work */ });
    return an;
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

  function speak(text) {
    S.phase = 'speak'; S.speaking = true; S.lastSaid = text; S.loudFor = 0; S.spokeFor = 0; S.hintAt = 0; S.unknownVoice = false;
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
    S.outAnalyser = an;
    var at = Math.max(ctx.currentTime + 0.05, S.cue && S.cue.ends ? S.cue.ends : 0), rate = 24000, carry = null, any = false;
    return S.opts.apiFetch('/api/voice/speak?stream=1', { method: 'POST', headers: { 'Content-Type': 'application/json', Accept: 'audio/L16' }, body: JSON.stringify({ text: text }) }).then(function (res) {
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
    u.onboundary = function () { if (S) S.voiceBump = 0.6 + Math.random() * 0.35; };
    u.onend = function () { if (S && S.speaking) afterSpeech(); };
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
    return u;
  }

  // ─── the "one moment, Sir" cue ───
  /**
   * The cues for the voice in use: the server's clips (decoded now, so one
   * plays with no network wait) or, with the browser's own voice, the phrases
   * to say. An older server has no list: the browser's voice says ours, a
   * Deepgram voice plays none (it would not match).
   */
  function loadCues() {
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

  function playCue() {
    if (!S || !S.cues.length) return;
    var i = pickCue(S.cues.length, S.cueLast, Math.random), c = S.cues[i];
    if (!c) return;
    S.cueLast = i;
    stopCue();
    var s = S, cue = { text: c.text, ends: 0 };
    var done = function () { if (S === s && s.cue === cue) { s.cue = null; if (s.live && !s.speaking) { said(); s.unknownVoice = false; releaseLater(); } } };
    if (c.buf && s.ctx) {
      var node = s.ctx.createBufferSource(); node.buffer = c.buf; node.connect(outNode() || s.ctx.destination);
      cue.node = node; cue.ends = s.ctx.currentTime + 0.01 + c.buf.duration;
      node.onended = done;
      node.start(s.ctx.currentTime + 0.01);
    } else if (s.cueDevice && window.speechSynthesis) {
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
    if (S.loudFor < 400) return;
    // With the stream on, a voice that loud only lowers the bar for his words
    // (a cough or a door is not an interruption); an older server has only this.
    if (S.live) { S.hintAt = Date.now(); S.loudFor = 0; } else bargeIn();
  }

  function tick() {
    if (!S) return;
    // After a short silence (Settings, default a minute) the voice screen closes itself; "Hey Cru" opens it again.
    var idleMs = ((S.settings && S.settings.screenCloseSeconds) || 60) * 1000;
    if ((S.phase === 'ended' || S.phase === 'listen') && !S.pending && Date.now() - S.quietSince > idleMs) { close(); return; }
    if (S.phase === 'listen' && !S.pending && Date.now() - S.quietSince > SILENCE_MS) { S.phase = 'ended'; sleepMic(); render(); }
  }

  // ─── the screen ───
  function render() {
    if (!S) return;
    var q = function (s) { return S.ov.querySelector(s); };
    var ph = S.phase, pal = PALETTE[ph] || PALETTE.listen;
    q('.cv-st').textContent = ph === 'listen' ? 'Listening' : ph === 'think' ? 'Thinking' : ph === 'speak' ? 'Speaking' : ph === 'start' ? 'Waking' : 'Asleep';
    q('.cv-pill').style.background = hex(pal[0], 0.14); q('.cv-pill').style.border = '1px solid ' + hex(pal[0], 0.35); q('.cv-pill i').style.background = pal[0];
    var h = (ph === 'listen' || ph === 'think') ? S.heard : S.asked;
    q('.cv-ht').textContent = h ? '“' + h + '”' : (ph === 'listen' && !S.reply ? 'Ask me anything, Sir.' : '');
    q('.cv-cx').hidden = !canCancel(S);
    q('.cv-reply').textContent = S.reply;
    var d = q('.cv-detail'); d.hidden = !S.detail; d.textContent = S.detail;
    q('.cv-yesno').hidden = !(S.question && ph !== 'think');
    var n = q('.cv-note'); n.hidden = !S.notice; n.textContent = S.notice;
    q('.cv-mic').setAttribute('aria-label', ph === 'speak' ? 'Stop talking' : 'Talk');
  }

  function frame(ms) {
    if (!S) return;
    S.raf = requestAnimationFrame(frame);
    var t = ms / 1000, ph = S.phase;
    // Crundi's level: the output analyser (streamed voice), or a bump on each word (browser voice).
    if (S.outAnalyser) { var b = new Uint8Array(S.outAnalyser.fftSize); S.outAnalyser.getByteTimeDomainData(b); var sum = 0; for (var i = 0; i < b.length; i++) { var x = (b[i] - 128) / 128; sum += x * x; } S.voice = Math.min(1, Math.sqrt(Math.sqrt(sum / b.length)) * 1.4); }
    else if (S.speaking) { S.voiceBump = (S.voiceBump || 0.4) * 0.94; S.voice = 0.3 + S.voiceBump * 0.6 + 0.1 * Math.abs(Math.sin(t * 9)); }
    else S.voice = 0;
    var target = ph === 'listen' ? S.mic : ph === 'speak' ? S.voice : ph === 'think' ? 0.08 + 0.05 * Math.sin(t * 3) : 0.03;
    S.peak = Math.max(S.peak * 0.996, Math.max(0.08, target));
    var norm = (ph === 'listen' || ph === 'speak') ? Math.min(1, target / S.peak) : target;
    S.lvl += (norm - S.lvl) * (norm > S.lvl ? 0.45 : 0.15);
    if (ms - S.lastShift > 16) { S.wave.copyWithin(1, 0); S.wave[0] = S.lvl; S.lastShift = ms; }
    drawField(t, ph);
    drawMic(t, ms, ph);
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
    var pal = PALETTE[ph] || PALETTE.listen, a = pal[0], b = pal[1];
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
    var raw = ph === 'listen' ? S.mic : 0;
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
    var cx = W / 2, cy = H / 2, R = 30, m = Lq.m, pal = PALETTE[ph] || PALETTE.listen;
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
  function cruIcon(px) {
    var id = 'cru-g' + (++iconSeq), P = 96, lines = 14, disc = 2.6, gap = 0.5, swell = 2.4, seed = 1.9, out = '';
    for (var i = 0; i < lines; i++) {
      var f = (i + 1) / lines, base = disc + 0.5 + i * gap, amp = swell * f * f + 0.35 * f, d = '';
      for (var k = 0; k <= P; k++) {
        var an = k / P * Math.PI * 2, frac = Math.abs(k / P * 2 - 1);
        var v = 0.5 + 0.5 * Math.sin((1 - frac) * 7 + seed) * Math.cos((1 - frac) * 3.1);
        var sh = 0.55 * Math.sin(an * 3 + seed + f * 1.2) + 0.30 * Math.sin(an * 5 - seed * 0.7 + f * 2.1) + 0.15 * Math.sin(an * 2 + seed * 1.3);
        var rr = base + amp * (0.35 + 0.65 * v) * (0.6 + 0.4 * sh) + amp * 0.5 * sh;
        d += (k ? 'L' : 'M') + (12 + Math.cos(an - Math.PI / 2) * rr).toFixed(2) + ' ' + (12 + Math.sin(an - Math.PI / 2) * rr).toFixed(2);
      }
      out += '<path d="' + d + 'Z" stroke="url(#' + id + ')" opacity="' + Math.max(0.15, 1 - f * 0.7).toFixed(2) + '"/>';
    }
    return '<svg class="cru-ic" viewBox="0 0 24 24" width="' + px + '" height="' + px + '" fill="none" stroke-width="0.32" stroke-linejoin="round" aria-hidden="true">' +
      '<defs><linearGradient id="' + id + '" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#38bdf8"/><stop offset=".45" stop-color="#6366f1"/><stop offset=".75" stop-color="#a78bfa"/><stop offset="1" stop-color="#e879f9"/></linearGradient></defs>' +
      out + '<circle cx="12" cy="12" r="2.6" fill="#05050a" stroke="url(#' + id + ')" stroke-width="0.58"/></svg>';
  }

  function attach(button, getOpts) {
    if (!button || button._cvAttached) return;
    button._cvAttached = true;
    // The top-bar button wears Cru's mark: no outline, no background.
    if (!document.getElementById('cru-btn-css')) {
      var bs = el('style'); bs.id = 'cru-btn-css';
      bs.textContent = '.cru-btn,.cru-btn:hover{border:none!important;background:transparent!important;padding:2px!important}.cru-btn .cru-ic{display:block;transition:transform .2s}.cru-btn:hover .cru-ic{transform:scale(1.12)}';
      document.head.appendChild(bs);
    }
    button.classList.add('cru-btn');
    button.innerHTML = cruIcon(26);
    var hoverTimer = null;
    var openWith = function (how) { var o = getOpts() || {}; o.anchor = button; o.how = how; open(o); };
    button.addEventListener('click', function (e) { e.stopPropagation(); clearTimeout(hoverTimer); if (S) { if (S.how === 'hover') S.how = 'click'; else close(); } else openWith('click'); });
    button.addEventListener('mouseenter', function () { if (window.innerWidth < 900 || S) return; hoverTimer = setTimeout(function () { if (!S) openWith('hover'); }, 350); });
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
    if (!S) { open(opts || {}); if (S) S.reports.push({ speak: say, show: show }); return; }
    tell(say, show);
  }

  window.CrundiVoice = { _test: { isEcho: isEcho, talksOver: talksOver, pickCue: pickCue, CUE_TEXTS: CUE_TEXTS, voicedShare: voicedShare, createGate: createGate, createPreroll: createPreroll, makeLoopback: makeLoopback, richOpus: richOpus, isDiscard: isDiscard, cancelledNotice: cancelledNotice, canCancel: canCancel }, open: open, close: close, attach: attach, icon: cruIcon, report: report, shortcutLabel: function () { return label(); }, recordShortcut: recordShortcut, resetShortcut: resetShortcut, get isOpen() { return !!S; },
    // The PC's "Hey Cru" (app/voice-wake/controller.js): an asleep screen has let the microphone go, and is woken where it is.
    get holdsMic() { return !!S && !S.micOff; }, wake: function () { if (S && S.micOff) { wakeMic(); return true; } return false; } };
})();
