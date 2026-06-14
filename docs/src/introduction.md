# 引言

这是 **HiSilicon WS63**（Hi3863，RISC-V RV32IMFC，Wi-Fi 6 + SLE/SparkLink + BLE）的 Rust 嵌入式生态开发手册。

整套生态包含：

- **`hisi-riscv-hal`** —— 手写的安全外设驱动（GPIO/UART/I2C/SPI/DMA/Timer…），基于 `embedded-hal 1.0`，可选 `async` / `embassy`。
- **`ws63-pac`** —— svd2rust 生成的寄存器访问层。
- **`hisi-riscv-rt`** —— 启动汇编、链接脚本、中断向量。
- **`hisi-riscv` 工具链** —— 内置 `riscv32imfc-unknown-none-elf`（硬浮点、无原子）目标的定制 stable rustc。
- **`hisi-fwpkg`** —— 把 ELF 打包成可被 flashboot 加载的应用镜像（0x300 头）。
- **patched `probe-rs`** —— 支持 WS63 的 J-Link/SWD 烧录与调试。
- **`hisi-riscv-qemu`** —— 跑得动 vendor C SDK 与 Rust 固件的 QEMU 模型。
- **HIL 测试框架** —— 在真实芯片上构建→烧录→运行→断言 UART 标记串。

## 本手册如何组织（Diátaxis）

本手册按 [Diátaxis](https://diataxis.fr/) 框架分为四个象限，各自服务不同目的：

| 象限 | 面向 | 什么时候看 |
|------|------|-----------|
| [**教程**](tutorials/index.md) | 学习 | 你是新手，想从零跑通第一个程序 |
| [**操作指南**](how-to/index.md) | 解决问题 | 你知道要做什么，需要一份可照做的步骤 |
| [**参考**](reference/index.md) | 查信息 | 你需要准确的事实：地址、标记串、API、命令行参数 |
| [**原理与背景**](explanation/index.md) | 理解 | 你想搞懂"为什么这样设计" |

如果你是第一次接触，建议从[搭建开发环境](tutorials/01-setup.md)开始。

## 仓库

- 主仓库：<https://github.com/hispark-rs/hisi-riscv-rs>
- 工程模板：<https://github.com/hispark-rs/hisi-rs-template>（`cargo generate`）
- 其它仓库见 [CLI 工具速查](reference/cli-tools.md) 与各[组件文档](explanation/components/index.md)。
