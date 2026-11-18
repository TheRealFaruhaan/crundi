# crundi-worker

Lets a Crundi server use this PC. Phase 1: join with a one-time code, keep a
websocket link to the server, report info (OS, CPU, RAM, GPUs, disk, tools,
idle time, battery), pause and resume, and give the owner a terminal on the
PC. Phase 2: run commands (jobs). Phase 4: run whole chats. The contract is
[`specs/workers-protocol.md`](../specs/workers-protocol.md).

One crate (`crundi-worker`, lib + bin). With the `tray` feature (default) it
is a tray app; built with `--no-default-features` it is headless only.

## Build

You need Rust (stable, via rustup). TLS is rustls with the webpki roots, so
there is no OpenSSL dependency.

### Windows

```
cargo build --release
```

The tray build is a GUI-subsystem program (no console window); command-line
use still prints to the terminal it was started from.

### macOS

```
cargo build --release
```

### Linux

The tray needs GTK 3, an AppIndicator library and libxdo:

```
sudo apt install libgtk-3-dev libayatana-appindicator3-dev libxdo-dev
cargo build --release
```

For a server without a desktop, build headless (no GTK needed):

```
cargo build --release --no-default-features
```

The binary is `target/release/crundi-worker`.

### Tests

```
cargo test --no-default-features
```

The tests run a join against a small in-test HTTP server and the link against
an in-test websocket server, all on 127.0.0.1 with random ports.

## Jobs (phase 2)

The server can run a command on the PC (one job at a time; refused while
paused or when commands are not allowed). The code comes from:

- `git`: cloned once into `<work>/<project>` with the PC's own git, then
  fetched and checked out as branch `crundi/job` (a commit the remote does
  not have comes from the server as a bundle). Nothing is cleaned, so build
  caches (`target/`, `node_modules/`, ...) persist between jobs.
- `copy`: `<work>/<project>@copy`, kept in step with the server by a manifest
  (cached hashes in `.crundi-manifest.json`) and a tar of changed files.
- `none`: `<work>/<project>` (or the work folder) as it is.

Commands run through `sh -lc` (Windows: `cmd /d /s /c`) at low priority, in
their own process group so cancel and timeout stop the whole tree. Output is
spooled to `<work>/.crundi-jobs/<job>.log`, so a job keeps running when the
link drops and its output is resent from where the server left off.
Artifacts (globs relative to the project folder) come back as a .tar.gz.
When a commit is not on origin the server sends a bundle; the PC offers as
`have` the newer of `origin/<branch>` and what an earlier bundle brought
(`refs/crundi-bundle/heads/<branch>`), so only new commits come.

## Chats (phase 4)

The server can run a whole chat on the PC with the PC's own `claude`
(installed and signed in there; found on PATH or in `~/.local/bin`,
`~/.claude/local`, `%USERPROFILE%\.local\bin`, the npm global folder).
Refused while paused, when chats are not allowed, or when claude is missing
(with the install line). The folder:

- `git`: the project's clone (as for jobs), plus a worktree
  `<work>/<project>@<chat>` on branch `pc/<chat>`, made once from the
  chat's commit and kept, so a resumed chat finds its work as it left it
  (and a job's checkout never touches it).
- `copy`: `<work>/<project>@copy`, synced once when the chat starts (a
  manifest of it is kept as the chat's baseline).
- `none`: `<work>/<project>` or the work folder.

claude runs with its three pipes (not at low priority) and
`--mcp-config <work>/.crundi-chats/<chat>.mcp.json`, which names this same
executable as the chat's MCP server (`crundi-worker mcp ...`). That talks
to a loopback relay the worker opens for the chat (127.0.0.1, random port,
a token); tool calls go to the server over the link and wait up to 10
minutes, across reconnects. stdout is spooled to
`<work>/.crundi-chats/<chat>.out`, so a chat keeps running when the link
drops and its output is resent from where the server left off.

`chat.pull` sends the chat's work back: git, a bundle of `pc/<chat>` since
the base commit plus `git diff --binary HEAD` (new files included; the
PC's index is left as it was); copy, a tar of files changed since the
baseline plus the removed paths.

## Use

1. On the server: Workers, Add a PC. You get an 8-character join code (works
   once, for 10 minutes).
2. On the PC, either:
   - run `crundi-worker` (tray). The first time it opens a setup page in your
     browser (served on 127.0.0.1 with a one-time link) for the server address,
     the code, a name and the work folder; or "Join a server..." in the tray
     menu; or
   - `crundi-worker join https://your-server <CODE> [--name N] [--work-dir D]`
     and then `crundi-worker --headless` (or `install-service` on Linux).

The tray menu: status line, Pause/Resume, Open work folder, Join a server...
(when not joined), Start at login, Quit.

## CLI

```
crundi-worker                          tray if built with it and there is a desktop, else headless
crundi-worker --headless               no tray, logs to stderr
crundi-worker join <server-url> <code> [--name N] [--work-dir D]
crundi-worker --key <cw_...> --server <url> [--work-dir D] --embedded
                                       run with a key from the PC client (not saved);
                                       JSON status lines on stdout, exit when stdin closes
crundi-worker status                   print the config and whether joined
crundi-worker install-service          Linux: systemd user unit, enabled and started
crundi-worker uninstall-service
crundi-worker autostart on|off         Windows Run key, macOS LaunchAgent, Linux XDG autostart
crundi-worker mcp --chat C --port P --token T
                                       a chat's MCP server on stdio (claude starts it)
--config <path>                        use another config file
--kind <tray|headless|electron|native> override the kind reported to the server
```

Exit codes: 0 quit, 2 not joined, 3 the server refused the key (401) or
revoked/removed this PC (the key is forgotten; join again), 4 another link
for this worker took over (`bye replaced`), 64 bad arguments.

### Embedded mode (for the PC client)

stdout gets one JSON object per line:

```
{"event":"status","state":"online","online":true,"server":"https://...","id":"w...","name":"...",
 "paused":false,"allowChats":true,"allowCommands":true,"terminals":0,"line":"desk: online"}
{"event":"job","state":"started","job":"j...","command":"cargo test","project":"app"}
{"event":"job","state":"ended","job":"j...","command":"cargo test","project":"app","exitCode":0}
{"event":"chat","state":"started","chat":"...","project":"app"}
{"event":"chat","state":"ended","chat":"...","project":"app"}
{"event":"exit","code":0,"outcome":"Quit"}
```

The job and chat lines are for the client to notify. The tray shows the same
as desktop notifications (Windows and macOS through notify-rust; Linux runs
`notify-send` when it is there).

`state` is one of `notJoined`, `connecting`, `online`, `offline` (with
`error` and `retryInSec`), `unauthorized`, `removed`, `replaced`, `stopped`.
stdin takes `{"type":"pause","paused":true}` and `{"type":"quit"}` lines;
stdin closing means exit.

## Files

- Config: `<config dir>/crundi-worker/config.json` (`~/.config/...` on Linux,
  `%APPDATA%\crundi-worker\` on Windows, `~/Library/Application Support/...`
  on macOS). Holds `server, id, key, name, workDir, allowChats, allowCommands,
  paused, deviceId`. Mode 0600 on Unix.
- Work folder: `~/crundi-work` by default.
- Linux service: `~/.config/systemd/user/crundi-worker.service`
  (`journalctl --user -u crundi-worker -f`; `sudo loginctl enable-linger $USER`
  to keep it running while logged out).
