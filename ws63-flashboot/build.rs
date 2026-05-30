use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // Copy linker scripts to OUT_DIR
    let memory_x = Path::new("memory.x");
    let memory_out = Path::new(&out_dir).join("memory.x");
    fs::copy(memory_x, &memory_out).expect("Failed to copy memory.x");

    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rustc-link-arg=-T{}", memory_out.display());
    println!("cargo:rustc-link-search={}", out_dir);
}
