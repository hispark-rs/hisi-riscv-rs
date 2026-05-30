use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // Copy linker scripts to OUT_DIR
    for file in ["memory.x", "layout.ld"] {
        let src = Path::new(file);
        let dst = Path::new(&out_dir).join(file);
        fs::copy(src, &dst).unwrap();
        println!("cargo:rerun-if-changed={file}");
        println!("cargo:rustc-link-arg=-T{}", dst.display());
    }

    println!("cargo:rerun-if-changed=asm/startup.S");
    println!("cargo:rustc-link-search={}", out_dir);
}
