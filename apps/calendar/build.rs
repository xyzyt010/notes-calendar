fn main() {
    println!("cargo:rerun-if-changed=ui/");
    println!("cargo:rerun-if-changed=../shared-ui/");
    println!("cargo:rerun-if-changed=calendar.manifest");

    // Embed the Windows DPI-awareness manifest (fixes clipped/oversized layouts on 125%+ displays).
    #[cfg(target_os = "windows")]
    embed_resource::compile("calendar.rc", embed_resource::NONE);

    slint_build::compile("ui/calendar.slint").expect("failed to compile calendar.slint");
}
