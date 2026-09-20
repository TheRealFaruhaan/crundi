---
name: crundi
description: How to build, run, test and ship a project on a machine running Crundi, working through the Crundi interface — using the crundi MCP tools (services, forwards, tunnels, browser, terminals, secrets, kanban, mindmap, schedules, notifications) instead of shelling out, and running dev servers on ports that are safe to take and reachable by the user. Platform-specific throughout: Linux hosts and Windows hosts differ in paths, shells, service management, port checks and which tools exist at all. Use whenever the crundi MCP server is connected — starting or exposing a dev server or long-running process, previewing work for a user who is on a phone, driving a real browser, handling credentials, or reading/writing board, mindmap or schedule state.
---

# Crundi

Crundi is a self-hosted workbench: it runs projects, background services, terminals
and schedules on one machine, and serves a web UI you reach from anywhere. When it
is running, Claude sessions on that machine get a `crundi` MCP server whose tools
act on the real machine.

## Establish the platform first

Crundi runs on **Linux** (usually a headless server, installed by `install.sh`,
managed by systemd) and on **Windows** (usually the desktop app, installed by the
NSIS installer, or a headless server under pm2). Paths, service commands, port
checks and *which MCP tools work at all* differ between them. Getting this wrong
wastes calls on tools that cannot work and prints paths that do not exist.

Check once, cheaply, before giving any command or path:

```bash
uname -s        # Linux → Linux host
```

```powershell
$PSVersionTable.OS    # or: [System.Environment]::OSVersion
```

If `Bash` is a normal shell with `ss`, `systemctl` and `/home`, treat it as Linux.
If paths look like `C:\Users\...` and `%APPDATA%` resolves, treat it as Windows.

Then read the matching file and follow it, not the other one:

- **Linux:** `reference/platform-linux.md`
- **Windows:** `reference/platform-windows.md`

The rest of this file is what holds on both.

## Ground rules

**Never bind Crundi's own ports.** Taking one kills or silently breaks the server
running you. Defaults, on both platforms:

| Port | Owner | Consequence of taking it |
|---|---|---|
| 443 | Crundi HTTPS (when TLS is on) | The site goes down |
| 80 | ACME + HTTP redirect | Certificate renewal stops — silently, until expiry |
| 8888 | Loopback API (`127.0.0.1`) | Every MCP call fails with "socket hang up" |

