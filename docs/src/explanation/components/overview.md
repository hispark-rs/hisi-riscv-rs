# ws63-rs 总体架构

> 这是 ws63-rs 的 **Rust 代码架构**文档（与硬件手册 [`ws63-guide`](https://github.com/hispark-rs/ws63-guide) 互补：手册讲芯片，本文讲代码）。
> 完整评审台账见 [架构评审 2026-05](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/review/architecture-review-2026-05.md)，整改排期见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

## 这是什么

ws63-rs 是面向 HiSilicon **WS63 + BS2X**（BS21/BS20/BS22）RISC-V SoC 族的 Rust 嵌入式生态。WS63 覆盖 Wi-Fi 6 / BLE / SLE/星闪，BS2X 覆盖 BLE/SLE（M1/M2）。
采用多仓库（git submodule）+ 单一 Cargo workspace 的组织方式。

## 组件与依赖链

```console
┌─ crates/pac ────────────────────────────────────┐
│ ├─ ws63-pac/ws63-svd (CMSIS-SVD) ──svd2rust──┐ │
│ └─ bs2x-pac/bs2x-svd (CMSIS-SVD) ─────────────┤ │
│                                            ▼ ▼ │
│                    hisi-riscv-hal (多芯片 HAL，chip-ws63/chip-bs21 feature) ◀── embedded-hal 1.0
│                    │  ├─ feature "async"  ◀── embedded-hal-async / embedded-io-async
│                    │  │     asynch::block_on + IrqSignal + 各驱动 on_interrupt(中断→waker)
│                    │  └─ feature "embassy" ◀── embassy-time-driver / -queue-utils
│                    │        embassy::Driver  (now()=TCXO 64位计数器, alarm=TIMER 通道)
│                    ▼
│ examples/ws63/* (blinky/uart_hello/timer_irq/gpio_irq/reset_demo/dma_loopback/
│ examples/bs21/* (blinky/spi_loopback/i2c_scan/gadc_read/hid_demo/pwm_wdt/clock_rng/dma_mem)
│                    │    async_delay/async_bus/embassy_multitask/embassy_async_io/wifi_blob_link/rf_port_demo/semihost…
│                    ▲
│                    └── embassy-executor (platform-riscv32, thread mode)  ← embassy 示例
│ hisi-riscv-rt (启动/中断向量/链接脚本 + critical-section + 工具链原子垫片) ─┘（运行时）
│ chips/ws63/flashboot (实验性二级引导，独立，裸 MMIO)
│ chips/ws63/rf + ws63-RF (WS63 Wi-Fi/BT/BLE/SLE blob + Rust porting 层)
│ chips/ws63/guide (WS63 中文硬件手册，Sphinx)
│ chips/bs2x/guide (BS2X 中文硬件手册，Sphinx)
│
│ ws63-qemu (姊妹仓：`-M ws63/bs21/bs22/bs20` QEMU，WS63/BS2X 软件在环验证)
│ probe-rs fork hispark-rs/add-hisilicon-ws63-bs21（RISC-V-DM + HiSilicon DebugSequence + flash-algorithm）
└──────────────────────────────────────────────────┘
```

| 组件 | 类型 | 角色 | 架构文档 |
|------|------|------|----------|
| `crates/pac/ws63-pac` | submodule | WS63 svd2rust 生成的寄存器访问层 | [ws63-pac.md](ws63-pac.md) |
| `crates/pac/ws63-pac/ws63-svd` | 嵌套 submodule | WS63 SVD 真值 + 生成工具 | [ws63-svd.md](ws63-svd.md) |
| `crates/pac/bs2x-pac` | submodule | BS2X（BS21/BS20/BS22）svd2rust 生成的寄存器访问层 | [ws63-pac.md](ws63-pac.md) |
| `crates/pac/bs2x-pac/bs2x-svd` | 嵌套 submodule | BS2X SVD 真值 + 生成工具 | [ws63-svd.md](ws63-svd.md) |
| `crates/hisi-riscv-hal` | submodule | 多芯片 HAL（chip-ws63/chip-bs21 feature）+ 可选 `async`/`embassy` | [hisi-riscv-hal.md](hisi-riscv-hal.md) |
| `crates/hisi-riscv-rt` | submodule | 运行时：启动、中断向量、链接脚本、critical-section | [hisi-riscv-rt.md](hisi-riscv-rt.md) |
| `examples/ws63/*` | in-tree 独立工作区 | WS63 应用示例（blinky/uart/timer/gpio/dma/reset/async/embassy/wifi_blob_link/rf_port_demo…） | [ws63-examples.md](ws63-examples.md) |
| `examples/bs21/*` | in-tree 独立工作区 | BS2X 应用示例（blinky/spi/i2c/gadc/keyscan/qdec/rtc/trng/wdt/dma/pdm/usb…，全外设功能覆盖） | [ws63-examples.md](ws63-examples.md) |
| `examples/bs20/` | in-tree 独立工作区 | BS20（M1）示例 | — |
| `chips/ws63/flashboot` | in-tree | **实验性**二级引导（非安全启动） | [ws63-flashboot.md](ws63-flashboot.md) |
| `chips/ws63/rf/` | in-tree | WS63 Wi-Fi porting 层 `ws63-rf-rs` | — |
| `chips/ws63/rf/ws63-RF` | submodule（嵌套） | WS63 闭源协议栈 blob + porting 接口 | [ws63-RF.md](ws63-RF.md) |
| `chips/ws63/guide` | submodule | WS63 中文硬件手册（Sphinx） | [ws63-guide.md](ws63-guide.md) |
| `chips/bs2x/guide` | submodule | BS2X 中文硬件手册（Sphinx） | — |

## 核心设计模式
## 调试支持

- **probe-rs**（新）：fork [`hispark-rs/probe-rs`](https://github.com/hispark-rs/probe-rs) 分支 `add-hisilicon-ws63-bs21`，实现 RISC-V Debug Module + HiSilicon 厂商 DebugSequence（mem-AP DTM）+ flash-algorithm crate。软件完整，待硅片真机验证。用法：`probe-rs run --chip ws63 <bin>` 进行实时调试与 on-silicon 烧录。



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
  由自定义 **`hisi-riscv`** 工具链提供（stable rustc 把该 target 烤成 builtin，故**无需 `-Z build-std`**，
  工具链自带预编译 core/alloc）。`rust-toolchain.toml` pin `channel = "hisi-riscv"`；安装见
  <https://github.com/hispark-rs/hisi-riscv-rust-toolchain>（`rustup toolchain link hisi-riscv …`）。
  - WS63 核**无原子（A）扩展**：该 target 用 forced-atomics + no-CAS，原子 load/store 降为 ld/st、
    RMW 走 `portable-atomic` 的 critical-section polyfill，**不发 `lr/sc/amo`**。原默认 `riscv32imafc`
    会发原子指令、在硅片上触发非法指令陷阱，已弃用。
  - 历史：2026-05-31 阶段 0 曾先用 builtin `riscv32imc`（软浮点、stable、免 build-std）做过渡；
    随后切到 `ws63` 硬浮点工具链（与 ilp32f vendor blob ABI 一致，为阶段 3 链接做准备）。
- **单一 PAC 实例**：根 `Cargo.toml` 用 `[patch.crates-io]` 把 `ws63-pac` 的 registry 依赖重定向到本地 submodule，
  保证全仓库只链接一个 PAC（否则 `DEVICE_PERIPHERALS` 单例静态重复、类型不兼容）。
- **default-members = 库 + blinky**（`ws63-pac`/`hisi-riscv-hal`/`hisi-riscv-rt`/`examples/ws63/blinky`）。
  blinky 经 hisi-riscv-rt 导出的链接脚本可正常链接（`hisi-riscv-rt/build.rs` 用 `cargo:rustc-link-search` 导出脚本目录 +
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

1. **连接性状态**：
   - **WS63 Wi-Fi**（ROADMAP 阶段 3-5）：porting 层 + 链接 + netif→smoltcp 已实现并在 QEMU 自测（阶段 4），符号闭合已达成；真机连通待 HIL（阶段 5）。
   - **BS2X BLE/SLE**（已评估）：radio MMIO 模拟是死胡同（B_CTL 0x59000000 为 56 个写只 PHY 寄存器 + IRQ-26 blob 事件墙），HCI 边界为 blob-on-blob（无法干预）；完整分析见 `hisi-riscv-qemu/docs/bs21-connectivity-feasibility.md`。
2. ~~示例无法链接~~ **（已修，阶段 1）**；~~多芯片支持~~ **（已实现）**：hisi-riscv-hal 用 `chip-ws63`/`chip-bs21` feature 区分，二选一；examples 分为 WS63（submodule）、BS2X（in-tree 独立工作区）。
3. **硬件在环（HIL）进度**：QEMU 软件在环已成熟（WS63/BS21/BS22/BS20 均支持，全外设功能覆盖），HIL 脚手架已就位（烧录脚本 + 冒烟框架）；待真机板卡到位进行 blinky/UART/中断冒烟。
4. **正确性修复状态**：中断（LOCIEN/LOCIPRI/LOCIPCLR）、SPI（两级时钟）、超时（wait_until 有界）、复位（GLB_CTL + SYS_RST_RECORD）等核心问题已修（ROADMAP 阶段 2）；QEMU 软件在环验证已覆盖中断、复位、DMA、timer；上板验证仍待硬件（时钟精度、外设时序）。

## 参考资料
## 多芯片支持细节

- **PAC 组织**：`crates/pac/ws63-pac` 和 `crates/pac/bs2x-pac` 各自独立（SVD 源→svd2rust 生成），root `Cargo.toml` 经 `[patch.crates-io]` 统一链接到本地实例（保证单一 PAC 版本）。
- **HAL 多芯片**：`hisi-riscv-hal` 通过 `chip-ws63`（default）和 `chip-bs21` feature 区分，条件编译外设模块（WS63 含 Wi-Fi 相关，BS2X 含 GADC/KEYSCAN/QDEC/RTC/TRNG 等 M1 外设）。
- **示例组织**：WS63 示例遵循原 submodule 路径 `examples/ws63/`；BS2X 示例为 in-tree 独立工作区 `examples/bs21/` 和 `examples/bs20/`（避免 submodule 膨胀）。
- **QEMU 支持**：ws63-qemu 已支持 `-M ws63`（8 GB 地址空间）、`-M bs21`（不同时钟/外设）、`-M bs22`/`-M bs20`（M2/M1），完整的 QEMU 外设仿真（UART/GPIO/Timer/DMA/SDMA/SPI/I2C/WDT/PDM/USB DWC OTG 等）。



- **fbb_ws63**（`/root/fbb_ws63`）：官方 C SDK，寄存器/外设行为的真值来源。
- **esp-hal**（`/root/esp-hal`）：成熟 Rust HAL 参照（esp-radio/esp-rtos/embassy/众多示例）——WS63 的连接性轨迹可对标。
