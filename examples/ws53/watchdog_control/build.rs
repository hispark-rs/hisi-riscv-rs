fn main() {
    println!("cargo:rustc-link-arg=-Thisi-riscv-link.x");
    println!("cargo:rerun-if-changed=build.rs");
}
