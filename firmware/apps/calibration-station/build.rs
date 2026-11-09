use std::{env, fs, path::PathBuf};

fn main() {
    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"));

    fs::write(
        out_dir.join("memory.x"),
        include_bytes!("memory.x"),
    )
    .expect("Failed to copy memory.x");

    println!(
        "cargo:rustc-link-search={}",
        out_dir.display()
    );
    println!("cargo:rerun-if-changed=memory.x");
}
