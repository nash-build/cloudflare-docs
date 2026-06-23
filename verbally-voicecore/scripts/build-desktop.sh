#!/usr/bin/env bash
# Build the desktop reference CLI (with live mic + ElevenLabs streaming) and the
# C-linkable engine libs for Windows / macOS / Linux.
#
# Linux note: the `live` feature needs ALSA headers (apt install libasound2-dev).
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> core engine libs (release)"
cargo build --release -p verbally-ffi

echo "==> reference CLI"
if [[ "${WITH_LIVE:-1}" == "1" ]]; then
  cargo build --release -p verbally-cli --features live
else
  cargo build --release -p verbally-cli
fi

echo "done."
echo "  engine:  target/release/ (libverbally_ffi.*)"
echo "  cli:     target/release/verbally"
echo
echo "Try:  ./target/release/verbally enroll you.wav you.profile"
echo "      ./target/release/verbally live --agent-id <AGENT_ID> --profile you.profile"
