# Crundi cluster: chats and commands on connected PCs (design plan)

Card 6ab8d6b963784ecf. **Nothing here is built yet.** It is grounded in the
code as it is on dev (1.21.8-dev.3); file references are where each part
would hook in. Revision 2 (18 Nov) takes in the owner's decisions; see the
last section for them and for what is still open.

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

### 1. The worker: a compiled tray app that joins with a token

- **What it is:** `crundi-worker`, a compiled Rust program. One crate, two
  ways to run it:
  - **Tray app** (Windows, macOS, Linux desktops): a tray icon showing
    status (connected, jobs running, chats), Pause, Open work folder, and
    Quit. It starts at login: a Run key on Windows, a login item on macOS,
    an XDG autostart entry on Linux. It can be quit at any time; work in
    progress on it is then treated as "PC offline" (sections 2-4).
  - **Headless** (`crundi-worker --headless`) for Linux servers: no tray,
    runs as a systemd unit, logs to the journal.
  - **Built into the Crundi PC client:** for now the **Electron** client
    (the native one is still being built). It ships the worker binary and
    runs it as a child process behind an on/off switch in the client's
    settings, so a PC with the client needs no separate install. The native
    client links the same crate later; the owner briefs that chat once this
    is built. The tray app and the
    client never both run as a worker on one PC: the second one says so and
    stays off.
- **Building it:** with the native client's build rules (CARGO_BUILD_JOBS=2,
  one build at a time), in its own crate, so the tray app does not pull in
  GPUI. The Electron client's release jobs build it per OS and put it in
  the app's resources.
- **Joining:**
  1. Settings, Cluster, Add a PC makes a one-time join code (5 minutes,
     like pairing).
  2. The worker trades it for a long-lived worker key, `cw_<id>.<secret>`.
     The server keeps only its hash, like a homepool node key.
  3. The worker connects to `wss://<server>/api/cluster/worker` with it.
  4. The owner can rename, pause or revoke a worker. Revoking closes the
     socket at once.
  - In the PC client the key comes from the client's own sign-in: the
    switch is enough.
  - The tray app's first run asks for the server address and the join
    code; the headless one takes them as arguments.
- **A blank terminal on the PC:** like the server's own terminal, a shell on
  the PC opened from Crundi (in the Cluster screen and in a project), carried
  over the worker link. It is how the owner signs in to GitHub on that PC
  (`gh auth login`), installs a tool, or looks around. Owner only.
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
- **Trust follows the chat's permission mode** (the owner, 18 Nov). A PC
  is trusted the way the owner's own PC is when he runs `claude` there:
  - A whole chat on a PC is `claude` on that PC, with the permission mode
    the chat has (bypass included). It installs, edits, builds and pushes
    exactly as it would on the server.
  - A command on a PC (`worker_run`) is an ordinary tool call of a server
    chat: run at once in bypass mode, asked for like any tool otherwise.
  - So there are no command allowlists, no folder fence and no separate
    install approvals. The work folder is where clones and copies go, not
    a limit.
- **What stays, because it is not about the chat:**
  - Joining with a one-time code and a revocable key: which machines the
    server may command at all.
  - Per-PC switches, "allow chats" and "allow commands", for a machine the
    owner wants used one way only.
  - **Never collaborators:** their chats cannot reach a worker in any
    mode; the tools refuse a collaborator's key.
  - The job history (what ran, where, for which chat): a record for the
    owner, not a gate.
  - Secrets as on the server today: a job gets one only through the
    existing secret tools.

### 2. Commands only on a PC

- **New MCP tools**, owner chats only:
  - `worker_list`: the workers with their info and status.
  - `worker_run { worker, command, source?, cwd?, paths?, artifacts?, timeout?, background? }`
    runs `command` in the project's copy on that PC and streams output into
    the chat as a card, live, like a long tool result. It returns the exit
    code, the tail of the output and where the artifacts landed.
  - `worker_job` (status, logs, cancel) for `background: true` jobs, which
    are kept server-side like services' logs.
- **Where the code comes from:** one of two sources, `source: 'git' | 'copy'`,
  picked per job (and per chat or factory step):
  - **`git`** (described next): a GitHub clone and a local branch.
  - **`copy`** (further down): a bare copy sent over the worker link, for a
    PC without git, or a task that does not want it.
  - Default: `git` when the project has a GitHub remote and the PC has git;
    otherwise `copy`. A job that names one gets that one or a clear error.
