# Areas: building the GPUI client in parallel

The native PC client is a Cargo workspace (`native-gpui/`), one crate per
part, so several developers work at once without touching the same files,
and each part compiles on its own. This file is the contract: who owns which
crate, how a part plugs into the window, what is shared, how to build and
test. Read `DESIGN.md` (layout and experience, tokens only, every control
from the widgets crate) first.

The Slint client in `native-client/` stays as the reference (its code, its
specs under `native-client/specs/`, its tests and the test-server tools).
Don't delete or change it, apart from its shared tools when a test needs it.

## The crates

| Crate | What | Owner |
|---|---|---|
| `crates/core` (`crundi-core`) | No GPUI. The server's API and sign-in flows, the live link (`live`), the saved login (`store`), drafts, the chat model (`chat`: `Conversation`, its rows), tool summaries, markdown rules, questions, schedule, slash, secrets, secret requests, agents, results, resume, media, mosaic (the pane layout), voice I/O, the native host's hook (`host`), the test driver's protocol (`driver`). Ported from `native-client/src` as it was | Lead (a part changes its own rule file there with a note in `shared/requests.md`) |
| `crates/theme` | The tokens (`T`), registered as gpui-component's theme (light and dark), the bundled fonts, the icons (`IconName`, `AllAssets`) | Lead (areas add named sizes in their own section) |
| `crates/widgets` | Every control, built once (below) | Lead (see "A new control") |
| `crates/shell` | The window: frameless, the top bar, the projects sidebar, the project tab bar, the tab host (`registry`), the drawer's place and divider, sign-in and pairing, the session (`session`) | Lead |
| `crates/workbench` | The pane host: the mosaic's layout, pane kinds (`register_kind`), the pane header, gutters, moving panes, the "+" menu, launcher, empty panes, live state | Workspace |
| `crates/chat-model` | One chat as a GPUI entity (`ChatModel`: the core's `Conversation`, connected, error, text size), the open chats (`Chats`), routing the live link's chat messages | Lead (Planning and Operations ask) |
| `crates/chats` | A chat pane: log over composer, its pane kind (`live:`), keeping panes in step with the project's chats, the chat's driver actions | Lead (Planning and Operations ask) |
| `crates/chat-composer` | A chat's composer side | Planning |
| `crates/chat-log` | A chat's log side | Operations |
| `crates/webview` | The CEF bridge: start, pump, off-screen pages drawn as GPUI images, input, device sizes, annotation strokes | Desktop |
| `crates/desktop` | The browser drawer, the Browsers tab, the native host (port `native-client/src/server.rs` onto `crundi_core::host`), later tray, Inbox, bubble, saved passwords, the homepool node, Cru | Desktop |
| `crates/workspace` | Git, Files, Media (phase 3) | Workspace |
| `crates/planning` | Kanban, Mind map, Notes, Schedules, webhooks, Factories (phase 4) | Planning |
| `crates/operations` | Services, Docker, Secrets, Skills, Usage, Settings, Homepool, Info, Panels, What's new (phase 5) | Operations |
| `crates/app` | The binary (`crundi-gpui`): starts CEF and the window, registers every part, pumps, the test driver's commands | Lead |

A crate's owner is its only editor. Inside a crate, one file per screen or
feature. A part that needs something from another part's crate asks its
owner (`shared/requests.md`), never edits it.

## This round: phases 0 to 2 on GPUI

Each must behave as the Slint client does, checked with its tests
(`native-client/test/phase1.mjs`, `native-client/test/phase2.mjs`), item by
item as `native-client/PLAN.md` lists them (2.1-2.25) and their specs in
`native-client/specs/`:

| Who | What | Crates |
|---|---|---|
| **Lead** | The shell: the top bar (usage bars, Cru's mark, connection badge, window buttons), the sidebar (projects with their dots, folding, Add project, Inbox), the project tab bar; sign-in (server step, password and code, Google / GitHub through the browser, Telegram), pairing, resume, sign-out; `test/phase1.mjs` passing in full | shell, app, core, chat-model, chats |
| **Workspace** | The workbench: 2.1 pane header wide, 2.2 narrow, 2.3 moving panes (drag, swap, split, empty pane and its chips, the "+" menu's layouts and Fit), 2.4 live state; terminal panels' launcher | workbench |
| **Desktop** | The browser drawer as the amendment says: beside the whole project area, below the top bar, the divider; hidden, the project bar's Browser button with its dot; tabs per project, the address bar (back, forward, reload, address, Network and Device menus, Annotate, DevTools), the annotation bar, annotations sent to a chat; the server's control of the browser (the native host, `test/drive.mjs`) | webview, desktop |
| **Planning** | The chat's composer side: 2.5 composer and Stop, 2.6 the permission-mode picker, 2.7 attachments, 2.8 drafts, 2.9 queued messages, 2.10 scheduled messages, 2.11 the slash menu, 2.12 the chat secrets dialog, 2.12b the secret-request dock, 2.24 voice input | chat-composer |
| **Operations** | The chat's log side: 2.13 questions, 2.14 tool cards, 2.15 thinking, 2.16 the rest of the log (and 2.16m media, 2.16f live frames, through webview: ask Desktop), 2.17 suggestions and recaps, 2.18 the agent dock, 2.19 goal and plan, 2.20 cards, 2.21 side questions, 2.22 the resume picker, 2.23 parked panes | chat-log |

The phase 0 to 2 gate: `test/phase1.mjs` and `test/phase2.mjs` pass against
the GPUI client, and the UI reviewer passes the screens. Phases 3 to 6 (the
areas' own crates: workspace, planning, operations, desktop) follow it.

Adapt a test only where it depends on Slint's internals (a Slint probe's
props, say); its checks of behaviour stay. Each adaptation gets a comment in
the test saying why.

## How a part plugs in

### The tab host and the registry (`crundi_shell::registry`)

Each part registers once at start (`init(cx)`, called by the app):

```rust
crundi_shell::register(cx, Area::new("planning")
    .tabs(&["kanban", "notes"], |tab, window, cx| /* the tab's page */ )
    .on_event(|e, cx| /* AreaEvent: Connected, Chat, State, Server(name), Ws(type) */ )
    .on_project(|alias, cx| …).on_signed_in(|cx| …).on_signed_out(|cx| …).on_tab(|tab, cx| …)
    .test_ui(|name, args, window, cx| /* Some(json!({"ok": true})) for a name it knows */ )
    .props(|window, cx| json!({ … }))     // merged into the driver's "props"
    .state(|window, cx| json!({ … })));   // merged into the driver's "state"
```

- The project bar's tabs and their order are `registry::TABS` (the web's),
  each with the part that draws it; a tab nobody registered says it isn't
  built yet.
- A workbench pane kind: `crundi_workbench::register_kind(cx, PaneKind {
  prefix: "panel:kanban", render, title })` (pane keys: `live:<chat>`,
  `pend:<n>`, `panel:<kind>`).
- The project bar's "+" by Workbench: the workbench registers what it does
  (`register_add_menu`: its add menu).
- The shell's own buttons that a part provides: `register_action(cx,
  "add-project", f)` (Operations: the Add project dialog). The sidebar's
  Inbox shows Desktop's `inbox` page.
- A chat's side question: the composer's send asks `crundi_chat_model::aside`
  first (`/btw`); the log registers it (`register_aside`).
- The drawer: Desktop registers its view (`register_drawer`); the shell
  draws it beside the project area with the divider, and hides or shows it
  (`crundi_shell::set_drawer`).
- The session: `crundi_shell::api(cx)`, `project(cx)`, `call(cx, work, done)`
  (a server call off the UI thread; an ended sign-in signs out; a 404 reads
  "Not supported by this server"), `run(cx, work, done)`, `ws_send`,
  `subscribe` / `unsubscribe` (a chat on /ws), `session::get(cx)` (projects,
  terminals, online, server, username).

### The shared controls (`crates/widgets`, the owner's rule)

Every control comes from here: `button`, `primary`, `danger`, `ghost`,
`icon_button`, `icon_toggle`, `toggle`, `switch`, `check`, `segmented`,
`input` (over an `InputState`), `field_label`, `menu_button`, `more_menu`,
`dot`, `state_color`, `badge`, `count_badge`, `tag`, `chip`,
`status_badge`, `card`, `panel`, `pane_head`, `toolbar`, `title_bar`,
`page`, `tab_row`, `tab`, `nav_tab`, `sidebar_item`, `dashed_button`,
`outline_button`, `empty`, `error_state`, `loading`, `list_row`, `confirm`,
`close_dialog`, `dialog_open`, `toast`, `divider`, `rule`.

- Since the phase 2 merge (16 Nov) also: `tip` and `tooltip` (one tooltip:
  `tip(el, text)` puts it on an element, `tooltip(text)` is the builder for
  `.tooltip(…)`), `modal` (a part's view as a modal) and `overlay_modal` /
  `modal_head` (one modal look), `pick_menu` (the menu control:
  `menu_button` and `more_menu` are made on it), `dropdown`, `choice_row`,
  `choice_mark`, `head_button` / `menu_item` / `menu_panel`, `send_button`,
  `stop_button`, `tone_button`, `square_button`, `suggest_row`,
  `queue_drawer`, `textarea`, `spinner`, `shimmer`, `markdown`, `icon`.
- **Every control's dimensions are the theme's** (the owner, 16 Nov: "use
  the theme; buttons and such dimensions on the theme"). `crates/theme` holds
  them (`T::BTN_H`, `BTN_PAD_X`, `BTN_GAP`, `BTN_ICON`, `BTN_TEXT`,
  `BTN_WEIGHT`, `BTN_RADIUS`, `BTN_H_LG`…, `ICON_BTN`, `ICON_BTN_ICON`,
  `INPUT_H`, `INPUT_PAD_X`, `INPUT_TEXT`); widgets sets each control from
  them (`control.rs`: `sized`, `sized_text`, `icon_of`), never from
  gpui-component's presets (`.small()`, `.xsmall()`, `.compact()`). A part
  never sizes a control (no `.h`, `.px`, `.text_size`, `.small()` on a
  control outside widgets); a box around one may have a width.
- One confirm / prompt and context menu for the window: the ask layer
  (`w::confirm`, `w::ask`, `w::ask_confirm`, `w::menu_at`), drawn once by
  the shell. A part never draws `w::ask_layer` itself.
- Never a copy in a part, and never a gpui-component control used directly
  in a screen (wrap it here once). The UI reviewer flags either as blocking.
- **A new control** (one that two parts need, or that is a control by
  nature): add it as its own file `crates/widgets/src/<control>.rs`, `pub
  use`d from `lib.rs` with one line, and list it in `shared/requests.md` for
  the Lead to review. Never change an existing control's look without the
  Lead; extend it (a builder method) instead.
- Layout (`div()`, flex, gaps from tokens) is not a control: write it in the
  screen.

### Tokens (`crates/theme`)

`T::SP_*`, `T::R_*`, `T::TEXT_*`, `T::W_*`, `T::LH_*`, the colours
(`T::accent()`…) and the named structural sizes. An area adds a named size
only inside its own section at the end of `T` (`// ── planning ──`).
Nothing else is typed into a screen but 0 and `px(1.)` hairlines.

### The test driver

The Slint client's protocol, unchanged (`crundi_core::driver`): `state`,
`props`, `snapshot` (xwd of the window under X11), `ui`, `area`, `mouse`,
`key`, `resize`. The window's own `ui` actions (`check-server`,
`sign-in-password`, `sign-in-pair`, `sign-out`, `select-project`, `tab`,
`drawer`, `pick-next`, `stub`) are the app's; every other name goes to the
parts' `test_ui` (unprefixed names to each part in turn, the phase tests'
names: `send`, `respond`…; `<part>:<name>` to that part). `props` and `state`
merge every part's, under the Slint client's names, so the phase tests read
them as before.

