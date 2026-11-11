import ctypes
import subprocess
import sys
from ctypes import wintypes

u = ctypes.windll.user32

pname = sys.argv[1] if len(sys.argv) > 1 else "calendar.exe"
out = subprocess.check_output(['tasklist', '/FI', 'IMAGENAME eq ' + pname, '/FO', 'CSV', '/NH']).decode().strip().splitlines()
pids = []
for line in out:
    parts = [p.strip('"') for p in line.split(',')]
    if parts and parts[0].lower().endswith(pname.lower()):
        pids.append(int(parts[1]))
print(pname, 'pids:', pids)

def enum(h, l):
    if not u.IsWindowVisible(h):
        return True
    p = wintypes.DWORD()
    u.GetWindowThreadProcessId(h, ctypes.byref(p))
    if p.value in pids:
        tid = u.GetWindowThreadProcessId(h, None)
        ctx = u.GetThreadDpiAwarenessContext(tid)
        aware = u.GetAwarenessFromDpiAwarenessContext(ctx)
        try:
            path = ctypes.create_unicode_buffer(512)
            n = u.GetWindowTextW(h, path, 512)
            r = wintypes.RECT()
            u.GetWindowRect(h, ctypes.byref(r))
            print(f'  hwnd={h} tid={tid} title={path.value!r} ctx={ctx} aware={aware} rect=({r.left},{r.top})-({r.right},{r.bottom})')
        except Exception as e:
            print('  err', e)
    return True

u.EnumWindows(ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)(enum), 0)
