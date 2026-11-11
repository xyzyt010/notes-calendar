param(
    [string]$TitleMatch = 'Notes',
    [string]$OutFile = "win.png",
    [int]$TimeoutSeconds = 30
)

Add-Type @"
using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text;
public class WinCapture {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder s, int nMaxCount);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hDC, uint flags);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }

    public static IntPtr FindWindow(string needle) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hWnd, lParam) => {
            if (!IsWindowVisible(hWnd)) return true;
            var sb = new StringBuilder(256);
            GetWindowText(hWnd, sb, 256);
            string t = sb.ToString();
            if (t.IndexOf(needle, StringComparison.OrdinalIgnoreCase) >= 0) { found = hWnd; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static bool Capture(IntPtr hWnd, string path) {
        GetWindowRect(hWnd, out RECT r);
        int w = r.Right - r.Left, h = r.Bottom - r.Top;
        if (w <= 0 || h <= 0) return false;
        using (var bmp = new Bitmap(w, h, System.Drawing.Imaging.PixelFormat.Format32bppArgb))
        using (var g = Graphics.FromImage(bmp)) {
            IntPtr hdc = g.GetHdc();
            bool ok = false;
            try { ok = PrintWindow(hWnd, hdc, 2); } finally { g.ReleaseHdc(hdc); }
            if (!ok) g.CopyFromScreen(r.Left, r.Top, 0, 0, new Size(w, h));
            bmp.Save(path, System.Drawing.Imaging.ImageFormat.Png);
        }
        return true;
    }
}
"@ -ReferencedAssemblies System.Drawing

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
$hwnd = [IntPtr]::Zero
while ((Get-Date) -lt $deadline) {
    $hwnd = [WinCapture]::FindWindow($TitleMatch)
    if ($hwnd -ne [IntPtr]::Zero) { break }
    Start-Sleep -Milliseconds 400
}
if ($hwnd -eq [IntPtr]::Zero) { Write-Output 'WINDOW NOT FOUND'; exit 1 }
Write-Output "found hwnd $hwnd"
$ok = [WinCapture]::Capture($hwnd, $OutFile)
if ($ok) { Write-Output "saved $OutFile"; Get-Item $OutFile | Select-Object Length } else { Write-Output 'CAPTURE FAILED'; exit 1 }