- **`git`: GitHub, per the owner.**
  - Each worker has a **PC remote folder** (its work folder, chosen at
    setup). The first job for a project clones the project's GitHub remote
    into `<work folder>/<project>`, with the PC's own GitHub access. If it
    has none, the job says so and points at the PC's terminal to run
    `gh auth login`.
  - The server sends **the branch** to work from (`branch`, default the
    project's current branch) and the commit it expects. The PC fetches and
    **creates its own local branch from it** (`pc/<job or chat>`). Nothing
    is created or pushed on GitHub for this, so no empty branches appear
    there.
  - If the server has commits GitHub does not have yet (local, unpushed),
    the PC fetches those from the server's worktree over the worker link
    (`git fetch` through a git endpoint the server serves to workers only).
    The build is then of the exact commit the server has, without making
    the owner push first. Uncommitted server changes are not sent; the job
    says so when there are any.
- **`copy`: a bare copy over the worker link.** No git needed on the PC,
  and no GitHub access.
  - The server sends the project's files as they are on the server, working
    changes included, into `<work folder>/<project>@copy` on the PC (or a
    folder per job when two jobs must not share).
  - **What is sent:** with git on the server, the tracked files plus
    untracked files that are not ignored; without it, everything except the
    usual heavy and generated folders (`node_modules`, `target`, `dist`,
    `build`, `.git`) and whatever a `.crundiignore` in the project lists.
    `paths` narrows it to some folders or files for a task that needs only
    those.
  - **Only what changed:** the PC answers with a manifest (path, size,
    hash) of the copy it already has; the server sends only new and changed
    files and a list of removed ones, as one tar stream. A second build of a
    large app sends little.
  - Not a git repository on the PC, so a chat or step in copy mode cannot
    commit or push from there; its changes come back as files (section 3).
- **For a build or a plain command** that is all: check out (or copy),
  run, return the output and artifacts. Nothing goes back to git.
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
- **Where the code lives:** with `git`, the chat works in the PC's clone,
  on a local branch the PC creates from the branch the server sent (as in
  section 2). With `copy`, it works in its own bare copy (see below).
  - **Changes come back by pulling from the PC's worktree:** the server
    `git fetch`es the PC's branch over the worker link into a matching
    worktree on the server, at each turn end and on demand. The owner sees
    the changes, diffs and history as usual, and merges into dev as with
    factory runs. Nothing writes into the server's main checkout.
  - Pushing that branch to GitHub is optional: the chat can do it from the
    PC when the work should be there, with the PC's GitHub access.
  - **In copy mode** changes come back as files: at each turn end the PC
    sends a manifest, and the server takes the changed, new and removed
    files (relative to what it sent) into a worktree of its own made for
    that chat, then commits them there on a branch `pc/<worker>/<chat>`.
    The owner reviews and merges it like any other. If the server project
    has no git, they land in `<project>/.crundi/pc-changes/<chat>/` with a
    list of removed files, for the owner to apply.
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
  - The step's work happens in the PC's clone, on a local branch made from
    the run's branch (sent with the step, as in section 2).
  - At the step's start the PC brings that branch up to the run branch's
    head (from GitHub, plus the server's worktree for unpushed commits).
  - At handoff the runner pulls the PC's branch into the run's worktree on
    the server **before** it accepts the handoff. A join therefore never
    passes part-built work.
  - A step in copy mode: the run's worktree is sent as a copy at the step's
    start (only what changed since its last copy), and the step's changed
    files are taken back into the run's worktree before the handoff is
    accepted, the same guarantee.
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

- **Work folder (the PC remote folder):** chosen when the worker is set up
  (in the tray app's first run, the client's switch, or `--work-dir`); the
  default is `~/crundi-work`. Every project is cloned to
  `<work>/<project>`, and every job, chat or run branch lives in it.
- **Prepare a project:** done on the first job, or with "Prepare on <PC>"
  in the project menu: clone from GitHub (section 2), then the project's
  setup command if it has one (the same one factory runs use).
- **Tools (what "installs" means):** the things a job needs that the PC
  may lack: git, Node/npm, Rust (rustup, cargo), Python, ffmpeg, Docker,
  platform SDKs (Android SDK and a JDK, Visual Studio build tools on
  Windows), and the Claude Code CLI for whole chats.
  - The worker reports which it has, with versions. git is needed only for
    the `git` source; copy mode works without it.
  - A job that needs a missing one stops before it starts and says what is
    missing, with the install command for that OS (winget, Homebrew,
    apt/dnf, rustup, the Claude Code installer).
  - **Installing is seamless, as in any chat** (the owner, 18 Nov): the
    chat (or the job's chat) installs what is missing itself, under its
    permission mode; in bypass it just does it. The only stop is what the
    OS itself guards: a Windows UAC prompt, or `sudo` needing a password
    where the PC does not allow it without one. The chat says so and
    carries on with what it can. The PC's terminal is there for the owner
    too.
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
- **PC client:** the "Use this PC as a worker" switch in its settings,
  running the built-in worker.
- **Tray app:** status, Pause, Open work folder, Quit; first-run setup.
- **Electron client:** the "Use this PC as a worker" switch runs the
  bundled worker binary.
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
- the PC makes its local branch from the branch sent, fetching unpushed
  commits from the server, and nothing is pushed to GitHub;
- the server pulls a PC branch into its worktree;
- copy mode: the first copy sends the project (ignores respected,
  `paths` narrowing it), a second sends only what changed, removed files
  are removed; a chat's changed files come back into a server worktree and
  are committed on its branch; a step's come back before its handoff;
- a chat on the worker:
  - stream-json goes through the launcher;
  - MCP calls carry the chat key, never the API key;
  - the PC branch is pulled into the server worktree at turn end;
- a factory with a step on the worker:
  - the step is held when the worker drops, and the others carry on;
  - the Manager on the worker pauses the run;
  - reconnect resumes the step;
  - handoff is refused until the PC branch is pulled in.
- The Rust worker has its own tests (cargo test, -j2) for the protocol,
  the work-folder guard and git steps, against a fake server.

## Phases (each shippable on dev, in this order)

1. **Workers:** the Rust worker (tray and headless), joining, info, the
   PC terminal, Cluster settings. Linking it into the PC client follows
   when that client is ready for it. Nothing runs jobs yet.
2. **Commands on a PC:** `worker_run`, both sources (clone and branch, and
   the bare copy), artifacts, jobs.
   This is most of the resource win: builds and renders off the server.
3. **PC setup:** prepare a project, tool detection and install commands.
4. **Whole chats on a PC:** the launcher seam, MCP forwarding, pulling the
   PC branch, reconnect.
5. **Factory steps on a PC:** `runsOn`, hold and resume on disconnect,
   pulling the PC branch before handoff.

## Decisions (the owner, 18 Nov)

1. **Worker:** a compiled tray app that starts at login and can be quit; a
   headless mode for Linux servers; and built into the PC client with an
   on/off switch in its settings, so no separate install there.
2. **Changes made on a PC** come back by the server pulling from the PC's
   worktree.
3. **Code comes from GitHub.** The PC clones into its PC remote folder and
   creates a local branch from whatever branch it was sent; Crundi does not
   create or push a branch for it. GitHub sign-in on the PC is done by the
   owner in a blank terminal on that PC, as on the server. Pushing is only
   when the work needs to be on GitHub; a build or a command needs none.
4. **Installs:** the owner asked what they are; answered in section 5.
5. **Never collaborators.**
6. **Order:** commands before whole chats, as planned.
7. **A bare copy over the worker link** as well as GitHub (owner, 18 Nov),
   for a PC without git or a task that does not want it: `source: 'copy'`,
   sections 2-4.

8. **Installs:** seamless, as in any chat, under the chat's permission
   mode (bypass included). No separate approvals.
9. **Unpushed commits:** the PC clones the pushed commit from GitHub and
   fetches what the server has ahead of it over the worker link.
10. **PC client:** the Electron client for now (it bundles and runs the
    worker binary). The native client later; the owner briefs its chat.
11. **Security follows the chat's permission mode** (section 1): no
    allowlists, no folder fence; joining, per-PC switches, never
    collaborators and the job history stay.

## Revision 3: approved by the owner (18 Nov), all ten

From the "not gimmicky, professional" review, 18 Nov:
1. Warm build caches per worker (cargo target, node_modules by lockfile),
   and content-addressed copies, so offloading is actually faster; show
   "ran on X in N" on the job card.
2. Labels per worker (os, gpu, ram, tools); jobs and factories ask for
   labels, a PC name is an override; a fallback per job (wait, server,
   fail).
3. Be a good guest on the PC: low priority, core and RAM caps, "only when
   I'm away", pause on battery or a full-screen app, instant Pause that
   suspends rather than kills.
4. Jobs survive a dropped link: they keep running, the log is spooled on
   the PC and resumes from its offset, a lease and job id prevent running
   twice; offline only after a grace period.
5. Job history and a per-worker page (what ran, queue, load, disk).
6. Signed worker updates, applied when idle; version handling by the
   capability lists.
7. Artifacts in Crundi's data folder with retention and checksums, not in
   the project; a disk limit and clean-up per worker.
8. Copy mode handles case-insensitive file systems, long Windows paths,
   the executable bit, symlinks and line endings, each with a test.
9. Name it "Workers" (or "Compute") rather than "Cluster".
10. Ship commands first and use them for a while before whole chats.

## Status

- Phase 1 shipped in 1.21.8-dev.3 (joining, info, labels, pause, terminal).
- Phase 2, commands on a PC (worker_run / worker_job, git and copy sources,
  artifacts, logs that survive the link dropping, low priority, caches kept),
  is in 1.21.8-dev.4. Contract: specs/workers-protocol.md.
- Still to come: be-a-good-guest beyond low priority (only when away, on
  battery), whole chats on a PC (phase 4), factory steps on a PC (phase 5),
  signed worker updates.
