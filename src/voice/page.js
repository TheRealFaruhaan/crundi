/**
 * voice/page.js — /voice, a small test page for the voice assistant (Phase 1).
 *
 * Open it in a browser where you are signed in to Crundi (it uses the same
 * sign-in, kept in this browser). It sets the voice assistant up (model,
 * monthly limit, Claude and Deepgram keys) and talks to it: type a request,
 * or hold the button and speak — the recording goes through Deepgram on the
 * server, the answer is spoken with the browser's British voice and the
 * detail shown.
 *
 * The page holds nothing private: keys are typed in and sent to the server,
 * never read back.
 */

export function getVoicePageHtml() {
  return `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Crundi Voice</title>
<style>
:root { --bg:#0f1317; --card:#171c22; --line:#2a323b; --text:#e8edf2; --muted:#93a1af; --accent:#8ab4f8; --bad:#f28b82; --good:#81c995; }
:root[data-theme="light"] { --bg:#f6f7f9; --card:#fff; --line:#dde2e8; --text:#1b2027; --muted:#5b6672; --accent:#1a5fd0; --bad:#b3261e; --good:#1e7d3a; }
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--text); font:15px/1.45 system-ui, -apple-system, Segoe UI, Roboto, sans-serif; }
main { max-width: 720px; margin: 0 auto; padding: 20px 16px 60px; }
h1 { font-size: 22px; margin: 0 0 4px; } h2 { font-size: 16px; margin: 0 0 12px; }
.sub { color: var(--muted); margin: 0 0 18px; }
.card { background: var(--card); border: 1px solid var(--line); border-radius: 12px; padding: 16px; margin-bottom: 14px; }
label { display:block; color: var(--muted); font-size: 13px; margin: 10px 0 4px; }
input, select, textarea { width: 100%; background: var(--bg); color: var(--text); border: 1px solid var(--line); border-radius: 8px; padding: 9px 10px; font: inherit; }
textarea { min-height: 70px; resize: vertical; }
.row { display:flex; gap: 8px; flex-wrap: wrap; margin-top: 12px; }
button { background: var(--accent); color: #0b1220; border: 0; border-radius: 8px; padding: 10px 14px; font: inherit; font-weight: 600; cursor: pointer; }
button.ghost { background: transparent; color: var(--accent); border: 1px solid var(--line); }
button:disabled { opacity: .5; cursor: default; }
#live { flex: 2; min-width: 180px; padding: 16px; font-size: 16px; }
#live.on { background: var(--bad); color: #fff; }
#hold { flex: 1; min-width: 120px; padding: 16px; touch-action: none; user-select: none; }
#hold.on { background: var(--bad); color: #fff; }
.heard { min-height: 1.5em; margin-top: 10px; font-size: 17px; }
.heard .interim { color: var(--muted); }
.voices { display: grid; gap: 6px; margin-top: 10px; max-height: 360px; overflow: auto; }
.voice { display: flex; align-items: center; gap: 8px; border: 1px solid var(--line); border-radius: 8px; padding: 8px 10px; }
.voice.chosen { border-color: var(--accent); }
.voice .meta { flex: 1; min-width: 0; }
.voice .meta small { color: var(--muted); display: block; }
.voice button { padding: 6px 10px; }
.spend { color: var(--muted); font-size: 13px; margin-top: 10px; }
.msg { border-top: 1px solid var(--line); padding: 10px 0; }
.msg:first-child { border-top: 0; }
.who { font-size: 12px; color: var(--muted); }
.speak { font-size: 16px; }
pre { white-space: pre-wrap; word-break: break-word; background: var(--bg); border: 1px solid var(--line); border-radius: 8px; padding: 10px; margin: 8px 0 0; font-size: 13px; }
.err { color: var(--bad); } .ok { color: var(--good); }
</style></head>
<body><main>
<h1>Crundi Voice</h1>
<p class="sub">Talk to your voice assistant. Phase 1: it can look at everything and change nothing. "Start listening" transcribes live and answers each time you stop talking.</p>

<section class="card">
  <h2>Talk</h2>
  <div class="row">
    <button id="live">Start listening</button>
    <button class="ghost" id="hold">Hold to talk</button>
  </div>
  <div id="heard" class="heard"></div>
  <label for="text">Or type</label>
  <textarea id="text" placeholder="How much usage do I have left this week?"></textarea>
  <div class="row">
    <button id="send">Send</button>
    <button class="ghost" id="reset">New conversation</button>
    <label style="display:flex;align-items:center;gap:6px;margin:0 0 0 auto"><input type="checkbox" id="mute" style="width:auto"> Don't speak replies</label>
  </div>
  <div id="log" style="margin-top:12px"></div>
</section>

<section class="card">
  <h2>Voice</h2>
  <div class="spend" id="voiceNow"></div>
  <label for="sample">Preview line</label>
  <input id="sample" value="Yes, Master. Your weekly usage is at forty two percent, and all four services are running.">
  <div class="row">
    <label style="display:flex;align-items:center;gap:6px;margin:0"><input type="checkbox" id="britOnly" style="width:auto" checked> British only</label>
    <label style="display:flex;align-items:center;gap:6px;margin:0"><input type="checkbox" id="maleOnly" style="width:auto"> Male only</label>
    <button class="ghost" id="useDevice" style="margin-left:auto">Use this device's voice</button>
  </div>
  <div class="voices" id="voices"></div>
</section>

<section class="card">
  <h2>Settings</h2>
  <label for="model">Model</label>
  <select id="model"></select>
  <label for="limit">Monthly limit (US$)</label>
  <input id="limit" type="number" min="0" step="1">
  <label for="bday">Claude API billing day (the day of the month the Claude API is billed; its spend restarts then)</label>
  <input id="bday" type="number" min="1" max="31" step="1">
  <label for="dday">Deepgram billing day</label>
  <input id="dday" type="number" min="1" max="31" step="1">
  <label for="akey">Claude API key <span id="akeyState"></span></label>
  <input id="akey" type="password" autocomplete="off" placeholder="Leave empty to keep the current key">
  <label for="dkey">Deepgram API key <span id="dkeyState"></span></label>
  <input id="dkey" type="password" autocomplete="off" placeholder="Leave empty to keep the current key">
  <div class="row"><button id="save">Save</button><span id="saved"></span></div>
  <div class="spend" id="spend"></div>
</section>
</main>
<script>
(() => {
  const $ = (id) => document.getElementById(id);
  const token = () => { try { return localStorage.getItem('crundi_token') || ''; } catch { return ''; } };
  async function api(path, opts = {}) {
    const res = await fetch(path, { ...opts, headers: { ...(opts.headers || {}), Authorization: 'Bearer ' + token() } });
    if (res.status === 401) throw new Error('Not signed in here. Open Crundi in this browser, sign in, then reload this page.');
    return res.json();
  }
  const warm = () => api('/api/voice/warm', { method: 'POST' }).catch(() => {});
  const esc = (s) => String(s || '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

  function add(who, html) {
    const d = document.createElement('div');
    d.className = 'msg';
    d.innerHTML = '<div class="who">' + esc(who) + '</div>' + html;
    $('log').prepend(d);
  }

  // ─── speaking ───
  let voice = null;
  function pickVoice() {
    const vs = speechSynthesis.getVoices();
    voice = vs.find(v => /en-GB/i.test(v.lang) && /male|rjs|daniel|arthur|george|ryan/i.test(v.name))
      || vs.find(v => /en-GB/i.test(v.lang)) || null;
  }
  if ('speechSynthesis' in window) { pickVoice(); speechSynthesis.onvoiceschanged = pickVoice; }
  let ttsVoice = '';
  let player = null;
  function browserSay(text) {
    return new Promise((done) => {
      if (!('speechSynthesis' in window)) return done();
      speechSynthesis.cancel();
      const u = new SpeechSynthesisUtterance(text);
      if (voice) u.voice = voice;
      u.lang = 'en-GB';
      u.rate = 1.02;
      u.onend = u.onerror = () => done();
      speechSynthesis.speak(u);
    });
  }
  async function deepgramSay(text, model) {
    const res = await fetch('/api/voice/speak', {
      method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + token() },
      body: JSON.stringify({ text, voice: model }),
    });
    if (!res.ok) { let e = 'Could not speak'; try { e = (await res.json()).error || e; } catch {} throw new Error(e); }
    const url = URL.createObjectURL(await res.blob());
    if (player) { player.pause(); }
    player = new Audio(url);
    await new Promise((done) => { player.onended = player.onerror = done; player.play().catch(done); });
    URL.revokeObjectURL(url);
  }
  /** Speak, pausing live listening so Crundi doesn't hear itself. */
  async function say(text, model = ttsVoice) {
    if (!text || $('mute').checked) return;
    pauseLive(true);
    try {
      if (model) await deepgramSay(text, model);
      else await browserSay(text);
    } catch (e) {
      add('Crundi', '<div class="err">' + esc(e.message) + " — using this device's voice.</div>");
      await browserSay(text);
    } finally { pauseLive(false); }
  }

  function showAnswer(r) {
    if (r.heard !== undefined) add('Heard', '<div>' + esc(r.heard || '(nothing)') + '</div>');
    if (!r.ok) { add('Crundi', '<div class="err">' + esc(r.error) + '</div>'); return; }
    const c = r.cache || {};
    const hit = (c.read + c.written + c.uncached) ? Math.round(100 * c.read / (c.read + c.written + c.uncached)) : 0;
    add('Crundi' + (r.tools && r.tools.length ? ' · looked at ' + r.tools.join(', ') : '') + (r.costUsd ? ' · $' + r.costUsd.toFixed(4) : '') + (c.read !== undefined ? ' · ' + hit + '% from cache' : ''),
      '<div class="speak">' + esc(r.speak) + '</div>' + (r.show ? '<pre>' + esc(r.show) + '</pre>' : ''));
    say(r.speak);
    loadSettings();
  }

  async function send() {
    const text = $('text').value.trim();
    if (!text) return;
    $('send').disabled = true;
    add('Master', '<div>' + esc(text) + '</div>');
    $('text').value = '';
    try { showAnswer(await api('/api/voice/turn', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ text, where: 'the voice test page' }) })); }
    catch (e) { add('Crundi', '<div class="err">' + esc(e.message) + '</div>'); }
    finally { $('send').disabled = false; }
  }
  $('send').onclick = send;
  $('text').addEventListener('keydown', (e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); } });
  $('reset').onclick = async () => { await api('/api/voice/reset', { method: 'POST' }); $('log').innerHTML = ''; };

  // ─── hold to talk ───
  let rec = null, chunks = [], stream = null;
  async function start(e) {
    e.preventDefault();
    if (rec) return;
    warm();
    try { stream = await navigator.mediaDevices.getUserMedia({ audio: true }); }
    catch (err) { add('Crundi', '<div class="err">Microphone not available: ' + esc(err.message) + '</div>'); return; }
    chunks = [];
    rec = new MediaRecorder(stream);
    rec.ondataavailable = (ev) => { if (ev.data.size) chunks.push(ev.data); };
    rec.onstop = async () => {
      stream.getTracks().forEach(t => t.stop());
      const type = (rec && rec.mimeType) || 'audio/webm';
      rec = null;
      const blob = new Blob(chunks, { type });
      if (blob.size < 1000) return;
      $('hold').textContent = 'Thinking…';
      try {
        showAnswer(await api('/api/voice/transcribe?turn=1&where=' + encodeURIComponent('the voice test page'), {
          method: 'POST', headers: { 'Content-Type': type.split(';')[0] }, body: blob,
        }));
      } catch (err) { add('Crundi', '<div class="err">' + esc(err.message) + '</div>'); }
      $('hold').textContent = 'Hold to talk';
    };
    rec.start();
    speechSynthesis && speechSynthesis.cancel();
    $('hold').classList.add('on');
    $('hold').textContent = 'Listening… release to send';
  }
  function stop(e) {
    e.preventDefault();
    $('hold').classList.remove('on');
    if (rec && rec.state === 'recording') rec.stop();
  }
  $('hold').addEventListener('pointerdown', start);
  $('hold').addEventListener('pointerup', stop);
  $('hold').addEventListener('pointerleave', stop);
  $('hold').addEventListener('pointercancel', stop);


  // ─── live listening (Deepgram streaming) ───
  let liveWs = null, liveRec = null, liveStream = null, answering = false;
  function pauseLive(on) {
    if (liveWs && liveWs.readyState === 1) liveWs.send(JSON.stringify({ type: on ? 'pause' : 'resume' }));
  }
  function showHeard(text, interim) {
    $('heard').innerHTML = text ? (interim ? '<span class="interim">' + esc(text) + '</span>' : esc(text)) : '';
  }
  async function answer(text) {
    if (answering) return;
    answering = true;
    add('Master', '<div>' + esc(text) + '</div>');
    showHeard('');
    try { showAnswer(await api('/api/voice/turn', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ text, where: 'the voice test page' }) })); }
    catch (e) { add('Crundi', '<div class="err">' + esc(e.message) + '</div>'); }
    finally { answering = false; }
  }
  function stopLive() {
    if (liveRec && liveRec.state !== 'inactive') liveRec.stop();
    if (liveStream) liveStream.getTracks().forEach(t => t.stop());
    if (liveWs && liveWs.readyState === 1) liveWs.send(JSON.stringify({ type: 'stop' }));
    liveWs = liveRec = liveStream = null;
    $('live').classList.remove('on');
    $('live').textContent = 'Start listening';
  }
  async function startLive() {
    warm();
    try { liveStream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true } }); }
    catch (err) { add('Crundi', '<div class="err">Microphone not available: ' + esc(err.message) + '</div>'); return; }
    const proto = location.protocol === 'https:' ? 'wss' : 'ws';
    liveWs = new WebSocket(proto + '://' + location.host + '/api/voice/listen?token=' + encodeURIComponent(token()));
    liveWs.binaryType = 'arraybuffer';
    $('live').classList.add('on');
    $('live').textContent = 'Connecting…';
    liveWs.onmessage = (ev) => {
      let m; try { m = JSON.parse(ev.data); } catch { return; }
      if (m.type === 'ready') { $('live').textContent = 'Listening — tap to stop'; }
      else if (m.type === 'speech') { if (player) player.pause(); if ('speechSynthesis' in window) speechSynthesis.cancel(); }
      else if (m.type === 'interim') showHeard(m.text, true);
      else if (m.type === 'utterance') { showHeard(m.text); answer(m.text); }
      else if (m.type === 'error') add('Crundi', '<div class="err">' + esc(m.error) + '</div>');
    };
    liveWs.onclose = () => { if (liveWs) stopLive(); };
    liveRec = new MediaRecorder(liveStream);
    liveRec.ondataavailable = (ev) => { if (ev.data.size && liveWs && liveWs.readyState === 1) liveWs.send(ev.data); };
    liveWs.onopen = () => liveRec.start(250);
  }
  $('live').onclick = () => (liveWs ? stopLive() : startLive());

  // ─── voice picker (Deepgram Aura) ───
  let allVoices = [];
  function drawVoices() {
    const brit = $('britOnly').checked, male = $('maleOnly').checked;
    const list = allVoices.filter(v => (!brit || /brit|uk|england/i.test(v.accent) || !v.accent) && (!male || v.gender === 'male' || !v.gender));
    $('voiceNow').innerHTML = 'Replies are spoken with: <b>' + esc(ttsVoice ? (allVoices.find(v => v.model === ttsVoice) || { name: ttsVoice }).name + ' (Deepgram)' : "this device's voice") + '</b>';
    $('voices').innerHTML = list.length ? list.map(v =>
      '<div class="voice' + (v.model === ttsVoice ? ' chosen' : '') + '"><div class="meta"><b>' + esc(v.name) + '</b>'
      + '<small>' + esc([v.accent || 'accent not listed', v.gender, (v.tags || []).filter(t => !/^(masculine|feminine|male|female)$/i.test(t)).slice(0, 3).join(', '), v.aura2 ? 'Aura-2' : 'Aura'].filter(Boolean).join(' · ')) + '</small></div>'
      + '<button class="ghost" data-play="' + esc(v.model) + '">▶</button>'
      + '<button data-use="' + esc(v.model) + '">' + (v.model === ttsVoice ? 'In use' : 'Use') + '</button></div>'
    ).join('') : '<div class="spend">No voices match. Untick the filters.</div>';
  }
  async function loadVoices() {
    try {
      const r = await api('/api/voice/voices');
      if (!r.ok) { $('voices').innerHTML = '<div class="spend">' + esc(r.error) + '</div>'; return; }
      allVoices = r.voices;
      if (!r.fromDeepgram) add('Crundi', '<div class="spend">Deepgram&#39;s voice list could not be fetched; showing known Aura-2 voices, accents unknown.</div>');
      drawVoices();
    } catch (e) { $('voices').innerHTML = '<div class="spend err">' + esc(e.message) + '</div>'; }
  }
  async function setVoice(model) {
    const r = await api('/api/voice/settings', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ttsVoice: model }) });
    if (r.ok) { ttsVoice = r.ttsVoice || ''; drawVoices(); }
    else add('Crundi', '<div class="err">' + esc(r.error) + '</div>');
  }
  $('voices').addEventListener('click', (e) => {
    const play = e.target.closest('[data-play]'), use = e.target.closest('[data-use]');
    if (play) say($('sample').value, play.dataset.play);
    if (use) setVoice(use.dataset.use);
  });
  $('britOnly').onchange = $('maleOnly').onchange = drawVoices;
  $('useDevice').onclick = () => setVoice('');

  // ─── settings ───
  async function loadSettings() {
    try {
      const s = await api('/api/voice/settings');
      if (!s.ok) throw new Error(s.error || 'Could not load settings');
      $('model').innerHTML = s.models.map(m => '<option value="' + m.id + '"' + (m.id === s.model ? ' selected' : '') + '>' + esc(m.label) + '</option>').join('');
      if (document.activeElement !== $('limit')) $('limit').value = s.monthlyLimitUsd;
      if (document.activeElement !== $('bday')) $('bday').value = s.billingDay || 1;
      if (document.activeElement !== $('dday')) $('dday').value = s.deepgramBillingDay || 1;
      if (ttsVoice !== (s.ttsVoice || '')) { ttsVoice = s.ttsVoice || ''; drawVoices(); }
      if (s.deepgramKeySet && !allVoices.length) loadVoices();
      if (!s.deepgramKeySet) $('voices').innerHTML = '<div class="spend">Add a Deepgram key below to hear its voices.</div>';
      $('akeyState').innerHTML = s.anthropicKeySet ? '<span class="ok">· set</span>' : '<span class="err">· not set</span>';
      $('dkeyState').innerHTML = s.deepgramKeySet ? '<span class="ok">· set</span>' : '<span class="err">· not set</span>';
      const sp = s.spend;
      $('spend').innerHTML = 'This billing month (Claude restarts ' + esc(s.nextReset || '') + ', Deepgram ' + esc(s.deepgramNextReset || '') + '): $' + sp.totalUsd.toFixed(2) + ' of $' + s.monthlyLimitUsd
        + ' · Claude $' + sp.claudeUsd.toFixed(2) + ' · Deepgram $' + sp.deepgramUsd.toFixed(2) + ' · ' + sp.turns + ' requests'
        + (s.limitReached ? ' · <span class="err">limit reached</span>' : '');
    } catch (e) { $('spend').innerHTML = '<span class="err">' + esc(e.message) + '</span>'; }
  }
  $('save').onclick = async () => {
    const body = { model: $('model').value, monthlyLimitUsd: Number($('limit').value), billingDay: Number($('bday').value) || 1, deepgramBillingDay: Number($('dday').value) || 1 };
    if ($('akey').value.trim()) body.anthropicKey = $('akey').value.trim();
    if ($('dkey').value.trim()) body.deepgramKey = $('dkey').value.trim();
    const r = await api('/api/voice/settings', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    $('saved').innerHTML = r.ok ? '<span class="ok">Saved</span>' : '<span class="err">' + esc(r.error) + '</span>';
    $('akey').value = ''; $('dkey').value = '';
    loadSettings();
  };
  loadSettings();
  warm();
  $('text').addEventListener('focus', warm);
})();
</script>
</body></html>`;
}
