/**
 * voice/stream.js — live speech to text: the client's microphone, relayed to
 * Deepgram's streaming API while the owner speaks.
 *
 *   client ──audio chunks──▶ /api/voice/listen ──▶ Deepgram /v1/listen (WebSocket)
 *   client ◀──words as they're heard, then each finished utterance──┘
 *
 * The client sends the microphone as it records (WebM/Opus from a browser's
 * MediaRecorder; Deepgram reads the container itself) and gets back:
 *
 *   { type: 'ready', echoFilter: true }       Deepgram is listening; this server
 *                                             takes 'saying' / 'said' (below)
 *   { type: 'interim', text }                 words so far, may still change
 *   { type: 'utterance', text }               the owner finished a sentence
 *   { type: 'speech' }                        the owner started speaking
 *   { type: 'error', error }                  and the socket closes
 *
 * A client text message { type: 'stop' } ends the stream; { type: 'pause' } /
 * { type: 'resume' } stop and restart forwarding audio (while Crundi itself is
 * talking, so it doesn't transcribe its own voice) — Deepgram is kept alive in
 * between.
 *
 * Talking over Cru (barge-in): a newer client keeps the audio flowing while
 * Cru speaks and says what it is saying, { type: 'saying', text }, then
 * { type: 'said' } when it stops. Finished phrases that are Cru's own words
 * coming back through the microphone (isEcho) are dropped here, for as long
 * as it speaks and ECHO_TAIL_MS after, so what reaches the client is the
 * owner's words alone and the client can cut Cru off the moment they come.
 * A client that never sends 'saying' gets everything, as before.
 *
 * The key stays here; the client never sees it. Seconds of audio Deepgram
 * reports are added to the month's voice spend when the stream ends.
 */

import WebSocket from 'ws';
import { getSettings, addDeepgramSpend, deepgramOverLimit } from './settings.js';
import { keyterms } from './stt.js';

const STREAM_URL = 'wss://api.deepgram.com/v1/listen';
/** Deepgram closes a stream after about 10 s without audio; it asks for a KeepAlive every 3–5 s. */
const KEEPALIVE_MS = 4000;

/**
 * Raw audio formats a client may declare (?encoding=&sample_rate= on
 * /api/voice/listen). The phone records 16-bit PCM; a browser sends WebM/Opus
 * and declares nothing, because Deepgram reads the container itself.
 */
export const RAW_ENCODINGS = ['linear16'];
export const SAMPLE_RATES = [8000, 16000, 24000, 44100, 48000];

/** The client's declared format, or null for a container (WebM, Ogg) Deepgram reads itself. */
export function rawAudio({ encoding, sample_rate: rate } = {}) {
  if (!encoding) return null;
  const n = Number(rate);
  if (!RAW_ENCODINGS.includes(String(encoding)) || !SAMPLE_RATES.includes(n)) return null;
  return { encoding: String(encoding), sample_rate: n };
}

/** How long the owner may pause before what they said is sent (D44): settings.pauseMs, within these. */
export const PAUSE_MS = { min: 1000, max: 5000, default: 2000 };

/**
 * The streaming URL: Nova-3, words as they're heard, end of speech detected.
 * What the owner says is complete only after `pauseMs` without a word
 * (UtteranceEnd), so a slow speaker with pauses isn't cut off mid-sentence.
 */
export function streamUrl(terms, { language = 'en', audio = null, pauseMs = PAUSE_MS.default } = {}) {
  const pause = Math.min(PAUSE_MS.max, Math.max(PAUSE_MS.min, Math.round(Number(pauseMs) || PAUSE_MS.default)));
  const q = new URLSearchParams({
    model: 'nova-3', language,
    smart_format: 'true', punctuate: 'true',
    interim_results: 'true',      // words while still speaking
    endpointing: '500',           // a short pause finalises a phrase (not the utterance)
    utterance_end_ms: String(pause), // this long with no words ends the utterance
    vad_events: 'true',           // "speech started"
  });
  if (audio) { q.set('encoding', audio.encoding); q.set('sample_rate', String(audio.sample_rate)); q.set('channels', '1'); }
  for (const t of terms) q.append('keyterm', t);
  return `${STREAM_URL}?${q}`;
}

/** After Cru stops, its last words may still be finalised for this long. */
export const ECHO_TAIL_MS = 2500;
/** A 'saying' with no 'said' (a client gone quiet) is forgotten after this. */
const SAYING_MAX_MS = 120_000;

