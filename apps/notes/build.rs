fn main() {
    println!("cargo:rerun-if-changed=ui/");
    println!("cargo:rerun-if-changed=../shared-ui/");
    slint_build::compile("ui/notes.slint").expect("failed to compile notes.slint");
}
