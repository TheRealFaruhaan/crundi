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
const IDLE_DETECT_MS = 2000;       // ms of silence before checking for prompt
const SPAWN_GRACE_MS = 10_000;     // ignore idle detection for first 10s after spawn
const isWin = process.platform === 'win32';

// Claude Code idle prompt patterns — match full last non-empty line
const PROMPT_PATTERNS = [
  /^\s*>\s*$/,                 // Claude Code input prompt (actual character)
  /^\s*❯\s*$/,                 // Claude Code alternate prompt
  /^\s*\$\s*$/,                // Shell prompt
];

// Completion phrases in scrollback — alternative idle signal
const COMPLETION_PATTERNS = [
  /total cost:/i,
  /what would you like/i,
  /is there anything else/i,
  /I've (?:completed|finished|updated|created|fixed|implemented|added)/i,
];

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
 * Add .mcp.json to .gitignore if .gitignore exists and doesn't already list it.
 */
export function ensureGitignore(projectPath) {
  const gitignoreFile = join(projectPath, '.gitignore');
  if (!existsSync(gitignoreFile)) return;
  try {
    const content = readFileSync(gitignoreFile, 'utf-8');
    if (content.split('\n').some(line => line.trim() === '.mcp.json')) return;
    const nl = content.endsWith('\n') ? '' : '\n';
    writeFileSync(gitignoreFile, content + nl + '.mcp.json\n');
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

/**
 * Create the Claude terminal manager.
 *
 * @param {{ onComplete?: (alias: string) => void, apiUrl?: string, apiKey?: string }} opts
 *   onComplete is called when Claude appears to finish (idle prompt detected).
 *   apiUrl/apiKey are passed to MCP stdio servers via .mcp.json env vars.
 */
export function createClaudeTerminals({ onComplete, apiUrl: initApiUrl, apiKey: initApiKey } = {}) {
  // alias → { proc, scrollback, emitter, idleTimer, busy, cols, rows }
  const terminals = new Map();
  let apiUrl = initApiUrl;
  let apiKey = initApiKey;

  /**
   * List all active terminals.
   * @returns {{ project: string, status: string }[]}
   */
  function list() {
    const result = [];
    for (const [alias, t] of terminals) {
      result.push({
        project: alias,
        status: t.proc ? 'running' : 'exited',
        busy: t.busy,
      });
    }
    return result;
  }

  /**
   * Create a Claude Code terminal for a project.
   */
  async function create(alias, { cols = 120, rows = 30, skipPermissions = false } = {}) {
    if (!pty) return { ok: false, error: 'node-pty is not available. Install it with: npm install node-pty' };

    const key = alias.toLowerCase();
    if (terminals.has(key)) {
      const existing = terminals.get(key);
      if (existing.proc) return { ok: false, error: `Terminal already running for "${alias}"` };
      // Exited — clean up and re-create
      terminals.delete(key);
    }

    const project = getProject(key);
    if (!project) return { ok: false, error: `Project "${alias}" not found` };
    if (!existsSync(project.path)) return { ok: false, error: `Project path does not exist: ${project.path}` };

    // Write .mcp.json so Claude Code picks up the Crundi MCP server
    if (apiUrl && apiKey) {
      try { writeMcpConfig(project.path, apiUrl, apiKey, key); } catch { /* non-fatal */ }
    }

    // Spawn claude CLI. Only resume a prior conversation when one exists for
    // this project — `--continue` against a fresh project exits with
    // "No conversation found to continue".
    const flagList = [];
    if (hasExistingConversation(project.path)) flagList.push('--continue');
    if (skipPermissions) flagList.push('--dangerously-skip-permissions');
    const flags = flagList.join(' ');
    const command = flags ? `claude ${flags}` : 'claude';
    const shell = isWin ? 'cmd.exe' : '/bin/bash';
    const shellArgs = isWin ? ['/c', command] : ['-c', command];

    let proc;
    try {
      proc = pty.spawn(shell, shellArgs, {
        name: 'xterm-256color',
        cols,
        rows,
        cwd: project.path,
        env: { ...process.env, FORCE_COLOR: '1' },
      });
    } catch (err) {
      return { ok: false, error: `Failed to spawn claude: ${err.message}` };
    }

    const emitter = new EventEmitter();
    emitter.setMaxListeners(50);

    const entry = {
      proc,
      scrollback: '',
      emitter,
      idleTimer: null,
      busy: false,
      cols,
      rows,
      lastOutputTime: 0,
      spawnedAt: Date.now(),
    };
    terminals.set(key, entry);

    proc.onData((data) => {
      // Append to scrollback (trim if too long)
      entry.scrollback += data;
      if (entry.scrollback.length > MAX_SCROLLBACK) {
        entry.scrollback = entry.scrollback.slice(-MAX_SCROLLBACK);
      }
      entry.lastOutputTime = Date.now();

      // Broadcast to WebSocket subscribers
      emitter.emit('data', data);

      // Only reset idle timer on meaningful output (not cursor blinks / TUI re-renders)
      const meaningful = stripAnsi(data).replace(/\s/g, '').length > 0;
      if (meaningful) {
        entry.busy = true;
        clearTimeout(entry.idleTimer);
        entry.idleTimer = setTimeout(() => checkIdle(key), IDLE_DETECT_MS);
      } else if (!entry.idleTimer && entry.busy) {
        // No timer running but still marked busy — start one
        entry.idleTimer = setTimeout(() => checkIdle(key), IDLE_DETECT_MS);
      }
    });

    proc.onExit(({ exitCode }) => {
      console.log(`[claude-terminals] "${alias}" exited (code ${exitCode})`);
      entry.proc = null;
      entry.busy = false;
      clearTimeout(entry.idleTimer);
      emitter.emit('data', `\r\n[Process exited with code ${exitCode}]\r\n`);
    });

    console.log(`[claude-terminals] Spawned claude for "${alias}" in ${project.path} (${cols}x${rows})`);
    return { ok: true, project: alias };
  }

  /**
   * Check if Claude has returned to its idle prompt.
   */
  function checkIdle(alias) {
    const entry = terminals.get(alias);
    if (!entry || !entry.proc) return;

    // Don't fire during startup grace period (avoids false positive on initial prompt)
    if (Date.now() - entry.spawnedAt < SPAWN_GRACE_MS) return;

    // Strip all ANSI escapes, check last non-empty line + completion phrases
    const raw = entry.scrollback.slice(-2000);
    const clean = stripAnsi(raw);
    const lines = clean.split('\n').map(l => l.trimEnd()).filter(Boolean);
    const lastLine = lines[lines.length - 1] || '';

    const isPrompt = PROMPT_PATTERNS.some(re => re.test(lastLine));
    const hasCompletion = COMPLETION_PATTERNS.some(re => re.test(clean));

    // Require prompt match; completion text alone is not enough (could be mid-task)
    if (isPrompt && entry.busy) {
      entry.busy = false;
      console.log(`[claude-terminals] "${alias}" appears idle (prompt: ${JSON.stringify(lastLine)}, completion: ${hasCompletion})`);
      if (onComplete) {
        try { onComplete(alias); } catch { /* ignore */ }
      }
    }
  }

  /**
   * Close a terminal.
   */
  function close(alias) {
    const key = alias.toLowerCase();
    const entry = terminals.get(key);
    if (!entry) return { ok: false, error: `No terminal for "${alias}"` };

    clearTimeout(entry.idleTimer);
    if (entry.proc) {
      try { entry.proc.kill(); } catch { /* ignore */ }
    }
    entry.emitter.removeAllListeners();
    terminals.delete(key);
    console.log(`[claude-terminals] Closed "${alias}"`);
    return { ok: true };
  }

  /**
   * Write input to a terminal.
   */
  function write(alias, data) {
    const key = alias.toLowerCase();
    const entry = terminals.get(key);
    if (!entry?.proc) return;
    entry.proc.write(data);
  }

  /**
   * Resize a terminal.
   */
  function resize(alias, cols, rows) {
    const key = alias.toLowerCase();
    const entry = terminals.get(key);
    if (!entry?.proc) return { ok: false, error: `No terminal for "${alias}"` };
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
   * Get scrollback buffer for a terminal.
   */
  function getScrollback(alias) {
    const key = alias.toLowerCase();
    const entry = terminals.get(key);
    return entry?.scrollback || '';
  }

  /**
   * Subscribe to terminal output.
   */
  function on(alias, handler) {
    const key = alias.toLowerCase();
    const entry = terminals.get(key);
    if (entry) entry.emitter.on('data', handler);
  }

  /**
   * Unsubscribe from terminal output.
   */
  function off(alias, handler) {
    const key = alias.toLowerCase();
    const entry = terminals.get(key);
    if (entry) entry.emitter.off('data', handler);
  }

  /**
   * Close all terminals (for shutdown).
   */
  function closeAll() {
    for (const [alias] of terminals) {
      close(alias);
    }
  }

  function info(alias) {
    const key = alias.toLowerCase();
    const entry = terminals.get(key);
    if (!entry) return null;
    return { alias: key, busy: entry.busy, scrollback: entry.scrollback, lastOutputTime: entry.lastOutputTime };
  }

  const self = {
    list, create, close, write, resize, getScrollback, on, off, closeAll, info,
    set apiUrl(v) { apiUrl = v; },
    get apiUrl() { return apiUrl; },
    set apiKey(v) { apiKey = v; },
    get apiKey() { return apiKey; },
  };
  return self;
}
