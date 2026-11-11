#!/usr/bin/env node
/**
 * test-voice-barge.mjs — talking over Cru (barge-in) and the "one moment,
 * Sir" cues.
 *
 *   stream   Cru's own words heard back are dropped while it speaks
 *            ('saying' / 'said'), the owner's are kept; 'ready' says so
 *   cues     the phrases, made once per voice and speed in the background,
 *            counted as Deepgram TTS, served only by their own names
 *   web      crundi-voice.js's pure helpers (talksOver, isEcho, pickCue)
 *   vad      the speech detector (Silero VAD, run in JS): the same as the
 *            model itself, speech cuts in, noise and music never do
 *
 * Run: node scripts/test-voice-barge.mjs
 */

import { mkdtempSync, rmSync, mkdirSync, readFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import { EventEmitter } from 'node:events';

const dir = mkdtempSync(join(tmpdir(), 'voice-barge-'));
process.env.DATA_DIR = join(dir, 'data');
mkdirSync(process.env.DATA_DIR, { recursive: true });

const vs = await import('../src/voice/settings.js');
const vstream = await import('../src/voice/stream.js');
const cues = await import('../src/voice/cues.js');
const { CAPABILITIES } = await import('../src/client-version.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const R = (text, f = {}) => ({ type: 'Results', start: 0, duration: 1, is_final: !!f.final, channel: { alternatives: [{ transcript: text }] } });

// ─── the stream drops Cru's own words ───
{
  let t = 1000;
  const a = vstream.createAssembler({ now: () => t });
  const said = 'You have three tasks in progress on the Crundi board, Sir.';
  a.saying(said);
  ok(a.push(R('three tasks in progress')) === null, 'an echo of what Cru is saying is not shown as the owner\'s words');
  ok(a.push(R('you have three tasks in progress', { final: true })) === null, 'and a finished echo phrase is dropped');
  const own = a.push(R('stop that please', { final: true }));
  ok(own && own.type === 'interim' && own.text === 'stop that please', 'the owner talking over Cru comes straight through');
  a.said();
  t += 1000;
  a.push(R('on the Crundi board Sir', { final: true }));
  ok(a.push({ type: 'UtteranceEnd' }).text === 'stop that please', 'the late tail of Cru\'s words is dropped too; the owner\'s words alone are the utterance');
  t += vstream.ECHO_TAIL_MS + 10;
  a.push(R('three tasks in progress', { final: true }));
  ok(a.push({ type: 'UtteranceEnd' }).text === 'three tasks in progress', 'well after Cru stopped, the same words are the owner\'s');
  const b = vstream.createAssembler();
  b.push(R('three tasks in progress', { final: true }));
  ok(b.push({ type: 'UtteranceEnd' }).text === 'three tasks in progress', 'a client that never says what Cru says gets everything, as before');
  ok(vstream.isEcho('one moment sir', 'One moment, Sir.') && !vstream.isEcho('open the board', 'One moment, Sir.'), 'isEcho: a cue heard back, not the owner');

  {
    // The loop seen live: the cue's echo was still being transcribed when the
    // reply started, so it no longer matched; and "Right away, Sir." came back
    // as "Riseway, sir.". Each became a new turn, with a new cue.
    let t2 = 5000;
    const c = vstream.createAssembler({ now: () => t2 });
    c.saying('Looking now, Sir.');
    t2 += 300;
    c.saying('Right away, Sir.');
    t2 += 900;
    c.saying('Sir, the API chat has finished: the tests pass.');
    t2 += 400;
    c.push(R('Riseway, sir.', { final: true }));
    ok(c.push({ type: 'UtteranceEnd' }) === null, 'a cue heard back after the reply has started is still Cru\'s, even misheard');
    c.push(R('Looking now, sir.', { final: true }));
    ok(c.push({ type: 'UtteranceEnd' }) === null, 'an earlier saying is remembered alongside the latest');
    // The owner often says these words himself: only Cru's own, just said, are dropped.
    const d = vstream.createAssembler();
    d.push(R('Just a moment.', { final: true }));
    ok(d.push({ type: 'UtteranceEnd' }).text === 'Just a moment.', 'the owner saying a cue\'s words is his, when Cru has not just said them');
    t2 += vstream.ECHO_TAIL_MS + 10;
    c.said();
    c.push(R('Right away', { final: true }));
    ok(c.push({ type: 'UtteranceEnd' }).text === 'Right away', 'and once Cru\'s last words are seconds old, the same words are the owner\'s');
  }

  {
    // Taking it back: "scratch that" / "never mind" / "ignore that" (never plain "cancel").
    const e = vstream.createAssembler();
    e.push(R('open the dhivo chat, scratch that, open crundi', { final: true }));
    ok(e.push({ type: 'UtteranceEnd' }).text === 'open crundi', 'mid-sentence "scratch that": only what came after is the request');
    e.push(R('send it to the API chat. Never mind.', { final: true }));
    const dd = e.push({ type: 'UtteranceEnd' });
    ok(dd.type === 'discard', 'ending with "never mind": nothing is sent, the client is told to discard');
    e.push(R('cancel', { final: true }));
    ok(e.push({ type: 'UtteranceEnd' }).text === 'cancel', 'plain "cancel" is the owner answering a question, sent as ever');
    e.push(R('open the board', { final: true }));
    e.discard();
    e.push(R('open notes', { final: true }));
    ok(e.push({ type: 'UtteranceEnd' }).text === 'open notes', 'the x drops what was heard so far');
    ok(vstream.applyDiscard('what is never mindful').text === 'what is never mindful', 'only the whole phrase counts');
  }

  class FakeWs extends EventEmitter {
    constructor(url, opts) { super(); this.sent = []; this.readyState = 0; FakeWs.last = this; }
    send(d) { this.sent.push(d); }
    close() { this.readyState = 3; this.emit('close'); }
  }
  vs.updateSettings({ deepgramKey: 'dg-test' });
  const client = new EventEmitter();
  client.readyState = 1; client.out = [];
  client.send = (m) => client.out.push(JSON.parse(m));
  client.close = () => { client.readyState = 3; };
  vstream.relay(client, { WebSocketImpl: FakeWs });
  const dg = FakeWs.last;
  dg.readyState = 1; dg.emit('open');
  ok(client.out.some(m => m.type === 'ready' && m.echoFilter === true), 'ready tells the client the server drops Cru\'s own words');
  client.emit('message', JSON.stringify({ type: 'saying', text: 'Your build passed an hour ago, Sir.' }), false);
  client.emit('message', Buffer.from('a1'), true);
  ok(dg.sent.length === 1, 'while Cru speaks (saying) the audio keeps flowing');
  dg.emit('message', JSON.stringify(R('your build passed an hour ago', { final: true })));
  dg.emit('message', JSON.stringify(R('wait which build', { final: true })));
  client.emit('message', JSON.stringify({ type: 'said' }), false);
  dg.emit('message', JSON.stringify({ type: 'UtteranceEnd' }));
  ok(client.out.filter(m => m.type === 'utterance').map(m => m.text).join('|') === 'wait which build', 'through the relay: only the owner\'s words reach the client');
  client.emit('message', JSON.stringify({ type: 'pause' }), false);
  client.emit('message', Buffer.from('a2'), true);
  ok(dg.sent.length === 1, 'an older client\'s pause still pauses');
  client.emit('message', JSON.stringify({ type: 'stop' }), false);
  ok(CAPABILITIES.includes('voice-barge-in') && CAPABILITIES.includes('voice-cues'), 'capabilities: voice-barge-in, voice-cues');
}

// ─── cues ───
{
  const data = join(dir, 'cuedata');
  ok(cues.CUES.length >= 6 && cues.CUES.length <= 8 && cues.CUES.every(t => t.length <= 20), 'six to eight short phrases');
  vs.updateSettings({ ttsVoice: '', deepgramKey: 'dg-test' });
  const dev = cues.listCues({ dataDir: data, speakImpl: async () => { throw new Error('must not be called'); } });
  ok(dev.ok && dev.device && dev.cues.length === cues.CUES.length && dev.cues.every(c => c.url === null && c.text), 'no Deepgram voice: the device says the phrases, nothing is made');

  vs.updateSettings({ ttsVoice: 'aura-2-draco-en', speechRate: 1.2 });
  const calls = [];
  const spendBefore = vs.publicSettings().spend.deepgramUsd;
  const speakImpl = async (text, voice, o) => { calls.push({ text, voice, rate: o.rate }); vs.addDeepgramTtsSpend(text.length); return { ok: true, audio: Buffer.from('ID3' + text) }; };
  const first = cues.listCues({ dataDir: data, speakImpl });
  ok(!first.device && first.voice === 'aura-2-draco-en' && first.rate === '1.2' && first.cues.every(c => c.url === null), 'the first time: none ready yet, and the list comes back at once');
  await cues.makeCues('aura-2-draco-en', '1.2', { dataDir: data, speakImpl });
  ok(calls.length === cues.CUES.length && calls.every(c => c.voice === 'aura-2-draco-en' && c.rate === 1.2), 'each phrase is made once, in the voice and speed in use');
  ok(vs.publicSettings().spend.deepgramUsd > spendBefore, 'making them is counted as Deepgram TTS');
  const second = cues.listCues({ dataDir: data, speakImpl });
  ok(second.cues.every(c => c.url && c.url.startsWith('/api/voice/cues/clip?')), 'then every cue has its clip');
  await cues.makeCues('aura-2-draco-en', '1.2', { dataDir: data, speakImpl });
  ok(calls.length === cues.CUES.length, 'nothing is made twice');
  const u = new URL(second.cues[0].url, 'http://x');
  const clip = cues.readClip(u.searchParams.get('voice'), u.searchParams.get('rate'), u.searchParams.get('id'), data);
  ok(clip && clip.toString() === 'ID3' + cues.CUES[0], 'a clip is read back by its name');
  ok(existsSync(join(data, 'voice-cues', 'aura-2-draco-en', '1.2')), 'kept under <dataDir>/voice-cues/<voice>/<rate>/');
  ok(cues.readClip('../../etc', '1', u.searchParams.get('id'), data) === null && cues.readClip('aura-2-draco-en', '1/../..', u.searchParams.get('id'), data) === null && cues.readClip('aura-2-draco-en', '1.2', 'deadbeef0000', data) === null, 'only our own clip names are served');
  ok(cues.rateKey(1) === '1' && cues.rateKey(9) === '1.5' && cues.rateKey(0.75) === '0.75', 'speeds as folder names, within Deepgram\'s range');
  vs.updateSettings({ speechRate: 1 });
  ok(cues.listCues({ dataDir: data, speakImpl: async () => ({ ok: false }) }).cues.every(c => !c.url), 'a new speed needs its own clips');
  vs.updateSettings({ ttsVoice: '', deepgramKey: '' });
}

// ─── crundi-voice.js helpers ───
{
  const root = join(dirname(fileURLToPath(import.meta.url)), '..');
  const src = readFileSync(join(root, 'app', 'vendor', 'crundi-voice.js'), 'utf8');
  const ctx = { window: {}, document: {}, navigator: {}, localStorage: {}, setTimeout, clearTimeout };
  vm.runInNewContext(src, ctx);
  const T = ctx.window.CrundiVoice._test;
  const said = 'Your build passed an hour ago, Sir.';
  ok(!T.talksOver('your build passed an hour', said), 'Cru\'s own words heard back are not an interruption');
  ok(!T.talksOver('um', said) && !T.talksOver('uh hmm', said), 'a hum is not an interruption');
  ok(!T.talksOver('really', said), 'one stray word is not an interruption');
  ok(T.talksOver('stop', said) && T.talksOver('wait', said), '"stop" or "wait" alone is');
  ok(T.talksOver('which build was that', said), 'two or more of his own words are');
  // The end of Cru's line, from what is known (12 Nov: the speaking animation went on; sleep came at odd times).
  {
    const tok = {};
    const base = { phase: 'speak', speakToken: tok, speakAt: 0, speakDeadline: 20000 };
    const stream = (o) => ({ ...base, speakVia: 'stream', ...o });
    ok(!T.speechOver(stream({ playEndsAt: 10000, streamDone: tok, streamLast: 9000 }), 9500, false), 'streamed: still playing, not over');
    ok(T.speechOver(stream({ playEndsAt: 10000, streamDone: tok, streamLast: 9000 }), 10500, false), 'streamed: done and played, over at once (no waiting for the deadline)');
    ok(T.speechOver(stream({ playEndsAt: 10000, streamDone: null, streamLast: 6000 }), 10500, false), 'the stream\'s end never came: over once it has played and gone quiet');
    ok(!T.speechOver(stream({ playEndsAt: 10000, streamDone: null, streamLast: 9800 }), 10500, false), 'but not while chunks are still arriving');
    ok(!T.speechOver(stream({ playEndsAt: 0, speakAt: 0 }), 5000, false) && T.speechOver(stream({ playEndsAt: 0, speakAt: 0 }), 16000, false), 'nothing ever arrived: over after 15 s');
    ok(!T.speechOver({ ...base, speakVia: 'device', deviceAt: 0 }, 1000, false), 'device voice: a moment to start');
    ok(T.speechOver({ ...base, speakVia: 'device', deviceAt: 0 }, 2000, false) && !T.speechOver({ ...base, speakVia: 'device', deviceAt: 0 }, 2000, true), 'device voice: over when it stops speaking, whatever its events did');
    ok(T.speechOver({ ...base, speakVia: 'device', deviceAt: 0 }, 28001, true) && T.speechOver(stream({ playEndsAt: 40000, streamDone: null, streamLast: 39000 }), 48001, false), 'never past its deadline or playback plus 8 s');
    ok(!T.speechOver({ ...base, phase: 'listen', speakVia: 'stream', playEndsAt: 1 }, 99999, false), 'only while speaking');
    // Sleep counts from the last activity, never before playback really ends.
    ok(T.idleSince({ quietSince: 5000, playEndsAt: 12000 }) === 12000, 'an end of speech reported early: idle counts from the real end of playback');
    ok(T.idleSince({ quietSince: 30000, playEndsAt: 12000 }) === 30000, 'the owner speaking later: from then');
    ok(T.idleSince({ quietSince: 7000 }) === 7000 && T.SILENCE_MS === 30000, 'no playback known: from the last activity; asleep after 30 s of it');
  }
  // Woken from sleep in the same window: Cru's output chain must be rebuilt in the new audio context (12 Nov: text, no voice).
  {
    const ctxA = { state: 'running' }, ctxB = { state: 'running' };
    const out = { context: ctxA };
    ok(!T.staleOutput({ out, ctx: ctxA }), 'the output chain of the context in use is kept');
    ok(T.staleOutput({ out, ctx: ctxB }), 'a chain left from the context before waking is rebuilt');
    ok(T.staleOutput({ out, ctx: null }) && T.staleOutput({ out, ctx: { state: 'closed' } }) && T.staleOutput({ out: { context: ctxA }, ctx: Object.assign(ctxA, { state: 'closed' }) }), 'and one whose context is gone or closed');
    ok(!T.staleOutput({ out: null, ctx: ctxB }), 'no chain yet: nothing to rebuild');
  }
  // Cru's voice: typed last, unprompted news is shown, not said; muted, nothing is said; kid mode always talks.
  {
    const sp = T.speaks;
    ok(sp({}, 'unprompted') && sp({ lastTyped: false }, 'unprompted'), 'spoken last: news is said');
    ok(!sp({ lastTyped: true }, 'unprompted'), 'typed last: news (a wake-up, a report) is text only');
    ok(sp({ lastTyped: true }, 'reply', false) && !sp({ lastTyped: false }, 'reply', true), 'a reply follows how its own question came in');
    ok(sp({ lastTyped: true }, 'tap'), 'the speaker on a bubble still says it');
    ok(!sp({ voiceOff: true }, 'reply', false) && !sp({ voiceOff: true }, 'unprompted') && !sp({ voiceOff: true }, 'tap'), 'muted: nothing is said');
    ok(sp({ kid: true, voiceOff: true, lastTyped: true }, 'unprompted') && sp({ kid: true }, 'reply', true), 'kid mode always talks');
  }
  ok(T.talksOver('really', said, 1), 'with the voice level heard over Cru, one word is enough');
  ok(!T.talksOver('build sir', said, 1), 'but never Cru\'s own words');
  ok(T.isEcho('one moment sir', 'One moment, Sir.'), 'a cue heard back is an echo');
  const seq = [0, 0.99, 0.5, 0.1, 0.7];
  let last = -1, repeat = false;
  for (let k = 0; k < 200; k++) { const i = T.pickCue(8, last, () => seq[k % seq.length]); if (i === last || i < 0 || i > 7) repeat = true; last = i; }
  ok(!repeat, 'a cue is never the same as the one before, and always one of them');
  ok(T.pickCue(1, 0, Math.random) === 0 && T.pickCue(0, -1, Math.random) === -1, 'one cue: that one; none: none');
  ok(T.CUE_TEXTS.join('|') === cues.CUES.join('|'), 'the web client\'s fallback phrases match the server\'s');

  // ─── taking back what was said ───
  ok(T.isDiscard('Scratch that.') && T.isDiscard('open dhivo, never mind') && T.isDiscard('Nevermind!') && T.isDiscard('ignore that'), 'web: "scratch that", "never mind", "ignore that" take back what was said');
  ok(!T.isDiscard('cancel') && !T.isDiscard('scratch that itch') && !T.isDiscard('let me check'), 'web: plain "cancel" (it answers Cru\'s questions) and other words do not');
  ok(T.cancelledNotice([]) === 'Cancelled.' && T.cancelledNotice([{ tool: 'send_to_chat', what: 'sent "hi" to the API chat' }]) === 'Cancelled. Already done: sent "hi" to the API chat.', 'web: a cancelled turn says what it had already done');
  ok(T.canCancel({ cancellable: true, heard: 'open dhivo', phase: 'listen' }) && T.canCancel({ cancellable: true, heard: 'open dhivo', phase: 'think' }), 'web: the × shows while listening and thinking');
  ok(!T.canCancel({ cancellable: true, heard: '', phase: 'listen' }) && !T.canCancel({ cancellable: true, heard: 'x', phase: 'speak' }) && !T.canCancel({ cancellable: false, heard: 'x', phase: 'think' }), 'web: not without words, not once the answer is being said, not on an older server');

  // ─── the local gate: Cru's own voice decided on the device ───
  const voicedClip = () => { const c = new Float32Array(6400); for (let i = 0; i < c.length; i++) { const t = i / 16000; c[i] = 0.5 * Math.sin(2 * Math.PI * 140 * t) + 0.25 * Math.sin(2 * Math.PI * 280 * t) + 0.1 * Math.sin(2 * Math.PI * 420 * t); } return c; };
  let seed = 7; const rnd = () => { seed = (seed * 16807) % 2147483647; return seed / 2147483647; };
  const noiseClip = () => { const c = new Float32Array(6400); for (let i = 0; i < c.length; i++) c[i] = rnd() * 2 - 1; return c; };
  ok(T.voicedShare(voicedClip()) > 0.8 && T.voicedShare(noiseClip()) < 0.3, 'voicing: a pitched voice is voiced, a hiss or a clap is not');
  const speech = () => true, clap = () => false;
  const playFor = (g, frames, mic, play, voiced = speech) => { let hit = -1; for (let i = 0; i < frames; i++) { const p = typeof play === 'function' ? play(i) : play; if (g.hear(typeof mic === 'function' ? mic(i, p) : mic, p, 85, voiced) && hit < 0) hit = i; } return hit; };
  {
    // Speakers, no echo cancelling: Cru's voice comes back at a third of its level, rising and falling.
    const g = T.createGate(); for (let i = 0; i < 50; i++) g.learn(0.01);
    g.start();
    const play = (i) => (i % 5 === 0 ? 0.15 : 0.55 + 0.1 * Math.sin(i));
    ok(playFor(g, 60, (i, p) => p * 0.33, play) < 0, 'speakers: five seconds of Cru\'s own voice through the mic never count as the owner');
    const hit = playFor(g, 12, 0.75, 0.55);
    ok(hit >= 0 && hit * 85 <= 600, 'speakers: the owner talking over it is heard within about half a second (' + hit * 85 + ' ms)');
    g.start();
    playFor(g, 20, (i, p) => p * 0.33, play);
    ok(playFor(g, 12, 0.75, 0.55, clap) < 0, 'a clap or a cough as loud is not him (not speech)');
  }
  {
    // Headphones (or the loopback's echo cancelling working): hardly any echo.
    const g = T.createGate(); for (let i = 0; i < 50; i++) g.learn(0.01);
    g.start();
    ok(playFor(g, 40, (i, p) => p * 0.02, 0.6) < 0, 'headphones: silence through a long answer stays silent');
    const hit = playFor(g, 10, 0.2, 0.6);
    ok(hit >= 0 && hit * 85 <= 450, 'headphones: even a quiet voice cuts in at once (' + hit * 85 + ' ms)');
  }
  {
    // The browser's own voice: its level is unknown.
    const g = T.createGate(); for (let i = 0; i < 50; i++) g.learn(0.01);
    g.start();
    ok(playFor(g, 5, 0.4, null) < 0, 'unknown voice: at first only a loud voice counts');
    ok(playFor(g, 60, (i) => (i % 4 === 0 ? 0.42 : 0.3), null) < 0, 'unknown voice: its steady echo, loud syllables and all, is learnt and never counts');
    ok(playFor(g, 12, 0.85, null) >= 0, 'unknown voice: the owner well above it does');
  }
  {
    const p = T.createPreroll(1300);
    for (let i = 0; i < 30; i++) p.push(i, 85);
    const got = p.drain();
    ok(got.length >= 15 && got.length <= 16 && got[got.length - 1] === 29 && got[0] === 30 - got.length, 'pre-roll: the last 1.3 s, oldest first');
    ok(p.drain().length === 0, 'pre-roll: drained once');
    for (let i = 0; i < 30; i++) p.push(i, 100);
    const last = p.drain(900);
    ok(last.length === 9 && last[0] === 21 && last[8] === 29, 'pre-roll: only from just before he spoke (Cru before that is never sent)');
  }
  ok(/maxaveragebitrate=96000/.test(T.richOpus('a=rtpmap:111 opus/48000/2\r\na=fmtp:111 minptime=10;useinbandfec=1\r\n')) && T.richOpus('no opus') === 'no opus', 'the loopback asks for good Opus');
  {
    const ctx = { createMediaStreamDestination: () => ({ stream: { getAudioTracks: () => [{}] } }) };
    const rejects = async (W, ms) => { try { await T.makeLoopback(ctx, W, ms || 60); return false; } catch { return true; } };
    ok(await rejects({}), 'no WebRTC: played directly');
    ok(await rejects({ RTCPeerConnection: function () { throw new Error('blocked'); } }), 'WebRTC refused: played directly');
    class Never { constructor() { this.localDescription = null; } addTrack() {} createOffer() { return Promise.reject(new Error('no')); } setLocalDescription() {} setRemoteDescription() {} createAnswer() {} addIceCandidate() {} close() { this.closed = true; } }
    ok(await rejects({ RTCPeerConnection: Never }), 'an offer that fails: played directly');
    class Silent extends Never { createOffer() { return Promise.resolve({ type: 'offer', sdp: '' }); } setLocalDescription(d) { this.localDescription = d; return Promise.resolve(); } setRemoteDescription() { return Promise.resolve(); } createAnswer() { return Promise.resolve({ type: 'answer', sdp: '' }); } }
    ok(await rejects({ RTCPeerConnection: Silent }, 50), 'a loopback that never connects: played directly after the timeout');
    const made = [];
    class Works extends Silent {
      constructor() { super(); made.push(this); }
      setRemoteDescription() { const me = this; if (made.indexOf(me) === 1) setTimeout(() => { me.ontrack({ streams: [{}] }); me.iceConnectionState = 'connected'; me.oniceconnectionstatechange(); }, 5); return Promise.resolve(); }
    }
    const W = { RTCPeerConnection: Works, Audio: function () { this.play = () => Promise.resolve(); this.pause = () => {}; }, MediaStream: function () {} };
    let lb = null; try { lb = await T.makeLoopback(ctx, W, 500); } catch { /* fails the check */ }
    ok(lb && lb.dest && typeof lb.close === 'function', 'a loopback that connects and plays: Cru\'s voice goes through it');
    const W2 = { ...W, Audio: function () { this.play = () => Promise.reject(new Error('needs a click')); this.pause = () => {}; } };
    made.length = 0;
    ok(await rejects(W2, 500), 'audio not allowed to play yet: played directly');
  }
}

// ─── the speech detector: barge-in only on speech ───
{
  const root = join(dirname(fileURLToPath(import.meta.url)), '..');
  const src = readFileSync(join(root, 'app', 'vendor', 'crundi-voice.js'), 'utf8');
  const ctx = { window: {}, document: {}, navigator: {}, localStorage: {}, setTimeout, clearTimeout };
  vm.runInNewContext(src, ctx);
  const T = ctx.window.CrundiVoice._test;
  const bin = readFileSync(join(root, 'app', 'vendor', 'silero-vad-16k.bin'));
  const W = T.parseVad(bin.buffer.slice(bin.byteOffset, bin.byteOffset + bin.length));
  ok(W && W.enc.length === 4 && W.wih.length === 512 * 128, 'the weights file reads (SVAD v1, every layer)');
  ok(T.parseVad(new ArrayBuffer(16)) === null && T.parseVad(bin.buffer.slice(bin.byteOffset, bin.byteOffset + bin.length - 2)) === null, 'anything else is refused (the voicing check stands in)');
  ok(T.SPEECH_MS >= 200 && T.SPEECH_MS <= 320, 'barge-in needs about a quarter of a second of speech (' + T.SPEECH_MS + ' ms)');

  const raw = readFileSync(join(root, 'test', 'fixtures', 'vad-speech.s16'));
  const voice = new Int16Array(raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.length));
  const ref = JSON.parse(readFileSync(join(root, 'test', 'fixtures', 'vad-speech.probs.json'), 'utf8'));
  {
    // The model's own output (ONNX Runtime, silero_vad.onnx v5.1.2) for the same frames.
    const v = T.createVad(W), got = [];
    for (let i = 0; i + 512 <= voice.length; i += 512) { v.push(voice.subarray(i, i + 512)); got.push(v.prob); }
    let worst = 0; for (let i = 0; i < ref.length; i++) worst = Math.max(worst, Math.abs(ref[i] - got[i]));
    ok(got.length === ref.length && worst < 0.01, 'frame for frame the same as Silero VAD itself (worst ' + worst.toFixed(4) + ')');
  }
  // Fed as the microphone comes (85 ms at a time): how long the longest run of speech is.
  // The detector runs all the while the microphone is open: a second of a quiet room comes first.
  const f32 = (n, fn) => { const a = new Float32Array(n); for (let i = 0; i < n; i++) a[i] = fn(i, i / 16000); return a; };
  let seed = 11; const rnd = () => { seed = (seed * 16807) % 2147483647; return seed / 2147483647 * 2 - 1; };
  const room = f32(16000, () => rnd() * 0.004);
  // How long after it starts it is taken for someone talking (-1: never).
  const talks = (pcm) => { const v = T.createVad(W); v.push(room); for (let i = 0; i < pcm.length; i += 1365) { v.push(pcm.subarray(i, i + 1365)); if (v.talking) return Math.round((i + 1365) / 16); } return -1; };
  const voiceF = f32(voice.length, (i) => voice[i] / 32768);
  // The sentence's first word starts about 220 ms in.
  const at = talks(voice);
  ok(at > 0 && at <= 800, 'a spoken sentence cuts in (' + at + ' ms into the clip)');
  ok(T.VAD_WARM * 32 >= 800, 'nothing counts until the detector has run a moment (a sound out of silence reads as speech at first)');
  let pk = 0; const pink = f32(80000, () => { pk = pk * 0.97 + rnd() * 0.15; return pk; });
  // A fan 10 dB under his voice (as loud as a fan worth talking over gets).
  const rms = (a) => Math.sqrt(a.reduce((t, x) => t + x * x, 0) / a.length);
  const fanGain = rms(voiceF.filter((x) => Math.abs(x) > 0.01)) / rms(pink) / Math.pow(10, 10 / 20);
  const noisy = f32(voice.length, (i) => voiceF[i] + pink[i % pink.length] * fanGain);
  const atNoisy = talks(noisy);
  ok(atNoisy > 0 && atNoisy <= 1000, 'and still does over a fan\'s noise (' + atNoisy + ' ms)');
  const sounds = {
    'white noise': f32(80000, () => rnd() * 0.3),
    'a fan (pink noise)': pink,
    // Started at once, or faded in: either way the detector takes it for speech for a moment; it never rises and falls as speech does.
    'a loud hum starting': f32(80000, (i, t) => 0.4 * (Math.sin(2 * Math.PI * 120 * t) + 0.4 * Math.sin(2 * Math.PI * 240 * t))),
    'a hum fading in': f32(80000, (i, t) => Math.min(1, i / 4800) * 0.4 * (Math.sin(2 * Math.PI * 120 * t) + 0.4 * Math.sin(2 * Math.PI * 240 * t))),
    'music (a chord, changing)': f32(80000, (i, t) => { const n = [220, 277, 330, 440][Math.floor(t * 2) % 4]; return Math.min(1, i / 4800) * 0.2 * (Math.sin(2 * Math.PI * n * t) + Math.sin(2 * Math.PI * n * 1.5 * t) + 0.5 * Math.sin(2 * Math.PI * n * 2 * t)); }),
    'clicks and knocks': f32(80000, (i) => (i % 8000 < 300 ? rnd() * 0.8 * Math.exp(-(i % 8000) / 60) : 0)),
    'a door slam': f32(32000, (i) => (i > 8000 ? rnd() * 0.9 * Math.exp(-(i - 8000) / 1600) : 0)),
  };
  for (const [name, pcm] of Object.entries(sounds)) ok(talks(pcm) < 0, name + ' never cuts in');
  {
    // Tones that start while it listens are where it is closest: over ten rooms, never once.
    const tones = ['a hum fading in', 'music (a chord, changing)'];
    let hits = 0, missed = 0;
    for (let r = 1; r <= 10; r++) {
      let s2 = r * 7919; const rr = () => { s2 = (s2 * 16807) % 2147483647; return s2 / 2147483647 * 2 - 1; };
      const room2 = f32(16000, () => rr() * 0.004);
      const heard = (pcm) => { const v = T.createVad(W); v.push(room2); for (let i = 0; i < pcm.length; i += 1600) { v.push(pcm.subarray(i, i + 1600)); if (v.talking) return true; } return false; };
      for (const t of tones) if (heard(sounds[t])) hits++;
      if (!heard(voice)) missed++;
    }
    ok(hits === 0, 'a hum or music starting never cuts in, whatever the room (' + hits + ' of 20)');
    ok(missed === 0, 'and the sentence always does (' + missed + ' of 10 missed)');
  }
  {
    // The mic level drawn (10 Nov): it moves for speech only, never for the room's noise.
    const drawn = (pcm) => {
      const v = T.createVad(W), st = {}; let on = 0, n = 0, now = 0;
      v.push(room);
      for (let i = 0; i + 1365 <= pcm.length; i += 1365) { v.push(pcm.subarray(i, i + 1365)); now += 85; n++; if (T.shownLevel(v, false, 0.5, now, st) > 0) on++; }
      return on / Math.max(1, n);
    };
    for (const name of ['white noise', 'a fan (pink noise)', 'clicks and knocks', 'a door slam', 'a loud hum starting', 'a hum fading in', 'music (a chord, changing)']) {
      const share = drawn(sounds[name]);
      ok(share === 0, 'the drawn mic stays still for ' + name + ' (' + Math.round(share * 100) + '% of the time)');
    }
    ok(drawn(voice) > 0.3, 'and moves while someone speaks (' + Math.round(drawn(voice) * 100) + '% of the sentence)');
    ok(drawn(noisy) > 0.3, 'also over a fan');
    ok(T.shownLevel(null, true, 0.4, 0, {}) === 0.4 && T.shownLevel(null, false, 0.4, 0, {}) === 0, 'no detector to be had: the level as it is; still loading: still');
    // 10 Nov: the volume node was taken for the level meter; every frame threw while Cru talked and the mic froze.
    ok(!/S\.outAnalyser = an;/.test(src) && /S\.outAnalyser = S\.out && S\.out\.getByteTimeDomainData \? S\.out : null/.test(src) && /try \{ drawMic\(/.test(src), 'Cru\'s level is read from the meter itself, and a bad frame never stops the mic being drawn');
    const cold = T.createVad(W);
    ok(T.shownLevel(cold, false, 0.4, 0, {}) === 0, 'still until the detector has warmed up');
  }
  {
    const v = T.createVad(W);
    v.push(voice); v.reset();
    ok(v.prob === 0 && v.speechMs === 0 && !v.speech && !v.talking, 'reset starts afresh');
    const t0 = performance.now(); const n = Math.floor(voice.length / 512);
    v.push(voice);
    const per = (performance.now() - t0) / n;
    ok(per < 16, 'fast enough for the microphone (' + per.toFixed(2) + ' ms per 32 ms frame)');
  }
}

try { rmSync(dir, { recursive: true, force: true }); } catch { /* fine */ }
console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
