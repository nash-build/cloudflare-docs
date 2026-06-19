"""Generate a Fernet key for encrypting the local token store.

    python -m google_mcp.keygen
"""

from __future__ import annotations

from cryptography.fernet import Fernet


def main() -> None:
    print(Fernet.generate_key().decode("utf-8"))


if __name__ == "__main__":
    main()
