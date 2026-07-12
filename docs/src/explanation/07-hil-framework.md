# HIL 测试框架

这一篇讲硬件在环（hardware-in-the-loop, HIL）测试的**设计取舍**：为什么 QEMU 已经能跑还要上板、
为什么 HAL 驱动测试走 `embedded-test` + semihosting、为什么示例 smoke 仍用 UART 标记串，以及当前
bring-up 状态的边界。操作步骤见 [运行 HIL 测试](../how-to/07-run-hil-tests.md)，脚本和 runner 变量见
[HIL 脚本与 runner 环境变量](../reference/07-hil-markers.md)；示例标记串见
[示例目录与验证标记串](../reference/02-examples.md)。

## HIL 存在的意义：验 QEMU 验不了的那部分

[QEMU](06-qemu-model.md) 已经把很多软件逻辑钉住了：内存布局、启动序列、外设寄存器模型、
中断投递、DMA 搬运都能在软件在环里跑。但 QEMU 本质上模拟不了真实的物理现实：
真实时钟频率、真实波特、真实外设时序、真实引导链、RF。

所以 HIL 的定位很明确：**它不是再验一遍逻辑，而是验 QEMU 验不了的物理现实。**
一个固件如果在 QEMU 里跑通、却在硅片上出问题，那个问题常落在“真实时钟/时序/外设/接线”
这些类别，而不是纯逻辑 bug。这个前提决定了 HIL triage 的思路：先问硬件现实是否和模型一致，
再回头看软件逻辑。

## 两条观测通道

HIL 现在不是单一脚本，而是两条互补轨道。

### HAL 驱动级：embedded-test + semihosting

`hisi-hal/tests/hil.rs` 是 `harness = false` 的 RISC-V integration test，测试入口由
[`embedded-test`](https://github.com/probe-rs/embedded-test) 提供。测试 ELF 仍由 `hisi-riscv-rt`
启动：rt 负责 reset vector、链接脚本、critical-section impl，以及 WS63 `boot-header` 让测试 ELF 可被
flashboot 引导；`embedded-test` 导出 `main`、panic handler 和 semihosting 测试调度。

运行时 `hil/embedded-test-runner.sh` 做两件事：

1. `hisi-fwpkg patch-hash <test-elf>`：把 link-time 0x300 header 里的 body SHA-256 补真实。
2. `probe-rs run --chip WS63 ... <test-elf>`：烧录/启动测试 ELF，并通过 RISC-V semihosting 逐个运行
   `#[test]`，把 libtest 兼容结果回传给 `cargo test`。

这条轨道适合做**驱动级证据**：寄存器窗口是否 live、配置是否 latch、typed config 是否拒绝非法值、
PAC singleton 与 critical-section 是否在真核上成立。它不依赖 UART banner，也不要求每个测试写一个示例 crate。
0.6.0 的 stable API 毕业证据主要来自这条轨道。

### 示例级：UART 标记串 smoke

`hil/hil-smoke.sh` 仍然有价值，但它验证的是另一件事：一个完整示例镜像在真硅片上启动后，
是否通过 UART 打出与 QEMU smoke 对齐的标记串。例如 `timer_irq` 打印 `timer irq #N`，就同时观察到
真实启动链、真实 UART、真实 timer 中断节奏和实际板级接线。

UART 标记串的优点是低门槛且能做 QEMU↔silicon parity：同一行输出，QEMU chardev 和真串口都能看到。
它特别容易暴露 QEMU 抓不到的时钟/波特问题；缺点是它只适合示例级端到端 smoke，不适合作为每个
HAL API 的细粒度证据源。

## 两条工作流

驱动级工作流：

1. 在 `hisi-hal/tests/hil.rs` 注册 `embedded-test` 用例，具体实现放在 `tests/hil/*.rs`。
2. 本地先 `cargo test -p hisi-hal ... --test hil --no-run` 确认测试 ELF 能链接。
3. 用 `hil/embedded-test-runner.sh` 跑真机，拿到逐用例 PASS/FAIL。
4. 若该用例证明了一个默认公开 API，再同步更新 [Stable API 清单与门控状态](../reference/10-stable-api.md)。

示例级工作流：

1. QEMU smoke 先建立 baseline：示例应该打印什么、按什么节奏。
2. `hil-smoke` 在真板上逐例烧录 + 读 UART + grep 标记串。
3. QEMU 过而真机不过时，进入 triage：先查时钟、波特、引导、IRQ、外设接线。

## 分歧的几类

QEMU↔硅片的分歧高度集中在固定几类：

1. **UART 波特**：乱码 / 没 banner 通常指向 UART 时钟分频假设错。QEMU chardev 不限速，抓不到这类问题。
2. **定时器周期偏差**：`timer_irq` 来得太快/太慢，常指向 timer 的实际时钟基与软件常量不一致。
3. **引导挂死 / 全程静默**：先查供电、PWR_ON、flash 地址、boot header/hash、`probe-rs` flash 算法和 reset。
4. **semihosting 无响应**：embedded-test 路径先查 runner 是否执行了 `patch-hash`、`probe-rs run` 是否识别
   `.embedded_test`、测试是否用 `--test hil` 而不是裸 `cargo test --target riscv...`。
5. **IRQ 没投递**：查 LOCI 使能、触发沿、IRQ 号、`mie`/PLIC 类路径，以及测试是否只记录/唤醒而不是在 ISR 里做长事务。
6. **外设接线**：`spi_loopback`、UART loopback、I2C 外设扫描等需要真实跳线/上拉/外部设备；这类失败可能是台架问题。

诊断时对时序类症状要做算术：从 HAL 的时钟常量算出期望周期/波特，再从实测值反推真实时钟是多少，
用数字定位，而不是只凭现象猜。

## 当前 bring-up 状态

这里要分清两条 HIL 轨道，否则文档很容易互相打架：

- **HAL 驱动级 embedded-test：默认稳定面已有 WS63 真机证据。** 这是 `hisi-hal` 的稳定 API 证据线；
  当前用例数、覆盖面与 stable/unstable 边界以 [Stable API 清单与门控状态](../reference/10-stable-api.md) 为唯一事实源。
- **跨切面 `tests-hil`：** 用同一个 embedded-test runner 覆盖 CPU/PAC/critical-section 这类非单驱动事实；
  它是补充证据，不替代 HAL 驱动级 suite。
- **示例 smoke：仍按示例逐项推进。** 它证明完整示例镜像的 UART/semihosting/GPIO 标记在 QEMU 和真机上的一致性，
  不等同于 HAL 驱动级 HIL。当前示例状态见 [示例目录与验证标记串](../reference/02-examples.md)。
- **blinky：已在真硅片上确认。** 完整的 Rust -> flash -> 启动主流程于 2026-06-14 在真 WS63 硅片上跑通，
  blinky 上电启动并翻转 GPIO0。
- **连接性（ROADMAP C1-C5）：** WS63 Wi-Fi 的 porting + 链接 + netif->smoltcp 已在 QEMU 软件在环自测、符号闭合达成；
  真机 init/scan/connect/ping 仍待 HIL。BS2X 的 BLE/SLE 在 radio 层已论证不可行，走 HCI 边界。

不夸大、不假装：HAL 默认公开面的证据线已经比 2026-06 清楚很多，但示例级 smoke、连接性和需外部台架的
外设场景仍要继续补。HIL 文档的职责是解释为什么这样测；当前事实状态应收敛到 reference 页。
