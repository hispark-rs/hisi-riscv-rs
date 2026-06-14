# hisi-riscv-hal 架构与评审

> 本文是 ws63-rs 架构文档的一部分。完整评审台账见 [架构评审 2026-05](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/review/architecture-review-2026-05.md)，整改排期见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

> **2026-06 更新**：HAL 现为**多芯片** —— `chip-ws63`（默认）/ `chip-bs21` 特性。后者基于 `bs2x-pac` 服务 BS21/BS2X（BLE 5.4 + SLE/星闪）家族；BS2X 全部功能外设（SPI/GADC/I2C/KEYSCAN/QDEC/RTC/TRNG/WDT/DMA/PDM/USB）已在 QEMU `-M bs21/bs22/bs20` 上验证。crate 路径 `crates/hisi-riscv-hal`。

## 职责与边界

`hisi-riscv-hal` 是 WS63 SoC 的硬件抽象层（HAL），在 `ws63-pac` 的裸寄存器之上手写安全、符合 embedded-hal 习惯的驱动 API。

- **负责**：
  - 为 35 个 PAC 外设提供生命周期化的安全单例封装（`peripherals.rs`），并在其上实现 35 个外设驱动模块（GPIO、UART、SPI、I2C、DMA、PWM、Timer、WDT、RTC、TRNG、Tsensor、SFC、I2S、LSADC、eFuse、以及 KM/PKE/SPACC 等加密外设）。
  - 时钟架构：时钟门控（`clock.rs` 的 `ClockControl` + `Peripheral` 枚举）、引导期时钟树初始化（`clock_init.rs`）。
  - GPIO 三层驱动模型、DMA 双控制器抽象、sealed trait 体系（`private.rs`）。
  - embedded-hal 1.0 / embedded-hal-nb 1.0 / embedded-io 0.6 / nb 的 trait 实现。
- **不负责**：
  - 裸寄存器布局与地址映射（属 `ws63-pac`）。
  - 启动汇编、链接脚本、中断向量表（属 `hisi-riscv-rt`）。
  - 应用业务逻辑（属 `ws63-examples`）。
  - 连接性协议栈（WiFi/BLE/SLE）、porting 层、HCC IPC（尚未实现，见 ROADMAP 阶段 4-5）。

`#![no_std]`、无堆、无 `Vec`（`lib.rs:20`）。寄存器访问全部经 `unsafe { w.bits(...) }` 封装在驱动方法内部。

## 在依赖链中的位置

```console
ws63-svd (XML)
   │ svd2rust 生成
   ▼
ws63-pac ──► hisi-riscv-hal ──► examples/ws63/*
                ▲
       hisi-riscv-rt（启动汇编 / 链接脚本 / 中断向量）并行提供运行期支撑
```

`hisi-riscv-hal` 是承上启下的核心层：向下消费 `ws63-pac` 的 `RegisterBlock`，向上为示例提供驱动。它**不**直接依赖 `hisi-riscv-rt`，但其中断子系统依赖 `riscv` crate 的 trap 模型，运行期向量表由 `hisi-riscv-rt` 的 `device.x` 提供。

依赖：`embedded-hal 1.0`、`embedded-hal-nb 1.0`、`embedded-io 0.6`、`nb`、`portable-atomic`、`riscv`。

## 关键设计

### 外设单例 + `'d` 生命周期

`peripherals.rs` 用两个宏生成全套封装：

- `peripheral!($name, $pac_ty)`（`peripherals.rs:10-48`）— 为每个外设生成零大小、`'d` 参数化的 ZST，提供 `unsafe steal()`、`ptr()`、`register_block()`。
- `peripherals!(...)`（`peripherals.rs:50-87`）— 生成 `Peripherals` 结构体，`take()` 经 PAC 单例校验（`peripherals.rs:61-64`），`unsafe steal()` 绕过校验。

全部 35 个 PAC 外设都有 HAL 封装（`peripherals.rs:157-193`）。`'d` 生命周期防止 `Peripherals` token 被释放后仍持有驱动，是这一层的核心安全不变量（评审优点）。

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

### Sealed trait + 异步层

`private.rs` 定义 `Sealed` 超 trait，封印 `DmaWord`、`PeripheralInput`、`PeripheralOutput`。早先空壳的 `DriverMode`/`Blocking`/`Async` mode 标记（associated type 恒等、零消费者）**已删除**。

**真正的异步层已实现**（feature `async`/`embassy`，详见 [async-embassy.md](async-embassy.md)）：`embedded-hal-async`/`embedded-io-async` 的 `DelayNs`/`digital::Wait`/`spi::SpiBus`/`i2c::I2c`/`Read`/`Write`，加上 `asynch::block_on` + `IrqSignal`（中断→waker 桥）+ 各驱动的 `on_interrupt` 钩子（不自动装 ISR）；外加 LSADC/DMA 的自研异步。还提供一个 embassy-time `Driver`（`now()`=TCXO 64 位计数器、alarm=TIMER 通道），让 `embassy-executor`（platform-riscv32）跑 `Timer::after` + 多任务。全部跑在无原子的 WS63 上（portable-atomic + critical-section）。

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

