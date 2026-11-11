Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class WRect {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder s, int nMaxCount);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
    public static void FindAll(string needle) {
        EnumWindows((h, l) => {
            if (!IsWindowVisible(h)) return true;
            var sb = new StringBuilder(256);
            GetWindowText(h, sb, 256);
            if (sb.ToString().IndexOf(needle, StringComparison.OrdinalIgnoreCase) >= 0) {
                GetWindowRect(h, out RECT r);
                Console.WriteLine("hwnd=" + h + " title='" + sb + "' rect=(" + r.Left + "," + r.Top + ")-(" + r.Right + "," + r.Bottom + ") width=" + (r.Right - r.Left) + " height=" + (r.Bottom - r.Top));
            }
            return true;
        }, IntPtr.Zero);
    }
}
'@
[WRect]::FindAll('Notes')
