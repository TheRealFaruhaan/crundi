# Native host protocol (PC client as a server's browser host)

What the Electron client (app/) does when it attaches to a remote Crundi
server as its **native host**, and answers that server's browser requests.
The Rust client must speak exactly this. Refs are `file:line` at the time of
writing (main.js = app/main.js, drawer = app/browser-drawer.js,
page = app/vendor/crundi-browser-drawer.js, server = src/browser.js).

## 1. Connection and registration

### Who connects, when
- Only in **remote mode** (`client.json` `mode: "remote"` with a `serverUrl`),
  and only once the web UI has handed over its access token
  (`ipcMain.on('client:setToken')`, main.js:1647). A new token closes the
  socket and reconnects (main.js:1652-1654); switching mode stops it
  (main.js:1641). No separate credential is stored: the token is the one the
  web page uses.
- URL: `serverUrl` with `http` → `ws` + `/ws?token=<urlencoded token>`
  (main.js:2097).
- One socket at a time. Every handler is bound to *its* socket (`isCurrent()`);
  a superseded socket that opens late closes itself (main.js:2104-2114).
  The Rust client must do the same (a late-opening socket must not register).

### Register (client → server), on open (main.js:2118)
```json
{ "type": "register-native-host",
  "capabilities": ["notify", "browser-touch", "browser-drawer", "browser-mouse"],
  "deviceId": "pc-<random><time36>" }
```
- `deviceId` = `hpDeviceId()` (main.js:1567): read from the homepool config
  file; created once as `'pc-' + Math.random().toString(36).slice(2) +
  Date.now().toString(36)` and persisted. The server truncates it to 80 chars
  (src/webapp.js:9207) and uses it (a) for the drawer's homepool streams and
  (b) in browser-gate.js:32 to refuse a homepool network this PC is itself a
  node of. It must be stable across runs.
- Capabilities and what the server gates on them (server:137 `hostSupports`):
  | capability | server effect if absent |
  |---|---|
  | `notify` | "desktop" notification channel unavailable (src/webapp.js:1418) |
  | `browser-touch` | `browser_touch` → "Touch is not supported by the desktop app…" (server:621-633) |
  | `browser-mouse` | only `move`/`click` allowed; others → "The mouse action "X" is not supported…" (server:551-571) |
  | `browser-drawer` | no drawer: `drawerHosts()` excludes this host (server:934-948) |
- The server ignores a second register on the same socket, and any register
  from a collaborator (`confined`) socket (src/webapp.js:9199-9200). Only the
  owner's token can host.

### Ready (server → client) (src/webapp.js:9217)
```json
{ "type": "native-host-ready", "features": ["net-tunnel", "browser-drawer", "browser-homepool"] }
```
The client stores `features` (`hostFeatures`, main.js:2127) for the drawer's
"server network" (drawer-net.js `remoteFeatures()`), then refreshes the
drawer's network labels (`browserDrawer.notifyInfo()`). Older servers send no
`features`: treat as `[]`.

### Server → client envelope
Every server message is `{ "type": "native-host", "payload": <msg> }`
(src/webapp.js:9202). `payload` is either:
- a **request**: has `type` and a numeric `requestId` (server:242-250); or
- a **notice**: no `requestId`. Two exist:
  - `{ type: "notify", title, body, url? }` (server:150, webapp.js:1423):
    show an OS notification. Title ≤120, body ≤400 chars; `url` only of the
    form `/chat/<id>` (re-checked by the client, `^/chat/([A-Za-z0-9_-]{1,80})$`);
    clicking focuses the window and opens that chat (main.js:1975-1993).
    Keep a reference to the notification until clicked/closed (main.js:1972).
  - `{ type: "browsers", data: [ …listBrowsers() ] }` (server:897-899): sent
    after every open/close/navigate/resize of a hidden browser. The client
    ignores it (main.js:2049, no requestId → dropped).
- Unknown request types are silently dropped (main.js:2050) — no reply, so
  the server times out (40 s). The Rust client should instead reply
  `{ ok:false, error:"<type> is not supported by this app" }` (backwards
  compat rule: never hang).

### Reply (client → server)
```json
{ "type": "native-host-result",
  "payload": { "type": "browserResult", "requestId": <same>, "ok": true, ...fields } }
```
(main.js:2029, 2135). Errors: `{ type:"browserResult", requestId, ok:false,
error:"<message>" }` (main.js:2031) — any thrown error / rejected promise /
timeout becomes this. The server resolves the pending promise with the whole
payload (server:276-281) and reads named fields from it.

