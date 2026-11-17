# Crundi's native PC client on GPUI

The PC client, native, on GPUI (Zed's UI framework) with CEF for the
browser: the owner's decision of 16 Nov 2026 (`PLAN.md`). It replaced the
Slint client, removed on 17 Nov 2026 (in git history). Its specs (`specs/`)
and its tests (`test/`), which drive this client through the same test
driver, live here.

- `PLAN.md`: the phases, what is done, the gaps.
- `AREAS.md`: the crates, who owns which, how a part plugs in, the shared
  controls, the test driver, building and testing side by side.
- `DESIGN.md`: the PC client's layout and experience, the theme's tokens,
  every control from the widgets crate.

## The workspace

| Crate | |
|---|---|
| `core` | no GPUI: API, live link, saved login, chat model, rules, voice I/O, the driver's protocol |
| `theme`, `widgets` | the tokens (a registered gpui-component theme, fonts, icons); every control, once |
| `shell`, `app` | the window, the tab host, sign-in; the binary (`crundi-gpui`) |
| `workbench`, `chat-model`, `chats`, `chat-composer`, `chat-log` | the pane host; a chat pane: its model, log and composer |
| `webview`, `browser-link`, `browser-host`, `desktop` | the CEF bridge (the app's side, what both sides share, the browser host's executable); the browser drawer and the PC extras |
| `workspace`, `planning`, `operations` | phases 3 to 5's screens |

## What's new: a step for a native-only feature

The tour (`crates/operations/src/extras.rs`) keeps its words, dots, Skip,
Next, arrow keys and the recap native; each step's picture is the step's own
page, live in an off-screen CEF view inside the tour's frame
(`crundi_webview::frames`). The page comes from the signed-in server:
`/api/whatsnew` gives each step a `page`, `/whatsnew/<version>/<file>.html`,
which the server serves as a whole page (`src/whatsnew.js` `artPage`: the
step's fragment on the 640 x 400 stage, the app's colours, scaled to the
frame). Only the visible step's view is live; it is swapped on a change of
step and freed when the tour closes. If the page can't load, the frame shows
the step's title.

So a feature only the native client has gets its step exactly as any other
feature does, on the server, never in the client:

1. an HTML fragment under `app/vendor/whatsnew/<version>/` (markup and styles
   only, one root `<div class="wna a-name">`, looping, the app's colours:
   `app/vendor/whatsnew/README.md`, "An animation");
2. its entry in that folder's `steps.json`, the manifest the server reads
   (`id`, `tag`, `title`, `body`, optional `keys`, `art` = the file,
   `artTitle`); the server adds `page` itself;
3. `node scripts/test-whatsnew.mjs` (it checks the step has its page).

The version is the server's release that ships with the client's feature
(the README's rules on editions, dev editions and bug fixes apply as they
are). The web and Android tours show the step too, so word it for everyone
("On the PC app, ...").

## Stack (pinned)

- `gpui-pre =0.3.12` (as `gpui`) and `gpui-pre-platform =0.3.12` with `x11`
  and `wayland` (without them GPUI runs headless with no error);
- `gpui-component =0.10.0` and `gpui-kit-assets =0.10.0` (Lucide's icons,
  registered as the app's assets);
- `cef =156.4.0`: off-screen, BGRA frames shown as GPUI images, in a
  process and an executable of its own (the browser host, below): only
  `crates/browser-host` depends on `cef`, so `cef::*` and `gpui::*` never
  meet.

## The browser host (Chromium in its own process)

Every web view (the drawer's tabs, hidden browsers the server opens, widget
panels, a chat's live frames, Cru's screen, What's new's step pages) lives
in the **browser host**: an executable of its own, `crundi-browser-host`
(`crates/browser-host`), next to the app's. The app never loads or starts
CEF itself, and never maps libcef.

- **Three crates.** `crates/browser-host` is the only one depending on
  `cef`: CEF, its handlers (`views.rs`), its loop (`host.rs`), macOS's
  framework and NSApp (`mac.rs`). `crates/browser-link` has what both sides
  share with neither CEF nor GPUI (`ipc`, `shm`, `proc`, `common`, `gpu`).
  `crates/webview` is the app's side (the same API the areas always used:
  `engine`, `PageView`, `frames`; `client.rs` starts the host).
- **Process model.** The app starts `crundi-browser-host --browser-host`;
  CEF starts the same executable again for its helpers (`--type=…`), which
  leave in its `main`. `main` of the app still calls
  `crundi_webview::init_process()` first: started as `--browser-host` or
  `--type=…` (an older layout, a hand test) it execs `crundi-browser-host`
  with the same arguments (the same PID), so nothing of CEF ever runs in
  the app's image.
- **Packaging.** The host must sit beside the app with CEF's files:
  Linux and Windows the same folder (`tools/build.sh` and CI's
  `stage-cef.mjs` with `STAGE_HOST=crundi-browser-host`; the host's RUNPATH
  is `$ORIGIN`, the app's links nothing of CEF); macOS the bundle's
  `Contents/MacOS` beside `crundi-gpui`, CEF's five helper apps copies of
  the host (they load the framework, the app never does). Installers ship
  both executables.
- **The link** (`ipc.rs`): length-prefixed JSON, commands (`Cmd`: hello,
  open, close, size, navigate, history, mouse, wheel, keys, IME text,
  focus, hidden, CDP, script, DevTools, cookies, quit) and events (`Ev`:
  ready, frame segment, paint, loading, load error and end, title, address,
  cursor, console, popup, scroll, network, CDP answers, page process gone,
  texture, stats). Linux and macOS: a Unix socket pair, the host's end as its stdin (moved
  off fd 0, close-on-exec). Windows: the host's stdin and stdout.
- **Frames** (`shm.rs`): one shared-memory segment per view, BGRA, made
  once per size (a bigger frame makes a new one). Linux: a memfd passed
  over the socket (SCM_RIGHTS; never in /dev/shm). macOS: `shm_open`, the
  name unlinked by the app once mapped. Windows: a named file mapping.
  The handshake: the host writes a frame and sends `Paint`; the app's
  reader thread takes it and answers `Ack`; a frame that comes before the
  `Ack` is drawn again after it (`invalidate`, a whole frame).
- **Copies per frame: two on the CPU, then GPUI's upload** (was three).
  1. The host copies CEF's buffer into the segment, **only its dirty
     rects** when the segment holds the page's previous frame (a new
     segment, a resize, a frame let go while the app was busy or the view
     hidden: the next one whole). A frame whose dirty rects hold the same
     pixels as before is **not sent at all** (counted as `unchanged`).
  2. The app's reader copies the segment straight into a new GPUI image
     (`client.rs image_from`: the image owns the pixels), which is the
     panel's frame and the view's picture at once (before: the segment into
     a buffer, the buffer cloned into an image, the buffer kept beside it:
     two frames per view held in the app, now one).
  3. GPUI uploads the image into its atlas when it is first drawn.
  GPUI 0.3.12 cannot do better without a fork: a `RenderImage` is
  immutable and owns its `Vec` (no reuse of one buffer per view, no
  borrowed memory, so the segment cannot be the image), and its sprite
  atlas has no partial update (a tile is uploaded whole once per image:
  dirty rects stop at the segment). The replaced image leaves the atlas
  (`drop_image`) when the view takes the next one.
