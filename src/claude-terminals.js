/**
 * claude-terminals.js — Claude Code CLI PTY manager for Crundi.
 *
 * Spawns interactive `claude` CLI sessions via node-pty, one per project.
 * Manages lifecycle, output buffering, WebSocket streaming, and
 * idle-prompt detection for Telegram notifications.
 */

import { createRequire } from 'module';
import { EventEmitter } from 'events';
import { existsSync, writeFileSync, readFileSync, mkdirSync, readdirSync } from 'fs';
import { join, dirname, resolve as resolvePath } from 'path';
import { homedir } from 'os';
import { fileURLToPath } from 'url';
import { getProject } from './project-store.js';

const __dirname = dirname(fileURLToPath(import.meta.url));

const require = createRequire(import.meta.url);
let pty;
try { pty = require('node-pty'); }
catch (err) { console.warn('[claude-terminals] node-pty unavailable:', err?.message || err); }

const MAX_SCROLLBACK = 100_000;    // characters of scrollback per terminal
const isWin = process.platform === 'win32';

/**
 * Comprehensive ANSI/VT escape code stripping.
 * Handles CSI, OSC (BEL and ST terminated), DCS, single-char escapes, and control chars.
 */
export function stripAnsi(str) {
  return str
    .replace(/\x1b\[[0-9;?]*[a-zA-Z]/g, '')              // CSI sequences
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, '')   // OSC sequences (BEL or ST)
    .replace(/\x1bP[^\x1b]*\x1b\\/g, '')                  // DCS sequences
    .replace(/\x1b[()][A-Z0-9]/g, '')                     // Character set selection
    .replace(/\x1b[>=<MNOP78]/g, '')                       // Mode/cursor escapes
    .replace(/\r/g, '')                                    // Carriage returns
    .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, '');   // Control chars (keep \n \t)
}

/**
 * Write .mcp.json into a project directory so Claude Code auto-discovers
 * the Crundi MCP server.
 */
function writeMcpConfig(projectPath, apiUrl, apiKey, projectAlias) {
  const mcpFile = join(projectPath, '.mcp.json');
  const mcpStdioPath = resolvePath(join(__dirname, 'mcp-stdio.js'));

  const mcpConfig = {
    mcpServers: {
      crundi: {
        command: 'node',
        args: [mcpStdioPath],
        env: {
          CRUNDI_API_URL: apiUrl,
          CRUNDI_API_KEY: apiKey,
          CRUNDI_PROJECT: projectAlias,
        },
      },
    },
  };

  // Merge with existing .mcp.json if present
  let existing = {};
  if (existsSync(mcpFile)) {
    try { existing = JSON.parse(readFileSync(mcpFile, 'utf-8')); } catch { /* ignore */ }
  }

  const merged = {
    ...existing,
    mcpServers: {
      ...(existing.mcpServers || {}),
      ...mcpConfig.mcpServers,
    },
  };

  writeFileSync(mcpFile, JSON.stringify(merged, null, 2));
  ensureGitignore(projectPath);
  console.log(`[claude-terminals] Wrote .mcp.json in ${projectPath}`);
}

/**
 * Ensure .claude/settings.local.json carries Crundi's lifecycle hooks so the
 * server can track each terminal's agent state. The hook command is shared by
 * every Claude in this folder; it identifies its terminal via the per-terminal
 * CRUNDI_TERMINAL_ID env var (injected at spawn). Existing user hooks are kept;
 * any stale Crundi hook entries are refreshed (path may change across updates).
 */
function writeHooksConfig(projectPath) {
  const dir = join(projectPath, '.claude');
  const file = join(dir, 'settings.local.json');
  // Forward slashes work on Windows node and avoid backslash escaping in JSON.
  const hookPath = resolvePath(join(__dirname, 'claude-hook.js')).replace(/\\/g, '/');
  const cmd = (state) => 'node "' + hookPath + '" ' + state;

  let cfg = {};
  if (existsSync(file)) { try { cfg = JSON.parse(readFileSync(file, 'utf-8')) || {}; } catch { cfg = {}; } }
  const hooks = cfg.hooks || {};
  const ensure = (event, state, matcher) => {
    // Drop any prior Crundi hook for this event, keep the user's others.
    let groups = (hooks[event] || []).map(g => ({
      ...g, hooks: (g.hooks || []).filter(h => !(h && typeof h.command === 'string' && h.command.includes('claude-hook.js'))),
    })).filter(g => (g.hooks || []).length);
    const entry = { hooks: [{ type: 'command', command: cmd(state) }] };
    if (matcher != null) entry.matcher = matcher;
    hooks[event] = [...groups, entry];
  };
  ensure('UserPromptSubmit', 'working');
  ensure('PreToolUse', 'working', '*');
  ensure('Notification', 'needs-input');
  ensure('Stop', 'idle');
  cfg.hooks = hooks;

  try {
    if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
    writeFileSync(file, JSON.stringify(cfg, null, 2));
  } catch { /* non-fatal */ }
}

