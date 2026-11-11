# Homepool in Crundi

A shared exit. Traffic from chats, software (scrapers, scripts, containers)
and, later, devices leaves through a **network** the owner chose. Nothing
leaves without a key he made or approved.

Code: `src/homepool.js` (engine), `src/homepool-store.js` (record),
`src/homepool-service.js` (routes, node socket, MCP), `src/chat-token.js`,
`app/homepool-node.js` (the node and its wire format), `app/vendor/crundi-homepool.js`
(the web tab), `android/.../homepool/` (the Android node core) and
`android/.../ui/operations/Homepool.kt` (the app's screen),
`homepool-node/` (the standalone Android node app and the Docker node).
Tests: `scripts/test-homepool.mjs`, `scripts/test-homepool-service.mjs`.

## Networks

| Network | What it is |
|---|---|
| `server` | The server opens the connection itself. Built in, always up, **the default** for a key. |
| `home`, `alis-place`, … | The owner's own networks. He makes them, and puts each node in the one it is really on. |
| `pool` | Built in, a bridge: the standalone homepool relay (homepool-relay / homepool-chisel) as it runs today, as one network. Slot N of a key goes to the relay's slot N, so identities keep their lines. Switch it off in the tab when it is no longer needed. |

A **node** is a device that lends its connection: the Crundi Android app or
the PC app ("Share this connection"), the standalone Homepool Node Android
app, or the Docker node. It holds one websocket to `/api/homepool/node`,
signed in with its **node key** (`hpn_…`, not a Crundi login), and the server
asks it, stream by stream, to open TCP connections. It only dials out,
listens on nothing, and refuses anything on its own private network. A node
not in a network carries nothing.

Within a key's networks, slots work as the standalone relay's do: a slot is
bound to one exit and stays there, is re-drawn after 6 h only if another exit
is up, and is released at once when its exit leaves. A key with two networks
spreads its slots over both. With no exit up a connection is **refused**;
leaving from the server instead happens only if the key says so
(`fallbackToServer`, off by default).

## Access control

**Nothing uses homepool without a key.** Only the owner's session can make,
change or revoke one: the routes are owner only, collaborators are refused by
the access policy, and collaborator chats get none of the MCP tools.

A key (`data/homepool.json`, mode 600, secrets kept only as hashes):

- **who**: a *chat* (one chat), a *project* (every chat of it), or *software*
  (a named login). Software that cannot send a proxy login (Chromium with
  SOCKS) gets a **port block** instead: ports that take no login and accept
  connections only from the sources the owner lists (Docker network names,
  `loopback`, address ranges).
- **networks** it may use (one or more), and fallback to the server or not;
- **limits**: expiry, data per day, number of slots;
- **on / off**.

Off, revoked, expired or over its cap: refused from that moment, and its open
connections are cut. A software key's secret is shown once and can be
reissued. Every change, refusal and node coming and going is in the Activity
log.

### Chats ask, the owner decides

`homepool_request` (MCP) puts a request in the owner's inbox and on his
channels and waits up to 10 minutes. He approves it, approves it changed
(other networks, expiry, cap, the whole project, a port block) or refuses it.
Nothing is made before that. An approved software key's secret is handed to
the asking chat once.

Which chat is asking is proven, not claimed: every chat Crundi starts gets a
`CRUNDI_CHAT_TOKEN`, an HMAC of its id and project with a secret that exists
only in the server's memory, and the MCP bridge sends it. One chat cannot
name another. The limit: chats run as the same system user, so a chat that
reads other processes' environments could find another chat's token.

### MCP tools (owner chats)

- `homepool_status`: what this chat may use, what is up, its pending requests.
- `homepool_request`: ask (see above).
- `homepool_proxy`: a 12-hour proxy login for this chat, `socks5h://` and
  `http://` on 127.0.0.1, within its key.
- `homepool_fetch`: one HTTP(S) request through its key, the answer returned.

The server network never reaches the server's own private addresses (no
`127.0.0.1:8888` through a proxy).

### Where the owner manages it

The **Homepool** tab on the web page and the PC app, and Settings › Homepool
in the Android app:

- **Networks**: Server, Standalone homepool (on/off), his networks, their
  nodes (status, network, public IP, up for, data today, slots, open
  streams), move a node, switch it off, new key, remove. **Add node** gives a
  code, its QR and a one-line Docker command for a friend.
- **Access**: every key, on/off, usage, "how to use", change, new secret, revoke.
- **Requests**: approve, change and approve, refuse.
- **Activity**: the log.
- **This PC** (PC app) / **Share this phone's connection** (Android): this
  device as a node, with its status and stats.

## Ways in

