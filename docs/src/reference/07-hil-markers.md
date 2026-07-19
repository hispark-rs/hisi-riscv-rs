# HIL 脚本与 runner 环境变量

HIL（hardware-in-the-loop）脚本和 cargo runner 的环境变量参考。事实取自
`hil/embedded-test-runner.sh`、`hil/hil-smoke.sh`、`hil/flash.sh`、`hil/pack.sh`、`hil/cargo-run-hw.sh`
以及 CI/agent wrapper `.agents/skills/hil-smoke/hil.sh`。
完整示例清单、成功标记串和失败标记串的唯一事实源是 [示例目录与验证标记串](02-examples.md)。

HIL 框架原理见 [HIL 测试框架](../explanation/07-hil-framework.md)；运行步骤见
[运行 HIL 测试](../how-to/07-run-hil-tests.md)。

## HIL 入口速查

| 入口 | 用途 | 观测通道 |
|------|------|----------|
| `hil/embedded-test-runner.sh` | `hisi-hal --test hil` 与 `tests-hil` 的 on-target test runner | `probe-rs run` + RISC-V semihosting，libtest 兼容输出 |
| `hil/hil-smoke.sh` | WS63 示例级 UART smoke | UART0 grep 标记串 |
| `hil/ws63-connectivity-smoke.sh` | WS63 A4/W2 connectivity gate | upstream profile: plain Cargo RF link；vendor oracle: guarded link；之后均为 planned-bin download + J-Link nRST + UART markers |
| `.agents/skills/hil-smoke/hil.sh` | CI/agent wrapper：preflight、chip 封装；WS63 全套委托 `hil/hil-smoke.sh` | UART0 grep 标记串 |
| `hil/flash.sh` | 示例/固件烧录封装 | hisi-fwpkg plan + probe-rs bin download，或 hisiflash |
| `hil/cargo-run-hw.sh` | 把单次 `cargo run` 改成烧真机 | hisi-fwpkg plan + probe-rs bin download，可选 UART stream |

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
| `PROBE_RS` | `probe-rs` | probe-rs 二进制；需要 `hispark-rs/probe-rs` 的 `add-hisilicon-ws63-bs21-hil-baseline` 分支 |
| `PROBE_CHIP` | `WS63` | probe-rs `--chip` 值 |
| `PROBE_YAML` | 空 | `--chip-description-path` YAML；需要显式指定时填 `HiSilicon_WS63.yaml` |
| `PROBE_SPEED` | `2000` | 调试传输时钟，单位 kHz |
| `HISI_FWPKG` | `hisi-fwpkg` | `hisi-fwpkg` 二进制名，用于 `patch-hash` |

典型启用方式：

```bash
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
cargo test -Zbuild-std=core,alloc -p hisi-hal --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf --test hil
```

## `hil-smoke.sh` 当前检查的 grep 模式

`hil-smoke.sh` 逐例烧录后读 UART，用 `grep -qE` 匹配下列模式（`check <example> <egrep> <desc>`）。
这是脚本行为参考，不是完整示例标记串清单；完整清单见 [示例目录与验证标记串](02-examples.md)。

| 示例 | 匹配的 egrep 模式 | 描述 |
|------|-------------------|------|
| `uart_hello` | `Hello from WS63` | 已在 WS63 真机通过；验证 HAL UART boot-clock 配置与 115200 8N1 输出 |
| `timer_irq` | `timer irq #|OK: timer` | Timer IRQ 投递（验证 24 MHz TCXO 定时器时钟） |
| `gpio_irq` | `gpio irq #` | GPIO IRQ 投递 |
| `reset_demo` | `OK: software reset observed` | software_reset + reset_reason（第二次启动标记） |
| `spi_loopback` | `SPI loopback OK` | 阻塞 SPI0（先短接 MOSI<->MISO） |
| `i2c_scan` | `scan done|no devices` | I2C0 扫描 |

