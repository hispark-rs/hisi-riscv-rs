//! Build script for the BS21 blinky example.
//!
//! BS21 has its own memory map, so this crate ships its own `memory.x` (hisi-riscv-rt's
//! bundled WS63 one is disabled via `default-features = false`). Copy ours into
//! OUT_DIR and put that dir on the linker search path, so `ws63-link.x`'s
//! `INCLUDE memory.x` resolves to THIS file (exactly one on the path → no
//! link-order ambiguity).
//!
//! hisi-riscv-rt still supplies layout.ld / riscv-rt-symbols.x / startup and the
//! `ws63-link.x` entry script. Under `chip-bs21` the interrupt `device.x` comes
//! from bs2x-pac's `rt` feature (enabled transitively by hisi-riscv-hal's `rt`), whose
//! build.rs adds its own OUT_DIR to the link search path.
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");

    println!("cargo:rustc-link-arg=-Tws63-link.x");
    println!("cargo:rerun-if-changed=build.rs");
}
