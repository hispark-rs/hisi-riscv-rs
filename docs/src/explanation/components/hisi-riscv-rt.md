# hisi-riscv-rt 架构与评审

> 本文是 ws63-rs 架构文档的一部分。完整评审台账见 [架构评审 2026-05](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/review/architecture-review-2026-05.md)，整改排期见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

> **2026-06 更新**：同一 runtime 服务 WS63 与 BS2X（BS21/BS20）。BS2X 示例自带按芯片的 `memory.x`（BS21E/BS22 160K、BS20 128K L2RAM），见 `examples/bs21` / `examples/bs20`。

## 职责与边界

`hisi-riscv-rt` 是 WS63（RISC-V RV32IMFC_Zicsr）的最小运行时（runtime），负责把芯片从复位状态带到可执行 Rust `main()` 的环境。

**负责：**

- **复位向量**：`reset_vector` 作为链接到 PROGRAM 区最前端的入口（`asm/startup.S:18-26`、`layout.ld:19` 的 `ENTRY(reset_vector)`）。
- **CPU 早期初始化**：清 PMP、设 `mtvec`、关中断、开 FPU、清 `fcsr`、初始化 `gp`/`sp`、栈金丝雀填充（`asm/startup.S:28-73`）。
- **trap/中断向量与汇编分发**：异常入口 `trap_entry`、NMI、6 个 MIE 中断、60 个 local 中断的向量与寄存器保存/恢复（`asm/startup.S:76-428`）。
- **段重定位**：ROM data/BSS、TCM text/data/BSS、SRAM text、`.data`、`.bss` 从 flash 拷到 RAM 并清零（`src/startup.rs:75-193`）。
- **缓存与 PMP**：I/D cache 使能（`src/startup.rs:59-69`），PMP 由 startup.S 在复位时清零（doc 注释提到 PMP 配置，但当前仅做禁用）。
- **链接脚本**：内存布局（`memory.x`）、段布局（`layout.ld`）、中断符号默认值（`device.x`）。`build.rs` 生成 `ws63-link.x` 包装脚本（按 memory→layout→device→symbols `INCLUDE`），下游 bin 用一个 `-Tws63-link.x` 引入。**`bundled-memory-x` 默认 feature**：hisi-riscv-rt 默认把自己的 `memory.x` 放上链接搜索路径（零配置）；需要自定义布局的 bin 设 `default-features = false` 自带 `memory.x`（见 `examples/ws63/custom_memory`）。
- **入口属性与 prelude**：re-export `riscv_rt::entry` 与 PAC 中断类型（`src/lib.rs:44-64`）。
- **临界区基础设施**：作为持有单一应用 hart 的 crate，启用 `riscv` 的 `critical-section-single-hart`，为全固件提供唯一的 `critical-section` 实现（`Cargo.toml` 依赖注释；支撑 PAC 的 `Peripherals::take()` 与 HAL 的 portable-atomic polyfill）。

**不负责：**

- 不实现中断控制器逻辑（SYS_CTL1 / LOCIPRI / LOCIEN 的优先级与使能仅在 device.x 给出占位符，实际派发模型有误，见评审）。
- 不提供堆分配器（`.heap` 段仅预留地址，无 allocator）。
- 不做镜像头/验签/AB 切换（属 flashboot 与下游范畴）。
- 不做 porting/HCC/blob 连接性相关初始化。

## 在依赖链中的位置

```
ws63-svd (XML) → ws63-pac (svd2rust 生成) → hisi-riscv-hal → examples/ws63/*
                                  hisi-riscv-rt ─┘  提供启动/向量/链接脚本
```

`hisi-riscv-rt` 是“横切”运行时：它不在 PAC→HAL→examples 这条数据流主线上，而是为最终的 **bin（examples）** 提供入口、trap 向量与链接脚本。它依赖 `ws63-pac`（仅为 re-export 中断类型与共享单一 PAC 实例）、`riscv` 与 `riscv-rt`。

> 链接脚本传播（已解决）：lib 依赖的 `cargo:rustc-link-arg` 不传播到下游 bin。早先这导致示例无法链接；**现已修**——`build.rs` 改为 `cargo:rustc-link-search` 导出 OUT_DIR + 生成 `ws63-link.x`，bin 用 `-Tws63-link.x` 引入（`rustc-link-search` 会传播）。见评审“问题”表「本轮已修」条。

## 关键设计

### 启动序列（标准 RV32 bring-up）

