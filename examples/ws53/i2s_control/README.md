# WS53 I2S 轮询流 QEMU 测试

本示例仅运行于显式 `ws53-i2s.assume-profile=on` 模型。使用真实 Rust HAL/PAC，
覆盖16/18/20/24/32位、主/从模式、上升/下降边沿，共20组配置；每组连续收发
100对样本并检查虚拟对端返回的数据，随后测试低于RX中断阈值的短接收、
TX FIFO清空和丢弃恢复。FIFO空不代表最后一帧已发送完成。

`clock-stall-smoke`变体在模型上游时钟停止时运行：TX队列满和RX无数据均必须
在预算内超时、标记poisoned；恢复可读回配置，但再次传输仍超时。配置恢复成功
不能证明上游时钟运行。收发缓冲区不会被驱动保留，CPU IRQ67保持屏蔽。

从主工程根目录运行（QEMU需包含本轮I2S模型）：

```sh
cargo build --locked --manifest-path examples/ws53/Cargo.toml -p ws53_i2s_control \
  -Zbuild-std=core --release --target riscv32imfc-unknown-none-elf
uv run --script scripts/test-ws53-i2s.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_i2s_control
cargo build --locked --manifest-path examples/ws53/Cargo.toml -p ws53_i2s_control \
  -Zbuild-std=core --release --target riscv32imfc-unknown-none-elf --features clock-stall-smoke
uv run --script scripts/test-ws53-i2s.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_i2s_control --clock-stall
```

运行器核对RAM段、ABI、栈边界和无A扩展指令，严格检查三行UART输出、模型诊断、
结束时移位器无在途帧。不连接调试器、串口或实板，不烧录。

示例的MGPIO17/20/19/18是模型中的四个独占pad token，不会配置物理复用。
`assume_application_ownership`和`release_after_external_quiescence`的安全前提
由显式模型提供：无控制核/DMA并发，模型复位确实取消所有队列及部分帧。
这些前提尚未在实板证明，**不得将此示例直接加载开发板**。PCM/多声道、DMA、
中断驱动/async、精确音频时序和无损停止仍未适配或验收。
