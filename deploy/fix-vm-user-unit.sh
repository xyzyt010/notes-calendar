#!/usr/bin/env bash
set -e
printf '%s\n' \
'[Unit]' \
'Description=yellod suite (Notes & Calendar web apps)' \
'After=network-online.target' \
'Wants=network-online.target' \
'' \
'[Service]' \
'Type=simple' \
'WorkingDirectory=%h/suite-web' \
'Environment=SUITE_STATIC_DIR=%h/suite-web/static' \
'Environment=SUITE_DB=%h/suite-web/suite.db' \
'Environment=SUITE_BIND=127.0.0.1:8760' \
'Environment=SUITE_DISABLE_NOTES=1' \
'ExecStart=%h/suite-web/suite-web' \
'Restart=always' \
'RestartSec=5' \
'' \
'[Install]' \
'WantedBy=default.target' \
> /home/hs01/.config/systemd/user/suite-web.service
export XDG_RUNTIME_DIR=/run/user/1000
systemctl --user daemon-reload
systemctl --user start suite-web
sleep 2
systemctl --user status suite-web --no-pager | head -8
echo "=== health ==="
curl -s http://127.0.0.1:8760/api/health; echo
echo "=== root redirect ==="
curl -s -o /dev/null -w '/ -> %{http_code} -> %{redirect_url}\n' http://127.0.0.1:8760/
echo "=== notes redirect ==="
curl -s -o /dev/null -w '/notes.html -> %{http_code} -> %{redirect_url}\n' http://127.0.0.1:8760/notes.html
echo "=== notes api ==="
curl -s -o /dev/null -w '/api/notes -> %{http_code}\n' http://127.0.0.1:8760/api/notes
echo "=== calendar ==="
curl -s -o /dev/null -w '/calendar.html -> %{http_code}\n' http://127.0.0.1:8760/calendar.html
