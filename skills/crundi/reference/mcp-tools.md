# crundi MCP tool catalogue

Tools are exposed as `mcp__crundi__<name>`. Everything is scoped to the project
the session is running in, except secrets and services, which are machine-wide.

**Availability is platform-specific.** Check the host before reaching for a tool;
half of these cannot work on a headless server:

| Group | Linux server (headless) | Windows headless | Windows desktop app |
|---|---|---|---|
| Messaging, services, forwards, tunnels, stores, secrets, usage | yes | yes | yes |
| Browser (`browser_*`) | yes — bundled headless browser | yes | yes — app webview |
| Screen capture (`capture_*`, `list_windows`, `list_displays`) | **no** — needs a display | **no** | yes |
| `disconnect_rdp` | **no** | **no** | yes — Windows only |

On any headless box, use `Bash` for commands and a registered service for anything
long-lived. See `platform-linux.md` / `platform-windows.md`.

## Messaging

| Tool | Notes |
|---|---|
| `send_message_to_user` | Out-of-band; arrives when nobody is watching the chat |
| `alert_user` | `title`, optional `body`, `level` (`urgent` default, or `normal`). Rings the paired phone for the person's ring time (5 min default, 1 to 15) and waits for the answer: `answer` is attended, dismissed, snoozed (`snoozeMinutes`), `no answer` or `not confirmed`. Call it from a background agent if other urgent work must go on meanwhile. Only for what must be acted on or known now; at most 3 urgent an hour (then sent without ringing) and 20 alerts an hour. With no phone it goes through the other channels. |
| `send_photo_to_user` | Image by path, to Telegram: for when they are not looking at the chat |
| `show_image` | Draws images in the chat itself, under the call. `path`, or `paths` (up to 8, strings or `{path, caption}`), optional `caption`. PNG/JPEG/GIF/WebP, 10 MB each; files are copied. |
| `show_video` | A video player in the chat, with a poster frame. Same arguments. MP4/MOV/WebM, 250 MB each; copied. |
| `show_audio` | An audio player in the chat. Same arguments. MP3/WAV/OGG/M4A/FLAC/WebM, 60 MB each; copied. |
| `show_file` | A download card in the chat: the file's icon, name, size and a Download button. Same arguments, any file. Not copied: it downloads the file as it is when the button is pressed. |
| `show_embed` | A live, sandboxed frame in the chat. `url`: an ordinary https link; YouTube, Vimeo, Dailymotion, Loom, X, Instagram, TikTok, Facebook, Reddit, Spotify, SoundCloud, CodePen, gists and Google Maps embed links become proper embeds, anything else is framed as it is (many sites refuse). `html`: your own HTML/CSS/JS (max 512 KB), which cannot read files or call Crundi. Optional `title`, `caption`, `height` (80 to 1200). |
| `speak` | Say something aloud to the person in this chat, in Crundi's voice. Only for a message that starts with "voice input:" (spoken in the chat's voice mode): a short spoken version of the answer, one to three sentences of plain speech. `text`. The written answer still goes in the chat. |

The four `show_*` tools exist only in chat (UI) mode, in owner chats. **Prefer them over the `send_*_to_user` tools whenever the person is in the chat**: `send_*` goes to Telegram and is for reaching someone who is not looking.
| `send_file_to_user` | Any file by path. Attached on Telegram up to 50 MB (a link beyond that); always returns a 30-minute download link and says how it was delivered. If nothing went out of band, put the link in your reply. |

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

Use the browser drawer first: when `browser_list` says it is available, open a
drawer tab, unless told otherwise; else `on: "server"`. A dev server on this
machine needs no forward: a drawer tab with `network: "server"` (or
`on: "server"`) opens `http://127.0.0.1:<port>` directly. Use the person's own
network only when they ask to test through it. When the PC app runs on this
same machine (server network "Server (this PC)"), client and server are one
network. For a phone or tablet layout, `browser_resize` the drawer tab to the
device's size (it keeps a desktop user agent; the person can pick a full device
profile in the drawer's device menu).

