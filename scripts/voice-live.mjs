#!/usr/bin/env node
/**
 * voice-live.mjs: run real voice turns against the real Claude API.
 *
 * Not a test (it is not named test-*, so `npm run check` never runs it): it
 * spends a little API credit. It drives the real agent and the real read-only
 * tools over a Crundi data folder. Point DATA_DIR at a COPY of the data, not
 * the live one. Settings are kept in memory, so the key never touches disk.
 *
 *   DATA_DIR=/tmp/copy node scripts/voice-live.mjs \
 *     "how much usage do I have left?" "what's in the todo column of crundi?"
 *
 * The key is ANTHROPIC_API_KEY, or else the one in the copy's voice.json.
 * Every argument is one turn, in order, in one conversation. Before the first
 * turn it calls warm(), as a voice session does. Each turn prints what would
 * be spoken, what would be shown, the tools used, the cost and the cache use.
 * VOICE_MODEL picks the model (default claude-haiku-5-5).
 */

if (!process.env.DATA_DIR) { console.error('Set DATA_DIR to a copy of the Crundi data folder'); process.exit(2); }

const { createVoiceAgent } = await import('../src/voice/agent.js');
const { makeTools } = await import('../src/voice/tools.js');
const real = await import('../src/voice/settings.js');
const { listProjects } = await import('../src/project-store.js');
const kanban = await import('../src/kanban-store.js');
const notes = await import('../src/notes-store.js');
const mindmap = await import('../src/mindmap-store.js');
const schedule = await import('../src/schedule-store.js');
const usage = await import('../src/usage.js');

// The key: ANTHROPIC_API_KEY, else the one saved in the copy's voice.json.
const key = process.env.ANTHROPIC_API_KEY || real.getSettings().anthropicKey;
if (!key) { console.error('No key: set ANTHROPIC_API_KEY or save one in Voice assistant settings'); process.exit(2); }
const state = { model: process.env.VOICE_MODEL || real.DEFAULT_MODEL, anthropicKey: key, monthlyLimitUsd: 5, spent: 0 };
const settings = {
  DEFAULT_MODEL: real.DEFAULT_MODEL,
  getSettings: () => state,
  overLimit: () => state.spent >= state.monthlyLimitUsd,
  countTurn: () => {},
  claudeCost: real.claudeCost,
  addClaudeSpend: (model, u) => { state.spent += real.claudeCost(model, u); },
};

const tools = makeTools({
  listProjects: () => Object.values(listProjects()).map(p => ({ alias: p.alias, name: p.name || p.alias, path: p.path })),
  settings: () => ({ note: 'settings are not loaded in voice-live' }),
  voiceSpend: () => ({ spend: { totalUsd: state.spent }, monthlyLimitUsd: state.monthlyLimitUsd }),
  services: () => [],
  serviceLogs: () => '',
  notes, kanban, mindmap,
  schedules: (p) => schedule.listSchedules(p),
  usage: () => usage.getUsage(),
  secrets: () => [],
  chats: () => [], terminals: () => [],
  chatHistory: () => null, terminalScrollback: () => '',
});

// The actions too (on the copy), with no live chats: kanban, notes, mindmap,
// schedules act on the copied data; the confirmation gate is the real one.
const actions = await import('../src/voice/actions.js');
const pending = actions.createPending();
const ctx = { pending, locked: () => false };
const actionTools = actions.makeActionTools({
  chats: () => [], sendMessage: () => ({ ok: false, error: 'no chats in voice-live' }),
  createChat: async () => ({ ok: false, error: 'no chats in voice-live' }), closeChat: async () => ({ ok: true }),
  parkChat: () => ({ ok: true }), resumeChat: async () => ({ ok: true }), watch: () => {},
  listProjects: () => Object.values(listProjects()).map(p => ({ alias: p.alias, name: p.name || p.alias, path: p.path })),
  projectsDir: () => '', registerProject: () => ({ ok: false, error: 'not in voice-live' }),
  kanban, notes, mindmap, chatSchedule: { list: () => [], add: () => ({ ok: true }), remove: () => ({ ok: true }) },
  schedules: schedule, services: () => [], voiceSettings: { updateSettings: () => ({ ok: true }) },
}, ctx);
// Memory too, kept in the copy's data folder (DATA_DIR/voice-memory).
const { createVoiceMemory } = await import('../src/voice/memory.js');
const memory = createVoiceMemory({ dataDir: process.env.DATA_DIR });
const agent = createVoiceAgent({ tools: [...tools, ...actionTools, memory.notebook.tool, memory.searchTool], settings, beforeTurn: () => pending.newTurn(), store: memory.store, onTurnDone: (t) => memory.remember(t) });
const pct = (c) => { const all = c.read + c.written + c.uncached; return all ? Math.round(100 * c.read / all) : 0; };

const w = await agent.warm();
console.log('warm:', JSON.stringify(w));

for (const said of process.argv.slice(2)) {
  const t0 = Date.now();
  const r = await agent.turn(said, { where: 'voice-live script' });
  console.log(`\n> ${said}`);
  if (!r.ok) { console.log('  ERROR:', r.error); continue; }
  console.log(`  speak: ${r.speak}`);
  if (r.show) console.log(`  show:  ${r.show.replace(/\n/g, '\n         ')}`);
  if (r.pending?.length) console.log(`  awaiting yes: ${r.pending.map(p => p.question).join(' | ')}`);
  console.log(`  tools: ${r.tools.join(', ') || 'none'} · ${Date.now() - t0} ms · $${r.costUsd.toFixed(5)}`);
  console.log(`  cache: read ${r.cache.read}, written ${r.cache.written}, uncached ${r.cache.uncached} → ${pct(r.cache)}% from cache`);
}
console.log(`\ntotal spent: $${state.spent.toFixed(5)}`);
