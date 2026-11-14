# Crundi native PC client (Rust + Slint + CEF)

A first look at a desktop client without Electron, where web pages are real
Chromium browsers placed inside the app's own layout (card ee7d694caaf487e3).
Background: `docs/research/native-client.md`.

What it does:

- Two browser panels side by side in a Slint window. Each panel is a CEF
  browser rendered off-screen, and its frames are drawn as a Slint image.
- Mouse, wheel and keyboard are passed into the page: clicks, typing, Enter.
- An annotation layer: tick **Annotate** and drag over a page. Slint draws
  the strokes on top of the browser. A chat can drop a pin there as well.
- Device emulation for each panel (iPhone 15 Pro, Pixel 7, iPad Air,
  Desktop, Responsive). It sets the viewport size, the DPR, touch and the user
  agent through CDP. The page is scaled to fit the panel.
- Control from a Crundi server. The client registers on `/ws` as a native host,
  the same way the Electron client does, so the server's `src/browser.js` (the
  chats' `browser_*` tools) drives the panels without any change.

## Build

The same steps on Linux, Windows and macOS (Rust, Node 22; on Windows, bash from Git):

```
export CEF_PATH=$(node scripts/fetch-cef.mjs)   # CEF 156 for this OS, laid out for cef-rs
cargo build --release
node scripts/stage-cef.mjs target/release       # prints what to run
```

`fetch-cef.mjs` exists because cef-rs's own download (download-cef 3.0.0)
fails: CEF's index.json is larger than its 10 MB limit. `stage-cef.mjs` puts
CEF next to the binary on Linux (symlinks) and Windows (copies). On macOS it
builds `crundi-native.app`, which holds the framework and the five helper apps
(each one is the same binary). The .app is signed ad hoc.

## Run

```
target/release/crundi-native                                # Linux, Windows (.exe)
target/release/crundi-native.app/Contents/MacOS/crundi-native  # macOS (or open the .app)
scripts/run-xvfb.sh                                         # a server with no display (Xvfb, software renderer)
CRUNDI_SERVER='ws://host:port/ws?token=…'                   # driven by a Crundi server
CRUNDI_URL_A=… CRUNDI_URL_B=…                               # first page of each panel
```

## GPU path (experimental)

`cargo build --features gpu` compiles in a second way to get the page into the
window. CEF renders on the GPU and hands each frame over as a shared texture
(DMA-BUF on Linux, D3D11 on Windows, IOSurface on macOS). cef-rs copies it
into a wgpu texture on Slint's own device, and one render pass converts it
from BGRA to RGBA (`src/gpu.rs`), so the page never passes through the CPU.
It stays off unless `CRUNDI_GPU=1` is set. Without a usable GPU it says so and
falls back to the CPU path. Not yet seen working: it needs a machine with a
real GPU (this server has none; Chromium wants a GBM device on Linux).

## CI

`.github/workflows/native-client.yml` runs for pushes to the `native-client`
branch that touch this folder. On Ubuntu, Windows and macOS (Apple silicon) it
builds, runs the server test below, and keeps the test's pictures. It also
keeps a runnable package for each OS (artifacts, 14 days).

## Phase 1: signing in, projects, chats

The client has its own sign-in. Today's Electron client borrows the session of
the web page it shows; this one has no such page.

- **Sign-in methods:**
  - a pairing link from a signed-in device (Settings, then Connect a phone);
  - the server's address plus a password and authenticator code;
  - Google or GitHub, in the system browser.
- **The saved login:** the refresh token, in the user's config folder, readable
  by the user only (`src/store.rs`). Access tokens are never written down. A
  restart signs in again on its own.
- **The window copies the PC client's design exactly** (src/webapp-html.js,
  measured from the running page):
  - the 48px top bar, with the logo, the project and the connection badge;
  - the 240px projects sidebar, with Add project and Inbox;
  - the tab bar, with Workbench and its "+", the other tabs, and Browser on
    the right (with its dot when tabs are open behind a hidden drawer);
  - the **workbench**: panes tiled as the web UI's mosaic (`src/mosaic.rs`:
    splits and leaves, 6px padding, 7px gutters, panes at least 340px), saved
    per project; every live chat has a pane;
  - the "+" menu, with its Layout presets; the new-pane launcher (Claude: UI
    Mode); empty panes with their split buttons;
  - chat panes as the web chat draws them: the 28px header (CHAT tag, tools,
    "…" under 560px, a two-click ×), bubbles, markdown, tool groups ("N tool"),
    approvals and questions, the result line, the composer and its meta row;
  - the browser drawer on the right, with its tab strip, address bar, device
    menu and annotation.
