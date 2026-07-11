# hisi-riscv-hal 架构

> 本文是 ws63-rs 组件深入文档的一部分，聚焦当前架构、职责边界和设计原因。当前优先级见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

> **2026-06 更新**：HAL 现为**多芯片** —— 使用 `chip-ws63` / `chip-bs21` 特性二选一（HAL standalone 无默认芯片）。后者基于 `bs2x-pac` 服务 BS21/BS2X（BLE 5.4 + SLE/星闪）家族，但因没有 BS2X 真机 HIL，整个 `chip-bs21` target 目前需 `unstable`。BS2X 全部功能外设（SPI/GADC/I2C/KEYSCAN/QDEC/RTC/TRNG/WDT/DMA/PDM/USB）已在 QEMU `-M bs21/bs22/bs20` 上验证。crate 路径 `crates/hisi-riscv-hal`。

## 职责与边界

`hisi-riscv-hal` 是 WS63 SoC 的硬件抽象层（HAL），在 `ws63-pac` 的裸寄存器之上手写安全、符合 embedded-hal 习惯的驱动 API。

- **负责**：
  - 为 36 个 PAC 外设/寄存器块提供生命周期化的安全单例封装（`peripherals.rs`），并在其上实现按功能聚合的驱动模块（GPIO、UART、SPI、I2C、DMA、PWM、Timer、WDT、RTC、TRNG、Tsensor、SFC、I2S、LSADC、eFuse、KM/PKE/SPACC、shared memory 等）。
  - 时钟架构：`clock.rs` 的 `Peripheral`/`cken_info()` 审计图、引导期时钟树初始化（`clock_init.rs`，实验面）。
  - GPIO 三层驱动模型、DMA 双控制器抽象、sealed trait 体系（`private.rs`）。
  - embedded-hal 1.0 / embedded-hal-nb 1.0 / embedded-io 0.6 / nb 的 trait 实现。
- **不负责**：
  - 裸寄存器布局与地址映射（属 `ws63-pac`）。
  - 启动汇编、链接脚本、中断向量表（属 `hisi-riscv-rt`）。
  - 应用业务逻辑（属 `ws63-examples`）。
  - 连接性协议栈（WiFi/BLE/SLE）、RF porting 层与 HCC IPC（属 `ws63-rf-rs`/厂商 blob 边界；HAL 只提供底层外设能力）。

`#![no_std]`、无堆、无 `Vec`（`lib.rs:20`）。寄存器访问全部经 `unsafe { w.bits(...) }` 封装在驱动方法内部。

## 在依赖链中的位置

```console
ws63-svd (XML)
   │ svd2rust 生成
   ▼
ws63-pac ──► hisi-riscv-hal ──► examples/ws63/*
                ▲
       hisi-riscv-rt（启动汇编 / 链接脚本；中断符号来自 PAC/rt）并行提供运行期支撑
```

`hisi-riscv-hal` 是承上启下的核心层：向下消费 `ws63-pac` 的 `RegisterBlock`，向上为示例提供驱动。它**不**直接依赖 `hisi-riscv-rt`，但其中断子系统依赖 `riscv` crate 的 trap 模型，运行期中断符号由当前 PAC 的 `rt` feature（WS63 为 `ws63-pac/rt`）提供，并由 `hisi-riscv-rt` 的 linker contract 引入。

依赖：`embedded-hal 1.0`、`embedded-hal-nb 1.0`、`embedded-io 0.6`、`nb`、`portable-atomic`、`riscv`。

## 关键设计

### 类型化配置 — "能编译就能上板"（0.5.0）

0.5.0 把**配置面**全面收紧为「能写出来的值就是能在硅上跑的值」：不存在能编译却被静默
clamp / 截断 / 没接时钟的参数。约定与 A/B/C/D 缺陷分类见
[类型化配置](../policies/01-typed-config.md)；当前默认稳定面与真机证据见 [Stable API 清单](../../reference/10-stable-api.md)。
两层结构：

- **配置/构造面（HAL 自有，可自由类型化）**：受校验 newtype + 可失败构造子返回
  `Option`/`Result`（`SpiHz`/`DataBits`/`BaudRate`/`WdtTimeout`/`SampleCount` 等），越界
  在构造点拒绝；角色用 type-state（I2S `new_master(非零派生分频)` 已验证，`new_slave()` 仍 unstable，零分频
  Master 不可表达）；驱动在 `new`/`configure` 里**自起本外设时钟门**（construct→clocked，
  如 PWM/I2S）。类型编码的是**实测硅事实而非数据手册**（如 `pwm::PwmPeriod` 是 `u16`，
  因 WS63 `pwm_freq_h` 高半字在硅上不 latch）。
- **操作面（embedded-hal trait，固定签名）**：`SetDutyCycle`/`SpiBus`/`I2c`/`Read`/`Write`
  保留标准 `u16`/`&[u8]` + `Result`（`Result` 即 embedded-hal 的非法输入惯用法），不改 trait 签名。

危险外设（`Wdt`/`PwmChannel`/`Output`）实现 scoped `Drop`（停表/关输出/回高阻）；逃生口按 HIL
证据分别稳定或门控（如 `Watchdog::into_armed`/`leak` 已验证，`PwmChannel::into_running` 仍是 unstable）。
DMA 提供拥有缓冲区的
`Transfer` guard（`embedded_dma` bound + 缓存维护折进类型），safe 代码里 use-after-free 不可表达。
每个收紧面都有 host newtype/property 测试，并在连接的真机经 HIL 套件（`tests/hil.rs`）复验。

