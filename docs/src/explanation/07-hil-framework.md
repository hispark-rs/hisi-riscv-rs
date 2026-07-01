# HIL 测试框架

这一篇讲硬件在环（hardware-in-the-loop, HIL）测试的**哲学**——为什么我们用 UART 里的一行
字符串来判定一次真机测试通过、QEMU 已经验过的东西为什么还要上板再验一遍、以及一份**诚实的**
当前 bring-up 状态。操作步骤见 [运行 HIL 冒烟测试](../how-to/07-run-hil-tests.md)，
标记串与环境变量见 [HIL 标记串与环境变量](../reference/07-hil-markers.md)；本篇讲"为什么这样测"。

## HIL 存在的意义：验 QEMU 验不了的那部分

[QEMU](06-qemu-model.md) 已经把逻辑钉死了——内存布局、启动序列、外设行为、中断投递、DMA 搬运
都在软件在环里过了。那为什么还要 HIL？因为 QEMU **本质上**模拟不了真实的物理现实：
真实时钟频率、真实波特、真实外设时序、真实引导链、RF。

所以 HIL 的定位很明确：**它不是再验一遍逻辑，而是验 QEMU 验不了的物理现实。**
一个固件如果在 QEMU 里跑通、却在硅片上出问题，那个问题**几乎必然**落在"真实时钟/时序/外设/
接线"这几类——而不是逻辑 bug（逻辑 QEMU 已经替你筛过了）。这个前提决定了 HIL 故障诊断
（triage）的整个思路：**先怀疑物理现实，而不是先怀疑代码逻辑。**

## 为什么用 UART 标记串做验证通道

HIL 的验证通道是**串口里打印的标记字符串**——比如 `uart_hello` 该打印
`Hello from WS63 ...`、`timer_irq` 该周期打印 `timer irq #N`。为什么选这么"土"的通道，
而不是用调试器读寄存器、或者别的什么？三个理由：

1. **QEMU 可证 + 硅片可观测**。同一行标记串，QEMU 的 smoke-test 能看到、真硅片接串口也能
   看到。这就让 **parity 比对**成立：QEMU baseline 打印什么、硅片就该打印什么，
   两边逐字对照。换个调试器专属的通道，QEMU 那边就对不上了。
2. **它恰好能抓住 QEMU 抓不到的 bug**。一行 UART 输出要正确出现，背后牵连**真实波特
   （UART 时钟分频对不对）、真实定时（timer 时钟基对不对）、引导是否真的跑到了 main**——
   这些全是物理现实。QEMU 的 chardev 不限速，所以**波特算错它照样原样吐字节、根本不报错**；
   而真串口接收端波特不匹配就是一屏乱码或者干脆没有。也就是说，UART 这个通道**天生**能暴露
   时钟/波特/定时类 bug，正好补上 QEMU 的盲区。
3. **门槛低、确定**。一根串口线、一个标记串比对，不需要逻辑分析仪也能跑出第一轮结论
   （需要时再上逻辑分析仪做更细的诊断）。

## 三段式流程：qemu-smoke → hil-smoke → hil-triage

整套验证是三段接力，逐步逼近真机现实：

- **qemu-smoke**——在 QEMU 里逐例跑，建立**baseline**：每个固件该打印什么标记串、按什么
  顺序、什么节奏。这是"应该是什么样"的真值。
- **hil-smoke**——在真硅片上逐例烧录 + 读 UART + 比对标记串，**镜像** QEMU 的 smoke-test。
  通过则该例的物理现实也对了；不通过则进下一步。
- **hil-triage**——诊断**单个**失败步。它的工作假设很关键：**板子跑的是 QEMU 已验证的固件，
  所以失败通常意味着一个 QEMU 模拟不了的真实时钟/时序/外设/接线现实，而不是逻辑 bug。**
  triage 的任务是带证据点名最可能的那一类原因，而不是漫无目标地猜。

## 分歧的几类——先查这些

QEMU↔硅片的分歧高度集中在固定几类，triage 按这个清单逐项对照：

