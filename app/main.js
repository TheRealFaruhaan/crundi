/**
 * Electron main process for Crundi.
 *
 * Simplified terminal-first app:
 *   1. Spawns src/index.js (webapp + bot + terminals)
 *   2. Opens a BrowserWindow that loads the webapp URL
 *   3. Tray icon with start/stop/quit
 *   4. Setup wizard for first-time .env configuration
 */

import { app, BrowserWindow, Tray, Menu, ipcMain, nativeImage, dialog, clipboard } from 'electron';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';
import { dirname, join } from 'path';
import { spawn, execFileSync } from 'child_process';
import { existsSync, readFileSync, writeFileSync, appendFileSync, mkdirSync, copyFileSync, readdirSync, statSync } from 'fs';
import { tmpdir, homedir } from 'os';

const __dirname = dirname(fileURLToPath(import.meta.url));

const appRoot = app.isPackaged
  ? join(dirname(app.getPath('exe')), 'resources', 'app.asar.unpacked')
  : join(__dirname, '..');
const srcDir = join(appRoot, 'src');
const assetsDir = app.isPackaged
  ? join(dirname(app.getPath('exe')), 'resources', 'assets')
  : join(__dirname, '..', 'assets');

// Crundi data directory — separate from old "Claude Telegram Bot" app
const userDataDir = app.isPackaged ? app.getPath('userData') : join(__dirname, '..');
const envPath = join(userDataDir, '.env');
const dataDir = app.isPackaged ? join(userDataDir, 'data') : join(__dirname, '..', 'data');

// ─── State ───
let mainWindow = null;
let tray = null;
let botProcess = null;
let botStatus = 'stopped';
let botLogs = [];
let userStopped = false;
let webappPort = null;
let webappApiKey = null;
const MAX_LOGS = 1000;

// ─── Persistent log file ───
const logFilePath = join(userDataDir, 'crundi.log');
const LOG_FILE_MAX = 1000;
let logFileLines = 0;

function writeLogLine(text) {
  const ts = new Date().toISOString();
  const line = `[${ts}] ${text}\n`;
  try {
    appendFileSync(logFilePath, line, 'utf8');
    logFileLines++;
    if (logFileLines >= LOG_FILE_MAX * 2) {
      try {
        const all = readFileSync(logFilePath, 'utf8').split('\n');
        const trimmed = all.slice(-LOG_FILE_MAX).join('\n') + '\n';
        writeFileSync(logFilePath, trimmed, 'utf8');
        logFileLines = LOG_FILE_MAX;
      } catch { /* ignore trim errors */ }
    }
  } catch { /* ignore write errors */ }
}

// ─── Early boot log ───
appendLog('═══════════════════════════════════════');
appendLog('[lifecycle] Crundi starting');
appendLog(`[lifecycle] packaged=${app.isPackaged}`);
appendLog(`[lifecycle] appRoot=${appRoot}`);
appendLog(`[lifecycle] srcDir=${srcDir}`);
appendLog(`[lifecycle] userDataDir=${userDataDir}`);
appendLog(`[lifecycle] envPath=${envPath}`);
appendLog(`[lifecycle] dataDir=${dataDir}`);
appendLog(`[lifecycle] logFile=${logFilePath}`);
appendLog(`[lifecycle] electron=${process.versions.electron} node=${process.versions.node} platform=${process.platform} arch=${process.arch}`);

// One-shot migration from old app
function migrateLegacyData() {
  if (!app.isPackaged) return;
  const legacyDataDir = join(appRoot, 'data');
  if (!existsSync(legacyDataDir)) return;
  appendLog('[lifecycle] Migrating legacy data...');
  try { mkdirSync(dataDir, { recursive: true }); } catch { /* ignore */ }
  try {
    for (const entry of readdirSync(legacyDataDir)) {
      const src = join(legacyDataDir, entry);
      const dst = join(dataDir, entry);
      if (existsSync(dst)) continue;
      try { if (statSync(src).isFile()) copyFileSync(src, dst); } catch { /* skip */ }
    }
    appendLog('[lifecycle] Legacy migration complete');
  } catch { /* ignore */ }
}
migrateLegacyData();

// ─── Window ───

function showWindow() {
  if (mainWindow) {
    if (mainWindow.isMinimized()) mainWindow.restore();
    mainWindow.show();
    mainWindow.focus();
  } else {
    createWindow();
  }
}

