/**
 * Electron main process for Crundi.
 *
 * Simplified terminal-first app:
 *   1. Spawns src/index.js (webapp + bot + terminals)
 *   2. Opens a BrowserWindow that loads the webapp URL
 *   3. Tray icon with start/stop/quit
 *   4. Setup wizard for first-time .env configuration
 */

import { app, BrowserWindow, WebContentsView, Tray, Menu, ipcMain, nativeImage, dialog, clipboard, Notification, shell } from 'electron';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';
import { dirname, join } from 'path';
import { spawn, execFileSync } from 'child_process';
import { existsSync, readFileSync, writeFileSync, appendFileSync, mkdirSync, copyFileSync, readdirSync, statSync, unlinkSync } from 'fs';
import { tmpdir, homedir } from 'os';
import electronUpdater from 'electron-updater';
const { autoUpdater } = electronUpdater;

const __dirname = dirname(fileURLToPath(import.meta.url));

const appRoot = app.isPackaged
  ? join(dirname(app.getPath('exe')), 'resources', 'app.asar.unpacked')
  : join(__dirname, '..');
const srcDir = join(appRoot, 'src');
const assetsDir = app.isPackaged
  ? join(dirname(app.getPath('exe')), 'resources', 'assets')
  : join(__dirname, '..', 'assets');

// On Linux the all-in-one app keeps its settings in a folder of its own.
// Left alone it would use ~/.config/crundi, which is exactly where a server
// installed with install.sh keeps its .env and data: the two would be one
// installation run twice, each starting a server on the same port over the
// same files. Set before anything asks where userData is.
if (app.isPackaged && process.platform === 'linux' && existsSync(join(srcDir, 'index.js'))) {
  app.setPath('userData', join(app.getPath('appData'), 'crundi-desktop'));
  // Electron makes its usual folder by itself but not one it was pointed at,
  // and until it existed the first lines of the log (written below, before
  // the app is ready) had nowhere to go and were lost.
  try { mkdirSync(app.getPath('userData'), { recursive: true }); } catch { /* the log says so later */ }
}

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
// The server asked to be started again (a restored backup is applied as it starts).
let restartRequested = false;
// First run chose "Restore from a backup": open the web page on its restore form.
let openRestoreForm = false;
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

/**
 * Show the window and put it in front, for a click on a notification.
 *
 * show() and focus() are enough on Windows. GNOME's focus-stealing prevention
 * refuses a window that raises itself, and shows "Crundi is ready" instead:
 * the click is the person asking for it, so the app asks for focus outright
 * and holds the window on top for a moment while the desktop catches up.
 */
/**
 * Show one chat in the main window. The page does the switching
 * (window.crundiOpenChat, which waits for the chat list if the page is still
 * loading); the id is checked to be an id before it goes anywhere near code.
 */
function openChatInWindow(id) {
  if (!/^[A-Za-z0-9_-]{1,80}$/.test(id)) return;
  const w = mainWindow;
  if (!w || w.isDestroyed()) return;
  const run = () => w.webContents.executeJavaScript('window.crundiOpenChat && window.crundiOpenChat(' + JSON.stringify(id) + ')').catch(() => {});
  if (w.webContents.isLoading()) w.webContents.once('did-finish-load', run); else run();
}

