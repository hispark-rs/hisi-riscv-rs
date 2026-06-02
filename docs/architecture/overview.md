# ws63-rs 总体架构

> 这是 ws63-rs 的 **Rust 代码架构**文档（与硬件手册 [`ws63-guide`](../../ws63-guide/) 互补：手册讲芯片，本文讲代码）。
> 完整评审台账见 [架构评审 2026-05](../review/architecture-review-2026-05.md)，整改排期见 [ROADMAP](../../ROADMAP.md)。

## 这是什么

ws63-rs 是面向 HiSilicon **WS63** RISC-V SoC（Wi-Fi 6 + BLE + SLE/星闪）的 Rust 嵌入式生态。
采用多仓库（git submodule）+ 单一 Cargo workspace 的组织方式。

## 组件与依赖链

```
ws63-pac/ws63-svd (CMSIS-SVD) ──svd2rust──▶ ws63-pac (寄存器访问层)
                                               │
                                               ▼
                    ws63-hal (安全驱动 HAL) ◀── embedded-hal 1.0
                    │  ├─ feature "async"  ◀── embedded-hal-async / embedded-io-async
                    │  │     asynch::block_on + IrqSignal + 各驱动 on_interrupt(中断→waker)
                    │  └─ feature "embassy" ◀── embassy-time-driver / -queue-utils
                    │        embassy::Driver  (now()=TCXO 64位计数器, alarm=TIMER 通道)
                    ▼
              ws63-examples/* (blinky/uart_hello/timer_irq/gpio_irq/reset_demo/
                    │           dma_loopback/semihost_selftest/custom_memory/
                    │           async_delay/async_bus/embassy_multitask/embassy_async_io …)
                    ▲
                    └── embassy-executor (platform-riscv32, thread mode)  ← embassy 示例
ws63-rt (启动/中断向量/链接脚本 + critical-section + 工具链原子垫片) ─┘（运行时）

ws63-flashboot (实验性二级引导，独立，裸 MMIO)
ws63-rf-rs + ws63-RF (闭源 Wi-Fi/BT/BLE/SLE blob 的 Rust porting 层；连接性北极星)
ws63-guide     (中文硬件手册，Sphinx)
ws63-qemu      (姊妹仓：`-M ws63` QEMU，软件在环验证全部上述固件)
```

| 组件 | 类型 | 角色 | 架构文档 |
|------|------|------|----------|
| `ws63-pac` | submodule | svd2rust 生成的寄存器访问层 | [ws63-pac.md](ws63-pac.md) |
| `ws63-pac/ws63-svd` | 嵌套 submodule（在 ws63-pac 下） | SVD 真值 + 生成工具（归 pac 所有） | [ws63-svd.md](ws63-svd.md) |
| `ws63-hal` | submodule | 手写安全驱动 + 可选 `async`/`embassy`（见 [async-embassy.md](async-embassy.md)） | [ws63-hal.md](ws63-hal.md) |
| `ws63-rt` | submodule | 运行时：启动、中断向量、链接脚本、critical-section | [ws63-rt.md](ws63-rt.md) |
| `ws63-examples` | submodule | 应用示例（blinky/uart/timer/gpio/dma/reset/semihost/custom_memory + 4 个异步/embassy 示例） | [ws63-examples.md](ws63-examples.md) |
| `ws63-flashboot` | in-tree | **实验性**二级引导（非安全启动） | [ws63-flashboot.md](ws63-flashboot.md) |
| `ws63-rf-rs` | in-tree | 闭源 blob 的 Rust porting 层 | — |
| `ws63-rf-rs/ws63-RF` | submodule（嵌套路径） | 闭源协议栈 blob + porting 接口（归 rf-rs 所有） | [ws63-RF.md](ws63-RF.md) |
| `ws63-guide` | submodule | 中文硬件手册 | [ws63-guide.md](ws63-guide.md) |

## 核心设计模式

- **外设单例 + `'d` 生命周期**：`Peripherals::take()`（PAC 单例，critical-section 保护）分发 `'d` 参数化的 ZST 外设令牌；
  驱动经构造器消费令牌，借生命周期防 use-after-drop。
- **多实例外设**（UART/I2C/SPI/DMA）：用 `PhantomData<&'d T>` + 每实例构造器（`new_uart0`/`new_uart1`…）区分。
- **sealed trait**（`private.rs`）：`Sealed` 超 trait 防外部实现 `DmaWord`/`PeripheralInput`/`PeripheralOutput`。
- **`#![no_std]`**：无堆、无 `Vec`，数据缓冲用定长数组。
- **寄存器访问 `unsafe`**：裸 PAC 写封装在驱动方法内。