- **Data:** projects and chats come from the server's API (`src/api.rs`).
- **Live updates:** one `/ws` socket and the event stream (`src/live.rs`).
- Tabs other than Workbench say they come in a later phase. Sign out is in
  Settings.
- **The look:** the web UI's colours (`ui/theme.slint`) and its icon set,
  generated from `ICON_PATHS` by `scripts/gen-icons.mjs` (`ui/icons/`), with
  the chat's permission-mode icons from `MODE_INFO` in `app/vendor/claude-chat.js`.
- **The server's browser control** is off unless `CRUNDI_NATIVE_HOST=1` is set.
  It would take that control over from an Electron client signed in to the
  same server.

`test/phase1.mjs` runs all of that against a real server through the test
driver (`CRUNDI_TEST_PORT`, `src/testctl.rs`):

1. password sign-in;
2. projects, a new chat, Claude's answer;
3. an approval allowed from the client;
4. sign-out, then pairing;
5. a restart that signs in from the saved login.

It needs a fresh dev Crundi that shares nothing with a live one. Start it
with `tools/test-server.sh`, never with `node src/index.js` by hand: a dev
server started without its empty `.env` reads the real one and runs the LIVE
Telegram bot. The script takes the port (`DEV_WEB_PORT` or the first
argument, default 8889; in a factory run, use the run's range), keeps its data
and config in `/tmp/crundi-devtest-<port>` so servers on different ports run
side by side, and stops only the server it started before on that port.
`--bare` leaves sign-in unset, which phase 1 and phase 2 need; they refuse a
server that has a Telegram bot configured.

```
native-client/tools/test-server.sh 4811 --bare          # never the live bot
CRUNDI_TEST_SERVER=http://localhost:4811 CRUNDI_TEST_PROJECT=/tmp/crundi-devtest-4811/proj \
  NATIVE_BIN=native-client/target/debug/crundi-native node native-client/test/phase1.mjs
kill $(cat /tmp/crundi-devtest-4811/server.pid)          # when done
```

Each test needs a fresh server: start `test-server.sh <port> --bare` again
before the next one (`test/phase2.mjs` the same way).

## Phase 2: chats

Everything a chat does in the PC client, screen by screen (PLAN.md lists
them; each has a spec in `specs/`). So far:

- **2.1 The pane header, wide** (`specs/pane-header-wide.md`):
  - the title: the long form on a wide header (capped at min(440px, 62%)),
    the short one at 160px; click to rename (Enter or leaving saves, Esc
    puts it back, empty becomes "Terminal"); a rename on the server shows
    live and doesn't clobber an open one;
  - the CHAT tag; the secrets lock with its count of grants, live from the
    `chat-secrets` event and counted again every 30 s (shown only when the
    server has `chat-secret-preauth`);
  - A- / reset / A+: the chat's text size (9 to 24, px/13 on every text size
    in the log and composer); each chat keeps its own, and every change is
    the size new chats start at (saved: `font`, `font_default_chat`);
  - pin: the pane keeps its px size, the dividers by it lock, presets keep
    it (saved in the layout);
  - full screen: the pane covers the window; the button or Esc leaves;
  - the two-click × ("Sure?", disarmed after 3 s or by a layout change; one
    pane at a time); a refused close (409) keeps the pane and says why in a
    toast;
  - double-click the header (not a button) to maximise the pane in its split
    (92 / 8), again to even it.

- **2.2 The pane header, narrow** (`specs/pane-header-narrow.md`): when
  the header's content box is under 560px (a pane of 567px or less), the
  header is 26px and keeps only full screen, "…" and ×; the short title has no
  cap and elides when the row runs out. "…" opens the folded tools in a
  menu under the header (CHAT, the lock with its label, A-, reset, A+, pin),
  which wraps right-aligned in a tight pane:
  - one menu at a time; A-, reset, A+ and pin leave it open;
  - the lock and full screen close it; so does Esc, and a press anywhere
    outside it, which still does its own job;
  - it stays open through live updates and closes when its pane goes wide.

