"""Thin Gmail API helpers built on top of authorized credentials."""

from __future__ import annotations

import base64
from email.message import EmailMessage
from typing import Any

from googleapiclient.discovery import build

from . import auth, config


def _service(account: str):
    creds = auth.get_credentials(account)
    return build("gmail", "v1", credentials=creds, cache_discovery=False)


def _header(headers: list[dict], name: str) -> str:
    for h in headers:
        if h.get("name", "").lower() == name.lower():
            return h.get("value", "")
    return ""


def _decode_body(payload: dict) -> str:
    """Best-effort extraction of a plain-text body from a message payload."""

    def walk(part: dict) -> str | None:
        mime = part.get("mimeType", "")
        body = part.get("body", {})
        data = body.get("data")
        if mime == "text/plain" and data:
            return base64.urlsafe_b64decode(data).decode("utf-8", "replace")
        for sub in part.get("parts", []) or []:
            found = walk(sub)
            if found:
                return found
        # Fall back to HTML if no plain text was found at this level.
        if mime == "text/html" and data:
            return base64.urlsafe_b64decode(data).decode("utf-8", "replace")
        return None

    return walk(payload) or ""


def search_messages(account: str, query: str, max_results: int = 10) -> list[dict[str, Any]]:
    svc = _service(account)
    resp = (
        svc.users()
        .messages()
        .list(userId="me", q=query, maxResults=max(1, min(max_results, 50)))
        .execute()
    )
    results = []
    for ref in resp.get("messages", []):
        msg = (
            svc.users()
            .messages()
            .get(userId="me", id=ref["id"], format="metadata",
                 metadataHeaders=["From", "To", "Subject", "Date"])
            .execute()
        )
        headers = msg.get("payload", {}).get("headers", [])
        results.append(
            {
                "id": msg["id"],
                "threadId": msg.get("threadId"),
                "from": _header(headers, "From"),
                "to": _header(headers, "To"),
                "subject": _header(headers, "Subject"),
                "date": _header(headers, "Date"),
                "snippet": msg.get("snippet", ""),
            }
        )
    return results


def get_message(account: str, message_id: str) -> dict[str, Any]:
    svc = _service(account)
    msg = (
        svc.users()
        .messages()
        .get(userId="me", id=message_id, format="full")
        .execute()
    )
    payload = msg.get("payload", {})
    headers = payload.get("headers", [])
    return {
        "id": msg["id"],
        "threadId": msg.get("threadId"),
        "labelIds": msg.get("labelIds", []),
        "from": _header(headers, "From"),
        "to": _header(headers, "To"),
        "cc": _header(headers, "Cc"),
        "subject": _header(headers, "Subject"),
        "date": _header(headers, "Date"),
        "snippet": msg.get("snippet", ""),
        "body": _decode_body(payload),
    }


def list_labels(account: str) -> list[dict[str, Any]]:
    svc = _service(account)
    resp = svc.users().labels().list(userId="me").execute()
    return [
        {"id": l["id"], "name": l["name"], "type": l.get("type")}
        for l in resp.get("labels", [])
    ]


def send_message(account: str, to: str, subject: str, body: str,
                 cc: str | None = None) -> dict[str, Any]:
    if not config.send_enabled():
        raise RuntimeError(
            "Sending is disabled. Add the gmail.send scope to GOOGLE_MCP_SCOPES "
            "and re-authorize the account before using send_message."
        )
    svc = _service(account)
    message = EmailMessage()
    message["To"] = to
    if cc:
        message["Cc"] = cc
    message["From"] = account
    message["Subject"] = subject
    message.set_content(body)
    raw = base64.urlsafe_b64encode(message.as_bytes()).decode("utf-8")
    sent = (
        svc.users()
        .messages()
        .send(userId="me", body={"raw": raw})
        .execute()
    )
    return {"id": sent["id"], "threadId": sent.get("threadId"), "status": "sent"}
