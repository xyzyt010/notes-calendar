#!/usr/bin/env bash
export PATH=/home/hs01/.nvm/versions/node/v24.19.0/bin:$PATH
echo "=== expiration_time (value, not a secret) ==="
python3 -c 'import json; d=json.load(open("/home/hs01/.config/cloudflare/config/default.json")); print(d.get("expiration_time")); print("scopes:", d.get("scopes"))'
echo "=== now (UTC) ==="
date -u +%Y-%m-%dT%H:%M:%SZ
echo "=== whoami with --profile default ==="
timeout 25 cf auth whoami --profile default 2>&1 | head -10
echo "=== activate default in /tmp and whoami ==="
mkdir -p /tmp/cf-zone-check && cd /tmp/cf-zone-check && timeout 25 cf auth activate default 2>&1 | head -10 && timeout 25 cf auth whoami 2>&1 | head -10
echo "=== dns list with --profile default ==="
timeout 25 cf dns records list --profile default --zone=yellod.dpdns.org 2>&1 | head -25