function createWindow() {
  if (mainWindow && !mainWindow.isDestroyed()) {
    showWindow();
    return;
  }

  appendLog('[lifecycle] Creating main window...');
  mainWindow = new BrowserWindow({
    width: 1100,
    height: 750,
    minWidth: 800,
    minHeight: 500,
    title: 'Crundi',
    icon: join(assetsDir, 'icon.ico'),
    frame: false,
    titleBarStyle: 'hidden',
    backgroundColor: '#0a0a0f',
    webPreferences: {
      preload: join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
    show: false,
  });

  const htmlPath = join(__dirname, 'index.html');
  appendLog(`[lifecycle] Loading ${htmlPath}`);
  mainWindow.loadFile(htmlPath);

  // Handle file downloads (triggered from webapp iframe)
  mainWindow.webContents.session.on('will-download', (event, item) => {
    // Show save dialog instead of silently blocking
    const fname = item.getFilename();
    dialog.showSaveDialog(mainWindow, {
      defaultPath: fname,
    }).then(({ canceled, filePath }) => {
      if (canceled || !filePath) {
        item.cancel();
      } else {
        item.setSavePath(filePath);
      }
    });
  });

  mainWindow.once('ready-to-show', () => {
    appendLog('[lifecycle] Window ready-to-show');
    mainWindow.show();
    // Dev tools: Ctrl+Shift+I still works; don't auto-open
  });

  mainWindow.on('close', (e) => {
    if (!app.isQuitting) {
      e.preventDefault();
      mainWindow.hide();
    }
  });
}

// ─── Tray ───

function createTray() {
  appendLog('[lifecycle] Creating tray...');
  const iconPath = join(assetsDir, 'icon.ico');
  const icon = existsSync(iconPath)
    ? nativeImage.createFromPath(iconPath).resize({ width: 16, height: 16 })
    : nativeImage.createEmpty();

  tray = new Tray(icon);
  tray.setToolTip('Crundi');
  updateTrayMenu();
  tray.on('double-click', () => showWindow());
}

function updateTrayMenu() {
  if (!tray) return;

  const statusLabels = {
    stopped: 'Stopped',
    starting: 'Starting...',
    running: 'Running',
    error: 'Error',
  };

  const menu = Menu.buildFromTemplate([
    { label: 'Crundi', enabled: false },
    { type: 'separator' },
    { label: statusLabels[botStatus] || 'Unknown', enabled: false },
    { type: 'separator' },
    { label: 'Open', click: () => showWindow() },
    { type: 'separator' },
    {
      label: 'Start',
      enabled: botStatus === 'stopped' || botStatus === 'error',
      click: () => startBot(),
    },
    {
      label: 'Stop',
      enabled: botStatus === 'running' || botStatus === 'starting',
      click: () => stopBot(),
    },
    { type: 'separator' },
    {
      label: 'Quit',
      click: () => gracefulShutdownAndExit(),
    },
  ]);

  tray.setContextMenu(menu);
  tray.setToolTip(`Crundi — ${statusLabels[botStatus]}`);
}

// ─── Logging ───

function appendLog(line) {
  const entry = { time: Date.now(), text: line };
  botLogs.push(entry);
  if (botLogs.length > MAX_LOGS) botLogs.shift();
  writeLogLine(line);
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('bot:log', entry);
  }
}

function setBotStatus(status) {
  botStatus = status;
  updateTrayMenu();
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('bot:status', status);
  }
}

// ─── Bot Process ───

function getOldAppDataDir() {
  const home = homedir();
  let dir;
  if (process.platform === 'win32') dir = join(process.env.APPDATA || join(home, 'AppData', 'Roaming'), 'Claude Telegram Bot');
  else if (process.platform === 'darwin') dir = join(home, 'Library', 'Application Support', 'Claude Telegram Bot');
  else dir = join(process.env.XDG_CONFIG_HOME || join(home, '.config'), 'claude-telegram-bot');
  if (existsSync(join(dir, '.env')) || existsSync(join(dir, 'data'))) return dir;
  return null;
}