Listening on 127.0.0.1 and the Docker bridge gateways only (docker0, br-*),
never a public interface. Production: SOCKS5 on 7480, HTTP on 7481, port
blocks from 7490 to 7579. A dev instance (`CRUNDI_DEV=1`) uses 7580 and up.
`HOMEPOOL_PORT` moves the base, `HOMEPOOL_DISABLE=1` turns it off.

- SOCKS5 login: user `<key id>`, `<key id>@<network>` to pin a network,
  `#<n>` for slot n; password the key's secret.
- HTTP proxy: the same login as `Proxy-Authorization`. `GET /status` answers
  with the key's status.
- Port block: `P` = any slot, `P+N` = slot N, `P+slots+1` = status.

The status answer carries the standalone relay's fields (`liveCount`,
`egressAvailable`, `allowFallback`, `fallbackInUse`), so software written for
the relay checks it unchanged.

## Nodes for a friend

Settings › Homepool › Networks › **Network** (e.g. "Ali's place"), then
**Node** in it. Give the friend the code (or its QR) for the Homepool Node
Android app (`homepool-node/android`), or the Docker line:

    docker run -d --name crundi-homepool-node --restart unless-stopped \
      -e HOMEPOOL_CODE='crundi-homepool://node?s=…&k=hpn_…' \
      node:22-alpine sh -c 'wget -qO /n.mjs https://<server>/api/homepool/node.mjs && exec node /n.mjs'

It needs no image of ours and no Crundi sign-in. The key lets that machine be
that one node and nothing else; "New key" or "Remove" ends it.

## Switching the Instagram scraper (the owner's choice)

Nothing in insta-scraper or the standalone homepool is changed by this work.
To switch, on a Crundi that has this:

1. Homepool › Access › **+ Access**: For *Software*, name `insta-scraper`,
   network *Standalone homepool* (same lines and slots as today; later *Home*
   once the nodes run the Crundi app), Slots 8, *Port block*
   from `homepool, insta-scraper_default` (the scraper is on both, and which
   one its traffic to the host leaves by is Docker's choice; today `homepool`).
2. "How to use" shows the lines. In `insta-scraper/docker-compose.yml`, service
   `instagram-scraper`:

       PROXY_URL: socks5://host.docker.internal:<port>
       PROXY_STATUS_URL: http://host.docker.internal:<status port>/
       NO_PROXY: localhost,127.0.0.1,db,ocr-server,warp,homepool-relay,host.docker.internal
       extra_hosts: ["host.docker.internal:host-gateway"]

   and `docker compose up -d instagram-scraper`.
3. Back: the two old lines (`socks5://homepool-relay:1080`,
   `http://homepool-relay:1099/`).

Moving to the Crundi nodes later is a change of the key's network in the tab;
the scraper is not touched again.

## The PC browser key (the desktop app's browser drawer)

A drawer tab can go out through a homepool network. The owner makes **one**
key of kind `browser` ("PC browser") in the drawer (network icon, Homepool:
set up) or edits it there and in Access: which networks, on/off, fallback.

- **How it flows.** The tab's profile (one per network: Chromium sets a proxy
  per profile) uses a loopback proxy in the app that tags each stream
  `hp|<network>|host:port` and sends it over the app's existing native-host
  websocket (or, for a bundled server, its IPC channel). The server's
  `net-tunnel.js` hands it to `browserConnect()`, which opens it with
  `engine.open()` on that network's **slot 1** and `splice()`s it, so usage is
  counted and switching the key off cuts it. No port is opened anywhere.
- **One line per network.** Every tab on "Home" leaves through the same exit,
  so its tabs keep one address. The drawer shows the exit (node and IP), or
  "No exit is up"; with no exit the page says so and nothing leaves another
  way unless the key's fallback is on.
- **Chats ask first.** A chat opening or switching a tab onto homepool makes a
  request of kind `browser` (proved by its chat token) and waits for the
  owner. Approving adds the chat (or, changed so, its project) to the key's
  `browserChats` / `browserProjects`; the drawer lists them and removes them.
  A chat that is not approved cannot use an existing homepool tab either.
  Cru is the owner's own assistant and is not asked.
- **Not through itself.** A network the drawer's PC is a node of (a node whose
  `deviceId` is the app's) is not offered, is refused to chats, and refused
  by `browserConnect()`.
- Code: `src/homepool-store.js` (kind, grants), `src/homepool-service.js`
  (`browserStatus`, `browserConnect`, `browserAsk`, `/api/homepool/browser`),
  `src/browser-gate.js`, `app/drawer-net.js`. Tested end to end in
  `scripts/test-browser-drawer.mjs`.

## Later (not in this card)

Devices *using* an exit: the PC app's tabs and the phone leaving through home
or the server (a VpnService on Android). The key model covers it.
