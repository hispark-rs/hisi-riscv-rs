# `hisi-rtos` 调度语义与验证计划

## 状态与归属

**配套工作 / P1。** A5R-F2 已闭合；当前下一批执行项是 A5R-F3 至 F5，并受单一 WIP 规则约束：A5U
闭合前不抢占 active slot。跨计划优先级与依赖以
[工程计划注册表](README.md)为准。

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

## 目标

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

## 待冻结的规范模型

第一个产物应是后续新增的 `docs/spec/hisi-rtos-scheduling.md`。它是 normative
contract；本文只管理它如何产生、证明和演进。规范中每条要求使用稳定 ID，
例如 `RTOS-SCHED-001`、`RTOS-WAIT-004` 和 `RTOS-BUDGET-003`。

### 身份与状态

规范必须定义：

- `ThreadId = { slot, generation }`，slot 复用后旧句柄必须失效；
- `ThreadState = Free | Ready | Running | Blocked | Sleeping | Throttled | Exited`；
- ready/wait/sleep/throttle queue 的 ownership；
- `WakeReason = Signal | Timeout | Interrupt | Cancel | BudgetReplenished`；
- single-hart 每时刻最多一个 `Running` thread；SMP 中每 hart 最多一个，
  且同一 thread 不能同时在两个 hart 运行。

### 优先级与运行策略

priority 决定从 eligible ready set 中选谁；`RunPolicy` 决定当前 thread 在什么
条件下可被强制切换。规范必须固定：

- `Cooperative`：只在显式 yield/block/exit 或明确 handoff 时调度；更高优先级 ready
  只记录 pending，不立即抢占；不因同优先级普通 time slice 切换；
- `Budgeted`：保留 cooperative 路径，但执行资格受预算限制；
- `Preemptive`：更高优先级 ready 立即请求切换，同优先级可按时间片
  round-robin；
- lower-priority readiness、equal-priority FIFO、yield-to-tail 和 time-slice expiry
  的精确顺序。

### Budget 契约

`Budgeted` 实现前必须在两种模型中显式选择，不得使用模糊的“超时后
调度一次”：

1. 最大连续执行 burst；或
2. `BudgetSpec { capacity, replenishment_period }` 的周期性 CPU quota 上限；
   它不承诺每周期获得最低 CPU 服务量。

当高优先级 vendor thread 在预算耗尽后仍 ready 时，仅把它放回 ready queue 可能
立即再次选中，无法保护低优先级 Embassy executor。因此初始 normative candidate
采用周期性 quota：耗尽后 thread 进入 `Throttled`，在 replenishment
deadline 前不进入 eligible ready set。最终决策必须经 TLA+ 反例搜索和 RF
workload 数据评审后冻结。

还必须固定：

- 是否只计 thread-mode CPU time，IRQ 时间如何归属；
- 被更高优先级抢占后剩余预算是否保留；
- yield/block 是否补充预算；
- scheduler lock 内耗尽先记录 pending；若连续持锁超过 ported config 的非零
  上界，则通过 port 的 non-returning contract-violation handler fail-stop；
- 无其他 eligible thread 时是否允许借用空闲 CPU；
- deadline 滚动、时钟 wrap、多次过期和迁移 hart 后的计费。

<a id="quota-closure-and-guaranteed-service-evolution"></a>

### Quota 收口与保证服务演进

`Budgeted` 的名称和语义在当前 alpha API 中保持稳定：它是周期 CPU quota 上限，
用于限制不合作 thread 的最大 CPU 消耗，不保证 runnable thread 每周期获得最低服务量。
后续若 RF 或其他系统服务需要可证明的最低 CPU 时间，必须新增独立
`ReservationSpec`/reservation contract；不得通过修改 `Budgeted` 的既有语义隐式引入。

实施分为两个连续但相互独立的阶段。Q0-Q4 已为当前 A3 payload 闭合；payload、archive
hash 或 task set 变化时必须重新执行 Q3/Q4。G0-G5 是按测量证据触发的可选扩展，不因
A3 已完成而自动启动。

#### Q0-Q4 -- Quota 与可观测性收口

1. **Q0 语义冻结**：保持一个 scheduler backend；所有 ready thread 始终按
   effective priority 排队，`RunPolicy` 只控制当前 thread 何时允许被强制切换。
   `Cooperative`、`Budgeted` 和 `Preemptive` 不形成三套 ready-queue 语义。
