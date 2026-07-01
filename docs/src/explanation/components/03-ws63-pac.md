# ws63-pac 架构与评审

> 本文是 ws63-rs 架构文档的一部分。完整评审台账见 [架构评审 2026-05](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/review/architecture-review-2026-05.md)，整改排期见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

> **2026-06 更新**：PAC crate 现归并在 `crates/pac/ws63-pac`（内嵌生成源 `ws63-svd`）。其 BS2X 同胞 `crates/pac/bs2x-pac`（由 `bs2x-svd` 生成）以同样的 svd2rust 流水线服务 BS21/BS2X 家族。

## 职责与边界

`ws63-pac` 是 WS63 SoC 的外设访问层（Peripheral Access Crate），由 `svd2rust` 从 SVD 描述生成。它的职责非常聚焦：

- **负责**：为芯片上的 35 个外设提供 `RegisterBlock` 结构体与类型安全的寄存器读/写/改访问器；提供 `Peripherals` 单例（`take()` / `steal()`）；提供外部中断枚举 `ExternalInterrupt`；在 `rt` feature 下提供中断向量表 `device.x`。
- **不负责**：任何驱动逻辑、时钟门控策略、引脚复用、外设初始化时序。这些全部上移到 `hisi-riscv-hal`。PAC 只暴露"裸寄存器 + 地址映射"，是 `unsafe` 寄存器写入的最底层封装边界。

crate 元数据齐全（`Cargo.toml:1-9`）：`license = "MIT"`、`repository`、`keywords`、`categories`，具备发布到 crates.io 的条件。

## 在依赖链中的位置

```console
ws63-svd (XML)
   │ svd2rust 0.37.1 生成
   ▼
ws63-pac ──► hisi-riscv-hal ──► examples/ws63/*
   │
   └──► hisi-riscv-rt（rt feature 提供 device.x 中断向量 + RISCV_RT_BASE_ISA）
```

- 上游：`ws63-svd` 的 XML 描述，经 `svd2rust v0.37.1` 一次性生成（`src/lib.rs:1` doc 注释标注版本）。
- 下游：`hisi-riscv-hal`（安全驱动）与 `hisi-riscv-rt`（启动/链接）均消费本 crate。两者通过 **registry 版本依赖** `version = "0.1"` 声明（`crates/hisi-riscv-hal/Cargo.toml:12`、`crates/hisi-riscv-rt/Cargo.toml:21`），在 monorepo 内由根 `Cargo.toml` 的 `[patch.crates-io]` 重定向到本地路径（`Cargo.toml:50-51`），保证全工作区只链接**单一** PAC 实例。

## 关键设计

- **svd2rust 0.37.1 现代访问器**：generic 层用 `Periph<RB, const A: usize>` 把外设基址作为 const 泛型参数编码（`src/lib.rs:14-20`），`ptr()` 是 `const fn`（`src/lib.rs:23-25`），`Deref` 直接解到寄存器块（`src/lib.rs:45-51`）。这是新版 svd2rust 的 const-fn 访问器风格，零运行时开销。
- **Peripherals 单例**：`static mut DEVICE_PERIPHERALS: bool`（`src/lib.rs:31681`）作为一次性标志；`take()` 在 `critical-section` 内检查并返回 `Option<Self>`（`src/lib.rs:31760-31767`），`steal()` 为 `unsafe` 无检查版本（`src/lib.rs:31774-31813`）。`Peripherals` 结构体逐字段持有 35 个外设的 ZST 句柄（`src/lib.rs:31684-31755`）。
- **35 外设覆盖**：从 `sys_ctl1`、三路 `gpio0/1/2`、三路 `uart0/1/2`、双 `i2c`、双 `spi`、`dma`/`sdma`，到安全引擎 `spacc`/`pke`/`km`/`trng` 与时钟复位 `cldo_crg` 等全部映射（`src/lib.rs:31685-31754`）。
- **中断模型**：`ExternalInterrupt` 枚举用 `#[riscv::pac_enum(unsafe ExternalInterruptNumber)]` 标注（`src/lib.rs:902-904`），中断号从 26 起（`TIMER_INT0 = 26`，`src/lib.rs:906`）。`rt` feature 下 `build.rs` 把 `device.x` 写入 `OUT_DIR` 并加入 link-search（`build.rs:8-18`），向量表用 `PROVIDE(... = DefaultHandler)` 提供弱默认（`device.x:1-30`）。
- **feature 设计**：`default = ["critical-section"]`，外加 `rt`（`Cargo.toml:16-18`）。`take()` 仅在 `critical-section` 下编译（`src/lib.rs:31758`），符合 svd2rust 约定。
- **ISA 协同**：`rt` feature 下 `build.rs` 导出 `RISCV_RT_BASE_ISA=rv32i`（`build.rs:16`），与本轮 ISA 修复（默认 target 切到 `riscv32imc`、产物零原子指令）一致。

