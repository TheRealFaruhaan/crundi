# Replace Electron with the native client: plan and progress

Each item of the change request and of PARITY-ELECTRON.md: done, tested, with how long each step took.
Rules: `package/CONTRACT.md`. Gap list: `PARITY-ELECTRON.md` (mark each line done and tested there).

## Steps
| Step | Where | Took | Result |
|---|---|---|---|
| Updates and lifecycle | PC farhan-RP-15 | about 1 h 45 min (build 10 min, workspace tests, e2e 2 min) | Updater, launch at startup (one copy), single instance, crundi.log, system-wide lock keys; test/electron.mjs updates/lifecycle/install 49/49 |
| Prep | PC farhan-RP-15 | about 25 min (release build 701 s, test build 190 s, references 184 s, Docker image 122 s) | Warm builds, Electron v1.21.7 references in ~/.cache/crundi-electron-ref, image crundi-upgrade-test |
| Packaging | PC farhan-RP-15 | about 75 min; `package/check.sh both` 310 s warm (first run 589 s, build image 254 s) | Linux green on the PC (package/check-pc.log). Windows CI job written, NSIS compiles (makensis 3.09 in Docker), not yet run: this PC's GitHub token has no `workflow` scope |

## Change request
| Item | Stream | Done | Tested |
|---|---|---|---|
| Local mode (start, watch, restart, stop the bundled server) | local | | |
| Local or remote with the first-run wizard | local | | |
| PC worker through worker/ | local | | |
| Electron install moved over (data, config, start at login, shortcuts) | local, updates, pack | | |
| Updates for the combined install | updates | yes | yes (test/electron.mjs updates, install; cargo tests) |
| Windows .exe and Linux .deb replacing Electron's | pack | yes (.deb built; .exe script compiles) | .deb: Docker upgrade test, both editions; .exe: waits for the CI run |
| Docker (Linux) upgrade test from the latest Electron release | pack | yes | yes, v1.21.7 → 1.21.8-dev.8, all-in-one and client (check-pc.log) |
| windows-latest upgrade test from the latest Electron setup | pack | yes (upgrade-test/windows.ps1, workflow job `windows`) | not run: workflow not pushable from the PC |
| Quitting leaves no process behind | local, tester | | |
| Every PARITY-ELECTRON.md line done and tested | all | | |
| Owner: the first browser start froze the window ("not responding") | gaps | yes: webview client.rs starts the host on a thread of its own (commands wait in the link); the drawer tab shows its page loading from the start (Reload arrow turning, Electron's .brd-loading) | yes (test/electron.mjs browserstart: host start slowed to 3 s, real Tab presses answered within 200 ms, the tab loading meanwhile) |
| Owner: a long chat title pushed the pane head's badge and buttons off the pane | gaps | yes: workbench head.rs (the shared pane head, every pane): the title takes only the room left, elided (the web's .term-head-title); the badge and the tools keep their size | yes (test/workbench.mjs "a very long title in a narrow pane": the drawn title, badge and buttons against the pane; fails without the fix) |
| UI review approves | reviewer | | |
| Owner has tried it | owner | | |

## Packaging sizes (Linux, against Electron v1.21.7)
| Package | Native | Electron |
|---|---|---|
| Crundi .deb | 207 MB (766 MB installed) | 352 MB (1153 MB installed) |
| Crundi Client .deb | 136 MB (429 MB installed) | 86 MB |
Windows sizes come from the CI job's summary.
