# Crundi mobile and client plan

Status: in progress. Stage 1 has started (see "Progress" at the end).
Estimate: 6 to 7 weeks of focused work for Android, first installable build
about 2 weeks in. The day counts are rough and came from reading the server
code, not from building any of it.

## Decisions

- **One published app, provisioned per server.** Crundi is not a hosted
  service. Everyone installs the same app and pairs it with their own server,
  the way 3CX does.
- **Reuse the web UI.** The app is a Kotlin shell around the page the server
  already serves, plus a bridge for native features. No second UI. Native
  windows (bubble, picture in picture) load the same page in a compact mode.
- **Background pieces are real Kotlin.** A notification reply, a home screen
  widget or a quick settings tile runs with no page open, so those call the
  server API directly with their own login.
- **Push goes through a relay run by the publisher.** Apple and Google only
  accept pushes signed with the publisher's keys, so those keys live on the
  relay and never on anyone's server.
- **The relay refuses anything without a key.** Keys are issued by hand, one
  per server, and entered in that server's settings. To get one, a server owner
  contacts the publisher on Telegram: `@TheRealFaruhaan`.
- **Relay hosting:** Cloudflare Workers. The relay's code is in this repo, in
  `relay/`. A GitHub Action tests and deploys it when a push to `main` changes
  that folder, and at no other time.
- **The published app works with the published relay only**, at
  `https://cr-rl-pub1.obitech.mv`. Anyone wanting their own relay ships
  their own build of the app with their own push credentials; their users ask
  them for keys. Bringing your own Firebase project to the published app is
  not supported. `android/RELAY.md` documents it.
- **App code:** this folder, on the `dev` branch.
- **Versions are checked in both directions** (see below).
- **Two Android builds.** `github` is built by CI and updates itself from
  GitHub releases. `play` is built by us for the Play Store and contains no
  GitHub checking or updating code at all; it uses Play's update flow. The
  split is made at build time, so the app never has to remember how it was
  installed.
- **iOS later.** Same pairing, same relay, same bridge contract, a Swift shell.
  Until then iPhone users have the home screen web app with web push.
- **Linux desktop client** after Android is built and tested, in both forms:
  with a server, and client only.

## Pairing

1. The web UI shows a "Connect a phone" screen with a Crundi code: the logo
   with five rings of dashes turning round it. It carries the server address
   and a one-time code that expires in a few minutes.
2. The app scans it and swaps the code for a login, kept in the phone's
   keystore. No password or 2FA code is typed on the phone.
3. On the phone itself, the mobile web UI offers "Open in app", which hands
   over the same code by link.
4. Fallback: type the server address and sign in normally inside the app.

### The Crundi code

- Our own format, not a QR code (`src/crundi-code.js`, which both draws it and
  reads it). The app reads it with that same file, run on camera frames.
- Two plain rings locate it and give its tilt. Five data rings lie between;
  one amber dash in each marks where that ring starts, so the rings can turn
  independently and the phone can be held at any angle.
- Reed-Solomon parity: partial damage is corrected, and a bad read is refused.
- A QR code of the same link is still returned by the server (`qr`). It is the
  fallback for an address the Crundi code cannot carry (longer than 46
  characters, or an IPv6 address), and the only thing a phone's own camera
  app can read.
- Four sizes now, the largest for host names of about 61 symbols. Common
  pieces of names (`.trycloudflare.com`, `.ts.net`, `.com`, `192.168.` and so
  on) cost one symbol each, so a Cloudflare quick tunnel address fits. The
  list of pieces is part of the format: add to its end, never reorder it.
- **It has an ending.** The screen showing the code asks
  `POST /api/pair/status` with `{ code }` every second or two. When the answer
  is `{ state: "paired", device }`, it adds the class from `doneClass()` to the
  `<svg>`: the rings wind into the logo, the outer ring closes and turns
  green, and a tick appears. After that the picture is no longer scannable.
- **Not yet proven on a real camera.** It reads back from browser screenshots
  at tilts up to 45 degrees and from simulated blur, grain and glare, but no
  phone has pointed at a screen yet.

