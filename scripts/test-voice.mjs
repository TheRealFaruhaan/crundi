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

import { mkdtempSync, rmSync, mkdirSync, writeFileSync, readFileSync, symlinkSync } from 'node:fs';
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
const vstream = await import('../src/voice/stream.js');
const vtts = await import('../src/voice/tts.js');
const { EventEmitter } = await import('node:events');

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
  ok(Math.abs(vs.claudeCost('claude-haiku-5-5', { cache_creation_input_tokens: 1e6 }) - 0.20) < 1e-9, '1-hour cache writes cost twice the input price');
  ok(Math.abs(vs.claudeCost('claude-haiku-5-5', { cache_creation_input_tokens: 2e6, cache_creation: { ephemeral_5m_input_tokens: 1e6, ephemeral_1h_input_tokens: 1e6 } }) - 0.325) < 1e-9, '5-minute writes are priced at 1.25x when reported');

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
  ok(/needsConfirmation/.test(SYSTEM_PROMPT) && /never call confirm_action in the same turn/i.test(SYSTEM_PROMPT), 'system prompt: ask first, never confirm for Master');
  ok(/never type into a terminal/.test(SYSTEM_PROMPT), 'system prompt: never a terminal (D21)');
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


// ─── live streaming ───
{
  const u = new URL(vstream.streamUrl(['kanban']));
  ok(u.protocol === 'wss:' && u.searchParams.get('interim_results') === 'true' && u.searchParams.get('utterance_end_ms') && u.searchParams.get('endpointing'), 'stream URL: live words, end of speech detected');
  ok(u.searchParams.getAll('keyterm').includes('kanban'), 'stream URL carries keyterms');
  ok(u.searchParams.get('utterance_end_ms') === '2000', 'by default, two seconds of quiet end what Master says');
  ok(new URL(vstream.streamUrl([], { pauseMs: 3500 })).searchParams.get('utterance_end_ms') === '3500' && new URL(vstream.streamUrl([], { pauseMs: 99999 })).searchParams.get('utterance_end_ms') === '5000', 'the pause is Master\'s setting, within 1 to 5 s');
  ok(!vs.updateSettings({ pauseMs: 500 }).ok && vs.updateSettings({ pauseMs: 2500 }).ok && vs.publicSettings().pauseMs === 2500, 'the pause before sending is a setting');
  vs.updateSettings({ pauseMs: 2000 });

  const a = vstream.createAssembler();
  const R = (text, f = {}) => ({ type: 'Results', start: f.start || 0, duration: f.duration || 1, is_final: !!f.final, speech_final: !!f.sf, channel: { alternatives: [{ transcript: text }] } });
  ok(a.push({ type: 'SpeechStarted' }).type === 'speech', 'speech started is passed on');
  ok(a.push(R('add the')).text === 'add the', 'interim words shown as they come');
  ok(a.push(R('add the UI overhaul', { final: true })).text === 'add the UI overhaul', 'a finished phrase is kept');
  ok(a.push(R('of the home', {})).text === 'add the UI overhaul of the home', 'later words added to it');
  const mid = a.push(R('of the home screen', { final: true, sf: true, start: 2, duration: 1.5 }));
  ok(mid.type === 'interim' && mid.text === 'add the UI overhaul of the home screen', 'a short pause (speech_final) does not send: Master may be pausing mid-sentence');
  a.push(R('as a kanban task', { final: true, sf: true, start: 3.5, duration: 1 }));
  const done = a.push({ type: 'UtteranceEnd' });
  ok(done.type === 'utterance' && done.text === 'add the UI overhaul of the home screen as a kanban task', 'the utterance completes at UtteranceEnd, the pause Master chose');
  ok(a.push({ type: 'UtteranceEnd' }) === null, 'nothing left to send twice');
  a.push(R('and one more', { final: true }));
  ok(a.push({ type: 'UtteranceEnd' }).text === 'and one more', 'UtteranceEnd completes one too');
  ok(a.seconds === 4.5, 'audio seconds tracked for the spend');

  class FakeWs extends EventEmitter {
    constructor(url, opts) { super(); this.url = url; this.opts = opts; this.sent = []; this.readyState = 0; FakeWs.last = this; }
    send(d) { this.sent.push(d); }
    close() { this.readyState = 3; this.emit('close'); }
  }
  const client = new EventEmitter();
  client.readyState = 1; client.out = [];
  client.send = (m) => client.out.push(JSON.parse(m));
  client.close = () => { client.readyState = 3; };
  vs.updateSettings({ deepgramKey: 'dg-secret' });
  const spentBefore = vs.publicSettings().spend.deepgramUsd;
  vstream.relay(client, { projects: ['crundi'], WebSocketImpl: FakeWs });
  const dg = FakeWs.last;
  ok(dg.opts.headers.Authorization === 'Token dg-secret', 'the key goes to Deepgram from the server');
  client.emit('message', Buffer.from('a1'), true);
  ok(dg.sent.length === 0, 'audio before Deepgram is ready is held');
  dg.readyState = 1; dg.emit('open');
  ok(dg.sent.length === 1 && client.out.some(m => m.type === 'ready'), 'then sent, and the client told it is listening');
  client.emit('message', JSON.stringify({ type: 'pause' }), false);
  client.emit('message', Buffer.from('a2'), true);
  ok(dg.sent.length === 1, 'paused while Crundi talks: no audio forwarded');
  client.emit('message', JSON.stringify({ type: 'resume' }), false);
  client.emit('message', Buffer.from('a3'), true);
  ok(dg.sent.length === 2, 'resumed');
  dg.emit('message', JSON.stringify(R('how much usage do I have left', { final: true, sf: true, duration: 3 })));
  dg.emit('message', JSON.stringify({ type: 'UtteranceEnd' }));
  ok(client.out.some(m => m.type === 'utterance' && m.text === 'how much usage do I have left'), 'the utterance reaches the client');
  client.emit('message', JSON.stringify({ type: 'stop' }), false);
  ok(dg.sent.some(d => typeof d === 'string' && JSON.parse(d).type === 'CloseStream'), 'stop closes the Deepgram stream');
  ok(vs.publicSettings().spend.deepgramUsd > spentBefore, 'the streamed seconds are added to the spend');

  vs.updateSettings({ deepgramKey: '' });
  const c2 = new EventEmitter(); c2.readyState = 1; c2.out = []; c2.send = (m) => c2.out.push(JSON.parse(m)); c2.close = () => {};
  vstream.relay(c2, { WebSocketImpl: FakeWs });
  ok(c2.out[0]?.type === 'error' && /No Deepgram key/.test(c2.out[0].error), 'no key: a clear error, no connection');
  vs.updateSettings({ deepgramKey: 'dg-secret' });
}