function bringToFront() {
  showWindow();
  const w = mainWindow;
  if (!w || w.isDestroyed()) return;
  try { app.focus({ steal: true }); } catch { /* not on this platform */ }
  if (process.platform === 'linux') {
    w.setAlwaysOnTop(true);
    w.show();
    w.focus();
    setTimeout(() => { if (!w.isDestroyed()) w.setAlwaysOnTop(false); }, 300);
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
    // An .ico is a Windows file; everywhere else the window takes a PNG.
    icon: join(assetsDir, process.platform === 'win32' ? 'icon.ico' : 'icon_256x256.png'),
    frame: false,
    titleBarStyle: 'hidden',
    backgroundColor: '#0a0a0f',
    webPreferences: {
      preload: join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      webviewTag: true, // interactive browser panel embeds <webview> overlays
    },
    show: false,
  });

  // Links that open a new window (chat, notes, release notes) go to the
  // system browser, not a bare Electron window with none of its controls.
  mainWindow.webContents.setWindowOpenHandler(({ url: u }) => {
    if (/^https?:\/\//i.test(u)) { shell.openExternal(u).catch(() => {}); }
    return { action: 'deny' };
  });

  const htmlPath = join(__dirname, 'index.html');
  appendLog(`[lifecycle] Loading ${htmlPath}`);
  mainWindow.loadFile(htmlPath);

  wireDownloads(mainWindow.webContents.session);

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

// ─── Downloads ───
//
// One Save dialog per download. This used to open a dialog of its own, from a
// promise, after will-download had returned. A download with no save path set
// by then gets Electron's own dialog as well, so on Linux every click asked
// twice and could save the file twice. And the handler was added to the
// shared session each time the window was made, which a second window (the
// setup wizard, a reopen) did again, adding another dialog each time. Now the
// dialog's options are given at once and Electron shows its one dialog, and
// a session is wired only once.
const wiredSessions = new WeakSet();

function wireDownloads(ses) {
  if (!ses || wiredSessions.has(ses)) return;
  wiredSessions.add(ses);
  ses.on('will-download', (_event, item) => {
    let dir = '';
    try { dir = app.getPath('downloads'); } catch { /* no Downloads folder: the dialog picks */ }
    item.setSaveDialogOptions({
      title: 'Save ' + item.getFilename(),
      defaultPath: dir ? join(dir, item.getFilename()) : item.getFilename(),
    });
  });
}

// ─── Tray ───

function createTray() {
  appendLog('[lifecycle] Creating tray...');
  // The tray is 16 pixels: use the head, which is drawn for that size, rather
  // than the whole robot scaled down from the window icon.
  const small = join(assetsDir, 'icon_32x32.png');
  const iconPath = existsSync(small) ? small : join(assetsDir, 'icon.ico');
  const icon = existsSync(iconPath)
    ? nativeImage.createFromPath(iconPath).resize({ width: 16, height: 16 })
    : nativeImage.createEmpty();

  tray = new Tray(icon);
  tray.setToolTip('Crundi');
  updateTrayMenu();
  tray.on('double-click', () => showWindow());
  // Linux trays report a single click, where they report a click at all
  // (an AppIndicator shows only the menu, which has Open in it).
  if (process.platform === 'linux') tray.on('click', () => showWindow());
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
    {
      // In remote mode the window goes straight to the other server, so this is
      // the way back to the local shell to change or clear that setting.
      label: 'Server…',
      click: () => {
        showWindow();
        const send = () => mainWindow?.webContents.send('show:server');
        try {
          const url = mainWindow?.webContents.getURL() || '';
          if (url.startsWith('file://')) send();
          else {
            mainWindow.loadFile(join(__dirname, 'index.html'));
            mainWindow.webContents.once('did-finish-load', send);
          }
        } catch { /* ignore */ }
      },
    },
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
    updateState.downloaded
      ? { label: 'Restart to Install Update', click: () => confirmAndInstall() }
      : { label: 'Check for Updates…', click: () => checkForUpdates(true) },
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

// ─── Client mode ───
//
// The app can either run its own server (the all-in-one install, unchanged) or
// attach to one somewhere else — a container on the LAN, say. In remote mode we
// spawn nothing and instead lend that server this machine's GUI, so the browser
// panel still opens in front of the person using it.
const clientConfigFile = () => join(dataDir, 'client.json');

// Whether THIS build ships a server at all. The client-only installer omits
// src/, which makes the question self-answering — no build-time flag to set,
// and no way for the two to disagree.
const serverBundled = existsSync(join(srcDir, 'index.js'));

function readClientConfig() {
  // A client-only build has nothing to run locally, so 'local' is not a mode it
  // can be in — say so plainly rather than failing later at spawn time.
  const fallback = { mode: serverBundled ? 'local' : 'remote', serverUrl: '' };
  try {
    if (!existsSync(clientConfigFile())) return fallback;
    const d = JSON.parse(readFileSync(clientConfigFile(), 'utf8'));
    const wantsRemote = d.mode === 'remote' || !serverBundled;
    return {
      mode: wantsRemote ? 'remote' : 'local',
      serverUrl: String(d.serverUrl || ''),
    };
  } catch { return fallback; }
}

function writeClientConfig(cfg) {
  try {
    mkdirSync(dataDir, { recursive: true });
    writeFileSync(clientConfigFile(), JSON.stringify({
      mode: cfg.mode === 'remote' ? 'remote' : 'local',
      serverUrl: String(cfg.serverUrl || '').replace(/\/+$/, ''),
    }, null, 2));
    return true;
  } catch (err) {
    appendLog(`[client] Could not save client config: ${err.message}`);
    return false;
  }
}

// ─── Running the server on Linux and macOS ───
//
// Started from a desktop launcher, an app does not get the PATH a terminal
// has: nothing from ~/.bashrc or ~/.profile, so no ~/.local/bin, no nvm, no
// npm-global. The server finds `claude`, `git` and everything else it runs
// through PATH, and with the launcher's bare one it finds none of what the
// person installed for themselves. So the PATH is asked of their own login
// shell, once, and what it says goes in front of what we were given.
let shellPathCache;
function loginShellPath() {
  if (process.platform === 'win32') return '';
  if (shellPathCache !== undefined) return shellPathCache;
  shellPathCache = '';
  try {
    const sh = process.env.SHELL || '/bin/bash';
    // Marked on both sides: an interactive shell may print a greeting, and
    // only what is between the marks is the PATH.
    const out = execFileSync(sh, ['-ilc', 'printf "__CRUNDI__%s__CRUNDI__" "$PATH"'], {
      encoding: 'utf8', timeout: 5000, stdio: ['ignore', 'pipe', 'ignore'],
    });
    const m = /__CRUNDI__([\s\S]*?)__CRUNDI__/.exec(out);
    if (m) shellPathCache = m[1].trim();
  } catch (err) {
    appendLog(`[electron] Could not ask the login shell for its PATH: ${err.message}`);
  }
  return shellPathCache;
}

/** The Node that ships inside the Linux package, or '' where none does. */
function bundledNode() {
  if (!app.isPackaged) return '';
  const p = join(process.resourcesPath, 'node', 'bin', 'node');
  return existsSync(p) ? p : '';
}

/** PATH for the server: the login shell's, then ours, then the usual per-user places, then the bundled Node. */
function serverPath() {
  const sep = process.platform === 'win32' ? ';' : ':';
  const parts = [];
  const add = (list) => { for (const d of String(list || '').split(sep)) if (d && !parts.includes(d)) parts.push(d); };
  add(loginShellPath());
  add(process.env.PATH);
  if (process.platform !== 'win32') {
    add([join(homedir(), '.local', 'bin'), '/usr/local/bin'].join(sep));
    // Last, so a Node the person installed is the one their tools get; this
    // one is there so that `node` exists at all on a machine without it.
    const node = bundledNode();
    if (node) add(dirname(node));
  }
  return parts.join(sep);
}

function startBot() {
  if (botProcess) return;

  // Remote: nothing to spawn. Point the window at the other server and wait for
  // the page to hand us a token so we can register as its native host.
  const client = readClientConfig();
  if (client.mode === 'remote') {
    if (!client.serverUrl) {
      // Nothing to connect to yet — ask, rather than sitting on a blank window.
      appendLog('[client] Remote mode with no server set — prompting');
      setBotStatus('stopped');
      showWindow();
      mainWindow?.webContents.send('show:server');
      return;
    }
    appendLog(`[client] Remote mode — using ${client.serverUrl}`);
    setBotStatus('running');
    if (mainWindow && !mainWindow.isDestroyed()) mainWindow.loadURL(client.serverUrl);
    return;
  }

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

  // Windows runs the server with the Node on the machine. The Linux package
  // carries its own, the one its native modules were built against, so the
  // server starts on a machine with no Node and never meets one too old.
  const nodeExe = app.isPackaged ? (bundledNode() || 'node') : process.execPath;
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
  if (process.platform !== 'win32') spawnEnv.PATH = serverPath();
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
          const restoreParam = openRestoreForm ? (authParam ? '&' : '?') + 'restore=1' : '';
          openRestoreForm = false;
          const url = `http://localhost:${webappPort}${authParam}${restoreParam}`;
          appendLog(`[lifecycle] Navigating to ${url}`);
          mainWindow.loadURL(url);
        }
      }
    }
  });

  botProcess.stderr?.on('data', (data) => {
    String(data).split('\n').filter(Boolean).forEach(line => appendLog(`[err] ${line}`));
  });

  botProcess.on('message', (m) => handleBotMessage(m));

  botProcess.on('exit', (code, signal) => {
    appendLog(`[electron] Process exited: code=${code} signal=${signal}`);
    botProcess = null;
    webappPort = null;
    if (app.isQuitting) return;
    if (restartRequested) {
      restartRequested = false;
      appendLog('[electron] Restarting the server as it asked...');
      setTimeout(() => startBot(), 400);
      return;
    }
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

// Gracefully stop the server process (it stops services/tunnels/Claude terminals via
// the IPC 'shutdown' message — reliable on Windows where SIGTERM isn't catchable),
// then hard-kill the tree as a guaranteed fallback.
async function stopServerProcess() {
  // Local (main-process) children first.
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

// ─── Auto-update (electron-updater via GitHub Releases) ───
// The official electron-builder updater: reads latest.yml from this repo's
// GitHub Releases, verifies the installer's Authenticode signature (publisherName),
// downloads in the background and installs on restart. Checks shortly after
// launch and once every 24h — but only if the user opted in (first-run consent,
// toggleable in Settings). Public repo → no token needed.
//
// The opt-in lives in its own file (NOT .env), so the server rewriting .env on a
// settings-save can never clobber it.
const updateConfigPath = join(userDataDir, 'update-config.json');
let updateInstalling = false;
let updateFile = '';   // the downloaded installer (update-downloaded's downloadedFile)
const updateState = {
  enabled: true, available: false, downloading: false, downloaded: false, percent: 0, version: '', current: app.getVersion(),
  channel: 'production',   // 'production' | 'dev' — which releases this app follows
  latestKind: '',          // 'production' | 'dev' — what the offered update is
};

// The whole file, merged on write: it holds more than one setting now, and
// writing just the one being changed used to drop the rest.
function readUpdateConfig() {
  try {
    if (existsSync(updateConfigPath)) {
      const c = JSON.parse(readFileSync(updateConfigPath, 'utf8'));
      if (c && typeof c === 'object') return c;
    }
  } catch { /* default */ }
  return {};
}
function writeUpdateConfig(patch) {
  try { writeFileSync(updateConfigPath, JSON.stringify({ ...readUpdateConfig(), ...patch }), 'utf8'); } catch { /* non-fatal */ }
}
function readAutoUpdate() {
  const c = readUpdateConfig();
  return typeof c.autoUpdate === 'boolean' ? c.autoUpdate : true; // default ON
}
function writeAutoUpdate(enabled) { writeUpdateConfig({ autoUpdate: !!enabled }); }
function readChannel() { return readUpdateConfig().channel === 'dev' ? 'dev' : 'production'; }

// ─── Update channels ───
//
// Production follows GitHub's "latest release", which never includes a
// prerelease. Dev follows the highest version of either kind — a dev build, or
// a production release once one is newer than it — which is how a dev
// subscriber is always offered the newer production release.
//
// electron-updater's own prerelease mode cannot do that: on a custom channel
// like "dev" it skips stable releases entirely, and it switches itself on
// whenever the running version is a prerelease. So for Dev we choose the
// release ourselves and point a generic feed straight at it, and for
// Production we turn that mode off explicitly. Downgrades are never allowed:
// switching from Dev back to Production waits for a newer production release.

/** Owner, repo and update-file name this build publishes to (from app-update.yml). */
function feedInfo() {
  let owner = 'TheRealFaruhaan', repo = 'crundi', channel = 'latest';
  try {
    const y = readFileSync(join(process.resourcesPath, 'app-update.yml'), 'utf8');
    owner = (y.match(/^owner:\s*(\S+)/m) || [])[1] || owner;
    repo = (y.match(/^repo:\s*(\S+)/m) || [])[1] || repo;
    channel = (y.match(/^channel:\s*(\S+)/m) || [])[1] || channel;   // 'client' for the client-only app
  } catch { /* the defaults */ }
  return { owner, repo, channel };
}

/** a > b, with 1.2.0-dev.3 < 1.2.0-dev.10 < 1.2.0. */
function versionGt(a, b) {
  const p = (v) => { const m = String(v).replace(/^v/, '').match(/^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?/); return m ? { n: [+m[1], +m[2], +m[3]], pre: m[4] ? m[4].split('.') : [] } : null; };
  const x = p(a), y = p(b);
  if (!x || !y) return false;
  for (let i = 0; i < 3; i++) if (x.n[i] !== y.n[i]) return x.n[i] > y.n[i];
  if (!x.pre.length || !y.pre.length) return !x.pre.length && !!y.pre.length;
  for (let i = 0; i < Math.max(x.pre.length, y.pre.length); i++) {
    const s = x.pre[i], t = y.pre[i];
    if (s === undefined) return false;
    if (t === undefined) return true;
    const sn = /^\d+$/.test(s), tn = /^\d+$/.test(t);
    if (sn && tn && +s !== +t) return +s > +t;
    if (sn !== tn) return !sn;
    if (s !== t) return s > t;
  }
  return false;
}

const UPDATE_FILE_SUFFIX = process.platform === 'linux' ? '-linux' : process.platform === 'darwin' ? '-mac' : '';

/** The newest release of either kind carrying this build's update file. */
async function pickDevRelease() {
  const { owner, repo, channel } = feedInfo();
  const res = await fetch(`https://api.github.com/repos/${owner}/${repo}/releases?per_page=30`, {
    headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'crundi' },
    signal: AbortSignal.timeout(15000),
  });
  if (!res.ok) throw new Error('GitHub returned ' + res.status);
  let best = null;
  for (const r of await res.json()) {
    // The update file is latest.yml / client.yml on Windows and carries the
    // platform in its name elsewhere (latest-linux.yml), which is how one
    // release holds a build for each without them reading each other's.
    if (r.draft || !(r.assets || []).some(a => a.name === channel + UPDATE_FILE_SUFFIX + '.yml')) continue;
    const v = String(r.tag_name || '').replace(/^v/, '');
    if (!best || versionGt(v, best.version)) best = { tag: r.tag_name, version: v, prerelease: !!r.prerelease };
  }
  return best;
}

/** Point the updater at the right place for the chosen channel. */
async function applyFeed() {
  const { owner, repo, channel } = feedInfo();
  autoUpdater.allowDowngrade = false;
  if (updateState.channel === 'dev') {
    const pick = await pickDevRelease();
    if (!pick) throw new Error('No release found');
    autoUpdater.allowPrerelease = true;
    autoUpdater.setFeedURL({ provider: 'generic', url: `https://github.com/${owner}/${repo}/releases/download/${pick.tag}`, channel });
    updateState.latestKind = pick.prerelease ? 'dev' : 'production';
  } else {
    autoUpdater.allowPrerelease = false;
    autoUpdater.setFeedURL({ provider: 'github', owner, repo, ...(channel !== 'latest' ? { channel } : {}) });
    updateState.latestKind = 'production';
  }
}

function setUpdateChannel(ch) {
  const next = ch === 'dev' ? 'dev' : 'production';
  writeUpdateConfig({ channel: next });
  updateState.channel = next;
  updateState.available = false; updateState.version = ''; updateState.latestKind = '';
  downloadPrompted = '';
  appendLog('[update] Channel: ' + next);
  sendUpdateStatus();
  checkForUpdates(true);
}

function sendUpdateStatus() {
  if (mainWindow && !mainWindow.isDestroyed()) {
    try { mainWindow.webContents.send('update:status', { ...updateState }); } catch { /* ignore */ }
  }
  updateTrayMenu();
}

let downloadPrompted = ''; // version we've already shown the "download?" modal for

function setupAutoUpdate() {
  updateState.enabled = readAutoUpdate();
  updateState.channel = readChannel();
  autoUpdater.autoDownload = false;             // ask first (modal on detection)
  // On Linux the library installs a .deb through pkexec synchronously, which
  // freezes the app (and, at quit, springs a password prompt). We install it
  // ourselves there, asynchronously; see installUpdateNow().
  autoUpdater.autoInstallOnAppQuit = process.platform !== 'linux';
  // Full downloads only. Differential (blockmap) reconstruction of a code-signed
  // NSIS installer yields a different hash → "sha512 checksum mismatch". A full
  // download verifies cleanly.
  autoUpdater.disableDifferentialDownload = true;
  autoUpdater.logger = {
    info: (m) => appendLog('[update] ' + m),
    warn: (m) => appendLog('[update] ' + m),
    error: (m) => appendLog('[update] ' + m),
    debug: () => {},
  };
  autoUpdater.on('checking-for-update', () => appendLog('[update] Checking for updates…'));
  autoUpdater.on('update-available', (info) => {
    appendLog('[update] Available: ' + info.version);
    updateState.available = true; updateState.version = info.version;
    updateState.downloading = false; updateState.downloaded = false; updateState.percent = 0;
    sendUpdateStatus();
    promptDownload(info); // modal on detection (e.g. shortly after launch)
  });
  autoUpdater.on('update-not-available', () => {
    appendLog('[update] Up to date');
    updateState.available = false; updateState.downloading = false; updateState.downloaded = false; updateState.percent = 0;
    sendUpdateStatus();
  });
  autoUpdater.on('download-progress', (p) => {
    updateState.downloading = true; updateState.percent = Math.round(p.percent || 0);
    appendLog(`[update] Downloading ${updateState.percent}%`);
    sendUpdateStatus();
  });
  autoUpdater.on('error', (err) => {
    updateState.downloading = false;
    appendLog('[update] Error: ' + ((err && err.message) || String(err)));
    sendUpdateStatus();
  });
  autoUpdater.on('update-downloaded', (info) => {
    appendLog('[update] Downloaded ' + info.version);
    updateFile = (info && info.downloadedFile) || '';
    updateState.available = true; updateState.downloaded = true; updateState.downloading = false; updateState.percent = 100; updateState.version = info.version;
    sendUpdateStatus();
    confirmAndInstall(); // restart-to-install prompt (save your work)
  });
}

// Modal shown when an update is first detected (once per version per session).
async function promptDownload(info) {
  if (!mainWindow || mainWindow.isDestroyed()) return;
  if (downloadPrompted === info.version) return;
  downloadPrompted = info.version;
  try {
    const { response } = await dialog.showMessageBox(mainWindow, {
      type: 'info',
      buttons: ['Download & Install', 'Later'],
      defaultId: 0, cancelId: 1,
      title: 'Update available',
      // On the Dev channel, say which kind it is: a newer production release
      // is offered over the dev build, and that should not look like another dev build.
      message: updateState.channel === 'dev'
        ? (/-/.test(info.version) ? `Dev build Crundi ${info.version} is available.` : `A production release, Crundi ${info.version}, is available.`)
        : `Crundi ${info.version} is available.`,
      detail: (updateState.channel === 'dev' && !/-/.test(info.version) ? 'It is newer than the dev build you are on. ' : '')
        + 'Download it now? You can watch progress in Settings, and Crundi will ask you to save your work before it restarts to install.',
    });
    if (response === 0) startDownload();
  } catch { /* ignore */ }
}

function startDownload() {
  if (updateState.downloaded) { confirmAndInstall(); return; }
  if (updateState.downloading) return; // already in progress
  appendLog('[update] Downloading update…');
  updateState.downloading = true; sendUpdateStatus();
  autoUpdater.downloadUpdate().catch((err) => {
    updateState.downloading = false; sendUpdateStatus();
    appendLog('[update] download error: ' + (err.message || err));
  });
}

function checkForUpdates(manual = false) {
  if (!app.isPackaged) {
    appendLog('[update] Skipped (dev build — packaged app only)');
    if (manual && mainWindow) dialog.showMessageBox(mainWindow, { type: 'info', message: 'Update checks run in the installed app only.' });
    return;
  }
  if (!updateState.enabled && !manual) return; // respect the opt-out for automatic checks
  applyFeed()
    .then(() => autoUpdater.checkForUpdates())
    .catch((err) => appendLog('[update] check failed: ' + err.message));
}

function setAutoUpdate(enabled) {
  updateState.enabled = !!enabled;
  writeAutoUpdate(updateState.enabled);
  appendLog('[update] Auto-update ' + (updateState.enabled ? 'enabled' : 'disabled'));
  sendUpdateStatus();
  if (updateState.enabled) checkForUpdates(); // re-check immediately on enable
}

// ─── Launch at Windows startup (OS login-item; OS is the source of truth) ───
//
// Linux has no login items in Electron. What every desktop there reads is a
// .desktop file in ~/.config/autostart, so that file is the setting: there
// means on, absent means off.
function autostartFile() {
  return join(process.env.XDG_CONFIG_HOME || join(homedir(), '.config'), 'autostart', `${app.getName()}.desktop`);
}
function getLaunchAtStartup() {
  if (process.platform === 'linux') return existsSync(autostartFile());
  try { return !!app.getLoginItemSettings().openAtLogin; } catch { return false; }
}
function setLaunchAtStartup(enabled) {
  try {
    if (process.platform === 'linux') {
      const file = autostartFile();
      if (!enabled) { try { unlinkSync(file); } catch { /* was not there */ } }
      else {
        mkdirSync(dirname(file), { recursive: true });
        // An AppImage runs from a mount that is gone after it exits; the
        // file that was started is the one to start again.
        const exe = process.env.APPIMAGE || app.getPath('exe');
        writeFileSync(file, [
          '[Desktop Entry]', 'Type=Application', `Name=${app.getName()}`,
          `Exec="${exe.replace(/(["\\`$])/g, '\\$1')}"`, 'Terminal=false', 'X-GNOME-Autostart-enabled=true', '',
        ].join('\n'), 'utf8');
      }
      appendLog('[startup] Launch at login ' + (enabled ? 'enabled' : 'disabled'));
      return;
    }
    app.setLoginItemSettings({ openAtLogin: !!enabled });
    appendLog('[startup] Launch at login ' + (enabled ? 'enabled' : 'disabled'));
  } catch (e) { appendLog('[startup] error: ' + (e.message || e)); }
}