2. **Q1 Ported 能力**：`start_cooperative` 只产生 `CooperativeOnly` handle；带
   timer/SWI 的启动产生 `Ported` handle，只有后者可配置 `Budgeted`/`Preemptive`。
   Ported config 必须提供非零 scheduler-lock 上限和 non-returning
   `contract_violation` handler；timer re-arm、MIE/SWI handoff、stale event 和时间回绕
   必须有独立回归。
3. **Q2 按线程记录证据**：为 CPU time、dispatch、budget exhaustion、最长连续执行、
   ready latency、scheduler-lock latency 和 IRQ interference 建立低扰动 trace。只有这些
   数据能决定哪个 thread 真正需要抢占或最低服务保证。
4. **Q3 调度 Profile**：芯片/blob adapter 按 archive hash 和任务角色维护兼容
   profile；time-critical、worker、background 和 unknown thread 不得长期共用一个
   无差别的默认策略。通用 kernel 不编码 WS63 task 名称或 vendor priority policy。
5. **Q4 Group quota 门槛**：先统计 subsystem aggregate CPU，再决定是否为 worker/
   background group 执行总 quota。不得在尚未区分 critical thread 时对全部 radio task
   粗暴 throttle；per-thread quota 与 group quota 分别解决单任务失控和子系统总占用。

当前状态：Q0-Q4 已为冻结的 A3 archive/payload 闭合。Q2 的低扰动 task metrics 在真实
RF workload 中捕获了 ported switch handoff 竞态，并通过 20 次 unchanged-image nRST
验证修复路径；Q3 已绑定 archive hash 和真实任务分类，Q4 已据此决定当前 payload 不启用
group quota 或 Reservation。证据见
[A3 switch-race and observability](evidence/ws63-rf-a3-switch-race-observability-2026-07-14.md)。
新的 archive/payload 在重新闭合 Q3 前不得凭单次样本固化 task policy，在重新闭合 Q4
前不得实现 group quota。

#### G0-G5 -- 可选的保证服务扩展

G0 只在 Q0-Q4 的压力 HIL 仍显示 critical ready latency 接近协议 timeout、并发
Wi-Fi/BLE/SLE/Embassy 出现可归因 deadline miss，或产品明确需要响应时间承诺时启动。

1. **G0 内部归一化**：内部将策略归一化为 `DispatchMode` 与 `CpuControl`；
   旧 API 映射为 `Cooperative + Unlimited`、`Cooperative + Quota` 和
   `Preemptive + Unlimited`，不立即破坏 `RunPolicy` 公共入口。
2. **G1 Reservation 契约**：新增 validated
   `ReservationSpec { capacity, period, deadline }`，固定
   `capacity <= deadline <= period`。其唯一承诺是 thread 持续 runnable 时，从 release
   到 deadline 至少获得 capacity CPU 时间；blocked 时间、额度结转、miss handling 和
   IRQ accounting 必须规范化。
3. **G2 准入控制**：静态 RTOS manifest 汇总 reservation、优先级、IRQ、最大
   scheduler-lock/mutex blocking 和 context-switch 开销。只有分析通过后生成的
   `ReservationToken` 才能创建 guaranteed thread；动态创建不得绕过 admission。
4. **G3 固定优先级 Server**：第一版 reservation 复用现有 fixed-priority scheduler、
   priority inheritance 和 ported preemption，不同时引入 EDF。RF 只给经证据识别的
   authentication/RX-management/protocol-timer 等 critical thread 或 reservation group
   提供保证，worker/background 继续使用 quota。
5. **G4 证明与 HIL**：TLA+/Kani/host model 证明 admitted reservation 的服务下界、
   quota 上界、lock/IRQ blocking 和 replenishment 状态；真机覆盖 CPU hog、IRQ storm、
   mutex inversion、Embassy 共存和多轮 connectivity reset matrix。
6. **G5 Cooperative reservation 决策**：初始不公开硬保证的
   `Cooperative + Reservation`。只有所有干扰 thread 的最大 non-yield、Future 单次 poll、
   scheduler lock 和 IRQ mask 时间均有可信上界，且 response-time analysis 通过后，才可
   作为独立 experimental contract 评估。

这一演进保持原始的 cooperative-first 目标：普通 Rust/Embassy 路径不因未来实时能力
自动变成全抢占系统；最低服务保证只属于显式 admitted、ported、preemptive 的少数
critical thread。

<a id="ported-switch-intentticket-protocol-deferred"></a>

### Ported Switch Intent/Ticket 协议（延期）

当前 `recover_completed_switch_request` 已修复并通过真机证明“thread mode 选出 target
后、提交 SWI 前，IRQ 已完成切换并又恢复 previous”的高频竞态。它是近期正确性修复，
在下述 ticket protocol 完成全部 parity/HIL 前不得删除。长期强化使用显式 intent、
identity generation 和单次消费协议替代裸 `(previous, target)` 与 state inference；该工作
不改变公开 API、`RunPolicy`、`Budgeted` quota、RF archive profile 或 Reservation。