// ─── Deepgram voices ───
{
  const list = vtts.parseVoices({ tts: [
    { name: 'thalia', canonical_name: 'aura-2-thalia-en', languages: ['en', 'en-US'], metadata: { accent: 'American', tags: ['feminine'] } },
    { name: 'draco', canonical_name: 'aura-2-draco-en', languages: ['en', 'en-GB'], metadata: { accent: 'British', tags: ['masculine', 'warm'], sample: 'https://x/s.mp3' } },
    { name: 'helios', canonical_name: 'aura-helios-en', languages: ['en'], metadata: { accent: 'British', tags: ['masculine'] } },
    { name: 'nestor', canonical_name: 'aura-2-nestor-es', languages: ['es'], metadata: { accent: 'Spanish' } },
    { name: 'bad', canonical_name: '../../etc', languages: ['en'] },
  ] });
  ok(list.map(v => v.model).join() === 'aura-2-draco-en,aura-2-thalia-en,aura-helios-en', 'English only, Aura-2 first, British first');
  ok(list[0].gender === 'male' && list[0].accent === 'British' && list[0].name === 'Draco', 'accent and gender read from Deepgram');
  ok(vtts.isVoiceModel('aura-2-draco-en') && !vtts.isVoiceModel('aura-2-draco-en&x=1') && !vtts.isVoiceModel(''), 'voice names are checked');
  ok(!vs.updateSettings({ ttsVoice: 'evil?x' }).ok && vs.updateSettings({ ttsVoice: 'aura-2-draco-en' }).ok, 'only a real voice name can be chosen');
  ok(vs.publicSettings().ttsVoice === 'aura-2-draco-en', 'the chosen voice is a setting');

  vtts._clearCache();
  const fromDg = await vtts.listVoices({ fetchImpl: async () => ({ ok: true, json: async () => ({ tts: [{ canonical_name: 'aura-2-draco-en', languages: ['en'] }] }) }) });
  ok(fromDg.ok && fromDg.fromDeepgram && fromDg.voices.length === 1, 'the list comes from Deepgram');
  vtts._clearCache();
  const down = await vtts.listVoices({ fetchImpl: async () => { throw new Error('offline'); } });
  ok(down.ok && !down.fromDeepgram && down.voices.length > 5, 'Deepgram unreachable: known voices offered');

  let req = null;
  const audio = Buffer.from('ID3fake');
  const spoke = await vtts.speak('Yes, Master.', '', { fetchImpl: async (url, init) => { req = { url, init }; return { ok: true, headers: { get: () => 'audio/mpeg' }, arrayBuffer: async () => audio }; } });
  ok(spoke.ok && spoke.audio.equals(audio) && spoke.mime === 'audio/mpeg', 'speech comes back as audio');
  ok(new URL(req.url).searchParams.get('model') === 'aura-2-draco-en' && JSON.parse(req.init.body).text === 'Yes, Master.', 'the chosen voice is used');
  ok(!(await vtts.speak('hi', 'not-a-voice', { fetchImpl: async () => ({ ok: true }) })).ok, 'a bad voice name is never sent');
  vs.updateSettings({ ttsVoice: '' });
}

