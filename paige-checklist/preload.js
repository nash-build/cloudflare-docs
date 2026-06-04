'use strict';

const { contextBridge, ipcRenderer } = require('electron');

// Minimal, explicit API surface exposed to the renderer. No raw Node access.
contextBridge.exposeInMainWorld('api', {
  loadChecklist: () => ipcRenderer.invoke('checklist:load'),
  saveChecklist: (items) => ipcRenderer.invoke('checklist:save', items),
  getConfig: () => ipcRenderer.invoke('config:get'),
  closeWindow: () => ipcRenderer.send('window:close'),
  onPaigeToggle: (handler) => ipcRenderer.on('paige:toggle', () => handler()),
});
