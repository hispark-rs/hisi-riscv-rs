# 改造成一个 UART 程序

上一课你的工程只会闪灯，这一课我们让它**开口打印**。最简单的办法是用
`uart_hello` 起手重新生成一个工程，在 QEMU 里看到它打印 `Hello from WS63 ...`。

> 本课用 QEMU 完成无需硬件的学习路径。生成的 `uart_hello` 使用 HAL UART 驱动和
> flashboot boot-clock 配置；同一路径已在 WS63 真实硅片上通过 115200 8N1 UART smoke。

## 第 1 步：用 uart_hello 起手生成工程

再跑一次 `cargo generate`，这次 **Starter app** 选 `uart_hello`：

{{#tutorial-snippet app_uart_generate}}

- 项目名：比如 `my-uart`。
- **Target chip**：`ws63`（默认）。
- **Starter app**：选 `uart_hello`。
- **App partition flash address**：默认 `0x00230000`。

进入工程目录：

{{#tutorial-snippet app_uart_cd}}

## 第 2 步：在 QEMU 里运行

{{#tutorial-snippet app_uart_run}}

`-nographic` 会把 UART0 接到你的终端。

## 第 3 步：看到它说话

控制台上你应当立刻看到 banner，随后是不断递增的 tick 计数：

```console
Hello from WS63 (HAL UART driver)!
tick 0
tick 1
tick 2
...
```

计数器会一直涨下去。看到这些输出，说明你的 Rust 程序成功通过 UART0 打印了文本。

按 `Ctrl-A` 然后按 `X` 退出 QEMU。

成功了！你刚刚让一个属于你的工程打印出了第一行串口日志。

## 关于真机

真机路径由 `hil/hil-smoke.sh` 构建同一个 `uart_hello`、生成完整 flash image、烧录并匹配
`Hello from WS63` 标记。烧录器、串口和 runner 参数见
[HIL 脚本与 runner 环境变量](../../reference/07-hil-markers.md)；这些硬件步骤不在普通
GitHub-hosted 教程 CI 中执行，而由 self-hosted HIL 轨道保留硅片证据。

接下来想做点什么？

- 想完成具体任务（加驱动、调试读内存）——看 [操作指南](../../how-to/00-index.md)。
- 想查命令、地址、API——看 [参考](../../reference/00-index.md)。
- 想搞懂背后的原理——看 [原理与背景](../../explanation/00-index.md)。
- 想给生态本身贡献代码——看 [生态贡献者路径](../contrib/00-index.md)。
