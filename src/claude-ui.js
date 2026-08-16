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
import { existsSync, readdirSync, statSync, openSync, readSync, closeSync, mkdirSync, writeFileSync, readFileSync, renameSync, unlinkSync } from 'fs';
import { join, delimiter, resolve as resolvePath } from 'path';
import { homedir } from 'os';
import { config } from './config.js';
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

/** Directory Claude Code stores a project's transcripts in. */
function transcriptDir(projectPath) {
  const encoded = resolvePath(projectPath).replace(/[^a-zA-Z0-9]/g, '-');
  return join(homedir(), '.claude', 'projects', encoded);
}

/**
 * Approximate a transcript's context size in tokens.
 *
 * Assistant messages record their own `usage`, and input + cache_read +
 * cache_creation on a late message is exactly the context the model was carrying
 * — the same number Claude Code quotes ("this session is … 180.6k tokens").
 * Only the file's tail is scanned, so this stays cheap on multi-MB transcripts.
 *
 * Take the LAST such record, never the largest. `/compact` keeps writing to the
 * same transcript, so the peak is a historical high-water mark that survives the
 * compaction that was supposed to clear it — a compacted session would stay
 * flagged as heavy forever. The final record is the context a resume inherits.
 *
 * Sidechain records are subagent turns carrying their own small contexts; they
 * would understate the main thread, so prefer the last main-thread record.
 */
function estimateTranscriptTokens(file, size) {
  try {
    const fd = openSync(file, 'r');
    let text;
    try {
      const len = Math.min(1024 * 1024, size);
      const buf = Buffer.alloc(len);
      const n = readSync(fd, buf, 0, len, Math.max(0, size - len));
      text = buf.toString('utf8', 0, n);
    } finally { closeSync(fd); }
    const lines = text.split('\n');
    lines.shift(); // first line is probably truncated mid-JSON
    let last = 0;
    let lastMain = 0;
    for (const line of lines) {
      if (!line.includes('cache_read_input_tokens')) continue;
      let o; try { o = JSON.parse(line); } catch { continue; }
      const u = o.message && o.message.usage;
      if (!u) continue;
      const ctx = (u.input_tokens || 0) + (u.cache_read_input_tokens || 0) + (u.cache_creation_input_tokens || 0);
      last = ctx;
      if (!o.isSidechain) lastMain = ctx;
    }
    return lastMain || last;
  } catch { return 0; }
}

/**
 * The conversation `--continue` would pick up, with the numbers needed to decide
 * whether that is an expensive thing to do.
 *
 * Claude Code's interactive "this session is old and large — resume from a
 * summary?" prompt is NOT available over the stream-json protocol (verified: no
 * request_user_dialog is emitted at spawn or on the first turn, even with
 * supportedDialogKinds declared). A stream-json resume is therefore always a
 * FULL resume, so the host has to surface the cost itself — before spawning,
 * while it is still free to choose.
 */
export function latestTranscript(projectPath) {
  try {
    const dir = transcriptDir(projectPath);
    if (!existsSync(dir)) return null;
    let newest = null;
    for (const f of readdirSync(dir)) {
      if (!f.endsWith('.jsonl')) continue;
      const full = join(dir, f);
      let st; try { st = statSync(full); } catch { continue; }
      if (!newest || st.mtimeMs > newest.mtime) newest = { id: f.slice(0, -6), full, mtime: st.mtimeMs, size: st.size };
    }
    if (!newest) return null;
    return {
      id: newest.id,
      title: readTranscriptTitle(newest.full) || '(untitled session)',
      tokens: estimateTranscriptTokens(newest.full, newest.size),
      ageHours: (Date.now() - newest.mtime) / 3600000,
      sizeBytes: newest.size,
      updatedAt: new Date(newest.mtime).toISOString(),
    };
  } catch { return null; }
}

/** Mirrors Claude Code's own "old and large" heuristic for its summary prompt. */
export const HEAVY_TOKENS = 100_000;
export const HEAVY_AGE_HOURS = 24;
export function isHeavyResume(t) {
  return !!t && (t.tokens >= HEAVY_TOKENS || t.ageHours >= HEAVY_AGE_HOURS);
}

