# 应用开发者路径 · 导读

本路径面向**用 WS63 写自己 App 的开发者**。你会用已发布到 crates.io 的库
（`hisi-riscv-hal` / `hisi-riscv-rt` / `ws63-pac`），从模板
[`hisi-rs-template`](https://github.com/hispark-rs/hisi-rs-template) 生成一个
**自包含的工程**，在 QEMU 里跑起来，再烧到真板。

你**不需要**克隆这个monorepo——所有依赖都来自 crates.io，生成的工程自带 justfile。

## 适合谁

- 想基于现成的库快速做一个 WS63 应用，而不是改 HAL/PAC 本身。
- 想要一条"生成工程 → `just run` → `just flash`"的最短上手路径。

## 你需要准备什么

- 一台 Linux 电脑（本路径在 x86_64 Linux 上验证）。
- 已安装 [`rustup`](https://rustup.rs)、`git`、`curl`。
- 第 1 课会带你装好其余工具（自定义工具链、`cargo-generate`、`just`、烧录器、可选 QEMU）。
- **开发板可选**：QEMU 不需要硬件；只有要烧真机（第 2 课后半段）时才需要一块 WS63 开发板。

## 三节课

1. [搭建环境（应用开发）](01-setup.md) —— 装好 `hisi-riscv` 工具链、`cargo-generate`、`just`、`hisi-fwpkg`、烧录用的 probe-rs 分支，以及可选的 QEMU。
2. [从模板创建你的第一个工程](02-first-project.md) —— `cargo generate` 生成 blinky 工程，`just run` 在 QEMU 里跑，再 `just flash` 烧到真板看 LED 闪。
3. [改造成一个 UART 程序](03-uart.md) —— 用 `uart_hello` 起手，在 QEMU 里看到 `Hello from WS63 ...`。

学完这三课，你就有了一个自己的、能跑能烧的 WS63 工程。开始吧 ——
[搭建环境（应用开发）](01-setup.md)。
