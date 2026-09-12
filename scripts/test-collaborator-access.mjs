#!/usr/bin/env node
/**
 * test-collaborator-access.mjs — the boundary around an outside collaborator.
 *
 * Crundi was a single-owner tool. `validateToken` returned a boolean, and past
 * that one line every route was reachable: a shell, the filesystem above
 * project roots, secrets, the server log. Collaborators add a second trust
 * level to a codebase that had exactly one, which makes almost every failure
 * here a QUIET one — the feature still works, it just also works for someone
 * it should not.
 *
 * So this checks the shape of the boundary rather than the happy path:
 *
 *  - the policy is an ALLOWLIST. A route nobody thought about must be refused,
 *    because the alternative is that route 121 ships reachable and nothing
 *    fails loudly.
 *  - owners are untouched. If mayAccess ever starts consulting the list for
 *    them, the single-user install breaks in a hundred places at once.
 *  - the dangerous routes are refused TWICE — once by omission, once by name —
 *    so widening a pattern cannot quietly re-expose them.
 *  - the enforcement points that are easy to delete still exist: the gate, the
 *    project shadow, the absolute-path removal, the WebSocket gating, the
 *    collaborator MCP key check, and the --restricted launch.
 *
 * The CLI-side claims were verified against 2.1.269 on this machine before any
 * of it was written:
 *
 *   Read /etc/passwd      -> "outside <cwd>; --restricted confines the file
 *                             tools to the working directory"
 *   Bash head /etc/passwd -> blocked, same reason
 *   Bash eval $(printf …) -> "Contains command_substitution" (refused, not asked)
 *   --restricted --tools A,B,C -> exactly 3 tools, i.e. an exact allowlist
 *
 * Run: node scripts/test-collaborator-access.mjs
 */

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  mayAccess, isConfined, ROLE_OWNER, ROLE_COLLABORATOR,
  COLLABORATOR_MCP_TOOLS, COLLABORATOR_CLAUDE_TOOLS, collaboratorSettings,
} from '../src/access-policy.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (...p) => readFileSync(join(root, ...p), 'utf8');

