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
- **Relay hosting:** Cloudflare Workers. Relay code lives in a separate private
  repo.
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

1. The web UI shows a "Connect a phone" screen with a QR code: the server
   address and a one-time code that expires in a few minutes.
2. The app scans it and swaps the code for a login, kept in the phone's
   keystore. No password or 2FA code is typed on the phone.
3. On the phone itself, the mobile web UI offers "Open in app", which hands
   over the same code by link.
4. Fallback: type the server address and sign in normally inside the app.

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
- The relay address is a server setting, so a fork can run its own app and
  relay.
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
- **Next:** pairing and the device registry.
