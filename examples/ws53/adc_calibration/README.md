# WS53 ADC 校准 QEMU 用例

仅运行于显式ADC校准模型，使用实际Rust HAL/PAC。不会连接开发板、扫描IO、
启动ADC采样或MCU_DIAG写入，也不烧录。

从主工程根目录构建并运行：

```sh
cargo build --locked --manifest-path examples/ws53/Cargo.toml -p ws53_adc_calibration \
  -Zbuild-std=core --release --target riscv32imfc-unknown-none-elf
uv run --script scripts/test-ws53-adc.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_adc_calibration
```

运行器执行13个场景：自动偏移输入0/3/4/31/32/59/60/63，每个重复20次完整
自动偏移→手动偏移→电容校准；另测试断钟、手动阶段停滞、电容阶段停滞、
无法清除旧完成标志、电容错误。错误场景重复两次有界尝试，必须保持无校准
报告；正常场景核对源码分段修正。每个场景检查ELF/ABI/RAM边界、串口三行
输出、唯一的模型假设诊断、QOM启动/完成计数、最终两路校准使能读回为0。

模型的5us模拟稳定时间、100/128/160us阶段时间、清除/复位效果均为明确假设，
不能证明电气时序、真实校准精度或硬件排空。程序中的unsafe资源交接前提只在
这个QEMU配置成立，**不得直接加载实板**。板上ADC/CPU trace/MCU_DIAG、缓存与
缓冲区所有权仍需确认。本例不执行采样；独立的`adc_capture`用例验证单次采样
及外部模型排空交接，仍不提供安全自动回收或实板排空保证。
