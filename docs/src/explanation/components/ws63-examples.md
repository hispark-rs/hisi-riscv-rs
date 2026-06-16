# ws63-examples 架构与评审

> 本文是 ws63-rs 架构文档的一部分。完整评审台账见 [架构评审 2026-05](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/review/architecture-review-2026-05.md)，整改排期见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

## 职责与边界

`ws63-examples` 是面向最终用户的**应用示例集合**，演示 WS63、BS21 等多芯片的固件组合。例子展示如何把 `hisi-riscv-rt`（启动）+ `hisi-riscv-hal`（驱动，支持 `chip-ws63`/`chip-bs21` 特性）+ PAC（`ws63-pac` 或 `bs2x-pac`，见 `crates/pac/`）+ 连接性场景下的 `ws63-rf-rs`（RF porting），组合成可烧录的裸机固件。

- **负责**：提供可参考的 `#![no_std]` / `#![no_main]` 入口，以及各外设/子系统的最小调用示例（GPIO/UART/Timer/DMA、中断、复位、semihosting、自定义内存布局、async/embassy、RF porting）。
- **不负责**：实现任何驱动或运行时逻辑（这些属于 `hisi-riscv-hal` / `hisi-riscv-rt` / `ws63-rf-rs`）；不承担系统测试覆盖职责（单测在各 crate 内）。

当前含 **14 个工作区示例**，全部在 `default-members`（`Cargo.toml:30-48`），默认 `cargo build` 即构建：

| 示例 | 演示内容 |
|------|----------|
| `blinky` | GPIO 点灯（最小裸机入口模板） |
| `uart_hello` | UART 输出 |
| `timer_irq` | Timer 中断（WS63 自定义 LOCI* 中断模型） |
| `gpio_irq` | GPIO 输入 + 边沿/电平中断 |
| `reset_demo` | `software_reset` / `reset_reason` 往返 |
| `dma_loopback` | DMA mem-to-mem 搬运 |
| `semihost_selftest` | semihosting `exit()`/print（CI 免解析 UART 即得 pass/fail） |
| `custom_memory` | 示例自带 `memory.x` 覆盖默认内存布局 |
| `async_delay` | `embedded-hal-async` `DelayNs` + `asynch::block_on` |
| `async_bus` | 异步 SPI/I2C 总线（`SpiBus`/`I2c`） |
| `embassy_multitask` | embassy-executor 多任务 + embassy-time |
| `embassy_async_io` | embassy 下的异步 UART I/O |
| `wifi_blob_link` | 把 vendor RF blob 链入镜像（符号闭合冒烟） |
| `rf_port_demo` | 经 `ws63-rf-rs` 调用 porting 层 + FRW/HCC 数据通路 |

