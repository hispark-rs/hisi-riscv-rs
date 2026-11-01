# WS53 TCXO counter — QEMU only

此例验证真实 Rust HAL/PAC 的 TCXO 计数取样与错误路径，**不能加载到实板**。
尚未证明当前固件的计数器、时钟和双核归属；默认 `ram_smoke` 不包含本例。

构造驱动不改变硬件。普通镜像检查未使能错误、保留继承的计数起点、64位取样、
显式32MHz换算、清零后识别旧时间戳及继续计数。`timeout-smoke` 镜像验证取样
失败返回 `SnapshotTimeout`，不会伪装成成功读到零。所有轮询有上限。

从父仓库构建并保存两个镜像（不要使用 `cargo run`）：

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml -p ws53_tcxo_control --release --locked
cp examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_tcxo_control /tmp/ws53-tcxo.elf
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml -p ws53_tcxo_control --release --locked --features timeout-smoke
cp examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_tcxo_control /tmp/ws53-tcxo-timeout.elf
uv run --script scripts/test-ws53-tcxo.py --qemu /path/to/qemu-system-riscv32 \
  --elf /tmp/ws53-tcxo.elf --timeout-elf /tmp/ws53-tcxo-timeout.elf
```

QEMU 必须含 `ws53-tcxo` 模型。测试器先检查 ELF，再运行四种场景：命令位读回零、
命令位保持上次值、停止输入时钟、阻塞取样。两种命令读回配置用于发现误重放
CLEAR 的问题，不代表已证明芯片的实际读回时序。只有精确的 UART 成功标记且
没有任何未实现访问诊断才通过。复用 `scripts/ws53_qemu_fixture.py` 完成 QMP、
qtest 及退出清理；它也用于 WDT 测试。

参考 SDK 使用 TCXO ROM 组件；源码辅助函数对照不等于 ROM 执行证明。32MHz 是
参考配置，不是实测频率。计数起点、清零、睡眠恢复和频率变化必须由拥有者协调，
本例不提供全局 `Instant`、低功耗时间补偿或 Embassy 时间驱动，仿真通过不计入
20轮实板 RAM/IRQ 验收。
