#![windows_subsystem = "console"]
#[link(name = "user32")]
extern "system" {
    fn SetProcessDpiAwarenessContext(value: isize) -> i32;
    fn GetThreadDpiAwarenessContext(thread_id: u32) -> isize;
    fn GetAwarenessFromDpiAwarenessContext(value: isize) -> i32;
    fn GetDpiForSystem() -> u32;
}

#[link(name = "shcore")]
extern "system" {
    fn SetProcessDpiAwareness(value: i32) -> i32;
}

fn dump(label: &str) {
    unsafe {
        let ctx = GetThreadDpiAwarenessContext(0);
        let aware = GetAwarenessFromDpiAwarenessContext(ctx);
        println!("{label}: ctx={ctx} aware={aware} systemDpi={}", GetDpiForSystem());
    }
}

fn main() {
    dump("start (no manifest)");
    let ok = unsafe { SetProcessDpiAwarenessContext(-4) };
    println!("SetProcessDpiAwarenessContext(-4) => {ok}");
    dump("after PerMonitorV2");
    let ok2 = unsafe { SetProcessDpiAwareness(2) }; // PROCESS_PER_MONITOR_DPI_AWARE
    println!("SetProcessDpiAwareness(2) => {ok2}");
    dump("after SetProcessDpiAwareness(2)");
}
