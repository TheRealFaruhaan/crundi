#!/usr/bin/env node
/**
 * test-voice-say-now.mjs — Cru speaks more than once in a turn (voice/say-now.js,
 * capability 'voice-say-now'): say_now puts a line in front of the owner at once,
 * as its own message, and waits until it has been said, so the next step (the
 * next ring of a walkthrough) lines up with it; the answer still ends the turn.
 * Pure node. Run: node scripts/test-voice-say-now.mjs
 */
import { mkdtempSync, mkdirSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const dir = mkdtempSync(join(tmpdir(), 'voice-say-now-test-'));
process.env.DATA_DIR = join(dir, 'data');
mkdirSync(process.env.DATA_DIR, { recursive: true });

const { makeSayNowTools, createSayAcks, foldUpdates, heardWithinMs, SAY_MAX_PER_TURN } = await import('../src/voice/say-now.js');
const { createVoiceAgent, runVoiceTool, SYSTEM_PROMPT } = await import('../src/voice/agent.js');
const { createVoiceBrain } = await import('../src/voice/brain.js');
const { splitTurn, createMemoryIndex } = await import('../src/voice/memory-index.js');
const vs = await import('../src/voice/settings.js');

let failed = 0;
const ok = (c, m) => { if (c) console.log('PASS  ' + m); else { failed++; console.log('FAIL  ' + m); } };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const signalsFor = (o = {}) => ({ pending: [], needsUnlock: false, phoneActions: [], close: false, acted: [], media: [], updates: [], requestId: 'w1', sayNow: true, report: false, ...o });

// ─── The tool ───
{
  const sent = []; const acks = createSayAcks(); const order = [];
  const [tool] = makeSayNowTools({ acks, send: (m) => { sent.push(m); order.push('say'); return 1; }, point: async (a) => { order.push('ring ' + a.highlight); return { ok: true, found: true }; } });
  ok(tool.name === 'say_now' && /walk him through/.test(tool.description) && /update/.test(tool.description), 'one general tool: updates, walkthroughs, anything said before the answer');
  const sig = signalsFor();
  const p = runVoiceTool(tool, 'say_now', { text: 'First, open the Kanban tab.', point: { highlight: 'Kanban' } }, sig);
  await sleep(5);
  ok(sent.length === 1 && sent[0].type === 'update' && sent[0].requestId === 'w1' && sent[0].speak === 'First, open the Kanban tab.', 'the line goes to the screen at once, for the request it belongs to');
  ok(order.join() === 'ring Kanban,say', 'the ring comes first, so the step\'s words and its ring arrive together');
  let done = false; p.then(() => { done = true; });
  await sleep(20);
  ok(!done, 'it waits until the screen has said it (the next step must not run ahead of his ears)');
  ok(acks.answer(sent[0].id, { how: 'said' }), 'the screen\'s ack is known');
  const r = JSON.parse((await p).content);
  ok(r.ok && r.heard === 'said' && r.pointed && r.pointed.ok, 'then it returns: said, and the ring was found');
  ok(sig.updates.length === 1 && sig.updates[0].speak === 'First, open the Kanban tab.' && !r.update, 'the line is kept with the turn, not handed back to the model');

  // Typed, muted, or he was speaking: the screen only shows it.
  const p2 = runVoiceTool(tool, 'say_now', { text: 'Checking.' }, sig);
  await sleep(5);
  acks.answer(sent[1].id, { how: 'shown' });
  ok(/shown/.test(JSON.parse((await p2).content).heard), 'only shown: the model is told');

  // No screen waits on this turn: kept for the answer, no wait.
  for (const [name, s2, want] of [['a report', signalsFor({ report: true, requestId: '' }), /before your report/], ['an older screen', signalsFor({ sayNow: false }), /with your answer/]]) {
    const before = sent.length;
    const t0 = Date.now();
    const o = JSON.parse((await runVoiceTool(tool, 'say_now', { text: 'Found it.' }, s2)).content);
    ok(sent.length === before && Date.now() - t0 < 50 && want.test(o.note) && s2.updates.length === 1, `${name}: not sent, not waited on, kept for the answer`);
  }
  // No voice screen open at all.
  const [lone] = makeSayNowTools({ acks, send: () => 0 });
  const s3 = signalsFor();
  const o3 = JSON.parse((await runVoiceTool(lone, 'say_now', { text: 'Hello.' }, s3)).content);
  ok(/No voice screen/.test(o3.note) && s3.updates.length === 1, 'no screen open: not waited on, it comes with the answer');
  // Not answered at all: the tool gives up by itself.
  const [deaf] = makeSayNowTools({ acks: { wait: async () => null }, send: () => 1 });
  const o4 = JSON.parse((await runVoiceTool(deaf, 'say_now', { text: 'Hello.' }, signalsFor())).content);
  ok(o4.ok && /did not say/.test(o4.note), 'no ack: it carries on');
  ok(heardWithinMs('short') >= 4000 && heardWithinMs('x'.repeat(5000)) <= 25000, 'the wait is bounded by the line\'s length');
  const full = signalsFor({ updates: Array.from({ length: SAY_MAX_PER_TURN }, (_, i) => ({ id: 'u' + i, speak: 'x' })) });
  ok(JSON.parse((await runVoiceTool(lone, 'say_now', { text: 'More.' }, full)).content).ok === false, 'a limit per turn: not a running commentary');
  ok(JSON.parse((await runVoiceTool(lone, 'say_now', { text: '  ' }, signalsFor())).content).ok === false, 'nothing to say: refused');
}

// ─── An older screen gets the lines in front of the answer ───
{
  const r = foldUpdates({ ok: true, speak: 'Done, Sir.', updates: [{ id: 'a', speak: 'Checking the server.' }, { id: 'b', speak: 'Found it.' }] });
  ok(r.speak === 'Checking the server. Found it. Done, Sir.', 'folded: every word is still heard, in order');
  ok(foldUpdates({ ok: true, speak: 'Hi.' }).speak === 'Hi.' && foldUpdates({ ok: false, error: 'x' }).error === 'x', 'nothing to fold: unchanged');
}

// ─── The API brain: a walkthrough in one turn ───
{
  vs.updateSettings({ anthropicKey: 'sk-test', model: 'claude-haiku-5-5' });
  const sent = []; const acks = createSayAcks(); const rings = [];
  const say = makeSayNowTools({ acks, send: (m) => { sent.push(m); setTimeout(() => acks.answer(m.id, { how: 'said' }), 10); return 1; }, point: async (a) => { rings.push(a.highlight); return { ok: true }; } });
  const script = [
    { stop_reason: 'tool_use', usage: {}, content: [{ type: 'tool_use', id: 't1', name: 'say_now', input: { text: 'Open the Kanban tab.', point: { highlight: 'Kanban' } } }] },
    { stop_reason: 'tool_use', usage: {}, content: [{ type: 'tool_use', id: 't2', name: 'say_now', input: { text: 'Now click Add card.', point: { highlight: 'Add card' } } }] },
    { stop_reason: 'end_turn', usage: {}, content: [{ type: 'text', text: 'That is all, Sir.' }] },
  ];
  const timeline = [];
  const client = () => ({ messages: { create: async () => { timeline.push(`model (${sent.length} said)`); return script.shift(); } } });
  const remembered = [];
  const agent = createVoiceAgent({ tools: say, client, onTurnDone: (t) => remembered.push(t) });
  const r = await agent.turn('show me how to add a card', { requestId: 'w9', sayNow: true });
  ok(r.ok && r.speak === 'That is all, Sir.', 'the answer still ends the turn');
  ok(r.updates.length === 2 && r.updates.map((u) => u.speak).join(' | ') === 'Open the Kanban tab. | Now click Add card.', 'and carries the lines said during it, in order');
  ok(sent.every((m) => m.requestId === 'w9') && rings.join() === 'Kanban,Add card', 'each step went to his screen with its own ring');
  ok(timeline.join(', ') === 'model (0 said), model (1 said), model (2 said)', 'each step comes after the one before was said: they line up');
  ok(remembered[0] && remembered[0].updates.length === 2, 'kept with the turn');
  ok(/say_now/.test(SYSTEM_PROMPT), 'the system prompt tells Cru it can speak more than once');

  // Through the brain's queue, as a report: the lines come back with the report (said before it).
  const script2 = [
    { stop_reason: 'tool_use', usage: {}, content: [{ type: 'tool_use', id: 't3', name: 'say_now', input: { text: 'The build chat finished.' } }] },
    { stop_reason: 'end_turn', usage: {}, content: [{ type: 'text', text: 'All tests pass, Sir.' }] },
  ];
  const agent2 = createVoiceAgent({ tools: say, client: () => ({ messages: { create: async () => script2.shift() } }) });
  const brain = createVoiceBrain({ api: agent2, settings: { getSettings: () => ({ brain: 'api', model: 'claude-haiku-5-5' }) } });
  const before = sent.length;
  const rep = await brain.report('[Report] the build chat has finished its turn');
  ok(rep.ok && rep.updates.length === 1 && rep.speak === 'All tests pass, Sir.' && sent.length === before, 'a report keeps its lines for the report queue, never sent as a turn\'s');
}

// ─── Memory: each line of a turn is kept beside its reply ───
{
  const idx = createMemoryIndex({ dir: join(dir, 'mem'), embedder: { status: 'off', embed: async () => null } });
  idx.add('turn', 'Owner: show me how\nCru: That is all.', { updates: [{ id: 'a', speak: 'Open the Kanban tab.' }, { speak: 'Now click Add card.' }] });
  idx.add('turn', 'Owner: hello\nCru: Hello, Sir.');
  const [plain, walk] = idx.turns();
  ok(walk.updates && walk.updates.join(' | ') === 'Open the Kanban tab. | Now click Add card.' && walk.speak === 'That is all.', 'a turn\'s lines come back with it from the history');
  ok(!plain.updates, 'a turn without any has none');
  ok(splitTurn({ id: 1, at: 1, text: 'Owner: x\nCru: y', updates: 'not json' }).updates === undefined, 'an unreadable column is left out');
  idx.close();
}

// ─── The voice screen (app/vendor/crundi-voice.js): the history shows each line on its own ───
{
  const w = { addEventListener() {}, location: {}, navigator: {} };
  const src = readFileSync(fileURLToPath(new URL('../app/vendor/crundi-voice.js', import.meta.url)), 'utf-8');
  vm.runInNewContext(src, { window: w, document: { addEventListener() {}, createElement() { return {}; } }, navigator: {}, localStorage: { getItem() { return null; } } });
  const T = w.CrundiVoice._test;
  const m = T.expandTurns([{ at: 5, said: 'show me how', speak: 'That is all.', show: '', updates: ['Open the Kanban tab.', 'Now click Add card.'] }, { at: 6, said: 'hi', speak: 'Hello.' }]);
  ok(m.map((x) => x.said || '·').join('|') === 'show me how|·|·|·|hi', 'his words, then each line, then the answer');
  ok(m.map((x) => x.speak || '·').join('|') === '·|Open the Kanban tab.|Now click Add card.|That is all.|Hello.', 'each line its own message, in order');
}

rmSync(dir, { recursive: true, force: true });
if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nall passed');
