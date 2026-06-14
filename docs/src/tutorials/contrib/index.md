# 生态贡献者路径 · 导读

本路径面向**给生态本身贡献代码的人**：改 HAL、PAC、运行时（rt）、QEMU 模型，
或者维护示例目录。你会克隆带子模块的monorepo，构建并运行完整的示例集，
最后做硬件在环（HIL）测试。

## 适合谁

- 想给 `hisi-riscv-hal` / `ws63-pac` / `hisi-riscv-rt` / `hisi-riscv-qemu` 提交改动。
- 想新增或调试示例（`examples/ws63/*`），并用 QEMU + HIL 验证它们。

## 你需要准备什么

- 一台 Linux 电脑（本路径在 x86_64 Linux 上验证）。
- 已安装 [`rustup`](https://rustup.rs)、`git`、`curl`。
- 第 1 课会带你**克隆monorepo**（带子模块）、装好工具链、QEMU 和烧录器。
- 第 3 课（HIL）需要一块 WS63 开发板；第 1、2 课只用 QEMU，无需硬件。

## 三节课

1. [搭建环境（贡献生态）](01-setup.md) —— 克隆monorepo、装工具链/QEMU/probe-rs，并以一次成功的 `cargo build -p blinky` 收尾。
2. [构建与运行示例集](02-examples.md) —— 在 QEMU 里跑完整示例目录：GPIO、UART、中断、半主机退出码。
3. [第一次硬件在环测试](03-hil.md) —— 把 blinky 烧到真板，观察 GPIO 翻转，认识 `hil/hil-smoke.sh`。

学完这三课，你就能在这个仓库里构建、运行并验证示例与改动了。开始吧 ——
[搭建环境（贡献生态）](01-setup.md)。
