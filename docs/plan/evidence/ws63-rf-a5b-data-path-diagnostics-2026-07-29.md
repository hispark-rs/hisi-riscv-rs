# WS63 A5B 数据面诊断证据（2026-07-29）

## 目标

在不改变 packet ownership、不捕获帧内容且不重复烧录的条件下，区分
smoltcp、vendor TX submission、vendor/Rust RX boundary、MAC/DMAC 和中断层的停滞。
本证据只用于关闭 A5B 可靠性归因，不替代 pure-WPA3 gate。

## 固件与执行契约

- ELF SHA-256：
  `cea59dd111a1336d9ef8e6e5cf2401ffdcf545a35201cded1e2d6f364bb4643c`
- profile：`wifi-connectivity-upstream-wpa2`
- 烧录：probe-rs binary image，3 MHz，完整 verify，82.57 s
- 矩阵：同一已烧录镜像，20 次 J-Link nRST；每轮最多 90 s
- 结果：18 pass、1 ping timeout、1 connect operation timeout
- 20 轮 `auth_rsp2_timeouts=0`

下载/verify 是独立 transport 证据；矩阵中不重复下载，避免把 probe-rs 波动混入
Wi-Fi 结果。

## 诊断能力边界

`hisi-rf-radio-diagnostics/v5` 的 `instrumented_capabilities` 按位声明实际测量点：

| Bit | 边界 | 窄诊断 |
|---:|---|---|
| 0 | vendor TX submission | 有 |
| 1 | vendor RX boundary | 有 |
| 2 | MAC RX statistics | 无 |
| 3 | DMAC TX completion | 无 |

因此本轮 `path_caps=0x03`。未声明的计数为零只表示不可用，不能解释为硬件没有活动。
窄诊断仅在 feature 启用时增加 TX 原子计数；普通 profile 不包含该存储和热路径操作。

第一次原型把 ROM `hh503_get_mac_rx_statistics_data()` 放入统一快照。两轮均完成
WPA2 connect，却在进入 DHCP runner 前停止。该 helper 不是任意上下文安全的只读
getter，现已从窄快照移除。修正后的单轮 smoke 完整通过，随后才执行 20-reset
矩阵。后续若恢复 MAC capability，必须先证明调用上下文、等待上界和失败恢复，不能
让统一诊断 API 包含无界 ROM 调用。

## 失败归因

### Ping timeout

该轮完成 scan、WPA2 connect 和 DHCP，随后 gateway 与 public ICMP 均为 `0/5`。
关键快照：

- smoltcp TX `14`
- vendor TX submission `14`
- TX reject `0`
- vendor RX boundary `6`
- Rust RX `6`
- RX queue drop `0`
- DHCP client/server `3/2`
- IRQ44 `2`，IRQ45 `774`

这证明 Rust TX 没有在 vendor submission 之前丢失，vendor/Rust RX boundary 也没有
出现内部计数分叉。由于 capability mask 不包含 MAC/DMAC，本证据不能继续断言帧已由
DMAC 发出，也不能断言 MAC 是否收到 echo reply。A5B 数据面归因仍未闭合。

### Connect operation timeout

该轮在 connect operation deadline 结束，backend code 为 `0x5732b004`；没有
auth-response-2 timeout，event queue drop 为 0。它没有进入 DHCP/data-path 阶段，
因此不能与 ping timeout 合并统计。后续连接诊断需要保留最后 association/EAPOL
阶段，而不是把它记成数据面失败。

## 当前结论

- 低扰动诊断自身不再阻塞 DHCP；单轮 smoke 和 18 个完整矩阵轮次证明 happy path
  保持可运行。
- Rust 可观测 TX/RX 两端在 ping 全丢轮继续前进，但 MAC/DMAC 层仍缺安全观测点。
- A5B 仍是 active：20/20 reliability gate 未完成，默认 backend 不因本证据切换。
- probe-rs 3 MHz 完整 verify 成功；其下载稳定性继续作为独立工具链问题跟踪。
