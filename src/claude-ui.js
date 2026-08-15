/**
 * claude-ui.js — Claude Code chat-session manager for Crundi.
 *
 * The UI counterpart to claude-terminals.js. Where terminal mode attaches a PTY
 * and streams raw bytes, UI mode spawns the SAME `claude` binary with the
 * newline-delimited JSON protocol and reconstructs a structured conversation:
 * assistant text, thinking, tool calls, permission prompts and multiple-choice
 * questions — everything the interactive CLI renders, as data.
 *
 * We deliberately drive the user's own installed CLI rather than
 * @anthropic-ai/claude-agent-sdk: Anthropic does not permit third-party products
 * to run the Agent SDK on claude.ai subscription auth (API-key only, unless
 * previously approved), whereas spawning the CLI the user already logged into is
 * exactly what terminal mode has always done. It also keeps the dependency count
 * at zero — the SDK ships a ~320 MB bundled CLI per platform.
 *
 * Wire protocol (captured from the SDK's own transport, all NDJSON):
 *
 *   spawn: claude --output-format stream-json --verbose --input-format stream-json
 *                 --permission-prompt-tool stdio --setting-sources=user,project,local
 *                 [--permission-mode M] [--model M] [--effort E] [--continue|--resume ID]
 *
 *   host -> cli   {"type":"control_request","request_id":R,"request":{"subtype":"initialize",...}}
 *                 {"type":"user","message":{"role":"user","content":[{"type":"text","text":"..."}]}}
 *                 {"type":"control_response","response":{"subtype":"success","request_id":R,"response":{...}}}
 *
 *   cli  -> host  {"type":"system","subtype":"init",...}            session_id, model, tools, slash_commands
 *                 {"type":"assistant","message":{...}}              text / thinking / tool_use blocks
 *                 {"type":"user","message":{...}}                   tool_result blocks
 *                 {"type":"stream_event",...}                       token deltas (--include-partial-messages)
 *                 {"type":"result",...}                             turn finished, cost, duration
 *                 {"type":"control_request","request":{"subtype":"can_use_tool",...}}   permission prompt
 *
 * `can_use_tool` is answered with a control_response carrying
 * {behavior:'allow'|'deny', updatedInput, toolUseID}. AskUserQuestion arrives on
 * the same channel with requires_user_interaction:true — the host renders the
 * choices and answers by injecting an `answers` map into updatedInput.
 */

import { spawn } from 'child_process';
import { EventEmitter } from 'events';
import { existsSync } from 'fs';
import { join, delimiter } from 'path';
import { getProject } from './project-store.js';
import { hasExistingConversation } from './claude-terminals.js';

const isWin = process.platform === 'win32';
const MAX_MESSAGES = 2000;      // conversation entries kept per session
const MAX_TEXT = 200_000;       // chars kept per streamed text block

/**
 * Resolve the `claude` executable to an absolute path.
 *
 * child_process.spawn() without a shell does not apply PATHEXT on Windows, so a
 * bare "claude" fails to launch even when it works in cmd. Terminal mode dodges
 * this by going through `cmd /c`; we need a real path because we own the pipes.
 */
let _claudeBin = null;
export function resolveClaudeBin() {
  if (_claudeBin) return _claudeBin;
  const names = isWin ? ['claude.exe', 'claude.cmd', 'claude.bat', 'claude'] : ['claude'];
  const dirs = [
    join(process.env.HOME || process.env.USERPROFILE || '', '.local', 'bin'),
    ...String(process.env.PATH || '').split(delimiter).filter(Boolean),
  ];
  for (const dir of dirs) {
    for (const n of names) {
      const p = join(dir, n);
      if (existsSync(p)) { _claudeBin = p; return p; }
    }
  }
  return null;
}

/** 16-char hex id, matching claude-terminals.js's scheme. */
function genId() {
  let s = '';
  for (let i = 0; i < 16; i++) s += Math.floor(Math.random() * 16).toString(16);
  return s;
}

let _seq = 0;
const nextSeq = () => ++_seq;

/**
 * Create the Claude UI-session manager.
 *
 * Sessions are keyed by id and carry the same {alias,title,order} shape as
 * terminals, so webapp.js can merge both lists into one workbench grid.
 */
