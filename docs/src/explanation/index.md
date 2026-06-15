# 原理与背景 · Explanation

这一章不是教你"怎么做"，也不是给你查"叫什么"的索引——它讲的是**为什么**：
为什么栈这样分层、为什么要一条自定义工具链、为什么一个裸 ELF 在硅片上不会启动、
为什么我们用 UART 里的一行字符串来判定一次硬件测试是否通过。

如果你想动手，去 [教程](../tutorials/index.md) 或 [操作指南](../how-to/index.md)；
如果你想查一个确切的地址、字段或 API，去 [参考](../reference/index.md)。
本章是给你靠在椅背上读的——读完你会理解这套生态**为什么长成现在这样**，
也就更容易判断某个改动是否合理、某个故障应该往哪个方向想。

## 概念章节

这些章节自顶向下，把分散在各组件里的事实**串成一个整体**：

- [系统架构总览](architecture.md)——crate 依赖链（svd → pac → hal → 示例，rt 管启动）、
  以及那些贯穿全栈的设计取舍：`no_std`、用生命周期泛型保证安全、把 `unsafe` 寄存器访问
  封进驱动、用 sealed trait 锁住扩展点。**为什么这样分层。**
- [类型化配置：能编译就能在硅片上跑](typed-config.md)——本项目 HAL API 的头号约定:配置面
  用校验 newtype / type-state / 自起时钟收紧,操作面保持 embedded-hal 的 `Result`,
  **为什么「能写出来的值就该能跑」**、以及 A/B/C/D 缺陷分类法与逐字段决策树。
- [启动流程：mask ROM → flashboot → app](boot-flow.md)——从上电到 `main()` 的整条引导链，
  以及为什么"补 0x300 头部、烧到 app 分区"是必须的，**为什么一个裸 ELF 不会启动**。
- [硬浮点工具链](hardfloat-toolchain.md)——为什么是一条把 `riscv32imfc` 烤进 builtin 的
  自定义 rustc，**而不是** `-Z build-std`；hard-float ABI、无原子、code model 的来龙去脉。
- [async 与 embassy](async-embassy.md)——阻塞驱动、`async` feature、`embassy` feature
  三者的关系，以及它**为什么能在一颗没有原子扩展的核上跑起来**。
- [安全启动与签名](secure-boot.md)——为什么开发片把 secure boot **关掉**、这意味着什么、
  为什么一个全零"假签名"镜像照样能启动、真正签名又需要什么。
- [QEMU 模型](qemu-model.md)——hisi-riscv-qemu 为什么存在、它**能**模拟什么、**不能**
  模拟什么，以及 QEMU 里跑和真硅片上跑的根本差别。
- [HIL 测试框架](hil-framework.md)——硬件在环测试的哲学：为什么用 UART 标记串做验证通道、
  QEMU↔硅片的几类分歧、以及**诚实的当前 bring-up 状态**。

## 组件深入文档

[组件深入文档索引](components/index.md) 列出了 10 篇**逐组件**的权威深入文档
（HAL、rt、pac、svd、示例、flashboot、RF、guide、async-embassy）。
本章的概念章节会**链接进**这些深入文档：概念章讲"为什么、怎么串起来"，
深入文档讲"这一个组件内部到底怎么实现"。两者互补——读概念建立全局，读深入查实现。
