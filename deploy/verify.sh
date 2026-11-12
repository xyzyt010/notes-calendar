#!/usr/bin/env bash
set -e
echo "=== health ==="
curl -s http://127.0.0.1:8760/api/health; echo
echo "=== login ==="
curl -s -c /tmp/suite-cookies.txt -X POST http://127.0.0.1:8760/api/auth/login -H 'Content-Type: application/json' -d '{"username":"admin","password":"admin"}'; echo
echo "=== me ==="
curl -s -b /tmp/suite-cookies.txt http://127.0.0.1:8760/api/auth/me; echo
echo "=== create note ==="
NOTE=$(curl -s -b /tmp/suite-cookies.txt -X POST http://127.0.0.1:8760/api/notes -H 'Content-Type: application/json' -d '{"title":"Deploy verification","body":"suite-web is live on the VM"}')
echo "$NOTE"
NOTE_ID=$(python3 -c 'import sys,json; print(json.loads(sys.stdin.read()).get("id",""))' <<< "$NOTE")
echo "NOTE_ID=$NOTE_ID"
echo "=== list notes ==="
curl -s -b /tmp/suite-cookies.txt 'http://127.0.0.1:8760/api/notes?limit=5' | head -c 1000; echo
echo "=== calendars ==="
curl -s -b /tmp/suite-cookies.txt http://127.0.0.1:8760/api/calendars; echo
echo "=== create event ==="
CAL_ID=$(curl -s -b /tmp/suite-cookies.txt http://127.0.0.1:8760/api/calendars | python3 -c 'import sys,json; xs=json.load(sys.stdin); print(xs[0]["id"] if xs else "")')
echo "CAL_ID=$CAL_ID"
EV=$(curl -s -b /tmp/suite-cookies.txt -X POST http://127.0.0.1:8760/api/events -H 'Content-Type: application/json' -d "{\"title\":\"Deploy verification\",\"start\":\"2026-11-20T09:00:00Z\",\"end\":\"2026-11-20T10:00:00Z\",\"all_day\":false,\"color\":\"#aecbfa\",\"calendar_id\":\"$CAL_ID\"}")
echo "$EV"
EVENT_ID=$(python3 -c 'import sys,json; print(json.loads(sys.stdin.read()).get("id",""))' <<< "$EV")
echo "=== cleanup test data ==="
[ -n "$EVENT_ID" ] && curl -s -b /tmp/suite-cookies.txt -X DELETE http://127.0.0.1:8760/api/events/"$EVENT_ID" >/dev/null && echo "event deleted"
[ -n "$NOTE_ID" ] && curl -s -b /tmp/suite-cookies.txt -X DELETE http://127.0.0.1:8760/api/notes/"$NOTE_ID" >/dev/null && echo "note deleted"
echo "=== done ==="
