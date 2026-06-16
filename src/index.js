/**
 * index.js — Crundi headless entry point.
 *
 * Starts the webapp (HTTP + WebSocket + tunnel), Claude terminal manager,
 * and the slim Telegram bot for notifications.
 *
 * Replaces the old bot-centric entry point.
 */

import { existsSync, readFileSync, writeFileSync, mkdirSync, copyFileSync, unlinkSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { config, getOldAppDataDir, isFreshInstall } from './config.js';
import { createBot } from './bot.js';
import { createWebApp } from './webapp.js';
import { createClaudeTerminals } from './claude-terminals.js';
import { killAllServices } from './services.js';

const __dirname = dirname(fileURLToPath(import.meta.url));

// ─── Server log capture ───
const serverLogs = [];
const MAX_SERVER_LOGS = 500;
const serverLogListeners = new Set();

function captureLog(level, args) {
  const text = args.map(a => typeof a === 'string' ? a : String(a)).join(' ');
  const entry = { time: Date.now(), level, text };
  serverLogs.push(entry);
  if (serverLogs.length > MAX_SERVER_LOGS) serverLogs.shift();
  for (const fn of serverLogListeners) { try { fn(entry); } catch { /* ignore */ } }
}

const origLog = console.log.bind(console);
const origWarn = console.warn.bind(console);
const origError = console.error.bind(console);
console.log = (...args) => { origLog(...args); captureLog('info', args); };
console.warn = (...args) => { origWarn(...args); captureLog('warn', args); };
console.error = (...args) => { origError(...args); captureLog('error', args); };

// ─── Data directory setup ───
if (!existsSync(config.dataDir)) {
  mkdirSync(config.dataDir, { recursive: true });
  console.log(`[crundi] Created data dir: ${config.dataDir}`);
}

// ─── First-launch migration from old app ───
if (isFreshInstall()) {
  const oldDir = getOldAppDataDir();
  if (oldDir) {
    console.log(`[crundi] Old app data found at ${oldDir} — importing...`);
    // Copy .env if we don't have one in the new location
    const oldEnv = join(oldDir, '.env');
    const newEnv = join(config.appDir, '.env');
    if (existsSync(oldEnv) && !existsSync(newEnv)) {
      try {
        mkdirSync(config.appDir, { recursive: true });
        copyFileSync(oldEnv, newEnv);
        console.log('[crundi] Imported .env from old app');
      } catch (err) {
        console.warn('[crundi] Failed to import .env:', err.message);
      }
    }
    // Import projects and services
    try {
      const { importFromOldData, importServicesFromOldData } = await import('./project-store.js');
      const pr = importFromOldData(oldDir);
      if (pr.ok) console.log(`[crundi] Imported ${pr.imported} projects from old app`);
      const sr = importServicesFromOldData(oldDir);
      if (sr.ok) console.log('[crundi] Imported services from old app');
    } catch (err) {
      console.warn('[crundi] Import error:', err.message);
    }
  }
}

// ─── State persistence ───
const stateFile = join(config.dataDir, '.crundi-state.json');

function loadState() {
  try {
    if (existsSync(stateFile)) return JSON.parse(readFileSync(stateFile, 'utf-8'));
  } catch { /* ignore */ }
  return {};
}

function saveState(state) {
  try { writeFileSync(stateFile, JSON.stringify(state)); } catch { /* ignore */ }
}

const savedState = loadState();

// ─── Telegram bot (slim — notifications only) ───
const { bot, notify, getChatId, setChatId, setWebappUrl, onChatId } = createBot();

// Restore cached chat ID
if (savedState.chatId) setChatId(savedState.chatId);

// Persist chat ID immediately when discovered via /start
onChatId((id) => {
  console.log(`[crundi] Chat ID discovered: ${id}`);
  saveState({ ...loadState(), chatId: id });
});

// ─── Claude terminal manager ───
// apiUrl and apiKey will be set after webapp starts
const claudeTerminals = createClaudeTerminals();

// ─── MCP dispatch handler (for browser, screenshots, terminals, RDP) ───
async function mcpDispatch(tool, args) {
  // Browser tools
  if (tool.startsWith('browser_') || tool === 'list_browsers') {
    const b = await import('./browser.js');
    switch (tool) {
      case 'browser_open': return b.openBrowser(args);
      case 'browser_close': return b.closeBrowser(args.key);
      case 'browser_navigate': return b.navigateBrowser(args.key, args.url);
      case 'browser_screenshot': {
        const r = await b.screenshotBrowser(args.key);
        return r; // data is already base64 from IPC
      }
      case 'browser_read_page': return b.readBrowserPage(args.key);
      case 'browser_view_source': return b.getBrowserSource(args.key);
      case 'browser_click': return b.clickBrowser(args.key, args.selector);
      case 'browser_type': return b.typeBrowser(args.key, args.text, args.selector);
      case 'browser_fill': return b.fillBrowser(args.key, args.selector, args.value);
      case 'browser_select': return b.selectBrowser(args.key, args.selector, args.value);
      case 'browser_eval': return b.evalBrowser(args.key, args.code);
      case 'browser_mouse': return b.mouseBrowser(args.key, args.x, args.y, args.action);
      case 'browser_resize': return b.resizeBrowser(args.key, args.width, args.height);
      case 'browser_scroll': return b.scrollBrowser(args.key, { x: args.x, y: args.y, selector: args.selector });
      case 'browser_wait': return b.waitBrowser(args.key, { selector: args.selector, timeout: args.timeout });
      case 'browser_go_back': return b.goBackBrowser(args.key);
      case 'browser_go_forward': return b.goForwardBrowser(args.key);
      case 'browser_console': return b.getBrowserConsole(args.key, { clear: args.clear, start: args.start, end: args.end, countOnly: args.countOnly });
      case 'browser_network': return b.browserNetwork(args.key, args.action, { start: args.start, end: args.end, countOnly: args.countOnly });
      case 'browser_cookies': return b.cookiesBrowser(args.key, { action: args.action, cookie: args.cookie, filter: args.filter, url: args.url });
      case 'browser_snapshot': return b.snapshotBrowser(args.key, { maxDepth: args.maxDepth, maxLength: args.maxLength });
      case 'browser_elements': return b.elementsBrowser(args.key, { start: args.start, end: args.end, countOnly: args.countOnly, selector: args.selector });
      case 'browser_pdf': return b.pdfBrowser(args.key, { landscape: args.landscape });
      case 'browser_list':
      case 'list_browsers':
        return { ok: true, browsers: b.listBrowsers(args.alias) };
      default:
        return { ok: false, error: `Unknown browser tool: ${tool}` };
    }
  }

  // Screenshot / display tools
  if (tool === 'list_windows' || tool === 'list_displays' ||
      tool === 'capture_window' || tool === 'capture_display' ||
      tool === 'send_window_screenshot_to_user' || tool === 'send_display_screenshot_to_user') {
    const sys = await import('./system.js');
    if (tool === 'list_windows') return { ok: true, ...sys.listWindows() };
    if (tool === 'list_displays') return { ok: true, displays: sys.listScreens() };
    if (tool === 'capture_window') {
      const buf = await sys.screenshotWindow(args.windowId);
      return { ok: true, data: buf.toString('base64') };
    }
    if (tool === 'capture_display') {
      const buf = await sys.screenshotDisplay(args.display || 1);
      return { ok: true, data: buf.toString('base64') };
    }
    // Compound: capture + send to user via Telegram
    if (tool === 'send_window_screenshot_to_user' || tool === 'send_display_screenshot_to_user') {
      const chatId = getChatId();
      if (!chatId) return { ok: false, error: 'No chat ID configured — send /start to the bot first' };
      const buf = tool === 'send_window_screenshot_to_user'
        ? await sys.screenshotWindow(args.windowId)
        : await sys.screenshotDisplay(args.display || 1);
      const tmpFile = join(tmpdir(), `crundi-screenshot-${Date.now()}.png`);
      writeFileSync(tmpFile, buf);
      try {
        const { InputFile } = await import('grammy');
        await bot.api.sendPhoto(chatId, new InputFile(tmpFile));
        return { ok: true };
      } finally {
        try { unlinkSync(tmpFile); } catch { /* ignore */ }
      }
    }
  }

  // User terminal tools (non-Claude PTY instances)
  if (tool === 'spawn_terminal' || tool === 'terminal_input' || tool === 'terminal_output' ||
      tool === 'terminal_wait' || tool === 'close_terminal' || tool === 'list_terminals') {
    const t = await import('./terminals.js');
    const alias = args.alias || 'default';
    switch (tool) {
      case 'spawn_terminal': return t.spawnTerminal(alias, alias, args.name, { command: args.command, cwd: args.cwd });
      case 'terminal_input': return t.writeTerminal(alias, args.name, args.input);
      case 'terminal_output': return t.getTerminalOutput(alias, args.name, { start: args.start, end: args.end, countOnly: args.countOnly });
      case 'terminal_wait': return t.waitTerminal(alias, args.name, { pattern: args.pattern, timeout: args.timeout });
      case 'close_terminal': return t.closeTerminal(alias, args.name);
      case 'list_terminals': return t.listTerminals(alias);
    }
  }

  // RDP tools
  if (tool === 'disconnect_rdp') {
    const rpa = await import('./rpa.js');
    return rpa.disconnectRdp();
  }

  return { ok: false, error: `Unknown tool: ${tool}` };
}

// ─── Web app ───
const webapp = createWebApp({
  config,
  claudeTerminals,
  bot: bot,
  mcpDispatch,
  serverLogs,
  onServerLog: (fn) => serverLogListeners.add(fn),
  getChatId,
  setChatId,
});

// ─── Start everything ───
console.log('[crundi] Starting...');

// Start webapp (HTTP + WebSocket + tunnel)
let port, tunnelUrl;
try {
  ({ port, tunnelUrl } = await webapp.start(config.webPort));
} catch (err) {
  console.error(`[crundi] Failed to start webapp: ${err.message}`);
  process.exit(1);
}

// Update Claude terminals with API info for MCP config
const apiKey = webapp.getInternalApiKey();
claudeTerminals.apiUrl = `http://localhost:${port}`;
claudeTerminals.apiKey = apiKey;
// Log API key for Electron to pick up (local auth for iframe)
console.log(`[crundi] API_KEY=${apiKey}`);

// Refresh .mcp.json in ALL project directories so stale keys are updated
try {
  const { listProjects } = await import('./project-store.js');
  const allProjects = listProjects();
  for (const p of allProjects) {
    if (p.path && existsSync(p.path)) {
      const mcpFile = join(p.path, '.mcp.json');
      const mcpStdioPath = join(__dirname, 'mcp-stdio.js');
      let existing = {};
      if (existsSync(mcpFile)) {
        try { existing = JSON.parse(readFileSync(mcpFile, 'utf-8')); } catch { /* ignore */ }
      }
      const merged = {
        ...existing,
        mcpServers: {
          ...(existing.mcpServers || {}),
          crundi: {
            command: 'node',
            args: [mcpStdioPath],
            env: { CRUNDI_API_URL: `http://localhost:${port}`, CRUNDI_API_KEY: apiKey, CRUNDI_PROJECT: p.alias },
          },
        },
      };
      writeFileSync(mcpFile, JSON.stringify(merged, null, 2));
      // Add .mcp.json to .gitignore if present
      const { ensureGitignore } = await import('./claude-terminals.js');
      ensureGitignore(p.path);
    }
  }
  console.log(`[crundi] Refreshed .mcp.json in ${allProjects.length} project(s)`);
} catch (err) {
  console.warn('[crundi] Failed to refresh .mcp.json files:', err.message);
}

// Tell the bot about the webapp URL
const webappUrl = tunnelUrl || `http://localhost:${port}`;
setWebappUrl(webappUrl);
config.botUsername = '';

// Mark as ready BEFORE bot init — webapp is usable without Telegram
console.log(`[crundi] Ready!`);
console.log(`[crundi] Web UI: ${webappUrl}`);
console.log(`[crundi] Local:  http://localhost:${port}`);

// Start Telegram bot in background — don't block the app
(async () => {
  try {
    await Promise.race([
      bot.init(),
      new Promise((_, reject) => setTimeout(() => reject(new Error('Telegram init timeout (15s)')), 15000)),
    ]);
    config.botUsername = bot.botInfo?.username || '';
    console.log(`[crundi] Bot: @${config.botUsername}`);
    webapp.refreshHtml(config.botUsername);

    bot.start({
      onStart: () => {
        console.log(`[crundi] Telegram bot connected. Notifications enabled for @${config.allowedUsername}`);
        if (getChatId() && getChatId() !== savedState.chatId) {
          saveState({ ...loadState(), chatId: getChatId() });
        }
      },
    });
  } catch (err) {
    console.warn(`[crundi] Telegram bot failed to start: ${err.message}`);
    console.warn('[crundi] Notifications will not be sent. Webapp still available.');
  }
})();

// ─── Shutdown ───
let shuttingDown = false;

async function shutdown(signal) {
  if (shuttingDown) return;
  shuttingDown = true;
  console.log(`\n${signal} — shutting down`);

  // Save state
  if (getChatId()) saveState({ ...loadState(), chatId: getChatId() });

  // Stop all running services + their Cloudflare tunnels (await so
  // `docker compose down` / taskkill actually finish before we exit).
  try { await killAllServices(); } catch { /* ignore */ }

  // Close Claude terminals (node-pty sessions)
  claudeTerminals.closeAll();

  // Stop webapp
  webapp.stop();

  // Stop bot
  try { await bot.stop(); } catch { /* ignore */ }

  process.exit(0);
}

process.once('SIGINT', () => shutdown('SIGINT'));
process.once('SIGTERM', () => shutdown('SIGTERM'));

// PM2 graceful shutdown
process.on('message', (msg) => {
  if (msg === 'shutdown') shutdown('PM2 shutdown');
});

process.once('uncaughtException', (err) => {
  console.error('[crundi] Uncaught exception:', err);
  shutdown('uncaughtException');
});

process.once('unhandledRejection', (reason) => {
  console.error('[crundi] Unhandled rejection:', reason);
  // Don't crash on rejections — log and continue
});
