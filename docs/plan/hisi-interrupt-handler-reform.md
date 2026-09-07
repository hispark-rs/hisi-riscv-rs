# 中断处理整改规划

**版本**: 2.0
**日期**: 2026-09-07
**范围**: `hisi-riscv-rt` + `hisi-hal` + `hisi-rtos` + consumer adapters

## 状态

**延期 / P2，条件触发。** 当前 connectivity 固件已有可工作的 WS63 RTOS typed
binding；没有证据表明必须立即建立全局动态 handler registry。执行前先完成 IRQ consumer
inventory，由真实缺口决定是扩展 compile-time binding，还是增加受限的动态注册能力。
跨计划优先级与触发条件以[工程计划注册表](README.md)为准。

## 当前事实

WS63 的中断责任已经分成三层：

- `hisi-riscv-rt` 拥有 trap entry、272-byte frame save/restore、四栈切换、弱 IRQ
  symbol、`__hisi_irq_epilogue` 和最终 `mret`；
- `hisi-hal::interrupt` 只拥有 mask、priority、threshold、pending 和 clear 等控制器机制，
  不拥有应用 handler 生命周期；
- `hisi-rtos::ws63` 已提供 typed `Binding<Handler>` 与 `bind_interrupts!`，为
  `TIMER_INT0`/`SOFT_INT0` 生成唯一 strong symbol，并把 callback 固定接到 RTOS
  enter/exit、timer/SWI scheduler path。

因此旧计划中“HAL 完全没有 handler API，所以先加入通用 `set_handler` 表”的前提已经
过期。现有 RTOS adapter 不能直接泛化成所有外设的全局事实源，但也不应被另一张动态表
重复覆盖。

## 决策边界

1. trap ABI 与 context restore 继续只归 `hisi-riscv-rt`；HAL 和 RTOS 不复制汇编入口。
2. HAL 保持 interrupt-controller owner；driver binding 只使用 typed IRQ token/trait，
   不让 HAL 承担应用 scheduler policy。
3. RTOS scheduler-critical IRQ 继续使用 compile-time typed binding，不经过运行时函数指针表。
4. 普通 peripheral IRQ 优先采用 compile-time binding；只有出现热替换、共享 IRQ 或
   runtime-selected driver 的真实消费者后，才评审动态 registry。
5. WS63 是 single hart + no A。不得直接假设 core `AtomicUsize` 是硬件单指令原子；若
   动态 registry 被触发，发布/撤销必须基于 `portable-atomic`/短
   `critical-section-single-hart`，并定义 ISR 并发、unregister 与 lifetime 语义。
6. handler/ISR 只 ack、记录、入有界队列和 wake；用户 callback 不在 IRQ、scheduler
   lock 或 critical section 中执行。

## 目标形态

```text
hisi-riscv-rt
  trap/vector + complete frame + weak dispatch ABI + epilogue/mret
        |
        +-- hisi-rtos::ws63 typed scheduler binding (TIMER_INT0/SOFT_INT0)
        |
        +-- hisi-hal typed peripheral binding contract
              |
              +-- driver-owned top half -> bounded state/waker -> task/future
```

公共 API 不承诺一个万能 closure registry。compile-time binding 应表达：

- IRQ identity 来自当前 chip PAC；
- 每个 exclusive IRQ 只有一个 owner；
- handler ABI 固定为无捕获 symbol 或实现受控 trait 的类型；
- 重复 binding 在编译或链接阶段 fail closed；
- driver 对 ack/clear 顺序和 deferred work 负责。

## 里程碑

### IR0 -- Consumer 与 ABI 清单

- 扫描 examples、HAL async driver、RTOS port、RF adapter 中所有 exported IRQ symbol、
  手写 `#[unsafe(no_mangle)]` handler、waker 和 callback path。
- 为每个 IRQ 记录 owner、ack/clear、是否共享、是否需要 runtime replacement、是否进入
  RTOS epilogue，以及现有 QEMU/HIL marker。
- 将 PAC IRQ enum、runtime weak symbol 和实际 consumer 做 machine-readable drift check。

**门槛：**不存在“计划声称未建模、代码其实已有 adapter”的双重事实；每个准备迁移的
IRQ 有唯一 owner 和现有行为基线。

### IR1 -- 编译期绑定契约

- 在不改变 272-byte frame/trap ABI 的前提下，抽取 scheduler binding 已验证的最小模式。
- 明确 `Binding<Handler>` safety contract、strong/weak symbol ownership、重复定义错误和
  top-half 限制。
- 对只需驱动唤醒的外设提供 typed handler trait；不接受可捕获闭包，也不把任意用户代码
  放进 ISR。

**门槛：**compile-fail 覆盖错误 IRQ、重复 owner、错误 handler 类型；ELF symbol audit
证明一个 IRQ 只有一个 strong implementation。

### IR2 -- Driver 纵向切片

- 选择一个 MIE IRQ 和一个 custom local IRQ 做纵向切片，优先复用现有 timer/GPIO/UART
  HIL，而不是一次迁移所有 IRQ。
- driver top half 完成 clear/ack、状态记录和 wake；业务逻辑在 task/future 中运行。
- 保留旧手写 symbol 一个迁移周期，并做 binary/symbol 与行为 parity。

**门槛：**host lost-wake/cancellation tests、QEMU marker、WS63 HIL 和 IRQ storm/queue-full
diagnostics 全部通过。

### IR3 -- 动态 registry（仅真实需求触发）

- 只有 IR0 证明 compile-time binding 无法覆盖真实 consumer 时才实施。
- API 必须使用 generation-bearing registration handle；drop/unregister 与正在执行 ISR 的
  竞态有明确线性化点，stale handle fail closed。
- shared IRQ 需要 bounded fan-out 与逐 owner pending predicate；不得遍历无界 callback list。
- no-A 实现使用项目统一的 portable-atomic/critical-section policy，不在临界区调用 handler。

**门槛：**Kani/TLA+ 或等价 deterministic model 覆盖 register/dispatch/unregister/stale
generation；没有 HIL 证据前保持 unstable。

### IR4 -- Embassy 与稳定性评审

- HAL 保留 peripheral async trait 与 waker mechanism；Embassy executor/time ownership 归
  `hisi-rtos`，避免第二个 TIMER owner。
- 只有真实 controller-only trait 语义匹配时才实现生态标准 trait；不为语法相似伪造
  compatibility。
- 稳定 API 只毕业有命名 HIL 的纵向切片。

## 验证矩阵

- 静态检查：PAC IRQ、runtime symbol、binding owner、feature combination drift。
- Host：重复 binding compile-fail、lost wake、cancel/drop、queue conservation、nested IRQ
  bookkeeping。
- QEMU 验证：MIE/local IRQ、clear/ack、handler return、RTOS epilogue。
- HIL：timer、GPIO/UART 纵向切片，长时间 IRQ storm，无 user callback in IRQ，无 task/waker
  丢失。
- ELF：strong/weak symbol 唯一性、trap/frame ABI 和 `mret` path 不变。

## 非目标

- 不把 `hisi-riscv-rt` 变成 driver registry；
- 不在 HAL 中复制 RTOS interrupt nesting/scheduling；
- 不以一个 `AtomicUsize` 函数指针数组替代所有 typed ownership；
- 不为尚无消费者的共享 IRQ、热插拔或 SMP 提前扩张稳定 API。
