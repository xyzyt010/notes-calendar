#!/usr/bin/env bash
rm -f /home/hs01/suite-src.tar /home/hs01/verify-vm.sh /home/hs01/fix-vm-user-unit.sh /home/hs01/cf-*.sh /home/hs01/suite-build*.log /home/hs01/rustup-install.log /home/hs01/rustup-init.sh
rm -f /home/hs01/suite-web/static/index.html
echo "static: $(ls /home/hs01/suite-web/static)"
echo "=== public smoke tests ==="
for p in / /notes /notes.html /calendar.html /api/health /api/notes /api/events; do
  printf '%-16s ' "$p"
  curl -s -o /dev/null -w '%{http_code} -> %{redirect_url}\n' "https://calendar.yellod.dpdns.org$p"
done