> 下表为 **2026-05 评审快照**；其后多数阶段 2 项已修（见各行状态），权威进度以 [评审台账](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/review/architecture-review-2026-05.md) 为准。全部修复在姊妹仓 `ws63-qemu` 软件在环验证。

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|--------|------|------|-----------------|------|
| 严重 | 正确性 | 中断子系统曾建在不存在的 PLIC 模型上。WS63 用自定义 CSR（`LOCIPRI`=0xBC0 / `LOCIEN`=0xBE0 / `LOCIPD`=0xBE8） | `interrupt.rs` | ✅ 阶段2已修：重写为 LOCIPRI/LOCIEN/LOCIPD CSR 模型 + 优先级/阈值；ws63-qemu `timer_irq`/`gpio_irq`(IRQ≥32) 端到端验证 |
| 严重 | 正确性 | SPI `ctra` 写入 `trsm=3`（bits 19:18），该值是 EEPROM-Read 模式；全双工 TX+RX 应为 `0`。注释误写"TX+RX mode"导致 `transfer`/`SpiBus` 全双工语义不成立 | `spi.rs:76` | 已排期(ROADMAP 阶段 2) |
| 高 | 正确性 | I2C/SPI 多处无超时死循环；错误码定义却从不返回 | `spi.rs`、`i2c.rs` | ✅ 阶段2已修：I2C/SPI 加 bounded 超时并真正返回 `Timeout` 等错误 |
| 高 | 正确性 | `software_reset` 执行 `ebreak`（非系统复位）；`reset_reason` 恒返回 `PowerOn` | `system.rs` | ✅ 阶段2已修：`software_reset` 置 GLB_CTL_M 复位位，`reset_reason` 解析 SYS_RST_RECORD；ws63-qemu `reset_demo` 往返验证 |
| 中 | 正确性 | GPIO `InputConfig.pull` 被静默忽略：`init_input` 只设 OEN | `gpio.rs` | ✅ 阶段2已修：`init_input` 经 IO_CONFIG pad 寄存器应用上下拉 + 中断触发模式 |
| 高 | 正确性 | eFuse / LSADC 寄存器布局为猜测，与 SDK 矛盾 | `efuse.rs`、`lsadc.rs` | 🟡 已对照 fbb_ws63 + ws63-qemu(eFuse 写=按位或、LSADC 转换 IRQ72) 验证读写序列；逐寄存器复核仍按阶段 2 推进 |
| 中 | 维护性 | `safety.rs` 多条 `const_assert!` 为恒真断言；模块头措辞夸大 | `safety.rs` | ✅ 阶段2已修：删除恒真断言 + 夸大措辞 |
| 中 | 架构 | 零消费者死代码：RAII 时钟守卫、DMA 安全 trait、async marker | `clock.rs`/`dma.rs`/`private.rs` | ✅ 大部已清：async marker(`Blocking`/`Async`) 与 RAII 时钟守卫已删；**并已实现真正的异步层**（见上「Sealed trait + 异步层」）；DMA `DmaEligible`/`DmaChannelFor` 保留约束语义 |
| 高 | 维护性 | 测试为恒真式（重抄被测公式再断言），从未上板验证 | `spi.rs`/`i2c.rs`/`clock.rs`/`safety.rs` | 🟡 已大幅缓解：ws63-qemu `smoke-test.sh` 用**真实固件**端到端验证（含异步/embassy 示例 + C SDK 交叉验证）；真机 HIL 冒烟仍待补（阶段 1 尾） |

## 改进项与排期

按 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)（多数已完成，下记现状）：

- **阶段 1（bring-up + 链接脚本集成）**：✅ 链接脚本集成已打通（`hisi-riscv-rt` 经 `cargo:rustc-link-search` + `ws63-link.x`，示例正常链接）；✅ 恒真式测试已由 **ws63-qemu 软件在环**大幅替代（`smoke-test.sh` 跑真实固件 + C SDK 交叉验证）；🟡 真机 HIL 冒烟仍待补。
- **阶段 2（死代码清理 + 正确性修复）**：✅ 中断子系统已重写到 `LOCIPRI`/`LOCIEN`/`LOCIPD` CSR 模型；✅ I2C/SPI 超时并返回错误；✅ `software_reset`/`reset_reason`；✅ GPIO pull + 中断触发；✅ `safety.rs` 恒真断言 + 夸大措辞已删；✅ async marker / RAII 时钟守卫死代码已删。🟡 SPI `trsm`、eFuse/LSADC 逐寄存器复核仍在推进。
- **新增（超出原评审）**：✅ **异步 HAL**（`async`/`embassy` feature，见 [async-embassy.md](async-embassy.md)）—— `embedded-hal-async`/`embedded-io-async` 全套 + embassy-time `Driver`，全部 ws63-qemu 验证。
- **阶段 4-5（porting 层 + HCC IPC + 连接性）**：HAL 之上接入 WiFi/BLE/SLE 协议栈所需的 porting 与 IPC 通道。
- **阶段 6（async）**：在确有异步消费者后再恢复 `Blocking`/`Async` 类型状态（阶段 2 已先删除空壳）。