// ─── found against the real services (2026-11-05) ───
{
  const { compactBoard, makeTools: mk } = await import('../src/voice/tools.js');
  const board = {
    statuses: ['backlog', 'todo', 'in_progress', 'done'],
    tasks: [
      { id: 'a', title: 'One', status: 'todo', description: 'x'.repeat(50000), todos: [{ done: true }, { done: false }, { done: true, deleted: true }] },
      { id: 'b', title: 'Two', status: 'done', todos: [] },
      { id: 'c', title: 'Gone', status: 'todo', deleted: true },
    ],
    history: [{ big: 'y'.repeat(50000) }],
  };
  const c = compactBoard(board);
  ok(c.counts.total === 2 && c.counts.todo === 1 && c.counts.done === 1, 'a board is counted per column, deleted tasks left out');
  ok(c.tasks[0].todosDone === 1 && c.tasks[0].todosTotal === 2 && !('description' in c.tasks[0]), 'tasks are one short line each');
  ok(JSON.stringify(c).length < 1000, 'a busy board fits a tool result (no history, no descriptions)');
  const kt = mk({
    listProjects: () => [{ alias: 'p1', path: '/' }, { alias: 'p2', path: '/' }, { alias: 'p3', path: '/' }],
    kanban: { getBoard: (a) => a === 'p1' ? board : a === 'p2' ? { tasks: [{ id: 'z', status: 'todo' }] } : { tasks: [] }, getColumn: () => ({}), getTask: () => ({}) },
  }).find(t => t.name === 'get_kanban');
  const all = await kt.run({});
  ok(all.length === 2 && all[0].project === 'p1' && all[0].total === 2 && all[1].total === 1, 'no project: every project\'s counts, busiest first, empty boards left out');

  ok(vstream.rawAudio({}) === null, 'no declared format: a container Deepgram reads itself');
  ok(JSON.stringify(vstream.rawAudio({ encoding: 'linear16', sample_rate: '16000' })) === '{"encoding":"linear16","sample_rate":16000}', 'the phone\'s 16 kHz PCM is accepted');
  ok(vstream.rawAudio({ encoding: 'linear16&x=1', sample_rate: '16000' }) === null && vstream.rawAudio({ encoding: 'linear16', sample_rate: '12345' }) === null, 'only known formats reach Deepgram');
  const pu = new URL(vstream.streamUrl(['kanban'], { audio: { encoding: 'linear16', sample_rate: 16000 } }));
  ok(pu.searchParams.get('encoding') === 'linear16' && pu.searchParams.get('sample_rate') === '16000' && pu.searchParams.get('channels') === '1', 'raw PCM is described in the stream URL');
  ok(!new URL(vstream.streamUrl(['kanban'])).searchParams.has('encoding'), 'WebM is sent without an encoding');
}

