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

import { resolve, sep, join } from 'path';
import { readFileSync, existsSync } from 'fs';
import { PERMISSION_PRESETS } from './factory-model.js';

export const RESERVED_PORTS = [80, 443, 8888, 22, 53];

/**
 * Passed as --allowedTools to every factory chat. Calling an MCP tool needs
 * permission unless allowed (verified: a normal-mode Manager sat waiting on
 * its own rename_chat), so the factory's own tools must never ask.
 */
export const ALLOW_RULES = [
  'mcp__crundi__factory_handoff',
  'mcp__crundi__factory_permission',
  'mcp__crundi__factory_delegate',
  'mcp__crundi__factory_result',
  'mcp__crundi__factory_status',
  'mcp__crundi__factory_control',
  'mcp__crundi__factory_message',
  'mcp__crundi__factory_tick',
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
  // By name they hit whatever else matches (another project's server, Crundi
  // itself: a test chat's `pkill -f "node src/index.js"` took down a Crundi
  // dev instance). Stop your own processes by PID, or as Crundi services.
  'Bash(pkill:*)',
  'Bash(killall:*)',
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
  // A factory secret request (factory-secrets.js): the secret's name and the
  // command, never its value.
  if (req.kind === 'secret') return `run \`${clip(i.command, 400)}\` with the secret "${clip(i.secret, 120)}" bound to $${clip(i.envName || 'SECRET', 60)} (the value is never shown to anyone)${i.reason ? `. Reason: ${clip(i.reason, 200)}` : ''}`;
  if (req.toolName === 'Bash') return `run \`${clip(i.command, 400)}\`${i.description ? ` (${clip(i.description, 160)})` : ''}`;
  if (req.toolName === 'WebFetch') return `fetch ${clip(i.url, 300)}`;
  if (req.toolName === 'WebSearch') return `search the web for "${clip(i.query, 200)}"`;
  if (req.toolName === 'Read' || req.toolName === 'Glob' || req.toolName === 'Grep') return `${req.toolName.toLowerCase()} ${clip(i.file_path || i.path || i.pattern, 300)}`;
  const shown = clip(JSON.stringify(i), 400);
  return `use ${req.displayName || req.toolName} with ${shown}`;
}

/** The message carrying a batch of requests to the Manager. */
export function batchMessage(items, { bypass = false, gatekeeper = false, escalated = false, rulesAllowed = true } = {}) {
  const secretsOnly = items.length > 0 && items.every(it => it.kind === 'secret');
  if (secretsOnly) return secretBatchMessage(items, { gatekeeper, escalated });
  const lines = [gatekeeper
    ? `[Factory] Permission request${items.length === 1 ? '' : 's'} for you to answer by the Manager's policy (factory_permission, mcp__crundi__factory_permission).`
    : `[Factory] Permission request${items.length === 1 ? '' : 's'} from your team${escalated ? ', passed on by the Gatekeeper' : ''}. Answer with factory_permission (mcp__crundi__factory_permission); your decision is final.`, ''];
  for (const it of items) {
    lines.push(`- ${it.id} · ${it.employee} wants to ${describeRequest(it)}`);
    if (it.description && it.toolName !== 'Bash' && it.kind !== 'secret') lines.push(`    ${clip(it.description, 300)}`);
    if (it.gateNote) lines.push(`    The Gatekeeper: ${clip(it.gateNote, 300)}`);
  }
  if (gatekeeper) {
    lines.push('', 'For each: { id, allow: true|false, reason }, or { id, escalate: true, reason } to pass it to the Manager when unsure.');
  } else {
    lines.push('', 'For each: { id, allow: true|false, reason }.');
    if (rulesAllowed) lines.push('Add rule (for example "Bash(npm test:*)") to an allow so this person may do the same again without asking; add ruleFor: "everyone" to grant it to the whole team.');
    else lines.push('Standing rules are turned off for this factory: answer each request on its own.');
  }
  if (items.some(it => it.kind === 'secret')) lines.push('A request to use a secret is answered on its own: a rule never covers it.');
  if (bypass && !gatekeeper) lines.push('The person launched you with bypass permissions: they trust you. Approve reasonable requests promptly' + (rulesAllowed ? ' and grant rules for safe repeated commands' : '') + '; deny what is risky or off-task.');
  lines.push('This is not a handoff: answer these, then carry on with whatever you were doing.');
  return lines.join('\n');
}

/**
 * The message carrying secret requests: "[Factory] Secret request". Answered
 * the same way as a permission request, but never by a rule: each use of a
 * secret is decided on its own.
 */
export function secretBatchMessage(items, { gatekeeper = false, escalated = false } = {}) {
  const lines = [`[Factory] Secret request${items.length === 1 ? '' : 's'} from your team${escalated ? ', passed on by the Gatekeeper' : ''}. Answer with factory_permission (mcp__crundi__factory_permission)${gatekeeper ? ' by the Manager\'s policy' : '; your decision is final'}.`, ''];
  for (const it of items) {
    lines.push(`- ${it.id} · ${it.employee} wants to ${describeRequest(it)}`);
    if (it.gateNote) lines.push(`    The Gatekeeper: ${clip(it.gateNote, 300)}`);
  }
  lines.push('', 'For each: { id, allow: true|false, reason }' + (gatekeeper ? ', or { id, escalate: true, reason } to pass it to the Manager when unsure.' : '.'));
  lines.push('Allow only a command the task needs, that uses the secret for what it is for and does not print, save or send it anywhere else. A rule cannot cover a secret: each use is answered on its own. If the person has not unlocked the secret for this run, they are asked for its PIN after you allow it.');
  lines.push('This is not a handoff: answer these, then carry on with whatever you were doing.');
  return lines.join('\n');
}

