# 如何安装官方 Rust 工具链

WS63 / BS2X 应用核使用 `RV32IMFC_Zicsr`：**硬件单精度浮点（ilp32f）、没有原子（`a`）扩展**。当前仓库使用官方 upstream Rust nightly：`rustc` 已内置 `riscv32imfc-unknown-none-elf` target。

注意边界：这个 target 现在能被 `rustc --print target-list` 看见，但 rustup 还没有提供对应的预编译 `rust-std` 组件。因此不要写 `rustup target add riscv32imfc-unknown-none-elf`；现在的正确路径是安装 pinned nightly + `rust-src`，构建时使用 `-Zbuild-std=core,alloc`。

工具链事实见[工具链与编译目标](../reference/05-toolchain.md)；硬浮点和上游化背景见[硬浮点工具链](../explanation/03-hardfloat-toolchain.md)。

## 安装 pinned nightly

仓库根的 `rust-toolchain.toml` 已经 pin 住当前验证过的 nightly。你可以让 rustup 按该文件自动安装，也可以显式安装：

```bash
rustup toolchain install nightly-2026-07-09 \
    --profile minimal \
    --component rust-src \
    --component clippy \
    --component rustfmt \
    --component llvm-tools-preview
```

`rust-src` 用于 `-Zbuild-std=core,alloc`；`llvm-tools-preview` 提供 `rust-objcopy` / `rust-size` 等固件打包和 size report 工具。

## 验证 target

确认 `rustc` 已经认识这个官方 target：

```bash
rustc --print target-list | grep riscv32imfc
```

预期能看到：

```text
riscv32imfc-unknown-none-elf
```

再确认 rustup 组件状态。现在通常没有输出，这是预期状态：

```bash
rustup target list --toolchain nightly-2026-07-09 | grep riscv32imfc || \
    echo "rustup has no prebuilt rust-std yet; use -Zbuild-std=core,alloc"
```

## 构建方式

RISC-V 构建命令需要带 build-std：

```bash
cargo build -Zbuild-std=core,alloc --release
cargo check -Zbuild-std=core,alloc --workspace --exclude wifi_softap --features hisi-rf/chip-ws63
cargo check -Zbuild-std=core,alloc -p wifi_softap
cargo clippy -Zbuild-std=core,alloc --workspace --exclude wifi_softap --features hisi-rf/chip-ws63 -- -D warnings
cargo clippy -Zbuild-std=core,alloc -p wifi_softap -- -D warnings
```

SoftAP 与 STA 使用互斥的 target archive，所以分成两次命令是构建契约的一部分，
不是临时绕过。

`hisi-rs-template` 生成的新项目已经把这个细节封装进 `just build` / `just run` / `just image`，所以应用开发者通常不需要手写 `-Zbuild-std=core,alloc`。

## 排错

- **`can't find crate for core`**：命令缺少 `-Zbuild-std=core,alloc`，或 toolchain 没装 `rust-src`。先重跑上面的 `rustup toolchain install ... --component rust-src`。
- **`target may not be installed`**：不要对这个 target 跑 `rustup target add`；当前 rustup 还没有预编译 std 组件。确认你在仓库里使用 pinned nightly，并给 RISC-V 命令加 build-std。
- **`the option Z is only accepted on the nightly compiler`**：当前命令没有使用 `rust-toolchain.toml` 里的 nightly。检查 `rustup show active-toolchain`。
- **找不到 `rust-objcopy` / `rust-size`**：安装 `llvm-tools-preview` 组件，或重跑本页的安装命令。

## 历史说明

旧的 `hispark-rs/hisi-riscv-rust-toolchain` 曾发布自定义 rustc tarball，把该 target 和预编译 `core`/`alloc` 一起打包。现在它不再是 happy path；生态主线改用官方 nightly。该仓库后续作为官方工具链外部 radar，跟踪 latest nightly、rustup 组件状态和 Tier-2 readiness。
