# Native PC client on GPUI: plan

Scope: the **PC client only**. It replaces the Electron client (`app/`) with a
native one on Windows, macOS and Linux. The Android app is out of scope, and
so is anything for invitees (collaborators), who use the web UI. Electron is
not fixed or changed on the way.

Decision (16 Nov 2026, the owner): the native client is built on **GPUI**
(Zed's UI framework, `gpui-pre 0.3.12` with `gpui-component 0.10.0`), not
Slint. CEF stays for the browser. Everything is compiled Rust: no screens
loaded at run time. The Slint client in `native-client/` (phases 0 to 2 done
there) is the reference: its behaviour, its specs (`native-client/specs/`)
and its tests (`native-client/test/`), which run against this client through
the same test driver.

What it must do is what the PC client does today: the Electron shell
(`app/`), plus the Crundi interface it shows (`src/webapp-html.js`). Those are
the specification. Every screen keeps the PC client's layout and user
experience, drawn with one constant theme of tokens, every control from the
widgets crate (`DESIGN.md`). Who builds what: `AREAS.md`.

## Phases

- [x] **F. Foundation** (the Lead, 16 Nov 2026): the Cargo workspace (core,
  theme, widgets, webview, shell, workbench, chat-model, chats,
  chat-composer, chat-log, workspace, planning, operations, desktop, app),
  versions pinned; the Slint client's non-UI Rust ported into `core`; the
  tokens as a registered gpui-component theme with the bundled fonts and
  Lucide icons; every control in `widgets`; the CEF bridge in `webview`;
  the shell (frameless window, top bar, sidebar, project bar, tab host with
  per-part registration, the drawer's place and divider, sign-in and
  pairing); the pane host; a chat pane (log and composer); the test driver
  on the Slint client's protocol (`test/phase1.mjs`: sign-in, the project
  list, a new chat with Claude's reply, a tool run, sign-out and pairing
  pass); `tools/build.sh`, `tools/smoke.mjs`; CI on three systems
  (`.github/workflows/native-gpui.yml`).

0. **Shell and browser panels** (Desktop; Lead for the window): the drawer
   on the phase 0 panels, the native host over `/ws` (`test/drive.mjs`),
   builds for Linux, Windows and macOS in CI.
1. **Foundation** (Lead): connecting to a server (the HTTP API and `/ws`),
   pairing and sign-in as the PC client does (server step, password and
   code, Google / GitHub / Telegram through the browser), saved settings,
   the window (projects and navigation). Test: `test/phase1.mjs` in full.
   - [x] sign-in through the browser (`sign-in-browser`, app-login)
   - [x] a restart signs in from the saved login
   - [x] the top bar (17 Nov): weekly and 5-hour usage bars (used against
     time passed, over-pace colour, countdowns, "updated … ago", a click
     opens Usage; owner only), Cru's mark (the PC client's: bars and "cru",
     breathing while live, unread dot, hover / click / Ctrl+Space push to
     talk), Telegram's `send` icon and dot, the
     connection badge, the window buttons; the sidebar's dots, folding,
     Add project and Inbox
2. **Chats** (Workspace: 2.1-2.4; Planning: 2.5-2.12b, 2.24; Operations:
   2.13-2.23), each item as `native-client/PLAN.md` describes it, with its
   spec in `native-client/specs/`. Test: `test/phase2.mjs`.
   - [x] 2.1 Pane header, wide · [x] 2.2 narrow · [x] 2.3 moving panes · [x] 2.4 live state (Workspace)
   - [x] 2.5 Composer and Stop · [x] 2.6 permission-mode picker · [x] 2.7 attachments · [x] 2.8 drafts ·
     [x] 2.9 queued messages · [x] 2.10 scheduled messages · [x] 2.11 slash menu · [x] 2.12 chat secrets dialog ·
     [x] 2.12b secret requests · [x] 2.24 voice input (Planning)
   - [x] 2.13 questions · [x] 2.14 tool cards · [x] 2.15 thinking · [x] 2.16 the rest of the log (2.16m media,
     2.16f live frames) · [x] 2.17 suggestions and recaps · [x] 2.18 agent dock · [x] 2.19 goal and plan ·
     [x] 2.20 cards · [x] 2.21 side questions · [x] 2.22 resume picker · [x] 2.23 parked panes (Operations)
   - The gate: `test/phase1.mjs` and `test/phase2.mjs` pass against this
     client, and the UI reviewer passes the screens. Then phases 3 to 6.