## Building and testing side by side

- **Build** only with `native-gpui/tools/build.sh [cargo args]`: the one
  shared `CARGO_TARGET_DIR=$HOME/.cache/crundi-native-target`, `-j2`, the
  shared lock, `oom_score_adj 1000`, CEF staged next to the binary. Never
  `CARGO_INCREMENTAL=0`, no other build folder, no copy of the source.
  `tools/build.sh -p crundi-chat-log` builds one crate (seconds);
  `tools/build.sh test --workspace` runs the tests the same way. Four jobs
  (the owner, 16 Nov), still one build at a time. When the last build in
  the shared folder came from another worktree, build.sh first touches this
  worktree's crate sources, so cargo never takes another worktree's build of
  a crate as yours (F18): always build through it.
- **The binary** is `$CARGO_TARGET_DIR/debug/crundi-gpui`. One build in a
  shared checkout serves everyone's tests: if a test needs your build to
  hold still, copy it next to CEF's files (`debug/crundi-gpui-<area>`).
- **Headless:** under Xvfb with Mesa's software Vulkan:
  `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a -s "-screen 0 1440x900x24" …`
  (`tools/run-xvfb.sh` for one picture, `tools/smoke.mjs` to check it
  starts). CPU figures on this server aren't representative (no GPU).
