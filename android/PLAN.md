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
- **Proven on a phone:** pairing. Scanning the Crundi code with the camera,
  claiming it, and coming up signed in all worked on the owner's phone.
- **Built, not yet proven on a phone: notifications, bubbles and the floating
  window** (app 1.0.0-dev.2). The phone registers with each server, opens what
  the server seals (tested against a message the real server sealed), shows
  chat notifications as conversations with a reply box, and can float a chat
  as a bubble or as picture in picture. A chat's menu in the web UI gets
  Bubble and Float when the page is inside the app (`CrundiNative.can()`).
- **Not built:** approve and deny buttons on a "needs you" notification, and
  buttons inside the floating window. Their What's New step waits until the
  feature has been seen working.
- **Asked for, not started:**
  - **Live updates** (what iOS calls live activities), for the next app
    release: while Claude is working in a chat, a chip in the status bar and
    an entry on the lock screen, as sports scores and timers have. The owner's
    reference is a live score chip on a Samsung phone. On Android this is an
    ongoing notification asked to be "promoted"; it is new in Android 16 and
    older phones show it as an ordinary ongoing notification. It needs the
    server to tell the phone when a chat STARTS working, not only when it
    stops, which no notification does today.
  - Browser tools should let Claude choose where the browser runs: on the
    server, or on a named attached client (PC, phone). Today an attached
    client's browser is always used, which cannot reach the server's own
    localhost.
  - The phone app as a browser host, so the Browser pane works in it.
  - Whenever an MCP tool changes, the instructions that describe it change
    with it: the tool's own description, the system prompt text the server
    gives Claude, and the `crundi` skill's reference pages.
- **Built since app 1.0.0-dev.2, not yet released:**
  - **App settings**, a screen of the app's own, reached from "App settings"
    at the foot of the sidebar: notifications, bubbles, camera, microphone and
    files, each with how it stands and a button to change it; the paired
    servers, with open, remove and connect another; where updates come from.
  - **Downloads.** Inside the app the page now hands the phone a real address
    and the phone's own downloader saves it to Downloads. Before, the page
    saved from memory to an address only it could read, and the app answered
    "This file cannot be saved from here yet".
- **The live card, as agreed from the owner's sketch and demos:**
  - Chip: elapsed time for one working chat, a count for several, "Needs you"
    when any chat is waiting on the person (it takes over the chip), "Done"
    briefly at the end.
  - Card: one line per chat with the time it started and its name; a chat
    that needs action is marked and listed first; Open and Stop.
  - Android does not allow a custom layout on a live card (no RemoteViews; the
    standard, big text, call, progress and metric styles only), so the
    sketch's layout is carried as content in the system's shape, with the
    chat's mark as the picture.
  - Needs: `android.permission.POST_PROMOTED_NOTIFICATIONS`, an ongoing
    notification with a title that asks to be promoted, and the server
    telling the phone when a chat starts working.
- **Built, in app 1.0.0-dev.3 and server 1.20.0-dev.4, not yet proven on a
  phone:**
  - **The live card.** One per project. The server sends a silent message
    whenever an owner chat's state changes, and when a chat on a card is
    closed; the app keeps what it has been told and redraws that project's
    card. Open and Stop on it. App settings shows whether the chip is allowed.
  - **Approve and Deny** on a "needs you" notification, when exactly one
    permission request is waiting. The notification says what Claude wants to
    do; Approve needs the phone unlocked. Questions and plans are never
    offered as yes or no.
- **Still not built:** the What's New
  steps for notifications, bubbles and the live card (held until they have
  been seen working).
- **First test on the owner's phone (app 1.0.0-dev.3): nothing arrived.** No
  notification, no live card. The server had the phone registered and Google
  accepted every message, so the fault was on the phone. A likely cause was
  found and fixed in app 1.0.0-dev.4: on a first start the app registered
  twice at once, each time with a new key pair, and could keep one pair while
  the server held the other, after which nothing it received could be opened.
  Registration is now one at a time, repeats once per app build, and repeats
  itself whenever a message arrives that no paired server's keys can open.
  **This is a likely cause, not a proven one.**
- **App settings has a notification check** (1.0.0-dev.4): a test notification
  made on the phone, a Register again button, and a log of what arrived and
  what became of it, with no message text or tokens in it. If notifications
  still fail, that log says at which step.
- **The floating window (picture in picture) was removed** at the owner's
  request. Bubbles are the one way a chat floats.
- **Release builds are shrunk with R8** from 1.0.0-dev.4. The page calls into
  the app by name; `proguard-rules.pro` keeps those names.