let failures = 0;
const check = (label, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail === undefined ? '' : ` -> ${detail}`}`);
};

const owner = { role: ROLE_OWNER, projects: [], roots: {} };
const collab = { role: ROLE_COLLABORATOR, collabKey: 'pc:sam', projects: ['demo'], roots: { demo: '/tmp/wt' } };

// ── Roles ─────────────────────────────────────────────────────────────────
check('a collaborator is confined', isConfined(collab));
check('an owner is not', !isConfined(owner));
check('no principal is not a free pass', !mayAccess(null, 'GET', '/api/state'));

// ── The owner must be entirely unaffected ─────────────────────────────────
// If this ever fails, the single-user install has been broken by the feature
// meant to sit beside it.
const EVERY_AREA = [
  ['GET', '/api/secrets'], ['POST', '/api/secrets'], ['GET', '/api/schedules'],
  ['POST', '/api/terminals/spawn'], ['GET', '/api/server-logs'], ['POST', '/api/settings'],
  ['GET', '/api/state'], ['POST', '/api/git/push'], ['GET', '/api/files/read'],
  ['POST', '/api/some/route/added/next/year'],
];
check('owners reach everything, including routes that do not exist yet',
  EVERY_AREA.every(([m, p]) => mayAccess(owner, m, p)));

// ── Default deny ──────────────────────────────────────────────────────────
check('an unlisted route is refused for a collaborator',
  !mayAccess(collab, 'POST', '/api/some/route/added/next/year'));
check('a listed route with the wrong method is refused',
  !mayAccess(collab, 'DELETE', '/api/files/list'));

// ── The things that must never be reachable ───────────────────────────────
const FORBIDDEN = [
  ['GET', '/api/secrets'], ['POST', '/api/secrets'],
  ['GET', '/api/schedules'], ['POST', '/api/schedules'], ['POST', '/api/chat-schedule'],
  ['GET', '/api/settings'], ['POST', '/api/settings'], ['POST', '/api/auth/config'],
  ['POST', '/api/terminals/create'], ['POST', '/api/terminals/spawn'], ['GET', '/api/terminals'],
  ['GET', '/api/server-logs'], ['POST', '/api/maintenance'], ['POST', '/api/update'],
  ['POST', '/api/claude-update'], ['POST', '/api/import'], ['POST', '/api/tunnel'],
  ['GET', '/api/forwards'], ['GET', '/api/containers'], ['POST', '/api/mcp/call'],
  ['GET', '/api/stats'], ['GET', '/api/usage'], ['GET', '/api/clipboard'],
  ['POST', '/api/collaborators'], ['GET', '/api/collaborators'], ['GET', '/api/approvals'],
  // Git operations that reach past their own branch, or destroy work.
  ['POST', '/api/git/push'], ['POST', '/api/git/pull'],
  ['POST', '/api/git/discard'], ['POST', '/api/git/stageHunk'],
  // Their chat must not be switchable into a mode that drops the checks.
  ['POST', '/api/ui-sessions/abc/permission-mode'],
];
for (const [m, p] of FORBIDDEN) {
  check(`refused: ${m} ${p}`, !mayAccess(collab, m, p));
}

// ── The things they genuinely need ────────────────────────────────────────
const ALLOWED = [
  ['GET', '/api/state'], ['GET', '/api/projects'], ['GET', '/api/files/list'],
  ['GET', '/api/files/read'], ['POST', '/api/files/write'], ['POST', '/api/files/delete'],
  ['GET', '/api/git/info'], ['GET', '/api/git/diff'], ['POST', '/api/git/commit'],
  ['POST', '/api/collab/push'], ['POST', '/api/collab/request'], ['GET', '/api/collab/me'],
  ['GET', '/api/kanban'], ['POST', '/api/kanban'], ['GET', '/api/mindmap'],
  ['POST', '/api/ui-sessions/create'], ['POST', '/api/ui-sessions/abc/send'],
  ['GET', '/api/services'], ['POST', '/api/services'],
  ['GET', '/api/browsers'],
];
for (const [m, p] of ALLOWED) {
  check(`allowed: ${m} ${p}`, mayAccess(collab, m, p));
}

// ── Tool surfaces ─────────────────────────────────────────────────────────
for (const t of ['secret_get', 'secret_run', 'secret_search', 'schedule_add', 'schedule_delete',
  'spawn_terminal', 'terminal_input', 'enable_tunnel', 'delete_service', 'get_usage',
  'capture_display', 'send_file_to_user']) {
  check(`MCP tool withheld: ${t}`, !COLLABORATOR_MCP_TOOLS.has(t));
}
for (const t of ['register_service', 'start_service', 'get_service_logs', 'browser_navigate', 'kanban_add_task']) {
  check(`MCP tool available: ${t}`, COLLABORATOR_MCP_TOOLS.has(t));
}
// --tools under --restricted is an exact allowlist, so an omission here is a
// missing capability rather than a loose one — but Bash present with the path
// confinement is the deliberate trade, and it should not drift silently.
check('Bash is granted deliberately', COLLABORATOR_CLAUDE_TOOLS.includes('Bash'));
check('WebFetch is not granted', !COLLABORATOR_CLAUDE_TOOLS.includes('WebFetch'));

const deny = collaboratorSettings().permissions.deny.join(' ');
for (const cmd of ['sudo', 'systemctl', 'docker', 'apt', 'git push', 'git checkout',
  'git merge', 'git rebase', 'git reset', 'git branch', 'nohup']) {
  check(`denied in their Claude's settings: ${cmd}`, deny.includes(`Bash(${cmd}`));
}
check('the owner’s Claude config is unreadable to them', /Read\(\/\/home\/[^)]*\.claude/.test(deny));

// ── Enforcement points that are easy to delete ────────────────────────────
const webapp = read('src', 'webapp.js');
const ui = read('src', 'claude-ui.js');

check('the gate authorises, not just authenticates',
  /if \(principal && !mayAccess\(principal, req\.method, path\)\)/.test(webapp));
check('checkAccess returns a principal, not a boolean',
  /function checkAccess\(tok\)[\s\S]{0,2400}?role: ROLE_COLLABORATOR/.test(webapp));
check('collaborator expiry is re-checked per request',
  /collaborators\.liveByIdentity\(entry\.collabKey\)[\s\S]{0,120}?tokens\.delete\(tok\)/.test(webapp));
