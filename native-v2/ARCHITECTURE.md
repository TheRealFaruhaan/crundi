# native-v2: the architecture

Crundi's native PC client, built for speed the way Zed is built on GPUI.
It replaces `native-gpui/` with **exactly the same user experience**: the
reference is the web UI and the Electron app (`src/webapp-html.js`, `app/`,
`app/vendor/*.js`), with `native-gpui/` as a second reference where it
matches them (where it differs, the web and Electron client win). Speed
work changes how a screen is built, never what the person sees or does.
Nothing is invented: when the reference is unclear, ask the Manager.

This file holds the rules. Every crate follows them; a rule you need to
break is a question for the Manager, not a shortcut. `AREAS.md` is the
contract between the areas (who owns which crate, the hooks, the shared
controls); `PLAN.md` is where every screen stands.

GPUI is `gpui-pre =0.3.12` (a snapshot of Zed's gpui, imported as `gpui`)
with `gpui-component` / `gpui-kit-assets =0.10.0`; CEF `=156.4.0` only in
the browser host (`crates/browser-host`), started lazily in a process of
its own.

## The seven rules

1. **State lives in GPUI entities owned by the area that uses it.**
   Nothing reads a global store while rendering. A view renders from its
   own fields and from the entities it holds a handle to (and observes).
   `Global`s hold only *hooks and handles* set once at start (the area
   registry, the `Stores` handles): never data that changes, and they are
   read in event handlers and init, never in `render`.
2. **A change notifies only the views that show it.** A store calls
   `cx.notify()` when, and only when, what it holds changed; the views
   that show it `cx.observe(&store, |_, _, cx| cx.notify())` (or are the
   store). There is **no `cx.refresh_windows()` and no `window.refresh()`
   anywhere** (both drop every view cache in the window); `tools/check.sh`
   fails the build on one, and CI runs it.
3. **Every view is cached at the right level** (`Entity::cached(style)`),
   and **only leaves are cached**. Two facts of gpui (`view.rs`,
   `window.rs`) decide where: a notify marks the view *and all its
   ancestors* dirty (`Window::mark_view_dirty`), and a cached view that
   re-renders does so with `window.refreshing` set, so **every cached view
   inside it is ignored and re-rendered too**. So:
   - a **layout view** (the Shell, a tab's root, the Workbench) holds child
     views and little else; it is **not cached** (it is an ancestor of
     everything that changes and re-renders with any of it, so it must be
     cheap: no text, no lists, no parsing);
   - **between a layout view and its leaves there is no view at all** when
     the part is layout only and holds several leaves (a chat pane's log
     over its composer): it is a **body**, elements built in its layout
     view's render (`workbench::PaneParts::body`, `chat::ChatPane::body`),
     placing its leaves cached. As a view it would be in every leaf's dirty
     path: uncached, every pane's would re-render with any one log's
     notify (F31, 20 Nov: 2164 ChatPane renders in 541 frames); cached, it
     would re-render its other leaves (its composer) with it;
   - a **leaf view** (TopBar, Sidebar, ProjectBar, a PaneHead, a LogView, a
     Composer, a kanban column…) is cached and holds **no cached view
     inside it**; the top cached views sit side by side;
   - a part of a leaf that changes often (a streaming row, a clock, a dot)
     is its own leaf view beside it, never a cached view inside it.
   A cached view needs a definite size from its style (`size_full`, a fixed
   height): it is laid out from that style, not from its content.
4. **Big content is virtualised.** Chat logs, the kanban, the mind map,
   file trees and every list longer than a screen draw only what is on
   screen: `gpui::list` (`ListState`, variable heights, measured once and
   re-measured only for the rows that changed, via `splice`) or
   `uniform_list` (fixed row height). A store change splices only the rows
   it touched; it never `reset`s the list unless the whole content was
   replaced (a new history).
5. **Work happens once per change, off the render path, and where it can
   off the UI thread.** Markdown, ANSI, JSON and diffs are turned into
   view state when the data changes, not when it is drawn: chat text into
   blocks of styled runs (`core::webmd`, the web's own markdown rules; a
   long reply on the background executor, a streamed one only from its
   last open block), server JSON into typed rows by the store. `render` only lays out what is ready. Streams
   (a chat's events, logs, the event stream) are **batched to at most one
   update per frame**, as Zed's terminal and agent panel do (below).
6. **Animations are capped and run only while visible.** Every
   `with_animation` uses `Animation::with_max_fps` (30 unless the
   reference needs smoother); an animation runs only while its element is
   drawn and its reason holds (a spinner while working, the sidebar's fold
   for its 220 ms). `window.request_animation_frame()` only inside such an
   animation's own view, only until it ends. **Nothing redraws while
   nothing changes**: no polling timers that touch views, no periodic
   notify. A clock that must tick ("updated 3 min ago") is its own tiny
   view, notified at the resolution it shows (once a minute).
7. **Every interactive element that shows hover has an id** (`.id(...)`):
   GPUI tracks hover only on elements with one; a hover style or tooltip on
   an element without an id never shows.

## The frame budget

On the owner's PC (1920x1080 at 144 Hz, AMD Renoir): **95% of frames at
most 7 ms** (`ZED_MEASUREMENTS=1`, gpui's own "frame duration" line per
drawn frame) while scrolling a chat, the kanban and the mind map with five
panes open and a chat streaming; **idle with nothing animating: no frames
at all.** `tools/frames.mjs` reads the app's stderr and gives p50, p95, the
share of frames at most 7 ms and the number of frames drawn in an idle
window; `test/lib.mjs` measures every suite with it (`T.frames`). Release
builds on the owner's PC are the numbers that count; debug builds and
software Vulkan under Xvfb give the *idle* check and relative numbers.

## The state model

### Where data lives

```
 server ──/ws──┐                       ┌─ Session      (shell)    signed in, server, user, online, sign-in step
               ├─ core::live (threads) ┤  Projects     (shell)    the projects, the open one, the chats' states (dots)
 server ─SSE───┘   │ one channel       ├─ Usage        (shell)    the top bar's bars
                   ▼                   ├─ Chats        (chat)     id -> Entity<ChatStore>
           LiveHub (shell, UI thread)  │   ChatStore   (chat)     one chat: Conversation + its rows (parsed blocks)
           batch per frame, route ─────┤  Workbench    (workbench) the mosaic, focus, pane hosts
                                       └─ area stores (planning, workspace, operations, desktop: their own)
```

- `crundi-core` has **no GPUI**: the API (`api`), sign-in flows, the live
  link (`live`), the saved login (`store`), the conversation model
  (`chat::Conversation`), the mosaic (`mosaic`), the tool / markdown /
  question rules, the test driver's protocol (`driver`). Pure data and
  rules; unit-tested with `cargo test`.
- Each **store is an entity** owned by the area that uses it. It turns
  server data into view state when the data arrives (once per change) and
  notifies only if something changed (a state event equal to the last one
  notifies nothing).
- **Handles, not lookups.** At start the app makes the stores and passes
  their handles (`crundi_shell::Stores`) to each area's `init`; a view
  keeps the handles it needs in its fields. A view never looks a store up
  by name while rendering.
- **Who may change what:** a store changes only through its own methods.
  Another area asks through a hook (`AREAS.md`), never by reaching in.

### How server data becomes view state

1. `core::live` runs the links on their own threads (below) and sends each
   message, parsed into `serde_json::Value` there (off the UI thread),
   down one unbounded channel.
2. `crundi_shell::live_hub` awaits the channel on the UI thread (no
   polling), takes **everything waiting** as one batch, and routes it in
   one `update`: chat messages grouped by chat to that chat's
   `ChatStore::apply_batch` (one notify per chat per batch), the state
   event (only the last of the batch) to `Projects`, usage to `Usage`,
   other events to the areas' `on_event` hooks. Then it waits one frame
   (`FRAME`, 1/144 s rounded up to 8 ms) before taking the next batch:
   at most one update per frame, however fast the server streams.
3. A store derives its rows only for what the batch touched
   (`Conversation` reports which entries changed), parses again only the
   tail of a streamed reply (`webmd::update`; a long one on the background
   executor), splices its `ListState` for the rows that changed, and
   notifies once.
4. The views that show the store were observing it: only they re-render.
   A streaming chat redraws its log's visible rows and its pane's head
   when the state changes; the top bar, the sidebar, the other panes and
   the drawer are reused from their caches.

## The view tree and where it is cached

```
Root (gpui-component)
└─ Shell                 layout   the frame; sign-in or the signed-in tree
   ├─ top bar            elements its leaves side by side, each cached at its measured width:
   │                              UsageRows, TopBar (brand, project), UsagePill, TopRight (Telegram,
   │                              Push / Ask to merge), CruButton (Cru's mark: its 24 fps breathing
   │                              redraws it alone, F66), Approvals, ConnBadge, WinButtons, ReconnLine
   ├─ Sidebar            cached   PROJECTS, fold, the projects and their dots, + Add project, Inbox
   ├─ ProjectBar         cached   the tabs, "+", Browser
   ├─ <tab's root>       layout   one per tab, made when first shown; the workbench tab's is the Workbench
   │  └─ Workbench       layout   the mosaic's rectangles, gutters, empty panes
   │     ├─ (leaf)       elements one per pane: its panel's box, placed by the mosaic
   │     ├─ PaneHead     cached   one per pane: grip, dot, title, badge, tools, ×
   │     ├─ (chat body)  elements (a pane kind's body, no view) log over composer
   │     │  ├─ LogView   cached   the virtualised log (list): only visible rows
   │     │  └─ Composer  cached   the box, Send / Stop
   │     └─ Launcher     cached   ("pend:" panes) New terminal: Empty Shell, UI Mode, Terminal
   ├─ Drawer             cached   the browser drawer (Desktop)
   ├─ (overlays)         cached   the areas' layers over the window (register_overlay), side by side
   └─ layers             layout   the ask layer (confirm, prompt, menus), the modal layer
```

- A head redraws only when what it shows changed: the kind's `watch`
  calls `PaneHead::refresh`, which compares the info; the Workbench, as it
  lays the panes out, notifies only the heads whose frame (armed, pinned,
  narrow, focused) changed. A head never observes a whole store (F11).
- A streaming chat notifies its LogView: the LogView, the Workbench and
  the Shell are dirty. The two layout views re-render (cheap: no text),
  the LogView re-renders its visible rows, and every other cached leaf
  (the top bar, the sidebar, every head, the other logs, every composer,
  the drawer) is reused from its cache.
- A wheel tick over a log is the same: the list notifies the LogView, and
  GPUI marks every ancestor view dirty with it (`Window::mark_view_dirty`
  walks the view path up; Zed's workspace does the same), so the Workbench
  and the Shell re-run their `render` once per frame that scrolls (F21).
  That is why the layout views hold no text and do no work: the
  Workbench's render is the mosaic's few rectangles, one div per leaf and
  each pane's body (a few divs around cached views); nothing under them
  that is not dirty is rebuilt. The driver's `renders` shows it: a wheel
  over one log renders `log` once a frame and no `composer` or `head`.
- An animation (the launcher's gradient and caret, 30 fps) re-renders its
  view and that view's ancestors once per animated frame, and nothing else.
- An empty workbench is never blank (F18): with no panes it puts in a
  "New terminal" launcher pane (the web's placeholder cell), the first
  screen of a fresh project and after the last pane closes; the next pane
  placed (a chat at sign-in) takes the launcher's place. The empty leaf
  ("Drop a panel here…") appears only for a split, as in the web.
- The SignIn view replaces the signed-in tree while signed out (one cached
  view; its fields are its own `InputState`s).
- Any view that changes per frame during an animation (the sidebar's fold)
  is the animated view alone; its siblings stay cached.
- Focus listeners (`on_focus`, `on_blur`, `on_focus_in/out`, an input's
  Focus and Blur events) run inside the draw, after paint. A notify there
  marks the view dirty but asks for no frame (gpui's `invalidate_view`
  wakes the platform only outside a draw), so the view would stay stale
  until some unrelated frame, an idle frame much later (F137). The Shell
  handles it once for every area: each focus change re-renders the window
  (`window.focus` and `blur` refresh it), and when the Shell sees the
  keys moved it asks for one more frame, which draws what the listeners
  changed. Areas notify from focus listeners as usual; nothing else is
  needed.

## Virtualised lists

- **Chat log**: `gpui::list` with `ListAlignment::Top`, `FollowMode::Tail`
  (stays at the end while following; the person scrolling up stops it,
  "Go to the newest message" brings it back), overdraw `T::LOG_OVERDRAW`.
  Rows are the store's: `ChatStore.rows: Vec<Row>` where a row is a typed
  record (user bubble, tool card, thinking, result, error, notice) or **one
  markdown block of an assistant text** (`Rc<webmd::Block>`: its kind and
  its text as styled runs, parsed once). `render_item` reads a row and lays
  it out; it never parses.
- **Claude's text is drawn as the web draws it** (F19/F24): `core::webmd`
  is a port of app/vendor/claude-chat.js's `md()`, `inline()` and
  `linkify()` (headings `#` 14 px bold, `##` bold at body size, `###`+ the
  browser's h5/h6; a nested list item flat, as the web; the rule the
  browser's `hr`; code spans, links, bare URLs and paths), and the log
  gives each block the margins the web's `.cc-assistant` CSS gives it,
  collapsed with its neighbour's as CSS collapses them
  (`chat::log::margins`). Two things differ from the web by the owner's
  call (19 Nov): a GFM table is drawn as a table, and a code block's text
  is coloured by its language (`core::syntax`: syntect with bat's syntaxes,
  the crates gpui-component builds; colours from the theme's `syn_*`
  tokens, readable on the dark background), with no language label.
- **A reply is one row per block** (F10; Zed's agent panel keeps a long
  message cheap the same way: only what is on screen is laid out and
  painted): a paragraph, a heading, a list item, a quote line, a code
  block, a table, a rule. While a reply streams, `webmd::update` parses
  again only from the last line where nothing was open (no paragraph, no
  unclosed fence; a list's place in it is carried over), so a batch splices
  the new blocks and the last one or two before them; earlier rows are
  kept as they are.
- **What a scroll frame costs, and why a block is one element** (F31):
  GPUI 0.3.12 has no scroll that skips the view. `gpui::list` and
  `uniform_list` move their offset and then notify the view that drew them
  (list.rs `StateInner::scroll`: `cx.notify(current_view)`; Zed's lists do
  the same), and a cached view is reused only at the same bounds, so every
  visible row is built, laid out and painted again on each frame that
  scrolls. So the work per row is what has to be small. As Zed's editor
  draws its lines, **each of Claude's blocks is one element**:
  `chat::log::BlockEl` (a paragraph, a heading, a quote, a list item, a
  rule) and `chat::log::TableEl` (a table). Each is a single measured
  taffy leaf, with no subtree. Its text is shaped once per block, width,
  text size and selection, kept in the LogView (`laid`, `tables`, by the
  block's `Rc`), and only painted while the log scrolls. A streamed batch
  reshapes only the blocks it spliced. A code block keeps its box (it
  scrolls sideways), and a user bubble, a tool card and the rest stay
  divs. Hit-testing reads the same cached lines (`HitLayout::Lines`), so
  selection, double- and triple-click, shift-click, links and copy work
  as before (test/log-element.mjs).
- **The same pixels**: these elements reproduce what the divs drew,
  pixel for pixel (checked against the div version at several widths and
  scroll positions): lengths snap to device pixels as GPUI snaps authored
  lengths, measured heights round up as GPUI rounds a measured leaf, a
  table's columns are flex shares (each cell's padding and border its base,
  the rest shared equally) with edges rounded where they fall in the
  window, and a border quad's background is its own colour at no alpha,
  as a div paints it.
- **One paint layer per log** (`w::layer` around the list; "Paint layers" below): each
  clean cached view's last paint is replayed every frame, primitive by
  primitive (scene.rs `replay`). Outside a layer, each glyph and quad is
  inserted again into the scene's bounds tree. Inside one, it takes the
  layer's order, so a log costs one insert. For the same reason a line is
  painted straight into that layer (`paint_glyphs`, GPUI's `paint_line`
  without its per-line layer). A line with a background (a code span, the
  selection) keeps GPUI's two passes, so its glyphs stay above its
  backgrounds. Inside a layer, overlapping quads draw in an order set by
  their kind, not by when they were painted. So nothing in a log row may
  paint a quad over glyphs.
- **What every frame still costs**: replaying the panes that did not
  change grows with what is on screen. `perf/floor-*.md` measures GPUI's
  own floor and the client's chat panes (`crundi-v2-floor`,
  tools/measure-floor.mjs).
- **Where the parse runs**: on the UI thread only within `INLINE_PARSE` (4 KB
  per update, well under 1 ms; a streamed tail is far less); anything more
  (a history's long replies) is parsed on the background executor, one job
  per entry at a time, the rows refreshed when it lands. Until then a known
  reply keeps its last cut; a reply never cut draws nothing yet. The syntax
  set loads on a background thread at start (`syntax::warm`).

## Paint layers

Every frame, GPUI replays the last paint of every clean cached view into the
new scene, primitive by primitive (scene.rs `replay`, window.rs
`reuse_paint`), and the renderer uploads and draws the whole scene. A
primitive painted outside a paint layer is inserted into the scene's bounds
tree to find its draw order, and that tree holds every such primitive of the
frame, so each insert costs more the more there is on screen; a primitive
inside a layer takes the layer's order with no insert, and the layer's
primitives batch together (fewer draw calls). Measured on the owner's PC
(perf/pc-2026-11-20-architect.md), the pane heads, the composers and the
bars, painted outside layers, cost more per frame than the five chat logs
inside theirs.

- **The rule**: a view replayed on every frame paints in layers
  (`crundi_widgets::layer`, one element with no layout of its own): every
  pane head, the top bar, the sidebar, the project bar, the planning
  toolbars, the composer's and the terminal input's buttons and rows, each
  log, the Kanban's lists, the map, the terminal's screen.
- **What may go in one**: inside a layer GPUI keeps the paint order among
  quads and among glyphs and icons, but draws every quad, path and
  underline under every glyph and icon. So a layer holds what draws its
  backgrounds under its text, and nothing that must cover text or an icon
  drawn before it. Such a thing (the Telegram dot on its icon) is a layer of
  its own inside: a nested layer is drawn over the one around it.
- **Text boxes**: a focused text box draws its caret, a quad, over its text,
  so it is never in a layer while focused. `w::text_box` puts it in one only
  while the focus is not in it (no caret then), and the view that draws it
  re-renders on focus and blur (`w::watch_focus`): the composer's message
  box, the terminal's input.
- **What stays outside**: the layout views' own boxes (the panel frames,
  gutters), a few quads a frame; a text box with the focus; whatever a view
  draws over its own text (a scrollbar thumb draws its own layer).
- **Fewer elements in the layout views**: the Workbench re-renders on
  every frame that draws any pane (GPUI marks every ancestor of a notified
  view dirty), and a cached view under a dirty cached view is drawn again,
  not reused (view.rs: a dirty view renders with `refreshing` set), so the
  Workbench cannot itself be cached. It spends as few elements as it can: a
  pane is its panel's box (`w::panel_box`, which is also the leaf), its
  cached head, its body's box and the kind's body.

## Text and selection

- **One selection across the whole log** (F15), as in the web: a drag
  selects from block to block and message to message; a double-click
  selects a word, a triple-click a block; shift-click extends; a drag past
  the log's top or bottom (within 56 px of it, the web's `SEL_EDGE`)
  scrolls it, a 16 ms timer that runs only while the pointer is held there;
  Ctrl+C (Cmd+C) copies, a block's pieces by lines (a table's cells by
  tabs), list items by lines, other blocks and messages by a blank line.
  A click (no drag) on a link opens it (a path: the "open-path" action).
- **How**: every selectable text is a `StyledText` whose runs carry the
  span styles and the selection's background; a `canvas` after it reports
  its `TextLayout` to the log once it is laid out (only texts that were
  actually prepainted, so the layouts are valid). The pointer is mapped
  onto those layouts (`index_for_position`), the nearest one when it is
  between texts (Zed's markdown element does the same within one
  document; here the document is the log). The selection is kept as
  places in the text (entry, block, piece, byte), never as pixels, so it
  stays right while the log scrolls or streams, and rows scrolled away keep
  it. The pointer's moves and release while a press is held are taken
  window-wide (registered in the log's paint), so a drag may leave the log.
- No `InteractiveText`: it calls `window.refresh()` on every press
  (gpui's text.rs), which rule 2 forbids; the log handles its own links.

- **Kanban, mind map, file trees, long lists** (the areas'): the same; the
  mind map draws only nodes intersecting the viewport (a spatial index in
  its store) on a canvas.
- Item measurement: a row is re-measured only when the store splices it.

## The live link

- **/ws** (`core::live`): one socket for the chats this client watches
  (`subscribe-ui` → `ui-history`, `ui-event`) and the server's browser
  control (the native host). Its thread blocks on the socket with a short
  read timeout to interleave sends; it never wakes the UI on its own.
- **/api/events** (SSE): the state, usage, inbox, chat secrets, secret
  requests and the areas' events.
- **Heartbeat**: the link pings /ws every `PING_EVERY` (10 s); if nothing
  (a message or a pong) is heard for `SILENT_FOR` (25 s) the connection is
  taken as dead without having closed (a network change, a sleeping
  router), dropped and opened again (≈ 30 s worst case, under the 45 s the
  CR allows). The server sends no SSE keep-alive, so every new /ws
  connection after a silence also starts a fresh event stream (the
  generation counter: a stale stream's thread drops what it reads and
  ends). After a reconnect the chats re-subscribe and the server sends
  their history again; the stores replace their rows (`reset`).
- Every (re)connection and every silence-reconnect is counted
  (`Session.online`, the driver's `live` state), so a test can see it.

## Background parsing

- Markdown: an assistant text's blocks (above), parsed when its text
  changes, never when drawn; within the budget on the UI thread, else on
  the background executor; a code block's colours with it.
- ANSI (terminals), JSON tool bodies, diffs and the git log are parsed by
  the store when data arrives; anything over a few KB on
  `cx.background_spawn`, the result applied in one update.
- Images are decoded by GPUI's image cache off the UI thread.

## Animations

- `with_animation(id, Animation::new(d).repeat().with_max_fps(30), …)` for
  spinners, shimmers, the breathing Cru mark; a one-off ease (the
  sidebar's fold, 220 ms) requests frames only while it runs.
- An animation lives in the smallest view that shows it, inside its own
  cached parent, so its frames redraw that view alone.
- Nothing animates in an idle window: no cursor blink outside a focused
  input, no spinner without work, no polling redraw.
- A focused text box's caret blinks (500 ms) and rests, visible, 10 s after
  the last key or move (the theme's `CursorBlink` idle timeout; written
  through `gpui_component::Theme` and pushed to gpui_base's copy with
  `Theme::sync_base`, which the caret reads: F109). A box that loses the
  keys stops its caret at once (gpui-component's blur), and so does one in
  a window that goes to the background (GPUI sends the blur then).
- An animation's frame notifies its view, and GPUI marks the view's
  ancestors dirty with it: an animated mark is a leaf placed **beside** its
  neighbours (Cru's mark beside TopRight), never inside a view that does
  work in its render.
- Never a cached view around cached leaves to save their replay: while a
  cached view re-renders, GPUI renders everything inside it afresh
  (`window.refreshing`), so one leaf's notify would re-render all of them
  (tried for the top bar and the overlays in round 9: the suites caught it).

## The crates

| Crate | What |
|---|---|
| `crates/core` (`crundi-core`) | No GPUI. API, sign-in, live link (batched channel, heartbeat), saved login, `Conversation` (with changed-entry tracking), mosaic, rules, driver protocol |
| `crates/theme` (`crundi-theme`) | The tokens (`T`), registered as gpui-component's theme, fonts, icons (ported from native-gpui) |
| `crates/widgets` (`crundi-widgets`) | Every control once, from tokens (ported from native-gpui) |
| `crates/shell` (`crundi-shell`) | The stores' handles (`Stores`), `Session`, `Projects`, `Usage`, the live hub, the area registry; the window's views: Shell, TopBar, Sidebar, ProjectBar, TabBody, SignIn, the drawer's place |
| `crates/workbench` (`crundi-workbench`) | The pane host: the mosaic's layout, pane kinds, PaneHead, PaneBody, gutters, moving panes, "+" menu |
| `crates/chat` (`crundi-chat`) | `Chats`, `ChatStore`, ChatPane, LogView (virtualised), Composer, the streaming path |
| `crates/app` (`crundi-app`) | The binary (`crundi-v2`): makes the stores, inits the areas, opens the window, the test driver (never redraws the whole window) |
| `crates/browser-link`, `crates/browser-host` | The browser host process and its link (ported from native-gpui; the only CEF) |

The areas' crates (workspace, planning, operations, desktop) are added by
their owners on these rules (`AREAS.md`).

## The test driver

`CRUNDI_TEST_PORT=<port>`: the native-gpui protocol (`core::driver`: one
JSON command per line, one answer per line). Its requests arrive on a
channel the app awaits (no polling timer). It reads state through the
stores' handles and acts through the same methods the views call. It
**never** refreshes the window: an action that changes something notifies
the store it changed, as a click would. It answers as soon as the command
has run (one write, `TCP_NODELAY`: well under a millisecond, so a harness
can send input at a 144 Hz wheel's rate), and its `state` holds `renders`,
each counted view's renders (`core::driver::rendered`), so a suite can
check that a change redraws only the views that show it. At its first
command it gives the window the keys (the X input focus: under Xvfb no
window manager does), so focus, blur and carets behave as on a display.
Its `state` also holds `window` (active, as the system has it, and the
bounds): on Wayland "active" is the compositor's activated state, so it
shows the window is mapped and in front. A suite never reaches the
desktop's own session bus: `test/lib.mjs` drops the login session's bus
(the tray icon would show on that person's panel with its window on
Xvfb); test/desktop.mjs runs under `dbus-run-session`.

## Building

Only through `tools/build.sh` (one shared `CARGO_TARGET_DIR`, two jobs, a
lock, `oom_score_adj 1000`); `tools/check.sh` (the no-refresh check) runs
first. CI: `.github/workflows/native-v2.yml` on Linux, Windows and macOS.
