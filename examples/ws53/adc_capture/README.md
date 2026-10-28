# WS53 单次 ADC 采样与缓冲区保留

这是实际 Rust HAL/PAC 的 **QEMU 专用** RAM 用例，不得加载到开发板。
程序开始后需要配套主机测试器握手；直接启动会等待主机，不能单独宣称通过。

```sh
cargo build --locked --manifest-path examples/ws53/Cargo.toml -p ws53_adc_capture \
  -Zbuild-std=core --release --target riscv32imfc-unknown-none-elf
uv run --script scripts/test-ws53-adc-capture.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_adc_capture
```

QEMU需包含本地模型提交`a1cb102`。运行器自动启用显式校准/捕获profile，并执行：

| 场景 | 独立检查 |
| --- | --- |
| N 正常 | 0/3/7三通道共9字，RAM格式/顺序和两侧哨兵 |
| E 早完成＋总线停滞 | DONE及最终地址可见，但在途计数1且RAM未改变；不能取回缓冲 |
| T 总线停滞超时 | HAL超时并停用诊断，仍有1笔待写入；恢复总线后才归还 |
| B 总线错误 | 不返回完成，RAM保持原值，错误计数1，外部排空后可以回收 |
| S 转换停滞 | 有限超时，没有RAM写入 |
| D Drop | 请求停止，但最后一笔写入仍可到达；静态缓冲区永久保留 |
| F forget | 无法依赖析构停止；静态内存仍有效且无安全引用可以重建 |

除Drop/forget外，主机在固件报告状态后暂停虚拟CPU，独立检查诊断使能、在途
写入计数和RAM；必要时恢复总线并确认待写入完成，再核对RAM/哨兵并发送R。
固件只在这个模型排空证明之后调用unsafe资源取回；校准报告随之失效。
QOM状态不是芯片寄存器，R也不是硬件停止协议。此握手仅适用于明确无其他写入者、
无缓存的测试模型，不能迁移为“等一会儿/读DONE就释放”的实板做法。

HAL要求缓冲区为应用核独占的静态一致性DTCM，模拟引脚以规范化Pad标识交接，
MCU_DIAG必须单独取得所有权。构造profile的unsafe前提包括端地址/长度语义、
源配置与时钟；这些尚未实板确认。配置、错误/超时和取消全部保留资源，
不再自动重启。无借用栈缓冲区、无安全自动释放、无真实电压精度或缓存维护承诺。
