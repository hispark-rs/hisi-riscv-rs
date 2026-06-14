# HAL API 总览

`hisi-riscv-hal` 是手写的安全驱动层，建模自 esp-hal 模式。本页给出公开 API 的结构图。

> **完整 API 文档（rustdoc）↗** —— 在线:<https://hispark-rs.github.io/hisi-riscv-rs/api/>（`hisi-riscv-hal` / `ws63-pac` / `hisi-riscv-rt`，与本手册同站部署，CI 自动构建）;本地:`cargo doc -p hisi-riscv-hal --open`。本页只是结构图,**逐项 API 以 rustdoc 为准**。

事实取自 [`crates/hisi-riscv-hal/src/lib.rs`](https://github.com/hispark-rs/hisi-riscv-hal) 及各模块头。

模块全清单与外设映射见 [外设清单](peripherals.md)；async/embassy 的原理见 [async 与 embassy](../explanation/async-embassy.md)。

## crate 约定

- `#![no_std]`（`cfg(test)` 下链接 `std` 供主机单测）。
- 必须**恰好**选一个芯片特性：`chip-ws63`（默认）或 `chip-bs21`（二者互斥，否则 `compile_error!`）。
- 依赖 `embedded-hal 1.0`、`embedded-hal-nb 1.0`、`embedded-io 0.6`、`portable-atomic`。

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

配置结构：`InputConfig { pull }`、`OutputConfig { open_drain, initial_high }`。另有 `Io::new(IoConfig)` 顶层封装。

## 多实例外设

UART / I2C / SPI / DMA 用 `PhantomData<&'d T>` 区分实例，构造函数按实例分开（每实例可能配置不同）：

| 外设 | 类型 | 构造函数 |
|------|------|----------|
| UART | `Uart<'d, T>` | `new_uart0(UART0, Config)`、`new_uart1(...)`、`new_uart2(...)` |
| I2C (WS63 v150) | `I2c<'d, T>` | `new_i2c0(I2C0, freq)`、`new_i2c1(...)` |
| SPI (DesignWare SSI v151) | `Spi<'d, T>` | `new_spi0(SPI0, Config)`、`new_spi1(SPI1, Config)` |
| DMA | `DmaDriver<'d, T: DmaInstance>` | 泛型于 `Dma0` / `Sdma0` 标记 |

> SPI `Config { frequency, mode, data_bits }`；UART `Config { baud, data, parity, stop }`。

## 单外设驱动（`new()` 模式）

多数驱动遵循 `DriverName::new(peripheral)`：`Watchdog::new`、`TimerDriver::new`、`TcxoDriver::new`、`RtcDriver::new`、`LsAdc::new`、`I2sDriver::new`、`PwmChannel::new(&Pwm, channel)`、`SfcDriver::new`、`PkeDriver::new`、`SpaccDriver::new`、`KmDriver::new`、`TrngDriver::new`、`TempSensor::new`、`EfuseDriver`、`System::new(SysCtl0, GlbCtlM, CldoCrg)`。完整签名见各模块 rustdoc。

## 时钟（`clock.rs` / `clock_init.rs`，仅 `chip-ws63`）

- `clock_init::init_clocks(&sys_ctl0, &cldo_crg) -> SystemClocks` — 为不经 flashboot 启动的固件初始化系统时钟。
- `ClockControl` 包裹 `CldoCrg`，两种访问：直接方法（`enable_uart()` 等）或 RAII `PeripheralGuard`（`AtomicU8` 引用计数）。
- `Peripheral` 枚举把每个外设映射到 `(cken_register_index, bit_position)`。

> 多数外设访问寄存器前需经 CLDO_CRG 门控使能；复位默认即使能；WDT/RTC/TCXO 常开。

## sealed trait（`private.rs`）

`Sealed` 作为超 trait，阻止外部实现 `DmaWord`、`PeripheralInput`、`PeripheralOutput`。（旧的空 `DriverMode`/`Blocking`/`Async` 标记 trait 已移除。）

## 特性（features）

| 特性 | 内容 |
|------|------|
| `chip-ws63`（默认） / `chip-bs21` | 选芯片，互斥 |
| `async` | `embedded-hal-async`/`embedded-io-async` 实现 + `asynch::block_on` + `IrqSignal` + 各驱动 `on_interrupt` |
| `embassy` | embassy-time `Driver`，使 `embassy-executor` (platform-riscv32) 可跑 `Timer::after` |

> `async`/`embassy` 在无原子的 WS63 上经 `portable-atomic` + `critical-section` 工作。