function startBot() {
  if (botProcess) return;

  if (!existsSync(envPath)) {
    // Check for old "Claude Telegram Bot" config to pre-fill setup wizard
    const oldDir = getOldAppDataDir();
    let oldConfig = null;
    if (oldDir) {
      const oldEnv = join(oldDir, '.env');
      if (existsSync(oldEnv)) {
        appendLog(`[lifecycle] Found old app config at ${oldDir}`);
        try {
          const content = readFileSync(oldEnv, 'utf8');
          oldConfig = {};
          for (const line of content.split('\n')) {
            const m = line.match(/^([A-Z_]+)=(.*)$/);
            if (m) oldConfig[m[1]] = m[2].trim();
          }
        } catch (err) {
          appendLog(`[lifecycle] Failed to read old .env: ${err.message}`);
        }
      }
    }
    appendLog('No .env found — opening setup wizard...');
    showSetupWizard(oldConfig);
    return;
  }

  userStopped = false;
  setBotStatus('starting');
  appendLog('Starting Crundi...');

  const nodeExe = app.isPackaged ? 'node' : process.execPath;
  const entryScript = join(srcDir, 'index.js');
  appendLog(`[electron] node: ${nodeExe}`);
  appendLog(`[electron] script: ${entryScript}`);
  appendLog(`[electron] exists: ${existsSync(entryScript)}`);
  appendLog(`[electron] cwd: ${appRoot}`);
  appendLog(`[electron] env: ${envPath}`);

  const spawnEnv = {
    ...process.env,
    ELECTRON_RUN: '1',
    DOTENV_PATH: envPath,
    DATA_DIR: dataDir,
  };
  // Dev mode (npm run dev → electron . --dev): use a separate port and skip
  // Cloudflare so it never collides with a production instance on 8888.
  const isDev = !app.isPackaged && process.argv.includes('--dev');
  if (isDev) { spawnEnv.CRUNDI_DEV = '1'; appendLog('[electron] DEV mode: port 8889, Cloudflare disabled'); }
  delete spawnEnv.NODE_OPTIONS;

  try {
    botProcess = spawn(nodeExe, ['--no-deprecation', entryScript], {
      cwd: appRoot,
      env: spawnEnv,
      stdio: ['pipe', 'pipe', 'pipe', 'ipc'],
      windowsHide: true,
      shell: false,
    });
    appendLog(`[electron] Process spawned, pid=${botProcess.pid}`);
  } catch (err) {
    appendLog(`[electron] Spawn failed: ${err.message}`);
    setBotStatus('error');
    return;
  }

  botProcess.stdout?.on('data', (data) => {
    const lines = String(data).split('\n').filter(Boolean);
    for (const line of lines) {
      appendLog(line);
      const portMatch = line.match(/HTTP server on 0\.0\.0\.0:(\d+)/);
      if (portMatch) {
        webappPort = parseInt(portMatch[1], 10);
      }
      const keyMatch = line.match(/\[crundi\] API_KEY=([a-f0-9]+)/);
      if (keyMatch) {
        webappApiKey = keyMatch[1];
      }
      if (line.includes('[crundi] Ready')) {
        setBotStatus('running');
        if (webappPort && mainWindow && !mainWindow.isDestroyed()) {
          const authParam = webappApiKey ? `?key=${webappApiKey}` : '';
          const url = `http://localhost:${webappPort}${authParam}`;
          appendLog(`[lifecycle] Navigating to ${url}`);
          mainWindow.loadURL(url);
        }
      }
    }
  });

  botProcess.stderr?.on('data', (data) => {
    String(data).split('\n').filter(Boolean).forEach(line => appendLog(`[err] ${line}`));
  });

  botProcess.on('message', handleBotMessage);

  botProcess.on('exit', (code, signal) => {
    appendLog(`[electron] Process exited: code=${code} signal=${signal}`);
    botProcess = null;
    webappPort = null;
    if (app.isQuitting) return;
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.loadFile(join(__dirname, 'index.html'));
    }
    if (userStopped) {
      userStopped = false;
      setBotStatus('stopped');
      return;
    }
    if (code === 0) {
      setBotStatus('stopped');
    } else {
      setBotStatus('error');
      appendLog('Process crashed — check logs above. Click Start to retry.');
    }
  });

  botProcess.on('error', (err) => {
    appendLog(`[electron] Spawn error: ${err.message}`);
    botProcess = null;
    setBotStatus('error');
  });
}

