#![windows_subsystem = "windows"]

fn main() -> anyhow::Result<()> {
    // Web parity: the webapp renders at html `zoom: 1.25` on desktop
    // (layout px * 1.25 = visual px). Match it so card sizes, fonts and
    // masonry columns look identical at the same window size.
    if std::env::var("SLINT_SCALE_FACTOR").is_err() {
        std::env::set_var("SLINT_SCALE_FACTOR", "1.25");
    }
    // Force PerMonitorV2 DPI awareness and, on systems whose Win32 DPI APIs
    // disagree with each other (GetDpiForWindow reports e.g. 125% while GDI
    // reports 96 DPI), force SLINT_SCALE_FACTOR=1 before Slint initializes.
    // On such systems Slint's renderer scale and winit's window size diverge,
    // clipping the UI; a consistent 1.0 scale always renders correctly.
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn SetProcessDpiAwarenessContext(value: isize) -> i32;
            fn GetDC(hwnd: isize) -> isize;
            fn ReleaseDC(hwnd: isize, dc: isize) -> i32;
            fn GetDeviceCaps(dc: isize, index: i32) -> i32;
            fn CreateWindowExW(
                exstyle: u32, class: *const u16, name: *const u16,
                style: u32, x: i32, y: i32, w: i32, h: i32,
                parent: isize, menu: isize, instance: isize, param: *mut core::ffi::c_void,
            ) -> isize;
            fn DestroyWindow(hwnd: isize) -> i32;
            fn GetDpiForWindow(hwnd: isize) -> u32;
            fn GetDpiForSystem() -> u32;
        }
        const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        // Compare what the window would report vs what GDI reports for screen DC.
        let gdi_dpi = unsafe {
            let dc = GetDC(0);
            let dpi = GetDeviceCaps(dc, 88); // LOGPIXELSX
            ReleaseDC(0, dc);
            dpi as u32
        };
        let sys_dpi = unsafe { GetDpiForSystem() }.max(96);
        let hwnd_dpi = unsafe {
            let class: Vec<u16> = "STATIC".encode_utf16().chain([0]).collect();
            let name: Vec<u16> = "probe".encode_utf16().chain([0]).collect();
            let hwnd = CreateWindowExW(0, class.as_ptr(), name.as_ptr(), 0, 0, 0, 1, 1, 0, 0, 0, std::ptr::null_mut());
            let dpi = if hwnd != 0 { GetDpiForWindow(hwnd) } else { sys_dpi };
            if hwnd != 0 { DestroyWindow(hwnd); }
            dpi.max(96)
        };
        if hwnd_dpi != gdi_dpi || hwnd_dpi != sys_dpi {
            eprintln!("[notes] inconsistent DPI reporting (window={hwnd_dpi} gdi={gdi_dpi} sys={sys_dpi}); forcing SLINT_SCALE_FACTOR=1");
            std::env::set_var("SLINT_SCALE_FACTOR", "1");
        }
    }
    notes::run()
}
