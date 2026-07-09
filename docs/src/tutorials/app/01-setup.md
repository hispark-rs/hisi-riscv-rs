# 搭建环境（应用开发）

本课带你装好开发 WS63 应用所需的全部工具。注意：**你不需要克隆monorepo**——
所有库依赖都来自 crates.io，工程将在下一课用 `cargo generate` 生成。

> 本课只求"把工具装上"。每个工具的深入说明与故障排查见
> [安装官方 Rust 工具链](../../how-to/01-install-toolchain.md)。

## 第 0 步：安装 Rust（rustup）

后面所有步骤都依赖 `rustup` / `cargo`。如果你机器上还没有，按 Rust 官方指引装一下
（一条命令、跨平台）：

{{#tutorial-snippet app_setup_rustup}}

> 官方安装页（含 Windows / 其它方式）：<https://www.rust-lang.org/tools/install>。
> 装完确认：`rustup --version` 和 `cargo --version` 都能打印版本即可。

## 第 1 步：安装官方 Rust nightly

WS63 应用核是 `riscv32imfc-unknown-none-elf`（硬件单精度浮点、无原子扩展）。
这个 target 已经进入官方 `rustc` nightly；当前 rustup 还没有它的预编译
`rust-std` 组件，所以需要安装 `rust-src`，由项目在构建时用
`-Zbuild-std=core,alloc` 编出 `core` / `alloc`。

{{#tutorial-snippet app_setup_toolchain}}

确认 target 和 rustup 组件状态：

{{#tutorial-snippet app_setup_check_toolchain}}

第一行应能看到 `riscv32imfc-unknown-none-elf`。第二行现在通常会提示还没有预编译
`rust-std`，这是预期状态。

## 第 2 步：安装 cargo-generate 与 just

下一课用 `cargo generate` 从模板生成工程；生成出来的工程用 `just` 跑各种命令：

{{#tutorial-snippet install_cargo_generate_just}}

## 第 3 步：安装打包工具 hisi-fwpkg（烧真机用）

烧到真板时，flashboot 期望一个带 `0x300` 启动头的应用镜像，`hisi-fwpkg` 负责打包：

{{#tutorial-snippet install_hisi_fwpkg}}

> 也可以克隆 [github.com/hispark-rs/hisi-fwpkg](https://github.com/hispark-rs/hisi-fwpkg)
> 自行构建。确认就位：`hisi-fwpkg --help`。

## 第 4 步：安装打过补丁的 probe-rs 分支（烧真机用）

上游 probe-rs 不认识 WS63，必须用打过补丁的分支，并配上它自带的
`HiSilicon_WS63.yaml` 芯片描述文件：

{{#tutorial-snippet install_probe_rs}}

确认就位：`probe-rs --version`。深入说明见
[用 probe-rs 烧录到真机](../../how-to/04-flash-probe-rs.md)。

> 只想在 QEMU 里跑、暂时不烧真机，可以先跳过第 3、4 步。

## 第 5 步：安装 QEMU（可选，`just run` 用）

想用 `just run` 在模拟器里跑，需要 [`hisi-riscv-qemu`](https://github.com/hispark-rs/hisi-riscv-qemu)——
一个带 `-M ws63` 机器模型的 QEMU 分支。克隆并构建，把它的 `qemu-system-riscv32` 放进 `PATH`：

{{#tutorial-snippet app_setup_qemu}}

确认 `ws63` 机器可用：

{{#tutorial-snippet app_setup_check_qemu}}

## 第 6 步：验证工具链

确认当前工程目录里 `rustc` 能看到 WS63 目标：

{{#tutorial-snippet app_setup_check_target}}

你应当看到：

```console
riscv32imfc-unknown-none-elf
```

> 看到这一行就说明 pinned nightly 生效了。下一课生成的工程里有
> `rust-toolchain.toml` 和 `justfile`，会自动选用该 nightly，并在构建 recipe
> 中带上 `-Zbuild-std=core,alloc`。

工具齐了！下一课我们生成你的第一个工程 ——
[从模板创建你的第一个工程](02-first-project.md)。