Details that follow from how the server works today:

- **Two logins per phone**, one for the page and one for the background code.
  Refresh tokens rotate, and reusing an old one revokes the login, so the two
  cannot share one.
- **A paired phone is a named device** in the sessions list and can be cut off
  there. Revoking it revokes both logins.
- **Several servers** in one app, from the start.
- **Addresses without a real certificate** need an explicit "allow insecure"
  choice in the app.

## Push

1. The app gets a push token from Google (later Apple) and registers it with
   its own server, with an encryption key made on the phone.
2. The server encrypts each notification for that phone and posts it to the
   relay with the server's relay key.
3. The relay checks the key, applies a per-key rate limit, and forwards to
   Google or Apple. It sees a device token, ciphertext and timing, never
   content.
4. The phone decrypts and shows it. Reply and approve buttons call the user's
   own server directly.

- Encryption reuses the web push scheme already in `src/web-push.js`.
- The relay address is a server setting, for a custom build of the app with
  its own relay. Changing it drops the stored key, and the relay names its
  own contact for keys.
- A server with no key still works fully while the app is open; alerts fall
  back to Telegram and web push.
- An optional foreground service shows live progress while a chat is working.
  It is not the delivery path.

### Relay keys

- Issued by a command on the relay, with a label saying who it is for. The
  relay stores only a hash.
- Server settings get a "Mobile push" section: with no key it says how to ask
  for one, and has a field to paste it and a Test button.
- A key can be revoked without affecting anyone else.

## Versions

- **The server holds the oldest client it accepts**, per kind: web, desktop,
  android, ios. Below that the client is refused (HTTP 426, and on the socket).
- **Each client holds the oldest server it accepts**, and checks that itself.
- **`GET /api/handshake`** is the one exchange both checks use. It needs no
  login and its shape is frozen: fields are added, never renamed or removed.
- **An out-of-date client can still reach** the page, the handshake and login.
- **Callers that do not say what they are** (MCP tools, hooks, scripts) are
  never refused.
- **First minimum: `1.20.0-dev.1`** for web and desktop, the first build that
  has the check. Starting strict was deliberate.
- **Raise a minimum only for a real break.** The Play Store updates apps on its
  own while servers are updated by hand, so "app newer than server" is the
  normal case. For most changes the app hides what the server does not list in
  the handshake's `capabilities`.

What each client does when refused:

- **Web:** reloads itself once. If that does not help it says so, and does not
  loop.
- **Desktop:** an "Update required" screen with Check for update and Install,
  driving the client's own updater.
- **Android, `play` build:** Android's in-app update flow.
- **Android, `github` build:** the newest APK from GitHub releases.
- **Background actions:** a refused notification reply becomes "Update Crundi
  to reply".

What a client does when the server is too old:

- A native "This server needs updating" screen with both versions.
- The owner gets an Update server button, with a warning that it restarts the
  server and ends every running chat and terminal.
- A collaborator is told to ask the owner.
- If both sides are too old for each other, the screen lists both updates.

## Native features

- Notifications: inline reply, approve or deny, grouped per chat.
- Bubbles: a floating, fully interactive chat.
- Picture in picture: a live view with stop and approve buttons. Android does
  not allow typing in it, which is why bubbles are also built.
- Share text, photos or files from any app into a chat.
- Native file and camera pickers, downloads, image paste.
- Voice input.
- Biometric lock.
- Launcher shortcuts and a quick settings tile.
- Home screen widget. The one piece with its own native layout, since Android
  widgets cannot host a web page.
- Deep links, back gestures, edge to edge, split screen and tablets.

Not coming to the phone: the pieces that exist only in the desktop client (the
embedded browser panel, RDP setup, Windows startup, local server controls).

## Bubbles: the reference

The owner's reference is Telegram's floating chat heads on Android (a
screenshot of them open over Crundi). That is Android's Bubbles API, and it is
the main way a chat should be reachable on the phone.

What the reference shows, and what Crundi's version is:

- **A row of round heads along the top, one per conversation, and a `+`.**
  One head per Crundi chat. The row, the `+` overflow and the small app badge
  on each head are drawn by Android, not by the app.