// Always confirm before restarting to install — give the user a chance to save
// work and confirm running services are safe to stop.
async function confirmAndInstall() {
  if (!updateState.downloaded) { startDownload(); return; }
  if (!mainWindow || mainWindow.isDestroyed()) { installUpdateNow(); return; }
  try {
    const { response } = await dialog.showMessageBox(mainWindow, {
      type: 'warning',
      buttons: ['Restart & Install', 'Not now'],
      defaultId: 1, cancelId: 1,
      title: 'Install Crundi ' + updateState.version,
      message: `Crundi ${updateState.version} is ready to install.`,
      detail: 'Crundi will close and restart to apply the update. Before continuing:\n\n'
        + '•  Save any unsaved work in your terminals and editors.\n'
        + '•  Make sure any running services are safe to stop — they will be shut down.\n\n'
        + (process.platform === 'linux'
          ? 'Your system will ask for your password to install it.'
          : 'It will also install automatically the next time you quit Crundi.'),
    });
    if (response === 0) installUpdateNow();
  } catch { /* ignore */ }
}

/**
 * Linux, .deb: install with pkexec in the background, so the window keeps
 * responding while the password prompt waits and dpkg runs. electron-updater
 * does this with spawnSync, which froze the app ("Not Responding"), and Force
 * Quit then killed the install half way. Resolves { ok, code, output }.
 */
