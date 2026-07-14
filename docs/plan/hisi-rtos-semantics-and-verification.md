# `hisi-rtos` 调度语义与验证计划

## Status And Ownership

本文是 `hisi-rtos` 严格调度语义、证明义务和实现一致性验证的唯一计划
事实源。后续规范、TLA+/PlusCal 模型、Kani harness、deterministic host model
和 HIL evidence 都必须从本计划建立可追溯关系，不得在各实现或适配层里隐式
定义另一套调度语义。

本计划不要求用形式化模型替代 QEMU/WS63 HIL，也不把 LiteOS 兼容行为当作
`hisi-rtos` 通用语义。WS63 blob 的 ABI、tick/priority 转换、direct handoff profile
和 `LOS_*`/`osal_*` 行为仍属于
[WS63 RF runtime compatibility](ws63-rf-runtime-compatibility.md)。

当前 connectivity parity 和已启动的 RF 提交不因本计划中断；但新的
`Budgeted` 策略、SMP 语义、稳定 IPC/scheduler API 和 protection-domain 调度
在对应证明 gate 通过前不得宣称完成。

## Goals

1. 把 cooperative-first、budgeted fallback 和 preemptive worker 的混合调度写成可执行的
   规范状态机，而不只是描述性文字。
2. 在实现功能前搜索 lost wakeup、double enqueue、stale donation、锁内切换、
   budget starvation 和跨 hart 竞态反例。
3. 让抽象规范、Rust core、trace schema、QEMU 和 HIL 通过稳定 requirement ID
   对齐，可回答“某个性质由哪一层证明”。
4. 保持一个统一 scheduler core；`Cooperative`、`Budgeted`和 `Preemptive`
   是每 thread 策略，不是三套内核 backend。
5. 先验证 single-hart，但不在 core 不变量中假设“关本 hart 中断等于
   全局互斥”；SMP 扩展使用同一语义模型加 hart/affinity/IPI 维度。

## Normative Model To Freeze

第一个产物应是后续新增的 `docs/spec/hisi-rtos-scheduling.md`。它是 normative
contract；本文只管理它如何产生、证明和演进。规范中每条要求使用稳定 ID，
例如 `RTOS-SCHED-001`、`RTOS-WAIT-004` 和 `RTOS-BUDGET-003`。

### Identity And State

规范必须定义：

- `ThreadId = { slot, generation }`，slot 复用后旧句柄必须失效；
- `ThreadState = Free | Ready | Running | Blocked | Sleeping | Throttled | Exited`；
- ready/wait/sleep/throttle queue 的 ownership；
- `WakeReason = Signal | Timeout | Interrupt | Cancel | BudgetReplenished`；
- single-hart 每时刻最多一个 `Running` thread；SMP 中每 hart 最多一个，
  且同一 thread 不能同时在两个 hart 运行。

### Priority And Run Policy

priority 决定从 eligible ready set 中选谁；`RunPolicy` 决定当前 thread 在什么
条件下可被强制切换。规范必须固定：

- `Cooperative`：显式 yield/block/exit 调度；更高优先级 ready 可抢占；不因
  同优先级普通 time slice 切换；
- `Budgeted`：保留 cooperative 路径，但执行资格受预算限制；
- `Preemptive`：更高优先级 ready 立即请求切换，同优先级可按时间片
  round-robin；
- lower-priority readiness、equal-priority FIFO、yield-to-tail 和 time-slice expiry
  的精确顺序。

### Budget Contract

`Budgeted` 实现前必须在两种模型中显式选择，不得使用模糊的“超时后
调度一次”：

1. 最大连续执行 burst；或
2. `BudgetSpec { capacity, replenishment_period }` 的周期性 CPU reservation。

当高优先级 vendor thread 在预算耗尽后仍 ready 时，仅把它放回 ready queue 可能
立即再次选中，无法保护低优先级 Embassy executor。因此初始 normative candidate
采用周期性 reservation：耗尽后 thread 进入 `Throttled`，在 replenishment
deadline 前不进入 eligible ready set。最终决策必须经 TLA+ 反例搜索和 RF
workload 数据评审后冻结。

还必须固定：

