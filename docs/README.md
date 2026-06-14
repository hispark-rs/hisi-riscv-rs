# ws63-rs 文档

本目录是 WS63 Rust 嵌入式生态的文档,采用 [mdBook](https://rust-lang.github.io/mdBook/) + [Diátaxis](https://diataxis.fr/) 框架组织。

## 本地构建 / 预览

```sh
cargo install mdbook        # 若未安装
mdbook serve docs           # 本地预览，http://localhost:3000
mdbook build docs           # 构建到 docs/book/
```

书的源码在 [`src/`](src/),入口是 [`src/introduction.md`](src/introduction.md),目录见 [`src/SUMMARY.md`](src/SUMMARY.md)。

## 结构(Diátaxis 四象限)

- [`src/tutorials/`](src/tutorials/) —— **教程**:从零跟着做、保证成功的学习路径。
- [`src/how-to/`](src/how-to/) —— **操作指南**:面向具体目标的"如何 X"步骤。
- [`src/reference/`](src/reference/) —— **参考**:内存映射、示例标记串、HAL API、镜像格式等准确事实。
- [`src/explanation/`](src/explanation/) —— **原理与背景**:架构、启动流程、工具链、签名、QEMU、HIL 等"为什么"。
  - [`src/explanation/components/`](src/explanation/components/) —— 各组件的深入架构文档(原 `docs/architecture/`,讲代码怎么组织、为什么这么设计、有什么问题)。

与硬件手册 [`ws63-guide`](../chips/ws63/guide/) / [`bs2x-guide`](../chips/bs2x/guide/)(讲芯片本身)互补——本目录讲**代码与工具链**。

## 其它

- 架构评审台账:[`review/architecture-review-2026-05.md`](review/architecture-review-2026-05.md)
- 整改排期:[`../ROADMAP.md`](../ROADMAP.md)
- BS21 侦察笔记:[`bs21-recon.md`](bs21-recon.md)
