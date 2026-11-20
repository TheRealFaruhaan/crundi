# native-v2: the areas' contract

Who owns which crate, how an area plugs into the window, what is shared,
how to build and test. Read `ARCHITECTURE.md` first: its seven rules bind
every crate. The experience is the PC client's (the web UI and the
Electron app; `native-gpui/` where it matches them): the same layout,
controls, interactions, states and wording. Nothing invented; when the
reference is unclear, ask the Manager.

## The crates

| Crate | Owner | What |
|---|---|---|
| `crates/core` (`crundi-core`) | Architect | No GPUI. API and sign-in, the live link (heartbeat, one channel), saved login, `Conversation` (changed-entry tracking), mosaic, head / tool / markdown / question rules, the driver's protocol. Ported from `native-gpui/crates/core` |
| `crates/theme` (`crundi-theme`) | Architect | The tokens `T`, the registered theme, fonts, icons (an area adds named sizes only in its own section at the end of `T`) |
| `crates/widgets` (`crundi-widgets`) | Architect | Every control once, from tokens (ported from native-gpui; the list below) |
| `crates/shell` (`crundi-shell`) | Architect (Shell area) | `Stores` (Session, Projects, Usage, Nav), the live hub, the registry; Shell, TopBar, Sidebar, ProjectBar, SignIn, the drawer's place |
| `crates/workbench` (`crundi-workbench`) | Workbench area | The pane host: Workbench, PaneHead, pane kinds, gutters, empty panes, moving panes, the "+" menu, launcher, live state |
| `crates/chat` (`crundi-chat`) | Chat area | `Chats`, `ChatStore` + `ChatMeta`, ChatPane, LogView, Composer, the streaming path. The log's features (2.13-2.23) and the composer's (2.5-2.12b, 2.24) are added here, one module each (`log/…`, `composer/…`) |
| `crates/app` (`crundi-app`) | Architect | The binary `crundi-v2`: makes the stores, inits the areas, the window, the test driver |
| `crates/browser-link`, `crates/browser-host` | Desktop area | The browser host process (the only CEF), its link. Ported from native-gpui; not a default member |
| `crates/webview`, `crates/desktop` (to come) | Desktop area | The browser bridge, the drawer, Browsers tab, native host, tray, Inbox, passwords, homepool node, Cru |
| `crates/workspace` (to come) | Workspace area | Git, Files, Media |
| `crates/planning` (to come) | Planning area | Kanban, Mind map, Notes, Schedules, webhooks, Factories |
| `crates/operations` (to come) | Operations area | Services, Docker, Secrets, Skills, Usage, Settings, Homepool, Info, Panels, What's new |

