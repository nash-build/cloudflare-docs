// Paige Checklist — renderer
// Loads the ElevenLabs Conversational AI client from a CDN (ESM). The voice
// agent "Paige" is wired to client tools that read & mutate the checklist.
import { Conversation } from 'https://cdn.jsdelivr.net/npm/@elevenlabs/client/+esm';

const listEl = document.getElementById('list');
const addForm = document.getElementById('add-form');
const addInput = document.getElementById('add-input');
const micBtn = document.getElementById('mic-btn');
const closeBtn = document.getElementById('close-btn');
const statusText = document.getElementById('status-text');
const paigeDot = document.getElementById('paige-dot');

let items = []; // { id, text, done, remindAt, notified }
let listVersion = 0; // server sync version (0 = local-only / not yet synced)

// ---------------------------------------------------------------------------
// State + persistence (local file + optional Cloudflare backend sync)
// ---------------------------------------------------------------------------
const uid = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 6);

let syncCfg = null; // { url, token } when configured

function syncHeaders() {
  return { 'content-type': 'application/json', authorization: 'Bearer ' + syncCfg.token };
}

// Push the whole list to the backend (last-write-wins; updates listVersion).
async function pushRemote() {
  if (!syncCfg) return;
  try {
    const res = await fetch(syncCfg.url.replace(/\/+$/, '') + '/list', {
      method: 'PUT',
      headers: syncHeaders(),
      body: JSON.stringify({ items }),
    });
    if (res.ok) {
      const state = await res.json();
      listVersion = state.version || listVersion;
    }
  } catch (err) {
    console.error('Sync push failed:', err);
  }
}

// Pull from the backend; adopt it if it is newer than what we have locally.
async function pullRemote() {
  if (!syncCfg) return;
  try {
    const res = await fetch(syncCfg.url.replace(/\/+$/, '') + '/list', { headers: syncHeaders() });
    if (!res.ok) return;
    const state = await res.json();
    if ((state.version || 0) > listVersion) {
      listVersion = state.version;
      items = Array.isArray(state.items) ? state.items : [];
      render();
      await window.api.saveChecklist(items); // keep local cache warm
    }
  } catch (err) {
    console.error('Sync pull failed:', err);
  }
}

async function persist() {
  await window.api.saveChecklist(items);
  pushRemote(); // fire-and-forget to the backend
}

function setStatus(msg) {
  statusText.textContent = msg;
}

// Turn a reminder spec into an absolute epoch (ms). Accepts a number of
// minutes from now, or an ISO/parseable date string. Returns null if neither.
function resolveWhen({ in_minutes, when } = {}) {
  if (in_minutes != null && !Number.isNaN(Number(in_minutes))) {
    return Date.now() + Number(in_minutes) * 60_000;
  }
  if (when) {
    const t = Date.parse(when);
    if (!Number.isNaN(t)) return t;
  }
  return null;
}

function formatWhen(ts) {
  const d = new Date(ts);
  const today = new Date();
  const sameDay = d.toDateString() === today.toDateString();
  const time = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  return sameDay ? time : `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })} ${time}`;
}

