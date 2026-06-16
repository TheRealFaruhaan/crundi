const { contextBridge, ipcRenderer, webUtils } = require('electron');

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
  maximize: () => ipcRenderer.invoke('window:maximize'),
  close: () => ipcRenderer.invoke('window:close'),
  isMaximized: () => ipcRenderer.invoke('window:isMaximized'),

  getClipboardImage: () => ipcRenderer.invoke('clipboard:getImage'),
  getClipboardFilePaths: () => ipcRenderer.invoke('clipboard:getFilePaths'),

  // DisconnectRDP scheduled task (locked-screen capture)
  getRdpStatus: () => ipcRenderer.invoke('rdp:status'),
  setupRdp: () => ipcRenderer.invoke('rdp:setup'),

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
