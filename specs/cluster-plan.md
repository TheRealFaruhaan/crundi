# Crundi cluster: chats and commands on connected PCs (design plan)

Card 6ab8d6b963784ecf. **A plan for the owner to approve; nothing here is
built yet.** It is grounded in the code as it is on dev (1.21.8-dev.3); file
references are where each part would hook in.

## Goal

The Crundi server stays the one master. PCs (Windows, Linux, macOS) join it as
**workers** so heavy work (cargo and app builds, renders, ffmpeg) leaves the
11 GB server. Two modes, picked per job:

- **Commands on a PC.** The chat stays on the server; a command (a build, a
  render) runs on a named PC, which streams the output back and returns the
  artifacts.
- **Whole chats on a PC.** Claude Code runs on the PC and streams back to the
  server, which shows the chat exactly as a local one.

The server stays the default everywhere. Nothing changes for anyone who never
adds a PC.

One thing to keep in mind: Claude usage is account-wide. A chat on a PC
saves the server's CPU and RAM, not usage.

## What exists today (and what does not)

- Chats are local child processes. `claude-ui.js` `create()` spawns `claude`
  with stream-json pipes. Its cwd and transcripts (`~/.claude/projects`) are
  on the server, and its MCP server `mcp-stdio.js` calls
  `http://localhost:8888/api/mcp/call` with the internal API key. None of
  that can leave the machine as it is.
- The factory runner takes its `agents` adapter as a parameter
  (`claudeUiAgents()`, factory-runner.js). That is the seam where a remote
  chat plugs in. Nothing says where a step runs (no field on roster or node).
- The desktop app's native-host socket carries browser control (JSON) and
  net-tunnel streams (binary, opened from the app side only). There is no
  RPC, no process channel, and no server-opened streams.
- `homepool-node` is the closest precedent for a headless program that joins
  with a key. It uses an `hpn_<id>.<secret>` key whose hash is stored on the
  server, a websocket, `info` reports and revocation. It lends a connection,
  not compute, and has no service installer.
- Commands run only on the server: services, the factory setup and wait
  commands, `request_owner_command` and `secret_run`.

## Design

### 1. The worker: a headless program that joins with a token

- **What it is:** `crundi-worker`, one Node program, `app/cluster-worker.js`,
  in the style of `app/homepool-node.js`.
  - The server serves it (`/api/cluster/worker.mjs`), so a PC always runs
    the server's version.
  - The desktop app can run it inside itself, the way it runs its homepool
    node (`ELECTRON_RUN_AS_NODE`, a Settings switch). A PC with the app
    needs nothing else.
- **Joining:**
  1. Settings, Cluster, Add a PC makes a one-time join code (5 minutes,
     like pairing).
  2. On the PC, the worker trades the code for a long-lived worker key,
     `cw_<id>.<secret>`. The server stores only its hash, like a homepool
     node key.
  3. The worker connects to `wss://<server>/api/cluster/worker` with that
     key.
  4. The owner can rename a worker, pause it, or revoke its key. Revoking
     closes the socket at once.
- **Installing as a service** (from a one-liner the Add a PC dialog shows):
  - Linux: a systemd user unit, `loginctl enable-linger` for boot.
  - macOS: a launchd agent.
  - Windows: a Scheduled Task at logon, running as the user. A real Windows
    service runs as SYSTEM, has no access to the user's `claude` login, and
    so is wrong for chats.
  - The script needs Node 20+. If it is missing, the installer fetches a
    portable Node into the worker's own folder and touches nothing
    system-wide.
- **What it reports** in `info` every 30 s and on change:
  - OS, architecture, CPU model and cores, RAM total and free, GPU (from
    nvidia-smi or the OS);
  - free disk in the work folder, load;
  - tools found with their versions: git, node, npm, cargo/rustup, python,
    ffmpeg, docker, claude (and whether it is logged in);
  - its work folder;
  - what it allows: `commands`, `chats`, `installs`.
- **Protocol:** one websocket.
  - JSON control messages: `hello`, `info`, `job.*`, `chat.*`, `fs.*`,
    `setup.*`.
  - Binary streams in the net-tunnel framing (`[type][stream id][payload]`),
    extended so either side can open a stream. Stream ids are split by
    range, as `net-tunnel-client.js` already does. Streams carry stdout and
    stderr, stdin, tar archives and MCP calls.
  - Backpressure uses the existing pause threshold.
- **Safety on the PC:**
  - Every cwd must be under the worker's work folder.
  - Each capability is the PC owner's switch: run commands, run chats,
    install tools. Anything off is refused on the PC, not just hidden on the
    server.
  - Only the owner can target a worker. Collaborators' chats never can,
    because the tools refuse a collaborator key.

### 2. Commands only on a PC