3. [x] **Workbench** (Workspace; Desktop for the browser), test `test/workspace.mjs`:
   the workbench's panels (Files, Git, Kanban, Notes, Mindmap, Media,
   Factory, Panel), Claude terminal panels (their input inside each panel:
   key tools, attach, Send; no shared bar), Git, Files, Media; the browser
   drawer in full and the native host's full protocol (Desktop, `test/desktop.mjs`).
4. [x] **Planning** (Planning), test `test/planning.mjs`: Kanban (and the card
   dialog), Mind map, Notes, Schedules, webhooks, Factories (the editor, the
   live run graph, the factory panel).
5. [x] **Operations** (Operations), test `test/operations.mjs`: Services,
   forwards, Docker; Secrets, Skills, Usage; Settings, sign-in, backup; Add
   project; Homepool; Info; Panels (widgets); What's new (17 Nov: each step's
   picture is its own page from the server, live in the frame; a native-only
   feature adds its step on the server, README "What's new"); the approvals button.
6. [x] **PC-client extras from `app/`** (Desktop), test `test/desktop.mjs`:
   notifications and the tray, the Inbox, the chat bubble, saved passwords,
   the homepool node, Cru on the PC (the voice screen, the live screen, kid
   mode and the wake word "Hey Cru").
7. **Ship** (not this run): installers and auto-update, signing, the release
   workflow, moving PC users off Electron.

## The browser host (17 Nov 2026, the owner's card)

Chromium runs in a process of its own, started lazily, cleaned up on any
crash or close, with no tabs loaded on relaunch (README, "The browser
host"). Every web view goes through it: the drawer and its tabs, hidden
browsers, widget panels, a chat's live frames, Cru's screen, What's new.
Tests: `test/desktop.mjs` part `hostproc` (no host at launch; it starts with
the drawer; tabs listed but not loaded after a relaunch, the active one
loads on open; the idle stop; the host killed: "This page crashed", the app
up, Reload; the app killed: no host, helper or segment left; a stale host
reaped at start), and the unit tests of `ipc`, `shm`, `proc`.

- [x] the host process, its link, frames in shared memory (Linux, macOS,
  Windows), lazy start, idle stop, kill-on-close per OS, the PID file and
  reaping, crash state and Reload
- [x] stage 2 (17 Nov 2026): libcef only in the host (its own executable,
  `crundi-browser-host`; the app maps no CEF), two CPU copies per frame
  (dirty rects only into the segment, unchanged frames not sent, the
  segment straight into the GPUI image), no frames for hidden views (a tab
  behind another, the drawer closed, hidden browsers, embeds off screen),
  the GPU path's host side behind `CRUNDI_BROWSER_GPU` (README, "GPU
  textures"). Tests: `hostproc` adds the app's maps without libcef (idle
  and with the host up), an animating tab's frames stopping behind
  another tab and with the drawer closed and coming back when shown, the
  host's counters; `gpu.rs`'s unit tests.

