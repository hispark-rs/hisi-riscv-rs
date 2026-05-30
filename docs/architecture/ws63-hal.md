# ws63-hal 架构与评审

> 本文是 ws63-rs 架构文档的一部分。完整评审台账见 [架构评审 2026-05](../review/architecture-review-2026-05.md)，整改排期见 [ROADMAP](../../ROADMAP.md)。

## 职责与边界

`ws63-hal` 是 WS63 SoC 的硬件抽象层（HAL），在 `ws63-pac` 的裸寄存器之上手写安全、符合 embedded-hal 习惯的驱动 API。

- **负责**：
  - 为 35 个 PAC 外设提供生命周期化的安全单例封装（`peripherals.rs`），并在其上实现 31 个外设驱动模块（GPIO、UART、SPI、I2C、DMA、PWM、Timer、WDT、RTC、TRNG、Tsensor、SFC、I2S、LSADC、eFuse、以及 KM/PKE/SPACC 等加密外设）。
  - 时钟架构：时钟门控（`clock.rs` 的 `ClockControl` + `Peripheral` 枚举）、引导期时钟树初始化（`clock_init.rs`）。
  - GPIO 三层驱动模型、DMA 双控制器抽象、sealed trait 体系（`private.rs`）。
  - embedded-hal 1.0 / embedded-hal-nb 1.0 / embedded-io 0.6 / nb 的 trait 实现。
- **不负责**：
  - 裸寄存器布局与地址映射（属 `ws63-pac`）。
  - 启动汇编、链接脚本、中断向量表（属 `ws63-rt`）。
  - 应用业务逻辑（属 `ws63-examples`）。
  - 连接性协议栈（WiFi/BLE/SLE）、porting 层、HCC IPC（尚未实现，见 ROADMAP 阶段 4-5）。

`#![no_std]`、无堆、无 `Vec`（`lib.rs:20`）。寄存器访问全部经 `unsafe { w.bits(...) }` 封装在驱动方法内部。

## 在依赖链中的位置

```
ws63-svd (XML)
   │ svd2rust 生成
   ▼
ws63-pac ──► ws63-hal ──► ws63-examples/*
                ▲
       ws63-rt（启动汇编 / 链接脚本 / 中断向量）并行提供运行期支撑
```

`ws63-hal` 是承上启下的核心层：向下消费 `ws63-pac` 的 `RegisterBlock`，向上为示例提供驱动。它**不**直接依赖 `ws63-rt`，但其中断子系统依赖 `riscv` crate 的 trap 模型，运行期向量表由 `ws63-rt` 的 `device.x` 提供。

依赖：`embedded-hal 1.0`、`embedded-hal-nb 1.0`、`embedded-io 0.6`、`nb`、`portable-atomic`、`riscv`。

## 关键设计

### 外设单例 + `'d` 生命周期

`peripherals.rs` 用两个宏生成全套封装：

- `peripheral!($name, $pac_ty)`（`peripherals.rs:10-48`）— 为每个外设生成零大小、`'d` 参数化的 ZST，提供 `unsafe steal()`、`ptr()`、`register_block()`。
- `peripherals!(...)`（`peripherals.rs:50-87`）— 生成 `Peripherals` 结构体，`take()` 经 PAC 单例校验（`peripherals.rs:61-64`），`unsafe steal()` 绕过校验。

全部 35 个 PAC 外设都有 HAL 封装（`peripherals.rs:89-161`）。`'d` 生命周期防止 `Peripherals` token 被释放后仍持有驱动，是这一层的核心安全不变量（评审优点）。

### 时钟架构

两套并存：

1. **`clock_init.rs`（标杆）** — 逐寄存器对照 fbb_ws63 C SDK 的启动时钟序列核实。文件头部完整记录了 `CLDO_CRG_CLK_SEL` 位图、寄存器地址映射、时钟树（`clock_init.rs:1-74`）。`init_clocks()`（`clock_init.rs:197-253`）实现 flash→PLL（bit 18）、UART0/1/2→PLL（bits 1/2/3）、SPI→PLL（bit 6）的切换，并经 `REG_EXCEP_RO_RG` bit 12 轮询 PLL 锁定（`clock_init.rs:127-148`）。TCXO 频率检测读 `HW_CTL` bit 0（`clock_init.rs:103-107`）。所有地址均注明 fbb_ws63 出处。
2. **`clock.rs` 的 RAII 时钟门控** — `ClockControl` 封装 `CldoCrg`，提供 `enable_uart()`/`enable_spi()` 等直接方法（`clock.rs:192-260`），以及 `PeripheralGuard` 引用计数守卫（`clock.rs:86-125`），用 `static REF_COUNTS: [AtomicU8; 17]`（`clock.rs:74-78`）做并发安全的开/关计数。`Peripheral::cken_info()`（`clock.rs:45-67`）将每个外设映射到 `(cken 寄存器索引, 位)`，PWM 的 9 位连续门控（bits 2:10）特殊处理。

