# native-v2: the plan

Every area and screen of the PC client (`native-gpui/AREAS.md`,
`native-gpui/PLAN.md`; the reference is the web UI and the Electron app),
where it stands in v2, and its measurement. Built on `ARCHITECTURE.md`;
owned as `AREAS.md` says.

Status: **built** (in v2, its suite passes on the server), **slice** (in
the vertical slice: built for the architecture's proof, its full detail
still to port), **to do**. Measured: the suite's frames (p50 / p95 /
share ≤ 7 ms while it scrolls or streams; idle frames) — numbers that count
are the owner's PC, release build; "—" not yet.

## F. Foundation (Architect, this step)

- [x] `ARCHITECTURE.md` (the seven rules, the state model, the view tree
  and its caching, virtualised lists, the live link, background parsing,
  animations, the frame budget), `AREAS.md`, this plan
- [x] the Cargo workspace, versions pinned (gpui-pre =0.3.12 with x11 and
  wayland, gpui-component and gpui-kit-assets =0.10.0, cef =156.4.0 only
  in `crates/browser-host`, not a default member)
- [x] `tools/build.sh` (the shared target folder, two jobs, flock,
  oom_score_adj 1000, the 20 GB floor), `tools/check.sh` (no
  `refresh_windows()` / `window.refresh()`; run by build.sh and CI)
- [x] CI `.github/workflows/native-v2.yml`: the check, build, unit tests,
  smoke run (driver answers, sign-in page, idle draws no frames, a
  picture) and the Linux package, on Linux, Windows and macOS, as
  native-gpui's
- [x] the frame harness: `tools/frames.mjs` (ZED_MEASUREMENTS frame
  durations → p50, p95, share ≤ 7 ms, idle count), `test/lib.mjs`'s
  `T.frames`, `tools/smoke.mjs`'s idle check
- [x] core ported, with the live link's heartbeat (ping 10 s, silence
  25 s → reconnect, a new event stream with it; unit test against a
  silent fake server) and the conversation's changed-entry tracking
  (O(1) entry lookup, deltas appended in place)
- [x] the live hub: one channel awaited on the UI thread, one batch per
  frame (unit test: a burst of 201 events is one update; the next waits
  a frame)
- [x] the vertical slice (slice): the shell (top bar, sidebar, project
  bar, sign-in), the workbench (mosaic, heads, gutters, empty panes, pin,
  full screen, maximise, two-click ×), a chat pane (virtualised log over
  the composer, streaming through `ChatStore::apply_batch`; unit test: a
  batch of deltas splices one row)
- [ ] the slice's suite on the server: `test/slice.mjs` (sign-in, a new
  chat streaming real server events with no other action, frames while
  streaming and scrolling, idle 0 frames, a silent connection reconnects
  within 45 s and the next reply shows) — **written; to run on the
  server** (the owner's PC runs no window and no test server)
- [x] frame numbers on the owner's PC (release, `tools/measure-pc.mjs`,
  perf/pc-2026-11-19.md): idle 0 frames; streaming and a long reply on
  screen over budget (one markdown row per message)
