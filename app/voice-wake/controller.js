/**
 * "Hey Cru" on the PC (D40), the main-process side.
 *
 * When the setting is on and the PC is on charge, a hidden window holds the
 * microphone (listener.html) and streams 16 kHz PCM over IPC to a utility
 * process running the keyword spotter (worker.js). Nothing goes to the
 * network: the hidden window's session refuses every web request and grants
 * the microphone to that one window only.
 *
 * It listens only when all of these hold:
 *   - the setting is on (off by default, kept in client.json);
 *   - "Hey Cru" has been trained from the owner's voice (enroll.js): the
 *     profile is <userData>/voice-wake/profile.json, and without it an "on"
 *     setting counts as off;
 *   - no training is going on (the page has the microphone then);
 *   - the PC is on AC power (powerMonitor; checked again every minute too,
 *     since not every platform sends on-ac / on-battery);
 *   - the voice screen is not open in the main window (one microphone owner
 *     at a time; the page is asked every second whether it is open);
 *   - the screen is not locked and the PC is not asleep.
 *
 * On "Hey Cru" the main window is shown and brought to the front, and the
 * voice screen is opened in the page. A hit whose voice does not match the
 * profile is logged and ignored (worker.js).
 *
 * Training: the page records the five samples itself (it asks trainBegin()
 * first, which lets go of the microphone here), checks each with check(),
 * then hands all five to train(), which runs enroll.js train() in a
 * utility process of its own and saves the profile.
 */

import { join } from 'path';
import { pathToFileURL } from 'url';
import { readFileSync, writeFileSync, mkdirSync, chmodSync, renameSync, unlinkSync, existsSync } from 'fs';
import { ensureInstalled, isInstalled } from './install.js';
import { validProfile, checkSample, SAMPLES_NEEDED, RATE } from './enroll.js';
import { speakerDir, validVoice } from './owner.js';
import { ensureModel as ensureVoiceModel } from './voiceprint.js';

const POLL_MS = 1000;
const POWER_RECHECK_MS = 60 * 1000;
const RESUME_DELAY_MS = 800;
const RETRY_MS = 15 * 1000;
// Training gives the microphone back by itself if the page goes quiet
// (closed, reloaded) without saying it is done.
const TRAIN_IDLE_MS = 3 * 60 * 1000;
const TRAIN_TIMEOUT_MS = 2 * 60 * 1000;
const MAX_SAMPLE_SECONDS = 10;

const LABELS = {
  installing: 'Downloading the wake word model…',
  'on-battery': 'Paused: on battery',
  'voice-open': 'Paused: voice screen open',
  // Shown in place of voice-open (same state) while a chat has the microphone.
  'chat-voice': 'Paused: a chat is listening',
  paused: 'Paused: screen locked',
  training: 'Paused while training',
  starting: 'Starting…',
  listening: 'Listening',
};

// Asks the page to open the voice screen. Prefers a page function made for
// this, then the top-bar mic button (its click opens the screen and keeps it
// open). Says what it did.
const OPEN_SCRIPT = `(function () {
  var V = window.CrundiVoice;
  if (V && V.isOpen && V.holdsMic === false && typeof V.wake === 'function') { V.wake(); return 'woken'; }
  if (V && V.isOpen) return 'open';
  if (typeof window.crundiOpenVoice === 'function') { window.crundiOpenVoice({ how: 'wake' }); return 'called'; }
  var b = document.getElementById('voice-btn');
  if (b) { b.click(); return 'clicked'; }
  return 'none';
})()`;
// 'open' (it has the microphone), 'asleep' (open, but it let the microphone
// go: listen for "Hey Cru" to wake it), 'chat' (a chat is in voice mode or a
// push to talk, CrundiChat.voiceActive: the microphone is taken, and "Hey Cru"
// said there is words for that chat), or 'closed'. An older page: open or closed.
export const IS_OPEN_SCRIPT = "(function () { var V = window.CrundiVoice; if (!V || !V.isOpen) { var C = window.CrundiChat; return C && C.voiceActive === true ? 'chat' : 'closed'; } return V.holdsMic === false ? 'asleep' : 'open'; })()";

/**
 * @param {object} d
 * @param {typeof import('electron')} d.electron
 * @param {string} d.moduleDir     folder of this file (unpacked when packaged)
 * @param {string} d.dataDir       where the engine and model go (<userData>/voice-wake)
 * @param {() => boolean} d.getEnabled
 * @param {(on: boolean) => boolean} d.saveEnabled
 * @param {() => Electron.BrowserWindow|null} d.getMainWindow
 * @param {() => void} d.bringToFront
 * @param {(s: string) => void} d.log
 * @param {(status: object) => void} [d.onStatus]
 */
