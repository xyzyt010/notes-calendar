#!/usr/bin/env bash
python3 - <<'EOF'
import json
p = '/home/hs01/.config/cloudflare/config/default.json'
try:
    d = json.load(open(p))
    def desc(v):
        if isinstance(v, str):
            return f'str len={len(v)}'
        return type(v).__name__
    print({k: desc(v) for k, v in d.items()})
except Exception as e:
    print('read error:', e)
EOF
ls -la ~/.config/cloudflare/config/
