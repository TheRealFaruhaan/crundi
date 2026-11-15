# Inbox, tray, notifications and the chat bubble (PC client)

What the Electron PC client (`app/`) and the page it hosts (`src/webapp-html.js`)
do today for: the system tray, OS notifications, urgent alerts, start-at-login,
close-to-tray, the Inbox, and the chat "bubble" / compact chat page. The native
Rust+Slint client must match the behaviour and wording below (layout and UX,
not pixels; all sizes and colours are theme tokens).

Line numbers are against this worktree at the time of writing.

---

## A. Tray, notifications, alerts, startup

### A.1 What exists and what does not

| Thing | In the Electron client? | Where |
|---|---|---|
| Tray icon + context menu | Yes | `app/main.js:521-618` |
| OS notifications pushed by the server | Yes | `app/main.js:1962-1998` |
| Unread badge on taskbar / dock (`setOverlayIcon`, `app.setBadgeCount`) | **No** | grep finds nothing |
| Tray icon that changes with unread / status | **No** (tooltip text only changes) | `app/main.js:617` |
| Custom notification sounds | **No** (OS default; `silent` never set) | `app/main.js:1978` |
| Urgent alert ringing / attend / snooze UI | **No** — phone only | `src/alert-user.js`, `android/URGENT-ALERTS.md` |
| Floating bubble window | **No** — Android only (see C) | `app/main.js` has no bubble |
| Start at login | Yes | `app/main.js:1202-1235`, `src/webapp-html.js:4704` |
| Close to tray | Yes | `app/main.js:217-225`, `2715-2717` |
| Single instance | Yes | `app/main.js:2660-2671` |

The native client should implement the "Yes" rows. The "No" rows are not
regressions if left out; do not invent them.

### A.2 Tray icon

`createTray()` `app/main.js:523-540`:

- Icon: `assets/icon_32x32.png` (the head, drawn for small sizes), falling
  back to `icon.ico`, resized to **16x16**. Empty image if neither exists.
- Tooltip at creation: `Crundi`; after every menu rebuild:
  `Crundi — <status>` (em dash), status from the table below
  (`app/main.js:617`).
- **Double-click** the icon: show the main window (`showWindow()`).
- **Linux only**: single **click** also shows the window (AppIndicator trays
  may only show the menu; the menu has "Open").
- Created on `app.whenReady` before the window (`app/main.js:2684-2691`); a
  failure is logged (`[lifecycle] Tray error: …`) and the app carries on.

`showWindow()` `app/main.js:124-132`: restore if minimised, show, focus; if the
window does not exist, create it.

### A.3 Tray menu (exact order and wording)

`updateTrayMenu()` `app/main.js:542-618`. Rebuilt whenever server status, kid
lock, voice-wake status or update state changes (`setBotStatus` 624-630, update
code 1091, voice wake 1503/1508, kid lock 462).

Server status labels (`statusLabels`, 545-550): `stopped` → `Stopped`,
`starting` → `Starting...` (three ASCII dots), `running` → `Running`,
`error` → `Error`, anything else → `Unknown`.

| # | Label | Enabled | Action |
|---|---|---|---|
| 1 | `Crundi` | never (heading) | — |
| — | separator | | |
| 2 | `<status label>` | never | — |
| — | separator | | |
| 3 | `Open` | always | `showWindow()` |
| 4 | `Lock (visible)` accel `CommandOrControl+Shift+L` | `!kidLocked` | `pcLockAsk('visible')` |
| 5 | `Lock (full)` accel `Super+Shift+L` | `!kidLocked` | `pcLockAsk('full')` |
| 6 | `Kid mode` (shows `Locked` while locked) | `!kidLocked` | show window, send `kid:start` to page |
| 7 | `Server…` (ellipsis char) | always | show window; if the page is not the local shell (`file://`), reload `index.html` first; then send `show:server` |
| — | separator | | |
| 8 | `Start` | status `stopped` or `error` | `startBot()` |
| 9 | `Stop` | `!kidLocked` and status `running`/`starting` | `stopBot()` |
| — | separator | | |
| 10+ | voice-wake items (below), only if the voice-wake module loaded | | |
| n | `Restart to Install Update` if an update is downloaded, else `Check for Updates…` | always | `confirmAndInstall()` / `checkForUpdates(true)` |
| — | separator | | |
| last | `Quit` | `!kidLocked` | `gracefulShutdownAndExit()` (refused while kid-locked) |

