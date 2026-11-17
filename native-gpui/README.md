# Crundi's native PC client on GPUI

The PC client, native, on GPUI (Zed's UI framework) with CEF for the
browser: the owner's decision of 16 Nov 2026 (`PLAN.md`). It replaces the
Slint client in `native-client/`, which stays as the reference: its
behaviour, its specs and its tests, which drive this client through the same
test driver.

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
| `webview`, `desktop` | the CEF bridge; the browser drawer and the PC extras |
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
  process of its own (the browser host, below); CEF's code stays in its own
  modules (`cef::*` and `gpui::*` clash).

## The browser host (Chromium in its own process)

Every web view (the drawer's tabs, hidden browsers the server opens, widget
panels, a chat's live frames, Cru's screen, What's new's step pages) lives
in the **browser host**: the app's own binary started with `--browser-host`
(`crates/webview`). The app never loads or starts CEF itself.

- **Process model.** `main` calls `crundi_webview::init_process()` first:
  CEF's helpers (`--type=…`) run and leave there, and the host runs there
  and never returns. The app keeps a thin client with the same API the
  crates used before (`engine`, `PageView`, `frames`); `client.rs` starts the
  host, `host/` is the host's side (CEF, its handlers, its loop).
- **The link** (`ipc.rs`): length-prefixed JSON, commands (`Cmd`: open,
  close, size, navigate, history, mouse, wheel, keys, IME text, focus,
  hidden, CDP, script, DevTools, cookies, quit) and events (`Ev`: ready,
  frame segment, paint, loading, load error and end, title, address,
  cursor, console, popup, scroll, network, CDP answers, page process gone).
  Linux and macOS: a Unix socket pair, the host's end as its stdin (moved
  off fd 0, close-on-exec). Windows: the host's stdin and stdout.
- **Frames** (`shm.rs`): one shared-memory segment per view, BGRA. The host
  copies CEF's frame in and sends `Paint`; the app's reader thread copies
  it out and answers `Ack`; a frame that comes before the `Ack` is drawn
  again after it (`invalidate`). Linux: a memfd passed over the socket
  (SCM_RIGHTS; never in /dev/shm). macOS: `shm_open`, the name unlinked by
  the app once mapped. Windows: a named file mapping. A bigger frame makes
  a new segment.
- **Lifecycle.** No host at launch. The first view starts it (the drawer
  opened, an embed scrolled into view, a widget panel shown, a What's new
  step); the page shows a quiet spinner until its first frame. With no view
  for `CRUNDI_BROWSER_IDLE_SECS` (default 60) and the drawer closed, the app
  sends `Quit` (killed after 3 s) and all its memory is returned; the next
  view starts a new one. The drawer starts closed after a relaunch, its
  tabs listed (title, address, network, device) and nothing loaded;
  opening it loads the active tab, the others load when shown. Closed for
  the grace, the drawer lets its pages go. Embeds and panels hold a view
  only while visible.
- **Dying with the app.** Linux: `PR_SET_PDEATHSIG` (SIGKILL) set before
  exec, and a process group of its own. Windows: the app's job object with
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, the host put in it before `Hello`
  (so before any helper). macOS and everywhere: the host leaves on EOF of
  its link, plus a watchdog on its parent. CEF's helpers leave with the
  host. The host's PID is in `browser-host.pid` in the config folder; at
  start the app kills a host a crashed run left (with its group) and
  clears stale CEF locks (`SingletonLock`, `-Socket`, `-Cookie`,
  `lockfile`) in the cache folder and each network's profile.
- **Crashes.** The host gone without being asked: every view shows "This
  page crashed" with Reload (`w::page_crashed`); the app stays up, and
  Reload (or a server tool using a drawer tab) starts a new host. A single
  page process ending (`Gone`) affects only its view.

**Debugging it.** The app logs `[browser] …` (started, up, stopped, went
away) and the host `[browser host] …`, both to the app's stderr. The test
driver shows `browserHost` in the desktop area's props (state, pid, views,
starts, crashes, reaped, idleSecs); `desktop:browser-host-stop` stops it,
`desktop:reload <tab>` is a tab's Reload. `ps --ppid <app pid>` shows the
host, `ps -g <host pid>` it and its helpers. `CRUNDI_BROWSER_IDLE_SECS=5`
makes the idle stop quick; `CRUNDI_NO_BROWSER=1` runs with no browser.
`crundi-gpui --browser-host` by hand waits for `Hello` on stdin (30 s).
`native-client/test/desktop.mjs` part `hostproc` (`ONLY=hostproc`) checks
all of the above.

## Build and run (this server)

```
tools/build.sh                 # the shared build folder, -j2, the lock, OOM-safe; stages CEF
tools/build.sh -p crundi-chats # one crate
tools/run-xvfb.sh              # one screenshot to /tmp/gpui.png (headless: Xvfb + lavapipe)
node tools/smoke.mjs "$CARGO_TARGET_DIR/debug/crundi-gpui" /tmp/out    # starts, answers its driver (CI's check)
tools/cpu.sh <pid> [secs]      # CPU and memory of the client and CEF's helpers
```

The phase tests run against it with `NATIVE_BIN=$CARGO_TARGET_DIR/debug/crundi-gpui`
(`AREAS.md`, "Building and testing side by side"). Linux needs `libxcb1-dev`,
`libxkbcommon-x11-dev` and `libasound2-dev` to link, and Mesa's
`mesa-vulkan-drivers` (lavapipe) to draw without a GPU. CEF's files sit next
to the binary (`tools/build.sh` stages them; `native-client/scripts/fetch-cef.mjs`
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