此项排在已冻结的 A3/ping 行为基线之后；若 archive/payload 变化，可与重新执行的 Q3
profile 并行，但不阻塞当前 A5U；也不得与 Q4 group quota 或 G0-G5
guaranteed-service 扩展合并实施。

1. **T0 规范冻结**：定义 `SwitchIntent`/`PendingSwitch` 状态机。冻结唯一 `Running`、
   `current` 一致、ready queue/live intent 唯一归属、intent 单次 Commit/Cancel、pending
   单次消费、`TaskRef` generation、idle 不入普通队列、cancel 恢复 detached target。
2. **T1 内部 Ticket**：引入 `#[must_use] SwitchIntent { sequence, previous:
   TaskRef, target: SwitchTarget, previous_resume_generation }`；`prepare_yield/block/exit`
   在同一 critical section 返回 intent，`execute_switch` 只接受 intent。
3. **T2 Generation 验证**：intent 创建时记录 slot identity generation 和 resume
   generation；提交时变化必须显式分类为 stale/identity mismatch，cancel 并恢复 target。
   FFI/unsafe 边界继续保留运行时防御。
4. **T3 结构化 Pending 状态**：用 `PendingSwitch { sequence, previous, target }` 替换
   `forced_next: usize`，trap 只能按 sequence 消费一次。
5. **T4 离队 Ownership**：分阶段引入 `ReadyQueued` / `ReadyDetached {
   intent_sequence }`；先保留当前 `ready_contains` 防御，只有 membership 已显式建模且
   parity 通过后才删除 bounded scan。
6. **T5 验证**：覆盖 host interleaving、TLA+ A/B/C 状态机与 Kani；诊断记录
   created/committed/cancelled-stale/cancelled-identity/completed/max-age。旧
   `switch_race_recoveries` 至少保留一个迁移周期。
7. **T6 HIL**：Cooperative/Budgeted/Preemptive/Embassy 加 RF 压力，至少 100 次 reset；
   不得出现多 `Running`、task 遗失、永久 pending 或 `0xdeadbeef`。静止点必须满足
   `created = committed + cancelled` 与 `committed = completed + pending`。
8. **T7 清理**：只有 ticket parity、形式化检查和 T6 真机门槛全部通过后，才能移除
   当前 state-inference recovery；删除必须是独立提交并保留迁移证据。

### Scheduler Lock 与中断

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

### 等待、超时与优先级继承

规范必须为 signal/timeout/cancel/IRQ wake 定义唯一 linearization point，保证一次
wait 只有一个 `WakeReason` 获胜。deadline 相等时的优先顺序、wrap-safe
comparison、长 deadline 分段 arm 和 stale timer event 都必须明确。

recursive mutex 必须规定 owner-only unlock、direct handoff、multiple donors、
transitive donation、timeout/cancel donation removal、base-priority change 和最后一层
recursive unlock 后的重算。semaphore 没有唯一 owner，不伪装 priority inheritance。

### SMP 扩展

single-hart 规范冻结后再扩展：`HartId`、affinity、per-hart current/run queue、
reschedule IPI、memory ordering、跨 hart donation 和 budget accounting。平台只有在
`SmpPort` capability 明确存在时才启用；不得将 single-hart interrupt disable
当作跨 hart 互斥。

## 必须满足的不变量

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
eligible，以及在 Preemptive policy 或 Cooperative 显式 handoff 的公平性假设下，
更高优先级 ready thread 不被无限延迟。

## 验证分层

### 抽象协议：TLA+ / PlusCal

对 2-4 个 thread、2-3 个 priority、0-2 个 hart 的有界系统搜索事件交错。TLC
或 Apalache 验证 safety invariant 和带公平性假设的 liveness，并把最小反例
保存为 regression fixture。抽象模型证明协议，不证明 Rust ownership、汇编
context frame 或真实中断控制器。

### Rust 状态机：Kani

在 `hisi-rtos-core` 抽出无 PAC/CSR/MMIO 的纯状态转换后，用 Kani 对实际 Rust
代码做 bounded proof。harness 覆盖 queue ownership、wake winner、direct handoff、
donation propagation/removal、lock deferral、budget arithmetic 和 generation handle。
每个 harness 引用至少一个 requirement ID，不允许仅证明与生产代码分离的简化
Rust 重写。

### 长 Trace：性质测试与差分测试