- **New MCP tools**, owner chats only:
  - `worker_list`: the workers with their info and status.
  - `worker_run { worker, command, cwd?, sync?, artifacts?, timeout?, background? }`
    runs `command` in the project's copy on that PC and streams output into
    the chat as a card, live, like a long tool result. It returns the exit
    code, the tail of the output and where the artifacts landed.
  - `worker_job` (status, logs, cancel) for `background: true` jobs, which
    are kept server-side like services' logs.
- **`sync`**, so the PC builds exactly what the server has:
  - **`git` (the default):** the PC's copy is a clone. The server sends the
    commit, and the working tree's uncommitted diff with binary files. The
    PC checks out that commit and applies the diff in a clean tree.
    - The clone comes from the server: a read-only git smart-HTTP endpoint
      for the project, reached over the worker socket, so the PC needs no
      GitHub credentials.
  - **`copy`:** a tar of `git ls-files` plus untracked files that are not
    ignored, for projects without git.
  - **`none`:** run in whatever is there.
- **`artifacts`:** globs whose matches come back as a tar stream into
  `<project>/.crundi/artifacts/<job>/` (or a path given). The chat can then
  `show_file` or `show_video` them.
- **Limits:**
  - one job per worker by default, settable per worker;
  - a timeout, default 2 h;
  - jobs are killed and reported if the PC disconnects. A rerun is the
    chat's call; jobs are not resumed blindly.

### 3. Whole chats on a PC

- **Where it runs:** chat create takes `where: 'server' | 'worker:<id>'`.
  - UI: a "Runs on" picker in the new-chat menu, shown only when a worker
    allows chats.
  - The chat header shows the PC's name and whether it is connected.
- **Process seam:** `claude-ui.js` spawns through a small launcher
  interface, `launch({ bin, args, cwd, env }) -> { stdin, stdout, stderr, kill, on('exit') }`.
  - The local launcher is today's `spawn`.
  - The worker launcher sends `chat.spawn` and maps the three pipes onto
    binary streams.
  - Everything above the pipes stays as it is: stream-json parsing, cards,
    permissions, the factory adapter.
- **The PC's `claude`:** the PC's own Claude Code install and login, the
  person's subscription. This keeps the No Agent SDK rule (spawn the user's
  own `claude`). Setup (section 5) installs it and asks for the login when it
  is missing.
- **MCP from the PC:**
  - `mcp-stdio.js` runs on the PC (the worker ships it).
  - Its `CRUNDI_API_URL` is a loopback port the worker opens on the PC. The
    worker forwards each call over the socket to the server's MCP dispatcher.
  - The credential is a **per-chat key** (the HMAC chat token, already in
    `chat-token.js`). The internal API key never leaves the server.
  - The server runs every tool as it does today, on the server. Services,
    the browser drawer, kanban and secrets behave the same for a chat on a
    PC.
  - Tools that act on "this machine" say which machine they mean. For
    example `register_service` stays server-side; a dev server on the PC is
    reachable through a net-tunnel forward, which comes in a later phase.
- **Where the code lives:** a chat on a PC works in the PC's copy, on a
  branch `pc/<worker>/<chat>` checked out in a worktree there.
  - At each turn end, the worker sends a `git bundle` of new commits plus a
    diff of uncommitted work.
  - The server applies it to a matching worktree on the server, so the
    owner sees the changes, diffs and git history as usual.
  - Merging into dev stays the owner's step, as with factory runs. Nothing
    writes into the server's main checkout behind his back.
- **Transcripts and resume:**
  - The JSONL lives on the PC. Resume is `--resume` on the same PC.
  - Server features that read `~/.claude/projects` (history after a restart,
    title, summaries, recall) get it through `fs.get` from the worker. They
    fall back to the chat's in-memory messages while the PC is away.
- **When the PC disconnects:**
  - The chat shows "PC offline" and its composer is disabled.
  - Messages typed meanwhile are kept and sent when it is back.
  - A turn that was running is reported as interrupted. On reconnect the
    worker says which of its chats are still alive, and they reattach; a dead
    one resumes with `--resume`.

### 4. Factories: steps that name a PC

- **Model** (`factory-model.js`): `runsOn` (`''` = server, or a worker id)
  on a roster member, and on a step node as an override. "Area" maps to a
  member, since each member is an area of the work.
  - Validation warns about an unknown worker. It does not fail, because a PC
    may be offline while the factory is being edited.
- **Runner:** `ensureSession` passes `runsOn` to `agents.spawn`. The
  `claudeUiAgents` adapter creates the chat with `where`, so the step lives
  on that PC.
  - The step's work happens in the PC's copy of the run's worktree.
  - At the step's start the PC syncs to the run branch's head.
  - At handoff the runner applies the PC's bundle to the server worktree
    **before** it accepts the handoff. A join therefore never passes
    part-built work.
