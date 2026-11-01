# WS63 GPIO async 模型夹具

仅用于 QEMU：模型把 GPIO0 输出 0/1 经 pinmux 连接至 GPIO1 输入 8/9。
这不是实际开发板接线或固件烧录流程，不要据此对实板驱动这些引脚。

覆盖同组两个 waiter、空中断、取消（包括已锁存事件和保留另一引脚）、即时电平、
上升/下降/any-edge 及真实 GPIO_INT1 陷阱。使用显式 poll 检查状态，不依赖当前
有已知唤醒缺陷的最小 block_on，也不建立另一套产品执行器。

在父仓执行：

```sh
cargo build --locked -Zbuild-std=core,alloc --manifest-path examples/ws63-gpio-async-model/Cargo.toml --release
uv run --script scripts/test-gpio-async-qemu.py --chip ws63 --qemu <qemu-system-riscv32> --elf examples/ws63-gpio-async-model/target/riscv32imfc-unknown-none-elf/release/ws63_gpio_async_model --output <new-report.json>
```

PASS 只证明这些模型场景。芯片引脚电气、其他 GPIO bank、时序、跨核交接和实板
恢复仍需具名验收；原 any-edge 的初始极性选择限制保持不变。
