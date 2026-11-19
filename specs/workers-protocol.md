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

# Phase 2: commands on a PC

A chat (the owner's, never a collaborator's) runs a command on a worker and
gets its output and files back. Caps: the worker adds `"jobs"` to hello caps;
the server adds `"jobs"` to welcome caps. Neither side sends job messages to
a peer that does not list it.

## Where the code comes from (`source`)

- `git` (default when the project has a GitHub/https remote): the PC clones
  `git.url` into `<workDir>/<project>` the first time (its own git and
  credentials; GitHub SSH URLs are sent as https), then for each job:
  `git fetch origin <branch>`; if `git.commit` is still not there it asks the
  server for a bundle (`job.need`), fetches the bundle, then
  `git checkout --force -B crundi/job <commit>` and `git clean -fdq -e <cache dirs>`
  is NOT run (build caches stay: target/, node_modules/, dist/ and the like
  persist between jobs, which is the point). Nothing is pushed or created on
  GitHub. Uncommitted server changes are not sent (the result says so).
- `copy`: a bare copy over the link, no git needed on the PC, into
  `<workDir>/<project>@copy`. The PC sends a manifest of what it has
  (`job.manifest`: path -> [size, sha256]); the server sends only new and
  changed files as a tar stream plus a list of removed paths (`job.files`).
  Excluded: `.git`, `node_modules`, `target`, `dist`, `build`, `.next`,
  `__pycache__`, plus `.crundiignore` lines; with git on the server, only
  tracked + untracked-not-ignored files (`git ls-files -co --exclude-standard`).
  The kept copy is the cache: unchanged files are not sent again, and build
  folders in it persist.
- `none`: run in `<workDir>/<project>` (or the work folder) as it is.

## Messages (server -> worker)

- `job.start`:
  ```
  { type:"job.start", job:"j<hex>", project:"<alias>", command:"...",
    source:"git"|"copy"|"none",
    git?: { url, branch, commit },
    cwd?: "relative/sub/dir",            // inside the project folder
    env?: { NAME: "value" },
    timeoutSec: 7200, artifacts: ["glob", ...], lowPriority: true }
  ```
  The worker answers `job.accepted` or `job.refused {job, error}` (paused,
  commands not allowed, already running one: one job at a time in phase 2).
- `job.bundle`: `{ type:"job.bundle", job, seq, b64, last:bool }` chunks of a
  git bundle (answer to `job.need`).
- `job.files`: `{ type:"job.files", job, seq, b64, last:bool, removed?:[paths] }`
  chunks of a tar (uncompressed) of new/changed files; `removed` on the last.
- `job.cancel`: `{ type:"job.cancel", job }` -> kill the process tree.
- `job.attach`: `{ type:"job.attach", job, from }` after a reconnect: resend
  output from byte offset `from`, then carry on live.

## Messages (worker -> server)

- `job.accepted` / `job.refused`.
- `job.need`: `{ type:"job.need", job, kind:"bundle", have:"<sha or ''>" }` or
  `{ type:"job.manifest", job, files:{ "path": [size, "sha256hex"] } }` (copy).
- `job.status`: `{ type:"job.status", job, phase:"preparing"|"running"|"collecting", detail }`.
- `job.out`: `{ type:"job.out", job, off, b64 }` output (stdout and stderr
  together, in order) from byte offset `off` of the job's log. The worker
  spools the whole log to `<workDir>/.crundi-jobs/<job>.log` so it can be
  resent.
- `job.exit`: `{ type:"job.exit", job, code, signal?, durationMs, timedOut?:bool, cancelled?:bool }`.
- `job.artifact`: `{ type:"job.artifact", job, seq, b64, last:bool, count, bytes }`
  chunks of a .tar.gz of the files matching `artifacts` (relative to the
  project folder; at most 2 GB; `count` 0 when nothing matched), sent after
  job.exit and before `job.done`.
- `job.done`: `{ type:"job.done", job }` nothing more is coming.
- In `hello`, `jobs: [{ job, state:"running"|"finished", logBytes }]` for jobs
  it still knows (running, or finished within the last hour and not yet
  done-acknowledged), so the server can `job.attach`.

