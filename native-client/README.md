# Crundi native client: prototype (Rust + Slint + CEF)

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

## CI

`.github/workflows/native-client.yml` runs for pushes to the `native-client`
branch that touch this folder. On Ubuntu, Windows and macOS (Apple silicon) it
builds, runs the server test below, and keeps the test's pictures. It also
keeps a runnable package for each OS (artifacts, 14 days).

## Server test

`node native-client/test/drive.mjs 7611` runs the real `src/browser.js`
behind a `/ws`. With `NATIVE_BIN=<the binary>` it starts the prototype too;
otherwise, start the prototype with `CRUNDI_SERVER=ws://127.0.0.1:7611/ws?token=test`.
The test opens both panels and runs eval, read-page, click, the mouse,
navigate, device emulation, resize, a pin, screenshots, a snapshot of the
whole window, and close. The pictures go to `DRIVE_OUT` (default: the temp dir).

## Layout

- `src/main.rs`: starts CEF (one binary is also CEF's helper processes) and
  pumps CEF's message loop from Slint's event loop.
- `src/mac.rs`: macOS only. It loads the framework and makes winit's
  NSApplication adopt CEF's protocol.
- `src/browser.rs`: the Slint and CEF bridge. It holds the render, life-span,
  load, display and DevTools handlers, input, device emulation and annotations.
- `src/server.rs`: the `/ws` native-host protocol.
- `ui/main.slint`: the window, the panels and the annotation layer.