## 评审发现

### 优点

- svd2rust **0.37.1** 现代 const-fn 访问器，generic 层零开销（`src/lib.rs:14-51`）。
- 编译快（约 6s 过编译），单文件无复杂构建依赖。
- 工程化完备：`device.x` 中断向量、`critical-section`/`rt` feature（`Cargo.toml:16-18`）、crates.io 元数据齐全（`Cargo.toml:1-9`）。
- 单例语义正确：`take()`/`steal()` 配合 `DEVICE_PERIPHERALS` 标志在临界区内做一次性保护（`src/lib.rs:31760-31775`）。

### 问题

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|--------|------|------|-----------------|------|
| 中 | 维护性 | 单文件 `lib.rs` 体积约 1.8MB / 31814 行，难以审阅与定位 | `src/lib.rs`（1797361 字节、31814 行） | 暂不修（svd2rust 生成产物，按惯例不拆分；通过 CHANGELOG + grep 定位缓解） |
| 高 | 维护性 | 寄存器手补进生成代码：KM keyslot 寄存器（`KC_REECPU_LOCK_CMD` 等）在生成后人工添加，下次重生成会被覆盖 | `src/lib.rs:28415`、`28569`；`CHANGELOG.md:13-21` | 已排期(ROADMAP 阶段 2)：应回填到 ws63-svd 源头由生成器产出 |
| 中 | 依赖 | 版本曾停在 `0.1.0` 而 tag 后又追加了公开寄存器，违反 SemVer | `Cargo.toml:3`、`CHANGELOG.md` | 已修：bump `0.1.0` →（经 0.1.1/0.1.2）现 `0.1.3`，由 ws63-pac 自有仓库流水线发布 |
| 中 | 依赖 | 曾被 `hisi-riscv-hal` 以 git 依赖引入，导致工作区出现双 PAC 实例 | `crates/hisi-riscv-hal/Cargo.toml:12`、`Cargo.toml:45-51` | 本轮已修：改 registry 版本依赖 + 根 `[patch.crates-io]` 指向本地，`cargo tree` 仅单一 `ws63-pac` 实例 |

## 改进项与排期

本轮（2026-05-31，ROADMAP 阶段 0）已完成的构建完整性修复中，与本 crate 直接相关：

- **双 PAC 消除**：`hisi-riscv-hal`/`ws63-flashboot` 改为 registry 版本依赖，根 `Cargo.toml` 用 `[patch.crates-io]` 统一指向本地（`Cargo.toml:50-51`），全工作区单实例。
- **版本对齐**：`0.1.0` → `0.1.1`（与 tag 后新增的 KM 寄存器对齐），其后随各仓自有流水线发布到 **`0.1.3`**。
- **ISA 协同**：`rt` feature 导出 `RISCV_RT_BASE_ISA=rv32i`（`build.rs:16`），配合默认 target = builtin、无原子的 **`riscv32imfc-unknown-none-elf`**（硬件单精度浮点 ilp32f，原子由 portable-atomic critical-section polyfill 提供）。

仍需后续处理（指向 ROADMAP 对应阶段）：

- **手补寄存器回源（阶段 2）**：把 KM keyslot 等人工添加的寄存器回填到 `ws63-svd`，使其由 svd2rust 重生成产出，消除"生成产物被手改"的维护风险；同阶段一并补齐 efuse / lsadc 等外设寄存器的正确性。
- **单文件体积**：作为生成产物，按 svd2rust 惯例暂不拆分；若后续 SVD 重构，可评估按外设分模块生成。
