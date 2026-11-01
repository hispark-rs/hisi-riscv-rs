fn main() {
    println!("cargo:rustc-link-arg-bin=ipc-model=-Thisi-riscv-link.x");
    println!("cargo:rustc-link-arg-bin=ipc-spsc-model=-Thisi-riscv-link.x");
    println!("cargo:rustc-link-arg-bin=ipc-random-model=-Thisi-riscv-link.x");
}
