fn main() {
    println!("cargo:rustc-link-arg-bin=sdio-init-model=-Thisi-riscv-link.x");
    println!("cargo:rustc-link-arg-bin=sdio-transfer-model=-Thisi-riscv-link.x");
    println!("cargo:rustc-link-arg-bin=sdio-async-model=-Thisi-riscv-link.x");
}
