// PMA-aware window capture (like tools/capture.py but not DPI-virtualized).
// Build: rustc -o target/winsnap.exe tools/winsnap.rs
// Usage: winsnap.exe <out.bmp> [window-title-substring]
#![windows_subsystem = "console"]

use std::ffi::c_void;
use std::io::Write;

#[repr(C)]
struct RECT { left: i32, top: i32, right: i32, bottom: i32 }
#[repr(C)]
struct BITMAPINFOHEADER {
    biSize: u32, biWidth: i32, biHeight: i32, biPlanes: u16, biBitCount: u16,
    biCompression: u32, biSizeImage: u32, biXPelsPerMeter: i32, biYPelsPerMeter: i32,
    biClrUsed: u32, biClrImportant: u32,
}

#[link(name = "user32")]
extern "system" {
    fn SetProcessDpiAwarenessContext(value: isize) -> i32;
    fn EnumWindows(cb: unsafe extern "system" fn(*mut c_void, isize) -> i32, lparam: isize) -> i32;
    fn IsWindowVisible(hwnd: *mut c_void) -> i32;
    fn GetWindowTextW(hwnd: *mut c_void, buf: *mut u16, len: i32) -> i32;
    fn GetWindowRect(hwnd: *mut c_void, rect: *mut RECT) -> i32;
    fn GetWindowDC(hwnd: *mut c_void) -> *mut c_void;
    fn ReleaseDC(hwnd: *mut c_void, hdc: *mut c_void) -> i32;
    fn PrintWindow(hwnd: *mut c_void, hdc: *mut c_void, flags: u32) -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: *mut c_void) -> *mut c_void;
    fn CreateCompatibleBitmap(hdc: *mut c_void, w: i32, h: i32) -> *mut c_void;
    fn SelectObject(hdc: *mut c_void, obj: *mut c_void) -> *mut c_void;
    fn DeleteObject(obj: *mut c_void) -> i32;
    fn DeleteDC(hdc: *mut c_void) -> i32;
    fn GetDIBits(hdc: *mut c_void, bmp: *mut c_void, start: u32, lines: u32, bits: *mut c_void, info: *mut BITMAPINFOHEADER, usage: u32) -> i32;
}

static mut FOUND: *mut c_void = std::ptr::null_mut();
static mut W: i32 = 0;
static mut H: i32 = 0;
static mut LX: i32 = 0;
static mut TY: i32 = 0;
static mut TITLE: String = String::new();

unsafe extern "system" fn enum_cb(hwnd: *mut c_void, _: isize) -> i32 {
    if IsWindowVisible(hwnd) == 0 { return 1; }
    let mut buf = [0u16; 256];
    let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
    if n <= 0 { return 1; }
    let title = String::from_utf16_lossy(&buf[..n as usize]);
    if title.to_lowercase() == "notes" {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetWindowRect(hwnd, &mut r);
        let w = r.right - r.left;
        let h = r.bottom - r.top;
        if (w as i64) * (h as i64) > (W as i64) * (H as i64) {
            FOUND = hwnd;
            W = w; H = h; LX = r.left; TY = r.top;
            TITLE = title;
        }
    }
    1
}

fn main() {
    unsafe { SetProcessDpiAwarenessContext(-4) };
    let out = std::env::args().nth(1).unwrap_or_else(|| "capture.bmp".into());
    unsafe { EnumWindows(enum_cb, 0) };
    let (hwnd, w, h, x, y, title) = unsafe { (FOUND, W, H, LX, TY, TITLE.clone()) };
    if hwnd.is_null() {
        eprintln!("no matching window");
        std::process::exit(1);
    }
    eprintln!("capturing {}x{} at ({},{}) title={}", w, h, x, y, title);

    let mut bih = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32,
        biCompression: 0, biSizeImage: 0, biXPelsPerMeter: 0, biYPelsPerMeter: 0,
        biClrUsed: 0, biClrImportant: 0,
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    unsafe {
        let hdc = GetWindowDC(hwnd);
        let dest = CreateCompatibleDC(hdc);
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        SelectObject(dest, bmp);
        PrintWindow(hwnd, dest, 2);
        GetDIBits(dest, bmp, 0, h as u32, buf.as_mut_ptr() as *mut c_void, &mut bih, 0);
        DeleteObject(bmp);
        DeleteDC(dest);
        ReleaseDC(hwnd, hdc);
    }

    // Write 32-bit BMP (BGRA, top-down)
    let row = (w * 4) as u32;
    let img_size = row * h as u32;
    let file_size = 54 + img_size;
    let mut f = std::fs::File::create(&out).unwrap();
    f.write_all(b"BM").unwrap();
    f.write_all(&file_size.to_le_bytes()).unwrap();
    f.write_all(&[0; 4]).unwrap();
    f.write_all(&54u32.to_le_bytes()).unwrap();
    f.write_all(&bih.biSize.to_le_bytes()).unwrap();
    f.write_all(&bih.biWidth.to_le_bytes()).unwrap();
    f.write_all(&(-bih.biHeight).to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&32u16.to_le_bytes()).unwrap();
    f.write_all(&0u32.to_le_bytes()).unwrap();
    f.write_all(&img_size.to_le_bytes()).unwrap();
    f.write_all(&bih.biXPelsPerMeter.to_le_bytes()).unwrap();
    f.write_all(&bih.biYPelsPerMeter.to_le_bytes()).unwrap();
    f.write_all(&0u32.to_le_bytes()).unwrap();
    f.write_all(&0u32.to_le_bytes()).unwrap();
    f.write_all(&buf).unwrap();
    eprintln!("saved {}", out);
}
