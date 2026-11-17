# Phase 6: saved passwords, the homepool node, Cru on the PC

Three PC-client features the native client (Rust + Slint + CEF) must carry
over from the Electron client (`app/`). Each section says what the feature
does, where its state lives, what goes over the wire, the exact wording, and
what to build natively. Line refs are to this tree at the time of writing.

Rule 2 of DESIGN.md applies: every size and colour is a theme token. Icons
come from the flat icon set, never emoji.

Two things are easy to mix up. The **PIN** is the server's one owner PIN
(kid mode, the PC lock, showing saved passwords). The **homepool node**
(`homepool-node.js`) lends the PC's connection to the server. That is a
different thing from `net-tunnel-client.js`, which carries a *drawer tab's*
traffic out through the server or a homepool network (§B.5).

---

## A. Saved passwords (browser drawer) with the PIN

### A.1 Where they live

- **Local only, never on the server.** The file is
  `<userData>/drawer-passwords.json`, mode 0600, written atomically through
  `.tmp` + rename (`app/drawer-passwords.js:56-60`; path at
  `app/main.js:2237-2245`).
- The file holds `{ v: 1, logins: [...], never: [origin...] }`. Each login is
  `{ id (uuid), origin, username, pw (base64 of the OS-encrypted bytes),
  created, updated, used }` (ms timestamps) (`drawer-passwords.js:46-124`).
- Only the password is encrypted, with Electron `safeStorage`
  (`main.js:2241-2243`): DPAPI on Windows, Keychain on macOS, the desktop
  keyring (libsecret / KWallet) on Linux.
- **Origin** means scheme + host + port, http/https only, compared exactly
  (`originOf`, `drawer-passwords.js:35-41`).
- When storage is unavailable (`safeStorageBackend`, `:127-136`), the reasons
  are shown word for word:
  - "This computer offers no encrypted storage for this app, so passwords cannot be saved."
  - Linux `basic_text`/`unknown` backend: "No desktop keyring (GNOME Keyring or KWallet) is running, so passwords could not be kept safely and are not saved."

**Native:** use the `keyring` crate (Windows Credential Manager / DPAPI, macOS
Keychain, Secret Service on Linux). Store one random 256-bit key under
service `Crundi Client`, account `drawer-passwords`, and AES-GCM each password
with it into the same JSON shape (`pw` = base64(nonce‖ct)). If no Secret
Service is running on Linux, report `available:false` with the keyring
reason above. Never fall back to plaintext.

**Migration:** existing files hold Chromium-OSCrypt blobs (`v10`/`v11`
prefix). Reading them means re-implementing OSCrypt, which needs the
"Crundi Safe Storage" keyring secret on Linux and macOS, or the DPAPI key in
Electron's `Local State` on Windows. That is optional. If it is skipped, an
old file is ignored (not deleted), and Passwords says the old ones could not
be read. Mark this as a known gap if it is not done.

### A.2 Store operations (port as a pure module + tests)

These come from `drawer-passwords.js:65-123`.
- `list(origin?)`: no passwords in the result, sorted by origin, then username.
- `count(origin)`
- `check(origin, user, pw)` returns `never` | `new` | `same` | `update`.
- `save({origin, username, password})`:
  - Upserts by (origin, username).
  - Removes the origin from `never`.
  - Errors: "not a site", "no password".
- `password(id)` gives `{origin, username, password}`; error: "No such saved login".
- `used(id)`
- `update(id, {username?, password?})`; error: "That site already has a login with this username".
- `remove(id)`, `never(origin)`, `neverList()`, `unnever(origin)`, `clear()`.

### A.3 The page scripts (run in an isolated world)

- World name `crundi-pw` (`:32`).
- `captureScript(prefix)` (`:157-213`) is injected into every drawer tab with
  CDP `Page.enable` + `Page.addScriptToEvaluateOnNewDocument {source,
  worldName:'crundi-pw', runImmediately:true}` (`app/browser-drawer.js:652-661`).
  Do **not** use `Runtime.enable`: anti-bot scripts detect it.
