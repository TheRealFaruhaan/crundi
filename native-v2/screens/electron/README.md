# Electron-parity screens (Tester, 7fd41030, farhan-RP-15, Xvfb 1440x900)

Made by test/electron/local.mjs (run with PACKAGE_DIR=~/.cache/crundi-pack/stage-full: the packaged release client, its bundled Node and server), test/electron/updates.mjs and test/electron/browserstart.mjs.

- setup-wizard-1.png, setup-wizard-1-alert.png, setup-wizard-1-bot-token.png, setup-wizard-2-username.png, setup-wizard-3-alert.png, setup-wizard-3-projects.png, setup-wizard-4-tunnel.png, setup-wizard-5.png, setup-wizard-6.png, setup-wizard-7-updates-startup.png: the all-in-one's first-run wizard, every step and both alerts.
- settings-before-signin-setup.png: the next start, the bundled server with no sign-in method yet: the sign-in page's first-run setup (N1).
- local-chat-streaming.png: a chat on the bundled server while its reply streams.
- local-status-stopped.png / local-status-crashed.png: the status screen after Tray › Stop and after the server was killed.
- server-screen-local.png, server-screen-remote.png (1100 wide), server-screen-remote-narrow.png, server-screen-local-narrow.png (800 wide): Tray › Server…, both choices, normal and narrow (F8).
- settings-folded.png, settings-desktop-app.png, settings-server.png: Settings on the bundled server.
- update-available-prompt.png, update-ready-prompt.png, update-no-password-prompt.png: the updater's prompts (debug build).
- browser-first-start-loading.png / browser-first-start-loaded.png: the first browser start on a fresh CEF profile (host start slowed to 3 s): the drawer tab loading (spinner, Reload turning), then the page shown (debug build).
- tray-menu-{stopped,running,error}.json: the tray menu (labels, enabled, checked) in each state. The tray is a StatusNotifierItem menu drawn by the desktop shell, not by the app, so Xvfb has no picture of it.
