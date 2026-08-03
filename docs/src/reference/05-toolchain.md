# 工具链与编译目标

本页记录当前构建事实。安装步骤见[安装官方 Rust 工具链](../how-to/01-install-toolchain.md)；为什么选择硬浮点 target 见[硬浮点工具链](../explanation/03-hardfloat-toolchain.md)。

## 工具链 / 目标速查

| 项 | 值 |
|----|----|
| rustup 通道 | `nightly-2026-07-09` |
| rustc 版本 | `1.99.0-nightly` 系列 |
| 默认目标三元组 | `riscv32imfc-unknown-none-elf` |
| ISA | RV32IMFC_Zicsr |
| 浮点 | 硬件单精度，ABI `ilp32f` |
| 原子 | **无** `a` 扩展 |
| rustup 预编译 std | 当前没有 |
| build-std | 当前需要 `-Zbuild-std=core,alloc` |
| 上游状态 | rustc 已内置 target；rustup 组件与 Tier-2 readiness 由 radar 跟踪 |
| radar 仓库 | github.com/hispark-rs/hisi-riscv-rust-toolchain |

目标三元组写法是 `riscv32imfc`：含硬浮点 `f`，不含原子 `a`。

## `rust-toolchain.toml`

```toml
[toolchain]
channel = "nightly-2026-07-09"
profile = "minimal"
components = ["rust-src", "clippy", "rustfmt", "llvm-tools-preview"]
```

该 nightly 的 `rustc --print target-list` 已包含 `riscv32imfc-unknown-none-elf`。但 `rustup target list --toolchain nightly-2026-07-09` 目前还没有这个 target 的预编译 `rust-std`，所以不能用 `rustup target add riscv32imfc-unknown-none-elf` 作为安装步骤。

## `.cargo/config.toml`

```toml
[build]
target = "riscv32imfc-unknown-none-elf"

[target.riscv32imfc-unknown-none-elf]
runner = ".cargo/run-riscv.sh"
rustflags = ["-C", "link-arg=--no-relax"]
```

| 字段 | 值 | 说明 |
|------|----|----|
| `[build] target` | `riscv32imfc-unknown-none-elf` | 默认编译目标 |
| `runner` | `.cargo/run-riscv.sh` | 默认 QEMU runner；真机 runner 经 env 覆盖，见[硬件 runner](../how-to/06-hardware-runner.md) |
| `rustflags` | `-C link-arg=--no-relax` | 关闭 RISC-V 链接器松弛，匹配厂商 C SDK 流，避免 gp 相对松弛与链接脚本冲突 |

## RISC-V 构建命令

因为 std 组件尚未由 rustup 分发，RISC-V 命令必须显式构建 `core` / `alloc`：

```bash
cargo check -Zbuild-std=core,alloc --workspace --exclude wifi_softap --features hisi-rf/chip-ws63
cargo check -Zbuild-std=core,alloc -p wifi_softap
cargo build -Zbuild-std=core,alloc --release
cargo clippy -Zbuild-std=core,alloc --workspace --exclude wifi_softap --features hisi-rf/chip-ws63 -- -D warnings
cargo clippy -Zbuild-std=core,alloc -p wifi_softap -- -D warnings
```

`wifi_softap` 与 STA 固件选择互斥的 target archive。Cargo 会在单次命令中合并
workspace features，因此 AP 必须作为独立固件角色检查；不能通过关闭
`ws63-radio-sys` 的互斥门禁来伪造“全 workspace 同时可构建”。

不要把 build-std 写成全局 Cargo 配置。host unit tests 需要在 `stable` + `x86_64-unknown-linux-gnu` 上运行；全局 `[unstable] build-std` 会污染 host 目标。

## 原子与 critical-section

WS63 当前路径是单 hart + 无 A 扩展：目标不得发射 `lr/sc/amo*`。原子 load/store 降为普通 load/store；RMW/CAS 语义由 `portable-atomic` 通过 `critical-section-single-hart` 提供。更完整的同步策略见[稳定 / 不稳定 API 门控](../explanation/policies/02-stable-unstable.md)中的 critical-section 纪律。

## Legacy custom toolchain

`hispark-rs/hisi-riscv-rust-toolchain` 曾经发布自定义 rustc tarball。该路径现在仅作为历史兼容说明保留；新文档、CI、template 和 happy path 不再推荐或依赖它。该仓库后续职责是官方 upstream nightly 的外部 CI / radar：检查 latest nightly target-list、rustup std 组件状态、build-std 回归、QEMU/HIL canary，并积累未来 Tier-2 推进证据。
