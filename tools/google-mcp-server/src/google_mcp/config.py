"""Configuration loading for the Google MCP server.

All sensitive values come from the environment (or a local, git-ignored `.env`).
Nothing here is committed to source control.
"""

from __future__ import annotations

import os
from pathlib import Path

from dotenv import load_dotenv

# Load a local .env if present (the .env file itself is git-ignored).
# Prefer the .env that ships next to this package so the server works no matter
# which directory it is launched from (e.g. when an MCP client spawns it). Fall
# back to the normal cwd-upward search if that file isn't there.
_PACKAGE_ENV = Path(__file__).resolve().parents[2] / ".env"
if _PACKAGE_ENV.is_file():
    load_dotenv(_PACKAGE_ENV)
else:
    load_dotenv()

# Least-privilege default: read-only access to Gmail.
DEFAULT_SCOPES = ["https://www.googleapis.com/auth/gmail.readonly"]

# Scope required before the `send_message` tool is allowed to operate.
SEND_SCOPE = "https://www.googleapis.com/auth/gmail.send"


def _default_token_dir() -> Path:
    base = os.environ.get("XDG_CONFIG_HOME") or os.path.join(
        os.path.expanduser("~"), ".config"
    )
    return Path(base) / "google-mcp" / "tokens"


def get_scopes() -> list[str]:
    raw = os.environ.get("GOOGLE_MCP_SCOPES", "").strip()
    if not raw:
        return list(DEFAULT_SCOPES)
    return [s for s in raw.split() if s]


def get_client_secrets_file() -> Path:
    value = os.environ.get("GOOGLE_CLIENT_SECRETS_FILE", "").strip()
    if not value:
        raise RuntimeError(
            "GOOGLE_CLIENT_SECRETS_FILE is not set. Point it at the OAuth "
            "'Desktop app' client JSON downloaded from Google Cloud."
        )
    path = Path(value).expanduser()
    if not path.is_file():
        raise RuntimeError(f"OAuth client secrets file not found: {path}")
    return path


def get_token_dir() -> Path:
    value = os.environ.get("GOOGLE_MCP_TOKEN_DIR", "").strip()
    token_dir = Path(value).expanduser() if value else _default_token_dir()
    token_dir.mkdir(parents=True, exist_ok=True)
    # Restrict directory permissions to the owner.
    try:
        token_dir.chmod(0o700)
    except OSError:
        pass
    return token_dir


def get_encryption_key() -> bytes:
    value = os.environ.get("GOOGLE_MCP_ENCRYPTION_KEY", "").strip()
    if not value:
        raise RuntimeError(
            "GOOGLE_MCP_ENCRYPTION_KEY is not set. Generate one with "
            "`python -m google_mcp.keygen` and add it to your .env."
        )
    return value.encode("utf-8")


def send_enabled() -> bool:
    return SEND_SCOPE in get_scopes()
