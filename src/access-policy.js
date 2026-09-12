/**
 * access-policy.js — who may call what.
 *
 * Until now Crundi had exactly one trust level. `validateToken` returned a
 * boolean, and past that single line every one of ~120 route handlers was
 * reachable: a shell, the filesystem above project roots, secrets, services,
 * the server log. That is a defensible design for a single-owner tool and the
 * README says so outright.
 *
 * Outside collaborators break that assumption, so this file introduces the
 * missing half: a PRINCIPAL (who is calling) and a POLICY (what that principal
 * may reach).
 *
 * ─── Default deny ───
 *
 * The collaborator policy is an allowlist, and deliberately so. A denylist over
 * 120 routes is wrong the first time someone adds route 121 — the new route is
 * reachable by default and nothing fails loudly. With an allowlist a forgotten
 * route is merely broken for collaborators, which someone reports, rather than
 * silently exposed, which nobody does.
 *
 * ─── This is not the only boundary ───
 *
 * Route access is necessary and nowhere near sufficient. A collaborator's
 * request that reaches an allowed route still has to be SCOPED — to their
 * project, and to their worktree — by the handler itself. See scopeForPath()
 * and the callers of requireScope() in webapp.js. Hiding a tab in the browser
 * is presentation, not security, and is not counted here at all.
 */

/** Roles, most privileged first. */
export const ROLE_OWNER = 'owner';
export const ROLE_COLLABORATOR = 'collaborator';

/**
 * Routes a collaborator may reach, as [method, matcher] pairs.
 *
 * A matcher is a string (exact path) or a RegExp (tested against the path).
 * Anything not listed here is refused with 403 before the handler runs.
 *
 * Every entry carries the reason it is safe, because the reason is the part
 * that rots. "Scoped" means the handler narrows the request to the caller's
 * project or worktree and would be unsafe without that narrowing.
 */