- The script reports through `console.debug(prefix + JSON)`. The prefix is
  `"⁣crundi-pw:" + uuid + ":"`, random for each app run
  (`browser-drawer.js:651`). The host takes these lines out of the tab's
  console before anything (a chat's `browser_console`) can read them (`:228`).
- Messages always carry `origin` (location.origin) and `top` (is it the top
  frame?):
  - `{t:'page', hasLogin}`: sent at load, and again when a visible
    `input[type=password]` appears or goes. A MutationObserver re-checks,
    debounced 400 ms.
  - `{t:'submit', username, password}`: sent on a form `submit`, on a click on
    `button, input[type=submit|button], [role=button]`, or on Enter in a
    password field.
    - The password is the **last** filled visible password field (so a
      sign-up or change form gives the new one), max 1000 chars.
    - The username is the nearest filled text/email/tel input before it (else
      the first one), max 300 chars.
    - A pair identical to the last one sent is not sent again.
  - `{t:'after', stillForm}`: sent after a submit, polling every 500 ms for up
    to 8 s, saying whether the login form has gone.
- `fillScript(origin, user, pw)` (`:220-246`) runs in an isolated world (Electron:
  `executeJavaScriptInIsolatedWorld(1999, …)`, `browser-drawer.js:736`).
  - If `location.origin !== o`, it returns `{ok:false, error:'The page is no longer on that site'}`.
  - Otherwise it sets the first visible, enabled password field, and the
    username field (the input before it, else one matching
    `/user|email|login|account|name|phone/`). It uses the native value
    setter + `input` + `change` so React and Vue pick the value up.
  - With neither field: "There is no login form on this page".
- `redactSecrets` (`:254-266`): every password a tab was given or typed (kept
  per tab in `pwSecrets`, `browser-drawer.js:676,735`) is replaced by
  `••••••••` in **anything a chat reads back** from that tab (eval, text,
  HTML, console, network). Binary keys (`data|image|png|pdf`) are left alone.

**Native (CEF):** inject via `CefBrowserHost::ExecuteDevToolsMethod`
(`Page.enable`, then `Page.addScriptToEvaluateOnNewDocument` with
`worldName`). Read reports from `CefDisplayHandler::OnConsoleMessage`. Verify
that isolated-world console lines reach it; if not, use a
`CefRenderProcessHandler` + process message. Fill with `Page.createIsolatedWorld`
(→ `executionContextId`), then `Runtime.evaluate {contextId,
returnByValue:true}`; neither needs `Runtime.enable`. Apply the redaction
in the native host's browser-tool answers (the same place
`browserDrawer.redact` is called, `main.js:2027`).

### A.4 Deciding to offer a save (`browser-drawer.js:667-707`)

- Only top-frame reports whose origin is the tab's current origin count.
- **On `submit`:**
  - Add the password to `pwSecrets`.
  - Run `check`. `never` or `same` ends it here.
  - Otherwise keep a *pending* entry `{origin, username, password, kind}`,
    with a verdict timer of `PW_VERDICT_MS = 20000`. When the timer fires, the
    verdict is "worked" for `new` and "failed" for `update`.
- **Verdict "worked":**
  - `{t:'after', stillForm:false}`, or
  - a `page` with `hasLogin:false`.
- **Verdict "failed":**
  - `stillForm:true`, or
  - a `page` with `hasLogin:true` after a navigation (`did-navigate` sets
    `navigated`, `:218`).
  - The log line is `login on <origin> looks refused: nothing offered`.
- **Worked:** emit offer `{offerId, tabId, origin, username, kind,
  available, reason}`. Offers are kept for `PW_OFFER_MS = 5 min`; the password
  never goes to the page.
- **Fill offer:** a `page` with `hasLogin:true` and nothing pending, on a site
  that has saved logins and was not dismissed this run (`pwFillDismissed`).
  Emit `{tabId, origin, logins:[{id, username}]}`.
- The tab's state carries `logins` = `count(origin)` (`:173`). That count
  shows the lock button.

### A.5 IPC surface (preload `drawer.pw`, `app/preload.js:137-152`)

