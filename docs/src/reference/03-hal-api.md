# HAL API 总览

`hisi-riscv-hal` 是手写的安全驱动层，建模自 esp-hal 模式。本页给出公开 API 的结构图。

> **完整 API 文档（rustdoc）↗** —— 在线:<https://hispark-rs.github.io/hisi-riscv-rs/api/>（`hisi-riscv-hal` / `ws63-pac` / `hisi-riscv-rt`，与本手册同站部署，CI 自动构建）;本地:`cargo doc -p hisi-riscv-hal --open`。本页只是结构图,**逐项 API 以 rustdoc 为准**。

事实取自 [`crates/hisi-riscv-hal/src/lib.rs`](https://github.com/hispark-rs/hisi-riscv-hal) 及各模块头。

模块全清单与外设映射见 [外设清单](04-peripherals.md)；稳定/不稳定 API 门控见 [稳定 / 不稳定 API 门控](../explanation/policies/02-stable-unstable.md)；async/embassy 的原理见 [async 与 embassy](../explanation/04-async-embassy.md)。

## crate 约定

- `#![no_std]`（`cfg(test)` 下链接 `std` 供主机单测）。
- 必须**恰好**选一个芯片特性：`chip-ws63` 或 `chip-bs21`（二者互斥；HAL standalone 无默认芯片，否则 `compile_error!`）。
- 依赖 `embedded-hal 1.0`、`embedded-hal-nb 1.0`、`embedded-io 0.6`、`portable-atomic`。
- 默认只导出 HIL 真机验证过的稳定面；实验性 API 需显式启用 `unstable` feature。

## 顶层导出

| 项 | 说明 |
|----|------|
| `hal::Peripherals` | 外设单例（`peripherals` 模块） |
| `hal::System` | 系统控制（仅 `chip-ws63`） |
| `hal::prelude` | 常用 trait/类型再导出 |

## 单例模式（`peripherals.rs`）

两个宏生成单例：

- `peripheral!($name, $pac_ty)` — 生成生命周期参数化 ZST `$name<'d>`，带 `steal()`、`ptr()`、`register_block()`。
- `peripherals!(...)` — 生成 `Peripherals` 结构，带 `take() -> Option<Self>`（安全，仅一次）与 `steal()`（`unsafe`）。

```rust,ignore
let p = hal::Peripherals::take().unwrap();        // 一次性安全取得
let uart = Uart::new_uart0(p.UART0, Config::default());
```

每个驱动经构造函数消费其外设 token（`'d` 生命周期参数防止 `Peripherals` 失效后再用）。

## GPIO 三级驱动（`gpio.rs`）

| 级别 | 类型 | 创建方式 |
|------|------|----------|
| 1 类型擦除 | `AnyPin<'d>` | `unsafe AnyPin::steal(pin_number)` |
| 2 类型化驱动 | `Input<'d>` / `Output<'d>` / `Flex<'d>` | 由 `AnyPin` 经 `init_input()` / `init_output()` / `init_flex()` |
| 3 旧式类型态 | `GpioPin<'d, MODE>` | 向后兼容 |

配置结构：`InputConfig { pull }`、`OutputConfig { initial_high }`。旧 `open_drain` 字段没有硬件落地，已删除。

## 多实例外设

UART / I2C / SPI / DMA 用 `PhantomData<&'d T>` 区分实例，构造函数按实例分开（每实例可能配置不同）：

| 外设 | 类型 | 构造函数 |
|------|------|----------|
| UART | `Uart<'d, T>` | `new_uart0(UART0, Config)`、`new_uart1(...)`、`new_uart2(...)` |
| I2C (WS63 v150) | `I2c<'d, T>` | `new_i2c0(I2C0, freq)`、`new_i2c1(...)` |
| SPI (DesignWare SSI v151) | `Spi<'d, T>` | `new_spi0(SPI0, Config)`、`new_spi1(SPI1, Config)` |
| DMA | `DmaDriver<'d, T: DmaInstance>` | 泛型于 `Dma0` / `Sdma0` 标记；公共模块需 `unstable` |

> SPI `Config { frequency, mode, data_bits }`；UART `Config { baudrate, data_bits, parity, stop_bits, clock }`，其中 `clock` 是 `UartClock::{Pll, Boot}`。

## 单外设驱动（`new()` 模式）

多数驱动遵循 `DriverName::new(peripheral)`：`Watchdog::new`、`TimerDriver::new`、`TcxoDriver::new`、`RtcDriver::new`、`LsAdc::new`、`I2sDriver::new`、`PwmChannel::new(&Pwm, channel)`、`SfcDriver::new`、`PkeDriver::new`、`SpaccDriver::new`、`KmDriver::new`、`TrngDriver::new`、`TempSensor::new`、`EfuseDriver`、`System::new(SysCtl0, GlbCtlM, CldoCrg)`。其中未上板或 soundness 未闭合的模块/方法会被 `unstable` 门控；完整签名见各模块 rustdoc。

## 时钟（`clock.rs` / `clock_init.rs`，仅 `chip-ws63`）

- `clock_init::init_clocks(&sys_ctl0, &cldo_crg) -> SystemClocks` — 为不经 flashboot 启动的固件初始化系统时钟，需 `unstable`。
- `Peripheral` 枚举把每个已审计外设映射到 `(cken_register_index, bit_position)`；无 SDK/SVD 证据的外设返回 `None`。
- 旧 `ClockControl` / `PeripheralGuard` RAII 层已删除；当前驱动依赖复位默认开，未来若恢复门控需从 `Peripheral::cken_info()` 派生。

> 多数外设访问寄存器前需经 CLDO_CRG 门控使能；复位默认即使能；WDT/RTC/TCXO 常开。

## sealed trait（`private.rs`）

`Sealed` 是 crate 内部超 trait，阻止外部实现 GPIO signal trait（如 `PeripheralInput`、`PeripheralOutput`）。旧的空 `DriverMode`/`Blocking`/`Async` 标记 trait 以及 vestigial `DmaWord` 均已移除。

## 特性（features）

| 特性 | 内容 |
|------|------|
| `chip-ws63` / `chip-bs21` | 选芯片，互斥；HAL standalone 无默认芯片 |
| `async` | blocking-backed SPI/I2C `embedded-hal-async` 实现；GPIO wait、timer delay、UART async I/O、DMA/LSADC async hook 还需 `unstable` |
| `embassy` | embassy-time `Driver` feature；公共 `embassy` 模块还需 `unstable` |
| `unstable` | 暴露未毕业实验性 API（DMA、interrupt/waker async helpers、embassy、未上板驱动等） |

> `async`/`embassy` 在无原子的 WS63 上经 `portable-atomic` + `critical-section` 工作；但默认稳定 API 只承诺已 HIL 覆盖且 soundness 闭合的子集。
