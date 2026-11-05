// Preload for the hidden "Hey Crundi" window (sandboxed, context isolated).
// The page gets three calls and nothing else.
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('voiceWake', {
  pcm: (samples) => { if (samples instanceof Float32Array) ipcRenderer.send('voice-wake:pcm', samples); },
  state: (state, detail) => ipcRenderer.send('voice-wake:state', String(state || ''), String(detail || '')),
  onCommand: (cb) => { ipcRenderer.on('voice-wake:command', (_e, cmd) => cb(String(cmd))); },
});
