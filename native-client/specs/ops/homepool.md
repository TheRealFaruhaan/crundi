# Homepool (tab `homepool`)

Web: `app/vendor/crundi-homepool.js`, `loadHomepoolTab` (src/webapp-html.js).
Native: `ui/ops/homepool.slint`, `src/ops/homepool.rs` (pure rules
unit-tested). Test: `test/ops/homepool.mjs`. Screens: `screens/ops/homepool-*.png`.

## Parts

- Nav: Networks (node count), Access (key count), Requests (pending; accent
  pill when any), Activity; on Networks "+ Network" and "+ Node" (primary),
  on Access "+ Access".
- Summary: nodes up, keys in use, data today, waiting for you.
- **Networks**: the hint line; a card per network: Server ("always up", its
  line), Standalone homepool (bridge off / n node(s) up / down, the switch,
  its line, nodes up, error), own networks (n up, Rename, Remove; its nodes
  or "No node in this network yet…"); "Not in a network" for loose nodes.
  A node: dot, name, state chip (connected / paused: reason / offline /
  switched off), its line (kind · platform, network, IP, up, open or last
  seen, slots, today, mobile cap), network select, on/off, Rename, New key,
  Remove.
- **Access**: empty line; a card per key: name, its networks, state
  (refusal / exit up / leaving from the server (fallback) / no exit up),
  on/off; its line (who, today, connections, refused, used, open, cap,
  expires, fallback, ports); How to use / Hide (login, SOCKS, HTTP, status,
  from a container; port block for compose; each code with Copy), Change,
  New secret (software), Revoke.
- **Requests**: empty line; per request: kind "name", networks, status;
  from chat …, when; reason; Approve, Change and approve, Refuse (dimmed
  once decided).
- **Activity**: time, what, subject.
- Dialogs: Add a node (name, network) → "Node … is ready" (warning, QR,
  code + Copy, docker line, no public address warning); Give access /
  Change access / Approve with changes (For, Name, Project, Chat id,
  networks, fallback, Expires, cap, slots, port block + who may use it) →
  the secret shown once. Confirms and prompts are the shared ones.
- Live: GET every 3s while shown, not while a dialog is open.

## Endpoints

`GET /api/homepool`, `POST /api/homepool {action}` (network-add/-rename/
-remove, node-add/-update/-reissue/-remove, key-add/-update/-reissue/
-remove, request-decide, settings). A 404 says "This server does not
support homepool yet. Update the server to use it."

## Differences

- "This PC" (the desktop app lending its connection as a node) is the
  desktop area's homepool node; not drawn here until desktop offers it.
- Expires is a text field (YYYY-MM-DD HH:MM, local) for the web's
  datetime-local picker.
- The tests cannot make a chat's request (it needs a chat's MCP token), so
  Requests is checked empty end to end; its rows are built by the same
  render as the rest.

## Checklist

- [x] networks: add, rename (prompt), remove (confirm), bridge on/off
- [x] nodes: add (validation, setup with QR), move, on/off, rename, new key, remove
- [x] access: add (validation, secret), how to use, on/off, change, revoke
- [x] requests (empty), activity, summary, tabs' counts