const COLLABORATOR_ROUTES = [
  // ─── Session plumbing ───
  ['POST', '/api/auth/refresh'],            // their own token family only
  ['POST', '/api/auth/logout'],
  ['GET', '/api/auth/methods'],             // pre-gate anyway; listed for clarity
  ['GET', '/api/auth/sessions'],            // scoped: their own families
  ['POST', '/api/auth/sessions/revoke'],    // scoped: their own families
  ['GET', '/api/status'],                   // scoped: trimmed for collaborators
  ['GET', '/api/state'],                    // scoped: buildState filters by principal
  ['GET', '/api/events'],                   // scoped: same filter as /api/state

  // ─── Their project ───
  ['GET', '/api/projects'],                 // scoped: only theirs is returned

  // ─── Files, inside their worktree only ───
  // resolveFsPath's absolute-path escape is closed for collaborators and the
  // root is their worktree, not the project. See scopeForPath().
  ['GET', '/api/files/list'],
  ['GET', '/api/files/read'],
  ['GET', '/api/files/search'],
  ['GET', '/api/files/download'],
  ['POST', '/api/files/download-link'],
  ['POST', '/api/files/write'],
  ['POST', '/api/files/upload'],
  ['POST', '/api/files/delete'],
  ['POST', '/api/resolve-file-path'],
  ['POST', '/api/attachments'],

  // ─── Git: read, stage, commit. Nothing that moves anyone else. ───
  //
  // A worktree has its own HEAD on its own branch, so committing here cannot
  // move the branch you are standing on. These are the operations that stay
  // inside that guarantee.
  //
  // Push is NOT here. It is /api/collab/push below, which pushes their branch
  // and refuses any other — `git push` with the wrong argument is exactly the
  // operation that reaches main. Pull, discard and stageHunk are absent too:
  // pull rewrites their tree from a remote, and discard destroys work with no
  // undo. See COLLABORATOR_NEVER.
  ['GET', '/api/git/info'],
  ['GET', '/api/git/diff'],
  ['POST', '/api/git/stage'],
  ['POST', '/api/git/unstage'],
  ['POST', '/api/git/commit'],
  ['POST', '/api/collab/push'],       // their branch only; see handler
  ['GET', '/api/collab/me'],          // who am I, when does it end, what branch
  ['POST', '/api/collab/request'],    // ask the owner for something refused

  // ─── Board and map, scoped to their project ───
  ['GET', '/api/kanban'],
  ['POST', '/api/kanban'],
  ['GET', '/api/mindmap'],                  // scoped: project passed to the store
  ['POST', '/api/mindmap'],
  ['GET', '/api/media/list'],
  [/^(GET|POST)$/, /^\/api\/media\//],

  // ─── Chat. Not terminals. ───
  // The CLI is reachable only as a chat cell: no PTY, so no shell that is not
  // mediated by Claude Code's own permission checks.
  ['POST', '/api/ui-sessions/create'],
  ['GET', '/api/ui-sessions/resumable'],
  [/^(GET|POST)$/, /^\/api\/ui-sessions\/[^/]+\/(send|cancel-queued|respond|answer-closed|interrupt|close|rename|model|history|dismiss-agents)$/],

  // ─── Services and browsers: the sanctioned way to run things ───
  // A collaborator gets no terminal, so a dev server has to be a registered
  // service. That is the point: it is visible, named, stoppable, and it does
  // not die with their chat.
  ['GET', '/api/services'],                 // scoped: their project's only
  ['POST', '/api/services'],                // scoped: forced onto their project
  [/^(GET|POST)$/, /^\/api\/services\/[^/]+\/(start|stop|restart|logs)$/],
  ['GET', '/api/browsers'],
  [/^(GET|POST)$/, /^\/api\/browsers\//],

  ['POST', '/api/push/subscribe'],
];

/**
 * Routes a collaborator must never reach, even if a pattern above widens.
 *
 * Belt and braces: the allowlist is the real control, but these are the ones
 * where a mistake is unrecoverable rather than merely wrong, so they are
 * refused a second time, explicitly, by name.
 */
const COLLABORATOR_NEVER = [
  // Git operations that reach past their own branch, or destroy work.
  // `git push` unqualified can update main; pull rewrites their tree from a
  // remote; discard throws away changes with no undo. A collaborator who needs
  // one of these asks, through /api/collab/request.
  /^\/api\/git\/push$/,
  /^\/api\/git\/pull$/,
  /^\/api\/git\/discard$/,
  /^\/api\/git\/stageHunk$/,
  /^\/api\/secrets/,        // decryption, and the agent-approval queue
  /^\/api\/schedules/,      // arbitrary commands on a timer
  /^\/api\/chat-schedule/,
  /^\/api\/settings/,
  /^\/api\/auth\/config/,   // would let them turn auth off
  /^\/api\/terminals/,      // every PTY path, including /spawn
  /^\/api\/server-logs/,    // other people's projects scroll past in here
  /^\/api\/maintenance/,
  /^\/api\/update/,
  /^\/api\/claude-update/,
  /^\/api\/import/,
  /^\/api\/tunnel/,         // exposing a port to the public internet
  /^\/api\/forwards/,
  /^\/api\/containers/,     // shared docker daemon: other projects' containers
  /^\/api\/mcp\/call/,      // has its own key; collaborators get a scoped one
  /^\/api\/stats/,          // host-level machine detail
  /^\/api\/usage/,          // the owner's account spend
  /^\/api\/notify/,
  /^\/api\/clipboard/,      // reads the host machine's clipboard
];

function matches(matcher, value) {
  return matcher instanceof RegExp ? matcher.test(value) : matcher === value;
}

/**
 * May this principal call this route?
 *
 * Owners: yes, unchanged — this must stay a no-op for the existing single-user
 * install, or every route gains a new way to break.
 */
export function mayAccess(principal, method, path) {
  if (!principal) return false;
  if (principal.role !== ROLE_COLLABORATOR) return true;
  if (COLLABORATOR_NEVER.some(re => re.test(path))) return false;
  return COLLABORATOR_ROUTES.some(([m, p]) => matches(m, method) && matches(p, path));
}

/** True when this principal is confined to one project and one folder. */
export function isConfined(principal) {
  return !!principal && principal.role === ROLE_COLLABORATOR;
}

/**
 * The MCP tools a collaborator's Claude may call.
 *
 * The internal API key reaches every MCP tool, including secret_get and
 * secret_run, and it is handed to every spawned session through .mcp.json. A
 * collaborator's session gets its own key instead, and that key is checked
 * against this list.
 *
 * Services and the browser tools are here on purpose — they are what the
 * collaborator is told to ask Claude for, and they are the reason they do not
 * need a shell.
 */
export const COLLABORATOR_MCP_TOOLS = new Set([
  // Services: the sanctioned way to run a dev server.
  'list_services', 'register_service', 'start_service', 'stop_service', 'restart_service', 'get_service_logs',
  // Browser automation against a real browser.
  'browser_open', 'browser_navigate', 'browser_click', 'browser_type', 'browser_fill',
  'browser_select', 'browser_eval', 'browser_snapshot', 'browser_elements', 'browser_console',
  'browser_network', 'browser_cookies', 'browser_pdf', 'browser_read_page', 'browser_screenshot',
  'browser_scroll', 'browser_resize', 'browser_wait', 'browser_go_back', 'browser_go_forward',
  'browser_view_source', 'browser_list', 'browser_close', 'browser_mouse',
  // Their own project's board and map.
  'kanban_list', 'kanban_list_column', 'kanban_get_task', 'kanban_add_task', 'kanban_update_task',
  'kanban_move_task', 'kanban_delete_task', 'kanban_add_todo', 'kanban_update_todo', 'kanban_delete_todo',
  'mindmap_list', 'mindmap_search', 'mindmap_get_subtree', 'mindmap_get_children', 'mindmap_get_ancestors',
  'mindmap_add_node', 'mindmap_update_node', 'mindmap_move_node', 'mindmap_delete_node',
  'mindmap_add_note', 'mindmap_remove_note', 'mindmap_link_node', 'mindmap_unlink_node',
  // Syntax check is pure and local.
  'syntax_check',
]);

/**
 * Tools a collaborator's Claude may use from Claude Code's built-in set.
 *
 * Passed to --tools, which under --restricted is an EXACT allowlist rather
 * than an addition — verified against 2.1.269, where naming five tools yielded
 * exactly five. So everything wanted has to appear here, including the ones
 * that are normally implicit.
 *
 * Bash is present because development is not possible without it, and because
 * --restricted confines its read and write paths to the working directories
 * and refuses commands it cannot analyse (command substitution is rejected
 * outright rather than escalated). That is a real boundary, not a prompt.
 */
export const COLLABORATOR_CLAUDE_TOOLS = [
  'Bash', 'Read', 'Write', 'Edit', 'NotebookEdit', 'Glob', 'Grep',
  'Task', 'TaskOutput', 'TaskStop', 'Skill', 'ToolSearch', 'WebSearch',
];

/**
 * Settings applied to a collaborator's Claude session via --settings.
 *
 * --restricted ignores the user, project and local settings files, so a
 * collaborator cannot write themselves an allow-rule; --settings still
 * applies, which makes this the one ruleset they cannot edit.
 *
 * These deny rules are a second line, not the first. Their job is the commands
 * that are perfectly analysable and perfectly legitimate-looking, which the
 * path confinement therefore has no reason to stop.
 */
export function collaboratorSettings() {
  return {
    permissions: {
      deny: [
        // This box has passwordless sudo. Nothing else on the list matters if
        // this one is reachable.
        'Bash(sudo:*)', 'Bash(su:*)', 'Bash(doas:*)',
        // The service manager, the container daemon and the package manager
        // all reach the whole machine, not just this project.
        'Bash(systemctl:*)', 'Bash(journalctl:*)', 'Bash(service:*)',
        'Bash(docker:*)', 'Bash(docker-compose:*)', 'Bash(podman:*)',
        'Bash(apt:*)', 'Bash(apt-get:*)', 'Bash(dpkg:*)', 'Bash(snap:*)',
        // Crundi's own control surfaces.
        'Bash(crundi:*)', 'Bash(pm2:*)',
        // Credentials and other people's work live under these.
        'Read(//home/crundi/.claude/**)', 'Read(//home/crundi/.config/crundi/**)',
        'Read(//home/crundi/.ssh/**)', 'Read(//root/**)', 'Read(//etc/shadow)',
        'Edit(//home/crundi/.claude/**)', 'Edit(//home/crundi/.config/crundi/**)',
        // A dev server started from a shell outlives nothing and is visible to
        // no one. Services exist for this.
        'Bash(nohup:*)', 'Bash(disown:*)',
        // Git, through Bash, reaches the same places the HTTP routes were
        // stopped from reaching. Their worktree is on its own branch and is
        // theirs to commit to; everything that leaves that branch is the
        // owner's to do. `push` is denied outright rather than pattern-matched
        // on a ref, because the refspec forms are too many to enumerate and
        // the Push button exists for the one case that is wanted.
        'Bash(git push:*)', 'Bash(git checkout:*)', 'Bash(git switch:*)',
        'Bash(git merge:*)', 'Bash(git rebase:*)', 'Bash(git reset:*)',
        'Bash(git branch:*)', 'Bash(git worktree:*)', 'Bash(git remote:*)',
        'Bash(git clean:*)', 'Bash(git cherry-pick:*)', 'Bash(git tag:*)',
        'Bash(git config:*)', 'Bash(git filter-branch:*)', 'Bash(git update-ref:*)',
      ],
    },
  };
}
