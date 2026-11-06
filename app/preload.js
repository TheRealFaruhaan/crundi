const { contextBridge, ipcRenderer, webUtils } = require('electron');

// Audio from the page, as a Float32Array whatever it arrived as (the bridge
// copies typed arrays; plain arrays work too).
const toF32 = (x) => (x instanceof Float32Array ? x : new Float32Array(x && typeof x.length === 'number' ? x : 0));

contextBridge.exposeInMainWorld('api', {
  getPathForFile: (file) => { try { return webUtils.getPathForFile(file); } catch { return ''; } },
  getBotStatus: () => ipcRenderer.invoke('bot:getStatus'),
  getBotLogs: () => ipcRenderer.invoke('bot:getLogs'),
  getLogPath: () => ipcRenderer.invoke('bot:getLogPath'),
  startBot: () => ipcRenderer.invoke('bot:start'),
  stopBot: () => ipcRenderer.invoke('bot:stop'),
  restartBot: () => ipcRenderer.invoke('bot:restart'),

  checkSetup: () => ipcRenderer.invoke('setup:check'),
  saveSetup: (config) => ipcRenderer.invoke('setup:save', config),
  browseDir: () => ipcRenderer.invoke('setup:browse'),
  onShowSetup: (cb) => {
    ipcRenderer.on('show:setup', (_, oldConfig) => cb(oldConfig));
    return () => ipcRenderer.removeAllListeners('show:setup');
  },

  minimize: () => ipcRenderer.invoke('window:minimize'),
  showWindow: () => ipcRenderer.invoke('window:show'),
  maximize: () => ipcRenderer.invoke('window:maximize'),
  close: () => ipcRenderer.invoke('window:close'),
  isMaximized: () => ipcRenderer.invoke('window:isMaximized'),

  getClipboardImage: () => ipcRenderer.invoke('clipboard:getImage'),
  getClipboardFilePaths: () => ipcRenderer.invoke('clipboard:getFilePaths'),

  // DisconnectRDP scheduled task (locked-screen capture)
  getRdpStatus: () => ipcRenderer.invoke('rdp:status'),
  setupRdp: () => ipcRenderer.invoke('rdp:setup'),

  // Which server this app is attached to, and whether we are acting as its
  // native host (lending it this machine's GUI for the browser panel).
  getClientConfig: () => ipcRenderer.invoke('client:getConfig'),
  setClientConfig: (cfg) => ipcRenderer.invoke('client:setConfig', cfg),
  setServerToken: (token) => ipcRenderer.send('client:setToken', String(token || '')),
  onShowServer: (cb) => {
    ipcRenderer.on('show:server', () => cb());
    return () => ipcRenderer.removeAllListeners('show:server');
  },

  // Diagnostic: let the shell renderer append to crundi.log.
  shellLog: (m) => ipcRenderer.send('shell:log', String(m)),

  // Auto-update (electron-updater) — only meaningful in the installed desktop app.
  getUpdateState: () => ipcRenderer.invoke('update:getState'),
  setAutoUpdate: (enabled) => ipcRenderer.invoke('update:setEnabled', !!enabled),
  checkForUpdates: () => ipcRenderer.invoke('update:check'),
  installUpdate: () => ipcRenderer.invoke('update:install'),
  setUpdateChannel: (ch) => ipcRenderer.invoke('update:setChannel', ch === 'dev' ? 'dev' : 'production'),
  setStartup: (enabled) => ipcRenderer.invoke('startup:set', !!enabled),
  onUpdateStatus: (cb) => { const h = (_e, s) => cb(s); ipcRenderer.on('update:status', h); return () => ipcRenderer.removeListener('update:status', h); },

  // "Hey Cru" while plugged in (D40), the PC's own wake word. get() is the
  // setting; set(on) saves it and returns the status; status() is
  // { enabled, state, onBattery, installed, error, lastHeard } with state one
  // of off | installing | starting | listening | on-battery | voice-open |
  // paused | error | unavailable.
  voiceWake: {
    get: () => ipcRenderer.invoke('voice-wake:get'),
    set: (on) => ipcRenderer.invoke('voice-wake:set', !!on),
    status: () => ipcRenderer.invoke('voice-wake:status'),
    onStatus: (cb) => { const h = (_e, s) => cb(s); ipcRenderer.on('voice-wake:status', h); return () => ipcRenderer.removeListener('voice-wake:status', h); },
    // Training it from the owner's voice. The page records 16 kHz mono
    // Float32Arrays itself, after trainBegin() (which frees the microphone);
    // check(samples) -> { ok, why, ms } for each; train([five]) ->
    // { ok: true, trainedAt } | { ok: false, why, redo: [indexes] };
    // trainEnd() when done or cancelled; forget() deletes the profile.
    // status() also carries { trained, trainedAt, training }.
    check: (samples) => ipcRenderer.invoke('voice-wake:check', toF32(samples)),
    trainBegin: () => ipcRenderer.invoke('voice-wake:train-begin'),
    trainEnd: () => ipcRenderer.invoke('voice-wake:train-end'),
    train: (recordings) => ipcRenderer.invoke('voice-wake:train', Array.from(recordings || [], toF32)),
    forget: () => ipcRenderer.invoke('voice-wake:forget'),
  },

  // Interactive browser panel — WebContentsView managed in the main process.
  wbrowser: {
    open: (id, url) => ipcRenderer.invoke('wbrowser:open', id, url),
    sync: (id, rect, visible) => ipcRenderer.invoke('wbrowser:sync', id, rect, visible),
    navigate: (id, url) => ipcRenderer.invoke('wbrowser:navigate', id, url),
    nav: (id, dir) => ipcRenderer.invoke('wbrowser:nav', id, dir),
    emulate: (id, config, scale) => ipcRenderer.invoke('wbrowser:emulate', id, config, scale),
    devtools: (id) => ipcRenderer.invoke('wbrowser:devtools', id),
    close: (id) => ipcRenderer.invoke('wbrowser:close', id),
    onState: (cb) => { const h = (_e, s) => cb(s); ipcRenderer.on('wbrowser:state', h); return () => ipcRenderer.removeListener('wbrowser:state', h); },
  },

  onBotStatus: (cb) => {
    ipcRenderer.on('bot:status', (_, status) => cb(status));
    return () => ipcRenderer.removeAllListeners('bot:status');
  },
  onBotLog: (cb) => {
    ipcRenderer.on('bot:log', (_, entry) => cb(entry));
    return () => ipcRenderer.removeAllListeners('bot:log');
  },
  onWebappReady: (cb) => {
    ipcRenderer.on('webapp:ready', (_, url) => cb(url));
    return () => ipcRenderer.removeAllListeners('webapp:ready');
  },
  onWebappStopped: (cb) => {
    ipcRenderer.on('webapp:stopped', () => cb());
    return () => ipcRenderer.removeAllListeners('webapp:stopped');
  },
});
