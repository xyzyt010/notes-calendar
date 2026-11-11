import ctypes
from ctypes import wintypes
import subprocess

u = ctypes.windll.user32
out = subprocess.check_output(['tasklist', '/FI', 'IMAGENAME eq calendar.exe', '/FO', 'CSV', '/NH']).decode().strip().splitlines()
pid = None
for line in out:
    parts = [p.strip('"') for p in line.split(',')]
    if parts and parts[0].lower().endswith('calendar.exe'):
        pid = int(parts[1])
print('pid', pid)

def enum(h, l):
    if not u.IsWindowVisible(h):
        return True
    p = ctypes.wintypes.DWORD()
    u.GetWindowThreadProcessId(h, ctypes.byref(p))
    if p.value == pid:
        r = ctypes.wintypes.RECT()
        u.GetClientRect(h, ctypes.byref(r))
        print('client rect', r.left, r.top, r.right, r.bottom, 'size', r.right - r.left, r.bottom - r.top)
        r2 = ctypes.wintypes.RECT()
        u.GetWindowRect(h, ctypes.byref(r2))
        print('window rect', r2.left, r2.top, r2.right, r2.bottom)
        print('IsProcessDPIAware:', u.IsProcessDPIAware())
        print('GetDpiForWindow:', u.GetDpiForWindow(h))
        try:
            ctx = u.GetThreadDpiAwarenessContext(u.GetWindowThreadProcessId(h, None))
            print('thread dpi ctx raw:', ctx)
            print('awareness:', u.GetAwarenessFromDpiAwarenessContext(ctx))
        except Exception as e:
            print('dpi ctx err', e)
    return True

u.EnumWindows(ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.wintypes.HWND, ctypes.wintypes.LPARAM)(enum), 0)
