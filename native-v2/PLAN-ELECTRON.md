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
| UI review approves | reviewer | | |
| Owner has tried it | owner | | |
| Owner: a Mindmap idea dropped on a chat's box left its drag picture floating there | pack | yes: the map takes the release outside it in the capture phase (the chat's drop stops the bubble) (mindmap/draw.rs) | yes: test/planning/drops.mjs drags an idea onto a chat's box with the pointer, checks the reference goes in and no idea is left in hand; fails without the fix |
| Owner: a long idea hovered opened up see-through, its neighbour's words over it | pack | yes: the opened idea is a paint layer of its own over the map's (in one layer every card is under every word) (mindmap/draw.rs) | yes: test/planning/mindmap-look.mjs pictures the opened idea over its neighbour, changes the neighbour's words, the pixels there stay the same; fails without the fix |

## Packaging sizes (Linux, against Electron v1.21.7)
| Package | Native | Electron |
|---|---|---|
| Crundi .deb | 207 MB (766 MB installed) | 352 MB (1153 MB installed) |
| Crundi Client .deb | 136 MB (429 MB installed) | 86 MB |
Windows sizes come from the CI job's summary.
