/**
 * voice/stream.js — live speech to text: the client's microphone, relayed to
 * Deepgram's streaming API while the owner speaks.
 *
 *   client ──audio chunks──▶ /api/voice/listen ──▶ Deepgram /v1/listen (WebSocket)
 *   client ◀──words as they're heard, then each finished utterance──┘
 *
 * The client sends the microphone as it records (16-bit PCM, declared with
 * ?encoding=linear16&sample_rate=N, from the phone and the web client; an
 * older browser client's WebM/Opus, which Deepgram reads itself, still works
 * but its voice can't be checked) and gets back:
 *
 *   { type: 'ready', echoFilter: true }       Deepgram is listening; this server
 *                                             takes 'saying' / 'said' (below)
 *   { type: 'interim', text }                 words so far, may still change
 *   { type: 'utterance', text, owner }        the owner finished a sentence;
 *                                             owner: the voice check (speaker.js),
 *                                             true / false / null (can't tell)
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
import { fixNames, nameList } from './names.js';
import { CUES } from './cues.js';
import { createAudioRing, utteranceAudio, SHORT_MIN_SECONDS } from './speaker.js';

const STREAM_URL = 'wss://api.deepgram.com/v1/listen';
/** Deepgram closes a stream after about 10 s without audio; it asks for a KeepAlive every 3–5 s. */
const KEEPALIVE_MS = 4000;

/**
 * Raw audio formats a client may declare (?encoding=&sample_rate= on
 * /api/voice/listen). The phone and the web client send 16-bit PCM; an older
 * browser client sent WebM/Opus and declared nothing (Deepgram reads it).
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
export const ECHO_TAIL_MS = 4000;
/** How many of Cru's recent sayings (cues and replies) are kept to match echoes against. */
const RECENT_SAYINGS = 4;
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
  if (h.filter(w => set.has(w)).length / h.length >= 0.6) return true;
  // Misheard (Cru's "Right away, Sir." comes back as "Riseway, sir."): a short
  // phrase that sounds like Cru's words, letter by letter.
  return h.length <= 6 && soundsLike(h, s);
}

/** Letters alone, as one string. */
const letters = (ws) => ws.join('').replace(/[^\p{L}\p{N}]/gu, '');

function editDistance(a, b) {
  if (a === b) return 0;
  let prev = Array.from({ length: b.length + 1 }, (_, j) => j);
  for (let i = 1; i <= a.length; i++) {
    const cur = [i];
    for (let j = 1; j <= b.length; j++) cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
    prev = cur;
  }
  return prev[b.length];
}

/**
 * [h] (heard words) is close, letter by letter, to [s] or to a run of [s]'s
 * words about as long: at least 60% the same.
 */
export function soundsLike(h, s, min = 0.6) {
  const a = letters(h);
  if (a.length < 4 || !s.length) return false;
  const close = (b) => b.length && 1 - editDistance(a, b) / Math.max(a.length, b.length) >= min;
  if (s.length <= 6) return close(letters(s));
  for (let n = Math.max(1, h.length - 1); n <= h.length + 1; n++) {
    for (let i = 0; i + n <= s.length; i++) if (close(letters(s.slice(i, i + n)))) return true;
  }
  return false;
}

/**
 * The owner taking back what they said: "scratch that", "never mind",
 * "ignore that". Not plain "cancel": that answers Cru's yes/no questions.
 */
const DISCARD = /\b(?:scratch that|never\s*mind|ignore that)\b[\s.,!?;:]*/gi;

/**
 * An utterance with the owner's own correction applied: everything up to the
 * last "scratch that" (etc.) is dropped. { discard: true } when nothing is
 * left: the whole thing was taken back.
 */
export function applyDiscard(text) {
  const t = String(text || '');
  let last = -1, end = 0, m;
  DISCARD.lastIndex = 0;
  while ((m = DISCARD.exec(t))) { last = m.index; end = m.index + m[0].length; }
  if (last < 0) return { text: t.trim(), discard: false };
  const rest = t.slice(end).trim();
  return rest ? { text: rest, discard: false } : { text: '', discard: true };
}

