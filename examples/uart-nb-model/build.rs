fn main() {
    if std::env::var_os("CARGO_FEATURE_CHIP_BS21").is_some() {
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        std::fs::write(out.join("memory.x"), include_bytes!("bs21-memory.x")).unwrap();
        println!("cargo:rustc-link-search={}", out.display());
    }
    println!("cargo:rerun-if-changed=bs21-memory.x");
    println!("cargo:rustc-link-arg=-Thisi-riscv-link.x");
    println!("cargo:rustc-link-arg=--no-relax");
}
