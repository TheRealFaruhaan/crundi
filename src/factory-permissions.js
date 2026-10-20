/**
 * factory-permissions.js — what an employee may do without asking, what it
 * may never do, and how a request is put to the Manager. Pure.
 *
 * An employee's permission request goes, in order:
 *   1. hardDeny()  — refused outright, in every mode, whoever would answer:
 *                    sudo, restarting or stopping Crundi, pushing or rewiring
 *                    remotes, binding the ports the server lives on, and
 *                    writing outside the run's own folders.
 *   2. run rules   — run-wide allow rules the Manager granted ("Bash(npm test:*)")
 *   3. the Manager — queued and asked in a batch; its answer is final.
 *
 * The same deny list is also passed to every factory chat as
 * --disallowedTools (DENY_RULES), which is what holds for a Manager running
 * in bypass mode: a bypassed session never asks, so only rules apply to it.
 * Prefix rules cannot see every way of writing a command, so they are a
 * second line, not the only one.
 *
 * The machine's reserved ports are the ones in ~/.claude/CLAUDE.md: 443 and 80
 * (Crundi's site and certificate renewal), 8888 (its loopback API), 22 (ssh)
 * and 53 (DNS).
 */

import { resolve, sep } from 'path';

export const RESERVED_PORTS = [80, 443, 8888, 22, 53];

/**
 * Passed as --allowedTools to every factory chat. Calling an MCP tool needs
 * permission unless allowed (verified: a normal-mode Manager sat waiting on
 * its own rename_chat), so the factory's own tools must never ask.
 */
export const ALLOW_RULES = [
  'mcp__crundi__factory_handoff',
  'mcp__crundi__factory_permission',
  'mcp__crundi__rename_chat',
  // Skills are part of how a factory is meant to work (spec §2.5).
  'Skill',
];

/** Passed as --disallowedTools to every factory chat. */
export const DENY_RULES = [
  'Bash(sudo:*)',
  'Bash(git push:*)',
  'Bash(git remote add:*)',
  'Bash(git remote set-url:*)',
  'Bash(systemctl restart:*)',
  'Bash(systemctl stop:*)',
  'Bash(pm2 restart:*)',
  'Bash(pm2 stop:*)',
  'Bash(pm2 delete:*)',
];

const WRITE_TOOLS = new Set(['Edit', 'Write', 'MultiEdit', 'NotebookEdit']);
const PORTS = RESERVED_PORTS.join('|');

