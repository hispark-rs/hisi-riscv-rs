//! Build script for ws63-rf-rs.
//!
//! Sets up linking against the vendor RF blobs owned by `ws63-radio-sys` and
//! records the Wi-Fi packet-RAM linker contract the blobs reference. These
//! `cargo:rustc-link-*` directives propagate to any binary that depends on
//! ws63-rf-rs (the library itself is not linked).
//!
//! NOTE: the actual `--whole-archive` link of a specific blob is left to the
//! consumer (an example/firmware) so a plain `cargo build` of the library does
//! not require the blobs — see `examples`/the `rf_port_demo`. We only export the
//! search path here.
use std::path::PathBuf;

fn main() {
    let lib_dir = PathBuf::from(
        std::env::var_os("DEP_WS63_RADIO_SYS_LIB_DIR")
            .expect("ws63-radio-sys did not export its archive directory"),
    );
    if let Ok(canon) = lib_dir.canonicalize() {
        println!("cargo:rustc-link-search=native={}", canon.display());
    }

    // NOTE: the Wi-Fi packet-RAM linker symbols the blobs reference
    // (__wifi_pkt_ram_begin__ / __wifi_pkt_ram_end__) are the final firmware
    // linker's responsibility because it owns the memory map. The WS63
    // hisi-riscv-rt adapter reserves 0xA00000..0xA0C000 (48 KB) as a real
    // `.wifi_pkt_ram` NOLOAD section. Custom runtimes must provide the same
    // symbols and range.

    // For this crate's own examples (e.g. sched_selftest): link via hisi-riscv-rt's
    // scripts. `rustc-link-arg` applies only to THIS package's bins/examples/
    // tests, not to downstream consumers (they set their own).
    let target = std::env::var("TARGET").expect("TARGET");
    if target.starts_with("riscv32") {
        println!("cargo:rustc-link-arg=-Thisi-riscv-link.x");
    }

    println!("cargo:rerun-if-changed=build.rs");
}
