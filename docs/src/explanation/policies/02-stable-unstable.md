# 稳定 / 不稳定 API 门控

这是本项目 HAL（0.6.0+）的**第二号约定**：凡是没有上板 HIL 真机测试覆盖，或 safe/unsafe soundness 还没闭合的接口，都关在 `unstable` feature 门后 —— 默认 `cargo build` 只暴露经过硅片验证且无已知 soundness blocker 的稳定 API，实验性接口要用户显式 `features = ["unstable"]` 才能用。这和 [类型化配置](01-typed-config.md) 互补：一个保证"能编译就能跑"，一个保证"默认暴露的是跑过且可承诺的"。

本篇讲**为什么**这样设计、**机制**怎么工作、以及**哪些在门后、哪些稳定**。

## 问题：没上过板的 API 照样 pub

WS63 HAL 有大量驱动：有些在真实硅片上跑过 HIL 测试（GPIO/SPI/UART/Timer/PWM/WDT/TRNG/eFuse…），有些**从未上过板**（crypto 加速器 PKE/SPACC/KM、flash 控制器 SFC、ULP-GPIO、整个 BS2X 系列因为没板子）。还有一类更危险：个别路径有 HIL，但公共 safe API 的所有权、cache、取消或超时不变式还没闭合，例如 DMA。0.5.x 里它们全是 `pub` —— 用户无差别依赖，下个小版本一改签名就坏。

更糟的是：外设 DMA 的 `UartDma::write_dma` 源码注释写着"silicon-verified"，但其实 #5（UART1 TX 移位寄存器不推进）导致它上板 timeout 了，从没真正验过。**注释和事实不符**，因为没有一个机制把"验过"和"没验过"在编译期分开。

## 标准：凡没上板测试的都门控

判定规则简单且严格：

| 有 HIL 真机测试覆盖？ | 判定 |
|---|---|
| 有（在 `tests/hil.rs` 里能找到调用了该 API 的测试，且在真实 WS63 硅片上跑过）且无已知 soundness blocker | **STABLE** —— 默认 `pub`，不加门控 |
| 没有，或测试是 opt-in 且从未在连的板上跑，或 safe/unsafe 不变式未闭合 | **UNSTABLE** —— 关在 `unstable` 后 |

跨芯片驱动（gpio/spi/timer 等）只要在 **WS63 硅片**上验过就算 STABLE；BS2X 没板子是芯片 bring-up 的问题，不是 API 稳定性问题，所以 BS2X 构建下它们仍 STABLE。但**整个 BS2X-specific 系列**（BS2X-only 驱动：gadc/keyscan/pdm/qdec/usb/i2c-v151/rtc-v150/trng-v1）从没在 BS2X 硅片上跑过 → 全部 UNSTABLE。

## 机制：instability crate（esp-hal 同款）