- **Proven on the owner's phone with app 1.0.0-dev.4:** notifications arrive,
  with the reply box, and the live card shows in the notification panel. So
  the registration race was the fault, or at least a fault.
- **Reported from that test, and addressed in app 1.0.0-dev.5 and server
  1.20.0-dev.6 (none of these fixes proven on a phone yet):**
  - No vibration. The first Chats channel was made without it and Android
    does not let a channel be changed, so there is a new channel (`chats.v2`)
    with vibration, and the old one is removed.
  - The chat's suggested next message is now offered on its notification, as
    a choice under it. The server sends it to phones silently when it appears
    and when it goes. Android's own guessed replies are switched off.
  - The live card's picture was drawn half off the card on Samsung. It is
    removed; the project's name is in the card's top line.
  - Pasting a picture from the keyboard. The app's web view now tells the
    keyboard it takes images, and hands one over to the page as a paste.
- **Open: the live card is not treated as a live update on the owner's
  Samsung.** It shows in the panel as an ordinary ongoing notification, with
  no status bar chip and not under the live category. The card asks for
  promotion as Android 16 documents. 1.0.0-dev.5 writes to the notification
  log which Android version the phone runs, whether Crundi may post promoted
  notifications, whether the card meets Android's rules, and whether Android
  promoted it. Samsung has had its own live notification system limited to
  approved apps; whether that is what stands in the way is not known yet.
- **Was asked for, and is now built (see below): notifications that go away
  once attended to.** The request as it was made:
  - A request notification (a secret, a permission) should disappear once it
    has been answered, wherever it was answered.
  - A chat's notification should disappear once that chat has been opened.
  - Needs: the app clearing a chat's notification when its page is opened in
    the app, and the server sending phones a silent "clear" when a request is
    resolved or a chat is read somewhere else.
- **The log line came back (S24 Ultra, One UI 8.5, Android 16):** chip
  allowed = true, card meets Android's rules = true, Android promoted it =
  true. So Android does treat the card as a live update, and Samsung still
  shows it as an ordinary notification: no chip, not in the Now Bar. Samsung
  keeps those for apps on its own list.
- **Tried in app 1.0.0-dev.6, unproven:** the undocumented Samsung extra
  `android.ongoingActivityNoti.automation = true`, reported by another app's
  developers to let an already-promoted card through on One UI 8.5 and later.
  Some One UI builds also have a developer option, "Live notifications for
  all apps". If neither works, the chip is Samsung's to grant and not
  something the app can do.
- **Also in 1.0.0-dev.6 and server 1.20.0-dev.7:**
  - No Stop on the live card, at the owner's request: nothing that stops or
    closes a chat is reachable straight from a notification.
  - A bubble's window is named after its chat in the recent apps list.
  - A pasted picture over about 1.5 MB is sent as a JPEG no larger than 2560
    pixels on its long side.
  - A failed upload now says which HTTP status answered and what it said. One
    pasted upload failed with "Bad response from the server" and later ones
    worked; the cause was not found. The server logged nothing and handles
    20 MB uploads in a test, and the site is behind Cloudflare, so the likely
    answer is that something other than Crundi replied that once.
- **Coming to the main window no longer reloads it** (app 1.0.0-dev.6, server
  1.20.0-dev.8). A tapped notification, or "Open Crundi" from a bubble, used
  to load an address in the main window, which threw the page away and built
  it again behind a loading screen. The app now asks the loaded page for the
  chat (`window.crundiOpenChat(id)`), and loads an address only if the page
  is too old to be asked.
- **Rebuilt as the same app 1.0.0-dev.6 (it had not been uploaded), with
  server 1.20.0-dev.9. None of this has run on a phone:**
  - **What Claude shows in a chat is drawn in a bubble too.** Files to
    download, pictures, video, sound and frames were invisible there, because
    the compact page never loaded the code that draws them. That code now
    lives in `crundi-widgets.js` (`toolMedia`) and both pages use it. A
    bubble can save files.
  - **Notifications take themselves down.** A chat's, when the chat is on
    screen in the app (the page reports which chats are really in view) or
    when the chat starts working again, which means somebody answered it. A
    request's (a secret, a collaborator's permission), when the last waiting
    request of that kind has been answered anywhere; the server sends phones
    a silent "clear".
  - **A chat open in the main window loses its bubble**, so its own head is
    not floating over it.
  - **Bubbles for active chats when leaving the app.** Chats that are
    working, waiting, or waiting on the person get a closed bubble as the app
    is left. On by default; App settings turns it off.
  - **Approval requests vibrate.** They went to the quiet Server channel;
    they have a Requests channel of their own now.