- **2.3 Moving panes** (`specs/moving-panes.md`):
  - drag a pane by its header (more than 6px): a clone follows the pointer,
    every pane shows a dashed outline and the one under the pointer lights
    up; the drop swaps two panes, or moves a pane into an empty one (its old
    place closes up). A pinned pane is refused with a toast; Esc puts it back;
    near the grid's left or right edge it scrolls;
  - the split controls that fade in over a pane; the empty pane with its
    Terminal chip, Split ▸ / Split ▾ and ×;
  - the "+" menu's Fit to screen (every split even, a toast when it still
    doesn't fit) and Layout presets (2 columns, 3 columns, 2×2 cross,
    Left + right split, Top + bottom split: the web's trees and ids, so the
    saved layout is the same JSON);
  - the gutters' look (accent while any is held, locked by a pin), the 6px
    scrollbar when the grid scrolls sideways, and the 220ms layout motion.

- **2.4 Live state** (`specs/live-state.md`): each pane's dot (with the
  needs-input ring and the raw state as its tooltip) and its badge (WORKING,
  NEEDS INPUT, WAITING); the project's dot in the sidebar (needs input >
  working > waiting > running, with its ring; the active project with nothing
  running is green) and its folded avatar; titles changed by the server or
  another client, live. All from the `state` event's `terminals`, as the web
  does: an exited chat is grey whatever its stream last said, and a factory's
  employees don't light a project.

- **2.T One theme** (DESIGN.md, rule 2): every size and colour is a token
  in `ui/theme.slint` (spacing 2–48, radii, font sizes, weights, line
  heights, tracking, recurring sizes, motion, opacities, colours, gradients,
  and shadows as soft-layer lists). Screens type no number but 0 and 1px;
  `tools/tokens.sh` lists any that are left. The Rust layout numbers that
  match a token are named constants (`mosaic::{PAD, GUTTER, MIN_W}`,
  `head::{PANE_FOLD, TITLE_CAP, LONG_CAP}`).

- **2.5 Composer and Stop** (specs/composer.md): the message box grows
  from `control` to `composer-max` and then scrolls; Enter (or Ctrl+Enter,
  by the switch: one setting, saved as `chat_enter`) sends; Send reads
  Queue while Claude is busy and is never disabled; a send turns the chat
  working at once; Stop arms on one click ("Sure?", 3 s) and interrupts on
  the second. The meta row shows the state, the mode, the switch, the clock,
  the session id (a click copies the whole id), the model and the cost, and
  wraps. Under `composer-stack` (520) the composer stacks. A refused send
  shows the server's error in the log and gives the text back.

- **2.6 Permission-mode picker** (specs/mode-picker.md): the mode button
  wears each mode's colour; a click opens the picker (`ui/modes.slint`),
  centred in the window over a scrim, with the chat's modes (Bypass all
  first, for chats launched with skip permissions). Choosing changes the
  button at once and posts it; a refused change goes back with a toast.
  Plan mode puts PLANNING in the pane header (in the "…" tray when narrow).
  The mode follows `meta`/`init` events live; a mode outside the chat's list
  is ignored.
  For every modal to come: `Pointer.modal` (ui/widgets.slint, set by
  main.slint's `modal-open`) makes whatever follows the pointer ignore it
  under a scrim, and `RevealFlickable` scrolls a dialog to keep the
  keyboard's focus in view, with a `ScrollThumb` (also the composer's).

- **2.7 Attachments and 2.8 Drafts** (specs/attachments-drafts.md): attach
  opens the system's file picker (`rfd`); a picked or pasted file (a
  picture as image.png, through `arboard`) uploads to
  `/api/attachments/upload` with the upload bar (name, share, "42%" then
  "saving"), and the server's path goes into the box at the caret
  (`attach::insert_path`). Files dropped from the system on a chat go in as
  their own paths when the server is on this machine, else upload. Drafts
  (`src/drafts.rs`) are kept per server and chat with the project's copy
  (claimed by one open chat), restored when a composer is built, cleared on
  send, kept on close, pruned after 30 days.

- **2.9 Queued messages** (specs/queued-messages.md): sent while Claude
  works, a message is handed over at once and shows in a slate drawer
  ("SENDING not read yet" → "SENT Claude is reading it"), driven by the
  server's `queued-state` and `injected` events and its
  `pendingInjections`; sent while it needs input, lines are held in a
  dashed drawer and go out as one message when the prompt is answered or
  the turn ends. A click on either drawer, or ↑ on the box's first row,
  takes everything back (`cancel-queued` for each that can still be
  recalled). The rules are in `src/chat.rs` (unit-tested).

