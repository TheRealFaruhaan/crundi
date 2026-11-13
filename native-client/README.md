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

## Build (Linux)

cef-rs downloads CEF while it builds. download-cef 3.0.0 fails here because the
CEF index is larger than its 10 MB limit, so fetch the minimal archive once by
hand and point `CEF_PATH` at it:

```
# cef_binary_156.0.26+g8ea0c73+chromium-156.0.8078.88_linux64_minimal.tar.bz2
# from https://cef-builds.spotifycdn.com, flattened into ~/.local/share/cef/cef_linux_x64
# (Release/* and Resources/* at the top, with include/, cmake/, libcef_dll/, and an archive.json)
CEF_PATH=$HOME/.local/share/cef/cef_linux_x64 cargo build
CEF_PATH=$HOME/.local/share/cef/cef_linux_x64 scripts/stage-cef.sh   # libcef + resources next to the binary
```

## Run

```
./target/debug/crundi-native                        # on a desktop
scripts/run-xvfb.sh                                 # headless server (Xvfb + Slint software renderer)
CRUNDI_SERVER='ws://host:port/ws?token=…' …        # driven by a Crundi server
CRUNDI_URL_A=… CRUNDI_URL_B=…                       # first page of each panel
```

## Server test

`node native-client/test/drive.mjs 7611` runs the real `src/browser.js`
behind a `/ws`. Start the prototype with
`CRUNDI_SERVER=ws://127.0.0.1:7611/ws?token=test`. The test opens both panels
and runs eval, read-page, click, the mouse, navigate, device emulation,
resize, a pin, screenshots and close.

## Layout

- `src/main.rs`: starts CEF (one binary is also CEF's helper processes) and
  pumps CEF's message loop from Slint's event loop.
- `src/browser.rs`: the Slint and CEF bridge. It holds the render, life-span,
  load, display and DevTools handlers, input, device emulation and annotations.
- `src/server.rs`: the `/ws` native-host protocol.
- `ui/main.slint`: the window, the panels and the annotation layer.
