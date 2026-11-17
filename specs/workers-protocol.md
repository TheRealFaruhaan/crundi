# Workers: the link protocol (phase 1)

The contract between the Crundi server (`src/workers*.js`) and the worker
program (`worker/`, Rust, crate `crundi-worker`). Plan: `specs/cluster-plan.md`.
Phase 1: join, the link, info and labels, pause, the PC terminal. No jobs yet.

Everything here is versioned by `proto` (now 1) and by capability lists, so
either side can be newer (the backwards-compatibility rule): unknown message
types are ignored, unknown fields are ignored, and a feature is used only
when the other side lists it.

## Joining

1. The owner makes a join code on the server (Workers tab, Add a PC):
   `POST /api/workers {action:"code"}` -> `{ code, expiresAt }`. 8 characters
   from `ABCDEFGHJKMNPQRSTUVWXYZ23456789`, single use, 10 minutes, memory only.
2. The worker trades it, unauthenticated except for the code:
   `POST /api/workers/join` with
   `{ code, name, deviceId?, kind: "tray"|"headless"|"electron"|"native", version, os }`
   -> `200 { ok:true, id, key, name }` or `4xx { ok:false, error }`.
   `key` is `cw_<id>.<secret>` (id `w` + 10 hex; secret 32 bytes base64url),
   shown once; the server keeps only its SHA-256.
3. The PC client joins without a code: signed in as the owner it calls
   `POST /api/workers {action:"join-self", name, deviceId, kind:"electron", version, os}`
   -> the same `{ id, key, name }`. One worker per deviceId: the same device
   joining again gets a new key for the same worker id.
4. Rate limit on /api/workers/join: 10 failed codes per IP per 10 minutes.

The worker stores `{ server, id, key, name, workDir, allowChats, allowCommands }`
in its config file (see "The worker program").

## The link

`GET /api/workers/link` upgraded to a websocket, with
`Authorization: Bearer cw_<id>.<secret>` (or `?key=` for clients that cannot
set headers). 401 for a bad, revoked or removed key. A second link for the
same worker replaces the first (the old one gets `{type:"bye",reason:"replaced"}`
and is closed).

Text frames are JSON objects with a `type`. Binary frames are reserved for
phase 2 streams (net-tunnel framing); phase 1 ignores them.

Keepalive: the server pings every 25 s (websocket ping); a link with no pong
for 60 s is closed. The worker reconnects with backoff 1 s doubling to 60 s,
with jitter; after a 401 it stops and shows "this PC was removed" (tray) or
exits 3 (headless).

### Worker -> server

- `hello` (first frame, required):
  `{ type:"hello", proto:1, version, kind, name, deviceId?, caps:[...], info }`
  caps in phase 1: `["info","terminal","pause"]`.
- `info` every 30 s and on change: `{ type:"info", info }` where `info` is
  ```
  { os: "windows"|"macos"|"linux", osVersion, arch, hostname,
    cpu: { model, cores, threads }, ramMb, ramFreeMb, load: [1m,5m,15m]|null,
    gpus: [{ vendor, name, vramMb? }],
    disk: { path, freeMb, totalMb },            // of the work folder
    workDir, onBattery: bool|null, idleSec: int|null,
    tools: { git:"2.43.0", node:"20.11.1", cargo:"1.83.0", ... },  // found ones only
    allow: { chats: bool, commands: bool },
    paused: bool }
  ```
  Tools probed (version string or absent): git, gh, node, npm, python
  (python3/python), cargo, rustc, go, java, ffmpeg, docker, claude
  (and `claudeLoggedIn: bool|null` beside it, phase 2).
- `paused`: `{ type:"paused", paused:bool, by:"pc" }` when the person pauses
  from the tray (also reflected in the next info).
