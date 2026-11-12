#!/usr/bin/env bash
BASE=/home/hs01/.nvm/versions/node/v24.19.0/lib/node_modules/cf
echo "=== files mentioning refresh_token ==="
grep -rli 'refresh_token' "$BASE" 2>/dev/null | head -10
echo "=== files mentioning 'Not logged in' ==="
grep -rli "not logged in" "$BASE" 2>/dev/null | head -10
echo "=== files mentioning expiration_time ==="
grep -rli 'expiration_time' "$BASE" 2>/dev/null | head -10
