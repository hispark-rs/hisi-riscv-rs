# HIL 脚本与 runner 环境变量

HIL（hardware-in-the-loop）脚本和 cargo runner 的环境变量参考。事实取自
`hil/embedded-test-runner.sh`、`hil/hil-smoke.sh`、`hil/flash.sh`、`hil/pack.sh`、`hil/cargo-run-hw.sh`。
完整示例清单、成功标记串和失败标记串的唯一事实源是 [示例目录与验证标记串](02-examples.md)。

HIL 框架原理见 [HIL 测试框架](../explanation/07-hil-framework.md)；运行步骤见
[运行 HIL 测试](../how-to/07-run-hil-tests.md)。

## HIL 入口速查

| 入口 | 用途 | 观测通道 |
|------|------|----------|
| `hil/embedded-test-runner.sh` | `hisi-riscv-hal --test hil` 与 `tests-hil` 的 on-target test runner | `probe-rs run` + RISC-V semihosting，libtest 兼容输出 |
| `hil/hil-smoke.sh` | WS63 示例级 UART smoke | UART0 grep 标记串 |
| `hil/flash.sh` | 示例/固件烧录封装 | probe-rs download/reset 或 hisiflash |
| `hil/cargo-run-hw.sh` | 把单次 `cargo run` 改成烧真机 | probe-rs download/reset，可选 UART stream |

## 串口约定

| 串口 | 用途 | 参数 |
|------|------|------|
| UART0 = `/dev/ttyUSB0` | 示例输出 / `hil-smoke.sh` grep | 115200 8N1 |
| `ttyACM0` | J-Link VCOM | - |

## `embedded-test-runner.sh`

Cargo 以 `<runner> <built-test-elf> [embedded-test args...]` 调用 runner。脚本先原地执行
`hisi-fwpkg patch-hash <elf>`，再调用补丁版 `probe-rs run`；尾随参数会原样转发给 `embedded-test`
测试调度。

| 变量 | 默认 | 说明 |
|------|------|------|
| `PROBE_RS` | `probe-rs` | probe-rs 二进制；需要 `hispark-rs/probe-rs` 的 `add-hisilicon-ws63-bs21` 分支 |
| `PROBE_CHIP` | `WS63` | probe-rs `--chip` 值 |
| `PROBE_YAML` | 空 | `--chip-description-path` YAML；需要显式指定时填 `HiSilicon_WS63.yaml` |
| `HISI_FWPKG` | `hisi-fwpkg` | `hisi-fwpkg` 二进制名，用于 `patch-hash` |

典型启用方式：

```bash
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
cargo test -p hisi-riscv-hal --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf --test hil
```

## `hil-smoke.sh` 当前检查的 grep 模式

`hil-smoke.sh` 逐例烧录后读 UART，用 `grep -qE` 匹配下列模式（`check <example> <egrep> <desc>`）。
这是脚本行为参考，不是完整示例标记串清单；完整清单见 [示例目录与验证标记串](02-examples.md)。

| 示例 | 匹配的 egrep 模式 | 描述 |
|------|-------------------|------|
| `uart_hello` | `Hello from WS63` | UART banner（验证 160 MHz 波特基） |
| `timer_irq` | `timer irq #|OK: timer` | Timer IRQ 投递（验证 24 MHz TCXO 定时器时钟） |
| `gpio_irq` | `gpio irq #` | GPIO IRQ 投递 |
| `reset_demo` | `OK: software reset observed` | software_reset + reset_reason（第二次启动标记） |
| `spi_loopback` | `SPI loopback OK` | 阻塞 SPI0（先短接 MOSI<->MISO） |
| `i2c_scan` | `scan done|no devices` | I2C0 扫描 |

`blinky`（GPIO 翻转无 UART，需 LED/逻辑分析仪）与 `semihost_selftest`（需 debugger 半主机）在裸 HIL 跳过。
总结果：全过打印 `HIL SMOKE: PASS`，否则 `HIL SMOKE: FAIL` 并 `exit 1`。

