# ws63-examples 架构与评审

> 本文是 ws63-rs 架构文档的一部分。完整评审台账见 [架构评审 2026-05](../review/architecture-review-2026-05.md)，整改排期见 [ROADMAP](../../ROADMAP.md)。

## 职责与边界

`ws63-examples` 是面向最终用户的**应用示例集合**，目的是演示如何把 `ws63-rt`（启动）+ `ws63-hal`（驱动）+ `ws63-pac`（寄存器）组合成一个可烧录的裸机固件。

- **负责**：提供可参考的 `#![no_std]` / `#![no_main]` 入口、面板 LED 点灯演示、HAL API 的最小调用示例。
- **不负责**：实现任何驱动或运行时逻辑（这些属于 `ws63-hal` / `ws63-rt`）；不承担测试覆盖职责（不是测试套件）。

当前该组件**仅含一个示例 `blinky`**（GPIO 点灯，见 `ws63-examples/README.md:9`），尚无 UART / SPI / I2C / 定时器 / 连接性（Wi-Fi/BLE/SLE）示例。

## 在依赖链中的位置

examples 位于整条依赖链的**最下游**（叶子节点），消费上游所有 crate：

```
ws63-svd (XML)
   └─> ws63-pac   (svd2rust 生成的寄存器块)
          └─> ws63-hal   (手写安全驱动)
                 └─> ws63-examples/blinky   ← 本组件
ws63-rt  (启动汇编 / 链接脚本 / 中断向量)  ──提供 #[entry] 与启动────┘
```

`blinky/Cargo.toml:13-17` 直接依赖 `ws63-pac` / `ws63-hal` / `ws63-rt` 三者（外加 `embedded-hal` / `riscv`）。其中 `ws63-pac` 的直接依赖其实是冗余的——`blinky` 源码只用到 `ws63_hal` 与 `ws63_rt`（`main.rs:14-15`）。

在工作区中，`blinky` 是 `members` 但**不在** `default-members`（`Cargo.toml:7` vs `:19-23`），即默认 `cargo build` 不会构建它，需 `cargo build -p blinky` 显式触发。原因见下文构建问题。

## 关键设计

- **入口与运行时集成**：`main.rs:27` 用 `#[entry]`（来自 `ws63_rt`，`main.rs:15`）声明 `fn main() -> !`，并自带 `#[panic_handler]`（`main.rs:43-48`，自旋空转）。这是 `riscv-rt` 体系下的标准裸机入口形态。
- **GPIO 使用方式**：通过 `create_output_pin(0)` 拿到一个 `GpioPin<'static, OutputMode>`（`main.rs:30`；helper 定义见 `ws63-hal/src/gpio.rs:471`），再调用 `set_high()` / `set_low()`（`gpio.rs:199/203` 对应的 legacy 路径）。
  - 需注意：`blinky` 用的是 **legacy 类型态 GPIO**（`create_output_pin` + `GpioPin<MODE>`），并**未直接演示** HAL 新的 `OutputConfig` / `InputConfig` 构建器 API（`gpio.rs:27-65`，含 `Default` 派生与 `with_*` 链式方法）。该配置 API 是 HAL 的优点，但示例尚未覆盖它。
- **延时实现**：`delay_ms` 是**手写忙等**双重 `for` 循环 + `core::hint::spin_loop()`，按 240 MHz「240 周期 ≈ 1 µs」估算（`main.rs:17-25`）。它绕过了 HAL 的 timer/delay 抽象，精度依赖编译器不优化掉循环且时钟假设固定。
- **构建配置**：`blinky/Cargo.toml:19-21` 单独设了 `[profile.release]`（`opt-level="s"`, `lto=true`），与工作区根 profile 一致。

与参考实现的关系：esp-hal 的示例普遍调用 `Delay`/`embedded-hal` delay trait 而非手写忙等；`blinky` 当前形态更接近「最小可演示」而非「最佳实践」。

## 评审发现

### 优点