Other work on the box takes ports too, so check before binding — the command to
check is platform-specific, see your platform file. Pick a high port (4000–9999,
excluding Crundi's) and bind to loopback unless the thing must be reachable
externally. A dev server on `0.0.0.0` on a public box is on the internet. To
publish something, use `add_forward` — never an open firewall port.

**Restarting the server ends every running chat and terminal on it**, including
other people's. Ask first; it is not a decision to make alone.

**Prefer the MCP tool over the shell.** `register_service` over a backgrounded
start command; `add_forward` over opening a port; `secret_run` over pasting a
token. The tools leave state the UI can see and the user can manage.

**Nothing here assumes a language, framework or project layout.** Find the
project's own commands — its README, build file, scripts or CI config — and run
those. This skill covers the Crundi side of the work, not how any particular
project is built.

## The MCP tools

Full catalogue with signatures and per-platform availability:
`reference/mcp-tools.md`.

### Reaching the user

`send_message_to_user`, `send_photo_to_user`, `send_file_to_user` arrive even when
nobody is watching the transcript. Use them for anything worth knowing before the
user next looks — a long job finishing, a blocker, a result they asked to be told
about. Do not narrate routine progress into them.

### Long-running processes

Register them as services instead of leaving them in a foreground shell:

```
register_service { name, command, cwd, stopCommand? }   → key "alias::name"
start_service / stop_service / restart_service { key }
get_service_logs { key, lines }
list_services
```

A service survives the chat that created it and the terminal it started in. A
backgrounded shell command does not, and nothing in the UI can manage it. The
`command` itself is shell, so it is platform-specific — write it for the host you
are actually on.

### Exposing a port

- **`add_forward`** — this server answers for the port on its own certificate.
  `mode: "subdomain"` → `name.<domain>`; `mode: "path"` → `<domain>/tunnel/name/`
  (needs no DNS). The hostname is chosen and stable. **Private by default**:
  private forwards require a Crundi sign-in. Set `public: true` only for
  something with its own authentication — an OAuth callback, an app behind its
  own login.
- **`enable_tunnel`** — a Cloudflare tunnel for a local port. Hostname is not
  stable. Use when a forward will not do.

Prefer a forward.

### Driving a browser

`browser_open` → `browser_navigate` / `browser_click` / `browser_fill` /
`browser_eval` / `browser_snapshot` / `browser_console` / `browser_screenshot`.
Check what a page actually does rather than reasoning about it — especially after
a UI change. Works on both platforms (bundled headless browser, or the desktop
app's webview). Up to 5 instances per project; close them when done.

### Secrets

```
secret_search { query }   → names, descriptions, ids. Never values. No approval.
secret_run    { command, id|name, envName?, reason }
secret_get    { id|name, reason }
```

**Prefer `secret_run`.** It binds the value to an environment variable for one
command; the value never enters the transcript, and stdout/stderr come back with
it redacted. Reach for `secret_get` only when you genuinely must know the value.

Both block until the user approves and enters that secret's PIN (~3 min timeout).
Always give a real `reason` — they read it before approving. The `command` runs in
the host's shell, so quote and reference the variable the way that shell expects.

Each secret is sealed with a key derived from its own PIN, which is never stored —
a lost PIN means delete and recreate, with no recovery. Names and descriptions are
stored in the clear and searchable without authentication; never put a value, or
anything you would not show a collaborator, in a description.

### Project state

`kanban_*` (board: backlog / todo / in_progress / done), `mindmap_*` (idea tree,
linkable to tasks), `schedule_*` (recurring project tasks), `media_*` (files
attached to tasks or nodes).

These are scoped to the current project and are what the user sees in the UI —
shared state, not scratch space. Read before writing (`kanban_list_column` and
`mindmap_search` are the cheap reads; `kanban_list` and `mindmap_list` can be
large). The mindmap view is partial: unscoped and other-project nodes are
invisible, so "not found" may mean "not linked to this project" — say that rather
than concluding it does not exist.

### Platform-limited tools

`spawn_terminal` / `terminal_*` and `capture_window` / `capture_display` /
`list_windows` need a desktop host. On a headless server they return a graceful
error. See the availability table in `reference/mcp-tools.md` before reaching for
them.

### Other

`get_usage` — the account's real 5-hour and weekly limit utilisation.
`syntax_check { files }` — fast parse of files you just wrote, any language.
`list_forwards`, `list_terminals`, `list_services` — check state before changing it.

## Building, testing and shipping a project here

Full loop, with the platform differences called out:
`reference/developing-on-crundi.md`. The shape of it:

1. **Anything long-running becomes a service.** `register_service` survives the
   chat and the user can manage it; a backgrounded command in a Bash call does
   not. Check `get_service_logs` before calling it up — a crash on boot still
   counts as "started".
2. **If it listens on a port, take one that is safe.** Check it is free first,
   pick 4000–9999 avoiding Crundi's own, and bind `127.0.0.1`.
3. **If the user needs to reach it, forward it.** `add_forward` gives a stable
   URL on the server's certificate, private (sign-in required) by default. Send
   them the URL.
4. **Test what actually runs**, with the project's own tests or checks. If it has
   a web interface, also open it with the browser tools and read
   `browser_console` — a silent error looks exactly like a styling problem.
5. **Show, don't describe.** The user is often on a phone and cannot see your
   terminal; `send_photo_to_user` with a screenshot settles a visual question in
   one round trip.
6. **Clean up.** Stop and delete services you created for the work, remove
   forwards, close browser instances.

For credentials at deploy time, use `secret_run` — never paste a token into a
file, a command you echo, a commit, or a service definition.

## Maintaining this skill

This skill ships with Crundi and is **overwritten on every install**, on both
platforms. Edit the copy in the Crundi repository's `skills/` directory, never
the installed copy, and keep it generic — no hostnames, domains, usernames,
project names, secret names or addresses.
