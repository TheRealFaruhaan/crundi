#!/usr/bin/env node
/**
 * test-voice.mjs — the voice assistant's server side (src/voice/*).
 *
 *   settings   keys never leave the server; model list; monthly limit; the
 *              month rolls over; cost arithmetic
 *   stt        Crundi's words and the live project/service names go to
 *              Deepgram as keyterms; a reply is parsed; the spend is counted
 *   tools      the read-only list is pinned; files stay inside the project;
 *              credentials and secret values never reach the model
 *   agent      the tool loop, the spoken/shown split, append-only history,
 *              refusals, the limit, the fallback only where it exists
 *   access     collaborators can't reach /api/voice
 *
 * Run: node scripts/test-voice.mjs
 */

import { mkdtempSync, rmSync, mkdirSync, writeFileSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'voice-test-'));
process.env.DATA_DIR = join(dir, 'data');
mkdirSync(process.env.DATA_DIR, { recursive: true });

const vs = await import('../src/voice/settings.js');
const stt = await import('../src/voice/stt.js');
const vt = await import('../src/voice/tools.js');
const { createVoiceAgent, splitReply, SYSTEM_PROMPT } = await import('../src/voice/agent.js');
const { mayAccess, ROLE_COLLABORATOR, ROLE_OWNER } = await import('../src/access-policy.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };
const throwsWith = (fn, re) => { try { fn(); return false; } catch (e) { return re.test(String(e.message)); } };

// ─── settings ───
{
  const p = vs.publicSettings();
  ok(p.model === 'claude-haiku-5-5', 'default model is Claude Haiku 5.5 (D8)');
  ok(p.monthlyLimitUsd === 50, 'default monthly limit is $50 (D25)');
  ok(p.models.map(m => m.id).join() === 'claude-haiku-5-5,claude-sonnet-5-5,claude-opus-5-5', 'three models offered');
  ok(!vs.updateSettings({ model: 'gpt-4' }).ok, 'an unknown model is refused');
  ok(!vs.updateSettings({ monthlyLimitUsd: -1 }).ok && !vs.updateSettings({ monthlyLimitUsd: 'x' }).ok, 'a bad limit is refused');
  vs.updateSettings({ anthropicKey: ' sk-ant-secret ', deepgramKey: 'dg-secret' });
  const pub = JSON.stringify(vs.publicSettings());
  ok(!pub.includes('sk-ant-secret') && !pub.includes('dg-secret'), 'public settings never carry the keys');
  ok(vs.publicSettings().anthropicKeySet && vs.publicSettings().deepgramKeySet, 'they say each key is set');
  ok(vs.getSettings().anthropicKey === 'sk-ant-secret', 'keys are stored trimmed');
  vs._reload();
  ok(vs.getSettings().deepgramKey === 'dg-secret', 'settings survive a reload from disk');

  // cost: Haiku 5.5, 1M in + 1M out = $0.10 + $0.50
  const c = vs.claudeCost('claude-haiku-5-5', { input_tokens: 1e6, output_tokens: 1e6 });
  ok(Math.abs(c - 0.60) < 1e-9, 'Haiku cost arithmetic');
  ok(vs.claudeCost('claude-opus-5-5', { cache_read_input_tokens: 1e6 }) === 0.20, 'cache reads priced separately');

  vs.updateSettings({ monthlyLimitUsd: 1 });
  const nov = new Date('2026-11-10T12:00:00Z');
  ok(!vs.overLimit(nov), 'under the limit at first');
  vs.addClaudeSpend('claude-opus-5-5', { output_tokens: 60000 }, nov); // $1.20
  ok(vs.overLimit(nov), 'over the limit once the month spent it');
  ok(vs.publicSettings(nov).limitReached, 'and says so');
  ok(!vs.overLimit(new Date('2026-12-01T00:00:01Z')), 'a new month starts from zero');
  vs.addDeepgramSpend(600, new Date('2026-12-02T00:00:00Z'));
  ok(Math.abs(vs.publicSettings(new Date('2026-12-02T00:00:00Z')).spend.deepgramUsd - 0.09) < 1e-6, 'Deepgram spend counted by the minute');
  vs.updateSettings({ monthlyLimitUsd: 50 });
}

// ─── stt ───
{
  const k = stt.keyterms({ projects: ['api-server', 'Crundi', 'my_app'], services: ['web_worker'] });
  ok(k.includes('kanban') && k.includes('mindmap'), 'Crundi vocabulary is always sent');
  ok(k.includes('api server') && k.includes('my app') && k.includes('web worker'), 'names are sent the way they are said');
  ok(k.filter(t => t.toLowerCase() === 'crundi').length === 1, 'duplicates dropped');
  ok(stt.keyterms({ projects: Array.from({ length: 500 }, (_, i) => `p${i}`) }).length === 100, 'capped at 100 keyterms');
  const u = new URL(stt.listenUrl(['kanban', 'api server']));
  ok(u.searchParams.get('model') === 'nova-3' && u.searchParams.getAll('keyterm').join('|') === 'kanban|api server', 'URL carries model and every keyterm');

  let sent = null;
  const fake = async (url, init) => {
    sent = { url, init };
    return { ok: true, json: async () => ({ metadata: { duration: 3 }, results: { channels: [{ alternatives: [
      { transcript: 'Add the UI overhaul of the home screen as a kanban task', confidence: 0.97 },
    ] }] } }) };
  };
  const before = vs.publicSettings().spend.deepgramUsd;
  const r = await stt.transcribe(Buffer.from('RIFF'), 'audio/wav', { projects: ['crundi'], fetchImpl: fake });
  ok(r.ok && r.text === 'Add the UI overhaul of the home screen as a kanban task' && r.confidence === 0.97, 'transcript parsed');
  ok(sent.init.headers.Authorization === 'Token dg-secret' && sent.init.headers['Content-Type'] === 'audio/wav', 'key and audio type sent');
  ok(vs.publicSettings().spend.deepgramUsd > before, 'spend added');
  const bad = await stt.transcribe(Buffer.from('x'), 'audio/wav', { fetchImpl: async () => ({ ok: false, status: 401, json: async () => ({ err_msg: 'Invalid credentials' }) }) });
  ok(!bad.ok && /401.*Invalid credentials/.test(bad.error), 'a refusal is reported with its reason');
  ok(!(await stt.transcribe(Buffer.alloc(0), 'audio/wav', { fetchImpl: fake })).ok, 'empty audio refused');
}

// ─── tools ───
const root = join(dir, 'proj');
mkdirSync(join(root, 'src'), { recursive: true });
writeFileSync(join(root, 'src', 'app.js'), 'console.log("hi")\n');
writeFileSync(join(root, 'big.txt'), 'x'.repeat(30000));
writeFileSync(join(root, 'bin.dat'), Buffer.from([1, 0, 2, 3]));
writeFileSync(join(dir, 'outside.txt'), 'secret');
try { symlinkSync(join(dir, 'outside.txt'), join(root, 'escape.txt')); } catch { /* no symlinks here */ }

const deps = {
  listProjects: () => [{ alias: 'proj', name: 'Proj', path: root }],
  settings: () => ({ settings: { TELEGRAM_BOT_TOKEN: '123:abc', CLOUDFLARE_TUNNEL_TOKEN: '', ALLOWED_USERNAME: 'master' }, autoParkMinutes: 30 }),
  voiceSpend: () => ({ totalUsd: 0 }),
  services: () => [{ key: 'proj:web', status: 'running' }],
  serviceLogs: (key, n) => key === 'proj:web' ? ['\x1b[32mready\x1b[0m'] : null,
  notes: { listPages: () => [], getPage: () => ({}) },
  kanban: { getBoard: () => ({ todo: [] }), getColumn: () => [], getTask: () => ({}) },
  mindmap: { getMindmap: () => [], searchMindmap: () => [] },
  schedules: () => [{ id: 's1', hookKey: 'k-123' }],
  usage: async () => ({ ok: true }),
  secrets: () => [{ id: '1', name: 'GITHUB_TOKEN', description: 'push', value: 'ghp_NEVER' }],
  chats: () => [{ id: 'c1', project: 'proj', title: 'API', status: 'running', agentState: 'idle', collaborator: { key: 'COLLAB-KEY' } }],
  terminals: () => [{ id: 't1', project: 'proj', title: 'sh', status: 'running' }],
  chatHistory: (id) => id === 'c1' ? { state: 'idle', messages: [{ kind: 'user', text: 'hi' }, { kind: 'tool', name: 'Read' }, { kind: 'assistant-text', text: 'done' }] } : null,
  terminalScrollback: () => '\x1b[1mnpm test\x1b[0m\r\n\x1b]0;title\x07passing 12\n',
};
const tools = vt.makeTools(deps);
const T = Object.fromEntries(tools.map(t => [t.name, t]));
{
  ok(tools.map(t => t.name).join() === vt.READ_ONLY_TOOL_NAMES.join(), 'the tool list is exactly the pinned read-only list');
  ok(!tools.some(t => /^(add|update|delete|move|create|write|send|close|start|stop|set|edit|remove)_/.test(t.name)), 'no tool name is a write verb');
  ok(tools.every(t => t.input_schema.additionalProperties === false), 'every schema is closed');

  ok(T.list_folder.run({ project: 'proj' }).entries.some(e => e.name === 'src' && e.type === 'folder'), 'list_folder lists the project');
  ok(throwsWith(() => T.list_folder.run({ project: 'proj', path: '..' }), /outside/), '.. is refused');
  ok(throwsWith(() => T.read_file.run({ project: 'proj', path: '/etc/passwd' }), /absolute/), 'absolute paths are refused');
  ok(throwsWith(() => T.read_file.run({ project: 'proj', path: 'src/../../outside.txt' }), /outside/), 'climbing out is refused');
  ok(throwsWith(() => T.read_file.run({ project: 'nope', path: 'a' }), /No project/), 'an unknown project is refused');
  try { symlinkSync; ok(throwsWith(() => T.read_file.run({ project: 'proj', path: 'escape.txt' }), /outside/), 'a symlink out of the project is refused'); } catch { /* skipped */ }
  ok(T.read_file.run({ project: 'proj', path: 'src/app.js' }).text.includes('console.log'), 'read_file reads');
  const big = T.read_file.run({ project: 'proj', path: 'big.txt' });
  ok(big.truncated && big.text.length === 20000, 'long files are cut');
  ok(T.read_file.run({ project: 'proj', path: 'bin.dat' }).binary === true, 'binary files are not dumped');
  ok(T.search_files.run({ project: 'proj', query: 'app' }).matches.includes('src/app.js'), 'search_files finds by path');

  const st = JSON.stringify(T.get_settings.run());
  ok(!st.includes('123:abc') && st.includes('"TELEGRAM_BOT_TOKEN":"set"') && st.includes('"CLOUDFLARE_TUNNEL_TOKEN":"not set"'), 'credentials in settings show only as set / not set');
  ok(st.includes('master'), 'ordinary settings are readable');
  ok(!JSON.stringify(T.list_schedules.run({})).includes('k-123'), 'schedule hook keys are hidden');
  const sec = JSON.stringify(T.list_secrets.run());
  ok(sec.includes('GITHUB_TOKEN') && !sec.includes('ghp_NEVER'), 'secrets: names, never values (D16)');
  ok(!JSON.stringify(T.list_panels.run()).includes('COLLAB-KEY'), 'a collaborator chat key never reaches the model');
  ok(T.get_service_logs.run({ key: 'proj:web' }).lines[0] === 'ready', 'service logs without colour codes');
  const term = T.read_terminal.run({ id: 't1' }).lines;
  ok(term.join('|') === 'npm test|passing 12', 'terminal output without escapes');
  const chat = T.read_chat.run({ id: 'c1' });
  ok(chat.messages.length === 3 && chat.messages[0].from === 'Master' && chat.messages[2].from === 'Claude', 'read_chat summarises the conversation');
  ok(throwsWith(() => T.read_chat.run({ id: 'zzz' }), /No open chat/), 'an unknown chat is refused');
}

// ─── agent ───
{
  ok(/Master/.test(SYSTEM_PROMPT) && /---/.test(SYSTEM_PROMPT) && /begin your visible answer immediately/.test(SYSTEM_PROMPT), 'system prompt: Master, the spoken/shown split, low latency');
  ok(/cannot change anything yet/.test(SYSTEM_PROMPT), 'system prompt says it is read only');
  const sp = splitReply('Three tasks in to do, Master.\n---\n- A\n- B\n- C');
  ok(sp.speak === 'Three tasks in to do, Master.' && sp.show === '- A\n- B\n- C', 'reply split into spoken and shown');
  ok(splitReply('Just this.').show === '', 'no detail, nothing shown');

  const calls = [];
  const script = [
    { stop_reason: 'tool_use', usage: { input_tokens: 1000, output_tokens: 50 }, content: [
      { type: 'thinking', thinking: '', signature: 'sig' },
      { type: 'tool_use', id: 'tu1', name: 'list_projects', input: {} },
      { type: 'tool_use', id: 'tu2', name: 'get_kanban', input: { project: 'proj' } },
    ] },
    { stop_reason: 'end_turn', usage: { input_tokens: 1200, output_tokens: 40 }, content: [
      { type: 'text', text: 'One project, Master, and the board is empty.\n---\nproj: no tasks' },
    ] },
  ];
  const fakeClient = () => ({
    messages: { create: async (req) => { calls.push({ ...req, messages: [...req.messages] }); return script.shift(); } },
    beta: { messages: { create: async (req) => { calls.push({ beta: true, ...req }); return { stop_reason: 'end_turn', usage: {}, content: [{ type: 'text', text: 'Yes.' }] }; } } },
  });
  const agent = createVoiceAgent({ tools, client: fakeClient });
  const r = await agent.turn('what is on the kanban', { where: 'workbench', now: new Date('2026-11-10T09:00:00Z') });
  ok(r.ok && r.speak === 'One project, Master, and the board is empty.' && r.show === 'proj: no tasks', 'a turn with two tool calls answers');
  ok(r.tools.join() === 'list_projects,get_kanban', 'both tools ran (in parallel, one round)');
  const h = agent.history;
  ok(h.length === 4 && h[1].content[0].type === 'thinking' && h[1].content[0].signature === 'sig', 'assistant content appended whole, thinking unchanged');
  ok(h[2].content.length === 2 && h[2].content.every(b => b.type === 'tool_result'), 'all tool results go back in one message');
  ok(calls[0].model === 'claude-haiku-5-5' && calls[0].output_config.effort === 'low', 'Haiku, low effort');
  ok(calls[0].system[0].cache_control.ttl === '1h' && calls[0].cache_control.ttl === '1h', '1-hour breakpoint on system + tools, 1-hour automatic breakpoint on the conversation');
  ok(calls[0].system.length === 1 && calls[0].system[0].text === SYSTEM_PROMPT, 'the system prompt is one fixed block');
  ok(calls[1].messages.length === 3 && JSON.stringify(calls[1].messages.slice(0, 1)) === JSON.stringify(calls[0].messages), 'round two starts with round one unchanged (prefix match)');
  ok(!('tool_choice' in calls[0]) && !('temperature' in calls[0]), 'no forced tool choice, no sampling parameters');
  ok(calls[0].system === calls[1].system && JSON.stringify(calls[0].tools) === JSON.stringify(calls[1].tools), 'system and tools identical across rounds (cache-stable)');
  ok(/workbench/.test(h[0].content) && /what is on the kanban/.test(h[0].content), 'context rides in the user message, not the system prompt');
  ok(r.costUsd > 0, 'cost reported');
  ok(r.cache && typeof r.cache.read === 'number' && typeof r.cache.written === 'number', 'cache use reported');

  // warm(): re-sends the last request with max_tokens 0, unless recent
  const n0 = calls.length;
  const w1 = await agent.warm();
  ok(w1.skipped === 'already warm' && calls.length === n0, 'warm() skips a cache that was just used');
  const fresh = [];
  const warmClient = () => ({ messages: { create: async (req) => { fresh.push(req); return { stop_reason: 'max_tokens', usage: { cache_creation_input_tokens: 900 }, content: [] }; } } });
  const a2 = createVoiceAgent({ tools, client: warmClient });
  const w2 = await a2.warm();
  ok(w2.ok && fresh[0].max_tokens === 0 && fresh[0].messages.length === 1 && fresh[0].system[0].cache_control.ttl === '1h', 'warm() with no conversation writes the system + tools prefix');
  ok(!('stream' in fresh[0]) && fresh[0].output_config.effort === 'low', 'warm() matches the real requests (no stream, same effort)');

  vs.updateSettings({ model: 'claude-opus-5-5' });
  await agent.turn('hello');
  const last = calls[calls.length - 1];
  ok(last.beta && last.fallbacks === 'default' && last.betas.includes('server-side-fallback-2026-07-01'), 'Opus uses the server-side refusal fallback');
  vs.updateSettings({ model: 'claude-haiku-5-5' });

  const refuser = () => ({ messages: { create: async () => ({ stop_reason: 'refusal', usage: {}, content: [] }) } });
  const ra = await createVoiceAgent({ tools, client: refuser }).turn('x');
  ok(ra.ok && /can't help/.test(ra.speak), 'a refusal is spoken politely');

  vs.updateSettings({ monthlyLimitUsd: 0 });
  const lim = await createVoiceAgent({ tools, client: fakeClient }).turn('x');
  ok(!lim.ok && lim.limitReached, 'nothing is sent once the limit is reached');
  vs.updateSettings({ monthlyLimitUsd: 50, anthropicKey: '' });
  const nokey = await createVoiceAgent({ tools, client: fakeClient }).turn('x');
  ok(!nokey.ok && /No Claude API key/.test(nokey.error), 'no key, a clear message');
}

// ─── access ───
{
  const collab = { role: ROLE_COLLABORATOR, projects: ['proj'] };
  ok(!mayAccess(collab, 'GET', '/api/voice/settings') && !mayAccess(collab, 'POST', '/api/voice/turn'), 'collaborators cannot reach the voice assistant');
  ok(mayAccess({ role: ROLE_OWNER }, 'POST', '/api/voice/turn'), 'the owner can');
}

rmSync(dir, { recursive: true, force: true });
console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
