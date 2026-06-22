# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.1.0] - 2026-06-22

### Added
- Multi-alias Gmail MCP server (`list_accounts`, `search_messages`,
  `get_message`, `list_labels`, `send_message`).
- OAuth 2.0 loopback + PKCE flow with CSRF `state` for adding accounts.
- Encrypted (Fernet), per-account token storage with `0600` permissions.
- Least-privilege scopes (read-only Gmail by default; send is opt-in).
- `add_account` CLI (`--list`, `--remove`) and `keygen` helper.
- Email-format validation on tool inputs and stderr logging.
- Test suite (config, token store, Gmail helpers) and CI workflow.