- **The owner's rule: anything built for a chat must also work in the bubble.**
  Two pages draw a chat (the workbench, and the compact page a bubble shows).
  What lives inside the shared chat component reaches both; what the
  workbench does around a chat as its host does not, and each such gap was
  found on the phone as a silent failure. Added to the compact page and the
  bubble's window in server 1.20.0-dev.10 and the rebuilt app 1.0.0-dev.6:
  - The secret approval prompt, with the PIN. A request raised from a chat in
    a bubble used to be invisible there and time out.
  - A secret request now knows which chat asked (the tool process says so,
    Claude cannot), so its notification is that chat's and reaches its bubble.
  - The attach button, camera and microphone in a bubble.
  - Panels Claude places in a chat, the plan, renaming.
  - **Still missing from the compact page:** resuming a chat that was closed
    by a server restart (it says "Chat closed" and offers the main app),
    opening a file path from a message, and the owner's inbox of
    collaborators' requests.
- **Proven on the owner's phone with app 1.0.0-dev.6: the live card shows as
  a live notification on Samsung** (One UI 8.5). The undocumented
  `android.ongoingActivityNoti.automation` extra is what did it; it may stop
  working in a later One UI.
- **In app 1.0.0-dev.7 and server 1.20.0-dev.11, not yet proven on a phone:**
  - The live card's glow on Samsung was heavy, on the lock screen too. The
    glow is Samsung's. Two things within reach were changed: the card no
    longer sets an accent colour, and it is not posted again when what it
    says has not changed. Whether either lightens the glow is not known.
  - A panel in a bubble drew with an icon the size of the bubble: the compact
    page lacked the one rule that sizes every icon. A panel docked to a chat
    now sits above the conversation there, with live updates from the
    server's event stream, which also brings secret requests at once.
  - The Bubble button now sends the app to the background, since a chat open
    in the main window loses its bubble.
  - **A bubble closed by itself when a message was sent from it**, not every
    time. Not reproduced off the phone, so the cause is not proven. Three ways
    the app could take a bubble down from underneath were closed: a suggested
    reply being withdrawn (which happens on send) no longer posts a bubble's
    notification again; the main window's "this chat is on screen" counts
    only while that window has the focus, not while it is merely drawn under
    a bubble; and nothing clears a chat's notification while that chat's own
    window exists, which the app now tracks itself rather than asking Android
    whether the notification is a bubble. Each time the app does take a
    bubble down it writes a line to the log in App settings, with the reason.
    Unexplained: when this happened, the chat was also missing from the live
    card. If it recurs with no line in the log, the app did not close it.
  - **The live card has a Bubble button** beside Open chat, for the same chat
    (the first line of the card). It goes through `BubbleActivity`, a window
    that shows nothing and finishes at once, so that the app is in front
    when the bubble is posted and Android may open it expanded. Android
    draws notification buttons as words on most phones, so the icon it was
    given (`ic_bubble`) will probably not be what shows. Whether Samsung
    expands the bubble or only shows its head is not known.
- **Project icons (server 1.20.0-dev.13, app 1.0.0-dev.8), not yet seen on a
  phone.** A project with an obvious icon in its folder wears it in place of
  its letters: sidebar and top bar of the main window, the compact chat's
  header, a bubble's head, the chat's picture on a notification, the live
  card. `src/project-icon.js` looks in the usual folders for the usual names
  (icon, favicon, apple-touch-icon, app icon, logo), and takes only a real
  picture that is square and not tiny, so a wordmark or a screenshot is
  passed over. It looks each time a chat is opened in the project, and once
  for a project never looked at; the full path found is kept in
  `project-icons.json` beside the project list. The address handed out ends
  in the file's time and size, which is how a changed icon is noticed. A
  `.ico` leaves the server as a PNG (the one inside it, or its bitmap
  converted in memory; nothing is written into the project). The phone
  fetches an address once, as its own login, keeps one file per project
  (`Icons.kt`), and draws only from what is kept. Everywhere, a missing,
  removed or unreadable icon falls back to the letters. A drawing (.svg)
  shows in the pages but not on the phone, which cannot draw one. With an
  icon, every chat of a project has the same bubble head; the letters that
  told them apart are gone there. The live card has NO large picture: that is a second
  picture on the right, and the owner wanted the one on the left changed,
  not another added. The one on the left is drawn by the phone from the
  app's own icon and there is no documented way to change it. As a guess,
  the project's icon is passed in two of One UI's own extras
  (`android.ongoingActivityNoti.chipIcon`, `.nowbarIcon`); whether they do
  anything is not known.
- **Next:** see how the live card looks on the phone with 1.0.0-dev.8.

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
