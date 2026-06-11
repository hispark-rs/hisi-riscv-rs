# HIL — 真机在环（hardware-in-the-loop）bring-up

ws63-qemu 已把固件「跑得足够真」做软件在环验证；这一层是它的**真机对应**——板子到位后用它把 ROADMAP
阶段 1（HIL bring-up）跑通，验证 QEMU 证明不了的部分（**真实时钟/时序、真实外设**）。

烧录工具用 [`hisiflash`](https://github.com/hispark-rs/hisiflash)（本机在 `/root/hisiflash`）；QEMU 端调试见
`ws63-qemu/scripts/debug.sh`。
> **多芯片支持**：本文针对 WS63 HIL。BS21/BS2X（BLE/SLE，无 Wi-Fi）也有 QEMU 镜像 + 链接脚本，但真机支持仍待验证（见 `chips/bs2x/guide`）。

## 准备（板子相关，首板时确认）

```bash
cargo install hisiflash-cli            # 或 cargo install --path /root/hisiflash/hisiflash-cli
sudo apt-get install -y gdb-multiarch  # 真机/QEMU 调试（rust-gdb 驱动它）
```

两个**必须按板子确认**的量（写错可能烧不进 / 烧错位置）：

- **`LOADERBOOT`** —— 厂商 LoaderBoot 二进制，`hisiflash write-program` 会先把它推进去再写程序。
  取自 fbb_ws63 构建产物（`src/output/ws63/.../*loaderboot*.bin`）。
- **`ADDRESS`** —— 程序写入的 flash 偏移。**对照板子的分区表确认**（典型 app 分区偏移；内存映射里
  NOR_FLASH/XIP 起始 `0x0020_0000`，但 write 的是 SFC flash 偏移，二者不一定相同）。

## 用法

```bash
# 单个固件（按例子名自动找 .bin，没有就用 rust-objcopy 从 ELF 生成）
PORT=/dev/ttyUSB0 LOADERBOOT=/path/loaderboot.bin ADDRESS=0x200000 \
    hil/flash.sh blinky

# 全套 HIL 冒烟（逐例烧录 + 读 UART + 比对标记，镜像 QEMU smoke-test）
PORT=/dev/ttyUSB0 LOADERBOOT=/path/loaderboot.bin ADDRESS=0x200000 \
    hil/hil-smoke.sh
```

环境变量：`PORT`（串口）、`BAUD`（烧录波特，hisiflash 默认 921600）、`UART_BAUD`（例子 UART0 波特，默认
115200）、`LOADERBOOT`、`ADDRESS`、`HISIFLASH`（二进制名）、`SETTLE`（每次烧录后读 UART 秒数）。

## Bring-up 清单（按序，每步附预期 + 失败诊断）

| 步 | 验证 | 预期 | 失败诊断 |
|----|------|------|----------|
| 1 | **上电 + flashboot** | 串口有 flashboot/loaderboot 输出 | 检查电源、PWR_ON、串口线/波特、LOADERBOOT |
| 2 | **blinky** | LED 闪（GPIO0）/ 逻辑分析仪见方波 | GPIO 引脚映射、init_output、时钟门控 |
| 3 | **uart_hello** | `Hello from WS63 …` @115200 | **验证 160 MHz 波特基**——波特不对说明 UART 时钟假设错（见 ch8 时钟树） |
| 4 | **timer_irq** | `timer irq #…` 周期到达 | **验证 24 MHz TCXO 定时器时钟**——周期偏 10× 说明时钟仍按 240 MHz 算 |
| 5 | **gpio_irq** | `gpio irq #…`（按键/注入） | 中断接线、LOCI* 使能、触发沿 |
| 6 | **reset_demo** | 复位 + `reset_reason=Software` | GLB_CTL_M(0x4000_2110) / SYS_RST_RECORD_0 |
| 7 | **SPI / I2C** | `spi_loopback`（短接 MOSI-MISO）/ `i2c_scan` | SPI 两级时钟、I2C 24 MHz SCL |
| 8 | **DMA（可选）** | `dma_loopback` | DMA 握手 ID、外设/mem 转移（QEMU 已验证，通用拓展用；首板非门禁） |
| 9 | **连接性（阶段 4/5）** | blob 链接镜像跑通 FRW/HCC → netif | ROM 地址 + 厂商重定位（HIL 专属，QEMU 无法）；仅 WS63 支持（BS2X BLE/SLE 见可行性分析） |

**首板第一目标**：跑通步 3–6 确认**本会话的时钟修复在真硅片上准确**（24 MHz 定时器、160 MHz UART 波特、
SPI/I2C、GPIO/复位中断）——这正是 QEMU 数字验证不了、必须上板验的部分（QEMU 已验证投递逻辑，但无真时钟/时序）。一旦通过，步 7 及阶段 4/5（连接性上板）即可推进。

## 真机调试（JTAG/SWD / 串口）

**QEMU**：用 `ws63-qemu/scripts/debug.sh`（gdbstub）——已验证投递闭环但无真时钟/时序。

**真硅片（WS63）**：探针（J-Link/OpenOCD）+ `gdb-multiarch`，或用 HiSilicon 定制 probe-rs：
- 标准 OpenOCD + gdb：

```bash
# 例：OpenOCD 起 gdbstub 后
RUST_GDB=gdb-multiarch rustup run ws63 rust-gdb \
    -ex 'target remote :3333' \
    target/riscv32imfc-unknown-none-elf/release/blinky
```

- **HiSilicon 定制（RISC-V-DM 后端，正在验证）**：fork [`hispark-rs/probe-rs`](https://github.com/hispark-rs/probe-rs/tree/add-hisilicon-ws63-bs21)（branch `add-hisilicon-ws63-bs21`） 已加 DM-behind-CoreSight mem-AP 适配 + flash-algorithm；链接稳定后可用 `probe-rs run --chip ws63`。

`rust-gdb` 会自动加载 ws63 工具链的 Rust 美化打印器；JTAG/SWD 引脚见 ws63-guide ch7。

> 状态：本目录是**脚手架**——脚本结构与标记对齐 QEMU 冒烟，`LOADERBOOT`/`ADDRESS`/串口监控的确切参数在
> 首板 bring-up 时填实。无板时不可运行。

## 参考

- **ROADMAP**：见 [`ROADMAP.md`](../ROADMAP.md) 阶段 1–2 bring-up 规划 + QEMU 验收标准。
- **QEMU 验证范围**：时钟树改正、IRQ 投递、DMA 握手已在 ws63-qemu 上验证；真机需验证时序精度、真实外设行为、RF blob 链接。
- **连接性**：WS63 Wi-Fi porting 见 `chips/ws63/rf`；BS2X BLE/SLE 可行性分析见 `chips/bs2x/guide/README.md`。
