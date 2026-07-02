# 类型化配置：「能编译就能在硅片上跑」

这是本项目 HAL API 的**头号约定**:配置面被设计成 ——**你能写出来的值,就是能在真硅片上跑起来的值**。不存在「编译通过、却被静默 clamp / 截断 / 没接时钟」的参数。

本篇讲**为什么**这样设计、它**怎么和 embedded-hal 分层**、以及落地时该**怎么判断**。配方见 [如何新增一个外设驱动](../../how-to/10-add-driver.md);仓库内的可调用清单见 `.agents/skills/typed-config/`(含缺陷分类法 + 候选扫描器)。

## 问题:能写出但跑不了

很多嵌入式 HAL 的配置接口会接受一个**结构上合法、却在硅片上跑不通**的值,而且**不报错**:

- 算出来的分频/周期/计数超过寄存器位宽 → 被静默掩码截断,频率/波特悄悄错;
- 角色与分频的组合合法但**不产生时钟**(比如 I2S Master 配了零分频);
- 需要一个没被强制的前提(时钟门没开、板上没焊晶振、模拟 AFE 没上电);
- 越界被悄悄 `.clamp()` / `saturating_` / `if x == 0 { 1 }` 掉,而不是报错。

这类 bug 的代价极高:**编译、烧录、上板,全过,但行为是错的**,且无任何信号 —— 往往要拿逻辑分析仪或半天 debug 才发现。本约定的目标就是把这类错误从「上板才暴露」提前到「**根本写不出来**」。

## 两层:config 面收紧,操作面保持 embedded-hal

关键边界:**出问题的是配置面,而 embedded-hal 的 trait 根本不约束配置面。** 所以两层互不打架。

| 层 | 谁 | 规则 |
|----|----|------|
| **配置 / 构造** | HAL 自有方法(`Config`、`new*`、`configure`、`set_*`)—— **不是** embedded-hal | **随便上类型。** 校验 newtype + 可失败构造;角色用 type-state;驱动自起时钟门。 |
| **操作** | embedded-hal traits(`SetDutyCycle`、`SpiBus`、`I2c`、`Read`/`Write`、`DelayNs`…) | **签名写死**(`u16`/`&[u8]` + `Result`)。`Result` 就是 embedded-hal 表达「非法输入」的官方手段。**不要改 trait 方法签名。** |

为什么操作面只能 `Result`:embedded-hal 1.0 的 trait 方法**必须 fallible**(见下「参考」),`set_duty_cycle(u16)` 这种签名你改不了 —— 越界就返回 `Err`,这是它的惯例,不是妥协。而配置/构造方法是 HAL 自有的 inherent 方法,不归 trait 管,可以放手编译期化。

## 缺陷分类法(给每个配置字段定级)

- **A —— 寄存器位宽溢出。** 算出的值比硬件字段宽,被静默掩码/截断(`& 0xFFFF`、`as u16`)。
- **B —— 合法但死的组合。** 结构合法却不产生可用时钟/输出(如零分频的 I2S Master)。
- **C —— 未强制的前提。** 必须先开的时钟门、必须焊的晶振 / 上电的模拟 AFE、XIP-unsafe 上下文。
- **D —— 静默 clamp/wrap。** 越界被悄悄夹/饱和/`if x==0 {1}`,而不是报错。

## 决策树:每类字段怎么收

- **频率 / 波特 / 周期 / 超时**(从运行时值算出来的)→ **校验 newtype** + `const fn try_from_hz(u32) -> Option<Self>` / `from_count` / `try_new`,越出可达寄存器范围就返回 `None`。**拒绝,不要 clamp。**(治 A、D)
- **角色相关配置**(合法字段取决于模式)→ **type-state**:需要额外参数的那个状态在**构造函数里强制要求**它,非法组合在类型上不可表达。(治 B)
- **小的有限选择** → **enum**(本就装不下非法值;除非现在是裸整数)。
- **外设身份 / 通道 / 地址** → **token / enum / newtype**。safe API 不接收会影响 unsafe 前提的裸 `u8`/`usize`/裸地址:例如 DMA `DmaChannel`,GPIO/IO mux 的 `GpioPad`/`UartPad`/`MuxFunction`,GPIO IRQ 的 `GpioBank`,timer 的 `TimerChannel`,UART 的 `UartPort`,UART 时钟源 `UartClock`,PWM 的 `PwmChannelId`,eFuse 的 `EfuseByteAddress`。
- **时钟门没开** → 驱动在 `configure`/`new` 里**自起自己的时钟门**(照搬 vendor `*_porting` 的 CKEN + DIV_CTL 分频 + LOAD_DIV 序列)。(治 C)
- **板级/模拟前提**(RTC 32 kHz 晶振、ADC AFE/LDO 上电)类型治不了 → **doc + 守护**:命名明确 / `cfg` / feature 门控的构造,有界轮询(**绝不**用会拖死总线的无界轮询),加一行 `# 硬件要求` 文档。(治 C)
- **本就是全宽 32 位寄存器 / 本就是 enum** → **不动。** 不要无中生有造约束,只收真缺陷。

