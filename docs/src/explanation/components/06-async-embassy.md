# 异步与 embassy 适配

> hisi-riscv-hal 的异步层（`async`/`embassy` feature）如何工作、代码在哪、以及之后如何上游化。
> 总体架构见 [overview.md](01-overview.md)。
> 0.6.0 稳定性边界：SPI/I2C 的 blocking-backed async trait impl 随 `async` 暴露；interrupt/waker async、DMA/LSADC async 和 `embassy` 公共模块仍需 `unstable`。

## 一句话

hisi-riscv-hal 在阻塞驱动之上加了**blocking-backed async trait impl**(`async` feature)、一层**中断 + waker 驱动的异步驱动**(`async + unstable`) 和**一个 embassy-time `Driver`**(`embassy + unstable`)。于是同一套 HAL 既能阻塞用,也能在 `embedded-hal-async` / `embassy-executor` 下 `.await`。全部跑在**单核、无原子扩展**的 WS63 上,靠 `portable-atomic` + `critical-section` 垫片。

## 三块地基

### 1. `asynch::block_on` + `IrqSignal`(`crates/hisi-riscv-hal/src/asynch.rs`, `unstable`)

- **`block_on(fut)`**:极简单 future 执行器 —— poll,Pending 就 `wfi` 休眠,硬件中断唤醒后重 poll。无堆、无全局执行器。给"不上 embassy 也想 `.await`"的场景用。
- **`IrqSignal`**:`const` 可构造的「ISR → future」桥。一个 `portable_atomic::AtomicBool`(fired 标志)+ 一个 `critical_section::Mutex` 停放的 `Waker`。驱动把它放进 `static`;ISR 调 `signal()`,future poll 时 `take_fired()`/`register(waker)`。

### 2. 每驱动的 `on_interrupt` 钩子(不自动装 ISR)

**关键设计**:异步驱动**不**抢占中断向量。每个 interrupt/waker 驱动导出一个 `on_interrupt`(`timer::on_interrupt(ch)`、`gpio::on_interrupt(bank)`、`uart::on_interrupt(idx)`、`lsadc::on_interrupt()`、`dma::on_interrupt()`、`embassy::on_alarm_interrupt()`),由**应用的 trap 处理函数**按 `mcause` 路由过去(见示例)。这些钩子在 0.6.0 默认稳定面之外,需 `unstable`。

这样开 `async`/`embassy` feature(哪怕被 cargo 工作区特性合并全局打开)**绝不**改变非异步固件的行为 —— 因为没有任何 ISR 被默认安装。

### 3. WS63 没有原子扩展 —— 怎么跑起来的

WS63 是 `riscv32imfc`(**无 A 扩展**,`lr.w/sc.w` 会陷入)。

- hisi-riscv-hal 一直用 `portable-atomic`(开 `critical-section` feature)做 CAS 垫片;`hisi-riscv-rt` 提供 `riscv/critical-section-single-hart` 实现。
- **embassy-executor 在无 CAS 目标上也能跑**(thumbv6m / riscv32imc 同理):它内部按编译期 cfg 在 `core::sync::atomic` 与 `portable_atomic` 间切换,riscv 平台模块的 `SIGNAL_WORK` 只用 load/store(WS63 支持)。所以**无需改 embassy**。
- 一个真实踩过的坑:`target/` 里**陈旧的 host proc-macro 工件**(syn/quote 来自旧 rustc)会让 embassy 宏构建莫名失败 → `cargo clean` 后全量通过。

## embassy-time `Driver`(`crates/hisi-riscv-hal/src/embassy.rs`, `unstable`)

