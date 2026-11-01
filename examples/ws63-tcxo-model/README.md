# WS63 TCXO model smoke fixture

This fixture runs TCXO and Embassy reads in the WS63 QEMU model. It does not
configure a physical board or validate long uptime, real clocks, reset, or
interrupt behavior. Its `model-only` feature is required for building.

```sh
cargo build --locked -Zbuild-std=core,alloc --release \
  --manifest-path examples/ws63-tcxo-model/Cargo.toml --features model-only
uv run scripts/test-ws63-tcxo-model.py --qemu <qemu-system-riscv32> \
  --elf examples/ws63-tcxo-model/target/riscv32imfc-unknown-none-elf/release/ws63-tcxo-model \
  --output <new-report.json>
```
