#!/usr/bin/env bash
python3 - <<'EOF'
import json, urllib.request, sys

ACCT = 'f7af385004468ba0e6d2a8f989e4d76f'
TUNNEL = '2707259a-a883-44b7-8095-63ffb4c0bca2'
cfg = json.load(open('/home/hs01/.config/cloudflare/config/default.json'))
token = cfg.get('oauth_token')
if not token:
    print('no oauth_token in profile'); sys.exit(1)

def api(method, path, body=None):
    req = urllib.request.Request(
        'https://api.cloudflare.com/client/v4/' + path,
        method=method,
        data=json.dumps(body).encode() if body is not None else None,
        headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'},
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)

try:
    d = api('GET', f'accounts/{ACCT}/cfd_tunnel/{TUNNEL}')
    t = d.get('result', {})
    print('tunnel result keys:', sorted(t.keys()))
    for k in ('id', 'name', 'config_src', 'status', 'created_at', 'tun_type', 'remote_managed'):
        if k in t:
            print(f'{k}: {t[k]}')
except Exception as e:
    print('GET tunnel error:', e)
    sys.exit(2)
EOF
