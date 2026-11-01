# BS21 I²C examples

`bs21_i2c_scan` is the existing board-oriented scan example. The separate
`bs21_i2c_model` binary is a QEMU-only regression fixture: the model supplies a
virtual target at `0x50` and a NACK at `0x51`. It does not validate physical
wiring, repeated START waveforms, other abort reasons, or bus recovery.

From the integration repository root:

```sh
cargo build --locked -Zbuild-std=core,alloc --release \
  --manifest-path examples/bs21/Cargo.toml -p bs21_i2c_scan \
  --bin bs21_i2c_model --features model-only
uv run scripts/test-bs21-i2c-model.py --qemu <qemu-system-riscv32> \
  --elf examples/bs21/target/riscv32imfc-unknown-none-elf/release/bs21_i2c_model \
  --output <new-report.json>
```
