# WS63 RF Blob Runtime 兼容计划

## 状态

**配套工作 / P1。** 当前 archive-bound profile 已建立。仅在 archive/profile 变化或
A5R 暴露兼容缺口时重开本计划。它为执行中的 connectivity 计划提供配套支持，但不占用
独立的主要 WIP 槽位。跨计划优先级以[工程计划注册表](README.md)为准。

## 范围与归属

本计划定义固定 WS63 radio archive 与原生 Rust runtime 之间的有界兼容层。它不会把
`hisi-rtos` 变成 LiteOS 克隆，也不会增加 LiteOS backend。

必须明确分开两类事实：

- `hisi-rtos` 行为来自其 Rust API、Embassy/embedded Rust 需求、
  [RTOS 调度语义与验证计划](hisi-rtos-semantics-and-verification.md)、host deterministic
  测试和未来跨芯片 runtime 架构。
- WS63 RF 兼容行为来自 blob ABI、精确 archive hash、对应的 fbb_ws63 LiteOS
  行为/反汇编和 RF HIL。

依赖方向如下：

```text
WS63 blob -> ws63-radio-sys -> WS63 compatibility adapter
                                      |
                                      v
                         hisi-rf-rtos-driver -> hisi-rtos
Application / Embassy --------------------------^
```

`hisi-rf-rtos-driver` 拥有芯片中立的小能力契约。WS63 adapter 拥有 `LOS_*`/`osal_*`
调用约定、priority/tick 转换、direct-handoff profile、callback context 和兼容测试。
原厂 LiteOS 只作为产品依赖图之外的 oracle。

## 三层证据门槛

### ABI 契约

由 `ws63-radio-sys` 负责：

- archive SHA-256 和 build identity；
- `llvm-nm -u` 必需符号 manifest；
- RV32 函数签名、callback ABI、variadic/`va_list` 边界；
- 结构体 size/alignment/offset 断言；
- ROM symbol、relocation 输入和 linker 要求。

`required-symbols.txt` 必须纳入版本控制。出现新的 unresolved symbol 或 archive hash
时，CI 必须失败并要求重新评审 profile；兼容面不得静默扩大。

### 语义契约

由 WS63 compatibility adapter 负责。LiteOS 场景要重写为 Rust 行为断言，不能复制成
`LOS_*` API，也不能宣称为通用 RTOS 语义。deterministic harness 使用 `Spawn`、
`Yield`、`AdvanceTime`、`LockScheduler`、`SemWait`、`SemPost`、`EnterIrq`、`ExitIrq`
等 action，并观测 `Running`、`Blocked`、`TimedOut`、`Granted`、
`PreemptionDeferred`、`ContextSwitched` 等状态。

差异由类型化 adapter state 吸收，包括 `VendorPriority`、tick-to-duration 转换、deferred
callback worker 和 ABI return-code mapping。出现不匹配时先修 adapter；只有它确实是
合理的跨平台能力时，才改变通用 runtime 行为。

这些场景是兼容断言，不是规范性 scheduler model。它们依赖的每条通用不变量都必须引用
[RTOS 调度语义与验证计划](hisi-rtos-semantics-and-verification.md)拥有的 requirement；
vendor-only 行为只能留在当前 profile 及其固定 archive hash 内。

### 真机契约

由 RF HIL 负责：init、scan、connect、WPA、DHCP、ARP、ping、reset matrix、
IRQ/scheduler stress 和统计失败率。Host 语义测试不能替代 ABI 闭合或真机证据。

## 首批场景

只启用固定 blob 实际引用的能力。

- Scheduler：nested lock 延迟调度、unlock 后选择最高 priority、显式数字 priority 映射、
  ready task 抢占、同 priority FIFO yield、zero-tick vendor yield、只在同 priority 间
  time slicing。
- Semaphore：count/block、永久等待、timeout 后移出 wait queue、direct handoff、最高
  priority waiter、IRQ exit 后 ISR wake，以及 grant 不可被第三方抢走。
- Mutex：仅 owner 可 recursive unlock、最高 priority inheritance、多个 donor、传递
  donation、timeout 恢复和 direct handoff。
- Task/timer：one-shot/rearm/cancel、timeout rounding/wrap、callback context、task
  argument/stack alignment/return-to-exit、handle generation、stack reclamation，以及完整
  GPR/FPR/FCSR 保存。

只有 `nm -u` 和 call-site 证据证明 archive 使用 queue、event 或 software timer 时，
它们才能加入清单。LiteOS shell/POSIX 的完整能力不在范围内。

## Oracle 与署名

开放的 LiteOS V2 BSD-3 demo 可以在保留来源和 attribution 注释的前提下改写为 Rust
断言。WS63 原厂 5.10 source、最终 map 和 disassembly 只作为行为 oracle；除非增量许可
允许，否则不得复制。每项测试记录 oracle path/version、blob SHA-256，以及一个或多个
证据标签：`OpenSourceBehavior`、`VendorSourceBehavior`、`DisassemblyConfirmed`、
`BlobHilConfirmed`。

compatibility profile 绑定 archive hash，而不是笼统的“LiteOS 5.10”。archive 变化后
必须重新生成 symbol、重开语义评审并重跑 RF HIL。

## 里程碑

1. **CABI0 Manifest：**生成 archive hash、required symbol、ABI layout 和
   symbol-to-capability mapping。
2. **CSEM0 测试框架：**实现 deterministic scheduler/semaphore/IRQ 场景，优先覆盖曾与
   `WLAN_AUTH_RSP2_TIMEOUT` 风险相关的路径。
3. **CSEM1 Mutex/task：**只增加 blob 实际使用的 mutex、timer、queue、event 和 task
   lifecycle 场景。
4. **CHIL0 一致性验证：**对 unchanged image 运行 reset matrix 与
   init/scan/connect/ping，对照原厂 firmware oracle 比较失败和 trace。
5. **CCI0 门槛：**把 archive hash、symbol closure、semantic profile 和 HIL evidence
   固定为显式 release input。

Embassy 集成继续属于原生 `hisi-rtos` 工作。它与 vendor thread 共享 runtime，永远不
启动第二套 LiteOS scheduler。
