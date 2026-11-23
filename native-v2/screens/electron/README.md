# Electron-parity screens (Tester, 9401e817, farhan-RP-15, Xvfb 1440x900)

- setup-wizard-*.png: the all-in-one's first-run wizard, steps 1-7 and both alerts (local mode, debug build).
- local-status-stopped.png / local-status-crashed.png: the status screen after Tray › Stop and after the server was killed.
- server-screen-local.png / server-screen-remote.png: Tray › Server…, both choices.
- settings-folded.png, settings-desktop-app.png, settings-server.png: Settings on the bundled server once a sign-in method exists.
- settings-before-signin-setup.png: Settings on a bundled server with no sign-in method yet (see the finding: Electron shows the sign-in setup page here).
- update-available-prompt.png, update-ready-prompt.png, update-no-password-prompt.png: the updater's prompts (test/electron/updates.mjs).
- tray-menu-{stopped,running,error}.json: the tray menu (labels, enabled, checked) in each state. The tray is a StatusNotifierItem menu drawn by the desktop shell, not by the app, so Xvfb has no picture of it; the items match app/main.js updateTrayMenu word for word.