host-deterministic backend 使用固定 seed 和虚拟时间生成长事件序列，每步检查
invariant，并把生产 Rust core 与可读 reference model 做 differential comparison。
property failure 必须 shrink 为可提交的最小序列。

Loom 不用来模拟当前 single-hart IRQ masking；待 SMP 引入真正 atomic/lock-free
共享状态后，再用 Loom 搜索 Rust memory-ordering 交错。Miri 用于 UB 检查，
不代替调度语义证明。

### 机制证据：QEMU 与 HIL

context switch/trap 汇编使用 `size_of`/`offset_of` 断言、disassembly 检查、
GPR/FPR/FCSR 哨兵值、QEMU 和真机 timer/yield/IRQ stress。真实 PAC、timer、
software IRQ、cache、blob ABI 和 RF latency 只能由 QEMU/HIL/oracle evidence 闭环，
不能由 TLA+ 或 Kani 代替。

## 可追溯性与 CI

建立 machine-readable requirement manifest，至少记录 requirement ID、normative text、
TLA+ property/action、Kani harness、host/property test、QEMU/HIL test 或
`NotApplicable` 理由，以及 implementation symbols。

CI 最终分为：

- `rtos-spec`：规范 ID、链接、manifest 和 trace schema drift；
- `rtos-model`：TLC/Apalache invariant 与 liveness check；
- `rtos-kani`：有界 Rust proof，慢 proof 可 nightly，核心 proof 必须 PR gate；
- `rtos-host`：unit/property/differential/replay 测试样例；
- `rtos-qemu`：context/trap/timer 机制；
- `rtos-hil`：WS63 真机和 RF parity。

任何规范改动必须同时更新受影响的 model、proof/test 和 traceability manifest；
只修正文字但不处理反例的 PR 不得合并。

## 里程碑

1. **V0 语义清单**：从当前 code/README/evidence 收集已承诺行为，分离
   generic runtime 与 WS63 compatibility profile，建立 requirement ID。
2. **V1 单 Hart 基础规范**：冻结 identity/state/queue/priority、Cooperative、
   Preemptive、scheduler lock 和 IRQ epilogue。
3. **V2 Wait 与 Mutex 规范**：冻结 signal-timeout race、semaphore grant、recursive
   mutex 和 transitive priority inheritance；补齐 TLA+ 与 Kani。
4. **V3 Budget 设计冻结**：已冻结 `Budgeted` 为周期 CPU quota 上限，并实现
   throttle/replenishment、scheduler-lock fail-stop 上界与可执行模型；后续不改变其为
   最低服务保证。
5. **V4 实现一致性**：抽出 pure core，建立 differential model、Kani harness
   和 requirement manifest PR gate。
6. **V5 机制收口**：QEMU/HIL 闭环 GPR/FPR/FCSR、timer、SOFT_INT、IRQ
   epilogue、budget latency 和 mixed vendor/Embassy stress。
7. **V6 SMP 扩展**：在 single-hart properties 上增加 hart/affinity/IPI/memory ordering、
   跨 hart inheritance/budget，使用 TLA+ + Loom/Kani + host multi-hart model。
8. **V7 稳定门槛**：只有在 requirement/model/proof/host/QEMU/HIL 追溯矩阵完整后，
   对应 scheduler/IPC/profile API 才可进入 stable candidate。

## 当前证据映射

已完成的 unified context、IRQ epilogue、software interrupt、priority inheritance、
scheduler stress 和 Budgeted HIL 是 V0/V1/V2/V3/V5 的输入，不需要推倒重做。
`Budgeted` 已有 host tests、Kani harness、TLA+ model 和 WS63 quota marker，但这些证据
仍只覆盖已列出的性质与场景，不证明所有事件排列。wait 线性化已由
`WaitLinearization.tla` 对 post/timeout/cancel/grant/consume 交错完成 47/27 状态搜索，
并由两条 production-path Kani harness 显式穷举 signal/cancel 与 signal/timeout 顺序；
它证明 wait/ready/grant ownership 和 permit conservation，不替代 mutex PI、全局
scheduler state 或真机机制证据。Q2-Q4 的观测、archive profile 和
group-quota gate 仍未完成；不得从现有 quota 逻辑隐式演化出 Reservation 契约。

## 非目标

- 不形式化证明整个 HAL、RF 协议栈或闭源 blob。
- 不用 LiteOS 行为限制 Rust-native `hisi-rtos` 公共设计。
- 不因建模暂停已有 connectivity parity，但不允许未定义的新调度策略进入
  默认路径。
- 不宣称抽象模型已证明汇编、中断控制器、cache 或真实 RF 时序。
