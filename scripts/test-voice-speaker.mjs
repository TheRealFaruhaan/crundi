#!/usr/bin/env node
/**
 * test-voice-speaker.mjs — Cru knows the owner's voice (src/voice/speaker.js):
 * the model's input features match kaldi-native-fbank, enrolment, verdicts,
 * the live stream's check, and what an unrecognised voice may do.
 *
 * Pure node: the model itself is faked (a real run needs onnxruntime-node
 * and the 26 MB download; set CRUNDI_SPEAKER_MODEL_DIR to a folder holding
 * it to run that too). Run: node scripts/test-voice-speaker.mjs
 */
import { mkdtempSync, readFileSync, statSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { EventEmitter } from 'node:events';

const here = dirname(fileURLToPath(import.meta.url));
const dir = mkdtempSync(join(tmpdir(), 'cru-speaker-'));
process.env.DATA_DIR = dir;
const S = await import('../src/voice/speaker.js');
const settings = await import('../src/voice/settings.js');
const { relay } = await import('../src/voice/stream.js');
const { makeActionTools, createPending, UNRECOGNISED } = await import('../src/voice/actions.js');
const { makeAdminTools } = await import('../src/voice/admin.js');
const { contextLine } = await import('../src/voice/agent.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const tick = () => new Promise((r) => setTimeout(r, 5));

// ─── features: the same as kaldi-native-fbank (fixture made with it) ───
{
  const fx = JSON.parse(readFileSync(join(here, 'fixtures', 'voice-fbank.json'), 'utf8'));
  const x = new Float32Array(fx.samples);
  for (let i = 0; i < x.length; i++) x[i] = 0.3 * Math.sin(2 * Math.PI * 440 * i / 16000) + 0.2 * Math.sin(2 * Math.PI * 1250 * i / 16000 + 0.5) + 0.05 * (((i * 7919) % 1000) / 1000 - 0.5);
  const f = S.fbank(x);
  ok(f.frames === fx.frames && S.numFrames(fx.samples) === fx.frames && f.dim === 80, `as many frames as kaldi (${f.frames})`);
  let worst = 0;
  for (const [i, row] of Object.entries(fx.rows)) for (let d = 0; d < 80; d++) worst = Math.max(worst, Math.abs(f.data[Number(i) * 80 + d] - row[d]));
  ok(worst < 2e-3, `log mel energies match kaldi-native-fbank (worst difference ${worst.toExponential(2)})`);
  S.subtractMean(f);
  let m = 0; for (let t = 0; t < f.frames; t++) m += f.data[t * 80 + 7];
  ok(Math.abs(m / f.frames) < 1e-4, 'each bin\'s mean over the utterance is taken away');
  ok(S.fbank(new Float32Array(0)).frames === 0, 'no audio, no frames');
}

// ─── audio in ───
function wav(samples, rate, channels = 1) {
  const n = samples.length, b = Buffer.alloc(44 + n * 2 * channels);
  b.write('RIFF', 0); b.writeUInt32LE(36 + n * 2 * channels, 4); b.write('WAVE', 8); b.write('fmt ', 12);
  b.writeUInt32LE(16, 16); b.writeUInt16LE(1, 20); b.writeUInt16LE(channels, 22); b.writeUInt32LE(rate, 24);
  b.writeUInt32LE(rate * 2 * channels, 28); b.writeUInt16LE(2 * channels, 32); b.writeUInt16LE(16, 34); b.write('data', 36); b.writeUInt32LE(n * 2 * channels, 40);
  for (let i = 0; i < n; i++) for (let c = 0; c < channels; c++) b.writeInt16LE(Math.round(Math.max(-1, Math.min(1, samples[i])) * 32767), 44 + (i * channels + c) * 2);
  return b;
}
const tone = (hz, secs, rate = 16000, amp = 0.3) => Float32Array.from({ length: Math.round(secs * rate) }, (_, i) => amp * Math.sin(2 * Math.PI * hz * i / rate));
const silence = (secs, rate = 16000) => new Float32Array(Math.round(secs * rate));
const join_ = (...xs) => { const n = xs.reduce((k, x) => k + x.length, 0), o = new Float32Array(n); let p = 0; for (const x of xs) { o.set(x, p); p += x.length; } return o; };
const pcm = (x) => { const b = Buffer.alloc(x.length * 2); for (let i = 0; i < x.length; i++) b.writeInt16LE(Math.round(Math.max(-1, Math.min(1, x[i])) * 32767), i * 2); return b; };
{
  const d = S.decodeAudio(wav(tone(300, 1, 48000), 48000, 2), 'audio/wav');
  ok(Math.abs(d.length - 16000) <= 1, 'a 48 kHz stereo WAV becomes 16 kHz mono');
  const l = S.decodeAudio(pcm(tone(300, 0.5)), 'audio/L16;rate=16000');
  ok(l.length === 8000 && Math.abs(l[10] - tone(300, 0.5)[10]) < 1e-3, 'raw audio/L16 at 16 kHz is read as it is');
  let threw = false; try { S.decodeAudio(Buffer.from('OggS....'), 'audio/ogg'); } catch { threw = true; }
  ok(threw, 'another format is refused with a reason');
  const sp = S.findSpeech(join_(silence(1), tone(300, 2), silence(1)));
  ok(Math.abs(sp.seconds - 2) < 0.1 && Math.abs(sp.start / 16000 - 1) < 0.05, `the speech is found (${sp.seconds} s, from ${(sp.start / 16000).toFixed(2)} s)`);
  ok(S.trimSilence(silence(2)).seconds === 0, 'silence alone has no speech');
}

// ─── verdicts and the rules ───
{
  ok(S.decide(0.41, 2) === true && S.decide(0.39, 2) === false, 'cosine 0.40 is the line from 1.5 s');
  ok(S.decide(0.37, 1.0) === true && S.decide(0.35, 1.0) === false, 'short words (0.8-1.5 s) use 0.36');
  ok(S.decide(0.9, 0.6) === null, 'under 0.8 s there is no verdict');
  ok(S.turnPolicy({ enrolled: true, verdict: { owner: true, reason: 'owner', brief: true } }).admin.owner === false, 'a brief check is not enough for admin mode');
  ok(S.turnPolicy({ enrolled: true, verdict: { owner: true, reason: 'owner', brief: true } }).unrecognised === false, 'but everyday requests are fine');
  ok(S.turnPolicy({ enrolled: true, verdict: { owner: true, reason: 'owner', brief: true, word: 0.6 }, recentOwner: true }).admin.owner === true, 'admin: a taught short reply just after a recognised request answers');
  ok(S.turnPolicy({ enrolled: true, verdict: { owner: true, reason: 'owner', brief: true, word: 0.6 }, recentOwner: false }).admin.owner === false, 'admin: a taught short reply on its own does not');
  ok(S.turnPolicy({ enrolled: true, verdict: { owner: true, reason: 'owner', brief: true }, recentOwner: true }).admin.owner === false, 'admin: a brief check without a taught word does not');
  const P = S.turnPolicy;
  ok(!P({ enrolled: false, verdict: { owner: false } }).unrecognised && P({ enrolled: false }).admin.owner === true, 'no voice enrolled: everything as before, admin allowed');
  ok(P({ enrolled: true, verdict: { owner: false, reason: 'other' } }).unrecognised, 'another voice: changes are held back');
  ok(P({ enrolled: true, verdict: { owner: null, reason: 'short' } }).unrecognised, 'too short to tell: changes are held back too');
  ok(!P({ enrolled: true, verdict: null }).unrecognised, 'could not be checked at all (the phone\'s own recogniser): not held back');
  ok(!P({ enrolled: true, ownerOnly: false, verdict: { owner: false } }).unrecognised, '"Only act on my voice" off: another voice may act');
  ok(!P({ enrolled: true, typed: true }).unrecognised && P({ enrolled: true, typed: true }).admin.owner === true, 'typed by the owner: acts, admin allowed');
  ok(P({ enrolled: true, verdict: { owner: true } }).admin.owner === true, 'the owner\'s voice: admin allowed');
  ok(P({ enrolled: true, ownerOnly: false, verdict: { owner: false } }).admin.owner === false, 'admin needs the owner\'s voice even with "Only act on my voice" off');
  const short = P({ enrolled: true, verdict: { owner: null, reason: 'short' } }).admin;
  ok(short.owner === false && /longer/.test(short.reason), 'admin, too short: refused, asked to say it longer');
  ok(P({ enrolled: true, verdict: null }).admin.owner === false, 'admin, voice not checked: refused');

  let t = 1000;
  const rv = S.createRecentVerdicts({ now: () => t });
  rv.remember('Restart the build, please.', { owner: false, reason: 'other' });
  rv.remember('what time is it', { owner: true, reason: 'owner' });
  ok(rv.lookup('restart the build please')?.owner === false, 'a turn finds its utterance\'s verdict (case and punctuation aside)');
  ok(rv.lookup('What time is it? Restart the build, please.')?.owner === false, 'several utterances in one turn: another voice in any part counts');
  ok(rv.lookup('something typed') === null, 'typed text has no verdict');
  t += 121_000;
  ok(rv.lookup('what time is it') === null, 'verdicts are forgotten after two minutes');
}

// ─── the owner's voice, with a fake model ───
// The fake "voice": a low tone is the owner, a high one somebody else.
function fakeEmbedder() {
  const owner = Float32Array.from({ length: 8 }, (_, i) => (i === 0 ? 1 : 0.1));
  const other = Float32Array.from({ length: 8 }, (_, i) => (i === 5 ? 1 : 0.1));
  return {
    calls: 0,
    async available() { return true; },
    get state() { return 'ready'; },
    async embed(x) {
      this.calls++;
      let z = 0; for (let i = 1; i < x.length; i++) if ((x[i - 1] < 0) !== (x[i] < 0)) z++;
      const hz = (z / 2) / (x.length / 16000);
      return hz < 800 ? owner : other;
    },
  };
}
const emb = fakeEmbedder();
const sdir = join(dir, 'voice-memory');
const spk = S.createSpeaker({ dir: sdir, embedder: emb });
{
  let st = await spk.status();
  ok(st.available && !st.enrolled && st.prompts.length >= 8, 'not enrolled at first; the phrases to read are offered');
  ok((await spk.verify(tone(300, 3))).owner === null, 'no profile: no verdict');
  let r = await spk.enrolPhrase(join_(silence(0.5), tone(300, 3), silence(0.5)), { restart: true });
  ok(r.ok && r.accepted && Math.abs(r.seconds - 3) < 0.2 && r.consistency === null, `a phrase is kept, with its seconds of speech (${r.seconds} s)`);
  r = await spk.enrolPhrase(tone(300, 0.8));
  ok(r.ok && !r.accepted && /short/.test(r.problem), 'too short a phrase is asked again');
  r = await spk.enrolPhrase(tone(320, 3));
  ok(r.accepted && r.consistency > 0.9, 'a second phrase in the same voice agrees with the first');
  r = await spk.enrolPhrase(tone(2000, 3));
  ok(!r.accepted && /did not sound like/.test(r.problem), 'another voice mid-enrolment is not kept');
  ok(spk.finish().ok === false, `fewer than ${S.MIN_PHRASES} phrases: not finished`);
  for (let i = 0; i < S.MIN_PHRASES - 2; i++) await spk.enrolPhrase(tone(280 + i * 10, 3));
  const f = spk.finish();
  ok(f.ok && f.phrases === S.MIN_PHRASES, 'enough phrases: the profile is made');
  ok(existsSync(join(sdir, 'speaker.json')), 'the profile is kept in the voice data folder');
  if (process.platform !== 'win32') ok((statSync(join(sdir, 'speaker.json')).mode & 0o077) === 0, 'privately (0600)');
  ok(!readFileSync(join(sdir, 'speaker.json'), 'utf8').includes('samples'), 'only the voice\'s mean, never a recording');
  st = await spk.status();
  ok(st.enrolled && st.phrases === S.MIN_PHRASES && st.enrolledAt, 'status: enrolled, phrases, when');
  const a = await spk.verify(tone(310, 2));
  ok(a.owner === true && a.reason === 'owner' && a.score > 0.9, 'the owner\'s voice: true');
  const b = await spk.verify(tone(1800, 2));
  ok(b.owner === false && b.reason === 'other', 'another voice: false');
  const c = await spk.verify(tone(310, 1), { seconds: 0.6 });
  ok(c.owner === null && c.reason === 'short', 'too short: null');
  const c2 = await spk.verify(tone(310, 1), { seconds: 1 });
  ok(c2.owner === true && c2.brief === true, 'a short word: checked, marked brief');
  // A fresh speaker reads the profile back from disk.
  ok(S.createSpeaker({ dir: sdir, embedder: emb }).enrolled, 'the profile survives a restart');
}

// ─── the live stream: each utterance's voice checked ───
{
  class FakeWs extends EventEmitter {
    constructor(url) { super(); this.url = url; this.sent = []; this.readyState = 0; FakeWs.last = this; }
    send(d) { this.sent.push(d); }
    close() { this.readyState = 3; this.emit('close'); }
  }
  settings.updateSettings({ deepgramKey: 'dg-test' });
  const client = new EventEmitter();
  client.readyState = 1; client.out = [];
  client.send = (m) => client.out.push(JSON.parse(m));
  client.close = () => { client.readyState = 3; };
  relay(client, { WebSocketImpl: FakeWs, audio: { encoding: 'linear16', sample_rate: '48000' }, speaker: spk });
  const dg = FakeWs.last;
  ok(new URL(dg.url).searchParams.get('sample_rate') === '48000', 'Deepgram is told the phone\'s rate');
  dg.readyState = 1; dg.emit('open');
  // 0-2 s: the owner (low tone), 2-4 s: someone else, at 48 kHz in 100 ms chunks.
  const audio = join_(tone(300, 2, 48000), tone(1800, 2, 48000), tone(300, 0.5, 48000));
  for (let i = 0; i < audio.length; i += 4800) client.emit('message', pcm(audio.subarray(i, i + 4800)), true);
  const words = (from, to, n) => Array.from({ length: n }, (_, i) => ({ word: 'w', start: from + (i * (to - from)) / n, end: from + ((i + 1) * (to - from)) / n }));
  const R = (text, start, dur, ws) => ({ type: 'Results', is_final: true, start, duration: dur, channel: { alternatives: [{ transcript: text, words: ws }] } });
  dg.emit('message', JSON.stringify(R('open the crundi project', 0.05, 1.9, words(0.05, 1.95, 4))));
  dg.emit('message', JSON.stringify({ type: 'UtteranceEnd' }));
  dg.emit('message', JSON.stringify({ type: 'SpeechStarted' }));
  await tick(); await tick();
  const u1 = client.out.filter((m) => m.type === 'utterance');
  ok(u1.length === 1 && u1[0].owner === true && !('spans' in u1[0]), 'the owner\'s utterance: owner true (the word times stay on the server)');
  const i1 = client.out.findIndex((m) => m.type === 'utterance'), i2 = client.out.findIndex((m) => m.type === 'speech');
  ok(i1 >= 0 && i2 > i1, 'what came after waits for the check: the order is kept');
  dg.emit('message', JSON.stringify(R('delete everything', 2.05, 1.9, words(2.05, 3.95, 2))));
  dg.emit('message', JSON.stringify({ type: 'UtteranceEnd' }));
  await tick(); await tick();
  const u2 = client.out.filter((m) => m.type === 'utterance')[1];
  ok(u2 && u2.owner === false, 'another voice\'s utterance: owner false');
  dg.emit('message', JSON.stringify(R('yes', 4.05, 0.3, words(4.05, 4.35, 1))));
  dg.emit('message', JSON.stringify({ type: 'UtteranceEnd' }));
  await tick();
  ok(client.out.filter((m) => m.type === 'utterance')[2]?.owner === null, 'one short word: owner null');
  ok(spk.recent.lookup('Delete everything.')?.owner === false && spk.recent.lookup('open the Crundi project')?.owner === true, 'the verdicts are remembered for the turns');
  ok(spk.recent.lookup('yes')?.reason === 'short', 'and why there was none');
  client.emit('message', JSON.stringify({ type: 'stop' }), false);

  // A browser's WebM (no raw format declared): nothing to check, owner null at once.
  const c2 = new EventEmitter(); c2.readyState = 1; c2.out = []; c2.send = (m) => c2.out.push(JSON.parse(m)); c2.close = () => {};
  relay(c2, { WebSocketImpl: FakeWs, speaker: spk });
  FakeWs.last.readyState = 1; FakeWs.last.emit('open');
  FakeWs.last.emit('message', JSON.stringify(R('hello there friend', 0, 2, words(0, 2, 3))));
  FakeWs.last.emit('message', JSON.stringify({ type: 'UtteranceEnd' }));
  ok(c2.out.some((m) => m.type === 'utterance' && m.owner === null), 'no raw audio: owner null, sent at once');
}

// ─── the audio ring ───
{
  const ring = S.createAudioRing({ seconds: 2, rate: 16000 });
  ring.push(pcm(tone(300, 3)));
  ok(Math.abs(ring.seconds - 3) < 1e-6 && ring.span(0, 0.5) === null && ring.span(2, 2.5).length === 8000, 'the ring keeps the last seconds only');
  const r44 = S.createAudioRing({ rate: 44100 });
  for (let i = 0; i < 10; i++) r44.push(pcm(tone(300, 0.1, 44100)));
  ok(Math.abs(r44.seconds - 1) < 0.001, `44.1 kHz chunks keep the stream's time exactly (${r44.seconds.toFixed(4)} s)`);
  const u = S.utteranceAudio(ring, [[2.1, 2.4], [2.35, 2.6]]);
  ok(u && Math.abs(u.seconds - 0.5) < 1e-6, 'an utterance\'s seconds are its words\', overlaps merged');
}

// ─── an unrecognised voice: questions yes, changes no ───
{
  let unrec = true;
  const ran = [];
  const ctx = { locked: () => false, unrecognised: () => unrec, pending: createPending() };
  const wakeupDefs = [
    { name: 'fake_read', read: true, description: '', input_schema: { type: 'object', properties: {} }, run: () => { ran.push('read'); return { ok: true }; } },
    { name: 'fake_safe', safe: true, description: '', input_schema: { type: 'object', properties: {} }, run: () => { ran.push('safe'); return { ok: true }; } },
  ];
  const tools = makeActionTools({ listProjects: () => [], chats: () => [], voiceSettings: settings, wakeupDefs }, ctx);
  const t = (n) => tools.find((x) => x.name === n);
  ok((await t('fake_read').run({})).ok && ran.includes('read'), 'unrecognised: reading still works');
  const r = await t('fake_safe').run({});
  ok(r.unrecognised && r.say === UNRECOGNISED && !ran.includes('safe'), 'unrecognised: even a safe change refuses');
  ok(/couldn.t recognise who you are/.test(UNRECOGNISED) && !/your voice/i.test(UNRECOGNISED.split('.')[0]), 'the refusal speaks of who, not of the voice');
  const c = await t('confirm_action').run({ id: 'x' });
  ok(c.unrecognised, 'unrecognised: confirm_action refuses');
  unrec = false;
  ok((await t('fake_safe').run({})).ok && ran.includes('safe'), 'recognised: it runs');
  ok(/not recognised as the owner/.test(contextLine({ unrecognised: true, admin: false })) && !/recognised/.test(contextLine({ admin: false })), 'Cru is told in the context line, only when it applies');
}

// ─── admin mode obeys the owner's voice only ───
{
  let v = null;
  const capable = Object.keys(settings.MODELS).find((m) => !/haiku/.test(m));
  settings.updateSettings({ model: capable, adminMode: true });
  const tools = makeAdminTools({ settings, ownerVoice: () => v, run: async () => ({ ok: true, stdout: 'ran' }) });
  const run = tools.find((x) => x.name === 'run_command');
  v = S.turnPolicy({ enrolled: true, verdict: { owner: null, reason: 'short' } }).admin;
  let r = await run.run({ command: 'echo x' });
  ok(!r.ok && /longer/.test(r.error), 'admin, a short phrase: refused, say it longer');
  v = S.turnPolicy({ enrolled: true, verdict: { owner: false, reason: 'other' } }).admin;
  r = await run.run({ command: 'echo x' });
  ok(!r.ok && /not recognised/.test(r.error), 'admin, another voice: refused');
  v = S.turnPolicy({ enrolled: true, verdict: { owner: true } }).admin;
  ok((await run.run({ command: 'echo x' })).ok, 'admin, the owner\'s voice: runs');
  v = S.turnPolicy({ enrolled: true, typed: true }).admin;
  ok((await run.run({ command: 'echo x' })).ok, 'admin, typed: runs');
  settings.updateSettings({ adminMode: false });
}

// ─── short replies: taught, stored, and a bare "yes" checked ───
{
  // The rule, by numbers.
  const D = S.decideShort;
  ok(D({ word: 0.6, main: 0.4, seconds: 0.4 }) === true, 'a short reply like the taught ones and like the voice: the owner');
  ok(D({ word: 0.6, main: 0.2, seconds: 0.4 }) === null, 'like a taught reply but not like the voice: no verdict');
  ok(D({ word: 0.4, main: 0.4, seconds: 0.5 }) === null, 'in between: no verdict (short), never another voice');
  ok(D({ word: 0.2, main: 0.1, seconds: 0.5 }) === false, 'clearly unlike both: another voice');
  ok(D({ word: 0.9, main: 0.9, seconds: 0.2 }) === null, `under ${S.WORD_MIN_SECONDS} s: no verdict even so`);
  ok(D({ word: 0.35, main: 0.37, seconds: 1.0 }) === true, 'from 0.8 s the phrases\' own bar still counts');
  ok(D({ word: 0.35, main: 0.3, seconds: 1.0 }) === null, 'and below it, unsure is no verdict rather than another voice');
  ok(D({ word: NaN, main: 0.9, seconds: 0.5 }) === null, 'no word score: no verdict');
  const u = (i) => Float32Array.from({ length: 4 }, (_, j) => (j === i ? 1 : 0));
  ok(Math.abs(S.wordScore(u(0), [u(0), u(0), u(1), u(0)]) - 1) < 1e-6 && Math.abs(S.wordScore(u(0), [u(0), u(1), u(1)]) - 1 / 3) < 1e-6, 'the word score is the mean of the best three');
  const says = S.splitSayings(join_(silence(0.4), tone(300, 0.4), silence(0.5), tone(300, 0.45), silence(0.5), tone(300, 0.4), silence(0.4)));
  ok(says.length === 3 && says.every((x) => Math.abs(x.seconds - 0.4) < 0.07), `a reply said three times is three sayings (${says.map((x) => x.seconds).join(', ')} s)`);

  // Taught through the speaker (enrolled above with the fake model).
  for (let i = 0; i < S.MIN_PHRASES; i++) await spk.enrolPhrase(tone(300 + i * 5, 3), { restart: i === 0 });
  ok(spk.finish().ok, 'enrolled again for the short replies');
  let st = await spk.status();
  ok(st.shortWords && st.shortWords.trained === false && st.wordPrompts.length >= 8 && st.wordRepeats === 3, 'status: short replies not taught yet, the replies to say offered');
  ok((await spk.verify(tone(310, 1), { seconds: 0.5 })).reason === 'short' && !spk.shortWords, 'not taught: a short reply is still too short');
  const thrice = (hz) => join_(silence(0.3), tone(hz, 0.4), silence(0.5), tone(hz, 0.4), silence(0.5), tone(hz, 0.4), silence(0.3));
  let r = await spk.enrolWord(thrice(300), { word: 0, restart: true });
  ok(r.ok && r.accepted && r.sayings === 3 && r.words === 1 && r.likeYou > 0.9, 'a reply said three times is kept');
  r = await spk.enrolWord(join_(silence(0.3), tone(300, 0.4), silence(0.3)), { word: 1 });
  ok(r.ok && !r.accepted && /times/.test(r.problem), 'said once: asked to say it again, a few times');
  r = await spk.enrolWord(thrice(2000), { word: 1 });
  ok(r.ok && !r.accepted && /did not sound like/.test(r.problem), 'another voice teaching: not kept');
  r = await spk.enrolWord(tone(300, 3), { word: 1 });
  ok(!r.accepted && /just/.test(r.problem), 'a whole sentence: say just the word');
  ok((await spk.enrolWord(thrice(300), { word: 99 })).ok === false, 'a word not on the list: refused');
  ok(spk.finishWords().ok === false, `fewer than ${S.MIN_WORDS} replies: not saved`);
  for (let w = 1; w < S.MIN_WORDS; w++) await spk.enrolWord(thrice(290 + w * 5), { word: w });
  const fw = spk.finishWords();
  ok(fw.ok && fw.words === S.MIN_WORDS && fw.clips === S.MIN_WORDS * 3, `saved: ${fw.words} replies, ${fw.clips} clips`);
  const rec = JSON.parse(readFileSync(join(sdir, 'speaker.json'), 'utf8'));
  ok(rec.mean.length === 8 && rec.words.list.length === S.MIN_WORDS && rec.words.list[0].word === 'Yes' && rec.words.list[0].clips.length === 3, 'kept beside the profile, in the same private file');
  if (process.platform !== 'win32') ok((statSync(join(sdir, 'speaker.json')).mode & 0o077) === 0, 'still private (0600)');
  st = await spk.status();
  ok(st.shortWords.trained && st.shortWords.words === S.MIN_WORDS && st.enrolled, 'status: short replies taught');
  const fresh = S.createSpeaker({ dir: sdir, embedder: emb });
  ok(fresh.shortWords, 'they survive a restart');
  const yes = await fresh.verify(tone(310, 0.5), { seconds: 0.4 });
  ok(yes.owner === true && yes.brief === true && yes.word > 0.9, 'a bare "yes" in the owner\'s voice: the owner, brief');
  ok(S.turnPolicy({ enrolled: true, verdict: yes }).unrecognised === false && S.turnPolicy({ enrolled: true, verdict: yes }).admin.owner === false, 'it may act, but admin mode still wants more');
  const no = await fresh.verify(tone(1800, 0.5), { seconds: 0.4 });
  ok(no.owner === false && no.reason === 'other', 'a bare "yes" in another voice: another voice');
  ok((await fresh.verify(tone(310, 0.5), { seconds: 0.2 })).reason === 'short', 'a blip is still too short');

  // The live stream checks a short word now.
  class FakeWs extends EventEmitter {
    constructor(url) { super(); this.url = url; this.readyState = 0; FakeWs.last = this; }
    send() {}
    close() { this.readyState = 3; }
  }
  const client = new EventEmitter();
  client.readyState = 1; client.out = [];
  client.send = (m) => client.out.push(JSON.parse(m));
  client.close = () => {};
  relay(client, { WebSocketImpl: FakeWs, audio: { encoding: 'linear16', sample_rate: '16000' }, speaker: fresh });
  const dg = FakeWs.last; dg.readyState = 1; dg.emit('open');
  const audio = join_(silence(0.2), tone(300, 0.5), silence(0.3));
  for (let i = 0; i < audio.length; i += 1600) client.emit('message', pcm(audio.subarray(i, i + 1600)), true);
  dg.emit('message', JSON.stringify({ type: 'Results', is_final: true, start: 0.2, duration: 0.5, channel: { alternatives: [{ transcript: 'yes', words: [{ word: 'yes', start: 0.22, end: 0.62 }] }] } }));
  dg.emit('message', JSON.stringify({ type: 'UtteranceEnd' }));
  await tick(); await tick();
  ok(client.out.find((m) => m.type === 'utterance')?.owner === true, 'the live stream: "yes" (0.4 s) in the owner\'s voice is his');
  client.emit('message', JSON.stringify({ type: 'stop' }), false);

  ok(fresh.forgetWords().ok && !fresh.shortWords && fresh.enrolled, 'the short replies can be forgotten on their own');
  ok((await fresh.verify(tone(310, 0.5), { seconds: 0.4 })).reason === 'short', 'and a bare "yes" is too short again');
  await fresh.enrolWord(thrice(300), { word: 0, restart: true });
  for (let w = 1; w < S.MIN_WORDS; w++) await fresh.enrolWord(thrice(300), { word: w });
  ok(fresh.finishWords().ok && fresh.shortWords, 'taught again');
  for (let i = 0; i < S.MIN_PHRASES; i++) await fresh.enrolPhrase(tone(300, 3), { restart: i === 0 });
  ok(fresh.finish().ok && !fresh.shortWords, 'enrolling the voice again starts without short replies');
}

// ─── settings and forgetting ───
{
  ok(settings.publicSettings().ownerOnly === true, '"Only act on my voice" is on by default');
  ok(settings.updateSettings({ ownerOnly: false }).ok && settings.publicSettings().ownerOnly === false, 'and can be switched off');
  ok(settings.updateSettings({ ownerOnly: 'yes' }).ok === false, 'only true or false');
  settings.updateSettings({ ownerOnly: true });
  ok(spk.forget().ok && !spk.enrolled && !existsSync(join(sdir, 'speaker.json')), 'forgetting removes the profile');
  ok(spk.recent.lookup('delete everything') === null, 'and the remembered verdicts');
}

// ─── the real model, when it is here ───
const real = process.env.CRUNDI_SPEAKER_MODEL_DIR;
if (real && existsSync(join(real, S.MODEL.file)) && await S.loadOrt()) {
  const e = S.createOnnxEmbedder({ modelDir: real });
  const v = await e.embed(tone(300, 2));
  ok(v.length === S.MODEL.dim, `the real model gives a ${S.MODEL.dim}-number embedding`);
} else console.log('SKIP  the real model (set CRUNDI_SPEAKER_MODEL_DIR to a folder holding it)');

console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