`asm/startup.S` 的复位流程对照 fbb_ws63 SDK 的 `start.S`，符合标准 RV32 裸机启动惯例：

1. 清 `pmpcfg0..3`（`startup.S:30-37`，EDA/仿真 workaround）。
2. `la t0, trap_vector; csrw mtvec, t0`（`startup.S:40-41`）。
3. 关中断：`csrwi mstatus,0` + `csrwi mie,0`（`startup.S:44-45`）。
4. 开 FPU：`mstatus.FS=0b11`，清 `fflags`（`startup.S:48-50`）。
5. 初始化 `gp`（`norelax` 包裹，`startup.S:53-56`）与 `sp = __stack_top__`（`startup.S:59`）。
6. 栈金丝雀填充 `0xefbeadde`（`startup.S:62-70`）。
7. `tail runtime_init`（`startup.S:73`）→ Rust 侧做重定位/清 BSS/再开 `mie`（`src/startup.rs:21-50`）。

内存地址（BOOTROM 0x100000、ROM 0x109000、ITCM 0x14C000、DTCM 0x180000、FLASH 0x200000、PROGRAM 0x230300、SRAM 0xA00000）与 fbb_ws63 一致（`memory.x:16-41`）。

### trap/异常/中断汇编分发

- **异常**：`trap_entry`（`startup.S:320`）用 `save_all`（36 字，含 `mcause`/`mbadaddr`/`ccause` 自定义 CSR）保存上下文，通过 `mscratch` 切到 `__exc_stack_top__`（`startup.S:327-328`），按 `mcause` 索引 `.rodata` 中的 `excp_vect_table`（`startup.S:132-153`、335-342）分发；M-mode ecall 单独走 `handle_ecall_m`（`startup.S:356-361`）。
- **NMI**：切到 `__nmi_stack_top__`，调 `nmi_handler`（`startup.S:369-383`）。
- **MIE 中断**：`mie_interrupt_handler` 宏生成 6 个（bits 26-31），切到 `__irq_stack_top__`，`call mie\n\()_interrupt_handler`（`startup.S:389-410`）。
- **local 中断**：60 个向量统一进 `local_interrupt_handler`，调 `local_isr_dispatch`（`startup.S:418-428`）。

每条 trap 路径都做了 `mscratch` 栈切换 + 上下文保存，异常路径还按 `mcause` 做表驱动派发，结构清晰。

### 链接脚本布局

`layout.ld` 改编自 fbb_ws63 的 `linker.prelds`：ITCM 放 patch 表/ROM-RAM 回调/TCM text；DTCM 放 ROM data/BSS、TCM data/BSS；SRAM 放 SRAM text、`.data`、`.bss`、栈、堆；FLASH(PROGRAM) 放 `.text`/`.rodata` 与各初始化段 LMA。`.startup` 段 `KEEP(*(.text.entry))` 确保复位向量在 PROGRAM 区最前（`layout.ld:125-129`）。栈区在 `.stacks (NOLOAD)` 内自高地址向下生长（`layout.ld:189-216`）。

### ISA / 原子性

`build.rs` 设 `RISCV_RT_BASE_ISA=rv32i`（无原子扩展）。默认 target 是 builtin 的 **`riscv32imfc-unknown-none-elf`**（RV32IMFC，硬件单精度浮点 ilp32f）；无 A 扩展，原子由 portable-atomic 的 critical-section polyfill 提供，`hisi-riscv-rt` 启用 `riscv` 的 `critical-section-single-hart` 作为整个固件唯一的 CS 实现（`Cargo.toml`）。这套 CS 也支撑 hisi-riscv-hal 的 `async`/`embassy` 异步层。

## 评审发现

### 优点

- **标准 RV32 启动**：PMP 清零、`mtvec`、关中断、FPU、`gp`/`sp`、栈金丝雀、BSS/data 重定位齐备，流程对照 fbb_ws63（`asm/startup.S:28-73`、`src/startup.rs:75-193`）。
- **内存地址权威**：`memory.x` 各区起始/长度与 fbb_ws63 SDK 对齐（`memory.x:16-41`）。
- **trap 汇编质量高**：异常/IRQ/NMI 均有 `mscratch` 栈切换 + 分栈（exc/irq/nmi 独立栈），异常按 `mcause` 索引 `excp_vect_table` 表驱动分发（`asm/startup.S:318-428`、132-153）。
- **单一 CS 实现的依赖边界清晰**：由持有 hart 的 `hisi-riscv-rt` 独家启用 `critical-section-single-hart`，避免多处重复实现（`Cargo.toml` 注释）。

