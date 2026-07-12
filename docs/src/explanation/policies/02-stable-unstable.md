# 稳定 / 不稳定 API 门控

这是本项目 HAL（0.6.0+）的**第二号约定**：凡是没有上板 HIL 真机测试覆盖，或 safe/unsafe soundness 还没闭合的接口，都关在 `unstable` feature 门后 —— 默认 `cargo build` 只暴露经过硅片验证且无已知 soundness blocker 的稳定 API，实验性接口要用户显式 `features = ["unstable"]` 才能用。这和 [类型化配置](01-typed-config.md) 互补：一个保证"能编译就能跑"，一个保证"默认暴露的是跑过且可承诺的"。

本篇讲**为什么**这样设计、**机制**怎么工作、以及 API **如何毕业**。当前 stable / unstable 清单是参考事实，见 [Stable API 清单与门控状态](../../reference/10-stable-api.md)。

## 问题：没上过板的 API 照样 pub

WS63 HAL 有大量驱动：有些在真实硅片上跑过 HIL 测试（GPIO/SPI/UART/Timer/PWM/WDT/TRNG/eFuse…），有些**从未上过板**（crypto 加速器 PKE/SPACC/KM、flash 控制器 SFC、ULP-GPIO、整个 BS2X 系列因为没板子）。还有一类更危险：个别路径有 HIL，但公共 safe API 的所有权、cache、取消或超时不变式还没闭合，例如 DMA。0.5.x 里它们全是 `pub` —— 用户无差别依赖，下个小版本一改签名就坏。

更糟的是：外设 DMA 的 `UartDma::write_dma` 源码注释写着"silicon-verified"，但其实 #5（UART1 TX 移位寄存器不推进）导致它上板 timeout 了，从没真正验过。**注释和事实不符**，因为没有一个机制把"验过"和"没验过"在编译期分开。

## 标准：凡没上板测试的都门控

判定规则简单且严格：

| 有 HIL 真机测试覆盖？ | 判定 |
|---|---|
| 有（在 `tests/hil.rs` 里能找到调用了该 API 的测试，且在真实 WS63 硅片上跑过）且无已知 soundness blocker | **STABLE** —— 默认 `pub`，不加门控 |
| 没有，或测试是 opt-in 且从未在连的板上跑，或 safe/unsafe 不变式未闭合 | **UNSTABLE** —— 关在 `unstable` 后 |

稳定承诺只面向 **WS63 默认稳定子集**。BS2X 目前没有硅片 HIL，不能把 WS63 上的跨芯片驱动结论外推到 BS2X；因此整个 `chip-bs21` target 需要显式 `unstable`，包括共享驱动和 BS2X-only 驱动。

## 机制：instability crate（esp-hal 同款）

采用 [instability](https://crates.io/crates/instability) proc-macro（esp-hal 同款），`#[instability::unstable]` 是**软门**：

- `unstable` feature **开**时：项是 `pub`（正常导出）；
- `unstable` feature **关**时（默认）：项降级为 `pub(crate)` + `#[allow(dead_code)]`（仍在编译里，crate 内能调，但外部看不见）。

软门而非硬删的好处：一个被遗漏的 stable→unstable 引用不会编译失败（它通过 `pub(crate)` 还能编过），而且 host 单测照样能跑（`#[cfg(test)]` 模块在 crate 内能看到 `pub(crate)` 项）。

模块级用两类 crate-local 宏：

- `unstable_module!` 是**模块软门**：`unstable` 开时 `pub mod`，默认时 `pub(crate) mod` + `#[doc(hidden)]`/
  `#[allow(dead_code)]`。适合 DMA 这类 crate 内仍可能被测试或其它模块引用的实验面。
- `unstable_driver!` 是**驱动硬门**：`unstable` 开时才编译 `pub mod`，默认时整个模块不存在。适合 standalone
  且默认稳定面不依赖的驱动/集成层，例如 `asynch` / `embassy` 这类 feature + unstable 双重同意的路径。

### 门控规则（关键）

- **inherent impl 块不挂属性** —— `instability` 对 impl 块是**硬删**（关时整个消失），会让被它调的私有函数变 dead_code。只挂 impl 块里的**各个 pub fn**（软门 `pub(crate)`，私有 helper 不受影响）。
- **`impl Drop` 不挂** —— 保持它调的 helper 活着。
- **trait impl 可以整块挂**（关时消失，安全）。
- **standalone 实验驱动可用 `unstable_driver!` 硬门**，前提是默认稳定代码没有任何路径依赖该模块；否则用
  `unstable_module!` 软门，保持 crate 内引用可编译。
- **STABLE 的 pub fn 签名里不能出现 UNSTABLE 类型**（`private_interfaces` lint）。如果 `write_dma`（STABLE）接收 `DmaChannel`，那 `DmaChannel` 也必须 STABLE。
- **`async`/`embassy` 不等于自动稳定。** `async` feature 只表示用户同意编译 async trait impl；当前只有 SPI/I2C 的 blocking-backed async traits 随 `async` 暴露。`asynch::block_on`、`IrqSignal`、GPIO wait、timer async delay、UART async I/O、DMA/LSADC async hook 还需 `unstable`。`embassy` 模块也需 `embassy + unstable`。

## 用户怎么用

```toml
# 想用实验性接口（DMA、interrupt/waker async、embassy 等）：
[dependencies]
hisi-hal = { version = "0.7.0-alpha.1", features = ["chip-ws63", "unstable"] }

# 想构建实验性的 BS2X target：
[dependencies]
hisi-hal = { version = "0.7.0-alpha.1", features = ["chip-bs21", "unstable"] }

# 只用稳定接口（默认）：
[dependencies]
hisi-hal = { version = "0.7.0-alpha.1", features = ["chip-ws63"] }
```

实验性接口的签名**可能在小版本中变**；开 `unstable` = 同意承担 breakage。

当前默认 stable 面与 `unstable` 门后的 API 清单见参考页：[Stable API 清单与门控状态](../../reference/10-stable-api.md)。

## 毕业流程（unstable → stable）

一个接口写了 HIL 测试并在硅片上跑过后，**删掉 `#[instability::unstable]`**（或把模块从 `unstable_module!` /
`unstable_driver!` 里移出来）即可。软门下项本来就在编译里（只是 `pub(crate)`），删属性瞬间从
`pub(crate)` 变 `pub`，lint 状态不变；硬门毕业时要额外确认默认 feature 组合也能编译。可选地换成
`#[instability::stable(since = "0.x.0")]` 保留一个"已在 X 版稳定"的文档标注。

## 发布门控矩阵

发布前的门控应覆盖这些正例组合（全过 `clippy -D warnings`），并额外确认 BS2X stable-off 负例会失败：

```
{ws63,rt}  {ws63,rt,unstable}  {ws63,rt,async,embassy}
{ws63,rt,async,unstable}  {ws63,rt,async,embassy,unstable}
{bs21,rt,unstable}
```

`{bs21,rt}` 不带 `unstable` 必须触发 `compile_error!`。加 BS2X 隔离工作区（examples/bs2X/*，不在 `cargo check --workspace` 里）的显式 `cargo check --manifest-path ... --features unstable`。
