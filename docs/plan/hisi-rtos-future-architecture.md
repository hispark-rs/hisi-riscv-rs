# `hisi-rtos` 未来架构展望

## Status And Scope

本文是 **connectivity baseline 之后的 deferred architecture outlook**，不是当前实施
清单。当前工作仍按 [Connectivity 全栈计划](hisi-connectivity-stack.md) 完成 WS63
RF parity；不得为了本文的微内核方向中断或扩大 init/scan/connect/ping 临界路径。

`hisi-rtos` 的长期定位既不是另一个包办应用生态的 Ariel OS，也不只是 RF blob 的
OSAL shim。它是可裁剪、可移植、支持栈式线程、保护域和主机调测的 embedded runtime
kernel，负责执行、隔离、时间、IPC 和调试。它不拥有 Wi-Fi/BLE/SLE API、TCP/IP、TLS、
NVS 格式、ROM symbols、镜像格式、HAL 或应用框架。

调测是这一架构的一等能力；协议、CLI、transport 和 D0-D9 的唯一详细计划见
[RTOS observability 与 CLI 计划](hisi-rtos-debugging-cli.md)。
调度策略、timeout/IRQ 竞态、priority inheritance、budget 补充与 SMP
扩展的规范和证明 gate 唯一详细计划见
[RTOS 调度语义与验证](hisi-rtos-semantics-and-verification.md)。

## Ecosystem Position

```text
Standalone app -> hisi-rtos -> hisi-hal
Ariel OS app   -> ariel-os-hisi adapter -> hisi-rtos / hisi-hal / hisi-rf
hisi-rf        -> hisi-rf-rtos-driver -> replaceable backend
                                           |-- hisi-rtos
                                           |-- future Ariel adapter
                                           `-- host mock
