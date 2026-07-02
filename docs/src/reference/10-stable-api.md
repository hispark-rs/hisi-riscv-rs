# Stable API 清单与门控状态

本页是 `hisi-riscv-hal` 0.6.0 当前默认公开面的事实清单。判定规则与机制见[稳定 / 不稳定 API 门控政策](../explanation/policies/02-stable-unstable.md)；这里不解释为什么，只列出默认 stable 面和需要 `unstable` feature 的面。

## STABLE（默认暴露）

- **WS63 默认稳定子集**：GPIO `Input`/`Output`/`Flex` + `GpioBank`（不含 GPIO IRQ 子面），SPI0 blocking + blocking-backed `async` `SpiBus`，UART0/1 blocking + `BaudRate`/`UartClock`/`UartPort`/sealed `UartInstance`，Timer raw configure/enable/current/interrupt paths + `TimerChannel`（不含 one-shot/periodic wrapper），TCXO，PWM Ch0 config/enable/disable + `PwmPeriod`/`Duty`/`PwmChannelId` + fallible duty writes，WDT typed configure/feed/counter/drop/`into_armed`（不含 WDT IRQ），TRNG default blocking read + byte-fill，eFuse `read_byte`，clock metadata，`System::reset_reason`，peripherals ownership tokens（raw register block 入口不稳定），WS63 I2C0 blocking + blocking-backed `async` I2c with 7-bit address rejection，I2S master config/liveness subset，IO_CONFIG GPIO/UART mux (`GpioPad`/`UartPad`/`MuxFunction`)，LSADC scan-config subset，TSENSOR basic conversion subset。
- **跨芯片 + 基础设施**：interrupt identity/types (`Interrupt`/`Priority`/`Threshold`) plus basic enable/disable/pending paths，peripherals，prelude，macros，soc，`Duration`/`Rate`。`private` 是 crate-internal sealed-trait 模块，不是 public API。

## UNSTABLE（需 `features = ["unstable"]`）

- **DMA 整个公共模块**：`Dma0`/`Sdma0`、`DmaDriver`、typed channel tokens、mem-to-mem `Transfer`、`DmaTransferSize`/`DmaSyncMask`、`SpiDma`/`UartDma`、`PeripheralTransfer`、`DmaFrame`/`PeriDmaCtl`/`PeriKind` 以及所有 DMA async hook。原因不是只缺 HIL，还包括 cache-line alignment、timeout quiescence、async cancellation、SPI1/UART DMA 证据未闭合。
- **cache 与 raw PAC escape hatch**：`cache::{clean_range,invalidate_range,flush_range}` 是 by-range D-cache CSR 原语，和 DMA 的 cache-line ownership/alignment 不变量一起毕业；`register_block()` 这类直接返回 PAC `RegisterBlock` 的入口绕过 typed-config/driver gating，保持 `unstable` + `unsafe`。
- **interrupt/waker async**：`asynch::block_on`、`IrqSignal`、GPIO `Wait`、timer `AsyncDelay`、UART async I/O、LSADC async。SPI/I2C 的 blocking-backed async trait impl 是例外，随 `async` 暴露。
- **默认面先收窄的子面**：GPIO IRQ (`InterruptTrigger`、per-pin enable/clear/pending/trigger)、`Uart::new_uart2`、`Spi::new_spi1`、`I2c::new_i2c1`、timer one-shot/periodic wrappers、WDT IRQ 方法、PWM polarity/start/pulse-count/`into_running`、I2S slave role/config、eFuse status、TRNG `done/read/fill_words`、TSENSOR `disable/clear_status`。这些不是“永远不稳定”，只是当前没有足够真机证据或所有权约束。
- **不可逆 / 未闭合 soundness 的路径**：`EfuseDriver::set_clock_period`/`read_buffer`/`write_byte`（默认稳定面只保留自动 clock period + 单字节只读路径）、`System::software_reset*`、`Instant::now`/`elapsed`、`interrupt::free`、interrupt priority/threshold setter/getter、SFC pad config、I2S data/FIFO/IRQ 方法、LSADC analog/conversion/filter/calibration/data-path 方法、TSENSOR mode/threshold/interrupt/auto-refresh/calibration/blocking-read 方法、TRNG manual clock/divider/status knob。
- **embassy**：无端到端 HIL（`timer_int0_named_routing` 还专门 `not(feature="embassy")` 排除它）。
- **WS63 未测试驱动**：`clock_init`/`km`/`pke`/`safety`/`sfc`/`spacc`/`ulp_gpio`/`rtc`-WS63（`hil-rtc` 是 opt-in 且这块板没晶振从没跑过）/`delay`。
- **整个 BS2X target**（无 BS2X 板）：`chip-bs21` 需要 `unstable`；这覆盖共享驱动、`gadc`/`keyscan`/`pdm`/`qdec`/`usb`/`i2c`-v151/`rtc`-v150/`trng`-v1 等 BS2X-only 模块。
- **prelude 的 unstable re-export**：`Delay`、`InterruptTrigger`、`OneShotTimer`/`PeriodicTimer`、`Dma0`/`DmaDriver`/`Sdma0`、`RtcDriver`、`SfcDriver`、`UlpGpioPin`。
