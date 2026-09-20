# crundi MCP tool catalogue

Tools are exposed as `mcp__crundi__<name>`. Everything is scoped to the project
the session is running in, except secrets and services, which are machine-wide.

**Availability is platform-specific.** Check the host before reaching for a tool;
half of these cannot work on a headless server:

| Group | Linux server (headless) | Windows headless | Windows desktop app |
|---|---|---|---|
| Messaging, services, forwards, tunnels, stores, secrets, usage | yes | yes | yes |
| Browser (`browser_*`) | yes — bundled headless browser | yes | yes — app webview |
| Terminals (`spawn_terminal` …) | **no** — graceful error | **no** | yes |
| Screen capture (`capture_*`, `list_windows`, `list_displays`) | **no** — needs a display | **no** | yes |
| `disconnect_rdp` | **no** | **no** | yes — Windows only |

On any headless box, use `Bash` for commands and a registered service for anything
long-lived. See `platform-linux.md` / `platform-windows.md`.

## Messaging

| Tool | Notes |
|---|---|
| `send_message_to_user` | Out-of-band; arrives when nobody is watching the chat |
| `send_photo_to_user` | Image by path |
| `send_file_to_user` | Any file by path |

## Services

| Tool | Arguments |
|---|---|
| `list_services` | — |
| `register_service` | `name`, `command`, `cwd?`, `stopCommand?` → key `alias::name` |
| `start_service` / `stop_service` / `restart_service` | `key` |
| `get_service_logs` | `key`, `lines?` (default 50) |
| `delete_service` | `key` |

## Exposure

| Tool | Arguments |
|---|---|
| `list_forwards` | — (also reports the domain and available modes) |
| `add_forward` | `name`, `port`, `mode?` (`subdomain` \| `path`), `public?` (default false), `description?` |
| `remove_forward` | `host` |
| `enable_tunnel` | `key`, `port` — Cloudflare, unstable hostname |
| `disable_tunnel` | `key` |

A private forward requires a Crundi sign-in. Only mark a forward public when the
thing behind it has its own authentication.

## Browser

`browser_open` (`url`, `name?`, `width?`, `height?`) returns a `key` used by all
the rest. Max 5 per project.

Navigation: `browser_navigate`, `browser_go_back`, `browser_go_forward`,
`browser_wait` (`selector`, `timeout?`), `browser_close`, `browser_list`.

Interaction: `browser_click` (`selector`), `browser_type` (`text`, `selector?`),
`browser_fill` (`selector`, `value`), `browser_select` (`selector`, `value`),
`browser_mouse` (`x`, `y`, `action`), `browser_scroll`, `browser_resize`.

Inspection: `browser_eval` (`code`), `browser_snapshot` (accessibility tree),
`browser_elements`, `browser_read_page`, `browser_view_source`,
`browser_screenshot`, `browser_console` (`clear?`, `countOnly?`),
`browser_network` (`action`: start \| stop \| log \| clear), `browser_cookies`,
`browser_pdf`.

Paged readers (`browser_console`, `browser_elements`, `browser_network`,
`terminal_output`) take `start` / `end` / `countOnly` — ask for the count first
when output might be huge.

## Terminals — Windows desktop app only

`spawn_terminal` (`name`, `command?`, `cwd?`), `terminal_input` (`name`, `input`),
`terminal_output` (`name`, paging), `terminal_wait` (`name`, `pattern`,
`timeout?`), `close_terminal`, `list_terminals`.

## Screens — needs a desktop session (Windows app)

`list_windows`, `list_displays`, `capture_window` (`windowId`), `capture_display`
(`display`), `send_window_screenshot_to_user`, `send_display_screenshot_to_user`.

## Kanban

Statuses: `backlog`, `todo`, `in_progress`, `done`.

| Tool | Notes |
|---|---|
| `kanban_list_column` | Cheapest read — summaries for one column |
| `kanban_get_task` | One task in full, with linked mindmap nodes |
| `kanban_list` | Whole board; can be large |
| `kanban_add_task` | `title`, `description?`, `status?`, `todos?` |
| `kanban_update_task` / `kanban_move_task` | Move columns; `index` reorders within one |
| `kanban_add_todo` / `kanban_update_todo` | Checklist items |
| `kanban_delete_task` / `kanban_delete_todo` | Soft delete — recoverable |
| `kanban_restore_task` / `kanban_restore_todo` | Undo a delete |
| `kanban_history` | Immutable change log |

## Mindmap

| Tool | Notes |
|---|---|
| `mindmap_search` | Keyword hits with breadcrumbs — start here on a large map |
| `mindmap_get_children` / `mindmap_get_subtree` / `mindmap_get_ancestors` | Walk it cheaply |
| `mindmap_list` | Everything in scope; pass `compact: true` for a skeleton |
| `mindmap_add_node` | `text`, `parentId?`, `notes?`, `taskId?`, `todoId?` |
| `mindmap_update_node` | `notes` REPLACES the whole list |
| `mindmap_add_note` / `mindmap_remove_note` | Append / remove one note |
| `mindmap_move_node` | Reparent and reorder; cycles rejected |
| `mindmap_link_node` / `mindmap_unlink_node` | Tie a node to a task or subtask |
| `mindmap_delete_node` | Deletes descendants too — not soft |

The visible map is only this project's nodes. Unscoped and other-project nodes
are invisible, so treat "not found" as "not linked here", and say that rather
than telling the user their note does not exist.

## Schedules

`schedule_list`, `schedule_get`, `schedule_add`, `schedule_update`,
`schedule_set_enabled`, `schedule_delete`.

A schedule fires **only at its exact scheduled minute** — there is no past-due
catch-up — and only when all its extra conditions are met. The action runs as a
workbench terminal panel (agent or command) or starts/stops a service.

## Media

`media_list`, `media_get`, `media_add_path` (`path`, `name?`, plus `taskId` /
`todoId` / `nodeId` to link it), `media_delete`. Each item reports an absolute
`path` so a file can be read directly.

## Secrets

| Tool | Approval | Use |
|---|---|---|
| `secret_search` | none | Discover names, descriptions, ids — never values |
| `secret_run` | PIN, blocks ~3 min | Run a command with the value bound to an env var |
| `secret_get` | PIN, blocks ~3 min | Only when the value itself is genuinely needed |

`secret_run` takes `command`, `id`\|`name`, `envName?` (default `SECRET`), `cwd?`,
`timeoutMs?`, `reason`. Reference the secret as `$SECRET` in the command; output
comes back with the value redacted. The user sees the exact command before
approving, so write it plainly.

Each secret is sealed with a key derived from its own PIN, which is never stored —
a lost PIN means delete and recreate, with no recovery. Names and descriptions
are stored in the clear and are searchable without authentication; never put a
value, or anything you would not show a collaborator, in a description.

## Miscellaneous

| Tool | Notes |
|---|---|
| `get_usage` | Real account usage: 5-hour and weekly utilisation, reset times, tier. `force` bypasses the 60s cache |
| `syntax_check` | `files` (absolute paths); language detected by extension |
| `request_owner_command` | Outside collaborators only: ask the owner to run one command the sandbox cannot |
| `disconnect_rdp` | Drop an RDP session while keeping the GUI alive for capture |