Chunks are at most 512 KB of data before base64. The process runs in the
project folder (or `cwd` under it) with the user's login environment plus
`env`, through the platform shell (`sh -lc` / `cmd /c` or PowerShell), at low
priority when `lowPriority` (unix: nice 10; Windows:
BELOW_NORMAL_PRIORITY_CLASS). Timeout kills the process tree.

## The link dropping

A job keeps running on the PC when the link drops. On reconnect the worker
lists it in hello; the server sends `job.attach {from: bytes it has}`. A job
whose PC stays away longer than 30 minutes is marked lost on the server
(the PC still finishes it; its result is dropped).

## MCP (owner chats only)

- `worker_run { command, worker?, labels?, source?, branch?, cwd?, artifacts?, env?, timeoutMinutes?, wait? }`
  picks the worker (by id/name, else the first online, unpaused, commands
  allowed, idle one matching every label), starts the job in this chat's
  project and, with `wait` (default true), waits up to 10 minutes for it to
  end. Returns `{ ok, job, worker, exitCode, durationMs, tail (last 4 KB),
  log (server path), artifacts: { path, bytes, count } | null, notes:[...] }`
  or, still running, `{ ok, job, running:true, ... }`.
- `worker_job { job, action: "status"|"log"|"cancel"|"wait", tailBytes? }`.

The server keeps jobs in `<data>/workers/jobs.json` (last 200) with logs in
`<data>/workers/jobs/<job>.log` and artifacts in
`<data>/workers/artifacts/<job>.tar.gz` (kept 7 days). The Workers tab shows
a PC's jobs with their live log.

## Settled while building phase 2

- Copy removals: the server lists in `removed` only paths an earlier copy to
  that PC and project had (kept in `<data>/workers/copies.json`) and the
  server no longer has. Anything else in the PC's folder (what a build made)
  is the PC's and is never removed. Unknown history: nothing is removed.
- A job that fails before its command runs (clone, copy, bad cwd, cancelled
  while preparing): `job.exit` has `code: -1`, `durationMs: 0` and `error`.
- Killed by a signal (unix): `code` 128 + signal, `signal` set.
- `job.attach` for a job the PC no longer has: `job.done {unknown:true}`; the
  server marks it lost.
- Commands get `CRUNDI_JOB=<job>` in their environment. `durationMs` is the
  command's alone. Windows runs `cmd /d /s /c`, at BELOW_NORMAL priority; the
  whole process tree is killed on cancel or timeout (taskkill /T /F).
- A job waits at most 10 minutes for a bundle or the files; then it fails.

# Phase 4: whole chats on a PC (and phase 5: factory steps on a PC)

Caps: worker adds `"chats"` to hello caps; server adds `"chats"` to welcome.

## How a chat runs on a PC

The server's chat machinery (src/claude-ui.js) is unchanged above the
process: it spawns through a launcher, `launch({ bin, args, cwd, env }) ->
{ stdin, stdout, stderr, kill(), on('exit', code) }`. The local launcher is
today's child_process.spawn. The worker launcher (src/workers-chats.js) sends
`chat.spawn` and maps the three pipes onto link messages. stream-json parsing,
cards, permissions and the factory adapter stay as they are.

