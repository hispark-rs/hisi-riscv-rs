# 搭建开发环境

本课带你装好全部工具，并以一次成功的编译收尾。请逐步执行，每一步都有可见的结果。

> 本课只求"把工具跑起来"。每个工具的深入安装与故障排查见
> [安装 hisi-riscv 工具链](../how-to/install-toolchain.md)。

## 第 1 步：安装 hisi-riscv 工具链

WS63 应用核是 `riscv32imfc-unknown-none-elf`（硬件单精度浮点、无原子扩展）。
这个目标被内建进了一个自定义的 `hisi-riscv` 工具链——它**不是** rustup 频道，需要手动下载并链接。

下载与你主机匹配的压缩包（这里以 x86_64 Linux 为例），解压，然后链接：

```bash
curl -LO https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/download/v1.96.0-2/hisi-riscv-rust-1.96.0-x86_64-unknown-linux-gnu.tar.gz
tar xzf hisi-riscv-rust-1.96.0-*.tar.gz
rustup toolchain link hisi-riscv "$PWD/stage2"
```

确认链接成功：

```bash
rustup toolchain list | grep hisi-riscv
```

你应当看到：

```
hisi-riscv
```

## 第 2 步：克隆仓库（带子模块）

示例、HAL、PAC、运行时都是子模块，务必带 `--recurse-submodules` 克隆：

```bash
git clone --recurse-submodules https://github.com/hispark-rs/hisi-riscv-rs.git
cd hisi-riscv-rs
```

> 如果你已经克隆但忘了子模块，补一句：`git submodule update --init --recursive`。

仓库根目录的 `rust-toolchain.toml` 已经把频道钉成了 `hisi-riscv`，
所以在本仓库内执行的所有 `cargo` 命令都会自动用上刚装好的工具链。

## 第 3 步：安装 QEMU 模拟器

第 2、3、4 课要用 [`hisi-riscv-qemu`](https://github.com/hispark-rs/hisi-riscv-qemu)——
一个带 WS63 机器模型（`-M ws63`）的 QEMU 分支。在仓库**同级**目录里克隆并构建它：

```bash
cd ..
git clone https://github.com/hispark-rs/hisi-riscv-qemu.git
cd hisi-riscv-qemu
bash scripts/build.sh
```

构建完成后，确认 `qemu-system-riscv32` 可用并支持 `ws63` 机器：

```bash
./build/qemu-system-riscv32 -M help | grep ws63
```

你应当看到 `ws63` 出现在机器列表中。把这个二进制加入 `PATH`，
或记下它的路径——第 2 课会用到。详细步骤见
[QEMU 模型](../explanation/qemu-model.md)。

## 第 4 步：安装烧录工具（真机用）

第 2、5 课要烧到真板，需要两个工具：

- [`hisi-fwpkg`](https://github.com/hispark-rs/hisi-fwpkg)：把 ELF 打包成可启动镜像（加 `0x300` 启动头）。
- **打过补丁的 probe-rs 分支**（`hispark-rs/probe-rs`，分支 `add-hisilicon-ws63-bs21`）：
  上游 probe-rs 不认识 WS63，必须用这个分支，并配上 `HiSilicon_WS63.yaml`。

安装方法（深入说明见 [安装工具链](../how-to/install-toolchain.md) 与
[用 probe-rs 烧录到真机](../how-to/flash-probe-rs.md)）：

```bash
# hisi-fwpkg
cargo install --git https://github.com/hispark-rs/hisi-fwpkg

# 打过补丁的 probe-rs 分支
cargo install --git https://github.com/hispark-rs/probe-rs --branch add-hisilicon-ws63-bs21 probe-rs-tools
```

确认两者就位：

```bash
hisi-fwpkg --help
probe-rs --version
```

> 第 3、4 课只用 QEMU，可以暂时跳过本步；等到第 2 课要烧真机时再装也行。

## 第 5 步：验证你的环境

回到仓库根目录，编译 blinky 示例——这是检验工具链是否就绪的最快办法：

```bash
cd ../hisi-riscv-rs
cargo build -p blinky --release
```

第一次编译会拉取依赖、编译 HAL，需要几分钟。结束时你应当看到类似：

```
    Finished `release` profile [optimized + debuginfo] target(s) in ...
```

产物在这里：

```bash
ls target/riscv32imfc-unknown-none-elf/release/blinky
```

看到这个文件，就说明工具链、目标、仓库都配好了。

> 编译过程中会有一些 `.weak StorePageFault` 之类的汇编 warning，这是正常的，可以忽略。

环境就绪！下一课我们就让这个 blinky 真正跑起来 ——
[点亮第一个 LED（blinky）](02-blinky.md)。