- **2.C Theme consolidation**: the theme is the scales (spacing
  2/4/6/8/10/12/16/20/24/32/40/48, radius 4/6/8/10/12/pill, font
  10–26, two letter-spacings) plus a short structural list (window, top
  bar, tab bar, sidebar, pane header and limits, drawer, control heights,
  icons); every other size is a step or a sum of two. `white` is a token,
  and `tools/tokens.sh` also flags named colours (`white`, `Colors.*`).
  Shadows and glows are `SoftShadow` (ui/widgets.slint): one smooth fade
  of gradients, never stacked layers.

The pure rules are in `src/head.rs` and `src/mosaic.rs` (unit-tested).
`test/phase2.mjs` drives each feature against a fresh server (the same setup
as phase 1's test), by the pointer and keys where it matters.

The test driver also takes `{"cmd":"mouse","kind":"move|click|dblclick|leave","x","y"}`
and `{"cmd":"key","text":"Escape|Return|…"}`, and `test/look.mjs` has the same as
`mouse:<kind>:<x>:<y>` and `key:<text>`, so snapshots reach hover and other
states.

## Server test (browser panels)

`node native-client/test/drive.mjs 7611` runs the real `src/browser.js`
behind a `/ws`. With `NATIVE_BIN=<the binary>` it starts the prototype too;
otherwise, start the prototype with `CRUNDI_SERVER=ws://127.0.0.1:7611/ws?token=test`.
The test opens both panels and runs eval, read-page, click, the mouse,
navigate, device emulation, resize, a pin, screenshots, a snapshot of the
whole window, and close. The pictures go to `DRIVE_OUT` (default: the temp dir).

## Layout

- **`src/main.rs`**: starts CEF (one binary is also CEF's helper processes),
  opens the window, and hands over to the app.
- **`src/api.rs`**: the server's HTTP API, the sign-in flows, and the one
  refresh at a time.
- **`src/store.rs`**: the saved login (server, refresh token, the last project
  and chat).
- **`src/live.rs`**: `/ws` (the chats' events, and the server's browser
  control) and `/api/events` (the chat list); they reconnect on their own.
- **`src/app.rs`**: the app on the UI thread: signing in, projects, chats, the
  open chat. Server calls run on worker threads (`bg`, `call`).
- **`src/chat.rs`**: one chat's conversation; turns the server's entries into
  rows on screen.
- **`src/head.rs`**: the pane header's rules (rename, text size, the secrets
  count, titles).
- **`src/testctl.rs`**: the test driver (`CRUNDI_TEST_PORT`).
- **`src/browser.rs`**: the Slint and CEF bridge (panels, input, device
  emulation, annotations). `src/server.rs` answers the server's browser
  requests.
- **Platform code:**
  - `src/pump.rs`: CEF's message loop;
  - `src/mac.rs`: the framework and NSApp on macOS;
  - `src/gpu.rs`: the GPU path.
- **The interface (`ui/`):**
  - `main.slint`: the window;
  - `signin.slint`, `shell.slint` (top bar, sidebar, tab bar), `workbench.slint`, `chat.slint`,
    `browser.slint` (the drawer): the screens;
  - `widgets.slint`: shared parts;
  - `theme.slint` and `icons.slint`: the look.

## Adding a screen

**Read `DESIGN.md` first.** Every screen copies the PC client's design
exactly: measured, compared side by side, and reviewed against a checklist.

Each screen follows the same pattern, so they stay alike:

1. **Spec:**
   - what today's PC client does there: the web UI (`src/webapp-html.js`) and
     `app/` for desktop-only behaviour;
   - the endpoints it calls, in the server's `src/`.
2. **Data:**
   - server calls go through `Api::get` / `Api::post`, on a worker thread via
     `app::call`, never on the UI thread;
   - live changes come from the event stream (`Event::State`) or `/ws`
     (subscribe-style messages): add a `live::Event` for a new kind;
   - turn the server's JSON into a plain Rust struct with a pure function, and
     unit-test it (like `chat.rs`).
3. **The view:**
   - a `ui/<screen>.slint` with `in` properties and callbacks only, made from
     `widgets.slint`, `Theme` and `Icons` (no emoji, no hard-coded colours);
   - the window passes its properties and callbacks through;
   - lists use a `VecModel` that is rewritten only where rows changed.
4. **Wiring:** `app.rs` sets the models and handles the callbacks. Add the
   callbacks to `testctl.rs` too, so tests can drive them.
5. **Proof:**
   - a step in a `test/*.mjs` against a real server;
   - snapshots from the test driver, compared with the web screen
     (`tools/compare.sh`, as `DESIGN.md` describes);
   - the CI build on all three systems.
6. **A server without an endpoint** (older version): the screen says "Not
   supported by this server" (`ApiError::Status(404)`) and doesn't break.
