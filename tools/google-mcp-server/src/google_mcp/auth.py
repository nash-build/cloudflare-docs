"""OAuth + encrypted, per-account token storage.

Design notes (security best practices):

* Uses the loopback (127.0.0.1) installed-app flow with PKCE — the deprecated
  out-of-band copy/paste flow is never used.
* A random CSRF `state` is generated and verified by ``run_local_server``.
* Tokens are encrypted at rest with Fernet and written with ``0600`` perms.
* Each account is stored in its own file keyed by a hash of its email, so
  aliases are isolated and can be added/removed independently.
"""

from __future__ import annotations

import hashlib
import json
import os
from dataclasses import dataclass
from pathlib import Path

from cryptography.fernet import Fernet, InvalidToken
from google.auth.transport.requests import Request
from google.oauth2.credentials import Credentials
from google_auth_oauthlib.flow import InstalledAppFlow
from googleapiclient.discovery import build

from . import config


def _account_filename(email: str) -> str:
    digest = hashlib.sha256(email.lower().encode("utf-8")).hexdigest()[:16]
    return f"acct_{digest}.enc"


@dataclass
class StoredAccount:
    email: str
    path: Path


class TokenStore:
    """Encrypted token store, one file per Google account."""

    def __init__(self) -> None:
        self._dir = config.get_token_dir()
        self._fernet = Fernet(config.get_encryption_key())

    def _write_encrypted(self, path: Path, payload: dict) -> None:
        token = self._fernet.encrypt(json.dumps(payload).encode("utf-8"))
        # Write to a temp file then move, and lock down permissions.
        tmp = path.with_suffix(path.suffix + ".tmp")
        with open(tmp, "wb") as fh:
            fh.write(token)
        os.chmod(tmp, 0o600)
        os.replace(tmp, path)
        os.chmod(path, 0o600)

    def _read_encrypted(self, path: Path) -> dict:
        with open(path, "rb") as fh:
            blob = fh.read()
        try:
            data = self._fernet.decrypt(blob)
        except InvalidToken as exc:  # wrong/rotated encryption key
            raise RuntimeError(
                f"Could not decrypt {path.name}. The GOOGLE_MCP_ENCRYPTION_KEY "
                "does not match the key used to store this account."
            ) from exc
        return json.loads(data)

    def save(self, email: str, creds: Credentials) -> None:
        payload = {
            "email": email,
            "credentials": json.loads(creds.to_json()),
        }
        self._write_encrypted(self._dir / _account_filename(email), payload)

    def load(self, email: str) -> Credentials | None:
        path = self._dir / _account_filename(email)
        if not path.is_file():
            return None
        payload = self._read_encrypted(path)
        return Credentials.from_authorized_user_info(payload["credentials"])

    def remove(self, email: str) -> bool:
        path = self._dir / _account_filename(email)
        if path.is_file():
            path.unlink()
            return True
        return False

    def list_accounts(self) -> list[StoredAccount]:
        accounts: list[StoredAccount] = []
        for path in sorted(self._dir.glob("acct_*.enc")):
            try:
                payload = self._read_encrypted(path)
                accounts.append(StoredAccount(email=payload["email"], path=path))
            except Exception:
                # Skip unreadable entries rather than failing the whole list.
                continue
        return accounts


def get_credentials(email: str) -> Credentials:
    """Return valid credentials for an account, refreshing if needed."""
    store = TokenStore()
    creds = store.load(email)
    if creds is None:
        raise RuntimeError(
            f"No authorized account for '{email}'. Run "
            "`python -m google_mcp.add_account` to authorize it."
        )
    if not creds.valid and creds.expired and creds.refresh_token:
        creds.refresh(Request())
        store.save(email, creds)
    if not creds.valid:
        raise RuntimeError(
            f"Credentials for '{email}' are invalid. Re-run "
            "`python -m google_mcp.add_account` to re-authorize."
        )
    return creds


def authorize_new_account() -> str:
    """Run the interactive consent flow and persist the resulting tokens.

    Returns the email address that was authorized.
    """
    scopes = config.get_scopes()
    flow = InstalledAppFlow.from_client_secrets_file(
        str(config.get_client_secrets_file()), scopes=scopes
    )
    # Loopback flow with PKCE + CSRF state handled by run_local_server.
    # access_type=offline + prompt=consent ensures we receive a refresh token.
    creds = flow.run_local_server(
        port=0,
        prompt="consent",
        access_type="offline",
        authorization_prompt_message=(
            "Opening your browser to authorize a Google account. "
            "Sign in with the alias you want to add.\n{url}"
        ),
        success_message=(
            "Authorization complete. You can close this tab and return to the terminal."
        ),
    )

    email = _fetch_email(creds)
    TokenStore().save(email, creds)
    return email


def _fetch_email(creds: Credentials) -> str:
    """Resolve the authorized account's primary email via the Gmail profile."""
    service = build("gmail", "v1", credentials=creds, cache_discovery=False)
    profile = service.users().getProfile(userId="me").execute()
    return profile["emailAddress"]


def revoke_local(email: str) -> bool:
    """Remove an account's locally stored tokens."""
    return TokenStore().remove(email)