采用 [instability](https://crates.io/crates/instability) proc-macro（esp-hal 同款），`#[instability::unstable]` 是**软门**：

- `unstable` feature **开**时：项是 `pub`（正常导出）；
- `unstable` feature **关**时（默认）：项降级为 `pub(crate)` + `#[allow(dead_code)]`（仍在编译里，crate 内能调，但外部看不见）。

软门而非硬删的好处：一个被遗漏的 stable→unstable 引用不会编译失败（它通过 `pub(crate)` 还能编过），而且 host 单测照样能跑（`#[cfg(test)]` 模块在 crate 内能看到 `pub(crate)` 项）。

模块级用 `unstable_module!` 宏（`#[cfg(feature="unstable")] pub mod` + `#[cfg(not)] #[allow(unused)] pub(crate) mod`，esp-hal 同款 crate-local 形式）。

### 门控规则（关键）

- **inherent impl 块不挂属性** —— `instability` 对 impl 块是**硬删**（关时整个消失），会让被它调的私有函数变 dead_code。只挂 impl 块里的**各个 pub fn**（软门 `pub(crate)`，私有 helper 不受影响）。
- **`impl Drop` 不挂** —— 保持它调的 helper 活着。
- **trait impl 可以整块挂**（关时消失，安全）。
- **STABLE 的 pub fn 签名里不能出现 UNSTABLE 类型**（`private_interfaces` lint）。如果 `write_dma`（STABLE）接收 `DmaChannel`，那 `DmaChannel` 也必须 STABLE。
- **`async`/`embassy` 不等于自动稳定。** `async` feature 只表示用户同意编译 async trait impl；当前只有 SPI/I2C 的 blocking-backed async traits 随 `async` 暴露。`asynch::block_on`、`IrqSignal`、GPIO wait、timer async delay、UART async I/O、DMA/LSADC async hook 还需 `unstable`。`embassy` 模块也需 `embassy + unstable`。

## 用户怎么用

```toml
# 想用实验性接口（DMA、interrupt/waker async、BS2X-only 驱动、embassy 等）：
[dependencies]
hisi-riscv-hal = { version = "0.6", features = ["chip-ws63", "unstable"] }

# 只用稳定接口（默认）：
[dependencies]
hisi-riscv-hal = { version = "0.6", features = ["chip-ws63"] }
```

实验性接口的签名**可能在小版本中变**；开 `unstable` = 同意承担 breakage。

## 哪些 STABLE / 哪些 UNSTABLE

### STABLE（HIL 真机验证过 — 默认暴露）

- **WS63 默认稳定子集**：GPIO `Input`/`Output`/`Flex` + `GpioBank`，blocking SPI + blocking-backed `async` `SpiBus`，blocking UART + `BaudRate`/`UartClock`/`UartPort`/sealed `UartInstance`，blocking Timer + `TimerChannel`，TCXO，PWM `PwmPeriod`/`Duty`/`PwmChannelId` + fallible duty writes，WDT，TRNG default read/fill，eFuse `read_byte`，clock metadata，`System::reset_reason`，WS63 I2C blocking + blocking-backed `async` I2c with 7-bit address rejection，I2S config/liveness subset，IO_CONFIG GPIO/UART mux (`GpioPad`/`UartPad`/`MuxFunction`)，LSADC scan-config subset，TSENSOR basic conversion subset，cache unsafe primitives。
- **跨芯片 + 基础设施**：interrupt identity/types (`Interrupt`/`Priority`/`Threshold`) plus basic enable/disable/pending paths，peripherals，prelude，macros，soc，`Duration`/`Rate`。`private` 是 crate-internal sealed-trait 模块，不是 public API。

### UNSTABLE（没上板 — 关在门后）

- **DMA 整个公共模块**：`Dma0`/`Sdma0`、`DmaDriver`、typed channel tokens、mem-to-mem `Transfer`、`DmaTransferSize`/`DmaSyncMask`、`SpiDma`/`UartDma`、`PeripheralTransfer`、`DmaFrame`/`PeriDmaCtl`/`PeriKind` 以及所有 DMA async hook。原因不是只缺 HIL，还包括 cache-line alignment、timeout quiescence、async cancellation、SPI1/UART DMA 证据未闭合。
- **interrupt/waker async**：`asynch::block_on`、`IrqSignal`、GPIO `Wait`、timer `AsyncDelay`、UART async I/O、LSADC async。SPI/I2C 的 blocking-backed async trait impl 是例外，随 `async` 暴露。
- **不可逆 / 未闭合 soundness 的路径**：`EfuseDriver::set_clock_period`/`read_buffer`/`write_byte`（默认稳定面只保留自动 clock period + 单字节只读路径）、`System::software_reset*`、`Instant::now`/`elapsed`、interrupt priority/threshold setter/getter、SFC pad config、I2S data/FIFO/IRQ 方法、LSADC analog/conversion/filter/calibration/data-path 方法、TSENSOR mode/threshold/interrupt/auto-refresh/calibration/blocking-read 方法、TRNG manual clock/divider/status knob。
- **embassy** —— 无端到端 HIL（`timer_int0_named_routing` 还专门 `not(feature="embassy")` 排除它）。
- **WS63 未测试驱动**：`clock_init`/`km`/`pke`/`safety`/`sfc`/`spacc`/`ulp_gpio`/`rtc`-WS63（`hil-rtc` 是 opt-in 且这块板没晶振从没跑过）/`delay`。
- **整个 BS2X 系列**（无 BS2X 板）：`gadc`/`keyscan`/`pdm`/`qdec`/`usb`/`i2c`-v151/`rtc`-v150/`trng`-v1。
- **prelude 的 unstable re-export**（`Delay`、`Dma0`/`DmaDriver`/`Sdma0`、`RtcDriver`、`SfcDriver`、`UlpGpioPin`）—— re-export 的是现在 UNSTABLE 的模块/类型。

## 毕业流程（unstable → stable）

一个接口写了 HIL 测试并在硅片上跑过后，**删掉 `#[instability::unstable]`**（或把模块从 `unstable_module!` 里移出来）即可。软门下项本来就在编译里（只是 `pub(crate)`），删属性瞬间从 `pub(crate)` 变 `pub`，lint 状态不变 —— **零残留、零新 lint**。可选地换成 `#[instability::stable(since = "0.x.0")]` 保留一个"已在 X 版稳定"的文档标注。

## 构建矩阵

CI 验证 7 种组合（全过 `clippy -D warnings`）：

```
{ws63,rt}  {ws63,rt,unstable}  {ws63,rt,async,embassy}
{ws63,rt,async,unstable}  {ws63,rt,async,embassy,unstable}
{bs21,rt}  {bs21,rt,unstable}
```

加 BS2X 隔离工作区（examples/bs2X/*，不在 `cargo check --workspace` 里）的显式 `cargo check --manifest-path ... --features unstable`。
