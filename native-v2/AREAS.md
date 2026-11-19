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
| `crates/webview`, `crates/desktop` (to come) | Desktop area | The browser bridge, the drawer, Browsers tab, native host, tray, Inbox, bubble, passwords, homepool node, Cru |
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
  `add-project` (Operations), `wb-chip:<kind>` (an empty pane's chips).
- **Pane kinds** (`crundi_workbench::register_kind(cx, PaneKind { prefix,
  chat, make, close, zoom })`): `make` returns the pane's body view (a
  layout view whose parts are cached), the head's `info` (title, long
  title, state) and `watch` (the subscriptions that make the head redraw:
  observe only what the title and dot read). Keys: `live:<chat>`,
  `term:<id>`, `panel:<kind>[:<item>]`.
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

### The shared controls (`crates/widgets`)

Every control comes from here (ported from native-gpui, the owner's rule:
never a copy in an area, never a gpui-component control used directly in a
screen): `button`, `primary`, `danger`, `ghost`, `icon_button`,
`icon_toggle`, `toggle`, `switch`, `check`, `segmented`, `input`,
`field_label`, `menu_button`, `more_menu`, `pick_menu`, `dropdown`,
`select`, `dot`, `state_color`, `badge`, `count_badge`, `tag`, `chip`,
`status_badge`, `card`, `panel`, `pane_head`, `head_button`, `menu_item`,
`menu_panel`, `toolbar`, `title_bar`, `page`, `tab_row`, `tab`, `nav_tab`,
`sidebar_item`, `rail_avatar`, `dashed_button`, `outline_button`, `empty`,
`error_state`, `loading`, `list_row`, `confirm`, `ask`, `ask_confirm`,
`menu_at` (the one ask layer, drawn once by the shell), `modal` (the one
modal layer, drawn once by the shell), `overlay_modal`, `modal_head`,
`toast`, `divider`, `rule`, `tip` / `tooltip`, `send_button`,
`stop_button`, `tone_button`, `square_button`, `suggest_row`,
`queue_drawer`, `textarea` / `message_box`, `code_state` / `code_editor`,
`spinner`, `shimmer`, `markdown`, `icon`, `brand_mark`, `cru_mark`,
`primary_icon`, `liquid_mic`, `meter`, `spark`, `kbd`, `key_row`, `link`,
`disclosure`, `filter_chip`, `seg_tabs`, `segmented_icons`, `icon_tag`,
`page_item`, `round_button`, `arm_button`, `choice_row`, `choice_mark`,
`label_icon_button`, `drag_ref` / `ref_target` (dragging a file reference, dropping one).

- Every control's dimensions are the theme's (`T::BTN_H`, `ICON_BTN`,
  `INPUT_H`…); an area never sizes a control (no `.h`, `.px`,
  `.text_size`, `.small()` on a control outside widgets).
- A new control (two areas need it, or it is a control by nature) is its
  own file in `crates/widgets/src/`, asked of the Architect in
  `shared/requests.md`. An existing control's look is extended (a builder
  method), never changed without the Architect.
- Tokens only: nothing is typed into a screen but 0 and `px(1.)` hairlines.

### The test driver

`CRUNDI_TEST_PORT=<port>`: `state` (and `props`, the same), `snapshot`
(xwd under X11), `ui`, `mouse`, `key`, `resize`. The window's own `ui`
actions: `check-server`, `sign-in-password`, `sign-in-pair`, `sign-out`,
`select-project`, `tab`, `drawer`, `stub`, `dialog-answer`, `menu-pick`,
`toasts-clear`; every other name goes to the areas' `test_ui`
(unprefixed: each in turn; `<area>:<name>`: that area). The state holds
`signedIn`, `online`, `live` (`connects`, `silentDrops`, `streams`),
`projects`, `project`, `tab`, and every area's own (`panes`,
`focusedPane`, `chat`, `entries`, `chatState`, `batches`, `spliced`…).
The driver never redraws the window.

## Building and testing

- **Build** only with `native-v2/tools/build.sh [cargo args]` (the shared
  `CARGO_TARGET_DIR=$HOME/.cache/crundi-native-target`, two jobs, the lock,
  `oom_score_adj 1000`, the 20 GB floor, `tools/check.sh` first). One build
  at a time. `tools/build.sh -p crundi-chat` builds one crate;
  `tools/build.sh test --workspace --exclude crundi-browser-host` runs the
  unit tests (the browser host needs CEF: `CEF_PATH`, as CI fetches it).
- **The binary**: `$CARGO_TARGET_DIR/debug/crundi-v2`.
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
