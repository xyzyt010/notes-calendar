import ctypes
import subprocess
import sys
from ctypes import wintypes

s = ctypes.WinDLL("shcore.dll")
k = ctypes.windll.kernel32
u = ctypes.windll.user32

pname = sys.argv[1] if len(sys.argv) > 1 else "calendar.exe"
out = subprocess.check_output(['tasklist', '/FI', 'IMAGENAME eq ' + pname, '/FO', 'CSV', '/NH']).decode().strip().splitlines()

names = {0: 'UNAWARE', 1: 'SYSTEM_AWARE', 2: 'PER_MONITOR_AWARE'}

for line in out:
    parts = [p.strip('"') for p in line.split(',')]
    if not parts or not parts[0].lower().endswith(pname.lower()):
        continue
    pid = int(parts[1])
    proc = k.OpenProcess(0x00100000 | 0x0400, False, pid)  # QUERY_INFORMATION | VM_READ
    awareness = ctypes.c_int(-1)
    hr = s.GetProcessDpiAwareness(proc, ctypes.byref(awareness))
    print(f'pid={pid} GetProcessDpiAwareness hr=0x{hr:x} awareness={awareness.value} ({names.get(awareness.value, "?")})')
    k.CloseHandle(proc)
