# Contract (native-v2/package/CONTRACT.md)

Every stream builds to this. A change is a question for the Manager.

## Editions
- All-in-one "Crundi": the server folder is present. Same ids as Electron's all-in-one.
- Client "Crundi Client": no server folder. Same ids as Electron's client build.
- Both come from one native build. The edition is chosen at package time only by whether `server/` exists.

## Install layout (both OSes, under the install dir)
- `crundi[.exe]`: the native client (all-in-one; `crundi-client[.exe]` for the client edition).
- `crundi-browser-host[.exe]` and CEF files.
- `crundi-worker[.exe]`: the worker binary from worker/.
- `version`: the release's version (package.json's), one line. The client reports it (logs, Settings, the updater's current version).
- `node/`: Node 22 for that OS. Linux: `node/bin/node`; Windows: `node/node.exe`. Bundled on both in the all-in-one; Windows falls back to the system node. The client edition ships no `node/`.
- `server/`: `src/`, `app/vendor/`, `app/homepool-node.js` (the server imports it), `scripts/`, `skills/`, `assets/`, `package.json`, `.env.example`, and the production `node_modules`. node-pty and the other native modules are built for that Node; cloudflared is in its usual `node_modules` place.
- The client finds the server at `<exe dir>/server/src/index.js` and Node at `<exe dir>/node/...`. Overrides `CRUNDI_SERVER_DIR` and `CRUNDI_NODE` are for tests.

## Where things live (Electron's, so an install moves over in place)
- Server data and .env (`DATA_DIR`, `DOTENV_PATH`): Electron's userData. Linux all-in-one: `~/.config/crundi-desktop`. Windows: `%APPDATA%\Crundi`. Client edition: `~/.config/Crundi Client` and `%APPDATA%\Crundi Client`.
- The native client's own settings stay in its config folder (`~/.config/crundi-native`, `%APPDATA%\Crundi Native`).
- On the first start, the client imports once from Electron's userData:
  - `client.json`: mode, serverUrl and voiceWake.
  - `update-config.json`.
  - `homepool-node.json`, including the deviceId.
  - `drawer-history.json`.
  - `worker.json`.
  - `voice-wake/`: Hey Cru's training (`profile.json` with the owner's voice prints, `speaker/`, the keyword model), when its profile is in the format the client reads and the client has none of its own. It is taken once with its own marker (`voice-wake/electron-import.json`), so an install that already imported the rest still takes it (T16).
- Electron's saved drawer passwords cannot be decrypted, so they are not imported.

## Identity (so the new installer replaces Electron's)
- **Windows:**
  - NSIS, one-click, per-user, into `%LOCALAPPDATA%\Programs\crundi` (client: `...\crundi-client`): electron-builder's folder, the package name (read in the CI upgrade test).
  - appId and AUMID `com.crundi.app` (client `com.crundi.client`), with the same uninstall key electron-builder used (read it from an installed Electron setup in the CI upgrade test).
  - Shortcuts as `build/installer.nsh` makes them; the DisconnectRDP task as it does.
- **Linux:** a .deb with package name `crundi` (client `crundi-client`), executable `crundi` (client `crundi-client`), installed in `/opt/Crundi/` (client `/opt/Crundi Client/`). It replaces the Electron package of the same name.
- **Launch at startup:** Linux `~/.config/autostart/crundi.desktop` (Electron's app.getName() is the package name; seen in the Docker upgrade test; client `Crundi Client.desktop`); Windows HKCU Run with Electron's value name `electron.app.Crundi` (client `electron.app.Crundi Client`; read in the CI upgrade test). There is one implementation, and it replaces Electron's entry rather than adding a second one.

## Release asset names (so Electron installs update into the native client by themselves)
- Windows: `Crundi-Setup-<v>.exe` and `latest.yml`. Client edition: `Crundi-Client-Setup-<v>.exe` and `client.yml`.
- Linux: `Crundi-<v>-linux-x64.deb` and `latest-linux.yml`. Client edition: `Crundi-Client-<v>-linux-x64.deb` and `client-linux.yml`.
- The `.yml` feed files use electron-updater's format (version, files, sha512) so Electron's updater accepts them. The native updater reads the same files.

## Checks each stream hands over green
- **local, updates and gaps:** `flock $HOME/.cache/crundi-build.lock native-v2/tools/build.sh test --workspace` plus their part of `node native-v2/test/electron.mjs` under xvfb-run (local creates the suite; the others add to it).
- **pack:** `native-v2/package/check.sh` builds the .debs and runs the Docker upgrade test from the latest Electron release, plus the Windows CI job (`.github/workflows/native-v2-package.yml`) green, with its upgrade test from the latest Electron setup.
- Release .debs are built on Ubuntu 24.04 (the CI linux job), so their glibc dependency matches Electron's users; a PC build (glibc 2.43) is tested on an ubuntu:26.04 image only.
