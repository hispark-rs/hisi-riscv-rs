fn main() {
    println!("cargo:rustc-link-arg=-Thisi-riscv-link.x");
    // Retain resolved relocations for the post-link RAM closure audit.
    println!("cargo:rustc-link-arg=--emit-relocs");
    println!("cargo:rerun-if-changed=build.rs");
}
