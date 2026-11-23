# Owner's trial on farhan-RP-15 (crundi-client .deb, 1.21.8-dev.8): final fix round

Owner's words are the reference. Each owner adds the test named and marks PLAN-ELECTRON.md. Blocking = must pass before the run finishes.

| # | Finding | Owner | Done looks like |
|---|---|---|---|
| T1 | Touchpad glide happens after scroll-and-hold; a glide must follow only a real flick. A lift-off always has micro-movement, so lifting is not a flick | gaps | Glide starts only when: velocity over the last ~50-100 ms before lift-off is above a threshold (near Chromium/libinput, ~100-150 px/s of content); the last motion is within ~50 ms of lift-off (any rest zeroes it); the recent deltas agree in direction (jitter cancels). Velocity from the recent window, not the gesture peak. No lift signal (some X11) = no glide. Tests replaying real-shaped gestures: flick glides; scroll-hold-lift, slow drag, 1-3 px slip on lift, wobble: no glide |
| T2 | The packaged client lacks its bundled fonts: "Failed to load bundled font fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf / fonts/lilex/Lilex-Regular.ttf: could not find asset" (falls back to Ubuntu) | pack | The fonts ship with the package (stage.mjs and both OSes) and load. The upgrade tests fail on any "could not find asset" in the client log |
| T3 | The .deb version is 1.21.8-dev.8 (Debian revision "dev.8") where Electron's was 1.21.8~dev.8. dpkg then sorts the real 1.21.8 below the dev build, so a release won't upgrade over it | pack | Debian version maps -dev.N/-beta.N to ~dev.N as Electron's electron-builder did. linux.sh checks `dpkg --compare-versions` (1.21.8 > 1.21.8~dev.8) and the installed version string |
| T4 | Hovering the suggested-reply chip ("Suggested, tap to send") makes it see-through over the chat text | local | The chip has an opaque theme surface in every state. Check the shared widgets for other hover colours that are translucent with no opaque surface under them. Screenshot test of the chip hovered over chat text |
| T5 (blocking) | Kid mode cannot be ended from the screen. The owner had to end it from the phone. The lock icon top right is near-invisible and/or does not open the PIN | local | As Electron: a visible but calm lock control opens the PIN entry on the kid screen; the right PIN ends kid mode and the window returns; a wrong PIN shows the error, five wrong tries lock for a minute. Test with real clicks and keys: open, wrong PIN, right PIN, kid mode off |
| T6 (blocking) | Wayland app_id is "crundi" for every edition, so the client window doesn't group under the pinned crundi-client.desktop and gets the generic gear icon | updates | Wayland app_id = the edition's desktop file id (crundi / crundi-client); X11 WM_CLASS = its StartupWMClass ("Crundi" / "Crundi Client"). linux.sh checks the running window's app_id/class against the installed .desktop |
| T7 (blocking) | The workbench never scrolls sideways when its panes overflow (with or without the drawer): each panel takes horizontal wheel/touchpad input even when it cannot scroll horizontally. It only works over the pane headers | gaps | A panel claims horizontal scroll only if it can use it (a horizontally scrollable area, or a canvas like the mindmap or a widget); otherwise it goes to the workbench strip. The strip has a visible horizontal scrollbar, takes sideways touchpad and Shift+wheel, and scrolls a new or focused pane into view; opening the drawer keeps the focused pane visible. Test: overflow without and with the drawer; scroll with real events over a pane body; the last pane comes fully into view and takes typing |
| T8 (blocking) | Crash on horizontal scroll: "thread 'main' panicked at vendor/gpui/src/scene.rs:197:41" (Scene::extent slicing paint_operations with a stale range, the cached-views patch b) | gaps | Cached ranges are invalidated when scroll/offset changes, or checked before slicing, never panicking. PATCH.md updated. Test: scroll a cached pane horizontally (and vertically) through its whole range repeatedly with no panic |
| T9 (blocking) | Desktop notifications never show on Linux: "[notify] could not show:" followed by gdbus's Usage text (a malformed gdbus call) | updates | Notifications show through org.freedesktop.Notifications with the right arguments. Clicking one opens its chat (as Electron). Test against a real notification daemon on the session bus, checking Notify's arguments and an ActionInvoked click |
| T10 (blocking) | Closing a chat that holds an embed (show_embed) leaves its page running; its sound keeps playing | local | Closing a chat pane, or switching project, closes every embed page in it; an embed out of view or in a hidden pane is muted/paused, as in Electron. Test: an embed playing audio, close the chat, its page is gone from the browser host (by PID or the host's page list) and no audio stream remains |

F9 (DevTools closed, then browser tools on that tab) stays the owner's check on a real desktop. The owner confirmed the drawer cursor and the browser tools work through the client.

---

# Tester: fix round 5 verified on 133b76e8 (farhan-RP-15, Xvfb, fresh test servers on 4830)

| Check | Result |
|---|---|
| cargo test --workspace | 396 passed, 0 failed |
| electron.mjs (all parts, with the new window part) | 133/0 (local 84/0) |
| chat-tail (stub) / operations full order / desktop / shell / workbench | 28/0, 432/0, 395/0, 34/0, 73/0 |
| check.sh both (Docker upgrade from v1.21.7, both editions) | passed, 915 s (one earlier try hung in the container's apt-get update on a slow Ubuntu mirror; stopped and run again) |
| Local mode from the package layout (stage-full of this head) | 91/0: chat streams, the worker as kind native, Electron's device id kept, Quit leaves no process (by PID) |
| Package workflow | No run on this head: GitHub refuses the push (the token has no `workflow` scope and d5c6d60e changes native-v2-package.yml). Last green: 45146419021 (36d0e720); 45153765294 (e3f0626b, without the Kid lock step) windows success, linux in progress |

| Item | Verified | How |
|---|---|---|
| F1 (F9) | yes, 20 of 23 lines | Lines 36-43 by electron/window.mjs (frameless, Minimize/Maximize/× by real clicks, tray icon, tooltip, click), 67 cargo test, 98 gaps.mjs clear per site, 111 local.mjs device id; the rest name suites that passed above. Not yet: 66 and 68 (their Windows tests are in a workflow step GitHub does not have, so they never ran) and 94 (PARTIAL, DevTools, F4) |
| F2 (N4) | yes | operations.mjs in full order 432/0 |
| F3 (N5) | yes | chat-tail.mjs 28/0; screens/electron/local-chat-streaming.png at the same moment as before: "twenty-s" right under "twenty-six", no blank row |

New:
- The Kid lock step in native-v2-package.yml (committed in d5c6d60e) is not on GitHub: pushing needs a token with `workflow` scope (the owner). Until it runs green on windows-latest, PARITY 66 and 68 are not tested, and no branch holding this head can be pushed.
- PARITY 94 is PARTIAL (F4): the CR's first Done-when needs it done, or the owner's word that DevTools can be left out.
- PARITY 56 and 58 say DONE but name no test (test/electron/updates.mjs, local.mjs "first-run consent…" cover them).

---

# Tester: fix round 4 verified on 7fd41030 (farhan-RP-15, Xvfb, fresh test servers on 4830)

## The full pass

| Check | Result |
|---|---|
| cargo test --workspace (tools/build.sh test) | 394 passed, 0 failed |
| check.sh both (release build, both .debs, Docker upgrade from Electron v1.21.7, both editions) | passed, 477 s, 60 ok (package/check-pc.log) |
| slice / toast / shell / workbench / workspace | 41/0, 6/0, 34/0, 73/0, 192/0 |
| chat-composer / chat-log / chat-mini / chat-tail (stub server) | 83/0, 66/0, 58/0, 21/0 |
| operations / desktop / planning (full order) / log-element | 429/2 (N4: flaky, ONLY=docker 62/0 twice), 395/0, 521/0, 18/0 |
| electron.mjs (all parts) | 102/0 (local 77/0); after this pass's additions: local 84/0 (debug), browserstart 21/0 |
| Local mode from the package layout (PACKAGE_DIR=~/.cache/crundi-pack/stage-full, electron/local.mjs) | 91/0: the release client starts the bundled server with node/bin/node, the wizard, the setup page, sign-in, a chat streams (stub Claude), the worker (packaged crundi-worker) joins as kind native, Stop/Start, a killed server restarted, Restart, restart-server, Quit leaves no process (client and server trees by PID) |
| Package workflow, run 45138039085 on 7fd41030 | success, both jobs; Windows: "the client started the bundled server … node\\node.exe … server\\src\\index.js", "…and it answers on :60135 within 120 s", version 1.21.8-dev.8, Up to date |

## Round 4 items, each against its test

| # | Verified | How |
|---|---|---|
| F1 | yes | check.sh: `/opt/Crundi/version is 1.21.8-dev.8`, "the client reports version 1.21.8-dev.8", "Up to date", both editions; CI the same on Windows |
| F2 | yes | operations.mjs extras in the full run (What's new loads); electron browserstart "…and the page loads once the host connects", "…a fresh host, and the page loads" |
| F3 | yes | desktop.mjs tray 0 failed; tray-menu-{stopped,running,error}.json from local.mjs (Start/Stop/status as app/main.js) |
| F4 | yes | planning.mjs in full order 521/0 |
| F5 | yes | test/lib.mjs sets XDG_CONFIG_HOME/APPDATA per suite; shell.mjs 34/0 on this PC, which has a real Electron profile |
| F6 | partly | section 1 and lines 40, 89, 112 are marked done with tests. 23 other lines are still marked only PRESENT: see F9 |
| F7 | yes | CI 45138039085 windows: the check is in place and passes |
| F8 | yes | local.mjs "normal/narrow: the options' words and the address box end inside the card's padding", "…wrap (two lines)", "the card inside the window"; screens/electron/server-screen-{local,remote}{,-narrow}.png |
| N1 | yes | local.mjs "the next start: … the setup page, not the app"; screens/electron/settings-before-signin-setup.png |
| N2 | yes | local.mjs Next twice, Escape, keys back in the box, again: passed in 4 runs |
| N3 | yes | local.mjs with the packaged crundi-worker: kind "native", `--kind native` |

## New

| # | Finding | Where | Blocking |
|---|---|---|---|
| F9 | 23 PARITY lines are marked only "PRESENT" with a manual "How to test", not done and tested naming a test (the change request's first Done-when): lines 36, 37, 39, 43, 66, 67, 68, 69, 78, 79, 80, 81, 87, 90, 92, 94, 96, 97, 98, 99, 100, 102, 111. Most are covered by suites that pass (desktop.mjs tray, lock, kid, wake, drawer, passwords, browsers, unload, hostproc; electron.mjs gaps): name them. The Windows-only ones (66, 68) need a CI check or n/a with the reason | native-v2/PARITY-ELECTRON.md | yes |
| N4 | Flaky: the Docker shell lost the start of a command sent right after its prompt showed ("/ # -$((6*7))-…", "echo shell" missing), once in the full run; ONLY=docker passed twice | test/operations/docker.mjs:110 (crates/operations docker shell) | no |
| N5 | While a reply streams, a blank row shows before the item still being written ("twenty-six", blank, "twenty-s") | screens/electron/local-chat-streaming.png (crates/chat log, streamed markdown list) | no (UI reviewer to judge against the web) |

Tests added in this pass: electron/local.mjs takes the screens/electron pictures and tray JSON, runs the packaged layout with PACKAGE_DIR (bundled Node and server, stub Claude, a chat that streams, the packaged worker), and its Quit check covers the client's tree as well as the server's; electron/browserstart.mjs opens the drawer first on the run's fresh CEF profile (a cold start) and pictures it loading and loaded.

The PC has no xwd or ImageMagick (the driver's snapshots and lib.mjs `diff` need them): this pass used x11-apps and GraphicsMagick unpacked into /tmp/qa-tools (apt-get download + dpkg-deb -x, nothing installed) and a PIL stand-in for `compare -metric AE`.

---

# Tester findings on 9401e817 (fix round 4)

Each stream fixes its own items, adds or updates the test that shows it, marks it in PLAN-ELECTRON.md, and hands over with its check green. The tester verifies every item afterwards.

## Blocking

| # | Finding | Owner | What done looks like |
|---|---|---|---|
| 1 | A packaged client reports v0.1.0: CRUNDI_VERSION is never set in the workflow or in check.sh, so the updater offers an update on every launch | pack | Version comes from package.json in check.sh, stage.mjs and the workflow (both OSes), and is checked in the upgrade tests (the installed client reports the package version and offers no update) |
| 2 | The What's new frame never loads its page (operations/extras.mjs:104). Likely cause: the off-thread browser start in webview client.rs (16185fac) | gaps | extras.mjs passes. Frames created before the host is up are queued and loaded once it connects |
| 3 | desktop/tray.mjs:15 still expects the old tray menu without Start/Stop | local | tray.mjs matches Electron's menu with Start/Stop and status, all-in-one and client |
| 4 | Planning: the two mindmap-press checks fail only in the full run order. mindmap-look or drops leaves the map in the wrong state | pack (made the mindmap fixes) | The planning suite passes in full order. Each new test restores the map (selection, hover, drag state) when it ends |
| 5 | Suites other than electron.mjs set no XDG_CONFIG_HOME/config dir of their own, so the test client imports the owner's real Electron settings (shell.mjs fails 30+ checks) | updates | test/lib.mjs gives every suite a temp XDG_CONFIG_HOME (and APPDATA on Windows), as electron.mjs does. The one-time Electron import never reads the real profile in tests. shell.mjs passes |
| 6 | PARITY-ELECTRON.md section 1 and lines 40, 89 and 112 are not marked done or n/a | local (section 1), gaps (40, 89, 112) | Every line is marked done and tested (naming the test) or n/a (with the reason) |
| 7 | The Windows upgrade test only logs "bundled server started" and doesn't check it. It was False in runs 45070403494 and 45073936777 | local (the cause), pack (make the test check it) | windows.ps1 fails unless the all-in-one's bundled server answers within its timeout. CI is green with the check in place. Local finds the cause on Windows (node path, PATH, DATA_DIR, quoting) |
| 8 | Owner: on the Server screen (local or remote), the description under "Connect to a server" runs past the card's right edge instead of wrapping, and the URL field is aligned with the overflow (screens/electron/server-screen-local.png) | local | Option descriptions wrap within the card. The field stays inside the card's padding. A test at the normal and a narrow width checks that nothing extends past the card |

## Nits (do them in the same round)

| # | Finding | Owner |
|---|---|---|
| N1 | A bundled server with no sign-in method shows the app with errors where Electron shows the setup page (screens/electron/settings-before-signin-setup.png) | local |
| N2 | Escape on the wizard's step-3 alert was flaky once | local |
| N3 | The worker registers with kind "electron"; it should be "native" | local |
