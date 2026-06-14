# 构建与运行示例集

QEMU 是不用硬件就能跑 WS63 固件的"软件在环"环境。作为生态贡献者，你会反复
构建并运行 `examples/ws63/*` 里的示例来验证改动。本课带你跑通示例目录里的几个
代表：blinky（GPIO trace）、uart_hello（banner）、timer_irq / gpio_irq（中断串口输出）、
semihost_selftest（半主机退出码）。每一步都有可见结果。

> QEMU 模型的原理见 [QEMU 模型](../../explanation/qemu-model.md)。

示例都是**根工作区的成员**，所以一律从仓库根目录用 `-p <name>` 构建，再用同一条
命令模板运行：

```bash
cargo build -p <name> --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/<name>
```

- `-M ws63`：WS63 机器模型。
- `-nographic`：无图形界面，把 UART0 接到当前终端。
- `-bios none`：不加载额外固件，直接跑我们的 `-kernel`。
- `-kernel <elf>`：要运行的示例 ELF。

退出 QEMU 始终是：按 `Ctrl-A`，再按 `X`。

## 第 1 步：blinky（GPIO 翻转，无串口）

```bash
cargo build -p blinky --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/blinky
```

blinky 把 **GPIO0** 配成推挽输出，死循环里拉高、延时、拉低、延时。它**没有串口输出**，
所以控制台不会打印东西——这是预期的：程序在安静地翻转引脚（机器 trace 里能看到 GPIO0
每 500 ms 变一次）。想"看见"可观测行为，用下面带串口的示例。

> 关于构建目标和产物路径的细节，见 [构建一个示例](../../how-to/build-example.md)。

## 第 2 步：uart_hello（串口 banner）

```bash
cargo build -p uart_hello --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/uart_hello
```

`uart_hello` 为 QEMU 设计：它故意不初始化时钟，只碰 UART0 寄存器（`0x4401_0000`）。
控制台上你应当立刻看到 banner，随后是不断递增的 tick 计数：

```console
Hello from WS63 on QEMU!
ws63-qemu: UART0 @ 0x44010000 is alive.
tick 0
tick 1
tick 2
...
```

> 真机上的串口 banner 仍在打磨（需要先初始化时钟），本课只承诺 QEMU。第 3 课的
> blinky 才是当前确认可观测的真机行为。

## 第 3 步：timer_irq（定时器中断）

```bash
cargo build -p timer_irq --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/timer_irq
```

`TIMER_0` 周期性触发 IRQ 26，处理函数每次累加计数并打印。你应当看到：

```console
WS63 timer-IRQ test (TIMER_0 -> IRQ 26)
timer irq #0
timer irq #1
timer irq #2
...
OK: timer interrupts delivered
```

看到 `timer irq #` 不断递增、最后出现 `OK: timer interrupts delivered`，
说明 QEMU 的中断投递闭环正常。

## 第 4 步：gpio_irq（GPIO 中断）

```bash
cargo build -p gpio_irq --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/gpio_irq
```

这个示例把 GPIO0 的边沿映射到一个自定义本地 IRQ（≥32）。你应当看到：

```console
WS63 GPIO-IRQ test (GPIO0 pin0 -> IRQ 33, custom local)
gpio irq #0
gpio irq #1
...
OK: custom local IRQ (>=32) delivered
```

## 第 5 步：semihost_selftest（半主机退出码）

有些示例不靠串口打印，而是通过 RISC-V **半主机**把结果报告给宿主——
`semihost_selftest` 跑完后会用半主机的"退出"操作返回退出码：PASS 返回 `0`，FAIL 返回 `1`，
panic 返回 `2`。这个退出码会变成 **QEMU 进程自己的退出码**，非常适合写进自动化脚本。

要让半主机生效，必须加上 `-semihosting`：

```bash
cargo build -p semihost_selftest --release
qemu-system-riscv32 -M ws63 -nographic -bios none -semihosting \
    -kernel target/riscv32imfc-unknown-none-elf/release/semihost_selftest
```

控制台会打印（通过半主机控制台）：

```console
semihost_selftest: PASS
```

随后 QEMU 自行退出。检查它的退出码：

```bash
echo $?
```

你应当看到：

```console
0
```

`0` 就代表自检通过——脚本可以直接据此判定成败，无需解析串口文本。

> 各示例的预期标记串汇总见 [示例目录与验证标记串](../../reference/examples.md)；
> 半主机相关的环境变量见 [HIL 标记串与环境变量](../../reference/hil-markers.md)。

你现在已经能从仓库根目录构建并在 QEMU 里跑示例、读中断输出、用退出码做自检了。
下一课我们走出模拟器，做第一次**真机**测试 ——
[第一次硬件在环测试](03-hil.md)。