check('a refresh cannot outlive the invitation',
  /entry\.collabKey && !collaborators\.liveByIdentity\(entry\.collabKey\)\.length/.test(webapp));
check('getProject is shadowed so new routes inherit the scope',
  /const getProject = \(alias\) => \{[\s\S]{0,400}?principal\.projects\.includes/.test(webapp));
check('the absolute-path escape is REMOVED for a collaborator, not checked after',
  /if \(isConfined\(principal\)\) return resolve\(root, '\.' \+ sep/.test(webapp));
check('state is filtered per principal before it is sent',
  /function filterStateFor\(principal, state\)[\s\S]{0,2000}?userTerminals: \[\]/.test(webapp));
check('the websocket refuses terminal traffic',
  /confined && \(msg\.type === 'subscribe'[\s\S]{0,160}?return;/.test(webapp));
check('the websocket checks chat ownership before replaying a transcript',
  /if \(!ownsUiSession\(sid\)\) return;/.test(webapp));
check('the server log is owner-only',
  /subscribe-logs[\s\S]{0,300}?if \(confined\) return;/.test(webapp));
check('the forward cookie is withheld from collaborators',
  /!hasForwardCookie\(req\)\s*\n?\s*&& !isConfined\(principal\)/.test(webapp));
check('a collaborator MCP key is checked against the tool list',
  /COLLABORATOR_MCP_TOOLS\.has\(body\.tool\)/.test(webapp));
check('the MCP alias is forced, not trusted',
  /const allowed = collaborators\.liveByIdentity\(mcpCollabKey\)[\s\S]{0,300}?Unknown project/.test(webapp));

// ── The alias hole ────────────────────────────────────────────────────────
// Found by driving a real server, not by reading: the route policy ALLOWS
// /api/kanban, and the handler passed the caller's `project` string straight
// to the store without ever resolving a project — so getProject's scoping
// never ran and `?project=<someone-else's>` returned their board with a 200.
// The same shape exists wherever a route takes an alias rather than a project.
check('query aliases are checked at one choke point',
  /for \(const k of \['project', 'alias'\]\)[\s\S]{0,200}?aliasDenied\(v\)/.test(webapp));
check('the kanban body alias is checked',
  /path === '\/api\/kanban' && req\.method === 'POST'[\s\S]{0,400}?aliasDenied\(project\)/.test(webapp));
check('the mindmap body alias is checked',
  /path === '\/api\/mindmap' && req\.method === 'POST'[\s\S]{0,300}?aliasDenied\(body\.project\)/.test(webapp));
check('a mindmap node is checked by its OWN project, not the body\u2019s',
  /mindmap\.projectOfNode\(body\.id\)/.test(webapp));
check('the mindmap read route is scoped for a collaborator',
  /if \(isConfined\(principal\)\)[\s\S]{0,260}?principal\.projects\.map\(p => mindmap\.getMindmap\(p\)\)/.test(webapp));
check('a service must live inside their worktree',
  /A service must run inside your own working folder/.test(webapp));

check('their chat launches --restricted', /args\.push\('--restricted'\)/.test(ui));
check('their chat gets an exact tool allowlist',
  /args\.push\('--tools', COLLABORATOR_CLAUDE_TOOLS\.join\(','\)\)/.test(ui));
check('their chat gets settings it cannot override',
  /args\.push\('--settings', JSON\.stringify\(collaboratorSettings\(\)\)\)/.test(ui));
check('their chat cannot be launched with skip-permissions',
  /if \(skipPermissions && !collaborator\)/.test(ui));
check('their chat gets its own MCP key, not the internal one',
  /collaborator \? \(collaborator\.apiKey \|\| apiKey\) : apiKey/.test(ui));
check('the owner’s private prompt layers are withheld',
  /userLayers: collaborator \? false : userLayers/.test(ui));
check('they cannot answer their own permission prompts',
  /if \(s\.collaborator && !isQuestion\)[\s\S]{0,260}?escalateCb/.test(ui));

console.log(failures ? `\n${failures} failure(s)` : '\nAll collaborator-access checks passed.');
process.exit(failures ? 1 : 0);