// Force-kill a process AND its whole subtree so spawned services / cloudflared
// tunnels / PTYs can't orphan (Windows doesn't kill children with the parent).
function killTree(pid) {
  if (!pid) return;
  try {
    if (process.platform === 'win32') {
      execFileSync('taskkill', ['/PID', String(pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
    } else {
      try { process.kill(-pid, 'SIGKILL'); } catch { process.kill(pid, 'SIGKILL'); }
    }
  } catch { /* already gone */ }
}

// Gracefully stop the server process (it stops services/tunnels/terminals via
// the IPC 'shutdown' message — reliable on Windows where SIGTERM isn't catchable),
// then hard-kill the tree as a guaranteed fallback.
async function stopServerProcess() {
  // Local (main-process) children first.
  for (const [, entry] of userTerminals) { try { entry.proc.kill(); } catch { /* ignore */ } }
  userTerminals.clear();
  for (const [, entry] of browserWindows) { try { entry.win.destroy(); } catch { /* ignore */ } }
  browserWindows.clear();
  if (!botProcess) return;
  const proc = botProcess, pid = botProcess.pid;
  await new Promise((resolve) => {
    let done = false;
    const finish = () => { if (!done) { done = true; resolve(); } };
    proc.once('exit', finish);
    try { proc.send('shutdown'); } catch { finish(); }
    setTimeout(finish, 6000); // never hang on a wedged process
  });
  killTree(pid); // nuke any survivors (services, cloudflared) so nothing is left running
  if (botProcess === proc) botProcess = null;
}

function stopBot() {
  if (!botProcess) { setBotStatus('stopped'); return; }
  userStopped = true;
  appendLog('Stopping...');
  stopServerProcess();
}

let quitting = false;
async function gracefulShutdownAndExit() {
  if (quitting) return;
  quitting = true;
  app.isQuitting = true;
  appendLog('[lifecycle] Quitting — stopping services, terminals, and tunnels...');
  try { await stopServerProcess(); } catch { /* ignore */ }
  app.exit(0);
}

// ─── Setup Wizard ───

function showSetupWizard(oldConfig) {
  if (mainWindow) {
    mainWindow.webContents.send('show:setup', oldConfig || null);
  } else {
    createWindow();
    mainWindow.webContents.once('did-finish-load', () => {
      mainWindow.webContents.send('show:setup', oldConfig || null);
    });
  }
}

// ─── IPC Handlers ───

ipcMain.handle('setup:save', (_e, config) => {
  const lines = [
    `# ─── Crundi Configuration ───`,
    `TELEGRAM_BOT_TOKEN=${config.botToken || ''}`,
    `ALLOWED_USERNAME=${config.username || ''}`,
    `PROJECTS_DIR=${config.projectsDir || ''}`,
    `WEB_PORT=${config.webPort || (config.tunnelToken ? '3000' : '0')}`,
    `CLOUDFLARE_TUNNEL_TOKEN=${config.tunnelToken || ''}`,
    `CLOUDFLARE_TUNNEL_URL=${config.tunnelUrl || ''}`,
    `DATA_DIR=`,
  ];
  try { mkdirSync(dirname(envPath), { recursive: true }); } catch { /* ignore */ }
  writeFileSync(envPath, lines.join('\n'), 'utf8');
  appendLog('.env saved — starting Crundi...');
  startBot();
  return true;
});

ipcMain.handle('setup:check', () => existsSync(envPath));
ipcMain.handle('setup:browse', async () => {
  const result = await dialog.showOpenDialog(mainWindow, {
    properties: ['openDirectory'],
    title: 'Select Projects Directory',
  });
  return result.canceled ? null : result.filePaths[0];
});

// DisconnectRDP scheduled task (locked-screen capture). rpa.js lives in src/
// (asar-unpacked when packaged); import it by file URL so it resolves either way.
async function loadRpa() {
  const { pathToFileURL } = await import('url');
  return import(pathToFileURL(join(srcDir, 'rpa.js')).href);
}
ipcMain.handle('rdp:status', async () => {
  if (process.platform !== 'win32') return { ok: true, supported: false, installed: false, isRdp: false };
  try {
    const rpa = await loadRpa();
    return { ok: true, supported: true, installed: rpa.isTaskInstalled(), isRdp: rpa.isRdpSession() };
  } catch (err) {
    return { ok: false, supported: true, installed: false, error: err.message };
  }
});
ipcMain.handle('rdp:setup', async () => {
  if (process.platform !== 'win32') return { ok: false, error: 'Windows only' };
  try {
    const rpa = await loadRpa();
    return rpa.createTask();
  } catch (err) {
    return { ok: false, error: err.message };
  }
});

ipcMain.handle('bot:getStatus', () => botStatus);
ipcMain.handle('bot:getLogs', () => botLogs);
ipcMain.handle('bot:getLogPath', () => logFilePath);
ipcMain.on('preload:log', (_e, msg) => appendLog(msg));
ipcMain.handle('bot:start', () => startBot());
ipcMain.handle('bot:stop', () => stopBot());
ipcMain.handle('bot:restart', () => { stopBot(); setTimeout(startBot, 2000); });

ipcMain.handle('window:minimize', () => mainWindow?.minimize());
ipcMain.handle('window:maximize', () => {
  if (mainWindow?.isMaximized()) mainWindow.unmaximize();
  else mainWindow?.maximize();
});
ipcMain.handle('window:close', () => mainWindow?.close());
ipcMain.handle('window:isMaximized', () => mainWindow?.isMaximized() || false);

// ─── Clipboard ───
ipcMain.handle('clipboard:getImage', () => {
  const img = clipboard.readImage();
  if (img.isEmpty()) return null;
  const png = img.toPNG();
  const name = `clipboard-${Date.now()}.png`;
  const tmpPath = join(tmpdir(), name);
  writeFileSync(tmpPath, png);
  return { name, tmpPath, size: png.length };
});

ipcMain.handle('clipboard:getFilePaths', () => {
  if (process.platform === 'win32') {
    const raw = clipboard.readBuffer('FileNameW');
    if (raw && raw.length > 2) {
      const str = raw.toString('utf16le').replace(/\0+$/, '');
      return str.split('\0').filter(Boolean);
    }
  }
  return [];
});

// ─── User Terminals (node-pty instances for MCP terminal tools) ───

const require = createRequire(import.meta.url);
let pty;
try { pty = require('node-pty'); } catch { /* node-pty not available */ }

const userTerminals = new Map(); // key → { proc, output[], maxOutput }
const MAX_TERM_OUTPUT = 500;
const isWin = process.platform === 'win32';

// Strip ANSI escape codes and control sequences from PTY output
function stripAnsi(str) {
  return str
    .replace(/\x1b\[[0-9;?]*[a-zA-Z]/g, '')   // CSI sequences (colors, cursor, clear)
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, '') // OSC sequences (title etc)
    .replace(/\x1b[()][0-9A-Z]/g, '')           // Character set selection
    .replace(/\x1b[>=<]/g, '')                  // Keypad/cursor mode
    .replace(/\r/g, '')                          // Carriage returns
    .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f]/g, ''); // Other control chars (keep \n \t)
}

const terminalIpcHandlers = {
  async terminalSpawn(msg) {
    if (!pty) return { ok: false, error: 'node-pty not available' };
    const { key, name, command, cwd } = msg;
    if (userTerminals.has(key)) return { ok: false, error: `Terminal '${name}' already exists` };

    const shell = isWin ? 'cmd.exe' : (process.env.SHELL || '/bin/bash');
    const shellArgs = command
      ? (isWin ? ['/c', command] : ['-c', command])
      : [];
    const proc = pty.spawn(shell, shellArgs, {
      name: 'xterm-256color',
      cols: 120, rows: 30,
      cwd: cwd || process.cwd(),
      env: { ...process.env, FORCE_COLOR: '1' },
    });

    const entry = { proc, output: [], name };
    userTerminals.set(key, entry);

    proc.onData((data) => {
      const clean = stripAnsi(data);
      const lines = clean.split('\n');
      for (const line of lines) {
        const trimmed = line.trimEnd();
        if (trimmed) entry.output.push(trimmed);
      }
      while (entry.output.length > MAX_TERM_OUTPUT) entry.output.shift();
    });

    proc.onExit(({ exitCode }) => {
      const inst = userTerminals.get(key);
      if (inst) inst.exited = true;
      botProcess?.send({ type: 'terminalExited', key, exitCode });
    });

    return { ok: true, name };
  },

  async terminalWrite(msg) {
    const entry = userTerminals.get(msg.key);
    if (!entry) return { ok: false, error: 'Terminal not found' };
    if (entry.exited) return { ok: false, error: 'Terminal has exited' };
    entry.proc.write(msg.input + '\r');
    return { ok: true };
  },

  async terminalOutput(msg) {
    const entry = userTerminals.get(msg.key);
    if (!entry) return { ok: false, error: 'Terminal not found' };
    const { start, end, countOnly } = msg;
    const total = entry.output.length;
    if (countOnly) return { ok: true, total };
    const s = Math.max(0, start || 0);
    const e = Math.min(total, end || total);
    return { ok: true, lines: entry.output.slice(s, e), total };
  },

  async terminalClose(msg) {
    const entry = userTerminals.get(msg.key);
    if (!entry) return { ok: false, error: 'Terminal not found' };
    try { entry.proc.kill(); } catch { /* ignore */ }
    userTerminals.delete(msg.key);
    return { ok: true };
  },

  async terminalResize(msg) {
    const entry = userTerminals.get(msg.key);
    if (!entry) return { ok: false, error: 'Terminal not found' };
    if (entry.exited) return { ok: false, error: 'Terminal has exited' };
    try { entry.proc.resize(msg.cols || 120, msg.rows || 30); } catch { /* ignore */ }
    return { ok: true };
  },

  async terminalWait(msg) {
    const entry = userTerminals.get(msg.key);
    if (!entry) return { ok: false, error: 'Terminal not found' };
    const pattern = msg.pattern;
    const timeout = Math.min(msg.timeout || 30000, 60000);
    const re = new RegExp(pattern);

    // Check existing output first
    for (const line of entry.output) {
      if (re.test(line)) return { ok: true, matched: true, line };
    }

    // Watch for new output
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        cleanup();
        resolve({ ok: true, matched: false, line: null });
      }, timeout);

      const onData = (data) => {
        const lines = data.split('\n');
        for (const line of lines) {
          if (re.test(line)) {
            cleanup();
            resolve({ ok: true, matched: true, line });
            return;
          }
        }
      };

      const cleanup = () => {
        clearTimeout(timer);
        entry.proc.removeListener?.('data', onData);
      };

      entry.proc.onData(onData);
    });
  },
};