/**
 * List resumable Claude Code conversations for a project directory.
 *
 * Claude Code stores one transcript per session at
 * ~/.claude/projects/<encoded-cwd>/<session-uuid>.jsonl, where the path is
 * encoded by replacing every non-alphanumeric character with a dash. The file
 * name IS the session id that `--resume` takes.
 *
 * We read only the tail-relevant record types (`ai-title` for the generated
 * title, `last-prompt` as a fallback label) rather than parsing whole
 * transcripts, which can be tens of MB.
 *
 * @returns {{ id, title, updatedAt, sizeBytes }[]} newest first
 */
export function listResumable(projectPath, limit = 30) {
  try {
    const encoded = resolvePath(projectPath).replace(/[^a-zA-Z0-9]/g, '-');
    const dir = join(homedir(), '.claude', 'projects', encoded);
    if (!existsSync(dir)) return [];
    const files = readdirSync(dir)
      .filter(f => f.endsWith('.jsonl'))
      .map(f => {
        const full = join(dir, f);
        let st;
        try { st = statSync(full); } catch { return null; }
        return { id: f.slice(0, -6), full, mtime: st.mtimeMs, size: st.size };
      })
      .filter(Boolean)
      .sort((a, b) => b.mtime - a.mtime)
      .slice(0, limit);

    return files.map(f => ({
      id: f.id,
      title: readTranscriptTitle(f.full) || '(untitled session)',
      updatedAt: new Date(f.mtime).toISOString(),
      sizeBytes: f.size,
      tokens: estimateTranscriptTokens(f.full, f.size),
      ageHours: (Date.now() - f.mtime) / 3600000,
    }));
  } catch {
    return [];
  }
}

/**
 * Best-effort title for a transcript. Reads at most the first ~256 KB — the
 * `ai-title` record is written early and rewritten as the session evolves, so
 * the head is enough for a picker label without paying for a huge file.
 */