- **Tests** against a fresh test server, only through
  `native-client/tools/test-server.sh <port> --bare`:

  ```
  native-client/tools/test-server.sh 4392 --bare
  VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json CRUNDI_CEF_CACHE=/tmp/crundi-gpui-cef-planning \
    CRUNDI_TEST_SERVER=http://localhost:4392 CRUNDI_TEST_PORT=4393 CRUNDI_TEST_PROJECT=/tmp/crundi-devtest-4392/proj \
    NATIVE_BIN=$HOME/.cache/crundi-native-target/debug/crundi-gpui \
    xvfb-run -a -s "-screen 0 1440x900x24" node native-client/test/phase2.mjs
  kill $(cat /tmp/crundi-devtest-4392/server.pid)      # yours only, by PID
  ```

- **Your ports** (never 80, 443, 8888, 22, 53 or 7480-7579):

  | Part | Test server | Driver |
  |---|---|---|
  | Workspace | 4390 | 4391 |
  | Planning | 4392 | 4393 |
  | Operations | 4394 | 4395 |
  | Desktop | 4396 | 4397 |
  | Lead / Tester | 4398 | 4399 |

- **One checkout, several people:** keep your crate compiling at every save;
  if the build fails in another part's crate, wait and build again, never
  fix theirs; git is off limits (Crundi commits at hand-off).
- **Disk:** keep 20 GB free (`df -h /`); `native-client/tools/clean-build.sh`
  at the end of a step; screens under `native-gpui/screens/<area>/` (latest
  round only), scratch in one /tmp folder you remove.
- **Before handing off:** your parts of the phase tests pass;
  `tools/build.sh` and `tools/build.sh test --workspace` pass;
  `tools/smoke.mjs` passes.

## Changing a shared file

Lead files: `Cargo.toml` (the workspace; a new dependency is a request),
`crates/core`, `crates/theme` (outside the area sections), `crates/widgets`
(see "A new control"), `crates/shell`, `crates/chat-model`, `crates/chats`,
`crates/app`, `tools/`, this file, `PLAN.md`, `DESIGN.md`. Ask in
`shared/requests.md`: one line (part, file, what and why). Until it is done,
work around it inside your crate. A blocking one goes to the Manager in your
hand-off.

Server changes (`src/` outside the clients): only for an endpoint that is
truly missing, small and backwards compatible, tested, with `npm run check`
passing. Never `app/` or `android/`.
