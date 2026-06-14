# 硬浮点工具链

这一篇解释一个看起来"过度工程"的决定：**为什么 WS63 要用一条自定义的 rustc——把
`riscv32imfc-unknown-none-elf` 烤进 builtin 的 `hisi-riscv` 工具链——而不是用现成的
stable rustc 加 `-Z build-std`？** 这背后串着三个互相牵连的约束：硬浮点 ABI、没有原子扩展、
以及 code model。逐项的安装与版本细节见
[工具链与编译目标](../reference/toolchain.md) 和
[安装 hisi-riscv 工具链](../how-to/install-toolchain.md)；这里只讲**为什么是这条路**。

## 这颗核到底是什么

WS63 的核是 **RV32IMFC**：

- **I**（基础整数）、**M**（乘除）、**C**（压缩指令）——常规；
- **F**（单精度浮点）——**有**硬件浮点单元；
- **没有 A**（原子扩展）——`lr.w/sc.w/amo*` 这些指令会**陷入非法指令**。

这两个非常规点（有 F、没有 A）合起来，把"选哪个编译目标"这件事从"随手挑个标准 target"
变成了一道需要权衡的题。

## 为什么硬浮点（ilp32f ABI）

既然硅片**有** FPU，最自然的选择就是让它用起来——也就是 **ilp32f** ABI：浮点参数走浮点
寄存器、浮点运算发真正的 `f*` 指令，而不是软件模拟。软浮点（ilp32）当然也能跑（编译器把
`f32` 运算翻成调用 `libgcc`/`compiler-builtins` 里的软件例程），但那是在一颗有 FPU 的核上
**白白浪费硬件**、还更慢更大。

但硬浮点 ABI 的真正分量不只在性能。**ilp32f 是一条 ABI 边界**——用 ilp32f 编的代码和用
ilp32 编的代码**不能直接链接**（浮点参数的传递约定不同）。而 WS63 的北极星是连接性，
连接性意味着最终要和**厂商的闭源 blob** 链接，那些 blob 是用厂商 gcc 按 **ilp32f** 编的。
所以选 ilp32f 不仅是"用上 FPU"，更是"为了将来能和 vendor blob 在同一个 ABI 上对接"
——这是阶段 3（blob 链接）的前置条件。这一层动机，使硬浮点从"优化"升级成"必需"。

## 为什么没有原子是个真问题

RV32IMFC 缺 A 扩展，意味着任何会发 `lr/sc/amo` 的代码在硅片上都会**陷入**。
而 Rust 的 `core::sync::atomic` 默认假设有原子指令。历史上一度用过 `riscv32imafc`
（带 A）作为权宜——但那会让编译器发原子指令、在真硅片上触发非法指令陷阱，所以**被弃用**。

正解是两段配合：

1. **目标本身声明为无原子**——用 forced-atomics + no-CAS 配置，让原子 load/store 降级成
   普通 `ld/st`（单核下这是安全的），而需要 RMW（compare-and-swap 之类）的操作**不发
   原子指令**；
2. **RMW 走 polyfill**——`portable-atomic`（开 `critical-section` feature）把 CAS
   实现成"关中断 → 读改写 → 开中断"的临界区，`hisi-riscv-rt` 提供
   `critical-section-single-hart` 这个单核实现。

这套机制正是 async/embassy 能在这颗核上跑的地基（见 [async 与 embassy](async-embassy.md)）。
它和"用不用自定义工具链"正交——但**目标必须被正确声明为无原子**，否则 polyfill 也救不了，
编译器照样会在别处发出原子指令。

## 核心抉择：自定义 builtin target，还是 `-Z build-std`？

到这里问题收敛成：我们需要一个**标准 rustc 里没有的目标**（`riscv32imfc`，硬浮点、无原子）。
Rust 提供两条路拿到一个非标准 target，二者是真正的取舍：

### 路线 A：`-Z build-std`（用现成 stable rustc + nightly 特性）

