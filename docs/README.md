# ws63-rs 文档

本目录是 ws63-rs 的 **Rust 代码架构与评审**文档（中文）。它与硬件手册
[`ws63-guide`](../chips/ws63/guide/) 和 [`bs2x-guide`](../chips/bs2x/guide/)（讲 WS63/BS2X 芯片本身）互补——本目录讲**代码**怎么组织、为什么这么设计、有什么问题。

## 目录

- [架构总览](architecture/overview.md) — 依赖链、核心模式、构建/目标策略、全局性问题。
- 组件架构与评审 `architecture/`：
  - [ws63-svd](architecture/ws63-svd.md) — WS63 SVD 源 + 生成工具
  - [ws63-pac](architecture/ws63-pac.md) — WS63 寄存器访问层
  - [bs2x-pac](architecture/ws63-pac.md) — BS2X（BS21/BS20/BS22）寄存器访问层
  - [hisi-riscv-hal](architecture/hisi-riscv-hal.md) — 多芯片 HAL（chip-ws63/chip-bs21 features）
  - [hisi-riscv-rt](architecture/hisi-riscv-rt.md) — 运行时（启动/中断/链接脚本）
  - [ws63-examples](architecture/ws63-examples.md) — WS63 示例 + 异步/embassy
  - [bs2x-examples](architecture/ws63-examples.md) — BS2X 示例（外设完整验证：SPI/GADC/I2C/KEYSCAN/QDEC/RTC/TRNG/WDT/DMA/PDM/USB）
  - [ws63-flashboot](architecture/ws63-flashboot.md) — 实验性二级引导
  - [ws63-RF](architecture/ws63-RF.md) — WS63 闭源协议栈 blob + porting
  - [ws63-guide](architecture/ws63-guide.md) — WS63 中文硬件手册
  - [bs2x-guide](architecture/ws63-guide.md) — BS2X 中文硬件手册
- [架构评审台账 2026-05](review/architecture-review-2026-05.md) — 41 条发现的完整列表（多 agent + 对抗式验证）。

## 相关

- [`../ROADMAP.md`](../ROADMAP.md) — 整改排期与北极星。
- [`../CLAUDE.md`](../CLAUDE.md) — 面向 AI agent 的工作指南。

> 每个子模块仓库里也有一份薄 `ARCHITECTURE.md`，指回本目录对应的组件文档（主仓库集中、子模块薄链接）。
