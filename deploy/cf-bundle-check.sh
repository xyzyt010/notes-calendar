#!/usr/bin/env bash
export PATH=/home/hs01/.nvm/versions/node/v24.19.0/bin:$PATH
BASE=/home/hs01/.nvm/versions/node/v24.19.0/lib/node_modules/cf
echo "=== dist tree (top 2 levels, sizes) ==="
find "$BASE" -maxdepth 3 -type d | head -20
echo "=== largest js files ==="
find "$BASE" -name '*.js' -type f -printf '%s %p\n' 2>/dev/null | sort -rn | head -8
echo "=== oauth/token URLs (filenames only) ==="
grep -rlE 'oauth/(token|refresh)|auth/token|refresh_token' "$BASE" --include='*.js' 2>/dev/null | head -10
