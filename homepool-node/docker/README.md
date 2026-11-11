# Homepool node in Docker

A homepool node lends the connection of the machine it runs on to a Crundi
server: it holds one websocket to the server and opens, stream by stream, the
TCP connections the server asks for. It only dials out, listens on nothing and
needs no port forwarding. It refuses to open anything on its own machine or
private network (loopback, 10/8, 172.16/12, 192.168/16, 169.254/16,
100.64/10, `*.local`, …).

The code it runs is `app/homepool-node.js` (plain Node 22, no packages).

## Getting a node code

The owner of the Crundi server makes the node: **Settings › Homepool › Nodes ›
Add node**. Crundi shows its code, `crundi-homepool://node?s=…&k=hpn_…`, and a
ready-made `docker run` line. The key in it lets a node carry traffic for that
server and nothing else; it is not a Crundi login. The owner can switch the
node off or remove it there at any time, and the node then stops for good
(the container exits with code 3: ask for a new code).

## Way 1: no build (what Crundi shows you)

```bash
docker run -d --name crundi-homepool-node --restart unless-stopped \
  -e HOMEPOOL_CODE='crundi-homepool://node?s=…&k=hpn_…' \
  node:22-alpine sh -c 'wget -qO /n.mjs https://<server>/api/homepool/node.mjs && exec node /n.mjs'
```

The stock `node:22-alpine` image fetches the node script from the server
itself (`/api/homepool/node.mjs`, public, no sign-in) each time the container
starts, so it is always the server's own version.

## Way 2: this image

From the repository root:

```bash
docker build -f homepool-node/docker/Dockerfile -t crundi-homepool-node .
docker run -d --name crundi-homepool-node --restart unless-stopped \
  -e HOMEPOOL_CODE='crundi-homepool://node?s=…&k=hpn_…' crundi-homepool-node
```

The script is baked in, so the container starts without reaching the server
over HTTP first; rebuild it to pick up a newer node.

## Settings

| Variable | |
|---|---|
| `HOMEPOOL_CODE` | The node code. Or, instead: |
| `HOMEPOOL_SERVER` + `HOMEPOOL_KEY` | The server URL and the `hpn_…` key. |
| `HOMEPOOL_PLATFORM` | What the owner sees as the node's platform (default `docker`). |

`docker logs crundi-homepool-node` shows it connecting, its public IP, and
anything the server says to it.
