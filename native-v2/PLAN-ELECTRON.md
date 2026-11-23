# Replace Electron with the native client: plan and progress

Each item of the change request and of PARITY-ELECTRON.md: done, tested, with how long each step took.
Rules: `package/CONTRACT.md`. Gap list: `PARITY-ELECTRON.md` (mark each line done and tested there).

## Steps
| Step | Where | Took | Result |
|---|---|---|---|
| Updates and lifecycle | PC farhan-RP-15 | about 1 h 45 min (build 10 min, workspace tests, e2e 2 min) | Updater, launch at startup (one copy), single instance, crundi.log, system-wide lock keys; test/electron.mjs updates/lifecycle/install 49/49 |
| Prep | PC farhan-RP-15 | about 25 min (release build 701 s, test build 190 s, references 184 s, Docker image 122 s) | Warm builds, Electron v1.21.7 references in ~/.cache/crundi-electron-ref, image crundi-upgrade-test |
| Packaging | PC farhan-RP-15 | about 75 min; `package/check.sh both` 310 s warm (first run 589 s, build image 254 s) | Linux green on the PC (package/check-pc.log). Windows CI job written, NSIS compiles (makensis 3.09 in Docker) |
| Packaging (last loop) | PC farhan-RP-15 + CI | about 3 h (CI runs about 50 min each, Windows) | check.sh both green again (330 s). CI: feed.mjs ran its command line when imported (fixed); Electron's Windows folders are `crundi`/`crundi-client` and its Run values `electron.app.Crundi`/`electron.app.Crundi Client` (installer, test, lifecycle.rs fixed); the owner's two Mindmap items |

## Change request
| Item | Stream | Done | Tested |
|---|---|---|---|
| Local mode (start, watch, restart, stop the bundled server) | local | | |
| Local or remote with the first-run wizard | local | | |
| PC worker through worker/ | local | | |
| Electron install moved over (data, config, start at login, shortcuts) | local, updates, pack | | |
| Updates for the combined install | updates | yes | yes (test/electron.mjs updates, install; cargo tests) |
| Windows .exe and Linux .deb replacing Electron's | pack | yes | .deb: Docker upgrade test, both editions; .exe: CI windows-latest upgrade test, both editions |
| Docker (Linux) upgrade test from the latest Electron release | pack | yes | yes, v1.21.7 → 1.21.8-dev.8, all-in-one and client (check-pc.log) |
| windows-latest upgrade test from the latest Electron setup | pack | yes (upgrade-test/windows.ps1, workflow job `windows`) | yes, v1.21.7 → 1.21.8-dev.8, all-in-one and client (CI run on branch …-aca5c8-pack2) |
| Quitting leaves no process behind | local, tester | | |
| Every PARITY-ELECTRON.md line done and tested | all | | |
| Owner: the first browser start froze the window ("not responding") | gaps | yes: webview client.rs starts the host on a thread of its own (commands wait in the link); the drawer tab shows its page loading from the start (Reload arrow turning, Electron's .brd-loading) | yes (test/electron.mjs browserstart: host start slowed to 3 s, real Tab presses answered within 200 ms, the tab loading meanwhile) |
| Owner: a long chat title pushed the pane head's badge and buttons off the pane | gaps | yes: workbench head.rs (the shared pane head, every pane): the title takes only the room left, elided (the web's .term-head-title); the badge and the tools keep their size | yes (test/workbench.mjs "a very long title in a narrow pane": the drawn title, badge and buttons against the pane; fails without the fix) |
| Owner: touchpad scrolling stopped dead when the fingers lifted (no momentum) | gaps | yes: vendor/gpui patch (f), the one scroll dispatch every list and pane uses: a precise (pixel) scroll glides on at its last ~100 ms's speed with exponential decay, stops at the edge; new input or a press stops it; Linux only (Windows and macOS send their own momentum; X11 sends lines, unchanged); the wheel's line scroll never glides | yes (test/workbench.mjs "Kinetic scrolling": synthetic precise steps keep moving after the last one then stop, the edge, a press, a line scroll does not glide) |
| Findings 2: the What's new frame never loaded its page (operations/extras.mjs:104) | gaps | yes: not the off-thread start (frames made before the host is up wait in the link and load; a host restarted while stopping too). The cause: under a suite's private session bus (dbus-run-session) the first page that needs cookies waited 25 s and more on Chromium's keyring (the portal's Secret.RetrieveSecret, no secret service on that bus). Under the test driver the host now uses password-store=basic (browser-host main.rs), and every suite has a CEF profile of its own (test/lib.mjs CRUNDI_CEF_CACHE in its config folder): never the person's keyring or profile | yes (operations extras under dbus-run-session failed at step 1, passes; test/electron.mjs browserstart: What's new opened with the host off and while it stops, its page loads) |
| Findings 6: PARITY lines 40 (tray), 89 (local IPC path), 112 (worker) | gaps | yes: marked done with their tests (40 and 112 built by the local stream; 89 one path, the native-host socket, CRUNDI_NATIVE_RUN) | yes (test/desktop/tray.mjs, test/electron/local.mjs, a new check that the local server gets CRUNDI_NATIVE_RUN and no ELECTRON_RUN) |
| UI review approves | reviewer | | |
| Owner has tried it | owner | | |
| Owner: a Mindmap idea dropped on a chat's box left its drag picture floating there | pack | yes: the map takes the release outside it in the capture phase (the chat's drop stops the bubble) (mindmap/draw.rs) | yes: test/planning/drops.mjs drags an idea onto a chat's box with the pointer, checks the reference goes in and no idea is left in hand; fails without the fix |
| Owner: a long idea hovered opened up see-through, its neighbour's words over it | pack | yes: the opened idea is a paint layer of its own over the map's (in one layer every card is under every word) (mindmap/draw.rs) | yes: test/planning/mindmap-look.mjs pictures the opened idea over its neighbour, changes the neighbour's words, the pixels there stay the same; fails without the fix |

## Packaging sizes (Linux, against Electron v1.21.7)
| Package | Native | Electron |
|---|---|---|
| Crundi .deb | 207 MB (766 MB installed) | 352 MB (1153 MB installed) |
| Crundi Client .deb | 136 MB (429 MB installed) | 86 MB |
| Crundi Setup .exe (Windows) | 262.0 MB | 209.4 MB |
| Crundi Client Setup .exe (Windows) | 149.9 MB | 105.2 MB |
