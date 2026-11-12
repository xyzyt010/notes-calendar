#!/usr/bin/env bash
BASE=/home/hs01/.nvm/versions/node/v24.19.0/lib/node_modules/cf/dist
echo "=== auth module: how it validates (first 120 matching lines, trimmed) ==="
grep -n -E 'not logged in|authenticated|expiration|refresh' "$BASE/auth-CZqics6E.mjs" | head -40
echo "=== auth-Cp7gT-AF ==="
grep -n -E 'not logged in|authenticated|expiration|refresh' "$BASE/auth-Cp7gT-AF.mjs" | head -40
echo "=== login module ==="
grep -n -E 'sameFile|refresh|expiry|expires|offline' "$BASE/login-CEQbOgBM.mjs" | head -40
echo "=== login module DEWv ==="
ls "$BASE"/login-DEWvGuaU.mjs && grep -n -E 'sameFile|refresh|expiry|expires|offline' "$BASE/login-DEWvGuaU.mjs" | head -40
