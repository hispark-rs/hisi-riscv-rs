# 如何用硬件 runner 让 `cargo run` 烧真机

平时 `cargo run` 走的是 QEMU runner（在模拟器里跑）。本篇让 **`cargo run` 改成「编译 → 打包 → 烧进真机 → 复位 → 串口看输出」**，靠的是 cargo 的 per-target runner 机制 + `hil/cargo-run-hw.sh`。

> 这只影响你显式覆盖 runner 的那一次（或那个 shell）。**不覆盖时，普通 `cargo run` 仍然走 QEMU**，互不影响。

## 原理

cargo 调用 runner 的方式是 `<runner> <编译出的ELF路径> [args...]`。`hil/cargo-run-hw.sh` 接住 `$1` 这个 ELF，用 `hisi-fwpkg image` 包成 0x300 头镜像，用补丁版 probe-rs `download` 写进 app 分区，`reset` 复位，并（若设了 `PORT`）在复位前就开始抓 UART0 输出。

> 它依赖[补丁版 probe-rs fork](flash-probe-rs.md) 和 `hisi-fwpkg`——脚本启动时会检查这两个二进制在不在。

## 用法

用 per-target runner 环境变量覆盖（target 是 `riscv32imfc-unknown-none-elf`，转成大写下划线即环境变量名）：

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

脚本全部参数都有合理默认：

| 变量 | 含义 | 默认 |
| --- | --- | --- |
| `APP_ADDR` | app 分区 flash 地址 | `0x00230000`（WS63；BS2X 用 `0x00090000`） |
| `PROBE_RS` | probe-rs 二进制 | PATH 里的 `probe-rs` |
| `PROBE_CHIP` | `probe-rs --chip` 值 | `WS63` |
| `PROBE_YAML` | `--chip-description-path` yaml | 空 = 用内置数据库 |
| `HISI_FWPKG` | hisi-fwpkg 二进制 | PATH 里的 `hisi-fwpkg` |
| `PORT` | 复位后要抓的板子 UART0 | 空 = 不抓串口 |
| `UART_BAUD` | 抓串口的波特率 | `115200` |
| `MONITOR` | 抓串口的秒数 | `10` |

> 装的 probe-rs 内置库里若没有 WS63 描述，必须显式给 `PROBE_YAML=/path/HiSilicon_WS63.yaml`（fork 自带）。本地编译的 fork 用 `PROBE_RS=/home/.../probe-rs/target/debug/probe-rs`。

典型一条龙（指定 fork 二进制 + yaml + 抓串口 15 秒）：

```bash
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/cargo-run-hw.sh \
PROBE_RS=/home/me/probe-rs/target/debug/probe-rs \
PROBE_YAML=/home/me/probe-rs/targets/HiSilicon_WS63.yaml \
PORT=/dev/ttyUSB0 UART_BAUD=115200 MONITOR=15 \
    cargo run -p uart_hello --release
```

## 与模板 justfile 的对应

从模板生成的工程（见[如何从模板新建一个工程](new-project.md)）用 `just` 封装了同样的流程：

- `just flash` ≈ 这里的「打包 + download + reset」（`image` → `probe-rs download` → `probe-rs reset`）。
- 要让 `cargo run`/`just run-hw` 烧真机而非 QEMU，是同一套机制：模板的 `just run` 走 QEMU，烧真机用 `just flash`（或在工程里照本篇加一条 `run-hw` 配方，把 `CARGO_TARGET_..._RUNNER` 指向 `cargo-run-hw.sh`）。

`just flash` 的实现就是上面三步的等价命令，区别只是用 justfile 变量（`CHIP`/`CHIP_DESC`/`APP_ADDR`）代替环境变量。
