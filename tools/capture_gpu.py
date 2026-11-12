"""Capture a window with PrintWindow(PW_RENDERFULLCONTENT) — works for GPU content."""
import ctypes, subprocess, sys, time
from ctypes import wintypes
from PIL import Image

pname = sys.argv[1] if len(sys.argv) > 1 else "notes.exe"
outfile = sys.argv[2] if len(sys.argv) > 2 else r"C:\Users\hemsh_sfya5gq\AppData\Local\Temp\winfull.png"

u = ctypes.windll.user32
gdi = ctypes.windll.gdi32

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
    if u.IsWindowVisible(h):
        pid = wintypes.DWORD()
        u.GetWindowThreadProcessId(h, ctypes.byref(pid))
        b = ctypes.create_unicode_buffer(256)
        u.GetWindowTextW(h, b, 256)
        if pid.value in pids and "notes" in b.value.lower():
            r = wintypes.RECT()
            u.GetWindowRect(h, ctypes.byref(r))
            found.append((h, r.left, r.top, r.right, r.bottom, b.value))
    return True
u.EnumWindows(ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)(enum), 0)
print("windows:", found)
for h, l, t, r, b, title in found:
    w, hh = r - l, b - t
    hdc_wnd = u.GetWindowDC(h)
    hdc_mem = gdi.CreateCompatibleDC(hdc_wnd)
    bmp = gdi.CreateCompatibleBitmap(hdc_wnd, w, hh)
    gdi.SelectObject(hdc_mem, bmp)
    ok = u.PrintWindow(h, hdc_mem, 2)
    gdi.DeleteObject(bmp)
    # read the bitmap via GetDIBits
    bmp2 = gdi.CreateCompatibleBitmap(hdc_wnd, w, hh)
    gdi.SelectObject(hdc_mem, bmp2)
    gdi.BitBlt(hdc_mem, 0, 0, w, hh, hdc_wnd, 0, 0, 0x00CC0020)  # SRCCOPY
    gdi.SelectObject(hdc_mem, bmp2)
    bmih = ctypes.create_string_buffer(40)
    # get bits
    class BITMAPINFOHEADER(ctypes.Structure):
        _fields_ = [("biSize", wintypes.DWORD), ("biWidth", wintypes.LONG), ("biHeight", wintypes.LONG),
                    ("biPlanes", wintypes.WORD), ("biBitCount", wintypes.WORD), ("biCompression", wintypes.DWORD),
                    ("biSizeImage", wintypes.DWORD), ("biXPelsPerMeter", wintypes.LONG), ("biYPelsPerMeter", wintypes.LONG),
                    ("biClrUsed", wintypes.DWORD), ("biClrImportant", wintypes.DWORD)]
    class BITMAPINFO(ctypes.Structure):
        _fields_ = [("bmiHeader", BITMAPINFOHEADER), ("bmiColors", wintypes.DWORD * 3)]
    bi = BITMAPINFO()
    bi.bmiHeader.biSize = 40
    bi.bmiHeader.biWidth = w
    bi.bmiHeader.biHeight = -hh  # top-down
    bi.bmiHeader.biPlanes = 1
    bi.bmiHeader.biBitCount = 32
    bi.bmiHeader.biCompression = 0
    buf = ctypes.create_string_buffer(w * hh * 4)
    lines = gdi.GetDIBits(hdc_mem, bmp2, 0, hh, buf, ctypes.byref(bi), 0)
    print("PrintWindow flag2 =", ok, "GetDIBits lines =", lines)
    if lines == hh:
        img = Image.frombuffer("RGB", (w, hh), buf, "raw", "BGRX", 0, 1)
        img.save(outfile)
        print("saved", outfile)
    gdi.DeleteObject(bmp2)
    gdi.DeleteDC(hdc_mem)
    u.ReleaseDC(h, hdc_wnd)
