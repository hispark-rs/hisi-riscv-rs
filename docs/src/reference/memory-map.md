# 内存映射

本页复现 WS63 的内存布局，事实取自 [`crates/hisi-riscv-rt/memory.x`](https://github.com/hispark-rs/hisi-riscv-rt) 与 [`crates/hisi-riscv-rt/asm/startup.S`](https://github.com/hispark-rs/hisi-riscv-rt)。默认配置：576K SRAM、16K ITCM、16K DTCM。TCM 与 SRAM 大小可经 CONFIG 标志配置（参见 fbb_ws63）。

启动流程的"为什么"见 [启动流程](../explanation/boot-flow.md)。

## 内存区域（`MEMORY{}`）

| 区域 | 属性 | ORIGIN | LENGTH | 结束 | 说明 |
|------|------|--------|--------|------|------|
| `BOOTROM` | `rx` | `0x100000` | `0x9000` (36K) | `0x109000` | 掩膜 ROM 启动码 |
| `ROM` | `rx` | `0x109000` | `0x43000` (268K) | `0x14C000` | 应用 ROM：SFC、pinmux、watchdog、timer、systick、TCXO、BT、WiFi |
| `ITCM` | `rwx` | `0x14C000` | `0x4000` (16K) | `0x150000` | 指令 TCM（默认 16K，可配至 64K） |
| `DTCM` | `rw` | `0x180000` | `0x4000` (16K) | `0x184000` | 数据 TCM（默认 16K，可配至 64K） |
| `FLASH` | `rx` | `0x200000` | `0x800000` (8MB) | `0xA00000` | 外部 SPI NOR flash，XIP |
| `PROGRAM` | `rx` | `0x230300` | `0x240000` (~2.25MB) | `0x470300` | flash 内应用代码区（启动头之后） |
| `SRAM` | `rwx` | `0xA00000` | `0x90000` (576K) | `0xA90000` | 主系统 RAM（SRAM/L2RAM） |
| `PRESERVE` | `rw` | `0xA90000 - 0x100` = `0xA8FF00` | `0x100` (256B) | `0xA90000` | SRAM 尾部 256 字节，保留启动状态 |

> `BOOTROM` + `ROM` 共 304K（36K + 268K），地址连续（`0x100000`–`0x14C000`）。

## 关键地址

| 名称 | 地址 | 说明 |
|------|------|------|
| app 分区 | `0x230000` | flashboot 从此处加载 app 镜像 |
| app 入口 | `0x230300` | 入口 = 分区地址 + 0x300（跳过 0x300 字节镜像头） |
| 复位向量 | `0x100000` | 链接为程序内存中的第一项；`reset_vector: j HandleReset`（`startup.S`） |
| 栈顶 `_stack_start` | `0xA90000` | `= ORIGIN(SRAM) + LENGTH(SRAM)` |

> flashboot **无条件**跳到 `app_partition + 0x300`，故 app 镜像必须带 0x300 字节 HiSilicon 镜像头。镜像头字段见 [应用镜像格式](image-format.md)。

## 导出的链接符号（`PROVIDE`）

区域符号（用于运行时重定位）：

| 符号 | 值 |
|------|-----|
| `__rom_start` | `ORIGIN(ROM)` = `0x109000` |
| `__rom_length` | `LENGTH(ROM)` = `0x43000` |
| `__itcm_start` | `ORIGIN(ITCM)` = `0x14C000` |
| `__itcm_length` | `LENGTH(ITCM)` = `0x4000` |
| `__dtcm_start` | `ORIGIN(DTCM)` = `0x180000` |
| `__dtcm_length` | `LENGTH(DTCM)` = `0x4000` |
| `__sram_start` | `ORIGIN(SRAM)` = `0xA00000` |
| `__sram_length` | `LENGTH(SRAM)` = `0x90000` |
| `__flash_start` | `ORIGIN(FLASH)` = `0x200000` |
| `__flash_length` | `LENGTH(FLASH)` = `0x800000` |
| `__program_start` | `ORIGIN(PROGRAM)` = `0x230300` |
| `__program_length` | `LENGTH(PROGRAM)` = `0x240000` |

riscv-rt v0.14 所需符号：

| 符号 | 值 |
|------|-----|
| `_stack_start` | `ORIGIN(SRAM) + LENGTH(SRAM)` = `0xA90000` |
| `_max_hart_id` | `0` |
| `_hart_stack_size` | `0x2000` |

数据/BSS 符号（`memory.x` 中为占位 `0`，权威值在 `layout.ld`）：`__sidata`、`__sdata`、`__edata`、`__sbss`、`__ebss`。

## 栈大小（可被用户覆盖）

| 符号 | 默认值 | 用途 |
|------|--------|------|
| `__stack_size` | `0x2000` (8K) | 用户栈 |
| `__irq_stack_size` | `0x800` (2K) | IRQ 栈 |
| `__exc_stack_size` | `0x800` (2K) | 异常栈 |
| `__nmi_stack_size` | `0x400` (1K) | NMI 栈 |

> IRQ/异常/NMI 栈顶在 `layout.ld` 的 `.stacks` 段中权威定义（trap 处理器引用它们，`KEEP` 的 `.trap` 段经 `--gc-sections` 保活）。

## 区域别名（riscv-rt v0.14 `REGION_ALIAS`）

| 别名 | 指向 |
|------|------|
| `REGION_TEXT` | `PROGRAM` |
| `REGION_RODATA` | `PROGRAM` |
| `REGION_DATA` | `SRAM` |
| `REGION_BSS` | `SRAM` |
| `REGION_STACK` | `SRAM` |
| `REGION_HEAP` | `SRAM` |

## 复位向量

`startup.S` 中 `reset_vector` 位于 `.text.entry` 段，链接为程序内存第一项，内容为 `j HandleReset`。`HandleReset` 依次：禁用 PMP（`pmpcfg0..3 = 0`）、以 vectored 模式（`mtvec[1:0]=01`）装载 `trap_vector`、关中断、使能 FPU（`mstatus.FS = 0b11`）、初始化 `gp`。

> 复位地址 `0x100000` 是掩膜 ROM 入口；上电后由 ROM 经 flashboot 转交到 app 入口 `0x230300`。
