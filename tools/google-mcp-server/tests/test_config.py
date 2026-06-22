"""Tests for configuration loading."""

from __future__ import annotations

import importlib

import pytest


def _reload_config():
    from google_mcp import config
    return importlib.reload(config)


def test_default_scopes_are_readonly(monkeypatch):
    monkeypatch.delenv("GOOGLE_MCP_SCOPES", raising=False)
    config = _reload_config()
    assert config.get_scopes() == [
        "https://www.googleapis.com/auth/gmail.readonly"
    ]
    assert config.send_enabled() is False


def test_send_enabled_when_scope_present(monkeypatch):
    monkeypatch.setenv(
        "GOOGLE_MCP_SCOPES",
        "https://www.googleapis.com/auth/gmail.readonly "
        "https://www.googleapis.com/auth/gmail.send",
    )
    config = _reload_config()
    assert config.send_enabled() is True


def test_missing_client_secrets_raises(monkeypatch):
    monkeypatch.delenv("GOOGLE_CLIENT_SECRETS_FILE", raising=False)
    config = _reload_config()
    with pytest.raises(RuntimeError, match="GOOGLE_CLIENT_SECRETS_FILE"):
        config.get_client_secrets_file()


def test_missing_encryption_key_raises(monkeypatch):
    monkeypatch.delenv("GOOGLE_MCP_ENCRYPTION_KEY", raising=False)
    config = _reload_config()
    with pytest.raises(RuntimeError, match="GOOGLE_MCP_ENCRYPTION_KEY"):
        config.get_encryption_key()
