"""Capture the notes.exe window to a PNG regardless of z-order (PrintWindow)."""
import ctypes, sys
from ctypes import wintypes
import subprocess
from PIL import Image

out = sys.argv[1] if len(sys.argv) > 1 else "app.png"
pname = sys.argv[2] if len(sys.argv) > 2 else "notes.exe"
title_match = sys.argv[3].lower() if len(sys.argv) > 3 else "notes"

u = ctypes.windll.user32
g = ctypes.windll.gdi32

task = subprocess.check_output(["tasklist", "/FI", "IMAGENAME eq " + pname, "/FO", "CSV", "/NH"]).decode()
pids = []
for line in task.strip().splitlines():
    parts = [p.strip('"') for p in line.split(",")]
    if len(parts) > 1 and parts[0].lower().endswith(pname.lower()):
        try:
            pids.append(int(parts[1]))
        except ValueError:
            pass

found = []
def enum(h, l):
    if u.IsWindowVisible(h):
        pid = wintypes.DWORD()
        u.GetWindowThreadProcessId(h, ctypes.byref(pid))
        b = ctypes.create_unicode_buffer(256)
        u.GetWindowTextW(h, b, 256)
        title = b.value
        if pid.value in pids and title_match in title.lower():
            r = wintypes.RECT()
            u.GetWindowRect(h, ctypes.byref(r))
            found.append((h, r.left, r.top, r.right, r.bottom, title))
    return True

u.EnumWindows(ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)(enum), 0)
print("windows:", found)
if not found:
    print("NO APP WINDOW FOUND")
    sys.exit(1)

h, l, t, r, b, title = max(found, key=lambda f: (f[3]-f[1])*(f[4]-f[2]))
w, ht = r - l, b - t
print(f"capturing hwnd={h} ({l},{t})-({r},{b}) {w}x{ht} title={title!r}")

class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD), ("biWidth", wintypes.LONG), ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD), ("biBitCount", wintypes.WORD), ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD), ("biXPelsPerMeter", wintypes.LONG), ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD), ("biClrImportant", wintypes.DWORD),
    ]

bih = BITMAPINFOHEADER()
bih.biSize = ctypes.sizeof(BITMAPINFOHEADER)
bih.biWidth = w
bih.biHeight = -ht  # top-down
bih.biPlanes = 1
bih.biBitCount = 32
bih.biCompression = 0  # BI_RGB

hdc = u.GetWindowDC(h)
dest_dc = g.CreateCompatibleDC(hdc)
dest_bmp = g.CreateCompatibleBitmap(hdc, w, ht)
g.SelectObject(dest_dc, dest_bmp)
ok = u.PrintWindow(h, dest_dc, 2)  # PW_RENDERFULLCONTENT
buf = ctypes.create_string_buffer(w * ht * 4)
g.GetDIBits(dest_dc, dest_bmp, 0, ht, buf, ctypes.byref(bih), 0)
g.DeleteObject(dest_bmp)
g.DeleteDC(dest_dc)
u.ReleaseDC(h, hdc)
print("PrintWindow ok:", bool(ok))

img = Image.frombuffer("RGBA", (w, ht), buf, "raw", "BGRA", 0, 1).convert("RGB")
img.save(out)
print("saved", out)
