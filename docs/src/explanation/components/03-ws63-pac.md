# ws63-pac 架构

> 本文是 ws63-rs 组件深入文档的一部分，聚焦当前架构、职责边界和设计原因。当前优先级见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

> **2026-06 更新**：PAC crate 现归并在 `crates/pac/ws63-pac`（内嵌生成源 `ws63-svd`）。其 BS2X 同胞 `crates/pac/bs2x-pac`（由 `bs2x-svd` 生成）以同样的 svd2rust 流水线服务 BS21/BS2X 家族。

## 职责与边界

`ws63-pac` 是 WS63 SoC 的外设访问层（Peripheral Access Crate），由 `svd2rust` 从 SVD 描述生成。它的职责非常聚焦：

- **负责**：为芯片上的 36 个外设/寄存器块提供 `RegisterBlock` 结构体与类型安全的寄存器读/写/改访问器；提供 `Peripherals` 单例（`take()` / `steal()`）；提供外部中断枚举 `ExternalInterrupt`；在 `rt` feature 下提供中断向量表 `device.x`。
- **不负责**：任何驱动逻辑、时钟门控策略、引脚复用、外设初始化时序。这些全部上移到 `hisi-riscv-hal`。PAC 只暴露"裸寄存器 + 地址映射"，是 `unsafe` 寄存器写入的最底层封装边界。

crate 元数据齐全（`Cargo.toml:1-9`）：`license = "MIT"`、`repository`、`keywords`、`categories`，具备发布到 crates.io 的条件。

## 在依赖链中的位置

```console
ws63-svd (XML)
   │ svd2rust 0.37.1 生成
   ▼
ws63-pac ──► hisi-riscv-hal ──► examples/ws63/*
   │
   └──► hisi-riscv-rt（通过 chip-ws63 启用 ws63-pac/rt，并在 linker contract 中 INCLUDE device.x）
```

- 上游：`ws63-svd` 的 XML 描述，经 `svd2rust v0.37.1` 一次性生成（`src/lib.rs:1` doc 注释标注版本）。
- 下游：`hisi-riscv-hal`（安全驱动）与 `hisi-riscv-rt`（启动/链接）均消费本 crate。两者对外通过 **registry 版本依赖** 声明（`ws63-pac = "0.2"`；BS2X 路径使用 `bs2x-pac = "0.1"`），standalone CI / publish 按各自 `Cargo.lock` 从 crates.io 解析；在 monorepo 内由根 `Cargo.toml` 的 `[patch.crates-io]` 重定向到本地 submodule，保证开发时全工作区只链接**单一** PAC 实例。

## 关键设计

- **svd2rust 0.37.1 现代访问器**：generic 层用 `Periph<RB, const A: usize>` 把外设基址作为 const 泛型参数编码（`src/lib.rs:14-20`），`ptr()` 是 `const fn`（`src/lib.rs:23-25`），`Deref` 直接解到寄存器块（`src/lib.rs:45-51`）。这是新版 svd2rust 的 const-fn 访问器风格，零运行时开销。
- **Peripherals 单例**：`static mut DEVICE_PERIPHERALS: bool` 作为一次性标志；`take()` 在 `critical-section` 内检查并返回 `Option<Self>`，`steal()` 为 `unsafe` 无检查版本。`Peripherals` 结构体逐字段持有 36 个外设/寄存器块的 ZST 句柄。
- **36 个块覆盖**：从 `sys_ctl1`、三路 `gpio0/1/2`、三路 `uart0/1/2`、双 `i2c`、双 `spi`、`dma`/`sdma`，到安全引擎 `spacc`/`pke`/`km`/`trng`、共享 RAM `share_mem_ctl`/`bt_em_ctl` 与时钟复位 `cldo_crg` 等全部映射。
- **中断模型**：`ExternalInterrupt` 枚举用 `#[riscv::pac_enum(unsafe ExternalInterruptNumber)]` 标注（`src/lib.rs:902-904`），中断号从 26 起（`TIMER_INT0 = 26`，`src/lib.rs:906`）。`rt` feature 下 `build.rs` 把 `device.x` 写入 `OUT_DIR` 并加入 link-search（`build.rs:8-18`），向量表用 `PROVIDE(... = DefaultHandler)` 提供弱默认（`device.x:1-30`）。
- **feature 设计**：`default = ["critical-section"]`，外加 `rt`（`Cargo.toml:16-18`）。`take()` 仅在 `critical-section` 下编译（`src/lib.rs:31758`），符合 svd2rust 约定。
- **ISA 协同**：`rt` feature 下 `build.rs` 导出 `RISCV_RT_BASE_ISA=rv32i`（`build.rs:16`）；当前默认目标是官方 `riscv32imfc-unknown-none-elf`，无 A 扩展，产物不得发射 `lr/sc/amo`。
