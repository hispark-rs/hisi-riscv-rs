//! Build script: emit the BS21 interrupt linker fragment (device.x) for riscv-rt
//! when the `rt` feature is on, mirroring ws63-pac.
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    if env::var_os("CARGO_FEATURE_RT").is_some() {
        let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
        File::create(out.join("device.x"))
            .unwrap()
            .write_all(include_bytes!("device.x"))
            .unwrap();
        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rerun-if-changed=device.x");
        println!("cargo:rustc-env=RISCV_RT_BASE_ISA=rv32i");
        println!("cargo:rerun-if-env-changed=RISCV_RT_BASE_ISA");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