`browser_open` (`url`, `name?`, `width?`, `height?`, `on?`) returns a `key` used
by all the rest. Max 5 per project. `on` is `"server"` (this machine: its
localhost is your dev servers) or `"desktop"` (the person's connected desktop
app, on their network); omitted, the desktop app when one is connected. Every
later call on that key runs in the same place; `browser_list` shows where.

Drawer tabs: `on: "drawer"` opens a visible tab in this project's browser drawer
in the PC client (`network?`: `"client"` default, `"server"`, or `"homepool:<network>"`, which asks the owner first; `show?`).
`browser_list` also returns the drawer tabs the person opened (key
`drawer:<id>`, `network`, `active`, `shown`). Drawer-only tools:
`browser_point` (`key`, `x`/`y` or `selector`, `label?`, `show?`),
`browser_annotate` (`key`, `shapes`: `{ kind: pin|box|arrow|label, x, y, width?,
height?, toX?, toY?, selector?, text? }`, `clear?`, `show?`),
`browser_annotations` (`key`, `image?`: the person's and your marks, the element
under each, and an annotated screenshot), `browser_tab_network` (`key`,
`network`: the page reloads on the new network). Coordinates are page CSS pixels
in the viewport. Collaborators' chats cannot use the drawer.

Navigation: `browser_navigate`, `browser_go_back`, `browser_go_forward`,
`browser_wait` (`selector`, `timeout?`), `browser_close`, `browser_list`.

Interaction: `browser_click` (`selector`; or `text` with `role?` and `within?`,
as a person names it, e.g. text "Add to Shopping List", role button, and an
open dialog is searched first; or `x`,`y` for a real mouse click; `nth` picks
among several matches). It returns what it clicked (tag, role, text, x, y) and
warns when something covers it: check it is what you meant. When nothing
matches it lists what is clickable. `browser_type` (`text`, `selector?`),
`browser_fill` (`selector`, `value`), `browser_select` (`selector` or `label`;
`value`: the option's text or value, or `index`; a native `<select>`'s list
opens outside the page, where the person sees it but you cannot: click it
open for the person to choose, or choose with this),
`browser_mouse`, `browser_scroll`, `browser_resize`.

Mouse: `browser_mouse` (`x`, `y`, `action`: move \| click \| double_click \|
down \| up \| drag; `button` left \| right \| middle; `drag` takes `toX`/`toY`
and `duration`). It is real mouse input, hit-tested like a person's mouse, so
it reaches what selectors cannot: a cross-site `<iframe>` (a CodePen or
JSFiddle preview, an embedded payment or sign-in form), a canvas, a slider. The
pointer remembers where it is and which button is held, so `down`, any number
of `move`s and `up` (with or without a point) can be separate calls; moves
while held send the whole path, as a drag needs. In a drawer tab the person
sees your cursor glide to each point, press and release; it hides by itself a
moment after you stop, and stays while a button is held, so release it when
you are done.

Touch: `browser_touch` (`x`, `y`, `action`: tap \| double_tap \| long_press \|
swipe \| pinch). A swipe takes `toX`/`toY`, or `direction` (where the finger
moves: `"up"` scrolls the content down) and `distance`; a short `duration`
flings, a long one drags. A pinch takes `scale` around `x`,`y`. Use it for
carousels, bottom sheets, pull-to-refresh, swipe-to-dismiss: anything that
listens for touch or pointer events from a finger, which `browser_mouse` and
`browser_scroll` never fire. Resize to a phone size first; the first touch
turns on touch emulation, so reload if the page only checks for touch at load.

Every `browser_touch` returns a `report` of what took the gesture; read
`report.summary` first. It names the element under the finger (and any overlay
on top of the one you meant), `touch-action` up to the nearest scroller, each
`preventDefault` / `stopPropagation` / `setPointerCapture` made during the
gesture with the script line that made it, the touch and pointer listeners on
that path (`report.listeners`), and what actually scrolled, moved or zoomed.
When a swipe "does nothing", this is how to find out what swallowed it.

Inspection: `browser_eval` (`code`), `browser_snapshot` (accessibility tree),
`browser_elements`, `browser_read_page`, `browser_view_source`,
`browser_screenshot`, `browser_console` (`clear?`, `countOnly?`),
`browser_network` (`action`: start \| stop \| log \| clear), `browser_cookies`,
`browser_pdf`.

