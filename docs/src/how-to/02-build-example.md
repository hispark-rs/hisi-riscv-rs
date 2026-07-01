# 如何构建一个示例

仓库带了一批 WS63 示例（`blinky`、`uart_hello`、`timer_irq`、`spi_loopback`、`i2c_scan`、`async_delay`、`embassy_multitask`…，完整清单见[示例目录与验证标记串](../reference/02-examples.md)）。本篇讲怎么把它们编出来。

> 前提：已[安装 hisi-riscv 工具链](01-install-toolchain.md)。在仓库目录里 `rust-toolchain.toml` 会自动选它，默认 target 是 `riscv32imfc-unknown-none-elf`，无需 `+hisi-riscv` 或 `--target`。

## 从仓库根工作区构建（推荐）

**在仓库根目录**用 `-p <包名>` 构建任意示例：

```bash
cargo build -p blinky --release
cargo build -p uart_hello --release
cargo build -p spi_loopback --release
```

包名就是 crate 名（见根 `Cargo.toml` 的 `members`），和它在磁盘上的 `examples/ws63/<name>` 路径无关。一次构建全部库 + 默认示例：

```bash
cargo build --release        # 构建 default-members（库 + 全部 ws63 示例）
```

> **坑：别在 `examples/ws63/` 里构建全部示例。** `examples/ws63/` 自带一个**嵌套工作区**，但它的 `members` 只列了 `blinky` 一个。所以在那个目录里 `cargo build -p timer_irq` 会失败（不是它的成员）。**从仓库根构建**，根工作区才把全部示例列全。

## ELF 落点

根工作区共用根 `target/`，release ELF 在：

```console
target/riscv32imfc-unknown-none-elf/release/<name>
```

例如 `target/riscv32imfc-unknown-none-elf/release/blinky`。注意是**无扩展名**的 ELF（cargo 按 `[[bin]]`/crate 名命名产物）。

> 如果你是在 `examples/ws63/` 嵌套工作区里单独构建（只有 `blinky`），它的产物在 `examples/ws63/target/riscv32imfc-unknown-none-elf/release/`——`hil/` 里的脚本默认就找这个目录。两个 `target/` 不要混。

## `--release` vs debug

- **`--release`**：默认就用它。优化后体积小（blinky 约 48 KB），是真机/HIL 烧录用的产物。
- **debug（去掉 `--release`）**：体积大很多、有完整调试信息，适合 GDB 调试，但**可能因为体积/布局**在受限的 app 分区里不合适。烧真机一律用 `--release`。

## objcopy 成裸 bin

打包成可启动镜像时 `hisi-fwpkg` 直接吃 ELF（见[如何打包镜像](03-package-image.md)），**通常不需要**手动 objcopy。但若某条工具链确实要裸 bin，用工具链自带的 `rust-objcopy`：

```bash
OBJCOPY="$(rustc +hisi-riscv --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-objcopy"
"$OBJCOPY" -O binary \
    target/riscv32imfc-unknown-none-elf/release/blinky \
    blinky.bin
```

（host 三元组那段按你的 host 改；`hil/flash.sh` 的 hisiflash 路径就是这么从 ELF 生成 bin 的。）

## 下一步

- 打包成可启动镜像 → [如何打包成可启动镜像](03-package-image.md)
- 烧到真机 → [用 probe-rs](04-flash-probe-rs.md) / [用 hisiflash](05-flash-hisiflash.md)
- 在 QEMU 里跑 → 见教程 [在 QEMU 里运行与调试](../tutorials/contrib/02-examples.md)