function installDebAsync(file, hold = {}) {
  return new Promise((resolve) => {
    let out = '';
    let child;
    try {
      // dpkg first; if dependencies are missing, let apt pull them in.
      child = spawn('pkexec', ['/bin/sh', '-c', 'dpkg -i "$1" || apt-get install -f -y', 'crundi-update', file], { stdio: ['ignore', 'pipe', 'pipe'] });
      hold.child = child;
    } catch (err) { resolve({ ok: false, code: -1, output: err.message }); return; }
    const add = (b) => { out = (out + b.toString()).slice(-4000); };
    child.stdout.on('data', add); child.stderr.on('data', add);
    child.on('error', (err) => resolve({ ok: false, code: -1, output: err.message }));
    child.on('close', (code) => resolve({ ok: code === 0, code, output: out.trim() }));
  });
}

/** The first terminal emulator this desktop has, with how to run a command in it. */
function findTerminal() {
  const dirs = (process.env.PATH || '/usr/bin:/bin').split(':');
  const has = (b) => dirs.some(d => d && existsSync(join(d, b)));
  const list = [
    ['x-terminal-emulator', (c) => ['-e', 'bash', '-c', c]],
    ['gnome-terminal', (c) => ['--', 'bash', '-c', c]],
    ['konsole', (c) => ['-e', 'bash', '-c', c]],
    ['xfce4-terminal', (c) => ['-x', 'bash', '-c', c]],
    ['kitty', (c) => ['bash', '-c', c]],
    ['alacritty', (c) => ['-e', 'bash', '-c', c]],
    ['foot', (c) => ['bash', '-c', c]],
    ['wezterm', (c) => ['start', '--', 'bash', '-c', c]],
    ['xterm', (c) => ['-e', 'bash', '-c', c]],
  ];
  const hit = list.find(([b]) => has(b));
  return hit ? { bin: hit[0], args: hit[1] } : null;
}

