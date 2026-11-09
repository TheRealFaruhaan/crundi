/**
 * voice/agent-cli.js — the experiment: Cru answered by the owner's own Claude
 * Code, in the JSON mode (stream-json) Crundi's chats use, instead of the
 * Claude API. Chosen by the "brain" setting; same interface as agent.js
 * (turn, report, warm, compact, reset, lastActivity), so nothing else changes.
 *
 * How it runs:
 *   - one long-lived `claude -p` per conversation, in Cru's notes folder
 *     (voice-memory/memories); started when a voice screen opens (warm) or on
 *     the first turn, stopped after cliIdleMinutes without a turn (Settings, default 30). The session id is kept on
 *     disk and resumed, so a restart carries on the same conversation.
 *   - locked down: no built-in tools at all (--tools ""), only the cru MCP
 *     server (--strict-mcp-config), none of the owner's settings, hooks,
 *     CLAUDE.md or skills (--setting-sources "", --disable-slash-commands),
 *     and anything that would need a permission is refused (dontAsk; only
 *     mcp__cru is allowed).
 *   - the tools are the API brain's own: mcp-cru.js only forwards calls back
 *     here (callTool), so every rule is enforced in one place.
 *   - the system prompt is Cru's, replacing Claude Code's; the persona and the
 *     time ride in the messages exactly as for the API.
 *
 * Paid from the subscription, not the API key: the monthly $ limit does not
 * apply; the cost Claude Code reports (list price) is logged for comparison.
 */

