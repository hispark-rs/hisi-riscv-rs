fn main() {
    println!("cargo:rustc-link-arg=-Thisi-riscv-link.x");
    println!("cargo:rustc-link-arg=--emit-relocs");
    println!("cargo:rerun-if-changed=build.rs");
}