const shq = (v) => "'" + String(v).replace(/'/g, "'\\''") + "'";

/**
 * The fallback when no password prompt appears: install in a terminal (sudo
 * asks there), then start the new version. Returns false when there is no
 * terminal to open.
 */
function installInTerminal(file) {
  const term = findTerminal();
  if (!term) return false;
  const exe = process.env.APPIMAGE || app.getPath('exe');
  const script = `echo "Installing the Crundi Client update. sudo asks for your password."; sudo apt install -y ${shq(file)} && echo "Installed. Starting Crundi Client…" || echo "The install did not finish."; (nohup ${shq(exe)} >/dev/null 2>&1 &); sleep 3`;
  try {
    const child = spawn(term.bin, term.args(script), { detached: true, stdio: 'ignore' });
    child.unref();
    appendLog(`[update] Installing in a terminal (${term.bin})`);
    return true;
  } catch (err) {
    appendLog('[update] Could not open a terminal: ' + err.message);
    return false;
  }
}

async function installUpdateNow() {
  if (updateInstalling) return;
  updateInstalling = true;
  if (process.platform === 'linux' && /\.deb$/i.test(updateFile) && existsSync(updateFile) && existsSync('/usr/bin/pkexec')) {
    // Install while everything still runs: cancelling the password prompt
    // leaves Crundi exactly as it was. Only a successful install restarts it.
    appendLog('[update] Installing ' + updateFile + ' with pkexec…');
    updateState.installing = true; sendUpdateStatus();
    // pkexec needs the desktop's password agent. Where there is none (some
    // window managers), or its prompt opens out of sight, pkexec just waits.
    // After 20 s, offer a terminal instead; after 10 min, give up.
    const hold = {};
    let viaTerminal = false;
    const offer = setTimeout(async () => {
      if (!hold.child || hold.child.exitCode != null || !mainWindow || mainWindow.isDestroyed()) return;
      const term = findTerminal();
      const buttons = [...(term ? ['Install in a terminal'] : []), 'Copy the install command', 'Keep waiting'];
      try {
        const { response } = await dialog.showMessageBox(mainWindow, {
          type: 'question', buttons, defaultId: 0, cancelId: buttons.length - 1,
          title: 'No password prompt?',
          message: 'Your system has not shown a password prompt for the update.',
          detail: 'Some desktops have no password agent, or open its prompt out of sight. You can install it in a terminal instead (sudo asks for your password there), then Crundi Client starts again by itself.',
        });
        const pick = buttons[response];
        if (pick === 'Install in a terminal' && hold.child && hold.child.exitCode == null) {
          viaTerminal = true;
          try { hold.child.kill('SIGTERM'); } catch { /* gone */ }
        } else if (pick === 'Copy the install command') {
          clipboard.writeText('sudo apt install ' + shq(updateFile));
          appendLog('[update] Install command copied');
        }
      } catch { /* the window went away */ }
    }, 20_000);
    const giveUp = setTimeout(() => { if (hold.child && hold.child.exitCode == null) { appendLog('[update] No answer from pkexec after 10 minutes; giving up'); try { hold.child.kill('SIGTERM'); } catch { /* gone */ } } }, 10 * 60_000);
    const r = await installDebAsync(updateFile, hold);
    clearTimeout(offer); clearTimeout(giveUp);
    updateState.installing = false; sendUpdateStatus();
    if (viaTerminal) {
      if (installInTerminal(updateFile)) {
        // The terminal installs and starts the new version: get out of its way.
        app.isQuitting = true;
        quitting = true;
        try { await stopServerProcess(); } catch { /* ignore */ }
        app.exit(0);
        return;
      }
      updateInstalling = false;
      dialog.showMessageBox(mainWindow, { type: 'info', message: 'No terminal found.', detail: 'Install it yourself with:\n\nsudo apt install ' + updateFile }).catch(() => {});
      return;
    }
    if (!r.ok) {
      updateInstalling = false;
      appendLog(`[update] Install did not finish (exit ${r.code}): ${r.output}`);
      // 126: the password prompt was dismissed; 127: not authorised.
      const cancelled = r.code === 126 || r.code === 127;
      if (mainWindow && !mainWindow.isDestroyed()) {
        dialog.showMessageBox(mainWindow, {
          type: cancelled ? 'info' : 'error',
          message: cancelled ? 'The update was not installed.' : 'The update could not be installed.',
          detail: cancelled
            ? 'The password prompt was closed. Crundi carries on as it was; choose Restart & Install again whenever you like.'
            : 'Crundi carries on as it was. What the installer said:\n\n' + (r.output || 'exit code ' + r.code).slice(-1200),
        }).catch(() => {});
      }
      return;
    }
    appendLog('[update] Installed; restarting…');
    app.isQuitting = true;
    quitting = true;
    try { await stopServerProcess(); } catch { /* ignore */ }
    app.relaunch();
    app.exit(0);
    return;
  }
  appendLog('[update] Installing — stopping server first…');
  app.isQuitting = true;
  quitting = true; // bypass the graceful before-quit handler; we stop the server here
  try { await stopServerProcess(); } catch { /* ignore */ }
  autoUpdater.quitAndInstall(true, true); // silent install, relaunch after
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

// ─── Client / native-host IPC ───

ipcMain.handle('client:getConfig', () => nativeHostStatus());

ipcMain.handle('client:setConfig', (_e, cfg) => {
  const next = { mode: cfg?.mode === 'remote' ? 'remote' : 'local', serverUrl: cfg?.serverUrl || '' };
  if (next.mode === 'remote' && !/^https?:\/\//i.test(next.serverUrl)) {
    return { ok: false, error: 'Enter the full server address, including http:// or https://' };
  }
  if (!writeClientConfig(next)) return { ok: false, error: 'Could not save the setting' };
  // Switching mode changes what this process is FOR, so start clean rather than
  // trying to unpick a half-torn-down server or socket.
  stopNativeHost();
  hostToken = null;
  return { ok: true, restartRequired: true };
});

// The page hands us its access token once the user is logged in. Only useful in
// remote mode; a locally spawned server is reached over the parent channel.
ipcMain.on('client:setToken', (_e, token) => {
  const t = String(token || '');
  if (!t || t === hostToken) return;
  hostToken = t;
  // A fresh token after a refresh should replace the socket built on the old one.
  stopNativeHost();
  connectNativeHost();
});

ipcMain.handle('setup:save', (_e, config) => {
  const lines = [
    `# ─── Crundi Configuration ───`,
    `TELEGRAM_BOT_TOKEN=${config.botToken || ''}`,
    `ALLOWED_USERNAME=${config.username || ''}`,
    `PROJECTS_DIR=${config.projectsDir || ''}`,
    // 8888 with a tunnel: the port the tunnel's Public Hostname points at, the
    // one the setup steps and the README name. It was 3000 here, which no
    // instructions mentioned, and a tunnel set up from them showed "Bad
    // gateway". Without a tunnel any free port will do.
    `WEB_PORT=${config.webPort || (config.tunnelToken ? '8888' : '0')}`,
    `CLOUDFLARE_TUNNEL_TOKEN=${config.tunnelToken || ''}`,
    `CLOUDFLARE_TUNNEL_URL=${config.tunnelUrl || ''}`,
    `DATA_DIR=`,
  ];
  try { mkdirSync(dirname(envPath), { recursive: true }); } catch { /* ignore */ }
  writeFileSync(envPath, lines.join('\n'), 'utf8');
  // Restoring: the server starts with nothing set up, which is exactly when
  // its page offers to restore; the backup brings its own settings.
  if (config.restore) openRestoreForm = true;
  // First-run consent: update checks + launch-at-startup (both default on).
  writeAutoUpdate(config.autoUpdate !== false);
  updateState.enabled = config.autoUpdate !== false;
  setLaunchAtStartup(config.autoLaunch !== false);
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

// ─── Auto-update + startup IPC (renderer ↔ main) ───
ipcMain.handle('update:getState', () => ({ ...updateState, launch: getLaunchAtStartup() }));
ipcMain.handle('update:setEnabled', (_e, enabled) => { setAutoUpdate(enabled); return { ...updateState, launch: getLaunchAtStartup() }; });
ipcMain.handle('update:check', () => { checkForUpdates(true); return { ...updateState, launch: getLaunchAtStartup() }; });
ipcMain.handle('update:install', () => { confirmAndInstall(); return { ...updateState, launch: getLaunchAtStartup() }; });
ipcMain.handle('update:setChannel', (_e, ch) => { setUpdateChannel(ch); return { ...updateState, launch: getLaunchAtStartup() }; });
ipcMain.handle('startup:set', (_e, enabled) => { setLaunchAtStartup(enabled); return { ...updateState, launch: getLaunchAtStartup() }; });

// ─── Interactive browser panel — real pages via WebContentsView ───
// A WebContentsView is parented to the main window and overlaid on the panel.
// The shell renderer (index.html) forwards rect / navigation / device-emulation
// from the webapp iframe to these handlers; page state is sent back to the
// renderer, which relays it to the iframe. Steps are logged to crundi.log.
const BROWSER_MOBILE_UA = 'Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36';
const wbViews = new Map(); // id → WebContentsView
function _attachDbg(wc) { try { wc.debugger.attach('1.3'); } catch (e) { if (!/already attached/i.test(e.message)) throw e; } }
function wbCanGo(wc, dir) { try { const h = wc.navigationHistory; if (h) return dir === 'back' ? h.canGoBack() : h.canGoForward(); return dir === 'back' ? wc.canGoBack() : wc.canGoForward(); } catch { return false; } }
function wbEmitState(id, extra) {
  const view = wbViews.get(id); if (!view || !mainWindow || mainWindow.isDestroyed()) return;
  const wc = view.webContents;
  let url = '', title = '', loading = false;
  try { url = wc.getURL(); title = wc.getTitle(); loading = wc.isLoading(); } catch { /* ignore */ }
  const s = Object.assign({ id, url, title, loading, canGoBack: wbCanGo(wc, 'back'), canGoForward: wbCanGo(wc, 'forward') }, extra || {});
  try { mainWindow.webContents.send('wbrowser:state', s); } catch { /* ignore */ }
}

ipcMain.on('shell:log', (_e, m) => { try { appendLog('[shell] ' + m); } catch { /* ignore */ } });

ipcMain.handle('wbrowser:open', (_e, id, url) => {
  try {
    if (wbViews.has(id)) { wbEmitState(id); return { ok: true }; }
    if (!mainWindow || mainWindow.isDestroyed()) return { ok: false, error: 'no window' };
    const view = new WebContentsView({ webPreferences: { partition: 'persist:crundi-browser' } });
    wbViews.set(id, view);
    mainWindow.contentView.addChildView(view);
    view.setVisible(false);
    view.setBounds({ x: 0, y: 0, width: 800, height: 600 });
    const wc = view.webContents;
    appendLog(`[wbrowser] open id=${id} url=${url}`);
    wc.setWindowOpenHandler(({ url: u }) => { try { wc.loadURL(u); } catch { /* ignore */ } return { action: 'deny' }; });
    ['did-navigate', 'did-navigate-in-page', 'page-title-updated', 'did-stop-loading', 'did-start-loading'].forEach(ev => wc.on(ev, () => wbEmitState(id)));
    wc.on('did-finish-load', () => { appendLog(`[wbrowser] finished id=${id} ${wc.getURL()}`); wbEmitState(id); });
    wc.on('did-fail-load', (_ev, code, desc, validatedURL, isMainFrame) => {
      if (isMainFrame && code !== -3) { appendLog(`[wbrowser] fail id=${id} code=${code} ${desc} ${validatedURL}`); wbEmitState(id, { diag: `Load failed (${code}): ${desc}` }); }
    });
    wc.on('render-process-gone', (_ev, det) => appendLog(`[wbrowser] gone id=${id} ${det && det.reason}`));
    wc.loadURL(url || 'about:blank').catch(err => appendLog(`[wbrowser] loadURL err id=${id} ${err.message}`));
    return { ok: true };
  } catch (err) { appendLog(`[wbrowser] open ERROR ${err.message}`); return { ok: false, error: err.message }; }
});

ipcMain.handle('wbrowser:sync', (_e, id, rect, visible) => {
  const view = wbViews.get(id); if (!view) return { ok: false };
  try {
    if (!visible || !rect) { view.setVisible(false); return { ok: true }; }
    view.setBounds({ x: Math.round(rect.x), y: Math.round(rect.y), width: Math.max(1, Math.round(rect.width)), height: Math.max(1, Math.round(rect.height)) });
    view.setVisible(true);
    return { ok: true };
  } catch (err) { return { ok: false, error: err.message }; }
});

ipcMain.handle('wbrowser:navigate', (_e, id, url) => {
  const v = wbViews.get(id); if (!v) return { ok: false };
  try { appendLog(`[wbrowser] navigate id=${id} ${url}`); v.webContents.loadURL(url); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; }
});

ipcMain.handle('wbrowser:nav', (_e, id, dir) => {
  const v = wbViews.get(id); if (!v) return { ok: false };
  const wc = v.webContents;
  try {
    const h = wc.navigationHistory;
    if (dir === 'reload') wc.reload();
    else if (dir === 'back' && wbCanGo(wc, 'back')) { h ? h.goBack() : wc.goBack(); }
    else if (dir === 'forward' && wbCanGo(wc, 'forward')) { h ? h.goForward() : wc.goForward(); }
    return { ok: true };
  } catch (err) { return { ok: false, error: err.message }; }
});

// Device emulation via CDP. setDeviceMetricsOverride renders the page at the true
// device width/height (correct screen.width + media queries) and its `scale` param
// shrinks the resulting image to fit the view (sized to dev*scale) — no overflow
// scrollbars, no setZoomFactor conflict. `scale` updates on resize (cheap image
// rescale); the page is only reloaded when the device itself changes (so the site
// re-serves mobile/desktop markup).
const wbCfg = new Map(); // id → last applied config
ipcMain.handle('wbrowser:emulate', async (_e, id, config, scale) => {
  const v = wbViews.get(id); if (!v) return { ok: false }; const wc = v.webContents;
  const prev = wbCfg.get(id) || null;
  wbCfg.set(id, config || null);
  const deviceChanged = (!!config !== !!prev)
    || (config && prev && (config.width !== prev.width || config.height !== prev.height || config.mobile !== prev.mobile || config.ua !== prev.ua));
  try {
    if (config) {
      _attachDbg(wc);
      await wc.debugger.sendCommand('Emulation.setDeviceMetricsOverride', {
        width: config.width, height: config.height,
        deviceScaleFactor: config.deviceScaleFactor || 0, mobile: !!config.mobile,
        scale: scale || 1, screenWidth: config.width, screenHeight: config.height,
      });
      await wc.debugger.sendCommand('Emulation.setTouchEmulationEnabled', { enabled: !!config.mobile, maxTouchPoints: config.mobile ? 5 : 0 });
      await wc.debugger.sendCommand('Emulation.setEmitTouchEventsForMouse', { enabled: !!config.mobile, configuration: 'mobile' });
      if (deviceChanged) {
        await wc.debugger.sendCommand('Emulation.setUserAgentOverride', { userAgent: config.ua || (config.mobile ? BROWSER_MOBILE_UA : '') });
        appendLog(`[wbrowser] device id=${id} ${config.width}x${config.height} mobile=${config.mobile} scale=${(scale || 1).toFixed(3)} reload`);
        wc.reload();
      }
    } else {
      try { wc.setZoomFactor(1); } catch { /* ignore */ }
      if (wc.debugger.isAttached()) {
        try { await wc.debugger.sendCommand('Emulation.setEmitTouchEventsForMouse', { enabled: false }); } catch { /* ignore */ }
        try { await wc.debugger.sendCommand('Emulation.setTouchEmulationEnabled', { enabled: false, maxTouchPoints: 0 }); } catch { /* ignore */ }
        try { await wc.debugger.sendCommand('Emulation.setUserAgentOverride', { userAgent: '' }); } catch { /* ignore */ }
        try { await wc.debugger.sendCommand('Emulation.clearDeviceMetricsOverride'); } catch { /* ignore */ }
        try { wc.debugger.detach(); } catch { /* ignore */ }
      }
      if (deviceChanged) { appendLog(`[wbrowser] device id=${id} responsive reload`); wc.reload(); }
    }
    return { ok: true };
  } catch (err) { appendLog(`[wbrowser] emulate ERROR ${err.message}`); return { ok: false, error: err.message }; }
});

ipcMain.handle('wbrowser:devtools', (_e, id) => {
  const v = wbViews.get(id); if (!v) return { ok: false }; const wc = v.webContents;
  try { wc.isDevToolsOpened() ? wc.closeDevTools() : wc.openDevTools({ mode: 'detach' }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; }
});

ipcMain.handle('wbrowser:close', (_e, id) => {
  const v = wbViews.get(id); if (!v) return { ok: true };
  try { if (mainWindow && !mainWindow.isDestroyed()) mainWindow.contentView.removeChildView(v); } catch { /* ignore */ }
  try { v.webContents.close(); } catch { /* ignore */ }
  wbViews.delete(id); wbCfg.delete(id);
  appendLog(`[wbrowser] close id=${id}`);
  return { ok: true };
});

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
  if (process.platform === 'linux') {
    // Files copied in a Linux file manager are a list of file:// addresses.
    try {
      const raw = clipboard.readBuffer('text/uri-list').toString('utf8');
      return raw.split(/\r?\n/).filter((l) => l.startsWith('file://')).map((l) => { try { return fileURLToPath(l.trim()); } catch { return ''; } }).filter(Boolean);
    } catch { /* nothing copied */ }
  }
  return [];
});

// `require` for the CommonJS-only packages loaded further down (ws).
const require = createRequire(import.meta.url);

// ─── Browser IPC (hidden BrowserWindows for MCP browser tools) ───

const browserWindows = new Map(); // key → BrowserWindow

// A single hung renderer call (blocked dialog, heavy JS, stalled navigation)
// must never wedge the whole tool. Race every handler against a timeout that is
// shorter than the MCP-side IPC backstop, so we always reply with a clean error.
function withTimeout(promise, ms, label) {
  let timer;
  const guard = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms (page may be stalled or showing a dialog)`)), ms);
  });
  return Promise.race([promise, guard]).finally(() => clearTimeout(timer));
}

// Load a URL without hanging on pages that never fire did-finish-load (websockets,
// long-polling, slow third-party resources). Resolve as soon as the page stops
// loading, or after a grace period (treat a slow page as "open, just slow"), and
// only hard-fail on a real main-frame error.
function loadUrlWithGrace(wc, url, graceMs = 15000) {
  return new Promise((resolve, reject) => {
    let done = false;
    const finish = (fn) => { if (done) return; done = true; cleanup(); fn(); };
    const onStop = () => finish(resolve);
    const onFinish = () => finish(resolve);
    const onFail = (_e, code, desc, _vurl, isMainFrame) => {
      if (isMainFrame && code !== -3 /* ERR_ABORTED, fired on redirects/SPAs */) {
        finish(() => reject(new Error(`Load failed (${code}): ${desc}`)));
      }
    };
    const timer = setTimeout(() => finish(resolve), graceMs);
    function cleanup() {
      clearTimeout(timer);
      wc.off('did-stop-loading', onStop);
      wc.off('did-finish-load', onFinish);
      wc.off('did-fail-load', onFail);
    }
    wc.on('did-stop-loading', onStop);
    wc.on('did-finish-load', onFinish);
    wc.on('did-fail-load', onFail);
    // Events drive resolution; ignore the promise (it rejects with ERR_ABORTED on redirects).
    wc.loadURL(url).catch(() => {});
  });
}

function browserActionTimeout(msg) {
  // browser_wait runs up to msg.timeout in-page; give it that plus a small buffer.
  if (msg.type === 'browserWait') return (msg.timeout || 30000) + 3000;
  // Navigation can legitimately take a while to finish loading.
  if (msg.type === 'browserOpen' || msg.type === 'browserNavigate') return 30000;
  return 25000;
}

// Reply channel is a parameter, not a hard-wired botProcess.send: the same
// handlers now serve a server we spawned ourselves AND a remote server that has
// borrowed this app as its native host.
/**
 * A notification pushed from the server.
 *
 * This exists because the desktop app cannot receive Web Push - Electron's
 * Chromium has no push service - so the server sends it down the socket the
 * app already holds and it is shown natively here.
 */
// Shown notifications, held until clicked or closed. A Notification only
// referenced from inside showServerNotification is collected once it is on
// screen, and its click handler with it: clicking it then did nothing, which
// is what it did on Ubuntu.
const liveNotifications = new Set();

function showServerNotification(msg) {
  try {
    if (!Notification.isSupported()) { appendLog('[notify] not supported on this OS'); return; }
    const n = new Notification({
      title: String(msg.title || 'Crundi').slice(0, 120),
      body: String(msg.body || '').slice(0, 400),
    });
    liveNotifications.add(n);
    const done = () => liveNotifications.delete(n);
    // The chat it is about, when the server says (older servers do not).
    const chat = /^\/chat\/([A-Za-z0-9_-]{1,80})$/.exec(String(msg.url || ''));
    n.on('click', () => {
      done();
      try { bringToFront(); } catch { /* tray-only is fine */ }
      if (chat) openChatInWindow(chat[1]);
    });
    n.on('close', done);
    // Belt and braces: a notification server that never reports either.
    setTimeout(done, 6 * 60 * 60 * 1000).unref?.();
    n.show();
  } catch (err) {
    appendLog(`[notify] could not show: ${err?.message || err}`);
  }
}

function handleBotMessage(msg, reply) {
  if (!msg || !msg.type) return;
  const send = reply || ((payload) => botProcess?.send(payload));

  // Fire-and-forget: no requestId, nothing to reply to.
  if (msg.type === 'notify') { showServerNotification(msg); return; }
  // The server wants a fresh start (to apply a restored backup). Stopping it
  // and starting it again from here is the only way it can get one.
  if (msg.type === 'restart-server') {
    if (!botProcess) return;
    restartRequested = true;
    try { botProcess.kill(); } catch { restartRequested = false; }
    return;
  }

  // Browser IPC
  if (!msg.requestId) return;
  const handler = browserIpcHandlers[msg.type];
  if (handler) {
    withTimeout(handler(msg), browserActionTimeout(msg), msg.type).then(result => {
      send({ type: 'browserResult', requestId: msg.requestId, ...result });
    }).catch(err => {
      send({ type: 'browserResult', requestId: msg.requestId, ok: false, error: err.message });
    });
  }
}

/**
 * Serve a REMOTE server that has registered this app as its native host.
 *
 * Deliberately narrower than the parent-process channel: only the browser
 * handlers are exposed. Anything that runs commands belongs where that
 * server's code lives, not on the laptop looking at it — bridging it here would
 * silently run commands on the wrong machine.
 */
function handleNativeHostMessage(msg, reply) {
  if (!msg || !msg.type) return;
  // Notifications carry no requestId and are not browser calls, so they have
  // to be let through before the request-shaped checks below.
  if (msg.type === 'notify') { showServerNotification(msg); return; }
  if (!msg.requestId) return;
  if (!browserIpcHandlers[msg.type]) return;
  handleBotMessage(msg, reply);
}

// ─── Native host client ───
//
// In remote mode we are the GUI for a server that has none. It cannot dial out
// to us, so we open the socket and register; from then on its browser requests
// arrive here and are answered by the same handlers the local server uses.
//
// The token comes from the page itself once the user has logged in, rather than
// being stored separately: the web UI already owns that session and refreshes
// it, and a second credential to keep in sync would only drift.
let hostWs = null;
let hostToken = null;
let hostRetry = null;
let hostClosing = false;

function nativeHostStatus() {
  const cfg = readClientConfig();
  return {
    mode: cfg.mode,
    serverUrl: cfg.serverUrl,
    connected: !!(hostWs && hostWs.readyState === 1),
    serverBundled,
  };
}

function stopNativeHost() {
  hostClosing = true;
  clearTimeout(hostRetry); hostRetry = null;
  try { hostWs?.close(); } catch { /* ignore */ }
  hostWs = null;
  hostClosing = false;
}

function connectNativeHost() {
  const cfg = readClientConfig();
  if (cfg.mode !== 'remote' || !cfg.serverUrl || !hostToken) return;
  if (hostWs && (hostWs.readyState === 0 || hostWs.readyState === 1)) return;

  let WSImpl;
  try { WSImpl = require('ws'); } catch (err) {
    appendLog(`[nativehost] ws module unavailable: ${err.message}`);
    return;
  }
  const wsUrl = cfg.serverUrl.replace(/^http/, 'ws') + '/ws?token=' + encodeURIComponent(hostToken);
  appendLog(`[nativehost] connecting to ${cfg.serverUrl}`);
  // Every handler below talks to THIS socket, never to whatever `hostWs`
  // happens to hold when it fires. Saving the server URL closes the in-flight
  // socket and starts another, but a TLS handshake already past the point of
  // no return still emits 'open' afterwards — and the old code then called
  // hostWs.send() on a hostWs that stopNativeHost() had just set to null,
  // which crashed the whole main process with an uncaught TypeError.
  let ws;
  try { ws = new WSImpl(wsUrl); } catch (err) {
    appendLog(`[nativehost] connect failed: ${err.message}`);
    return scheduleHostRetry();
  }
  hostWs = ws;
  const isCurrent = () => hostWs === ws;

  ws.on('open', () => {
    // Superseded while connecting: close quietly and let the live one register.
    if (!isCurrent()) { try { ws.close(); } catch { /* ignore */ } return; }
    appendLog('[nativehost] connected — registering as native host');
    // Tell the server what this build can do, so it does not offer the user a
    // notification channel that an older app would silently drop.
    try { ws.send(JSON.stringify({ type: 'register-native-host', capabilities: ['notify'] })); }
    catch (err) { appendLog(`[nativehost] register failed: ${err.message}`); }
  });
  ws.on('message', (raw) => {
    if (!isCurrent()) return;
    let m; try { m = JSON.parse(raw.toString()); } catch { return; }
    if (m.type === 'native-host-ready') { appendLog('[nativehost] registered'); return; }
    if (m.type !== 'native-host') return;
    handleNativeHostMessage(m.payload, (payload) => {
      if (ws.readyState !== 1) return;
      try { ws.send(JSON.stringify({ type: 'native-host-result', payload })); }
      catch (err) { appendLog(`[nativehost] reply failed: ${err.message}`); }
    });
  });
  ws.on('close', () => {
    // An old socket closing must not null out the connection that replaced it.
    if (!isCurrent()) return;
    hostWs = null;
    if (!hostClosing) { appendLog('[nativehost] disconnected'); scheduleHostRetry(); }
  });
  ws.on('error', (err) => appendLog(`[nativehost] socket error: ${err.message}`));
}

function scheduleHostRetry() {
  clearTimeout(hostRetry);
  // The server may simply be restarting; keep trying, but not in a tight loop.
  hostRetry = setTimeout(connectNativeHost, 5000);
}

const browserIpcHandlers = {
  async browserOpen(msg) {
    const { key, url, width, height } = msg;
    if (browserWindows.has(key)) return { ok: false, error: `Browser "${key}" already open` };
    if (browserWindows.size >= 20) return { ok: false, error: 'Too many browser instances (max 20)' };
    const win = new BrowserWindow({
      width: width || 1280, height: height || 720,
      show: false,
      // Isolated, in-memory session per instance so network capture (and cookies)
      // don't bleed between browsers or into the main app's default session.
      webPreferences: { contextIsolation: true, nodeIntegration: false, partition: `mcp-browser-${key}` },
    });
    win.webContents.on('console-message', (_e, _level, message) => {
      const inst = browserWindows.get(key);
      if (inst) inst.consoleLogs.push({ time: Date.now(), text: message });
    });

    // Neutralize JS dialogs. On a hidden window, alert()/confirm()/prompt() and
    // beforeunload would block the renderer forever with no way to dismiss them,
    // wedging every subsequent call. Cancel unload prompts (a real main-process
    // event) and override the dialog functions in the page's main world on load.
    win.webContents.on('will-prevent-unload', (e) => e.preventDefault());
    const suppressDialogs = () => {
      win.webContents.executeJavaScript(`(function(){try{
        window.alert=function(){};window.confirm=function(){return true;};
        window.prompt=function(){return null;};window.onbeforeunload=null;
      }catch(_){}})();`, true).catch(() => {});
    };
    win.webContents.on('did-start-loading', suppressDialogs);
    win.webContents.on('dom-ready', suppressDialogs);

    const ses = win.webContents.session;
    const entry = { win, ses, consoleLogs: [], networkEntries: [], recording: false };
    browserWindows.set(key, entry);

    // Network request capture via webRequest (on this instance's own session)
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
      await loadUrlWithGrace(win.webContents, url);
      return { ok: true };
    } catch (err) {
      browserWindows.delete(key);
      try { ses.webRequest.onBeforeSendHeaders(null); ses.webRequest.onCompleted(null); ses.webRequest.onErrorOccurred(null); } catch { /* ignore */ }
      try { win.destroy(); } catch { /* ignore */ }
      return { ok: false, error: err.message };
    }
  },

  async browserClose(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    // Release the per-instance webRequest listeners so closures don't pin the
    // destroyed window's session.
    try { entry.ses.webRequest.onBeforeSendHeaders(null); entry.ses.webRequest.onCompleted(null); entry.ses.webRequest.onErrorOccurred(null); } catch { /* ignore */ }
    try { entry.win.destroy(); } catch { /* ignore */ }
    browserWindows.delete(msg.key);
    return { ok: true };
  },

  async browserNavigate(msg) {
    const entry = browserWindows.get(msg.key);
    if (!entry) return { ok: false, error: 'Browser not found' };
    await loadUrlWithGrace(entry.win.webContents, msg.url);
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
        // innerText on the LIVE body, not a clone. A cloned node is detached,
        // so it has no layout, so innerText silently degrades to textContent
        // and every line break is lost ("HeadingFirst para.Second para.").
        // Live innerText also already omits script/style/noscript, because
        // they are not rendered — the clone-and-remove dance was doing damage
        // to solve a problem that did not exist.
        return (document.body.innerText || '').substring(0, 50000);
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

// One instance per install. The lock is keyed on the app id, so the client and
// the all-in-one each get their own and can still run side by side — but a
// second copy of the SAME build would fight the first over client.json, the
// tray icon and the native-host socket. Hand the focus back instead.
if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on('second-instance', () => {
    appendLog('[lifecycle] Second instance attempted — focusing this one');
    showWindow();
  });
}

// A stray async throw — a socket firing after its owner was torn down, say —
// used to take the whole app down behind a modal "A JavaScript error occurred
// in the main process". Losing a running session to that is worse than
// carrying on: log it where the user (and we) can see it, and stay up.
process.on('uncaughtException', (err) => {
  appendLog(`[main] UNCAUGHT: ${err?.stack || err}`);
});
process.on('unhandledRejection', (reason) => {
  appendLog(`[main] UNHANDLED REJECTION: ${reason?.stack || reason}`);
});

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
  try {
    setupAutoUpdate();
    setTimeout(() => checkForUpdates(), 8000);                  // shortly after launch
    setInterval(() => checkForUpdates(), 24 * 60 * 60 * 1000);  // once every day
    appendLog('[lifecycle] Auto-update scheduled (launch + daily)');
  } catch (err) {
    appendLog(`[lifecycle] Auto-update setup error: ${err.message}`);
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
