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

let items = []; // { id, text, done }

// ---------------------------------------------------------------------------
// State + persistence
// ---------------------------------------------------------------------------
const uid = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 6);

async function persist() {
  await window.api.saveChecklist(items);
}

function setStatus(msg) {
  statusText.textContent = msg;
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

    const del = document.createElement('button');
    del.className = 'delete-btn';
    del.textContent = '🗑';
    del.title = 'Remove';
    del.addEventListener('click', () => removeItem(item.id));

    li.append(box, label, del);
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
      .map((i, n) => `${n + 1}. ${i.text}${i.done ? ' (done)' : ''}`)
      .join('\n');
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
// Uses the browser SpeechRecognition API to listen continuously for the name.
// When heard, it opens the Paige conversation. (See README for the more robust
// Picovoice Porcupine option if SpeechRecognition is unreliable in Electron.)
// ===========================================================================
function startWakeWord() {
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
  if (config.wakeWord !== false) startWakeWord();
})();
