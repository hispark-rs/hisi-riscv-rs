# 硬浮点工具链

这一篇解释为什么本生态使用 `riscv32imfc-unknown-none-elf`，以及为什么当前构建方式是**官方 Rust nightly + `rust-src` + `-Zbuild-std=core,alloc`**。

安装与版本事实见[工具链与编译目标](../reference/05-toolchain.md)和[安装官方 Rust 工具链](../how-to/01-install-toolchain.md)；这里讲设计原因和历史取舍。

## 这颗核到底是什么

WS63 / BS2X 应用核是 **RV32IMFC**：

- **I/M/C**：基础整数、乘除、压缩指令；
- **F**：有单精度 FPU；
- **没有 A**：`lr.w` / `sc.w` / `amo*` 会触发非法指令。

这意味着目标必须同时表达两件事：使用硬浮点 ABI，又不能让编译器生成硬件原子指令。

## 为什么硬浮点（ilp32f ABI）

硅片有 FPU，所以 Rust 固件应使用 **ilp32f** ABI：浮点参数走浮点寄存器，`f32` 运算生成真实 FPU 指令。软浮点当然也能跑，但会浪费硬件，并且代码更慢更大。

更重要的是 ABI 边界。WS63 的连接性目标最终需要和厂商闭源 blob 链接；这些 blob 按厂商 gcc 的 **ilp32f** ABI 产出。Rust 侧如果用 ilp32 软浮点，浮点参数传递约定就不一致，后续 blob 链接会变得不可控。因此硬浮点不是单纯优化，而是长期互操作前提。

## 为什么没有原子是个真问题

如果误用带 `A` 扩展的 target，例如 `riscv32imafc`，编译器可以合法发出 `lr/sc/amo*`。但这些指令在真硅片上不存在，会直接陷入非法指令。

正确模型是：

- target 声明无原子扩展；
- 单 hart 下原子 load/store 可降为普通 load/store；
- RMW/CAS 由 `portable-atomic` 通过 `critical-section` polyfill 实现；
- `hisi-riscv-rt` 提供当前产品路径所需的 single-hart critical-section 实现。

所以 `riscv32imfc-unknown-none-elf` 的价值不是“名字更贴近芯片”这么简单，而是防止整个生态误发 A 扩展指令。

## 为什么现在仍需要 `-Zbuild-std`

`rust-lang/rust#158473` 已经把 `riscv32imfc-unknown-none-elf` 加进 upstream rustc，当前 nightly 的 `rustc --print target-list` 能看到它。这个状态解决了“rustc 不认识 target”的问题。

但 rustup 还没有为该 target 分发预编译 `rust-std` 组件。也就是说，`rustup target add riscv32imfc-unknown-none-elf` 还不是可用安装路径。短期构建必须让 Cargo 从 `rust-src` 编译 `core` / `alloc`：

```bash
cargo build -Zbuild-std=core,alloc --release
```

这就是当前 pinned nightly 的原因：业务仓 pin 一个已验证 nightly 保证可复现；外部 radar 跟踪 latest nightly 的上游状态。

## 旧自定义工具链为什么退役

旧的 `hisi-riscv` 自定义 rustc tarball 解决过一个真实问题：当 upstream rustc 还没有这个 target 时，它把 target spec 烤成 builtin，并随工具链分发预编译 `core` / `alloc`。这让用户可以在 stable-like 体验下直接 `cargo build`。

现在 upstream rustc 已经认识 target，继续依赖自定义 rustc 会带来更大的长期成本：

- 生态仍被私有工具链绑定，难以上游化；
- 每次 rustc 版本更新都需要维护自定义构建和多平台 tarball；
- 外部项目、CI、template 难以复用标准 Rust 安装路径；
- 推动 Tier 2 需要证明官方工具链路径持续可用，而不是继续绕开它。

因此当前策略是：业务仓库使用官方 pinned nightly；`hisi-riscv-rust-toolchain` 仓库转型为外部 radar，持续检查 latest nightly、rustup std 组件、build-std、QEMU/HIL canary，并积累 Tier-2 readiness 证据。

## code model：medlow 还是 medany

另一个裸机上会出问题的旋钮是 code model。WS63 的 flash、SRAM、外设分布在较高地址，不能假设所有符号都在低 2 GiB。

- **medlow**：假设代码和数据落在低地址，用更短寻址序列；
- **medany**：使用 PC 相对寻址，能覆盖更灵活的地址布局。

当前 target 需要匹配这类高地址裸机布局，避免链接脚本把段放到高地址后出现寻址范围问题。

## 这件事对其他部分的影响

async / embassy 本身不关心 target 是“自定义工具链”还是“官方 nightly + build-std”；它关心的是无原子模型是否正确，以及 `portable-atomic` / `critical-section` 是否可靠。

真正被工具链路径影响的是上游化：只要生态仍依赖私有 rustc，就很难进入更广泛的 Rust embedded CI。迁移到官方 target 后，剩下的路线更清晰：先用 build-std 维持可用性，再推动 rustup 预编译 std 组件和更高 tier 支持。
