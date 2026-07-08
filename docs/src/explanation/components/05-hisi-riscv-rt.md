# hisi-riscv-rt 架构

`hisi-riscv-rt` 是最终 firmware bin 进入 Rust `main()` 前经过的 runtime crate。它现在的外部 Interface 有意保持很薄：复用 `riscv-rt` 的 `#[entry]` / `#[pre_init]`，重导出当前 PAC 的 interrupt enum，并为单 hart / 无 A 扩展产品路径注册全局 critical-section 实现。

真正和芯片绑定的 reset、trap、linker、boot header 被收进 startup adapter；中断符号 `device.x` 则由当前 PAC 的 `rt` feature 负责。这个拆分记录在 [ADR 0001](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/adr/0001-runtime-adapter-seams.md)。

## Adapter 分层

- **`rt_core`**：芯片中立层，只承接 `riscv-rt` re-export 与通用 linker contract，不放芯片地址。
- **WS63 adapter**：`asm/ws63/startup.S`、`linker/ws63/{memory.x,layout.ld,boot-header.x}`、`src/chips/ws63/startup.rs`。它拥有 WS63 reset、trap dispatch、段搬运、link-time boot header；`device.x` 来自 `ws63-pac/rt`。
- **BS2X compatibility adapter**：`linker/bs2x/{memory.x,layout.ld,boot-header.x}` 收纳 BS2X adapter 事实；`memory.x` 默认是 BS21/BS2X 160K L2RAM，BS20/自定义板卡可继续自带 `memory.x` 覆盖；`bs2x-pac/rt` 提供 `device.x`。当前仍复用 legacy M-core startup，它是 `unstable` 兼容路径，不是“已经有独立 BS2X runtime HIL 验证”的声明。`boot-header.x` 只放占位说明，尚未被 build.rs include。
- **Hi3322 placeholder**：不暴露可启动 feature。Hi3322 的 TES/TEE reset、CLIC、内存分区和镜像格式见 [Hi3322 runtime 移植预研](hi3322-runtime-porting.md)。

## 架构图

```text
下游 firmware / examples
        │
        ├── uses hisi-riscv-hal ───────► 当前芯片 PAC
        │                                  ├── interrupt enum
        │                                  └── device.x via PAC/rt
        │
        └── uses hisi-riscv-rt
               │
               ├── rt_core
               │     └── re-export riscv-rt::{entry, pre_init}
               │
               ├── chip startup adapters
               │     ├── ws63
               │     │     ├── asm/ws63/startup.S
               │     │     ├── linker/ws63/memory.x
               │     │     ├── linker/ws63/layout.ld
               │     │     └── linker/ws63/boot-header.x
               │     │
               │     ├── bs2x
               │     │     └── compatibility adapter
               │     │           ├── linker/bs2x/memory.x
               │     │           ├── linker/bs2x/layout.ld
               │     │           ├── linker/bs2x/boot-header.x placeholder
               │     │           └── reuse legacy startup for now
               │     │
               │     └── hi3322
               │           └── placeholder/spec only; no bootable feature
               │
               ├── linker/common/riscv-rt-symbols.x
               │
               └── build.rs
                     ├── emits hisi-riscv-link.x
                     
                     ├── copies runtime-owned memory/layout/header fragments
                     └── does not own device.x
```

特性选择把 runtime adapter 和 PAC 事实源绑在一起：

```toml
chip-ws63 = ["dep:ws63-pac", "ws63-pac/rt"]
chip-bs21 = ["dep:bs2x-pac", "bs2x-pac/rt"] # requires unstable
```

这条边界很重要：**PAC owns interrupt enum + `device.x`; `hisi-riscv-rt` owns reset/startup、linker layout contract、默认 `memory.x`、boot-header、critical-section 注册。** 因此 `hisi-riscv-link.x` 会 `INCLUDE device.x`，但这个文件必须来自当前 PAC 的 `rt` feature，而不是 runtime crate 自己私藏一份芯片中断符号。

