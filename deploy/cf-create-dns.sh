#!/usr/bin/env bash
export PATH=/home/hs01/.nvm/versions/node/v24.19.0/bin:$PATH
echo "=== dry-run ==="
timeout 30 cf dns records create --zone=yellod.dpdns.org --dry-run --body '{"type":"CNAME","name":"calendar.yellod.dpdns.org","content":"2707259a-a883-44b7-8095-63ffb4c0bca2.cfargotunnel.com","proxied":true,"ttl":1}' 2>&1 | head -40
echo
echo "=== real create ==="
timeout 30 cf dns records create --zone=yellod.dpdns.org --body '{"type":"CNAME","name":"calendar.yellod.dpdns.org","content":"2707259a-a883-44b7-8095-63ffb4c0bca2.cfargotunnel.com","proxied":true,"ttl":1}' 2>&1 | head -40