All calls are refused unless they come from the main window's page: `{ok:false,
error:"not the drawer's page"}`. With no store: "Saved passwords are not
available in this app." Results are `{ok:true, …}` or `{ok:false, error}`
(`browser-drawer.js:709-747`).

| call | answer |
|---|---|
| `status()` | `{available, reason, count}` |
| `list()` | `{logins, never, available, reason}` |
| `answer(offerId, 'save'\|'never'\|'dismiss', username?)` | `{done:'saved'\|'updated'\|'never'\|'dismissed', login?}`; error "That offer has expired" |
| `fill(tabId, id)` | `{filled:{user, password}}`; errors "No such tab", "This tab is not on <origin>", "Could not fill this page" |
| `dismissFill(origin)` | `{}` (not offered again until restart) |
| `reveal(id)` | `{password}` (the page has checked the PIN first) |
| `update / remove / unnever / clear` | as the store |
| events | `onOffer`, `onFillOffer` |

Natively these become calls inside the process. The *gate* is the same: only
the app's own UI may call them, never a chat's browser tool.

### A.6 The UI (`app/vendor/crundi-browser-drawer.js`)

**Lock button** in the address bar (`:278`):
- Shown when the active tab's `logins > 0`.
- Title: "A saved login for this site", or "<n> saved logins for this site".
- Click (`pwSiteMenu`, `:1381-1393`):
  - none: toast "No saved login for this site";
  - one: fill it;
  - several: the fill bar with a picker.

**Prompt bar** under the address bar, per tab (`renderPw`, `:1305-1338`).
The bar is dropped once the tab leaves that origin.
- Save offer: [lock] "Save the password for <host>?" + a Username input
  (placeholder "Username", prefilled) + **Save** (primary) + **Never** (title
  "Never offer to save passwords for this site") + × (title "Not now").
- Update offer: "Update the saved password for **<user or 'this login'>** on
  <host>?" + **Update** + ×. There is no Never and no username field.
- Fill offer: "Fill your saved login for <host>?" + the username in bold, or a
  `<select>` of usernames ("(no username)" when one is empty) + **Fill** + ×
  (title "Not for this site (until the app restarts)").
- An offer replaces any fill bar. A fill offer is ignored while an offer is
  showing.
- Enter in the username field = Save.

**Save flow** (`:1359-1368`):
1. `GET /api/lock`. If `!pinSet`, toast (error) "Set your PIN first
   (Settings, Voice assistant, Your PIN): it protects your saved passwords."
   and keep the bar.
2. Otherwise call `answer(...,'save', username)`, then toast "Password saved" /
   "Password updated", or the error ("Not saved").

**Never** toast: "Passwords for <host> will not be offered again (Passwords, in the menu, undoes it)".

**Offer with `available:false`:** toast (error) "A login was not saved: <reason>" (`:1373`).

**Menu, Passwords** (`:1060`, `showPasswords` `:1399-1419`):
- Panel title "Saved passwords" + close ×.
- Note: "Kept on this computer only, encrypted for your account on it, as Chrome keeps them. Filling needs no PIN; showing one does. Chats can never read them."
- When unavailable, the reason comes first.
- Rows: username (or "(no username)") with the host small under it, then
  `••••••••`, **Show**, and a trash button (confirm "Delete this saved password?").
- Empty: "No saved passwords yet. Sign in to a site in a drawer tab and choose Save."
- Then "Never offered for:" with a row per host and **Remove**.
- **Show:**
  - If the PIN was given less than 5 min ago (`pwUnlocked`, in memory only),
    reveal at once.
  - Otherwise show the PIN row: a password input (numeric, maxlength 8,
    placeholder "PIN") + **Unlock**; Enter = Unlock.
  - A wrong PIN gives a toast with the server's error, else "That is not the PIN".
  - Revealed: the value replaces the dots, and the button says **Hide**.
  - Reveal failed: "Could not show it".

**Clear browsing data** (`:1096-1106`):
- Checkbox "Saved passwords", sub-line "Every saved login, on every site (not limited to one site)".
- Confirm "Delete every saved password on this computer?", then toast "Saved passwords deleted".
- With nothing ticked: toast "Choose something to clear".

### A.7 The PIN (server side, shared with kid mode and the PC lock)

- **Set / change:** `POST /api/voice/kid/pin {pin}` (4-8 digits,
  `PIN_RE /^\d{4,8}$/`, `src/voice/kid.js:29`), or `{remove:true}`
  (`src/webapp.js:5501-5511`).
  - Errors: "The PIN is 4 to 8 digits"; 423 "The PIN cannot change while kid mode is on".
- **UI:** Settings › Voice assistant › "Your PIN and the PC lock"
  (`src/webapp-html.js:5902, 6266-6300`).
  - Input placeholder "4-8 digits" / "New PIN (4-8 digits)", button **Set PIN** / **Change PIN**.
  - Hint: "One PIN for kid mode and the PC lock. Five wrong tries lock it for a minute, then longer."
- **State:** `GET /api/lock` gives `{ok, on, mode, since, by, pinSet, auto, autoMinutes}`
  (`webapp.js:3431, 5514`).
- **Check (no side effects):** `POST /api/lock/check {pin}` (`webapp.js:5532-5539`) answers:
  - `{ok:true}`
  - 400 `{noPin:true, error:"Set your PIN first (Settings, Voice assistant, Your PIN)."}`
  - 403 `{triesLeft, error:"That is not the PIN"}`
  - 429 `{locked, retryInMs, error:"Too many tries. Try again in <n> seconds."}`
- **Guard** (`createPinGuard`, `kid.js:48-75`): `MAX_TRIES = 5`, then a lockout
  of 60 s × 2^(n-1), at most 1 h. The counter is shared with the PC lock and
  kid mode.
- The native client only needs `GET /api/lock` and `POST /api/lock/check`
  here. The PIN settings UI belongs to the Settings spec.

---

## B. The homepool node (this PC lends its connection)

### B.1 What it is

The PC becomes a homepool **exit**. The server asks it, stream by stream, to
open outbound TCP connections, and pipes bytes through one websocket. The
node only dials out, listens on nothing, and resolves DNS locally. It
**refuses private targets** (`isPrivateTarget`, `app/homepool-node.js:79-91`):
- `localhost`, `*.localhost`, `*.local`, `*.internal`;
- 10/8, 127/8, 0/8, 169.254/16, 172.16/12, 192.168/16, 100.64/10, ≥224;
- IPv6 `::1`, `::`, fc/fd, fe80.

The reply is `ERROR "private address refused by the node"`. The same codec
runs in the Docker node, on Android (`NodeLink.kt`), and on the server
(`src/homepool.js`).

### B.2 Protocol (`homepool-node.js:16-30`)

- **URL:** `ws(s)://<server>/api/homepool/node?key=<hpn_…>`. The key is also
  sent as `Authorization: Bearer <key>` where headers are possible (`:216-219`).
