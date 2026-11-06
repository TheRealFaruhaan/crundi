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
  const ctx = { window: {}, document: {}, navigator: {}, localStorage: {} };
  vm.runInNewContext(src, ctx);
  const T = ctx.window.CrundiVoice._test;
  const said = 'Your build passed an hour ago, Sir.';
  ok(!T.talksOver('your build passed an hour', said), 'Cru\'s own words heard back are not an interruption');
  ok(!T.talksOver('um', said) && !T.talksOver('uh hmm', said), 'a hum is not an interruption');
  ok(!T.talksOver('really', said), 'one stray word is not an interruption');
  ok(T.talksOver('stop', said) && T.talksOver('wait', said), '"stop" or "wait" alone is');
  ok(T.talksOver('which build was that', said), 'two or more of his own words are');
  ok(T.talksOver('really', said, 1), 'with the voice level heard over Cru, one word is enough');
  ok(!T.talksOver('build sir', said, 1), 'but never Cru\'s own words');
  ok(T.isEcho('one moment sir', 'One moment, Sir.'), 'a cue heard back is an echo');
  const seq = [0, 0.99, 0.5, 0.1, 0.7];
  let last = -1, repeat = false;
  for (let k = 0; k < 200; k++) { const i = T.pickCue(8, last, () => seq[k % seq.length]); if (i === last || i < 0 || i > 7) repeat = true; last = i; }
  ok(!repeat, 'a cue is never the same as the one before, and always one of them');
  ok(T.pickCue(1, 0, Math.random) === 0 && T.pickCue(0, -1, Math.random) === -1, 'one cue: that one; none: none');
  ok(T.CUE_TEXTS.join('|') === cues.CUES.join('|'), 'the web client\'s fallback phrases match the server\'s');
}

try { rmSync(dir, { recursive: true, force: true }); } catch { /* fine */ }
console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
