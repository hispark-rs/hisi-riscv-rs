# 如何用 hisiflash 烧录到真机

这是**厂商串口 / YMODEM 路径**：把程序打成 `.fwpkg`，用 `hisiflash` 经串口烧进去。它不需要 SWD/JTAG 探针，只要一根 UART 线——适合**手上没有补丁版 probe-rs 探针、或就想用厂商工具**的场景。

> 想用探针的验证主路径请看[如何用 probe-rs 烧录](04-flash-probe-rs.md)。两者怎么选见文末。

## 前提

- `hisiflash`：`cargo install hisiflash-cli`（或自行构建 hisiflash 仓库）。
- **`LOADERBOOT`**：厂商 LoaderBoot 二进制。`hisiflash` 会先把它推进 SRAM，再让它接管 flash 写入。取自 fbb_ws63 构建产物（`src/output/ws63/.../*loaderboot*.bin`）。**必填。**
- **`ADDRESS`**：程序写入的 flash 偏移（典型 app 分区偏移 `0x230000`）。**对照板子的分区表确认**——写错可能烧不进或烧错位置。

## 两步走

### 1. 打成 .fwpkg

```bash
hisi-fwpkg pack -o blinky.fwpkg --chip ws63 \
    target/riscv32imfc-unknown-none-elf/release/blinky
# 或用脚本：
FWPKG=1 hil/pack.sh blinky          # -> examples/ws63/target/.../blinky.fwpkg
```

`.fwpkg` 是单分区容器（V1 + CRC），内含已带 0x300 头的 app 镜像（见[如何打包镜像](03-package-image.md)）。

### 2. 用 hisiflash 烧

`hisiflash` 直接吃 `.fwpkg`：

```bash
hisiflash flash blinky.fwpkg
```

或者走 `hil/flash.sh` 的 `METHOD=hisiflash` 分支（它写程序而非 fwpkg，先推 LoaderBoot 再 `write-program`）：

```bash
METHOD=hisiflash PORT=/dev/ttyUSB0 \
    LOADERBOOT=/path/loaderboot.bin ADDRESS=0x230000 \
    hil/flash.sh blinky
```

环境变量（hisiflash 路径）：

| 变量 | 含义 | 默认 |
| --- | --- | --- |
| `PORT` | 串口（导出为 `HISIFLASH_PORT`） | 自动探测 |
| `BAUD` | 烧录波特率（`HISIFLASH_BAUD`） | hisiflash 默认 `921600` |
| `LOADERBOOT` | 厂商 LoaderBoot bin（**必填**） | — |
| `ADDRESS` | 程序写入偏移（**必填**） | — |
| `HISIFLASH` | hisiflash 二进制 | `hisiflash` |

> 波特率注意：fwpkg/YMODEM 流程常见 **230400**，更稳的可降到 **115200**；`hisiflash` 本身的 `write-program` 默认 921600。波特率太高在差线材上易丢包，烧不进就降速重试。

## 何时用 hisiflash vs probe-rs

| | probe-rs（验证主路径） | hisiflash（厂商路径） |
| --- | --- | --- |
| 接线 | SWD/JTAG 探针 | 一根 UART |
| 依赖 | 补丁版 probe-rs fork + yaml | 厂商 LoaderBoot + hisiflash |
| 调试 | 能 attach、读内存、下断点 | 仅烧录 |
| 验证状态 | 真机验证 | 厂商成熟路径 |

**优先 probe-rs**（能顺带调试）；**没有探针、或只想用厂商成熟链路**时用 hisiflash。
