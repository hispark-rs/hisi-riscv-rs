# 示例目录与验证标记串

`examples/ws63/` 下 18 个示例。下表的成功标记串、失败标记串、是否需接线均直接取自各 `examples/ws63/<name>/src/main.rs`。所有 UART 输出走 **UART0 @ 115200 8N1**；`semihost_selftest` 走 RISC-V 半主机（semihosting），不走 UART。

如何构建/运行见 [构建一个示例](../how-to/build-example.md) 与 [在 QEMU 里运行](../tutorials/contrib/02-examples.md)。HIL 标记串汇总见 [HIL 标记串与环境变量](hil-markers.md)。

## 一览表

| 示例 | 用途 | 观测通道 | 成功标记串 | 需接线 | QEMU | 真机 |
|------|------|----------|-----------|:----:|:----:|:----:|
| `blinky` | GPIO0 1 Hz 闪灯（现代 `Output` 路径） | GPIO | 无（GPIO0 翻转，逻辑分析仪/LED 观测） | 否 | ✅ | ✅ (2026-06-14) |
| `uart_hello` | UART hello + tick 计数 | UART | `Hello from WS63 on QEMU!` | 否 | ✅ | ⚠️⁹ |
| `gpio_irq` | GPIO0 pin0 上升沿 → IRQ 33（自定义 local IRQ ≥32） | UART | `OK: custom local IRQ (>=32) delivered` | 否¹ | ✅ | ⚠️ |
| `i2c_scan` | I2C0 100 kHz 扫描 0x08..0x77 | UART | `scan done` / `no devices acked` | 否² | ✅ | ⚠️³ |
| `spi_loopback` | SPI0 Mode0 1 MHz 全双工自环 | UART | `SPI loopback OK` | 真机需短接 MOSI↔MISO | ✅ | ⚠️⁴ |
| `dma_loopback` | MDMA 外设环 + SDMA mem→mem | UART | `DMA LOOPBACK TEST: PASS` | 否 | ✅ | ⚠️ |
| `timer_irq` | TIMER_0 周期 → IRQ 26（local trap） | UART | `OK: timer interrupts delivered` | 否 | ✅ | ⚠️ |
| `async_bus` | `async` SpiBus + I2c + LSADC（`block_on`） | UART | `ASYNC BUS: PASS` | 否 | ✅ | ⚠️ |
| `async_delay` | `async` `DelayNs`（TIMER0 + `wfi`） | UART | `ASYNC DELAY: PASS` | 否 | ✅ | ⚠️ |
| `embassy_async_io` | embassy GPIO Wait + async UART + Timer | UART | `EMBASSY ASYNC IO: PASS` | 否¹ | ✅ | ⚠️ |
| `embassy_multitask` | embassy 双任务 `Timer::after` | UART | `EMBASSY MULTITASK: PASS` | 否 | ✅ | ⚠️ |
| `net_ping` | smoltcp over ws63-netmac + SLIRP（ARP/ICMP/UDP） | UART | `NET PING: PASS` | 否⁵ | ✅ | ❌⁶ |
| `reset_demo` | `software_reset` + `reset_reason` 端到端 | UART | `OK: software reset observed` | 否 | ✅ | ⚠️ |
| `rf_port_demo` | ws63-rf-rs porting 层 + Wi-Fi ROM-data blob 链接 | UART | `RF PORT DEMO: PASS` | 否⁷ | ✅ | ⚠️ |
| `semihost_selftest` | CPU 自检（M/F 扩展、mcycle），半主机退出码 | semihosting | 退出码 `0`，console `semihost_selftest: PASS` | 否⁸ | ✅ | ❌⁸ |
| `custom_memory` | 验证 per-example `memory.x` 覆盖 rt 自带 | UART | `custom_memory: OK (per-example memory.x in effect)` | 否 | ✅ | ⚠️ |
| `wifi_blob_link` | `--whole-archive` 链接 Wi-Fi ROM-data blob + 重定位证明 | UART | `BLOB LINK SPIKE: PASS` | 否⁷ | ✅ | ⚠️ |

图例（**真机**列）：✅ 已在真实硅片上验证通过；⚠️ QEMU 通过、真机尚未逐一验证（bring-up 进行中）；❌ 该观测通道真机不适用。截至 2026-06-14，只有 `blinky` 经硅片确认；其余 UART 类示例的真机标记串套件正在 bring-up（见 [HIL 测试框架](../explanation/hil-framework.md)）。

