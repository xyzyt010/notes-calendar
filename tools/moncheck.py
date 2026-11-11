import ctypes
import subprocess
from ctypes import wintypes

u = ctypes.windll.user32
s = ctypes.WinDLL("shcore.dll")

pname = "calendar.exe"
out = subprocess.check_output(['tasklist', '/FI', 'IMAGENAME eq ' + pname, '/FO', 'CSV', '/NH']).decode().strip().splitlines()
pid = int([p.strip('"') for p in out[0].split(',')][1])
print('pid', pid)

def enum(h, l):
    if not u.IsWindowVisible(h):
        return True
    p = wintypes.DWORD()
    u.GetWindowThreadProcessId(h, ctypes.byref(p))
    if p.value == pid:
        r = wintypes.RECT()
        u.GetWindowRect(h, ctypes.byref(r))
        if r.right - r.left > 100:
            hmon = u.MonitorFromWindow(h, 2)  # MONITOR_DEFAULTTONEAREST
            dx = wintypes.UINT(96)
            dy = wintypes.UINT(96)
            hr = s.GetDpiForMonitor(hmon, 0, ctypes.byref(dx), ctypes.byref(dy))
            print(f'hwnd={h} rect=({r.left},{r.top})-({r.right},{r.bottom}) {r.right-r.left}x{r.bottom-r.top}')
            print(f'  hmon={hmon} GetDpiForMonitor hr={hr} dx={dx.value} dy={dy.value}')
            print(f'  GetDpiForWindow={u.GetDpiForWindow(h)}')
    return True

u.EnumWindows(ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)(enum), 0)
