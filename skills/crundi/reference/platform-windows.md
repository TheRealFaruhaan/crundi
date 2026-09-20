# Working on a Windows Crundi host

Applies when the host is Windows. For Linux read `platform-linux.md` instead —
none of the commands below transfer.

Two shapes:

- **The desktop app** (the usual one) — Crundi runs as a Windows application with
  a real desktop session, so the terminal and screen-capture tools work.
- **A headless Windows server** — the server run under pm2, no Electron. Same
  limits as a Linux headless box for terminals and capture.

## Checking a port before you bind it

```powershell
Get-NetTCPConnection -LocalPort PORT -ErrorAction SilentlyContinue   # no output = free
```

```cmd
netstat -ano | findstr :PORT
```

Bind `127.0.0.1` unless the thing must be reachable from outside. Binding a
non-loopback address can raise a Windows Firewall prompt — and a prompt nobody is
there to answer leaves a service that looks started and is unreachable. To let the
user reach a dev server, use `add_forward`, not a firewall rule.

## Starting things

Run anything long-lived through `register_service` rather than leaving it in a
shell. Examples — whatever the project actually uses goes here:

```
register_service { name: "app-dev",
                   command: "npm run dev -- --port 4300 --host 127.0.0.1",
                   cwd: "C:\\Users\\<user>\\projects\\<project>" }

register_service { name: "api",
                   command: ".venv\\Scripts\\uvicorn.exe app:app --host 127.0.0.1 --port 4310",
                   cwd: "C:\\Users\\<user>\\projects\\<project>" }

register_service { name: "worker",
                   command: "powershell -NoProfile -File .\\scripts\\worker.ps1",
                   cwd: "C:\\Users\\<user>\\projects\\<project>",
                   stopCommand: "taskkill /IM worker.exe /F" }
```

`command` runs through the Windows shell. A command copied from Linux notes will
not behave — POSIX quoting, `$VAR` expansion, `/`-style paths, `&&` chains and
`.venv/bin/...` layouts all differ (`Scripts\`, not `bin/`). Use backslash paths
and quote anything containing spaces.

Windows has no POSIX `SIGTERM`, so anything that does not stop cleanly needs an
explicit `stopCommand` — `taskkill`, or whatever the tool provides.

## Which MCP tools work here

Under the **desktop app**, everything works — including the two groups a headless
host cannot offer:

- **Terminal tools** — `spawn_terminal`, `terminal_input`, `terminal_wait`,
  `terminal_output`, `close_terminal`, `list_terminals`. Real terminals that
  outlive a single tool call; good for an interactive process you need to watch.
- **Screen capture** — `list_windows`, `list_displays`, `capture_window`,
  `capture_display`, and `send_window_screenshot_to_user` /
  `send_display_screenshot_to_user`. Use these to show the user a desktop app's
  actual window, not just a browser page.
- **`disconnect_rdp`** — Windows only. Drops an RDP session while keeping the GUI
  session alive, so capture keeps working on a locked screen.

Under a **headless Windows server**, terminal and capture tools return graceful
errors; use ordinary shell commands and registered services instead. Browser
tools work in both cases.

## The Crundi server itself

| | Path |
|---|---|
| Install location (desktop app) | `%LOCALAPPDATA%\Programs\Crundi` |
| Data and config | `%APPDATA%\Crundi\` — state in `data\`, `.env` alongside |
| Skills for Claude | `%USERPROFILE%\.claude\skills\` |

The desktop app is an ordinary application: quitting it stops the server, and it
updates itself through the in-app updater.

A headless server under pm2, from its install directory:

```powershell
npm run service:status
npm run service:logs
npm run service:restart     # ends EVERY chat and terminal — ask first
```

Startup at login is a Task Scheduler entry running `pm2 resurrect`; no admin
rights are needed. **There is no `systemctl` here — never suggest it.**

**Restarting ends every running chat and terminal on the machine**, including
other people's. Ask first.

## Elevation

Windows elevates through UAC, not `sudo`. A non-elevated session simply cannot do
some things, and there is no password prompt you can answer on the user's behalf —
if a step needs administrator rights, say so and let the user run it.