- **One rule for every web view: not visible, hidden at once; not visible
  for the unload grace, freed; visible again, loaded again.** Not visible
  means: a chat embed scrolled out of its log's view (or its box no longer
  drawn: the pane parked, hidden, in another tab), a drawer tab behind
  another, every tab while the drawer is closed or in another project, a
  widget panel not drawn, a What's new or Cru page not shown. Hidden, it is
  hidden in the host at once (`Cmd::Hidden`: Chromium's `WasHidden`, which
  stops its drawing and throttles the page as a background tab; its frame
  rate down to 1; any frame that still comes not sent), so it costs no
  frames. Not visible for `CRUNDI_BROWSER_UNLOAD_SECS` (default 30) it is
  freed (its view closed in the host); visible again, it is made again and
  shows the loading state until its first frame. Where: chat embeds
  (`chat-log/src/frames.rs`: made only when on the log's view; the 4500 px
  keep-alive is gone; CAP, 6, stays the ceiling, a new visible box taking
  the place of the view longest out of sight), drawer tabs
  (`desktop/src/drawer.rs unload_tick`: the tabs stay listed, the active
  one loads when the drawer opens, the others when clicked or used by a
  chat; a chat's use counts as visible), widget panels
  (`operations/src/panels.rs`: hidden 2 s after their box was last drawn,
  freed after the grace). What's new's step pages and Cru's screen are
  closed as soon as they are not shown (stricter than the grace). Hidden
  browsers the server opens are hidden after 0.5 s (as Electron's
  `show: false` windows) but never freed by the grace: they are the
  server's. A screenshot (`Page.captureScreenshot`) wakes a hidden panel
  until it is taken. With no views at all (and the drawer closed), the host
  stops after `CRUNDI_BROWSER_IDLE_SECS` (default 60).
- **GPU textures** (`gpu.rs`, behind `CRUNDI_BROWSER_GPU`; off by
  default). `CRUNDI_BROWSER_GPU=1` (Windows, macOS): Chromium keeps the GPU
  (no `disable-gpu`), CEF's accelerated off-screen rendering
  (`shared_texture_enabled`) hands each frame to `OnAcceleratedPaint` as a
  shared texture, and the host reads it back into the view's segment inside
  the callback (Windows: CEF's DXGI NT handle opened with D3D11
  `OpenSharedResource1` on the host's own device, copied to a staging
  texture, mapped; macOS: the IOSurface locked read only). The app's side
  is unchanged. `CRUNDI_BROWSER_GPU=texture` (Windows) also duplicates each
  handle into the app's process (`DuplicateHandle`, `Ev::Texture`); the
  app opens it on its own D3D11 device with `OpenSharedResource1`, checks
  its size, counts it (`browserHost.gpu.textures`) and closes it: the
  whole cross-process path, short of drawing. Linux stays on shared memory
  (no GPU here; the flag is ignored). Status per OS:
  - **Windows: host side and the hand-over written, compile-checked here
    (`cargo check --target x86_64-pc-windows-msvc -p crundi-browser-link`),
    not run; drawing the texture is blocked by GPUI.** GPUI's DirectX
    renderer gives the app no access to its `ID3D11Device` and has no
    primitive for an outside texture: `PaintSurface` exists but its
    `draw_surfaces` is empty on Windows, and the atlas takes only bytes.
  - **macOS: host side written (IOSurface readback), compile-checked, not
    run; drawing it is blocked by GPUI, the hand-over not done.** GPUI's
    `surface()` element takes a `CVPixelBuffer`, but its Metal renderer
    accepts only `420YpCbCr8BiPlanarFullRange` (NV12, for video): CEF's
    surfaces are BGRA, and converting to NV12 would blur text (4:2:0
    chroma). Handing the IOSurface to the app would need its mach port
    (`IOSurfaceCreateMachPort`) over a bootstrap service: not worth it
    until GPUI can draw it.
  - **Linux: shared memory only.** GPUI's wgpu renderer skips
    `PaintSurface` and does not expose its `wgpu::Device` (the `cef`
    crate's `osr_texture_import` needs one to import a DMA-BUF).
  What GPUI would need (any one, without a fork): access to its GPU device
  and queue, and an element that samples an app-owned texture (BGRA) in
  its scene; or `PaintSurface` taking BGRA on Metal and implemented on
  DirectX and wgpu.
- **Lifecycle.** No host at launch. The first view starts it (the drawer
  opened, an embed scrolled into view, a widget panel shown, a What's new
  step); the page shows a quiet spinner until its first frame. With no view
  for `CRUNDI_BROWSER_IDLE_SECS` (default 60) and the drawer closed, the app
  sends `Quit` (killed after 3 s) and all its memory is returned; the next
  view starts a new one. The drawer starts closed after a relaunch, its
  tabs listed (title, address, network, device) and nothing loaded;
  opening it loads the active tab, the others load when shown. Closed for
  the unload grace, the drawer's pages go (the rule above). Embeds and
  panels hold a view only while visible, and for the grace after.
- **Dying with the app.** Linux: `PR_SET_PDEATHSIG` (SIGKILL) set before
  exec, and a process group of its own. Windows: the app's job object with
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, the host put in it before `Hello`
  (so before any helper). macOS and everywhere: the host leaves on EOF of
  its link, plus a watchdog on its parent. CEF's helpers leave with the
  host. The host's PID is in `browser-host.pid` in the config folder; at
  start the app kills a host a crashed run left (with its group; a host is
  a process with `--browser-host` whose executable is the host's or the
  app's) and clears stale CEF locks (`SingletonLock`, `-Socket`, `-Cookie`,
  `lockfile`) in the cache folder and each network's profile.
- **Crashes.** The host gone without being asked: every view shows "This
  page crashed" with Reload (`w::page_crashed`); the app stays up, and
  Reload (or a server tool using a drawer tab) starts a new host. A single
  page process ending (`Gone`) affects only its view.

**Debugging it.** The app logs `[browser] …` (started, up, stopped, went
away) and the host `[browser host] …`, both to the app's stderr. The test
driver shows `browserHost` in the desktop area's props (state, pid, views,
starts, crashes, reaped, idleSecs; `frames` taken by the app, the host's
counters under `host`: sent, unchanged, hidden, bytes copied, GPU frames;
`unloadSecs`;
`gpu`: the mode and textures handed over; `libcef`: whether the app has it
mapped, always false), each drawer tab's `frames` and `paused` (hidden in
the host), each embed's in `frames`; `desktop:browser-host-stop` stops it,
`desktop:reload <tab>` is a tab's Reload. `ps --ppid <app pid>` shows the
host, `ps -g <host pid>` it and its helpers. `CRUNDI_BROWSER_IDLE_SECS=5`
makes the idle stop quick; `CRUNDI_NO_BROWSER=1` runs with no browser.
`crundi-browser-host --browser-host` by hand waits for `Hello` on stdin (30 s).
`native-gpui/test/desktop.mjs` part `hostproc` (`ONLY=hostproc`) checks
all of the above. `tools/pss.mjs` measures the app's memory (PSS over the
app's and the host's processes, idle and with one tab; its header says how
to run it); the numbers are in PLAN.md, "The browser host".

