# Areas: building phases 3 to 6 in parallel

> **Superseded** (the owner, 16 Nov 2026): the PC client is now built on GPUI in `native-gpui/` (`native-gpui/AREAS.md`). This Slint client stays as the reference.

Four developers build the rest of the PC client at the same time, in the
same checkout, each in an **area** of its own. This file is the contract
between them: where each area's files go, how an area plugs into the
window, what is shared, how to build and test without getting in each
other's way. Read `DESIGN.md` (layout and experience, theme tokens only) and
`README.md` ("Adding a screen") first; they still hold.

The foundation (the tab host, the hooks, the shared kit, the test driver's
registration) is the Lead's. If you need a change to a Lead file, see
"Changing a shared file" below.

## The four areas

| Area | Phase | Screens | Tab ids / panel kinds |
|---|---|---|---|
| **workspace** | 3 | The workbench (pane layout, Claude terminal panels, the "+" menu's panels), Git (status, diff, history, commit, branches), Files (browse, view with highlighting, edit, upload, download), Media (library, viewer, player) | tabs `git`, `files`, `media`; panels `git`, `files`, `media` |
| **planning** | 4 | Kanban (columns, cards, todos, drag to move, the card dialog), Mind map, Notes, Schedules, webhooks, Factories (list, the factory editor, the live run graph, the factory panel) | tabs `kanban`, `notes`, `mindmap`, `factories`, `schedule`; panels `kanban`, `notes`, `mindmap`, `factory` |
| **ops** | 5 | Services, forwards, Docker; Secrets (the PC client's unlock flow), Skills, Usage; Settings (all of it, for the owner), sign-in and backup; Add project; Homepool; Info; Panels (widgets: the tab, the `widget` panel, the inline widget card under `widget_open`); What's new; the top bar's approvals button | tabs `services`, `docker`, `homepool`, `info`, `secrets`, `settings`, `panels`; panel `widget` |
| **desktop** | 3 + 6 | The browser drawer in full on the phase 0 panels (tabs per project, networks, devices, annotations sent to a chat), the native host's full browser protocol from `app/`, the Browsers tab; notifications and the tray; the Inbox; the chat bubble (and the pane header's bubble button); saved passwords with the PIN; the homepool node; Cru on the PC | tab `browsers` |

The tab order is the web's and lives in `ui/main.slint` (`tabs`); the
mapping from a tab id or panel kind to its area is `Areas.of()` in
`ui/areas.slint`. Both are the Lead's: a new kind is a one-line request.

## Where each area's files go

Each area owns these, and only it edits them:

```
ui/<area>/api.slint      the globals Rust reads, sets and handles, and their structs
ui/<area>/index.slint    <Area>View and <Area>Overlay: the area's entry points
ui/<area>/*.slint        every other screen and part of the area
src/<area>/mod.rs        the area's Rust: the registry's functions (below)
src/<area>/*.rs          the rest: one file per screen, pure rules unit-tested
test/<test>.mjs          workspace.mjs, planning.mjs, operations.mjs, desktop.mjs
specs/<area>/            the area's specs and side-by-side snapshots
screens/<area>/          the review's screens (latest round only, overwrite)
```

`<area>` is `workspace`, `planning`, `ops` or `desktop`; `<Area>` is
`Workspace`, `Planning`, `Ops`, `Desktop`. Name every exported global and
struct with the `<Area>` prefix (`PlanningUi`, `PlanningCard`), and keep
components that two of your files share inside `ui/<area>/`.

Existing files an area takes over (it is their only editor from now on):

| File | Owner |
|---|---|
| `ui/workbench.slint`, `ui/chat.slint`, `ui/parked.slint`, `ui/resume.slint`, `ui/modes.slint`, `src/app.rs` (the workbench and chats), `src/mosaic.rs`, `src/chat.rs`, `src/head.rs` and the other chat modules | workspace |
| `ui/browser.slint`, `src/browser.rs`, `src/server.rs`, `src/frames.rs`, `src/pump.rs`, `src/gpu.rs`, `src/mac.rs` | desktop |
| `ui/secrets.slint`, `ui/sreq.slint`, `src/secrets.rs`, `src/sreq.rs` (the chat's secrets dialog and requests dock) | ops |

Another area that needs something from these asks their owner (see below),
never edits them.

## How an area plugs in

### The tab host (Slint)

- `main.slint` draws the tab bar and, for any tab but Workbench,
  `AreaView { kind: <tab id> }` (`ui/areas.slint`), which draws your
  `<Area>View { kind, key, embedded }`. Switch on `kind` inside your view
  for your tabs.
- A workbench pane with the key `panel:<kind>` (the "+" menu, the empty
  pane's chips) draws the same `AreaView`, with `embedded: true` and the
  pane's `key`. One component serves the tab and the pane, as the web moves
  one panel node between them. Per-pane state (Notes in two panes) is keyed
  by `key`.
- `<Area>Overlay` is drawn over the whole window (under the shared dialogs,
  the tooltip and the toast). Put your dialogs and popovers there. It must
  take no input where it draws nothing (no full-size TouchArea unless a
  dialog is open).
- A dialog that covers the window sets its area's modal flag,
  `Shell.modal-<area>` (`ui/hooks.slint`), while open: the window then keeps
  hover and presses off what is under the scrim (`Pointer.modal`).

### The shell's hooks (`ui/hooks.slint`, global `Shell`)

| Hook | Owner | What |
|---|---|---|
| `Shell.tab` | read by all | the tab on show |
| `Shell.add-project()` | ops | the sidebar's "+ Add project" |
| `Shell.inbox()` | desktop | the sidebar's Inbox |
| `Shell.cru()` | desktop | the top bar's Cru mark |
| `Shell.toggle-browser()` + `browser-hooked` | desktop | the tab bar's Browser; until `browser-hooked` is set, main.slint opens the drawer itself |
| `Shell.badge-factories` | planning | the Factories tab's count |
| `Shell.badge-secrets`, `Shell.update-dot` | ops | the Secrets tab's count, Settings' update dot |
| `Shell.server`, `Shell.sign-out()` | ops (Settings) | the signed-in server; signing out |
| `Shell.modal-<area>` | each its own | a modal of the area is open |

Handle the callbacks in Rust: `w.global::<crate::Shell>().on_inbox(...)` in
your `init`. The badges are wired into the tab bar by the Lead on request
(they are not drawn yet).

### The registry (Rust, `src/areas.rs`)

`app.rs` tells `src/areas.rs` what happens, and it calls the same functions
in every area's `src/<area>/mod.rs`. Keep their signatures:

```rust
pub fn init(w: &MainWindow)          // at start: set models, wire callbacks
pub fn signed_in()                   // a session starts (kit::api() is set)
pub fn signed_out()                  // it ends: clear what you hold
pub fn project(alias: &str)          // the project changed ("" none)
pub fn tab_shown(tab: &str)          // a tab came on show (any: check yours)
pub fn event(e: &AreaEvent)          // the live link, below
pub fn test_ui(name: &str, args: &[String]) -> Option<Value>   // the driver's "<area>:<name>"
pub fn props() -> Value              // the driver's view of the area
```

`AreaEvent` is `Connected(bool)` (send your `/ws` subscriptions again on
`true`), `State(&Value)` (the event stream's `state`), `Server(name, &Value)`
(every other `/api/events` event, and `usage`, `inbox`, `chat-secrets`,
`secret-requests`) and `Ws(type, &Value)` (every other `/ws` message).

### The kit for Rust (`src/kit.rs`)

Use these, never `app.rs`'s internals:

- `kit::call(|api| api.get(..), |r| ...)`: a server call off the UI thread,
  the answer on it (`Err` is the server's words; 404 reads "Not supported by
  this server": say so, don't break). `api.get/post/put/patch/delete` and
  `api.request(method, path, body)`.
- `kit::bg(work, done)`, `kit::ui()`, `kit::api()`, `kit::base()`,
  `kit::project()`, `kit::projects()`, `kit::terminals()`,
  `kit::with_project(path)` (adds `alias=`), `kit::ws_send(&json)`,
  `kit::show_tab(id)`.
- `kit::toast(..)`, `kit::toast_error(..)`, `kit::toast_ok(..)`.
- `kit::confirm(Confirm { title, message, ok, danger }, |ok| ..)`: the web's
  `confirm()`. `kit::prompt(title, message, value, placeholder, ok,
  multiline, |text| ..)`: the web's input modal. `kit::menu(x, y, items,
  |id| ..)` with `kit::item("id", "Label").icon(kit::icon(|i| i.get_trash())).danger().sep()`:
  the web's `.ctx-menu`. One instance each, drawn by the window.
- `kit::s`, `kit::n`, `kit::truthy`, `kit::enc`, `kit::parse_iso`,
  `kit::ago`, `kit::size`.
- `kit::model()`, `kit::rc(&m)`, `kit::sync_rows(&m, rows)`: a list's model
  set once, rewritten only where rows changed.

### The shared widgets (`ui/kit.slint`)

Import from `../kit.slint` (it re-exports `widgets.slint` too). Build from
these before drawing anything of your own; they are the web's shared looks:

| Widget | The web's | |
|---|---|---|
| `PanelPage { embedded }` | `.services-panel` | a tab's scrolling page, padding and gap |
| `Toolbar { title }` + `AddButton` | `.svc-toolbar`, `.svc-add-btn` | title left, actions right |
| `Card`, `CardHead { name, tone, tag }`, `MetaLine` | `.svc-card`, `.svc-header`, `.svc-meta` | |
| `SmallBtn { kind, confirm-text }` | `.svc-btn` (`primary`, `danger`, `on`, two-click `.confirming`) | |
| `StatusTag { tone }`, `StateDot`, `Tones`, `CountBadge` | `.svc-status`, `.svc-dot` | tones: running/ok, error, warn, accent, "" |
| `EmptyState`, `ErrorState { retried }`, `LoadingState` | `.services-empty`, the panels' lines | |
| `ListRow { context(x, y) }` | `.file-item` | hover, selected, "…" and right click → `kit::menu` |
| `Table { cols: [TableCol], rows: [TableRow] }` | lists with columns | |
| `FormLabel`, `TextField`, `TextArea { submit }`, `Seg`, `Toggle`, `CheckRow`, `Select` | `.sm-label`, `.sm-input`, `.sm-seg`, switches, `<select>` | |
| `Modal { title, close }` + `ModalActions` | `.sched-modal-card` | scrim, card, head with ×, Esc |
| `SendBtn`, `UploadStrip` | the composer's Send, `.cc-up` | shared by the chat composer and the terminal input |
| `Icon`, `IconButton`, `Btn`, `Field`, `Spinner`, `SoftShadow`, `DashedBox`, `RevealFlickable`, `ScrollThumb`, `AccentFill` | | from phases 1 and 2 |

Every part is shown in its states in `ui/gallery.slint` (the driver's
`kit-gallery` action; `test/areas.mjs` snaps it): look there for how to use
one. A part that two areas need goes into the kit: ask the Lead. A part only
you need stays in `ui/<area>/`.

### Theme and icons

- Tokens: only `ui/theme.slint`. The scales are the Lead's. An area adds a
  **named structural size** only inside its own marked section at the end of
  `Theme` (`// ── planning ──` … `// ── end planning ──`), named after its
  screen. Everything else is a scale step. `tools/tokens.sh` checks
  `ui/*.slint` and `ui/<area>/*.slint`: it must print 0 lines.
- Icons: `ui/icons.slint` is generated from the web's `ICON_PATHS`
  (`scripts/gen-icons.mjs`); don't edit it. An icon the web draws inline
  (not in `ICON_PATHS`) goes in `ui/<area>/icons/` with an
  `<Area>Icons` global in `ui/<area>/icons.slint`. Flat stroke icons, never
  emoji.

### The test driver

- Your actions: `{"cmd":"ui","name":"<area>:<name>","args":[...]}` reach
  `test_ui(name, args)`; return `Some(json!({"ok": true, ...}))`, `None` for
  an unknown name. Call the same functions the buttons do.
- Your props: `props()`, seen in `{"cmd":"props"}` as `areas.<area>` and
  alone through `{"cmd":"area","area":"<area>"}`.
- The shared dialogs: `props.dialog`, `props.menu`; answer with the
  `dialog-answer` (`"true"|"false"`, text) and `menu-pick` (id) actions.
- `tab` (show a tab), `add-pane` (`kanban`, `files`… a panel in the
  workbench), `mouse`, `key`, `snapshot` as before.
- `test/lib.mjs` is the harness: `t.begin(area)` sets up a fresh server
  (a password, the project `demo`), starts the app with its own config and
  CEF profile, signs in and opens the project; then `T.ui`, `T.act`
  (your area's actions), `T.until`, `T.snap` (to `screens/<area>/`),
  `T.dialog`, `T.menuPick`, `T.end`. `test/areas.mjs` is a worked example.

## Building and testing side by side

- **One build folder:** `CARGO_TARGET_DIR=$HOME/.cache/crundi-native-target`,
  nothing else: no other target dir, no copy of CEF, no `--release` unless a
  test needs it. Build with

  ```
  BIN=$(native-client/tools/area-build.sh <area>)
  ```

  It takes builds one at a time (a lock), stages CEF, and copies the binary
  to `debug/crundi-native-<area>` (next to CEF's files), so another area's
  build never swaps the binary under your running test.
- **One checkout, four people.** Everyone edits the same tree, and one build
  compiles everyone's files. So: keep your files compiling at every save
  (small steps, build often); if the build fails in **another area's
  file**, wait a minute and build again, never fix their file; never
  reformat or move code you don't own; `git` is off limits (Crundi commits
  at hand-off).
- **Your ports** (test server, test driver). Never 80, 443, 8888, 22, 53 or
  7480-7579:

  | Area | Test server | Driver |
  |---|---|---|
  | workspace | 4390 | 4391 |
  | planning | 4392 | 4393 |
  | ops | 4394 | 4395 |
  | desktop | 4396 | 4397 |
  | Lead / Tester | 4398 | 4399 |

- **A run:**

  ```
  native-client/tools/test-server.sh 4392 --bare        # a fresh server, never by hand
  BIN=$(native-client/tools/area-build.sh planning)
  NATIVE_BIN=$BIN xvfb-run -a -s "-screen 0 1440x900x24" node native-client/test/planning.mjs
  kill $(cat /tmp/crundi-devtest-4392/server.pid)       # yours only, by PID
  ```

  `xvfb-run -a` gives your client a display of its own; `t.begin` gives it
  its own CEF profile (`CRUNDI_CEF_CACHE`) and config, so four clients run
  at once. A server needs to be fresh for each run (start it again).
  Stop only what you started, by PID; never `pkill`/`killall`.
- **The web side** for the comparison (DESIGN.md "The method"): your own
  test server without `--bare`, `tools/web-session.mjs <port>` and Crundi's
  hidden browser at 1400×860.
- **Disk:** keep 20 GB free (`df -h /`). Run `tools/clean-build.sh` at the end
  of every step; snapshots go to `screens/<area>/` (latest round only) or
  `specs/<area>/`, scratch in one /tmp dir you remove.
- **Before handing off:** your test passes; `test/areas.mjs` still passes;
  `cargo test` passes; `tools/tokens.sh` prints 0. The Tester runs
  `test/phase1.mjs` and `test/phase2.mjs` for everyone.

## Changing a shared file

Lead files: `ui/main.slint`, `ui/areas.slint`, `ui/hooks.slint`,
`ui/kit.slint`, `ui/dialogs.slint`, `ui/widgets.slint`, `ui/shell.slint`,
`ui/theme.slint` (outside the area sections), `ui/icons.slint`,
`src/main.rs`, `src/areas.rs`, `src/kit.rs`, `src/api.rs`, `src/live.rs`,
`src/testctl.rs`, `src/store.rs`, `Cargo.toml`, `build.rs`, `test/lib.mjs`,
`test/areas.mjs`, `tools/`, `scripts/`, this file.

Ask in the run's shared folder, `shared/requests.md`: one line per request
(your area, the file, what and why). Until it is done, work around it inside
your area. A request that blocks you (a new crate in `Cargo.toml`, say) goes
to the Manager through your hand-off. Small, additive and urgent, if
nobody else is in the file: make it yourself, as an addition with a comment
naming your area, and list it in `shared/requests.md` as done so the Lead
reviews it.

Server changes (`src/` outside native-client): only for an endpoint that is
truly missing, small and backwards compatible, tested, with `npm run check`
passing. Never `app/` or `android/`.

## Area notes

### Workspace

- **The terminal input is inside each terminal panel** (owner's decision,
  15 Nov 2026; a deliberate difference from the web, listed in PLAN.md). The
  web has one shared bar under the workbench (`#term-input-bar`,
  src/webapp-html.js ~3800, `syncInputBar()` ~9650) that types into the
  focused terminal. The native client has no shared bar and nothing depends
  on which terminal has focus: each terminal panel has, at its bottom, as a
  chat panel has its composer:
  - the text box ("Type here... (Ctrl+Enter or Send button)"; Ctrl+Enter or
    Send sends to that terminal);
  - the keyboard-tools toggle with its key grid (`.ttg`: Esc, Tab, the
    arrows, the Ctrl keys and the rest, as the web's);
  - attach (uploads to `crundi_attachments`, with its progress strip:
    `UploadStrip`);
  - Send (`SendBtn`).

  Everything else is as the web's bar: wording, placeholder, upload, the
  key grid; drawn with the theme's tokens and the shared widgets,
  consistent with the chat composer.
- A path in a reply opens in Files (2.16's gap): workspace owns both ends.

### Planning

- The Factories tab's badge is `Shell.badge-factories`.
- The factory panel (`factory` kind) and the run graph are planning's; a
  factory's result card in a chat (2.20) stays workspace's.

### Ops

- Settings keeps "Sign out of <server>" (`Shell.sign-out()`); it is in the
  stub until Settings is built.
- The approvals button (top bar) and the Secrets badge are ops'. The inline
  widget card in a chat (`widget_open`, slot inline) is drawn inside the
  chat log: ops builds the widget view in `ui/ops/`, workspace places it in
  the log (ask workspace).

### Desktop

- The browser drawer (`ui/browser.slint`) is drawn by main.slint today; set
  `Shell.browser-hooked` when you take over the Browser toggle.
- The chat bubble button in the pane header lives in `ui/chat.slint`
  (workspace's): ask workspace to add the button, calling a desktop global.