- 是否只计 thread-mode CPU time，IRQ 时间如何归属；
- 被更高优先级抢占后剩余预算是否保留；
- yield/block 是否补充预算；
- scheduler lock 内耗尽时只记录 pending，还是触发其他约束；
- 无其他 eligible thread 时是否允许借用空闲 CPU；
- deadline 滚动、时钟 wrap、多次过期和迁移 hart 后的计费。

### Scheduler Lock And Interrupts

规范必须区分 interrupt masking、per-hart preemption guard 和 global kernel lock。
当前 `lock_scheduler` 的 single-hart contract 应固定为：

- 按当前 thread 记录嵌套深度，不关闭硬件中断；
- IRQ 可 ack/record/wake，但 thread switch 延迟到 outermost unlock；
- reschedule pending 不得丢失；
- 持有 scheduler lock 时禁止 block/sleep/exit 等无法继续运行的操作；
- budget/time 如何计费必须与 Budget Contract 一致。

IRQ contract 必须定义 depth、ack/wake/reschedule 顺序、SOFT_INT coalescing 和
outermost epilogue 切换。物理 nested IRQ 只能在每层 trap stack/context ownership
被证明后启用；当前 WS63 只承诺嵌套 runtime IRQ bracket，不承诺 MIE
重入。

### Waits, Timeouts And Priority Inheritance

规范必须为 signal/timeout/cancel/IRQ wake 定义唯一 linearization point，保证一次
wait 只有一个 `WakeReason` 获胜。deadline 相等时的优先顺序、wrap-safe
comparison、长 deadline 分段 arm 和 stale timer event 都必须明确。

recursive mutex 必须规定 owner-only unlock、direct handoff、multiple donors、
transitive donation、timeout/cancel donation removal、base-priority change 和最后一层
recursive unlock 后的重算。semaphore 没有唯一 owner，不伪装 priority inheritance。

### SMP Extension

single-hart 规范冻结后再扩展：`HartId`、affinity、per-hart current/run queue、
reschedule IPI、memory ordering、跨 hart donation 和 budget accounting。平台只有在
`SmpPort` capability 明确存在时才启用；不得将 single-hart interrupt disable
当作跨 hart 互斥。

## Required Invariants

1. 一个 thread 任何时刻最多属于一个 scheduler state 和一条 ownership queue。
2. 每 hart 最多一个 running thread；同一 thread 不在多 hart 同时运行。
3. 每次 wait 只有一个 wake reason 获胜，不 double enqueue、不丢 permit。
4. IRQ depth 或 scheduler-lock depth 非零时不运行被唤醒的 thread，pending
   reschedule 在条件允许后最终处理。
5. mutex effective priority 始终等于 base priority 和所有有效 donation 的最高者，
   timeout/handoff/unlock 后不留 stale donation。
6. `Budgeted` thread 不超过冻结规范允许的 capacity，除了规范明确允许的
   interrupt/scheduler-lock latency bound。
7. generation 不匹配的 task/mutex/semaphore handle 不影响新对象。
8. 不在 ISR、critical section 或 scheduler lock 中运行用户 callback。

活性性质至少包括：获得 grant 的 waiter 最终可运行，outermost unlock/IRQ
exit 后 pending reschedule 最终处理，预算补充后 throttled thread 最终恢复
eligible，以及在公平性假设下更高优先级 ready thread 不被无限延迟。

## Verification Layers

### Abstract Protocol: TLA+ / PlusCal

对 2-4 个 thread、2-3 个 priority、0-2 个 hart 的有界系统搜索事件交错。TLC
或 Apalache 验证 safety invariant 和带公平性假设的 liveness，并把最小反例
保存为 regression fixture。抽象模型证明协议，不证明 Rust ownership、汇编
context frame 或真实中断控制器。

### Rust State Machine: Kani

在 `hisi-rtos-core` 抽出无 PAC/CSR/MMIO 的纯状态转换后，用 Kani 对实际 Rust
代码做 bounded proof。harness 覆盖 queue ownership、wake winner、direct handoff、
donation propagation/removal、lock deferral、budget arithmetic 和 generation handle。
每个 harness 引用至少一个 requirement ID，不允许仅证明与生产代码分离的简化
Rust 重写。