Voice-wake items (`voiceWakeTrayItems()` `app/main.js:1523-1533`), present only
when the voice-wake module exists:

- Checkbox `Listen for "Hey Cru" while plugged in`, checked = enabled,
  disabled if not trained; click → `voiceWake.set(checked)`.
- If untrained, a disabled line `    Train it first in Settings, Voice assistant`
  (four leading spaces, an indent).
- Else if enabled and there is a note, a disabled line `    <note>`: on
  `error` → `Not listening: <error, max 80 chars>`; otherwise from
  `VOICE_WAKE_NOTE[state]` (e.g. `Paused: screen locked`, `Starting…`,
  `Listening`; full table just above 1523).
- Then a separator.

Note for native: "Start"/"Stop" control the bundled local server (all-in-one
mode). In client-only (remote) mode they are still drawn by Electron; the
native client may hide them when there is no local server, but keep the rest.

### A.4 Close to tray, quit, single instance

- Window `close` (`app/main.js:217-225`): if kid-locked, refuse. Otherwise,
  unless `app.isQuitting`, cancel the close and **hide** the window. The app
  keeps running in the tray.
- `window-all-closed` does nothing (`2715-2717`: "Don't quit — stay in tray").
- `activate` (macOS dock click) → `showWindow()` (2719).
- `before-quit` (2721-2726): the first time, cancel and run
  `gracefulShutdownAndExit()` (stops services/terminals/tunnels, then exits).
- Only one instance per install (`requestSingleInstanceLock`, 2664). A second
  launch quits itself; the first logs
  `[lifecycle] Second instance attempted — focusing this one` and calls
  `showWindow()`.

### A.5 OS notifications

Source: the server's **"Desktop app" notification channel**
(`src/webapp.js:1409-1429`): id `desktop`, label `Desktop app`, enabled by
default, available when the attached host advertises the `notify`
capability. Describe text: `Native notifications on the machine running the
desktop app.` Unavailable reasons: `The attached desktop app is too old to show
notifications. Update it.` / `No Crundi desktop app is attached to this server.`

Transport: fire-and-forget message `{ type: 'notify', title, body, url? }`
(`src/browser.js:150`):

- all-in-one: over the child-process IPC (`handleBotMessage`, `app/main.js:2005`);
- remote server: over the native-host websocket the app opens to
  `<serverUrl as ws(s)>/ws?token=<hostToken>` and registers on with
  `{ type: 'register-native-host', capabilities: ['notify', 'browser-touch',
  'browser-drawer', 'browser-mouse'], deviceId }` (`app/main.js:2096-2118`);
  handled in `handleNativeHostMessage` (2048) before the request-shaped checks.
  Reconnects every 5 s after a drop (`scheduleHostRetry`, 2151-2155).
  **The native client must advertise `notify`** or the server will not send.

Payload limits enforced by the server: `title` = `meta.title || 'Crundi'`,
`body` sliced to 400 chars, `url` only if it matches
`^/chat/[A-Za-z0-9_-]{1,80}$`.

`showServerNotification(msg)` `app/main.js:1975-1998`:

- If notifications unsupported: log `[notify] not supported on this OS`, stop.
- Title = `msg.title || 'Crundi'` (max 120), body = `msg.body` (max 400).
- No icon, no actions, no sound override, no urgency.
- Keep a strong reference to every shown notification until it is clicked,
  closed, or 6 hours pass (`liveNotifications`, 1973) — on Ubuntu a collected
  notification's click did nothing. Native: keep the handle alive likewise.
