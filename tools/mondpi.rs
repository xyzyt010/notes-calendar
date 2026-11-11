#[link(name = "user32")]
extern "system" {
    fn GetSystemMetrics(index: i32) -> i32;
    fn MonitorFromPoint(pt: POINT, flags: u32) -> isize;
}

#[link(name = "shcore")]
extern "system" {
    fn GetDpiForMonitor(hmonitor: isize, dpi_type: u32, dpi_x: *mut u32, dpi_y: *mut u32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn GetDeviceCaps(hdc: isize, index: i32) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn GetDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
}

#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

fn main() {
    let hmon = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, 1) };
    let mut dx: u32 = 96;
    let mut dy: u32 = 96;
    let hr = unsafe { GetDpiForMonitor(hmon, 0, &mut dx, &mut dy) };
    println!("GetDpiForMonitor hmon={} hr={} dx={} dy={}", hmon, hr, dx, dy);

    let sw = unsafe { GetSystemMetrics(0) };
    let sh = unsafe { GetSystemMetrics(1) };
    println!("SM_CXSCREEN={} SM_CYSCREEN={}", sw, sh);

    let hdc = unsafe { GetDC(0) };
    const LOGPIXELSX: i32 = 88;
    println!("LOGPIXELSX={}", unsafe { GetDeviceCaps(hdc, LOGPIXELSX) });
    unsafe { ReleaseDC(0, hdc) };
}