Paged readers (`browser_console`, `browser_elements`, `browser_network`) take `start` / `end` / `countOnly` — ask for the count first
when output might be huge.

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

## Skills

Claude skills are folders with a `SKILL.md`. Claude Code reads them from
`~/.claude/skills/<name>/` (every project on the machine) and
`<project>/.claude/skills/<name>/` (that project only). These tools manage both;
the user has the same thing under Settings → Skills.

| Tool | Use |
|---|---|
| `skill_list` | What is installed: `name`, `scope`, `description`, `kind`. `scope?` narrows it |
| `skill_get` | One skill's file list and `SKILL.md`; `path` reads another file in it |
| `skill_install` | Install from `path` (a skill folder, `.zip`, `.skill` or `SKILL.md` on this machine) or `content` (the text of a `SKILL.md`) |
| `skill_delete` | Remove a skill and its files. No undo |

`scope` is `global` (the default), `project` (this chat's project) or a project
alias. `kind` says what may be done to a skill: only `user` skills can be
changed or deleted. `bundled` skills ship with Crundi and are replaced on every
update, `synced` ones are downloaded by Claude Code from the signed-in account,
and `linked` ones are symlinks — all three are read-only here.

`skill_install` never replaces silently: if the name is taken it fails with
`conflict: true`. Repeat it with `overwrite: true` only when the user wants the
existing skill replaced. A name that belongs to a bundled skill is refused
outright — install under another `name`.

A chat loads its skills when it starts. Installing or deleting one does not
change a chat that is already running, including this one, and nothing here
restarts a chat; tell the user a new chat will pick it up.

## This chat

| Tool | What it does |
|---|---|
| `rename_chat` | Renames the chat or terminal you are running in. `title` (at most 20 characters, shown everywhere) and `long` (at most 50, shown where a pane's header has room). Call it first when the person gives a task, and not again until the scope or topic changes. Crundi also names an unnamed chat by itself as a backstop. Refused if the person renamed it themselves. Owner chats only. |

## Widgets

Live UI panels you author and Crundi shows in its own interface. Owner chats only. Read `widget_guide` before building one; the `crundi-widgets` skill covers when to.

| Tool | What it does |
|---|---|
| `widget_guide` | The authoring reference: files, the `crundi` frame API, tokens, kit classes, sources, slots, an example. |
| `widget_open` | Create, update or re-open a widget. `id`, `slot` (`dock`, `cell`, `tab`, `inline`, `chip`), optional `html` / `manifest` / `fixtures` / `data`. Returns `sourceDir`: edit the files there and it hot-reloads. |
| `widget_set_data` | Push a value: `value` (replace), `merge` (deep-merge, `null` deletes) or `append` (with `max`). |
| `widget_render` | Screenshots in Crundi's real chrome plus a lint report. `frames` (`cell`, `dock`, `dock-wide`, `tab`, `inline`, `chip`, `mobile`, `mobile-dock`, `mobile-tab`, `mobile-chip`), `states` (fixture names). Always run it before calling a widget done. |
| `widget_get` | Status: actual slot, source errors, faults from the live page, approval state, and events the person triggered (reading clears them). |
| `widget_list` | This project's widgets. |
| `widget_close` | Close (reopenable); `remove: true` deletes it and its files. |
| `widget_rollback` | Restore an earlier version of the source. |
| `widget_scope` | Make a widget global (shown in every project, pinned; as a `chip` it stays in the top bar everywhere) or return it to its project with `global: false`. Going global copies the widget's own folder (source plus any data kept in it) into Crundi's store, which is backed up; the original folder stays in the project, unread, so writers of that data must be repointed to the new `sourceDir`. Project data is not copied and is still read in place. |

A widget's frame has no network and no storage. Data comes from sources named in `widget.json` (`push`, `session`, `file`, `sqlite`, `command`, `http`, `crundi`, the last including this machine's live `stats`); `command` and `http` sources, files outside the project and `command` / `tool` / `prompt` actions need the owner's approval, shown on the widget.

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
