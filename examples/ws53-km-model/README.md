# WS53 KM 的 Rust/QEMU 对照程序

仅供包含 `ws53-km` 的 QEMU 使用，不应烧录到开发板。本例显式假定整个 KM 已交接、
没有控制核/Flash/SYMC 密钥消费者，不能复用为真实资源交接代码。

```sh
cargo build --locked -Zbuild-std=core,alloc \
  --manifest-path examples/ws53-km-model/Cargo.toml \
  --release --features model-handoff --bin km-model
uv run --script scripts/check-ws53-ram-elf.py \
  examples/ws53-km-model/target/riscv32imfc-unknown-none-elf/release/km-model
uv run --script scripts/test-ws53-km-model.py \
  --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53-km-model/target/riscv32imfc-unknown-none-elf/release/km-model \
  --output target/ws53-km-model
```

输出目录必须不存在；例程有独立 workspace 和锁文件。主机只写 ELF 中声明的测试
邮箱命令/参数，不写密钥、槽或外设 MMIO。实际 RISC-V CPU 创建公开测试字节，调用
生产 `hisi-hal::km`，持有真实 lease，执行 load/validate/release/Drop/forget。
主机通过独立 QOM/GPIO 注入虚拟故障，核对模型私有字节、访问计数和固件状态。

357个场景覆盖220个正常装载、48个parity拒绝、8个HMAC长度拒绝、20个逐段停滞、
20个逐段归属丢失、各4个其余故障/资源保留场景及十槽同时占用。释放后能重用槽；
Drop/forget及终止错误后不可重用，错误后的操作不再产生MMIO。释放不会把仍保留
字节当成物理擦除成功。字节检查包括全部非目标槽，防止跨槽写入。

测试等待使用有限的实际 NOP 指令，推动 `icount` 虚拟时间；未校准为真实微秒。
首轮使用的 `spin_loop` 在该目标可为空、循环被优化掉，导致32次轮询不够等待模型
5µs时延；已修正fixture，未增加 HAL 的隐含超时或放宽模型完成规则。

保存各场景的 QMP/qtest 操作、诊断与报告。脚本会拒绝主机向邮箱之外写入；ELF检查
核对 ABI、RAM区间、无A指令及栈边界。模型默认关闭，运行器明确启用假设profile。
本例不计算 AES/SM4/HMAC，不验证 KM IRQ/async，不授予真实共享交接或密钥擦除资格；
RAM实板、Flash实板与断电冷启动分别保持未验收。
