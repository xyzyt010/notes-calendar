#!/usr/bin/env bash
export PATH=/home/hs01/.nvm/versions/node/v24.19.0/bin:$PATH
CF_TARGET=$(readlink -f /home/hs01/.nvm/versions/node/v24.19.0/bin/cf)
echo "target=$CF_TARGET"
ls -la "$CF_TARGET" 2>&1 | head -2
wc -c "$CF_TARGET" 2>&1
head -c 300 "$CF_TARGET"
echo
ls -la "$(dirname "$CF_TARGET")/../lib/node_modules" 2>&1 | head -10
