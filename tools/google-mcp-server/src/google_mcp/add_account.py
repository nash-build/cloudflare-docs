"""CLI to authorize, list, or remove Google aliases.

    python -m google_mcp.add_account               # authorize a new alias
    python -m google_mcp.add_account --list        # list authorized aliases
    python -m google_mcp.add_account --remove EMAIL # remove an alias' tokens
"""

from __future__ import annotations

import argparse
import sys

from . import auth, config


def main() -> int:
    parser = argparse.ArgumentParser(description="Manage Google MCP aliases.")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--list", action="store_true", help="List authorized aliases.")
    group.add_argument("--remove", metavar="EMAIL", help="Remove an alias' tokens.")
    args = parser.parse_args()

    if args.list:
        accounts = auth.TokenStore().list_accounts()
        if not accounts:
            print("No accounts authorized yet.")
        else:
            print("Authorized accounts:")
            for a in accounts:
                print(f"  - {a.email}")
        return 0

    if args.remove:
        removed = auth.revoke_local(args.remove)
        if removed:
            print(f"Removed local tokens for {args.remove}.")
            print("To fully revoke access, also visit "
                  "https://myaccount.google.com/permissions")
        else:
            print(f"No stored tokens found for {args.remove}.")
        return 0 if removed else 1

    # Default: authorize a new account.
    try:
        scopes = config.get_scopes()
        print(f"Requesting scopes: {', '.join(scopes)}")
        email = auth.authorize_new_account()
    except Exception as exc:  # pragma: no cover - user-facing CLI error
        print(f"Authorization failed: {exc}", file=sys.stderr)
        return 1
    print(f"Authorized: {email}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
