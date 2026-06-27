# 已知问题索引

本页汇总当前已知的问题、对应影响与跟踪入口。**列在这里的问题都不影响
`cargo build` / `cargo check`**——它们要么是工具链打包/IDE 集成层面的坑,要么是
等待上游修复的缺陷。遇到对得上的现象,按"修复 / 规避"列处理即可。

## 工具链 / IDE 集成

| 现象 | 根因 | 修复 / 规避 | 跟踪 |
| --- | --- | --- | --- |
| rust-analyzer 海量误报:`unresolved macro println!`、`no method len on [u8; N]`、slice 强转报错 | 旧(`≤ v1.96.0-2`)tarball 的 rust-src 软链悬空(指向 CI 构建机绝对路径),RA 加载不到 `core`/`std` 源码 | ✅ **已在 `v1.96.0-3` 修复**;旧版规避见 [安装工具链 · IDE 已知问题](../how-to/install-toolchain.md) | [toolchain#1](https://github.com/hispark-rs/hisi-riscv-rust-toolchain/issues/1)(已修复) |
| RA 启动报 `'rust-analyzer' is not installed for toolchain 'hisi-riscv'` | 自定义工具链装不了 RA 组件,而 `rust-toolchain.toml` 把频道钉成了它 | 让编辑器用别的工具链的 RA 二进制,见 [安装工具链 · IDE 已知问题](../how-to/install-toolchain.md) | — |
| `cannot find io_config in pac` `E0433` 等 chip 相关误报 | RA 开了 `cargo.allFeatures`,把互斥的 `chip-ws63` / `chip-bs21` 同时打开 | 设 `rust-analyzer.cargo.allFeatures = false`(仓库已提供 `rust-analyzer.toml`) | — |
| `can't find crate for test` `E0463` | `--all-targets` 在裸机 target 构建 test 目标,而 no_std 无 `test` crate | 设 `cargo.allTargets = false` 与 `check.allTargets = false`(仓库已提供 `rust-analyzer.toml`) | — |

> 上表后两项属于"嵌入式 + 自定义工具链下的标准 RA 配置",不是缺陷,故无独立 issue;
> 仓库根与 `examples/bs2x` 已各放一份 `rust-analyzer.toml`,VS Code 等会直接生效。完整说明与
> client 优先级注意事项见 [安装 hisi-riscv 工具链](../how-to/install-toolchain.md)。

## 怎样新增一条

1. 能复现且属于上游缺陷的,先在对应仓库开 issue(本仓 / 工具链 / probe-rs / QEMU 分支),
   把链接填进"跟踪"列。
2. 仅属配置/环境坑、无需上游修的,"跟踪"列填 `—`,并在 how-to 里给出规避步骤后链接过去。
3. 保持"现象"列用用户**实际看到的报错原文**,方便搜索命中。