注：
1. `gpio_irq` / `embassy_async_io` 把 GPIO0 pin0 设为输出，依赖 ws63-qemu 建模的 输出→输入 自环产生边沿；真机需相应注入/接线。
2. `i2c_scan`：QEMU 下无真从机，`no devices acked` 是正常结果而非失败。
3. `i2c_scan` 真机需挂接真实 I2C 从机才会有 `found device`。
4. `spi_loopback`：QEMU 把 SPI0 TX FIFO 环回 RX，无需跳线；真机必须短接 MOSI↔MISO。
5. `net_ping` 需 QEMU user netdev（`-nic user`，默认），纯软件/SLIRP，无需外部网络。
6. `net_ping` 依赖 ws63-qemu 合成 MAC（`ws63-netmac @ 0x4421_0000`），真机无此通道。
7. `rf_port_demo` / `wifi_blob_link` 需厂商 blob `libwifi_rom_data.a`（ws63-RF 子模块）链接到位。
8. `semihost_selftest` 需 QEMU `-semihosting`；真机半主机陷阱为 no-op，`exit` 只自旋。
9. `uart_hello` 真机上已确认能跑到 `main` 并运行（probe-rs 单步/采样验证），但 UART banner 在 115200 下暂不可读 —— 疑似该例不做时钟初始化、波特率基于 QEMU 默认时钟假设，真机 UART 时钟不同。属已知 bring-up 待修项。

## 成功标记串（逐字，用于 grep）

| 示例 | 成功标记串（verbatim） |
|------|------------------------|
| `uart_hello` | `Hello from WS63 on QEMU!` |
| `gpio_irq` | `OK: custom local IRQ (>=32) delivered` |
| `i2c_scan` | `scan done`（有从机时）或 `no devices acked` |
| `spi_loopback` | `SPI loopback OK` |
| `dma_loopback` | `DMA LOOPBACK TEST: PASS` |
| `timer_irq` | `OK: timer interrupts delivered` |
| `async_bus` | `ASYNC BUS: PASS` |
| `async_delay` | `ASYNC DELAY: PASS` |
| `embassy_async_io` | `EMBASSY ASYNC IO: PASS` |
| `embassy_multitask` | `EMBASSY MULTITASK: PASS` |
| `net_ping` | `NET PING: PASS` |
| `reset_demo` | `OK: software reset observed` |
| `rf_port_demo` | `RF PORT DEMO: PASS` |
| `semihost_selftest` | console `semihost_selftest: PASS`（半主机退出码 0） |
| `custom_memory` | `custom_memory: OK (per-example memory.x in effect)` |
| `wifi_blob_link` | `BLOB LINK SPIKE: PASS` |

> `blinky` 无 UART 输出，只能由 GPIO0 翻转观测。

## 失败标记串

| 示例 | 失败/诊断标记串 |
|------|-----------------|
| `spi_loopback` | `SPI loopback MISMATCH`（rx≠tx）；`SPI error (timeout)` |
| `dma_loopback` | 各阶段 ` FAIL`；mismatch 诊断 `  mismatch @<idx> got=<x> want=<y>`；末行 `DMA LOOPBACK TEST: FAIL` |
| `async_bus` | `ASYNC BUS: FAIL`；SPI `MISMATCH`/`spi error`；ADC `no sample`（I2C `err` 不计失败） |
| `net_ping` | `NET PING: FAIL (no echo reply)`（5000 ms 超时） |
| `rf_port_demo` | `RF PORT DEMO: FAIL`；`memcpy_s/memset_s    : FAIL` |
| `semihost_selftest` | console `semihost_selftest: FAIL`（退出码 1）；`semihost_selftest: PANIC`（退出码 2） |
| `custom_memory` | `custom_memory: FAIL (unexpected memory.x)` |
| `wifi_blob_link` | `BLOB LINK SPIKE: FAIL`（验证少于 13/13） |

其余示例（`blinky`、`gpio_irq`、`timer_irq`、`reset_demo`、`async_delay`、`embassy_*`、`uart_hello`）无显式 FAIL 串；失败表现为成功标记串始终不出现。各 UART 示例的 `#[panic_handler]` 仅静默自旋（不输出），唯一例外是 `semihost_selftest`（写 `semihost_selftest: PANIC\n` 并 `exit(2)`）。

## semihost_selftest 退出码

| 退出码 | 含义 | console 输出 |
|:------:|------|--------------|
| `0` | PASS — 全部 CPU 不变量成立（乘法、硬浮点 ilp32f、mcycle 推进） | `semihost_selftest: PASS\n` |
| `1` | FAIL — 某 CPU 不变量检查失败 | `semihost_selftest: FAIL\n` |
| `2` | PANIC — 触达 Rust panic handler | `semihost_selftest: PANIC\n` |

机制：`exit(code)` 发 `SYS_EXIT_EXTENDED` (0x20) + `ADP_STOPPED_APPLICATION_EXIT` (0x2_0026) 块 `[reason, code]`，使 QEMU 进程退出码等于 `code`。console 写经 `SYS_WRITE0`，串末需 NUL（`\0`）。
