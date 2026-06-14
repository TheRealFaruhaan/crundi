# Crundi — Architecture Pivot Plan

**Branch**: `feature/terminal-claude`
**Goal**: Replace Agent SDK (metered) with interactive Claude Code CLI terminals (flat-rate).
**App Name**: Crundi
**Separate App**: Crundi installs to its own path and uses its own config/data directory,
completely independent from the old "Claude Telegram Bot" app. On first launch,
offers to import projects and services from the old app if detected.

Data directories:
- Windows: `%APPDATA%/Crundi/` (old: `%APPDATA%/Claude Telegram Bot/`)
- macOS: `~/Library/Application Support/Crundi/`
- Linux: `~/.config/crundi/`

## Why
Anthropic moved Agent SDK / headless `claude -p` to per-token API billing.
Interactive Claude Code CLI stays on the flat-rate subscription plan.
Crundi spawns real Claude Code terminals and exposes them via web + desktop.

## Architecture

```
User (Web / Mobile / Desktop)
      |  Telegram Login Widget
      v
[Crundi Web App]  <-- Cloudflare Tunnel --> [Crundi Server (Node.js)]
      |  WebSocket                                |
      v                                           v
[xterm.js]  <=========>  [Claude CLI PTY]  (node-pty, per-project)
                                |
                          [MCP Server]  (stdio, per-session)
                                |
                    [Services, Browser, Screenshots, Files]
                                |
                    [Telegram Bot]  (notifications only)
```

## What's Kept
- `src/topics.js` — project registry (rename concept to "projects" internally)
- `src/projects.js` — manual project persistence
- `src/config.js` — env configuration
- `src/services.js` + `src/service-registry.js` — service management
- `src/tunnel.js` — Cloudflare tunnels
- `src/terminals.js` — user terminals (non-Claude)
- `src/browser.js` — headless browser tools
- `src/system.js` — screenshots, window listing
- `src/rpa.js` — RDP management
- ~~`src/sessions.js`~~ — DROPPED: users resume sessions themselves in the terminal
- `app/main.js` — Electron main (updated)
- `app/index.html` — Electron UI (updated)
- MCP tools: send_message_to_user, send_photo_to_user, send_file_to_user,
  services, browser, terminals, screenshots, tunnels

## What's Dropped
- `@anthropic-ai/claude-agent-sdk` — the whole point
- `src/supervisor.js` — supervisor/worker system
- `src/worker-bridge.js` — worker dispatch
- `src/pending-questions.js` — ask_user MCP tool (user types in terminal now)
- `src/sessions.js` — users resume sessions themselves in the CLI
- `src/topics.js` — no more Telegram forum topics; projects are just { alias, path }
- All `dispatchToClaude` / `sendMessage` logic in bot.js
- Permission prompts, reaction tracking, message buffering
- Telegram message → Claude flow
- Forum group / channel / topics — DM only from authorized user
- `FORUM_CHAT_ID` env var — replaced by direct chat to authorized user

## What's New / Rewritten
- `src/webapp.js` — always-on HTTP + WebSocket server (replaces dashboard.js)
- `src/webapp-html.js` — web terminal UI (xterm.js, mobile-friendly, Telegram Login)
- `src/claude-terminals.js` — Claude Code CLI PTY manager (per-project)
- `src/mcp-stdio.js` — standalone MCP server (stdio transport for Claude Code)
- `src/bot.js` — stripped to: Telegram Login validation + notifications

## Phases

### Phase 1: Foundation + Rename
- [x] Create branch `feature/terminal-claude`
- [x] Rename project: package.json (name→crundi), Electron builder config,
      window titles, HTML branding, pm2 config, startup scripts
- [x] Create `src/webapp.js`: always-on HTTP server, WebSocket (ws library),
      Telegram Login Widget auth, JWT sessions, API routes (projects, services),
      vendor file serving, import API for old app data
- [x] Create `src/webapp-html.js`: xterm.js terminal UI, project sidebar,
      mobile responsive, Telegram Login button, dark theme
- [x] Create `src/project-store.js`: project registry with auto-discovery +
      registered projects, import from old app format
- [x] Update `src/config.js`: platform-specific data dir (%APPDATA%/Crundi),
      separate from old app, old app detection, fresh install detection
- [x] Update `.env.example` for Crundi (drop old fields)
- [x] Separate install/config paths from old "Claude Telegram Bot" app

### Phase 2: Claude Terminal Manager
- [x] Create `src/claude-terminals.js`: spawn `claude` CLI via node-pty per project,
      manage lifecycle (create/close/resize), output buffering
- [x] WebSocket protocol: terminal I/O streaming, resize events, project switching
- [x] Prompt completion detection: watch PTY output for Claude's idle prompt marker
- [x] Telegram notification on completion (via slim bot)

### Phase 3: MCP Tools Migration
- [x] Create `src/mcp-stdio.js`: standalone MCP server using @modelcontextprotocol/sdk
- [x] Local HTTP bridge: MCP server → webapp `/api/mcp/call` → dispatches to modules
- [x] Auto-configure: write `.mcp.json` in project dir when Claude terminal is created
- [x] Migrated tools: send_message/photo/file, services, tunnels, syntax_check

### Phase 4: Slim Telegram Bot
- [x] Rewrote `src/bot.js`: ~100 lines, /start command + notify() + DM only
- [x] Rewrote `src/index.js`: new Crundi entry point (webapp + bot + terminals)
- [x] Telegram Login validation in webapp (unchanged from Phase 1)
- [x] Claude completion → Telegram DM notification flow

### Phase 5: Desktop App
- [x] Rewrote `app/index.html`: minimal loading screen + iframe to webapp URL
- [x] Rewrote `app/main.js`: spawns src/index.js, navigates to webapp when ready
- [x] Rewrote `app/preload.js`: minimal IPC (bot controls, window controls, setup)
- [x] Setup wizard for first-time .env configuration

### Phase 6: Cleanup + Release
- [x] Deleted: supervisor.js, worker-bridge.js, pending-questions.js, dashboard.js,
      dashboard-html.js, claude.js, formatter.js, git-commands.js, topics.js,
      projects.js, sessions.js, settings-hook.js, mcp-server.js, markdown/
- [x] Removed `@anthropic-ai/claude-agent-sdk` + CodeMirror + markdown-it from deps
- [x] Updated .env.example
- [ ] Build + test

## Auth Flow (Telegram Login Widget)
1. Web app shows "Log in with Telegram" button (official widget)
2. User authenticates via Telegram
3. Server validates HMAC-SHA256: hash = HMAC(SHA256(bot_token), sorted_fields)
4. Server issues JWT session token (stored in httpOnly cookie or localStorage)
5. WebSocket connections include the JWT for auth
6. Bot token is needed for validation but bot doesn't handle messages

## Prompt Completion Detection
Claude Code CLI shows a prompt character (`❯` or `>`) when idle.
Watch PTY output for the prompt pattern after a period of no output (~500ms).
When detected → send Telegram notification: "Claude finished in <project>".

## File Size / Download
Same as current: ≤50MB files via sendDocument, >50MB via /dl/:token endpoint
on the web app (replaces dashboard file sharing).

## Tunnel Strategy
- Quick tunnel (trycloudflare.com) for dev/fallback
- Named tunnel with registered domain for production
- Both use existing `src/tunnel.js` infrastructure
- Web app tunnel is always-on (not 15-min idle shutdown)
