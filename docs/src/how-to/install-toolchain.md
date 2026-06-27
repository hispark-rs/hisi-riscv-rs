# 如何安装 hisi-riscv 工具链

WS63 应用核是 `RV32IMFC_Zicsr`：**硬件单精度浮点（ilp32f）、没有原子（'a'）扩展**。仓库用一套自定义的 `hisi-riscv` 工具链来构建——一套 stable rustc（1.96.0），把 target `riscv32imfc-unknown-none-elf` 作为 **builtin** 烤进去，并随附预编译的 `core`/`alloc`，因此**不需要 `-Z build-std`**。它不是可分发的 rustup channel，必须先手动安装并 `link`。

工具链的来历、ABI 取舍见[硬浮点工具链](../explanation/hardfloat-toolchain.md)；target 命名细节见[工具链与编译目标](../reference/toolchain.md)。

## 方式一：下载预编译 tarball（推荐）

发布页 <https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases> 为每个 host 提供 tarball（Linux x86_64/aarch64、macOS x86_64/aarch64、Windows x86_64）。挑你 host 对应的那个，**直接解压进 rustup 的 toolchains 目录**：

```bash
# 以 Linux x86_64 为例（换成你 host 对应的文件名）
curl -fLO https://github.com/hispark-rs/hisi-riscv-rust-toolchain/releases/download/v1.96.0-2/hisi-riscv-rust-1.96.0-x86_64-unknown-linux-gnu.tar.gz

# 解压进 ~/.rustup/toolchains/hisi-riscv/。tarball 顶层是 stage2/，用 --strip-components=1
# 把它剥掉，让 bin/lib/libexec 直接落到 hisi-riscv/ 根下。
mkdir -p ~/.rustup/toolchains/hisi-riscv
tar xzf hisi-riscv-rust-1.96.0-*.tar.gz --strip-components=1 -C ~/.rustup/toolchains/hisi-riscv
```

rustup 会自动把 `~/.rustup/toolchains/` 下的目录识别成工具链，**无需 `rustup toolchain link`**。这样装好的工具链是**自包含**的——不依赖任何外部的 `stage2/` 目录，删掉下载和解压的临时文件也不影响。

> 若你自定义过 `RUSTUP_HOME`，把上面的 `~/.rustup` 换成它（`rustup show home` 可查实际路径）。

## 方式二：从源码构建

源码与构建配方都在工具链仓库 <https://github.com/hispark-rs/hisi-riscv-rust-toolchain>（它本质是带 WS63 target spec 的 rustc 分支 + 一份 `config.toml`）。构建很重（需要完整的 rustc bootstrap，几十分钟到数小时、十几 GB 磁盘），照仓库 README 跑 `x.py build` 即可。构建产物同样是一个 `stage2/`——把它的内容拷进 `~/.rustup/toolchains/hisi-riscv/`（`cp -a path/to/stage2/. ~/.rustup/toolchains/hisi-riscv/`），或用 `rustup toolchain link hisi-riscv path/to/stage2` 指过去（源码构建时 `stage2/` 就在本地，link 不拷贝、更省空间）。

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

- **`error: toolchain 'hisi-riscv' is not installed`**：`~/.rustup/toolchains/hisi-riscv/` 不存在或为空（解压时 `-C` 路径写错、`--strip-components` 漏了导致多套了一层 `stage2/`）——重新解压。检查 `ls ~/.rustup/toolchains/hisi-riscv/bin/rustc` 是否在。
- **`error: target 'riscv32imfc-unknown-none-elf' not found` / 触发 build-std**：你用的是普通 stable 而不是 `hisi-riscv`。检查 `rustc +hisi-riscv --print target-list | grep riscv32imfc` 是否有输出；在仓库外构建时记得 `cargo +hisi-riscv ...` 或带上 `rust-toolchain.toml`。
- **下错 host tarball**（如在 aarch64 上用了 x86_64 包）：`rustc` 跑不起来。按 `uname -m` 重新挑文件名。