import { spawn as nodeSpawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as voiceSettings from './settings.js';
import { reflectionAsk, parseReflection } from './reflect.js';
import { SYSTEM_PROMPT, PERSONA_FILE, splitReply, contextLine, runVoiceTool, reportPrompt } from './agent.js';

/** A turn that takes longer than this is abandoned (the process is restarted). */
const TURN_MS = 180 * 1000;
/** Compact while idle from this many turns; force it before a turn past MAX_TURNS. */
const COMPACT_MIN_TURNS = 8;
const MAX_TURNS = 40;

export const CLI_PROMPT = `${SYSTEM_PROMPT}

Your tools: they come from the "cru" tool server, so their full names start with mcp__cru__ (mcp__cru__close_voice, mcp__cru__memory, ...); above they are called by the short name. They are your only tools.`;

/** The memory tool as an ordinary schema (the API brain gets Anthropic's built-in one). */
const MEMORY_DEF = {
  name: 'memory',
  description: 'Your notebook: files under /memories (view a folder or a file, create, str_replace, insert, delete, rename). Paths always start with /memories.',
  input_schema: {
    type: 'object', additionalProperties: false,
    properties: {
      command: { type: 'string', enum: ['view', 'create', 'str_replace', 'insert', 'delete', 'rename'] },
      path: { type: 'string', description: 'e.g. /memories/persona.md' },
      view_range: { type: 'array', items: { type: 'integer' }, description: 'view: [first, last] line' },
      file_text: { type: 'string', description: 'create: the whole file' },
      old_str: { type: 'string', description: 'str_replace: text to find (once)' },
      new_str: { type: 'string', description: 'str_replace: its replacement' },
      insert_line: { type: 'integer', description: 'insert: after this line (0 = top)' },
      insert_text: { type: 'string', description: 'insert: the text' },
      old_path: { type: 'string', description: 'rename: from' },
      new_path: { type: 'string', description: 'rename: to' },
    },
    required: ['command'],
  },
};

/**
 * @param {object} opts
 * @param {Array}  opts.tools        the same tools the API brain gets
 * @param {string} opts.workdir      where Claude Code runs (Cru's notes folder)
 * @param {string} opts.stateDir     where the session id and the MCP config are kept
 * @param {() => string} opts.apiUrl this Crundi's loopback URL, for mcp-cru.js
 * @param {() => string|null} opts.claudeBin  the claude executable
 * @param {Function} [opts.spawn]    tests pass a fake
 */
export function createCliVoiceAgent({
  tools, workdir, stateDir, apiUrl, claudeBin,
  settings = voiceSettings, beforeTurn = () => {}, beforeReport = () => {}, onTurnDone = () => {}, onCompacted = () => {}, persona = () => '', selfNote = null, selfFiles = null, onReflected = () => {},
  spawn = nodeSpawn,
  // Kid mode's conversation (see agent.js createVoiceAgent): its own prompt, first line and context line.
  systemPrompt = null, intro = null, context = null, turnNote = () => '',
} = {}) {
  const prompt = systemPrompt ? `${systemPrompt}\n\nYour tools: they come from the "cru" tool server, so their full names start with mcp__cru__; above they are called by the short name. They are your only tools.` : CLI_PROMPT;
  const byName = new Map(tools.map(t => [t.name, t]));
  const defs = tools
    .map(t => t.def ? (t.name === 'memory' ? MEMORY_DEF : null) : { name: t.name, description: t.description, input_schema: t.input_schema })
    .filter(Boolean)
    .sort((a, b) => a.name.localeCompare(b.name));
  // Reaches Cru's tools only, and only while this Crundi runs.
  const token = randomBytes(24).toString('hex');
  const stateFile = join(stateDir, 'cli-session.json');
  const toolsFile = join(stateDir, 'cli-tools.json');
  const mcpFile = join(stateDir, 'cli-mcp.json');
  mkdirSync(stateDir, { recursive: true, mode: 0o700 });
  mkdirSync(workdir, { recursive: true, mode: 0o700 });

  // { sessionId, turns, summary } — summary: the last compaction's, for the next conversation's first message.
  let state = { sessionId: '', turns: 0, summary: '' };
  try { if (existsSync(stateFile)) state = { ...state, ...JSON.parse(readFileSync(stateFile, 'utf-8')) }; } catch { /* start afresh */ }
  const saveState = () => { try { writeFileSync(stateFile, JSON.stringify(state), { mode: 0o600 }); } catch { /* next time */ } };

  let proc = null;          // the running claude, with .model
  let buf = '';
  let current = null;       // the turn in progress: { resolve, text, tools, timer }
  let busy = false;
  let lastActivity = Date.now();
  let idleTimer = null;
  let signals = null;       // the turn in progress's, filled by callTool
  // A cancelled turn's result is still to come from Claude Code: the next
  // turn waits for it (briefly), or it would be taken as the next answer.
  let draining = false;
  let drainTimer = null;
  // Claude Code keeps a cancelled request in its conversation: the next
  // message says it was cancelled, so it is not carried out later.
  let cancelledNote = '';

  const personaNote = () => {
    if (intro) { try { const n = String(intro() || ''); return n && !n.endsWith('\n') ? n + '\n' : n; } catch { return ''; } }
    // The whole note (persona, owner, routines) when the server builds it (reflect.js selfNote).
    if (selfNote) { try { const n = String(selfNote() || ''); if (n.trim()) return n.endsWith('\n') ? n : n + '\n'; } catch { /* fall back */ } }
    let p = '';
    try { p = String(persona() || '').trim(); } catch { /* none */ }
    return p ? `[How the owner wants you to be (${PERSONA_FILE}):\n${p.slice(0, 4000)}]\n` : `[${PERSONA_FILE} is empty: nothing learnt yet about how the owner wants you to be.]\n`;
  };

  function stop() {
    if (!proc) return;
    const p = proc; proc = null;
    try { p.stdin.end(); } catch { /* gone */ }
    setTimeout(() => { try { p.kill(); } catch { /* gone */ } }, 3000).unref?.();
  }

  function armIdle() {
    clearTimeout(idleTimer);
    // Settings › cliIdleMinutes, default 30.
    idleTimer = setTimeout(stop, (Number(settings.getSettings().cliIdleMinutes) || 30) * 60 * 1000);
    idleTimer.unref?.();
  }

  function finish(r) {
    if (!current) return;
    const c = current; current = null;
    clearTimeout(c.timer);
    c.resolve({ ...r, tools: c.tools });
  }

  function onLine(line, from) {
    let m;
    try { m = JSON.parse(line); } catch { return; }
    // Only the process the turn is running on speaks for it: an older one
    // (replaced on a model change, or stopped) never ends the current turn.
    if (current && current.proc && from && current.proc !== from) return;
    if (draining && !current && m.type === 'result') {
      draining = false; clearTimeout(drainTimer);
      const p = proc || {};
      p.costSoFar = Number(m.total_cost_usd) || 0; p.apiMsSoFar = Number(m.duration_api_ms) || 0;
      return;
    }
    if (m.type === 'system' && m.subtype === 'init' && m.session_id) {
      if (state.sessionId !== m.session_id) { state.sessionId = m.session_id; saveState(); }
      return;
    }
    if (!current) return;
    if (m.type === 'assistant' && Array.isArray(m.message?.content)) {
      for (const b of m.message.content) {
        if (b.type === 'tool_use') current.tools.push(String(b.name || '').replace(/^mcp__cru__/, ''));
      }
      return;
    }
    if (m.type === 'result') {
      if (m.session_id && state.sessionId !== m.session_id) { state.sessionId = m.session_id; saveState(); }
      // Cost and API time come as running totals for the process: this turn's is the difference.
      const p = proc || {};
      const cost = Math.max(0, (Number(m.total_cost_usd) || 0) - (p.costSoFar || 0));
      const apiMs = Math.max(0, (Number(m.duration_api_ms) || 0) - (p.apiMsSoFar || 0));
      p.costSoFar = Number(m.total_cost_usd) || 0; p.apiMsSoFar = Number(m.duration_api_ms) || 0;
      finish({ ok: !m.is_error, result: m, cost, apiMs });
    }
  }

  /** The process for this model, started (resuming the conversation) if need be. */
  function ensure(model) {
    if (proc && proc.model === model && proc.exitCode === null) return proc;
    stop();
    const bin = claudeBin();
    if (!bin) throw new Error('Claude Code (the claude command) is not installed on this server');
    writeFileSync(toolsFile, JSON.stringify(defs), { mode: 0o600 });
    writeFileSync(mcpFile, JSON.stringify({
      mcpServers: {
        cru: {
          command: process.execPath,
          args: [fileURLToPath(new URL('./mcp-cru.js', import.meta.url))],
          env: { CRU_API_URL: apiUrl(), CRU_TOKEN: token, CRU_TOOLS_FILE: toolsFile },
        },
      },
    }), { mode: 0o600 });
    const args = [
      '-p', '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose',
      '--system-prompt', prompt,
      '--model', model, '--effort', 'low',
      '--tools', '',
      '--strict-mcp-config', '--mcp-config', mcpFile,
      '--setting-sources', '',
      '--disable-slash-commands',
      '--permission-mode', 'dontAsk', '--allowedTools', 'mcp__cru',
    ];
    if (state.sessionId) args.push('--resume', state.sessionId);
    // The subscription pays: an API key in Crundi's environment must not be picked up instead.
    const env = { ...process.env };
    delete env.ANTHROPIC_API_KEY;
    delete env.CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR;
    const p = spawn(bin, args, { cwd: workdir, stdio: ['pipe', 'pipe', 'pipe'], env });
    p.model = model;
    p.err = '';
    buf = '';
    p.stdout.on('data', (d) => {
      buf += d.toString();
      let i;
      while ((i = buf.indexOf('\n')) >= 0) { const l = buf.slice(0, i).trim(); buf = buf.slice(i + 1); if (l) onLine(l, p); }
    });
    p.stderr.on('data', (d) => { p.err = (p.err + d.toString()).slice(-2000); });
    // A process ending fails the turn only if the turn is running on it.
    const mine = () => !current || !current.proc || current.proc === p;
    p.on('error', (e) => { if (proc === p) proc = null; if (mine()) finish({ ok: false, error: `Claude Code did not start: ${e.message}` }); });
    p.on('exit', (code) => {
      if (proc === p) proc = null;
      // A resumed session that no longer exists: forget it, the next turn starts afresh.
      if (/No conversation found|session.*not found/i.test(p.err)) { state.sessionId = ''; saveState(); }
      if (mine()) finish({ ok: false, error: `Claude Code stopped (exit ${code})${p.err ? `: ${p.err.trim().split('\n').pop()}` : ''}` });
    });
    proc = p;
    armIdle();
    return p;
  }

  /** One message into the conversation; resolves on its result. */
  function ask(model, content) {
    const p = ensure(model);
    return new Promise((resolve) => {
      current = { resolve, tools: [], proc: p };
      current.timer = setTimeout(() => { finish({ ok: false, error: 'Claude Code took too long to answer' }); stop(); }, TURN_MS);
      current.timer.unref?.();
      p.stdin.write(JSON.stringify({ type: 'user', message: { role: 'user', content } }) + '\n');
    });
  }

  async function turn(text, { where = '', locked = false, unrecognised = false, voice = null, confirmOk = false, speaker = '', now = new Date(), report = false } = {}) {
    const said = String(text || '').trim();
    if (!said) return { ok: false, error: 'Nothing was said' };
    if (busy) return { ok: false, error: 'Still answering the last request' };
    if (state.turns >= MAX_TURNS) await compact({ force: true, now });
    busy = true;
    const s = settings.getSettings();
    const model = s.model || settings.DEFAULT_MODEL;
    signals = { pending: [], needsUnlock: false, phoneActions: [], close: false, acted: [] };
    const turnSignals = signals;
    const t0 = Date.now();
    // The last turn was cancelled and its end has not come yet: wait a moment, else start afresh (resumed).
    for (let i = 0; i < 50 && draining; i++) await new Promise(ok => setTimeout(ok, 100));
    if (draining) { draining = false; clearTimeout(drainTimer); stop(); }
    try {
      if (!report) beforeTurn({ locked, unrecognised, voice, confirmOk }); else beforeReport();
      const first = !state.sessionId;
      const lead = first ? `${personaNote()}${state.summary ? `[What we have been doing, summarised. Earlier detail is in memory_search.]\n${state.summary}\n` : ''}` : '';
      settings.countTurn(now);
      let r;
      const note = cancelledNote; cancelledNote = '';
      let tn = '';
      try { tn = String(turnNote() || '').trim(); } catch { tn = ''; }
      const ctx = context ? context({ now, where, locked, unrecognised, speaker }) : contextLine({ now, where, locked, unrecognised, speaker, confirmOk });
      try { r = await ask(model, `${lead}${note}${ctx}${tn ? `\n${tn}` : ''}\n${said}`); }
      catch (e) { return { ok: false, error: String(e.message || e) }; }
      const tools = r.tools;
      if (r.cancelled) return { ok: false, cancelled: true, acted: turnSignals.acted, tools };
      if (!r.ok) return { ok: false, error: r.error || `Claude Code: ${String(r.result?.result || r.result?.subtype || 'error').slice(0, 300)}`, tools };
      if (first && state.summary) { state.summary = ''; }
      state.turns += 1; saveState();
      const res = r.result;
      const reply = String(res.result || '').trim();
      const parts = splitReply(reply);
      const u = res.usage || {};
      const cache = { read: u.cache_read_input_tokens || 0, written: u.cache_creation_input_tokens || 0, uncached: u.input_tokens || 0 };
      try { onTurnDone({ said: report ? `(report) ${said}` : said, ...parts, at: now.getTime() }); } catch { /* best effort */ }
      return {
        ok: true, ...parts, tools, costUsd: r.cost || 0, cache,
        pending: signals.pending, needsUnlock: signals.needsUnlock, phoneActions: signals.phoneActions, close: signals.close,
        brain: 'claude-code', ms: Date.now() - t0, apiMs: r.apiMs || 0, rounds: res.num_turns || 0, outputTokens: u.output_tokens || 0,
      };
    } finally {
      busy = false;
      signals = null;
      lastActivity = Date.now();
      armIdle();
    }
  }

  /** The owner cancelled what they said: interrupt Claude Code; the turn ends now, unanswered. */
  function cancel() {
    if (!busy || !current || !proc) return { ok: true, cancelled: false, acted: [] };
    const acted = (signals && signals.acted ? signals.acted : []).slice();
    try { proc.stdin.write(JSON.stringify({ type: 'control_request', request_id: 'cancel-' + Date.now(), request: { subtype: 'interrupt' } }) + '\n'); } catch { /* it ends anyway */ }
    draining = true;
    clearTimeout(drainTimer);
    drainTimer = setTimeout(() => { if (draining) { draining = false; stop(); } }, 5000);
    drainTimer.unref?.();
    finish({ ok: false, cancelled: true });
    cancelledNote = `[The owner cancelled the last request before it was answered: leave it, and don't carry it out${acted.length ? `. Already done before the cancel: ${acted.map(x => x.what).join('; ')}` : ''}.]\n`;
    return { ok: true, cancelled: true, acted };
  }

  async function report(note, { now = new Date(), wake = false } = {}) {
    for (let i = 0; i < 60 && busy; i++) await new Promise(ok => setTimeout(ok, 500));
    if (busy) return { ok: false, error: 'busy' };
    return turn(reportPrompt(note, { wake }), { where: wake ? 'a wake-up' : 'a report', now, report: true });
  }

  /**
   * The conversation so far becomes a summary (asked of the conversation
   * itself), and the next turn starts a new one from it.
   */
  async function compact({ force = false, now = new Date() } = {}) {
    if (busy) return { ok: false, skipped: 'busy' };
    if (!state.sessionId) return { ok: false, skipped: 'no conversation' };
    if (!force && state.turns < COMPACT_MIN_TURNS) return { ok: false, skipped: 'short' };
    busy = true;
    try {
      const model = settings.getSettings().model || settings.DEFAULT_MODEL;
      let r, files = null;
      try {
        files = selfFiles ? selfFiles() : null;
        r = await ask(model, '[Not from the owner: Crundi is starting you a fresh conversation.] Summarise this conversation so you can carry on from the summary: decisions, requests in progress, actions taken (what, where), what is waiting on someone, facts learnt about the owner\'s projects and preferences. Plain prose and short lists, no preamble, under 400 words. Use no tools.' + (files ? reflectionAsk(files) : ''));
      } catch (e) { return { ok: false, skipped: String(e.message || e) }; }
      const learnt = parseReflection(r.ok ? String(r.result?.result || '') : '');
      const summary = learnt.summary;
      if (files && (learnt.persona || learnt.owner)) { try { onReflected({ persona: learnt.persona, owner: learnt.owner }); } catch { /* best effort */ } }
      if (!summary) return { ok: false, skipped: r.error || 'empty summary' };
      stop();
      state = { sessionId: '', turns: 0, summary };
      saveState();
      try { onCompacted(summary, now.getTime()); } catch { /* best effort */ }
      return { ok: true, summary };
    } finally {
      busy = false;
    }
  }

  return {
    turn,
    report,
    cancel,
    /** A voice screen opened: have the process (and its tools) ready before the owner finishes speaking. */
    async warm() {
      if (busy) return { ok: true, skipped: 'busy' };
      try { ensure(settings.getSettings().model || settings.DEFAULT_MODEL); return { ok: true, started: true }; }
      catch (e) { return { ok: false, skipped: e.message }; }
    },
    compact,
    reset() { stop(); state = { sessionId: '', turns: 0, summary: '' }; saveState(); },
    /** A fresh conversation that starts from [summary] alone (Cru's start_fresh). */
    fresh(summary, now = new Date()) {
      const s = String(summary || '').trim();
      stop();
      state = { sessionId: '', turns: 0, summary: s };
      saveState();
      if (s) { try { onCompacted(s, now.getTime()); } catch { /* best effort */ } }
      return { ok: true };
    },
    stop,
    get lastActivity() { return lastActivity; },
    get token() { return token; },
    get running() { return !!proc; },
    /** From mcp-cru.js, through /api/voice/cli-tool. */
    async callTool(name, args) {
      return runVoiceTool(byName.get(name), name, args, signals);
    },
    /** Tests and diagnostics. */
    get toolDefs() { return defs; },
    get state() { return state; },
  };
}