/** What the employee is told when the Manager (or the deny list) refuses. */
export function denyMessage(reason, by = 'The Manager') {
  return `${by} denied this${reason ? `: ${reason}` : '.'} Find another way, or hand off with escalate if you cannot continue without it.`;
}

// ─── Rules from presets and from the project itself ───

const SCRIPT_NAME = /^(test|tests|build|lint|typecheck|type-check|check|format|fmt|e2e)([:_-][\w:-]*)?$/i;

/**
 * Rules for the project's own test/build/lint commands: package.json scripts
 * (npm run <name>) and Makefile targets (make <name>) whose names say what
 * they are. Granted with the "tests" preset.
 */
export function projectRules(repo) {
  const out = [];
  try {
    const pkg = JSON.parse(readFileSync(join(repo, 'package.json'), 'utf-8'));
    for (const name of Object.keys(pkg.scripts || {})) {
      if (SCRIPT_NAME.test(name) && /^[\w:.-]+$/.test(name)) out.push(`Bash(npm run ${name}:*)`);
    }
  } catch { /* no package.json */ }
  try {
    const mk = existsSync(join(repo, 'Makefile')) ? readFileSync(join(repo, 'Makefile'), 'utf-8') : '';
    for (const m of mk.matchAll(/^([A-Za-z][\w-]*):(?!=)/gm)) if (SCRIPT_NAME.test(m[1])) out.push(`Bash(make ${m[1]}:*)`);
  } catch { /* no Makefile */ }
  return [...new Set(out)];
}

/**
 * The rules a factory grants from the start, each with who it is for: the
 * factory's presets and hand-written rules for everyone ('*'), and each
 * employee's own presets and always-allowed rules for that employee.
 * @returns {Array<{rule:string, who:string}>}
 */
export function startRules(settings, repo, roster = []) {
  const out = [];
  const add = (rule, who) => { if ((checkRule(rule).ok || rule === 'WebSearch' || rule === 'WebFetch') && !out.some(x => x.rule === rule && x.who === who)) out.push({ rule, who }); };
  const fromPresets = (presets) => {
    const list = [];
    for (const k of presets || []) list.push(...(PERMISSION_PRESETS[k]?.rules || []));
    if ((presets || []).includes('tests') && repo) list.push(...projectRules(repo));
    return list;
  };
  for (const r of fromPresets(settings.presets)) add(r, '*');
  for (const r of settings.rules || []) add(r, '*');
  for (const e of roster) {
    for (const r of fromPresets(e.presets)) add(r, e.id);
    for (const r of e.tools?.allow || []) add(r, e.id);
  }
  return out;
}

/** Normalise a run's rules (older runs kept plain strings, for everyone). */
export function scopedRules(rules) {
  return (rules || []).map(r => typeof r === 'string' ? { rule: r, who: '*' } : r).filter(r => r && r.rule);
}

/** The rules that apply to one person. */
export function rulesFor(rules, who) {
  return scopedRules(rules).filter(r => r.who === '*' || r.who === who).map(r => r.rule);
}

export const GATEKEEPER_ID = '__gatekeeper';

/** The Gatekeeper's standing instructions. */
export function gatekeeperPrompt({ factory, task, project, policy, team }) {
  return [
    `[Crundi Factory "${factory.name}": you are the Gatekeeper]`,
    `You answer the permission requests of an AI team working on one task in the project "${project}", on behalf of its Manager. The task: ${task}`,
    '',
    'How this works:',
    '- Requests arrive as "[Factory] Permission request" messages with ids like p3. Answer every one with factory_permission (mcp__crundi__factory_permission): { id, allow, reason }.',
    '- Follow the Manager\'s policy below. Allow what the task plainly needs and the policy covers; deny what is risky, destructive or off-task, with a reason the employee can act on.',
    '- If you are unsure, or the policy does not cover it, pass it to the Manager: { id, escalate: true, reason }. Do not guess on anything that could do harm.',
    '- You never do the work yourself and never change files.',
    '- "[Factory] Secret request" messages ask to run a command with one of the factory\'s secrets (you see its name and the command, never the value). Allow only what the task needs and the policy covers; pass anything unclear to the Manager.',
    '- Some things are refused before they reach you: sudo, restarting Crundi, pushing or changing remotes, the server\'s ports, writing outside the run\'s folders.',
    '',
    'The team: ' + (team || []).map(r => r.name).join(', '),
    '',
    'The Manager\'s policy:',
    policy ? policy : '(none yet: allow ordinary development commands the task needs, such as running tests, building, reading files and installing the packages it asks for; pass anything else to the Manager)',
  ].join('\n');
}
