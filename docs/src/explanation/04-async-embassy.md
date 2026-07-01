# async 与 embassy

这一篇是异步故事的**概念总览**——它讲三种用 HAL 的方式（纯阻塞、`async` feature、
`embassy` feature）各自是什么、为什么并存、以及一个反直觉的事实：**这一切跑在一颗连原子
扩展都没有的核上。** 想看代码在哪个文件、每个 trait 怎么实现、怎么上游化，去
[async-embassy 深入文档](components/06-async-embassy.md)；这里建立的是全局直觉。

## 同一套 HAL，三种用法

hisi-riscv-hal 默认是一套**阻塞**驱动（符合 `embedded-hal 1.0`）。在它之上，
两个 feature 叠出了异步能力，于是同一套驱动有三档用法：

1. **不开任何 feature——纯阻塞**。`uart.write()` 就在那儿自旋等 FIFO 有位。
   简单、确定、没有执行器、没有 waker。绝大多数简单固件用这档就够。
2. **开 `async` feature——中断 + waker 驱动的 `.await`**。多了
   `embedded-hal-async` / `embedded-io-async` 的实现（`DelayNs`、`digital::Wait`、
   `SpiBus`、`I2c`、UART 的 `Read`/`Write`），外加一个极小的 `block_on` 执行器和一个
   `IrqSignal` 桥。让你"不上 embassy 也能 `.await`"。
3. **再开 `embassy` feature——完整的 embassy 时间生态**。多了一个 embassy-time 的
   `Driver`，于是 `embassy-executor`（platform-riscv32）能跑，`Timer::after` /
   `Instant` / `Ticker` 都可用。

这三档不是三套代码，而是**同一套阻塞驱动上逐层叠加**。这个分层本身是个设计取舍：
不想要异步复杂度的人完全感知不到它的存在，想要的人按需开 feature。

## `async` feature：两块地基

### block_on + IrqSignal

`async` 这一档的核心是两个极小的零件：

- **`block_on(fut)`**——一个最朴素的 future 执行器：poll，遇到 `Pending` 就 `wfi`
  休眠，硬件中断把核唤醒后再 poll。没有堆、没有全局执行器、没有任务队列。它存在的意义
  正是"轻"——给只想偶尔 `.await` 一下、不愿背上 embassy 全套的场景。
- **`IrqSignal`**——一座"ISR → future"的桥：一个 `AtomicBool`（fired 标志）加一个停在
  `critical_section::Mutex` 里的 `Waker`。中断里调 `signal()`，future poll 时检查
  fired、登记 waker。这是把"硬件中断这件异步的事"接到 Rust async 模型里的接缝。

### 一个关键克制：不抢中断向量

这套异步驱动有一条很重要的设计纪律——**它不自动安装 ISR、不抢占中断向量**。每个驱动只
**导出一个 `on_interrupt` 钩子**（`timer::on_interrupt`、`gpio::on_interrupt`、
`uart::on_interrupt`……），由**应用自己的 trap 处理函数**按 `mcause` 把中断路由过去。

为什么这么设计？因为 Rust 的 cargo 工作区会把 feature **并集**——只要工作区里有一个 crate
开了 `async`，整个工作区都可能被打开。如果异步层一旦被开启就**默认安装 ISR**，那它会**悄悄
改变**那些根本没打算用异步的固件的中断行为。"只导出钩子、由应用显式路由"保证了：
**开不开 async feature，对非异步固件的行为零影响**。这是一条"不给用户埋雷"的边界。

## 为什么能跑在没有原子的核上

这是最反直觉的一点。WS63 是 `riscv32imfc`——**没有 A 扩展**，`lr.w/sc.w` 会陷入
（详见 [硬浮点工具链](03-hardfloat-toolchain.md)）。而异步执行器、waker 这些东西通常被认为
"当然要原子操作"。它怎么还能跑？

三件事让它成立：

1. **HAL 一直走 `portable-atomic` + `critical-section`**。需要 CAS 的地方由
   `portable-atomic` 用临界区 polyfill 实现，`hisi-riscv-rt` 提供单核的
   `critical-section-single-hart`。
2. **embassy-executor 本身就支持无 CAS 目标**。它内部按编译期 `cfg` 在
   `core::sync::atomic` 和 `portable_atomic` 之间切换——这是它早就为 `thumbv6m`
   （Cortex-M0，同样无 CAS）准备好的能力。riscv32 平台模块里的 `SIGNAL_WORK` 只用
   load/store（这颗核支持），不需要 CAS。所以**无需改 embassy 一行**。
3. 一个真实踩过的坑值得记一笔：`target/` 里**陈旧的 host proc-macro 工件**会让 embassy
   宏构建莫名失败——`cargo clean` 后全量通过。这不是逻辑问题，是构建缓存问题。

也就是说，"无原子"在这里**没有**变成异步的拦路虎——它早被 `portable-atomic` +
`critical-section` 这层垫片吸收掉了，而 embassy 恰好已经为这种核留好了路。

## embassy feature：让 WS63 成为时间提供者

`embassy` feature 做的事可以一句话概括：**让 WS63 成为 embassy-time 的时间源**。
具体是实现一个 embassy-time `Driver`：

- **`now()`** 读 **TCXO 的 64 位自由计数器**（24 MHz），缩放到 embassy-time 的 1 MHz
  tick。单调、跟随真实（QEMU 上是虚拟）时间流逝。
- **`schedule_wake(at, waker)`** 把 waker 入队，并用一个 **TIMER 通道**编程一次性闹钟。
- 闹钟 IRQ 触发时排空到期 waker、重新武装下一个截止时间。

这里有个和 [HIL 框架](07-hil-framework.md) 直接相关的细节：**时间的真值来自 TCXO（24 MHz），
不是 PLL（240 MHz）**。如果时间源算错了时钟基，所有 `Timer::after` 都会偏 10×——
这正是 QEMU 验证不了、必须上板验的那类时钟假设。

## 这一档该用哪个

把三档放在一起，选择其实很自然：

- 简单顺序逻辑、不在乎并发 → **纯阻塞**。
- 想 `.await` 个别 IO、不想背 embassy → **`async` + `block_on`**。
- 要多任务、要 `Timer::after`、要 embassy 生态 → **`embassy`**。

覆盖范围、每个 trait 落在哪个文件、以及"为什么走 esp-hal 那种 out-of-tree 上游模型而不是
塞进 embassy monorepo"这些更深的讨论，都在
[async-embassy 深入文档](components/06-async-embassy.md) 里。那篇是权威；本篇负责让你先有
全局图景。
