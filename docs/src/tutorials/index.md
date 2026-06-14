# 教程 · 选择你的路径

欢迎！本章是一组**手把手的课程**：给确切的命令、给你应当看到的输出，照着做就能成功。

在开始之前，先选一条适合你的路径——两条路径**面向不同的人、起点也不同**，但风格一样（学习导向、一条happy path）。

## 你是哪一类？

### 应用开发者（用 WS63 写你自己的 App）

你想用我们**已发布到 crates.io 的库**（`hisi-riscv-hal` / `hisi-riscv-rt` / `ws63-pac`）
开发自己的 WS63 程序。你**不需要**克隆这个monorepo——
起点是用 `cargo generate` 从模板 [`hisi-rs-template`](https://github.com/hispark-rs/hisi-rs-template)
生成一个自包含的工程。有没有开发板都行（QEMU 不需要硬件）。

→ 走 [**应用开发者路径**](app/index.md)

### 生态贡献者（开发 HAL / PAC / rt / QEMU / 示例）

你想给 HAL、PAC、运行时、QEMU 模型或示例目录**贡献代码**。你需要克隆
带子模块的monorepo，构建并运行完整的示例集，并做硬件在环（HIL）测试。

→ 走 [**生态贡献者路径**](contrib/index.md)

---

> 教程只求**带你跑通**，不展开讲原理。想知道"为什么"看 [原理与背景](../explanation/index.md)；
> 想查命令和参数看 [参考](../reference/index.md)；想完成某个具体任务看 [操作指南](../how-to/index.md)。
