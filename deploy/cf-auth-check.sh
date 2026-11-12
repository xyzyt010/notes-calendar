#!/usr/bin/env bash
export PATH=/home/hs01/.nvm/versions/node/v24.19.0/bin:$PATH
echo "=== config dir ==="
ls -la ~/.config/cloudflare/config/
echo "=== auth list ==="
timeout 25 cf auth list 2>&1 | head -30
echo "=== whoami ==="
timeout 25 cf auth whoami 2>&1 | head -15
echo "=== whoami from notes-app dir ==="
cd /home/hs01/notes-app 2>/dev/null && timeout 25 cf auth whoami 2>&1 | head -15
echo "=== env profile var presence ==="
env | grep -cE '^CLOUDFLARE_[A-Z_]+' || true
echo "=== dns list (default profile) ==="
timeout 25 cf dns records list --zone yellod.dpdns.org 2>&1 | head -30
