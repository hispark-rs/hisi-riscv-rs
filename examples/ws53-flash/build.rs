fn main() {
    // The shared RAM fixture has an optional divider experiment. Recognize
    // that cfg without exposing/enabling the experiment in this workspace.
    println!("cargo:rustc-check-cfg=cfg(feature, values(\"pwm0-clock-probe\"))");
    println!("cargo:rustc-link-arg=-Thisi-riscv-link.x");
    println!("cargo:rerun-if-changed=build.rs");
}
