"""Tests for the encrypted, per-account token store."""

from __future__ import annotations

import importlib
import os
import stat

import pytest
from cryptography.fernet import Fernet
from google.oauth2.credentials import Credentials


def _make_creds() -> Credentials:
    return Credentials(
        token="access-token",
        refresh_token="refresh-token-secret",
        token_uri="https://oauth2.googleapis.com/token",
        client_id="client-id",
        client_secret="client-secret",
        scopes=["https://www.googleapis.com/auth/gmail.readonly"],
    )


def _store(env):
    from google_mcp import auth
    importlib.reload(auth)
    return auth


def test_roundtrip_and_permissions(env):
    auth = _store(env)
    store = auth.TokenStore()
    store.save("alias2@example.com", _make_creds())

    accounts = store.list_accounts()
    assert [a.email for a in accounts] == ["alias2@example.com"]

    # File is locked down to owner read/write only.
    path = accounts[0].path
    assert stat.S_IMODE(os.stat(path).st_mode) == 0o600

    loaded = store.load("alias2@example.com")
    assert loaded.refresh_token == "refresh-token-secret"


def test_ciphertext_has_no_plaintext_secrets(env):
    auth = _store(env)
    store = auth.TokenStore()
    store.save("alias2@example.com", _make_creds())
    blob = store.list_accounts()[0].path.read_bytes()
    assert b"refresh-token-secret" not in blob
    assert b"alias2@example.com" not in blob


def test_remove(env):
    auth = _store(env)
    store = auth.TokenStore()
    store.save("alias2@example.com", _make_creds())
    assert store.remove("alias2@example.com") is True
    assert store.remove("alias2@example.com") is False
    assert store.list_accounts() == []


def test_load_missing_returns_none(env):
    auth = _store(env)
    assert auth.TokenStore().load("nobody@example.com") is None


def test_get_credentials_unknown_account_raises(env):
    auth = _store(env)
    with pytest.raises(RuntimeError, match="No authorized account"):
        auth.get_credentials("nobody@example.com")


def test_wrong_key_cannot_decrypt(env, monkeypatch):
    auth = _store(env)
    auth.TokenStore().save("alias2@example.com", _make_creds())

    # Rotate the key; the existing file must now be undecryptable.
    monkeypatch.setenv("GOOGLE_MCP_ENCRYPTION_KEY",
                       Fernet.generate_key().decode("utf-8"))
    from google_mcp import config
    importlib.reload(config)
    auth = _store(env={})
    store = auth.TokenStore()
    # list_accounts skips unreadable entries rather than crashing.
    assert store.list_accounts() == []
    with pytest.raises(RuntimeError, match="does not match"):
        store.load("alias2@example.com")
