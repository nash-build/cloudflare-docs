# Google MCP Server (multi-alias)

A small, self-contained [Model Context Protocol](https://modelcontextprotocol.io)
server that exposes a Gmail account to an MCP client (Claude Desktop, Claude
Code, etc.).

It is designed to solve one specific problem: **connecting more than one Google
alias.** Most hosted Google connectors only let you authorize a single Google
identity. This server keeps an independent OAuth login *per account*, so you can
add a second (or third) Gmail account alongside the one you already have
connected.

> You run this server yourself, on your own machine, against your own Google
> Cloud OAuth client. No credentials are ever committed to this repository and
> nothing leaves your machine except calls to Google's own APIs.

## Quickstart (just run this)

If you only want it working and don't care about the details, run this one
command **on your own machine** from the repo root:

```bash
bash tools/google-mcp-server/setup.sh
```

It creates the virtualenv, installs everything, generates the encryption key,
writes your `.env`, and then walks you through authorizing each Gmail alias. It
is safe to run repeatedly — it skips whatever is already done. The script will
pause and tell you the one manual step it can't do for you: creating a Google
Cloud OAuth client (clicking through Google's website). Follow its printed
instructions, then run it again.

The sections below explain each step in detail if you'd rather do it by hand.

## Security model / best practices

This server follows Google's current guidance for desktop ("installed app")
OAuth clients:

- **Loopback redirect + PKCE.** Auth uses the `127.0.0.1` loopback flow with
  PKCE (proof key for code exchange). The deprecated out-of-band (`oob`) copy/paste
  flow is *not* used.
- **CSRF `state` parameter** is generated and verified on the callback.
- **Least privilege scopes.** Defaults to **read-only** Gmail
  (`gmail.readonly`). Sending/modifying is opt-in via configuration, so the
  server can't mutate your mailbox unless you explicitly grant it.
- **No secrets in source control.** Your OAuth client config and all tokens live
  outside the repo. `.gitignore` blocks `*.json` client secrets, `.env`, and the
  token store.
- **Encrypted token storage at rest.** Refresh/access tokens are encrypted with
  a Fernet key (`GOOGLE_MCP_ENCRYPTION_KEY`) and written with `0600` file
  permissions in a per-user config directory — never in the project tree.
- **Per-account isolation.** Each alias gets its own encrypted token file keyed
  by email address, so accounts can be added/removed independently.
- **Revocable.** `remove-account` deletes local tokens; you can also revoke
  access at <https://myaccount.google.com/permissions> at any time.

## One-time setup

### 1. Create a Google Cloud OAuth client

1. Open the [Google Cloud Console](https://console.cloud.google.com/) and create
   (or pick) a project.
2. **APIs & Services → Enable APIs** → enable the **Gmail API**.
3. **APIs & Services → OAuth consent screen** → configure it. While testing,
   leave it in "Testing" mode and add each alias as a **Test user** (this lets
   you authorize accounts without going through app verification).
4. **APIs & Services → Credentials → Create credentials → OAuth client ID** →
   choose **Desktop app**. Download the JSON.

### 2. Install

Requires Python 3.10+.

```bash
cd tools/google-mcp-server
python -m venv .venv && source .venv/bin/activate
pip install -e .
```

### 3. Configure

```bash
cp .env.example .env
# Generate an encryption key for the local token store:
python -m google_mcp.keygen   # prints a key; paste it into .env
```

Edit `.env` and point `GOOGLE_CLIENT_SECRETS_FILE` at the JSON you downloaded
(store it *outside* this repo, e.g. `~/.config/google-mcp/client_secret.json`).

### 4. Authorize each alias

Run this once per Google account. A browser window opens for the standard Google
consent screen; the token is stored encrypted locally.

```bash
python -m google_mcp.add_account            # add your second alias
python -m google_mcp.add_account            # repeat for a third, etc.
python -m google_mcp.add_account --list     # see connected accounts
python -m google_mcp.add_account --remove you@example.com
```

## Connect it to your MCP client

### Claude Code

```bash
claude mcp add google-second -- python -m google_mcp.server
```

### Claude Desktop (`claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "google-second": {
      "command": "python",
      "args": ["-m", "google_mcp.server"],
      "env": {
        "GOOGLE_CLIENT_SECRETS_FILE": "/Users/you/.config/google-mcp/client_secret.json",
        "GOOGLE_MCP_ENCRYPTION_KEY": "your-fernet-key"
      }
    }
  }
}
```

## Tools exposed

| Tool | Description |
| --- | --- |
| `list_accounts` | List the Gmail aliases currently authorized. |
| `search_messages` | Search a mailbox with a Gmail query (e.g. `from:foo is:unread`). |
| `get_message` | Fetch a single message (headers + body) by id. |
| `list_labels` | List labels/folders for an account. |
| `send_message` | Send mail. **Disabled unless** `gmail.send` scope is enabled in `.env`. |

Each tool takes an `account` argument (the email address) so a single server can
serve all your aliases at once.