function words(t) { return String(t || '').toLowerCase().split(/[^\p{L}\p{N}']+/u).filter(Boolean); }

/**
 * [heard] is Cru's own [said] picked up by the microphone: most of its words
 * (60%) are words of what was said; one word alone only when Cru said three
 * or fewer. The same rule as the phone's Speech.isEcho and crundi-voice.js.
 */
export function isEcho(heard, said) {
  const h = words(heard), s = words(said);
  if (h.length < 2) return h.length === 1 && s.includes(h[0]) && s.length <= 3;
  const set = new Set(s);
  return h.filter(w => set.has(w)).length / h.length >= 0.6;
}

/**
 * Turns Deepgram's messages into utterances. Pure, so it can be tested.
 * Finished phrases (is_final) are joined; the utterance is complete at
 * UtteranceEnd only: speech_final comes after a short pause, and the owner may
 * pause mid-sentence (D44).
 */
export function createAssembler({ now = () => Date.now() } = {}) {
  let parts = [];
  let audioEnd = 0;
  /** What Cru is saying (or just said), and when it stopped (null: still speaking). */
  let saying = '', saidAt = null, sayingFrom = 0;
  const echo = (text) => {
    if (!saying) return false;
    const t = now();
    if (saidAt === null ? t - sayingFrom > SAYING_MAX_MS : t - saidAt > ECHO_TAIL_MS) { saying = ''; return false; }
    return isEcho(text, saying);
  };
  const sofar = () => (parts.length ? { type: 'interim', text: parts.join(' ') } : null);
  return {
    /** @returns an event for the client, or null */
    push(msg) {
      if (!msg || typeof msg !== 'object') return null;
      if (msg.type === 'SpeechStarted') return { type: 'speech' };
      if (msg.type === 'UtteranceEnd') return flush();
      if (msg.type !== 'Results') return null;
      const end = (Number(msg.start) || 0) + (Number(msg.duration) || 0);
      if (end > audioEnd) audioEnd = end;
      const text = String(msg.channel?.alternatives?.[0]?.transcript || '').trim();
      // Cru's own voice heard back: never part of what the owner said.
      if (text && echo(text)) return msg.is_final ? sofar() : null;
      if (msg.is_final) {
        if (text) parts.push(text);
        return sofar();
      }
      if (!text) return null;
      return { type: 'interim', text: [...parts, text].join(' ') };
    },
    /** Cru starts saying [text] (a reply, a cue). */
    saying(text) { saying = String(text || '').slice(0, 4000); saidAt = null; sayingFrom = now(); },
    /** Cru stopped (finished, or was talked over). */
    said() { if (saying && saidAt === null) saidAt = now(); },
    /** Whatever was heard but not yet sent (the stream is ending). */
    flush,
    get seconds() { return audioEnd; },
  };
  function flush() {
    const text = parts.join(' ').trim();
    parts = [];
    return text ? { type: 'utterance', text } : null;
  }
}

/**
 * Relay one client socket to Deepgram until either side closes.
 * @param client  a ws WebSocket from the browser / phone
 * @param opts.projects, opts.services  names for keyterms
 * @param opts.audio  { encoding, sample_rate } for raw PCM (see rawAudio), or null
 * @param opts.WebSocketImpl  tests pass a fake
 */
export function relay(client, { projects = [], services = [], audio = null, pauseMs = PAUSE_MS.default, WebSocketImpl = WebSocket } = {}) {
  const send = (m) => { if (client.readyState === 1) client.send(JSON.stringify(m)); };
  const key = getSettings().deepgramKey;
  if (!key) {
    send({ type: 'error', error: 'No Deepgram key is set. Add one in Settings › Voice assistant.' });
    client.close();
    return;
  }
  if (deepgramOverLimit()) {
    // The phone then listens with its own recogniser (D46).
    send({ type: 'error', error: 'This month\'s Deepgram spend has reached its limit' });
    client.close();
    return;
  }
  const dg = new WebSocketImpl(streamUrl(keyterms({ projects, services }), { audio: rawAudio(audio || {}), pauseMs }), { headers: { Authorization: `Token ${key}` } });
  const asm = createAssembler();
  const queue = [];
  let open = false;
  let paused = false;
  let lastAudio = Date.now();
  let done = false;

  const keepAlive = setInterval(() => {
    if (open && (paused || Date.now() - lastAudio > KEEPALIVE_MS - 500)) {
      try { dg.send(JSON.stringify({ type: 'KeepAlive' })); } catch { /* closing */ }
    }
  }, KEEPALIVE_MS);

  function finish(err) {
    if (done) return;
    done = true;
    clearInterval(keepAlive);
    const rest = asm.flush();
    if (rest) send(rest);
    if (err) send({ type: 'error', error: err });
    if (asm.seconds > 0) addDeepgramSpend(asm.seconds);
    try { if (dg.readyState === 1) dg.send(JSON.stringify({ type: 'CloseStream' })); } catch { /* gone */ }
    setTimeout(() => { try { dg.close(); } catch { /* gone */ } }, 1500);
    try { client.close(); } catch { /* gone */ }
  }

  dg.on('open', () => {
    open = true;
    for (const chunk of queue.splice(0)) dg.send(chunk);
    send({ type: 'ready', echoFilter: true });
  });
  dg.on('message', (data) => {
    let msg;
    try { msg = JSON.parse(String(data)); } catch { return; }
    const ev = asm.push(msg);
    if (ev) send(ev);
  });
  dg.on('unexpected-response', (_req, res) => {
    finish(res.statusCode === 401 || res.statusCode === 403
      ? 'Deepgram refused the key. Check it in Settings › Voice assistant.'
      : `Deepgram refused the stream (${res.statusCode})`);
  });
  dg.on('error', (e) => finish(`Couldn't reach Deepgram: ${e.message}`));
  dg.on('close', () => finish());

  client.on('message', (data, isBinary) => {
    if (!isBinary) {
      let m = null;
      try { m = JSON.parse(String(data)); } catch { /* not ours */ }
      if (m?.type === 'stop') finish();
      else if (m?.type === 'pause') paused = true;
      else if (m?.type === 'resume') paused = false;
      // Barge-in: audio keeps flowing while Cru talks; its own words are dropped.
      else if (m?.type === 'saying') { paused = false; asm.saying(m.text); }
      else if (m?.type === 'said') asm.said();
      return;
    }
    if (paused || done) return;
    lastAudio = Date.now();
    if (open) dg.send(data); else if (queue.length < 200) queue.push(data);
  });
  client.on('close', () => finish());
  client.on('error', () => finish());
}
