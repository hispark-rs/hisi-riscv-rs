//! Build script for ws63-rf-rs.
//!
//! Sets up linking against the vendor RF blobs in the `ws63-RF` submodule
//! (nested under this crate at `ws63-rf-rs/ws63-RF` so the blob delivery is
//! owned by this crate and not reached into laterally) and
//! supplies the Wi-Fi packet-RAM linker symbols the blobs reference. These
//! `cargo:rustc-link-*` directives propagate to any binary that depends on
//! ws63-rf-rs (the library itself is not linked).
//!
//! NOTE: the actual `--whole-archive` link of a specific blob is left to the
//! consumer (an example/firmware) so a plain `cargo build` of the library does
//! not require the blobs — see `examples`/the `rf_port_demo`. We only export the
//! search path + the packet-RAM symbols here.
use std::path::PathBuf;

fn main() {
    // ws63-RF/lib holds the vendor archives (rom_data/dmac/bg_common/bt...).
    // The submodule is nested inside this crate (ws63-rf-rs/ws63-RF).
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let lib_dir = PathBuf::from(&manifest).join("ws63-RF/lib");
    if let Ok(canon) = lib_dir.canonicalize() {
        println!("cargo:rustc-link-search=native={}", canon.display());
        // Re-export the path so downstream build scripts can locate the blobs.
        println!("cargo:rustc-env=WS63_RF_LIB_DIR={}", canon.display());
    }

    // NOTE: the Wi-Fi packet-RAM linker symbols the blobs reference
    // (__wifi_pkt_ram_begin__ / __wifi_pkt_ram_end__) are the *consumer
    // firmware's* responsibility to provide (its linker owns the memory map) —
    // a build-script `rustc-link-arg` does not reliably propagate from a library
    // dependency to the final binary. The C SDK reserves 0xA00000..0xA0C000
    // (48 KB) as `.wifi_pkt_ram`; consumers should supply these via a `--defsym`
    // or, better, a reserved NOLOAD region (ROADMAP phase 4). See the
    // `rf_port_demo` example for the scaffold recipe.

    // For this crate's own examples (e.g. sched_selftest): link via hisi-riscv-rt's
    // scripts. `rustc-link-arg` applies only to THIS package's bins/examples/
    // tests, not to downstream consumers (they set their own).
    println!("cargo:rustc-link-arg=-Tws63-link.x");

    println!("cargo:rerun-if-changed=build.rs");
}
