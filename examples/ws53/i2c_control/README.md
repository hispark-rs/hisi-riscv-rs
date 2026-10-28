# WS53 I2C QEMU 夹具

此示例只在明确假设的 QEMU profile 中运行。pad 是虚拟独占权限，不是板卡接线；
不得直接加载到实板。普通默认 RAM 示例仍为 ram_smoke。

从父仓库运行：

```sh
cargo build --locked --manifest-path examples/ws53/Cargo.toml -p ws53_i2c_control --release -Zbuild-std=core
uv run --script scripts/test-ws53-i2c.py --qemu /path/to/qemu-system-riscv32 --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_i2c_control
cargo build --locked --manifest-path examples/ws53/Cargo.toml -p ws53_i2c_control --release -Zbuild-std=core --features clock-stall-smoke
uv run --script scripts/test-ws53-i2c.py --qemu /path/to/qemu-system-riscv32 --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_i2c_control --clock-stall
```

正常场景检查两实例、7/10位地址、连续 Operation、重复 START、长读、地址 NACK
恢复、资源回收和再次接管。停钟场景检查启用超时、有限清理和另一实例正常工作。
脚本固定启用 assume-profile 并精确检查诊断；不宣称物理速率或实板支持。