- **Binary frames:** `[type u8][stream id u32 BE][payload]`.
  - `OPEN=1` (server→node, JSON `{"host","port"}`)
  - `DATA=2` (both ways)
  - `CLOSE=3` (both ways)
  - `OPENED=4` (node→server)
  - `ERROR=5` (node→server, utf8 reason: `bad target`, the socket error code, …)
  - `PAUSE=6` / `RESUME=7` (both ways, flow control)
- **Text frames, server → node:**
  - `{type:'hello', node:{id,name,network,networkName}, bindTtlMs}`
  - `{type:'stats', conns, slots, bytesToday, connsToday, keys, grants}`, about every 10 s
  - `{type:'bye', reason, final?}`. With `final` the node stops and shows `refused`.
- **Text frames, node → server:** `{type:'info', platform, app, version,
  network, publicIp, paused, pauseReason, bytesToday, mobileBytesToday}`.
  - Sent on open and every 30 s.
  - The PC sends `platform` `windows|linux|darwin`, `app:'crundi-desktop'`,
    `version`, `network:'PC'`, `publicIp` (`main.js:1589`).
- **Flow control** (`:144-172`):
  - Once websocket `bufferedAmount` passes `WS_HIGH_WATER = 1 MiB`, pause the
    TCP read; resume below 512 KiB (polled every 20 ms).
  - When a TCP write is full, send `PAUSE`; on drain, send `RESUME`.
  - On received `CLOSE`: `end()`, then destroy after 5 s.
- **Reconnect:** backoff 1 s doubling to 60 s.
  - HTTP 401/403 on the upgrade means **refused** (final: setting
    `enabled=false`).
  - Every stream is closed when the socket drops.
  - `kick()` reconnects at once.
- **States:** `connecting` (detail "retrying in Ns"), `connected`, `stopped`, `refused`.
- **Public IP:** fetched from `https://api.ipify.org` every 10 min
  (`main.js:1578-1583`). A change resends `info`.

### B.3 Local state (`main.js:1550-1627`)

- **File:** `<userData>/homepool-node.json` (0600):
  `{deviceId, server, key, enabled}`. `deviceId` is `"pc-" + random + time`,
  made once.
- **The server is told the deviceId:** the bundled server's env
  `CRUNDI_DEVICE_ID` (`main.js:818-819`), and the drawer's
  `/api/homepool/browser?device=` (`crundi-browser-drawer.js:480`). The server
  then never offers this PC's browser a network the PC itself is a node of.
- **Startup and quit:** start at launch when `enabled`; stop on quit.
- **Activity log:** the last 60 lines kept, 25 returned, stamped `HH:MM:SS`.
- **`status()`:** `{supported:true, configured, enabled, server, deviceId, name
  (hostname), state ('off' when not enabled), connectedAt, node, streams,
  bytesUp, bytesDown, opened, serverView (last stats), publicIp, log}`.
- **Calls:**
  - `share({server,key})`: needs `https?://` and `hpn_`, else "Bad node
    settings."; saves with `enabled:true`, then starts.
  - `start()`: errors "Not set up." without a key.
  - `stop()`: sets `enabled:false`.
  - `forget()`: keeps only `deviceId`.
- **Native:** a `homepool.rs` with the codec (pure, tested against
  `homepool-node.js` vectors) and a thread or tokio task per node. The
  tungstenite client is already a dependency. The node code is a
  `crundi-homepool://node?s=<url>&k=<key>` URI (`:61-75`); the PC flow does
  not need it.

### B.4 UI: the "This PC" card in the Homepool tab

The card is in `app/vendor/crundi-homepool.js:213-258` and only shows in the
desktop app. The tab polls `status()` every 3 s (`:128, :565`). It is only
offered when the server lists the `homepool` capability
(`src/client-version.js:298-308`).

- **Header:** "This PC" + the hostname.
- **Server on this machine:** "This PC runs the server, so its connection is already the Server network."
- **Not configured:** chip "not shared", then the text
  "Lend this PC’s internet connection to the server as a node. Only keys you give the network you put it in can use it, and it never opens anything on this PC’s own network."
  - A network select: "No network yet" + the owner's networks, `home` preselected.
  - Primary button **Share this PC’s connection**:
    1. `POST /api/homepool {action:'self-node', deviceId, name, network}` →
       `{node:{id,key,…}}` (`src/homepool-service.js:140-153`; it reissues the
       key if this device already has a node).
    2. `share({server: <origin>, key})`.
    3. Toast "This PC is now a node". On failure: "The desktop app could not start the node".
