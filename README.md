<p align="center"><img src="assets/logo.svg" alt="" width="112" height="112"></p>

# Crundi

A self-hosted remote workbench for your projects. Crundi runs on your machine and exposes your projects — agent terminals, files, git, a task board, scheduling and more — through a mobile-friendly web UI reachable from anywhere over a secure tunnel. Your phone becomes a window into your dev box at home.

Agents and the access channel are pluggable. Today the available agent is **Claude Code** (interactive CLI — your flat-rate subscription, not the metered Agent SDK), and remote login + notifications run through **Telegram**; more can be added the same way.

## Features

- **Workbench** — multiple agent terminals per project, side by side, plus embeddable **Files / Git / Kanban / Mindmap** panels. Drag a file, git change, kanban card/subtask, or mindmap node onto a terminal to insert a reference (desktop + mobile).
- **Schedule** — on a one-time or recurring schedule, run an agent (with model / effort / mode / session + prompt), a CLI command, or start/stop a service — gated by conditions (5h/weekly usage %, open-terminal count, kanban status).
- **Kanban & Mindmap** — per-project task board and a global brainstorming mindmap that can link to tasks.
- **Files** — browser with gitignore-aware search, browse above the project root, upload/download/delete.
- **Git** — status, side-by-side diff, stage/unstage, commit, push/pull.
- **Services** — register and run background services (PM2-style) per project, with live status.
- **Docker** — when Docker is installed, the machine's whole engine: containers with live CPU and memory, logs, charts, a shell inside each and the raw inspect; Compose stacks (up, pull, restart, stop, down); images (pull, remove), volumes, networks, and disk use with clean-up.
- **Tools** — headless browser automation, screenshots/window capture, a secrets vault (PIN-gated), and an agent usage meter.
- **Skills** — upload, edit and remove Claude skills from Settings: for every project on the machine or for one. Takes a `.zip` / `.skill` archive or a single `SKILL.md`. Updates to Crundi replace only the skills it ships; yours are left alone.
- **MCP server** — exposes project-scoped tools (kanban, mindmap, schedule, services, browser, screenshots, secrets, skills, notifications) so agents can drive Crundi.
- **Chats** — a chat UI for Claude Code on your own subscription: plans, permission requests, attachments, queued messages, a suggested next reply, and media and live panels shown in the conversation.
- **Android app** — pairs with your server by scanning a picture; notifications you can answer, chats that float as bubbles, and a live card while Claude works. Source in [`android/`](android/); [privacy policy](https://crundi.faruhaan.com/privacy.html).
- **Desktop app** (Electron) + **mobile-friendly** web UI.

## Requirements

- **Node.js** 18+
- The agent's CLI on your PATH — currently **Claude Code** (`claude`), installed and authenticated
- A way to sign in — either a **Telegram bot token** from [@BotFather](https://t.me/BotFather), or a password with an authenticator code (set up on first run)
- *(Optional)* a Cloudflare account for a persistent tunnel domain — otherwise a free random quick tunnel is used

## Install

**Windows — everything on one machine:** download `Crundi.Setup.<version>.exe` from the [latest release](../../releases/latest) and run it. The installer is self-signed (`TheRealFaruhaan`).

**Windows — client only:** `Crundi.Client-Setup-<version>.exe` is the desktop app *without* a server. It attaches to one running elsewhere and lends it this machine's GUI, so the browser panel opens in front of you even though the server has no display. Tray → Server… to point it somewhere.

**Ubuntu desktop — everything on one machine:** download `Crundi-<version>-linux-x64.deb` from the [latest release](../../releases/latest) and install it with `sudo apt install ./Crundi-<version>-linux-x64.deb`. It carries its own Node, so the only thing to have installed first is Claude Code. It keeps its settings in `~/.config/crundi-desktop`, apart from a server installed with `install.sh`.

**Ubuntu desktop — client only:** `Crundi-Client-<version>-linux-x64.deb` is the same desktop app without a server, for one running elsewhere. Tray → Server… to point it somewhere.

Both are built for Ubuntu 22.04 and later on x64 (other Debian-family desktops should work), sit in the tray, and update themselves; installing an update asks for your password, as any package does.

**Linux server — recommended:** the native install, not Docker.

```bash
./scripts/install.sh
```

Installs to `~/.local/share/crundi`, keeps data in `~/.config/crundi`, and sets up a systemd **user** service with lingering so it survives logout without running as root. Re-run it to upgrade in place; `scripts/uninstall.sh` removes it and leaves your data unless you pass `--purge`.

The release tarball bundles its native modules, so it needs Node and no compiler. From a git checkout it compiles `node-pty`, and will offer to install `python3 make g++` for your distro if they are missing.

**Linux — Docker:** [`docker/`](docker/) has an image and compose file. It works, but prefer the native install: Claude works on *your* files with your git identity, SSH keys and shell, and a container turns each of those into a mounting problem. Services Crundi starts also die with the container. Use it if you already run everything in Docker or want the host left alone.

**From source:**

```bash
git clone https://github.com/TheRealFaruhaan/crundi.git
cd crundi
npm install
npm start                # desktop app (Electron)
# or
npm run start:headless   # server only (no Electron window)
```

## First-run setup

On first launch Crundi shows a step-by-step wizard:

1. **Create a bot** with @BotFather and paste the token (current login channel).
2. **Your Telegram username** — only this user can access Crundi.
3. **Projects** — point at a parent folder (each subfolder = a project), or add projects individually.
4. **Cloudflare tunnel** *(optional)* — a fixed address on your own domain (see [A fixed address with a Cloudflare tunnel](#a-fixed-address-with-a-cloudflare-tunnel)), or skip for a random quick-tunnel address that changes every start.
5. **Enable login** — set your bot's login domain in @BotFather (`/setdomain`) to your public URL so the login button works.

Config is stored in your OS app-data dir; you can also use a `.env` (see `.env.example`):

| Variable | Required | Description |
|---|---|---|
| `TELEGRAM_BOT_TOKEN` | yes | Bot token from @BotFather (login validation + notifications) |
| `ALLOWED_USERNAME` | yes | Your Telegram username, no `@` — the only allowed user |
| `PROJECTS_DIR` | * | Parent folder of projects (single-folder mode); leave empty for multi-folder mode |
| `WEB_PORT` | no | Web UI port (`8888` by default; `0` = any free port). A tunnel's Public Hostname must point at it |
| `CLOUDFLARE_TUNNEL_TOKEN` | no | Named-tunnel token for a persistent domain |
| `CLOUDFLARE_TUNNEL_URL` | no | The tunnel's public URL |
| `DATA_DIR` | no | Override the data directory |

## A fixed address with a Cloudflare tunnel

For `https://crundi.example.com` instead of a random quick-tunnel address. You need a domain whose DNS is on Cloudflare (the free plan is enough). **Do not install `cloudflared` yourself**: Crundi bundles and runs it, and a second copy fights it for the tunnel.

1. **Create the tunnel.** In the Cloudflare dashboard open **Zero Trust → Networks → Tunnels → Create a tunnel**, choose **Cloudflared** and name it.
2. **Copy the token, run nothing.** The "Install and run a connector" screen shows install commands. Do not run them. Copy only the token from one of them: the long string starting `eyJ`, after `--token`. Then **Next**.
3. **Point a hostname at Crundi.** On **Public Hostnames** (later: the tunnel → **Configure** → **Public Hostname**):

   | Field | Value |
   |---|---|
   | Subdomain / Domain | e.g. `crundi` / `example.com` |
   | Service type | **HTTP** |
   | URL | **`127.0.0.1:8888`**, Crundi's port (`WEB_PORT`) |

   Write `127.0.0.1`, not `localhost`: on Windows `localhost` can mean the IPv6 address, which Crundi does not listen on. Cloudflare adds the DNS record itself.
4. **Give Crundi the token and address**, in the setup wizard, in **Settings**, or in `.env` (`CLOUDFLARE_TUNNEL_TOKEN`, and `CLOUDFLARE_TUNNEL_URL=https://crundi.example.com`), then restart Crundi.

Crundi's **Info** page shows the tunnel's status and, under *Cloudflare service*, the exact `HTTP → 127.0.0.1:<port>` the hostname has to use.

| What you see | What it means |
|---|---|
| **Error 502, Bad gateway** | The tunnel works but the hostname points at the wrong place: check Service type HTTP and `127.0.0.1:<port>` against the Info page |
| **Error 1033** | No connector is running: Crundi is not running, or has no token |
| Info says *Connecting…*, the log says `named exited` | `cloudflared` is refusing to start. The lines after it, `[tunnel] cloudflared: …`, say why (most often an incomplete token) |

**Phones** pair through the tunnel: **Settings → Connect a phone** gives them the tunnel address, even when Crundi itself is open at `localhost`.

**Forwards on their own subdomain** (`myapp.crundi.example.com`) need a second Public Hostname, `*.crundi` on the same domain, pointing at the same `127.0.0.1:<port>`, and `FORWARD_DOMAIN=crundi.example.com`. Cloudflare's free certificate covers one level of subdomain only, so `*.example.com` works and `*.crundi.example.com` needs its paid add-on. Path forwards (`/tunnel/myapp/`) need none of this.

## Build

```bash
npm run build     # electron-builder --win → dist/
```

Code signing uses a `.pfx` supplied via the `CSC_LINK` / `CSC_KEY_PASSWORD` environment variables (not committed).

## Security model

Crundi is a **single-user remote-control tool**. Whoever authenticates gets, on the host machine:

- a full interactive shell (agent terminals + arbitrary commands),
- file read/write — and read/download can go **above** a project's root,
- start/stop of registered services,
- screen capture and basic GUI automation.

That power is the point — but treat it accordingly: **run it only for yourself**, keep your tunnel URL private, set the login domain, and never share your bot token. Anyone who reaches the URL and passes auth controls the machine.

## Contributing

Bug reports and questions are welcome — open an [issue](../../issues). External **code** contributions (pull requests) aren't being accepted right now, and `main` is protected; fork freely under MIT. For security issues see [SECURITY.md](SECURITY.md); details in [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE) © 2026 Hussain Faruhaan.