- The PC runs ITS OWN `claude` (installed and signed in there: the owner's
  own subscription; no Agent SDK). Missing: the chat is refused with
  "Install Claude Code on <PC>: curl -fsSL https://claude.ai/install.sh | bash
  (Windows: irm https://claude.ai/install.ps1 | iex), then run claude once to
  sign in" and the worker reports `tools.claude`.
- MCP: the PC has no Node (often). The worker binary is the chat's MCP
  server: claude is given `--mcp-config <file>` naming
  `{"mcpServers":{"crundi":{"command":"<worker exe>","args":["mcp","--chat","<chat>","--port","<p>","--token","<t>"]}}}`.
  `crundi-worker mcp` speaks MCP (JSON-RPC 2.0 over stdio, protocol
  2024-11-05: initialize, tools/list, tools/call, ping, notifications) and
  relays tools/list and tools/call over loopback TCP (127.0.0.1:<p>, one-time
  token <t>) to the running worker, which sends them over the link:
  `mcp.list {chat, req}` -> `mcp.tools {req, tools}` and
  `mcp.call {chat, req, tool, args}` -> `mcp.result {req, result}` (result is
  the server's JSON answer; the stdio side wraps it as MCP content, images
  as image content like mcp-stdio.js does). The server runs every tool as it
  does today, as that chat (its session id, project, chat token; never the
  internal API key on the PC).
- Permission prompts: as today, through stream-json (`--permission-prompt-tool stdio`).

## Messages

Server -> worker:
- `chat.spawn { chat, project, args:[...claude args...], source, git?, cwd?, env, resumeSessionId? }`
  The worker prepares the folder like a job (git: local branch `pc/<chat>`
  from git.commit, created once and kept for the chat's life; copy:
  `<project>@copy`), writes the MCP config, and starts `claude <args>` in it
  with stdin/stdout/stderr piped, NOT at low priority. Answers
  `chat.started { chat, pid }` or `chat.refused { chat, error }`.
- `chat.in { chat, b64 }` bytes for claude's stdin.
- `chat.kill { chat, signal? }` (interrupt = SIGINT; close = SIGTERM then kill).
- `chat.attach { chat, from }` after a reconnect: resend stdout from byte `from`.
- `chat.pull { chat, req, base }` -> the PC answers with the work since
  `base` (the commit the chat started from): `chat.bundle { chat, req, seq, b64, last, head }`
  (a git bundle of `pc/<chat>` with `^base`, empty when head == base) then
  `chat.patch { chat, req, b64 }` (`git diff --binary HEAD`, uncommitted
  work, may be empty). Copy source: `chat.files { chat, req, seq, b64, last, removed:[] }`
  (a tar of files changed since the copy, by the copy manifest) instead.
- `mcp.tools { req, tools }`, `mcp.result { req, result }`.

Worker -> server:
- `chat.started`, `chat.refused`.
- `chat.out { chat, stream:"stdout"|"stderr", off, b64 }` (stdout offsets
  count stdout bytes only; spooled to `<workDir>/.crundi-chats/<chat>.out`).
- `chat.exit { chat, code, signal? }`.
- `mcp.list`, `mcp.call` (above).
- `chat.bundle`, `chat.patch`, `chat.files` (answers to chat.pull).
- In hello: `chats: [{ chat, running:bool, outBytes }]`.

A chat keeps running on the PC when the link drops; stdin sent meanwhile is
queued on the server and delivered on reconnect; the server shows the chat
as "PC offline". A chat whose PC stays away 30 minutes is shown as ended.

## Changes coming back

At each turn end (and on "Bring changes back"), the server sends `chat.pull`
and applies the answer in a worktree of the server's project:
`<data>/workers/worktrees/<project>/<chat>` on branch `pc/<worker>/<chat>`,
created from `base`: git bundle fetched and the branch reset to its head,
then the patch applied (uncommitted work stays uncommitted there). Copy
source: the files written into that worktree (or, project without git,
`<project>/.crundi/pc-changes/<chat>/` plus `removed.txt`). The owner merges
as with factory runs. Nothing writes into the server's main checkout.

## Settled while building phase 4 (server side)

- Preparing a chat's folder: `chat.need {chat, kind:"bundle", have}` is
  answered with `chat.bundle {chat, seq, b64, last}` and `chat.manifest
  {chat, files}` with `chat.files {chat, seq, b64, last, removed}`, both
  WITHOUT `req` (pull answers carry `req`; direction tells them apart), served
  exactly as for a job (bundle exclusions, the copies.json rule). Asked in job
  words (`job.need` / `job.manifest`, job = the chat id), answered in job words.
- `chat.in` is held until `chat.started` (and while the PC is away), then
  sent in order. `chat.attach` is sent for every chat listed in hello that is
  still open here; a listed chat this server is not running gets
  `chat.kill {signal:"SIGTERM"}`; an open chat missing from hello has ended.
- `chat.refused` and `chat.exit {error}` show their text in the chat.
- `mcp.list` / `mcp.call` are answered only for a chat open here on that
  same PC. The list is an owner chat's (a factory run's chat: the run tools)
  without `syntax_check` (the files are on the PC); a call runs as
  /api/mcp/call with the chat's own project, session id and chat token.
