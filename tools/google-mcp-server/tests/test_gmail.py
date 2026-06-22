"""Tests for Gmail helpers that don't require network access."""

from __future__ import annotations

import base64
import importlib

import pytest


def _b64(text: str) -> str:
    return base64.urlsafe_b64encode(text.encode("utf-8")).decode("utf-8")


def test_header_lookup_is_case_insensitive():
    from google_mcp import gmail
    headers = [{"name": "From", "value": "a@b.com"},
               {"name": "Subject", "value": "Hi"}]
    assert gmail._header(headers, "from") == "a@b.com"
    assert gmail._header(headers, "SUBJECT") == "Hi"
    assert gmail._header(headers, "Missing") == ""


def test_decode_body_prefers_plain_text():
    from google_mcp import gmail
    payload = {
        "mimeType": "multipart/alternative",
        "parts": [
            {"mimeType": "text/plain", "body": {"data": _b64("plain body")}},
            {"mimeType": "text/html", "body": {"data": _b64("<b>html</b>")}},
        ],
    }
    assert gmail._decode_body(payload) == "plain body"


def test_decode_body_falls_back_to_html():
    from google_mcp import gmail
    payload = {
        "mimeType": "text/html",
        "body": {"data": _b64("<p>only html</p>")},
    }
    assert gmail._decode_body(payload) == "<p>only html</p>"


def test_decode_body_handles_nested_parts():
    from google_mcp import gmail
    payload = {
        "mimeType": "multipart/mixed",
        "parts": [
            {"mimeType": "multipart/alternative", "parts": [
                {"mimeType": "text/plain", "body": {"data": _b64("nested")}},
            ]},
        ],
    }
    assert gmail._decode_body(payload) == "nested"


def test_decode_body_empty_when_no_text():
    from google_mcp import gmail
    assert gmail._decode_body({"mimeType": "image/png", "body": {}}) == ""


def test_send_blocked_when_scope_disabled(env):
    from google_mcp import config, gmail
    importlib.reload(config)
    importlib.reload(gmail)
    with pytest.raises(RuntimeError, match="Sending is disabled"):
        gmail.send_message("a@b.com", "to@x.com", "subj", "body")