A crate's owner is its only editor. A part that needs something from
another crate asks its owner (the run's `shared/requests.md`), never
edits it. A new area crate is added to the workspace by the Architect.

## How an area plugs in

### Init and the stores

The app calls `crundi_shell::init(cx)`, which makes the shared stores and
returns their handles (`Stores`: `session`, `projects`, `usage`, `nav`),
then each area's `init(cx)`. An area keeps the handles it needs in its own
views' fields (rule 1: handles, not lookups); `crundi_shell::stores(cx)`
gives them to handlers that have no view (never call it in `render`).
An area's own data lives in its own entities, made in its init or when its
tab first shows.

### The registry (`crundi_shell::register(cx, Area::new("name")…)`)

```rust
crundi_shell::register(cx, Area::new("planning")
    .tab("kanban", |stores, window, cx| cx.new(|cx| Board::new(stores.clone(), window, cx)).into())
    .on_batch(|stores, batch: &[live::Event], cx| …)   // this frame's live events, in order
    .on_project(|stores, alias, cx| …)
    .on_signed_in(|stores, cx| …).on_signed_out(|stores, cx| …)
    .action("add-project", |stores, window, cx| …)      // a shell button's action
    .test_ui(|stores, name, args, window, cx| Some(json!({"ok": true})))
    .state(|stores, window, cx| json!({ … })));          // merged into the driver's state
```

- **The driver's state** is merged one level deep: an area's own keys go
  under `areas.<name>` (`json!({ "areas": { "planning": { … } } })`) or
  under a top-level key only it uses (`planningArea`, `workspace`); two
  areas never return the same top-level key with a plain value (F68).
- **The window's close**: the window's × and the system's close (Alt+F4)
  run the `window-close` action when an area registered one (Desktop's:
  hide to the tray while it is up, kept while the kid screen is locked),
  else the app quits (`crundi_shell::close_window`, F113).

- **Tabs**: `registry::TABS` (the web's order). A tab's view is made once,
  when first shown; it is a *layout* view (not cached by the shell), its
  parts are the cached leaves (rule 3). A tab nobody registered says it
  isn't built yet.
- **Live events**: the hub hands each area the whole frame's batch once
  (`on_batch`). Handle it in one pass: group by what it changes, update
  each store once, notify once. Never notify per event.
- **Shell actions** by name (`registry::run_action`): `add-menu` (the
  project bar's "+", Workbench), `browser` (the Browser button, Desktop),
  `cru` (Cru's mark, Desktop), `open-usage` (the usage badge, Operations),
  `add-project` (Operations), `wb-chip:<kind>` (an empty pane's chips),
  `open-path` (Workspace's Files: open `crundi_shell::OpenPath`'s path in
  the file viewer, a path clicked in a chat).
- **The launcher's hooks** (`crundi_workbench::launcher`): `set_chooser`
  (the chat area's resume picker, asked before UI Mode or Terminal start a
  conversation) and `set_terminal_launcher` (the terminal area: Empty
  Shell, Terminal and its Skip Permissions, given the launcher's pane key;
  the started pane takes the launcher's place with `Workbench::replace`).
- **Pane kinds** (`crundi_workbench::register_kind(cx, PaneKind { prefix,
  chat, make, close, zoom })`): `make` returns the pane's `body` (a
  function building its elements in the Workbench's render: layout only,
  its views placed `.cached(…)`; one view alone is `view.cached(size_full)`;
  never a layout view of its own, ARCHITECTURE.md rule 3), the head's `info` (title, long
  title, state) and `watch` (the subscriptions that make the head redraw:
  observe only what the title and dot read). Keys: `live:<chat>`,
  `term:<id>`, `panel:<kind>[:<item>]`, `pend:<n>` (the workbench's own
  "New terminal" launcher).
- **Chat text** (`core::webmd`, drawn by `chat::log`): Claude's markdown as
  the web draws it, one log row per block, one selection across the log.
  By the owner's call (19 Nov) two things differ from the web: tables are
  drawn as tables and code blocks keep syntax colours (`core::syntax`, the
  theme's `syn_*` tokens), with no language label. Any area showing
  Claude's text in the web's way uses `core::webmd`, not a TextView.
- **The shell's slots** (`crundi_shell::slots`): `register_top_item(cx, "approvals" | "chips", &entity, draw)` (the top bar's approvals button beside the connection badge; the panel chips after the spacer), `register_tab_badge(cx, tab, &entity, draw)` (a project-bar tab's badge: Secrets, Factories, Settings), `register_login_restore(cx, &entity, open, draw)` (the sign-in page's "Restore from a backup instead" and its layer). Each slot redraws only its own piece when its entity notifies.
- **Where things are, for Mini** (`crundi_shell::spots`, F101): a view that draws something Mini walks to calls `spots::begin(cx, group)` at the top of its render and puts `spots::spot(cx, group, name)` (a canvas, no layout) over it; `spots(cx).borrow().get(name)` gives its window bounds in handlers (a spot not drawn in its group's last render is gone). The shell's: `logo`, `usage:<wk|5h>[:fill]`, `approvals`, `tabbar`, `tab:<id>`, `tab-badge:<id>`, `sidebar:<input|->:<alias>`; the chat's: `composer|send|input|inrow|queue|sug|badge-cmd|badge-ag|pk-resume|pk-compact|pk-veil:<chat>`. `spots::set_logo_away` empties the logo's head while Mini is out (the TopBar observes it); `spots::tabs` is the project bar's scroll (Mini hauls it).
- **Pane heads** (`crundi_workbench::register_head(cx, prefix, KindHead)`): a kind's icon, its head buttons (`HeadButton::icon` / `text`, run through `on_action`), and per pane its tags and lock (`HeadExtra`: the chat tag, the plan pill, collaborator and factory tags, "Secrets for this chat"). The web's panel head is icon, title, the kind's buttons ("?" for the mind map), Refresh, pin, ×.
- **The "+" menu** is the workbench's (`add-menu`); `add-pane <kind>` is its driver action.
- **The drawer**: Desktop gives the shell its view
  (`Shell::set_drawer_view(Some(Drawer { view, left }))`); the shell draws
  it beside the project area with the divider; `nav.set_drawer(open)`.

### What every view does (ARCHITECTURE.md's rules, in practice)

- Holds the handles it shows; `cx.observe`s exactly those (or subscribes
  to a narrower event); notifies itself only when what it draws changed.
- Is either layout (holds child views, not cached, cheap) or a cached leaf
  (no cached view inside it). A part that changes often is its own leaf.
- Lists longer than a screen: `gpui::list` / `uniform_list`, spliced per
  change, never rebuilt per event.
- Parses on data change, not in render; big work on `cx.background_spawn`.
- Animations: `w::spinner`, `w::shimmer`, `w::cru_mark` (capped at
  `T::ANIM_FPS`); a new one uses `with_max_fps` and runs only while shown.
- Every element with hover or a tooltip has an `.id(…)`.
- No `refresh_windows()`, no `window.refresh()` (`tools/check.sh`, CI).
- A cached view that is replayed on every frame (a bar, a head, a list)
  paints in a layer (`w::layer`), its text boxes through `w::text_box` with
  `w::watch_focus` (ARCHITECTURE.md, "Paint layers": what may go in one).
- A layout view (not cached) never reads a store in `render`: it keeps a
  copy of what it lays out, set by an observer that notifies on a change
  (GPUI draws a frame for any notify on an entity read during the last draw).

### The shared controls (`crates/widgets`)

Every control comes from here (ported from native-gpui, the owner's rule:
never a copy in an area, never a gpui-component control used directly in a
screen): `button`, `primary`, `danger`, `ghost`, `icon_button`,
`icon_toggle`, `toggle`, `switch`, `check`, `segmented`, `input`,
`field_label`, `menu_button`, `more_menu`, `pick_menu`, `dropdown`,
`select` (one control with `dropdown`: `dropdown::choices`), `toggled` (the web's `.kanban-btn` / `.toggled`), `dot`, `state_color`, `badge`, `count_badge`, `tag`, `chip`,
`status_badge`, `card`, `panel` / `panel_box`, `pane_head`, `head_button`, `menu_item`,
`menu_panel`, `toolbar`, `title_bar`, `page`, `tab_row`, `tab`, `nav_tab`,
`sidebar_item`, `rail_avatar`, `dashed_button`, `outline_button`, `empty`,
`error_state`, `loading`, `list_row`, `confirm`, `ask`, `ask_confirm`,
`menu_at` (the one ask layer, drawn once by the shell), `modal` /
`modal_pinned` (the one modal layer, drawn once by the shell), `overlay_modal`,
`overlay_dialog`, `dialog`, `modal_head`, `modal_foot`,
`toast` (our own, the web's toast(): text only, no icon, the coloured left edge, under the top bar on the right, 3.5 s; one at a time), `divider`, `rule`, `tip` / `tooltip`, `send_button`,
`stop_button`, `tone_button`, `square_button`, `suggest_row`,
`queue_drawer`, `textarea` / `message_box`, `code_state` / `code_editor`,
`spinner`, `shimmer`, `markdown`, `icon`, `brand_mark`, `cru_mark`,
`primary_icon`, `liquid_mic`, `meter`, `spark`, `kbd`, `key_row`, `link`,
`disclosure`, `filter_chip`, `seg_tabs`, `segmented_icons`, `icon_tag`,
`page_item`, `round_button`, `arm_button`, `choice_row`, `choice_mark`,
`label_icon_button`, `leaf_chip` / `leaf_act` (an empty pane's launch chips and actions), `launch_button` (the launcher's Empty Shell, UI Mode, Terminal and shields), `drag_ref` / `drag_ref_row` / `ref_target` / `ref_target_drag` / `edge_scroll` (dragging a reference, dropping one, scrolling at the edges while dragging), `layer` / `text_box` / `watch_focus` (paint layers: ARCHITECTURE.md),
`number_input`, `date_input` (date, and date with time), `time_input`, `option_buttons` (the web's wrapping `.sm-seg`), `ReadOnly` / `readonly_box` (a URL or secret, selected whole on a click), `armed` (a danger button kept red), and `paint` (a row painted as one element: `Draw`, `Txt`, and the controls at rest: `button_at`, `check_at`, `count_badge_at`, `icon_tag_at`, `lite_button`).

- **Dialogs** (the web's): a press on the scrim closes a dialog, Esc does
  not (a dialog the web closes with Esc, such as Add project or a help sheet,
  handles it in its own keys); the title as written (not in capitals); at
  most 90% of the window tall; a long form opens with `modal_pinned` and draws
  `w::dialog(head, body, Some(foot))`: the head and footer stay in view and
  the body alone scrolls.
- **Danger buttons** (`danger`, an `arm_button` with the danger look) are
  quiet at rest, bordered as `button`, and red on hover or once armed (the
  web's `.svc-btn.danger`, `.dk-btn.armed`): never a red fill at rest.
- **Drag and drop between panels** works as on the web (the owner, 20 Nov):
  a row that carries a reference is a `drag_ref` source (Files, Git, Kanban
  cards and subtasks, Mindmap ideas, Media cards, Notes blocks); a chat's box,
  a terminal's screen and input, a notes page and Cru are `ref_target`s; a
  container that scrolls while something is dragged over its edge uses
  `edge_scroll`.

- Every control's dimensions are the theme's (`T::BTN_H`, `ICON_BTN`,
  `INPUT_H`…); an area never sizes a control (no `.h`, `.px`,
  `.text_size`, `.small()` on a control outside widgets).
- A new control (two areas need it, or it is a control by nature) is its
  own file in `crates/widgets/src/`, asked of the Architect in
  `shared/requests.md`. An existing control's look is extended (a builder
  method), never changed without the Architect.
- Tokens only: nothing is typed into a screen but 0 and `px(1.)` hairlines.

### The test driver

`CRUNDI_TEST_PORT=<port>`: `state` (and `props`, the same), `snapshot` (xwd and ImageMagick; on a PC without them, put `tools/pc-snap` first on PATH: ffmpeg grabs the virtual display),
(xwd under X11), `ui`, `mouse` (move, click, dblclick, tripleclick,
down, up, leave, wheel, `drag` from x,y to toX,toY in `steps`; a `move`
with `buttons: ["left"]`; `mods`), `key`, `resize`. The chat's
`log-texts` gives the log's texts on screen with their rects, the
selection and the clipboard. The window's own `ui`
actions: `check-server`, `sign-in-password`, `sign-out`,
`select-project`, `tab`, `drawer`, `stub`, `dialog-answer`, `menu-pick`,
`toasts-clear`, `toast <info|error|success> <text>`, `window-close` (the ×),
`usage-clock off|on` (the top bar's minute tick held: `test/lib.mjs`
holds it after sign-in, so an idle check never catches it, F122); every other name goes to the areas' `test_ui`
(unprefixed: each in turn; `<area>:<name>`: that area). The state holds
`signedIn`, `online`, `live` (`connects`, `silentDrops`, `streams`),
`projects`, `project`, `tab`, and every area's own (`panes`,
`focusedPane`, `chat`, `entries`, `rows`, `chatState`, `batches`,
`spliced`…), and `renders` (each counted view's renders: a view that
should redraw only on its own changes calls `crundi_core::driver::rendered`
at the top of its `render`). The driver never redraws the window.

The window has the keys, as a person's does (F109): at its first command
the driver gives the window the X input focus (under Xvfb there is no
window manager to do it). So focus and blur events, a text box's caret and
`watch_focus` behave as on a display: a box that loses the keys stops its
caret; a focused box blinks until the theme's 10 s rest (`CursorBlink`
with an idle timeout, pushed to gpui_base by `Theme::sync_base`); a focused
terminal's cursor blinks (redrawing its leaf). An idle check therefore
blurs first (or waits out the rest).

## Building and testing

- **Build** only with `native-v2/tools/build.sh [cargo args]` (the shared
  `CARGO_TARGET_DIR=$HOME/.cache/crundi-native-target`, two jobs, the lock,
  `oom_score_adj 1000`, the 20 GB floor, `tools/check.sh` first). One build
  at a time. `tools/build.sh -p crundi-chat` builds one crate;
  `tools/build.sh test --workspace` runs the unit tests (the browser
  host's need CEF, `CEF_PATH`, as CI fetches it; without CEF add
  `--exclude crundi-browser-host`). README.md says the same (F72).
- **The binary**: `$CARGO_TARGET_DIR/debug/crundi-v2` is the last build's,
  whichever worktree made it. A client build also links it (with the
  browser host and CEF's files) into this worktree's own
  `native-v2/.bin/<debug|release>/` (`FROM` says whose); `test/lib.mjs` and
  `tools/measure-pc.mjs` run that copy unless `NATIVE_BIN` / an argument
  says otherwise (F118).
- **Unit tests** are headless (`gpui::TestAppContext` with the
  `test-support` dev feature): the stores, the hub's batching, the rules.
- **End-to-end suites** (`native-v2/test/<area>.mjs` on `test/lib.mjs`)
  run **on the Crundi server only**, under Xvfb, against a fresh
  `native-gpui/tools/test-server.sh <port> --bare` on the run's ports
  (4880-4889), with the frame meter on (`T.frames`). Never on the owner's
  PC: no window of the app there, no test server, until the Manager says
  the owner said go. The owner's sign-in and tokens are never read or used:
  test servers have their own fresh accounts.
- **Frames**: `ZED_MEASUREMENTS=1` (set by `test/lib.mjs` and
  `tools/smoke.mjs`); `tools/frames.mjs` sums them up. Every suite reports
  the frames of its scrolling and streaming steps and checks idle = 0;
  the 7 ms budget is checked with `CRUNDI_FRAME_BUDGET=1` (the owner's PC,
  release build, when allowed).
- **Disk**: keep 20 GB free on `/`; scratch in one /tmp folder you remove.
- **Before handing off**: your crate builds warning-free, `tools/check.sh`
  passes, your unit tests pass, your suite passes on the server (the
  Tester runs every suite there before review).

## Changing a shared file

Architect's files: the workspace `Cargo.toml` (a new dependency is a
request), `crates/core`, `crates/theme` (outside the area sections),
`crates/widgets`, `crates/shell`, `crates/app`, `tools/`, `test/lib.mjs`,
`ARCHITECTURE.md`, this file, `PLAN.md`. Ask in `shared/requests.md`: one
line (area, file, what and why); work around it in your crate meanwhile.