- [x] the one rule of every web view (17 Nov 2026, the owner's): not
  visible, hidden at once with no frames; not visible for
  `CRUNDI_BROWSER_UNLOAD_SECS` (30 s), freed; visible again, loaded again
  with the loading state; chat embeds no longer kept within 4500 px (CAP
  stays the ceiling); drawer tabs the same (listed, the active one loads on
  open, the others when clicked); no views: the host stops after
  `CRUNDI_BROWSER_IDLE_SECS`. Tests: `desktop.mjs` part `unload` (an embed
  scrolled out and back, a background drawer tab, the drawer closed, the
  host's idle stop after each), phase2 2.16f's scroll-away check now "hidden
  at once, released after the unload grace" (3 s in phase2) where it was
  "released beyond one screen of the log" (the old keep-alive).

Memory (17 Nov 2026; `tools/pss.mjs`, release build; this server: no GPU,
Xvfb, lavapipe; PSS summed over the app's tree, which holds the host's;
signed in on a seeded test server, project demo; each step settled 40 s:
idle, the drawer on google.com, a tab more on wikipedia.org, a tab more on
github.com). Electron 41's PC client (app/ run from a copy, remote mode on
the same server, the drawer driven over its DevTools port), the same steps.

| | GPUI run 1 | GPUI run 2 | GPUI, tabs kept (unload grace 3600 s) | Electron run 1 | Electron runs 2-3 |
|---|---|---|---|---|---|
| idle | 202 MB | 201 MB | 202 MB | 432 MB | 535-539 MB |
| 1 tab (google.com) | 594 MB | 594 MB | 590 MB | 483 MB | 584-586 MB |
| 2 tabs (+ wikipedia.org) | 541 MB | 542 MB | 636 MB | 513 MB | 608-612 MB |
| 3 tabs (+ github.com) | 731 MB | 687 MB | 782 MB | 688 MB | 777-787 MB |

Per process, GPUI with one tab (run 2): the app 218 MB (no libcef; 201 MB
idle), the host 118, the page 128, the network service 41, the other
helpers 89. Electron with one tab (run 3): main 124, the web UI's renderer
239, the page 80, a helper 83, the network service 32, two zygotes 26.
Electron's web UI renderer was 166 MB in run 1 and 262-267 MB in runs 2-3
(the same steps; run 1 had the web UI signed in on its first load), so its
idle reading moves by about 100 MB. With the unload rule (the default),
GPUI's tabs behind the active one are freed after 30 s: two and three tabs
cost what the active one costs. Kept, each further tab adds about
45-145 MB (its renderer, and its last frame in the app). The browser host's
own process (about 118 MB) is the fixed price of running Chromium apart.

Gaps:
- **Windows and macOS are compile-only** (CI, and the platform modules
  cross-checked here): the job object, named mappings, `shm_open`, the
  macOS host's NSApp (an accessory app), the host as its own executable in
  the bundle (helpers copied from it, `browser_subprocess_path`) and the
  GPU path have not run on those systems. `crundi-browser-link` (where the
  D3D11 and IOSurface code is) is checked here for both targets; the host's
  own calls into it only on CI.
- **GPU textures are not drawn as textures**: GPUI 0.3.12 has no way to
  draw an outside texture (README, "GPU textures"); the GPU path reads
  them back into shared memory. Off by default.
- **GPUI's images**: one new image (and atlas tile) per frame, uploaded
  whole: no buffer reuse, no partial update without a fork.
- **IME**: the protocol carries committed text (`Ime`), but the views have
  no input-method handler yet (as before the host).
- **DevTools** opens as a window of the host process.
- A hidden browser's in-memory profile lives until the host stops.
- A hidden view's page runs as a background tab (timers throttled,
  `visibilityState` hidden), as Electron's hidden windows and tabs do.
- macOS: a frame segment's name can outlive both processes only if both
  die between the host making it and the app opening it.
- If the host dies before CEF is up three times in a row, the browser is
  off until the app restarts (as when CEF did not start).

## Deliberate differences decided by the owner

- **The terminal input is inside each terminal panel** (15 Nov 2026): no
  shared bar under the workbench. Each terminal panel has, at its bottom, as
  a chat panel has its composer: the text box (Ctrl+Enter or Send sends to
  that terminal), the keyboard-tools toggle with its key grid, attach
  (uploads to `crundi_attachments`, with its progress strip) and Send.
