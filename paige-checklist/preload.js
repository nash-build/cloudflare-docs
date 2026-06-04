'use strict';

const { contextBridge, ipcRenderer } = require('electron');

// Minimal, explicit API surface exposed to the renderer. No raw Node access.
contextBridge.exposeInMainWorld('api', {
  loadChecklist: () => ipcRenderer.invoke('checklist:load'),
  saveChecklist: (items) => ipcRenderer.invoke('checklist:save', items),
  getConfig: () => ipcRenderer.invoke('config:get'),
  pushPhone: (payload) => ipcRenderer.invoke('push:send', payload),
  closeWindow: () => ipcRenderer.send('window:close'),
  alertWindow: () => ipcRenderer.send('window:alert'),
  onPaigeToggle: (handler) => ipcRenderer.on('paige:toggle', () => handler()),
});
