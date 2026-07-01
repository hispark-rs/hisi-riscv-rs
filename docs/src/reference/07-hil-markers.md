# HIL 标记串与环境变量

HIL（hardware-in-the-loop）框架的标记串与环境变量参考。事实取自 `hil/hil-smoke.sh`、`hil/flash.sh`、`hil/pack.sh`、`hil/cargo-run-hw.sh`。

HIL 框架原理见 [HIL 测试框架](../explanation/07-hil-framework.md)；运行步骤见 [运行 HIL 冒烟测试](../how-to/07-run-hil-tests.md)。

## 串口约定

| 串口 | 用途 | 参数 |
|------|------|------|
| UART0 = `/dev/ttyUSB0` | 板子 UART0（示例输出） | 115200 8N1 |
| `ttyACM0` | J-Link VCOM | — |

## `hil-smoke.sh` 检查的标记串

`hil-smoke.sh` 逐例烧录后读 UART，用 `grep -qE` 匹配下列模式（`check <example> <egrep> <desc>`）：

| 示例 | 匹配的 egrep 模式 | 描述 |
|------|-------------------|------|
| `uart_hello` | `Hello from WS63` | UART banner（验证 160 MHz 波特基） |
| `timer_irq` | `timer irq #|OK: timer` | Timer IRQ 投递（验证 24 MHz TCXO 定时器时钟） |
| `gpio_irq` | `gpio irq #` | GPIO IRQ 投递 |
| `reset_demo` | `OK: software reset observed` | software_reset + reset_reason（第二次启动标记） |
| `spi_loopback` | `SPI loopback OK` | 阻塞 SPI0（先短接 MOSI↔MISO！） |
| `i2c_scan` | `scan done|no devices` | I2C0 扫描 |

`blinky`（GPIO 翻转无 UART，需 LED/逻辑分析仪）与 `semihost_selftest`（需 debugger 半主机）在裸 HIL 跳过。总结果：全过打印 `HIL SMOKE: PASS`，否则 `HIL SMOKE: FAIL` 并 `exit 1`。

## 各示例标记串（HIL 期望）

| 示例 | 成功标记串 |
|------|-----------|
| `uart_hello` | `Hello from WS63 on QEMU!` |
| `timer_irq` | `OK: timer interrupts delivered`（或周期性 `timer irq #N`） |
| `gpio_irq` | `OK: custom local IRQ (>=32) delivered`（或 `gpio irq #N`） |
| `reset_demo` | `OK: software reset observed` |
| `spi_loopback` | `SPI loopback OK` |
| `i2c_scan` | `scan done` / `no devices acked` |
| `blinky` | 无（GPIO0 翻转） |
| `semihost_selftest` | 半主机退出码 0 / console `semihost_selftest: PASS`（裸 HIL 跳过） |

完整 18 例标记串见 [示例目录与验证标记串](02-examples.md)。

## 环境变量

### `flash.sh`

烧录方式选 `METHOD=`（默认 `probe-rs`）。

| 变量 | 默认 | 适用 | 说明 |
|------|------|------|------|
| `METHOD` | `probe-rs` | — | `probe-rs`（验证主路径）或 `hisiflash`（厂商路径） |
| `CHIP_KIND` | `ws63` | 共享 | `ws63`\|`bs21`，决定默认 app 分区地址 |
| `WS63_RS` | 脚本父目录 | 共享 | ws63-rs 检出根 |
| `CHIP` | `WS63` | probe-rs | probe-rs `--chip` 目标 |
| `PROBE_RS_YAML` | （必填） | probe-rs | fork 的芯片描述 YAML（`HiSilicon_WS63.yaml`） |
| `BASE_ADDRESS` | `0x00230000`（ws63）/ `0x00090000`（bs21） | probe-rs | app 分区 flash 地址 |
| `PROBE_RS` | `probe-rs` | probe-rs | probe-rs 二进制名 |
| `PORT` | （自动探测） | hisiflash | 串口（导出为 `HISIFLASH_PORT`） |
| `BAUD` | hisiflash 默认 921600 | hisiflash | 烧录波特（`HISIFLASH_BAUD`） |
| `LOADERBOOT` | （必填） | hisiflash | 厂商 LoaderBoot 二进制（取自 fbb_ws63 产物） |
| `ADDRESS` | （必填） | hisiflash | 程序写入 flash 偏移（对照分区表确认） |
| `HISIFLASH` | `hisiflash` | hisiflash | hisiflash 二进制名 |

### `hil-smoke.sh`（在 `flash.sh` 变量之外另加）

| 变量 | 默认 | 说明 |
|------|------|------|
| `PORT` | （必填） | 板子 UART0（`/dev/ttyUSBx`） |
| `SETTLE` | `4` | 每次烧录后读 UART 的秒数 |
| `UART_BAUD` | `115200` | 示例 UART0 波特（8N1） |
| `MONITOR` | （raw read `$PORT`） | 打印原始 UART 到 stdout 的命令（覆盖适配器读法） |
| `HISIFLASH` | `hisiflash` | hisiflash 二进制名 |

### `pack.sh`

| 变量 | 默认 | 说明 |
|------|------|------|
| `CHIP` | `ws63` | 目标芯片（`ws63`\|`bs21`），决定 app 分区地址 |
| `APP_ADDR` | （未设） | 覆盖 app 分区 flash 地址（如 `0x230000`） |
| `FWPKG` | （未设） | 非空则同时产出 `.fwpkg`（厂商 hisiflash 路径） |
| `HISI_FWPKG` | `hisi-fwpkg` | hisi-fwpkg 二进制名 |
| `WS63_RS` | 脚本父目录 | ws63-rs 检出根 |

默认 app 分区地址：ws63 `0x00230000`、bs21 `0x00090000`。

### `cargo-run-hw.sh`（cargo runner）

cargo 以 `<runner> <built-elf>` 调用，脚本把 ELF 打包成 0x300-header 镜像、probe-rs download、复位、（设了 `PORT` 则）流式 UART0。

| 变量 | 默认 | 说明 |
|------|------|------|
| `APP_ADDR` | `0x00230000`（ws63） | app 分区 flash 地址 |
| `PROBE_RS` | `probe-rs` | probe-rs 二进制名 |
| `PROBE_CHIP` | `WS63` | probe-rs `--chip` 值 |
| `PROBE_YAML` | （空 = 内置 DB） | `--chip-description-path` YAML |
| `HISI_FWPKG` | `hisi-fwpkg` | hisi-fwpkg 二进制名 |
| `PORT` | （无 = 不流式） | 复位后流式 UART0 的端口 |
| `UART_BAUD` | `115200` | 流式 UART 波特 |
| `MONITOR` | `10` | 流式 UART 秒数 |

> 启用：`CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/cargo-run-hw.sh cargo run -p blinky --release`（或 `just run-hw`）。
