# WS53 watchdog control — QEMU only

此镜像只测试 Rust HAL 与显式假设时序的 QEMU WDT 模型，**不能加载到实板**。
当前没有证明实板看门狗时钟/复位/NMI交接，也没有验证真实超时后的恢复路径。
它不属于默认 `ram_smoke` workspace 成员选择，不参与20轮实板验收。

普通版本通过真实PAC访问测试：锁定时拒绝喂狗/取样、启动、计数递减、喂狗重新
装载、停止后计数不变及再次启动。`timeout-smoke` 版本由测试器在CPU启动前
分别停住WDT输入时钟或阻塞取样握手，确认返回 `SnapshotTimeout`，而非 `Ok(0)`。
延时使用已验证的32MHz TIMER通道1，不猜CPU频率或循环次数；轮询均有失败上限。
UART2沿用QEMU提供的已配置控制台状态，TIMER和WDT归属只在这个仿真fixture中成立。

从父仓库根目录构建两个镜像，分别保存（勿使用 `cargo run`）：

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml -p ws53_watchdog_control --release --locked
cp examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_watchdog_control /tmp/ws53-watchdog.elf
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml -p ws53_watchdog_control --release --locked --features timeout-smoke
cp examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_watchdog_control /tmp/ws53-watchdog-timeout.elf
uv run --script scripts/test-ws53-watchdog.py --qemu /path/to/qemu-system-riscv32 \
  --elf /tmp/ws53-watchdog.elf --timeout-elf /tmp/ws53-watchdog-timeout.elf
```

测试器先验证两个ELF的RAM地址/ABI/指令契约，再运行三种场景；使用确定性
instruction-count虚拟时钟，精确核对UART标记及唯一允许的“假设时序”诊断。
任何未实现MMIO、CPU NMI投递或SoC复位诊断都会使该HAL测试失败。完成时关闭
WDT/TIMER，QMP退出QEMU前保持qtest连接打开。

QEMU需至少包含模型提交 `c0ee224`。本测试只使用明确
启用的32000Hz、低字节255、count+1触发、阶段重新装载模型假设，不能证明真实
芯片遵循这些时序。没有触发看门狗超时、NMI或复位；相应输出路径仅由独立模型
测试覆盖，完整SoC复位和真实NMI投递仍未实现。没有烧录或物理设备访问。