### Per-request timeouts
- Client side (`withTimeout`, main.js:1913, `browserActionTimeout`
  main.js:1951): `browserWait` = `(timeout||30000)+3000`;
  `browserOpen`/`browserNavigate`/`browserDrawerOpen`/`browserDrawerSetNetwork`
  = 35000; everything else 25000. Error text:
  `"<type> timed out after <ms>ms (page may be stalled or showing a dialog)"`.
- Server side backstop (server:24, 243-246): 40000 ms →
  `"Browser request timed out"`; `browserWait` gets `waitMs+10000`,
  `browserMouse` `min(duration,10000)+(drag?1000:0)+40000`, `browserTouch`
  `sum(step.wait)+40000`, `browserDrawerList` 15000.

### Disconnect and reconnect
- On close: forget `hostFeatures`, drop drawer server-network streams
  (`serverNet.dropped()`), refresh drawer labels, and retry after **5 s**
  (main.js:2139-2153). No backoff growth.
- Server side on socket close (src/webapp.js:9341 → server:92-110): every
  hidden browser that lived on this host is forgotten, and every in-flight
  request is rejected with `"The desktop app disconnected"`. Later calls on
  such a key: `"The desktop app this browser ran on has disconnected."`
  (server:236). Drawer tabs are not tracked server-side, so they survive a
  reconnect (they live in the client).

### Binary frames (drawer "server" / homepool networks)
Binary websocket frames in both directions are the net tunnel
(app/net-tunnel-client.js:21-40, src/net-tunnel.js:12-26): `[type u8][id u32
BE][payload]`, types OPEN=1 (client→server, payload `"host:port"`, optionally
prefixed `hp|<network>|`), DATA=2, CLOSE=3, OPENED=4, ERROR=5. Byte 1 (top
byte of id) selects which local proxy owns the stream. Only used when the
server's `features` include `net-tunnel`. Separate spec; listed here because
it shares the socket (main.js:2124).

## 2. Keys, places and drawer vs hidden browsers

- **Hidden browser** key: `"<alias>:<name>"` (server:304, name default
  `default`). The client makes an offscreen `BrowserWindow` per key with its
  own in-memory partition `mcp-browser-<key>` (main.js:2252-2262). Limits:
  client max 20 (main.js:2255); server max 5/project, 20 total (server:20-21).
  Server idle-closes after 10 min (server:22, 291-300) by sending
  `browserClose`.
- **Drawer tab** key: `"drawer:<tabId>"`, tabId `^[A-Za-z0-9_-]{4,64}$`
  (drawer:47-55). Lives in the visible drawer, owned by the person; the
  server keeps no record beyond which host holds it (server:902-931).
- `on` (browser_open): `"server"` → the server's own parent/headless, never
  this client; `"desktop"` → this client (remote host) if attached;
  omitted → parent, else remote, else headless (server:191-208);
  `"drawer"` → `browserDrawerOpen` (server:318). Invalid →
  `'on must be "server", "desktop" or "drawer"'`. The choice is fixed per key
  (`keyBackend`, server:76).
- For every request whose `key` is a drawer key the server adds
  `alias` (calling project) and `hpAllowed` (bool) (server:215-226). Before
  any handler except `browserClose`/`browserResize`, the client runs
  `browserDrawer.prepare(msg)` (main.js:2023-2025, drawer:530-555):
  - bad tabId / unknown → `"No such drawer tab"`;
  - tab known to the page but not loaded → asks the page to `ensure` it and
    waits ≤20 s (`"The drawer tab did not load"`);
  - `msg.alias` ≠ tab's project → `"That drawer tab belongs to another project."`;
  - tab on `hp:<net>` and `!hpAllowed` → `"This tab goes out through homepool
    (<net>). A chat may use homepool tabs only once the owner approves it: …"`;
  - wakes a frozen (sleeping) tab.
- After any handler on a drawer key the result is passed through
  `browserDrawer.redact(key, result)` (main.js:2028, drawer:750): saved/typed
  passwords of that tab are replaced in every string field.
- Generic handlers find the target by `browserEntry(key)` = hidden window or
  drawer tab entry (main.js:2246); a drawer entry exposes
  `entry.win.webContents` so the same code drives it (drawer:199-212).
  Not found → `"Browser not found"`.
