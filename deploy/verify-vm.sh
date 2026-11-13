#!/usr/bin/env bash
export XDG_RUNTIME_DIR=/run/user/1000
echo "=== unit ==="
cat /home/hs01/.config/systemd/user/suite-web.service
echo
echo "=== static ==="
ls /home/hs01/suite-web/static/
echo
echo "=== routes ==="
for p in / /notes /notes.html /calendar.html /api/health /api/notes /api/events; do
  printf '%-16s ' "$p"
  curl -s -o /dev/null -w '%{http_code} -> %{redirect_url}\n' http://127.0.0.1:8760$p
done
echo "=== login roundtrip ==="
curl -s -c /tmp/suite-cookies.txt -X POST http://127.0.0.1:8760/api/auth/login -H 'Content-Type: application/json' -d '{"username":"admin","password":"admin"}'; echo
curl -s -b /tmp/suite-cookies.txt http://127.0.0.1:8760/api/auth/me; echo
curl -s -b /tmp/suite-cookies.txt http://127.0.0.1:8760/api/calendars; echo
