# 引言

这是 **HiSilicon RISC-V** 芯片的 Rust 嵌入式生态开发手册。当前选择的芯片是
**{{#chip-field display_name}}**；它的文档状态是 {{#chip-field status}}，HIL 状态是
{{#chip-field hil}}。

> **文档状态提示**：本生态仍在持续迭代，手册、模板和脚本可能短暂漂移。遇到不一致时，请优先以当前
> [GitHub Actions](https://github.com/hispark-rs/hisi-riscv-rs/actions)、版本化
> rustdoc、HIL/教程契约脚本的最新通过结果为准；也欢迎在
> [hispark-rs/hisi-riscv-rs issues](https://github.com/hispark-rs/hisi-riscv-rs/issues)
> 报告文档中过时、缺失或无法复现的内容。

整套生态包含：

- **`hisi-riscv-hal`** —— 手写的安全外设驱动（GPIO/UART/I2C/SPI/DMA/Timer…），基于 `embedded-hal 1.0`，可选 `async` / `embassy`。
- **{{#chip-field pac_crate}}** —— svd2rust 生成的寄存器访问层。
- **`hisi-riscv-rt`** —— 启动汇编、链接脚本、中断向量。
- **官方 Rust nightly 工具链** —— 使用 upstream `riscv32imfc-unknown-none-elf`（硬浮点、无原子）目标，当前经 `rust-src` + `-Zbuild-std=core,alloc` 构建。
- **`hisi-fwpkg`** —— 把 ELF 打包成可被 flashboot 加载的应用镜像（0x300 头）。
- **patched `probe-rs`** —— 支持当前芯片调试/烧录路径（目标名 {{#chip-field probe_chip}}）。
- **`hisi-riscv-qemu`** —— 跑得动 vendor C SDK 与 Rust 固件的 QEMU 模型。
- **HIL 测试框架** —— 在真实芯片上构建→烧录→运行→断言 UART 标记串。

## 本手册如何组织（Diátaxis）

本手册按 [Diátaxis](https://diataxis.fr/) 框架分为四个象限，各自服务不同目的：

| 象限 | 面向 | 什么时候看 |
|------|------|-----------|
| [**教程**](tutorials/00-index.md) | 学习 | 你是新手，想从零跑通第一个程序 |
| [**操作指南**](how-to/00-index.md) | 解决问题 | 你知道要做什么，需要一份可照做的步骤 |
| [**参考**](reference/00-index.md) | 查信息 | 你需要准确的事实：地址、标记串、API、命令行参数 |
| [**原理与背景**](explanation/00-index.md) | 理解 | 你想搞懂"为什么这样设计" |

如果你是第一次接触，先到[教程导读](tutorials/00-index.md)选择适合你的路径——本手册的教程分两条：

- **应用开发者**：用 `cargo generate` 从[模板](https://github.com/hispark-rs/hisi-rs-template)脚手架出**你自己的 {{#chip-field display_name}} 应用**（依赖来自 crates.io，无需克隆本仓库）。见[应用开发者路径](tutorials/app/00-index.md)。
- **生态贡献者**：克隆本 monorepo（含子模块），构建/运行完整示例集、改 HAL/PAC/运行时、跑完整 HIL。见[生态贡献者路径](tutorials/contrib/00-index.md)。

## 仓库

- 在线手册：<https://hispark-rs.github.io/hisi-riscv-rs/>（本书）
- API 文档（rustdoc）：{{#api-link hisi_riscv_hal/index.html|hisi-riscv-hal}}（按当前 chip/version 切换）
- 主仓库：<https://github.com/hispark-rs/hisi-riscv-rs>
- 工程模板：<https://github.com/hispark-rs/hisi-rs-template>（`cargo generate`）
- 其它仓库见 [CLI 工具速查](reference/08-cli-tools.md) 与各[组件文档](explanation/components/00-index.md)。