/**
 * Add Crundi's local artifacts (.mcp.json and the crundi_attachments folder) to
 * .gitignore if a .gitignore exists and doesn't already list them.
 */
export function ensureGitignore(projectPath) {
  const gitignoreFile = join(projectPath, '.gitignore');
  if (!existsSync(gitignoreFile)) return;
  try {
    const content = readFileSync(gitignoreFile, 'utf-8');
    const lines = content.split('\n').map(l => l.trim());
    const wanted = ['.mcp.json', 'crundi_attachments/', '.claude/settings.local.json'];
    const missing = wanted.filter(w => !lines.includes(w) && !lines.includes(w.replace(/\/$/, '')));
    if (!missing.length) return;
    const nl = content.endsWith('\n') ? '' : '\n';
    writeFileSync(gitignoreFile, content + nl + missing.join('\n') + '\n');
  } catch { /* non-fatal */ }
}

/**
 * Check whether Claude Code has a prior conversation recorded for this
 * project path. Claude Code stores per-project session transcripts under
 * ~/.claude/projects/<encoded-path>/*.jsonl, where the path is encoded by
 * replacing every non-alphanumeric character with a dash
 * (e.g. C:\Projects\crundi → C--Projects-crundi).
 *
 * Passing `--continue` when no conversation exists makes the CLI exit
 * immediately with "No conversation found to continue", so we only add the
 * flag when at least one transcript is present.
 */
export function hasExistingConversation(projectPath) {
  try {
    const encoded = resolvePath(projectPath).replace(/[^a-zA-Z0-9]/g, '-');
    const sessionDir = join(homedir(), '.claude', 'projects', encoded);
    if (!existsSync(sessionDir)) return false;
    return readdirSync(sessionDir).some(f => f.endsWith('.jsonl'));
  } catch {
    return false;
  }
}

/** 16-char hex id for a terminal (project-independent). */
function genTermId() {
  let s = '';
  for (let i = 0; i < 16; i++) s += Math.floor(Math.random() * 16).toString(16);
  return s;
}

/**
 * Create the Claude terminal manager.
 *
 * Terminals are keyed by a unique id (not project), so a single project can
 * host several concurrent Claude sessions. Each entry remembers its project
 * `alias`, a user-editable `title`, and an `order` (for drag reordering).
 *
 * @param {{ apiUrl?: string, apiKey?: string }} opts
 *   apiUrl/apiKey are passed to MCP stdio servers via .mcp.json env vars.
 */
