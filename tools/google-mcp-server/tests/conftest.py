"""Pytest fixtures shared across the test suite."""

from __future__ import annotations

import importlib

import pytest
from cryptography.fernet import Fernet


@pytest.fixture()
def env(monkeypatch, tmp_path):
    """Provide an isolated, fully-configured environment for a test.

    Sets an encryption key + a temp token dir and reloads the config module so
    no test touches the real user config directory.
    """
    key = Fernet.generate_key().decode("utf-8")
    monkeypatch.setenv("GOOGLE_MCP_ENCRYPTION_KEY", key)
    monkeypatch.setenv("GOOGLE_MCP_TOKEN_DIR", str(tmp_path / "tokens"))
    monkeypatch.setenv("GOOGLE_MCP_SCOPES",
                       "https://www.googleapis.com/auth/gmail.readonly")
    monkeypatch.delenv("GOOGLE_CLIENT_SECRETS_FILE", raising=False)

    from google_mcp import config
    importlib.reload(config)
    return {"key": key, "token_dir": tmp_path / "tokens"}