/** The whole utterance is one of the cues ("One moment, Sir."), however it was heard. */
export function isCue(heard) {
  const h = words(heard);
  if (h.length < 2 || h.length > 6) return false;
  return CUES.some((c) => {
    const s = words(c), set = new Set(s), hs = new Set(h);
    // Most of the cue's words are there, and little else; or it sounds just like it.
    const shared = s.filter((w) => hs.has(w)).length;
    return (shared / s.length >= 0.75 && h.filter((w) => set.has(w)).length / h.length >= 0.75) || soundsLike(h, s, 0.72);
  });
}

/**
 * Turns Deepgram's messages into utterances. Pure, so it can be tested.
 * Finished phrases (is_final) are joined; the utterance is complete at
 * UtteranceEnd only: speech_final comes after a short pause, and the owner may
 * pause mid-sentence (D44).
 */
export function createAssembler({ now = () => Date.now(), names = [] } = {}) {
  let parts = [];
  // Where each kept word is in the stream ([start, end] seconds), for the voice check.
  let spans = [];
  // Crundi's names put back into what was heard (names.js).
  const fix = (text) => (names.length ? fixNames(text, { names }) : text);
  let audioEnd = 0;
  /**
   * What Cru said lately (a cue, then the reply, ...), each with when it
   * started and stopped (null: still speaking). Several, not one: the cue's
   * echo is still being transcribed when the reply starts.
   */
  let recent = [];
  const echo = (text) => {
    const t = now();
    recent = recent.filter((r) => (r.saidAt === null ? t - r.from <= SAYING_MAX_MS : t - r.saidAt <= ECHO_TAIL_MS));
    return recent.some((r) => isEcho(text, r.text));
  };
  const sofar = () => (parts.length ? { type: 'interim', text: fix(parts.join(' ')) } : null);
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
        if (text) {
          parts.push(text);
          const ws = msg.channel?.alternatives?.[0]?.words;
          if (Array.isArray(ws) && ws.length) for (const w of ws) { const a = Number(w.start), b = Number(w.end); if (b > a) spans.push([a, b]); }
          else if (end > (Number(msg.start) || 0)) spans.push([Number(msg.start) || 0, end]);
        }
        return sofar();
      }
      if (!text) return null;
      return { type: 'interim', text: fix([...parts, text].join(' ')) };
    },
    /** Cru starts saying [text] (a reply, a cue). */
    saying(text) {
      const t = now();
      for (const r of recent) if (r.saidAt === null) r.saidAt = t;
      recent.push({ text: String(text || '').slice(0, 4000), from: t, saidAt: null });
      if (recent.length > RECENT_SAYINGS) recent = recent.slice(-RECENT_SAYINGS);
    },
    /** Cru stopped (finished, or was talked over). */
    said() { const t = now(); for (const r of recent) if (r.saidAt === null) r.saidAt = t; },
    /** Whatever was heard but not yet sent (the stream is ending). */
    flush,
    /** The owner threw away what they had said so far (the x). */
    discard() { parts = []; spans = []; },
    get seconds() { return audioEnd; },
  };
  function flush() {
    const heard = parts.join(' ').trim();
    const said = spans;
    parts = []; spans = [];
    // "…scratch that": nothing, or only what came after it, is the request.
    const d = applyDiscard(heard);
    if (d.discard) return { type: 'discard', text: heard };
    const text = d.text ? fix(d.text) : '';
    // spans: where its words are in the stream (the relay checks the voice, then drops them).
    return text ? { type: 'utterance', text, spans: said } : null;
  }
}

/**
 * Relay one client socket to Deepgram until either side closes.
 * @param client  a ws WebSocket from the browser / phone
 * @param opts.projects, opts.services  names for keyterms
 * @param opts.audio  { encoding, sample_rate } for raw PCM (see rawAudio), or null
 * @param opts.WebSocketImpl  tests pass a fake
 * @param opts.speaker  speaker.js createSpeaker(): each utterance's voice is
 *                      checked (raw PCM streams only) and remembered for its turn
 */
