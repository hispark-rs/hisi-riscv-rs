fn main() {
    if std::env::var_os("CARGO_FEATURE_I2C_WIRE_PROBE").is_some() {
        for (key, _) in std::env::vars() {
            if key.starts_with("CARGO_FEATURE_") {
                assert!(
                    matches!(
                        key.as_str(),
                        "CARGO_FEATURE_I2C_WIRE_PROBE" | "CARGO_FEATURE_I2C_WIRE_BUS1"
                    ),
                    "I2C wire observation requires its isolated feature profile"
                );
            }
        }
    }
    println!("cargo:rustc-link-arg=-Thisi-riscv-link.x");
    println!("cargo:rerun-if-changed=build.rs");
}