const COMMAND_DENY = [
  [/(^|[\s;&|(`])sudo(\s|$)/, 'sudo is never allowed in a factory: this machine\'s sudo reaches every project and the live server.'],
  [/(^|[\s;&|(`])(doas|su)\s/, 'Switching user is never allowed in a factory.'],
  [/\bgit\s+push\b/, 'Pushing is never allowed in a factory. Everything stays local; the person decides about the result.'],
  [/\bgit\s+remote\s+(add|set-url|remove|rm|rename)\b/, 'Changing git remotes is never allowed in a factory.'],
  [/\b(systemctl|service)\b[^\n;&|]*\b(restart|stop|kill|disable|reload)\b[^\n;&|]*\bcrundi\b|\bcrundi\b[^\n;&|]*\b(restart|stop)\b/i, 'Restarting or stopping Crundi ends every chat on this server. Never from a factory.'],
  [/\bpm2\s+(restart|stop|delete|kill)\b/, 'Restarting or stopping pm2 processes is never allowed in a factory.'],
  [/\b(pkill|killall)\b[^\n;&|]*\b(crundi|node|claude)\b/, 'Killing Crundi, node or Claude processes is never allowed in a factory.'],
  [new RegExp(`(--port|--listen|-p|\\bPORT|\\bport)[=\\s:]+(${PORTS})\\b`), `Ports ${RESERVED_PORTS.join(', ')} belong to the server. Use a high port (4000-9999, not 8888).`],
  [new RegExp(`\\b(0\\.0\\.0\\.0|127\\.0\\.0\\.1|localhost|\\[::\\]):(${PORTS})\\b`), `Ports ${RESERVED_PORTS.join(', ')} belong to the server. Use a high port (4000-9999, not 8888).`],
];

function isWithin(root, p) {
  if (!root || !p) return false;
  const r = resolve(root), c = resolve(p);
  return c === r || c.startsWith(r + sep);
}

/**
 * Why a request must be refused whatever anyone says, or '' if it may be asked.
 * @param {{ toolName:string, input:object }} req
 * @param {{ writableRoots: string[] }} ctx   the run's worktrees root and notes folder
 */
export function hardDeny(req, { writableRoots = [] } = {}) {
  const tool = String(req?.toolName || '');
  const input = req?.input && typeof req.input === 'object' ? req.input : {};
  if (tool === 'Bash') {
    const cmd = String(input.command || '');
    for (const [re, why] of COMMAND_DENY) if (re.test(cmd)) return why;
  }
  if (WRITE_TOOLS.has(tool)) {
    const p = String(input.file_path || input.notebook_path || input.path || '');
    if (p && !writableRoots.some(r => isWithin(r, p))) return `Writing outside the run's own folders is never allowed (${p}). Work in your worktree.`;
  }
  return '';
}

// ─── Run-wide rules ───

/**
 * Parse "Tool" or "Tool(spec)". A spec ending in ":*" is a prefix; otherwise
 * it must match exactly. WebFetch takes "domain:example.com".
 */
export function parseRule(text) {
  const s = String(text || '').trim();
  const m = s.match(/^([A-Za-z_][\w.-]*)(?:\((.*)\))?$/s);
  if (!m) return null;
  const tool = m[1];
  const spec = m[2] === undefined ? null : m[2].trim();
  if (spec === '') return null;
  return { tool, spec, text: s };
}

function primaryOf(toolName, input) {
  const i = input || {};
  return String(i.command ?? i.url ?? i.file_path ?? i.notebook_path ?? i.path ?? i.pattern ?? i.query ?? '');
}

/** Does a granted rule cover this request? */
export function ruleMatches(rule, req) {
  const r = typeof rule === 'string' ? parseRule(rule) : rule;
  if (!r || r.tool !== req.toolName) return false;
  if (r.spec === null) return true;
  if (req.toolName === 'WebFetch' && r.spec.startsWith('domain:')) {
    const want = r.spec.slice(7).toLowerCase();
    let host = '';
    try { host = new URL(String(req.input?.url || '')).hostname.toLowerCase(); } catch { return false; }
    return host === want || host.endsWith('.' + want);
  }
  const value = primaryOf(req.toolName, req.input);
  if (r.spec.endsWith(':*')) {
    const prefix = r.spec.slice(0, -2);
    // A prefix rule must not let a compound command ride along on it.
    if (req.toolName === 'Bash' && /[;&|`$<>]|\n/.test(value.slice(prefix.length))) return false;
    return value === prefix || value.startsWith(prefix + ' ') || value.startsWith(prefix);
  }
  return value === r.spec;
}

/** A rule the Manager wants to grant: refuse ones that are too broad to be safe. */
export function checkRule(text) {
  const r = parseRule(text);
  if (!r) return { ok: false, error: `"${text}" is not a rule. Write Tool or Tool(spec), for example Bash(npm test:*) or WebFetch(domain:docs.example.com).` };
  if (r.tool === 'Bash' && (r.spec === null || r.spec === '*' || r.spec === ':*')) return { ok: false, error: 'A rule for every shell command is too broad. Name the command, for example Bash(npm test:*).' };
  if (r.tool === 'Bash' && hardDeny({ toolName: 'Bash', input: { command: r.spec.replace(/:\*$/, '') } })) return { ok: false, error: `"${text}" would allow something that is never allowed.` };
  if (WRITE_TOOLS.has(r.tool)) return { ok: false, error: 'Edits are already free inside the run\'s worktrees and never allowed outside, so there is nothing to grant.' };
  return { ok: true, rule: r.text };
}

// ─── Putting requests to the Manager ───

const clip = (s, n) => { const t = String(s ?? ''); return t.length > n ? t.slice(0, n - 1) + '…' : t; };

/** One line describing what is being asked. */
export function describeRequest(req) {
  const i = req.input || {};
  if (req.toolName === 'Bash') return `run \`${clip(i.command, 400)}\`${i.description ? ` (${clip(i.description, 160)})` : ''}`;
  if (req.toolName === 'WebFetch') return `fetch ${clip(i.url, 300)}`;
  if (req.toolName === 'WebSearch') return `search the web for "${clip(i.query, 200)}"`;
  if (req.toolName === 'Read' || req.toolName === 'Glob' || req.toolName === 'Grep') return `${req.toolName.toLowerCase()} ${clip(i.file_path || i.path || i.pattern, 300)}`;
  const shown = clip(JSON.stringify(i), 400);
  return `use ${req.displayName || req.toolName} with ${shown}`;
}

/** The message carrying a batch of requests to the Manager. */
export function batchMessage(items, { bypass = false } = {}) {
  const lines = [`[Factory] Permission request${items.length === 1 ? '' : 's'} from your team. Answer with factory_permission (mcp__crundi__factory_permission); your decision is final.`, ''];
  for (const it of items) {
    lines.push(`- ${it.id} · ${it.employee} wants to ${describeRequest(it)}`);
    if (it.description && it.toolName !== 'Bash') lines.push(`    ${clip(it.description, 300)}`);
  }
  lines.push('', 'For each: { id, allow: true|false, reason }. Add rule (for example "Bash(npm test:*)") to an allow to approve the same kind of request for the rest of the run without asking.');
  if (bypass) lines.push('The person launched you with bypass permissions: they trust you. Approve reasonable requests promptly and grant rules for safe repeated commands; deny what is risky or off-task.');
  lines.push('This is not a handoff: answer these, then carry on with whatever you were doing.');
  return lines.join('\n');
}

/** What the employee is told when the Manager (or the deny list) refuses. */
export function denyMessage(reason, by = 'The Manager') {
  return `${by} denied this${reason ? `: ${reason}` : '.'} Find another way, or hand off with escalate if you cannot continue without it.`;
}