## Linker contract

下游 bin 使用一个中性脚本名：

```text
-Thisi-riscv-link.x
```

`build.rs` 把 adapter 资源复制到 `OUT_DIR`，并生成 `hisi-riscv-link.x`，按顺序 `INCLUDE memory.x`、`layout.ld`、`device.x`、`riscv-rt-symbols.x`。其中 `device.x` 由当前 PAC 的 `rt` feature 放到 linker search path；WS63 `boot-header` feature 额外 `INCLUDE boot-header.x`。


`bundled-memory-x` 发出当前 chip 的默认 `memory.x`：WS63 发出 WS63 memory map，BS2X 发出 BS21/BS2X 160K L2RAM 默认图。BS20 的 L2RAM 是 128K，因此 BS20 示例仍关闭 `bundled-memory-x` 并自带 `memory.x`。这不是第二份事实源，而是下游覆盖 runtime 默认的 linker contract：同一 firmware link graph 中仍只能有一个 `memory.x` 被解析。

## Stable / unstable 边界

`hisi-riscv-rt` 也采用稳定/不稳定边界，但它的颗粒度不是 HAL 外设 API 清单，而是 runtime adapter 承诺：

- **STABLE**：薄 `riscv-rt` facade（`entry` / `pre_init`）、WS63 默认 startup/linker 路径、WS63 `boot-header`。
- **UNSTABLE**：`chip-bs21` BS2X compatibility adapter、`riscv-rt-start-experiment`。

这样做的原因是 BS2X 现在有 QEMU/build 证据，但没有 BS2X 板级 HIL；而 `riscv-rt-start-experiment` 虽然用于验证更深的 `riscv-rt` 复用路径，但仍不是默认 release gate。WS63 HAL/HIL 仍是当前 release gate 的稳定证据主线。

## riscv-rt 复用边界

当前默认路径仍保留自定义 startup，原因是 WS63/BS2X 已有 QEMU/HIL 证据依赖这条 reset/trap/linker 组合。为了继续向 `riscv-rt` 靠拢，crate 保留非默认 `riscv-rt-start-experiment` feature 作为实验门禁；它把标准 `.data`/`.bss`/FPU/gp/sp 初始化交给 `riscv-rt` 的 `_start`，但默认 reset path 尚未切过去，默认 release gate 也不依赖它。

可以继续迁给 `riscv-rt` 的部分：

- `#[entry]` / `#[pre_init]`；
- 标准 `.data`/`.bss` 初始化；
- FPU/gp/sp 等普通 RISC-V `_start` 工作；
- `memory.x` 区域别名 contract。

仍应留在 chip adapter 的部分：

- WS63 direct-mode trap dispatch 与 local IRQ 表；
- WS63 boot header 与 flashboot 镜像入口 contract；
- BS2X/Hi3322 专属 reset/vector/interrupt-controller 初始化；
- Hi3322 TES/TEE reset、CLIC 与 SELiteOS 分区模型。

## Critical-section 职责

当前产品主路径是单 hart + 无 A 扩展。`hisi-riscv-rt` 启用 `riscv/critical-section-single-hart`，为 PAC `Peripherals::take()` 和 HAL 的 `portable-atomic` polyfill 提供唯一全局实现。

这不是跨 hart 锁。未来多 hart + A 扩展产品可以用硬件 atomics 处理单字标志/计数，但复合不变量仍需要真正的锁、`critical-section` 或平台级同步。

## 维护规则

- 新芯片先新增 adapter 或 adapter spec，不在 WS63 startup 里堆 feature 分支。
- 新 linker 事实放在对应 `linker/<chip>/`，通用符号才放 `linker/common/`。
- 修改 linker 脚本后必须验证 `custom_memory`、WS63 boot-header 地址和至少一个示例链接。
- Hi3322 在 PAC、linker、镜像打包和板级证据存在前，不新增 `chip-hi3322`。
