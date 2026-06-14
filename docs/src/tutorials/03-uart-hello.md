# 第一个 UART 程序（uart_hello）

上一课芯片只会闪灯，这一课我们让它**开口打印**。我们在 QEMU 里跑 `uart_hello`，
在控制台上看到它打印出 `Hello from WS63 on QEMU!`，再加上一个不断递增的计数器。

> QEMU 是本课**可靠的成功路径**。`uart_hello` 就是为 QEMU 设计的：它故意不初始化时钟，
> 只碰 UART0 寄存器（`0x4401_0000`）。

## 第 1 步：编译 uart_hello

在仓库根目录执行：

```bash
cargo build -p uart_hello --release
```

产物在：

```
target/riscv32imfc-unknown-none-elf/release/uart_hello
```

## 第 2 步：在 QEMU 里运行

用 WS63 机器模型加载这个 ELF。`-nographic` 会把 UART0 接到你的终端：

```bash
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/uart_hello
```

## 第 3 步：看到它说话

控制台上你应当立刻看到 banner，随后是不断递增的 tick 计数：

```
Hello from WS63 on QEMU!
ws63-qemu: UART0 @ 0x44010000 is alive.
tick 0
tick 1
tick 2
tick 3
...
```

计数器会一直涨下去。看到这些输出，说明你的 Rust 程序成功通过 UART0 打印了文本。

按 `Ctrl-A` 然后按 `X` 退出 QEMU。

成功了！你刚刚让 WS63 程序打印出了第一行串口日志。

## 关于真机

在真正的硬件上，串口 banner 的点亮工作**仍在进行中**——真机需要先初始化时钟，
让波特率分频与 PLL 匹配，而 `uart_hello` 为了适配 QEMU 故意省去了这一步。
所以本课**不承诺**真机上能看到这条 banner；要在真板上稳定看到串口输出，
请关注 [HIL 测试框架](../explanation/hil-framework.md) 的进展。

> 想"看见"真机上的可观测行为，最稳妥的仍是第 2 课的 blinky（GPIO 翻转，已验证）。
> 第 5 课会带你做这件事。

下一课我们用 QEMU 跑更多示例、看中断输出 ——
[在 QEMU 里运行与调试](04-qemu.md)。
