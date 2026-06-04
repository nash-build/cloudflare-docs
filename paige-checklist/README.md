# Paige Checklist

A small **always-on-top, top-right** checklist overlay for **macOS**, built with
Electron. It floats above every other app (including most fullscreen windows),
persists your list to disk, and includes **Paige** — an ElevenLabs Conversational
AI voice agent with **read/write** access to the checklist, activated by saying
her name.

> Heads up: this is a standalone desktop app you run on **your own Mac**. It is
> not connected to your Claude.ai chat history — items are added by you (typing
> or via Paige), not auto-imported from your conversations.

## Features

- Frameless, translucent card pinned to the **top-right** of your screen.
- **Always-on-top** at the `screen-saver` level + visible on all spaces /
  fullscreen, so no app can cover it.
- Add / remove / check off items; checking an item plays a **strike-through**
  animation. The list is saved to
  `~/Library/Application Support/paige-checklist/checklist.json`.
- **Paige** voice agent (ElevenLabs) acts as an **executive assistant**: she can
  read and write the checklist *and* set reminders on items.
- **Reminders** fire native macOS notifications, bounce the Dock, and highlight
  the item in the overlay at the scheduled time.
- **iPhone push** — when a reminder fires (or you ask Paige to "text my phone"),
  a notification is sent to your iPhone via [ntfy](https://ntfy.sh) or
  [Pushover](https://pushover.net).
- Say **"Paige"** to start talking, or click 🎙️ / press **⌘⇧P**.
- Lives in the **menu bar** (tray icon) and can **launch at login**, so it's
  always running and reminders fire even after a restart.
- **Phone access** — an optional Cloudflare Worker backend + mobile web app lets
  you send items to the list and talk to Paige from your iPhone, all synced with
  the Mac overlay.

## Requirements

- macOS
- [Node.js](https://nodejs.org/) 18+ (Electron runs on Node)
- An ElevenLabs account + a Conversational AI agent

## Setup

```bash
cd paige-checklist
npm install
cp config.example.json config.json   # then edit config.json
npm start
```

### Configure Paige (ElevenLabs)

1. In the [ElevenLabs](https://elevenlabs.io/) dashboard, create a
   **Conversational AI agent**. Give it a voice you like for "Paige".
2. Add **client tools** to the agent with these exact names and parameters:

   | Tool name         | Parameters                          | What it does                              |
   | ----------------- | ----------------------------------- | ----------------------------------------- |
   | `add_item`        | `text` (string)                     | Adds a new checklist item                 |
   | `remove_item`     | `text` (string)                     | Removes the matching item                 |
   | `complete_item`   | `text` (string)                     | Checks an item off (strike-through)       |
   | `uncomplete_item` | `text` (string)                     | Un-checks an item                         |
   | `list_items`      | _(none)_                            | Reads the current list back to you        |
   | `set_reminder`    | `text` (string), `in_minutes` (number) **or** `when` (ISO 8601 string) | Schedules a reminder on an item (creates it if new) |
   | `clear_reminder`  | `text` (string)                     | Removes the reminder from an item         |
   | `list_reminders`  | _(none)_                            | Reads back all pending reminders          |
   | `notify_phone`    | `message` (string)                  | Sends a push notification to your iPhone  |

   Mark them as **blocking** (await response) so Paige can confirm results.
3. Suggested agent system prompt:
   > You are Paige, a concise executive assistant managing a checklist. When the
   > user asks to add, remove, complete, or review tasks, call the matching
   > client tool. To set reminders, call `set_reminder`: prefer `in_minutes` for
   > relative times ("in half an hour" → 30); for clock/calendar times, compute
   > an absolute ISO 8601 datetime in the user's local timezone and pass it as
   > `when`. Use `notify_phone` to send a message to the user's iPhone on
   > request. Confirm briefly. Don't read the whole list unless asked.
4. Copy the agent's **Agent ID** into `config.json` as `agentId`.
   - If your agent is **public**, `agentId` alone works.
   - If it's **private**, you'll need to mint a signed URL server-side and pass
     it as `signedUrl` in `renderer.js` (see the comment in `startPaige`).

### iPhone push notifications

Reminders (and Paige's `notify_phone` tool) can ping your iPhone. Two options —
both are free or cheap and need **no Apple Developer account**:

**Option A — ntfy (free, recommended)**
1. Install the **ntfy** app on your iPhone (App Store).
2. Pick a **topic** name that is long and unguessable — it acts like a password,
   since anyone who knows it can post to it. e.g. `paige-7f3k9q2x`.
3. In the app, tap **+** and subscribe to that topic.
4. Put it in `config.json` under `push`:
   ```json
   "push": { "provider": "ntfy", "ntfyTopic": "paige-7f3k9q2x", "ntfyServer": "https://ntfy.sh" }
   ```
   (Self-hosting ntfy? Set `ntfyServer` to your URL and `ntfyToken` if it needs auth.)

**Option B — Pushover (one-time purchase, very reliable)**
1. Install **Pushover** on your iPhone and create an [application token](https://pushover.net/apps/build).
2. Set in `config.json`:
   ```json
   "push": { "provider": "pushover", "pushoverToken": "APP_TOKEN", "pushoverUser": "USER_KEY" }
   ```

Test it: start the app, set a reminder one minute out (*"Paige, remind me to test in
1 minute"*), or just say *"Paige, text my phone that it works."* Omit the `push`
block entirely to turn phone notifications off.

### Usage

- Say **"Paige"** → she connects and listens. Then: *"add buy milk"*,
  *"check off buy milk"*, *"remove buy milk"*, *"what's on my list?"*,
  *"remind me to call the bank in 30 minutes"*, *"remind me to leave at 5pm"*,
  *"what reminders do I have?"*.
- Reminders fire even while you're in another app — the notification appears,
  the Dock bounces, and the overlay flashes the item.
- Or type in the input box / click the checkbox to toggle, 🗑 to delete.
- **⌘⇧P** toggles Paige without the wake word.

### Menu bar & launch at login

The app runs as a **menu-bar (tray) app**. Click the tray icon to show/hide the
overlay; the tray menu has **Show/Hide checklist**, **Talk to Paige**, a
**Launch at login** checkbox, and **Quit**. Closing the window (✕) just hides it
to the tray — use **Quit** to fully exit. Enabling *Launch at login* makes it
start (hidden) when you log in, so reminders keep working after a reboot.

## Use it from your phone (Cloudflare backend + mobile app)

This adds a shared **source of truth** so your Mac, your iPhone, and any future
device (the 01 / a ring) all see the same list. It's a small Cloudflare Worker
backed by KV that also serves the mobile web app.

### Deploy the backend

```bash
cd server
npm install
npx wrangler login
npx wrangler kv namespace create PAIGE_KV   # paste the id into wrangler.toml
npx wrangler secret put PAIGE_TOKEN         # choose a long random token
npm run deploy
```

You'll get a URL like `https://paige-checklist.<you>.workers.dev`.

The API (all routes need `Authorization: Bearer <PAIGE_TOKEN>`):

| Route            | Method | Purpose                                            |
| ---------------- | ------ | -------------------------------------------------- |
| `/list`          | GET    | Read the whole list (`{ version, items }`)         |
| `/list`          | PUT    | Replace the whole list                             |
| `/add`           | POST   | Append one item `{ text }` — **the device hook**   |
| `/complete`      | POST   | Toggle an item `{ id \| text, done }`              |

### Use it on your iPhone

1. Open the Worker URL in **Safari** on your iPhone.
2. Tap **Share → Add to Home Screen** — now it's an app icon.
3. Open it, tap **⚙️**, and enter your **sync token** (the `PAIGE_TOKEN`) and,
   for voice, your **ElevenLabs Agent ID**.
4. Type in the box to fire items at the list, or tap the **🎙️ walkie-talkie**
   button to talk to Paige (she adds/removes/completes via the same backend).

Paige has the **same full tool set on the phone as on the Mac** (add, remove,
complete, uncomplete, list, set/clear/list reminders, notify) — so one
ElevenLabs agent works identically everywhere. Reminders set from the phone are
fired by the always-on Mac overlay (notification + iPhone push); on the phone
itself, `notify_phone` shows a local notification.

### Connect the Mac overlay to it

Add a `sync` block to the Mac app's `config.json`:

```json
"sync": { "url": "https://paige-checklist.<you>.workers.dev", "token": "SAME-AS-PAIGE_TOKEN" }
```

The overlay then pulls every few seconds and pushes on every change — so items
you send from your phone appear on the Mac (and reminders still fire there).

### Wiring up a wearable later

Any device that can make an HTTP request can drop items on the list — that's the
"bones" for the 01 / your ring. Just POST to `/add`:

```bash
curl -X POST https://paige-checklist.<you>.workers.dev/add \
  -H "Authorization: Bearer $PAIGE_TOKEN" -H "content-type: application/json" \
  -d '{"text":"pick up dry cleaning"}'
```

## Build a double-clickable app (no terminal)

To get a normal `.app` / `.dmg` you can launch from Finder:

```bash
npm install
npm run dist      # builds a .dmg + .zip into dist/ (run this ON your Mac)
```

This uses **electron-builder** (config is in `package.json`) and generates the
`.icns` from `assets/icon.png`. Open the `.dmg` in `dist/` and drag **Paige
Checklist** to Applications. Because the app isn't code-signed/notarized, the
first launch needs **right-click → Open** (or *System Settings → Privacy &
Security → Open Anyway*). To distribute it more widely you'd add an Apple
Developer ID and notarization — see electron-builder's macOS signing docs.

> Note: `npm run dist` must run **on macOS** to produce a Mac app; it can't be
> cross-built from Linux/Windows.

## Wake-word options

**Default — browser SpeechRecognition** (zero setup). In Electron this can be
unreliable (it depends on the embedded Chromium speech backend). If "Paige"
isn't detected, the 🎙️ button and **⌘⇧P** always work, or set
`"wakeWord": false` to disable always-listening.

**Robust — Picovoice Porcupine** (offline, reliable). To use a real "Paige"
wake word:

1. Create a free account at the [Picovoice Console](https://console.picovoice.ai/)
   and copy your **AccessKey**.
2. In the console, train a custom **wake word "Paige"** for **Web (WASM)** and
   download the `Paige.ppn` file.
3. Download the English model `porcupine_params.pv` (Picovoice provides it for
   the Web SDK).
4. Put **both files in `renderer/`** (next to `index.html`), and fill the
   `picovoice` block in `config.json`:
   ```json
   "picovoice": { "accessKey": "YOUR_KEY", "keywordPath": "Paige.ppn", "modelPath": "porcupine_params.pv", "sensitivity": 0.6 }
   ```
The app auto-detects this and uses Porcupine; otherwise it falls back to the
browser engine. (When packaging, the `.ppn`/`.pv` files in `renderer/` are
bundled automatically via the `renderer/**/*` glob.)

## What this app can't do

It can't read your Claude.ai chats, and it can't be driven by typing "finish"
inside the Claude.ai web chat — there's no link between that web page and this
app. Completion happens via Paige (Mac or phone), the checkbox, or the UI.

## Architecture at a glance

```
   iPhone PWA ─┐                        ┌─ macOS overlay (Electron)
               ├─► Cloudflare Worker ◄──┤   • always-on-top, reminders, push
   future 01/  │     + KV (shared list) │   • syncs every few seconds
   ring (POST  ┘                        └─
   /add) ──────►
```
ElevenLabs powers Paige's voice on both the Mac and the phone; each client runs
the checklist tools against whichever store it has (local file, or the shared
Worker when `sync` is configured).
