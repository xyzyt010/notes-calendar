import ctypes, subprocess
from ctypes import wintypes
u = ctypes.windll.user32
out = subprocess.check_output(["tasklist", "/FI", "IMAGENAME eq notes.exe", "/FO", "CSV", "/NH"]).decode()
pids = []
for line in out.strip().splitlines():
    p = [x.strip('"') for x in line.split(",")]
    try:
        pids.append(int(p[1]))
    except Exception:
        pass
def enum(h, l):
    pid = wintypes.DWORD()
    u.GetWindowThreadProcessId(h, ctypes.byref(pid))
    if pid.value in pids:
        b = ctypes.create_unicode_buffer(256)
        u.GetWindowTextW(h, b, 256)
        r = wintypes.RECT()
        u.GetWindowRect(h, ctypes.byref(r))
        vis = bool(u.IsWindowVisible(h))
        print("rect", r.left, r.top, r.right, r.bottom, "title", b.value, "vis", vis)
    return True
u.EnumWindows(ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)(enum), 0)
