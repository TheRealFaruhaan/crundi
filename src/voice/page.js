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
<title>Cru, the voice assistant</title>
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
<h1>Cru</h1>
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
  <input id="sample" value="Yes, Sir. Your weekly usage is at forty two percent, and all four services are running.">
  <div class="row">
    <label style="display:flex;align-items:center;gap:6px;margin:0"><input type="checkbox" id="britOnly" style="width:auto" checked> British only</label>
    <label style="display:flex;align-items:center;gap:6px;margin:0"><input type="checkbox" id="maleOnly" style="width:auto"> Male only</label>
    <button class="ghost" id="useDevice" style="margin-left:auto">Use this device's voice</button>
  </div>
  <div class="voices" id="voices"></div>
</section>

<section class="card">
  <h2>Settings</h2>
  <label for="brain">Brain (an experiment: the Claude API, paid from the key below, or your own Claude Code in JSON mode, on your subscription)</label>
  <select id="brain"><option value="api">Claude API</option><option value="claude-code">Claude Code</option></select>
  <label for="model">Model (both brains)</label>
  <select id="model"></select>
  <label for="limit">Claude API monthly limit (US$)</label>
  <input id="limit" type="number" min="0" step="1">
  <label for="dlimit">Deepgram monthly limit (US$; at it, the phone's own voice and recogniser take over)</label>
  <input id="dlimit" type="number" min="0" step="1">
  <label for="bday">Claude API billing day (the day of the month the Claude API is billed; its spend restarts then)</label>
  <input id="bday" type="number" min="1" max="31" step="1">
  <label for="dday">Deepgram billing day</label>
  <input id="dday" type="number" min="1" max="31" step="1">
  <label for="pause">Pause before sending (seconds; how long you may pause before what you said is sent)</label>
  <input id="pause" type="number" min="1" max="5" step="0.5">
  <label for="rate">Talking speed (1 is normal; 0.7 slowest, 1.5 fastest)</label>
  <input id="rate" type="number" min="0.7" max="1.5" step="0.05">
  <label for="sclose">Close the voice screen after this much silence (seconds; "Hey Cru" opens it again)</label>
  <input id="sclose" type="number" min="15" max="600" step="5">
  <label for="idle">Stop Claude Code after (minutes without a turn; Claude Code brain only)</label>
  <input id="idle" type="number" min="1" max="240" step="1">
  <label for="akey">Claude API key <span id="akeyState"></span></label>
  <input id="akey" type="password" autocomplete="off" placeholder="Leave empty to keep the current key">
  <label for="xkey">Anthropic Admin API key, optional: to show the whole account's usage <span id="xkeyState"></span></label>
  <input id="xkey" type="password" autocomplete="off" placeholder="sk-ant-admin… Leave empty to keep the current key">
  <label for="dkey">Deepgram API key <span id="dkeyState"></span></label>
  <input id="dkey" type="password" autocomplete="off" placeholder="Leave empty to keep the current key">
  <div class="row"><button id="save">Save</button><span id="saved"></span></div>
  <div class="spend" id="spend"></div>
  <h2 style="margin-top:18px">Brains compared</h2>
  <div class="spend" id="brainStats">Nothing logged yet.</div>
</section>
<section class="card">
  <h2>Memory</h2>
  <div class="spend" id="memInfo"></div>
  <label for="memQ">Search what was said before</label>
  <input id="memQ" placeholder="e.g. the kanban card about the home screen">
  <div id="memHits"></div>
  <label>Notebook (what Cru keeps for good; edit or empty a file to change it)</label>
  <div id="memFiles"></div>
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
      u.rate = Number(($('rate') || {}).value) || 1;
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
  /** Speak, pausing live listening so Cru doesn't hear itself. */
  async function say(text, model = ttsVoice) {
    if (!text || $('mute').checked) return;
    pauseLive(true);
    try {
      if (model) await deepgramSay(text, model);
      else await browserSay(text);
    } catch (e) {
      add('Cru', '<div class="err">' + esc(e.message) + " — using this device's voice.</div>");
      await browserSay(text);
    } finally { pauseLive(false); }
  }

  function showAnswer(r) {
    if (r.heard !== undefined) add('Heard', '<div>' + esc(r.heard || '(nothing)') + '</div>');
    if (!r.ok) { add('Cru', '<div class="err">' + esc(r.error) + '</div>'); return; }
    const c = r.cache || {};
    const hit = (c.read + c.written + c.uncached) ? Math.round(100 * c.read / (c.read + c.written + c.uncached)) : 0;
    add('Cru' + (r.tools && r.tools.length ? ' · looked at ' + r.tools.join(', ') : '') + (r.costUsd ? ' · $' + r.costUsd.toFixed(4) : '') + (c.read !== undefined ? ' · ' + hit + '% from cache' : ''),
      '<div class="speak">' + esc(r.speak) + '</div>' + (r.show ? '<pre>' + esc(r.show) + '</pre>' : ''));
    say(r.speak);
    loadSettings();
  }

  async function send() {
    const text = $('text').value.trim();
    if (!text) return;
    $('send').disabled = true;
    add('You', '<div>' + esc(text) + '</div>');
    $('text').value = '';
    try { showAnswer(await api('/api/voice/turn', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ text, where: 'the voice test page' }) })); }
    catch (e) { add('Cru', '<div class="err">' + esc(e.message) + '</div>'); }
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
    catch (err) { add('Cru', '<div class="err">Microphone not available: ' + esc(err.message) + '</div>'); return; }
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
      } catch (err) { add('Cru', '<div class="err">' + esc(err.message) + '</div>'); }
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
    add('You', '<div>' + esc(text) + '</div>');
    showHeard('');
    try { showAnswer(await api('/api/voice/turn', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ text, where: 'the voice test page' }) })); }
    catch (e) { add('Cru', '<div class="err">' + esc(e.message) + '</div>'); }
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
    catch (err) { add('Cru', '<div class="err">Microphone not available: ' + esc(err.message) + '</div>'); return; }
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
      else if (m.type === 'error') add('Cru', '<div class="err">' + esc(m.error) + '</div>');
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
      if (!r.fromDeepgram) add('Cru', '<div class="spend">Deepgram&#39;s voice list could not be fetched; showing known Aura-2 voices, accents unknown.</div>');
      drawVoices();
    } catch (e) { $('voices').innerHTML = '<div class="spend err">' + esc(e.message) + '</div>'; }
  }
  async function setVoice(model) {
    const r = await api('/api/voice/settings', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ttsVoice: model }) });
    if (r.ok) { ttsVoice = r.ttsVoice || ''; drawVoices(); }
    else add('Cru', '<div class="err">' + esc(r.error) + '</div>');
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
      if (document.activeElement !== $('dlimit')) $('dlimit').value = s.deepgramLimitUsd;
      if (document.activeElement !== $('bday')) $('bday').value = s.billingDay || 1;
      if (document.activeElement !== $('dday')) $('dday').value = s.deepgramBillingDay || 1;
      if (document.activeElement !== $('pause')) $('pause').value = (s.pauseMs || 2000) / 1000;
      if (document.activeElement !== $('rate')) $('rate').value = s.speechRate || 1;
      if (document.activeElement !== $('idle')) $('idle').value = s.cliIdleMinutes || 30;
      if (document.activeElement !== $('sclose')) $('sclose').value = s.screenCloseSeconds || 60;
      if (document.activeElement !== $('brain')) $('brain').value = s.brain || 'api';
      loadBrainStats();
      if (ttsVoice !== (s.ttsVoice || '')) { ttsVoice = s.ttsVoice || ''; drawVoices(); }
      if (s.deepgramKeySet && !allVoices.length) loadVoices();
      if (!s.deepgramKeySet) $('voices').innerHTML = '<div class="spend">Add a Deepgram key below to hear its voices.</div>';
      $('akeyState').innerHTML = s.anthropicKeySet ? '<span class="ok">· set</span>' : '<span class="err">· not set</span>';
      $('xkeyState').innerHTML = s.anthropicAdminKeySet ? '<span class="ok">· set</span>' : '<span>· not set</span>';
      $('dkeyState').innerHTML = s.deepgramKeySet ? '<span class="ok">· set</span>' : '<span class="err">· not set</span>';
      const sp = s.spend;
      $('spend').innerHTML = 'Claude API: $' + sp.claudeUsd.toFixed(2) + ' of $' + s.monthlyLimitUsd + ' (restarts ' + esc(s.nextReset || '') + ', ' + sp.turns + ' requests)'
        + (s.limitReached ? ' <span class="err">limit reached</span>' : '')
        + '<br>Deepgram: $' + sp.deepgramUsd.toFixed(2) + ' of $' + s.deepgramLimitUsd + ' (restarts ' + esc(s.deepgramNextReset || '') + ')'
        + (s.deepgramLimitReached ? ' <span class="err">limit reached</span>' : '');
    } catch (e) { $('spend').innerHTML = '<span class="err">' + esc(e.message) + '</span>'; }
  }
  // The brain experiment: speed, cost and cache use per brain and model (Claude Code's cost is what it reports at list price; the subscription pays).
  async function loadBrainStats() {
    try {
      const r = await api('/api/voice/brain-stats');
      if (!r.ok || !r.stats || !r.stats.length) return;
      const secs = (ms) => (ms / 1000).toFixed(1) + ' s';
      $('brainStats').innerHTML = '<table style="width:100%;border-collapse:collapse"><tr><th align="left">Brain · model</th><th align="right">Turns</th><th align="right">Median</th><th align="right">p90</th><th align="right">$/turn</th><th align="right">Cached</th></tr>'
        + r.stats.map(g => '<tr><td>' + esc(g.brain === 'claude-code' ? 'Claude Code' : 'API') + ' · ' + esc(g.model.replace('claude-', '')) + '</td><td align="right">' + g.turns + (g.failed ? ' (' + g.failed + ' failed)' : '') + '</td><td align="right">' + secs(g.medianMs) + '</td><td align="right">' + secs(g.p90Ms) + '</td><td align="right">' + g.costPerTurnUsd.toFixed(4) + '</td><td align="right">' + Math.round(g.cacheReadShare * 100) + '%</td></tr>').join('')
        + '</table><div style="margin-top:6px">Answering now: ' + (r.brain === 'claude-code' ? 'Claude Code' : 'Claude API') + '. Claude Code costs are at list price, for comparison; your subscription pays them.</div>';
    } catch (e) { /* stats are optional */ }
  }
  $('save').onclick = async () => {
    const body = { model: $('model').value, monthlyLimitUsd: Number($('limit').value), deepgramLimitUsd: Number($('dlimit').value), billingDay: Number($('bday').value) || 1, deepgramBillingDay: Number($('dday').value) || 1, pauseMs: Math.round((Number($('pause').value) || 2) * 1000), speechRate: Number($('rate').value) || 1, cliIdleMinutes: Math.round(Number($('idle').value) || 30), screenCloseSeconds: Math.round(Number($('sclose').value) || 60), brain: $('brain').value };
    if ($('akey').value.trim()) body.anthropicKey = $('akey').value.trim();
    if ($('dkey').value.trim()) body.deepgramKey = $('dkey').value.trim();
    if ($('xkey').value.trim()) body.anthropicAdminKey = $('xkey').value.trim();
    const r = await api('/api/voice/settings', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    $('saved').innerHTML = r.ok ? '<span class="ok">Saved</span>' : '<span class="err">' + esc(r.error) + '</span>';
    $('akey').value = ''; $('dkey').value = ''; $('xkey').value = '';
    loadSettings();
  };
  loadSettings();
  // ─── memory (Phase 2): the notebook and search ───
  async function loadMemory() {
    try {
      const r = await api('/api/voice/memory');
      if (!r.ok) { $('memInfo').textContent = r.error || 'Memory is not available on this server.'; return; }
      $('memInfo').textContent = r.entries + ' things remembered · meaning search: ' + r.meaning;
      $('memFiles').innerHTML = r.files.length ? '' : '<div class="spend">Nothing in the notebook yet.</div>';
      for (const f of r.files) {
        const box = document.createElement('div');
        box.innerHTML = '<div class="spend">' + esc(f.path) + '</div><textarea rows="5" style="width:100%"></textarea><button>Save</button>';
        box.querySelector('textarea').value = f.text;
        box.querySelector('button').onclick = async () => {
          const s = await api('/api/voice/memory', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ path: f.path, text: box.querySelector('textarea').value }) });
          box.querySelector('button').textContent = s.ok ? 'Saved' : (s.error || 'Not saved');
          if (s.ok) setTimeout(loadMemory, 600);
        };
        $('memFiles').appendChild(box);
      }
    } catch (e) { $('memInfo').textContent = e.message; }
  }
  let memTimer = null;
  $('memQ').addEventListener('input', () => {
    clearTimeout(memTimer);
    memTimer = setTimeout(async () => {
      const q = $('memQ').value.trim();
      if (!q) { $('memHits').innerHTML = ''; return; }
      const r = await api('/api/voice/memory/search?q=' + encodeURIComponent(q));
      $('memHits').innerHTML = (r.results || []).map(h => '<div class="spend"><b>' + esc(h.kind) + '</b> ' + esc(h.at.slice(0, 16).replace('T', ' ')) + '<br>' + esc(h.text).slice(0, 400) + '</div>').join('') || '<div class="spend">Nothing found.</div>';
    }, 350);
  });
  loadMemory();
  warm();
  $('text').addEventListener('focus', warm);
})();
</script>
</body></html>`;
}