Embassy app    -> hisi-rtos-embassy -> Embassy executor
```

`hisi-rf-rtos-driver` 保持 runtime-neutral；`hisi-rf` 不反向拥有 scheduler。Embassy
通过 adapter 与 RTOS thread/time 共存，不重写 async executor。Ariel OS 作为上层 library
OS、网络和跨 MCU 集成入口；本项目不重复建设它已有的通用网络、CoAP、传感器和应用生成体验。

原厂 LiteOS 只提供芯片对应版本的行为/汇编 oracle，不是可替换 backend，也不进入产品
依赖图。不同芯片的 LiteOS port、libc、allocator、IRQ 和 linker contract 不一致，维护
完整兼容 backend 会无边界扩张。`hisi-rtos` 是唯一 native backend；芯片 ABI shim 只映射
blob 实际引用且由未解析符号 manifest 固定的小集合。

## Execution Model

必须分离三个概念：

1. **ProtectionDomain**：安全、资源权限与 fault containment 单位。
2. **Thread**：内核调度单位，拥有独立栈，承载 vendor blob、阻塞 C ABI 和系统服务。
3. **Future**：Embassy executor thread 内的协作任务，不直接成为 PMP 调度实体。

一个 domain 可包含多个 thread；一个 executor thread 可承载多个 Future。同域 thread
切换可避免重复编程 PMP，但不能把 Future 误宣称为硬件隔离单位。

## Scheduling Model

远期策略以 validated policy 表达：

```rust,ignore
enum RunPolicy {
    Cooperative,
    Budgeted { budget: CpuBudget },
    Preemptive,
}
```

- `Preemptive` thread 在更高优先级任务 ready 时可立即抢占，同优先级可 round-robin；
  `Cooperative` thread 不因 IRQ wake 自动切换。
- 普通 Rust executor thread 主要主动 yield/block；显式 handoff 后仍通过统一 trap-frame
  mechanism 切换，不形成第二套 scheduler backend。
- vendor blob 默认 `Budgeted`，预算耗尽由 timer 强制切走。
- mutex 必须支持 priority inheritance，不能让高优先级 radio task 被低优先级 owner
  无限反转。
- IRQ top half 只 ack/record/wake；用户 callback、复杂协议和回收进入 deferred thread。
- 当前 WS63 flat backend 已具备显式 priority scheduling、TIMER/SOFT interrupt
  preemption、同优先级 time slicing、priority inheritance，以及共享 TIMER_INT0 的
  Embassy time/executor 真机共存；budget enforcement 与 trace 仍未闭合。当前共存证据见
  [A3 Embassy coexistence](evidence/ws63-rf-a3-embassy-coexistence-2026-07-14.md)，F3 仍负责
  把已验证行为收敛成正式 adapter/component boundary。

`RunPolicy` 的 normative 定义不由本文复制；尤其 `Budgeted` 的 capacity、
replenishment、IRQ 计费和 throttle 语义必须在
[RTOS 调度语义与验证](hisi-rtos-semantics-and-verification.md) V3 冻结并通过
反例搜索后才实现。

`Budgeted` 长期保持周期 CPU quota 上限，不承诺最低 CPU 服务量。完成当前 quota、
ported capability、观测和 RF scheduling profile 后，只有测量证据证明 critical thread
需要确定服务下界时，才引入独立 Reservation。Q0-Q4 与 G0-G5 的唯一实施顺序、
admission/proof gate 和 Cooperative Reservation 边界见
[Quota Closure And Guaranteed Service Evolution](hisi-rtos-semantics-and-verification.md#quota-closure-and-guaranteed-service-evolution)。

## Workspace Shape

早期先留在同一 workspace；只有 API 稳定且出现独立消费者后才拆成独立仓：

| Component | Responsibility |
| --- | --- |
| `hisi-rtos-core` | 无芯片依赖的 scheduler、thread、IPC、time、domain 状态机。 |
| `hisi-rtos` | profile facade、resources 与启动入口。 |
| `hisi-rtos-port-riscv` | RISC-V arch context/trap/syscall mechanism。 |
| `hisi-rtos-port-ws63` | WS63 timer/software IRQ 和 flat platform policy。 |
| `hisi-rtos-port-hi3322` | Hi3322 PMP/TES/TEE/SMP platform policy。 |
| `hisi-rtos-embassy` | Embassy executor/time adapter，不重写 executor；HAL 只保留外设 async traits 与底层 timer/IRQ mechanism。 |
| `hisi-rtos-host` | deterministic/native host backend。 |
| `hisi-rtos-trace` | snapshot/trace schema 与 observation hooks。 |
| `hisi-rtos-macros` | 静态 manifest/task/domain 声明的受控生成。 |

`hisi-rf-rtos-driver`、`hisi-rf`、`hisi-hal`、`hisi-rom-sys`、`hisi-nvs`、
`hisi-storage`、`hisi-crypto` 和 `hisi-tls` 保持独立 ownership boundary。

## Portable Core And Ports

`hisi-rtos-core` 不读取 PAC、CSR 或 MMIO。平台能力由窄接口注入：

- `ArchPort`：`context_switch`、`start_first`、`idle`；
- `TimerPort`：`now`、`arm_deadline`、`cancel`；
- `ProtectionPort`：activate domain configuration；
- `SmpPort`：`current_hart`、reschedule IPI。

因此 host、WS63 和 Hi3322 复用同一状态机；芯片 port 负责 unsafe hardware mechanism，
core 负责可 host-test 的 policy。

## Named Profiles

优先提供审计过的命名 profile，避免用户任意组合大量 feature：

| Profile | Contract |
| --- | --- |
| `embassy-only` | 纯 Embassy，不启动 RTOS scheduler。 |
| `minimal` | 单 thread/timer/无堆。 |
| `flat` | thread/IPC/preemption，无硬件隔离；WS63 目标。 |
| `radio` | `flat` + RF blob ABI。 |
| `protected` | PMP domain/syscall/fault supervisor。 |
| `secure` | TES/TEE service。 |
| `smp` | 多 hart、affinity、IPI。 |
| `host-test` | 虚拟时间、trace、fault injection、replay。 |

WS63 `flat` 只有逻辑 domain，不宣称安全隔离。Hi3322 PMP/TES profile 才能承诺对应
硬件保护；保护能力必须由 fault HIL 证明。

## Task Capacity And Static Storage Evolution

当前 A3/RF 兼容基线固定为 **15 个动态 task**。实现使用 17 个 scheduler slot：1 个
adopted main、1 个 internal idle 和 15 个 dynamic slot。main/idle 是 runtime-owned，
不得消耗公开的 15-task 配额；第 16 次 dynamic spawn 必须返回明确的 `NoTaskSlots`，
不能挂在 RF init，也不能与 heap、semaphore 或其他 control-block 耗尽混为同一个错误。
diagnostics 必须分别报告 internal count、dynamic capacity、dynamic used 和 dynamic free。

这个 17-slot table 只用于恢复并冻结当前 RF parity，**不是永久容量上限或公开存储
布局**。本轮 A3 HIL 收口只审计所有 task/ready/wait/trace 数组和 bitmask 都使用同一
slot-count contract，并证明 idle 永远不会被动态分配。完成 Cooperative、Budgeted、
Preemptive、Embassy 和 RF HIL 后，容量机制按独立原子迁移实施：

```rust,ignore
static SCHEDULER: StaticCell<SchedulerStorage<15>> = StaticCell::new();

