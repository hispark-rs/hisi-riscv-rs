# ws63-flashboot 架构（实验性）

> ⚠️ **实验性 / 学习用途——不是安全启动，不要用于生产。** 详见 [README.md](README.md)。

`ws63-flashboot` 是 ws63-rs monorepo 中**内置**（非子模块）的实验性 Rust 二级引导，裸 MMIO 实现
（有意不依赖 `ws63-pac`），用于学习 WS63 启动流程。它被排除在默认构建之外、`publish = false`。

核心结论（评审）：它把原厂"基于 efuse 根密钥的 ECC/SM2 签名验签"降级成了一个**仅完整性、可被攻击者重算**
的 SHA256 校验，且镜像头布局未对齐真实 WS63 镜像，partition/upgrade/clock 多为桩。**生产应复用 fbb_ws63
原厂 flashboot**，Rust 应用跑在其加载的分区中。

完整架构与评审（集中维护于主仓库）：
- 组件文档：<https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/src/explanation/components/ws63-flashboot.md>
- 整改排期：<https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md>（阶段 2，若继续维护）