- **Configured:**
  - State chip: "connected" (ok) / "connecting" (warn) / "the server refused this node key" (bad) / "stopped".
  - **Stop** or **Start** (primary).
  - **Stop sharing** (danger): confirm "Stop sharing this PC?" / "The node
    stops, its key is removed from this PC and from the server." / button
    "Stop sharing". Then `forget()` + `POST {action:'node-remove', id}`, toast
    "This PC no longer shares its connection".
  - Meta line, joined by " · ":
    - "node <name> in <network>", "IP <ip>", "up <dur>", "<n> open now"
    - "<bytes> since start", "<n> slot(s) carried"
    - "<bytes> / <n> connections today", "used by <keys>"
  - **Activity** / **Hide activity** toggles a log box ("Nothing yet." when empty).

If the native Homepool tab is still the web view, it must offer the same
`window.api.homepool` bridge. If it is native, port this card.

### B.5 Not the node: `net-tunnel-client.js` (drawer networks)

`createServerNetProxy` (`app/net-tunnel-client.js:56-188`) is the **browser
drawer's** "Server network" / `hp:<net>` network.

- **Proxy:** a loopback HTTP proxy on a random `127.0.0.1` port, with a
  per-launch Basic password (user `crundi`). It handles `CONNECT` and
  absolute-form http, one request per connection.
- **Frames:** each connection becomes a stream (`OPEN/DATA/CLOSE/OPENED/ERROR`,
  1-5, same layout) over the native-host websocket, or over IPC to a bundled
  server.
- **Targets:** `hp|<net>|` is put before targets that should leave through a
  homepool network.
- **Stream ids:** each proxy takes ids in its own range (`idBase<<24`); frames
  are routed back by `idRange`. `OPEN_TIMEOUT_MS = 20 s`.
