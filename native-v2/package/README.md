# native-v2/package: the installers that replace Electron's

Rules and ids: `CONTRACT.md`. Nothing here publishes anything.

| File | What it does |
|---|---|
| `stage.mjs` | Lays out one install folder (client, browser host + CEF, worker, `node/`, `server/` with production `node_modules`). Strips Linux binaries and CEF in the copy. |
| `feed.mjs` | electron-builder's uninstall GUID for an appId (`guid`), and the update feed `.yml` (`yml`). |
| `windows/crundi.nsi`, `windows/build.mjs` | Per-user one-click NSIS installer into `%LOCALAPPDATA%\Programs\Crundi` under Electron's uninstall key; makes the shortcuts, the DisconnectRDP task and the crundi skill as `build/installer.nsh` does; points Electron's Run value at the new exe. |
| `linux/build-deb.sh` | The `.deb` (`crundi` / `crundi-client`, `/opt/Crundi`, `/usr/bin` link, desktop entry), Depends computed for the glibc the binaries need. |
| `linux/Dockerfile.build` | Ubuntu 24.04 with GTK headers: builds the worker and the server's `node_modules` with the bundled Node, nothing installed on the host. |
| `upgrade-test/linux.sh` | Docker: Electron .deb → data, config, launch at login (set by Electron itself) → new .deb → checks → remove. |
| `upgrade-test/windows.ps1` | CI windows-latest: the same from Electron's Setup.exe, plus the uninstall entry, shortcuts and Run value. |
| `upgrade-test/electron-login.mjs` | Turns launch at login on in a running Electron through its inspector. |
| `check.sh` | The pack stream's check on the PC: worker, node_modules, both .debs, both Docker upgrade tests, sizes. |

CI: `.github/workflows/native-v2-package.yml` (jobs `windows` and `linux`).