## IDE / rust-analyzer 已知问题

`cargo build` / `cargo check` 一切正常，但编辑器里 rust-analyzer 可能冒出**大量误报**。
这些都不是代码问题，而是自定义工具链 + 嵌入式 target + 多 workspace 的组合给 RA 挖的坑。
逐条对症即可（命令以 macOS + stable 工具链为例，按你的主机/路径调整）。

### 1. 海量 `unresolved macro println!` / `no method len on [u8; N]` 等

**症状**：连 `println!`、`core::arch::asm!` 这种标准库宏，以及 `[u8; N]`/`u32` 的原始方法
（`len`/`iter`/`wrapping_add`）、slice unsize 强转都报错。

**根因**：预编译 tarball 里 sysroot 的 rust-src 软链是**悬空的**，指向打包用的 CI 构建机
绝对路径（`/Users/runner/work/...`）。RA 加载不到 `core`/`std` 源码，于是把一切基础设施
判成"未知"。（已反馈上游：<https://github.com/hispark-rs/hisi-riscv-rust-toolchain/issues/1>）

**修复**：把该软链重指向本机已装的某个 `rust-src`（版本接近即可，RA 容忍小版本差）：

```bash
rustup component add rust-src --toolchain stable
ln -sfn "$(rustc +stable --print sysroot)/lib/rustlib/src/rust" \
        "$(rustc +hisi-riscv --print sysroot)/lib/rustlib/src/rust"
```

> 重装 `hisi-riscv` 工具链后 tarball 会带回悬空软链，需重做这一步。

### 2. RA 启动即报 `'rust-analyzer' is not installed for toolchain 'hisi-riscv'`

`hisi-riscv` 是自定义工具链，装不了 rust-analyzer 组件；而 `rust-toolchain.toml` 把频道钉成
了它，导致 rustup 的 `rust-analyzer` 代理被路由过去后报错。

**修复**：让编辑器用**另一个工具链的** RA 二进制（不设 `RUSTUP_TOOLCHAIN`）——RA 分析时仍会
按 `rust-toolchain.toml` 调 `hisi-riscv` 的 `cargo`/`rustc`，嵌入式 target 不受影响。例如指向
`~/.rustup/toolchains/stable-*/bin/rust-analyzer`（先 `rustup component add rust-analyzer --toolchain stable`）。

### 3. `cannot find io_config in pac` `E0433` 等 chip 相关误报

**根因**：HAL 的 `chip-ws63` / `chip-bs21` 是**恰选其一**的互斥 feature。若 RA 开了
`cargo.allFeatures`（如 LazyVim 的 rust 扩展默认 `allFeatures=true`），两个 chip 会被同时打开，
ws63 专属文件被按 bs2x-pac 解析而报错。

**修复**：设 `rust-analyzer.cargo.allFeatures = false`（这本就是 RA / VS Code 的原生默认值）。

### 4. `can't find crate for test` `E0463`

**根因**：RA 默认 `--all-targets` 会构建每个 crate 的 test 目标，而裸机 target
`riscv32imfc-unknown-none-elf` 没有 `test` crate（no_std 无测试 harness）。

**修复**：设 `rust-analyzer.cargo.allTargets = false` 与 `rust-analyzer.check.allTargets = false`。

### 关于第 3、4 条的工程内配置

仓库已提交 `rust-analyzer.toml`（根 + `examples/bs20` + `examples/bs21` 各一份），把
`allFeatures` / `allTargets` 都设成 `false`。**VS Code 等会直接读取它**，开箱即用。

> ⚠️ 优先级坑：rust-analyzer 里**编辑器 client 配置优先级高于仓库 `rust-analyzer.toml`**。
> 若你的编辑器/插件（如 LazyVim）在 client 级强设了 `cargo.allFeatures=true`，会把仓库 ratoml
> 压住——此时需在你**编辑器自己的 RA 配置**里把上面第 3、4 条设回去（client 级覆盖）。