- **The head's picture.** Telegram uses the contact's avatar. Crundi uses a
  mark: the project's colour with the chat's initials on it, so chats in one
  project share a colour and can still be told apart.
- **Tapping a head opens that chat in a floating panel** with its own header
  (back, picture, title, a line under it, a menu), the messages, and the
  message box at the bottom. Crundi's panel is the existing chat in a compact
  layout: chat title, project name under it, the same messages and composer.
- **It floats over whatever is on screen**, including Crundi itself.

What Android requires for a chat to become a bubble:

- Each chat is a *conversation*: a long-lived shortcut plus a messaging-style
  notification that carries bubble details. So bubbles sit on top of the
  notification work and cannot come before it.
- A bubble appears when that chat notifies, or when the user asks for it from
  inside the app ("Open as bubble"). An app cannot put one up unprompted from
  the background.
- The user can turn bubbles off per app or per conversation, and Android then
  shows an ordinary notification. On Samsung phones, floating notifications
  must be set to "Bubbles" in the system's notification settings.

What this needs from the server:

- The compact chat layout (the panel's content).
- Each chat notification must say which chat it is about: its id, title and
  project. The payload in `RELAY.md` does not carry that yet.

## Build order

1. **Server groundwork (3 to 4 days).** Handshake and version gate. Pairing
   endpoint and QR screen. Device registry. Mobile push channel. Relay key
   setting. Compact chat view. Bridge detection. What's New step for pairing.
2. **Relay (3 to 4 days).** Send endpoint, key commands, limits, Firebase
   sender. Apple sender left as a stub.
3. **Android shell (3 to 4 days).** Pairing, server list, files, camera,
   downloads, paste, permissions, links, biometric lock, both build flavors,
   CI for the `github` build.
4. **Notifications (4 to 5 days).**
5. **Bubbles and picture in picture (4 to 6 days).**
6. **Extras (7 to 9 days).** Live progress, share in, shortcuts, tile, widget,
   voice.
7. **Parity and release (4 to 5 days).** Every existing feature on a real
   device, Play listing, closed test.

Then: iOS (about 3 to 4 weeks, needs a Mac or cloud Mac builds and TestFlight)
and the Linux desktop client.

## Needed from the publisher

- Google Play developer account and a Firebase project.
- A Cloudflare account for the relay.
- A release signing key, held in Actions secrets for the `github` build.
- A phone for testing. The dev server has no emulator support and no Android
  tooling, so builds run in Actions and testing happens on a real device.

## Risks and things not yet verified

- Play's closed test requirement for new personal accounts can add about two
  weeks of waiting, outside the build time.
- Bubbles and background behaviour vary on Samsung and Xiaomi.
- Apple is wary of web wrappers; the native features help the case, and
  reviewers need a demo server.
- Sending to Apple from a Cloudflare Worker has not been tested.
- The desktop "Install update" button has only been tested against a stub, not
  a real Windows client.

## Progress

- **Done:** handshake and version gate on the server, the page's reload and
  "Update required" screen, strict first minimums (`src/client-version.js`,
  `scripts/test-client-version.mjs`).
- **Done:** pairing on the server. One-time codes, the QR code, two logins per
  device, devices named in the sessions list and revoked as one
  (`src/pairing.js`, `src/qr.js`, `/api/pair/start`, `/api/pair/claim`,
  `/api/pair/cancel`).
- **Held back until the app can pair:** the "Connect a phone" screen in the web
  UI, the `/pair` landing page a phone's camera opens, and the What's New step.
  Showing a pairing screen before there is an app to scan it would advertise
  something nobody can use.
- **Done:** the Crundi code, with its ending (`src/crundi-code.js`).
- **Done:** the server's side of phone push (`src/mobile-push.js`): the
  "Phone app" notification channel, the relay key, device registration, and
  the contract the relay must meet (`android/RELAY.md`). Run end to end
  against a stand-in relay; no real relay exists yet.
- **Held back with the pairing screen:** the "Mobile push" section in Settings
  (key field, who to ask, Test button). The routes behind it exist.
- **Done and live:** the relay (`relay/`), at
  `https://cr-rl-pub1.obitech.mv`: key checking, issuing and revoking, rate
  limit, sending to Android through Firebase project `obitech-crundi`. Google
  accepts the relay's credentials (a send to a made-up device token came back
  `INVALID_ARGUMENT`, which is Google rejecting the token, not the relay). iOS
  sending is a stub.
