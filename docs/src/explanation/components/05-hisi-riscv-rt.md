# hisi-riscv-rt 架构

`hisi-riscv-rt` 是最终 firmware bin 进入 Rust `main()` 前经过的 runtime crate。它现在的外部 Interface 有意保持很薄：复用 `riscv-rt` 的 `#[entry]` / `#[pre_init]`，重导出当前 PAC 的 interrupt enum，并为单 hart / 无 A 扩展产品路径注册全局 critical-section 实现。

真正和芯片绑定的 reset、trap、linker、`device.x`、boot header 被收进 startup adapter。这个拆分记录在 [ADR 0001](../../../adr/0001-runtime-adapter-seams.md)。

## Adapter 分层

- **`rt_core`**：芯片中立层，只承接 `riscv-rt` re-export 与通用 linker contract，不放芯片地址。
- **WS63 adapter**：`asm/ws63/startup.S`、`linker/ws63/{memory.x,layout.ld,device.x,boot-header.x}`、`src/chips/ws63/startup.rs`。它拥有 WS63 reset、cache CSR、trap dispatch、段搬运、link-time boot header。
- **BS2X compatibility adapter**：BS20/BS21 示例自带 `memory.x`，`bs2x-pac/rt` 提供 `device.x`，当前仍复用 legacy M-core startup/layout。它是兼容路径，不是“已经有独立 BS2X runtime 验证”的声明。
- **Hi3322 placeholder**：不暴露可启动 feature。Hi3322 的 TES/TEE reset、CLIC、内存分区和镜像格式见 [Hi3322 runtime 移植预研](hi3322-runtime-porting.md)。

## Linker contract

下游 bin 使用一个中性脚本名：

```text
-Thisi-riscv-link.x
```

`build.rs` 把 adapter 资源复制到 `OUT_DIR`，并生成 `hisi-riscv-link.x`，按顺序 `INCLUDE memory.x`、`layout.ld`、`device.x`、`riscv-rt-symbols.x`。WS63 `boot-header` feature 额外 `INCLUDE boot-header.x`。

`ws63-link.x` 仍生成，但只是兼容别名；新示例、HIL 与文档都应使用 `hisi-riscv-link.x`。

WS63 的 `bundled-memory-x` 只在同时启用 `chip-ws63` 时发出 WS63 `memory.x`。BS2X 构建不会复制 WS63 `memory.x`，而是由示例或下游工程自己提供内存布局。

## riscv-rt 复用边界

当前默认路径仍保留自定义 startup，原因是 WS63/BS2X 已有 QEMU/HIL 证据依赖这条 reset/trap/linker 组合。为了继续向 `riscv-rt` 靠拢，crate 预留了非默认 `riscv-rt-start-experiment` feature 作为后续实验门禁；它现在只做 WS63-only 约束与文档标记，尚未把默认 reset path 切到 `riscv-rt` 的 `_start`，默认 release gate 也不依赖它。

可以继续迁给 `riscv-rt` 的部分：

- `#[entry]` / `#[pre_init]`；
- 标准 `.data`/`.bss` 初始化；
- FPU/gp/sp 等普通 RISC-V `_start` 工作；
- `memory.x` 区域别名 contract。

仍应留在 chip adapter 的部分：

- WS63 direct-mode trap dispatch 与 local IRQ 表；
- WS63 cache CSR、PMP workaround、boot header；
- BS2X 专属中断符号表；
- Hi3322 TES/TEE reset、CLIC 与 SELiteOS 分区模型。

## Critical-section 职责

当前产品主路径是单 hart + 无 A 扩展。`hisi-riscv-rt` 启用 `riscv/critical-section-single-hart`，为 PAC `Peripherals::take()` 和 HAL 的 `portable-atomic` polyfill 提供唯一全局实现。

这不是跨 hart 锁。未来多 hart + A 扩展产品可以用硬件 atomics 处理单字标志/计数，但复合不变量仍需要真正的锁、`critical-section` 或平台级同步。

## 维护规则

- 新芯片先新增 adapter 或 adapter spec，不在 WS63 startup 里堆 feature 分支。
- 新 linker 事实放在对应 `linker/<chip>/`，通用符号才放 `linker/common/`。
- 修改 linker 脚本后必须验证 `custom_memory`、WS63 boot-header 地址和至少一个示例链接。
- Hi3322 在 PAC、linker、镜像打包和板级证据存在前，不新增 `chip-hi3322`。
