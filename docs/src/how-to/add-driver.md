# 如何新增一个外设驱动

给 `hisi-riscv-hal` 加一个新外设驱动，照仓库统一的「驱动模块范式」走，再配一个带 UART PASS 标记的示例让 HIL 能验证它。本篇是配方；范式背后的设计取舍见 [HAL API 总览](../reference/hal-api.md) 和[外设清单与覆盖情况](../reference/peripherals.md)，也对照仓库 `CLAUDE.md` 的「Driver Module Pattern」一节。

## 0. 确认外设单例存在

驱动消费的是 `crates/hisi-riscv-hal/src/peripherals.rs` 里用宏生成的外设单例。文件里两个宏：

- `peripheral!($name, $pac_ty)` —— 为某个 PAC 类型生成带生命周期的 ZST `$name<'d>`，附 `steal()`、`ptr()`、`register_block()`。
- `peripherals!(...)` —— 生成 `Peripherals` 结构体，带 `take()`（安全单例）和 `steal()`（unsafe）。

若你的外设的 PAC 类型还没被 `peripheral!` 包过，先加一行（注意按芯片放进对应的 `#[cfg(feature = "chip-ws63")]` / `chip-bs21` 块），并在对应的 `peripherals!(...)` 列表里加 `字段 => 类型`。例如已有的：

```rust
peripheral!(Spi0, crate::soc::pac::Spi0);
// ...
peripherals!(
    // ...
    SPI0 => Spi0,
    // ...
);
```

## 1. 写驱动模块

在 `crates/hisi-riscv-hal/src/` 加 `<name>.rs`，并在 `lib.rs` 加 `pub mod <name>;`（按芯片可加 `#[cfg(...)]`）。模块结构：

```rust
//! <Name> driver for WS63.
use crate::peripherals::MyPeriph;
use core::marker::PhantomData;

/// 配置项：用一个 `Config` 结构体 + `Default`（对齐 spi/uart 的 `Config`）。
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub frequency: u32,
    // ...
}
impl Default for Config {
    fn default() -> Self { Self { frequency: 1_000_000 } }
}

pub struct MyDriver<'d> {
    _peripheral: MyPeriph<'d>,
}

impl<'d> MyDriver<'d> {
    /// 构造即配置：消费外设单例（保证独占 + 防 use-after-drop）。
    pub fn new(peripheral: MyPeriph<'d>, config: Config) -> Self {
        let me = Self { _peripheral: peripheral };
        // ...用 me.regs() 配置硬件...
        me
    }

    /// 拿到 PAC 寄存器块。指针是静态物理 MMIO 地址，恒有效。
    fn regs(&self) -> &'static crate::soc::pac::myperiph::RegisterBlock {
        // SAFETY: PAC 指针是静态物理 MMIO 地址，始终有效。
        unsafe { &*MyPeriph::ptr() }
    }

    // ...API 方法...
}
```

要点：

- **构造函数消费外设单例**（`MyPeriph<'d>`），靠生命周期 `'d` 防止 token 被 drop 后还用——这是仓库的安全主线。
- **`regs()` 返回 `&'static RegisterBlock`**，内部 `unsafe { &*MyPeriph::ptr() }`，把 unsafe 寄存器读写收口在驱动方法里。
- **`#![no_std]`**：不用堆 / `Vec`，要缓冲区用定长数组。

### 多实例外设（UART/I2C/SPI/DMA 那种）

同一类外设有多个实例时，用 `PhantomData<&'d T>` 区分，并为每个实例给独立构造函数（不是统一 `new()`，因为每个实例配置可能不同）：

```rust
pub struct MyBus<'d, T> { idx: u8, _peripheral: PhantomData<&'d T> }
impl<'d> MyBus<'d, Inst0<'d>> { pub fn new_inst0(_p: Inst0<'d>, c: Config) -> Self { /* ... */ } }
impl<'d> MyBus<'d, Inst1<'d>> { pub fn new_inst1(_p: Inst1<'d>, c: Config) -> Self { /* ... */ } }
```

实例到寄存器块的映射用一个按 `idx` 分发的小函数（参考 `spi.rs` 的 `spi_regs(idx)`）。

## 2. 实现 embedded-hal trait

在模块末尾为驱动实现对应的 `embedded-hal 1.0` trait（SPI 实现 `spi::SpiBus`、I2C 实现 `i2c::I2c`、串口实现 `embedded-io` 等），先 `ErrorType` 再具体 trait。对照 `spi.rs` 底部：

```rust
impl embedded_hal::spi::Error for SpiError { /* kind() */ }
impl embedded_hal::spi::ErrorType for Spi<'_, Spi0<'_>> { type Error = SpiError; }
impl embedded_hal::spi::SpiBus for Spi<'_, Spi0<'_>> { /* read/write/transfer/flush */ }
```

开了 `async` feature 还可以补 `embedded-hal-async` 的对应实现（多以阻塞版兜底，见 `spi.rs` 的 `embedded_hal_async::spi::SpiBus`）。

## 3. Sealed trait（需要时）

如果你要引入「只能内部实现」的标记 trait（比如限定哪些类型能当某外设的输入/输出，或 DMA word），加在 `private.rs`：以 `Sealed` 为 supertrait，外部就无法实现。现有的有 `DmaWord`、`PeripheralInput`、`PeripheralOutput`。**不要**复活已删掉的空 `DriverMode`/`Blocking`/`Async` 标记 trait——它们没有真实 async 后端时纯属误导。

## 4. 配一个带 PASS 标记的 HIL 示例

新建一个示例 crate（如 `examples/ws63/myperiph_demo`），用 UART0 打印一个**HIL 能 grep 的标记串**，并在根 `Cargo.toml` 的 `members` / `default-members` 里登记它。骨架（仿 `spi_loopback`）：

```rust
#![no_std]
#![no_main]
use hisi_riscv_hal::Peripherals;
use hisi_riscv_hal::uart::{Config as UartConfig, Uart};
use hisi_riscv_rt::entry;

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let uart = Uart::new_uart0(p.UART0, UartConfig::default());

    let mut dev = hisi_riscv_hal::myperiph::MyDriver::new(p.MYPERIPH, Default::default());
    match dev.do_thing() {
        Ok(_)  => uart.write(0, b"  MyPeriph OK\r\n"),   // <- HIL PASS 标记
        Err(_) => uart.write(0, b"  MyPeriph FAIL\r\n"),
    }
    loop { core::hint::spin_loop(); }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop { core::hint::spin_loop() } }
```

标记串约定：用一个稳定、唯一、好 grep 的短语（如 `MyPeriph OK`）。然后把它登进 HIL 冒烟脚本，让 `hil/hil-smoke.sh` 自动断言它（脚本里加一行 `check myperiph_demo "MyPeriph OK" "..."`，标记串清单见[HIL 标记串与环境变量](../reference/hil-markers.md)）。

## 5. 验证

```bash
cargo check -p hisi-riscv-hal              # 驱动能编过（host 上 check）
cargo build -p myperiph_demo --release     # 示例能编出 ELF
```

再按[如何运行 HIL 冒烟测试](run-hil-tests.md)烧到真机看标记串。

> 提交时记得：HAL 与示例若在 submodule 里，**先在 submodule 内 commit，再更新父仓库的 submodule 指针**（见 `CLAUDE.md`）。