export function createClaudeTerminals({ apiUrl: initApiUrl, apiKey: initApiKey } = {}) {
  // id → { id, alias, title, order, proc, scrollback, emitter, cols, rows }
  const terminals = new Map();
  let apiUrl = initApiUrl;
  let apiKey = initApiKey;

  /**
   * List all terminals, sorted by project then display order.
   * @returns {{ id, project, title, order, status }[]}
   */
  function list() {
    const result = [];
    for (const t of terminals.values()) {
      result.push({
        id: t.id,
        project: t.alias,
        title: t.title,
        order: t.order,
        status: t.proc ? 'running' : 'exited',
      });
    }
    result.sort((a, b) => a.project === b.project ? (a.order - b.order) : a.project.localeCompare(b.project));
    return result;
  }

  /** All terminal entries belonging to a project alias. */
  function entriesForAlias(key) {
    return [...terminals.values()].filter(t => t.alias === key);
  }

  /**
   * Create a Claude Code terminal for a project. Multiple may coexist per
   * project; only the FIRST live terminal for a project resumes the prior
   * conversation (`--continue`), so additional terminals start fresh sessions
   * rather than fighting over the same transcript.
   */
  async function create(alias, {
    cols = 120, rows = 30, skipPermissions = false, shell: shellOnly = false, title,
    // Scheduler / agent extras:
    command: rawCommand = '',          // run an arbitrary command instead of `claude`
    prompt = '',                       // appended in double-quotes to the claude command
    sessionMode = null,                // 'continue' | 'new' | 'resume' (null = interactive default)
    resumeId = '',
    model = '',                        // --model <name> (e.g. opus); '' = omit
    effort = '',                       // --effort <level>; '' = omit
  } = {}) {
    if (!pty) return { ok: false, error: 'node-pty is not available. Install it with: npm install node-pty' };

    const key = alias.toLowerCase();
    const project = getProject(key);
    if (!project) return { ok: false, error: `Project "${alias}" not found` };
    if (!existsSync(project.path)) return { ok: false, error: `Project path does not exist: ${project.path}` };

    // Write .mcp.json so Claude Code picks up the Crundi MCP server, and the
    // lifecycle hooks so the server can track this terminal's agent state.
    if (apiUrl && apiKey) {
      try { writeMcpConfig(project.path, apiUrl, apiKey, key); } catch { /* non-fatal */ }
      try { writeHooksConfig(project.path); } catch { /* non-fatal */ }
    }

    // Terminal id is generated up-front so it can be injected into the PTY env —
    // the hooks read CRUNDI_TERMINAL_ID to attribute events to THIS terminal.
    const id = genTermId();

    const siblings = entriesForAlias(key);
    const aliasHasLive = siblings.some(t => t.proc);

    // Build the command. Flags stay as discrete tokens so the prompt can be
    // passed as its OWN argv element on Windows (`cmd /c claude … "prompt"`).
    // Embedding the prompt in a single quoted command string breaks under
    // node-pty's Windows command-line quoting — cmd strips/mangles the quotes
    // and claude receives only the first word of the prompt.
    const flagList = [];
    if (sessionMode === 'resume' && resumeId) flagList.push('--resume', resumeId);
    else if (sessionMode === 'continue') flagList.push('--continue');
    else if (sessionMode === 'new') { /* explicit fresh session — no --continue */ }
    // Interactive default: resume a prior conversation only when one exists AND
    // no other live terminal is attached (avoids "No conversation found" and
    // two --continue clobbering the same session file).
    else if (!aliasHasLive && hasExistingConversation(project.path)) flagList.push('--continue');
    if (model) flagList.push('--model', String(model));
    if (effort) flagList.push('--effort', String(effort));
    if (skipPermissions) flagList.push('--dangerously-skip-permissions');
    const cleanPrompt = prompt ? String(prompt).replace(/[\r\n]+/g, ' ').trim() : '';
    const claudeTokens = ['claude', ...flagList];
    if (cleanPrompt) claudeTokens.push(cleanPrompt);

    const shell = isWin ? 'cmd.exe' : '/bin/bash';
    let shellArgs;
    if (shellOnly) {
      // "Empty shell": just an interactive shell, no command.
      shellArgs = isWin ? [] : ['-i'];
    } else if (rawCommand) {
      shellArgs = isWin ? ['/c', String(rawCommand)] : ['-c', String(rawCommand)];
    } else if (isWin) {
      // Discrete tokens → node-pty quotes the spaced prompt token correctly.
      shellArgs = ['/c', ...claudeTokens];
    } else {
      // bash -c takes ONE command string; double-quote the prompt and escape
      // shell-special chars so it survives intact.
      let cmd = 'claude' + (flagList.length ? ' ' + flagList.join(' ') : '');
      if (cleanPrompt) cmd += ' "' + cleanPrompt.replace(/(["\\$`])/g, '\\$1') + '"';
      shellArgs = ['-c', cmd];
    }

    let proc;
    try {
      proc = pty.spawn(shell, shellArgs, {
        name: 'xterm-256color',
        cols,
        rows,
        cwd: project.path,
        // Inject the API + this terminal's id so the lifecycle hooks can report
        // state back to Crundi, attributed to this exact terminal.
        env: {
          ...process.env, FORCE_COLOR: '1',
          CRUNDI_TERMINAL_ID: id,
          ...(apiUrl ? { CRUNDI_API_URL: apiUrl } : {}),
          ...(apiKey ? { CRUNDI_API_KEY: apiKey } : {}),
        },
      });
    } catch (err) {
      return { ok: false, error: `Failed to spawn ${shellOnly ? 'shell' : 'terminal'}: ${err.message}` };
    }

    const emitter = new EventEmitter();
    emitter.setMaxListeners(50);

    const nextOrder = siblings.length ? Math.max(...siblings.map(t => t.order)) + 1 : 0;
    const entry = {
      id,
      alias: key,
      title: (title && String(title).trim()) || (shellOnly ? 'Shell' : 'Terminal'),
      order: nextOrder,
      proc,
      scrollback: '',
      emitter,
      cols,
      rows,
    };
    terminals.set(id, entry);

    proc.onData((data) => {
      // Append to scrollback (trim if too long)
      entry.scrollback += data;
      if (entry.scrollback.length > MAX_SCROLLBACK) {
        entry.scrollback = entry.scrollback.slice(-MAX_SCROLLBACK);
      }
      // Broadcast to WebSocket subscribers
      emitter.emit('data', data);
    });

    proc.onExit(({ exitCode }) => {
      console.log(`[claude-terminals] "${key}" (${id}) exited (code ${exitCode})`);
      entry.proc = null;
      emitter.emit('data', `\r\n[Process exited with code ${exitCode}]\r\n`);
    });

    console.log(`[claude-terminals] Spawned claude "${entry.title}" for "${key}" (${id}) in ${project.path} (${cols}x${rows})`);
    return { ok: true, id, project: key, title: entry.title, order: entry.order };
  }

  /**
   * Close a single terminal by id.
   */
  function close(id) {
    const entry = terminals.get(id);
    if (!entry) return { ok: false, error: `No terminal "${id}"` };

    if (entry.proc) {
      try { entry.proc.kill(); } catch { /* ignore */ }
    }
    entry.emitter.removeAllListeners();
    terminals.delete(id);
    console.log(`[claude-terminals] Closed "${entry.alias}" (${id})`);
    return { ok: true };
  }

  /**
   * Close every terminal for a project (used when a project is removed).
   */
  function closeProject(alias) {
    const key = alias.toLowerCase();
    let closed = 0;
    for (const t of entriesForAlias(key)) { close(t.id); closed++; }
    return { ok: true, closed };
  }

  /**
   * Write input to a terminal.
   */
  function write(id, data) {
    const entry = terminals.get(id);
    if (!entry?.proc) return;
    entry.proc.write(data);
  }

  /**
   * Resize a terminal.
   */
  function resize(id, cols, rows) {
    const entry = terminals.get(id);
    if (!entry?.proc) return { ok: false, error: `No terminal "${id}"` };
    try {
      entry.proc.resize(cols, rows);
      entry.cols = cols;
      entry.rows = rows;
      return { ok: true };
    } catch (err) {
      return { ok: false, error: err.message };
    }
  }

  /**
   * Rename a terminal (display title only).
   */
  function rename(id, title) {
    const entry = terminals.get(id);
    if (!entry) return { ok: false, error: `No terminal "${id}"` };
    entry.title = (title && String(title).trim()) || 'Terminal';
    return { ok: true, title: entry.title };
  }

  /**
   * Reorder a project's terminals to match the given id sequence. Ids not in
   * the list keep a stable relative order after the listed ones.
   */
  function setOrder(alias, ids) {
    const key = alias.toLowerCase();
    if (!Array.isArray(ids)) return { ok: false, error: 'ids must be an array' };
    let n = 0;
    for (const id of ids) {
      const entry = terminals.get(id);
      if (entry && entry.alias === key) entry.order = n++;
    }
    for (const t of entriesForAlias(key)) {
      if (!ids.includes(t.id)) t.order = n++;
    }
    return { ok: true };
  }

  /**
   * Get scrollback buffer for a terminal.
   */
  function getScrollback(id) {
    const entry = terminals.get(id);
    return entry?.scrollback || '';
  }

  /**
   * Subscribe to terminal output.
   */
  function on(id, handler) {
    const entry = terminals.get(id);
    if (entry) entry.emitter.on('data', handler);
  }

  /**
   * Unsubscribe from terminal output.
   */
  function off(id, handler) {
    const entry = terminals.get(id);
    if (entry) entry.emitter.off('data', handler);
  }

  /**
   * Close all terminals (for shutdown).
   */
  function closeAll() {
    for (const id of [...terminals.keys()]) {
      close(id);
    }
  }

  function info(id) {
    const entry = terminals.get(id);
    if (!entry) return null;
    return { id, alias: entry.alias, title: entry.title, scrollback: entry.scrollback };
  }

  const self = {
    list, create, close, closeProject, write, resize, rename, setOrder, getScrollback, on, off, closeAll, info,
    set apiUrl(v) { apiUrl = v; },
    get apiUrl() { return apiUrl; },
    set apiKey(v) { apiKey = v; },
    get apiKey() { return apiKey; },
  };
  return self;
}