`blinky`（GPIO 翻转无 UART，需 LED/逻辑分析仪）与 `semihost_selftest`（需 debugger 半主机）在裸 HIL 跳过。
总结果：全过打印 `HIL SMOKE: PASS`，否则 `HIL SMOKE: FAIL` 并 `exit 1`。

## A4 connectivity marker contract

`hil/ws63-connectivity-smoke.sh` 是连接性 marker 和 HIL 流程的可执行事实源。它与普通
外设 smoke 分开，因为需要受控 Personal-mode AP、secret 和更长的 UART 窗口。
`upstream-wpa2`/`upstream-wpa3` 直接使用 Cargo-delivered normalized archives，执行普通
`cargo build --release`、final-ELF gate、`hisi-fwpkg plan`、probe-rs bin download 和
J-Link nRST；不调用外部 RISC-V GCC 或 post-link patch。`vendor-wpa2` 仍从公开
`ws63-RF` release 下载固定 archive（或接受 `WS63_WPA_ARCHIVE`），校验 SHA-256 后走
guarded link，但该分支只作为迁移 oracle。

成功必须同时出现：`RF2_INIT_OK ifname=hisi-rf`、initialized/scan-completed/connected
三个 `A4_RADIO_EVENT`、`RF5B_WPA_CONNECT_OK`、`RF5A_DHCP_OK`、smoltcp neighbor-cache
`RF5A_ARP_OK`、公共 ICMP 非零回复、零 RX queue drop、`A4_NET_RUNNER_STEADY` 和
`A4_DHCP_RENEW_OK`。脚本打印 `WS63 CONNECTIVITY SMOKE: PASS` 才算通过。

| 变量 | 默认 | 说明 |
|------|------|------|
| `PORT` | 必填 | WS63 UART0 |
| `WS63_WIFI_PASSPHRASE` | 必填 | 仅由 self-hosted secret 注入，不写入仓库或日志 |
| `WS63_CONNECTIVITY_PROFILE` | `vendor-wpa2` | `vendor-wpa2` / `upstream-wpa2` / `upstream-wpa3`；正式 upstream 验证使用 plain Cargo lane |
| `WS63_WIFI_AP_MODE` | 空 | `upstream-wpa3` 必须显式为 `transition` 或 `pure-wpa3` |
| `WS63_WPA_ARCHIVE` | 公开 release asset | 可覆盖为 runner 本地缓存路径；内容仍须匹配固定 hash |
| `PROBE_SPEED` | `1000` | 已验证的 WS63 download 速率；2 MHz page-program timeout 不作为固件失败 |
| `MONITOR` | `60` | 覆盖 connect、ping 与 smoke-only 20 秒 DHCP lease renew 的 UART 窗口 |

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
| `BASE_ADDRESS` | 未设 | probe-rs | 可选 app 分区覆盖值；未设则读取 `hisi-fwpkg plan` 的 `base_addr` |
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

默认 app 分区地址由 `hisi-fwpkg plan` 决定：ws63 `0x00230000`、bs21 `0x00090000`。

### `cargo-run-hw.sh`（cargo runner）

Cargo 以 `<runner> <built-elf>` 调用，脚本执行 `hisi-fwpkg plan --image-output`，再用 probe-rs
`download --binary-format bin --base-address <plan.base_addr>` 写入；设了 `PORT` 时会流式读取 UART0。

| 变量 | 默认 | 说明 |
|------|------|------|
| `PROBE_RS` | `probe-rs` | probe-rs 二进制名 |
| `PROBE_CHIP` | `WS63` | probe-rs `--chip` 值 |
| `PROBE_YAML` | 空 | `--chip-description-path` YAML |
| `PROBE_SPEED` | `2000` | 调试传输时钟，单位 kHz |
| `HISI_FWPKG` | `hisi-fwpkg` | hisi-fwpkg 二进制名 |
| `PORT` | 无 | 复位后流式 UART0 的端口 |
| `UART_BAUD` | `115200` | 流式 UART 波特 |
| `MONITOR` | `10` | 流式 UART 秒数 |

> 启用：`CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/cargo-run-hw.sh cargo run -p blinky --release`
>（或 `just run-hw`）。