- `term.data`: `{ type:"term.data", ref, b64 }` output of a terminal.
- `term.exit`: `{ type:"term.exit", ref, code }`.
- `term.error`: `{ type:"term.error", ref, error }`.
- `log`: `{ type:"log", level:"info"|"warn"|"error", text }` (shown on the
  worker's page, last 200 kept).

### Server -> worker

- `welcome` (answer to hello): `{ type:"welcome", proto:1, id, name, server:{version}, caps:[...], settings:{ allowChats, allowCommands, paused } }`
  caps in phase 1: `["terminal","pause","rename"]`.
- `settings`: `{ type:"settings", name?, allowChats?, allowCommands?, paused? }`
  when the owner changes them. The worker applies and persists them and
  sends a fresh `info`.
- `term.open`: `{ type:"term.open", ref, cols, rows, cwd? }` open a shell
  (Windows: PowerShell if present, else cmd; macOS/Linux: the user's $SHELL
  as a login shell, else /bin/sh). cwd default: the work folder.
- `term.input`: `{ type:"term.input", ref, b64 }`.
- `term.resize`: `{ type:"term.resize", ref, cols, rows }`.
- `term.close`: `{ type:"term.close", ref }`.
- `bye`: `{ type:"bye", reason:"revoked"|"removed"|"replaced" }` then close.
  On revoked/removed the worker forgets its key.

`ref` is a server-chosen string (<= 40 chars). At most 8 terminals per
worker. Terminals are not refused while paused (pause holds jobs, not the
owner's own shell).

## Labels

The server derives labels from info, and the owner may add his own (free
text, `[a-z0-9:._-]`, up to 20):
`os:<os>`, `arch:<arch>`, `ram:<GB rounded down>g`, `cores:<threads>`,
`gpu:<vendor>` per GPU, `tool:<name>` per tool found.
Jobs (phase 2) ask for labels; a PC name is an override.

## Server HTTP API (owner only, never collaborators)

- `GET /api/workers` -> `{ workers:[...], codes:[{code,expiresAt}] }` where a worker is
  `{ id, name, kind, version, online, connectedSince, lastSeenAt, info, labels, customLabels, allowChats, allowCommands, paused, pausedBy, createdAt, deviceId, logs:[...] }`
- `POST /api/workers` `{ action, ... }`:
  `code` | `join-self` | `rename {id,name}` | `set {id, allowChats?, allowCommands?, paused?, labels?}` |
  `revoke {id}` (key invalid, worker kept, shown as revoked; `reissue` makes a new join code for it) |
  `remove {id}` | `history {id}` (the event log: joined, connected, disconnected, renamed, paused, terminal opened/closed, revoked).
- UI updates: the server pushes `{ type:"workers" }` on the owner's UI
  websockets (and an SSE event `workers` on /api/events) whenever anything
  changes, batched over 50 ms; the page refetches.
- UI terminal over the UI websocket (owner only), as the Docker shell:
  `worker-term-open {ref, worker, cols, rows}`, `worker-term-input {ref, data}`,
  `worker-term-resize {ref, cols, rows}`, `worker-term-close {ref}`;
  back: `worker-term-data {ref, data}`, `worker-term-exit {ref, code}`,
  `worker-term-error {ref, error}`. Closing the page closes its terminals.

## MCP

`worker_list` (owner chats only): the workers with online, labels, info
summary and allow switches. Phase 2 adds `worker_run` / `worker_job`.

## The worker program (`worker/`)

- Rust crate `crundi-worker`, lib + bin, edition 2021.
- Feature `tray` (default): tray icon and menu (status line; Pause/Resume;
  Open work folder; Join a server... when not joined; Start at login
  on/off; Quit). Without it (`--no-default-features`), headless only.
- CLI:
  - `crundi-worker` : tray if built with it and a desktop is there, else headless.
  - `crundi-worker --headless` : no tray, logs to stderr.
  - `crundi-worker join <server-url> <code> [--name N] [--work-dir D]` : join and save.
  - `crundi-worker --key <cw_...> --server <url> [--work-dir D] --embedded` :
    run with a key given by the PC client (not saved; the client owns it).
  - `crundi-worker status` : print config and whether joined.
  - `crundi-worker install-service` (Linux): write and enable a systemd user
    unit; `uninstall-service`.
  - `crundi-worker autostart on|off` (desktop: Windows Run key, macOS
    LaunchAgent, Linux XDG autostart).
  - `--config <path>` overrides the config file.
- Config: `<config dir>/crundi-worker/config.json` (dirs::config_dir).
  Work folder default `~/crundi-work`.
- Tray first run (not joined): "Join a server..." opens a one-page setup
  served on 127.0.0.1 (random port, one-time token in the URL) in the
  default browser: server address, join code, name, work folder.
- Embedded mode (Electron client): stdout lines are JSON status events for
  the client (`{"event":"status","online":true,...}`), stdin closing means
  exit.
