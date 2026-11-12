#!/usr/bin/env bash
BASE=/home/hs01/.nvm/versions/node/v24.19.0/lib/node_modules/cf
echo "=== top-level dist dirs ==="
ls "$BASE" "$BASE/dist" 2>/dev/null
echo "=== search dist only (timeout 60s per grep) ==="
timeout 60 grep -rli 'refresh_token' "$BASE/dist" 2>/dev/null | head -5
timeout 60 grep -rli 'not logged in' "$BASE/dist" 2>/dev/null | head -5
timeout 60 grep -rli 'expiration_time' "$BASE/dist" 2>/dev/null | head -5
echo "=== search @cloudflare oauth-ish packages ==="
timeout 60 grep -rli 'refresh_token' "$BASE/node_modules/@cloudflare" 2>/dev/null | grep -v miniflare | head -5
