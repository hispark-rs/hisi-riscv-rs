# 如何运行 HIL 冒烟测试

`hil/hil-smoke.sh` 把每个示例逐个烧到真机、读串口、断言它打印了预期的标记串。它验证 QEMU 证明不了的部分——**真实时钟/时序、真实外设**（尤其是修正后的 24 MHz TCXO 定时器和 160 MHz UART 波特基准）。HIL 框架背景见[HIL 测试框架](../explanation/components/00-index.md)，全部标记串见[HIL 标记串与环境变量](../reference/07-hil-markers.md)。

> 前提：板子接好、UART0 接到 host、烧录环境就绪（见[用 probe-rs 烧录](04-flash-probe-rs.md)，hil-smoke 默认通过 `hil/flash.sh` 烧录，即默认 `METHOD=probe-rs`）。

## 运行

```bash
PORT=/dev/ttyUSB0 PROBE_RS_YAML=/path/HiSilicon_WS63.yaml hil/hil-smoke.sh
```

走厂商烧录路径时把 flash.sh 那套环境变量带上：

```bash
METHOD=hisiflash PORT=/dev/ttyUSB0 \
    LOADERBOOT=/path/loaderboot.bin ADDRESS=0x230000 \
    hil/hil-smoke.sh
```

环境变量（与 `hil/flash.sh` 同：`PORT`/`BAUD`/`LOADERBOOT`/`ADDRESS`/`HISIFLASH`/`PROBE_RS_YAML`…），外加：

| 变量 | 含义 | 默认 |
| --- | --- | --- |
| `PORT` | 板子 UART0（**必填**） | — |
| `UART_BAUD` | 示例的 UART0 波特率（8N1） | `115200` |
| `SETTLE` | 每次烧完读串口的秒数 | `4` |
| `MONITOR` | 自定义「打印原始 UART 到 stdout」的命令 | 直接 `cat $PORT` |

## 它检查哪些标记串

脚本逐示例烧录后，在 `SETTLE` 秒内 `grep -E` 串口输出找下面的模式（命中即 PASS）：

| 示例 | 期望标记串（egrep） | 验证什么 |
| --- | --- | --- |
| `uart_hello` | `Hello from WS63` | UART banner（验证 160 MHz 波特基准） |
| `timer_irq` | `timer irq #` 或 `OK: timer` | 定时器中断投递（验证 24 MHz TCXO 时钟） |
| `gpio_irq` | `gpio irq #` | GPIO 中断投递 |
| `reset_demo` | `reset_reason=Software` | 软复位 + 复位原因 |
| `spi_loopback` | `SPI loopback OK` | 阻塞 SPI0（**真机需先短接 MOSI↔MISO**） |
| `i2c_scan` | `scan done` 或 `no devices` | I2C0 总线扫描 |

两个示例不在自动断言里：`blinky`（GPIO0 翻转无 UART——用 LED / 逻辑分析仪看）、`semihost_selftest`（需要调试器的 semihosting——裸 HIL 跳过）。

## 读懂结果

- 每个 `check` 打印 `PASS: '<pat>' seen` 或 `FAIL`。FAIL 时会把串口最后几行 / flash 错误尾部打印出来帮你定位。
- 末行汇总 `HIL SMOKE: PASS`（退出码 0）或 `HIL SMOKE: FAIL`（退出码 = 失败数 / 非零）。
- 常见 FAIL 原因：
  - **flash failed**：烧录环境没配好（缺 yaml/LOADERBOOT/探针），看尾部错误。
  - **标记串没出现但板子像在跑**：`UART_BAUD` 不对（示例用 8N1，默认 115200），或 `SETTLE` 太短没等到输出——调大 `SETTLE`。
  - **`spi_loopback` FAIL**：真机上没短接 MOSI↔MISO（QEMU 会自环，真机不会）。

## 封装与 CI

- `.claude/skills/hil-smoke` 是这个脚本的 wrapper skill，给 agent 一键跑全套 HIL 冒烟。
- `.github/workflows/hil.yml` 在 **self-hosted runner**（接了真板子的机器）上跑同一脚本，把真机回归纳入 CI。
