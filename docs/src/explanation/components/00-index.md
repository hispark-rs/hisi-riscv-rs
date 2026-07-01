# 组件深入文档

这一节是 10 篇**逐组件深入文档**的索引。它们是每个组件的**权威实现说明**——讲这一个组件
内部到底怎么实现、设计上踩过哪些坑、架构评审发现了什么。和上一层的概念章节分工明确：

- [概念章节](../00-index.md)（架构 / 启动 / 工具链 / async / 安全启动 / QEMU / HIL）讲
  **"为什么、各组件怎么串成一个整体"**；
- 这里的深入文档（deep dive）讲 **"这一个组件内部怎么实现"**。

概念章节会**链接进**这些深入文档；想建立全局图景先读概念章，想查某个组件的实现细节就来这里。

## 从这里开始

- [总体架构 overview.md](01-overview.md)——整个栈的全景：依赖链、核心设计模式、构建与目标、
  多芯片支持、已知的全局性问题。**想先看一篇就看这篇。**

## 核心库栈（依赖链自下而上）

- [ws63-svd.md](02-ws63-svd.md)——SVD 真值（CMSIS-SVD XML）+ 生成工具，整条链的最底层真值来源。
- [ws63-pac.md](03-ws63-pac.md)——svd2rust 生成的裸寄存器访问层（含 BS2X 的 bs2x-pac）。
- [hisi-riscv-hal.md](04-hisi-riscv-hal.md)——手写的安全驱动层，多芯片、可选 async/embassy。
- [hisi-riscv-rt.md](05-hisi-riscv-rt.md)——运行时：启动汇编、中断向量、链接脚本、
  critical-section 实现。
- [async-embassy.md](06-async-embassy.md)——HAL 异步层的实现细节：`block_on`/`IrqSignal`、
  每驱动 `on_interrupt` 钩子、embassy-time `Driver`、代码地图与上游化路线
  （概念总览见 [async 与 embassy](../04-async-embassy.md)）。

## 应用与芯片支持

- [ws63-examples.md](07-ws63-examples.md)——示例集合（blinky/uart/timer/gpio/dma/async/
  embassy/连接性等）的组织与验证方式。
- [ws63-flashboot.md](08-ws63-flashboot.md)——实验性 Rust 二级引导的架构与评审
  （概念上的引导链见 [启动流程](../02-boot-flow.md)）。
- [ws63-RF.md](09-ws63-rf.md)——WS63 闭源 Wi-Fi/BT/BLE/SLE blob 与 Rust porting 层。

## 硬件手册

- [ws63-guide.md](10-ws63-guide.md)——WS63 中文硬件手册（Sphinx）的说明（讲芯片，与讲代码的
  [overview.md](01-overview.md) 互补）。
