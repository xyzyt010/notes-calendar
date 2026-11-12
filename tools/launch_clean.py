"""Win+D (show desktop), launch an app, wait, capture full screen."""
import ctypes, subprocess, time, sys

u32 = ctypes.windll.user32
VK_LWIN, VK_D = 0x5B, 0x44
u32.keybd_event(VK_LWIN, 0, 0, 0)
u32.keybd_event(VK_D, 0, 0, 0)
u32.keybd_event(VK_D, 0, 0x0002, 0)
u32.keybd_event(VK_LWIN, 0, 0x0002, 0)
time.sleep(1.5)
exe = sys.argv[1]
outfile = sys.argv[2]
subprocess.Popen([exe])
time.sleep(6)
from PIL import ImageGrab
ImageGrab.grab().save(outfile)
print("saved", outfile)
