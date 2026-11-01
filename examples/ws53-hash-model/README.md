# WS53 HASH 的 Rust/QEMU 对照程序

本例只供显式启用 HASH 假设配置的 QEMU 使用，不能直接烧录到开发板。它调用生产
`hisi-hal::hash`，运行六种非 HMAC 算法和两个通道；没有模拟控制核交接，也没有证明
实板 DMA 可访问 DTCM、缓存一致性或在途请求已排空。所有分配都保留，不自动回收。

从主仓库根目录构建（独立 workspace 和锁文件，不与其他芯片合并 feature）：

```sh
cargo build --locked -Zbuild-std=core,alloc \
  --manifest-path examples/ws53-hash-model/Cargo.toml \
  --release --features model-handoff --bin hash-model
uv run --script scripts/check-ws53-ram-elf.py \
  examples/ws53-hash-model/target/riscv32imfc-unknown-none-elf/release/hash-model
uv run --script scripts/test-ws53-hash-model.py \
  --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53-hash-model/target/riscv32imfc-unknown-none-elf/release/hash-model \
  --output target/ws53-hash-model
```

QEMU 需包含 `ws53-hash` 模型；运行器明确启用 `assume-profile`。输出目录必须不存在。
198 个独立进程场景包括 156 个摘要对照、12 个消息长度拒绝和 30 个故障/取消场景。
摘要对照使用 Python/OpenSSL `hashlib`，包括空消息、`abc`、`abcd` 重复、填充边界和
HAL 最大长度。故障覆盖两通道的 clear 超时、拒绝认领、应答失败、归属丢失、原始
错误快照、DMA 停滞/总线错误、完成抑制，以及已接受读取后的取消、Drop 和 forget。

主机只写 ELF 声明的测试邮箱中的命令/参数，通过独立输入注入虚拟故障，不初始化
外设 MMIO、消息或描述符。CPU 独立完成初始化、软件填充、描述符写入、PAC 访问和
摘要读取。运行器核对 ELF ABI、段/内存边界、静态缓冲区/描述符的大小、对齐及不重叠，
保存每个场景的命令、QMP/qtest 操作、诊断和最终报告；守卫区及消息/表不得被 DMA 改写。

测试邮箱的 `delay` 仅提供有限次数的 CPU 进展，未校准为真实微秒；`icount` 只驱动模型
虚拟时间。HASH IRQ59 的屏蔽和应答由 QEMU 仓库的寄存器测试覆盖，本例不声称验证了
Rust HASH 中断处理、async、流式处理、密钥功能、实板、Flash 或冷启动。
