# `hisi-rtos` Observability And CLI-First Debugging Plan

## Status

这是 [RTOS 未来架构](hisi-rtos-future-architecture.md) 的调测事实源，属于 ping baseline
之后的 deferred work，不阻塞当前 WS63 RF 路径。调测是 kernel contract 的一等能力，
不是后加文本日志；Host、QEMU 和真实芯片必须共享协议、CLI 与状态模型。

## Layers And Ownership

1. **Kernel observation hooks**：thread switch/block/wake、IPC wait、timer、IRQ、domain
   switch、budget exhausted、stack watermark、fault/restart。
2. **`hisi-rtos-debug-protocol` + target minimal debug agent**：版本化
   Request/Response/Event、sequence id、length/CRC、UART framing、golden vectors，保持
   `no_std`。
3. **`hisi-rtos-cli`**：独立 host tool/release unit，负责 ELF/DWARF/build-id/manifest
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

## Identity And Build Contract

- `ThreadId` 包含 slot + generation，restart/slot reuse 后旧 handle 必须失效。
- RTOS manifest 与 ELF 是 thread/domain/memory/trace schema 的事实源。
- firmware 暴露 build-id 与 protocol version；CLI 自动校验 target、ELF、manifest。
- dump 与 ELF build-id 不一致时拒绝符号化，不能输出看似合理的错误 backtrace。

## Initial Commands

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
- host：`record`、`replay`、`explore --schedule-seed ...`
- development-only：`task restart`、`fault inject`、`pause`、`resume`

每个命令必须支持 `--json`、稳定 exit code、`--timeout`；错误消息给出可执行下一步。
命令 reference 与 executable contract 由同一 schema 生成或在 CI 校验。Chrome Trace、
rtos-trace、wait/deadlock graph 和 DAP/GDB/IDE adapter 后置，不阻塞第一阶段。

## Transport Contract

- `--transport probe://...`：优先 RTT、memory mailbox 或 probe-rs 通用 library 能力；
  不向 probe-rs 加 HiSilicon RTOS/format parser。
- `--transport serial:///...`：COBS + length + CRC framed channel，可与文本日志复用物理 UART。
- `--transport tcp://...` / `unix://...`：host deterministic runtime。
- QEMU 使用同一 wire protocol 和 schema。
- semihosting 只用于 test/result，不作为常规实时 trace。
- attach 默认绝不 reset/flash；只有显式 run/flash flow 才改变目标状态。

Protocol 必须有 version negotiation、bounded frame、sequence id、unknown-message behavior、
golden vectors、corruption/truncation tests 和 backpressure policy。

## Host Modes And Parity

**host-deterministic**：单线程调度状态机、虚拟时间、固定随机 seed、IRQ/timeout/OOM/panic
injection、schedule exploration、snapshot/replay。

**host-native**：主机 thread/network integration，可接普通 debugger/profiler，验证 integration
而不宣称确定性。

Host/QEMU/HIL trace 共用 schema并支持 parity diff。最终目标是真机调度 fault 被捕获为带
build-id 的 trace/dump，并尽量在 deterministic backend 重放。

## Security And Footprint Profiles

| Profile | Capability |
| --- | --- |
| `off` | 完全移除。 |
| `metrics` | counters + crash summary。 |
| `read-only` | task/trace/dump inspection。 |
| `development` | restart/pause/fault injection。 |
| `host-test` | 完整 injection/replay/exploration。 |

产品 firmware 默认不得开放任意 memory read/control。危险控制需要编译期 feature、target
capability、可选认证和 CLI 显式 `--allow-control`；read-only attach 不能隐式升级权限。

## Deferred Milestones

1. **D0 Schema**：snapshot/trace/build-id/protocol schema 与 golden vectors。
2. **D1 CLI host mock**：`hisi-rtos-cli` 完成 info/ps/trace/waits。
3. **D2 WS63 UART**：framed serial transport 与 read-only agent。
4. **D3 Probe/RTT**：通用 probe transport，不魔改 probe-rs format。
5. **D4 Dumps**：crash dump、ELF symbolization、stack watermark。
6. **D5 Determinism**：record/replay/fault injection/schedule exploration。
7. **D6 Supervisor**：task generation、restart、fault policy。
8. **D7 Hi3322 diagnostics**：PMP/TES domain/fault visibility。
9. **D8 SMP diagnostics**：per-hart trace 与 clock calibration。
10. **D9 UX adapters**：TUI、DAP、IDE。

D0-D9 只能在 connectivity baseline 后排期，不插入当前 RF 临界路径。