- [x] round 2 (the Manager's F10-F13, F8, F9):
  - F10: a reply is one log row per markdown block (`core::mdblocks`; the
    row adds the room the whole document would have, `block_room`); a
    stream re-cuts only its tail and splices only the new blocks and the
    one that was last; long cuts run on the background executor. Unit
    tests: the cut (and its streaming agreeing with a whole cut), the
    store's splices while a 300-item list streams, a long history cut off
    the UI thread, and block rows standing exactly as tall as one TextView
  - F11: heads redraw only when their own info or frame changed
    (`PaneHead::refresh`, the workbench notifying the head whose frame
    changed); the driver counts renders (`renders`), the slice checks a
    wheel over the log redraws no head, composer, top bar or sidebar
  - F12: the driver answers in one write with `TCP_NODELAY` (~1 ms a
    command, was ~50 ms: Nagle and the delayed ACK)
  - F13: `tools/measure-pc.mjs` runs with its own fresh config folder and
    a fresh test server's own account, seeded there; it refuses otherwise
  - F8: Docker's tab only where Docker is installed; with no project, the
    global tabs only (and no "+"), on Info
  - F9: the empty pane's chips and actions as the web's (`leaf_chip`,
    `leaf_act`: pills, Split ▸ / Split ▾)
- [x] round 3 (the Manager's F15-F24):
  - F19/F24: Claude's text drawn as the web draws it: `core::webmd` (a port
    of claude-chat.js's md / inline / linkify) replaces gpui-component's
    TextView in the chat; `##`/`###` bold at body size, a nested item flat,
    the rule the browser's hr, the web's margins (collapsed). **The
    owner's call (19 Nov): tables stay drawn as tables, and code keeps its
    colours, readable on the dark background (`core::syntax`, the theme's
    `syn_*` tokens), with no language label.** Unit tests: the blocks, the
    inline styles, tables, streaming agreeing with a whole parse
  - F15: one selection across the log's blocks and messages (drag,
    double / triple click, shift-click, edge scroll, Ctrl+C); F20: the
    driver's `drag` (and `tripleclick`, a `move` with the button held,
    mods on the mouse); the slice drags across blocks and from a message
    into the next, copies, double-clicks a word
  - F18: an empty workbench shows the "New terminal" launcher pane (the
    web's pending cell), which the next pane replaces
  - F16, F22: `measure-pc.mjs` reads agentState, has a time limit, cleans
    up on signals, failures and the client's exit; the owner's config
    folders on Linux, Windows and macOS refused by name. F23: the suites'
    config folders removed at the end. F17: the slice's splice check is
    against the rows added (and a few per batch); "live" is a reply seen
    part-way or in two batches or more
  - F21: a wheel tick re-runs the ChatPane's and the Workbench's render
    because GPUI marks every ancestor of a notified view dirty
    (ARCHITECTURE.md, "The view tree"); they stay layout only
- [ ] the slice's suite again on the server, then the owner's PC measured
  again with the new harness (**to run**: Tester / Manager)

- [x] round 7 (the UI review's shared findings, F74-F84, F58, F59, F64, F65; the
  owner's answers of 20 Nov: drag and drop between panels as on the web, no
  pairing link):
  - widgets: the modal as the web's (a press outside closes, Esc does not,
    the title as written, up to 90% of the window, `modal_pinned` + `dialog`
    with the head and footer pinned: Schedule's dialog uses it); quiet danger
    buttons (red on hover or armed); `number_input`, `date_input`,
    `time_input`, `option_buttons`, `ReadOnly`; `paint` (moved from Planning:
    `Draw`, `Txt`, the controls at rest: Kanban uses them); the toast our own
    (the web's toast(): text only, the coloured edge; test/toast.mjs,
    screens/widgets/toast-web-vs-v2.png); `ref_drag` (a row's ghost, the
    dashed drop outline, `edge_scroll`; a terminal's screen takes a dropped
    reference into the program, a chat's whole pane into its box)
  - shell (F78, F79, F58, F59): the top bar as the web's (reconnecting, the
    reconnect line, tg pill, `status_pill`, slots for approvals and chips,
    Push / Ask to merge for collaborators), sidebar rows (heartbeat, clock,
    factory mark, remove, drag to reorder), tab badges and tooltips, the
    boot screen for a saved login; the sign-in page as the web's (Mini,
    wording, collaborator sign-in, first run, restore, Enter, providers'
    order and OR, the code box, Electron's server step, maximise); the
    pairing link removed. test/shell.mjs. Mini in chat panes: to do
  - workbench (F80, F77, F64, F83): drag and swap, rename, the "…" menu,
    head tags and the secrets lock (`register_head`), full screen over the
    window, double-click maximise, the launcher's last used, the "+" menu
    (`add-pane`). test/workbench.mjs
  - chat: the composer in full (F74, `composer/`: test/chat-composer.mjs),
    the log in full (F75, `log/`: test/chat-log.mjs), the resume picker
    (F76), the bubble's hooks (F65: a second list, `hold` / `release`,
    `head_info`). Live frames: the hook (`log::frames::set_host`) waits for
    Desktop's host