- **Click**: `bringToFront()`; then, if `url` is `/chat/<id>` (re-validated),
  `openChatInWindow(id)`.
- Errors log `[notify] could not show: <message>`.

`bringToFront()` (155-166): `showWindow()`, `app.focus({steal:true})`; on Linux
also set always-on-top, show, focus, and clear always-on-top after 300 ms.

`openChatInWindow(id)` (147-153): validate the id, then call the page's
`window.crundiOpenChat(id)` (after load if still loading). The page
(`src/webapp-html.js:9003-9012`) retries `goToChat` every 250 ms up to 24
times: switches project if the chat lives in another one, else switches to the
workbench tab, scrolls the chat cell (`[data-cellkey="live:<id>"]`) into view
centred and focuses its chat view. Native: the same "go to chat" — select the
chat's project, show the workbench, scroll its pane into view, focus its
composer; tolerate the chat list not being loaded yet (retry ~6 s).

What notifications are about is decided server-side (Settings, Notifications:
"Notification Channels" section, `src/webapp-html.js:15764-15766`, hint text
`Which of these get told. Everything for you lands in the Crundi inbox first,
and it is always on; turn on as many of the others as you like, or none at
all. The table below decides which events are worth telling you about.`).
Every notification is also an Inbox item (B).

### A.6 `alert_user` (urgent alerts) — what the PC sees

`src/alert-user.js`. The ring / attend / dismiss / snooze flow is **phone
only**:

- An alert goes first to paired phones as push kind `alert`
  (`meta = { title, tag:'alert', url, alertId, chat?, urgent?, downgraded? }`,
  118-129). Default level `urgent`. Limits: 3 urgent per rolling hour
  (`URGENT_PER_HOUR`), 20 alerts of any level (`ALERTS_PER_HOUR`); past the
  urgent limit it is sent as `normal` with `downgraded`.
- If a phone took it, **no other channel is used** — the PC shows nothing
  (only the Inbox item appears).
- If no phone could be reached, it is delivered through the other channels
  (`except: 'mobile'`) with text `title` or `title\nbody`, meta
  `{ title, tag:'alert', url, chat? }` — on the PC this is just an ordinary
  notification (A.5) titled with the alert title, clicking opens `/chat/<id>`.
- The phone reports back on `POST /api/alerts/answer`
  `{ alertId, action: 'ringing'|'attended'|'dismissed'|'snoozed', ringMs?,
  minutes? }` → `{ ok:true, waiting:bool }` (`src/webapp.js:7944-7949`).
  Snooze minutes are one of `5, 15, 30, 60`. Answers as read by Claude
  (`describeAnswer`, 167-176): `The person attended the alert (opened it).`,
  `The person dismissed the alert without opening it.`,
  `The person snoozed the alert for N minutes; it will ring again then.`,
  `Nobody answered: …`, `The phone did not confirm it was ringing …`.

**Native client: do not implement ringing or call `/api/alerts/answer`.** The
PC is not a ringing device. If a future version wants it, it would need its
own server support; out of scope.

### A.7 Unread badge

There is no OS-level badge. The only unread indicator is the **Inbox button's
badge in the sidebar** (B.2). Native may additionally set the taskbar overlay /
tray tooltip, but that is new behaviour, not parity; leave it out unless asked.

### A.8 Start at login

- Setting lives in Settings → `Desktop App` section
  (`buildUpdatesSection`, `src/webapp-html.js:4696-4708`):
  row title `Launch at startup`, sub `Start Crundi in the tray when Windows
  starts.`, switch (title/aria `Launch at startup`, action `startup-toggle`).
  Siblings: `Automatic updates` / `Check on launch and once a day. Always asks
  before installing.`; `Release channel` / `Dev gets preview builds early. A
  newer production release is offered either way.`
