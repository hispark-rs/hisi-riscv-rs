# 如何用硬件 runner 让 `cargo run` 烧真机

平时 `cargo run` 走 QEMU runner。本篇让 **`cargo run` 改成「编译 → 生成 plan image → probe-rs 烧裸 bin → 复位 → 串口看输出」**，靠的是 cargo 的 per-target runner 机制 + `hil/cargo-run-hw.sh`。

> 这只影响你显式覆盖 runner 的那一次（或那个 shell）。不覆盖时，普通 `cargo run` 仍然走 QEMU。

## 原理

cargo 调用 runner 的方式是 `<runner> <编译出的 ELF 路径> [args...]`。`hil/cargo-run-hw.sh` 接住 `$1` 这个 ELF 后：

1. 调 `hisi-fwpkg plan <elf> --chip ws63 --image-output <elf>.hisi.img`；
2. 从 plan JSON 读取 `base_addr`；
3. 执行 `probe-rs download --binary-format bin --base-address <base_addr> <image>`；
4. 触发硬件 nRST；
5. 如果设置了 `PORT`，提前打开 UART0 抓启动输出。

镜像 header、hash、body range、gap 填充和写入边界都来自 `hisi-fwpkg plan`。probe-rs 不理解 HiSilicon 镜像格式，只负责把完整 image 写到指定地址。

## 用法

用 per-target runner 环境变量覆盖：

```bash
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/cargo-run-hw.sh \
    cargo run -p blinky --release
```

要边烧边看串口，再加 `PORT`：

```bash
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/cargo-run-hw.sh \
PORT=/dev/ttyUSB0 \
    cargo run -p uart_hello --release
```

## 环境变量

| 变量 | 含义 | 默认 |
| --- | --- | --- |
| `PROBE_RS` | probe-rs 二进制 | PATH 里的 `probe-rs` |
| `PROBE_CHIP` | `probe-rs --chip` 值 | `WS63` |
| `PROBE_YAML` | `--chip-description-path` yaml | 空 = 用内置数据库 |
| `HISI_FWPKG` | hisi-fwpkg 二进制 | PATH 里的 `hisi-fwpkg` |
| `PORT` | 复位后要抓的板子 UART0 | 空 = 不抓串口 |
| `UART_BAUD` | 抓串口的波特率 | `115200` |
| `MONITOR` | 抓串口的秒数 | `10` |

典型一条龙：

```bash
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/cargo-run-hw.sh \
PROBE_RS=/home/me/probe-rs/target/debug/probe-rs \
PROBE_YAML=/home/me/probe-rs/targets/HiSilicon_WS63.yaml \
PORT=/dev/ttyUSB0 UART_BAUD=115200 MONITOR=15 \
    cargo run -p uart_hello --release
```

## 与 embedded-test 的区别

`cargo-run-hw.sh` 是 UART smoke/download runner，走 plan image。`hil/embedded-test-runner.sh` 暂时仍使用 `hisi-fwpkg patch-hash <elf>` + `probe-rs run <elf>`，因为 embedded-test 需要 ELF 中的测试元数据、符号和 semihosting 信息。

当前 probe-rs CLI 中 `run` 是 flash-and-run，`attach` 只 attach RTT/日志，不提供 embedded-test 的测试发现与调度；所以 embedded-test 先保留 ELF `run` 路径。如果未来 probe-rs 支持可靠拆分成 `download bin + run tests without reflashing`，再迁移到同一套 plan image 下载路径。