- **异步**（`async`/`embassy` feature，见 [async-embassy.md](async-embassy.md)）：中断 + waker 驱动的
  `embedded-hal-async`/`embedded-io-async` 驱动 + 一个 embassy-time `Driver`，跑在无原子的 WS63 上
  （portable-atomic + critical-section 垫片）。驱动只暴露 `on_interrupt` 钩子、不自动装 ISR。

> 注意：早先评审里的"零消费者脚手架"（DMA 安全 trait、空的 async marker）已处理 —— async marker 已删，
> **真正的异步层已实现并验证**（见上）；RAII 时钟守卫等仍按 ROADMAP 阶段 2 评估。详见各组件文档。

## 构建与目标（target）

- **默认 target / 工具链**：**`riscv32imfc-unknown-none-elf`**（RV32IMFC，硬件单精度浮点 `ilp32f`，无原子），
  由自定义 **`ws63`** 工具链提供（stable rustc 把该 target 烤成 builtin，故**无需 `-Z build-std`**，
  工具链自带预编译 core/alloc）。`rust-toolchain.toml` pin `channel = "ws63"`；安装见
  <https://github.com/sanchuanhehe/ws63-rust-toolchain>（`rustup toolchain link ws63 …`）。
  - WS63 核**无原子（A）扩展**：该 target 用 forced-atomics + no-CAS，原子 load/store 降为 ld/st、
    RMW 走 `portable-atomic` 的 critical-section polyfill，**不发 `lr/sc/amo`**。原默认 `riscv32imafc`
    会发原子指令、在硅片上触发非法指令陷阱，已弃用。
  - 历史：2026-05-31 阶段 0 曾先用 builtin `riscv32imc`（软浮点、stable、免 build-std）做过渡；
    随后切到 `ws63` 硬浮点工具链（与 ilp32f vendor blob ABI 一致，为阶段 3 链接做准备）。
- **单一 PAC 实例**：根 `Cargo.toml` 用 `[patch.crates-io]` 把 `ws63-pac` 的 registry 依赖重定向到本地 submodule，
  保证全仓库只链接一个 PAC（否则 `DEVICE_PERIPHERALS` 单例静态重复、类型不兼容）。
- **default-members = 库 + blinky**（`ws63-pac`/`ws63-hal`/`ws63-rt`/`ws63-examples/blinky`）。
  blinky 经 ws63-rt 导出的链接脚本可正常链接（`ws63-rt/build.rs` 用 `cargo:rustc-link-search` 导出脚本目录 +
  `ws63-link.x` 包装脚本，blinky 的 `build.rs` 以 `-Tws63-link.x` 引入）。实验性的 `ws63-flashboot` 不在默认构建里，
  仍是 `member`，`cargo check --workspace` 覆盖。

常用命令：

```bash
cargo build --release          # 构建库（default-members）
cargo check --workspace        # 检查全部（含 blinky/flashboot，不链接）
cargo clippy --workspace --exclude ws63-flashboot -- -D warnings
cargo build -p blinky             # 示例（已可链接；包含在默认构建中）
cargo build -p ws63-flashboot     # 显式构建实验性 flashboot（包名是 ws63-flashboot）
```

## 已知的全局性问题（详见评审台账）

1. **连接性 0%**：芯片价值（Wi-Fi/BLE/SLE）尚未触及；`ws63-RF` 仅 blob + 未实现 porting。→ ROADMAP 阶段 3-5。
2. ~~示例无法链接~~ **（已修，阶段 1）**：`ws63-rt` 的链接脚本原先经 `cargo:rustc-link-arg` 注入、不传播到下游二进制；
   已改为 `cargo:rustc-link-search` + `ws63-link.x` 包装脚本，blinky 现可链接。
3. **从未上板验证**：测试是 host 端恒真式；底座仍需 HIL 冒烟（真机烧 blinky/UART）。→ ROADMAP 阶段 1（剩余部分）。
4. **正确性地雷**：中断模型错误、SPI/复位/超时缺陷等。→ ROADMAP 阶段 2。

## 参考资料

- **fbb_ws63**（`/root/fbb_ws63`）：官方 C SDK，寄存器/外设行为的真值来源。
- **esp-hal**（`/root/esp-hal`）：成熟 Rust HAL 参照（esp-radio/esp-rtos/embassy/众多示例）——WS63 的连接性轨迹可对标。