### Long Traces: Property And Differential Tests

host-deterministic backend 使用固定 seed 和虚拟时间生成长事件序列，每步检查
invariant，并把生产 Rust core 与可读 reference model 做 differential comparison。
property failure 必须 shrink 为可提交的最小序列。

Loom 不用来模拟当前 single-hart IRQ masking；待 SMP 引入真正 atomic/lock-free
共享状态后，再用 Loom 搜索 Rust memory-ordering 交错。Miri 用于 UB 检查，
不代替调度语义证明。

### Mechanism Evidence: QEMU And HIL

context switch/trap 汇编使用 `size_of`/`offset_of` 断言、disassembly 检查、
GPR/FPR/FCSR 哨兵值、QEMU 和真机 timer/yield/IRQ stress。真实 PAC、timer、
software IRQ、cache、blob ABI 和 RF latency 只能由 QEMU/HIL/oracle evidence 闭环，
不能由 TLA+ 或 Kani 代替。

## Traceability And CI

建立 machine-readable requirement manifest，至少记录 requirement ID、normative text、
TLA+ property/action、Kani harness、host/property test、QEMU/HIL test 或
`NotApplicable` 理由，以及 implementation symbols。

CI 最终分为：

- `rtos-spec`：规范 ID、链接、manifest 和 trace schema drift；
- `rtos-model`：TLC/Apalache invariant 与 liveness check；
- `rtos-kani`：有界 Rust proof，慢 proof 可 nightly，核心 proof 必须 PR gate；
- `rtos-host`：unit/property/differential/replay fixtures；
- `rtos-qemu`：context/trap/timer 机制；
- `rtos-hil`：WS63 真机和 RF parity。

任何规范改动必须同时更新受影响的 model、proof/test 和 traceability manifest；
只修正文字但不处理反例的 PR 不得合并。

## Milestones

1. **V0 Semantic inventory**：从当前 code/README/evidence 收集已承诺行为，分离
   generic runtime 与 WS63 compatibility profile，建立 requirement ID。
2. **V1 Single-hart base spec**：冻结 identity/state/queue/priority、Cooperative、
   Preemptive、scheduler lock 和 IRQ epilogue。
3. **V2 Wait and mutex spec**：冻结 signal-timeout race、semaphore grant、recursive
   mutex 和 transitive priority inheritance；补齐 TLA+ 与 Kani。
4. **V3 Budget design freeze**：用反例和 RF trace 选择 budget/replenishment 模型，
   冻结 `BudgetSpec` 后才实现 `RunPolicy::Budgeted`。
5. **V4 Implementation conformance**：抽出 pure core，建立 differential model、Kani harness
   和 requirement manifest PR gate。
6. **V5 Mechanism closure**：QEMU/HIL 闭环 GPR/FPR/FCSR、timer、SOFT_INT、IRQ
   epilogue、budget latency 和 mixed vendor/Embassy stress。
7. **V6 SMP extension**：在 single-hart properties 上增加 hart/affinity/IPI/memory ordering、
   跨 hart inheritance/budget，使用 TLA+ + Loom/Kani + host multi-hart model。
8. **V7 Stable gate**：只有在 requirement/model/proof/host/QEMU/HIL 追溯矩阵完整后，
   对应 scheduler/IPC/profile API 才可进入 stable candidate。

## Current Evidence Mapping

已完成的 unified context、IRQ epilogue、software interrupt、priority inheritance 和
scheduler stress HIL 是 V0/V1/V2/V5 的输入，不需要推倒重做。它们目前证明
实例场景，还没有证明所有可能事件排列。`Budgeted` 尚无实现和证据，
必须从 V3 语义冻结开始，不得从当前 time-slice 逻辑直接演化出隐式契约。

## Non-Goals

- 不形式化证明整个 HAL、RF 协议栈或闭源 blob。
- 不用 LiteOS 行为限制 Rust-native `hisi-rtos` 公共设计。
- 不因建模暂停已有 connectivity parity，但不允许未定义的新调度策略进入
  默认路径。
- 不宣称抽象模型已证明汇编、中断控制器、cache 或真实 RF 时序。
