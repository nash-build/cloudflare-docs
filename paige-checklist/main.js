'use strict';

const { app, BrowserWindow, ipcMain, screen, globalShortcut } = require('electron');
const fs = require('fs');
const path = require('path');

// ---------------------------------------------------------------------------
// Persistence: the checklist lives in a JSON file in the OS user-data folder
// so it survives restarts. (On macOS: ~/Library/Application Support/paige-checklist)
// ---------------------------------------------------------------------------
const DATA_FILE = () => path.join(app.getPath('userData'), 'checklist.json');

function loadItems() {
  try {
    const raw = fs.readFileSync(DATA_FILE(), 'utf8');
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function saveItems(items) {
  try {
    fs.writeFileSync(DATA_FILE(), JSON.stringify(items, null, 2), 'utf8');
    return true;
  } catch (err) {
    console.error('Failed to save checklist:', err);
    return false;
  }
}

// ---------------------------------------------------------------------------
// Config: ElevenLabs agent id / Picovoice key etc. live in config.json
// (gitignored). config.example.json documents the shape.
// ---------------------------------------------------------------------------
function loadConfig() {
  const candidates = [
    path.join(app.getPath('userData'), 'config.json'),
    path.join(__dirname, 'config.json'),
  ];
  for (const file of candidates) {
    try {
      return JSON.parse(fs.readFileSync(file, 'utf8'));
    } catch {
      /* try next */
    }
  }
  return {};
}

const WIN_WIDTH = 340;
const WIN_HEIGHT = 520;
const MARGIN = 16;

let win = null;

function positionTopRight(targetWindow) {
  const { workArea } = screen.getDisplayNearestPoint(screen.getCursorScreenPoint());
  const x = workArea.x + workArea.width - WIN_WIDTH - MARGIN;
  const y = workArea.y + MARGIN;
  targetWindow.setBounds({ x, y, width: WIN_WIDTH, height: WIN_HEIGHT });
}

function createWindow() {
  win = new BrowserWindow({
    width: WIN_WIDTH,
    height: WIN_HEIGHT,
    frame: false,
    transparent: true,
    resizable: false,
    movable: true,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    skipTaskbar: true,
    hasShadow: false,
    // Keep it above everything, including other apps' fullscreen windows.
    alwaysOnTop: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  // "screen-saver" is the highest standard level — floats above normal and
  // most fullscreen windows. Combined with visibleOnAllWorkspaces this keeps
  // the checklist in the front layer no matter what app is focused.
  win.setAlwaysOnTop(true, 'screen-saver');
  win.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });

  positionTopRight(win);
  win.loadFile(path.join(__dirname, 'renderer', 'index.html'));

  // Grant microphone access to the renderer (needed for Paige + wake word).
  win.webContents.session.setPermissionRequestHandler((_wc, permission, callback) => {
    callback(permission === 'media' || permission === 'audioCapture');
  });

  // Re-pin to top-right if the display configuration changes.
  screen.on('display-metrics-changed', () => win && positionTopRight(win));
}

// ---------------------------------------------------------------------------
// IPC bridge used by the renderer (see preload.js)
// ---------------------------------------------------------------------------
ipcMain.handle('checklist:load', () => loadItems());
ipcMain.handle('checklist:save', (_evt, items) => saveItems(items));
ipcMain.handle('config:get', () => loadConfig());
ipcMain.on('window:close', () => app.quit());
ipcMain.on('window:alert', () => {
  if (!win) return;
  // Pull the overlay to the very front and bounce the Dock for a reminder.
  win.show();
  win.setAlwaysOnTop(true, 'screen-saver');
  win.moveTop();
  if (app.dock) app.dock.bounce('critical');
});

app.whenReady().then(() => {
  createWindow();

  // Global hotkey fallback to start/stop Paige without the wake word
  // (handy in Electron where browser speech recognition can be flaky).
  globalShortcut.register('CommandOrControl+Shift+P', () => {
    if (win) win.webContents.send('paige:toggle');
  });

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => app.quit());
app.on('will-quit', () => globalShortcut.unregisterAll());
