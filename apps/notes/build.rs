fn main() {
    println!("cargo:rerun-if-changed=ui/");
    println!("cargo:rerun-if-changed=../shared-ui/");
    println!("cargo:rerun-if-changed=notes.manifest");

    // Embed the Windows DPI-awareness manifest. Note: build.rs runs on the HOST,
    // so gate on the TARGET OS instead, otherwise cross-compilation picks up
    // Windows-specific link artifacts.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        embed_resource::compile("notes.rc", embed_resource::NONE);
    }

    slint_build::compile("ui/notes.slint").expect("failed to compile notes.slint");
}