另有 2 个 crate 内自测示例（在 `chips/ws63/rf/examples/`）：`sched_selftest`（协作调度器自测）、`net_selftest`（netif→smoltcp 自测）。此外 `examples/bs21`（BS21 examples，隔离工作区）和 `examples/bs20`（BS20 examples，隔离工作区）提供多芯片变种。所有示例全部在姊妹仓 [`ws63-qemu`](https://github.com/hispark-rs/hisi-riscv-qemu) 经 `scripts/smoke-test.sh` 端到端验证。仍缺真实**连接性**（Wi-Fi/BLE/SLE 实际链路）示例（北极星，待 blob 上板 HIL）。

## 在依赖链中的位置

examples 位于整条依赖链的**最下游**（叶子节点），消费上游各 crate：

```console
crates/pac/ws63-pac/ws63-svd (XML)      crates/pac/bs2x-pac/bs2x-svd (XML)
       │                                            │
       └─> ws63-pac   (svd2rust)                   └─> bs2x-pac   (svd2rust)
            │                                            │
            └─> hisi-riscv-hal   (手写安全驱动；chip-ws63/chip-bs21、async/embassy feature)
                 │
                 ├─> examples/ws63/*   (WS63 示例)
                 ├─> examples/bs21/*   (BS21 示例，隔离)
                 └─> examples/bs20/*   (BS20 示例，隔离)
hisi-riscv-rt      (启动汇编 / 链接脚本 / 中断向量) ──#[entry] + 导出 ws63-link.x──┘
ws63-rf-rs   (RF porting 层) ──仅 rf_port_demo / wifi_blob_link 用──┘
```

每个示例的 `Cargo.toml` 直接依赖其所需 crate（典型为 `hisi-riscv-hal` + `hisi-riscv-rt`；async 示例再加 `embassy-*`；RF 示例加 `ws63-rf-rs`）。

链接脚本传播问题已修：`hisi-riscv-rt` 经 `cargo:rustc-link-search` 导出 `ws63-link.x`（`hisi-riscv-rt/build.rs`），各二进制以自己的 `build.rs` 用 `-Tws63-link.x` 引入。因此**全部 14 个示例现已可链接并都在 `default-members`**，默认 `cargo build` 即构建并产 `.bin`（仅 `ws63-flashboot` 仍单独排除——它是实验性、非 secure boot，见其 README）。注：`blinky/Cargo.toml` 历史上多声明了一条 `ws63-pac` 直接依赖而源码未用（死代码，排期阶段 2 清理）。

## 关键设计

以 `blinky` 为最小模板说明裸机入口形态，其余示例在此之上各增量演示一个子系统：

- **入口与运行时集成**：用 `#[entry]`（来自 `hisi_riscv_rt`）声明 `fn main() -> !`，并自带 `#[panic_handler]`（自旋空转）。这是 `riscv-rt` 体系下的标准裸机入口形态。
- **GPIO 使用方式**：`blinky` 用 **legacy 类型态 GPIO**（`create_output_pin` + `set_high()/set_low()`）；`gpio_irq` 则演示新的输入 + 中断路径。HAL 的 `OutputConfig`/`InputConfig` 构建器 API 已落地，示例正逐步覆盖。
- **延时实现**：`blinky` 的 `delay_ms` 是**手写忙等**（按 240 MHz 估算，绕过 HAL timer），属「最小可演示」而非最佳实践；`async_delay` / `embassy_multitask` 演示了正确的 `DelayNs` / `Timer::after` 路径。
- **自定义内存布局**：`custom_memory` 演示用示例自带的 `memory.x` 覆盖 `hisi-riscv-rt` 的 bundled 链接脚本（`hisi-riscv-rt` 的默认 feature `bundled-memory-x`，关掉后由示例侧提供），从而不与 rt 冲突。
- **semihosting / CI 信号**：`semihost_selftest` 用 semihosting `exit()` 给 CI 一个免解析 UART 的 pass/fail 退出码。
- **异步**：`async_*` / `embassy_*` 用 hisi-riscv-hal 的 `async` / `embassy` feature + `embassy-executor`（机制见 [async-embassy.md](async-embassy.md)）。
- **RF porting**：`rf_port_demo` 经 `ws63-rf-rs` 行使 porting 函数，并把 vendor ROM-data blob 链入镜像（`g_dmac_alg_main` / `g_mac_res_etc` 在 rf-rs 解析）。

与参考实现的关系：esp-hal 示例普遍调用 `Delay` / embedded-hal trait；ws63 示例集现已从「单一点灯」扩展为覆盖各外设 + async + RF porting 的一组最小演示。

## 评审发现

### 优点

- 入口形态正确：`#[entry]` + `#[panic_handler]` 的裸机骨架完整，`blinky` 可作后续示例的模板。
- 覆盖面已大幅扩展：GPIO / UART / Timer / DMA + 中断 + 复位 + semihosting + 自定义内存 + async/embassy + RF porting，14 例全部在 ws63-qemu 端到端冒烟。
- 链接已打通且诚实标注：14 例全部在 `default-members`，`cargo build` 默认即构建；`ws63-flashboot` 的排除附了原因注释。

### 问题

| 严重度 | 类别 | 问题 | 状态 |
|--------|------|------|------|
| 高 | 构建 | （曾）`blinky` 无法链接：lib 依赖的 `cargo:rustc-link-arg` 不传播到下游二进制 | ✅ 已修：`hisi-riscv-rt` 导出 `ws63-link.x` + 各 `build.rs` 用 `-Tws63-link.x`，14 例全部可链接并回到 `default-members` |
| 高 | 方向 | （曾）唯一示例（blinky）+ 手写忙等，无法证明其余驱动可用 | ✅ 大部已破：现有 UART/Timer/GPIO/DMA + async SPI/I2C 等 13 个额外示例 |
| 中 | 演示覆盖 | `blinky` 仍用 legacy `create_output_pin`，未直接演示 `OutputConfig`/`InputConfig` | 🟡 `gpio_irq` 已演示输入/中断路径；`blinky` 升级待排期 |
| 中 | 文档 | 旧构建指引曾指向自定义 JSON target | ✅ 已统一为 builtin `riscv32imfc-unknown-none-elf`（硬浮点 ilp32f、无原子；2026-05-31 曾过渡用 stable `riscv32imc`） |
| 低 | 依赖 | `blinky/Cargo.toml` 多声明 `ws63-pac` 直接依赖，源码未用 | 🟡 排期阶段 2 死代码清理 |
| — | 连接性 | 缺真实 Wi-Fi/BLE/SLE 链路示例 | 🔴 待 blob 上板 HIL（阶段 5） |

## 改进项与排期

- **ROADMAP 阶段 1（已大部完成）**：链接脚本传播已修、示例覆盖面已扩。剩余：把 `blinky` 升级为使用 `OutputConfig`/`InputConfig` 配置 API；真机上板点灯验证。
- **ROADMAP 阶段 2（死代码清理）**：清理 `blinky` 冗余的 `ws63-pac` 直接依赖等。
- **ROADMAP 阶段 5（连接性示例）** 🔴：在 blob 上板（HIL）后新增 Wi-Fi/BLE/SLE 真实链路示例，使示例集真正覆盖 SoC 核心能力。
- **ROADMAP 阶段 6（async）** ✅ 已完成：`async_delay` / `async_bus` / `embassy_multitask` / `embassy_async_io` 四个异步示例已落地（依赖 HAL 的 `async`/`embassy` 支持，见 [async-embassy.md](async-embassy.md)）。
