# telegram-claude-bot

A Telegram bot for remote development. Chat with Claude Code on your local projects from your phone — while your computer runs at home. Organized around a Telegram supergroup with topics: one topic per project.

## Prerequisites

- **Node.js** v18+
- **Claude Code CLI** installed and authenticated (`claude` in PATH)
- A Telegram bot token from [@BotFather](https://t.me/BotFather)
- A Telegram **supergroup with Topics enabled** (not a channel)
- **PM2** for background service (installed globally via `npm install -g pm2`)

## Install

```bash
git clone <repo-url>
cd telegram-claude-bot
npm install
```

## Configure

```bash
cp .env.example .env
```

| Variable | Required | Description |
|---|---|---|
| `TELEGRAM_BOT_TOKEN` | Yes | Token from @BotFather |
| `ALLOWED_USERNAME` | Yes | Your Telegram username (no `@`) |
| `FORUM_CHAT_ID` | Yes | ID of your supergroup (negative number, e.g. `-1001234567890`) |
| `PROJECTS_DIR` | Yes | Root folder containing your projects, e.g. `C:\Projects` |
| `CLAUDE_MODEL` | No | Default model: `sonnet`, `opus`, or `haiku` (default: `sonnet`) |
| `DEFAULT_MODE` | No | Default permission mode (default: `acceptEdits`) |
| `CLAUDE_TIMEOUT` | No | Seconds before prompting to continue or kill (default: `300`) |
| `PERMISSION_SERVER_PORT` | No | Port for the internal permission hook server (default: `3001`) |
| `DATA_DIR` | No | Where to store topic state (default: `./data`) |
| `CLAUDE_PROJECTS_DIR` | No | Path to Claude Code session storage (default: `~/.claude/projects`) |

### Getting your supergroup ID

Add the bot to your supergroup as admin, send any message in the group, then run:

```bash
curl "https://api.telegram.org/bot<YOUR_TOKEN>/getUpdates"
```

Look for `"chat":{"id":...}` in the response. The ID is a negative number.

## Run

### Direct (foreground)

```bash
npm start
```

### Background service with PM2

First-time setup — starts the bot under PM2 and registers a Windows Task Scheduler task so it auto-starts on login:

```bash
npm run service:install
```

If the bot was already running manually (`npm start`), stop it first to avoid a 409 conflict with Telegram's getUpdates. Running two instances simultaneously causes a crash loop.

| Command | Description |
|---|---|
| `npm run service:start` | Start the background service |
| `npm run service:stop` | Stop the service |
| `npm run service:restart` | Restart the service |
| `npm run service:logs` | Tail live logs |
| `npm run service:status` | Show health and uptime |
| `npm run service:uninstall` | Remove from PM2 and startup |

Logs are written to `logs/out.log` and `logs/error.log`.

## Usage

### Forum group setup

1. Create a Telegram supergroup, enable **Topics** in group settings
2. Add the bot as admin (needs permission to manage topics and post messages)
3. Set `FORUM_CHAT_ID` in `.env`
4. Run `/sync` in the **General** topic to auto-create a topic for every project in `PROJECTS_DIR`

The **General topic** (topic ID 1) is the management area — create projects, run `/sync`, `/projects`.  
All other topics are project workspaces — messages there go straight to Claude.

### Project management (General topic only)

| Command | Description |
|---|---|
| `/project <name>` | Create a new project folder and its topic |
| `/projects` | List all discovered projects |
| `/sync` | Create topics for all projects that don't have one yet |

### Working in a project topic

Send any message in a project topic and Claude responds. The topic auto-loads the latest session for that project. Each topic independently tracks its model, mode, and session.

| Command | Description |
|---|---|
| `/sessions` | List Claude Code sessions for this topic's project |
| `/sessions <search>` | Filter sessions by keyword |
| `/resume <n>` | Resume a session by list number or tap its inline button |
| `/resume <session-id>` | Resume by full or partial session ID |
| `/model` | Switch AI model (per topic) |
| `/mode` | Set permission mode (per topic) |
| `/current` | Show topic state: project, model, mode, session |

### Permission modes

| Mode | Shell commands | File edits | Everything else |
|---|---|---|---|
| `plan` | Telegram prompt | Telegram prompt | Auto-allow |
| `acceptEdits` | Telegram prompt | Auto-allow | Auto-allow |
| `auto` / `bypassPermissions` | Auto-allow | Auto-allow | Auto-allow |

When Claude wants to run a shell command in `acceptEdits` or `plan` mode, a Telegram message appears with **Allow** and **Deny** buttons. The process waits indefinitely until you respond.

### Timeout

If Claude is still running after `CLAUDE_TIMEOUT` seconds, you'll receive a prompt:
- **Continue waiting** — resets the timer for another cycle
- **Kill it** — terminates the process immediately

Claude is never killed automatically.

### Screenshots

Take screenshots of your machine remotely.

| Command | Description |
|---|---|
| `/screens` | List all connected displays with resolution and tap-to-screenshot buttons |
| `/screenshot` | Screenshot the primary display immediately |
| `/screenshot display 2` | Screenshot a specific display by number |
| `/windows` | List all open windows with tap-to-screenshot buttons |

Window screenshots use the `PrintWindow` Win32 API — works even when a window is behind other windows.

## Architecture

```
src/
  index.js          Entry point — wires bot, cleanup handlers, PM2 signal handling
  bot.js            All Telegram commands, message routing, inline buttons
  claude.js         Claude Agent SDK wrapper — sends messages, handles permissions
  config.js         Loads env, scans PROJECTS_DIR for project aliases
  sessions.js       Scans ~/.claude/projects for existing CLI sessions
  topics.js         Persists per-topic state (model, mode, sessionId, project)
  formatter.js      Converts Claude markdown to Telegram HTML
  system.js         Window listing and screenshot capture (Windows)
  settings-hook.js  Strips any leftover legacy PreToolUse hook from ~/.claude/settings.json
```

Permissions are decided entirely by the SDK's `canUseTool` callback (see below) —
no global Claude settings are modified.

Claude is invoked via the `@anthropic-ai/claude-agent-sdk`:

```js
query({ prompt, options: { cwd, resume, model, permissionMode, canUseTool } })
```

`canUseTool` is the hook point — the SDK calls it before every tool and waits for `allow` or `deny`. In `acceptEdits` mode, bash tool calls are forwarded to Telegram for approval.

## Shutdown and cleanup

On exit (Ctrl+C, SIGTERM, crash), the bot:
1. Aborts all in-flight Claude SDK conversations
2. Stops the Telegram polling loop
3. Restores `~/.claude/settings.json` to its pre-bot state (synchronous, runs even on crash)

PM2 sends SIGTERM on `pm2 stop`, which triggers the same cleanup path.

## Security

- Only `ALLOWED_USERNAME` can interact with the bot. All others are silently rejected.
- The permission hook only fires when `CLAUDE_BOT_SESSION=1` is set (i.e. only for bot-spawned processes, not your normal CLI usage).
- Do not commit `.env` — it is in `.gitignore`.
- Regenerate your bot token if it was ever shared or exposed.