## 环境变量

### `flash.sh`

烧录方式选 `METHOD=`（默认 `probe-rs`）。

| 变量 | 默认 | 适用 | 说明 |
|------|------|------|------|
| `METHOD` | `probe-rs` | - | `probe-rs`（验证主路径）或 `hisiflash`（厂商路径） |
| `CHIP_KIND` | `ws63` | 共享 | `ws63` / `bs21`，决定默认 app 分区地址 |
| `WS63_RS` | 脚本父目录 | 共享 | ws63-rs 检出根 |
| `CHIP` | `WS63` | probe-rs | probe-rs `--chip` 目标 |
| `PROBE_RS_YAML` | 必填 | probe-rs | fork 的芯片描述 YAML（`HiSilicon_WS63.yaml`） |
| `BASE_ADDRESS` | `0x00230000`（ws63）/ `0x00090000`（bs21） | probe-rs | route 1 `.img` 的 app 分区 flash 地址 |
| `PROBE_RS` | `probe-rs` | probe-rs | probe-rs 二进制名 |
| `PORT` | 自动探测 | hisiflash | 串口（导出为 `HISIFLASH_PORT`） |
| `BAUD` | hisiflash 默认 921600 | hisiflash | 烧录波特（`HISIFLASH_BAUD`） |
| `LOADERBOOT` | 必填 | hisiflash | 厂商 LoaderBoot 二进制（取自 fbb_ws63 产物） |
| `ADDRESS` | 必填 | hisiflash | 程序写入 flash 偏移（对照分区表确认；WS63 常见 `0x230000`） |
| `HISIFLASH` | `hisiflash` | hisiflash | hisiflash 二进制名 |

### `hil-smoke.sh`（在 `flash.sh` 变量之外另加）

| 变量 | 默认 | 说明 |
|------|------|------|
| `PORT` | 必填 | 板子 UART0（`/dev/ttyUSBx`） |
| `SETTLE` | `4` | 每次烧录后读 UART 的秒数 |
| `UART_BAUD` | `115200` | 示例 UART0 波特（8N1） |
| `MONITOR` | raw read `$PORT` | 打印原始 UART 到 stdout 的命令（覆盖适配器读法） |
| `HISIFLASH` | `hisiflash` | hisiflash 二进制名 |

### `pack.sh`

| 变量 | 默认 | 说明 |
|------|------|------|
| `CHIP` | `ws63` | 目标芯片（`ws63` / `bs21`），决定 app 分区地址 |
| `APP_ADDR` | 未设 | 覆盖 app 分区 flash 地址（如 `0x230000`） |
| `FWPKG` | 未设 | 非空则同时产出 `.fwpkg`（厂商 hisiflash 路径） |
| `HISI_FWPKG` | `hisi-fwpkg` | hisi-fwpkg 二进制名 |
| `WS63_RS` | 脚本父目录 | ws63-rs 检出根 |

默认 app 分区地址：ws63 `0x00230000`、bs21 `0x00090000`。

### `cargo-run-hw.sh`（cargo runner）

Cargo 以 `<runner> <built-elf>` 调用，脚本对 ELF 执行 `hisi-fwpkg patch-hash`，再用 probe-rs
download/reset；设了 `PORT` 时会流式读取 UART0。

| 变量 | 默认 | 说明 |
|------|------|------|
| `PROBE_RS` | `probe-rs` | probe-rs 二进制名 |
| `PROBE_CHIP` | `WS63` | probe-rs `--chip` 值 |
| `PROBE_YAML` | 空 | `--chip-description-path` YAML |
| `HISI_FWPKG` | `hisi-fwpkg` | hisi-fwpkg 二进制名 |
| `PORT` | 无 | 复位后流式 UART0 的端口 |
| `UART_BAUD` | `115200` | 流式 UART 波特 |
| `MONITOR` | `10` | 流式 UART 秒数 |

> 启用：`CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/cargo-run-hw.sh cargo run -p blinky --release`
>（或 `just run-hw`）。
