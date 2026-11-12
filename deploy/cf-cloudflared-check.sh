#!/usr/bin/env bash
BASE=/home/hs01/.nvm/versions/node/v24.19.0/lib/node_modules/cf/dist
echo "=== cloudflared module head ==="
head -c 3000 "$BASE/cloudflared-DhItYrCq.mjs"
echo
echo "=== strings: command names in cloudflared module ==="
grep -oE '"[a-z-]+",|"[a-z-]+",' "$BASE/cloudflared-DhItYrCq.mjs" | sort | uniq -c | sort -rn | head -30
echo "=== config.yml mentions ==="
grep -c 'config.yml' "$BASE/cloudflared-DhItYrCq.mjs" 2>/dev/null
grep -oE '.{60}config\.yml.{60}' "$BASE/cloudflared-DhItYrCq.mjs" | head -5
