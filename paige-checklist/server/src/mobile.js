// The mobile PWA, served as static strings by the Worker. Self-contained:
// quick text capture + a "talk to Paige" walkie-talkie button (ElevenLabs),
// all syncing to the same KV-backed checklist as the Mac overlay.

export const MANIFEST_JSON = JSON.stringify({
  name: 'Paige Checklist',
  short_name: 'Paige',
  display: 'standalone',
  background_color: '#16181d',
  theme_color: '#16181d',
  start_url: '/',
  icons: [],
});

// A no-op service worker is enough to make iOS treat this as an installable
// app and keep it feeling native after "Add to Home Screen".
export const SERVICE_WORKER_JS = `self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));
self.addEventListener('fetch', () => {});`;

export const MOBILE_HTML = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no" />
<meta name="apple-mobile-web-app-capable" content="yes" />
<meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
<meta name="theme-color" content="#16181d" />
<link rel="manifest" href="/manifest.webmanifest" />
<link rel="apple-touch-icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'%3E%3Crect width='100' height='100' rx='22' fill='%237c9cff'/%3E%3Cpath d='M24 54 L44 72 L78 30' stroke='white' stroke-width='10' fill='none' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E" />
<title>Paige Checklist</title>
<style>
  :root { --bg:#16181d; --card:#1e2128; --text:#f2f3f5; --muted:#9aa0a6; --accent:#7c9cff; --done:#5f6571; --danger:#ff6b6b; }
  * { box-sizing:border-box; -webkit-tap-highlight-color:transparent; }
  html,body { margin:0; height:100%; background:var(--bg); color:var(--text);
    font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif; }
  body { display:flex; flex-direction:column; padding:env(safe-area-inset-top) 14px calc(env(safe-area-inset-bottom) + 12px); }
  header { display:flex; align-items:center; justify-content:space-between; padding:14px 2px 10px; }
  h1 { font-size:18px; margin:0; }
  .gear { background:none; border:none; color:var(--muted); font-size:20px; }
  .quick { display:flex; gap:8px; margin-bottom:10px; }
  .quick input { flex:1; background:var(--card); border:1px solid transparent; border-radius:12px;
    color:var(--text); padding:13px 14px; font-size:16px; outline:none; }
  .quick input:focus { border-color:var(--accent); }
  .quick button { background:var(--accent); border:none; color:#fff; border-radius:12px; padding:0 16px; font-size:20px; }
  .list { list-style:none; margin:0; padding:0; flex:1; overflow-y:auto; }
  .item { display:flex; align-items:center; gap:12px; background:var(--card); border-radius:12px;
    padding:14px; margin-bottom:8px; }
  .box { width:22px; height:22px; flex:0 0 auto; border:2px solid var(--muted); border-radius:7px; display:grid; place-items:center; }
  .item.done .box { background:var(--accent); border-color:var(--accent); }
  .item.done .box::after { content:"✓"; color:#fff; font-size:14px; }
  .label { flex:1; font-size:16px; }
  .item.done .label { color:var(--done); text-decoration:line-through; }
  .empty { color:var(--muted); text-align:center; padding:30px; }
  .talk { margin-top:10px; display:flex; flex-direction:column; align-items:center; gap:6px; }
  .ptt { width:88px; height:88px; border-radius:50%; border:none; background:var(--accent); color:#fff;
    font-size:30px; box-shadow:0 6px 20px rgba(124,156,255,.4); transition:transform .1s, background .2s; }
  .ptt:active { transform:scale(.94); }
  .ptt.live { background:#4ade80; animation:pulse 1.3s infinite; }
  @keyframes pulse { 50% { opacity:.6; } }
  .hint { color:var(--muted); font-size:12px; min-height:16px; }
  dialog { background:var(--card); color:var(--text); border:none; border-radius:16px; width:90%; max-width:380px; padding:18px; }
  dialog::backdrop { background:rgba(0,0,0,.6); }
  dialog label { display:block; font-size:13px; color:var(--muted); margin:10px 0 4px; }
  dialog input { width:100%; background:var(--bg); border:1px solid #333; border-radius:10px; color:var(--text); padding:11px; font-size:15px; }
  dialog .row { display:flex; gap:8px; margin-top:16px; }
  dialog button { flex:1; padding:11px; border-radius:10px; border:none; font-size:15px; }
  .save { background:var(--accent); color:#fff; }
  .cancel { background:#33363d; color:var(--text); }
</style>
</head>
<body>
  <header>
    <h1>Paige Checklist</h1>
    <button class="gear" id="gear" title="Settings">⚙️</button>
  </header>

  <form class="quick" id="quick">
    <input id="quick-input" placeholder="Send something to the list…" autocomplete="off" />
    <button type="submit">＋</button>
  </form>

  <ul class="list" id="list"></ul>

  <div class="talk">
    <button class="ptt" id="ptt" title="Talk to Paige">🎙️</button>
    <div class="hint" id="hint">Tap to talk to Paige</div>
  </div>

  <dialog id="settings">
    <label>Sync token</label>
    <input id="set-token" placeholder="your PAIGE_TOKEN" />
    <label>ElevenLabs Agent ID (for voice)</label>
    <input id="set-agent" placeholder="agent id (optional)" />
    <div class="row">
      <button class="cancel" id="set-cancel">Cancel</button>
      <button class="save" id="set-save">Save</button>
    </div>
  </dialog>

<script type="module">
import { Conversation } from 'https://cdn.jsdelivr.net/npm/@elevenlabs/client/+esm';

const API = location.origin;
const $ = (id) => document.getElementById(id);
const listEl = $('list'), hint = $('hint'), ptt = $('ptt');

const cfg = {
  get token() { return localStorage.getItem('paige_token') || ''; },
  set token(v) { localStorage.setItem('paige_token', v); },
  get agentId() { return localStorage.getItem('paige_agent') || ''; },
  set agentId(v) { localStorage.setItem('paige_agent', v); },
};

const headers = () => ({ 'content-type': 'application/json', authorization: 'Bearer ' + cfg.token });

async function api(path, opts = {}) {
  const res = await fetch(API + path, { ...opts, headers: { ...headers(), ...(opts.headers || {}) } });
  if (!res.ok) throw new Error('HTTP ' + res.status);
  return res.json();
}

let items = [];
async function refresh() {
  try { const s = await api('/list'); items = s.items || []; render(); }
  catch (e) { hint.textContent = 'Set your sync token in ⚙️'; }
}

function render() {
  listEl.innerHTML = '';
  if (!items.length) {
    const li = document.createElement('li'); li.className = 'empty'; li.textContent = 'Nothing yet. Add something above or tap 🎙️.';
    listEl.appendChild(li); return;
  }
  for (const it of items) {
    const li = document.createElement('li');
    li.className = 'item' + (it.done ? ' done' : '');
    const box = document.createElement('span'); box.className = 'box';
    box.onclick = () => toggle(it);
    const label = document.createElement('span'); label.className = 'label'; label.textContent = it.text;
    li.append(box, label); listEl.appendChild(li);
  }
}

async function toggle(it) {
  try { await api('/complete', { method: 'POST', body: JSON.stringify({ id: it.id, done: !it.done }) }); refresh(); }
  catch (e) { hint.textContent = 'Could not update — check token.'; }
}

$('quick').addEventListener('submit', async (e) => {
  e.preventDefault();
  const v = $('quick-input').value.trim();
  if (!v) return;
  $('quick-input').value = '';
  try { await api('/add', { method: 'POST', body: JSON.stringify({ text: v }) }); refresh(); }
  catch (err) { hint.textContent = 'Could not add — check token in ⚙️.'; }
});

// --- Settings ---
$('gear').onclick = () => { $('set-token').value = cfg.token; $('set-agent').value = cfg.agentId; $('settings').showModal(); };
$('set-cancel').onclick = () => $('settings').close();
$('set-save').onclick = () => { cfg.token = $('set-token').value.trim(); cfg.agentId = $('set-agent').value.trim(); $('settings').close(); refresh(); };

// --- Walkie-talkie: talk to Paige (ElevenLabs) ---
const tools = {
  add_item: async ({ text }) => { await api('/add', { method: 'POST', body: JSON.stringify({ text }) }); refresh(); return 'Added ' + text; },
  complete_item: async ({ text }) => { await api('/complete', { method: 'POST', body: JSON.stringify({ text, done: true }) }); refresh(); return 'Checked off ' + text; },
  remove_item: async ({ text }) => {
    const s = await api('/list'); const keep = s.items.filter(i => !i.text.toLowerCase().includes((text||'').toLowerCase()));
    await api('/list', { method: 'PUT', body: JSON.stringify({ items: keep }) }); refresh(); return 'Removed ' + text;
  },
  list_items: async () => { const s = await api('/list'); return (s.items||[]).map(i => i.text + (i.done?' (done)':'')).join(', ') || 'empty'; },
};

let convo = null;
ptt.addEventListener('click', async () => {
  if (convo) { await convo.endSession(); convo = null; ptt.classList.remove('live'); hint.textContent = 'Tap to talk to Paige'; return; }
  if (!cfg.agentId) { hint.textContent = 'Add your Agent ID in ⚙️ to use voice.'; return; }
  try {
    hint.textContent = 'Connecting…'; ptt.classList.add('live');
    convo = await Conversation.startSession({
      agentId: cfg.agentId,
      clientTools: tools,
      onConnect: () => hint.textContent = 'Listening — tap to stop',
      onDisconnect: () => { convo = null; ptt.classList.remove('live'); hint.textContent = 'Tap to talk to Paige'; refresh(); },
      onError: (e) => hint.textContent = 'Voice error: ' + (e?.message || e),
    });
  } catch (e) { ptt.classList.remove('live'); hint.textContent = 'Mic/agent error: ' + (e?.message || e); convo = null; }
});

if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js').catch(() => {});
refresh();
setInterval(refresh, 5000);
</script>
</body>
</html>`;