- 入口形态正确：`#[entry]` + `#[panic_handler]` 的裸机骨架完整（`main.rs:27,43`），可作为后续示例的模板。
- 演示了 GPIO 输出的最小调用链（`create_output_pin` → `set_high/set_low`），HAL 侧还提供了带 `Default` 的 `OutputConfig`/`InputConfig` 配置 API（`gpio.rs:27-65`）可供示例升级使用。
- 工作区已对 `blinky` 的不可链接现状做了诚实标注：从 `default-members` 排除并附带原因注释（`Cargo.toml:10-18`），避免 `cargo build` 默认失败。

### 问题

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|--------|------|------|-----------------|------|
| 高 | 构建 | （已修）`blinky` 曾无法链接：`ws63-rt` 的链接脚本经 lib 依赖的 `cargo:rustc-link-arg` 不传播到下游二进制，`__exc/nmi/irq_stack_top__` 等符号未定义。现 `ws63-rt` 以 `cargo:rustc-link-search` + `ws63-link.x` 导出脚本，`blinky` 新增 `build.rs` 以 `-Tws63-link.x` 引入 → **blinky 现可链接**，已加回 default-members 并产 `.bin` | `build.rs`（新增）；`ws63-rt/build.rs` | 本轮已修 |
| 高 | 方向 | 唯一示例且用手写忙等 `delay_ms` 绕过 HAL timer/delay，无法证明其余 ~31 个驱动可用；缺少 UART/SPI/I2C/连接性示例 | `main.rs:17-25`；`README.md:9-10` | 已排期(ROADMAP 阶段 1/5) |
| 中 | 文档 | `README.md` 的构建指引仍指向已弃用的自定义 JSON target（`riscv32imfc-...json`），而工作区默认 target 已改为 builtin 无原子 `riscv32imc-unknown-none-elf` | `README.md:16-22` vs `.cargo/config.toml:17` | 已排期(ROADMAP 阶段 1) |
| 中 | 演示覆盖 | 示例未直接演示 HAL 的 `OutputConfig`/`InputConfig` 构建器 API，仅用 legacy `create_output_pin` 类型态路径 | `main.rs:30`；`gpio.rs:471` vs `gpio.rs:27-65` | 已排期(ROADMAP 阶段 1) |
| 低 | 依赖 | `blinky/Cargo.toml` 直接声明 `ws63-pac` 依赖，但源码未直接使用（只用 hal/rt） | `blinky/Cargo.toml:13` vs `main.rs:14-15` | 已排期(ROADMAP 阶段 2，死代码清理) |

补充说明（本轮 2026-05-31 已完成、间接影响本组件的修复）：双 PAC 实例已消除（根 `Cargo.toml:50-51` `[patch.crates-io]` 指向本地 + hal 改 registry 版本依赖，`ws63-pac` bump 至 0.1.1）；ISA 已修为无原子 `riscv32imc`（`.cargo/config.toml:17`，portable-atomic 用 critical-section polyfill，实测编译产物零原子指令）；`ws63-rt` 已修 MIE 中断宏 typo 与栈顶符号 GC fallback。这些修复改善了 `blinky` 的**编译**（`cargo check -p blinky` 可过），但**链接**仍受阻于上表第一行的链接脚本传播问题。

## 改进项与排期

- **ROADMAP 阶段 1**：链接脚本传播已修，`blinky` 现可链接并产 `.bin`（已加回 default-members）。剩余：真机上板点灯验证；修正 README 的 target 指引（仍指向已弃用的自定义 JSON target，应为 `riscv32imc`）；将示例升级为使用 `OutputConfig`/`InputConfig` 配置 API。这是把本组件从「能链接」推进到「能跑」的关键阶段。
- **ROADMAP 阶段 2（死代码清理 + 正确性修复）**：随 HAL 中断模型（PLIC vs LOCIPRI/LOCIEN）、SPI/I2C 超时、GPIO pull 等修复，补充对应外设的最小示例并清理冗余声明。
- **ROADMAP 阶段 5（连接性示例）**：在 porting 层 + HCC IPC + blob 链接就绪后，新增 Wi-Fi/BLE/SLE 连接性示例，使示例集真正覆盖 SoC 核心能力。
- **ROADMAP 阶段 6（async）**：引入 Embassy/RTIC 风格的异步示例（依赖 HAL async 支持落地）。