让 WS63 成为 [embassy-time](https://docs.rs/embassy-time) 的**时间提供者**,于是 `Timer::after`/`Instant`/`Ticker` 在 embassy-executor 下可用:

- **`now()`**:读 **TCXO 64 位自由计数器**(24 MHz),缩放到 embassy-time 的 1 MHz tick(微秒)。单调、跟随真实(QEMU 上是虚拟)流逝时间。
- **`schedule_wake(at, waker)`**:把 waker 入 `embassy-time-queue-utils::Queue`;若最早截止变了,用一个 **TIMER 通道**(`ALARM_CH`,IRQ `TIMER_INT0`)编程一次性闹钟。
- **`on_alarm_interrupt()`**:闹钟 IRQ 触发时排空到期 waker、重新武装下一个截止。
- 经 `embassy_time_driver::time_driver_impl!` 注册为全局 driver(导出 `_embassy_time_now`/`_embassy_time_schedule_wake`,embassy-time 链接它们)。

应用侧:HAL 依赖需启用 `features = ["embassy", "unstable"]`,并开 `embassy-time/tick-hz-1_000_000`(对齐 `TICK_HZ`)、把闹钟通道的 trap 路由到 `on_alarm_interrupt`、`enable_global()`。其余照 embassy 标准用法。

## 代码地图

| 文件 | 内容 |
|------|------|
| `crates/hisi-riscv-hal/src/asynch.rs` | `block_on` + `IrqSignal`(地基; `unstable`)|
| `crates/hisi-riscv-hal/src/embassy.rs` | embassy-time `Driver`(now/alarm/queue; `embassy + unstable`)|
| `crates/hisi-riscv-hal/src/timer.rs` (末尾) | `AsyncDelay`(`DelayNs`)+ `on_interrupt` (`unstable`) |
| `crates/hisi-riscv-hal/src/gpio.rs` (末尾) | `Wait`(GPIO 边沿/电平)+ `on_interrupt` (`unstable`) |
| `crates/hisi-riscv-hal/src/uart.rs` (末尾) | `embedded_io_async::{Read,Write}` + `on_interrupt` (`unstable`) |
| `crates/hisi-riscv-hal/src/spi.rs` (末尾) | `embedded_hal_async::spi::SpiBus`(包装阻塞; `async`) |
| `crates/hisi-riscv-hal/src/i2c.rs` (末尾) | `embedded_hal_async::i2c::I2c`(包装阻塞; `async`) |
| `crates/hisi-riscv-hal/src/lsadc.rs` (末尾) | `read_async`(自研;IRQ 72; `unstable`) |
| `crates/hisi-riscv-hal/src/dma.rs` (末尾) | `wait_transfer_done`(自研;IRQ 59; `unstable`) |
| `crates/hisi-riscv-hal/Cargo.toml` | `async` / `embassy` feature + 可选依赖 |

**示例**(均在 ws63-qemu smoke-test 验证):
`examples/ws63/async_delay`(block_on + `DelayNs`)、`async_bus`(SPI/I2C/LSADC)、
`embassy_multitask`(embassy 多任务 + embassy-time)、`embassy_async_io`(capstone:embassy + GPIO `Wait` + async UART)。

## 覆盖范围

实现覆盖了 **`embedded-hal-async` / `embedded-io-async` 对 WS63 适用的 trait**:`DelayNs`(timer)、`digital::Wait`(GPIO)、`spi::SpiBus`、`i2c::I2c`、io `Read`/`Write`(UART);外加两个完成中断外设的自研异步(DMA IRQ 59、LSADC IRQ 72)。默认稳定面只暴露 SPI/I2C 的 blocking-backed impl；其余需 `unstable`。RTC/I2S/PWM 等无标准 async trait、语义为周期/流式/一次性 —— 保持阻塞,需要时按同一 `IrqSignal`+`on_interrupt` 模式加(RTC 的 IRQ 29 已建模)。

## 之后怎样上游化

按"上游"对象分四条线,**都不需要改 embassy 本身**:

1. **embassy 支持 —— 两种正规模型,WS63 走 out-of-tree 那条**。
   embassy 仓库**确实**收录了一批 **in-tree HAL**(`embassy-nrf`/`-stm32`/`-rp`/`-nxp`/`-imxrt`/`-microchip`/`-mspm0`/`-mcxa`…,主要是主流 Cortex-M),它们由 embassy 维护者**承诺维护**、与 embassy 内部同步演进。
   但 embassy 同时提供一套**给树外 HAL 用的接缝**(`embassy-time-driver` + `embassy-time-queue-utils` + `embassy-executor` 的 platform 抽象)—— 树外 HAL 只实现这些 trait 即可,**无需进 embassy 仓库**。最大的例子是 **esp-hal**(Espressif 自己维护、在 esp-rs/esp-hal,**不在** embassy 仓库),社区还有 ch32-hal/py32-hal 等几十个。
   **WS63 属于后者(esp-hal 模型)**,原因:① in-tree 要 embassy 维护者**采纳并长期维护**该芯片——对一颗 niche 的 HiSilicon 厂商芯片门槛极高;② embassy 的 in-tree HAL 全部基于 **stable rustc 标准 target** 构建,而 WS63 现在依赖自定义 `ws63` 工具链(无原子 target 烤进 builtin)——这是进 embassy CI 的硬阻塞(见第 3 点);③ hisi-riscv-hal 本就是**独立 HAL**(阻塞 embedded-hal + 可选 embassy),天然适合树外。
   所以**上游化 = 把带 `embassy` feature 的 hisi-riscv-hal 发布到 crates.io**(版本结构已就绪),而**不是**塞进 embassy monorepo。想更解耦可拆 `embassy-time-ws63`,但非必需。
   - 跟版:盯 `embassy-time-driver`(现 0.2)/`embassy-time-queue-utils`(0.3)/`embassy-executor`(0.10)的 semver;破坏性改动(如 `Driver` 从 alarm-handle 改成 `schedule_wake`+queue)集中在 `embassy.rs` 一个文件。

2. **embassy-executor 已是上游**:我们直接用 `platform-riscv32`,**零改动**。无 CAS 支持是它已有能力(thumbv6m 同理)。无需上游任何东西。

3. **工具链 / target**(最大的"非上游"项):现在依赖自定义 `ws63` rustc(把 `riscv32imfc-unknown-none-elf` 无原子 target 烤成 builtin)。两条上游路:
   - **短期**:改用 rustc **已有的稳定 target**(如 `riscv32imc`/`riscv32imac`)+ `-Z build-std` + `build-std-features`,去掉自定义工具链依赖 —— 代价是需要 nightly/`-Z`。
   - **长期**:把这个 target spec 提交进 rustc(niche,门槛高),或推动官方加 `riscv32imfc-*`。
   - 现状对异步**无影响**:异步只依赖 `portable-atomic`+`critical-section`,与 target 是否上游正交。

4. **QEMU 模型**(ws63-qemu):把 `-M ws63` 板卡 + `-cpu ws63` 命名核 + xlinx 自定义 ISA 解码上游到 QEMU —— 这是 ws63-qemu ROADMAP 阶段 6([github.com/hispark-rs/hisi-riscv-qemu](https://github.com/hispark-rs/hisi-riscv-qemu))。与本仓异步无直接关系,但能让 CI 不依赖 fork 的 QEMU。

> 简言之:**异步/embassy 这块本身已经是「按上游约定正确实现」**,真正的上游化工作量在 ① 把 hisi-riscv-hal(含 embassy feature)发版到 crates.io、② 摆脱自定义 rustc 工具链、③ ws63-qemu 进 QEMU 主线 —— 三者互相独立。
