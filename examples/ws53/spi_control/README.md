# WS53 SPI/QSPI 单线 QEMU 夹具

只在显式假设的 QEMU profile 内运行。Pad 是合成权限，不是板卡复用映射；
不得直接加载实板。默认示例仍是 ram_smoke，本程序不访问 Flash。

从父仓库执行：

```sh
cargo build --locked --manifest-path examples/ws53/Cargo.toml -p ws53_spi_control --release -Zbuild-std=core
uv run --script scripts/test-ws53-spi.py --qemu /path/to/qemu-system-riscv32 --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_spi_control
cargo build --locked --manifest-path examples/ws53/Cargo.toml -p ws53_spi_control --release -Zbuild-std=core --features clock-stall-smoke
uv run --script scripts/test-ws53-spi.py --qemu /path/to/qemu-system-riscv32 --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_spi_control --clock-stall
```

正常场景通过真实 HAL/PAC 访问两个布局，分别测试8/32位、四种CPOL/CPHA配置、
100字传输、不等长传输、原地传输、flush、资源回收和重新接管。虚拟对端返回
发送字与0xa5a5a5a5异或后的对应字宽；没有外设实物或native CS。

停钟场景在启动前切断SPI0模型源时钟。ENABLE寄存器仍可读写，排队的数据无法
完成，HAL报告RX等待超时和清理失败，保留poisoned实例；重复操作、recover和
try_free不能伪报成功。QSPI1仍完成正常测试。Drop仅有限尝试，不声称总线释放。
这与I2C模型的独立ENABLE_STATUS超时不同，不能套用同一失败点。

脚本先检查ELF RAM边界、ilp32f与原子指令，再启用显式模型假设，精确检查三行
串口结果与诊断。FIFO8、32MHz和1us跨域延迟不代表实板测量；不验证双/四线、
DMA、native CS、引脚、电气、精确时序或低功耗，也不构成稳定API验收。

`ws53_spi_collision_build.py --bus spi0`（或 `qspi1`）配合
`test-ws53-spi-collision.py` 测试模型合成的“仅状态、无中断”碰撞，确认
状态被驱动保存、清除注入后同一实例恢复并且另一实例可用。该输入不模拟
物理多主碰撞，也不证明硅片状态位与中断位的实际联动；DMA 路径另待 QEMU 验证。
