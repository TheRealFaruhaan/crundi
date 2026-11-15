# Browser drawer (PC client) and the web Browsers tab

Source of truth for the native rewrite of the Electron drawer. Abbreviations:
`VD` = app/vendor/crundi-browser-drawer.js (UI side), `BD` = app/browser-drawer.js
(main process), `DN` = app/drawer-net.js, `BP` = app/browser-pointer.js,
`NT` = app/net-tunnel-client.js, `PL` = app/preload.js, `MJ` = app/main.js,
`WH` = src/webapp-html.js, `SB` = src/browser.js, `WS` = src/webapp.js.

Native mapping: one CEF browser per tab (memory: CEF only for browser panels).
A tab's page must keep rendering while hidden (chats drive it); never destroy a
tab's browser except on close, network switch, or the 1-hour unload.

## 1. Layout

### 1.1 Placement
- A column beside the project area, below the app top bar. Order on the right
  (default): `project area | split | drawer`. On the left: `sidebar | drawer | split | project area` (VD:66-71).
- Split: 5px, `col-resize`, border colour, accent on hover/drag. Tooltip
  "Drag to resize; double-click to reset" (WH:3830). Drag sets width; double-click
  resets to 560 (VD:379-397). While dragging, pages must not swallow pointer moves (VD:384).
- Width: default 560, min 320; drawn width = min(chosen, layoutWidth − 420)
  (project area keeps ≥420). Chosen width persists unchanged even when narrowed (VD:329-341).
- Closed drawer: invisible, out of flow, pages keep running (VD:63-65). Focus
  inside it is dropped on close (VD:349-350).
- The project-area layout (terminals) refits on every drawer layout change (`onLayout`, WH:9446).

### 1.2 Opener in the project tab bar (WH:3741, VD:72-75, 362-368)
- Button `Browser` (globe icon), at the far right of the tab bar, only in the desktop app.
- Tooltip: "Browser (desktop app)"; when drawer hidden and this project has tabs:
  "Browser: hidden, its pages still open (click to show)" plus a 7px accent dot.
- Hidden entirely while the drawer is open (the drawer has its own hide/close).
- Click with no project: toast "Select a project first" (error) (WH:9453).
- Old app without drawer API: toast "The browser drawer needs a newer Crundi desktop app. Update the app." (WH:9437).
- Open with no tabs in the project creates one at `https://www.google.com` on `client` (VD:376).

### 1.3 Parts top to bottom (VD:271-308)
1. **Tab strip** (min-height 34): tabs, then `+`, spacer, `...`, hide, close-all.
2. **Address bar** (`.brd-bar`): Back, Forward, Reload, URL field, [lock], Network, Device, Annotate, DevTools.
3. **Annotation bar** (only while annotating).
4. **Password bar** (save/update/fill prompt; passwords spec).
5. **Stage**: dotted-grid background (radial dots 16px); tab pages; annotation overlay; chat cursor; diag text; empty state; full-cover panel.

