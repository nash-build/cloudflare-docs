#!/usr/bin/env bash
#
# One-shot setup for the Google MCP server.
#
# Run this once on your own machine:
#
#     bash tools/google-mcp-server/setup.sh
#
# It will:
#   1. create a Python virtualenv (.venv) and install the package
#   2. create your local .env (from .env.example) if you don't have one
#   3. generate + insert the token-encryption key automatically
#   4. check whether your Google OAuth client JSON is configured
#   5. if everything's ready, walk you through authorizing each Gmail alias
#
# It is safe to run again — it skips anything already done.

set -euo pipefail

# Always operate from this script's own directory, no matter where it's called.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

say()  { printf '\n\033[1;36m==>\033[0m %s\n' "$*"; }
ok()   { printf '\033[1;32m  ok\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m  !!\033[0m %s\n' "$*"; }

# ---------------------------------------------------------------------------
# 1. Python check
# ---------------------------------------------------------------------------
say "Checking Python..."
if command -v python3 >/dev/null 2>&1; then
  PY=python3
elif command -v python >/dev/null 2>&1; then
  PY=python
else
  warn "Python is not installed. Install Python 3.10+ from https://www.python.org/downloads/ and run this again."
  exit 1
fi
PY_VER="$($PY -c 'import sys; print("%d.%d" % sys.version_info[:2])')"
ok "Found Python $PY_VER ($PY)"

# ---------------------------------------------------------------------------
# 2. Virtualenv + install
# ---------------------------------------------------------------------------
if [ ! -d .venv ]; then
  say "Creating virtual environment (.venv)..."
  "$PY" -m venv .venv
  ok "Created .venv"
else
  ok ".venv already exists"
fi

# shellcheck disable=SC1091
source .venv/bin/activate

say "Installing the server (this can take a minute)..."
pip install --quiet --upgrade pip
pip install --quiet -e .
ok "Installed google-mcp-server"

# ---------------------------------------------------------------------------
# 3. .env
# ---------------------------------------------------------------------------
if [ ! -f .env ]; then
  say "Creating your local .env from the template..."
  cp .env.example .env
  ok "Created .env"
else
  ok ".env already exists"
fi

# ---------------------------------------------------------------------------
# 4. Encryption key (auto-generate + insert if blank)
# ---------------------------------------------------------------------------
if grep -qE '^GOOGLE_MCP_ENCRYPTION_KEY=$' .env; then
  say "Generating a token-encryption key..."
  KEY="$(python -m google_mcp.keygen)"
  # Portable in-place edit (works on both GNU and BSD/macOS sed).
  python - "$KEY" <<'PYEOF'
import sys, pathlib
key = sys.argv[1]
p = pathlib.Path(".env")
text = p.read_text()
text = text.replace("GOOGLE_MCP_ENCRYPTION_KEY=\n",
                    f"GOOGLE_MCP_ENCRYPTION_KEY={key}\n")
p.write_text(text)
PYEOF
  ok "Encryption key generated and saved to .env"
else
  ok "Encryption key already set in .env"
fi

# ---------------------------------------------------------------------------
# 5. Google OAuth client JSON
# ---------------------------------------------------------------------------
# Pull the configured path out of .env.
SECRETS_PATH="$(grep -E '^GOOGLE_CLIENT_SECRETS_FILE=' .env | head -1 | cut -d= -f2- || true)"
# Expand a leading ~ if present.
SECRETS_PATH_EXPANDED="${SECRETS_PATH/#\~/$HOME}"

NEED_GOOGLE_SETUP=0
if [ -z "$SECRETS_PATH" ]; then
  NEED_GOOGLE_SETUP=1
elif [ ! -f "$SECRETS_PATH_EXPANDED" ]; then
  NEED_GOOGLE_SETUP=1
fi

if [ "$NEED_GOOGLE_SETUP" -eq 1 ]; then
  cat <<'EOF'

------------------------------------------------------------------------
ALMOST THERE — one manual step left (only takes a few minutes, one time)
------------------------------------------------------------------------

I can't click through Google's website for you, but here's exactly what to do:

  1. Go to:  https://console.cloud.google.com/
     Create a project (or pick any existing one).

  2. Enable the Gmail API:
     APIs & Services  ->  Enable APIs  ->  search "Gmail API"  ->  Enable

  3. Configure the consent screen:
     APIs & Services  ->  OAuth consent screen
       - User type: External
       - Fill in the required name/email fields
       - Under "Test users", ADD EACH GMAIL ALIAS you want to connect
         (this lets you authorize them without app review)

  4. Create the credential:
     APIs & Services  ->  Credentials  ->  Create credentials
       ->  OAuth client ID  ->  Application type: "Desktop app"  ->  Create
       ->  click "DOWNLOAD JSON"

  5. Save that downloaded file somewhere safe OUTSIDE this repo, e.g.:
         mkdir -p ~/.config/google-mcp
         mv ~/Downloads/client_secret_*.json ~/.config/google-mcp/client_secret.json

  6. Open the file  tools/google-mcp-server/.env  and set this line to that path:
         GOOGLE_CLIENT_SECRETS_FILE=/Users/YOU/.config/google-mcp/client_secret.json

  7. Run this script again:
         bash tools/google-mcp-server/setup.sh

------------------------------------------------------------------------
EOF
  exit 0
fi

ok "Found your Google client secrets: $SECRETS_PATH_EXPANDED"

# ---------------------------------------------------------------------------
# 6. Authorize an account
# ---------------------------------------------------------------------------
say "Everything's configured. Let's authorize a Gmail alias."
echo "A browser window will open. Sign in with the alias you want to add and click Allow."
echo "(If nothing opens, copy the URL it prints into your browser.)"
echo
read -r -p "Press Enter to start authorizing an account (or Ctrl+C to stop)... " _ || true

google-mcp-add-account

cat <<'EOF'

------------------------------------------------------------------------
DONE. Useful follow-up commands (run them after `source .venv/bin/activate`
inside tools/google-mcp-server):

  google-mcp-add-account            # add another alias
  google-mcp-add-account --list     # see connected aliases
  google-mcp-add-account --remove you@example.com

To connect this to Claude Code, run (in this folder, venv active):

  claude mcp add google-second -- python -m google_mcp.server
------------------------------------------------------------------------
EOF
