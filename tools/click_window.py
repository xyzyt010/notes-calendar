"""Send WM_LBUTTONDOWN/WM_LBUTTONUP to the Notes window at client coords."""
import ctypes, subprocess, sys, time
from ctypes import wintypes
u = ctypes.windll.user32

pname = "notes.exe"
out = subprocess.check_output(["tasklist", "/FI", f"IMAGENAME eq {pname}", "/FO", "CSV", "/NH"]).decode()
pids = []
for line in out.strip().splitlines():
    p = [x.strip('"') for x in line.split(",")]
    try:
        pids.append(int(p[1]))
    except Exception:
        pass
found = []
def enum(h, l):
    pid = wintypes.DWORD()
    u.GetWindowThreadProcessId(h, ctypes.byref(pid))
    if pid.value in pids:
        b = ctypes.create_unicode_buffer(256)
        u.GetWindowTextW(h, b, 256)
        if u.IsWindowVisible(h) and b.value:
            found.append(h)
    return True
u.EnumWindows(ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)(enum), 0)
print("main window:", hex(found[0]) if found else None)
hwnd = found[0]

WM_LBUTTONDOWN, WM_LBUTTONUP, MK_LBUTTON = 0x0201, 0x0202, 0x0001

def post(x, y):
    lparam = (y << 16) | (x & 0xFFFF)
    u.PostMessageW(hwnd, WM_LBUTTONDOWN, MK_LBUTTON, lparam)
    time.sleep(0.06)
    u.PostMessageW(hwnd, WM_LBUTTONUP, 0, lparam)
    time.sleep(0.4)

x, y = int(sys.argv[1]), int(sys.argv[2])
print(f"click at client ({x},{y})")
post(x, y)
time.sleep(0.8)
