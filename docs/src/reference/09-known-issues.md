# 已知问题索引

本页汇总当前已知的问题、对应影响与跟踪入口。**列在这里的问题都不影响
`cargo build` / `cargo check`**——它们要么是工具链/IDE 集成层面的坑,要么是
等待上游修复的缺陷。遇到对得上的现象,按"修复 / 规避"列处理即可。

## 工具链 / IDE 集成

| 现象 | 根因 | 修复 / 规避 | 跟踪 |
| --- | --- | --- | --- |
| `can't find crate for core` | RISC-V 命令缺少 `-Zbuild-std=core,alloc`,或 pinned nightly 没装 `rust-src` | 按[安装官方 Rust 工具链](../how-to/01-install-toolchain.md)安装 `rust-src`,RISC-V 命令加 build-std | — |
| `rustup target add riscv32imfc-unknown-none-elf` 找不到 target | rustc 已内置 target,但 rustup 还没有预编译 `rust-std` 组件 | 不用 `rustup target add`;短期使用 `rust-src` + `-Zbuild-std=core,alloc` | [rust#158473](https://github.com/rust-lang/rust/pull/158473) |
| `cannot find io_config in pac` `E0433` 等 chip 相关误报 | RA 开了 `cargo.allFeatures`,把互斥的 `chip-ws63` / `chip-bs21` 同时打开 | 设 `rust-analyzer.cargo.allFeatures = false`(仓库已提供 `rust-analyzer.toml`) | — |
| `can't find crate for test` `E0463` | `--all-targets` 在裸机 target 构建 test 目标,而 no_std 无 `test` crate | 设 `cargo.allTargets = false` 与 `check.allTargets = false`(仓库已提供 `rust-analyzer.toml`) | — |

> 上表后两项属于"嵌入式 + 多芯片 feature 下的标准 RA 配置",不是缺陷,故无独立 issue;
> 仓库根与 `examples/bs2x` 已各放一份 `rust-analyzer.toml`,VS Code 等会直接生效。完整说明与
> client 优先级注意事项见 [安装官方 Rust 工具链](../how-to/01-install-toolchain.md)。

## 怎样新增一条

1. 能复现且属于上游缺陷的,先在对应仓库开 issue(本仓 / 工具链 / probe-rs / QEMU 分支),
   把链接填进"跟踪"列。
2. 仅属配置/环境坑、无需上游修的,"跟踪"列填 `—`,并在 how-to 里给出规避步骤后链接过去。
3. 保持"现象"列用用户**实际看到的报错原文**,方便搜索命中。
