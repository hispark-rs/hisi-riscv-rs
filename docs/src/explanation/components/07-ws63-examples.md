# ws63-examples 架构

> 本文是 ws63-rs 组件深入文档的一部分，聚焦当前架构、职责边界和设计原因。当前优先级见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

## 职责与边界

`ws63-examples` 是面向最终用户的**应用示例集合**，演示 WS63、BS21 等多芯片的固件组合。例子展示如何把 `hisi-riscv-rt`（启动）+ `hisi-hal`（驱动，支持 `chip-ws63`/实验性 `chip-bs21` + `unstable` 特性）+ PAC（`ws63-pac` 或 `bs2x-pac`，见 `crates/chips/<family>/`）+ 连接性场景下的 `ws63-rf-rs`（RF porting），组合成可烧录的裸机固件。

- **负责**：提供可参考的 `#![no_std]` / `#![no_main]` 入口，以及各外设/子系统的最小调用示例（GPIO/UART/Timer/DMA、中断、复位、semihosting、自定义内存布局、async/embassy、RF porting）。
- **不负责**：实现任何驱动或运行时逻辑（这些属于 `hisi-hal` / `hisi-riscv-rt` / `ws63-rf-rs`）；不承担系统测试覆盖职责（单测在各 crate 内）。

当前 WS63 示例集合由根 `Cargo.toml` 的 `default-members` 和各示例 `Cargo.toml` 决定；**权威清单、标记串和真机/QEMU
状态只维护在** [示例目录与验证标记串](../../reference/02-examples.md)。本页不复制逐项列表，避免示例新增/删除时出现第二份事实源。

示例覆盖 GPIO/UART/Timer/GPIO IRQ/SPI/I2C/DMA、复位、semihosting、自定义内存布局、async/embassy、RF blob 链接与
porting 层冒烟，以及 XIP flash 时钟 hazard 教学例。另有 crate 内自测示例（在 `chips/ws63/rf/examples/`），如
`sched_selftest`（协作调度器自测）、`net_selftest`（netif→smoltcp 自测）。`examples/bs21` 和 `examples/bs20`
是隔离工作区，提供 BS2X 多芯片变种。仍缺真实**连接性**（Wi-Fi/BLE/SLE 实际链路）示例（北极星，待 blob 上板 HIL）。

## 在依赖链中的位置

examples 位于整条依赖链的**最下游**（叶子节点），消费上游各 crate：

```console
crates/chips/ws63/ws63-pac/ws63-svd (XML)      crates/chips/bs2x/bs2x-pac/bs2x-svd (XML)
       │                                            │
       └─> ws63-pac   (svd2rust)                   └─> bs2x-pac   (svd2rust)
            │                                            │
            └─> hisi-hal   (手写安全驱动；chip-bs21 需 unstable、async/embassy feature)
                 │
                 ├─> examples/ws63/*   (WS63 示例)
                 ├─> examples/bs21/*   (BS21 示例，隔离)
                 └─> examples/bs20/*   (BS20 示例，隔离)
hisi-riscv-rt      (启动汇编 / 链接脚本 / 中断向量) ──#[entry] + 导出 hisi-riscv-link.x──┘
ws63-rf-rs   (RF porting 层) ──rf_port_demo / wifi_blob_link / wifi_init_smoke──┘
```

每个示例的 `Cargo.toml` 直接依赖其所需 crate（典型为 `hisi-hal` + `hisi-riscv-rt`；async 示例再加 `embassy-*`；RF 示例加 `ws63-rf-rs`）。0.6.0 起，演示 DMA、interrupt/waker async、embassy 或 software reset 的示例会显式启用 `unstable`，避免让默认稳定 API 暗示这些实验面已毕业。

链接脚本传播问题已修：`hisi-riscv-rt` 经 `cargo:rustc-link-search` 导出 `hisi-riscv-link.x`（`hisi-riscv-rt/build.rs`），各二进制以自己的 `build.rs` 用 `-Thisi-riscv-link.x` 引入。因此当前 WS63 default-member 示例均可链接，默认 `cargo build` 即构建（仅 `ws63-flashboot` 仍单独排除——它是实验性、非 secure boot，见其 README）。注：`blinky/Cargo.toml` 历史上多声明了一条 `ws63-pac` 直接依赖而源码未用，该问题已随示例依赖整理清理。

## 关键设计

以 `blinky` 为最小模板说明裸机入口形态，其余示例在此之上各增量演示一个子系统：

- **入口与运行时集成**：用 `#[entry]`（来自 `hisi_riscv_rt`）声明 `fn main() -> !`，并自带 `#[panic_handler]`（自旋空转）。这是 `riscv-rt` 体系下的标准裸机入口形态。
- **GPIO 使用方式**：`blinky` 走现代 `AnyPin::init_output(OutputConfig)` 路径；`gpio_irq` 演示输入 + 中断路径。逐项示例状态以 [示例目录与验证标记串](../../reference/02-examples.md) 为准。
- **延时实现**：`blinky` 的 `delay_ms` 是**手写忙等**（按 240 MHz 估算，绕过 HAL timer），属「最小可演示」而非最佳实践；`async_delay` / `embassy_multitask` 演示了正确的 `DelayNs` / `Timer::after` 路径。
- **自定义内存布局**：`custom_memory` 演示用示例自带的 `memory.x` 覆盖 `hisi-riscv-rt` 的 bundled 链接脚本（`hisi-riscv-rt` 的默认 feature `bundled-memory-x`，关掉后由示例侧提供），从而不与 rt 冲突。
- **semihosting / CI 信号**：`semihost_selftest` 用 semihosting `exit()` 给 CI 一个免解析 UART 的 pass/fail 退出码。
- **异步**：`async_*` / `embassy_*` 用 hisi-hal 的 `async` / `embassy` feature + `unstable` + `embassy-executor`（机制见 [async-embassy.md](06-async-embassy.md)）。
- **RF porting**：`rf_port_demo` 只行使 allocator/securec/log shims；`wifi_blob_link` 负责最小 archive link，`wifi_init_smoke` 负责完整 vendor runtime 与连接性路径，避免旧 demo 维护一套不完整的伪 blob 环境。

与参考实现的关系：esp-hal 示例普遍调用 `Delay` / embedded-hal trait；ws63 示例集现已从「单一点灯」扩展为覆盖各外设 + async + RF porting 的一组最小演示。
