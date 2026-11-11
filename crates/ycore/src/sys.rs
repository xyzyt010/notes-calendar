//! Shared runtime helpers for the yellod Windows apps.

/// Query the screen size in *logical* (DPI-independent) pixels.
/// Uses the primary monitor's effective DPI via GetDpiForMonitor.
#[cfg(target_os = "windows")]
pub fn screen_size_logical() -> Option<(f32, f32)> {
    #[repr(C)]
    struct POINT {
        x: i32,
        y: i32,
    }

    extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
        fn GetDpiForSystem() -> u32;
        fn MonitorFromPoint(pt: POINT, flags: u32) -> isize;
    }
    #[link(name = "shcore")]
    extern "system" {
        fn GetDpiForMonitor(hmonitor: isize, dpi_type: u32, dpi_x: *mut u32, dpi_y: *mut u32) -> i32;
    }
    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;
    const MONITOR_DEFAULTTOPRIMARY: u32 = 1;
    const MDT_EFFECTIVE_DPI: u32 = 0;

    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) } as f32;
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) } as f32;
    let hmon = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let mut dx: u32 = 96;
    let mut dy: u32 = 96;
    if unsafe { GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) } >= 0 {
        let scale = dx as f32 / 96.0;
        Some((w / scale, h / scale))
    } else {
        let dpi = unsafe { GetDpiForSystem() } as f32 / 96.0;
        Some((w / dpi, h / dpi))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn screen_size_logical() -> Option<(f32, f32)> {
    None
}
