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
- **Tools** — headless browser automation, screenshots/window capture, a secrets vault (PIN-gated), and an agent usage meter.
- **MCP server** — exposes project-scoped tools (kanban, mindmap, schedule, services, terminals, browser, screenshots, secrets, notifications) so agents can drive Crundi.
- **Desktop app** (Electron) + **mobile-friendly** web UI.

## Requirements

- **Node.js** 18+
- The agent's CLI on your PATH — currently **Claude Code** (`claude`), installed and authenticated
- A login/notification channel — currently a **Telegram bot token** from [@BotFather](https://t.me/BotFather)
- *(Optional)* a Cloudflare account for a persistent tunnel domain — otherwise a free random quick tunnel is used

## Install

**Windows (recommended):** download `Crundi.Setup.<version>.exe` from the [latest release](../../releases/latest) and run it. The installer is self-signed (`TheRealFaruhaan`).

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
4. **Cloudflare tunnel** *(optional)* — paste a named-tunnel token + URL for a fixed domain, or skip for a random quick tunnel. **Do not** install Cloudflare's connector; Crundi bundles and runs `cloudflared` itself.
5. **Enable login** — set your bot's login domain in @BotFather (`/setdomain`) to your public URL so the login button works.

Config is stored in your OS app-data dir; you can also use a `.env` (see `.env.example`):

| Variable | Required | Description |
|---|---|---|
| `TELEGRAM_BOT_TOKEN` | yes | Bot token from @BotFather (login validation + notifications) |
| `ALLOWED_USERNAME` | yes | Your Telegram username, no `@` — the only allowed user |
| `PROJECTS_DIR` | * | Parent folder of projects (single-folder mode); leave empty for multi-folder mode |
| `WEB_PORT` | no | Web UI port (`0` = auto-assign) |
| `CLOUDFLARE_TUNNEL_TOKEN` | no | Named-tunnel token for a persistent domain |
| `CLOUDFLARE_TUNNEL_URL` | no | The tunnel's public URL |
| `DATA_DIR` | no | Override the data directory |

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
