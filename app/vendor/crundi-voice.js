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
 * The stream pauses while Crundi speaks; talking over it stops it. Yes/no to
 * Crundi's questions by voice; on a PC you may also type.
 */
(function () {
  'use strict';
  if (window.CrundiVoice) return;

  var C = { sky: '#38bdf8', accent: '#6366f1', hover: '#818cf8', deep: '#4f46e5', ask: '#c084fc', wait: '#d946ef', green: '#10b981', yellow: '#f59e0b', red: '#ef4444', muted: '#5a5a78', text: '#e8e8f0', text2: '#8888a8', border: '#2a2a3d', bg: '#0a0a0f', card: '#12121a' };
  var KIND = { working: C.yellow, waiting: C.sky, needs: C.hover, error: C.red, running: C.green, idle: C.muted };
  var PALETTE = { listen: [C.sky, C.accent], think: [C.ask, C.deep], speak: [C.hover, C.ask], start: [C.accent, C.deep], ended: [C.muted, '#22223a'] };
  var LOGO = 'data:image/svg+xml;utf8,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="14" y1="14" x2="82" y2="82"><stop offset="0" stop-color="#818cf8"/><stop offset="1" stop-color="#4f46e5"/></linearGradient></defs><mask id="eh"><rect width="96" height="96" fill="#fff"/><rect x="32" y="43" width="10.5" height="18" rx="5.2" fill="#000"/><rect x="53.5" y="43" width="10.5" height="18" rx="5.2" fill="#000"/></mask><path d="M48 27V17" fill="none" stroke-linecap="round" stroke="#fbbf24" stroke-width="5.5"/><circle cx="48" cy="13" r="6.5" fill="#fbbf24"/><rect x="11" y="26" width="74" height="57" rx="23" fill="url(#g)" mask="url(#eh)"/></svg>');
  var PAUSE_AFTER_SPEECH_MS = 450, SILENCE_MS = 30000, ECHO_MS = 5000;

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
    '.cv-heard{color:' + C.text2 + ';font-size:16px;line-height:23px;min-height:23px}.cv-reply{font-size:19px;line-height:27px;font-weight:500;margin-top:12px}' +
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
      '<div class="cv-top"><span class="cv-wm">CRUNDI</span><span>' + (opts.host ? '· ' + esc(opts.host) : '') + '</span>' +
      '<div class="cv-pill"><i></i><span class="cv-st">Waking</span></div>' +
      '<button class="cv-x" title="Close (Esc)" aria-label="Close"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg></button></div>' +
      '<div class="cv-field"><canvas></canvas></div><div class="cv-leg"></div>' +
      '<div class="cv-words"><div class="cv-heard"></div><div class="cv-reply"></div><div class="cv-detail" hidden></div><div class="cv-yesno" hidden>Say yes or no</div><div class="cv-note" hidden></div></div>' +
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
      opts: opts, ov: ov, phase: 'start', heard: '', asked: '', reply: '', detail: '', notice: '', question: '', pending: '',
      lastSaid: '', saidUntil: 0, quietSince: Date.now(), streaming: true, speaking: false,
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
      if (!s.anthropicKeySet) { block('There is no Claude API key on the server yet. Add one on the /voice page.'); return; }
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
      var src = ctx.createMediaStreamSource(stream);
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
        var level = Math.sqrt(Math.sqrt(sum / Math.max(1, n)));
        S.mic = level;
        onLevel(level, x.length / ctx.sampleRate * 1000);
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
    if (e.type === 'ready') { if (S.phase === 'start') { S.phase = 'listen'; S.quietSince = Date.now(); } }
    else if (e.type === 'interim') { if (echo(e.text)) return; if (S.phase === 'listen' || S.phase === 'start') { S.phase = 'listen'; S.heard = e.text; S.quietSince = Date.now(); } else if (S.phase === 'think') { S.heard = (S.pending + ' ' + e.text).trim(); } }
    else if (e.type === 'utterance') { if (S.phase === 'listen') S.pttSent = true; utter(e.text, false); }
    else if (e.type === 'error') { S.notice = e.error; }
    render();
  }

  function echo(t) { return S.lastSaid && Date.now() - S.saidUntil <= ECHO_MS && isEcho(t, S.lastSaid); }

  function utter(text, typed) {
    var t = String(text || '').trim();
    if (!t || !S) return;
    if (!typed && echo(t)) { S.heard = ''; render(); return; }
    if (S.phase === 'think') { S.pending = (S.pending + ' ' + t).trim(); S.heard = S.pending; render(); return; }
    if (S.phase === 'speak') { if (!typed) return; stopSpeaking(S); }
    if (S.phase === 'ended' && !typed) return;
    // "Hey Cru" while already awake: here at once, without asking Claude.
    if (isCalling(t)) { S.asked = t; S.heard = ''; S.reply = HERE; S.detail = ''; S.notice = ''; S.quietSince = Date.now(); speak(HERE); render(); return; }
    ask(t);
  }

  /** What Cru says when called while it is already listening (as the phone's Speech.HERE). */
  var HERE = "Yes, Sir? I'm here.";
  /** Just "Hey Cru", or the name alone (the phone's Speech.isCalling). */
  function isCalling(t) {
    var w = String(t).toLowerCase().split(/[^\p{L}\p{N}']+/u).filter(Boolean);
    while (w.length && /^(hey|hi|hello|ok|okay|oi|yo)$/.test(w[0])) w.shift();
    return w.length > 0 && w.length <= 2 && w.every(function (x) { return /^(cru|crew|kru|krew|crue|crews)$/.test(x); });
  }

  function ask(t) {
    S.phase = 'think'; S.asked = t; S.heard = t; S.reply = ''; S.detail = ''; S.question = ''; S.notice = ''; S.quietSince = Date.now();
    render();
    post('/api/voice/turn', { text: t, where: 'web' + (S.opts.host ? ' (' + S.opts.host + ')' : ''), locked: false }).then(function (r) {
      if (!S) return;
      var ok = r && r.ok;
      S.reply = ok ? (r.speak || '') : ''; S.detail = ok ? (r.show || '') : ''; S.notice = ok ? '' : (r && r.error || 'Something went wrong');
      S.question = ok && r.pending && r.pending.length ? r.pending[0].question : '';
      if (ok && r.phoneActions && r.phoneActions.length) S.notice = 'Background listening is for the phone: turn it on from the Crundi app.';
      var say = ok ? S.reply : S.notice;
      api('/api/voice/settings').then(function (s) { if (S && s && s.ok !== false) { S.settings = s; refreshData(); } });
      if (say) speak(say); else afterSpeech();
      render();
    }).catch(function (e) { if (!S) return; S.notice = e.message || 'Could not reach the server'; afterSpeech(); render(); });
  }

  function afterSpeech() {
    if (!S) return;
    S.speaking = false; S.saidUntil = Date.now();
    S.phase = 'listen'; S.heard = ''; S.quietSince = Date.now();
    setTimeout(function () { if (S && (S.phase === 'listen' || S.phase === 'think')) { S.streaming = true; try { S.ws && S.ws.send(JSON.stringify({ type: 'resume' })); } catch (e) { /* gone */ } } }, PAUSE_AFTER_SPEECH_MS);
    if (S.pending) { var p = S.pending; S.pending = ''; ask(p); }
    render();
  }

  // ─── speaking ───
  function speak(text) {
    S.phase = 'speak'; S.speaking = true; S.lastSaid = text; S.streaming = false; S.loudFor = 0; S.spokeFor = 0;
    try { S.ws && S.ws.send(JSON.stringify({ type: 'pause' })); } catch (e) { /* gone */ }
    var voice = S.settings && S.settings.ttsVoice;
    if (voice) streamSpeech(text).catch(function () { browserSpeech(text); });
    else browserSpeech(text);
  }

  function streamSpeech(text) {
    var token = S.speakToken = {};
    var ctx = S.ctx || (S.ctx = new (window.AudioContext || window.webkitAudioContext)());
    var an = ctx.createAnalyser(); an.fftSize = 512; an.connect(ctx.destination);
    S.outAnalyser = an;
    var at = ctx.currentTime + 0.05, rate = 24000, carry = null, any = false;
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
    u.onboundary = function () { if (S) S.voiceBump = 0.6 + Math.random() * 0.35; };
    u.onend = function () { if (S && S.speaking) afterSpeech(); };
    speechSynthesis.cancel(); speechSynthesis.speak(u);
  }

  function stopSpeaking(s) {
    s.speakToken = {};
    (s.nodes || []).forEach(function (n) { try { n.stop(); } catch (e) { /* done */ } }); s.nodes = [];
    s.outAnalyser = null;
    try { window.speechSynthesis && speechSynthesis.cancel(); } catch (e) { /* none */ }
    s.speaking = false;
  }

  function tapMic() {
    if (!S) return;
    if (S.phase === 'speak') { bargeIn(); return; }
    if (S.phase === 'ended' && !S.notice) { S.phase = 'listen'; S.quietSince = Date.now(); render(); }
  }

  function bargeIn() {
    stopSpeaking(S);
    S.saidUntil = Date.now(); S.phase = 'listen'; S.heard = ''; S.quietSince = Date.now(); S.streaming = true;
    try { S.ws && S.ws.send(JSON.stringify({ type: 'resume' })); } catch (e) { /* gone */ }
    render();
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
    if (S.loudFor >= 400) bargeIn();
  }

  function tick() {
    if (!S) return;
    if (S.phase === 'listen' && !S.pending && Date.now() - S.quietSince > SILENCE_MS) { S.phase = 'ended'; render(); }
  }

  // ─── the screen ───
  function render() {
    if (!S) return;
    var q = function (s) { return S.ov.querySelector(s); };
    var ph = S.phase, pal = PALETTE[ph] || PALETTE.listen;
    q('.cv-st').textContent = ph === 'listen' ? 'Listening' : ph === 'think' ? 'Thinking' : ph === 'speak' ? 'Speaking' : ph === 'start' ? 'Waking' : 'Asleep';
    q('.cv-pill').style.background = hex(pal[0], 0.14); q('.cv-pill').style.border = '1px solid ' + hex(pal[0], 0.35); q('.cv-pill i').style.background = pal[0];
    var h = (ph === 'listen' || ph === 'think') ? S.heard : S.asked;
    q('.cv-heard').textContent = h ? '“' + h + '”' : (ph === 'listen' && !S.reply ? 'Ask me anything, Sir.' : '');
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

  window.CrundiVoice = { open: open, close: close, attach: attach, icon: cruIcon, shortcutLabel: function () { return label(); }, recordShortcut: recordShortcut, resetShortcut: resetShortcut, get isOpen() { return !!S; } };
})();