### GPIO 三层模型

19 个引脚分布在 3 个 block（GPIO0 bits 0-7、GPIO1 bits 8-15、GPIO2 bits 16-18），block 映射为 `pin / 8`、位为 `pin % 8`（`gpio.rs:86-88`，评审确认正确）。三层：

1. `AnyPin<'d>` — 类型擦除，经 `unsafe steal(pin)` 创建（`gpio.rs:75-134`）。
2. `Input` / `Output` / `Flex` — 由 `AnyPin` 经 `init_input()`/`init_output()`/`init_flex()` 派生（`gpio.rs:107-133`）。
3. `GpioPin<'d, MODE>` — 遗留 type-state GPIO（`gpio.rs:364-476`）。

`InputConfig { pull }` / `OutputConfig { open_drain, initial_high }` 为配置入口。embedded-hal `digital` trait 用 `Infallible` 错误类型实现（`gpio.rs:177-188` 等）。

### DMA 双控制器

`Dma0`（0x4A00_0000）与 `Sdma0`（0x520A_0000）共享 `dma::RegisterBlock`，经 `DmaInstance` trait 提供 `ptr()`（`dma.rs:25-44`）。`DmaDriver<'d, T: DmaInstance>` 泛型于控制器（`dma.rs:144`）。`DmaEligible`（`dma.rs:428-431`）+ `DmaChannelFor<P>`（`dma.rs:439`）意图提供编译期通道-外设绑定安全（刻意不写 blanket impl 以保留约束语义）。

### Sealed trait 与驱动 mode 标记

`private.rs` 定义 `Sealed` 超 trait，封印 `DmaWord`、`PeripheralInput`、`PeripheralOutput`、`DriverMode`。`Blocking`/`Async` 是为未来异步预留的 mode 标记（`private.rs:32-51`）。

### embedded-hal trait 选型（评审优点）

- SPI 实现 `SpiBus` 而非 `SpiDevice`（`spi.rs:135`）— HAL 层不持有 CS，符合分层惯例。
- I2C `transaction` 在操作间发 repeated-START、仅末尾发 STOP（`i2c.rs:215-265`），符合 embedded-hal 契约。NACK 映射为 `NoAcknowledge`（`i2c.rs:278-280`）。
- UART 同时实现 embedded-io `Read`/`Write` 与 embedded-hal-nb serial（`uart.rs:172-293`）。

### 编译期断言（`safety.rs`）

`const_assert!` 宏（`safety.rs:11-20`）校验 MMIO 地址范围、`PERIPHERAL_COUNT == 17`、各类外设/通道计数常量。注意此文件的多数断言为恒真（见评审问题）。

## 评审发现

### 优点

- **`clock_init.rs` 是全仓标杆**：逐寄存器、逐位对照 fbb_ws63 C SDK 核实，地址与位含义均注明出处（`clock_init.rs:36-74`、`197-253`）。
- **外设单例 + `'d` 生命周期健全**：宏生成统一、`take()` 经 PAC 单例校验，生命周期防 use-after-drop（`peripherals.rs:10-87`）。
- **embedded-hal/embedded-io/nb trait 选型正确**：`SpiBus`（非 `SpiDevice`）、I2C repeated-START、ACK→`NoAcknowledge` 均符合各 trait 契约（`spi.rs:135`、`i2c.rs:215-280`）。
- **GPIO block 映射正确**：`pin/8` 分 block、`pin%8` 取位，与 3 block × 8 位的硬件布局一致（`gpio.rs:86-88`）。

