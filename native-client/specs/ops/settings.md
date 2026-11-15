# Settings (ops, tab `settings`)

The web's `renderSettings` / `organizeSettings` (src/webapp-html.js
~15370-16000), the update sections (~4280-4720), the phone app
(app/vendor/crundi-phone.js), Cru's server settings (`showVoiceServer`
~5818) and the Electron client's "Desktop App" section (`buildUpdatesSection`).
Owner only. UI `ui/ops/settings.slint` (+ `settings-parts.slint`, brand marks
in `settings-icons/`), Rust `src/ops/settings.rs` (+ `settings/rules.rs` pure
rules with tests, `settings/local.rs` this PC's choices and launch at startup).

## Parts, in the web's order

Every section is a folded card (`.info-section.set-fold`): the title and a
chevron that turns; all start folded; which are open is kept while the client
runs (`settingsOpen`). Groups (`.set-group`) as `SETTINGS_GROUPS`:

| Group | Sections |
|---|---|
| CRUNDI | Server, Claude Code, What's new |
| CLAUDE | System prompt, Skills (skills.slint), Usage (skills.slint) |
| CRU | Voice assistant |
| NOTIFICATIONS | Notification Channels, Notification Settings |
| ACCESS | Sign-in, Phone app, Session |
| THIS SERVER | Configuration, Terminal, Backup & restore (skills.slint) |
| MORE | Desktop App (not in the groups: organizeSettings puts it last) |

## Controls and endpoints

- **Server**: version, state line, Check / Update to X (confirm), Check now;
  Release channel Production/Dev (dev confirms, `confirmDevChannel`); dev
  note, blocker, Release notes, the update log while applying (polls 5 s);
  Restart server (confirm). `GET /api/update/status[?force=1]` (forced when
  older than 60 s), `POST /api/update/{apply,channel}`, `POST /api/restart`.
- **Claude Code**: version, line, Check / Update to X (yellow past the tested
  version, confirm), tested line, blocker / sudo note, last result.
  `GET /api/claude-update/status`, `POST /api/claude-update/apply`.
- **What's new**: Show it again (the tour: extras, through `set_whats_new`).
- **System prompt**: Everywhere + Just <project> (only with a project open),
  Save prompts + status. `POST /api/settings {systemPrompt}`,
  `POST /api/projects/system-prompt`.
- **Voice assistant** (server part): Brain, Model, Thinking effort, Stop
  Claude Code after, Talking speed, Volume, Pause before sending, Close the
  voice screen after (each saves on pick), Admin mode, Cru on its own with
  its classes and Stop everything (confirm), Spending (line, four fields,
  Save spending), Keys (three, Save keys), Start a fresh conversation.
  `GET/POST /api/voice/settings`, `/api/voice/reset`, `/api/voice/away/stop`.
- **Notification Channels**: a switch per channel (inbox locked on,
  unavailable ones locked off); Web Push: Send a test; Telegram: bot state
  dot, "edit/set it up under Sign-in" (opens Sign-in), Chat ID + Save.
  `GET/POST /api/notify/channels`, `/api/push/test`, `/api/notify/telegram/chat`.
- **Notification Settings**: the matrix (Event × Always / When Away / Never),
  categories in accent; a lamp click saves at once (`POST /api/settings {notifyPrefs}`).
- **Sign-in**: Password + authenticator (switch, new password + Set/Change,
  the authenticator secret shown once); Telegram (switch, state, BotFather
  hint + Open, Bot token, Allowed username, Save / Restart now / Remove);
  Google and GitHub (switch locked until tested, Callback URL + Copy, where
  to make the app + Open, Client ID, secret, allowed account, Save / Test /
  Remove, the test's line: system browser + `POST /api/auth/oauth/poll`
  every 1.5 s for 10 min). `GET/POST /api/auth/config`, `/api/auth/oauth/start`.
- **Phone app**: Connect a phone (the pairing dialog: the server's Crundi
  code or QR as SVG, swap, copy link, countdown, New code, Close; cancels an
  unused code; `POST /api/pair/{start,status,cancel}`); Notifications on the
  phone: key line, contact link, Save / Test / Remove, the relay fold
  (`/api/push/mobile*`).
- **Session**: Sign out of <server> (two clicks, `Shell.sign-out()`), Signed
  in devices Show/Hide, each device with Revoke (two clicks), Sign out the
  other N devices (two clicks). `GET /api/auth/sessions`, `POST /api/auth/sessions/revoke`.
- **Configuration**: the .env path, five fields, Save + status ("Saved —
  restart required"). `POST /api/settings {settings}`.
- **Terminal**: New Line Key (this PC: `newline_key()` for the terminals),
  Close idle panes after (saved on Enter or leaving the field; 0 or the
  server's least; `POST /api/settings {autoParkMinutes}`).
- **Desktop App**: Launch at startup (autostart entry / LaunchAgent / Run
  key), Automatic updates, Release channel (dev confirms), the status line
  and Check now. The tab's update dot: `set_update_available()`.

## States

Loading, error (Retry), data; each update section: checking, error, data,
applying/installing; push: checking, unavailable, data; devices: loading,
error, empty, list; the pairing dialog: getting a code, refused (New code),
showing, expired, paired.

## Deliberate differences / gaps

- Web Push's "Allow in this browser" is a browser's: not offered (Send a
  test is).
- Cru: the voice picker, Your voice (enrolment), memory search and the
  notebook, Tickers and triggers, Your PIN and the PC lock, Kid mode, Brains
  compared, and the device's push-to-talk and wake word are not built here
  (Cru on the PC is the desktop area's, phase 6.6).
- Desktop App: no updater yet (the ship phase): Check now says so.
- The pairing picture is the server's SVG drawn still (Slint plays no SVG
  animation).
- "goto Sign-in" opens the section but does not scroll to the Telegram block.

## Checklist

- [x] groups and folds in the web's order; Skills / Usage / Backup placed
- [x] every control above wired to its endpoint, with the web's wording
- [x] test/ops/settings.mjs; screens/ops/settings-*.png
