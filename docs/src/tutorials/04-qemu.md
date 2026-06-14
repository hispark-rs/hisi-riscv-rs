# 在 QEMU 里运行与调试

QEMU 是不用硬件就能跑 WS63 固件的"软件在环"环境。本课我们用它跑几个示例，
看中断的串口输出，再用半主机（semihosting）拿到一个真正的退出码。每一步都有可见结果。

> QEMU 模型的原理见 [QEMU 模型](../explanation/qemu-model.md)。

我们用到的命令模板始终是这一条：

```bash
qemu-system-riscv32 -M ws63 -nographic -bios none -kernel <elf>
```

- `-M ws63`：WS63 机器模型。
- `-nographic`：无图形界面，把 UART0 接到当前终端。
- `-bios none`：不加载额外固件，直接跑我们的 `-kernel`。
- `-kernel <elf>`：要运行的示例 ELF。

退出 QEMU 始终是：按 `Ctrl-A`，再按 `X`。

## 第 1 步：跑定时器中断示例（timer_irq）

先编译再运行：

```bash
cargo build -p timer_irq --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/timer_irq
```

`TIMER_0` 周期性触发 IRQ 26，处理函数每次累加计数并打印。你应当看到：

```
WS63 timer-IRQ test (TIMER_0 -> IRQ 26)
timer irq #0
timer irq #1
timer irq #2
...
OK: timer interrupts delivered
```

看到 `timer irq #` 不断递增、最后出现 `OK: timer interrupts delivered`，
说明 QEMU 的中断投递闭环正常。按 `Ctrl-A` `X` 退出。

## 第 2 步：跑 GPIO 中断示例（gpio_irq）

```bash
cargo build -p gpio_irq --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/gpio_irq
```

这个示例把 GPIO0 的边沿映射到一个自定义本地 IRQ（≥32）。你应当看到：

```
WS63 GPIO-IRQ test (GPIO0 pin0 -> IRQ 33, custom local)
gpio irq #0
gpio irq #1
...
OK: custom local IRQ (>=32) delivered
```

退出同上。

## 第 3 步：用半主机拿到退出码（semihost_selftest）

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

```
semihost_selftest: PASS
```

随后 QEMU 自行退出。检查它的退出码：

```bash
echo $?
```

你应当看到：

```
0
```

`0` 就代表自检通过——脚本可以直接据此判定成败，无需解析串口文本。

> 各示例的预期标记串汇总见 [示例目录与验证标记串](../reference/examples.md)；
> 半主机相关的环境变量见 [HIL 标记串与环境变量](../reference/hil-markers.md)。

你现在已经能用 QEMU 跑示例、读中断输出、用退出码做自检了。
最后一课我们走出模拟器，做第一次**真机**测试 ——
[第一次硬件在环测试](05-first-hil.md)。