写一个 `*.json` 自定义 target spec，然后让 cargo 用 `-Z build-std` **从源码现编 `core`/`alloc`**。
- **好处**：不用自己造工具链，跟着官方 rustc 走。
- **代价**：`-Z build-std` 是 **nightly-only** 的不稳定特性。整条工具链就被钉死在 nightly
  上——nightly 每天变、偶尔回归，CI 的可重现性变差，用户也得装 nightly + rust-src。
  对一个要给别人用、要长期维护的嵌入式 SDK，"必须 nightly"是个不小的负担。

### 路线 B：自定义 rustc，把 target 烤成 builtin（现在走的路）

构建一条 `hisi-riscv` 工具链：一个 **stable rustc**，但在编译它的时候就把
`riscv32imfc-unknown-none-elf` 这个 target spec **编进 rustc 内部成为 builtin**，
并**预编译好 `core`/`alloc`** 一起分发。
- **好处**：用户拿到的是一条**稳定、自带预编译 core/alloc 的工具链**，`.cargo/config.toml`
  里设好默认 target 就行，**完全不需要 `-Z build-std`、不需要 nightly**。
  `cargo build` 直接出 RV32IMFC ilp32f 固件，可重现、好分发。
- **代价**：得自己**维护这条工具链**——跟 rustc 版本、出多平台预编译包、走自己的 CI。
  这是实打实的工程量，也是这套生态接受的那笔账。

权衡的结论很清楚：**用户体验和可重现性 > 维护方自己省事**。对一个嵌入式 SDK，"装好工具链
就能稳定 `cargo build`"远比"维护方不用管工具链、但每个用户都得忍 nightly"更值。
所以选了 B。工具链通过 `rust-toolchain.toml` pin 住 `channel = "hisi-riscv"`，
用 `rustup toolchain link` 接进 rustup。

## code model：medlow 还是 medany

还有一个容易被忽略、但在裸机上会真出问题的旋钮：**code model**，它决定编译器怎么寻址
全局符号。

- **medlow**：假设代码和数据都落在地址空间**低 2 GiB 以内**，用更短的寻址序列。
- **medany**：用 PC 相对寻址，可以放在地址空间**任意位置**，序列略长。

WS63 的地址布局把外设、flash、SRAM 散布在很高的地址（比如外设在 `0x4400_0000` 一带、
SRAM 更高），全局符号未必落在低 2 GiB。所以这条工具链用 **medany**——这样不管链接脚本把
段摆到哪个高地址，PC 相对寻址都能正确指到。如果误用 medlow，链接期或运行期会因为
"地址放不进 medlow 的寻址范围"而出错。这件事和硬浮点、无原子一样，是"WS63 的地址空间
不像教科书 RISC-V"逼出来的细节。

## 一段不算短的历史

这条路不是一步到位的：

- **2026-05-31，阶段 0**：先用 stable rustc 里**已有的 builtin** `riscv32imc`（软浮点、
  stable、免 build-std）做过渡——目的是先让整条构建/链接跑通，把"无原子 + critical-section"
  这套机制验证出来。
- **随后切到硬浮点工具链**：为了和 vendor blob 的 ilp32f ABI 对齐（阶段 3 的前置），
  把目标换成 `riscv32imfc`，并为此造了 `hisi-riscv` 工具链。

理解这段历史有助于读懂仓库里偶尔还能见到 `riscv32imc` 字样的地方——那是过渡期的遗存，
现在的**默认与正解是 `riscv32imfc` + `hisi-riscv` 工具链**。

## 这件事对其他部分的影响

值得强调的是：**异步/embassy 这块完全不在乎工具链是否上游**。异步只依赖
`portable-atomic` + `critical-section`，与"target 是 builtin 还是 build-std"正交。
真正被自定义工具链"绑住"的是**上游化**——只要还依赖自定义 rustc，hisi-riscv-hal 就难以
进 embassy 那种"基于标准 stable target 构建"的 in-tree CI。所以"摆脱自定义工具链"
（短期改用标准 target + build-std，长期推 target 进 rustc 主线）被列为一条独立的上游化
工作线，详见 [async 与 embassy 深入文档](components/async-embassy.md) 里的上游化讨论。
