#!/usr/bin/env bash
# Guided deploy for the Paige backend (Cloudflare Worker + KV).
# Run from anywhere:  bash paige-checklist/scripts/deploy.sh
set -euo pipefail
cd "$(dirname "$0")/../server"

echo "── Paige backend deploy ─────────────────────────────────────"
echo

# Cloudflare auth check (token in env, or interactive login).
if [ -z "${CLOUDFLARE_API_TOKEN:-}" ]; then
  if ! npx wrangler whoami >/dev/null 2>&1; then
    echo "You're not logged in to Cloudflare. Opening login…"
    npx wrangler login
  fi
fi

echo "1/5 Installing dependencies…"
npm install --silent

if grep -q "REPLACE_WITH_YOUR_KV_NAMESPACE_ID" wrangler.toml; then
  echo "2/5 Creating KV namespace PAIGE_KV…"
  OUT="$(npx wrangler kv namespace create PAIGE_KV)"
  echo "$OUT"
  ID="$(printf '%s' "$OUT" | grep -oE '[a-f0-9]{32}' | head -1)"
  if [ -z "$ID" ]; then
    echo "✗ Could not auto-detect the KV id above. Paste it into wrangler.toml, then re-run." >&2
    exit 1
  fi
  sed -i.bak "s/REPLACE_WITH_YOUR_KV_NAMESPACE_ID/$ID/" wrangler.toml && rm -f wrangler.toml.bak
  echo "   ✓ KV id wired into wrangler.toml: $ID"
else
  echo "2/5 KV namespace already configured — skipping."
fi

echo "3/5 Set the shared secret token (PAIGE_TOKEN)."
echo "    Use the SAME value later in the Mac config.json (sync.token) and the iPhone ⚙️ settings."
npx wrangler secret put PAIGE_TOKEN

printf "4/5 Configure Google Drive screenshot reading now? [y/N] "
read -r yn
if [[ "$yn" =~ ^[Yy]$ ]]; then
  npx wrangler secret put GDRIVE_CLIENT_ID
  npx wrangler secret put GDRIVE_CLIENT_SECRET
  npx wrangler secret put GDRIVE_REFRESH_TOKEN
  npx wrangler secret put ANTHROPIC_API_KEY
  printf "   Drive folder ID to limit the search (blank = search all of Drive): "
  read -r FOLDER
  if [ -n "$FOLDER" ]; then
    if grep -qE '^[[:space:]]*#?[[:space:]]*GDRIVE_FOLDER_ID' wrangler.toml; then
      sed -i.bak -E "s|^[[:space:]]*#?[[:space:]]*GDRIVE_FOLDER_ID.*|GDRIVE_FOLDER_ID = \"$FOLDER\"|" wrangler.toml && rm -f wrangler.toml.bak
    else
      printf '\n[vars]\nGDRIVE_FOLDER_ID = "%s"\n' "$FOLDER" >> wrangler.toml
    fi
    echo "   ✓ GDRIVE_FOLDER_ID set in wrangler.toml."
  fi
else
  echo "   Skipping screenshot secrets — you can add them later and re-deploy."
fi

echo "5/5 Deploying…"
npx wrangler deploy

echo
echo "✓ Deployed. Copy the https://…workers.dev URL printed above."
echo "  Next: run  bash scripts/healthcheck.sh <that-url> <PAIGE_TOKEN>"