export function createClaudeUiSessions({ apiUrl: initApiUrl, apiKey: initApiKey } = {}) {
  const sessions = new Map();
  let apiUrl = initApiUrl;
  let apiKey = initApiKey;
  // Notified on every session's agent-state transition, so webapp.js can drive
  // the shared status dots / Telegram pings without polling.
  let stateChangeCb = null;

  /** Public listing, sorted by project then display order. */
  function list() {
    const out = [];
    for (const s of sessions.values()) {
      out.push({
        id: s.id,
        project: s.alias,
        title: s.title,
        order: s.order,
        kind: 'ui',
        status: s.proc ? 'running' : 'exited',
        agentState: s.proc ? s.state : null,
        model: s.model || '',
        permissionMode: s.permissionMode,
        sessionId: s.sessionId || '',
        pending: [...s.pending.values()].map(p => p.entry),
      });
    }
    out.sort((a, b) => a.project === b.project ? (a.order - b.order) : a.project.localeCompare(b.project));
    return out;
  }

  const entriesForAlias = (key) => [...sessions.values()].filter(s => s.alias === key);

  /** Append a conversation entry and notify subscribers. */
  function emitEntry(s, entry) {
    entry.seq = nextSeq();
    s.messages.push(entry);
    if (s.messages.length > MAX_MESSAGES) s.messages.splice(0, s.messages.length - MAX_MESSAGES);
    s.emitter.emit('event', { type: 'entry', entry });
    return entry;
  }

  /** Patch an existing entry in place (tool results, streamed text, answers). */
  function patchEntry(s, entry, patch) {
    Object.assign(entry, patch);
    s.emitter.emit('event', { type: 'patch', id: entry.id, patch });
  }

  function setState(s, state) {
    if (s.state === state) return;
    s.state = state;
    s.emitter.emit('event', { type: 'state', state });
    s.onStateChange?.(s.id, state);
  }

  /** Write one NDJSON frame to the CLI's stdin. */
  function send(s, obj) {
    if (!s.proc?.stdin?.writable) return false;
    try { s.proc.stdin.write(JSON.stringify(obj) + '\n'); return true; }
    catch { return false; }
  }

  // ─── Session creation ───

  /**
   * Start a Claude chat session for a project.
   *
   * @param {string} alias                project alias
   * @param {object} opts
   * @param {string} opts.title           display title
   * @param {string} opts.model           --model (e.g. 'opus'); '' omits
   * @param {string} opts.effort          --effort (low|medium|high|xhigh|max)
   * @param {string} opts.permissionMode  default|acceptEdits|plan|bypassPermissions|dontAsk
   * @param {string} opts.sessionMode     'continue' | 'new' | 'resume'
   * @param {string} opts.resumeId        session id when sessionMode==='resume'
   */
  async function create(alias, {
    title = '', model = '', effort = '', permissionMode = 'default',
    sessionMode = null, resumeId = '',
  } = {}) {
    const key = String(alias || '').toLowerCase();
    const project = getProject(key);
    if (!project) return { ok: false, error: `Project "${alias}" not found` };
    if (!existsSync(project.path)) return { ok: false, error: `Project path does not exist: ${project.path}` };

    const bin = resolveClaudeBin();
    if (!bin) return { ok: false, error: 'Could not find the `claude` executable. Install Claude Code and make sure it is on your PATH.' };

    const args = [
      '--output-format', 'stream-json',
      '--verbose',
      '--input-format', 'stream-json',
      '--permission-prompt-tool', 'stdio',
      '--include-partial-messages',
      '--setting-sources=user,project,local',
      '--permission-mode', permissionMode || 'default',
    ];
    if (model) args.push('--model', String(model));
    if (effort) args.push('--effort', String(effort));
    // Same resume policy as terminal mode: only pass --continue when a prior
    // transcript exists, or the CLI exits immediately with "No conversation found".
    if (sessionMode === 'resume' && resumeId) args.push('--resume', String(resumeId));
    else if (sessionMode === 'continue' && hasExistingConversation(project.path)) args.push('--continue');

    let proc;
    try {
      proc = spawn(bin, args, {
        cwd: project.path,
        stdio: ['pipe', 'pipe', 'pipe'],
        env: {
          ...process.env,
          // Marks the session as Crundi-managed without giving the lifecycle
          // hooks a terminal id to report against — UI mode derives agent state
          // from the message stream, so the hooks intentionally no-op here.
          CRUNDI_UI_SESSION: '1',
          ...(apiUrl ? { CRUNDI_API_URL: apiUrl } : {}),
          ...(apiKey ? { CRUNDI_API_KEY: apiKey } : {}),
        },
      });
    } catch (err) {
      return { ok: false, error: `Failed to start claude: ${err.message}` };
    }

    const id = genId();
    const siblings = entriesForAlias(key);
    const s = {
      id,
      alias: key,
      title: (title && String(title).trim()) || 'Chat',
      order: siblings.length ? Math.max(...siblings.map(x => x.order)) + 1 : 0,
      proc,
      emitter: new EventEmitter(),
      messages: [],
      pending: new Map(),      // requestId -> { entry, request }
      state: 'idle',
      sessionId: '',
      model,
      effort,
      permissionMode: permissionMode || 'default',
      slashCommands: [],
      cwd: project.path,
      stdoutBuf: '',
      blocks: new Map(),       // streaming block index -> entry (partial messages)
      streamedQueue: [],       // streamed text/thinking entries awaiting their final message
      onStateChange: stateChangeCb,
      totalCostUsd: 0,
    };
    s.emitter.setMaxListeners(50);
    sessions.set(id, s);

    proc.stdout.setEncoding('utf8');
    proc.stdout.on('data', (chunk) => {
      s.stdoutBuf += chunk;
      let nl;
      while ((nl = s.stdoutBuf.indexOf('\n')) >= 0) {
        const line = s.stdoutBuf.slice(0, nl).trim();
        s.stdoutBuf = s.stdoutBuf.slice(nl + 1);
        if (!line) continue;
        let msg;
        try { msg = JSON.parse(line); } catch { continue; }
        try { handleMessage(s, msg); }
        catch (err) { console.error('[claude-ui] handler error:', err?.message || err); }
      }
    });

    proc.stderr.setEncoding('utf8');
    proc.stderr.on('data', (d) => {
      const text = String(d).trim();
      if (text) console.warn(`[claude-ui] (${id}) stderr: ${text.slice(0, 500)}`);
    });

    proc.on('exit', (code) => {
      console.log(`[claude-ui] "${key}" (${id}) exited (code ${code})`);
      s.proc = null;
      // Fail-closed: nothing can answer a parked prompt once the CLI is gone.
      for (const [, p] of s.pending) patchEntry(s, p.entry, { status: 'cancelled' });
      s.pending.clear();
      setState(s, 'idle');
      s.emitter.emit('event', { type: 'exit', code });
    });

    proc.on('error', (err) => {
      emitEntry(s, { id: genId(), kind: 'error', text: `Process error: ${err.message}` });
    });

    // Handshake. systemPrompt:[""] mirrors what the SDK sends for the default
    // preset; supportedDialogKinds opts us into blocking dialogs we can render.
    send(s, {
      type: 'control_request',
      request_id: 'init-' + genId(),
      request: { subtype: 'initialize', systemPrompt: [''] },
    });

    console.log(`[claude-ui] Started chat "${s.title}" for "${key}" (${id}) in ${project.path}`);
    return { ok: true, id, project: key, title: s.title, order: s.order, kind: 'ui' };
  }

  // ─── Inbound message handling ───

  function handleMessage(s, msg) {
    switch (msg.type) {
      case 'system':      return handleSystem(s, msg);
      case 'assistant':   return handleAssistant(s, msg);
      case 'user':        return handleUser(s, msg);
      case 'stream_event': return handleStreamEvent(s, msg);
      case 'result':      return handleResult(s, msg);
      case 'control_request':  return handleControlRequest(s, msg);
      case 'control_response': return handleControlResponse(s, msg);
    }
  }

  function handleSystem(s, msg) {
    if (msg.subtype === 'init') {
      s.sessionId = msg.session_id || s.sessionId;
      if (msg.model) s.model = msg.model;
      s.slashCommands = msg.slash_commands || [];
      s.emitter.emit('event', {
        type: 'init',
        sessionId: s.sessionId,
        model: s.model,
        slashCommands: s.slashCommands,
        tools: msg.tools || [],
        mcpServers: msg.mcp_servers || [],
      });
    } else if (msg.subtype === 'compact_boundary') {
      emitEntry(s, { id: genId(), kind: 'notice', text: 'Context compacted.' });
    }
  }

  /**
   * The final `assistant` message repeats content already delivered as deltas.
   * Each text/thinking block streamed since the last assistant message sits in
   * streamedQueue in emission order, so we consume it in the same order the
   * blocks arrive and patch it to the authoritative text — never appending a
   * second copy. Anything not streamed (partial messages off, or a block that
   * produced no deltas) falls through to a fresh entry.
   */
  function handleAssistant(s, msg) {
    for (const block of msg.message?.content || []) {
      if (block.type === 'text' || block.type === 'thinking') {
        const text = (block.type === 'text' ? block.text : block.thinking) || '';
        const prior = s.streamedQueue.shift();
        if (prior) patchEntry(s, prior, { text, streaming: false });
        else emitEntry(s, { id: genId(), kind: block.type === 'text' ? 'assistant-text' : 'thinking', text });
      } else if (block.type === 'tool_use') {
        emitEntry(s, {
          id: genId(), kind: 'tool', toolUseId: block.id, name: block.name,
          input: block.input || {}, status: 'running', result: null, isError: false,
        });
      }
    }
  }

  function handleUser(s, msg) {
    for (const block of msg.message?.content || []) {
      if (block.type !== 'tool_result') continue;
      const target = [...s.messages].reverse().find(e => e.kind === 'tool' && e.toolUseId === block.tool_use_id);
      const content = Array.isArray(block.content)
        ? block.content.map(c => (c.type === 'text' ? c.text : `[${c.type}]`)).join('\n')
        : (typeof block.content === 'string' ? block.content : JSON.stringify(block.content ?? ''));
      if (target) patchEntry(s, target, { status: 'done', result: String(content).slice(0, MAX_TEXT), isError: !!block.is_error });
    }
  }

  /** Token-level deltas from --include-partial-messages. */
  function handleStreamEvent(s, msg) {
    const ev = msg.event;
    if (!ev) return;
    if (ev.type === 'content_block_start') {
      const b = ev.content_block;
      if (b?.type !== 'text' && b?.type !== 'thinking') return;
      // Seed from the block's initial content — usually empty, but not always.
      const entry = emitEntry(s, {
        id: genId(),
        kind: b.type === 'text' ? 'assistant-text' : 'thinking',
        text: (b.type === 'text' ? b.text : b.thinking) || '',
        streaming: true,
      });
      s.blocks.set(ev.index, entry);
      // Queued at START (not stop) so a block missing its stop event still
      // reconciles against the final assistant message instead of duplicating.
      s.streamedQueue.push(entry);
    } else if (ev.type === 'content_block_delta') {
      const entry = s.blocks.get(ev.index);
      if (!entry) return;
      const d = ev.delta || {};
      const add = d.type === 'text_delta' ? d.text : d.type === 'thinking_delta' ? d.thinking : '';
      if (!add) return;
      entry.text = (entry.text + add).slice(-MAX_TEXT);
      // Deltas are high-frequency; ship just the increment, not the whole block.
      s.emitter.emit('event', { type: 'delta', id: entry.id, text: add });
    } else if (ev.type === 'content_block_stop') {
      const entry = s.blocks.get(ev.index);
      if (entry) patchEntry(s, entry, { streaming: false });
      s.blocks.delete(ev.index);
    }
  }

  function handleResult(s, msg) {
    s.blocks.clear();
    s.streamedQueue.length = 0;
    if (typeof msg.total_cost_usd === 'number') s.totalCostUsd = msg.total_cost_usd;
    emitEntry(s, {
      id: genId(), kind: 'result', subtype: msg.subtype || 'success',
      costUsd: msg.total_cost_usd || 0, durationMs: msg.duration_ms || 0,
      numTurns: msg.num_turns || 0, isError: !!msg.is_error,
      text: msg.subtype && msg.subtype !== 'success' ? String(msg.result || msg.subtype) : '',
    });
    setState(s, 'idle');
  }

  /**
   * The CLI asking US something. Today that is `can_use_tool` (permission
   * prompts and AskUserQuestion) plus MCP elicitation / blocking dialogs.
   * Each becomes a pending entry the UI renders and the user answers.
   */
  function handleControlRequest(s, msg) {
    const req = msg.request || {};
    const requestId = msg.request_id;
    if (req.subtype === 'can_use_tool') {
      const isQuestion = req.tool_name === 'AskUserQuestion';
      const entry = {
        id: genId(),
        kind: isQuestion ? 'question' : 'permission',
        requestId,
        toolUseId: req.tool_use_id || '',
        toolName: req.tool_name,
        displayName: req.display_name || req.tool_name,
        description: req.description || '',
        title: req.title || '',
        input: req.input || {},
        suggestions: req.permission_suggestions || [],
        decisionReason: req.decision_reason || '',
        decisionReasonType: req.decision_reason_type || '',
        // When set, one-tap allow/deny is not offered — the card itself is the
        // interaction surface (AskUserQuestion) or the disclosure can't ride the wire.
        requiresUserInteraction: !!req.requires_user_interaction,
        suppressAlwaysAllow: !!req.suppress_always_allow_rule,
        status: 'pending',
      };
      // Register and flip to needs-input BEFORE emitting: a subscriber may answer
      // re-entrantly inside the emit, and a later setState here would then
      // clobber the 'working' that respond() correctly set.
      s.pending.set(requestId, { entry, request: req });
      setState(s, 'needs-input');
      emitEntry(s, entry);
      return;
    }
    // Anything else we cannot render: decline so the CLI is never left parked.
    send(s, {
      type: 'control_response',
      response: { subtype: 'error', request_id: requestId, error: `Unsupported control request: ${req.subtype}` },
    });
  }

  function handleControlResponse(s, msg) {
    const r = msg.response;
    if (r?.subtype === 'error') console.warn(`[claude-ui] (${s.id}) control error: ${r.error}`);
  }

  // ─── Outbound actions ───

  /** Send a user turn. */
  function sendMessage(id, text) {
    const s = sessions.get(id);
    if (!s) return { ok: false, error: `No session "${id}"` };
    if (!s.proc) return { ok: false, error: 'Session is not running' };
    const body = String(text ?? '');
    if (!body.trim()) return { ok: false, error: 'Message is empty' };
    emitEntry(s, { id: genId(), kind: 'user', text: body });
    const ok = send(s, {
      type: 'user',
      session_id: s.sessionId || '',
      parent_tool_use_id: null,
      message: { role: 'user', content: [{ type: 'text', text: body }] },
    });
    if (!ok) return { ok: false, error: 'Failed to write to the session' };
    setState(s, 'working');
    return { ok: true };
  }

  /**
   * Answer a pending permission prompt or question.
   *
   * @param {string} id        session id
   * @param {object} answer
   * @param {string} answer.requestId
   * @param {'allow'|'deny'} answer.behavior
   * @param {object} [answer.updatedInput]  edited tool input, or AskUserQuestion answers
   * @param {boolean} [answer.always]       also apply the CLI's permission suggestions
   * @param {string} [answer.message]       denial reason shown to Claude
   */
  function respond(id, { requestId, behavior, updatedInput, always, message } = {}) {
    const s = sessions.get(id);
    if (!s) return { ok: false, error: `No session "${id}"` };
    const pending = s.pending.get(requestId);
    if (!pending) return { ok: false, error: 'That request is no longer pending' };
    const { entry, request } = pending;

    let response;
    if (behavior === 'deny') {
      response = { behavior: 'deny', message: message || 'The user declined this action.' };
    } else {
      response = {
        behavior: 'allow',
        updatedInput: updatedInput && typeof updatedInput === 'object' ? updatedInput : (request.input || {}),
        toolUseID: request.tool_use_id,
      };
      // "Always allow" replays the CLI's own suggestions verbatim, which is how
      // it wants the rule written (allow-rule, mode switch, or added directory).
      if (always && Array.isArray(request.permission_suggestions) && request.permission_suggestions.length) {
        response.updatedPermissions = request.permission_suggestions;
      }
    }

    const ok = send(s, { type: 'control_response', response: { subtype: 'success', request_id: requestId, response } });
    if (!ok) return { ok: false, error: 'Failed to write to the session' };

    s.pending.delete(requestId);
    patchEntry(s, entry, {
      status: behavior === 'deny' ? 'denied' : 'allowed',
      answeredInput: response.updatedInput || null,
      always: !!always,
    });
    if (!s.pending.size) setState(s, 'working');
    return { ok: true };
  }

  /** Stop the current turn without killing the session. */
  function interrupt(id) {
    const s = sessions.get(id);
    if (!s?.proc) return { ok: false, error: `No session "${id}"` };
    send(s, { type: 'control_request', request_id: 'int-' + genId(), request: { subtype: 'interrupt' } });
    setState(s, 'idle');
    return { ok: true };
  }

  /** Switch permission mode mid-session. */
  function setPermissionMode(id, mode) {
    const s = sessions.get(id);
    if (!s?.proc) return { ok: false, error: `No session "${id}"` };
    const valid = ['default', 'acceptEdits', 'plan', 'bypassPermissions', 'dontAsk'];
    if (!valid.includes(mode)) return { ok: false, error: `Invalid permission mode "${mode}"` };
    send(s, { type: 'control_request', request_id: 'pm-' + genId(), request: { subtype: 'set_permission_mode', mode } });
    s.permissionMode = mode;
    s.emitter.emit('event', { type: 'meta', permissionMode: mode });
    return { ok: true, permissionMode: mode };
  }

  /** Switch model mid-session. */
  function setModel(id, model) {
    const s = sessions.get(id);
    if (!s?.proc) return { ok: false, error: `No session "${id}"` };
    send(s, { type: 'control_request', request_id: 'mdl-' + genId(), request: { subtype: 'set_model', model: model || undefined } });
    s.model = model || '';
    s.emitter.emit('event', { type: 'meta', model: s.model });
    return { ok: true, model: s.model };
  }

  // ─── Lifecycle / bookkeeping (mirrors claude-terminals.js) ───

  function close(id) {
    const s = sessions.get(id);
    if (!s) return { ok: false, error: `No session "${id}"` };
    if (s.proc) {
      try { s.proc.stdin.end(); } catch { /* ignore */ }
      try { s.proc.kill(); } catch { /* ignore */ }
    }
    s.emitter.removeAllListeners();
    sessions.delete(id);
    console.log(`[claude-ui] Closed "${s.alias}" (${id})`);
    return { ok: true };
  }

  function closeProject(alias) {
    const key = String(alias || '').toLowerCase();
    let closed = 0;
    for (const s of entriesForAlias(key)) { close(s.id); closed++; }
    return { ok: true, closed };
  }

  function closeAll() {
    for (const id of [...sessions.keys()]) close(id);
  }

  function rename(id, title) {
    const s = sessions.get(id);
    if (!s) return { ok: false, error: `No session "${id}"` };
    s.title = (title && String(title).trim()) || 'Chat';
    return { ok: true, title: s.title };
  }

  function setOrder(alias, ids) {
    const key = String(alias || '').toLowerCase();
    if (!Array.isArray(ids)) return { ok: false, error: 'ids must be an array' };
    let n = 0;
    for (const id of ids) {
      const s = sessions.get(id);
      if (s && s.alias === key) s.order = n++;
    }
    for (const s of entriesForAlias(key)) if (!ids.includes(s.id)) s.order = n++;
    return { ok: true };
  }

  /** Full conversation for a session (used when a client subscribes). */
  function history(id) {
    const s = sessions.get(id);
    if (!s) return null;
    return {
      id: s.id, project: s.alias, title: s.title, kind: 'ui',
      status: s.proc ? 'running' : 'exited',
      state: s.state, sessionId: s.sessionId, model: s.model,
      permissionMode: s.permissionMode, slashCommands: s.slashCommands,
      totalCostUsd: s.totalCostUsd, cwd: s.cwd,
      messages: s.messages,
    };
  }

  const has = (id) => sessions.has(id);
  const on = (id, handler) => { const s = sessions.get(id); if (s) s.emitter.on('event', handler); };
  const off = (id, handler) => { const s = sessions.get(id); if (s) s.emitter.off('event', handler); };

  /** Register a callback fired whenever any session's agent state changes. */
  function onAnyStateChange(cb) {
    stateChangeCb = cb;
    for (const s of sessions.values()) s.onStateChange = cb;
  }

  return {
    list, create, close, closeProject, closeAll, rename, setOrder,
    sendMessage, respond, interrupt, setPermissionMode, setModel,
    history, has, on, off, onAnyStateChange,
    set apiUrl(v) { apiUrl = v; },
    get apiUrl() { return apiUrl; },
    set apiKey(v) { apiKey = v; },
    get apiKey() { return apiKey; },
  };
}
