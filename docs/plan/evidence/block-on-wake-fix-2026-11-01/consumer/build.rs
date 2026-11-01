fn main() {
    println!("cargo:rustc-link-arg=-Thisi-riscv-link.x");
    println!("cargo:rustc-link-arg=--no-relax");
}
