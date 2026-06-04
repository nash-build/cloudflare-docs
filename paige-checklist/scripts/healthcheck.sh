#!/usr/bin/env bash
# Verify a deployed Paige backend end-to-end.
# Usage:  bash scripts/healthcheck.sh <worker-url> <PAIGE_TOKEN>
set -euo pipefail

URL="${1:-}"; TOKEN="${2:-}"
if [ -z "$URL" ] || [ -z "$TOKEN" ]; then
  echo "Usage: bash scripts/healthcheck.sh <worker-url> <PAIGE_TOKEN>" >&2
  exit 1
fi
URL="${URL%/}"
A=(-H "authorization: Bearer $TOKEN")
code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }

echo "── Paige backend health check: $URL ──"
echo "PWA served (/) ............ $(code "$URL/")          (expect 200)"
echo "Auth gate (/list, no token) $(code "$URL/list")      (expect 401)"
echo "List (/list, with token) .. $(code "${A[@]}" "$URL/list")   (expect 200)"

echo -n "Screenshot reader ......... "
SS="$(curl -s "${A[@]}" "$URL/screenshot?query=test" || true)"
if printf '%s' "$SS" | grep -q '"description"'; then
  echo "OK (Drive + Claude configured and responding)"
elif printf '%s' "$SS" | grep -q 'not configured'; then
  echo "not configured yet (add Drive/Anthropic secrets to enable)"
else
  echo "response: $(printf '%s' "$SS" | head -c 160)"
fi

echo "── done ──"