// ─── the locked phone ───
{
  const { contextLine } = await import('../src/voice/agent.js');
  const at = new Date('2026-11-10T09:00:00Z');
  ok(/phone is locked/.test(contextLine({ now: at, locked: true })) && !/locked/.test(contextLine({ now: at })), 'a locked phone is said in the context line only when locked');
  ok(/phone is locked/.test(SYSTEM_PROMPT) && !/phone is locked: answer/.test(SYSTEM_PROMPT), 'the system prompt explains it once, without the changing part');
  const seen = [];
  const lockedAgent = createVoiceAgent({
    tools: [],
    settings: { DEFAULT_MODEL: 'claude-haiku-5-5', getSettings: () => ({ anthropicKey: 'k', model: 'claude-haiku-5-5', monthlyLimitUsd: 50 }), overLimit: () => false, countTurn: () => {}, claudeCost: () => 0, addClaudeSpend: () => {} },
    client: () => ({ messages: { create: async (req) => { seen.push(JSON.parse(JSON.stringify(req))); return { stop_reason: 'end_turn', content: [{ type: 'text', text: 'Unlock first, Master.' }], usage: {} }; } } }),
  });
  await lockedAgent.turn('add a kanban task', { locked: true, now: at });
  ok(/phone is locked/.test(seen[0].messages[0].content) && seen[0].system[0].text === SYSTEM_PROMPT, 'locked rides in the user message; the system block is unchanged');
}


// ─── billing month ───
{
  const p = (iso, day) => { const r = vs.billingPeriod(new Date(iso), day); return r.start.toISOString().slice(0, 10) + '>' + r.next.toISOString().slice(0, 10); };
  ok(p('2026-11-05T12:00:00Z', 1) === '2026-11-01>2026-12-01', 'billing day 1 is the calendar month');
  ok(p('2026-11-05T12:00:00Z', 20) === '2026-10-20>2026-11-20', 'before the billing day: the month began last month');
  ok(p('2026-11-20T00:00:00Z', 20) === '2026-11-20>2026-12-20', 'on the billing day a new month begins');
  ok(p('2026-02-15T00:00:00Z', 31) === '2026-01-31>2026-02-28', 'a day the month lacks falls on its last day');
  ok(p('2026-03-01T00:00:00Z', 31) === '2026-02-28>2026-03-31', 'and the next month uses the 31st again');
  ok(!vs.updateSettings({ billingDay: 0 }).ok && !vs.updateSettings({ billingDay: 32 }).ok && !vs.updateSettings({ billingDay: 'x' }).ok, 'only 1 to 31 is a billing day');
  ok(vs.updateSettings({ billingDay: 1 }).ok, 'billing day saved');
  const ps = vs.publicSettings(new Date('2026-11-05T12:00:00Z'));
  ok(ps.billingDay === 1 && ps.periodStart === '2026-11-01' && ps.nextReset === '2026-12-01', 'clients see the billing month and when it restarts');

  // A file from before billing months: spend counted for "2026-11".
  const f = join(process.env.DATA_DIR, 'voice.json');
  const saved = JSON.parse(readFileSync(f, 'utf-8'));
  writeFileSync(f, JSON.stringify({ ...saved, billingDay: 1, spend: { month: '2026-11', claudeUsd: 1.5, deepgramUsd: 0.5, turns: 9 } }));
  vs._reload();
  ok(vs.publicSettings(new Date('2026-11-05T12:00:00Z')).spend.totalUsd === 2, 'spend counted before billing months carries over');
  vs.updateSettings({ billingDay: 20 });
  const split = vs.publicSettings(new Date('2026-11-25T12:00:00Z'));
  ok(split.spend.claudeUsd === 0 && split.spend.deepgramUsd === 0.5, 'Claude restarts on its own billing day; Deepgram keeps its month');
  ok(split.nextReset === '2026-12-20' && split.deepgramNextReset === '2026-12-01', 'each provider says when it restarts');
  vs.updateSettings({ billingDay: 1, deepgramBillingDay: 1 });
}

