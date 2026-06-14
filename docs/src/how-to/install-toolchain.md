# 如何安装 hisi-riscv 工具链

WS63 应用核是 `RV32IMFC_Zicsr`：**硬件单精度浮点（ilp32f）、没有原子（'a'）扩展**。仓库用一套自定义的 `hisi-riscv` 工具链来构建——一套 stable rustc（1.96.0），把 target `riscv32imfc-unknown-none-elf` 作为 **builtin** 烤进去，并随附预编译的 `core`/`alloc`，因此**不需要 `-Z build-std`**。它不是可分发的 rustup channel，必须先手动安装并 `link`。

工具链的来历、ABI 取舍见[硬浮点工具链](../explanation/hardfloat-toolchain.md)；target 命名细节见[工具链与编译目标](../reference/toolchain.md)。

## 方式一：下载预编译 tarball（推荐）

发布页 <https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases> 为每个 host 提供 tarball（Linux x86_64/aarch64、macOS x86_64/aarch64、Windows x86_64）。挑你 host 对应的那个：

```bash
# 以 Linux x86_64 为例（换成你 host 对应的文件名）
curl -fLO https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/download/v1.96.0-2/hisi-riscv-rust-1.96.0-x86_64-unknown-linux-gnu.tar.gz
tar xzf hisi-riscv-rust-1.96.0-*.tar.gz

# 把解压出来的 stage2/ 作为名为 hisi-riscv 的 rustup 工具链链接进去
rustup toolchain link hisi-riscv "$PWD/stage2"
```

`rustup toolchain link` 只是把 `hisi-riscv` 这个名字指向 `stage2/` 目录，**不会拷贝**——所以别在 `link` 之后删/移动这个目录（要换位置就重新 `link`）。

## 方式二：从源码构建

源码与构建配方都在工具链仓库 <https://github.com/hispark-rs/hisi-riscv-rust-toolchain>（它本质是带 WS63 target spec 的 rustc 分支 + 一份 `config.toml`）。构建很重（需要完整的 rustc bootstrap，几十分钟到数小时、十几 GB 磁盘），照仓库 README 跑 `x.py build` 即可。构建产物同样是一个 `stage2/`，照方式一末尾 `rustup toolchain link` 进去。

> 大多数人不需要从源码构建——只有你要改 target spec / 编译器本身时才需要。

## 验证

确认 target 已 builtin（这是关键——没有它说明链接的工具链不对）：

```bash
rustc +hisi-riscv --print target-list | grep riscv32imfc
# 期望输出： riscv32imfc-unknown-none-elf
```

确认 `core` 预编译可用（直接试构建，下一篇[如何构建一个示例](build-example.md)）：

```bash
rustc +hisi-riscv --version          # 应打印 1.96.0 系列版本
```

## rust-toolchain.toml 会自动选它

仓库根的 `rust-toolchain.toml` 写着：

```toml
[toolchain]
channel = "hisi-riscv"
```

只要你在仓库目录里跑 `cargo`，rustup 就会**自动**用 `hisi-riscv` 工具链，不用每条命令都加 `+hisi-riscv`。换句话说：**链接好之后，在仓库里普通 `cargo build` 就走对了。** 默认 target 由 `.cargo/config.toml` 的 `target = "riscv32imfc-unknown-none-elf"` 指定。

## 排错

- **`error: toolchain 'hisi-riscv' is not installed`**：还没 `rustup toolchain link`，或 `stage2/` 被移动/删除了——重新 link。
- **`error: target 'riscv32imfc-unknown-none-elf' not found` / 触发 build-std**：你用的是普通 stable 而不是 `hisi-riscv`。检查 `rustc +hisi-riscv --print target-list | grep riscv32imfc` 是否有输出；在仓库外构建时记得 `cargo +hisi-riscv ...` 或带上 `rust-toolchain.toml`。
- **下错 host tarball**（如在 aarch64 上用了 x86_64 包）：`rustc` 跑不起来。按 `uname -m` 重新挑文件名。
