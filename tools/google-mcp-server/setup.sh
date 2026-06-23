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
# Prefer a known-stable interpreter (3.10-3.13). Very new releases like 3.14
# can fail to bootstrap pip in venvs and often lack prebuilt dependency wheels.
PY=""
for cand in python3.13 python3.12 python3.11 python3.10; do
  if command -v "$cand" >/dev/null 2>&1; then
    PY="$cand"
    break
  fi
done
# Fall back to whatever generic python is available.
if [ -z "$PY" ]; then
  if command -v python3 >/dev/null 2>&1; then
    PY=python3
  elif command -v python >/dev/null 2>&1; then
    PY=python
  else
    warn "Python is not installed. Install Python 3.12 from https://www.python.org/downloads/ and run this again."
    exit 1
  fi
fi
PY_VER="$($PY -c 'import sys; print("%d.%d" % sys.version_info[:2])')"
PY_MINOR="$($PY -c 'import sys; print(sys.version_info[1])')"
PY_MAJOR="$($PY -c 'import sys; print(sys.version_info[0])')"
ok "Found Python $PY_VER ($PY)"

if [ "$PY_MAJOR" -eq 3 ] && [ "$PY_MINOR" -ge 14 ]; then
  warn "Python $PY_VER is very new; some dependencies may not have wheels yet."
  warn "If install fails below, install Python 3.12 (https://www.python.org/downloads/release/python-3127/"
  warn "or 'brew install python@3.12' on macOS) and re-run this script."
fi

# ---------------------------------------------------------------------------
# 2. Virtualenv + install
# ---------------------------------------------------------------------------
if [ ! -d .venv ]; then
  say "Creating virtual environment (.venv)..."
  if "$PY" -m venv .venv 2>/tmp/google-mcp-venv-err; then
    ok "Created .venv"
  else
    warn "Standard venv creation failed (this is the ensurepip/pip bootstrap bug on new Python)."
    warn "Retrying without bundled pip, then installing pip manually..."
    rm -rf .venv
    "$PY" -m venv --without-pip .venv
    # shellcheck disable=SC1091
    source .venv/bin/activate
    if python -m ensurepip --upgrade >/dev/null 2>&1; then
      ok "Bootstrapped pip via ensurepip"
    elif command -v curl >/dev/null 2>&1; then
      curl -fsSL https://bootstrap.pypa.io/get-pip.py -o /tmp/google-mcp-get-pip.py
      python /tmp/google-mcp-get-pip.py
      ok "Bootstrapped pip via get-pip.py"
    else
      warn "Could not bootstrap pip automatically."
      warn "Please install Python 3.12 (https://www.python.org/downloads/) and re-run this script."
      exit 1
    fi
    deactivate 2>/dev/null || true
    ok "Created .venv"
  fi
else
  ok ".venv already exists"
fi

# shellcheck disable=SC1091
source .venv/bin/activate

# Make sure pip exists inside the venv even if a previous run left it half-built.
if ! python -m pip --version >/dev/null 2>&1; then
  python -m ensurepip --upgrade >/dev/null 2>&1 || true
fi

say "Installing the server (this can take a minute)..."
python -m pip install --quiet --upgrade pip
if ! python -m pip install --quiet -e .; then
  warn "Install failed. This is most often a too-new Python lacking dependency wheels."
  warn "Fix: install Python 3.12, delete the .venv folder, and re-run this script:"
  warn "    rm -rf tools/google-mcp-server/.venv && bash tools/google-mcp-server/setup.sh"
  exit 1
fi
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

# ---------------------------------------------------------------------------
# 7. Register with Claude Code (using the venv's Python, by absolute path)
# ---------------------------------------------------------------------------
# IMPORTANT: the MCP client must launch THIS venv's python, not the system one,
# or it won't find the installed package. Use the absolute interpreter path.
VENV_PY="$SCRIPT_DIR/.venv/bin/python"

if command -v claude >/dev/null 2>&1; then
  say "Registering the server with Claude Code..."
  # Remove any earlier (possibly broken) registration, then add the correct one.
  claude mcp remove google-second >/dev/null 2>&1 || true
  if claude mcp add google-second -- "$VENV_PY" -m google_mcp.server; then
    ok "Registered MCP server 'google-second'. RESTART Claude Code to use it."
  else
    warn "Auto-registration failed. Run this yourself, then restart Claude Code:"
    echo "    claude mcp remove google-second"
    echo "    claude mcp add google-second -- \"$VENV_PY\" -m google_mcp.server"
  fi
else
  warn "The 'claude' command isn't on your PATH, so I can't auto-register."
  echo "Run this yourself, then restart Claude Code:"
  echo "    claude mcp add google-second -- \"$VENV_PY\" -m google_mcp.server"
fi

cat <<EOF

------------------------------------------------------------------------
DONE. Useful follow-up commands (run them after \`source .venv/bin/activate\`
inside tools/google-mcp-server):

  google-mcp-add-account            # add another alias
  google-mcp-add-account --list     # see connected aliases
  google-mcp-add-account --remove you@example.com

The server is registered for Claude Code as 'google-second' using:
  $VENV_PY -m google_mcp.server
------------------------------------------------------------------------
EOF
