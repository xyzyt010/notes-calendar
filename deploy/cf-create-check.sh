#!/usr/bin/env bash
export PATH=/home/hs01/.nvm/versions/node/v24.19.0/bin:$PATH
echo "=== dns create help ==="
timeout 30 cf dns records create --help 2>&1 | head -45
echo "=== tunnels candidate commands ==="
for sub in tunnels; do timeout 30 cf zero-trust "$sub" --help 2>&1 | head -20; done
echo "=== connectivity settings help ==="
timeout 30 cf zero-trust connectivity-settings --help 2>&1 | head -25
