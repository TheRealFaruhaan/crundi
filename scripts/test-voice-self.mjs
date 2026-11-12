#!/usr/bin/env node
/**
 * test-voice-self.mjs — Cru's hold on itself (9 Nov): its volume, reordering
 * kanban cards, and compacting or restarting its own conversation after a turn.
 * Run: node scripts/test-voice-self.mjs
 */
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
process.env.DATA_DIR = mkdtempSync(join(tmpdir(), 'cru-self-'));
const settings = await import('../src/voice/settings.js');
const { makeActionTools: makeVoiceActionTools } = await import('../src/voice/actions.js');
const { createVoiceBrain } = await import('../src/voice/brain.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

// Volume: a setting, checked, public.
ok(settings.publicSettings().volume === 100, 'volume starts at 100');
ok(settings.updateSettings({ volume: 5 }).ok === false, 'below 10 is refused');
ok(settings.updateSettings({ volume: 40 }).ok && settings.publicSettings().volume === 40, 'volume 40 is kept');

const moved = [];
let told = 0;
const asked = [];
const tools = makeVoiceActionTools({
  voiceSettings: settings,
  onSettingsChanged: () => { told++; },
  listProjects: () => [{ alias: 'crundi', name: 'Crundi' }],
  kanban: { moveTask: (...a) => { moved.push(a); return { ok: true }; } },
  conversation: { requestAfterTurn: (k, s) => asked.push([k, s]) },
}, { locked: () => true, pending: { newTurn() {}, propose() { return 'x'; }, take() { return {}; }, cancel() {} } });
const tool = (n) => tools.find((t) => t.name === n);

const v = await tool('set_volume').run({ volume: 70 });
ok(v.ok && settings.publicSettings().volume === 70 && told === 1, 'set_volume changes it at once, even locked, and tells the screens');
ok(!v.needsConfirmation, 'no confirmation for volume');

ok(!!tool('move_kanban_task').input_schema.properties.position, 'move_kanban_task takes a position');

await tool('compact_conversation').run({});
await tool('start_fresh').run({ summary: 'Working on the inbox.' });
ok(asked.length === 2 && asked[0][0] === 'compact' && asked[1][0] === 'fresh' && asked[1][1] === 'Working on the inbox.', 'compact and fresh are asked of the brain');

// The brain does it after the turn, not during it.
const log = [];
const fake = {
  turn: async () => { log.push('turn'); brain.requestAfterTurn('fresh', 'carry this'); return { ok: true, speak: 'Done.' }; },
  report: async () => ({ ok: true }),
  compact: async () => { log.push('compact'); return { ok: true }; },
  fresh: (s) => { log.push('fresh:' + s); return { ok: true }; },
  lastActivity: 0,
};
const brain = createVoiceBrain({ api: fake, cli: null });
const r = await brain.turn('start over');
await new Promise((ok2) => setTimeout(ok2, 10));
ok(r.speak === 'Done.' && log.join(',') === 'turn,fresh:carry this', 'start_fresh runs after the turn, with its summary');
await brain.turn('next');
await new Promise((ok2) => setTimeout(ok2, 10));
ok(log.filter((x) => x.startsWith('fresh')).length === 2, 'only once per request');

// The chat view's history: remembered turns split back into messages.
const { createMemoryIndex } = await import('../src/voice/memory-index.js');
const idx = createMemoryIndex({ dir: join(process.env.DATA_DIR, 'idx'), embedder: { embed: async () => null, status: 'off' } });
idx.add('turn', 'Owner: hello\nCru: Hi, Sir.\nThe detail', { at: 1000 });
idx.add('turn', 'Owner: [Report for the owner, not said by them] a chat finished\nCru: Your chat is done.', { at: 2000 });
idx.add('summary', 'not a turn', { at: 3000 });
const th = idx.turns({ limit: 10 });
ok(th.length === 2 && th[0].report && th[0].said === '' && th[0].speak === 'Your chat is done.', 'a report is Cru speaking up, newest first');
ok(th[1].said === 'hello' && th[1].speak === 'Hi, Sir.' && th[1].show === 'The detail', 'a turn splits into said, spoken and shown');
ok(idx.turns({ before: 2000 }).length === 1, 'paged back with before');
idx.close();

// A new install starts with the general persona; an existing one is never touched.
const { createVoiceMemory } = await import('../src/voice/memory.js');
const { readFileSync: rf, writeFileSync: wf, mkdirSync: md } = await import('node:fs');
const fresh = join(process.env.DATA_DIR, 'fresh');
const m1 = createVoiceMemory({ dataDir: fresh, index: { add() {}, turns() { return []; } } });
ok(/How the owner wants Cru to behave/.test(rf(join(fresh, 'voice-memory', 'memories', 'persona.md'), 'utf8')), 'a fresh notebook starts with the default persona');
const kept = join(process.env.DATA_DIR, 'kept');
md(join(kept, 'voice-memory', 'memories'), { recursive: true });
wf(join(kept, 'voice-memory', 'memories', 'persona.md'), 'mine');
createVoiceMemory({ dataDir: kept, index: { add() {}, turns() { return []; } } });
ok(rf(join(kept, 'voice-memory', 'memories', 'persona.md'), 'utf8') === 'mine', 'an existing persona is left alone');
void m1;

// A short "yes" (too short to tell whose voice) may confirm what a recognised request asked, and nothing else.
{
  const { turnPolicy } = await import('../src/voice/speaker.js');
  const { createPending, makeActionTools: mk } = await import('../src/voice/actions.js');
  const short = { owner: null, reason: 'short' };
  ok(turnPolicy({ enrolled: true, verdict: short, answering: true }).confirmOk === true, 'short yes with a recognised question waiting may confirm');
  ok(turnPolicy({ enrolled: true, verdict: short, answering: false }).confirmOk === false, 'short yes with nothing recognised waiting may not');
  ok(turnPolicy({ enrolled: true, verdict: { owner: false }, answering: true }).confirmOk === false, 'another voice never confirms');
  const pend = createPending();
  let unrec = false, confirmOk = true, verified = true;
  const ctx = { pending: pend, locked: () => false, unrecognised: () => unrec, confirmOk: () => confirmOk, verified: () => verified };
  const ran = [];
  const tools2 = mk({ listProjects: () => [{ alias: 'crundi' }], kanban: { getTask: () => ({ id: 'x' }), deleteTask: (...a) => { ran.push(a); return { ok: true }; } }, voiceSettings: settings }, ctx);
  const del = tools2.find((x) => x.name === 'delete_kanban_task'), conf = tools2.find((x) => x.name === 'confirm_action');
  pend.newTurn();
  const asked = await del.run({ project: 'crundi', taskId: 'x1' });
  ok(asked.needsConfirmation && pend.verifiedWaiting(), 'a recognised request asks and is marked verified');
  pend.newTurn(); unrec = true; confirmOk = true;
  const other = await del.run({ project: 'crundi', taskId: 'x2' });
  ok(other.unrecognised, 'the short "yes" turn cannot start anything else');
  const yes = await conf.run({ id: asked.id });
  ok(ran.length === 1 && !yes.unrecognised, 'but it confirms the recognised request');
  // A question asked in an unrecognised turn is not confirmable by a short yes.
  pend.newTurn(); unrec = true; confirmOk = false; verified = false;
}

console.log(failed ? `\n${failed} FAILED` : '\nAll Cru-self checks passed');
process.exit(failed ? 1 : 0);