- A pull that fails on the PC says so with `error` on `chat.patch` (git) or
  the last `chat.files` (copy); no answer: it times out after 10 minutes.
- `mcp.list` / `mcp.call` resent with the same `req` (after a reconnect) are
  answered again from the first answer; a tool never runs twice.
- `chat.exit {unknown:true}` (the PC did not know the chat at `chat.attach`)
  ends it here; closing claude's stdin is `chat.in {chat, eof:true}`. Branch: `pc/<worker-name-slug>/<first 8 of chat>`.
- Owner HTTP: `POST /api/workers` `chat-places` (PCs a new chat may run on:
  online, chats allowed, claude found), `chat-pull {chat}` (Bring changes
  back: `{ok, head, branch, worktree}`), `chat-info {chat}`. Chat creation
  (`/api/ui-sessions/create`) takes `where: "worker:<id>"`; a session lists
  `where` and `pc: {name, online, branch, worktree}`.

## Phase 5: factory steps on a PC

- factory-model: `runsOn` ('' = server, or a worker id or name) on a roster
  member, and on a step node as an override; validation warns (not fails) on
  an unknown worker.
- The runner spawns that person's chat with `where: 'worker:<id>'` in a PC
  copy of the run's worktree (git: the run's branch, commits from the server's
  worktree via the bundle path; base = the run's head at step start).
- PC goes offline: event `worker-offline {worker, employees}`; those steps
  are held (like pauseHeld), their seats released; the Manager on that PC
  pauses the whole run (`by:'worker'`). A chat that ended because its PC went
  away is held, not escalated. `worker-online` resumes them (recovery prompt,
  `--resume` on the PC).
- At handoff the runner pulls the PC's work into the run's worktree BEFORE
  accepting the handoff (chat.pull; git: fetch the bundle onto the run
  branch, apply the patch, commit it as the step's work).
- factory_status shows where each person runs and which PCs are offline.

# Terminal mode on a PC

Claude in Terminal mode (a real terminal, claude-terminals.js) can run on a
PC too, through the chat messages:
- `chat.spawn { ..., pty: { cols, rows } }`: start `claude <args>` in a
  pseudo-terminal (portable-pty) instead of pipes, same folder preparation,
  same MCP config (the worker is the MCP server), same env. Its output is
  `chat.out { stream:"stdout", off, b64 }` (the terminal's bytes), spooled
  and resumable with `chat.attach` like a chat; `chat.in` is keystrokes.
- `chat.resize { chat, cols, rows }` resizes it.
- `chat.kill` ends it; `chat.exit` when claude exits.
- Pull-back of its changes as for chats (chat.pull).
The server's lifecycle hooks (CRUNDI_TERMINAL_ID posting to the loopback
API) do not reach from a PC; a PC terminal's status comes from its output
and exit only.

## Settled while building terminal mode (server side)

- `chat.spawn { ..., pty, command: "shell" }` (optional, only with `pty`):
  the user's login shell instead of claude (Windows: pwsh, else powershell,
  else cmd; macOS/Linux: $SHELL as a login shell, else /bin/sh), in the same
  prepared folder, with the same env; `args` is empty. It is an Empty Shell
  pane on that PC: resumable with `chat.attach`, pulled back with `chat.pull`.
  A worker that does not know `command` must refuse it (`chat.refused`),
  never start claude instead.
- As built on the worker (worker/src/chats.rs, update.rs, lock.rs):
  `chat.spawn` with `pty` -> `chat.started` carries `terminal:true`;
  `command:"shell"` runs the login shell (no claude, no MCP config, no resume
  handling) and is a pty even without `pty` (80x24); its `chat.started`
  carries `shell:true, terminal:true`. `hello.chats` entries for running
  terminals carry `terminal:true`. In terminal mode `chat.in` bytes are
  keystrokes, `chat.in {eof:true}` is ignored, `chat.kill {signal:"SIGINT"}`
  sends Ctrl-C; `chat.exit` after a signal is code 128+n with `signal`.
- Copy-source chats run in `<project>@copy-<chat>`, seeded from
  `<project>@copy`; the first `chat.manifest` already lists the seeded files,
  and the server serves the copy diff against it as usual.
- `chat.notice` comes BEFORE `chat.started` when `resumeSessionId` has no
  transcript on the PC (the worker then dropped `--resume/-r <id>`,
  `--continue/-c` and `--fork-session`).