## Build and run (this server)

```
tools/build.sh                 # crundi-gpui and crundi-browser-host; the shared build folder, the lock, OOM-safe; stages CEF
tools/build.sh -p crundi-chats # one crate
tools/run-xvfb.sh              # one screenshot to /tmp/gpui.png (headless: Xvfb + lavapipe)
node tools/smoke.mjs "$CARGO_TARGET_DIR/debug/crundi-gpui" /tmp/out    # starts, answers its driver (CI's check)
tools/cpu.sh <pid> [secs]      # CPU and memory of the client and CEF's helpers
```

The phase tests run against it with `NATIVE_BIN=$CARGO_TARGET_DIR/debug/crundi-gpui`
(`AREAS.md`, "Building and testing side by side"). Linux needs `libxcb1-dev`,
`libxkbcommon-x11-dev` and `libasound2-dev` to link, and Mesa's
`mesa-vulkan-drivers` (lavapipe) to draw without a GPU. CEF's files sit next
to the binary (`tools/build.sh` stages them; `native-gpui/scripts/fetch-cef.mjs`
fetches CEF once, shared with the Slint client).

CI: `.github/workflows/native-gpui.yml` builds the workspace on Linux,
Windows and macOS, runs its unit tests, and starts the client on each
(`tools/smoke.mjs`).

