# Stable API 清单与门控状态

本页是 `hisi-riscv-hal` 0.6.0 当前默认公开面的**唯一事实源**。判定规则与门控机制见
[稳定 / 不稳定 API 门控政策](../explanation/policies/02-stable-unstable.md)；历史评审文件只记录当时状态，
不作为当前 API 清单。

**证据基线（2026-07-04）：** WS63 真机 `tests/hil.rs` embedded-test 套件 26/26 通过。下面的 STABLE 表只列默认
`features = ["chip-ws63", "rt"]` 面向用户公开的子集；凡需额外台架、opt-in feature、或 soundness 不变量尚未闭合的入口，
即使代码存在，也放在 UNSTABLE。

**增量证据（2026-07-12）：** `tests::i2c0_nack_is_reported_after_done` 在 WS63 真机通过，验证 v150
控制器按 `DONE` 完成后报告 `ACK_ERR`；当前新增用例尚待 M2 默认套件全量重跑后并入新的总数基线。

## STABLE（默认暴露）

| 稳定面 | HIL / 证据边界 | 不包含 |
|--------|----------------|--------|
| GPIO `Input` / `Output` / `Flex`、`GpioBank` | 真机 GPIO 输出/输入基本路径覆盖 | GPIO IRQ、等待边沿 |
| UART0/1 blocking、`BaudRate`、`UartClock`、`UartPort`、sealed `UartInstance` | 真机 UART0/1 blocking 配置与收发路径覆盖 | UART2、async I/O、DMA |
| SPI0 blocking、blocking-backed `async` `SpiBus` trait impl | 真机 SPI0 配置/传输路径覆盖；外部 loopback 台架仍按需单独跑 | SPI1、DMA、interrupt/waker async |
| WS63 I2C0 blocking、blocking-backed `async` `I2c` trait impl | 真机 I2C0 配置、7-bit 地址拒绝，以及 `DONE` 后 NACK 报告路径覆盖 | I2C1、需真实从机的业务事务承诺 |
| Timer raw configure/enable/current/interrupt paths、`TimerChannel` | 真机 raw timer 与中断路径覆盖 | one-shot/periodic wrapper、async delay |
| TCXO | 真机计数器读取与推进覆盖 | — |
| PWM Ch0 config/enable/disable、`PwmPeriod`、`Duty`、fallible duty writes | 真机确认 WS63 `pwm_freq_h` 高半不 latch，因此稳定面只承诺 Ch0 register-latch 事实 | Ch1..Ch7、真实波形 HIL、polarity/start/pulse-count、`into_running` |
| WDT typed configure/feed/counter/drop/`into_armed`/`leak` | 真机覆盖配置、喂狗、计数、drop 关停、`into_armed`/`leak` 保持 armed | WDT IRQ |
| TRNG default blocking read、byte-fill path | 真机基本出数与 byte-fill 覆盖 | manual clock/divider/status、non-blocking `read`、`fill_words` |
| eFuse `EfuseDriver`、`EfuseByteAddress`、`read_byte` | 真机只读单字节路径覆盖 | `set_clock_period`、`read_buffer`、`write_byte`、status |
| `System::reset_reason` | 真机 reset reason 读取覆盖 | `software_reset*` |
| IO_CONFIG GPIO/UART mux、`GpioPad` / `UartPad` / `MuxFunction` | 真机 mux 选择的最小路径覆盖 | SFC pad config |
| LSADC scan-config subset | 真机 scan 配置/liveness 覆盖 | analog/filter/calibration/data-path/async |
| TSENSOR basic conversion subset | 真机 basic conversion/liveness 覆盖 | mode/threshold/interrupt/auto-refresh/calibration/blocking-read/disable/clear-status |
| I2S master config/liveness subset | 真机构造与版本/liveness 覆盖 | slave role/config、data/FIFO/IRQ |
| interrupt identity/types、basic enable/disable/pending paths | 真机基础中断门控路径覆盖 | priority/threshold setter/getter 语义承诺 |
| clock metadata | 审计用 CKEN 映射数据；无外设状态副作用 | clock-gating RAII/控制面 |
| peripherals ownership tokens | 单例令牌与 `'d` 生命周期是 HAL 基础设施，`take()` 由 critical-section 保护 | raw `register_block()` escape hatch |
| `Duration` / `Rate`、prelude、macros、soc | 纯基础设施 / re-export，不直接承诺外设行为 | `prelude` 中的 unstable re-export |