- Embedded status may be `{"event":"status","state":"conflict","detail":"..."}`
  (another worker on this PC holds the lock); the PC client's Settings says
  "Another worker on this PC is connected".
- `chat.notice { chat, text }` (worker -> server): something about the chat
  that is not an error, e.g. "This PC did not have that conversation;
  starting a new one" when `--resume` names a session the PC lacks (the
  worker drops `--resume <id>` and starts fresh). Shown as a notice line in a
  chat and in a PC terminal's bar (and as a dim line in its screen); nothing
  else changes.
- Server side, a terminal on a PC is created with
  `POST /api/terminals/create { project, where:"worker:<id>", shell?, ... }`
  (owner only). Its list entry has `where`, `pc: {where, name, online, branch,
  worktree}` and `notice`. No .mcp.json or hooks are written for it; its
  working/idle state comes from its output (quiet 4 s = idle) and its exit.
  Bring changes back: `POST /api/workers {action:"chat-pull", chat:<terminal id>}`.

## Worker updates

- `welcome` may carry `update: { version, url, sigUrl }` when the worker's
  `hello.version` differs from the server's version and its kind is `tray`
  or `headless` (`electron` / `native` update with their app). `url` is
  `https://github.com/TheRealFaruhaan/crundi/releases/download/v<V>/crundi-worker-<V>-<label>[.exe]`
  with label `windows-x64` (.exe) | `linux-x64` | `linux-x64-headless` (kind
  headless on linux) | `macos-arm64`, from `info.os` / `info.arch`;
  `sigUrl` = url + `.sig`. Sent only for a published release version
  (`X.Y.Z` or `X.Y.Z-dev.N`) and not when the server has
  `CRUNDI_NO_WORKER_UPDATE` set. The worker applies it when the version
  differs from its own and the PC is idle (checked every 5 s): https only, at
  most 100 MB, the signature raw 64 bytes or base64. Failure: `log
  {level:"error", text:"update: ..."}`, not retried that session; success
  logs "updated to X; restarting" and it reconnects as the new version.
  Embedded workers ignore it.
- The Workers list gives each worker `updates: "self" | "app"`.

## Factory steps: a PC away too long

- Factory setting `workerOfflineMinutes` (default 30, 0 = wait forever).
  When a PC holding a run's steps has been offline that long, the owner is
  notified once, and the run lists it in `pcsAway: [{ workerId, worker,
  since, minutes, overdue, employees }]`. `POST /api/factory-runs
  {action:"moveToServer", id, worker?}` (or factory_control action
  `move-to-server` from the Manager on the person's word) re-runs those steps
  on the server from the run's worktree with a recovery prompt; the people
  moved stay on the server for the rest of the run.
- Before each Manager step on a PC the runner pulls its work into the run's
  worktree; if the run's branch moved on since the Manager's PC copy was
  made, its PC chat is restarted from the run's head (`--resume` keeps the
  conversation), so later pulls fast-forward.

# Keeping a live chat's folder up to date (chat.sync)

A chat on a PC keeps its folder (`<project>@<chat>`, branch `pc/<chat>`) and
its build caches for its whole life. A factory step that reuses a live chat
(the Manager always; anyone a run hands steps to on one token) must first
see what others committed on the server since. Before each such step the
runner brings the person's work back (pullChat, committed into the run),
and if the run's head moved past their copy, the server sends:

- `chat.sync { chat, req, git: { url, branch, commit } }` (caps: worker
  `chat-sync`). The PC gets `commit` (origin, else `chat.need` -> `chat.bundle`
  without req, as at spawn), then: HEAD == commit: nothing; clean and HEAD
  is an ancestor: `git merge --ff-only`; otherwise `git stash push -u`
  (a backup; the work is already on the server) and `git reset --hard`.
  Never `git clean` (build caches stay). claude keeps running.
- `chat.synced { chat, req, ok, head, ffwd, stashed, error? }`.

A worker without `chat-sync`, or a failed sync: the runner starts a fresh
chat from the run's head (the conversation carries on with --resume), so a
step never runs on a stale folder. Journal: `worker-resync {head, inPlace}`.