### 问题

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|--------|------|------|-----------------|------|
| 高 | 正确性 | `mtvec` 以 Direct 模式写入（`la t0,trap_vector; csrw mtvec,t0`，未 `ori` 设置 MODE=Vectored），但同时构建了完整的 Vectored 跳转表（含 NMI/MIE/local 各项），导致除 `trap_entry` 外的向量项全部失效——所有 trap 都落到偏移 0 的异常入口 | `asm/startup.S:40-41`（Direct 写法）vs `asm/startup.S:88-127`（Vectored 表）| 已排期(ROADMAP 阶段 2，随中断子系统重构修正模式/表) |
| 中 | 正确性 | trap 相关段（`.trap`/`.trap.exception`/`.trap.nmi`/`.trap.mie*`/`.trap.local`）在 `layout.ld` 无显式输出段放置，成为孤立段（orphan），布局/对齐依赖链接器默认行为 | `asm/startup.S:85,318,367,390,416`（段声明）；`layout.ld:28-224`（无对应 `*(.trap*)` 放置）| 已排期(ROADMAP 阶段 2) |
| 高 | 构建 | （已修）链接脚本不传播到下游二进制：`build.rs` 原用 `cargo:rustc-link-arg=-T...` 注入，但该 arg 来自 lib 依赖、不传递到 bin；示例改用 lld 默认布局、`__exc/nmi/irq_stack_top__` 未定义 → blinky 链接失败。现改为 `cargo:rustc-link-search` 导出 OUT_DIR + 生成 `ws63-link.x` 包装脚本（按 memory→layout→device→symbols `INCLUDE`），blinky `build.rs` 以 `-Tws63-link.x` 引入 → **blinky 现可链接** | `build.rs`（link-search + ws63-link.x）；`examples/ws63/blinky/build.rs` | 本轮已修 |
| 高 | 构建 | （已修）MIE 中断宏 typo：`call mie\()_interrupt_handler` 缺少 `\n`，宏展开后符号名错误 | `asm/startup.S:397`（现为 `call mie\n\()_interrupt_handler`）| 本轮已修 |
| 中 | 构建 | （已修）栈顶符号 `__irq/exc/nmi_stack_top__` 在 `.stacks (NOLOAD)` 仅符号区被 `--gc-sections` 回收 → 链接期未定义；已在 `memory.x` 顶层加 GC-safe fallback | `layout.ld:199-204`（说明）；`memory.x:76-78`（fallback）| 本轮已修 |
| 中 | 构建 | （已修）`riscv` 启用 `critical-section-single-hart`，为无原子扩展的 WS63 提供单 hart CS 实现，支撑 PAC `take()` 与 HAL portable-atomic | `Cargo.toml`（`riscv` features）| 本轮已修 |
| 低 | 构建/发布 | （已修）`ws63-pac` 依赖补充 `version`（`version = "0.1", path = ...`）以便 `cargo publish` | `Cargo.toml`（ws63-pac 依赖）| 本轮已修 |

> 说明：构建完整性修复中与本组件相关的还包括——双 PAC 实例消除（根 `[patch.crates-io]` 指向本地，cargo tree 单一实例）、无原子 ISA 下实测产物零原子指令（lr/sc/amo）。默认 target 现为 ws63 工具链 builtin 的 `riscv32imfc-unknown-none-elf`（硬浮点；2026-05-31 曾过渡用 stable `riscv32imc`）。这些在仓库级评审中记录，本组件直接相关项已并入上表。

## 改进项与排期

- **阶段 1（链接脚本集成 ✅ 已完成 / 上板待硬件）**：链接脚本不传播问题已解决（`rustc-link-search` + `ws63-link.x` 包装脚本 + blinky `build.rs` 引入），blinky 现可链接并产出 `.bin`。剩余：真机上板冒烟、用 `readelf` 核实 WS63 布局生效。
- **阶段 2（死代码清理 + 正确性修复）**：修正 `mtvec` 模式与向量表的不一致（Direct vs Vectored）；为 trap 段在 `layout.ld` 增加显式输出段放置；统一 `.stacks` 布局与 `memory.x` 栈顶 fallback；并随中断子系统模型纠正（PLIC vs LOCIPRI/LOCIEN）一并处理。对应上表前两行。
- 其余仓库级排期（efuse/lsadc、flashboot 镜像头/验签/AB、porting+HCC+blob 连接性、async）见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md) 阶段 2-6，与本运行时组件无直接耦合。
