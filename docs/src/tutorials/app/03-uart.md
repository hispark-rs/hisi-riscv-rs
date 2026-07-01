# 改造成一个 UART 程序

上一课你的工程只会闪灯，这一课我们让它**开口打印**。最简单的办法是用
`uart_hello` 起手重新生成一个工程，在 QEMU 里看到它打印 `Hello from WS63 ...`。

> QEMU 是本课**可靠的成功路径**。`uart_hello` 就是为 QEMU 设计的：它故意不初始化时钟，
> 只碰 UART0 寄存器。

## 第 1 步：用 uart_hello 起手生成工程

再跑一次 `cargo generate`，这次 **Starter app** 选 `uart_hello`：

```bash
cargo generate --git https://github.com/hispark-rs/hisi-rs-template
```

- 项目名：比如 `my-uart`。
- **Target chip**：`ws63`（默认）。
- **Starter app**：选 `uart_hello`。
- **App partition flash address**：默认 `0x00230000`。

进入工程目录：

```bash
cd my-uart
```

## 第 2 步：在 QEMU 里运行

```bash
just run
```

`-nographic` 会把 UART0 接到你的终端。

## 第 3 步：看到它说话

控制台上你应当立刻看到 banner，随后是不断递增的 tick 计数：

```console
Hello from WS63 on QEMU!
UART0 @ 0x44010000 is alive.
tick 0
tick 1
tick 2
...
```

计数器会一直涨下去。看到这些输出，说明你的 Rust 程序成功通过 UART0 打印了文本。

按 `Ctrl-A` 然后按 `X` 退出 QEMU。

成功了！你刚刚让一个属于你的工程打印出了第一行串口日志。

## 关于真机

在真正的硬件上，串口 banner 的点亮工作**仍在进行中**——真机需要先初始化时钟，
让波特率分频与 PLL 匹配，而 `uart_hello` 为了适配 QEMU 故意省去了这一步。
所以本课**不承诺**真机上能看到这条 banner；要在真板上稳定看到串口输出，
请关注 [HIL 测试框架](../../explanation/07-hil-framework.md) 的进展。

> 想在真机上看到稳定可观测的行为，最稳妥的仍是上一课的 blinky（GPIO 翻转，已验证）。

接下来想做点什么？

- 想完成具体任务（加驱动、调试读内存）——看 [操作指南](../../how-to/00-index.md)。
- 想查命令、地址、API——看 [参考](../../reference/00-index.md)。
- 想搞懂背后的原理——看 [原理与背景](../../explanation/00-index.md)。
- 想给生态本身贡献代码——看 [生态贡献者路径](../contrib/00-index.md)。