- Toggle → `window.api.setStartup(!launch)` → IPC `startup:set`
  (`app/preload.js:67`, `app/main.js:1744`) → returns update state with
  `launch` re-read from the OS.
- Implementation (`app/main.js:1202-1235`): the OS is the source of truth.
  Windows/macOS: `app.setLoginItemSettings({ openAtLogin })`. Linux:
  `~/.config/autostart/<appName>.desktop` (or `$XDG_CONFIG_HOME/autostart`);
  present = on. Content:
  `[Desktop Entry]`, `Type=Application`, `Name=<app>`,
  `Exec="<$APPIMAGE or exe path, escaping " \ ` $>"`, `Terminal=false`,
  `X-GNOME-Autostart-enabled=true`. Logs `[startup] Launch at login
  enabled|disabled`.
- Launch at login starts the app normally (window is shown on ready; there is
  no `--hidden` flag today, despite "in the tray" in the wording).

---

## B. The Inbox

### B.1 Model (server, `src/inbox-store.js:1-56`)

Everything meant for the owner lands in the inbox first; copies then go to the
enabled channels. Owner only (collaborators get 403).

Endpoints:

| Method | Path | Body | Reply |
|---|---|---|---|
| GET | `/api/inbox/threads` | | `{ ok, threads:[thread], unread }` newest activity first |
| GET | `/api/inbox?thread=&limit=&before=` | | `{ ok, items, unread }` newest first; limit 1..200 (default 50); `before` = item id or `at` ms |
| GET | `/api/inbox/<id>` | | `{ ok, item, unread }`, 404 if gone |
| POST | `/api/inbox/read` | `{ ids:[…] }` \| `{ thread }` \| `{ all:true }`, optional `read:false` | `{ ok, changed, unread }` |
| POST | `/api/inbox/delete` | `{ ids:[…] }` \| `{ thread }` \| `{ allRead:true }` | `{ ok, removed, unread }` |
| GET | `/api/inbox/media/<id>/<name>` | | the file; normal `Authorization` header (web adds `?token=`); `?download=1` to download |

```
item   = { id, at (ms), kind, title, text, project, chatId, chatTitle,
           media:[{ url, name, mime, size }], read, link, thread }
kind   = message | photo | file | alert | question | permission | report |
         schedule | system
thread = { key, kind:'chat'|'project'|'cru'|'system', title, subtitle,
           project, chatId, last:{ id, at, kind, title, text }, unread, count }
keys   = 'cru' | 'chat:<chatId>' | 'project:<alias>' | 'system'
```

A media entry with no `url` means the file was not kept (over 200 MB, or gone).

Live: SSE event `inbox` on `/api/events` with data
`{ type:'inbox', unread, item?, thread? }` (item when one was added, thread
when one thread is involved). Capability name: `inbox`
(`src/client-version.js`). Retention: read items older than 90 days go; max
2000 items.

Support detection (`ibxGet`, `src/webapp-html.js:10643-10650`): a 404 (older
server) or 403 (collaborator) sets `supported = false` and hides the Inbox
button. Native: same; never show an error for this.

### B.2 Sidebar Inbox button

Markup `src/webapp-html.js:3717`; CSS 596-607.

