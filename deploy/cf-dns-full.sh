#!/usr/bin/env bash
export PATH=/home/hs01/.nvm/versions/node/v24.19.0/bin:$PATH
timeout 30 cf dns records list --zone=yellod.dpdns.org > /tmp/yellod-dns.json 2>&1
python3 - <<'EOF'
import json
try:
    data = json.load(open('/tmp/yellod-dns.json'))
except Exception as e:
    print('parse error:', e); print(open('/tmp/yellod-dns.json').read()[:2000]); raise SystemExit
recs = data if isinstance(data, list) else data.get('result', data)
print('records:', len(recs) if isinstance(recs, list) else type(recs).__name__)
for r in recs if isinstance(recs, list) else []:
    print(f"{r.get('name'):45} {r.get('type'):5} {r.get('content')}")
EOF
