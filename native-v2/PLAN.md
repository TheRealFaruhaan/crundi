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
| Workspace (workspace) | 3.2 Claude terminal panes (`term:<id>`): the screen (one element: rows derived once per change, painted in one paint layer, glyphs straight in, as the chat's log), the cursor (its own leaf: the blink redraws it alone, only while focused), the input bar (its own leaf: Ctrl+Enter / Send, the key tools, attach with its strip, a dragged reference), selection copy, wheel and scrollback, ↓ to the bottom, A- ↺ A+, the two-click ×, rename (the server's, seen in the head); the launcher's Empty Shell / Terminal / Skip Permissions (busy until the server answers); output batched to one feed and one notify per terminal per frame | workspace/terms.rs, vt.rs | built (suite on the PC under Xvfb: 103/103; snapshots on the server) | PC, release, six panes (F37, tools/measure-pc.mjs `term-*`): see the runs below; idle 0 frames |
| Workspace | 3.3 Git (status, stage / unstage / all, discard, commit, pull, push; rows derived on load, `uniform_list`; a refresh with nothing new draws nothing), the diff window (diffed off the UI thread, `gpui::list`, staging a chunk) | git.rs, diffview.rs | built | — |
| Workspace | 3.4 Files (browse, search, the row menu, cut / copy / paste, upload, download, drop from the system, drag a reference; rows derived per answer, `uniform_list`; the "…" menu opens under the button, as the web's openFileMenu, kept in the window), the file windows (editor, viewer, drag and snap, expand, tile / grid / cascade; one cached layer over the window, redrawn on a key only when Save turns on or off), a path from a chat ("open-path") | files.rs, wins.rs | built | — |
| Workspace | 3.5 Media (upload, search, kinds, scope, cards, lightbox, copy path, jump to source, delete; the grid virtualised as rows of cards) and Git / Files / Media as workbench panels (the empty pane's chips, a second refused) | media.rs | built | — |
| Planning (planning) | Kanban (and the card dialog): toolbar, Board / Trash / History, columns (a cached view each, a virtualised list of cards, shrunk to its cards as the web's), the stacked board (narrow: one virtualised list), blank card, subtasks, card menu, subtask menu, Edit (title, column, description), drag to move, attachments strip, Brainstorm, live `kanban` events; the `panel:kanban` pane | planning/kanban | ported (suites: test/planning/kanban.mjs; kanban-look.mjs: a card at rest, one element (card_el.rs), is pixel-identical to the live card, and its controls click). Media: the web's openMediaModal (a task's / subtask's media, uploads linked to it) is Workspace's to provide; until then the Media tab, as native-gpui (F42) | see the runs below |
| Planning | Mind map: toolbar, filter, project chips, the map (draws only the ideas and links in view; its own pan; links tessellated once per layout), typing ideas in place (Tab, Ctrl+Enter, Enter, Esc), editing, keyboard walk, fold, Delete twice, drag to rearrange, zoom (Ctrl+wheel, Ctrl +/−, pinch), idea dialog, link picker, shortcut sheet, live `mindmap` events; the `panel:mindmap` pane | planning/mindmap | ported (suites: test/planning/mindmap.mjs; mindmap-look.mjs: the ideas at rest, painted by one element (idea_el.rs), are pixel-identical to their divs) | see the runs below |
| Planning | Schedules and webhooks: the list (virtualised), filter, enable switch, the add / edit dialog (in the modal layer: name, project, when (one-time, recurring, when called), action (agent, background chat, CLI command, service), model, effort, mode, session, prompt, folder, conversation and its provisioning, extra conditions), webhooks (key or signature, headers, idempotency, URL and secret, new key / new secret), delete, live `schedule` events | planning/schedule | ported (suite: test/planning/schedule.mjs, 57 checks) | — |
| Planning | Notes, Factories (editor, live run graph, factory panel) | planning | to do (next) | — |
| Operations (operations) | Services, forwards, Docker; Secrets, Skills, Usage; Settings, sign-in, backup; Add project; Homepool; Info; Panels (widgets); What's new; approvals | operations | to do | — |
| Desktop (desktop, webview, browser-*) | The browser drawer (tabs per project, address bar, Network / Device menus, Annotate, DevTools), the Browsers tab, the native host (`test/drive.mjs`), tray and notifications, Inbox, the chat bubble, saved passwords, the homepool node, Cru (voice screen, live screen, kid mode, "Hey Cru") | desktop, webview, browser-host, browser-link | browser-host / link ported (not built here: CEF); the rest to do | — |

## The frame budget's runs (to fill)

| Run | Build | Machine | Streaming p50 / p95 / ≤7 ms | Scrolling chat | Kanban | Mind map | Idle frames |
|---|---|---|---|---|---|---|---|
| slice | debug | server (Xvfb, lavapipe) | — | — | n/a | n/a | — |
| workspace (F37) | release | owner's PC (1920x1040 window, Wayland), four chats + an Empty Shell pane (six panes), stub server (perf/pc-2026-11-20-workspace.md) | terminal printing 3000 lines over 29 s: 4.47 / 6.95 / 95%; with the wheel swept over every pane: 5.18 / 8.09 / 80% (an earlier run 4.69 / 7.71 / 91%) | the same run's chats: scroll-panes 6.06 / 100%, scroll-chat 5.82 / 100%, stream+scroll 6.49 / 99% | n/a | n/a | 0 (open, end); 1 after the terminal (the top bar and the workbench's layout once: the server's state, not the terminal) |
| planning | release | owner's PC, Kanban tab 80 cards / Mindmap tab 133 ideas / workbench with both panels beside the chats (perf/pc-2026-11-20-planning.md) | — | — | p95 8.0 (70% ≤7 ms; was 11.1) | p95 7.9 (60%; was 20.4) | 0 |
| slice | release | owner's PC (1920x1080, 144 Hz, Renoir, Wayland), 5 panes, a chat streaming (perf/pc-2026-11-19.md) | 16.9 / 24.2 / 0% (with wheel: 9.1 / 17.1 / 29%) | 5.5 / 28.7 / 71–82% (a 300-line reply on screen; 4.2 / 6.1 / 98% without it) | not built | not built | 0 (open), 0 (after the stream) |

## Open questions for the Manager

- The slice's Cru mark is drawn but static (no hover screen, no live
  breathing): Cru is Desktop's; until it lands, a click runs the `cru`
  action, which nobody registers yet.
- Tabs with no area yet show "<Tab> is not built yet in the v2 client."
  (native-gpui's placeholder wording), scaffolding only until every area
  lands.