### 1.4 Tab strip (VD:405-425)
- Tab: max 200 / min 64 wide, 29 high, rounded top. Contents: network badge, title, close `x` ("Close tab").
  - Title text: `title || url || "New tab"`. No favicon (none exists in the Electron drawer).
  - Badge: `SRV` (accent bg, white) for server; red when server is remote and not tunnelling; tooltip "Network: <server label>".
    `HP` (green #059669) for homepool; red if no exit; tooltip "<net label>: <exit text>". Client: no badge.
  - Tab tooltip (multi-line): title-or-url / url / "Network: <label>" / plus
    "Asleep: wakes, as it was, when shown or used" or
    "Unloaded after an hour asleep: loads again when shown or used".
  - Sleeping/unloaded tab: title at 50% opacity, italic.
  - Click activates; middle-click closes (VD:1236); `x` closes.
- `+` "New tab": new tab at google.com on the active tab's network (or client) (VD:1224).
- `...` "History and browsing data" → menu (1.8).
- Hide "Hide the browser (pages keep running)": chevron pointing toward the drawer's edge (right side: chevron-right; left side: chevron-left).
- Close-all `x` "Close the browser: every tab in this project": closes every tab of the current project, then hides (VD:370-375).

### 1.5 Address bar (VD:273-286, 426-461, 1183-1195)
- Back "Back", Forward "Forward" (disabled per canGoBack/canGoForward), Reload "Reload" (icon spins while loading).
- URL field: pill, mono font, placeholder "Enter URL or search". Shows the tab URL ("" for about:blank) unless focused.
  - Enter → `go()`: has `http(s)://` or `about:blank` → as is; looks like a host (no space and a `.` after index 0, or `localhost[:port]`, or `digits.:port`) → prefix `http://` for `localhost|127.|[::1]`, else `https://`; otherwise Google search `https://www.google.com/search?q=<enc>`. Esc blurs and resets.
  - Autocomplete: 120ms after typing, `drawer:history(q, 8)` fills a suggestion list (url, title as label) (VD:1038-1047).
- Lock "Saved passwords for this site" — only when the tab's state has `logins > 0`; tooltip "<n> saved logins for this site" / "A saved login for this site" (VD:1305-1311).
- Network button: tooltip "Network: <label>" (+ newline + exit text for homepool). Accent when server, green when homepool, red when bad.
- Device button (smartphone icon): accent when device ≠ Responsive; tooltip "Device size: <name>" / "Device size: Custom WxH".
- Annotate "Annotate the page" (toggle, filled when on). DevTools "DevTools" (toggles detached DevTools).
- With no active tab, URL/network/device are disabled.

### 1.6 Popovers: network and device (VD:1138-1180)
- Anchored under the button, min-width 210, max-height 60vh, closes on outside click or re-click.
- Network popover: header "This tab’s network"; for homepool tabs a note "<label>: <exit text>"; items:
  `<client label>`, `<server label>`, group "Homepool" with each allowed non-self network
  "<name>" or "<name> (no exit up)"; then "Homepool settings…" (key exists) or "Homepool: set up…".
  Homepool data is refreshed before showing. Picking the settings item opens the homepool panel.
- Device popover: header "Device size"; items "Responsive", "<name>  <w>x<h>" (two spaces), and
  "Custom  <w>x<h>" only when the tab is custom. Current item accent + bold.

### 1.7 Stage overlays
- Empty state (no tabs in project): "No browser tabs in this project yet." + button "Open a tab" (VD:307).
- Diag (centred, muted, non-blocking): from state `diag`: "Load failed (<code>): <desc>" (BD:221),
  "The page crashed. Reload to try again." (BD:232), "This tab could not start: <err>" (VD:604).
- Chat cursor: indigo arrow (#6366f1, white stroke) with tag "Claude", ring when pressed, ripple on click (VD:162-171, 305).

### 1.8 `...` menu (VD:1056-1065)
Items: "History" (clock), "Passwords" (lock; only if passwords available), "Clear browsing data…" (trash),
"Open on the left" / "Open on the right" (toggles side; persisted).

### 1.9 Full-cover panels (`.brd-panel`, close `x` "Close")
- **History** (VD:1067-1083): header "History" + button "Clear browsing data…"; search field "Search history";
  rows title-or-url, url (mono, muted), age (`now`, `N min`, `N h`, else locale date). Up to 200.
  Empty: "No history yet." / "Nothing in the history matches.". Click row: navigate active tab (or new client tab).
- **Clear browsing data** (VD:1084-1119): radios "Every site" (default) / "Only this site" ("Its subdomains too")
  + site field (placeholder "example.com", prefilled with active tab's host). Checkboxes:
  "Browsing history" ✓ ("Every address visited, and each tab’s back and forward list"),
  "Cookies and site data" ☐ ("Logins, local storage, databases, service workers. Signs you out of sites."),
  "Cached images and files" ✓ ("Pages load again from the network"),
  "Saved passwords" ☐ ("Every saved login, on every site (not limited to one site)").
  Note: "For this computer’s browser: every tab in every project, on every network. Open tabs stay open. For one site, the back and forward lists are left as they are."
  Buttons "Cancel", "Clear" (danger). Errors/toasts: "Choose something to clear"; confirm "Delete every saved password on this computer?" → "Saved passwords deleted";
  bad site "Type a site, such as example.com"; success "Cleared[ for <site>]: <list or nothing>"; failure "Could not clear: <err>".
  Site normalisation: lowercase, strip scheme, path/query/hash, port, leading dots; must match `[a-z0-9.-]+` and contain a dot or be `localhost`.
- **Saved passwords**: list/PIN/never-list — see the passwords spec (VD:1399-1455).
- **Homepool for this browser** (VD:508-557): see 3.4.

## 2. Tabs per project

- State is in `localStorage["crundi_brd"]` (VD:34, 232-242):
  `{ open: bool, width: number, side: "right"|"left", projects: { <alias lowercase>: { active: tabId|null, tabs: [{ id, url, title, device, custom: {w,h}|null, network }] } } }`.
  Native: same shape in the client's settings store; saved on every change.
- Tab id: `Date.now().toString(36) + 6 random base36` (VD:215); must match `^[A-Za-z0-9_-]{4,64}$` (BD:56).
- `device`: index into DEVICES (0 = Responsive) or `"custom"`. `network`: `client` | `server` | `hp:<net>`.
- Project key: alias lowercased. On project switch (`selectProject` → `brdSetProject`, WH:7571):
  annotation mode off, strip shows that project's tabs, its `active` tab shown (VD:1551-1557). Other projects'
  pages stay alive, invisible. Open/closed state and width are global, not per project.
- Restored tabs are lazy: a page is created only when it is shown (active in the current project with drawer open) or a chat asks (`ensure`) (VD:631-642, 1475).
- New tab: `MAX_TABS = 12` per project → toast "At most 12 tabs per project"; no project → "Select a project first" (VD:559-572).
  New tab becomes active if asked or if none active.
- Close tab: the left neighbour becomes active (index−1, else the new first) (VD:610-622); page destroyed; `drawer:detach`.
- Popups (`window.open`, target=_blank): denied as windows; open a new tab in the same project/network, made active but drawer not popped open (BD:235-240).
- Title/URL update from `drawer:state` events; navigating to a different URL (ignoring `#`) clears that tab's marks (VD:1257-1271).
- Sleep (BD:408-467): a tab not shown and unused for 5 min (`CRUNDI_DRAWER_SLEEP_MS`) is frozen (`Page.setWebLifecycleState frozen`),
  unless audible or DevTools open; window hidden counts as not shown. Frozen 1 h (`CRUNDI_DRAWER_UNLOAD_MS`) → page discarded
  (tab kept with url/title; reloads when shown/used). Shown or used by a chat → thawed. UI reports visibility via `drawer:visible`.
- History (BD:89-137): one per client, `<userData>/drawer-history.json`, `[{url,title,at,visits}]`, newest first, max 5000, only http(s),
  dedup by url, write debounced 1s via tmp+rename. Query: all words in url+title, limit 1-200 (default 8).
- Profiles: all tabs share one profile per network: `persist:crundi-browser` (client), `persist:crundi-browser-srv` (server),
  `persist:crundi-browser-hp-<net>` (homepool; list kept in `<userData>/profiles.json` so clearing reaches them) (BD:36-46, 97-105).

## 3. Networks

### 3.1 Meaning (DN:1-15, BD:26-28)
- `client` — this PC's network, no proxy. Label "This PC".
- `server` — the server's network (reaches server localhost). Labels (BD:77-85):
  local server: "Server (this PC)" (no proxy); remote + tunnel: "Server"; remote, server lacks `net-tunnel`:
  "Server (needs a newer server)"; remote not connected: "Server (not connected)".
- `hp:<net>` — out through a homepool network via the PC browser key. Label "Homepool: <name>".

### 3.2 How server/homepool traffic flows (DN:56-94, NT)
- A loopback HTTP proxy on `127.0.0.1:<random>`, Basic auth user `crundi`, random per-launch password
  (answered only for our proxies). Session proxy rules `http://127.0.0.1:<port>`, bypass `<-loopback>` (so localhost goes to the server).
- Each CONNECT / absolute-form request = one stream over the native-host websocket (`<server>/ws?token=…`, binary frames, MJ:2096, 2123)
  or, with the bundled server, over its IPC as `{type:'netTunnel', b64}` (MJ:2006, 2214).
- Frame: `[type u8][id u32 BE][payload]`; types OPEN=1 (payload = target, prefixed `hp|<net>|` for homepool), DATA=2, CLOSE=3, OPENED=4, ERROR=5.
  Each proxy owns id range `idBase<<24` (server=0, homepool 1,2,…); route incoming frames by `buf[1]`. Open timeout 20s.
- Proxy error pages (heading "Server network" or "Homepool: <net>"): 407 "Sign-in needed."; 502 "This tab goes out through the server, and the desktop app is not connected to the server right now.";
  502 with server's error text (title "No exit is up" / "Could not connect"); 400 "Bad request." / "Only http and https are carried." (NT:127-150).
- On socket close all streams die; `drawer:info` is re-emitted on connect (`native-host-ready` with `features`) and close (MJ:2125-2145).
- `info()` = `{ mode: 'local'|'remote', status: 'direct'|'tunnel'|'offline'|'unsupported', homepool: 'tunnel'|'unsupported'|'offline', labels:{client,server} }`.

### 3.3 Switching (VD:1240-1254, 1202-1207)
- Picking a network recreates the tab's page in the new profile at its current URL (reload); marks are kept. Persisted.
- `server` is "bad" (red) when mode remote and status ≠ tunnel.

### 3.4 Homepool panel (owner only; VD:463-557)
- Data: `GET /api/homepool/browser?device=<deviceId>` → `{ ok, key: {id, enabled, networks:[ids], fallbackToServer}|null, networks:[{id,name,exit:{name,publicIp,fallback}|null,refusal,selfNode}], available:[{id,name,selfNode}], chats:[{id,title,project}], projects:[alias], refusal }`.
  Polled every 15s while the drawer is open with a homepool tab in the project, or the panel is open.
- Exit text: "checking…" / "homepool is not set up for this browser" / "the PC browser key does not allow this network" / "this PC is a node of this network" /
  "No exit is up[ (<refusal>)]" / "via <exit name>[ · <publicIp>][ (server, fallback)]".
- Panel header "Homepool for this browser". No data: "Homepool is not available on this server."
- No key: intro "Tabs can go out through your homepool networks: your home line, a friend’s. This makes the PC browser key: you choose the networks it may use, and switch it off here or in Homepool › Access at any time. Chats ask you before they use it."
  checkboxes per network (selfNode disabled with "This PC is a node of this network, so its browser does not use it"; none: "You have no homepool networks yet: make one in the Homepool tab."),
  "Leave from the server when no exit is up" ("Off: with no node up, pages do not load (nothing leaves another way)"), buttons "Cancel", "Make the PC browser key".
- With key: "PC browser key on" ("Off: homepool tabs stop loading at once" or refusal in red); "Exits now (each network keeps one line, so its tabs keep one address):" rows (or "No networks.");
  "Networks it may use:" checkboxes; fallback checkbox; "Chats you approved (others ask you first; their requests are in Homepool › Requests):" rows with "Remove"
  ("Every chat of <project>" rows too; empty "None yet."); buttons "Close", "Save".
- Actions `POST /api/homepool`: `{action:'key-add', key:{kind:'browser', name:'PC browser', networks, fallbackToServer, slots:2}}`,
  `{action:'key-update', id, key:{networks, fallbackToServer, enabled}}`, `{action:'browser-revoke', chatId}` / `{…, project}`. Toasts "Saved", "Choose at least one network", or the error.

## 4. Devices (VD:36-56, 645-653; BD:472-503)

| # | Name | W×H | dpr | mobile | UA family |
|---|---|---|---|---|---|
| 0 | Responsive | fills stage | – | – | default |
| 1 | iPhone SE | 375×667 | 2 | y | iPhone OS 16_0 Safari |
| 2 | iPhone 12 Pro | 390×844 | 3 | y | iPhone OS 16_0 |
| 3 | iPhone 15 Pro | 393×852 | 3 | y | iPhone OS 17_0 |
| 4 | iPhone 16 Pro | 393×852 | 3 | y | iPhone OS 18_0 |
| 5 | iPhone 16 Pro Max | 430×932 | 3 | y | iPhone OS 18_0 |
| 6 | iPad Mini | 768×1024 | 2 | y | iPad OS 17_0 |
| 7 | iPad Air | 820×1180 | 2 | y | iPad OS 17_0 |
| 8 | iPad Pro | 1024×1366 | 2 | y | iPad OS 17_0 |
| 9 | Pixel 5 | 393×851 | 2.75 | y | Android 11 Chrome 90 |
| 10 | Pixel 7 | 412×915 | 2.625 | y | Android 13 Chrome 120 |
| 11 | Pixel 8 Pro | 430×932 | 2.625 | y | Android 14 Chrome 120 |
| 12 | Galaxy S22 | 360×800 | 3 | y | Android 13 SM-S901B |
| 13 | Galaxy S24 Ultra | 412×915 | 2.625 | y | Android 14 SM-S928B |
| 14 | Galaxy Z Fold 5 | 344×882 | 2.8 | y | Android 13 SM-F946B Chrome 114 |
| 15 | Galaxy Z Flip 5 | 264×844 | 2.8 | y | Android 13 SM-F731B Chrome 114 |
| 16 | Surface Pro 9 | 1200×1800 | 2 | y | Windows NT 10 Chrome 120 |
| 17 | Desktop 1280 | 1280×800 | 1 | n | default |
| 18 | Full HD 1920 | 1920×1080 | 1 | n | default |

Copy the exact UA strings from VD:38-53. Fallback mobile UA (no UA given): Pixel 7 Android 14 Chrome 131 (BD:50).
- Display: page renders at the device's true CSS size; box = fit inside stage minus 10px padding, scale = min(fitW/w, fitH/h), centred on the dotted grid (VD:645-653).
- Emulation via CDP: `Emulation.setDeviceMetricsOverride {width,height,deviceScaleFactor,mobile,scale,screenWidth,screenHeight}`, touch emulation (maxTouchPoints 5) and touch-for-mouse when mobile, UA override.
  Reload only when width/height/mobile/ua changed; Responsive clears all overrides (BD:475-503). Re-emulate when the box scale changes.
- Custom size: no UI to type one; set by chat `browser_resize` → `device:'custom', custom:{w,h}` clamped 200-3840, dpr 1, desktop (VD:1496-1502). Shown as "Custom  WxH" in the popover and tooltip.

## 5. Annotations

### 5.1 Bar (VD:287-302) — shown while annotating
Tools (one active): Select "Select an element and annotate it (its CSS selector goes with the note)" (layers, default) |
Pin "Draw: a pin with a note" | Box "Draw: a box" | Pen "Draw: freehand". Then a count, spacer, chat picker
(tooltip "Chat to send to"), label "Send", two send buttons, separator, Clear "Remove your marks" (trash), Done "Done annotating" (check).
- Count: "Select an element, or draw" when empty, else "<n> yours[, <m> from Claude]" (VD:809-811).
- No text tool, no colour picker. Colours fixed: person = orange #ea580c, Claude = indigo #6366f1.

### 5.2 Drawing (VD:855-938)
- Coordinates kept in page CSS px; overlay px = page px × scale.
- Select: hover outlines the element under the pointer (orange 2px, 12% fill, selector tag above), via `drawer:describe`. Click → `element` mark (dashed rect) with selector.
- Pin: click → numbered circle (r 11). Box: drag; dropped if <4px. Pen: polyline 3px; dropped if <3 points; simplified (every 2nd point, max 400), anchor = middle point.
- After each mark a note field appears: placeholder "Note (Enter to keep, Esc to skip)"; Enter keeps (≤500 chars), Esc skips, blur keeps non-empty text.
- Labels: "<n>[ Claude] <text>" plus selector (mono) for elements. Claude's marks (from `annotate` requests) can be pin, box, arrow, label.
- Clear removes only the person's marks. Marks clear on navigation to a new URL. Turning annotation off on drawer close/project switch.

### 5.3 Sending to a chat (VD:825-853, 990-1031; WH:9397-9430)
- Picker lists, for the current project: open UI chats (`kind==='ui'`, not exited, with a chat view) as `{id, title || 'Chat'}`, preferring the last pick, then the focused chat, then first.
  Owner also gets "Cru" (`id 'cru'`) at the top. Empty: option "No chat open in this project".
- "Send CSS selectors" (file-text icon): enabled only with ≥1 picked element and no drawn marks. Tooltips:
  "Open a chat in this project first" / "CSS selectors: not available while something is drawn on the page (a drawing has no selector). Send the full screenshot, or clear the drawing." /
  "CSS selectors: select an element first" / "Send to the chat: the selected elements, with their notes and CSS selectors (no picture)".
- "Send screenshot" (image icon): enabled when a chat is chosen. Tooltip "Send to the chat: the full screenshot with your marks drawn in[, and the selected elements' selectors]".
- Only the person's marks are sent, renumbered 1..n. Text:
  ```
  Browser: <url>[ (<title>)], viewport <W>x<H>[, server network] [drawer tab key: drawer:<tabId>][, screenshot attached with the marks numbered]
  <n>. element[: note]
     selector: <css>
     element: <tag>[ "<text ≤100>"]
     html: <opening tag, whitespace collapsed, ≤200 cut at a '>' with " …">
  <n>. box at X,Y size WxH (see the screenshot)[: note]     | drawing around X,Y | pin at X,Y
  (no marks: the page as it is)          ← when none
  ```
- Screenshot: `drawer:capture` PNG, marks composited (box, dashed element, arrow with head, pen, numbered disc) (VD:942-972).
- Delivery (no endpoint of its own): for a chat, PNG uploaded via `POST /api/attachments/upload {project, name:'browser-<ms>.png', data:<b64>}` → `{ok, path}`;
  then the text (+ " " + path) is inserted into that chat's composer and focused — **not sent**. For Cru: Cru's screen opens with text and the PNG as a File.
- Toasts: "Added to the chat: press Send there"; "Could not attach: <err>"; "Open a chat in this project first".

## 6. Other behaviour
- DevTools: detached window toggle (BD:320-323).
- Not present: find-in-page, zoom, custom context menu, downloads UI (none in the Electron drawer; Chromium defaults).
- `will-prevent-unload` is always overridden (no "leave page?" dialogs) (BD:233).
- Webviews hardened: no preload/Node, sandbox, contextIsolation, only drawer partitions, only http(s)/about:blank (BD:245-269).
- Console capture (max 500 lines/tab) and per-tab network capture for chats' `browser_console` / `browser_network` (BD:177-197, 223-231).
- Saved passwords exist (save/update/fill bar, lock button, Passwords panel, PIN via `/api/lock`, `/api/lock/check`) — covered by the passwords spec. Chat reads are redacted of a tab's passwords (BD:749-754, MJ:2028).
- Chat cursor for `browser_mouse` on drawer tabs: glide 140-480ms (140 + 0.45·distance), linear while a button is held, idle fade 1.8s, held 60s; restores focus to a text field the person was typing in (BP:140-146, VD:690-766).

## 7. Web Browsers tab (WH:3737, 3823, 14129-14310)
- Tab button "Browsers" (globe). Loads on tab switch only (`loadBrowsers`, WH:11421); refetches 500ms after open/close. **No live updates.**
- `GET /api/browsers?alias=<project>` → `{ browsers:[{key, alias, name, url, width, height, status, createdAt, lastActivityAt, consoleLogCount, on:'server'|'desktop'}], places:{server, desktop} }` (WS:6848, SB:735-750).
  Lists the hidden MCP browsers only, not drawer tabs. Load error toast "Failed to load browsers: <err>".
- Toolbar: device select grouped Desktop/Apple/Google/Samsung/Microsoft, options "<name> (<w>×<h>)", default "iPhone 16 Pro"
  (its own 16-profile list, WH:14129-14146: Desktop 1280×720, Full HD 1920×1080, iPhone SE, X, 12 Pro, 14 Pro, 15 Pro, 16 Pro, 16 Pro Max, iPad Air, iPad Pro, Pixel 7, Pixel 9 Pro, Galaxy S24 Ultra, Galaxy Z Fold 5, Surface Pro 9).
  "Run on" select only when both places exist: "Run on: default" / "Run on this server" / "Run on the desktop app"; tooltip "Which machine runs it: this server reaches its own localhost, the desktop app reaches your network".
  URL field default `http://localhost:3000`, placeholder "URL to open"; button "+ Open".
- Open: `POST /api/browsers/open {alias, name:'browser-<ms>', url, width, height, on?}` → `{ok, error?}`. Toasts "Enter a URL first", "Browser opened (<device>)", "<error>" or "Failed to open browser", "Error: <msg>".
- Empty state: globe icon + "No browser instances open".
- Card per browser: name (`name || key || 'browser'`), status pill "on the server" / "on the desktop app" / "open"; URL field (placeholder "URL") + "Go";
  screenshot area "Click to take screenshot" (→ "Loading...", then image or red error "Screenshot failed"/"Error: <msg>"); buttons "Screenshot", "Close" (danger).
- Go: adds `http://` for localhost, else `https://` when no scheme; `POST /api/browsers/<key>/navigate {url}` → error toast "Navigate failed".
- Screenshot: `GET /api/browsers/<key>/screenshot` → `{ok, data:<b64 png>}`. Close: `POST /api/browsers/<key>/close` → toast "Browser closed" / "Failed".

## 8. Endpoints and messages

### 8.1 UI ↔ drawer host (preload `window.api.wbrowser.drawer`, PL:113-151)
Invoke (→ result `{ok, error?, …}`): `attach({tabId, project, network, wcId})` → `{ok, networkLabel}`; `detach(tabId)`; `navigate(tabId,url)` ("Only http and https addresses open here.");
`nav(tabId, 'back'|'forward'|'reload'|'stop')`; `emulate(tabId, config|null, scale)`; `devtools(tabId)`; `capture(tabId)` → `{data:b64, size:{width,height}}`;
`describe(tabId, [[x,y]…≤60])` → `{elements:[{selector, tag, text≤160, html≤300, rect:{x,y,width,height}}|null]}`; `info()`; `visible(tabId, bool)`;
`history(q, limit)` → `{items}`; `clear({history, cookies, cache, site?})` → `{cleared:[…], site?}` ("bad site"); `pw.*` (passwords spec).
Send: `ready()`; `reply(reqId, result)`. Events: `drawer:state {tabId, url, title, loading, canGoBack, canGoForward, logins, diag?, crashed?, sleeping?}`,
`drawer:info` (info shape 3.2), `drawer:request {reqId, cmd, args}`, `drawer:pw-offer`, `drawer:pw-fill-offer`.
In native these are in-process calls; keep the shapes for the server side.

### 8.2 Requests from host to UI (`drawer:request`, VD:1457-1548; timeout 20s, "The browser drawer did not answer (<cmd>)")
`list {project, all}` → `{tabs:[{tabId, project, url, title, network, sleeping, exit?, active, shown, loaded, device}]}`;
`open {project, network, show, activate}` → `{tabId}` (error "This project already has 12 drawer tabs: close one first.");
`ensure {tabId, project}` → `{project, network}`; `close {tabId}`; `discard {tabId}` ("The tab is on screen"); `resize {tabId, width, height}`;
`set-network {tabId, network}`; `annotate {tabId, shapes, clear, show}` → `{drawn, total}`; `annotations {tabId, image}` → `{annotations:[{n, by:'person'|'claude', kind, text, x, y, anchor, width?, height?, picked?, toX?, toY?, points?(count)}], viewport:{width,height}, image?}`;
`cursor {tabId, x, y, pressed, ms, linear, click, end}`. Unknown: `{ok:false, error:'unknown request <cmd>'}`. Errors: "No such drawer tab (it may have been closed)", "That drawer tab belongs to another project.", "That tab is not loaded".

### 8.3 Server ↔ client (native-host websocket, MJ:2096-2150, WS:9198-9224)
- Client → `{type:'register-native-host', capabilities:['notify','browser-touch','browser-drawer','browser-mouse'], deviceId}`; server → `{type:'native-host-ready', features:['net-tunnel','browser-drawer','browser-homepool']}`.
- Server → `{type:'native-host', payload:{type, requestId, key?, alias, …}}`; client → `{type:'native-host-result', payload:{type:'browserResult', requestId, ok, …}}`. Binary frames = net tunnel (3.2).
- Drawer handlers (BD:569-616): `browserDrawerList {alias, all}` → `{tabs:[…+networkLabel, sleeping]}`; `browserDrawerOpen {alias, url, network, show, hpAllowed}` → `{tabId, network, networkLabel, url, warning?}`
  (errors "Only http and https addresses open in the drawer.", "Which project? (no alias)", "A homepool tab needs the owner's approval for this chat.");
  `browserDrawerSetNetwork {key, network, hpAllowed}` → `{network, networkLabel, url}` ('network must be "client", "server" or "homepool:<network>"');
  `browserDrawerAnnotate {key, shapes:[{kind:pin|box|arrow|label, text, selector?, x,y,width,height,toX,toY}], clear, show}` (selector resolved to rect; "No element matches <sel>");
  `browserDrawerAnnotations {key, image}` → `{url, title, viewport, annotations:[…, element], image}`.
- Every generic `browser*` handler accepts key `drawer:<tabId>`; `prepare` first ensures/loads the tab, checks project ("That drawer tab belongs to another project."), homepool approval, and thaws it (BD:530-555). Unavailable: "The browser drawer is not available: the Crundi window is not showing a web UI that has it (open the app, or update the server)."
- HTTP used by the drawer UI: `GET /api/homepool/browser`, `POST /api/homepool`, `POST /api/attachments/upload`, `GET /api/lock`, `POST /api/lock/check`.

## Checklist
- [ ] Drawer column left/right, split drag (min 320, area ≥420), dblclick → 560; persisted open/width/side.
- [ ] Tab-bar "Browser" opener: hidden while open, dot + tooltip when hidden with tabs; errors for no project.
- [ ] Tab strip: badges SRV/HP (red when bad), tooltips incl. asleep/unloaded, middle-click close, `+`, `...`, hide, close-all.
- [ ] Per-project tabs keyed by lowercase alias; max 12; left neighbour on close; lazy restore; popups → new tab.
- [ ] Persist `crundi_brd` shape exactly (id, url, title, device, custom, network; active per project).
- [ ] Address bar: back/forward/reload(spin)/URL rules + Google search + history suggestions; lock; network; device; annotate; DevTools.
- [ ] Network popover with labels ("This PC", "Server (this PC)", "Server", "Server (needs a newer server)", "Server (not connected)", "Homepool: <n>") and homepool items.
- [ ] Network switch recreates the page in the right profile at current URL; keeps marks.
- [ ] Server-network loopback proxy + frame codec over ws/IPC; error pages; homepool `hp|<net>|` prefix and id ranges.
- [ ] Homepool panel, exit texts, 15s poll, key-add/key-update/browser-revoke.
- [ ] 19 devices with exact UAs; fit-and-scale box on dotted stage; CDP emulation; reload only on device change; custom from browser_resize.
- [ ] Annotation bar: select/pin/box/pen, note field, count text, clear-mine, done; fixed colours.
- [ ] Send selectors / screenshot to picked chat or Cru: exact text format, composite PNG, upload, insert into composer (not send), toasts.
- [ ] Menu: History panel, Passwords (other spec), Clear browsing data panel with all wording, side toggle.
- [ ] Sleep 5 min / unload 1 h; visibility reporting; thaw on chat use.
- [ ] Shared history file (5000, debounced) and profiles list; clear all / per site.
- [ ] Empty state, diag texts, chat cursor (glide, ring, ripple, "Claude" tag, focus give-back).
- [ ] All drawer requests (8.2) and browserDrawer* handlers (8.3) with the same payloads and error strings; redaction of passwords.
- [ ] Web Browsers tab: list/open/navigate/screenshot/close, run-on select, device list, wording, no live updates.