## 类型编码的是实测硅片现实,不是手册

最有教育意义的一例:`pwm::PwmPeriod` 是 **`u16`**,因为 WS63 的 `pwm_freq_h` 高 16 位在硅片上**根本不存值**(实测:写 `0x0001` 读回 `0`,即便整条时钟树都拉起来),而 vendor `regs_def` 明明声明这个字段是 32 位。**类型编码实测行为,而不是数据手册。** 如果某字段的真实范围拿不准,**先上板量,再定类型边界** —— 别只信 PAC/SDK 的位宽。

0.6.0 收敛里的几个同类例子:

- `uart::Config::clock_hz: Option<u32>` 被 `UartClock::{Pll, Boot}` 替代。boot-console 固件显式选 `Boot`,避免把 flashboot 的 24/40 MHz TCXO 串口时钟误当 160 MHz PLL。
- `gpio::OutputConfig::open_drain` 是无硬件落地的 no-op,已删除而不是保留成误导性稳定旋钮。
- I2C 操作仍遵守 embedded-hal 签名,但对 `addr > 0x7f` 返回 `I2cError::InvalidAddress`,不再把非法 7-bit 地址悄悄塞进寄存器序列。
- PWM 的 `SetDutyCycle` 保持 trait 签名,但 `duty > max_duty_cycle()` 返回 `PwmError::DutyOutOfRange`,不再用 `Infallible` 掩盖非法输入。
- eFuse 稳定面只保留 `EfuseByteAddress` + `read_byte`;会改变时序或不可逆写入的 `set_clock_period`/`read_buffer`/`write_byte` 进 `unstable`。

## 落地流程(docs-first)

1. **先改文档** —— 本约定要求 docs-first:先更新该驱动的组件文档 + 本页 + ROADMAP,再写代码。
2. **扫候选**:`bash .agents/skills/typed-config/scan.sh crates/hisi-riscv-hal/src/<driver>.rs`。
3. **追到寄存器**:从 PAC 拿字段真实位宽,从 vendor SDK 拿有效范围 + 时钟前提,标 `file:line`。
4. **定级 + 选方案**(决策树),**只动配置层**,embedded-hal trait impl 的签名不碰。参考 `pwm.rs`。
5. **更新测试**:host 单测/property(newtype 的接受/拒绝边界)+ `tests/hil.rs`。
6. **上板验证**(硅片佐证):寄存器/轮询级事实可上板确认;示波器级行为(真实波形)和板级前提(RTC 晶振)不能 —— 如实说明。

## 参考实现与依据

- **参考实现**:`crates/hisi-riscv-hal/src/pwm.rs` —— `PwmPeriod`(u16,`from_count`/`try_from_hz`)、`Duty`(0..=100)、`configure` 自起时钟树、`SetDutyCycle` 用 `Result` 拒绝越界 duty。另见 `uart.rs` 的 `UartClock` 与 `i2c.rs` 的 7-bit 地址拒绝。
- **仓库约定**:`AGENTS.md` 的「Typed config — if it compiles, it runs on silicon」一节 + `.agents/skills/typed-config/` skill。
- **业界依据**:
  - [esp-hal API 准则](https://hackmd.io/@esp-rs/Hy8RR5FkC):「prefer compile-time checks over runtime checks; prefer a fallible API over panics」—— 本 HAL 本就仿照 esp-hal。
  - [Parse, don't validate](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/)(Alexis King):只给可失败构造,值要么解析成功要么不存在。
  - [Typestate pattern](https://docs.rust-embedded.org/book/static-guarantees/typestate-programming.html)(The Embedded Rust Book):把运行时状态编码进编译期类型,零运行时开销。
