# 搭建环境（应用开发）

本课带你装好开发 WS63 应用所需的全部工具。注意：**你不需要克隆monorepo**——
所有库依赖都来自 crates.io，工程将在下一课用 `cargo generate` 生成。

> 本课只求"把工具装上"。每个工具的深入说明与故障排查（含 **IDE / rust-analyzer
> 已知问题**）见 [安装 hisi-riscv 工具链](../../how-to/install-toolchain.md)。

## 第 0 步：安装 Rust（rustup）

后面所有步骤都依赖 `rustup` / `cargo`。如果你机器上还没有，按 Rust 官方指引装一下
（一条命令、跨平台）：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

> 官方安装页（含 Windows / 其它方式）：<https://www.rust-lang.org/tools/install>。
> 装完确认：`rustup --version` 和 `cargo --version` 都能打印版本即可。

## 第 1 步：安装 hisi-riscv 工具链

WS63 应用核是 `riscv32imfc-unknown-none-elf`（硬件单精度浮点、无原子扩展）。
这个目标被内建进了一个自定义的 `hisi-riscv` 工具链——它**不是** rustup 频道，需要手动下载并链接。

下载与你主机匹配的压缩包（这里以 x86_64 Linux 为例），**直接解压进 rustup 的 toolchains 目录**——rustup 会自动识别，无需 `link`：

```bash
HOST=x86_64-unknown-linux-gnu
curl -LO https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/latest/download/hisi-riscv-rust-1.96.0-$HOST.tar.gz
mkdir -p ~/.rustup/toolchains/hisi-riscv
tar xzf hisi-riscv-rust-1.96.0-$HOST.tar.gz --strip-components=1 -C ~/.rustup/toolchains/hisi-riscv
```

> tarball 顶层是 `stage2/`，`--strip-components=1` 把它剥掉，让 `bin/lib/libexec`
> 落到 `hisi-riscv/` 根下。这样工具链自包含，删掉下载的临时文件也不影响。
>
> 其它主机把 `HOST` 换成对应三元组即可：`aarch64-unknown-linux-gnu`、
> `aarch64-apple-darwin`、`x86_64-pc-windows-msvc`。

确认链接成功：

```bash
rustup toolchain list | grep hisi-riscv
```

你应当看到：

```console
hisi-riscv
```

## 第 2 步：安装 cargo-generate 与 just

下一课用 `cargo generate` 从模板生成工程；生成出来的工程用 `just` 跑各种命令：

```bash
cargo install cargo-generate just
```

## 第 3 步：安装打包工具 hisi-fwpkg（烧真机用）

烧到真板时，flashboot 期望一个带 `0x300` 启动头的应用镜像，`hisi-fwpkg` 负责打包：

```bash
cargo install --git https://github.com/hispark-rs/hisi-fwpkg
```

> 也可以克隆 [github.com/hispark-rs/hisi-fwpkg](https://github.com/hispark-rs/hisi-fwpkg)
> 自行构建。确认就位：`hisi-fwpkg --help`。

## 第 4 步：安装打过补丁的 probe-rs 分支（烧真机用）

上游 probe-rs 不认识 WS63，必须用打过补丁的分支，并配上它自带的
`HiSilicon_WS63.yaml` 芯片描述文件：

```bash
cargo install --git https://github.com/hispark-rs/probe-rs \
    --branch add-hisilicon-ws63-bs21 probe-rs-tools
```

确认就位：`probe-rs --version`。深入说明见
[用 probe-rs 烧录到真机](../../how-to/flash-probe-rs.md)。

> 只想在 QEMU 里跑、暂时不烧真机，可以先跳过第 3、4 步。

## 第 5 步：安装 QEMU（可选，`just run` 用）

想用 `just run` 在模拟器里跑，需要 [`hisi-riscv-qemu`](https://github.com/hispark-rs/hisi-riscv-qemu)——
一个带 `-M ws63` 机器模型的 QEMU 分支。克隆并构建，把它的 `qemu-system-riscv32` 放进 `PATH`：

```bash
git clone https://github.com/hispark-rs/hisi-riscv-qemu && cd hisi-riscv-qemu
./scripts/build.sh
```

确认 `ws63` 机器可用：

```bash
qemu-system-riscv32 -M help | grep ws63
```

## 第 6 步：验证工具链

`hisi-riscv` 工具链内建了 WS63 目标，确认它在目标列表里：

```bash
rustc +hisi-riscv --print target-list | grep riscv32imfc
```

你应当看到：

```console
riscv32imfc-unknown-none-elf
```

> 看到这一行就说明工具链装好了。下一课生成的工程里有 `rust-toolchain.toml`，
> 会自动选用 `hisi-riscv`，所以在工程目录里直接敲 `cargo` 即可，无需 `+hisi-riscv`。

工具齐了！下一课我们生成你的第一个工程 ——
[从模板创建你的第一个工程](02-first-project.md)。
