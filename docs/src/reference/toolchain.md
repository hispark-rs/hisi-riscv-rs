# 工具链与编译目标

WS63 用自定义 `hisi-riscv` 工具链构建。事实取自 [`rust-toolchain.toml`](https://github.com/hispark-rs/hisi-riscv-rs)、[`.cargo/config.toml`](https://github.com/hispark-rs/hisi-riscv-rs)、[`CLAUDE.md`](https://github.com/hispark-rs/hisi-riscv-rs)。

安装步骤见 [安装 hisi-riscv 工具链](../how-to/install-toolchain.md)；硬浮点选型原理见 [硬浮点工具链](../explanation/hardfloat-toolchain.md)。

## 工具链 / 目标速查

| 项 | 值 |
|----|----|
| rustup 通道名 | `hisi-riscv` |
| 基础 rustc 版本 | stable 1.96.0 |
| 默认目标三元组 | `riscv32imfc-unknown-none-elf` |
| ISA | RV32IMFC_Zicsr |
| 浮点 | 硬件单精度，ABI `ilp32f` |
| 原子 | **无** `a` 扩展（forced-atomics + no-CAS） |
| build-std | **不需要**（目标为 builtin，工具链自带预编译 core/alloc） |
| 工具链仓库 | github.com/hispark-rs/hisi-riscv-rust-toolchain |
| 当前发布 | `v1.96.0-2` |

> 目标三元组写法 `riscv32imfc`（注意是 `imfc`，含硬浮点 `f`、不含原子 `a`）。`CLAUDE.md` 中出现的 `riscv32imafc-unknown-none-elf` 是 `--target` 覆盖示例，并非默认目标。

## builtin 目标（无需 `-Z build-std`）

`riscv32imfc-unknown-none-elf` 在 `hisi-riscv` 工具链里被烤进为 **builtin** 目标，工具链随附该目标的预编译 `core`/`alloc`，故构建不需要 nightly 的 `-Z build-std`。工具链是芯片中立的（ws63 + bs2x 都目标 riscv32imfc）。

无原子的处理：原子 load/store 降为普通 `ld`/`st`（单 hart）；RMW 经 `portable-atomic` 的 `critical-section` polyfill。不发射 `lr.w`/`sc.w`/`amo*`。

## `rust-toolchain.toml`

```toml
[toolchain]
channel = "hisi-riscv"
```

> 该工具链不是可分发的 rustup channel，必须先手动安装并 `rustup toolchain link hisi-riscv`（见下"安装"）。

## `.cargo/config.toml`

```toml
[build]
target = "riscv32imfc-unknown-none-elf"

[target.riscv32imfc-unknown-none-elf]
runner = "gdb-multiarch"
rustflags = ["-C", "link-arg=--no-relax"]
```

| 字段 | 值 | 说明 |
|------|----|----|
| `[build] target` | `riscv32imfc-unknown-none-elf` | 默认编译目标 |
| `runner` | `gdb-multiarch` | `cargo run` 默认 runner（QEMU/真机 runner 经 env 覆盖，见 [硬件 runner](../how-to/hardware-runner.md)） |
| `rustflags` | `-C link-arg=--no-relax` | 关闭 RISC-V 链接器松弛，匹配厂商 C SDK 流，避免 gp 相对松弛与自定义链接脚本冲突 |

## 安装（release URL 形态）

按主机选 tarball（linux x86_64/aarch64、macOS x86_64/aarch64、windows x86_64）：

```bash
curl -fLO https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/download/v1.96.0-2/hisi-riscv-rust-1.96.0-x86_64-unknown-linux-gnu.tar.gz
tar xzf hisi-riscv-rust-1.96.0-*.tar.gz
rustup toolchain link hisi-riscv "$PWD/stage2"
```

release URL 形态：

```
https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/download/<tag>/hisi-riscv-rust-1.96.0-<host-triple>.tar.gz
```

当前 `<tag>` = `v1.96.0-2`；`<host-triple>` 如 `x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu`、`x86_64-apple-darwin`、`aarch64-apple-darwin`、`x86_64-pc-windows-msvc`。链接目标为解压后的 `stage2` 目录。
