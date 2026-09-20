# Working on a Linux Crundi host

Applies when the host is Linux. For Windows read `platform-windows.md` instead —
none of the commands below transfer.

The usual shape: a headless server reached over HTTPS, running as a normal
(non-root) user under systemd, with projects in that user's home directory.

## Checking a port before you bind it

```bash
ss -ltn | awk '{print $4}' | grep -E ':(PORT)$'   # empty output means free
```

Bind `127.0.0.1` unless the thing genuinely must be reachable from outside. On a
box with a public IP, `0.0.0.0` is the internet. To let the user reach a dev
server, use `add_forward`, not an open port.

## Starting things

Run anything long-lived through `register_service` rather than backgrounding it.
`command` goes through a POSIX shell, so `&&`, `$VAR`, pipes and quoting behave
as expected; `cwd` must be absolute. Examples — whatever the project actually
uses goes here:

```
register_service { name: "app-dev",
                   command: "npm run dev -- --port 4300 --host 127.0.0.1",
                   cwd: "/home/<user>/projects/<project>" }

register_service { name: "api",
                   command: ".venv/bin/uvicorn app:app --host 127.0.0.1 --port 4310",
                   cwd: "/home/<user>/projects/<project>" }

register_service { name: "worker",
                   command: "make worker",
                   cwd: "/home/<user>/projects/<project>",
                   stopCommand: "make worker-stop" }
```

A process that writes a pidfile or daemonises itself usually needs a
`stopCommand`; one that stays in the foreground and dies on SIGTERM does not.

For one-off commands use `Bash` directly. Do not use `spawn_terminal` here — see
below.

## Which MCP tools work here

Headless, so two groups are unavailable:

- **Terminal tools** (`spawn_terminal`, `terminal_input`, `terminal_wait`, …)
  return a graceful error; they need the Electron desktop app. Use `Bash` for
  commands and a registered service for anything long-lived.
- **Screen capture** (`capture_window`, `capture_display`, `list_windows`,
  `list_displays`) errors with "needs a machine with a display".

**Browser tools do work**, through the bundled headless browser — so you can still
open the dev server, read the console and take screenshots. Everything else
(services, forwards, tunnels, secrets, board, mindmap, schedules, messaging,
usage) works normally.

## The Crundi server itself

| | Path |
|---|---|
| Install prefix | `~/.local/share/crundi` |
| Data and config | `~/.config/crundi/` — state in `data/`, `.env` alongside |
| Skills for Claude | `~/.claude/skills/` |

Status and logs:

```bash
systemctl --user status crundi                  # user install
journalctl --user -u crundi -n 100 --no-pager

systemctl status crundi                         # system unit (installed --as-user)
journalctl -u crundi -n 100 --no-pager
```

**Restarting ends every running chat and terminal on the machine**, including
other people's. Ask before doing it; it is not a decision to make alone.

Upgrades are a re-run of the installer (`scripts/install.sh`, which keeps data) or
the in-app updater. Never install or run it as root: Claude Code refuses
`--dangerously-skip-permissions` when its euid is 0, so a root install silently
loses that mode and inherits root's git config and SSH keys.

## sudo

If the account has passwordless sudo, it exists for tasks that genuinely need it —
system packages, system services — not as a default. Anything run under it can
escalate, and it reaches the whole machine, not just this project.