// ─── streamed speech ───
{
  vs.updateSettings({ deepgramKey: 'dg-test', ttsVoice: 'aura-2-draco-en' });
  let asked = null;
  const body = new ReadableStream({ start(c) { c.enqueue(new Uint8Array([1, 0, 2, 0])); c.close(); } });
  const r = await vtts.speakStream('Yes, Master.', '', { fetchImpl: async (url) => { asked = new URL(url); return { ok: true, body }; } });
  ok(r.ok && r.rate === 24000 && r.body === body, 'speech is streamed as raw PCM');
  ok(asked.searchParams.get('encoding') === 'linear16' && asked.searchParams.get('container') === 'none' && asked.searchParams.get('sample_rate') === '24000', 'Deepgram is asked for raw 16-bit PCM');
  ok(!(await vtts.speakStream('hi', 'not-a-voice', { fetchImpl: async () => ({ ok: true }) })).ok, 'a bad voice name is never streamed');
  const { CAPABILITIES } = await import('../src/client-version.js');
  ok(CAPABILITIES.includes('voice-tts-stream') && CAPABILITIES.includes('voice-assistant'), 'clients can tell the server streams speech');
  vs.updateSettings({ deepgramKey: '', ttsVoice: '' });
}

// ─── the account's own usage (Anthropic Admin API) ───
{
  const au = await import('../src/voice/account-usage.js');
  ok(au.sumCost({ data: [{ results: [{ amount: '123.78912', currency: 'USD' }, { amount: '76.21088', currency: 'USD' }] }, { results: [] }] }) === 2, 'the cost report is summed in dollars (amounts are cents)');
  au._clearCache();
  ok((await au.accountUsage({ fetchImpl: async () => { throw new Error('no'); } })).needsKey, 'without an Admin key: says one is needed, asks nothing');
  vs.updateSettings({ anthropicAdminKey: 'sk-ant-admin-test', billingDay: 11 });
  ok(!JSON.stringify(vs.publicSettings()).includes('sk-ant-admin-test') && vs.publicSettings().anthropicAdminKeySet, 'the Admin key never reaches a client');
  const calls = [];
  const pages = [{ data: [{ results: [{ amount: '1000' }] }], has_more: true, next_page: 'p2' }, { data: [{ results: [{ amount: '250' }] }], has_more: false }];
  const r = await au.accountUsage({ now: new Date('2026-11-05T12:00:00Z'), fetchImpl: async (url, init) => { calls.push({ url: new URL(url), init }); return { ok: true, json: async () => pages[calls.length - 1] }; } });
  ok(r.ok && r.totalUsd === 12.5 && calls.length === 2 && calls[1].url.searchParams.get('page') === 'p2', 'every page is read and added up');
  ok(calls[0].url.searchParams.get('starting_at') === '2026-10-11T00:00:00.000Z' && calls[0].init.headers['x-api-key'] === 'sk-ant-admin-test', 'from the Claude billing day, with the Admin key');
  au._clearCache();
  const bad = await au.accountUsage({ fetchImpl: async () => ({ ok: false, status: 401, json: async () => ({ error: { message: 'invalid x-api-key' } }) }) });
  ok(!bad.ok && /Admin API key/.test(bad.error), 'an ordinary key is refused with a clear reason');
  vs.updateSettings({ anthropicAdminKey: '', billingDay: 1 });
}