### 问题

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|--------|------|------|-----------------|------|
| 严重 | 正确性 | 中断子系统建在不存在的 PLIC 模型上。WS63 用自定义 CSR（`LOCIPRI`=0xBC0 / `LOCIEN`=0xBE0 / `LOCIPD`=0xBE8，见 fbb_ws63 `riscv_interrupt.h`）。`enable`/`disable`/`bind_handler` 忽略 `_interrupt` 参数，仅做全局 `riscv::interrupt::enable()`，`disable` 是空函数 | `interrupt.rs:63-82` | 已排期(ROADMAP 阶段 2) |
| 严重 | 正确性 | SPI `ctra` 写入 `trsm=3`（bits 19:18），该值是 EEPROM-Read 模式；全双工 TX+RX 应为 `0`。注释误写"TX+RX mode"导致 `transfer`/`SpiBus` 全双工语义不成立 | `spi.rs:76` | 已排期(ROADMAP 阶段 2) |
| 高 | 正确性 | I2C/SPI 共 9+ 处 `while !...read()...{}` 无超时死循环；`I2cError::{Timeout,BusError}`、`SpiError::Overflow` 已定义但从不返回，硬件挂死则永久阻塞 | `spi.rs:88,90,102,116,118,149,151`；`i2c.rs:59,80,130,153,160,197,205,250,261` | 已排期(ROADMAP 阶段 2) |
| 高 | 正确性 | `software_reset` 执行 `ebreak`（调试陷阱，非系统复位）后死循环；`reset_reason` 读了寄存器但恒返回 `PowerOn`，从不解析状态位 | `system.rs:43-71`（`ebreak`@60，恒返回@49） | 已排期(ROADMAP 阶段 2) |
| 中 | 正确性 | GPIO `InputConfig.pull` 被静默忽略：`init_input` 只设 OEN，从不写上下拉寄存器；`config` 字段标 `#[allow(dead_code)]` | `gpio.rs:107-110,141-142` | 已排期(ROADMAP 阶段 2) |
| 高 | 正确性 | eFuse / LSADC 寄存器布局为猜测，与 SDK 矛盾 | `efuse.rs`（266 行）、`lsadc.rs`（323 行） | 已排期(ROADMAP 阶段 2) |
| 中 | 维护性 | `safety.rs` 多条 `const_assert!` 为恒真断言（如 `0x4401_0000 >= 0x4000_0000` 字面量比较，编译期必然成立），属"断言剧场"；模块头"Formal safety contracts / Type-level proofs"措辞夸大 | `safety.rs:1-6,37-44` | 已排期(ROADMAP 阶段 2) |
| 中 | 架构 | 零消费者死代码：RAII 时钟守卫（`ClockControl`/`PeripheralGuard`/`REF_COUNTS`）、DMA 安全 trait（`DmaEligible`/`DmaChannelFor`）、async marker（`Blocking`/`Async` 两者 `Async<D>=D` 恒等）全无下游使用者 | `clock.rs:86-168`；`dma.rs:428-439`；`private.rs:32-51` | 已排期(ROADMAP 阶段 2) |
| 高 | 维护性 | 测试是恒真式——重抄被测代码自身的公式再断言（如 SPI 分频器测试在测试内重算一遍同样的式子），从未上板验证 | `spi.rs:199-291`（分频器重算）；`i2c.rs:319-350`（地址移位重算）；`clock.rs:319-396`、`safety.rs:131-188` | 已排期(ROADMAP 阶段 1：硬件在环) |

## 改进项与排期

按 [ROADMAP](../../ROADMAP.md)：

- **阶段 1（硬件在环 bring-up + 链接脚本集成）**：示例当前因 `ws63-rt` 链接脚本不传播到下游二进制而无法链接（`blinky` 缺 `__exc/nmi/irq_stack_top__` 符号），需先打通链接脚本集成；随后用真实硬件替换恒真式测试做在环验证。
- **阶段 2（死代码清理 + 正确性修复）**：重写中断子系统到 `LOCIPRI`/`LOCIEN`/`LOCIPD` CSR 模型；修 SPI `trsm`；为 I2C/SPI 加超时并真正返回错误；修 `software_reset`/`reset_reason`；接通 GPIO pull；核实并修正 eFuse/LSADC 寄存器；删除 `safety.rs` 恒真断言与夸大措辞；删除 RAII 时钟守卫 / DMA 安全层 / async marker 死代码（在引入真实消费者之前）。
- **阶段 4-5（porting 层 + HCC IPC + 连接性）**：HAL 之上接入 WiFi/BLE/SLE 协议栈所需的 porting 与 IPC 通道。
- **阶段 6（async）**：在确有异步消费者后再恢复 `Blocking`/`Async` 类型状态（阶段 2 已先删除空壳）。
