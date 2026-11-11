fn main() {
    println!("cargo:rerun-if-changed=ui/");
    println!("cargo:rerun-if-changed=../shared-ui/");
    println!("cargo:rerun-if-changed=notes.manifest");

    // Embed the Windows DPI-awareness manifest (fixes clipped/oversized layouts on 125%+ displays).
    #[cfg(target_os = "windows")]
    embed_resource::compile("notes.rc", embed_resource::NONE);

    slint_build::compile("ui/notes.slint").expect("failed to compile notes.slint");
}
