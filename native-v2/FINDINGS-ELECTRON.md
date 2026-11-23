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
