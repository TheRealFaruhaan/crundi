#!/usr/bin/env node
/**
 * test-voice-cli.mjs — the Claude Code brain (src/voice/agent-cli.js), the
 * brain switch and its log (src/voice/brain.js), with a fake `claude`.
 *
 *   spawn      locked down: no built-in tools, only the cru server, none of
 *              the owner's settings, nothing that needs a permission; runs
 *              in Cru's notes folder; the subscription pays (no API key)
 *   turns      one long-lived process; the persona leads a new conversation;
 *              the reply is split; tool calls go through callTool with the
 *              same signals (confirmations, close) as the API brain
 *   session    the id is kept and resumed after a restart
 *   compact    a summary starts the next conversation
 *   idle       the process stops after the idle time in Settings
 *   brain      the setting picks the brain; every turn is logged; stats
 *
 * Run: node scripts/test-voice-cli.mjs
 */

import { mkdtempSync, mkdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';

const dir = mkdtempSync(join(tmpdir(), 'voice-cli-test-'));
process.env.DATA_DIR = join(dir, 'data');
mkdirSync(process.env.DATA_DIR, { recursive: true });

const vs = await import('../src/voice/settings.js');
const { createCliVoiceAgent, CLI_PROMPT } = await import('../src/voice/agent-cli.js');
const { createVoiceBrain } = await import('../src/voice/brain.js');

let failed = 0;
const ok = (c, l) => { console.log((c ? 'PASS  ' : 'FAIL  ') + l); if (!c) failed++; };

/** A fake claude: records what it was started with and what it was sent; answers from a script. */
function fakeClaude(answer) {
  const procs = [];
  const spawn = (bin, args, opts) => {
    const p = new EventEmitter();
    const n = procs.length;
    p.bin = bin; p.args = args; p.opts = opts; p.sent = []; p.exitCode = null;
    p.stdout = new PassThrough(); p.stderr = new PassThrough();
    p.stdin = new PassThrough();
    p.kill = () => { if (p.exitCode === null) { p.exitCode = 0; p.emit('exit', 0); } };
    let buf = '', spent = 0;
    p.stdin.on('data', async (d) => {
      buf += d.toString();
      let i;
      while ((i = buf.indexOf('\n')) >= 0) {
        const msg = JSON.parse(buf.slice(0, i)); buf = buf.slice(i + 1);
        p.sent.push(msg);
        // An interrupt ends the turn in progress (its result still comes, as with the real one).
        if (msg.type === 'control_request') { p.emit('interrupt'); continue; }
        const sid = (args.includes('--resume') ? args[args.indexOf('--resume') + 1] : `sess-${n}`);
        if (p.sent.length === 1) p.stdout.write(JSON.stringify({ type: 'system', subtype: 'init', session_id: sid }) + '\n');
        const out = await answer(msg.message.content, p);
        for (const m of out.messages || []) p.stdout.write(JSON.stringify(m) + '\n');
        p.stdout.write(JSON.stringify({ type: 'result', subtype: 'success', is_error: false, session_id: sid, result: out.text, total_cost_usd: (spent += 0.003), num_turns: out.rounds || 1, duration_api_ms: 900 * p.sent.length, usage: { input_tokens: 5, cache_read_input_tokens: 4000, cache_creation_input_tokens: 200, output_tokens: 30 } }) + '\n');
      }
    });
    p.stdin.on('finish', () => p.kill());
    procs.push(p);
    return p;
  };
  return { spawn, procs };
}

const calls = [];
const tools = [
  { name: 'list_projects', description: 'Projects', input_schema: { type: 'object', properties: {}, additionalProperties: false }, run: () => { calls.push('list_projects'); return { projects: ['api'] }; } },
  { name: 'set_voice_setting', description: 'Change a setting', input_schema: { type: 'object', properties: {}, additionalProperties: false }, run: () => ({ needsConfirmation: true, id: 'a1', ask: 'Change speechRate to 1.2?' }) },
  { name: 'close_voice', description: 'Close', input_schema: { type: 'object', properties: {}, additionalProperties: false }, run: () => ({ ok: true, close: true }) },
  { name: 'memory', def: { type: 'memory_20250818', name: 'memory' }, run: (i) => `viewed ${i.path}` },
];

vs.updateSettings({ model: 'claude-sonnet-5-5', cliIdleMinutes: 30 });
const work = join(dir, 'memories'), state = join(dir, 'cli');
let turnsSeen = 0;
let agent;
const make = (fake, extra = {}) => createCliVoiceAgent({
  tools, workdir: work, stateDir: state, apiUrl: () => 'http://127.0.0.1:8889', claudeBin: () => '/usr/bin/claude',
  spawn: fake.spawn, persona: () => '- Short answers.', beforeTurn: () => { turnsSeen++; }, ...extra,
});

// ─── spawn ───
{
  const fake = fakeClaude(async (content) => {
    if (/kanban/.test(content)) {
      const r = await agent.callTool('mcp__cru__list_projects'.replace(/^mcp__cru__/, ''), {});
      return { text: 'One project, Sir.\n---\napi', rounds: 2, messages: [{ type: 'assistant', message: { content: [{ type: 'tool_use', name: 'mcp__cru__list_projects', input: {} }] } }], r };
    }
    if (/faster/.test(content)) { await agent.callTool('set_voice_setting', {}); return { text: 'Shall I make it 1.2, Sir?' }; }
    if (/that's all/.test(content)) { await agent.callTool('close_voice', {}); return { text: '' }; }
    return { text: 'Yes, Sir.' };
  });
  agent = make(fake);
  const w = await agent.warm();
  ok(w.ok && fake.procs.length === 1, 'warm() starts the process before the owner finishes speaking');
  const a = fake.procs[0].args;
  const val = (f) => a[a.indexOf(f) + 1];
  ok(a.includes('-p') && val('--input-format') === 'stream-json' && val('--output-format') === 'stream-json', 'the JSON mode the chats use (stream-json both ways)');
  ok(val('--tools') === '' && a.includes('--strict-mcp-config') && val('--setting-sources') === '' && a.includes('--disable-slash-commands'), 'no built-in tools, only the cru server, none of the owner\'s settings, hooks or skills');
  ok(val('--permission-mode') === 'dontAsk' && val('--allowedTools') === 'mcp__cru', 'only Cru\'s tools are allowed; anything else is refused, never asked');
  ok(val('--system-prompt') === CLI_PROMPT && /mcp__cru__/.test(CLI_PROMPT), 'Cru\'s own prompt replaces Claude Code\'s, and names the tools as they arrive');
  ok(val('--model') === 'claude-sonnet-5-5' && val('--effort') === 'low', 'the model from Settings, low effort (same as the API brain)');
  ok(fake.procs[0].opts.cwd === work, 'runs in Cru\'s notes folder');
  ok(!('ANTHROPIC_API_KEY' in fake.procs[0].opts.env), 'the subscription pays: no API key reaches it');
  const mcp = JSON.parse(readFileSync(val('--mcp-config'), 'utf-8')).mcpServers.cru;
  ok(/mcp-cru\.js$/.test(mcp.args[0]) && mcp.env.CRU_TOKEN === agent.token && mcp.env.CRU_TOKEN.length >= 32, 'the cru server gets its own token, not Crundi\'s internal key');
  const defs = JSON.parse(readFileSync(mcp.env.CRU_TOOLS_FILE, 'utf-8'));
  ok(defs.map(d => d.name).join() === 'close_voice,list_projects,memory,set_voice_setting', 'the same tools as the API brain, sorted');
  ok(defs.find(d => d.name === 'memory').input_schema.properties.command.enum.includes('str_replace'), 'the memory tool as an ordinary schema');

  // ─── turns ───
  const r1 = await agent.turn('what is on the kanban', { where: 'workbench', now: new Date('2026-11-10T09:00:00Z') });
  ok(r1.ok && r1.speak === 'One project, Sir.' && r1.show === 'api', 'a turn answers, split into spoken and shown');
  ok(r1.tools.join() === 'list_projects' && calls.join() === 'list_projects', 'its tool call ran here, through callTool');
  ok(r1.brain === 'claude-code' && r1.costUsd === 0.003 && r1.cache.read === 4000 && r1.rounds === 2, 'cost, cache use and rounds are reported');
  const first = fake.procs[0].sent[0].message.content;
  ok(/Short answers/.test(first) && /workbench/.test(first) && /what is on the kanban/.test(first), 'the persona leads a new conversation; context rides in the message');
  ok(turnsSeen === 1, 'a turn by the owner counts (pending actions may now be confirmed)');
  const r2 = await agent.turn('talk faster');
  ok(Math.abs(r2.costUsd - 0.003) < 1e-9 && r2.apiMs === 900, 'Claude Code\'s running totals become this turn\'s cost and API time');
  ok(r2.ok && r2.pending.length === 1 && r2.pending[0].id === 'a1', 'a question needing the owner\'s yes comes back, as with the API');
  ok(!/Short answers/.test(fake.procs[0].sent[1].message.content), 'the persona only opens the conversation');
  ok(fake.procs.length === 1, 'one long-lived process for the conversation');
  const r3 = await agent.turn("that's all");
  ok(r3.ok && r3.close === true && r3.speak === '', 'close_voice with no reply: closes silently');
  const rep = await agent.report('the API chat finished');
  ok(rep.ok && turnsSeen === 3, 'a report is a turn, but not the owner speaking');
  ok((await agent.callTool('nope', {})).isError, 'an unknown tool is refused');

  // ─── session ───
  ok(agent.state.sessionId === 'sess-0', 'the session id is kept');
  const fake2 = fakeClaude(async () => ({ text: 'Still here, Sir.' }));
  agent = make(fake2);
  await agent.turn('hello again');
  const a2 = fake2.procs[0].args;
  ok(a2[a2.indexOf('--resume') + 1] === 'sess-0', 'after a restart the conversation is resumed');
  ok(!/Short answers/.test(fake2.procs[0].sent[0].message.content), 'a resumed conversation is not given the persona again');

  // ─── compact ───
  const fake3 = fakeClaude(async (c) => (/Summarise/.test(c) ? { text: 'We talked about the API chat.' } : { text: 'Yes, Sir.' }));
  agent = make(fake3, { onCompacted: (s) => { agent.compacted = s; } });
  for (let i = 0; i < 8; i++) await agent.turn('turn ' + i);
  const c = await agent.compact();
  ok(c.ok && /API chat/.test(c.summary) && agent.state.sessionId === '', 'compaction: a summary, and the next turn starts a new conversation');
  await agent.turn('new topic');
  const last = fake3.procs[fake3.procs.length - 1];
  ok(!last.args.includes('--resume') && /We talked about the API chat/.test(last.sent[0].message.content) && /Short answers/.test(last.sent[0].message.content), 'the new conversation opens with the persona and the summary');
}

// ─── a model change ───
{
  // The owner switches model: the process is replaced (resumed on the new
  // model). The old one ending must not fail the turn already on the new one.
  vs.updateSettings({ model: 'claude-sonnet-5-5' });
  const fake = fakeClaude(async () => { await new Promise(r => setTimeout(r, 60)); return { text: 'Yes, Sir.' }; });
  const realKill = null;
  const ag = make(fake);
  await ag.turn('hello');
  vs.updateSettings({ model: 'claude-opus-5-5' });
  // The old process takes a moment to go, as a real one does.
  const old = fake.procs[0];
  old.kill = () => setTimeout(() => { if (old.exitCode === null) { old.exitCode = 0; old.emit('exit', 0); } }, 20);
  old.stdin.removeAllListeners('finish');
  old.stdin.on('finish', () => old.kill());
  const r = await ag.turn('switch back');
  ok(r.ok && r.speak === 'Yes, Sir.' && fake.procs.length === 2, 'after a model change the old process ending does not fail the new turn');
  vs.updateSettings({ model: 'claude-sonnet-5-5' });
}

// ─── cancel ───
{
  let hold = null;
  const fake = fakeClaude(async (content, p) => {
    if (/slow/.test(content)) { await new Promise(r => { hold = r; p.once('interrupt', r); }); return { text: 'Too late, Sir.' }; }
    return { text: 'Yes, Sir.' };
  });
  const ag = make(fake);
  const pr = ag.turn('something slow');
  await new Promise(r => setTimeout(r, 30));
  const c = ag.cancel();
  const r = await pr;
  const sent = fake.procs[0].sent.map(m => m.type === 'control_request' ? m.request.subtype : m.type);
  ok(c.cancelled && r.cancelled && !r.speak && sent.includes('interrupt'), 'cancel interrupts Claude Code; the turn ends unanswered');
  const next = await ag.turn('hello');
  ok(next.ok && next.speak === 'Yes, Sir.', 'the interrupted turn\'s late result is not taken as the next answer');
  const told = fake.procs[0].sent.filter(m => m.type === 'user').at(-1).message.content;
  ok(/cancelled the last request/.test(told), 'and Claude Code is told the cancelled request is not to be carried out');
}

// ─── idle ───
{
  vs.updateSettings({ cliIdleMinutes: 1 });
  const realSet = globalThis.setTimeout;
  const delays = [];
  globalThis.setTimeout = (fn, ms, ...rest) => { delays.push(ms); return realSet(fn, Math.min(ms, 5), ...rest); };
  const fake = fakeClaude(async () => ({ text: 'Yes.' }));
  const ag = make(fake);
  await ag.warm();
  await new Promise(r => realSet(r, 50));
  globalThis.setTimeout = realSet;
  ok(delays.includes(60000) && fake.procs[0].exitCode !== null && !ag.running, 'the process stops after the idle time in Settings');
  // The owner speaks again: started again on its own, the same conversation resumed, no persona repeated.
  const was = ag.state.sessionId;
  const back = await ag.turn('are you there?');
  const again = fake.procs[fake.procs.length - 1];
  ok(back.ok && fake.procs.length === 2 && was && again.args[again.args.indexOf('--resume') + 1] === was, 'after an idle stop the next message starts it again and resumes the same conversation');
  ok(!/Short answers/.test(again.sent[0].message.content), 'and carries on rather than starting over');
  ok(!vs.updateSettings({ cliIdleMinutes: 0 }).ok && vs.updateSettings({ cliIdleMinutes: 30 }).ok && vs.publicSettings().cliIdleMinutes === 30, 'Claude Code\'s idle time is a setting, 1 to 240 minutes, 30 by default');
  ok(vs.publicSettings().screenCloseSeconds === 60 && !vs.updateSettings({ screenCloseSeconds: 5 }).ok && vs.updateSettings({ screenCloseSeconds: 30 }).ok && vs.publicSettings().screenCloseSeconds === 30, 'the voice screen closes after a short silence: a setting, 15 to 600 s, a minute by default');
}

// ─── brain ───
{
  const api = { turn: async () => ({ ok: true, speak: 'API.', costUsd: 0.01, cache: { read: 10, written: 0, uncached: 1 } }), report: async () => ({ ok: true }), warm: async () => ({ ok: true }), compact: async () => ({}), reset() {}, lastActivity: 1 };
  const cli = { turn: async () => ({ ok: true, speak: 'CLI.', brain: 'claude-code', costUsd: 0.002, ms: 1500, cache: { read: 90, written: 10, uncached: 0 } }), report: async () => ({ ok: true }), warm: async () => ({ ok: true }), compact: async () => ({}), reset() {}, lastActivity: 2 };
  const logFile = join(dir, 'brain-log.jsonl');
  const b = createVoiceBrain({ api, cli, logFile });
  ok(vs.publicSettings().brain === 'api' && (await b.turn('x')).speak === 'API.', 'the API answers by default');
  ok(!vs.updateSettings({ brain: 'gemini' }).ok, 'only the two brains');
  vs.updateSettings({ brain: 'claude-code' });
  ok((await b.turn('y')).speak === 'CLI.' && b.which === 'claude-code', 'the setting switches to Claude Code');
  await b.turn('z');
  const st = b.stats();
  const g = st.find(x => x.brain === 'claude-code');
  ok(st.length === 2 && g.turns === 2 && g.costPerTurnUsd === 0.002 && g.cacheReadShare === 0.9 && g.medianMs === 1500, 'every turn is logged; stats per brain and model');
  vs.updateSettings({ brain: 'api' });
}

// ─── the owner's thinking effort: a change starts Claude Code again with it ───
{
  const fake = fakeClaude(async () => ({ text: 'Yes, Sir.' }));
  const ea = make(fake);
  vs.updateSettings({ effort: 'low' });
  await ea.warm();
  vs.updateSettings({ effort: 'high' });
  await ea.warm();
  const at = (p, f) => p.args[p.args.indexOf(f) + 1];
  ok(fake.procs.length === 2 && at(fake.procs[0], '--effort') === 'low' && at(fake.procs[1], '--effort') === 'high', 'a new thinking effort: Claude Code starts again with it');
  await ea.warm();
  ok(fake.procs.length === 2, 'and only when it changes');
  vs.updateSettings({ effort: 'low' });
  ea.stop?.();
}

console.log(failed ? `\n${failed} failed` : '\nAll passed');
process.exit(failed ? 1 : 0);