1. **UART 波特**——乱码 / 没 banner ⇒ UART 时钟分频假设错。WS63 UART 从 **160 MHz** 基
   分频；若按别的时钟算分频，波特就偏。（QEMU chardev 不限速，永远抓不到这类。）
2. **定时器周期偏约 10×**——`timer_irq` 来得太快/太慢 ⇒ 定时器还在按 **240 MHz PLL** 算、
   而真值是 **24 MHz TCXO**（或反过来）。这是**最典型的"QEMU 过了、硅片不行"**的 bug。
3. **引导挂死 / 全程静默**——一点输出都没有 ⇒ 电源/PWR_ON、错的 `LOADERBOOT`、错的 flash
   `ADDRESS`、或启动时读了一个真硅片上**永远锁不上**的 PLL。
4. **IRQ 没投递**——`gpio_irq` 静默 ⇒ LOCI 使能、触发沿、或引脚接线；核对 IRQ 号和
   LOCIEN/mie 路径是否和 SDK 一致。
5. **外设接线**——`spi_loopback` 需要 MOSI↔MISO 短接、`i2c_scan` 需要真实上拉。
   这一类"失败"可能是**测试台架（rig）的问题，不是固件的问题**——triage 必须把
   "固件要改"和"台架要修"分开。

诊断时对时序类症状要**做算术**：从 HAL 的时钟常量算出期望周期/波特，再从实测值反推真实
时钟是多少，用数字说话而不是猜。

## 当前 bring-up 状态（诚实版，2026-06-14）

要诚实——这是进行中的工作，不是已完成的胜利：

- **blinky：已在真硅片上确认。** 完整的 Rust → flash → 启动主流程
  （`cargo build` → `hisi-fwpkg patch-hash <elf>` → `probe-rs run <elf>`）
  于 2026-06-14 在真 WS63 硅片上跑通，blinky 上电启动并翻转 GPIO0。这是第一个、也是目前
  唯一一个**端到端真机确认**的例子。WS63 走 `boot-header` feature——0x300 头在链接期就烤进
  ELF，链接后只需 `hisi-fwpkg patch-hash` 补上真实 body SHA-256（secure-off 仍校验 hash，
  只跳过 ECC 签名），裸 ELF 即可直接 `probe-rs download` / `probe-rs run`，没有中间 `.img`、
  也没有 `hisi-fwpkg image` 步骤。（BS2X 暂无链接期 boot-header，仍走 route 1 的
  `hisi-fwpkg image -o app.img <elf>` → 烧到 app 分区。）
- **uart_hello：跑到了 main 并在运行，但 banner 还读不出来。** 固件确实启动、确实进了
  `main`、确实在跑——但它的 UART banner 目前在真硅片上**还读不到**。**怀疑是波特/时钟
  假设的问题**（对照上面的分歧类 1：UART 的 160 MHz 时钟基），**正在排查中**。
  这正是 QEMU 抓不到、必须上板才暴露的那类——也正是为什么我们要 HIL。
- **其余例子（timer_irq / gpio_irq / reset_demo / SPI / I2C / DMA 等）：QEMU 已验证，
  真机 bring-up 进行中。** QEMU 端这些的逻辑（中断投递、复位记录、DMA 握手、回环）都验过了；
  真机这一侧在等逐例把 `LOADERBOOT`/串口监控参数按板填实、逐步推进。
- **连接性（阶段 4/5）**：WS63 Wi-Fi 的 porting + 链接 + netif→smoltcp 已在 QEMU 软件在环
  自测、符号闭合达成；真机连通仍待 HIL。BS2X 的 BLE/SLE 在 radio 层已论证不可行，
  走 HCI 边界。

**首板的第一目标**就是跑通 uart_hello → timer_irq → reset_demo 这几步，确认"本轮的时钟修复
在真硅片上准确"（24 MHz 定时器、160 MHz UART 波特、SPI/I2C、GPIO/复位中断）——这正是
QEMU 数字验证不了、必须上板验的核心。uart_hello 的 banner 问题就是这条路上的当前关卡。

不夸大、不假装：**逻辑这一层 QEMU 已经替我们筛得很干净，物理现实这一层才刚踩上第一块硅。**
这恰恰是 HIL 这套框架存在的全部理由。
