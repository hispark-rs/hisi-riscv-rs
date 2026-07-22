# `hisi-rtos` 可观测性与 CLI 优先调测计划

## 状态

这是 [RTOS 未来架构](hisi-rtos-future-architecture.md) 的调测事实源，属于 ping baseline
之后的 deferred work，不阻塞当前 WS63 RF 路径。调测是 kernel contract 的一等能力，
不是后加文本日志；Host、QEMU 和真实芯片必须共享协议、CLI 与状态模型。
其中 thread state、wake reason、switch reason、budget event 和 invariant ID 不由本文
重新定义，统一来自
[RTOS 调度语义与验证](hisi-rtos-semantics-and-verification.md)。

## 分层与归属

1. **Kernel 观测 hook**：thread switch/block/wake、IPC wait、timer、IRQ、domain
   switch、budget exhausted、stack watermark、fault/restart 事件。
2. **`hisi-rtos-debug-protocol` 加 target 端最小调试 agent**：版本化
   Request/Response/Event、sequence id、length/CRC、UART framing、golden vectors，保持
   `no_std`。
3. **`hisi-rtos-cli`**：独立 host 工具和 release unit，负责 ELF/DWARF/build-id/manifest
   解析、交互、JSON 和 CI 输出。

内核只保留稳定 snapshot/trace hooks 和最小 fault/restart primitive。复杂 wire protocol、
授权、符号化和控制策略属于 debug agent/host CLI，避免扩大 kernel TCB。

第一阶段 Rust package 名是 `hisi-rtos-cli`，用户命令可安装为 `hisi-rtos`：

```text
hisi-rtos ps
hisi-rtos trace --duration 5s
```

协议和 UX 稳定后，再决定是否由统一 `hisi` 前端委托为 `hisi rtos ...`。
`hisi-fwpkg`、`hisiflash`、`hisi-rtos-cli` 始终保持独立库/release unit；聚合前端不得
复制实现。

## 身份与构建契约

- `ThreadId` 包含 slot + generation，restart/slot reuse 后旧 handle 必须失效。
- RTOS manifest 与 ELF 是 thread/domain/memory/trace schema 的事实源。
- firmware 暴露 build-id 与 protocol version；CLI 自动校验 target、ELF、manifest。
- dump 与 ELF build-id 不一致时拒绝符号化，不能输出看似合理的错误 backtrace。

## 首批命令

- `hisi-rtos info`
- `hisi-rtos ps` / `top`
- `hisi-rtos domains`
- `hisi-rtos irq`
- `hisi-rtos memory` / `alloc`
- `hisi-rtos trace --duration ...`
- `hisi-rtos latency`
- `hisi-rtos waits` / `deadlocks`
- `hisi-rtos stack <thread>`
- `hisi-rtos explain <thread>`
- `hisi-rtos dump capture`
- `hisi-rtos dump inspect --elf ...`
- host 命令：`record`、`replay`、`explore --schedule-seed ...`
- 仅开发模式命令：`task restart`、`fault inject`、`pause`、`resume`

每个命令必须支持 `--json`、稳定 exit code、`--timeout`；错误消息给出可执行下一步。
命令 reference 与 executable contract 由同一 schema 生成或在 CI 校验。Chrome Trace、
rtos-trace、wait/deadlock graph 和 DAP/GDB/IDE adapter 后置，不阻塞第一阶段。

## Transport 契约

- `--transport probe://...`：优先 RTT、memory mailbox 或 probe-rs 通用 library 能力；
  不向 probe-rs 加 HiSilicon RTOS/format parser。
- `--transport serial:///...`：COBS + length + CRC framed channel，可与文本日志复用物理 UART。
- `--transport tcp://...` / `unix://...`：host deterministic runtime 通道。
- QEMU 使用同一 wire protocol 和 schema。
- semihosting 只用于 test/result，不作为常规实时 trace。
- attach 默认绝不 reset/flash；只有显式 run/flash flow 才改变目标状态。

Protocol 必须有 version negotiation、bounded frame、sequence id、unknown-message behavior、
golden vectors、corruption/truncation tests 和 backpressure policy。

## Host 模式与一致性

**host-deterministic**：单线程调度状态机、虚拟时间、固定随机 seed、IRQ/timeout/OOM/panic
injection、schedule exploration、snapshot/replay。

**host-native**：主机 thread/network integration，可接普通 debugger/profiler，验证 integration
而不宣称确定性。

Host/QEMU/HIL trace 共用 schema并支持 parity diff。最终目标是真机调度 fault 被捕获为带
build-id 的 trace/dump，并尽量在 deterministic backend 重放。

## 安全与资源占用 Profile

| Profile | 能力 |
| --- | --- |
| `off` | 完全移除。 |
| `metrics` | 计数器和 crash 摘要。 |
| `read-only` | 查看 task/trace/dump。 |
| `development` | restart/pause/fault injection 控制。 |
| `host-test` | 完整 injection/replay/exploration。 |

产品 firmware 默认不得开放任意 memory read/control。危险控制需要编译期 feature、target
capability、可选认证和 CLI 显式 `--allow-control`；read-only attach 不能隐式升级权限。

## 延期里程碑

1. **D0 Schema 定义**：在 RTOS 调度语义与验证 V0/V1 的 state/event/reason ID 基础上
   生成 snapshot/trace/build-id/protocol schema 与 golden vectors。
2. **D1 CLI Host Mock**：`hisi-rtos-cli` 完成 info/ps/trace/waits。
3. **D2 WS63 UART**：framed serial transport 与只读 agent。
4. **D3 Probe/RTT**：通用 probe transport，不魔改 probe-rs 格式。
5. **D4 Dump**：crash dump、ELF 符号化、stack watermark。
6. **D5 确定性**：record/replay/fault injection/schedule exploration。
7. **D6 Supervisor**：task generation、restart 和故障策略。
8. **D7 Hi3322 诊断**：PMP/TES domain/fault 可见性。
9. **D8 SMP 诊断**：per-hart trace 与时钟校准。
10. **D9 体验 adapter**：TUI、DAP、IDE。

D0-D9 只能在 connectivity baseline 后排期，不插入当前 RF 临界路径。
