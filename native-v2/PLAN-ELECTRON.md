# Replace Electron with the native client: plan and progress

Each item of the change request and of PARITY-ELECTRON.md: done, tested, with how long each step took.
Rules: `package/CONTRACT.md`. Gap list: `PARITY-ELECTRON.md` (mark each line done and tested there).

## Steps
| Step | Where | Took | Result |
|---|---|---|---|
| Prep | PC farhan-RP-15 | about 25 min (release build 701 s, test build 190 s, references 184 s, Docker image 122 s) | Warm builds, Electron v1.21.7 references in ~/.cache/crundi-electron-ref, image crundi-upgrade-test |

## Change request
| Item | Stream | Done | Tested |
|---|---|---|---|
| Local mode (start, watch, restart, stop the bundled server) | local | | |
| Local or remote with the first-run wizard | local | | |
| PC worker through worker/ | local | | |
| Electron install moved over (data, config, start at login, shortcuts) | local, updates, pack | | |
| Updates for the combined install | updates | | |
| Windows .exe and Linux .deb replacing Electron's | pack | | |
| Docker (Linux) upgrade test from the latest Electron release | pack | | |
| windows-latest upgrade test from the latest Electron setup | pack | | |
| Quitting leaves no process behind | local, tester | | |
| Every PARITY-ELECTRON.md line done and tested | all | | |
| UI review approves | reviewer | | |
| Owner has tried it | owner | | |