- **Not yet proven:** a notification reaching a real phone. That needs the
  app.
- **Auto-deploy starts when `dev` is merged:** GitHub only runs the workflow
  once it is on `main`.
- **Done:** the compact chat, at `/chat/<id>` (`src/chat-page.js`,
  `app/vendor/crundi-chat-page.js`). Its own small page, drawing the
  conversation with the same component the workbench uses. `?pip=1`, or
  `window.crundiSetPip(true)`, switches it to the picture-in-picture layout.
- **Done:** project marks (`src/project-mark.js`). Projects had no picture, so
  each gets a tile: its initials on a colour worked out from its name. The
  app's Kotlin copy must give the same values; the test pins them.
- **Done:** a chat's notifications say which chat they are about (id, title,
  project, mark) and point at its page.
- **Done:** the bridge contract. Inside the app, `window.CrundiNative` may
  offer `session()`, `saveSession(json)`, `close()`, `openMain(path)` and
  `chatState(json)`. Each is optional, so the page is the same page in a
  browser.
- **Stage 1 is complete** apart from the screens held back until there is an
  app (Connect a phone, Mobile push settings, the What's New step).
- **Done:** the Phone app section in Settings (`app/vendor/crundi-phone.js`):
  the Connect a phone dialog, which shows the Crundi code, waits, and plays
  the ending; and the push key box. Its What's New step is in the
  `1.20.0-dev.1` edition.
- **Done, first cut of the Android shell** (`android/app`): pairing by scan or
  pasted link, the version check both ways, the server's page in a web view
  signed in from the pairing, file picking, downloads, camera and microphone
  for the page, both build flavours. Both compile and the unit tests pass.
  **Not yet run on a phone.**
- **Not in the shell yet:** a way to switch or remove servers once paired
  (only reachable from an error screen), biometric lock, a release signing
  key, signed release builds on GitHub, Play's in-app update flow (the play
  build opens the store page), and the owner's Update server button.
- **Names settled:** the app is `mv.obitech.crundi` (the publisher's
  developer account is obitech.mv), registered under that name in Firebase
  project `obitech-crundi`. The relay is at `https://cr-rl-pub1.obitech.mv`;
  its first address, on faruhaan.com, no longer answers. The Windows desktop
  app keeps its own id (`com.crundi.app` in `package.json`): changing an id
  that is already installed would break its updates.
- **`android/app/google-services.json`** is on the build machine and not in
  the repo. Notifications will need it at build time; CI will need it as a
  secret.
- **First release route:** an internal testing track in the Play Console,
  from a signed `play` bundle.
- **Next:** try it on a real phone; then notifications (stage 4).

## API notes for the app

- `GET /api/handshake` with `X-Crundi-Client: android-play/1.0.0` (or
  `android-github/...`). No login. Check `client.accepted`, then compare
  `server.version` with the app's own minimum.
- The Crundi code (and the fallback QR code) holds `https://<server>/pair#<code>`.
  `decodeImage({ data, width, height })` in `src/crundi-code.js` returns
  `{ link }` from an RGBA camera frame, or null. The code is in the
  fragment so it is never sent to a server by a browser.
- `POST /api/pair/claim` with `{ code, device: { name, platform } }` returns
  `{ username, device, sessions: { page, background }, server }`. Each session
  is `{ token, refreshToken, expiresIn }`. Give `page` to the web view and keep
  `background` for native calls; never share one between the two.
- Access tokens last 15 minutes. `POST /api/auth/refresh` with
  `{ refreshToken }` returns a new pair and retires the old refresh token.
- HTTP 426 with `error: "client_too_old"` on any call means the app is below
  the server's minimum.