### Open for the owner (via the Manager)

- The launcher's UI Mode button: the web's gradient drifts (9 s, forever)
  and its caret blinks; v2 draws both still, as an animation that never
  ends would draw frames on an idle screen (rule 6, "idle: no frames").
  Animate them anyway (capped, only while the launcher is on screen)?

### Hooks the areas plug into (from round 3)

- `crundi_workbench::launcher::set_chooser` (the chat area's resume
  picker: which conversation, before UI Mode / Terminal start; the
  workspace asks it through `launcher::chooser`); `set_terminal_launcher`
  (registered by the workspace: Empty Shell, Terminal and its Skip
  Permissions, the button busy through `launcher::set_busy`).
- The "open-path" action with `crundi_shell::OpenPath` (registered by the
  workspace: the file viewer, a path clicked in a chat).
- `crundi_shell::register_overlay` (a cached layer over the signed-in tree:
  the file windows) and `Projects::path_of` / `paths` (a project's folder),
  added for the workspace (additive).
- The empty pane's chips `wb-chip:git|files|media|terminal` (workspace); a chip's
  pane fills the leaf it was clicked in (F35: `Workbench::chip` sets the leaf
  while the action runs, and `place` takes it).
- The workbench's driver actions (F36): `pane-close` (twice: "Sure?"),
  `pane-split`, `pane-zoom`, `leaf-chip`; its state's `leaves` (every leaf,
  empty ones too, and whether its × is armed).
- Still to port in the chat log: the hover copy button on code, links and
  paths (`.cc-copyhint`), the streaming caret (`.cc-cursor`), a path's
  hover colour; the launcher's "last used" dot.

## The areas and their screens

