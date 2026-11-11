#!/usr/bin/env node
/**
 * test-voice-queue.mjs — the owner's turns and Cru's reports / wake-ups share
 * one brain (voice/brain.js, voice/turns.js createTurnQueue), and the voice
 * screen tells news queued behind an answer (crundi-voice.js nextReport).
 *
 * 12 Nov 2026, from the owner: he asked something, and while Cru answered a
 * watched chat finished its turn. The report gave up as "busy", and the
 * screen stayed thinking for good, so the answer never showed and what he
 * said next ("show me some factory settings") waited unsent until later.
 * Pure node. Run: node scripts/test-voice-queue.mjs
 */
import { readFileSync, mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { EventEmitter } from 'node:events';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

// Settings (the relay needs a Deepgram key) in a folder of the test's own, never the server's.
const dir = mkdtempSync(join(tmpdir(), 'voice-queue-test-'));
process.env.DATA_DIR = join(dir, 'data');
mkdirSync(process.env.DATA_DIR, { recursive: true });

const { createTurnQueue, BUSY_ERROR } = await import('../src/voice/turns.js');
const { createVoiceBrain } = await import('../src/voice/brain.js');

let failed = 0;
const ok = (c, m) => { if (c) console.log('PASS  ' + m); else { failed++; console.log('FAIL  ' + m); } };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * A pretend brain as agent.js / agent-cli.js are: one turn at a time
 * ("Still answering" when busy), and report() waits only so long for a busy
 * turn (30 s there, [reportWait] ms here) before it gives up as 'busy'.
 */
function fakeBrain({ turnMs = 80, reportWait = 30 } = {}) {
  let busy = false; const ran = []; let cancelled = 0;
  const turn = async (text, { report = false } = {}) => {
    if (busy) return { ok: false, error: BUSY_ERROR };
    busy = true; ran.push(text);
    try { await sleep(report ? 20 : turnMs); return { ok: true, speak: `done: ${text}`, show: '' }; } finally { busy = false; }
  };
  return {
    ran, get cancelled() { return cancelled; },
    turn,
    async report(note) {
      for (let i = 0; i < reportWait / 5 && busy; i++) await sleep(5);
      if (busy) return { ok: false, error: 'busy' };
      return turn(note, { report: true });
    },
    cancel() { cancelled++; return { ok: true, cancelled: busy, acted: [] }; },
    compact: async () => ({ ok: false, skipped: 'short' }),
    warm: async () => ({ ok: true }), reset() {}, lastActivity: 0,
  };
}
const settings = { getSettings: () => ({ brain: 'api', model: 'test' }) };

{
  // The bug: the owner's turn outlasts the report's wait.
  const b = fakeBrain({ turnMs: 120, reportWait: 30 });
  const old = b.turn('what is the weather');
  await sleep(10);
  const r = await b.report('[Report] the API chat has finished its turn');
  await old;
  ok(!r.ok && r.error === 'busy', 'reproduced: straight to the brain, a report behind a long owner turn is dropped as busy');
}
{
  const b = fakeBrain({ turnMs: 120, reportWait: 30 });
  const cru = createVoiceBrain({ api: b, settings });
  const owner = cru.turn('what is the weather');
  await sleep(10);
  const report = cru.report('[Report] the API chat has finished its turn');
  const [o, r] = await Promise.all([owner, report]);
  ok(o.ok && o.speak === 'done: what is the weather', 'through the queue, the owner\'s answer arrives');
  ok(r.ok && r.speak.startsWith('done: [Report]'), 'and the report is told after his turn, not dropped');
  ok(b.ran.join('|') === 'what is the weather|[Report] the API chat has finished its turn', 'in that order, one at a time');
}
{
  // He speaks again while a report waits: his turn goes first.
  const b = fakeBrain({ turnMs: 60 });
  const cru = createVoiceBrain({ api: b, settings });
  const first = cru.turn('first');
  await sleep(5);
  const report = cru.report('[Report] chat done');
  await sleep(5);
  const second = cru.turn('show me some factory settings');
  const [, s, r] = await Promise.all([first, second, report]);
  ok(s.ok && r.ok, 'his next request and the waiting report are both answered');
  ok(b.ran.join('|') === 'first|show me some factory settings|[Report] chat done', 'and his request goes ahead of the report still waiting');
}
{
  // He speaks while a report is being answered: it waits for that one, never "busy", never lost.
  const b = fakeBrain({ turnMs: 30 });
  const cru = createVoiceBrain({ api: b, settings });
  const report = cru.report('[Report] chat done');
  await sleep(5);
  const s = await cru.turn('show me some factory settings');
  await report;
  ok(s.ok && s.speak === 'done: show me some factory settings', 'said during a report, his request is answered right after it');
  ok(b.cancelled === 0, 'and the report is not cut off for it');
}
{
  // Cancel: his waiting request goes; a report being said is left alone.
  const b = fakeBrain({ turnMs: 30 });
  const cru = createVoiceBrain({ api: b, settings });
  const report = cru.report('[Report] chat done');
  await sleep(5);
  const mine = cru.turn('never mind this');
  const c = cru.cancel();
  const [m, r] = await Promise.all([mine, report]);
  ok(c.cancelled && m.cancelled && !b.ran.includes('never mind this'), 'cancel takes back his waiting request, never asked');
  ok(r.ok && b.cancelled === 0, 'and leaves the report running');
}
{
  // Nothing waits for ever behind a turn that never ends.
  const q = createTurnQueue();
  const alive = setTimeout(() => {}, 5000);   // the queue's timers are unref'd (they never hold the server up)
  let end; q.run(() => new Promise((r) => { end = r; }));
  const r = await q.run(async () => ({ ok: true }), { owner: true, waitMs: 30 });
  ok(!r.ok && r.error === BUSY_ERROR, 'a turn kept waiting past its limit is answered busy, not left hanging');
  end({ ok: true });
  ok((await q.run(async () => 'next')) === 'next', 'and the queue carries on once the stuck one ends');
  clearTimeout(alive);
}
{
  const q = createTurnQueue(); const order = [];
  const a = q.run(async () => { await sleep(10); order.push('turn'); });
  const b = q.run(async () => { order.push('report'); });
  const c = q.run(async () => { order.push('compact'); }, { front: true });
  const d = q.run(async () => { order.push('owner'); }, { owner: true });
  await Promise.all([a, b, c, d]);
  ok(order.join() === 'turn,compact,owner,report', 'a compaction Cru asked for goes next, then the owner, then news');
}

// ─── The relay (voice/stream.js): words held by a pause ───
{
  const vs = await import('../src/voice/settings.js');
  const vstream = await import('../src/voice/stream.js');
  vs.updateSettings({ deepgramKey: 'dg-test' });
  class FakeWs extends EventEmitter {
    constructor(url, opts) { super(); this.url = url; this.opts = opts; this.sent = []; this.readyState = 0; FakeWs.last = this; }
    send(d) { this.sent.push(d); }
    close() { this.readyState = 3; this.emit('close'); }
  }
  const R = (text, f = {}) => ({ type: 'Results', start: f.start || 0, duration: f.duration || 1, is_final: !!f.final, speech_final: !!f.sf, channel: { alternatives: [{ transcript: text }] } });
  const client = new EventEmitter();
  client.readyState = 1; client.out = [];
  client.send = (m) => client.out.push(JSON.parse(m));
  client.close = () => { client.readyState = 3; };
  vstream.relay(client, { WebSocketImpl: FakeWs, pauseMs: 1000 });
  const dg = FakeWs.last;
  dg.readyState = 1; dg.emit('open');
  const said = () => client.out.filter((m) => m.type === 'utterance');
  // He says it; a report begins (the screen pauses the stream) before Deepgram has ended his utterance.
  dg.emit('message', JSON.stringify(R('show me some factory settings', { final: true, sf: true, duration: 2 })));
  client.emit('message', JSON.stringify({ type: 'saying', text: 'Sir, the API chat has finished.' }), false);
  client.emit('message', JSON.stringify({ type: 'pause' }), false);
  ok(said().length === 0, 'paused before the utterance ended: nothing yet (Deepgram gets no more audio, so no UtteranceEnd)');
  await sleep(1150);
  const u = said();
  ok(u.length === 1 && u[0].text === 'show me some factory settings' && u[0].held === true, 'after the pause, his words go on their own, marked held');
  // Resumed; his next sentence is his next sentence, nothing joined to it.
  client.emit('message', JSON.stringify({ type: 'resume' }), false);
  dg.emit('message', JSON.stringify(R('and the kanban board', { final: true, sf: true, start: 3, duration: 1 })));
  dg.emit('message', JSON.stringify({ type: 'UtteranceEnd' }));
  const u2 = said();
  ok(u2.length === 2 && u2[1].text === 'and the kanban board' && !u2[1].held, 'reproduced fix: the next utterance is not merged with the one before');
  // Resumed in time: Deepgram ends it as before, no held copy.
  dg.emit('message', JSON.stringify(R('one more', { final: true, start: 5, duration: 1 })));
  client.emit('message', JSON.stringify({ type: 'pause' }), false);
  client.emit('message', JSON.stringify({ type: 'resume' }), false);
  dg.emit('message', JSON.stringify({ type: 'UtteranceEnd' }));
  await sleep(1150);
  const u3 = said();
  ok(u3.length === 3 && u3[2].text === 'one more' && !u3[2].held, 'resumed before the wait is up: one utterance, ended by Deepgram');
  client.emit('message', JSON.stringify({ type: 'stop' }), false);
}

// ─── The voice screen (app/vendor/crundi-voice.js) ───
{
  const w = { addEventListener() {}, location: {}, navigator: {} };
  const src = readFileSync(fileURLToPath(new URL('../app/vendor/crundi-voice.js', import.meta.url)), 'utf-8');
  vm.runInNewContext(src, { window: w, document: { addEventListener() {}, createElement() { return {}; } }, navigator: {}, localStorage: { getItem() { return null; } } });
  const T = w.CrundiVoice._test;
  // Thinking on "what is the weather"; the report came meanwhile and was queued.
  const s = { phase: 'think', heard: 'what is the weather', pending: '', reports: [{ speak: 'The API chat has finished.', show: '' }] };
  ok(!T.canTell(s), 'reproduced: as the answer comes in, the screen is not yet listening, so tell() would queue the report again (stuck thinking)');
  const q = T.nextReport(s);
  ok(q && q.speak === 'The API chat has finished.' && !s.reports.length, 'after the answer, the queued report is taken');
  ok(T.canTell(s) && s.phase === 'listen' && !s.heard, 'and the screen is listening again, so it is told (no longer stuck)');
  const p = { phase: 'speak', heard: 'show me some factory settings', pending: 'show me some factory settings', reports: [{ speak: 'news', show: '' }] };
  ok(T.nextReport(p) === null && p.reports.length === 1 && p.phase === 'speak', 'what he said meanwhile goes before the news (it waits, and is not lost)');
  ok(T.nextReport({ phase: 'speak', heard: '', pending: '', reports: [] }) === null, 'no news: nothing changes');
  // A held utterance while Cru says the report: queued in sight, asked after (afterSpeech), not dropped.
  const h = { phase: 'speak', heard: '', pending: '', reports: [] };
  ok(T.holdUtterance(h, 'show me some factory settings') && h.pending === 'show me some factory settings', 'held words that come while Cru speaks wait as his next request');
  ok(T.queuedOf(h) === 'show me some factory settings', 'and stay in the chat view as queued (they used to vanish)');
  ok(!T.holdUtterance({ phase: 'listen', heard: '', pending: '' }, 'hi') && !T.holdUtterance({ phase: 'speak', kid: true, pending: '' }, 'hi') && !T.holdUtterance({ phase: 'speak', pending: '' }, 'scratch that'), 'not held when listening (asked at once), in kid mode, or for a take-back');
  ok(T.dropLive(h, 'queued') === true && h.pending === '' && h.heard === '', 'its X still takes it back while Cru speaks');
}
rmSync(dir, { recursive: true, force: true });

if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nall passed');