- Drawer → page requests (`request(cmd,args)`, drawer:140-151) fail with
  `"The browser drawer is not available: the Crundi window is not showing a
  web UI that has it (open the app, or update the server)."` when the page
  has not sent `drawer:ready`, and `"The browser drawer did not answer (<cmd>)"`
  after 20 s. Page errors come back as their message (e.g. `"No such drawer
  tab (it may have been closed)"`, `"This project already has 12 drawer tabs:
  close one first."`, page:1471).

## 3. Every request the server sends, and app/'s answer

Common: every request has `type`, `requestId`, `key` (except DrawerList/Open).
"wc" = the target's webContents. "eval" = `wc.executeJavaScript(code)`
(main world; its rejection is the error). All replies also carry
`type:"browserResult", requestId` (omitted below).

| type (server line) | request fields | app/ does (main.js line) | reply | server reads |
|---|---|---|---|---|
| `browserOpen` (340) | key, url, width, height (clamped 200..1920 × 200..1080, default 1280×720) | 2252: dup key → `Browser "<key>" already open`; >20 → `Too many browser instances (max 20)`. `new BrowserWindow({show:false, partition:"mcp-browser-"+key, contextIsolation, no node})`; console capture; `will-prevent-unload` → preventDefault; on `did-start-loading` and `dom-ready` inject `window.alert=function(){};window.confirm=function(){return true;};window.prompt=function(){return null;};window.onbeforeunload=null;`; webRequest capture hooks; `loadUrlWithGrace(url)` (main.js:1925: resolves on did-stop-loading / did-finish-load / 15 s grace; rejects only on main-frame did-fail-load with code ≠ -3 → `Load failed (<code>): <desc>`, and then destroys the window) | `{ok:true}` | ok, error |
| `browserClose` (375) | key | drawer key → `drawer.close` (page `close` cmd; prepare runs inside, drawer:756). Else remove webRequest listeners, destroy window (2333). Unknown → `Browser not found` | `{ok:true}` | nothing (errors ignored, server:376) |
| `browserNavigate` (399) | key, url | `loadUrlWithGrace(wc,url)` (2346) | `{ok, url: wc.getURL()}` | ok, error |
| `browserScreenshot` (417) | key | `wc.capturePage().toPNG()` base64 (2353). Drawer tabs stay laid out at opacity 0 when hidden so capture works (drawer:7-10) | `{ok, data}` (PNG base64) | data |
| `browserReadPage` (433) | key | eval `(document.body.innerText \|\| '').substring(0, 50000)` on the **live** body (2361) | `{ok, title: wc.getTitle(), url, text, consoleLogs: last 20}` | text, url, title, consoleLogs (keeps last 200) |
| `browserSource` (691) | key | eval `document.documentElement.outerHTML` (2385) | `{ok, html}` | html, url, title (url/title never sent) |
| `browserClick` (487) | key, selector | eval `document.querySelector(<sel>)?.click()` (2392). Only sent as a fallback when the server's findScript (via browserEval) matched only a hidden element | `{ok:true}` always (even if nothing matched) | ok, url |
| `browserType` (513) | key, text, selector? | if selector, eval `querySelector(sel)?.focus()`; then `wc.insertText(text)` (2401) | `{ok:true}` | ok, url (not sent) |
| `browserFill` (846) | key, selector, value | eval: querySelector, `throw new Error('Element not found')` if none, `focus()`, `el.value=v`, dispatch `input` and `change` (bubbles) (2413) | `{ok:true}` | ok, url (not sent) |
| `browserSelect` (—) | key, selector, value | handler exists (2429: set value, dispatch `change`) but the **server never sends it**: browser_select runs `selectScript` through `browserEval` (server:860-876). Keep for compat | `{ok:true}` | — |
| `browserEval` (677) | key, code | eval code (2443) | `{ok, result: string or JSON.stringify(value)}` (undefined → field absent) | result. Server builds browser_click, browser_select, the select picker, point lookups and touch probes on this (findScript/pointScript/selectScript/pickerScript, src/browser-find.js; probe scripts, src/browser-touch.js) — so it must return the script's JSON string verbatim |
| `browserMouse` (591, 601) | key, x, y, action ∈ move,click,double_click,down,up,drag; button? left/right/middle; toX,toY (drag); duration ms ≤10000 | 2452: attach debugger `wc.debugger.attach('1.3')`; `runMouse` (app/browser-pointer.js:61): validates (`mouseError`, :42 — e.g. `drag needs toX and toY (where to release)`, `up takes both x and y, or neither`, `button must be "left", "right" or "middle"`, `<action> needs x and y`); CDP `Input.dispatchMouseEvent` mouseMoved/mousePressed/mouseReleased with `button`, `buttons` (1/2/4), `clickCount`. Pointer state `{x,y,down}` persists per entry across calls. Held-button travel interpolates `clamp(ms/16,2,120)` steps; glide time `clamp(140+d*0.45,140,480)` ms. double_click = click, 60 ms, press/release with clickCount 2. drag default 500 ms. Drawer tab: before each step `drawer.cursor(key,p)` → page `cursor` cmd `{tabId,x,y,pressed,ms,linear,click,end}` draws the chat's cursor; final `end:true` gives keyboard focus back (drawer:622-634, page:1521) | `{ok, url, x, y, held: button\|null}` | url, x, y, held |
| `browserTouch` (646) | key, steps: [{type: touchStart/Move/End/Cancel, points:[{x,y,id}], wait}], listenersAt?: {x,y} | 2471: debugger attach; once per entry `Emulation.setTouchEmulationEnabled {enabled:true,maxTouchPoints:5}`; `Page.startScreencast {format:jpeg,quality:1,maxWidth:40,maxHeight:80}` (acking each `Page.screencastFrame`) so a hidden page keeps rendering, stopped 1.5 s after; if listenersAt, `touchListenersAt` (main.js:2158: `Runtime.evaluate` element chain at point → `Runtime.getProperties` → `DOMDebugger.getEventListeners` per node, types `/^(touch\|pointer\|mouse\|click\|dblclick\|contextmenu\|wheel\|gesture\|drag)/`, skipping handlers named crundiTouch*, then `Runtime.releaseObjectGroup`); each step `Input.dispatchTouchEvent {type, touchPoints}` then sleep `wait` | `{ok, url, listeners?: [{el, listeners:[{type,capture?,passive?,handler≤120}]}]}` | listeners |
| `browserResize` (531) | key, width, height (clamped) | drawer key → `drawer.resize` → page `resize` (tab device becomes `custom` w×h clamped 200..3840; page:1495). Else `win.setContentSize(w,h)` (2508) | `{ok:true}` | ok |
| `browserScroll` (782) | key, x?, y?, selector? | eval `(selector ? querySelector(sel) : window).scrollBy(x\|\|0, y\|\|0)` (2516) | `{ok:true}` | ok |
| `browserWait` (797) | key, selector, timeout (default 30000) | eval a Promise: resolve if `querySelector` matches, else MutationObserver on `document.body` (childList, subtree), reject `new Error('Timeout waiting for ' + selector)` after timeout (2528) | `{ok:true}`; on timeout error = rejection message | found (never sent) |
| `browserGoBack` (758) | key | `wc.goBack()`, no wait (2546) | `{ok:true}` | url (not sent) |
| `browserGoForward` (770) | key | `wc.goForward()` (2553) | `{ok:true}` | url (not sent) |
| `browserConsole` (705) | key, clear, start, end, countOnly | 2560: countOnly → `{ok, count}`; else `logs.slice(start\|\|0, end\|\|len)`, then if clear empty the buffer | `{ok, logs:[{time,text}]}` or `{ok,count}` | logs, **total** (never sent) |
| `browserNetwork` (725) | key, action start/stop/log/clear, start, end, countOnly | 2572: start → recording=true, buffer emptied; stop → false; clear → empty; log → countOnly `{recording,total}` else `{requests: slice(start\|\|0, end), recording, total}`; else `Unknown network action` | see left | requests, recording, total |
| `browserCookies` (809) | key, action get/set/delete, cookie, filter, url | 2587: get → `session.cookies.get(filter\|\|{})`; set → `cookies.set(cookie)` (needs cookie); delete → `cookies.remove(url \|\| wc.getURL(), cookie.name)`; else `Unknown cookie action` | `{ok, cookies}` (get) / `{ok}` | cookies |
| `browserSnapshot` (821) | key, maxDepth (10), maxLength (50000) | eval a recursive walk from `document.body`: `{role: getAttribute('role') \|\| tag, label: aria-label \|\| textContent[0..80] trimmed, children?}` to maxDepth; `JSON.stringify(tree,null,2).substring(0,maxLength)` (2608) | `{ok, snapshot}` | **tree**, url, title, length (none sent) |
| `browserElements` (833) | key, selector (default `a,button,input,select,textarea,[role="button"],[onclick]`), start, end, countOnly | eval map of querySelectorAll to `{tag,id,class,text≤80,type,href,value}` (2625) | `{ok, elements: slice(start,end)}` or `{ok, count}` | elements, **total**, url, title (none sent) |
| `browserPdf` (883) | key, landscape | `wc.printToPDF({landscape, printBackground:true})` base64 (2644) | `{ok, data}` | data |
| `browserDrawerList` (978) | alias, all (Cru with no alias) — **no key** | drawer:570: page `list` cmd; each tab gets `networkLabel` and `sleeping` | `{ok, tabs:[{tabId, project, url, title, network, sleeping, exit?, active, shown, loaded, device, networkLabel}]}` (page:1277-1282) | tabs → keys `drawer:<tabId>` (server:980-987) |
| `browserDrawerOpen` (1015) | alias, url (default about:blank), network client/server/hp:<n>, show, hpAllowed — no key | drawer:575: non-http(s) → `Only http and https addresses open in the drawer.`; no alias → `Which project? (no alias)`; hp without approval → `A homepool tab needs the owner's approval for this chat.`; page `open {project,network,show,activate:show}` (≤12 tabs/project), wait for the webview to attach, `loadUrl` (a load failure becomes `warning`, not an error) | `{ok, tabId, network, networkLabel, url, warning?}` | all |
| `browserDrawerSetNetwork` (1034) | key, network, alias, hpAllowed | drawer:582: prepare; bad → `network must be "client", "server" or "homepool:<network>"`; hp check; page `set-network` (moves the tab to that network's partition, reloads) | `{ok, network, networkLabel, url}` | all |
| `browserDrawerAnnotate` (1070) | key, shapes (≤50, kind pin/box/arrow/label, x,y,width,height,toX,toY,text,selector; validated server:1043), clear, show | drawer:590: selector shapes resolved by eval `rectOf` (scrolls into view if off-screen): box → rect; arrow → to centre, from `x??r.x-60`; else centre. `No element matches <sel>` / `bad selector: <sel>`. Page `annotate`: clear removes `by:'claude'` marks; show opens the tab | `{ok, drawn, total}` | drawn, total |
| `browserDrawerAnnotations` (1089) | key, image | drawer:607: page `annotations` (`That tab is not loaded` if no box); each annotation gets `element` (= picked, `{selector,tag,text,html,rect}` from `describe`) | `{ok, url, title, viewport:{width,height}, annotations, image: png b64\|null}` | all |

Tool → request map (src/browser-dispatch.js:20-75): browser_open/drawer_open
→ Open/DrawerOpen; browser_click → Eval (findScript) or Mouse (x,y), Click
fallback; browser_select → Eval; browser_view_source → Source; browser_point
→ DrawerAnnotate with one pin; browser_tab_network → DrawerSetNetwork;
browser_list → DrawerList (+ server's own hidden list); browser_touch → Eval
(probe install) + Touch + Eval (probe collect, after 700 ms). There is no
`browserDevice`, `browserPoint`, `browserMarks` or `windowSnapshot` in the
server.

Point/Annotate/Annotations/TabNetwork require a drawer key; others get
`Only drawer tabs can do that (keys starting "drawer:", see browser_list).
Open one with browser_drawer_open.` from the server (server:1023).

### Server-side shape mismatches (match app/ on the wire, add the read field too)
The headless backend (src/browser-headless.js) returns the same shapes as
app/, so these are server bugs, not client ones. The Rust client should send
app/'s fields **and** the ones the server reads, which is harmless to older
servers:
- Console: send `total` (and `count` for countOnly).
- Snapshot: send `snapshot` and `tree` (same string), plus `url`, `title`,
  `length`.
- Elements: send `total` (and `count`), `url`, `title`.
- Source: add `url`, `title`. Type/Fill/GoBack/GoForward/Click: add `url`
  (after go back/forward, wait for the navigation to commit first).
- Wait: add `found: true`.

## 4. Console and network capture

- Hidden browser console (main.js:2263): every `console-message` pushed as
  `{time: Date.now(), text}` — **no level**, **unbounded**. ReadPage returns
  the last 20; the server keeps ≤200 (server:23, 437).
- Drawer tab console (drawer:223-232): `{time, text}`, capped at **500**
  (`MAX_CONSOLE`, drawer:51, oldest dropped). Messages starting with the
  per-run password prefix (`⁣crundi-pw:<uuid>:`) are swallowed and never
  logged (drawer:228, 709). Rebinds when the tab's webContents changes.
- Network (hidden: main.js:2286-2316; drawer: one hook per session, entries
  routed to the tab by `webContentsId`, drawer:178-197): only while
  `recording`. `onBeforeSendHeaders` pushes `{id, url, method, resourceType,
  requestHeaders, timestamp}`; `onCompleted` adds `statusCode, statusLine,
  responseHeaders, fromCache, completedAt`; `onErrorOccurred` adds `error,
  completedAt`. No bodies, **no cap**, `start` clears. In CEF: use
  CefResourceRequestHandler / CefRequest + CefResponse, or CDP `Network.*`,
  matching these field names; cap it (suggest 1000) rather than grow forever.

## 5. When the server calls the drawer "available"

`browser_list` returns `{ browsers, drawer: "available" | <reason>,
openTab? }` (server:995-1005). It is "available" iff `drawerHosts()` is
non-empty and listing did not fail with zero tabs. For a remote client that
means **all** of:
1. Its socket is registered as native host (`remoteHost` set), and
2. its `capabilities` include **`browser-drawer`** (server:937);
3. `browserDrawerList` answers `{ok:true, tabs:[…]}` within 15 s — an error
   with no tabs makes `drawer` the error text (server:1000).
Also not for a collaborator's chat or a project with `noDrawer` ("The browser
drawer is not available on this project."). Unavailable texts:
`The browser drawer is not supported by the Crundi desktop app connected to
this server. Update the app.` (registered without the capability) or `The
browser drawer is in the Crundi desktop app, and none is connected to this
server.` (server:945-948). A server with an Electron parent always lists the
parent first (server:936), so a remote client only gets drawer calls from a
headless/container server.

## 6. What the native prototype (native-client/src/server.rs) lacks or does differently

Registration/transport:
- Registers `capabilities: ["browser-mouse"]` only and `deviceId:
  "native-prototype"` (server.rs:24): no `notify`, `browser-touch`,
  `browser-drawer` → the server reports the drawer unavailable and refuses
  touch. Needs the persistent `pc-…` deviceId shared with homepool.
- URL is passed in; no token hand-off from the logged-in page, no reconnect
  on token change. Retries every 3 s (app/: 5 s). Ignores `features`.
- Ignores binary frames (no net tunnel) and the `notify` notice (shows no
  notifications). Unknown types reply `"<type> is not in this prototype"`
  (fine; keep a reply).
- Plain sockets only get a read timeout; replies are only flushed between
  reads (30 ms poll).

Requests:
- `browserOpen`: maps a key onto one of **two** fixed panels; third key →
  `"Both panels are in use: close one first"`. Ignores width/height,
  duplicate-key check, isolation (no per-key partition), dialog suppression,
  load-failure error.
- Answered roughly like app/: Close, Navigate (+url), Eval (non-string →
  `to_string()`, so `undefined` becomes `"null"`), Screenshot (CDP
  `Page.captureScreenshot` on GPU path, else last painted frame), Resize.
- `browserReadPage`: `consoleLogs: []` always (no capture).
- `browserClick`: returns `"Element not found"` when nothing matches (app/
  always ok).
- `browserMouse`: CEF `send_mouse_*` events, left button only, actions
  move/down/up/click; no double_click/drag/button/duration, no persistent
  pointer state, `held` always null, no CDP (cross-site iframes) and no
  drawer cursor. Yet it advertises `browser-mouse`, so the server lets
  double_click/drag through and they fail.
- Missing entirely: Source, Type, Fill, Select, Touch, Scroll, Wait,
  GoBack, GoForward, Console, Network, Cookies, Snapshot, Elements, Pdf, and
  every drawer request (DrawerList, DrawerOpen, DrawerSetNetwork,
  DrawerAnnotate, DrawerAnnotations), plus prepare's project/homepool checks
  and password redaction.
- Prototype-only, not sent by any server: `windowSnapshot`, `browserDevice`,
  `browserPoint`, `browserMarks`. The real server's browser_point is
  `browserDrawerAnnotate` with one `pin` shape.
- No per-request timeout guard: a stalled CEF call never replies, and the
  server waits its 40 s backstop.
