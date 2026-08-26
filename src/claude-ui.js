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
import { hasExistingConversation, writeMcpConfig } from './claude-terminals.js';

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
  // One transcript per project — the last UI conversation, nothing more. It
  // exists for a single job: when a chat reattaches to that conversation with
  // --continue, show what was already said before the user types.
  //
  // Writes are debounced and atomic (tmp + rename) so a crash mid-write can't
  // leave a truncated file that fails to parse.
  const HISTORY_DIR = join(config.dataDir, 'chat-history');
  const PERSIST_MS = 1500;
  // Interactive entries are dropped: replaying a permission prompt whose
  // decision the CLI already consumed would render buttons that do nothing.
  const PERSIST_KINDS = new Set(['user', 'assistant-text', 'thinking', 'tool', 'result', 'error']);
  // Only a tail is kept. A long agentic session runs to thousands of entries
  // with large tool payloads; writing all of it on every patch would cost real
  // I/O and replaying it would stall the renderer. Bounded by count AND
  // serialized bytes, since a few big tool results blow the byte budget well
  // before the count fills.
  const PERSIST_MAX_MESSAGES = 200;
  const PERSIST_MAX_BYTES = 256 * 1024;
  // Subagent transcripts are stored too, so reopening a project still opens
  // the bubbles. They are the first thing sacrificed when the file overruns:
  // a subagent's working-out is the least valuable part of "what was said".
  const PERSIST_MAX_AGENTS = 6;
  const PERSIST_MAX_AGENT_MESSAGES = 40;

  function historyFile(alias) { return join(HISTORY_DIR, encodeURIComponent(alias) + '.json'); }

  function persistNow(s) {
    clearTimeout(s.persistTimer);
    s.persistTimer = null;
    if (!s.sessionId) return; // no conversation identity yet — nothing to pin it to
    // A freshly spawned session holds nothing but a startup notice. With one
    // file per project, writing that would wipe the transcript we are about to
    // replay, so hold off until there is something real to record.
    if (!s.messages.some(m => PERSIST_KINDS.has(m.kind))) return;
    try {
      mkdirSync(HISTORY_DIR, { recursive: true });
      const head = {
        uuid: s.sessionId, project: s.alias, title: s.title, cwd: s.cwd,
        model: s.model, savedAt: Date.now(),
      };
      let msgs = s.messages.filter(m => PERSIST_KINDS.has(m.kind)).slice(-PERSIST_MAX_MESSAGES);
      let agents = [...s.agents.values()].slice(-PERSIST_MAX_AGENTS).map(a => ({
        ...a, messages: a.messages.slice(-PERSIST_MAX_AGENT_MESSAGES),
      }));
      let json = JSON.stringify({ ...head, messages: msgs, agents });
      // Shed the agent transcripts first, then trim conversation from the
      // front — oldest is the cheapest to lose.
      if (json.length > PERSIST_MAX_BYTES && agents.length) {
        agents = agents.map(a => ({ ...a, messages: [] }));
        json = JSON.stringify({ ...head, messages: msgs, agents });
      }
      while (json.length > PERSIST_MAX_BYTES && msgs.length > 1) {
        msgs = msgs.slice(Math.max(1, Math.floor(msgs.length * 0.25)));
        json = JSON.stringify({ ...head, messages: msgs, agents });
      }
      const tmp = historyFile(s.alias) + '.tmp';
      writeFileSync(tmp, json);
      renameSync(tmp, historyFile(s.alias));
    } catch { /* disk full / permissions — history is best-effort, never fatal */ }
  }

  function schedulePersist(s) {
    if (s.persistTimer) return;
    s.persistTimer = setTimeout(() => persistNow(s), PERSIST_MS);
    if (s.persistTimer.unref) s.persistTimer.unref();
  }

  function loadPersisted(alias) {
    try {
      const f = historyFile(alias);
      if (!existsSync(f)) return null;
      const d = JSON.parse(readFileSync(f, 'utf8'));
      return (d && d.uuid && Array.isArray(d.messages)) ? d : null;
    } catch { return null; }
  }

  /**
   * Forget a project's stored transcript.
   *
   * Called when a TERMINAL session starts on that project: the user is about to
   * continue the conversation somewhere we can't observe, so anything we replay
   * afterwards would be a stale prefix presented as the whole story. Better to
   * show nothing. The next UI message rewrites the file, so it self-heals.
   */
  function clearHistory(alias) { clearPersisted(alias); }

  function clearPersisted(alias) {
    try { const f = historyFile(alias); if (existsSync(f)) unlinkSync(f); } catch { /* ignore */ }
  }

  /**
   * Render the stored conversation into a chat that is continuing it, before
   * the user types anything.
   *
   * The stored copy is only trustworthy while nothing else has touched that
   * conversation. If the user has since worked in a TERMINAL session, Claude's
   * own transcript has moved on and ours is a stale prefix — so the check is
   * simply whether that transcript still matches what we recorded. That needs
   * no hook into terminal input, and it self-corrects: the next UI message
   * rewrites the file and it works again.
   *
   * @param {object} s      session
   * @param {string} uuid   conversation the process is attaching to
   */
  function replayForContinue(s, uuid, authoritative = false) {
    // Guard on having actually spliced, not on having tried. The first attempt
    // runs against a GUESSED uuid; when init reveals a different one we retry,
    // and that retry must be able to proceed — but must never splice a second
    // copy if the first one landed.
    if (s.replaySpliced || s.replayTriedFor === uuid) return;
    s.replayTriedFor = uuid;
    const d = loadPersisted(s.alias);
    if (!d || !d.messages.length) return;
    if (d.uuid !== uuid) {
      // A mismatch against a GUESS proves nothing — the guess is just "the
      // newest transcript on disk", and --continue may well have attached
      // somewhere else. Deleting here destroyed the stored transcript AND left
      // the post-init retry nothing to find, so the retry could never succeed.
      // Only the uuid the CLI itself reports is grounds for discarding it.
      if (authoritative) clearPersisted(s.alias);
      return;
    }
    s.messages = [
      ...d.messages,
      { id: genId(), kind: 'notice', text: '— earlier messages in this conversation —', seq: nextSeq() },
      ...s.messages,
    ];
    for (const a of d.agents || []) {
      if (a && a.toolUseId && !s.agents.has(a.toolUseId)) {
        s.agents.set(a.toolUseId, { ...a, messages: a.messages || [] });
      }
    }
    s.replaySpliced = true;
    s.emitter.emit('event', { type: 'history', session: snapshot(s) });
    console.log(`[claude-ui] Replayed ${d.messages.length} stored entries for "${s.alias}"`);
  }

  // ─── Subagents ───
  //
  // When Claude launches a Task/Agent subagent, the CLI streams that agent's
  // OWN turns up the same pipe, tagged with `parent_tool_use_id` = the id of
  // the tool_use that spawned it. Verified on the wire: two parallel Explore
  // agents produced nine `assistant` messages and six `user` messages, all
  // carrying a parent id, interleaved with each other and with the main thread.
  //
  // Treating those as main-thread messages was wrong twice over. They rendered
  // in the transcript as if the user's own Claude had said them, and every
  // text/thinking block among them consumed a slot from streamedQueue — which
  // belongs to the MAIN thread — so a subagent's reasoning could overwrite the
  // text the user was watching stream in. Both faults are fixed by routing on
  // the parent id before any of that machinery runs.
  //
  // Partial-message deltas are always main-thread (parent id null on every
  // stream_event observed), so blocks/streamedQueue stay single-threaded and
  // need no partitioning — a subagent's text arrives only as complete messages.

  const MAX_AGENT_MESSAGES = 150;
  // A long session can spawn a great many subagents, and each one holds its own
  // transcript, so the count needs a ceiling of its own — MAX_AGENT_MESSAGES
  // only bounds a single agent. Finished agents are dropped oldest-first; a
  // running one is never dropped, since its bubble is still on screen.
  const MAX_AGENTS = 40;
  // Agent tool output is supporting detail, not the conversation, so it is kept
  // far tighter than the main transcript's MAX_TEXT (200k chars each, which
  // across 40 agents x 150 entries is a memory problem rather than a feature).
  const MAX_AGENT_TEXT = 20_000;

  function trimAgents(s) {
    if (s.agents.size <= MAX_AGENTS) return;
    const drop = (id, a) => {
      s.agents.delete(id);
      if (a.taskId) s.agentsByTask.delete(a.taskId);
    };
    for (const [id, a] of s.agents) {
      if (s.agents.size <= MAX_AGENTS) break;
      if (a.status === 'running') continue;
      drop(id, a);
    }
    // An agent whose terminal status never arrives (CLI killed mid-task) stays
    // 'running' forever and would otherwise be permanently exempt, so the cap
    // could be exceeded without limit. Past twice the cap, age wins.
    if (s.agents.size > MAX_AGENTS * 2) {
      for (const [id, a] of s.agents) {
        if (s.agents.size <= MAX_AGENTS) break;
        drop(id, a);
      }
    }
  }

  /** Get (or lazily create) the record for the subagent behind a tool_use id. */
  function getAgent(s, toolUseId) {
    let a = s.agents.get(toolUseId);
    if (!a) {
      a = {
        toolUseId, taskId: '', description: '', step: '', subagentType: '', prompt: '',
        status: 'running', summary: '', usage: null, lastTool: '',
        startedAt: Date.now(), endedAt: 0, messages: [],
      };
      s.agents.set(toolUseId, a);
      trimAgents(s);
    }
    return a;
  }

  /** Strip the transcript down to what the client needs to draw a bubble. */
  function agentMeta(a) {
    const { messages, ...meta } = a;
    return { ...meta, count: messages.length };
  }

  function emitAgent(s, a) {
    s.emitter.emit('event', { type: 'agent', agent: agentMeta(a) });
    schedulePersist(s);
  }

  function agentEntry(s, a, entry) {
    entry.seq = nextSeq();
    a.messages.push(entry);
    if (a.messages.length > MAX_AGENT_MESSAGES) {
      a.messages.splice(0, a.messages.length - MAX_AGENT_MESSAGES);
    }
    s.emitter.emit('event', { type: 'agent-entry', toolUseId: a.toolUseId, entry });
    schedulePersist(s);
    return entry;
  }

  function agentPatch(s, a, entry, patch) {
    Object.assign(entry, patch);
    s.emitter.emit('event', { type: 'agent-patch', toolUseId: a.toolUseId, id: entry.id, patch });
    schedulePersist(s);
  }

  /** A subagent's own assistant turn. */
  function handleAgentAssistant(s, msg, toolUseId) {
    const a = getAgent(s, toolUseId);
    for (const block of msg.message?.content || []) {
      if (block.type === 'text' || block.type === 'thinking') {
        agentEntry(s, a, {
          id: genId(),
          kind: block.type === 'text' ? 'assistant-text' : 'thinking',
          text: (block.type === 'text' ? block.text : block.thinking) || '',
          tokens: 0,
        });
      } else if (block.type === 'tool_use') {
        agentEntry(s, a, {
          id: genId(), kind: 'tool', toolUseId: block.id, name: block.name,
          input: block.input || {}, status: 'running', result: null, isError: false,
        });
      }
    }
  }

  /** A subagent's tool results (and its opening prompt, which we skip). */
  function handleAgentUser(s, msg, toolUseId) {
    const a = getAgent(s, toolUseId);
    for (const block of msg.message?.content || []) {
      if (block.type !== 'tool_result') continue;
      const target = [...a.messages].reverse().find(e => e.kind === 'tool' && e.toolUseId === block.tool_use_id);
      if (!target) continue;
      const content = Array.isArray(block.content)
        ? block.content.map(c => (c.type === 'text' ? c.text : `[${c.type}]`)).join('\n')
        : (typeof block.content === 'string' ? block.content : JSON.stringify(block.content ?? ''));
      agentPatch(s, a, target, {
        status: 'done', result: String(content).slice(0, MAX_AGENT_TEXT), isError: !!block.is_error,
      });
    }
  }

  /**
   * Agent lifecycle, reported as `system` messages alongside the transcript:
   * task_started (id, description, subagent_type, prompt), task_progress
   * (running commentary + token usage), task_updated (status patch, keyed by
   * task_id only) and task_notification (final status, summary, usage).
   */
  function handleTaskEvent(s, msg) {
    const toolUseId = msg.tool_use_id || s.agentsByTask.get(msg.task_id) || '';
    if (!toolUseId) return;
    const a = getAgent(s, toolUseId);
    if (msg.task_id) { a.taskId = msg.task_id; s.agentsByTask.set(msg.task_id, toolUseId); }

    if (msg.subtype === 'task_started') {
      a.description = msg.description || a.description;
      a.subagentType = msg.subagent_type || a.subagentType;
      a.prompt = String(msg.prompt || a.prompt).slice(0, MAX_TEXT);
      a.status = 'running';
    } else if (msg.subtype === 'task_progress') {
      // task_progress reuses `description` for the CURRENT STEP ("Reading
      // README.md"), not the agent's job. Overwriting the title with it left
      // every bubble showing the same passing detail and no way to tell the
      // agents apart, so the step is kept separately.
      if (msg.description) a.step = msg.description;
      if (msg.subagent_type) a.subagentType = msg.subagent_type;
      if (msg.last_tool_name) a.lastTool = msg.last_tool_name;
      if (msg.usage) a.usage = msg.usage;
    } else if (msg.subtype === 'task_updated') {
      const patch = msg.patch || {};
      if (patch.status) a.status = patch.status;
      if (patch.end_time) a.endedAt = patch.end_time;
    } else if (msg.subtype === 'task_notification') {
      if (msg.status) a.status = msg.status;
      if (msg.summary) a.summary = String(msg.summary).slice(0, MAX_TEXT);
      if (msg.usage) a.usage = msg.usage;
      if (!a.endedAt) a.endedAt = Date.now();
    }
    emitAgent(s, a);
  }

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
    // Make sure the project's Crundi MCP server is configured before spawning.
    // Terminal mode does this on every launch and index.js does it for every
    // project at boot, but chat mode did neither — so a project added AFTER
    // startup and only ever opened as a chat had no .mcp.json, and none of the
    // Crundi tools. Verified the tools do work once it exists. No-ops in dev.
    try { writeMcpConfig(project.path, apiUrl, apiKey, key); } catch { /* non-fatal */ }

    const aliasHasLive = entriesForAlias(key).some(x => x.proc);
    // 'compact' resumes and then immediately runs /compact, so the heavy context
    // is summarised once instead of riding along on every later turn.
    const compacting = sessionMode === 'compact';
    // Work out WHICH conversation this process will attach to, before it runs.
    // The CLI only reveals its session uuid via system/init, which it emits
    // lazily on the first turn — far too late to show the user what was already
    // said. --resume names the uuid outright, and --continue attaches to the
    // newest transcript, which is exactly what latestTranscript() reports.
    let continueUuid = '';
    if ((sessionMode === 'resume' || compacting) && resumeId) {
      // Explicitly picking a session from the resume list is a deliberate jump
      // to a named conversation — deliberately NOT replayed, so it behaves the
      // way it always has.
      args.push('--resume', String(resumeId));
      // Compacting is the exception. It is not a jump elsewhere: it is THIS
      // conversation, summarised, reached from the launch prompt's "compact
      // first" button. So the stored transcript still describes it and must be
      // replayed — otherwise choosing to compact silently threw away the very
      // history the replay exists to show.
      if (compacting) continueUuid = String(resumeId);
    } else if (sessionMode === 'new') { /* explicit fresh session */ }
    else if ((sessionMode === 'continue' || compacting || !aliasHasLive) && hasExistingConversation(project.path)) {
      args.push('--continue');
      const t = latestTranscript(project.path);
      if (t) continueUuid = t.id;
    }

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
      agents: new Map(),       // Task tool_use_id -> subagent record (see handleTaskEvent)
      agentsByTask: new Map(), // CLI task_id -> tool_use_id (task_updated only carries task_id)
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

    // Show what was already said BEFORE the user types anything — the whole
    // point is to walk into a resumed conversation already knowing where it
    // left off. handleSystem re-runs this with the CLI's authoritative uuid in
    // case --continue landed somewhere other than the newest transcript.
    if (continueUuid) { s.continueUuid = continueUuid; s.sessionId = continueUuid; replayForContinue(s, continueUuid); }

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
      const guessed = s.sessionId;
      s.sessionId = msg.session_id || s.sessionId;
      // Pre-spawn we can only infer which conversation --continue lands on. If
      // it went elsewhere, redo the replay against the real uuid.
      if (s.continueUuid && guessed && s.sessionId !== guessed) {
        replayForContinue(s, s.sessionId, true);
      }
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
    } else if (msg.subtype === 'task_started' || msg.subtype === 'task_progress'
            || msg.subtype === 'task_updated' || msg.subtype === 'task_notification') {
      handleTaskEvent(s, msg);
    } else if (msg.subtype === 'status' && msg.permissionMode
               && msg.permissionMode !== s.permissionMode) {
      // The CLI changes mode on its own too — accepting a plan drops out of
      // plan mode. Without this the picker would keep showing the old one.
      s.permissionMode = msg.permissionMode;
      s.emitter.emit('event', { type: 'meta', permissionMode: s.permissionMode });
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
    // A subagent's own turn — never the main transcript, and crucially never
    // allowed near streamedQueue (see the Subagents section above).
    if (msg.parent_tool_use_id) return handleAgentAssistant(s, msg, msg.parent_tool_use_id);
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
    if (msg.parent_tool_use_id) return handleAgentUser(s, msg, msg.parent_tool_use_id);
    for (const block of msg.message?.content || []) {
      if (block.type === 'text') { handleHookFeedback(s, block.text || ''); continue; }
      if (block.type !== 'tool_result') continue;
      const target = [...s.messages].reverse().find(e => e.kind === 'tool' && e.toolUseId === block.tool_use_id);
      const content = Array.isArray(block.content)
        ? block.content.map(c => (c.type === 'text' ? c.text : `[${c.type}]`)).join('\n')
        : (typeof block.content === 'string' ? block.content : JSON.stringify(block.content ?? ''));
      if (target) patchEntry(s, target, { status: 'done', result: String(content).slice(0, MAX_TEXT), isError: !!block.is_error });
    }
  }

  // ─── Goal mode ───
  //
  // `/goal <condition>` puts the CLI into an autonomous loop. Verified on the
  // wire: it is NOT implemented as self-driven turns — the whole loop is a
  // single `result` (turns=7 in the probe), and the evaluator pushes Claude
  // onward through a STOP HOOK, which arrives as a plain `user` text message:
  //
  //     Stop hook feedback:
  //     [<the goal condition>]: <why it is not yet met>
  //
  // handleUser only ever looked at tool_result blocks, so these were dropped
  // outright: the user watched Claude carry on turn after turn with nothing on
  // screen explaining why. Surfacing them is what makes goal mode legible.

  const GOAL_FEEDBACK_RE = /^Stop hook feedback:\s*\n\[([\s\S]*?)\]:\s*([\s\S]*)$/;

  function goalSnapshot(s) {
    return s.goal ? { ...s.goal } : null;
  }

  function emitGoal(s) {
    s.emitter.emit('event', { type: 'goal', goal: goalSnapshot(s) });
  }

  /**
   * Read an outgoing user turn for goal commands so the banner appears the
   * moment the goal is set, rather than waiting for the first verdict.
   */
  function noteGoalCommand(s, text) {
    const t = String(text || '').trim();
    if (!/^\/goal\b/.test(t)) return;
    const arg = t.slice(5).trim();
    if (!arg) return;                       // bare `/goal` is a status query
    if (/^clear$/i.test(arg)) {
      if (s.goal) { s.goal = null; emitGoal(s); }
      return;
    }
    s.goal = { condition: arg.slice(0, 2000), verdicts: 0, lastVerdict: '', looping: true, startedAt: Date.now() };
    emitGoal(s);
  }

  /** A hook injected feedback mid-turn. Only goal verdicts are surfaced. */
  function handleHookFeedback(s, text) {
    const m = GOAL_FEEDBACK_RE.exec(String(text || '').trim());
    if (!m) return;   // some other Stop hook — not ours to interpret
    const condition = m[1].trim();
    const reason = m[2].trim();
    // Adopt the condition if the goal was set before this session was watching
    // (a resumed conversation still mid-goal).
    if (!s.goal) s.goal = { condition, verdicts: 0, lastVerdict: '', looping: true, startedAt: Date.now() };
    s.goal.verdicts += 1;
    s.goal.lastVerdict = reason.slice(0, MAX_TEXT);
    s.goal.looping = true;
    emitEntry(s, {
      id: genId(), kind: 'goal-verdict', text: reason.slice(0, MAX_TEXT),
      condition, n: s.goal.verdicts,
    });
    emitGoal(s);
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
    // The whole goal loop lives inside ONE result, so a result means the loop
    // has stopped — met, abandoned, or needing input. We deliberately do NOT
    // claim it was met: nothing on the wire says so, and inventing a verdict
    // we never observed is exactly the kind of lie that erodes trust in it.
    if (s.goal && s.goal.looping) { s.goal.looping = false; emitGoal(s); }
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
      // replayForContinue() shows whatever we stored for this conversation; anything
      // older than that (or from before transcripts were kept) is still in
      // Claude's context even though it isn't rendered here.
      if (s.resuming) patchEntry(s, s.startupNotice, { text: 'Continuing your previous conversation. Claude has the full context; only recent messages are shown here.' });
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
    noteGoalCommand(s, body);
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
    // Verified at runtime: acceptEdits / plan / dontAsk / auto all switch fine,
    // INCLUDING out of and back into bypassPermissions. Only switching INTO
    // bypass is gated, and only for sessions not launched for it.
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
    // FLUSH, don't cancel. Writes are debounced by 1.5s, so cancelling here
    // threw away everything said in the last moment before the cell was
    // closed — and on a short exchange that is the entire conversation, which
    // is precisely what the stored transcript exists to keep.
    persistNow(s);
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
    return s ? snapshot(s) : null;
  }

  /** The full client-facing view of a session. */
  function snapshot(s) {
    return {
      id: s.id, project: s.alias, title: s.title, kind: 'ui',
      status: s.proc ? 'running' : 'exited',
      state: s.state, sessionId: s.sessionId, model: s.model,
      permissionMode: s.permissionMode, skipPermissions: !!s.skipPermissions,
      slashCommands: s.slashCommands,
      totalCostUsd: s.totalCostUsd, cwd: s.cwd,
      messages: s.messages,
      agents: [...s.agents.values()],
      goal: goalSnapshot(s),
    };
  }

  /**
   * The final assistant message of the turn that just finished, for the
   * Telegram ping — "Claude finished" says nothing you could act on, whereas
   * what it actually concluded usually does.
   *
   * Scoped to the CURRENT turn by walking back only as far as the last user
   * entry. A turn that ended without saying anything (interrupted, or pure tool
   * work) must report nothing rather than re-sending text from an earlier turn,
   * which would read as a fresh answer to the message you just sent.
   */
  function lastTurnOutput(id) {
    const s = sessions.get(id);
    if (!s) return '';
    for (let i = s.messages.length - 1; i >= 0; i--) {
      const m = s.messages[i];
      if (m.kind === 'user') return '';   // turn boundary — nothing was said
      if (m.kind === 'assistant-text') {
        const t = String(m.text || '').trim();
        if (t) return t;
      }
    }
    return '';
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
    list, create, close, closeProject, closeAll, rename, setOrder, clearHistory,
    sendMessage, respond, interrupt, setPermissionMode, setModel,
    history, has, on, off, onAnyStateChange, lastTurnOutput,
    set apiUrl(v) { apiUrl = v; },
    get apiUrl() { return apiUrl; },
    set apiKey(v) { apiKey = v; },
    get apiKey() { return apiKey; },
  };
}
