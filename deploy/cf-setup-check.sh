#!/usr/bin/env bash
export PATH=/home/hs01/.nvm/versions/node/v24.19.0/bin:$PATH
echo "=== whoami ==="
timeout 30 cf auth whoami 2>&1 | head -10
echo "=== DNS records for yellod.dpdns.org ==="
timeout 30 cf dns records list --zone=yellod.dpdns.org 2>&1 | head -60
echo "=== full root help: tunnel candidates ==="
timeout 30 cf --help 2>&1 | grep -iE 'tunnel|network|argo' | head -10