- **The PC disconnects:**
  - New event `worker-offline { worker, steps }`. Steps on that PC are held
    (`emp.workerHeld`, the same mechanism as `pauseHeld`). Their seats are
    released, and the rest of the run carries on.
  - If the Manager runs on that PC, the whole run pauses with
    `by: 'worker'`.
  - `onExit` treats a chat that ended because its PC went away as held, not
    as a crash, so there is no escalation and no auto-recover.
  - On `worker-online`, held steps restart with the recovery prompt (as
    `resume()` does), resuming their sessions on the PC.
  - Optional per-factory setting `workerOfflineMinutes`: after that long
    offline, the owner is notified and can move those steps to the server.
    Default: wait, and notify once.
- The Manager's `factory_status` shows where each person runs and which PCs
  are offline.

### 5. PC setup done by Crundi

- **Work folder:** chosen when the worker is installed; the default is
  `~/crundi-work`. Every project is at `<work>/<project>`, and every chat or
  run worktree is under it.
- **Prepare a project:** done on the first job, or with "Prepare on <PC>"
  in the project menu. The worker clones from the server's git endpoint
  (section 2) and runs the project's setup command if it has one (the same
  one factory runs use).
- **Tools:** a small recipe table per tool and OS:
  - Windows: winget;
  - macOS: Homebrew;
  - Linux: apt or dnf;
  - each tool's own installer where that is the norm: rustup, nvm, the
    Claude Code installer.
- **The flow:**
  1. A job or chat that needs a tool the PC lacks gets a setup plan: "install
     rustup and the stable toolchain".
  2. The owner approves it on that PC. With the desktop app, it shows a
     dialog. With a headless worker, it waits for approval in the server's
     Cluster settings, and only if the worker allows installs. A step that
     needs admin rights (apt, a system-wide winget) says so, and is never run
     silently.
  3. Each tool is checked by running it after install, and the result is
     reported in `info`.
- **Memory rule:** the `-j2` / one-build-at-a-time limits are a server rule.
  A worker uses its own per-worker job limit instead.

### UI and parity

- **Settings, Cluster:**
  - the workers, each with online status, CPU, RAM, GPU, tools, jobs and
    capabilities;
  - Add a PC (join code and the per-OS one-liners), rename, pause, revoke;
  - pending setup approvals.
- **Chats:** the "Runs on" picker and the PC badge in the header. These also
  go on the compact chat page (/chat/<id>).
- **Factory editor:** "Runs on" for each person and step, in the web editor
  and the Android one.
- **Android:** the same Cluster screen, the picker, and the factory field,
  in the same release.
- **Desktop app:** the "Use this PC as a worker" switch, which runs the
  bundled worker.
- **What's new:** one step per phase that ships something visible.
- **Compatibility:**
  - Everything is new endpoints and optional fields: `/api/cluster/*`,
    `where`, `runsOn`. New capability `cluster`.
  - Older clients simply do not show any of it.
  - MIN_CLIENT and MIN_SERVER stay as they are.

### Tests (in npm run check)

An in-process fake worker on a loopback websocket:

- the join code is single-use, and a revoked key is refused;
- `worker_run` streams output, returns the exit code, brings artifacts back,
  and enforces the timeout;
- `git` sync builds the server's exact tree, uncommitted changes included;
- a cwd outside the work folder is refused on the worker;
- a chat on the worker:
  - stream-json goes through the launcher;
  - MCP calls carry the chat key, never the API key;
  - the turn-end bundle lands on the server worktree;
- a factory with a step on the worker:
  - the step is held when the worker drops, and the others carry on;
  - the Manager on the worker pauses the run;
  - reconnect resumes the step;
  - handoff is refused until the bundle is applied.

## Phases (each shippable on dev, in this order)

1. **Workers:** the program, joining, info, Cluster settings, desktop-app
   bundling. Nothing runs yet.
2. **Commands on a PC:** `worker_run`, sync, artifacts, jobs. This is most
   of the resource win: builds and renders off the server.
3. **PC setup:** prepare a project, tool recipes, approvals.
4. **Whole chats on a PC:** the launcher seam, MCP forwarding, branch and
   bundle, reconnect.
5. **Factory steps on a PC:** `runsOn`, hold and resume on disconnect, the
   handoff bundle.

## Decisions for the owner

1. **Worker runtime.** The plan: a Node script, with portable Node fetched if
   missing, and the desktop app runs it in-process. Alternative: ship a
   single binary (bigger work, no Node needed).
2. **Changes made on a PC.** The plan: a branch per PC chat, brought back as
   git bundles to a server worktree, and the owner merges. Alternative: sync
   straight into the server's project folder (simpler, but two machines
   writing one tree).
3. **Where the PC's code comes from.** The plan: cloned from the server over
   the worker link (no credentials on the PC). Alternative: the PC's own
   GitHub access.
4. **Installs on a headless worker.** The plan: approved in the server UI,
   only if the PC allowed installs when it joined. Alternative: always on the
   PC itself.
5. **Who can use workers.** The plan: the owner's chats and factories only,
   never collaborators.
6. **Order.** The plan: commands (phase 2) before whole chats (phase 4),
   because commands give the resource win soonest.
