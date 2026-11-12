#!/usr/bin/env bash
export PATH=/home/hs01/.nvm/versions/node/v24.19.0/bin:$PATH
echo "=== cf real script path ==="
ls -la /home/hs01/.nvm/versions/node/v24.19.0/bin/cf
echo "=== find bundle, look for oauth endpoints (context only, no secrets printed) ==="
CF_TARGET=$(readlink -f /home/hs01/.nvm/versions/node/v24.19.0/bin/cf)
echo "target=$CF_TARGET"
grep -oE 'https://[a-zA-Z0-9./_-]*(oauth|token)[a-zA-Z0-9./_-]*' "$CF_TARGET" 2>/dev/null | sort | uniq -c | head -20
echo "=== refresh_token usage hints ==="
grep -oE '.{80}refresh_token.{80}' "$CF_TARGET" 2>/dev/null | head -5
echo "=== login/create help ==="
timeout 25 cf auth login --help 2>&1 | head -25
timeout 25 cf auth create --help 2>&1 | head -25