- **Error pages** (heading "Server network" or the network's name):
  - 407 "Sign-in needed."
  - 502 "This tab goes out through the server, and the desktop app is not connected to the server right now."
  - 502 "No exit is up: <why>. The page loads once a node of this network is online." (title "No exit is up")
  - 502 "The server could not connect: <why>"
  - 400 "Only http and https are carried."
- Wiring is in `app/drawer-net.js` and `main.js:2200-2226`. Chromium's proxy
  login is answered only for our own proxy (`isOurs`).

This belongs to the browser drawer spec. In CEF, set a per-`CefRequestContext`
proxy and answer `GetAuthCredentials(isProxy)`.

---

## C. Cru on the PC

### C.1 What and where

Cru is the voice assistant: a full voice screen (an overlay over the
workbench) that talks to the server's Cru brain. The same script runs on web
and PC: `app/vendor/crundi-voice.js` (2743 lines), `window.CrundiVoice`. The
PC adds two things: **"Hey Cru"** (`app/voice-wake/`) and voice reports that
come to the front.

**How it opens:**
- **Top bar button** `#voice-btn` (`src/webapp-html.js:3696`, title "Talk to
  Cru (voice assistant)"). `attach()` (`crundi-voice.js:2643-2678`) replaces
  its mic icon with Cru's mark (`cruIcon`, `:2612`) and adds a red unread dot
  (`GET /api/voice/unread` → `{count}`; `POST /api/voice/seen`).
  - **Click** opens (`how:'click'`, stays open); click again closes.
  - **Hover** 350 ms (only at width ≥ 900, and not while a chat is in voice
    mode) opens with `how:'hover'`. It closes 700 ms after the pointer leaves,
    if idle (`:739-747`).
  - Collaborators never get Cru (`webapp-html.js:5756`).
- **Push to talk:** hold **Ctrl+Space**. It can be rebound per device
  (localStorage `crundi_voice_ptt` = `{ctrl,alt,shift,meta,code}`; label like
  "Ctrl+Space", `Win`/`Cmd` for meta) (`:2680-2709`). It is window-level,
  not global.
  - Press: open with `how:'ptt'`, or barge in if Cru is speaking.
  - Release (key or any needed modifier): after 700 ms, send what was heard if
    no utterance was sent (`pttRelease`, `:750-758`).
  - Settings › Voice assistant › "On this device": "Push to talk" `<kbd>` +
    **Change** / **Reset** (`webapp-html.js:15713-15722`).
- **"Hey Cru"** (C.6).
- **Voice reports:** the workbench `/ws` message `{type:'voice-report', speak,
  show, media, needsOwner}`. With `needsOwner`, show and focus the window.
  Then `CrundiVoice.report()` opens the screen with `reportOnly` and says it.
  It closes again 8 s after (`REPORT_LINGER_MS`) unless the owner talks
  (`webapp-html.js:10493-10499`, `crundi-voice.js:2712-2716`).
- **Elsewhere:** the drawer's "Send to Cru" calls `CrundiVoice.share(text,
  files)`. Dragged references (kanban card…) go through `dropRef`.

### C.2 The screen (`open`, `crundi-voice.js:560-737`; render `:2392-2420`)

On a wide screen the screen is a dropdown anchored under the button
(`--cv-top/--cv-right`); under 900 px it is full screen. Its parts, top to
bottom:

- **Top row:**
  - Cru wordmark + usage legend: "wk N%", "5h N%", "Claude $x / $limit" (API
    brain only), "Deepgram $x / $limit".
  - State pill: **Waking / Listening / Thinking / Speaking / Asleep**.
  - Admin shield, shown when `settings.adminMode`. Title "Admin mode is on: Cru can run any command. Turn it off in Settings, or say: admin mode off".
  - "Normal view" (only in full view of Cru's screen).
  - **More** (⋯) menu:
    - **Chat view**
    - Cru's voice off (title "Replies as text only, until turned back on"; localStorage `crundi_cru_voice_off`)
    - **Compact the conversation**. It asks "Compact the conversation? Cru carries on from a summary; its notebook and memory search keep everything." [Compact] [Cancel]. Then `POST /api/voice/compact`.
      - Notices: "Compacting the conversation…"; "Compacted: Cru carries on from a summary, its notebook kept."; "Not compacted: <why>".
  - Close × (title "Close (Esc)").
- **Field:** a canvas with the Crundi head inside a contour visualiser that
  follows the live voice. The usage arcs sit in its disc, and the projects
  are cells (max 12) coloured by kind:
  - working = yellow, waiting = sky, needs = hover-indigo, error = red,
    running = green, idle = muted;
  - joined by strands.
  - Palettes per phase: `PALETTE` `:51`.
- **Cru's live panel** (`.cv-scr`, capability `voice-screen`): a title, then
  fold / full view / close.
  - Body: blocks of type heading, text, list, checklist, table, progress,
    image (https/data only), chips (`blocksHtml` `:2112-2124`).
  - Driven by the `screen` socket message and `GET /api/voice/screen`.
  - Close / normal / full: `POST /api/voice/screen/close|normal|full`.
- **Words:**
  - Heard text in quotes, or "Ask me anything, Sir." when idle.
  - × "Cancel what I said" → `POST /api/voice/cancel`. Its notice is
    "Cancelled." / "Cancelled. Already done: a; b."
  - "Queued, answered next: “…”".
  - Reply text (scrolls along with the speech), detail (`show`), Yes/No
    buttons for `pending[0].question`, notice line.
- **Chat view** (default on; localStorage `crundi_cru_chat_view`):
  - `GET /api/voice/history?limit=40[&before=ms]` → `{turns:[{at, said,
    speak, show, media, updates}], more}`; "Earlier messages" loads more.
  - Each Cru bubble has a speaker button that says it again.
- **Bottom row:**
  - Attach (paperclip, also drop and paste).
  - The **mic blob** (a canvas). Its colours give the state (`MIC_COLORS`
    `:58`): listen sky, think violet, speak green, waking indigo, asleep grey,
    offline amber, problem red.
  - A status word at the bottom right, only when the colour isn't enough:
    **Unavailable**, **Offline**, **Waking**, **Asleep**.
  - Keyboard toggle: title "Keyboard mode: type to Cru, no microphone" / "Back
    to voice"; localStorage `crundi_cru_keyboard`. In keyboard mode a textarea
    grows up to 160 px. Enter sends, Shift+Enter is a new line.
  - Hint "Hold <shortcut> to talk · click the mic to keep this open".
- **Keys while open:** Esc closes. Keys inside the screen do not reach the
  page. When docked at width ≥ 900, keys outside it are the page's.
- **Kid mode** (`opts.kid`): its own palette, a big cursor, a PIN pad to
  leave (`/api/voice/kid/exit`), and games. Out of scope here except "the
  screen cannot be closed".
- **PC lock on:** the chat view is hidden.

### C.3 Endpoints and the listen socket

| call | use |
|---|---|
| `POST /api/voice/warm` | on open |
| `GET /api/voice/settings` | `{ok, anthropicKeySet, brain ('api'\|'claude-code'), deepgramKeySet, deepgramLimitReached, ttsVoice, speechRate, volume, screenCloseSeconds, adminMode, spend:{claudeUsd, deepgramUsd}, monthlyLimitUsd, deepgramLimitUsd, …}` |
| `POST /api/voice/turn` | `{text, where, page, locked:false, typed, attachments:[id], refs:[str], requestId:'w…', sayNow}` |
| `POST /api/voice/speak?stream=1` | `{text, expressivity?}`, `Accept: audio/L16` |
| `POST /api/voice/attach` | `{name, type, data(b64), preview?(jpeg b64, ≤1568 px), from:'web'}` → `{id, kind, name}`; max 50 MB, 10 files |
| `POST /api/voice/cancel`, `/compact`, `/update/ack {id, how:'shown'\|'said'}` | |
| `GET /api/voice/history`, `/unread`; `POST /seen` | |

The other endpoints come from `crundi-voice.js:853-1880`.

- **`/api/voice/turn` answer:** `{ok, speak, show, media, pending:[{question}],
  close, updates:[{id,speak}], volume, expressivity, speaker:{unrecognised},
  phoneActions, cancelled, acted}`.
  - `close` = say the goodbye, then close.
  - `speaker.unrecognised` shows the notice "Cru could not recognise who was speaking, so nothing was changed."
  - Errors show `error`, else "Something went wrong".
- **`where`:** `"web (<host>), <ui>"`, at most 200 chars. Native should send
  `"pc (<host>), project x, kanban tab"` only after checking the server's
  device check (`src/voice/device.js` reads the leading word). Until then,
  keep `web`.
- **Blocked screen texts:**
  - "The voice assistant is not available on this server."
  - "There is no Claude API key on the server yet. Add one in Settings, Voice assistant." (shown when `!anthropicKeySet` and the brain is not `claude-code`)
- **No Deepgram key, or over its limit:** keyboard mode, with
  - "Voice needs a Deepgram key: add one in Settings, Voice assistant."
  - "Voice is off: this month's Deepgram limit is reached."
- **No microphone:** "The microphone is not allowed here, so it is keyboard mode. Allow the microphone for this site to talk."
- **Listen socket:** `ws(s)://<host>/api/voice/listen?encoding=linear16&sample_rate=16000&token=<t>`.
  - Keyboard mode uses `?keyboard=1&token=` (no audio; it carries reports and
    settings) (`:913, :995`).
  - **Up:** binary 16 kHz mono Int16 LE (a box-filter downsample, a
    ScriptProcessor of 4096). Text: `stop`, `pause`, `resume`, `discard`,
    `saying {text}`, `said`.
  - **Down** (`heard`, `:1000-1035`):
    - `ready {echoFilter}` (echoFilter = live barge-in)
    - `interim {text}`, `utterance {text, held?}`, `speech`, `discard`, `error {error}`
    - `update {id, requestId, speak}` (say_now lines during a turn)
    - `report {speak, show, media}`
    - `settings`, `screen {screen}`, `kid`, `game-command`, `say`
- **Audio out:** `audio/L16; rate=N; channels=1`, rate also in `X-Sample-Rate`
  (24000 usually) (`src/webapp.js:5842-5872`). Play as it streams.
  - With no `ttsVoice`, or when the stream fails, use the device voice
    (`browserSpeech`, en-GB).
- **Tones** (`TONES`, `:1844-1848`), sine with exponential envelopes:
  - listen: 659.25 Hz (0-0.11 s), then 880 Hz (+0.09 s, 0.16 s)
  - sent: 880, then 659.25
  - think: a soft 523.25 + 783.99 Hz chord, 0.9 s, repeated every 4 s while thinking (first after 1.4 s)
  - Spoken cues are retired (`loadCues` is a no-op).

### C.4 Listening behaviour to keep

- **Turn-taking:**
  - Utterance → `ask()`, phase `think`.
  - Answer → `speak()`, phase `speak`.
  - Then `listen`. Typed turns are read, not said; "voice off" is never said.
- **Barge-in:**
  - With `echoFilter`: while Cru speaks, send `saying`, and hold the mic
    locally (nothing is sent). A local gate decides whether the owner is
    talking: `createGate` (`:162-200`) and Silero VAD v5 on
    `/vendor/silero-vad-16k.bin` (`:203-345`), with SPEECH_MS 250 and a
    ≥10 dB swing.
  - On his voice: stop Cru, send a 1300 ms pre-roll (from 600 ms before the
    rise), then `said`/`resume`.
  - Without `echoFilter`: `pause` while speaking.
  - Echo / talk-over word rules: `isEcho`, `talksOver`, `STOP_WORDS`;
    "scratch that / never mind / ignore that" discard (`:80-112`).
- **Asleep:** after 30 s of silence (`SILENCE_MS`) the mic is released
  (phase `ended`, "Asleep"). A tap, the shortcut, or "Hey Cru" wakes it.
- **Closing:** after `settings.screenCloseSeconds` (default 60) idle the
  screen closes (`tick`, `:2086-2104`). A shown panel keeps it open.
- **Wake lock:** the screen is kept on while Cru is open.

### C.5 Native: what to reuse

`native-client/src/voice.rs` (chat voice, pure) and `voiceio.rs` (threads)
already cover most of the plumbing:
- **Reuse as is:**
  - `listen_url`, `Down` (16 kHz Int16 LE box filter), `mono`, `level`
  - `parse` → `Msg` (extend with `Update`, `Report`, `Speech`, `Settings`,
    `Screen`, `Ready{echo_filter}`; today these fall into `Other`)
  - `speak_rate` (reads `X-Sample-Rate`), `sentences`, `say_cap_ms`
  - `voiceio::Listen` (cpal capture + tungstenite socket; `Ctl::{Stop,
    Discard, Pause, Resume, Saying, Said}` already match Cru's verbs)
  - `voiceio::speaker` (rodio, `Begin/Pcm/End/Pause/Resume/Stop`)
  - `device_voice` (the `tts` crate) for the no-Deepgram fallback.
- **Add:**
  - Cru's three tones (`voice::tone` has only sent/listen, and its shape
    differs; port `TONES` exactly).
  - The `/api/voice/turn` client and the turn state machine (phases, pending,
    requestId, update acks, cancel).
  - Keyboard mode's report-only socket (`keyboard=1`: open without capture).
  - Attachments.
  - Barge-in: the gate (pure, port with tests) and Silero VAD (run with `ort`
    on the same `.bin`, or the ONNX model; or ship without barge-in at first
    and `pause` while speaking, as an old server does).
  - Echo cancellation: cpal gives raw input, with no AEC like Chrome's WebRTC
    loopback (`makeLoopback`, `:386`). Without it, keep the `pause`-while-speaking
    path as the default and enable live barge-in only when the gate proves
    reliable.
  - The Slint overlay: field canvas, blob, pill, words, chat view, panel blocks.
  - The PTT shortcut (Slint key events, window-level), the hover-open, and
    the unread dot.
- **Hand-over with a chat's voice mode:** only one mic owner at a time (the
  web's `CrundiChat.voiceActive`). See `specs/voice-input.md` §7.

### C.6 "Hey Cru" (`app/voice-wake/`, wired at `app/main.js:1467-1546`)

- **Engine:**
  - sherpa-onnx keyword spotter: the WASM build for Node, run in a utility
    process (`worker.js`, `spotter.js`).
  - Model: `sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01`.
  - Keyword: BPE `▁HE Y ▁C RU @HEY_CRU` (+ spellings `K RU`, `C RE W`, and
    decoys, `enroll.js:45-88`).
  - `TUNING`: score 1.5, threshold 0.2, cooldown 2500 ms (`manifest.js`).
  - Engine and model are downloaded on first use into `<userData>/voice-wake/`
    and checked against pinned SHA-256s (`manifest.js:26-58`, `install.js`).
- **Owner check:** the 3D-Speaker ERes2Net voiceprint
  (`eres2net-voxceleb`, 26 MB, dim 192) via onnxruntime-node. A wake counts
  when the mean of the 3 best cosines against the 5 training takes is ≥ 0.42
  (`owner.js:27-31`, `voiceprint.js`). Without the model there is no verdict
  and the wake stands.
- **Training:** 5 samples, max 10 s each.
  - The page records each sample itself after `trainBegin()` (which releases
    the listener's mic), checks each with `check()`, then `train(all)` saves
    `<userData>/voice-wake/profile.json` (0600). `forget()` deletes it.
  - Wording:
    - "Train Hey Cru first: say it five times so it knows your voice."
    - Buttons **Train Hey Cru** / **Train again**, **Record** / **Stop**.
    - "Click Record, then say “Hey Cru” the way you normally would."
    - "Got it."
    - "<why> Record it again."
    - "Train again so that only your voice wakes Cru: the voice check is not set up for this training."
  - Source: `webapp-html.js:6619-6780`.
- **Listens only when** (`controller.js:1-30`) all of these hold:
  - the setting is on (`client.json` `voiceWake`, off by default) **and** the
    owner is trained;
  - the PC is on AC power;
  - no training is running;
  - the voice screen does not hold the mic;
  - the screen is not locked and the PC is not asleep.
  - Exception: an *asleep* open voice screen is woken by "Hey Cru" even with
    the setting off or on battery.
- **Privacy:** the listener's session refuses every network request and grants
  the mic only to itself.
- **On a hit:**
  - Show and focus the main window.
  - Then `CrundiVoice.wake()` (asleep screen), or `crundiOpenVoice({how:'wake'})`,
    or click `#voice-btn`.
  - A voice mismatch is logged and ignored.
- **Status labels** (`controller.js:52-62`):
  - "Downloading the wake word model…"
  - "Paused: on battery"
  - "Paused: voice screen open"
  - "Paused: a chat is listening"
  - "Paused: screen locked"
  - "Paused while training"
  - "Starting…"
  - "Listening"
  - "Not listening: <error>"
- **UI:**
  - Settings checkbox "Listen for “Hey Cru” while plugged in" + status text.
  - Tray checkbox with the same label. It is disabled with the sub-item
    "Train it first in Settings, Voice assistant" when untrained
    (`main.js:1523-1533`).
- **Native:**
  - Use sherpa-onnx's C API (the `sherpa-rs` crate, or FFI to the prebuilt
    lib) with the same pinned model, keyword file and tuning, and `ort` for
    the voiceprint.
  - Capture with cpal on a thread of its own, never on the UI thread.
  - Keep the same profile.json format, so a trained owner stays trained. The
    enroll and match logic (`enroll.js`, 663 lines) must be ported with its
    tests (`scripts/test-voice-wake-*.mjs`).
  - This is the largest piece of C. It can ship after the voice screen; the
    settings row is then hidden (as `window.api.voiceWake` absent).

### C.7 Order of work

1. **A:** the store, the scripts and the drawer UI. It needs the drawer's CEF
   tab host.
2. **B:** the node codec + task, and the "This PC" card.
3. **C:** the overlay + turn loop + keyboard mode + PTT. Then barge-in. Then
   "Hey Cru".