let storage = SCHEDULER.init(SchedulerStorage::new());
start_with_port(config, resources, port, storage)?;
```

- `SchedulerStorage<const N: usize>` 中的 `N` 只表示 dynamic task capacity；adopted main
  与 internal idle 使用独立 TCB，不占用 `N`。
- `start_cooperative` 和 ported start 都接收应用提供的 `&'static mut
  SchedulerStorage<N>`；保持 no heap、内存成本编译期可见，不使用 Cargo feature 控制
  全局 task 数量。
- 提供 `minimal`、`wifi`、`connectivity` 等经真实 blob/HIL 校准的 alias/profile；数字
  必须来自 resource report，不让用户盲猜。
- 初始化前引入 task-slot reservation/quota。RF 在创建 worker 前一次性 reserve 所需
  dynamic slots；不足必须返回类似 `RF requires N task slots, available M`。后续 Wi-Fi、
  BLE、SLE、network、TLS 和 debug 分别声明需求，domain quota 互不侵占。
- 静态 RTOS manifest 最终汇总 domain/thread 需求，生成 storage 配置和 resource report；
  manifest 是容量规划事实源，profile 只是经过证据校准的便捷入口。
- 当前 `TaskId` 低 8 位编码 slot index。实现必须有 compile-time capacity bound；未来若
  超过 256 个 slot，必须版本化 handle ABI，禁止静默截断或复用旧编码。

`SchedulerStorage<N>`、reservation/quota、manifest generation 都是 ping/A3 baseline
之后的 deferred work，不得在当前 RF 回归中提前引入大规模存储重构。

## Protection, Faults And Host Testing

- 借鉴 Hubris：静态 task manifest、task generation、用户态 supervisor、独立 dump/restart。
- 借鉴 Xous/Zephyr：deterministic 与 native 两种 host mode、虚拟时间、固定随机 seed、
  schedule/IRQ/timeout/OOM/panic injection、trace/snapshot/replay、wait/deadlock graph。
- 借鉴 Tock：PMP process、scheduler policy 解耦、cooperative + time budget。
- 借鉴 esp-rtos：radio-driver boundary、stacked vendor task、Embassy coexistence。
- 借鉴 Ariel OS：上层 library OS 集成，而非重复建设完整 IoT application framework。

Thread identity 必须包含 generation；任务退出/重启后，旧 handle 不能误指向复用 slot 的
新实例。fault supervisor、dump 与 restart primitive 保持小而稳定，复杂策略进入独立
supervisor/debug agent。

## Deferred Milestones

1. **F0 Baseline freeze**：保留当前 RF5/ping marker、layout、15-dynamic-task capacity
   和 HIL 证据。
2. **F1 Pure core and storage ownership**：抽取无硬件状态机，在 host deterministic
   backend 验证；独立迁移到 `SchedulerStorage<N>` 与初始化前 reservation，不改变 F0
   connectivity parity。
3. **F2 WS63 flat port**：context/timer/software IRQ 进入明确 port。
4. **F3 Embassy coexistence**：把当前 flat-backend HIL fixture 收敛为正式 executor/time
   adapter，并保持 vendor thread parity。
5. **F4 Scheduling closure**：先通过调度语义与验证计划 V0-V4 和 Q0-Q4，再闭环
   quota preemption、priority inheritance、task/group observability 与 trace。最低服务
   Reservation 属于可选 G0-G5 扩展，不是当前 connectivity gate。
6. **F5 RF backend migration**：保持 init/scan/connect/ping parity。
7. **F6 Logical domains**：fault supervisor、generation、dump/restart。
8. **F7 Hi3322 PMP**：真实 domain isolation 与 fault evidence。
9. **F8 TES/TEE**：secure service boundary。
10. **F9 SMP**：affinity、per-hart run queue、IPI、clock calibration。
11. **F10 Generality gate**：跨芯片/非 RF consumer 证据后再决定独立 release 拆分。

F1-F10 不进入当前 RF connectivity release gate。尤其不在 ping baseline 前进行大拆分。

## Non-Goals

- 不重写 Ariel OS 已有的通用网络、CoAP、sensor 或 application generation。
- 不把 IP stack、TLS、NVS、ROM、image format 或 radio protocol 收进 kernel。
- 不把 WS63 logical domain 描述为安全隔离。
- 不因未来微内核规划扩大当前 RF 任务范围或阻塞 connectivity parity。
