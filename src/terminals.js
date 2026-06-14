/**
 * terminals.js — Named terminal instance manager.
 *
 * Manages per-topic named terminal (PTY) instances via IPC.
 * Claude uses MCP tools (in mcp-server.js) → this module → process.send() →
 * app/main.js (spawns node-pty, renders xterm.js in UI) → result.
 *
 * When running headless (no Electron), all operations return a graceful error.
 */

// ─── Constants ───

const MAX_PER_TOPIC = 5;
const MAX_TOTAL = 20;
const MAX_OUTPUT_LINES = 500;
const IPC_TIMEOUT_MS = 30_000;

// ─── State ───

// key (`${alias}::${name}`) → TerminalInstance
const terminals = new Map();

// requestId → { resolve, reject, timer }
const pendingRequests = new Map();
let nextRequestId = 1;

/**
 * @typedef {Object} TerminalInstance
 * @property {string} key
 * @property {string} alias       — project alias
 * @property {string} name        — user-given name
 * @property {string} topicId
 * @property {string} command     — initial command (if any)
 * @property {'running'|'exited'} status
 * @property {number} createdAt
 */

// ─── IPC Bridge ───

function isElectron() {
  return process.env.ELECTRON_RUN === '1';
}

function sendIpc(type, payload) {
  return new Promise((resolve, reject) => {
    if (!isElectron() || typeof process.send !== 'function') {
      return reject(new Error('Terminal tools require the Electron desktop app.'));
    }
    const requestId = nextRequestId++;
    const timer = setTimeout(() => {
      pendingRequests.delete(requestId);
      reject(new Error('Terminal IPC request timed out'));
    }, IPC_TIMEOUT_MS);

    pendingRequests.set(requestId, { resolve, reject, timer });
    try {
      process.send({ type, requestId, ...payload });
    } catch (err) {
      pendingRequests.delete(requestId);
      clearTimeout(timer);
      reject(err);
    }
  });
}

function handleIpcMessage(msg) {
  if (!msg) return;
  // Terminal exited — update local state
  if (msg.type === 'terminalExited' && msg.key) {
    const inst = terminals.get(msg.key);
    if (inst) inst.status = 'exited';
    return;
  }
  if (msg.type !== 'terminalResult') return;
  const entry = pendingRequests.get(msg.requestId);
  if (!entry) return;
  pendingRequests.delete(msg.requestId);
  clearTimeout(entry.timer);
  entry.resolve(msg);
}

if (typeof process.on === 'function') {
  process.on('message', handleIpcMessage);
}

// ─── Helpers ───

function makeKey(alias, name) {
  return `${alias}::${name}`;
}

function countForTopic(alias) {
  let n = 0;
  for (const [, inst] of terminals) {
    if (inst.alias === alias) n++;
  }
  return n;
}

// ─── Public API ───

export async function spawnTerminal(alias, topicId, name, { command, cwd } = {}) {
  if (!isElectron()) return { ok: false, error: 'Terminal tools require the Electron desktop app.' };
  if (!name || !name.trim()) return { ok: false, error: 'Terminal name is required.' };
  const safeName = name.trim().replace(/[^a-zA-Z0-9_-]/g, '_').substring(0, 40);
  const key = makeKey(alias, safeName);

  if (terminals.has(key)) {
    const existing = terminals.get(key);
    if (existing.status === 'running') return { ok: false, error: `Terminal '${safeName}' already running for this project.` };
    // Exited — allow re-spawn
    terminals.delete(key);
  }
  if (countForTopic(alias) >= MAX_PER_TOPIC) return { ok: false, error: `Max ${MAX_PER_TOPIC} terminals per project.` };
  if (terminals.size >= MAX_TOTAL) return { ok: false, error: `Max ${MAX_TOTAL} total terminals.` };

  try {
    const result = await sendIpc('terminalSpawn', { key, topicId, name: safeName, command, cwd });
    if (!result.ok) return { ok: false, error: result.error };

    terminals.set(key, {
      key, alias, name: safeName, topicId: String(topicId),
      command: command || '', status: 'running', createdAt: Date.now(),
    });
    return { ok: true, name: safeName };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function writeTerminal(alias, name, input) {
  if (!isElectron()) return { ok: false, error: 'Terminal tools require the Electron desktop app.' };
  const key = makeKey(alias, name);
  const inst = terminals.get(key);
  if (!inst) return { ok: false, error: `No terminal '${name}' found.` };
  if (inst.status === 'exited') return { ok: false, error: `Terminal '${name}' has exited.` };

  try {
    const result = await sendIpc('terminalWrite', { key, input });
    if (!result.ok) return { ok: false, error: result.error };
    return { ok: true };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function closeTerminal(alias, name) {
  if (!isElectron()) return { ok: false, error: 'Terminal tools require the Electron desktop app.' };
  const key = makeKey(alias, name);
  const inst = terminals.get(key);
  if (!inst) return { ok: false, error: `No terminal '${name}' found.` };

  try {
    const result = await sendIpc('terminalClose', { key });
    terminals.delete(key);
    return { ok: true };
  } catch (err) { return { ok: false, error: err.message }; }
}

export function listTerminals(alias) {
  const result = [];
  for (const [, inst] of terminals) {
    if (alias && inst.alias !== alias) continue;
    result.push({
      name: inst.name, alias: inst.alias, topicId: inst.topicId, status: inst.status,
      command: inst.command, createdAt: inst.createdAt,
    });
  }
  return { ok: true, terminals: result };
}

export async function getTerminalOutput(alias, name, { start, end, countOnly } = {}) {
  if (!isElectron()) return { ok: false, error: 'Terminal tools require the Electron desktop app.' };
  const key = makeKey(alias, name);
  const inst = terminals.get(key);
  if (!inst) return { ok: false, error: `No terminal '${name}' found.` };

  try {
    const result = await sendIpc('terminalOutput', { key, start, end, countOnly });
    if (!result.ok) return { ok: false, error: result.error };
    if (countOnly) return { ok: true, total: result.total, status: inst.status };
    return { ok: true, lines: result.lines, total: result.total, status: inst.status };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function waitTerminal(alias, name, { pattern, timeout } = {}) {
  if (!isElectron()) return { ok: false, error: 'Terminal tools require the Electron desktop app.' };
  const key = makeKey(alias, name);
  const inst = terminals.get(key);
  if (!inst) return { ok: false, error: `No terminal '${name}' found.` };

  try {
    const result = await sendIpc('terminalWait', { key, pattern, timeout: Math.min(timeout || 30000, 60000) });
    if (!result.ok) return { ok: false, error: result.error };
    return { ok: true, matched: result.matched, line: result.line };
  } catch (err) { return { ok: false, error: err.message }; }
}

export async function resizeTerminal(alias, name, cols, rows) {
  if (!isElectron()) return { ok: false, error: 'Terminal tools require the Electron desktop app.' };
  const key = makeKey(alias, name);
  const inst = terminals.get(key);
  if (!inst) return { ok: false, error: `No terminal '${name}' found.` };
  if (inst.status === 'exited') return { ok: false, error: `Terminal '${name}' has exited.` };

  try {
    const result = await sendIpc('terminalResize', { key, cols, rows });
    return { ok: true };
  } catch (err) { return { ok: false, error: err.message }; }
}