- In the sidebar footer, **under "+ Add project"**, full width, `margin-top:
  8px`, solid border (Add project's is dashed). Icon: envelope (mail) 16px,
  label `Inbox`, then a badge.
- Tooltip / aria: `Inbox`; with unread: tooltip `Inbox: N unread`
  (`ibxBadge`, 10631-10642).
- With unread > 0: class `has-unread` → primary text colour, accent border.
- Badge: pill 18px high, min-width 18px, accent background, white bold text
  (~0.68rem); text `N`, or `99+` above 99; hidden at 0.
- Collapsed sidebar rail: label hidden; badge becomes a 16px pill absolutely
  positioned top-right of the button (top 2px, right 6px).
- Hidden when `!supported` or the user is a collaborator.
- Click (`data-ibx="show"`) → `openInbox('')`.

Unread count is loaded on every `/api/events` (re)connect (`ibxLoad()` at
11097) and updated from every `inbox` event and every read/delete reply
(`ibxPost` takes `unread` from the reply).

### B.3 Where it is drawn

A **full tab panel** (`#inbox-panel`, `data-panel="inbox"`,
`src/webapp-html.js:3826`) in the main content area, replacing the workbench
etc.; it has **no tab-bar button** — it is only reached via the sidebar
button, `#inbox` / `#inbox/<id>` URL hash (10815-10824), or the agent's
"show tab inbox" action (6890; allowed with no project open). `openInbox`
closes the mobile sidebar, `switchTab('inbox')`, renders, loads.

Layout (CSS 609-688, render 10744-10787):

```
┌ ibx-head ─────────────────────────────────────────────────────────┐
│ [mail] Inbox   <sub>                          [check Mark all read]│
├ ibx-list (360px, scroll) ─┬ ibx-detail ───────────────────────────┤
│ thread row                │ ibx-chead: [<] [icon] Title           │
│ thread row (active)       │            subtitle   [Open chat]     │
│ ...                       │            [Mark read] [Delete thread]│
│                           │ ibx-conv (scroll): [Show older]       │
│                           │   message card (oldest → newest)      │
└───────────────────────────┴───────────────────────────────────────┘
```

- Header sub text (`#ibx-sub`): `N unread`, else `All read` if there are
  threads, else empty.
- Narrow (≤768px): list full width; opening a thread hides list and header,
  detail shows with a back button (`chevron-left`, aria `Back to the inbox`).
  On wide screens the back button is hidden. Native PC: implement the wide
  layout; the narrow rule applies only if the window can get that narrow
  (min width is 800, so effectively never).

### B.4 Thread row (`ibxThreadRow`, 10712-10720)

Button, full width, padding 12/14, bottom border; hover background; active
(open) row accent-dim background.

- Left: 34px round icon. Glyph by thread kind (`IBX_THREAD_IC`, 10612):
  chat `message-square`, project `folder`, cru `mic`, system `info`; colours:
  chat accent-hover, project sky, cru green, system amber.
  - Project thread whose project has an icon: that icon, 34px, radius 9.
  - Chat/project thread otherwise: the glyph plus a 16px corner badge
    (bottom-right, -3px, accent background, radius 5) showing the project's
    icon or its initials (`ibxThreadIcon`, 10700-10711). Project matched by
    alias, then name, case-insensitive.
- Middle: title (0.86rem, bold if unread), subtitle (0.7rem muted, only if
  present), last item's title (0.76rem; muted, secondary colour if unread),
  all single-line ellipsised.
- Right: relative time of `last.at` and, if unread, a count pill (`99+` cap).

Relative time (`ibxWhen`, 10617-10626): `<60s` → `now`; `<1h` → `Nm`;
`<1d` → `Nh`; `<6d` → short weekday (`Mon`); else `Nov 3` (month short +
day). Full form (messages): locale `month short, day, 2-digit hour:minute`.

Empty list text: envelope icon 34px (60% opacity) + `Nothing here yet.
Messages, pictures and files from your chats land here, with questions,
alerts and reports.`

### B.5 Conversation (`ibxConvHtml`, 10726-10743; `ibxMsgHtml`, 10734)

No thread open: centred empty state, envelope icon + `Choose a conversation
to read it.` (or `Nothing here yet.` if there are no threads).

Conversation header buttons (all `ibx-btn`: icon + text, small, bordered):

- `Open chat` (icon `message-square`) — only for `kind === 'chat'` with a
  `chatId`. Switches to workbench and `goToChat(chatId)`; if the chat is not
  live any more, selects the thread's project and toasts `That chat is not
  open any more` (10875-10883).
- `Mark read` (icon `check`) → `POST /api/inbox/read { thread }`; error toast
  `Could not mark it read: <msg>`.
- `Delete thread` (icon `trash`, danger hover red) → confirm `Delete this
  whole conversation from the inbox?` → `POST /api/inbox/delete { thread }`;
  closes the thread; error toast `Could not delete: <msg>`.

`Mark all read` (top header) → optimistic zero, then
`POST /api/inbox/read { all:true }`; error toast `Could not mark them read:
<msg>`.

Messages are shown **oldest at top, newest at bottom** (server order
reversed). Page size 100; if a page returned 100, a `Show older` link button
at the top loads `before=<oldest id>` and appends.

Message card (max-width 760, card background, subtle border, radius,
padding 10/14/12):

- Unread-when-opened items get a 3px accent left border (`new`), kept for the
  session the thread is open.
- A focused item (from a `#inbox/<id>` link) gets the focus ring and is
  scrolled to centre.
- Head row: kind glyph (14px, tooltip = kind name), bold title, time
  (full form) right-aligned. Kinds (`IBX_KIND`, 10606-10610):
  `message` message-square "Message", `photo` image "Picture", `file` file
  "File", `alert` bell "Alert", `question` help-circle "Question",
  `permission` shield-alert "Permission", `report` mic "Report", `schedule`
  clock "Scheduled job", `system` info "Crundi". Unknown → message. Alert and
  permission glyphs are amber; question is sky.
- Delete: a 22px round `x` button top-right, 55% opacity, red on hover;
  tooltip/aria `Delete this message` → `POST /api/inbox/delete { ids:[id] }`,
  no confirm. Removing the last message closes the thread.
- Text: pre-wrapped, 0.85rem, line-height 1.55. **Plain text, not markdown.**
- Media, stacked under the text:
  - mime png/jpeg/gif/webp: inline image, max height 50% of viewport,
    click opens the full image (browser: new tab; native: open externally or
    an image viewer).
  - other files: a chip `[file icon] name  size  [download icon]`, click
    downloads (`?download=1`). Size format: `N B`, `N KB` (rounded),
    `N.N MB`.
  - no url: muted chip `[file] name  not kept`.

There is **no reply / composer** in the Inbox. Replying is done by `Open
chat`.

Scrolling (10760-10785): pinned to the bottom while the owner is within 40px
of it; images re-pin as they load; scrolling away unpins and preserves
position across live re-renders; list scroll position is kept on re-render.

### B.6 Mark read rules

- Opening a thread (`ibxOpenThread`, 10788-10800): load it, mark its items
  read locally, zero the thread's count, and if it had unread send
  `POST /api/inbox/read { thread }`, then reload threads.
- Live event for the open thread while the Inbox tab is showing
  (`ibxOnEvent`, 10826-10839, debounced 150 ms): reload the thread; if the
  event added an item and the window is visible, mark the thread read. Then
  reload the thread list. Other events: just reload the list and badge.
- Nothing is marked read by merely being in the list.

### B.7 Hash links

`#inbox` opens the Inbox; `#inbox/<id>` fetches `/api/inbox/<id>`, opens its
thread and focuses it; if gone, toast `That message is no longer in the
inbox`. The hash is then cleared. Used by Telegram copies and browser pushes;
native equivalent: a deep-link/route "open inbox item <id>".

### B.8 Not the same as "Approvals"

The "Approvals inbox" (`src/webapp-html.js:10892+`, `approvals-btn`, SSE
`approvals`) is a separate panel for secrets/permission cards/collaborator
requests. Not covered here.

---

## C. The chat bubble and the compact chat page

### C.1 What the bubble is

A chat bubble is an **Android** notification bubble (`NotificationCompat.
BubbleMetadata`, desired height 640, `android/.../Notifier.kt:385, 600, 744`)
that floats over other apps and, expanded, shows the compact chat page
`/chat/<id>` in a web view. The Android OS draws the collapsed head, the drag,
the dismiss target and the unread dot; the app has no code for those.

**The Electron PC client has no bubble.** There is no bubble window in
`app/main.js` and `app/preload.js` exposes no `window.CrundiNative`. So on the
PC:

- The pane header's `Bubble` button is **not drawn**: it appears only when
  `window.CrundiNative.can()` lists `bubble` (`NATIVE_CAN`,
  `src/webapp-html.js:8952-8954`; `nativeChatBtns`, 9016-9020). Button:
  class `term-font-btn`, text `Bubble`, title `Open this chat as a floating
  bubble`, action `chat-bubble` → `CrundiNative.openBubble(tid)`; failure
  toast `This app cannot open a bubble` (16505-16506).
- Likewise the `App settings` sidebar button (`can()` lists `settings`) is
  hidden.

Native client: **do not add a Bubble button** (parity with the PC client).
If a floating mini-chat is ever wanted it would be a new feature; the notes
below (C.3) describe what such a window would host, for reference.

### C.2 Notifications ↔ "seen"

When a bridge with `seen` exists, the main page reports every 3 s which chats
are ≥40% on screen while visible on the workbench (`src/webapp-html.js:
8990-9002`) so the phone can drop their notifications. The PC client does not
do this (Electron notifications are not withdrawn). Native: optional; not
parity.

### C.3 The compact chat page `/chat/<id>`

Server HTML `src/chat-page.js`; behaviour `app/vendor/crundi-chat-page.js`
(also served to the web). It is a phone/bubble page, but it is also what the
`/chat/<id>` URL shows in a browser, and it is embedded in a frame by the live
factory graph. One conversation, full height:

Header (`src/chat-page.js:119-124`):

- Back button (aria `Back`, left arrow). Order of what it does
  (`crundi-chat-page.js:640-648`): close a shown file first; else
  `CrundiNative.close()` (put the bubble away); in a frame, post
  `{ type:'crundi-chat-close' }` to the parent; else `history.back()` or go
  to `/`.
- Project mark: project icon image, or initials on a gradient of the
  project's colours (`paintHead`, 440-467).
- Title (`#cp-title`, default `Chat`; document title `<title> · Crundi`) and
  under it a state dot + `<project> · <state word>`. State words
  (`STATE_WORDS`, 439): `working` → `Working`, `needs-input` → `Needs you`,
  `waiting` → `Waiting`, `idle` → `Ready`; exited → no word.
- Lock button (aria/title `Secrets for this chat`; with grants: count badge
  and title `N secret(s) pre-authorised for this chat`, else
  `Pre-authorise secrets for this chat`); shown ≥600px wide when supported.
- More button (aria `More`) → menu (`src/chat-page.js:138-144`):
  `Show the plan` (only when the chat has a plan), `Rename chat`,
  `Secrets for this chat` (narrow only; with count `Secrets for this chat
  (N)`), `Stop Claude` (disabled unless state is `working`; note: no click
  handler for it exists in `crundi-chat-page.js` — it currently does
  nothing), `Open in Crundi` (→ `CrundiNative.openMain('/chat/<id>')`, else
  navigate).
- Rename is inline: the title becomes a text box (aria `Chat name`, max 80);
  Enter/blur saves via `POST /api/ui-sessions/<id>/rename { title }`, Escape
  cancels; error toast `Could not rename it`.

Below the header:

- Connection line (`#cp-conn`): `Reconnecting` (or `This device is
  offline.`), shown after a short CSS delay; flashes `Connected again.` for
  1.7 s on recovery (508-530).
- Secret request cards (`#cp-ask`): `Run a command with <name>` / `Release
  <name>`, command, `The value is bound to $<ENV> and never shown to
  Claude.` / `Claude will be able to read this value.`, buttons `Approve`
  (then a PIN/passphrase box: `Enter a 6-digit PIN` / `Enter the
  passphrase`; errors `Wrong PIN`) and `Reject`; toasts `Approved`,
  `Approved, and allowed for this chat` (289-350).
- Factory link (`#cp-factory`, hidden unless the chat belongs to a run):
  `Part of a factory · Open the live graph` → `/#factory-run=<runId>`.
- The conversation: `window.CrundiChat.mount(...)` from `claude-chat.js` —
  exactly the workbench chat (composer, cards, tools), with
  `isMobile: () => true`. Widgets docked for the chat's project.
- File viewer overlay (`#cp-file`): name, `Download`, `Close`; messages
  `Loading…`, `This file could not be read.`, `This file is too large to show
  here. It can be downloaded.` (>1 MB), `This is not a text file. It can be
  downloaded.`.

Full-page notes (`note(title, text, action)`, 407-415):

| Case | Title | Text | Button |
|---|---|---|---|
| bad id | `Not a chat` | `This address does not name a chat.` | `Open Crundi` |
| chat.js missing | `Could not load` | `The chat could not be loaded. Check the connection and try again.` | `Try again` |
| no session | `Signed out` | `Sign in to Crundi to see this chat.` | `Sign in` |
| chat not live | `Chat closed` | `This chat is no longer open on the server.` | `Open Crundi` |
| 426 | `Update required` | `This page`/`This app` + ` is older than this server accepts. Version <min> or newer is needed.` | — |

Data: meta from `GET /api/ui-sessions` (find by id; `longTitle || title`);
chat stream over `/ws?token=…&web=<version>`, with
`{ type:'presence', active, engaged, project }` on visibility changes; SSE
`/api/events` for `widget`, `secret-requests`, `chat-secrets`, `state`.
Reconnect backoff `min(15000, 600·1.7^retry)` ms; a forced catch-up after 2
minutes of silence (626-638).

Bridge calls the page makes when hosted by an app (all optional;
`crundi-chat-page.js:12-28`): `session()`, `saveSession(json)`, `close()`,
`openMain(path)`, `chatState(json)` (`{ id, title, project, state, mark }`
for the bubble's own label), `download(url, name)`, `seen(chatId)`.

### C.4 Native client guidance

- The native PC client already renders chats natively; it does not need the
  compact page. `/chat/<id>` links (notification `url`, inbox item `link`)
  map to the in-app "go to chat" (A.5).
- No bubble window, no Bubble button, no `seen` reporting — matching the
  Electron PC client.
- Per the "chat features on the bubble page too" rule, anything new added to
  native chats has no bubble-page obligation on PC (the bubble page is the
  web/Android surface).

---

## D. Checklist for the native implementation

1. Tray icon (16px head), tooltip `Crundi — <status>`, double-click (and on
   Linux single-click) opens the window.
2. Tray menu exactly as A.3, rebuilt on status / lock / update / voice-wake
   changes; Quit disabled while kid-locked.
3. Closing the window hides it; quit only from the tray (or OS quit) with
   graceful shutdown; single instance focuses the existing window.
4. Register as native host with capability `notify`; show `{type:'notify'}`
   as OS notifications (title ≤120, body ≤400), keep handles alive, click →
   bring to front + go to chat if `url` is `/chat/<id>`.
5. No alert ringing on PC; alerts reach the PC only as ordinary notifications
   when no phone took them.
6. `Launch at startup` toggle in Settings → Desktop App; OS login item on
   Windows/macOS, `~/.config/autostart/<app>.desktop` on Linux.
7. Sidebar `Inbox` button with unread badge (`99+` cap), hidden for
   collaborators / old servers; updates from SSE `inbox`.
8. Inbox tab panel: header (`Inbox`, sub, `Mark all read`), 360px thread list,
   conversation with `Open chat` / `Mark read` / `Delete thread`, message
   cards with kind glyph, time, delete `x`, text, images, file chips,
   `Show older`; empty states verbatim; read rules per B.6; hash/deep link
   per B.7. No composer.