### 外设单例 + `'d` 生命周期

`peripherals.rs` 用两个宏生成全套封装：

- `peripheral!($name, $pac_ty)`（`peripherals.rs:10-48`）— 为每个外设生成零大小、`'d` 参数化的 ZST，提供 `unsafe steal()`、`ptr()`；raw PAC `register_block()` 是 `unstable` + `unsafe` 的逃生口。
- `peripherals!(...)`（`peripherals.rs:50-87`）— 生成 `Peripherals` 结构体，`take()` 经 PAC 单例校验（`peripherals.rs:61-64`），`unsafe steal()` 绕过校验。

全部 36 个 PAC 块都有 HAL ownership token；其中未经 HIL 的行为面（如 `shared_memory`）仍受 `unstable` 门控。`'d` 生命周期防止 `Peripherals` token 被释放后仍持有驱动，是这一层的核心安全不变量。

### 时钟架构

两套并存：

1. **`clock_init.rs`（标杆）** — 逐寄存器对照 fbb_ws63 C SDK 的启动时钟序列核实。文件头部完整记录了 `CLDO_CRG_CLK_SEL` 位图、寄存器地址映射、时钟树（`clock_init.rs:1-74`）。`init_clocks()`（`clock_init.rs:197-253`）实现 flash→PLL（bit 18）、UART0/1/2→PLL（bits 1/2/3）、SPI→PLL（bit 6）的切换，并经 `REG_EXCEP_RO_RG` bit 12 轮询 PLL 锁定（`clock_init.rs:127-148`）。TCXO 频率检测读 `HW_CTL` bit 0（`clock_init.rs:103-107`）。所有地址均注明 fbb_ws63 出处。
2. **`clock.rs` 的 CKEN 参考图** — 旧 `ClockControl` / `PeripheralGuard` RAII 时钟门控因零消费者已删除。当前 `Peripheral::cken_info()` 保存经 SDK/SVD 审计的 `(cken 寄存器索引, 位)` 映射，PWM 的 9 位连续门控（bits 2:10）特殊处理；无证据的外设返回 `None`，不虚构 gate bit。

### GPIO 三层模型

19 个引脚分布在 3 个 block（GPIO0 bits 0-7、GPIO1 bits 8-15、GPIO2 bits 16-18），block 映射为 `pin / 8`、位为 `pin % 8`。两层（0.5.0 删除了遗留的 type-state `GpioPin<MODE>`，统一到下面这一套）：

1. `AnyPin<'d>` — 类型擦除，经 `unsafe steal(pin)` 创建。
2. `Input` / `Output` / `Flex` — 由 `AnyPin` 经 `init_input()`/`init_output()`/`init_flex()` 派生；`degrade()` 可安全擦除回 `AnyPin`，`Flex::set_as_input/set_as_output` 提供显式方向。

`InputConfig { pull }` / `OutputConfig { initial_high }` 为配置入口。旧 `open_drain` 字段没有硬件落地，已删除。`Output` 实现 scoped `Drop`（回 input/高阻），逃生口 `into_latched()`/`into_flex()`。embedded-hal `digital` trait 用 `Infallible` 错误类型实现。

### DMA 双控制器

`Dma0`（0x4A00_0000）与 `Sdma0`（0x520A_0000）共享 `dma::RegisterBlock`，经 `DmaInstance` trait 提供 `ptr()`。`DmaDriver<'d, T: DmaInstance>` 泛型于控制器；typed channel token、`DmaTransferSize`、`DmaSyncMask` 和 owned transfer guard 负责收窄 unsafe-adjacent 输入。0.6.0 默认不暴露公共 `dma` 模块：cache-line ownership、timeout quiescence、async cancellation 与 SPI1/UART DMA 证据未闭合前，它整体在 `unstable` 后。

### Sealed trait + 异步层

`private.rs` 定义 crate 内部 `Sealed` 超 trait，封印 `PeripheralInput`、`PeripheralOutput` 等硬件限定 signal trait。早先空壳的 `DriverMode`/`Blocking`/`Async` mode 标记（associated type 恒等、零消费者）和 vestigial `DmaWord` **已删除**。

**异步层已实现但分层暴露**（feature `async`/`embassy`，详见 [async-embassy.md](06-async-embassy.md)）：SPI/I2C 的 blocking-backed `embedded-hal-async` trait impl 随 `async` 暴露；`asynch::block_on` + `IrqSignal`（中断→waker 桥）、GPIO wait、timer async delay、UART async I/O、LSADC/DMA 自研异步以及 embassy-time `Driver` 仍需 `unstable`。全部可在无原子的 WS63 上编译（portable-atomic + critical-section），但默认稳定面只承诺 HIL/soundness 已闭合的子集。

### embedded-hal trait 选型

- SPI 实现 `SpiBus` 而非 `SpiDevice`（`spi.rs:135`）— HAL 层不持有 CS，符合分层惯例。
- I2C `transaction` 在操作间发 repeated-START、仅末尾发 STOP（`i2c.rs:215-265`），符合 embedded-hal 契约。NACK 映射为 `NoAcknowledge`（`i2c.rs:278-280`）。
- UART 同时实现 embedded-io `Read`/`Write` 与 embedded-hal-nb serial（`uart.rs:172-293`）。

### 编译期断言（`safety.rs`）

`safety.rs` 保留少量编译期结构检查，用来把 MMIO 地址范围、外设/通道数量等维护假设显式化。它不是 public API 或 stable 证据的事实源；当前稳定面与真机证据仍以 [Stable API 清单](../../reference/10-stable-api.md) 为准。