function readTranscriptTitle(file) {
  let head = '';
  try {
    const fd = openSync(file, 'r');
    try {
      const buf = Buffer.alloc(262144);
      const n = readSync(fd, buf, 0, buf.length, 0);
      head = buf.toString('utf8', 0, n);
    } finally { closeSync(fd); }
  } catch { return ''; }

  let fallback = '';
  // Drop the last (possibly truncated) line before parsing.
  const lines = head.split('\n');
  lines.pop();
  for (const line of lines) {
    if (!line || (!line.includes('ai-title') && !line.includes('last-prompt') && !line.includes('"user"'))) continue;
    let o;
    try { o = JSON.parse(line); } catch { continue; }
    if (o.type === 'ai-title' && o.aiTitle) return String(o.aiTitle).slice(0, 120);
    if (!fallback && o.type === 'last-prompt' && o.lastPrompt) fallback = String(o.lastPrompt);
    if (!fallback && o.type === 'user' && o.message) {
      const c = o.message.content;
      const t = typeof c === 'string' ? c : (Array.isArray(c) ? (c.find(b => b?.type === 'text') || {}).text : '');
      if (t) fallback = String(t);
    }
  }
  return fallback.replace(/\s+/g, ' ').trim().slice(0, 120);
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
        skipPermissions: !!s.skipPermissions,
        sessionId: s.sessionId || '',
        pending: [...s.pending.values()].map(p => p.entry),
      });
    }
    out.sort((a, b) => a.project === b.project ? (a.order - b.order) : a.project.localeCompare(b.project));
    return out;
  }

  const entriesForAlias = (key) => [...sessions.values()].filter(s => s.alias === key);

  // ─── Transcript persistence ───
  //
  // Sessions live in memory, so a server restart used to lose the conversation
  // even though it plainly happened. Mirror each one to disk so history can be
  // read back afterwards. Writes are debounced and atomic (tmp + rename) so a
  // crash mid-write can't leave a truncated file that fails to parse.
  const HISTORY_DIR = join(config.dataDir, 'chat-history');
  const PERSIST_MS = 1500;
  // Interactive entries are dropped: replaying a permission prompt whose
  // decision the CLI already consumed would render buttons that do nothing.
  const PERSIST_KINDS = new Set(['user', 'assistant-text', 'thinking', 'tool', 'result', 'error']);

  function historyFile(id) { return join(HISTORY_DIR, encodeURIComponent(id) + '.json'); }

  // Only a tail is kept. A long agentic session can run to thousands of
  // entries with large tool payloads; writing all of it on every patch would
  // cost real I/O, and replaying it would stall the renderer. Bounded by count
  // AND serialized bytes, since a handful of big tool results can blow the
  // budget well before the count does.
  const PERSIST_MAX_MESSAGES = 200;
  const PERSIST_MAX_BYTES = 256 * 1024;

  function persistNow(s) {
    clearTimeout(s.persistTimer);
    s.persistTimer = null;
    try {
      mkdirSync(HISTORY_DIR, { recursive: true });
      const head = {
        id: s.id, project: s.alias, title: s.title, cwd: s.cwd,
        sessionId: s.sessionId, model: s.model,
        permissionMode: s.permissionMode, skipPermissions: !!s.skipPermissions,
        totalCostUsd: s.totalCostUsd, savedAt: new Date().toISOString(),
      };
      let msgs = s.messages.filter(m => PERSIST_KINDS.has(m.kind)).slice(-PERSIST_MAX_MESSAGES);
      let json = JSON.stringify({ ...head, messages: msgs });
      // Drop from the front until it fits — oldest is the cheapest to lose.
      while (json.length > PERSIST_MAX_BYTES && msgs.length > 1) {
        msgs = msgs.slice(Math.max(1, Math.floor(msgs.length * 0.25)));
        json = JSON.stringify({ ...head, messages: msgs });
      }
      const tmp = historyFile(s.id) + '.tmp';
      writeFileSync(tmp, json);
      renameSync(tmp, historyFile(s.id));
    } catch { /* disk full / permissions — history is best-effort, never fatal */ }
  }

  function schedulePersist(s) {
    if (s.persistTimer) return;
    s.persistTimer = setTimeout(() => persistNow(s), PERSIST_MS);
    if (s.persistTimer.unref) s.persistTimer.unref();
  }

  /** Read a stored transcript back. Returns null when there isn't one. */
  function loadPersisted(id) {
    try {
      const f = historyFile(id);
      if (!existsSync(f)) return null;
      const d = JSON.parse(readFileSync(f, 'utf8'));
      return (d && Array.isArray(d.messages)) ? d : null;
    } catch { return null; }
  }

  function dropPersisted(id) {
    try { if (existsSync(historyFile(id))) unlinkSync(historyFile(id)); } catch { /* ignore */ }
  }

  /**
   * Bring stored transcripts back as exited sessions on boot, so a restart
   * leaves the conversation readable in place instead of blanking the cell.
   * They own no process and can't be sent to — the UI shows them as ended.
   * Bounded by age and count so the workbench isn't repopulated with months
   * of dead chats.
   */
  const RESTORE_MAX = 20;
  const RESTORE_AGE_MS = 3 * 24 * 60 * 60 * 1000;
  function restorePersisted() {
    try {
      if (!existsSync(HISTORY_DIR)) return;
      const files = readdirSync(HISTORY_DIR)
        .filter(f => f.endsWith('.json'))
        .map(f => { const p = join(HISTORY_DIR, f); try { return { p, m: statSync(p).mtimeMs }; } catch { return null; } })
        .filter(Boolean)
        .filter(x => Date.now() - x.m < RESTORE_AGE_MS)
        .sort((a, b) => b.m - a.m)
        .slice(0, RESTORE_MAX);
      for (const { p } of files) {
        let d; try { d = JSON.parse(readFileSync(p, 'utf8')); } catch { continue; }
        if (!d || !d.id || !Array.isArray(d.messages) || sessions.has(d.id)) continue;
        sessions.set(d.id, {
          id: d.id, alias: d.project, title: d.title, cwd: d.cwd,
          proc: null, messages: d.messages, emitter: new EventEmitter(),
          state: 'idle', sessionId: d.sessionId, model: d.model,
          permissionMode: d.permissionMode, skipPermissions: !!d.skipPermissions,
          slashCommands: [], totalCostUsd: d.totalCostUsd || 0,
          pending: new Map(), blocks: new Map(), streamedQueue: [],
          order: 0, restored: true, persistTimer: null,
        });
      }
      if (files.length) console.log(`[claude-ui] Restored ${files.length} chat transcript(s)`);
    } catch { /* never block startup on this */ }
  }
  restorePersisted();

  /** Append a conversation entry and notify subscribers. */
  function emitEntry(s, entry) {
    entry.seq = nextSeq();
    s.messages.push(entry);
    if (s.messages.length > MAX_MESSAGES) s.messages.splice(0, s.messages.length - MAX_MESSAGES);
    s.emitter.emit('event', { type: 'entry', entry });
    schedulePersist(s);
    return entry;
  }

  /** Patch an existing entry in place (tool results, streamed text, answers). */
  function patchEntry(s, entry, patch) {
    Object.assign(entry, patch);
    s.emitter.emit('event', { type: 'patch', id: entry.id, patch });
    schedulePersist(s); // streamed text lands via patch, not emitEntry
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
   * @param {string} opts.permissionMode  default|acceptEdits|plan|auto|dontAsk ('' = respect the user's configured defaultMode)
   * @param {boolean} opts.skipPermissions  launch with --dangerously-skip-permissions
   * @param {string} opts.sessionMode     'continue' | 'new' | 'resume'
   * @param {string} opts.resumeId        session id when sessionMode==='resume'
   */
  async function create(alias, {
    title = '', model = '', effort = '', permissionMode = '',
    skipPermissions = false, sessionMode = null, resumeId = '',
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
    ];
    // bypassPermissions cannot be switched on later — the CLI rejects
    // set_permission_mode for it unless the session was LAUNCHED this way
    // ("Cannot set permission mode to bypassPermissions because the session was
    // not launched with --dangerously-skip-permissions"). So it is a launch
    // choice, mirroring terminal mode's Skip Permissions option.
    if (skipPermissions) args.push('--dangerously-skip-permissions');
    // Only pin a mode when one was actually chosen; otherwise let the CLI apply
    // the user's own permissions.defaultMode from settings.
    else if (permissionMode) args.push('--permission-mode', permissionMode);
    if (model) args.push('--model', String(model));
    if (effort) args.push('--effort', String(effort));
    // Same resume policy as terminal mode: only pass --continue when a prior
    // transcript exists, or the CLI exits immediately with "No conversation found".
    // Session continuity, mirroring claude-terminals.js: resume an explicit id,
    // or pick up the most recent conversation by default. Only the FIRST live
    // chat for a project continues — extra chats start fresh rather than two
    // processes clobbering the same transcript. `sessionMode: 'new'` opts out.
    const aliasHasLive = entriesForAlias(key).some(x => x.proc);
    // 'compact' resumes and then immediately runs /compact, so the heavy context
    // is summarised once instead of riding along on every later turn.
    const compacting = sessionMode === 'compact';
    if ((sessionMode === 'resume' || compacting) && resumeId) args.push('--resume', String(resumeId));
    else if (sessionMode === 'new') { /* explicit fresh session */ }
    else if ((sessionMode === 'continue' || compacting || !aliasHasLive) && hasExistingConversation(project.path)) args.push('--continue');

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
      // Provisional: the initialize response reports the mode actually in
      // effect (including one that came from the user's settings).
      permissionMode: skipPermissions ? 'bypassPermissions' : (permissionMode || 'default'),
      skipPermissions: !!skipPermissions,
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
    // preset. The response carries the slash commands, model list and effective
    // permission mode, and lands a couple of seconds after spawn — well before
    // the first turn — so we use it to mark the session ready.
    s.initRequestId = 'init-' + genId();
    s.resuming = args.includes('--resume') || args.includes('--continue');
    send(s, {
      type: 'control_request',
      request_id: s.initRequestId,
      request: { subtype: 'initialize', systemPrompt: [''] },
    });

    // Spawning takes a moment, so say so rather than showing an empty cell.
    // This is retired the instant the initialize response arrives — it must
    // never read as an operation still in progress, because the CLI does no
    // work at all until the first message is sent.
    s.startupNotice = emitEntry(s, { id: genId(), kind: 'notice', text: 'Starting Claude…' });

    // Fired as the first turn once the CLI reports ready (see handleControlResponse).
    if (compacting) s.pendingFirstMessage = '/compact';

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
      // The CLI is live — retire the "starting/resuming" placeholder.
      if (s.startupNotice) {
        patchEntry(s, s.startupNotice, { kind: 'gone', text: '' });
        s.startupNotice = null;
      }
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
      // Opus 4.7+ withholds reasoning text — `thinking.display` defaults to
      // "omitted", and on Opus 5 the raw chain of thought is never returned at
      // all. Those blocks arrive with `thinking: ""` but a populated
      // `estimated_tokens`, which is the only progress signal they carry.
      if (d.type === 'thinking_delta' && typeof d.estimated_tokens === 'number'
          && d.estimated_tokens > (entry.tokens || 0)) {
        patchEntry(s, entry, { tokens: d.estimated_tokens });
      }
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
    if (r?.subtype === 'error') {
      console.warn(`[claude-ui] (${s.id}) control error: ${r.error}`);
      // A rejected mode change used to fail silently server-side while the
      // dropdown kept showing the new mode — the UI then lied about what was
      // actually in effect. Roll back and say so.
      const pend = s.pendingModeChange;
      if (pend && pend.requestId === r.request_id) {
        s.pendingModeChange = null;
        s.permissionMode = pend.previous;
        emitEntry(s, { id: genId(), kind: 'error', text: 'Could not switch permission mode: ' + r.error });
        s.emitter.emit('event', { type: 'meta', permissionMode: s.permissionMode });
        return;
      }
      if (r.request_id === s.initRequestId) {
        s.initRequestId = null;
        if (s.startupNotice) { patchEntry(s, s.startupNotice, { text: 'Claude failed to start: ' + r.error, kind: 'error' }); s.startupNotice = null; }
      }
      return;
    }
    if (r?.subtype !== 'success') return;
    if (s.pendingModeChange && s.pendingModeChange.requestId === r.request_id) { s.pendingModeChange = null; return; }
    if (r.request_id !== s.initRequestId) return;

    // The session is up. The CLI stays completely idle until the first user
    // message (system/init, and therefore the session id, only arrive with that
    // first turn), so retire the "starting" line now instead of leaving a
    // placeholder that looks like a stalled operation.
    s.initRequestId = null;
    const resp = r.response || {};
    s.slashCommands = resp.commands || s.slashCommands;
    if (resp.current_permission_mode) s.permissionMode = resp.current_permission_mode;
    if (s.startupNotice) {
      // Worth stating plainly: --continue/--resume restores Claude's context but
      // does NOT replay the old messages into this log, so the empty cell is
      // expected rather than a sign the resume failed.
      if (s.resuming) patchEntry(s, s.startupNotice, { text: 'Continuing your previous conversation. Earlier messages are not shown here, but Claude still has them.' });
      else patchEntry(s, s.startupNotice, { kind: 'gone', text: '' });
      s.startupNotice = null;
    }
    s.emitter.emit('event', {
      type: 'init',
      sessionId: s.sessionId,          // still empty until the first turn
      model: s.model,
      slashCommands: s.slashCommands,
      permissionMode: s.permissionMode,
      account: resp.account || null,
    });

    // Auto-run the queued opener (currently only /compact) now that the CLI is
    // accepting input.
    if (s.pendingFirstMessage) {
      const text = s.pendingFirstMessage;
      s.pendingFirstMessage = null;
      sendMessage(s.id, text);
    }
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
    // Verified at runtime: acceptEdits / plan / dontAsk / auto all switch fine;
    // bypassPermissions is rejected unless the session was launched for it.
    const valid = ['default', 'acceptEdits', 'plan', 'dontAsk', 'auto'];
    if (mode === 'bypassPermissions' && !s.skipPermissions) {
      return { ok: false, error: 'Bypass permissions can only be chosen when the chat is launched — start a new "UI Mode — Skip Permissions" chat.' };
    }
    if (!valid.includes(mode) && mode !== 'bypassPermissions') return { ok: false, error: `Invalid permission mode "${mode}"` };
    const reqId = 'pm-' + genId();
    s.pendingModeChange = { requestId: reqId, mode, previous: s.permissionMode };
    send(s, { type: 'control_request', request_id: reqId, request: { subtype: 'set_permission_mode', mode } });
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
    clearTimeout(s.persistTimer);
    sessions.delete(id);
    // Closing a cell is the user saying they're done with it, so the stored
    // transcript goes too — otherwise it would come back on the next restart.
    dropPersisted(id);
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
    if (!s) {
      // Not in memory — most often because the server restarted. Serve the
      // stored transcript so the conversation is still readable, flagged as
      // exited so the UI doesn't offer to send into a process that is gone.
      const d = loadPersisted(id);
      if (!d) return null;
      return {
        id: d.id, project: d.project, title: d.title, kind: 'ui',
        status: 'exited', state: 'idle',
        sessionId: d.sessionId, model: d.model,
        permissionMode: d.permissionMode, skipPermissions: !!d.skipPermissions,
        slashCommands: [], totalCostUsd: d.totalCostUsd, cwd: d.cwd,
        messages: d.messages, restored: true,
      };
    }
    return {
      id: s.id, project: s.alias, title: s.title, kind: 'ui',
      status: s.proc ? 'running' : 'exited',
      state: s.state, sessionId: s.sessionId, model: s.model,
      permissionMode: s.permissionMode, skipPermissions: !!s.skipPermissions,
      slashCommands: s.slashCommands,
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
