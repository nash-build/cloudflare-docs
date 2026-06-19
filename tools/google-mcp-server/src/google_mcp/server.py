"""MCP server entry point exposing Gmail tools for multiple aliases.

Run directly (stdio transport):

    python -m google_mcp.server
"""

from __future__ import annotations

from mcp.server.fastmcp import FastMCP

from . import auth, config, gmail

mcp = FastMCP("google-mcp-server")


@mcp.tool()
def list_accounts() -> list[str]:
    """List the Gmail aliases that have been authorized on this server."""
    return [a.email for a in auth.TokenStore().list_accounts()]


@mcp.tool()
def search_messages(account: str, query: str, max_results: int = 10) -> list[dict]:
    """Search a mailbox using Gmail query syntax.

    Args:
        account: The email address of the authorized alias to search.
        query: A Gmail search query, e.g. "from:alice@example.com is:unread".
        max_results: Maximum number of messages to return (1-50).
    """
    return gmail.search_messages(account, query, max_results)


@mcp.tool()
def get_message(account: str, message_id: str) -> dict:
    """Fetch a single message (headers + body) by its id.

    Args:
        account: The email address of the authorized alias.
        message_id: The Gmail message id (from search_messages).
    """
    return gmail.get_message(account, message_id)


@mcp.tool()
def list_labels(account: str) -> list[dict]:
    """List labels/folders for an account.

    Args:
        account: The email address of the authorized alias.
    """
    return gmail.list_labels(account)


@mcp.tool()
def send_message(account: str, to: str, subject: str, body: str,
                 cc: str | None = None) -> dict:
    """Send an email. Requires the gmail.send scope to be enabled.

    Args:
        account: The authorized alias to send from.
        to: Recipient address(es).
        subject: Email subject.
        body: Plain-text body.
        cc: Optional CC address(es).
    """
    return gmail.send_message(account, to, subject, body, cc)


def main() -> None:
    # Surface configuration errors early with a clear message.
    config.get_scopes()
    mcp.run()


if __name__ == "__main__":
    main()
