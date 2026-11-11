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

  // Homepool: lend this PC's connection to the server as a node. The page
  // gets the node key from the server (it is signed in) and hands it here;
  // the main process keeps it and runs the node (app/homepool-node.js).
  homepool: {
    status: () => ipcRenderer.invoke('homepool:status'),
    share: (cfg) => ipcRenderer.invoke('homepool:share', cfg),
    start: () => ipcRenderer.invoke('homepool:start'),
    stop: () => ipcRenderer.invoke('homepool:stop'),
    forget: () => ipcRenderer.invoke('homepool:forget'),
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

  // Kid mode (crundi-voice.js): lock this window to the kid screen (kiosk,
  // above everything, keys swallowed) or let it go once the PIN was accepted;
  // onKidStart: the tray's "Kid mode" item.
  kidLock: (on) => ipcRenderer.invoke('kid:lock', !!on),
  onKidStart: (cb) => { const h = () => cb(); ipcRenderer.on('kid:start', h); return () => ipcRenderer.removeListener('kid:start', h); },
  // The PC lock: the shortcuts (Ctrl+Shift+L visible, Super+Shift+L full) and the tray; the window locks with kidLock.
  onPcLock: (cb) => { const h = (_e, mode, by) => cb(mode, by); ipcRenderer.on('pc-lock', h); return () => ipcRenderer.removeListener('pc-lock', h); },
  // Auto lock: { on, minutes, mode }, from GET /api/lock (the app watches how long the PC is idle).
  setAutoLock: (a) => ipcRenderer.invoke('pc-lock:auto', a),

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
    // The browser drawer (app/browser-drawer.js): the web UI holds the
    // <webview>s, the main process holds their webContents and the debugger.
    // Its presence is how a newer web UI knows this app has the drawer.
    drawer: {
      ready: () => ipcRenderer.send('drawer:ready'),
      attach: (a) => ipcRenderer.invoke('drawer:attach', a),
      detach: (tabId) => ipcRenderer.invoke('drawer:detach', tabId),
      navigate: (tabId, url) => ipcRenderer.invoke('drawer:navigate', tabId, url),
      nav: (tabId, dir) => ipcRenderer.invoke('drawer:nav', tabId, dir),
      emulate: (tabId, config, scale) => ipcRenderer.invoke('drawer:emulate', tabId, config, scale),
      devtools: (tabId) => ipcRenderer.invoke('drawer:devtools', tabId),
      capture: (tabId) => ipcRenderer.invoke('drawer:capture', tabId),
      describe: (tabId, points) => ipcRenderer.invoke('drawer:describe', tabId, points),
      info: () => ipcRenderer.invoke('drawer:info'),
      // Which tab is on screen; the others fall asleep after a while (app/browser-drawer.js).
      visible: (tabId, visible) => ipcRenderer.invoke('drawer:visible', tabId, visible),
      // The drawer's history (one per client, shared by every tab), and clearing it all.
      history: (q, limit) => ipcRenderer.invoke('drawer:history', q, limit),
      clear: (what) => ipcRenderer.invoke('drawer:clear', what),
      onInfo: (cb) => { const h = (_e, i) => cb(i); ipcRenderer.on('drawer:info', h); return () => ipcRenderer.removeListener('drawer:info', h); },
      onState: (cb) => { const h = (_e, s) => cb(s); ipcRenderer.on('drawer:state', h); return () => ipcRenderer.removeListener('drawer:state', h); },
      // Requests from the main process (the server's chats acting on a tab).
      onRequest: (cb) => { const h = (_e, r) => cb(r); ipcRenderer.on('drawer:request', h); return () => ipcRenderer.removeListener('drawer:request', h); },
      reply: (reqId, result) => ipcRenderer.send('drawer:reply', reqId, result),
    },
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
