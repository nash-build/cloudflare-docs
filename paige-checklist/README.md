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
- **Paige** voice agent (ElevenLabs) with client tools: `add_item`,
  `remove_item`, `complete_item`, `uncomplete_item`, `list_items`.
- Say **"Paige"** to start talking, or click 🎙️ / press **⌘⇧P**.

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
2. Add **client tools** to the agent with these exact names and a single
   string parameter `text` (except `list_items`, which takes no parameters):

   | Tool name         | Parameter      | What it does                          |
   | ----------------- | -------------- | ------------------------------------- |
   | `add_item`        | `text`         | Adds a new checklist item             |
   | `remove_item`     | `text`         | Removes the matching item             |
   | `complete_item`   | `text`         | Checks an item off (strike-through)   |
   | `uncomplete_item` | `text`         | Un-checks an item                     |
   | `list_items`      | _(none)_       | Reads the current list back to you    |

   Mark them as **blocking** (await response) so Paige can confirm results.
3. Suggested agent system prompt:
   > You are Paige, a concise checklist assistant. When the user asks to add,
   > remove, complete, or review tasks, call the matching client tool. Confirm
   > briefly. Don't read the whole list unless asked.
4. Copy the agent's **Agent ID** into `config.json` as `agentId`.
   - If your agent is **public**, `agentId` alone works.
   - If it's **private**, you'll need to mint a signed URL server-side and pass
     it as `signedUrl` in `renderer.js` (see the comment in `startPaige`).

### Usage

- Say **"Paige"** → she connects and listens. Then: *"add buy milk"*,
  *"check off buy milk"*, *"remove buy milk"*, *"what's on my list?"*.
- Or type in the input box / click the checkbox to toggle, 🗑 to delete.
- **⌘⇧P** toggles Paige without the wake word.

## Wake-word notes

The default wake word uses the browser **SpeechRecognition** API. In Electron
this can be unreliable (it depends on the embedded Chromium's speech backend).
If "Paige" isn't being detected:

- Use the 🎙️ button or **⌘⇧P** — these always work.
- Or set `"wakeWord": false` in `config.json` to disable always-listening.
- For a robust offline wake word, integrate
  [Picovoice Porcupine](https://picovoice.ai/platform/porcupine/) with a custom
  "Paige" keyword (`.ppn`) — drop-in point is `startWakeWord()` in
  `renderer/renderer.js`.

## What this app can't do

It can't read your Claude.ai chats, and it can't be driven by typing "finish"
inside the Claude.ai web chat — there's no link between that web page and this
local app. Completion happens here: via Paige, the checkbox, or the UI.
