/**
 * voice/stream.js — live speech to text: the client's microphone, relayed to
 * Deepgram's streaming API while Master speaks.
 *
 *   client ──audio chunks──▶ /api/voice/listen ──▶ Deepgram /v1/listen (WebSocket)
 *   client ◀──words as they're heard, then each finished utterance──┘
 *
 * The client sends the microphone as it records (WebM/Opus from a browser's
 * MediaRecorder; Deepgram reads the container itself) and gets back:
 *
 *   { type: 'ready' }                         Deepgram is listening
 *   { type: 'interim', text }                 words so far, may still change
 *   { type: 'utterance', text }               Master finished a sentence
 *   { type: 'speech' }                        Master started speaking
 *   { type: 'error', error }                  and the socket closes
 *
 * A client text message { type: 'stop' } ends the stream; { type: 'pause' } /
 * { type: 'resume' } stop and restart forwarding audio (while Crundi itself is
 * talking, so it doesn't transcribe its own voice) — Deepgram is kept alive in
 * between.
 *
 * The key stays here; the client never sees it. Seconds of audio Deepgram
 * reports are added to the month's voice spend when the stream ends.
 */

import WebSocket from 'ws';
import { getSettings, addDeepgramSpend } from './settings.js';
import { keyterms } from './stt.js';

const STREAM_URL = 'wss://api.deepgram.com/v1/listen';
const KEEPALIVE_MS = 8000;

/** The streaming URL: Nova-3, words as they're heard, end of speech detected. */
export function streamUrl(terms, { language = 'en' } = {}) {
  const q = new URLSearchParams({
    model: 'nova-3', language,
    smart_format: 'true', punctuate: 'true',
    interim_results: 'true',      // words while still speaking
    endpointing: '400',           // a 400 ms pause ends a phrase
    utterance_end_ms: '1200',     // 1.2 s with no words ends the utterance
    vad_events: 'true',           // "speech started"
  });
  for (const t of terms) q.append('keyterm', t);
  return `${STREAM_URL}?${q}`;
}

/**
 * Turns Deepgram's messages into utterances. Pure, so it can be tested.
 * Finished phrases (is_final) are joined; the utterance is complete at
 * speech_final or UtteranceEnd.
 */
export function createAssembler() {
  let parts = [];
  let audioEnd = 0;
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
      if (msg.is_final) {
        if (text) parts.push(text);
        if (msg.speech_final) return flush();
        return parts.length ? { type: 'interim', text: parts.join(' ') } : null;
      }
      if (!text) return null;
      return { type: 'interim', text: [...parts, text].join(' ') };
    },
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
 * @param opts.WebSocketImpl  tests pass a fake
 */
export function relay(client, { projects = [], services = [], WebSocketImpl = WebSocket } = {}) {
  const send = (m) => { if (client.readyState === 1) client.send(JSON.stringify(m)); };
  const key = getSettings().deepgramKey;
  if (!key) {
    send({ type: 'error', error: 'No Deepgram key is set. Add one in Settings › Voice assistant.' });
    client.close();
    return;
  }
  const dg = new WebSocketImpl(streamUrl(keyterms({ projects, services })), { headers: { Authorization: `Token ${key}` } });
  const asm = createAssembler();
  const queue = [];
  let open = false;
  let paused = false;
  let lastAudio = Date.now();
  let done = false;

  const keepAlive = setInterval(() => {
    if (open && Date.now() - lastAudio > KEEPALIVE_MS - 500) {
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
    send({ type: 'ready' });
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
      return;
    }
    if (paused || done) return;
    lastAudio = Date.now();
    if (open) dg.send(data); else if (queue.length < 200) queue.push(data);
  });
  client.on('close', () => finish());
  client.on('error', () => finish());
}
