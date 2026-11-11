#![windows_subsystem = "windows"]

fn main() -> anyhow::Result<()> {
    // Force PerMonitorV2 DPI awareness BEFORE any window is created, so Slint
    // receives the true system scale factor (no Windows bitmap scaling / clipping).
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn SetProcessDpiAwarenessContext(value: isize) -> i32;
        }
        const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
        let ok = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if ok == 0 {
            eprintln!("[notes] warning: SetProcessDpiAwarenessContext failed");
        }
    }
    notes::run()
}