## The GPUI test's findings (16 Nov 2026, before the owner chose GPUI)

| | GPUI test | Slint client |
|---|---|---|
| Largest single compile | 1.46 GB (gpui-component, a dependency, built once) | 6–7.7 GB (the client's own crate, every UI change) |
| The client's own crate | 0.18–0.53 GB | 6–7.7 GB |
| Rebuild after changing a screen | 3–10 s | 1–4 min, up to ~23 min in the factory run |
| First build from nothing | 269 s (about 760 crates, -j2) | – |
| Window to first frame (with CEF) | 0.66–1.5 s | – |

The scopes differ: this test is about 1,150 lines; the Slint client is all of
phases 0–2. But the build cost has a different shape: Slint compiles every
screen into one generated Rust file inside one crate, so every change
recompiles all of it, and that grows with the app. GPUI screens are ordinary
Rust and can be split into a crate per area, each compiled on its own.

**Runtime on this server is not representative.** It has no GPU, so GPUI is
drawn by Mesa's software Vulkan (llvmpipe): an idle window with a blank page
uses 17% of a core, but a page that keeps repainting (Wikipedia, about 14
frames a second) uses about 260%, of which about 200% is llvmpipe's six
threads doing the GPU's job and 37% is GPUI and CEF's frame copy. On a PC
with a graphics card that work moves to the GPU. Slint's software renderer is
CPU by design and redraws only what changed. A fair speed comparison needs a
real Windows or Linux PC.

**The CEF bridge** took about 300 lines and worked once it compiled: the
same off-screen approach as the Slint client (BGRA frames, which GPUI's images
take without conversion), pumped from GPUI's main thread every 8 ms.

**Rough edges met on the way** (each fixed here, each a sign of what working
with GPUI is like):

- Without the `x11` and `wayland` features on `gpui-pre-platform`, GPUI runs
  headless with no error: the window exists in memory and draws nothing.
- Editing gpui-component's theme colours directly does nothing: it rebuilds
  them from a theme definition, so the tokens go in as a registered theme.
- The default font name (".SystemUIFont") does not resolve on a Linux without
  desktop fonts: the fonts are bundled (DejaVu, as the Slint client).
- The crates are republished every week (0.3.12 on 9 Nov, 0.3.13 today), and
  the API moves: pin exact versions.
- `cef::*` and `gpui::*` both have `Window`, `Point` and `KeyEvent`: the CEF
  side lives in its own module.

- The kit has two icon sets: `gpui_component::IconName` (about 100) and
  `gpui_kit_assets::IconName` (all 1,830 Lucide icons, registered as the
  app's assets).

Known in this test: dragging the divider across the chat selects its text.
Not tested here: Windows and macOS (no machine; CI could build them), IME,
the GPU path for CEF frames (shared textures), and a long chat's scrolling
(the log is not virtualised yet; gpui's `list` would do it).
