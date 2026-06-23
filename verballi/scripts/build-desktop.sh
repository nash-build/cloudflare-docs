#!/usr/bin/env bash
# Build the desktop reference CLI (with live mic + ElevenLabs streaming) and the
# C-linkable engine libs for Windows / macOS / Linux.
#
# Linux note: the `live` feature needs ALSA headers (apt install libasound2-dev).
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> core engine libs (release)"
cargo build --release -p verballi-ffi

echo "==> reference CLI"
if [[ "${WITH_LIVE:-1}" == "1" ]]; then
  cargo build --release -p verballi-cli --features live
else
  cargo build --release -p verballi-cli
fi

echo "done."
echo "  engine:  target/release/ (libverballi_ffi.*)"
echo "  cli:     target/release/verballi"
echo
echo "Try:  ./target/release/verballi enroll you.wav you.profile"
echo "      ./target/release/verballi live --agent-id <AGENT_ID> --profile you.profile"