// ─── Browser IPC (hidden BrowserWindows for MCP browser tools) ───

const browserWindows = new Map(); // key → BrowserWindow

function handleBotMessage(msg) {
  if (!msg || !msg.type) return;

  // Terminal IPC
  const termHandler = terminalIpcHandlers[msg.type];
  if (termHandler) {
    termHandler(msg).then(result => {
      botProcess?.send({ type: 'terminalResult', requestId: msg.requestId, ...result });
    }).catch(err => {
      botProcess?.send({ type: 'terminalResult', requestId: msg.requestId, ok: false, error: err.message });
    });
    return;
  }

  // Browser IPC
  if (!msg.requestId) return;
  const handler = browserIpcHandlers[msg.type];
  if (handler) {
    handler(msg).then(result => {
      botProcess?.send({ type: 'browserResult', requestId: msg.requestId, ...result });
    }).catch(err => {
      botProcess?.send({ type: 'browserResult', requestId: msg.requestId, ok: false, error: err.message });
    });
  }
}

const browserIpcHandlers = {
  async browserOpen(msg) {
    const { key, url, width, height } = msg;
    if (browserWindows.has(key)) return { ok: false, error: `Browser "${key}" already open` };
    if (browserWindows.size >= 20) return { ok: false, error: 'Too many browser instances (max 20)' };
    const win = new BrowserWindow({
      width: width || 1280, height: height || 720,
      show: false,
      webPreferences: { contextIsolation: true, nodeIntegration: false },
    });
    win.webContents.on('console-message', (_e, _level, message) => {
      const inst = browserWindows.get(key);
      if (inst) inst.consoleLogs.push({ time: Date.now(), text: message });
    });
    const entry = { win, consoleLogs: [], networkEntries: [], recording: false };
    browserWindows.set(key, entry);

    // Network request capture via webRequest
    const ses = win.webContents.session;
    ses.webRequest.onBeforeSendHeaders((details, cb) => {
      if (entry.recording) {
        entry.networkEntries.push({
          id: details.id,
          url: details.url,
          method: details.method,
          resourceType: details.resourceType,
          requestHeaders: details.requestHeaders,
          timestamp: details.timestamp || Date.now(),
        });
      }
      cb({ cancel: false, requestHeaders: details.requestHeaders });
    });
    ses.webRequest.onCompleted((details) => {
      if (entry.recording) {
        const existing = entry.networkEntries.find(e => e.id === details.id);
        if (existing) {
          existing.statusCode = details.statusCode;
          existing.statusLine = details.statusLine;
          existing.responseHeaders = details.responseHeaders;
          existing.fromCache = details.fromCache;
          existing.completedAt = Date.now();
        }
      }
    });
    ses.webRequest.onErrorOccurred((details) => {
      if (entry.recording) {
        const existing = entry.networkEntries.find(e => e.id === details.id);
        if (existing) {
          existing.error = details.error;
          existing.completedAt = Date.now();
        }
      }
    });

    try {
      await win.loadURL(url);
      return { ok: true };
    } catch (err) {
      browserWindows.delete(key);
      win.destroy();
      return { ok: false, error: err.message };
    }
  },

  async browserClose(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    entry.win.destroy();
    browserWindows.delete(msg.key);
    return { ok: true };
  },

  async browserNavigate(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await entry.win.loadURL(msg.url);
    return { ok: true, url: entry.win.webContents.getURL() };
  },

  async browserScreenshot(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const image = await entry.win.webContents.capturePage();
    const buf = image.toPNG();
    return { ok: true, data: buf.toString('base64') };
  },

  async browserReadPage(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const wc = entry.win.webContents;
    const html = await wc.executeJavaScript(`
      (function() {
        const clone = document.body.cloneNode(true);
        clone.querySelectorAll('script,style,noscript,svg').forEach(el => el.remove());
        return clone.innerText.substring(0, 50000);
      })()
    `);
    return {
      ok: true,
      title: wc.getTitle(),
      url: wc.getURL(),
      text: html,
      consoleLogs: entry.consoleLogs.slice(-20),
    };
  },

  async browserSource(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const html = await entry.win.webContents.executeJavaScript('document.documentElement.outerHTML');
    return { ok: true, html };
  },

  async browserClick(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await entry.win.webContents.executeJavaScript(
      `document.querySelector(${JSON.stringify(msg.selector)})?.click()`
    );
    return { ok: true };
  },

  async browserType(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    if (msg.selector) {
      await entry.win.webContents.executeJavaScript(
        `document.querySelector(${JSON.stringify(msg.selector)})?.focus()`
      );
    }
    entry.win.webContents.insertText(msg.text);
    return { ok: true };
  },

  async browserFill(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await entry.win.webContents.executeJavaScript(`
      (function() {
        const el = document.querySelector(${JSON.stringify(msg.selector)});
        if (!el) throw new Error('Element not found');
        el.focus();
        el.value = ${JSON.stringify(msg.value)};
        el.dispatchEvent(new Event('input', {bubbles:true}));
        el.dispatchEvent(new Event('change', {bubbles:true}));
      })()
    `);
    return { ok: true };
  },

  async browserSelect(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await entry.win.webContents.executeJavaScript(`
      (function() {
        const el = document.querySelector(${JSON.stringify(msg.selector)});
        if (!el) throw new Error('Element not found');
        el.value = ${JSON.stringify(msg.value)};
        el.dispatchEvent(new Event('change', {bubbles:true}));
      })()
    `);
    return { ok: true };
  },

  async browserEval(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const result = await entry.win.webContents.executeJavaScript(msg.code);
    return { ok: true, result: typeof result === 'string' ? result : JSON.stringify(result) };
  },

  async browserMouse(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const wc = entry.win.webContents;
    if (msg.action === 'click') {
      wc.sendInputEvent({ type: 'mouseDown', x: msg.x, y: msg.y, button: 'left', clickCount: 1 });
      wc.sendInputEvent({ type: 'mouseUp', x: msg.x, y: msg.y, button: 'left', clickCount: 1 });
    } else {
      wc.sendInputEvent({ type: 'mouseMove', x: msg.x, y: msg.y });
    }
    return { ok: true };
  },

  async browserResize(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    entry.win.setContentSize(msg.width, msg.height);
    return { ok: true };
  },

  async browserScroll(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await entry.win.webContents.executeJavaScript(`
      (function() {
        const el = ${msg.selector ? 'document.querySelector(' + JSON.stringify(msg.selector) + ')' : 'window'};
        if (el) el.scrollBy(${msg.x || 0}, ${msg.y || 0});
      })()
    `);
    return { ok: true };
  },

  async browserWait(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const timeout = msg.timeout || 30000;
    const result = await entry.win.webContents.executeJavaScript(`
      new Promise((resolve, reject) => {
        const el = document.querySelector(${JSON.stringify(msg.selector)});
        if (el) return resolve(true);
        const obs = new MutationObserver(() => {
          if (document.querySelector(${JSON.stringify(msg.selector)})) { obs.disconnect(); resolve(true); }
        });
        obs.observe(document.body, { childList: true, subtree: true });
        setTimeout(() => { obs.disconnect(); reject(new Error('Timeout waiting for ' + ${JSON.stringify(msg.selector)})); }, ${timeout});
      })
    `);
    return { ok: true };
  },

  async browserGoBack(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    entry.win.webContents.goBack();
    return { ok: true };
  },

  async browserGoForward(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    entry.win.webContents.goForward();
    return { ok: true };
  },

  async browserConsole(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    let logs = entry.consoleLogs;
    if (msg.countOnly) return { ok: true, count: logs.length };
    const start = msg.start || 0;
    const end = msg.end || logs.length;
    logs = logs.slice(start, end);
    if (msg.clear) entry.consoleLogs = [];
    return { ok: true, logs };
  },

  async browserNetwork(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    if (msg.action === 'start') { entry.recording = true; entry.networkEntries = []; return { ok: true, recording: true }; }
    if (msg.action === 'stop') { entry.recording = false; return { ok: true, recording: false }; }
    if (msg.action === 'clear') { entry.networkEntries = []; return { ok: true, recording: entry.recording }; }
    if (msg.action === 'log') {
      const total = entry.networkEntries.length;
      if (msg.countOnly) return { ok: true, recording: entry.recording, total };
      const requests = entry.networkEntries.slice(msg.start || 0, msg.end || undefined);
      return { ok: true, requests, recording: entry.recording, total };
    }
    return { ok: false, error: 'Unknown network action' };
  },

  async browserCookies(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const ses = entry.win.webContents.session;
    if (!msg.action || msg.action === 'get') {
      const cookies = await ses.cookies.get(msg.filter || {});
      return { ok: true, cookies };
    }
    if (msg.action === 'set' && msg.cookie) {
      await ses.cookies.set(msg.cookie);
      return { ok: true };
    }
    if (msg.action === 'delete') {
      const url = msg.url || entry.win.webContents.getURL();
      const name = msg.cookie?.name || '';
      await ses.cookies.remove(url, name);
      return { ok: true };
    }
    return { ok: false, error: 'Unknown cookie action' };
  },

  async browserSnapshot(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const tree = await entry.win.webContents.executeJavaScript(`
      (function walk(el, depth) {
        if (depth > ${msg.maxDepth || 10}) return null;
        const role = el.getAttribute?.('role') || el.tagName?.toLowerCase() || '';
        const label = el.getAttribute?.('aria-label') || el.textContent?.substring(0, 80) || '';
        const children = [];
        for (const c of (el.children || [])) { const r = walk(c, depth+1); if(r) children.push(r); }
        return { role, label: label.trim(), children: children.length ? children : undefined };
      })(document.body, 0)
    `);
    const text = JSON.stringify(tree, null, 2);
    return { ok: true, snapshot: text.substring(0, msg.maxLength || 50000) };
  },

  async browserElements(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const sel = msg.selector || 'a,button,input,select,textarea,[role="button"],[onclick]';
    const elements = await entry.win.webContents.executeJavaScript(`
      Array.from(document.querySelectorAll(${JSON.stringify(sel)})).map(el => ({
        tag: el.tagName.toLowerCase(),
        id: el.id || undefined,
        class: el.className || undefined,
        text: (el.textContent || '').trim().substring(0, 80),
        type: el.type || undefined,
        href: el.href || undefined,
        value: el.value || undefined,
      }))
    `);
    if (msg.countOnly) return { ok: true, count: elements.length };
    return { ok: true, elements: elements.slice(msg.start || 0, msg.end || elements.length) };
  },

  async browserPdf(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    const buf = await entry.win.webContents.printToPDF({
      landscape: msg.landscape || false,
      printBackground: true,
    });
    return { ok: true, data: Buffer.from(buf).toString('base64') };
  },
};

// ─── App Lifecycle ───

app.whenReady().then(() => {
  appendLog('[lifecycle] app.whenReady fired');
  try {
    createTray();
    appendLog('[lifecycle] Tray created');
  } catch (err) {
    appendLog(`[lifecycle] Tray error: ${err.message}`);
  }
  try {
    createWindow();
    appendLog('[lifecycle] Window created');
  } catch (err) {
    appendLog(`[lifecycle] Window error: ${err.message}`);
  }
  try {
    startBot();
    appendLog('[lifecycle] startBot() called');
  } catch (err) {
    appendLog(`[lifecycle] startBot error: ${err.message}`);
  }
});

app.on('window-all-closed', () => {
  // Don't quit — stay in tray
});

app.on('activate', () => showWindow());

app.on('before-quit', (e) => {
  // Defer the real quit until services/terminals/tunnels are stopped, then exit.
  if (quitting) return;
  e.preventDefault();
  gracefulShutdownAndExit();
});
