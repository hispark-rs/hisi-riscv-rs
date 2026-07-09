# 搭建环境（贡献生态）

本课带你装好全部工具、克隆带子模块的monorepo，并以一次成功的编译收尾。
请逐步执行，每一步都有可见的结果。

> 本课只求"把工具跑起来"。每个工具的深入安装与故障排查见
> [安装官方 Rust 工具链](../../how-to/01-install-toolchain.md)。

## 第 0 步：安装 Rust（rustup）

下面的工具链安装、子模块克隆、编译都依赖 `rustup` / `cargo`。如果还没装，按 Rust 官方
指引装一下（一条命令、跨平台）：

{{#tutorial-snippet contrib_setup_rustup}}

> 官方安装页（含 Windows / 其它方式）：<https://www.rust-lang.org/tools/install>。
> 装完确认：`rustup --version` 和 `cargo --version` 都能打印版本即可。

## 第 1 步：安装官方 Rust nightly

WS63 应用核是 `riscv32imfc-unknown-none-elf`（硬件单精度浮点、无原子扩展）。
这个 target 已经进入官方 `rustc` nightly；当前 rustup 还没有它的预编译
`rust-std` 组件，所以需要安装 `rust-src`，由仓库的 RISC-V 构建命令使用
`-Zbuild-std=core,alloc`。

{{#tutorial-snippet contrib_setup_toolchain}}

确认 target 和 rustup 组件状态：

{{#tutorial-snippet contrib_setup_check_toolchain}}

第一行应能看到 `riscv32imfc-unknown-none-elf`。第二行现在通常会提示还没有预编译
`rust-std`，这是预期状态。

## 第 2 步：克隆仓库（带子模块）

示例、HAL、PAC、运行时都是子模块，务必带 `--recurse-submodules` 克隆：

{{#tutorial-snippet contrib_clone_repo}}

> 如果你已经克隆但忘了子模块，补一句：`git submodule update --init --recursive`。

仓库根目录的 `rust-toolchain.toml` 已经把频道钉成了验证过的官方 nightly。
RISC-V 构建命令需要带 `-Zbuild-std=core,alloc`；教程里的命令片段和 CI 都覆盖这一点。

## 第 3 步：安装 QEMU 模拟器

第 2、3 课要用 [`hisi-riscv-qemu`](https://github.com/hispark-rs/hisi-riscv-qemu)——
一个带 WS63 机器模型（`-M ws63`）的 QEMU 分支。在仓库**同级**目录里克隆并构建它：

{{#tutorial-snippet contrib_setup_qemu}}

构建完成后，确认 `qemu-system-riscv32` 可用并支持 `ws63` 机器：

{{#tutorial-snippet contrib_setup_check_qemu}}

你应当看到 `ws63` 出现在机器列表中。把这个二进制加入 `PATH`，
或记下它的路径——第 2 课会用到。详细步骤见
[QEMU 模型](../../explanation/06-qemu-model.md)。

## 第 4 步：安装烧录工具（真机用）

第 3 课要烧到真板，需要两个工具：

- [`hisi-fwpkg`](https://github.com/hispark-rs/hisi-fwpkg)：把 ELF 打包成可启动镜像（加 `0x300` 启动头）。
- **打过补丁的 probe-rs 分支**（`hispark-rs/probe-rs`，分支 `add-hisilicon-ws63-bs21`）：
  上游 probe-rs 不认识 WS63，必须用这个分支，并配上 `HiSilicon_WS63.yaml`。

安装方法（深入说明见 [安装官方 Rust 工具链](../../how-to/01-install-toolchain.md) 与
[用 probe-rs 烧录到真机](../../how-to/04-flash-probe-rs.md)）：

{{#tutorial-snippet contrib_install_flash_tools}}

确认两者就位：

{{#tutorial-snippet contrib_check_flash_tools}}

> 第 2 课只用 QEMU，可以暂时跳过本步；等到第 3 课要烧真机时再装也行。

## 第 5 步：验证你的环境

回到仓库根目录，编译 blinky 示例——这是检验工具链是否就绪的最快办法：

{{#tutorial-snippet contrib_build_blinky}}

第一次编译会拉取依赖、编译 HAL，需要几分钟。结束时你应当看到类似：

```console
    Finished `release` profile [optimized + debuginfo] target(s) in ...
```

产物在这里：

{{#tutorial-snippet contrib_ls_blinky}}

看到这个文件，就说明工具链、目标、仓库都配好了。

> 编译过程中会有一些 `.weak StorePageFault` 之类的汇编 warning，这是正常的，可以忽略。

环境就绪！下一课我们构建并运行完整的示例集 ——
[构建与运行示例集](02-examples.md)。