export function relay(client, { projects = [], services = [], audio = null, pauseMs = PAUSE_MS.default, WebSocketImpl = WebSocket, speaker = null } = {}) {
  const send = (m) => { if (client.readyState === 1) client.send(JSON.stringify(m)); };
  const raw = rawAudio(audio || {});
  // The last 30 s as sent to Deepgram (its word times index it), for the voice check.
  const ring = speaker && raw ? createAudioRing({ rate: raw.sample_rate }) : null;
  // Events go out in order: while an utterance's voice is being checked
  // (~50-100 ms, off the event loop), what comes after it waits behind it.
  const outQ = [];
  let pumping = false;
  const idle = [];
  async function pump() {
    if (pumping) return;
    pumping = true;
    while (outQ.length) { try { const ev = await outQ.shift(); if (ev) send(ev); } catch { /* dropped */ } }
    pumping = false;
    for (const f of idle.splice(0)) f();
  }
  const out = (ev) => {
    if (!pumping && !outQ.length && !(ev instanceof Promise)) { send(ev); return; }
    outQ.push(ev); pump();
  };
  const whenIdle = () => (pumping || outQ.length ? new Promise((r) => idle.push(r)) : Promise.resolve());
  /** An utterance from the assembler: its voice checked, the verdict remembered for the turn. */
  function deliver(ev) {
    if (!ev || ev.type !== 'utterance') { out(ev); return; }
    const { spans, ...e } = ev;
    if (!speaker) { out(e); return; }
    const remember = (v) => { try { speaker.recent.remember(e.text, v); } catch { /* best effort */ } return { ...e, owner: v.owner ?? null }; };
    if (!speaker.enrolled) { out(remember({ owner: null, reason: 'no-profile' })); return; }
    const clip = ring ? utteranceAudio(ring, spans) : null;
    if (!clip) { out(remember({ owner: null, reason: 'unavailable' })); return; }
    if (clip.seconds < SHORT_MIN_SECONDS) { out(remember({ owner: null, reason: 'short', seconds: clip.seconds })); return; }
    out(speaker.verify(clip.samples, { seconds: clip.seconds }).then(remember, () => remember({ owner: null, reason: 'unavailable' })));
  }
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
  const dg = new WebSocketImpl(streamUrl(keyterms({ projects, services }), { audio: raw, pauseMs }), { headers: { Authorization: `Token ${key}` } });
  const asm = createAssembler({ names: nameList({ projects, services }) });
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
    if (rest) deliver(rest);
    if (err) out({ type: 'error', error: err });
    if (asm.seconds > 0) addDeepgramSpend(asm.seconds);
    try { if (dg.readyState === 1) dg.send(JSON.stringify({ type: 'CloseStream' })); } catch { /* gone */ }
    setTimeout(() => { try { dg.close(); } catch { /* gone */ } }, 1500);
    whenIdle().then(() => { try { client.close(); } catch { /* gone */ } });
  }

  dg.on('open', () => {
    open = true;
    for (const chunk of queue.splice(0)) dg.send(chunk);
    out({ type: 'ready', echoFilter: true });
  });
  dg.on('message', (data) => {
    let msg;
    try { msg = JSON.parse(String(data)); } catch { return; }
    const ev = asm.push(msg);
    if (ev) deliver(ev);
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
      else if (m?.type === 'discard') asm.discard();
      else if (m?.type === 'said') asm.said();
      return;
    }
    if (paused || done) return;
    lastAudio = Date.now();
    if (open) dg.send(data); else if (queue.length < 200) queue.push(data); else return;
    // Deepgram's timeline is the audio it was sent: the ring keeps the same.
    if (ring) ring.push(data);
  });
  client.on('close', () => finish());
  client.on('error', () => finish());
}
