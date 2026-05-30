# ws63-rs 总体架构

> 这是 ws63-rs 的 **Rust 代码架构**文档（与硬件手册 [`ws63-guide`](../../ws63-guide/) 互补：手册讲芯片，本文讲代码）。
> 完整评审台账见 [架构评审 2026-05](../review/architecture-review-2026-05.md)，整改排期见 [ROADMAP](../../ROADMAP.md)。

## 这是什么

ws63-rs 是面向 HiSilicon **WS63** RISC-V SoC（Wi-Fi 6 + BLE + SLE/星闪）的 Rust 嵌入式生态。
采用多仓库（git submodule）+ 单一 Cargo workspace 的组织方式。

## 组件与依赖链

```
ws63-svd (手写 CMSIS-SVD)  ──svd2rust──▶  ws63-pac (寄存器访问层)
                                              │
                                              ▼
                                          ws63-hal (安全驱动 HAL) ◀── embedded-hal 1.0
                                              │
                                              ▼
                                          ws63-examples/blinky (应用)
ws63-rt (启动/中断向量/链接脚本) ──────────────┘（提供运行时）

ws63-flashboot (实验性二级引导，独立，裸 MMIO)
ws63-RF        (闭源 Wi-Fi/BT/BLE/SLE blob + porting 接口；尚未接入)
ws63-guide     (中文硬件手册，Sphinx)
```

| 组件 | 类型 | 角色 | 架构文档 |
|------|------|------|----------|
| `ws63-svd` | submodule | SVD 真值 + 生成工具 | [ws63-svd.md](ws63-svd.md) |
| `ws63-pac` | submodule | svd2rust 生成的寄存器访问层 | [ws63-pac.md](ws63-pac.md) |
| `ws63-hal` | submodule | 手写安全驱动（31 个外设） | [ws63-hal.md](ws63-hal.md) |
| `ws63-rt` | submodule | 运行时：启动、中断向量、链接脚本 | [ws63-rt.md](ws63-rt.md) |
| `ws63-examples` | submodule | 应用示例（目前仅 blinky） | [ws63-examples.md](ws63-examples.md) |
| `ws63-flashboot` | in-tree | **实验性**二级引导（非安全启动） | [ws63-flashboot.md](ws63-flashboot.md) |
| `ws63-RF` | submodule | 闭源协议栈 blob + porting 接口 | [ws63-RF.md](ws63-RF.md) |
| `ws63-guide` | submodule | 中文硬件手册 | [ws63-guide.md](ws63-guide.md) |

## 核心设计模式

- **外设单例 + `'d` 生命周期**：`Peripherals::take()`（PAC 单例，critical-section 保护）分发 `'d` 参数化的 ZST 外设令牌；
  驱动经构造器消费令牌，借生命周期防 use-after-drop。
- **多实例外设**（UART/I2C/SPI/DMA）：用 `PhantomData<&'d T>` + 每实例构造器（`new_uart0`/`new_uart1`…）区分。
- **sealed trait**（`private.rs`）：`Sealed` 超 trait 防外部实现 `DmaWord`/`PeripheralInput`/`PeripheralOutput`。
- **`#![no_std]`**：无堆、无 `Vec`，数据缓冲用定长数组。
- **寄存器访问 `unsafe`**：裸 PAC 写封装在驱动方法内。

> 注意：评审发现若干"模式"目前是**零消费者的脚手架**（RAII 时钟守卫、DMA 安全 trait、async marker），
> 计划在 ROADMAP 阶段 2 删除（删无用、留哨兵）。详见各组件文档。

## 构建与目标（target）

- **默认 target**：builtin **`riscv32imc-unknown-none-elf`**（stable）。WS63 核**无原子（A）扩展**，
  此 target 不发 `lr/sc/amo`，`portable-atomic` 用 critical-section polyfill 处理 RMW。
  - 这是 2026-05 修正的关键：原默认 `riscv32imafc` 会发原子指令，在硅片上触发非法指令陷阱。
  - 软浮点（ilp32）。链接 ilp32f 的 vendor blob 时（ROADMAP 阶段 3）切自定义硬浮点 `rv32imfc` target
    （`ws63-rt/target-specs/riscv32imfc-unknown-none-elf.json`，需 nightly + `-Z build-std`）。
- **单一 PAC 实例**：根 `Cargo.toml` 用 `[patch.crates-io]` 把 `ws63-pac` 的 registry 依赖重定向到本地 submodule，
  保证全仓库只链接一个 PAC（否则 `DEVICE_PERIPHERALS` 单例静态重复、类型不兼容）。
- **default-members = 库**（`ws63-pac`/`ws63-hal`/`ws63-rt`）。两个二进制 crate 不在默认构建里：
  `ws63-flashboot` 是实验性；`blinky` 暂时无法链接（见下）。二者仍是 `members`，`cargo check --workspace` 覆盖。

常用命令：

```bash
cargo build --release          # 构建库（default-members）
cargo check --workspace        # 检查全部（含 blinky/flashboot，不链接）
cargo clippy --workspace --exclude ws63-flashboot -- -D warnings
cargo build -p blinky          # 显式构建示例（当前链接失败，见 ROADMAP 阶段 1）
cargo build -p flashboot       # 显式构建实验性 flashboot
```

## 已知的全局性问题（详见评审台账）

1. **连接性 0%**：芯片价值（Wi-Fi/BLE/SLE）尚未触及；`ws63-RF` 仅 blob + 未实现 porting。→ ROADMAP 阶段 3-5。
2. **示例无法链接**：`ws63-rt` 的链接脚本不传播到下游二进制（`cargo:rustc-link-arg` 来自库依赖不达 bin），
   blinky 因 trap 栈符号未定义而链接失败。→ ROADMAP 阶段 1。
3. **从未上板验证**：测试是 host 端恒真式；底座需要 HIL 冒烟。→ ROADMAP 阶段 1。
4. **正确性地雷**：中断模型错误、SPI/复位/超时缺陷等。→ ROADMAP 阶段 2。

## 参考资料

- **fbb_ws63**（`/root/fbb_ws63`）：官方 C SDK，寄存器/外设行为的真值来源。
- **esp-hal**（`/root/esp-hal`）：成熟 Rust HAL 参照（esp-radio/esp-rtos/embassy/众多示例）——WS63 的连接性轨迹可对标。