`private` 是 crate-internal sealed-trait 模块，不是 public API。

## UNSTABLE（需 `features = ["unstable"]`）

- **DMA 整个公共模块**：`Dma0`/`Sdma0`、`DmaDriver`、typed channel tokens、mem-to-mem `Transfer`、`DmaTransferSize`/`DmaSyncMask`、`SpiDma`/`UartDma`、`PeripheralTransfer`、`DmaFrame`/`PeriDmaCtl`/`PeriKind` 以及所有 DMA async hook。原因不是只缺 HIL，还包括 cache-line alignment、timeout quiescence、async cancellation、SPI1/UART DMA 证据未闭合。
- **cache 与 raw PAC escape hatch**：`cache::{clean_range,invalidate_range,flush_range}` 是 by-range D-cache CSR 原语，和 DMA 的 cache-line ownership/alignment 不变量一起毕业；`register_block()` 这类直接返回 PAC `RegisterBlock` 的入口绕过 typed-config/driver gating，保持 `unstable` + `unsafe`。
- **interrupt/waker async**：`asynch::block_on`、`IrqSignal`、GPIO `Wait`、timer `AsyncDelay`、UART async I/O、LSADC async。SPI/I2C 的 blocking-backed async trait impl 是例外，随 `async` 暴露。
- **默认面先收窄的子面**：GPIO IRQ (`InterruptTrigger`、per-pin enable/clear/pending/trigger)、`Uart::new_uart2`、`Spi::new_spi1`、`I2c::new_i2c1`、timer one-shot/periodic wrappers、WDT IRQ 方法、PWM polarity/start/pulse-count/`into_running`、I2S slave role/config、eFuse status、TRNG `done/read/fill_words`、TSENSOR `disable/clear_status`。这些不是“永远不稳定”，只是当前没有足够真机证据或所有权约束。
- **不可逆 / 未闭合 soundness 的路径**：`EfuseDriver::set_clock_period`/`read_buffer`/`write_byte`（默认稳定面只保留自动 clock period + 单字节只读路径）、`System::software_reset*`、`Instant::now`/`elapsed`、`interrupt::free`、interrupt priority/threshold setter/getter、SFC pad config、I2S data/FIFO/IRQ 方法、LSADC analog/conversion/filter/calibration/data-path 方法、TSENSOR mode/threshold/interrupt/auto-refresh/calibration/blocking-read 方法、TRNG manual clock/divider/status knob。
- **embassy**：无端到端 HIL（`timer_int0_named_routing` 还专门 `not(feature="embassy")` 排除它）。
- **WS63 未测试驱动**：`clock_init`/`shared_memory`/`km`/`pke`/`safety`/`sfc`/`spacc`/`ulp_gpio`/`rtc`-WS63（`hil-rtc` 是 opt-in 且这块板没晶振从没跑过）/`delay`。`shared_memory` 当前只服务 RF bring-up，尚未作为通用 HAL 能力毕业。
- **整个 BS2X target**（无 BS2X 板）：`chip-bs21` 需要 `unstable`；这覆盖共享驱动、`gadc`/`keyscan`/`pdm`/`qdec`/`usb`/`i2c`-v151/`rtc`-v150/`trng`-v1 等 BS2X-only 模块；`hisi-riscv-rt` 的 BS2X compatibility adapter 也同样要求 `unstable`。
- **prelude 的 unstable re-export**：`Delay`、`InterruptTrigger`、`OneShotTimer`/`PeriodicTimer`、`Dma0`/`DmaDriver`/`Sdma0`、`RtcDriver`、`SfcDriver`、`UlpGpioPin`。