// ─── actions (Phase 4): the gate, the confirmation rules ───
{
  const A = await import('../src/voice/actions.js');
  let t = 1000;
  const pending = A.createPending({ now: () => t });
  let locked = false;
  const sent = [], closed = [], added = [];
  const deps = {
    chats: () => [{ id: 'c1', title: 'API work', project: 'proj', status: 'idle' }],
    sendMessage: (id, text) => { sent.push({ id, text }); return { ok: true }; },
    createChat: async () => ({ ok: true, id: 'c2' }),
    closeChat: async (id) => { closed.push(id); return { ok: true }; },
    parkChat: () => ({ ok: true }), resumeChat: async () => ({ ok: true }), watch: () => {},
    listProjects: () => [{ alias: 'proj', name: 'Proj', path: '/tmp/proj' }],
    projectsDir: () => '/tmp/projects', registerProject: () => ({ ok: true }),
    kanban: { addTask: (a, d) => { added.push(d); return { ok: true }; }, moveTask: () => ({ ok: true }), getTask: () => ({ ok: true, task: { title: 'Card' } }), deleteTask: () => ({ ok: true }), updateTask: () => ({ ok: true }), addTodo: () => ({ ok: true }), updateTodo: () => ({ ok: true }) },
    notes: { createPage: () => ({ ok: true }), getPage: () => null, savePage: () => ({ ok: true }), deletePage: () => ({ ok: true }) },
    mindmap: { addNode: () => ({ ok: true }), updateNode: () => ({ ok: true }), getNode: () => ({ text: 'n' }), deleteNode: () => ({ ok: true }) },
    chatSchedule: { list: () => [], add: () => ({ ok: true }), remove: () => ({ ok: true }) },
    schedules: { addSchedule: (d) => ({ ok: true, d }), getSchedule: () => ({ id: 's1', name: 'Nightly', when: {}, action: { kind: 'chat' } }), updateSchedule: () => ({ ok: true }), setEnabled: () => ({ ok: true }), deleteSchedule: () => ({ ok: true }) },
    services: () => [{ key: 'web', name: 'web' }],
    voiceSettings: { updateSettings: (p) => ({ ok: true, p }) },
  };
  const tools = A.makeActionTools(deps, { pending, locked: () => locked });
  const T = Object.fromEntries(tools.map(x => [x.name, x]));
  ok(JSON.stringify(tools.map(x => x.name)) === JSON.stringify(A.ACTION_TOOL_NAMES), 'the action tools are exactly the pinned list');
  ok(!tools.some(x => /terminal|secret|key|password|tunnel|collaborator|backup|update_server|skill/i.test(x.name)), 'no action reaches terminals, secrets, keys or critical settings (D17, D21)');

  pending.newTurn();
  const ex = await T.send_to_chat.run({ chat: 'API work', text: 'run the tests', intent: 'explicit' });
  ok(ex.ok && sent.length === 1 && sent[0].id === 'c1', 'Master clearly asked: sent at once (D15)');
  const inf = await T.send_to_chat.run({ chat: 'API work', text: 'maybe refactor the auth middleware', intent: 'inferred' });
  ok(inf.needsConfirmation && sent.length === 1 && /Send "maybe refactor/.test(inf.ask), 'the model decided to send it: read back first, nothing sent (D15)');
  const long = A.readBack('word '.repeat(200));
  ok(/about 200 words/.test(long) && long.length < 160, 'a long message is read back as one line');
  const tooSoon = await T.confirm_action.run({ id: inf.id });
  ok(!tooSoon.ok && /not answered/.test(tooSoon.error) && sent.length === 1, 'the model cannot confirm in the turn it asked');
  pending.newTurn();
  const yes = await T.confirm_action.run({ id: inf.id });
  ok(yes.ok && sent.length === 2, 'after Master answers, the confirmed action runs');
  ok(!(await T.confirm_action.run({ id: inf.id })).ok, 'and only once');

  const close = await T.close_chat.run({ chat: 'c1' });
  ok(close.needsConfirmation && closed.length === 0 && /It is idle/.test(close.ask), 'closing a chat always asks first (D24)');
  ok((await T.park_chat.run({ chat: 'c1' })).needsConfirmation, 'parking asks first (D31)');
  ok((await T.create_project.run({ alias: 'new-thing' })).needsConfirmation, 'creating a project asks first (D22)');
  ok((await T.set_voice_setting.run({ setting: 'monthlyLimitUsd', value: '80' })).needsConfirmation, 'a setting change asks first (D32)');
  let bad = null; try { await T.set_voice_setting.run({ setting: 'anthropicKey', value: 'x' }); } catch (e) { bad = e; }
  ok(bad && /not one voice may change/.test(bad.message), 'keys are not settings voice may change (D17)');
  ok((await T.add_schedule.run({ project: 'proj', name: 'n', kind: 'chat', prompt: 'p', mode: 'recurring', time: '09:00' })).needsConfirmation, 'creating a schedule asks first (D45)');
  const bl = await T.set_background_listening.run({});
  ok(bl.needsConfirmation && /30 minutes/.test(bl.ask), 'background listening asks first, 30 minutes when no time is named (D6, D32)');
  pending.newTurn();
  const blDone = await T.confirm_action.run({ id: bl.id });
  ok(blDone.phoneAction?.type === 'background-listening' && blDone.phoneAction.minutes === 30, 'and then tells the phone to listen');
  ok(!A.LISTEN_MINUTES.includes(0) && Math.max(...A.LISTEN_MINUTES) === 120, 'never unlimited by voice; two hours at most (D5, D6)');
  ok((await T.delete_kanban_task.run({ project: 'proj', taskId: 't1' })).needsConfirmation, 'deleting a card asks first');
  ok((await T.move_kanban_task.run({ project: 'proj', taskId: 't1', status: 'todo' })).needsConfirmation, 'moving a card to To do (a factory run) asks first');
  ok((await T.move_kanban_task.run({ project: 'proj', taskId: 't1', status: 'done' })).ok, 'other moves just happen');
  await T.add_kanban_task.run({ project: 'proj', title: 'New card' });
  ok(added.at(-1).status === 'backlog', 'a new card goes to Backlog, never To do');

  locked = true;
  const lk = await T.send_to_chat.run({ chat: 'c1', text: 'hi', intent: 'explicit' });
  ok(lk.needsUnlock && sent.length === 2, 'a locked phone changes nothing and asks to unlock');
  const lr = await T.list_chat_schedules.run({ project: 'proj' });
  ok(Array.isArray(lr), 'reading still works while locked');
  locked = false;

  pending.newTurn();
  const old = pending.propose('send_to_chat', {}, 'q');
  t += 11 * 60 * 1000; pending.newTurn();
  ok(!!pending.take(old).error, 'an unanswered question expires after ten minutes');

  // The agent passes the question to the client, so the phone can show Yes / No.
  const { createVoiceAgent: mk } = await import('../src/voice/agent.js');
  const fakeSettings = { DEFAULT_MODEL: 'claude-haiku-5-5', getSettings: () => ({ anthropicKey: 'k' }), overLimit: () => false, countTurn() {}, claudeCost: () => 0, addClaudeSpend() {} };
  let round = 0;
  const agent2 = mk({
    tools, settings: fakeSettings, beforeTurn: () => pending.newTurn(),
    client: () => ({ messages: { create: async () => (round++ === 0
      ? { stop_reason: 'tool_use', content: [{ type: 'tool_use', id: 'x1', name: 'close_chat', input: { chat: 'c1' } }], usage: {} }
      : { stop_reason: 'end_turn', content: [{ type: 'text', text: 'Close the API chat, Master? It is idle.' }], usage: {} }) } }),
  });
  const rep = await agent2.turn('close the API chat');
  ok(rep.ok && rep.pending.length === 1 && /Close the "API work" chat/.test(rep.pending[0].question) && closed.length === 0, 'the reply carries the question awaiting a yes, and nothing was closed');
}

// ─── memory (Phase 2) ───
{
  const { createNotebook } = await import('../src/voice/notebook.js');
  const { createMemoryIndex, createEmbedder, fuse, ftsQuery } = await import('../src/voice/memory-index.js');
  const { createVoiceMemory } = await import('../src/voice/memory.js');
  const mdir = join(dir, 'mem');
  const changes = [];
  const nb = createNotebook(join(mdir, 'memories'), { onChange: (p, t) => changes.push([p, t]) });
  const run = (i) => nb.tool.run(i);
  ok(run({ command: 'create', path: '/memories/master.md', file_text: 'Master prefers short answers.\nThe API project is called crundi.' }).startsWith('File created'), 'the notebook keeps a file');
  ok(/prefers short answers/.test(run({ command: 'view', path: '/memories/master.md' })), 'and reads it back');
  run({ command: 'str_replace', path: '/memories/master.md', old_str: 'short answers', new_str: 'very short answers' });
  ok(nb.list()[0].text.includes('very short answers') && changes.length === 2, 'edits are saved and reported for the index');
  let e1 = null; try { run({ command: 'view', path: '/memories/../../etc/passwd' }); } catch (e) { e1 = e; }
  ok(e1 && /outside/.test(e1.message), 'paths cannot leave /memories');
  let e2 = null; try { run({ command: 'create', path: '/memories/k.md', file_text: 'key: sk-ant-api03-abcdefghijklmnop' }); } catch (e) { e2 = e; }
  ok(e2 && /Secrets are never kept/.test(e2.message), 'a key is refused: secrets never go in memory (D16)');
  ok(nb.tool.def.type === 'memory_20250818', 'it is Anthropic\'s memory tool');

  ok(ftsQuery('the kanban card, please!') === '"the"* OR "kanban"* OR "card"* OR "please"*', 'free text becomes a keyword query');
  ok(JSON.stringify(fuse([[1, 2, 3], [3, 1]])) === '[1,3,2]', 'keyword and meaning rankings are merged (RRF)');

  // A stand-in embedder: words become a bag-of-topics vector, so "page" finds "screen".
  const topics = { screen: 0, page: 0, home: 0, ui: 0, card: 1, task: 1, kanban: 1, weather: 2, london: 2 };
  const fakeEmb = createEmbedder({ load: async () => async (texts) => texts.map(t => {
    const v = new Array(384).fill(0);
    for (const w of t.toLowerCase().split(/\W+/)) if (w in topics) v[topics[w]] += 1;
    const n = Math.hypot(...v) || 1; return v.map(x => x / n);
  }) });
  const ix = createMemoryIndex({ dir: mdir, embedder: fakeEmb });
  ix.add('turn', 'Master: add the UI overhaul of the home screen as a kanban task\nCrundi: Done, in the backlog.', { project: 'crundi' });
  ix.add('turn', 'Master: how is the weather in London', {});
  ix.add('summary', 'We moved three cards to done in dhivo.', { project: 'dhivo' });
  await ix.settle();
  const byWord = await ix.search('kanban');
  ok(byWord.results[0]?.text.includes('home screen'), 'found by keyword');
  const byMeaning = await ix.search('redesign of the main page');
  ok(byMeaning.meaning && byMeaning.results[0]?.text.includes('home screen'), 'found by meaning, with none of the same words');
  const scoped = await ix.search('cards', { project: 'crundi' });
  ok(!scoped.results.some(r => r.project === 'dhivo'), 'a project search leaves other projects out');
  const noModel = createMemoryIndex({ dir: join(dir, 'mem2'), embedder: createEmbedder({ load: async () => { throw new Error('no model'); } }) });
  noModel.add('turn', 'the kanban card'); await noModel.settle();
  const kw = await noModel.search('kanban');
  ok(kw.results.length === 1 && !kw.meaning, 'without the model, search is keyword only and still works');

  // The conversation is kept, and compaction starts a new one from a summary.
  const vm = createVoiceMemory({ dataDir: join(dir, 'vm'), index: ix });
  const { createVoiceAgent: mkA } = await import('../src/voice/agent.js');
  const fakeS = { DEFAULT_MODEL: 'claude-haiku-5-5', getSettings: () => ({ anthropicKey: 'k' }), overLimit: () => false, countTurn() {}, claudeCost: () => 0, addClaudeSpend() {} };
  let summarised = '';
  const fakeClient = () => ({ messages: { create: async (req) => req.tools
    ? { stop_reason: 'end_turn', content: [{ type: 'text', text: 'Yes, Master.' }], usage: {} }
    : { stop_reason: 'end_turn', content: [{ type: 'text', text: 'Master asked about usage several times; all fine.' }], usage: {} } } });
  const a1 = mkA({ tools: [vm.notebook.tool, vm.searchTool], settings: fakeS, client: fakeClient, store: vm.store, onTurnDone: (t) => vm.remember(t), onCompacted: (sum) => { summarised = sum; vm.rememberSummary(sum); } });
  for (let i = 0; i < 9; i++) await a1.turn('how much usage is left ' + i);
  const a2 = mkA({ tools: [], settings: fakeS, client: fakeClient, store: vm.store });
  ok(a2.history.length === 18, 'the conversation survives a restart (kept on disk)');
  const c = await a1.compact();
  ok(c.ok && a1.history.length === 2 && /summarised/.test(a1.history[0].content) && a1.history[1].role === 'assistant', 'compaction starts a new conversation from the summary');
  ok(summarised && (await ix.search('usage several times')).results.some(r => r.kind === 'summary'), 'the summary is searchable');
  ok((await ix.search('usage is left')).results.some(r => r.kind === 'turn'), 'and so are the turns before it');
  ok(a1.toolNames.includes('memory') && a1.toolNames.includes('memory_search'), 'the notebook and search are tools');
  // Windows cannot delete an open database: close them before the temp folder goes.
  ix.close(); noModel.close();
}

// ─── access ───
{
  const collab = { role: ROLE_COLLABORATOR, projects: ['proj'] };
  ok(!mayAccess(collab, 'GET', '/api/voice/settings') && !mayAccess(collab, 'POST', '/api/voice/turn'), 'collaborators cannot reach the voice assistant');
  ok(mayAccess({ role: ROLE_OWNER }, 'POST', '/api/voice/turn'), 'the owner can');
}

try { rmSync(dir, { recursive: true, force: true }); } catch { /* a locked temp file is not a failed test */ }
console.log(failed ? `\n${failed} FAILED` : '\nAll passed');
process.exit(failed ? 1 : 0);