// Resolve an item by free-text (used by voice tools — fuzzy, case-insensitive).
function findItem(query) {
  if (!query) return null;
  const q = query.trim().toLowerCase();
  return (
    items.find((i) => i.text.toLowerCase() === q) ||
    items.find((i) => i.text.toLowerCase().includes(q)) ||
    items.find((i) => q.includes(i.text.toLowerCase())) ||
    null
  );
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------
function render() {
  listEl.innerHTML = '';
  if (items.length === 0) {
    const li = document.createElement('li');
    li.className = 'empty';
    li.textContent = 'No items yet. Add one below or ask Paige.';
    listEl.appendChild(li);
    return;
  }
  for (const item of items) {
    const li = document.createElement('li');
    li.className = 'item' + (item.done ? ' done' : '');
    li.dataset.id = item.id;

    const box = document.createElement('span');
    box.className = 'checkbox';
    box.title = 'Toggle complete';
    box.addEventListener('click', () => toggleItem(item.id));

    const label = document.createElement('span');
    label.className = 'item-label';
    label.textContent = item.text;

    li.append(box, label);

    if (item.remindAt && !item.done) {
      const overdue = item.remindAt <= Date.now();
      if (overdue) li.classList.add('due');
      const badge = document.createElement('span');
      badge.className = 'reminder' + (overdue ? ' overdue' : '');
      badge.textContent = (overdue ? '⏰ ' : '🔔 ') + formatWhen(item.remindAt);
      badge.title = 'Reminder — click to clear';
      badge.addEventListener('click', () => clearReminder(item.id));
      li.appendChild(badge);
    }

    const del = document.createElement('button');
    del.className = 'delete-btn';
    del.textContent = '🗑';
    del.title = 'Remove';
    del.addEventListener('click', () => removeItem(item.id));

    li.appendChild(del);
    listEl.appendChild(li);
  }
}

// ---------------------------------------------------------------------------
// Mutations (shared by UI clicks and Paige's voice tools)
// ---------------------------------------------------------------------------
async function addItem(text) {
  const clean = (text || '').trim();
  if (!clean) return null;
  const item = { id: uid(), text: clean, done: false };
  items.push(item);
  render();
  await persist();
  return item;
}

async function removeItem(id) {
  items = items.filter((i) => i.id !== id);
  render();
  await persist();
}

async function toggleItem(id, forceDone) {
  const item = items.find((i) => i.id === id);
  if (!item) return null;
  item.done = typeof forceDone === 'boolean' ? forceDone : !item.done;
  render();
  await persist();
  return item;
}

async function setReminder(id, ts) {
  const item = items.find((i) => i.id === id);
  if (!item) return null;
  item.remindAt = ts;
  item.notified = false;
  render();
  await persist();
  return item;
}

async function clearReminder(id) {
  const item = items.find((i) => i.id === id);
  if (!item) return null;
  item.remindAt = null;
  item.notified = false;
  render();
  await persist();
  return item;
}

// ---------------------------------------------------------------------------
// Reminder scheduler + native notifications (executive-assistant behaviour)
// ---------------------------------------------------------------------------
function fireReminder(item) {
  // Bring the overlay forward and flash the item.
  window.api.alertWindow();
  render();
  try {
    const n = new Notification('⏰ Reminder', {
      body: item.text,
      requireInteraction: true,
      silent: false,
    });
    n.onclick = () => window.api.alertWindow();
  } catch (err) {
    console.error('Notification failed:', err);
  }
  // Also push to the iPhone (no-ops if push isn't configured).
  window.api.pushPhone({ title: 'Reminder', body: item.text });
  setStatus(`Reminder: ${item.text}`);
}

function startScheduler() {
  if ('Notification' in window && Notification.permission === 'default') {
    Notification.requestPermission().catch(() => {});
  }
  setInterval(() => {
    const now = Date.now();
    let dirty = false;
    let rerender = false;
    for (const item of items) {
      if (item.done || !item.remindAt) continue;
      if (!item.notified && item.remindAt <= now) {
        item.notified = true;
        dirty = true;
        fireReminder(item);
      }
      // keep "overdue" styling fresh as time passes
      if (item.remindAt <= now) rerender = true;
    }
    if (rerender) render();
    if (dirty) persist();
  }, 15_000);
}

// ---------------------------------------------------------------------------
// UI wiring
// ---------------------------------------------------------------------------
addForm.addEventListener('submit', (e) => {
  e.preventDefault();
  addItem(addInput.value);
  addInput.value = '';
});
closeBtn.addEventListener('click', () => window.api.closeWindow());

// ===========================================================================
// Paige — ElevenLabs Conversational AI voice agent
// ===========================================================================
let conversation = null;
let config = {};

// Client tools: these run locally in this renderer when Paige decides to call
// them. They give the agent full read/write access to the checklist.
const clientTools = {
  add_item: async ({ text }) => {
    const item = await addItem(text);
    return item ? `Added "${item.text}".` : 'I need the item text to add it.';
  },
  remove_item: async ({ text }) => {
    const item = findItem(text);
    if (!item) return `I couldn't find an item matching "${text}".`;
    await removeItem(item.id);
    return `Removed "${item.text}".`;
  },
  complete_item: async ({ text }) => {
    const item = findItem(text);
    if (!item) return `I couldn't find an item matching "${text}".`;
    await toggleItem(item.id, true);
    return `Checked off "${item.text}".`;
  },
  uncomplete_item: async ({ text }) => {
    const item = findItem(text);
    if (!item) return `I couldn't find an item matching "${text}".`;
    await toggleItem(item.id, false);
    return `Marked "${item.text}" as not done.`;
  },
  list_items: async () => {
    if (items.length === 0) return 'The checklist is empty.';
    return items
      .map((i, n) => {
        const r = i.remindAt && !i.done ? ` [reminder ${formatWhen(i.remindAt)}]` : '';
        return `${n + 1}. ${i.text}${i.done ? ' (done)' : ''}${r}`;
      })
      .join('\n');
  },
  // Executive-assistant: set a reminder on an item. Provide EITHER in_minutes
  // (number) OR when (absolute ISO 8601 datetime, e.g. "2026-06-04T15:00:00").
  // If the item doesn't exist yet, it is created.
  set_reminder: async ({ text, in_minutes, when }) => {
    const ts = resolveWhen({ in_minutes, when });
    if (!ts) return 'I need a time — say something like "in 30 minutes" or "at 3pm".';
    if (ts <= Date.now()) return 'That time is in the past — give me a future time.';
    let item = findItem(text);
    if (!item) item = await addItem(text);
    if (!item) return 'I need the item text to set a reminder.';
    await setReminder(item.id, ts);
    return `Okay — I'll remind you about "${item.text}" at ${formatWhen(ts)}.`;
  },
  clear_reminder: async ({ text }) => {
    const item = findItem(text);
    if (!item) return `I couldn't find an item matching "${text}".`;
    await clearReminder(item.id);
    return `Cleared the reminder on "${item.text}".`;
  },
  list_reminders: async () => {
    const pending = items.filter((i) => i.remindAt && !i.done);
    if (pending.length === 0) return 'You have no reminders set.';
    return pending
      .map((i) => `${i.text} — ${formatWhen(i.remindAt)}`)
      .join('\n');
  },
  // Find a screenshot in Google Drive by name/keyword and describe it.
  // Reads via the sync backend, which holds the Drive + Claude credentials.
  read_screenshot: async ({ query }) => {
    if (!syncCfg) return 'Screenshot reading needs the sync backend configured in config.json.';
    try {
      const res = await fetch(
        syncCfg.url.replace(/\/+$/, '') + '/screenshot?query=' + encodeURIComponent(query || ''),
        { headers: syncHeaders() }
      );
      const r = await res.json().catch(() => ({}));
      return res.ok && r.description
        ? `${r.name}: ${r.description}`
        : (r.error || "I couldn't read the screenshot.");
    } catch (err) {
      return "I couldn't reach the screenshot reader.";
    }
  },
  // Send a push notification to the user's iPhone right now.
  notify_phone: async ({ message }) => {
    const body = (message || '').trim();
    if (!body) return 'What should I send to your phone?';
    const res = await window.api.pushPhone({ title: 'Paige', body });
    return res && res.ok
      ? 'Sent that to your phone.'
      : `I couldn't send it${res && res.reason ? ` (${res.reason})` : ''}. Check the push settings in config.json.`;
  },
};

async function startPaige() {
  if (conversation) return;
  if (!config.agentId) {
    setStatus('⚠️ Set "agentId" in config.json to enable Paige.');
    return;
  }
  try {
    setStatus('Connecting to Paige…');
    paigeDot.className = 'dot active';
    conversation = await Conversation.startSession({
      agentId: config.agentId,
      // If your agent is private, mint a signed URL server-side instead and
      // pass { signedUrl }. A public agent only needs agentId.
      clientTools,
      onConnect: () => setStatus('Paige is listening…'),
      onDisconnect: () => {
        setStatus('Paige ended the session. Say “Paige” to resume.');
        paigeDot.className = 'dot';
        conversation = null;
      },
      onError: (err) => {
        console.error(err);
        setStatus('Paige error: ' + (err?.message || err));
      },
      onModeChange: ({ mode }) => {
        paigeDot.className = 'dot active';
        setStatus(mode === 'speaking' ? 'Paige is speaking…' : 'Paige is listening…');
      },
    });
  } catch (err) {
    console.error(err);
    setStatus('Could not start Paige: ' + (err?.message || err));
    paigeDot.className = 'dot';
    conversation = null;
  }
}

async function stopPaige() {
  if (!conversation) return;
  await conversation.endSession();
  conversation = null;
  paigeDot.className = 'dot';
  setStatus('Say “Paige” or press 🎙️ to talk.');
}

function togglePaige() {
  if (conversation) stopPaige();
  else startPaige();
}

micBtn.addEventListener('click', togglePaige);
window.api.onPaigeToggle(togglePaige); // global hotkey ⌘⇧P from main process

// ===========================================================================
// Wake word: "Paige"
// Two engines:
//   1. Picovoice Porcupine (robust, offline) — used when config.picovoice is set.
//   2. Browser SpeechRecognition (zero-setup fallback).
// ===========================================================================
let porcupine = null;

function startWakeWord() {
  const pv = config.picovoice || {};
  if (pv.accessKey && pv.keywordPath && pv.modelPath) {
    startPorcupine(pv);
  } else {
    startWebSpeechWakeWord();
  }
}

async function startPorcupine(pv) {
  try {
    setStatus('Loading the “Paige” wake-word engine…');
    const [{ PorcupineWorker }, { WebVoiceProcessor }] = await Promise.all([
      import('https://cdn.jsdelivr.net/npm/@picovoice/porcupine-web/+esm'),
      import('https://cdn.jsdelivr.net/npm/@picovoice/web-voice-processor/+esm'),
    ]);
    porcupine = await PorcupineWorker.create(
      pv.accessKey,
      { label: 'Paige', publicPath: pv.keywordPath, sensitivity: pv.sensitivity ?? 0.6 },
      () => {
        if (!conversation) {
          paigeDot.className = 'dot listening';
          startPaige();
        }
      },
      { publicPath: pv.modelPath }
    );
    await WebVoiceProcessor.subscribe(porcupine);
    paigeDot.className = 'dot listening';
    setStatus('Listening for “Paige”…');
  } catch (err) {
    console.error('Porcupine failed:', err);
    setStatus('Wake-word engine failed — using fallback. Use 🎙️ / ⌘⇧P.');
    startWebSpeechWakeWord();
  }
}

function startWebSpeechWakeWord() {
  const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!SpeechRecognition) {
    setStatus('Wake word unavailable here — use 🎙️ or ⌘⇧P to talk to Paige.');
    return;
  }
  const rec = new SpeechRecognition();
  rec.continuous = true;
  rec.interimResults = true;
  rec.lang = 'en-US';

  rec.onresult = (event) => {
    if (conversation) return; // already talking to Paige
    for (let i = event.resultIndex; i < event.results.length; i++) {
      const transcript = event.results[i][0].transcript.toLowerCase();
      if (/\bpaige\b|\bpage\b/.test(transcript)) {
        paigeDot.className = 'dot listening';
        startPaige();
        break;
      }
    }
  };
  rec.onerror = (e) => {
    // 'no-speech' / 'aborted' are routine; just keep going.
    if (e.error === 'not-allowed' || e.error === 'service-not-allowed') {
      setStatus('Mic blocked — allow microphone access to use the wake word.');
    }
  };
  // Keep the recogniser alive; it stops itself periodically.
  rec.onend = () => {
    if (!conversation) {
      try { rec.start(); } catch { /* already started */ }
    }
  };
  try {
    rec.start();
    if (!conversation) paigeDot.className = 'dot listening';
  } catch {
    /* will retry on end */
  }
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------
(async function init() {
  config = (await window.api.getConfig()) || {};
  items = (await window.api.loadChecklist()) || [];
  render();

  // Optional backend sync (shared with the phone + future devices).
  if (config.sync && config.sync.url && config.sync.token) {
    syncCfg = config.sync;
    await pullRemote();      // adopt server state on launch
    if (items.length) pushRemote(); // seed server if it was empty
    setInterval(pullRemote, 5000); // pick up changes from the phone
  }

  startScheduler();
  if (config.wakeWord !== false) startWakeWord();
})();