| Area (crate) | Screen / feature | native-gpui ref | v2 | Measured |
|---|---|---|---|---|
| Shell (shell) | Top bar: logo and project, usage bars (wk / 5h, over-pace, countdowns), "updated … ago" (→ Usage), Cru's mark, Telegram, connection badge, window buttons, drag / double-click | shell/lib.rs, usage.rs | slice (Cru's mark static: Desktop's hover / live / unread / push-to-talk to do) | — |
| Shell | Sidebar: PROJECTS, fold (220 ms ease), rows and dots, folded rail with initials, project icons, + Add project, Inbox | shell/lib.rs, project_icon.rs | slice | — |
| Shell | Project bar: the 17 tabs, "+" (→ add menu), Browser button and dot, sideways scroll with fades | shell/lib.rs | slice (Browser's dot to do with Desktop) | — |
| Shell | Sign-in: server step, password and code, Google / GitHub / Telegram (OAuth and app-login), the browser's code, pairing, resume, sign-out | shell/session.rs | slice (ported; suite: sign-in by password) | — |
| Shell | The drawer's place and divider (drag, double-click reset, left / right) | shell/lib.rs | slice (Desktop gives the view) | — |
| Shell | Push-to-talk shortcut (ptt.json), settings parts | shell/ptt.rs | to do | — |
| Workbench (workbench) | 2.1 pane header wide, 2.2 narrow (the "…" menu), rename on click | workbench/head.rs | slice (no rename, no "…" menu yet) | — |
| Workbench | 2.3 moving panes: drag, swap, split, empty pane and its chips, the "+" menu's layouts and Fit | workbench/lib.rs, view.rs, menu.rs | slice (split, gutters, empty pane; drag/swap, "+" menu to do) | — |
| Workbench | 2.4 live state (dots, badges) | workbench | slice | — |
| Workbench | Launcher (the "New terminal" pane: Empty Shell, UI Mode, Terminal, the Skip Permissions shields and their peek), Claude terminal panes (their input inside each panel) | workbench, workspace | slice: the launcher (UI Mode starts a chat in its place; the terminal buttons wait for the terminal area); terminal panes to do | — |
| Chat (chat) | The pane: log over composer, its kind `live:`, panes kept in step with the project's chats | chats | slice | — |
| Chat: log | (F10) a reply drawn one row per markdown block, as the web's markdown (F24; tables and code colours the owner's call); one selection across the log (F15); 2.13 questions, 2.14 tool cards (groups, bodies), 2.15 thinking, 2.16 prose / code blocks / bubble / results / errors / notices, 2.16m media, 2.16f live frames, 2.17 suggestions and recaps, 2.18 agent dock, 2.19 goal and plan, 2.20 cards, 2.21 side questions, 2.22 resume picker, 2.23 parked panes, "Go to the newest message" | chat-log | slice: bubble, markdown, tool card (bash / JSON), thinking, result, error, notice; the rest to do | streaming: — |
| Chat: composer | 2.5 composer and Stop, 2.6 mode picker, 2.7 attachments, 2.8 drafts, 2.9 queued, 2.10 scheduled, 2.11 slash menu, 2.12 secrets dialog, 2.12b secret requests, 2.24 voice input, the Enter switch, session id | chat-composer | slice: box, Send / Queue, Stop (Sure?), state, model, cost; the rest to do | — |
| Workspace (workspace) | 3.2 Claude terminal panes (`term:<id>`): the screen (one element; each visible line's row kept by the line's id, derived only when that line changes or comes into view, each character shaped once per face and size; painted in one paint layer, glyphs straight in, as the chat's log), the cursor (its own leaf: the blink redraws it alone, only while focused), the input bar (its own leaf: Ctrl+Enter / Send, the key tools, attach with its strip, a dragged reference), selection copy, wheel and scrollback, ↓ to the bottom, A- ↺ A+, the two-click ×, rename (the server's, seen in the head); the launcher's Empty Shell / Terminal / Skip Permissions (busy until the server answers); output batched to one feed and one notify per terminal per frame | workspace/terms.rs, vt.rs | built (suite on the PC under Xvfb: 103/103; snapshots on the server) | PC, release, six panes (F37, tools/measure-pc.mjs `term-*`): see the runs below; round 3 (F46/F50/F51): perf/pc-2026-11-20-round3-workspace.md (term-stream 6.5–6.7, term-stream+scroll 7.5–7.7, eight panes 7.3–7.7; the floor with no glyphs 6.9); idle 0 frames |
| Workspace | 3.3 Git (status, stage / unstage / all, discard, commit, pull, push; rows derived on load, `uniform_list`; a refresh with nothing new draws nothing), the diff window (diffed off the UI thread, `gpui::list`, staging a chunk) | git.rs, diffview.rs | built | — |
| Workspace | 3.4 Files (browse, search, the row menu, cut / copy / paste, upload, download, drop from the system, drag a reference; rows derived per answer, `uniform_list`; the "…" menu opens under the button, as the web's openFileMenu, kept in the window), the file windows (editor, viewer, drag and snap, expand, tile / grid / cascade; one cached layer over the window, redrawn on a key only when Save turns on or off), a path from a chat ("open-path") | files.rs, wins.rs | built | — |
| Workspace | 3.5 Media (upload, search, kinds, scope, cards, lightbox, copy path, jump to source, delete; the grid virtualised as rows of cards) and Git / Files / Media as workbench panels (the empty pane's chips, a second refused) | media.rs | built | — |
| Planning (planning) | Kanban (and the card dialog): toolbar, Board / Trash / History, columns (a cached view each, a virtualised list of cards, shrunk to its cards as the web's), the stacked board (narrow: one virtualised list), blank card, subtasks, card menu, subtask menu, Edit (title, column, description), drag to move, attachments strip, Brainstorm, live `kanban` events; the `panel:kanban` pane | planning/kanban | ported (suites: test/planning/kanban.mjs; kanban-look.mjs: a card at rest, one element (card_el.rs), is pixel-identical to the live card, and its controls click). Media: the web's openMediaModal (planning/media.rs, F44): Media · Kanban, a task's and a subtask's media ("Attached here", uploads linked to it), the lightbox; the board's strips reload after an upload or a delete; Workspace's card rules reused (crundi_workspace::media) | see the runs below |
| Planning | Mind map: toolbar, filter, project chips, the map (draws only the ideas and links in view; its own pan; links tessellated once per layout), typing ideas in place (Tab, Ctrl+Enter, Enter, Esc), editing, keyboard walk, fold, Delete twice, drag to rearrange, zoom (Ctrl+wheel, Ctrl +/−, pinch), idea dialog (its Media: the idea's media in the Media modal, the dialog again after it), link picker, shortcut sheet, Media · Mindmap, live `mindmap` events; the `panel:mindmap` pane | planning/mindmap | ported (suites: test/planning/mindmap.mjs; mindmap-look.mjs: the ideas at rest, painted by one element (idea_el.rs), are pixel-identical to their divs) | see the runs below |
| Planning | Schedules and webhooks: the list (virtualised), filter, enable switch, the add / edit dialog (in the modal layer: name, project, when (one-time, recurring, when called), action (agent, background chat, CLI command, service), model, effort, mode, session, prompt, folder, conversation and its provisioning, extra conditions), webhooks (key or signature, headers, idempotency, URL and secret, new key / new secret), delete, live `schedule` events | planning/schedule | ported (suite: test/planning/schedule.mjs, 57 checks) | — |
| Planning | Notes: the tab (bar; page list with + New page, search, Pages / Trash; the page beside it, over it when narrow), Notes panes `panel:notes:<n>` (picker, one page per pane, Pages, text size), the editor (blocks, markdown shortcuts, slash menu, grip menu, format bar and links, block selection and its bar, drag to move, tables, code with language / wrap / copy), autosave with a conflict copy, trash, live `notes` events | planning/notes | ported (suite: test/planning/notes.mjs, 71 checks; the page, the picker and the list virtualised; one store entity) | — |
| Planning | Factories: the tab (Library with templates, Runs, Cleanup), the editor (canvas, inspector, Team, Issues, undo / redo, autosave), the start dialog (change request, drafts, attachments, precheck, missing skills, PINs), the live run graph (the tab's, with minimap and replay; the panel's, with Follow, Details and transcript), the Factory panel (earlier runs, triggers: kanban, schedule, webhook), live `factoryRun` / `factories` events; the `panel:factory` pane | planning/factories | ported (suite: test/planning/factories.mjs, 54 checks; the graph draws only what is on screen, paths parsed once per layout; one store entity) | — |
| Operations (operations) | (the whole area: test/operations.mjs, 424 checks under Xvfb on the PC; the CR scenario with the area in: cr-sweep p95 5.99 (99%), cr-scroll 6.33 (98%), idle 0, perf/pc-2026-11-20-operations.md) | operations/ | ported | see the runs |
| Operations | Services (cards, register, start / stop / restart, logs, delete, exposure: port, tunnel, subdomain and path forwards, publish), the forwards no card shows, the live usage strip (2 s poll while on show; a poll with nothing new draws nothing) | operations/services.rs | ported (suite: test/operations/services.mjs, 54 checks); the view keeps the open project and the aliases (rule 1) | — |
| Operations | Docker (containers and the bay plan, stacks, images, volumes, networks, forwards, system and disk; a container's detail: logs as a virtual list, stats, shell, inspect; jobs; the refresh draws only when the list changed) | operations/docker.rs | ported (suite: test/operations/docker.mjs); the lists (containers, images, volumes) are still drawn whole, not virtualised (open) | PC, release: scroll p95 4.8 (100%) |
| Operations | Secrets: the list, the add form and its passphrase rule, Reveal / Copy / Hide through the unlock flow, Edit, Delete, the lock-update offer, access requests (badge, Approve with the PIN, Deny) | operations/vault.rs | ported (suite: test/operations/vault.mjs) | — |
| Operations | Settings, in full for the owner: Server and Claude Code (updates), What's new, System prompt, Skills, Usage, Voice assistant (and the push-to-talk shortcut on this device: ptt.json, Change / Reset), Notification channels and settings, Sign-in (password, Telegram, Google / GitHub), Phone app (push, relay, pairing), Session, Configuration, Terminal, Backup & restore, Desktop App | operations/settings.rs, skills/ | ported (suites: test/operations/settings.mjs, skills.mjs). The page is a virtual list of its sections. State in two entities (`St`, `Sk`, was two Globals with refresh_windows) that the page observes; Desktop fills the "voice-device" slot with `crundi_operations::register_settings_part`. The sign-in page's "Restore from a backup instead" needs a shell hook (asked) | PC, release (perf/pc-2026-11-20-operations.md): scroll, every section open, p95 8.7 (84% ≤7; was 21.3); idle 0 |
| Operations | Homepool (networks, nodes, access keys, requests, activity; its 3 s refresh draws only on a change), Info (the project, this machine (live, 2 s), reclaim disk space, outside collaborators, the tunnel, the server log as a virtual list spliced per line) | operations/homepool.rs, info.rs | ported (suites: test/operations/homepool.mjs, info.mjs) | PC, release: Info's scroll p95 8.5 (78%; was 10.2); still: redraws only for new machine figures |
| Operations | Panels (widgets): the tab (list, bar, Checks, History), the panel panes (`panel:widget:<n>` chooser, `panel:widget:@<id>`), the chip's popover, docks and the inline card (for the chat area), live `widget` events; the bridge to each copy's page | operations/panels.rs | ported; the pages are drawn by the Desktop area's browser through `crundi_operations::web::set_host` (until it is set, a copy says the browser did not start). No forever timer (native-gpui's 16 ms loop): a 250 ms tick only while copies exist, the host's view redraws itself as its page paints. Suite: test/operations/panels.mjs (the page checks run when the host is there) | — |
| Operations | Add project, the approvals panel (a window layer; its top-bar button needs a shell slot: asked), What's new (the tour; each step's page through the same browser hook), the scheduled-backup notice (a window layer) | operations/extras.rs, skills/backup.rs | ported (suite: test/operations/extras.mjs) | — |
| Desktop (desktop, webview, browser-*) | The browser drawer (tabs per project, address bar and history suggestions, Network / Device menus, Annotate and sending marks to a chat, DevTools, History, Clear browsing data; a layout view of 8 cached leaves: a page's frame redraws its `PageView` alone), the Browsers tab, the native host (the server's browser_* tools on drawer tabs and hidden browsers), saved passwords (the PIN), the unload rule and the browser host process (lazy, idle stop, crash and Reload); Inbox (threads and conversation, virtualised; the sidebar's unread badge), notifications, the tray (id `crundi-v2`), startup, the chat bubble (the chat crate's ChatPane in a window of its own), the homepool node; Cru (its screen as an overlay under the top bar's mark, the live screen, kid mode and its PIN, "Hey Cru", push-to-talk) | desktop, webview, browser-host, browser-link | ported (suite: test/desktop.mjs, 14 parts, 276 checks on the PC under Xvfb and a private D-Bus; snapshots on the server). No pump timer: the area's one pump task sleeps until woken (the host's frames and events, a deferred call, an effect) or until a part's deadline (`next_due`) | see the runs below |

## The frame budget's runs (to fill)

| Run | Build | Machine | Streaming p50 / p95 / ≤7 ms | Scrolling chat | Kanban | Mind map | Idle frames |
|---|---|---|---|---|---|---|---|
| slice | debug | server (Xvfb, lavapipe) | — | — | n/a | n/a | — |
| workspace (F37) | release | owner's PC (1920x1040 window, Wayland), four chats + an Empty Shell pane (six panes), stub server (perf/pc-2026-11-20-workspace.md) | terminal printing 3000 lines over 29 s: 4.47 / 6.95 / 95%; with the wheel swept over every pane: 5.18 / 8.09 / 80% (an earlier run 4.69 / 7.71 / 91%) | the same run's chats: scroll-panes 6.06 / 100%, scroll-chat 5.82 / 100%, stream+scroll 6.49 / 99% | n/a | n/a | 0 (open, end); 1 after the terminal (the top bar and the workbench's layout once: the server's state, not the terminal) |
| planning | release | owner's PC, Kanban tab 80 cards / Mindmap tab 133 ideas / workbench with both panels beside the chats (perf/pc-2026-11-20-planning.md) | — | — | p95 8.0 (70% ≤7 ms; was 11.1) | p95 7.9 (60%; was 20.4) | 0 |
| desktop | release | owner's PC (1920x1040 window, Wayland), the CR's five panes with a chat streaming, then the browser drawer open beside them on a static page (perf/pc-2026-11-20-desktop.md) | cr-sweep 4.59 / 5.89 / 99%; with the drawer: cr-drawer-sweep 4.64 / 6.11 / 99% | cr-scroll 5.57 / 100%; with the drawer 5.74 / 99% | (in the CR scenario) | (in the CR scenario) | 0 (open, end, planning); 1 with the drawer open (the stream's end, not the drawer) |
| round 7 (merged) | release | owner's PC (1920x1040 window, Wayland), the CR scenario (perf/pc-2026-11-20-round7.md) | stream+scroll 6.33–6.38 | scroll-chat 5.5–5.7; cr-sweep 6.89–6.92 / 96%; **cr-scroll 7.02–7.43 / 90–95% (over: F99)** | 4.11 | 5.48 | 0 |
| slice | release | owner's PC (1920x1080, 144 Hz, Renoir, Wayland), 5 panes, a chat streaming (perf/pc-2026-11-19.md) | 16.9 / 24.2 / 0% (with wheel: 9.1 / 17.1 / 29%) | 5.5 / 28.7 / 71–82% (a 300-line reply on screen; 4.2 / 6.1 / 98% without it) | not built | not built | 0 (open), 0 (after the stream) |

### Desktop: how it runs, and what is still open (round of 20 Nov)

- `crates/webview`: native-gpui's, its 8 ms pump replaced by a waker
  (`crundi_webview::wake`, set by the area) and deadlines (`next_due`: timers,
  the host's idle stop, a panel's hide). `crates/desktop`: native-gpui's logic
  (plain state on the UI thread); its views render from copies taken in the
  pump and notify only when the copy changed; chrome in paint layers.
- Shared-file edits (additive): the app inits the area (`init`,
  `init_window`); `crundi_shell::ptt` (ported, no window refresh); the
  shell's `CruMark` and `cru_anchor` (the top bar's mark breathes while Cru is
  live, its unread dot and title, hover and push-to-talk actions, the focus
  never lost to nothing); `InboxBadge` (the sidebar's Inbox count, the web's
  ibxBadge); the drawer placed uncached (it is layout: its bars and page are
  its leaves); `tools/stage-browser.sh` (the host and CEF beside the app);
  `tools/measure-pc.mjs`'s drawer scenarios (MEASURE_DRAWER=0 skips them).
- Not yet: a chat's live frames (2.16f, the chat area's) and so the "not
  visible" rule's chat-embed half; Settings › Voice assistant's "On this
  device" part (`wake::settings_part`) waits for a settings hook
  (Operations); `test/cru-voice.mjs` (needs a Deepgram stand-in).
- For the owner (via the Manager): "Hey Cru"'s speaker model on GitHub now
  hashes differently from the pinned checksum, so the voice check stays off
  (any voice wakes Cru, and it says so); the tray gained Electron's "Listen
  for "Hey Cru" while plugged in" item native-gpui lacked; Chromium waits
  ~25 s for a keyring on a session with no secrets service before the first
  page (pass `--password-store=basic`?).
- Requests to the chat area: a second list over one `ChatStore` (the bubble
  shares the pane's scroll), `head_info` public, a chat kept subscribed
  outside the open project (a bubble from another project).

## Open questions for the Manager

- The slice's Cru mark is drawn but static (no hover screen, no live
  breathing): Cru is Desktop's; until it lands, a click runs the `cru`
  action, which nobody registers yet.
- Tabs with no area yet show "<Tab> is not built yet in the v2 client."
  (native-gpui's placeholder wording), scaffolding only until every area
  lands.