export function createVoiceWake(d) {
  const { BrowserWindow, utilityProcess, powerMonitor, session, net, ipcMain } = d.electron;
  const log = (s) => { try { d.log('[voice-wake] ' + s); } catch { /* ignore */ } };

  let enabled = !!d.getEnabled();
  let onBattery = false;
  let locked = false;
  let asleep = false;
  let voiceOpen = false;
  let chatVoice = false;        // the microphone is a chat's (voice mode), not Cru's
  let installing = null;        // Promise while downloading
  let installError = '';
  let micState = 'stopped';     // from the window: loaded | listening | stopped | error
  let micError = '';
  let lastHeard = null;         // { keyword, at }
  let win = null;               // the hidden window
  let child = null;             // the utility process
  let childReady = false;
  let pollTimer = null;
  let polling = false;
  let powerTimer = null;
  let retryTimer = null;
  let resumeAt = 0;
  let training = false;         // the page is recording samples
  let trainTimer = null;
  let trainingRun = null;       // Promise while train() runs

  const profileFile = join(d.dataDir, 'profile.json');
  let profile = loadProfile();
  // Listening needs both the setting and a trained profile.
  const on = () => enabled && !!profile;
  // The voice screen is open but asleep: listen for "Hey Cru" to wake it,
  // with the setting on or not and on battery too, as the phone does; only
  // the owner's trained voice is needed.
  let screenAsleep = false;
  const listening = () => on() || (screenAsleep && !!profile);

  const listenerHtml = join(d.moduleDir, 'listener.html');
  const listenerUrl = pathToFileURL(listenerHtml).href;
  const ses = session.fromPartition('crundi-voice-wake');   // in memory, never on disk

  // ─── the hidden window's session: microphone for that window, no network ───
  ses.setPermissionRequestHandler((wc, permission, cb, details) => {
    const types = (details && details.mediaTypes) || [];
    const ok = !!win && !win.isDestroyed() && wc === win.webContents && permission === 'media'
      && types.length > 0 && types.every((t) => t === 'audio')
      && String((details && details.requestingUrl) || '').startsWith(listenerUrl);
    if (!ok) log('refused ' + permission + ' to ' + ((details && details.requestingUrl) || '?'));
    cb(ok);
  });
  ses.setPermissionCheckHandler((wc, permission, _origin, details) =>
    !!win && !win.isDestroyed() && wc === win.webContents && permission === 'media' && (!details || !details.mediaType || details.mediaType === 'audio'));
  ses.webRequest.onBeforeRequest({ urls: ['http://*/*', 'https://*/*', 'ws://*/*', 'wss://*/*'] }, (_det, cb) => cb({ cancel: true }));

  function readPower() {
    try { return !!powerMonitor.isOnBatteryPower(); } catch { return false; }
  }

  // ─── the owner's profile ───
  function loadProfile() {
    try {
      if (!existsSync(profileFile)) return null;
      const p = JSON.parse(readFileSync(profileFile, 'utf8'));
      if (validProfile(p)) return p;
      log('profile.json is not usable; train again');
    } catch (err) { log('could not read profile.json: ' + err.message); }
    return null;
  }
  function saveProfile(p) {
    mkdirSync(d.dataDir, { recursive: true });
    const tmp = profileFile + '.tmp';
    // Owner-only: it is a description of the owner's voice.
    writeFileSync(tmp, JSON.stringify(p), { mode: 0o600 });
    try { chmodSync(tmp, 0o600); } catch { /* not on this platform */ }
    renameSync(tmp, profileFile);
  }

  function status() {
    let state;
    if (!listening()) state = 'off';
    else if (training) state = 'training';
    else if (installing) state = 'installing';
    else if (installError) state = 'error';
    else if (onBattery && !screenAsleep) state = 'on-battery';
    else if (asleep || locked) state = 'paused';
    else if (voiceOpen) state = 'voice-open';
    else if (micState === 'listening' && childReady) state = 'listening';
    else if (micState === 'error') state = 'error';
    else state = 'starting';
    const error = installError || (micState === 'error' ? micError : '');
    return {
      enabled: on(),
      state,
      label: state === 'error' ? 'Not listening: ' + String(error || 'error').slice(0, 80) : LABELS[state === 'voice-open' && chatVoice ? 'chat-voice' : state] || '',
      onBattery,
      installed: safeInstalled(),
      installing: !!installing,
      error,
      lastHeard,
      trained: !!profile,
      trainedAt: profile ? String(profile.trainedAt || '') : '',
      // The owner's voice check on each "Hey Cru" (owner.js): 'on', or 'off' for a profile trained before it (or without the model).
      voiceCheck: profile ? (validVoice(profile.voice) ? 'on' : 'off') : '',
      training,
    };
  }
  function safeInstalled() { try { return isInstalled(d.dataDir); } catch { return false; } }

  let lastSent = '';
  function emit() {
    const s = status();
    const key = JSON.stringify(s);
    if (key === lastSent) return;
    lastSent = key;
    log('status ' + s.state + (s.error ? ' (' + s.error + ')' : ''));
    try { d.onStatus && d.onStatus(s); } catch { /* ignore */ }
  }

  const wanted = () => listening() && !training && (!onBattery || screenAsleep) && !locked && !asleep && !voiceOpen && !installing && !installError;

  // ─── the spotter's process ───
  function startChild() {
    if (child) return;
    childReady = false;
    child = utilityProcess.fork(join(d.moduleDir, 'worker.js'), [], { serviceName: 'Crundi Hey Cru', stdio: 'pipe' });
    const me = child;
    // The engine reports a bad model or keyword on stderr before it exits.
    const tell = (buf) => String(buf).split('\n').filter(Boolean).forEach((l) => log('spotter: ' + l.slice(0, 300)));
    try { me.stdout && me.stdout.on('data', tell); me.stderr && me.stderr.on('data', tell); } catch { /* no pipes */ }
    me.on('message', (m) => {
      if (me !== child || !m) return;
      if (m.type === 'ready') { childReady = true; emit(); }
      else if (m.type === 'error') { log('spotter: ' + m.message); }
      else if (m.type === 'detected') heard(m.keyword, m.score, m.via, m.voice);
      else if (m.type === 'rejected' && m.why) log('heard ' + m.keyword + ' via ' + m.via + ', but ' + m.why + ' (voice score ' + m.voice + ')');
      else if (m.type === 'rejected') log('heard ' + m.keyword + ' but not the trained voice (score ' + m.score + ', limit ' + (profile ? profile.limit : '?') + ')');
    });
    me.on('exit', (code) => {
      if (me !== child) return;
      child = null; childReady = false;
      log('spotter process ended (' + code + ')');
      if (wanted()) scheduleRetry();
      emit();
    });
    me.postMessage({ type: 'init', dir: d.dataDir, profile });
  }
  function stopChild() {
    const c = child; child = null; childReady = false;
    if (c) { try { c.kill(); } catch { /* gone */ } }
  }

  // ─── the microphone window ───
  function startWindow() {
    if (win && !win.isDestroyed()) { send('start'); return; }
    micState = 'stopped'; micError = '';
    win = new BrowserWindow({
      show: false, width: 200, height: 100, skipTaskbar: true, focusable: false,
      title: 'Hey Cru',
      webPreferences: {
        preload: join(d.moduleDir, 'listener-preload.cjs'),
        partition: 'crundi-voice-wake',
        sandbox: true, contextIsolation: true, nodeIntegration: false,
        backgroundThrottling: false, spellcheck: false, webgl: false,
      },
    });
    const w = win;
    w.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
    w.webContents.on('will-navigate', (e) => e.preventDefault());
    w.webContents.on('render-process-gone', (_e, det) => {
      if (w !== win) return;
      log('listener window gone: ' + (det && det.reason));
      try { w.destroy(); } catch { /* gone */ }
      win = null; micState = 'stopped';
      if (wanted()) scheduleRetry();
      emit();
    });
    w.on('closed', () => { if (w === win) { win = null; micState = 'stopped'; } });
    w.loadFile(listenerHtml).catch((err) => log('listener load failed: ' + err.message));
  }
  function send(cmd) {
    if (win && !win.isDestroyed()) { try { win.webContents.send('voice-wake:command', cmd); } catch { /* gone */ } }
  }
  function stopWindow() {
    const w = win; win = null; micState = 'stopped';
    if (w && !w.isDestroyed()) { try { w.destroy(); } catch { /* gone */ } }
  }

  ipcMain.on('voice-wake:state', (e, state, detail) => {
    if (!win || win.isDestroyed() || e.sender !== win.webContents) return;
    if (state === 'loaded') { if (wanted()) send('start'); return; }
    micState = state; micError = state === 'error' ? String(detail || '') : '';
    if (state === 'error') { log('microphone: ' + micError); stopWindow(); if (wanted()) scheduleRetry(); }
    emit();
  });
  ipcMain.on('voice-wake:pcm', (e, samples) => {
    if (!win || e.sender !== win.webContents || !child || !childReady || !wanted()) return;
    try { child.postMessage({ type: 'pcm', samples }); } catch { /* the exit handler tidies up */ }
  });

  function scheduleRetry() {
    if (retryTimer) return;
    retryTimer = setTimeout(() => { retryTimer = null; reconcile(); }, RETRY_MS);
  }

  // ─── the voice screen in the page ───
  function mainWc() {
    const w = d.getMainWindow();
    return w && !w.isDestroyed() ? w.webContents : null;
  }
  function startPolling() {
    if (pollTimer) return;
    pollTimer = setInterval(pollVoice, POLL_MS);
  }
  function stopPolling() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
  }
  function pollVoice() {
    const wc = mainWc();
    if (!wc) { setVoiceOpen(false); return; }
    if (polling || wc.isLoading()) return;
    polling = true;
    const t = setTimeout(() => { polling = false; }, 3000);
    wc.executeJavaScript(IS_OPEN_SCRIPT, false).then((open) => {
      clearTimeout(t); polling = false;
      const st = open === true ? 'open' : open === false ? 'closed' : String(open || 'closed');
      const wasAsleep = screenAsleep;
      screenAsleep = st === 'asleep';
      const wasChat = chatVoice;
      chatVoice = st === 'chat';
      setVoiceOpen(st === 'open' || st === 'chat');
      if (wasChat !== chatVoice) emit();
      if (wasAsleep !== screenAsleep) { reconcile(); emit(); }
    }, () => { clearTimeout(t); polling = false; screenAsleep = false; chatVoice = false; setVoiceOpen(false); });
  }
  function setVoiceOpen(open) {
    if (open === voiceOpen) {
      if (!open && resumeAt && Date.now() >= resumeAt) { resumeAt = 0; reconcile(); }
      return;
    }
    voiceOpen = open;
    // Give the voice screen a moment to let go of the microphone.
    if (!open) resumeAt = Date.now() + RESUME_DELAY_MS;
    else reconcile();
    emit();
  }

  function heard(keyword, score, via, voice) {
    lastHeard = { keyword: String(keyword || ''), at: Date.now() };
    // Which way it woke (enroll.js): 'phrase' (a burst matched the owner's takes, the model agreeing) or 'model' (the spotter fired, the voice matched).
    log('heard ' + lastHeard.keyword + (score != null ? ' (match ' + score + ')' : '') + (via ? ' via ' + via : '') + (voice != null ? ', owner\'s voice ' + voice : ''));
    // Listening only for an asleep voice screen (background listening off): a hit counts only if the
    // page still has one, asked now. The state polled each second can be stale just after Cru closed
    // the screen, and then a hit opened Cru again (12 Nov, the owner): after a close, only an
    // explicit open brings it back.
    if (!on()) {
      const wc0 = mainWc();
      if (!wc0 || wc0.isLoading()) { log('ignored: no asleep voice screen'); return; }
      wc0.executeJavaScript(IS_OPEN_SCRIPT, false).then((st) => {
        if (st !== 'asleep') { screenAsleep = false; log('ignored: the voice screen is ' + st + ', and background listening is off'); reconcile(); emit(); return; }
        wakeNow();
      }, () => log('ignored: could not ask the page'));
      return;
    }
    wakeNow();
  }

  function wakeNow() {
    // Hand the microphone over before the voice screen asks for it.
    voiceOpen = true; resumeAt = 0;
    reconcile();
    try { d.bringToFront(); } catch (err) { log('could not show the window: ' + err.message); }
    const wc = mainWc();
    if (!wc) { voiceOpen = false; resumeAt = Date.now() + RESUME_DELAY_MS; emit(); return; }
    const run = () => wc.executeJavaScript(OPEN_SCRIPT, true).then((how) => {
      log('voice screen: ' + how);
      // Nothing to open (the setup page, an older server): listen again.
      if (how === 'none') { voiceOpen = false; resumeAt = Date.now() + RESUME_DELAY_MS; }
      emit();
    }, (err) => { log('could not open the voice screen: ' + err.message); voiceOpen = false; resumeAt = Date.now() + RESUME_DELAY_MS; emit(); });
    if (wc.isLoading()) wc.once('did-finish-load', run); else run();
  }

  // ─── putting it together ───
  function install() {
    if (installing || safeInstalled()) return installing || Promise.resolve();
    installError = '';
    const fetchImpl = net && net.fetch ? (u, o) => net.fetch(u, o) : globalThis.fetch;
    installing = ensureInstalled(d.dataDir, { fetch: fetchImpl, log: (m) => { try { d.log(m); } catch { /* ignore */ } } })
      .catch((err) => { installError = String(err && err.message || err); log('install failed: ' + installError); })
      .finally(() => { installing = null; reconcile(); emit(); });
    const p = installing;
    emit();
    return p;
  }
  /** Install now whatever the setting (training needs the engine). */
  async function installed() {
    if (safeInstalled()) return;
    installError = '';
    await install();
    if (!safeInstalled()) throw new Error(installError || 'The wake word model could not be downloaded');
  }

  // ─── training ───
  function touchTraining() {
    if (trainTimer) clearTimeout(trainTimer);
    trainTimer = training ? setTimeout(() => { log('training left unfinished; listening again'); endTraining(); }, TRAIN_IDLE_MS) : null;
  }
  function beginTraining() {
    if (!training) log('training started');
    training = true;
    touchTraining();
    reconcile();     // lets go of the microphone
    // Fetch the engine and model while the samples are recorded.
    if (!safeInstalled() && !installing) install();
    return status();
  }
  function endTraining() {
    if (training) log('training ended');
    training = false;
    touchTraining();
    reconcile();
    return status();
  }
  function f32(x) {
    if (x instanceof Float32Array) return x;
    if (x && (Array.isArray(x) || ArrayBuffer.isView(x))) return new Float32Array(x);
    return new Float32Array(0);
  }
  function runTraining(recordings) {
    return new Promise((resolve) => {
      let p;
      try {
        p = utilityProcess.fork(join(d.moduleDir, 'worker.js'), [], { serviceName: 'Crundi Hey Cru training', stdio: 'pipe' });
      } catch (err) { resolve({ ok: false, why: 'Could not start training: ' + err.message, redo: [] }); return; }
      let done = false;
      const finish = (r) => {
        if (done) return;
        done = true; clearTimeout(t);
        try { p.kill(); } catch { /* gone */ }
        resolve(r);
      };
      const t = setTimeout(() => finish({ ok: false, why: 'Setting up took too long. Try again.', redo: [] }), TRAIN_TIMEOUT_MS);
      const tell = (buf) => String(buf).split('\n').filter(Boolean).forEach((l) => log('training: ' + l.slice(0, 300)));
      try { p.stdout && p.stdout.on('data', tell); p.stderr && p.stderr.on('data', tell); } catch { /* no pipes */ }
      p.on('message', (m) => {
        if (m && m.type === 'trained') finish(m.result || { ok: false, why: 'Training gave no answer', redo: [] });
        else if (m && m.type === 'error') finish({ ok: false, why: 'Training failed: ' + m.message, redo: [] });
      });
      p.on('exit', (code) => finish({ ok: false, why: 'Training stopped unexpectedly (' + code + ')', redo: [] }));
      p.postMessage({ type: 'train', id: 1, dir: d.dataDir, recordings });
    });
  }
  async function trainNow(list) {
    if (!Array.isArray(list) || list.length !== SAMPLES_NEEDED) return { ok: false, why: 'Needs ' + SAMPLES_NEEDED + ' recordings', redo: [] };
    const recs = list.map(f32).map((r) => (r.length > RATE * MAX_SAMPLE_SECONDS ? r.subarray(0, RATE * MAX_SAMPLE_SECONDS) : r));
    try { await installed(); } catch (err) { return { ok: false, why: String(err.message || err), redo: [] }; }
    // The owner's voice model (owner.js), fetched here so training does not wait on the download; without it, training still works.
    try { await ensureVoiceModel(speakerDir(d.dataDir), { fetchImpl: (u, o) => fetch(u, { ...(o || {}), signal: AbortSignal.timeout(90_000) }) }); }
    catch (err) { log('voice model: ' + String(err.message || err)); }
    const r = await runTraining(recs);
    if (!r || !r.ok) {
      log('training: ' + ((r && r.why) || 'failed') + (r && r.redo && r.redo.length ? ' (again: ' + r.redo.map((i) => i + 1).join(', ') + ')' : ''));
      return { ok: false, why: String((r && r.why) || 'Training failed'), redo: Array.isArray(r && r.redo) ? r.redo : [] };
    }
    if (!validProfile(r.profile)) return { ok: false, why: 'Training gave an unusable result', redo: [] };
    try { saveProfile(r.profile); } catch (err) { return { ok: false, why: 'Could not save the profile: ' + err.message, redo: [] }; }
    profile = r.profile;
    log('trained (threshold ' + profile.keywordsThreshold + ', limit ' + profile.limit + ', owner\'s voice check ' + (validVoice(profile.voice) ? 'on' : 'off: ' + (r.voiceWhy || '?')) + ')');
    if (child) { try { child.postMessage({ type: 'profile', profile }); } catch { /* the exit handler tidies up */ } }
    reconcile();
    return { ok: true, trainedAt: String(profile.trainedAt || ''), voiceCheck: validVoice(profile.voice) ? 'on' : 'off', note: String(r.voiceWhy || '') };
  }

  function reconcile() {
    if (listening() && !safeInstalled() && !installing && !installError) { install(); return; }
    // Watching for the voice screen matters whenever we might be listening:
    // also with the setting off, once trained, for an asleep voice screen.
    if ((on() && !onBattery) || profile) startPolling(); else stopPolling();
    if (wanted()) {
      startChild();
      startWindow();
    } else {
      stopWindow();
      // The spotter's process stays warm through a voice screen or a lock;
      // off or on battery, it goes too.
      if (!listening() || (onBattery && !screenAsleep) || installError) stopChild();
    }
    emit();
  }

  powerMonitor.on('on-ac', () => { onBattery = false; reconcile(); });
  powerMonitor.on('on-battery', () => { onBattery = true; reconcile(); });
  powerMonitor.on('lock-screen', () => { locked = true; reconcile(); });
  powerMonitor.on('unlock-screen', () => { locked = false; reconcile(); });
  powerMonitor.on('suspend', () => { asleep = true; reconcile(); });
  powerMonitor.on('resume', () => {
    asleep = false;
    // The microphone a sleep interrupted does not come back by itself.
    stopWindow();
    onBattery = readPower();
    reconcile();
  });

  onBattery = readPower();
  powerTimer = setInterval(() => {
    const b = readPower();
    if (b !== onBattery) { onBattery = b; reconcile(); }
  }, POWER_RECHECK_MS);
  reconcile();

  return {
    status,
    get: () => on(),
    /** Turn the option on or off; saves it. Returns the status. */
    set(want) {
      want = !!want;
      if (want && !profile) return { ...status(), error: 'Train Hey Cru first' };
      if (want !== enabled) {
        if (!d.saveEnabled(want)) return { ...status(), error: 'Could not save the setting' };
        enabled = want;
        if (want) installError = '';   // turning it on again retries a failed download
        log(want ? 'turned on' : 'turned off');
      }
      reconcile();
      return status();
    },
    /** One recording, checked as it is made: { ok, why, ms }. */
    check(samples) {
      if (training) touchTraining();
      const r = checkSample(f32(samples));
      return { ok: !!r.ok, why: r.why || '', ms: r.clip ? Math.round(r.clip.length / RATE * 1000) : 0 };
    },
    /** The page is about to record: let go of the microphone. Returns the status. */
    trainBegin: () => beginTraining(),
    /** The page is done recording (trained, cancelled or closed). Returns the status. */
    trainEnd: () => endTraining(),
    /** Five recordings -> { ok: true, trainedAt } | { ok: false, why, redo }. Saves the profile. */
    train(recordings) {
      if (trainingRun) return Promise.resolve({ ok: false, why: 'Already setting up', redo: [] });
      if (training) touchTraining();
      trainingRun = trainNow(recordings).finally(() => { trainingRun = null; if (training) touchTraining(); });
      return trainingRun;
    },
    /** Delete the profile and turn listening off. Returns the status. */
    forget() {
      try { unlinkSync(profileFile); } catch (err) { if (err.code !== 'ENOENT') return { ...status(), error: 'Could not delete the profile: ' + err.message }; }
      profile = null;
      if (enabled && d.saveEnabled(false)) enabled = false;
      log('profile forgotten');
      reconcile();
      return status();
    },
    stop() {
      stopPolling();
      if (powerTimer) clearInterval(powerTimer);
      if (retryTimer) clearTimeout(retryTimer);
      if (trainTimer) clearTimeout(trainTimer);
      stopWindow();
      stopChild();
    },
  };
}
