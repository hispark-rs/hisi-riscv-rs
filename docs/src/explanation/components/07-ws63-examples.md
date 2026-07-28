# ws63-examples 架构

> 本文是 ws63-rs 组件深入文档的一部分，聚焦当前架构、职责边界和设计原因。当前优先级见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

## 职责与边界

`ws63-examples` 同时包含两类用途，不能把整个目录都当成面向最终用户的模板：

- **应用示例**：演示 `hisi-riscv-rt` + `hisi-hal` + PAC 的外设、启动、
  async/embassy 和自定义布局用法。
- **maintainer fixtures**：`rf_port_demo`、`wifi_blob_link`、`wifi_init_smoke` 分别冻结
  RF porting、archive/link 和完整连接性诊断行为。它们会直接触达
  `ws63-rf-rs`/`ws63-radio-sys` 等迁移内部面，不是新应用的依赖范本。

面向用户的 Wi-Fi happy path 是
[从模板新建 Wi-Fi 工程](../../how-to/09-new-project.md)；旧应用应按
[`ws63-rf-rs` 到 `hisi-rf` 迁移指南](../../how-to/12-migrate-ws63-rf-to-hisi-rf.md)
改用公开 facade。

- **负责**：提供可参考的 `#![no_std]` / `#![no_main]` 外设入口，以及有明确边界的
  maintainer/HIL fixtures。
- **不负责**：实现驱动、运行时或 RF 公共 API；这些分别属于 `hisi-hal`、
  `hisi-riscv-rt`/`hisi-rtos` 和 `hisi-rf`。示例级 smoke 也不替代 crate 单测或
  HAL embedded-test HIL。

当前 WS63 示例集合由根 `Cargo.toml` 的 `default-members` 和各示例 `Cargo.toml` 决定；**权威清单、标记串和真机/QEMU
状态只维护在** [示例目录与验证标记串](../../reference/02-examples.md)。本页不复制逐项列表，避免示例新增/删除时出现第二份事实源。

示例覆盖 GPIO/UART/Timer/GPIO IRQ/SPI/I2C/DMA、复位、semihosting、自定义内存布局、
async/embassy、RF blob 链接与 porting 层冒烟，以及 XIP flash 时钟 hazard 教学例。
另有 crate 内自测示例，如 `sched_selftest`、`net_selftest`。`examples/bs21` 和
`examples/bs20` 是隔离工作区，提供 BS2X 多芯片变种。

WS63 Wi-Fi init/scan/connect/DHCP/ping 已有真实硅片证据；它由 `wifi_init_smoke` 作为
maintainer HIL oracle 冻结，并由 `hisi-rs-template` 的 Wi-Fi starter 提供用户形态。
BLE/SLE 连接性仍是后续里程碑，不能把它们与已经完成的 Wi-Fi 基线混写成“连接性都缺失”。

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
hisi-rf            (用户 facade) ──hisi-rs-template Wi-Fi starter──────────────────────────────┘
ws63-rf-rs / ws63-radio-sys (迁移 oracle) ──rf_port_demo / wifi_blob_link / wifi_init_smoke──┘
```

每个普通示例的 `Cargo.toml` 只直接依赖所需的公开 crate（典型为 `hisi-hal` +
`hisi-riscv-rt`；async 示例再加 `embassy-*`）。三个 RF maintainer fixture 是受 CI
allowlist 约束的例外，不能据此推导用户依赖边界。用户 Wi-Fi 工程只直接依赖
`hisi-rf`；`ws63-radio-sys`、blob 和 RF runtime driver 只能作为传递实现依赖出现。
0.6.0 起，演示 DMA、interrupt/waker async、embassy 或 software reset 的示例会显式启用
`unstable`，避免让默认稳定 API 暗示这些实验面已毕业。

链接脚本传播问题已修：`hisi-riscv-rt` 经 `cargo:rustc-link-search` 导出 `hisi-riscv-link.x`（`hisi-riscv-rt/build.rs`），各二进制以自己的 `build.rs` 用 `-Thisi-riscv-link.x` 引入。因此当前 WS63 default-member 示例均可链接，默认 `cargo build` 即构建（仅 `ws63-flashboot` 仍单独排除——它是实验性、非 secure boot，见其 README）。注：`blinky/Cargo.toml` 历史上多声明了一条 `ws63-pac` 直接依赖而源码未用，该问题已随示例依赖整理清理。

## 关键设计

以 `blinky` 为最小模板说明裸机入口形态，其余示例在此之上各增量演示一个子系统：

- **入口与运行时集成**：用 `#[entry]`（来自 `hisi_riscv_rt`）声明 `fn main() -> !`，并自带 `#[panic_handler]`（自旋空转）。这是 `riscv-rt` 体系下的标准裸机入口形态。
- **GPIO 使用方式**：`blinky` 走现代 `AnyPin::init_output(OutputConfig)` 路径；`gpio_irq` 演示输入 + 中断路径。逐项示例状态以 [示例目录与验证标记串](../../reference/02-examples.md) 为准。
- **延时实现**：`blinky` 的 `delay_ms` 是**手写忙等**（按 240 MHz 估算，绕过 HAL timer），属「最小可演示」而非最佳实践；`async_delay` / `embassy_multitask` 演示了正确的 `DelayNs` / `Timer::after` 路径。
- **自定义内存布局**：`custom_memory` 演示用示例自带的 `memory.x` 覆盖 `hisi-riscv-rt` 的 bundled 链接脚本（`hisi-riscv-rt` 的默认 feature `bundled-memory-x`，关掉后由示例侧提供），从而不与 rt 冲突。
- **semihosting / CI 信号**：`semihost_selftest` 用 semihosting `exit()` 给 CI 一个免解析 UART 的 pass/fail 退出码。
- **异步**：`async_*` / `embassy_*` 用 hisi-hal 的 `async` / `embassy` feature + `unstable` + `embassy-executor`（机制见 [async-embassy.md](06-async-embassy.md)）。
- **RF maintainer fixtures**：`rf_port_demo` 只行使 allocator/securec/log shims；
  `wifi_blob_link` 负责最小 archive link；`wifi_init_smoke` 负责完整 vendor runtime、
  底层诊断和连接性 HIL。它们冻结迁移证据，不定义应用 API。

与参考实现的关系：esp-hal 示例普遍调用 `Delay` / embedded-hal trait；本生态的普通
示例也保持这一形态，而连接性用户体验由 `hisi-rf` facade 和模板维护。