- **Nothing loads by itself** (17 Nov 2026): the drawer starts closed after
  a relaunch (its tabs listed, not loaded), and any web view not visible
  for the unload grace (30 s) lets its page go (it loads again when shown),
  so the host can stop. The PC client restored the drawer and kept pages.
- **The browser is a drawer** beside the whole project area, below the top
  bar, with a draggable divider; hidden, the project bar's Browser button
  returns with a dot.
- The Slint client's own deliberate differences (`native-client/PLAN.md`,
  Known gaps) carry over where they are behaviour, not Slint workarounds.

## Known gaps

- **Snapshots** come from `xwd` of the window: X11 only (Xvfb in tests). On
  Windows and macOS the driver's `snapshot` says so; CI's pictures are Linux's.
- **No compositor** (Xvfb): GPUI falls back to server-side decorations; the
  client still draws its own top bar and window buttons.
- **CPU on this server** is not representative: no GPU, so wgpu draws with
  Mesa's llvmpipe (lavapipe).
- **macOS** loads CEF's framework and makes NSApp CEF's (`browser-host/src/mac.rs`,
  from the Slint client), not yet tried by a person on a Mac; CI's macOS job builds it, runs the
  unit tests and starts it (green since 17 Nov, F83).
- **Cru on the PC** (`crates/desktop/src/cru*.rs`, `wake/`): the voice
  screen, the live screen (blocks native; pages and videos in frames of their
  own, with the message bridge; the full view), kid mode (the kid screen, the
  PIN pad, the break, the tray's Kid mode) and "Hey Cru" (sherpa-onnx's
  keyword spotter compiled in, static; the model downloaded once and
  hash-checked; enroll/voiceprint/owner ported; the controller's conditions;
  training in Settings › Voice assistant › On this device). Gaps:
  - a video on the live screen plays, but the voice is not gated while it
    plays (createVideoGate: the YouTube player's state messages);
  - kid mode does not lock the window (kiosk) as the Electron app's
    kidLock does, and the kid cursor and pointer are the system's;
  - kid mode opens on the device that started it (the tray here); the web's
    "kid mode is on elsewhere" card is not shown;
  - the wake word's power and lock readings: Linux (sysfs, logind) and
    macOS (pmset; no lock reading); Windows assumes AC and unlocked;
  - the voice check uses sherpa-onnx's own fbank on the same 3D-Speaker
    model, so a profile trained by the Electron client is not reused (it
    lives in that app's folder anyway): train once on the native client.
  - Cru's screen is the PC client's (17 Nov, the owner): the same parts and
    states as crundi-voice.js (cru_ui.rs, cru_field.rs, w::liquid_mic). GPUI
    paints no radial or conic gradients and no additive blending: the glows
    are stacked discs, the contours' sweep is drawn in twelve arcs per line,
    and the mic's three layers blend normally (a little darker than the
    web's "lighter"); the head is the LOGO in two tints (its body the accent,
    not its gradient). With no output analyser, Cru's speaking level is the
    web's browser-voice bump, not the streamed voice's loudness.
- **What's new on a server older than the native client** gives no step
  `page`: the frame shows the step's title (the web inlines the fragment;
  the client draws pages, not HTML). No swipe between steps (no touch).
  Its kickers' wide letter-spacing is a hair space between letters (GPUI
  has no letter-spacing).
- **Buttons with a label** use gpui-component's Button with the theme's
  sizes (`widgets/src/control.rs`); its own hover tooltip for `.tooltip()`
  on such a Button is gpui-component's managed style, not `w::tooltip`.
- **Switches and checkboxes** keep gpui-component's drawing, sized from
  the tokens.
- **Toasts** are gpui-component's notifications in one slot: a new toast
  replaces the last (the web stacks a few).
- **Editor highlighting** is syntect's (gpui-component's lighter
  highlighter), not tree-sitter: no code folding or outline.
