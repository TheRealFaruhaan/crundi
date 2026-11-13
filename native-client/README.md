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
  generated from `ICON_PATHS` by `scripts/gen-icons.mjs` (`ui/icons/`).
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

It needs a fresh dev Crundi that shares nothing with a live one:

```
mkdir -p /tmp/t/config/crundi /tmp/t/data /tmp/t/proj
printf 'TELEGRAM_BOT_TOKEN=\nPROJECTS_DIR=\n' > /tmp/t/config/crundi/.env   # never the live bot
env -u TELEGRAM_BOT_TOKEN -u PROJECTS_DIR XDG_CONFIG_HOME=/tmp/t/config DATA_DIR=/tmp/t/data \
  DOTENV_PATH=/tmp/t/config/crundi/.env node src/index.js --dev &     # port 8889
CRUNDI_TEST_SERVER=http://localhost:8889 CRUNDI_TEST_PROJECT=/tmp/t/proj \
  NATIVE_BIN=native-client/target/debug/crundi-native node native-client/test/phase1.mjs
```

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
  - `signin.slint`, `sidebar.slint`, `chat.slint`, `browser.slint`: the screens;
  - `widgets.slint`: shared parts;
  - `theme.slint` and `icons.slint`: the look.

## Adding a screen

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
   - snapshots from the test driver;
   - the CI build on all three systems.
6. **A server without an endpoint** (older version): the screen says "Not
   supported by this server" (`ApiError::Status(404)`) and doesn't break.
