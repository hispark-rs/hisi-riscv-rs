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

## DMAC 边界补测

同日补入两个只做 `AtomicU32` 递增并原样转发的 linker wrapper：

- bit 3：`dmac_tx_complete_event_handler` 调用次数；
- bit 4：`dmac_rx_prepare_data_patch` 调用次数。

窄诊断 capability mask 因而从 `0x03` 扩为 `0x1b`。wrapper 不解析帧、不分配内存、
不调用用户代码，也不读取曾阻塞的 ROM MAC statistics helper。TX completion 是 callback
调用数，一次 callback 可能完成多个 descriptor；DMAC RX prepare 还包含管理帧和驱动内部
流量，二者都不能与 Rust L2 frame 数一一对应。

补测固件经 plain Cargo 单次链接，37 项 ROM patch 存在、vendor relocation 为 0。
第一次 3 MHz 下载在首个 page write 超时，物理 nRST 恢复后重新执行；第二次 3 MHz
完整 verify 成功，耗时 83.33 s。下载失败单列为 probe-rs transport 事件，没有计入
网络矩阵。

恢复后的同一最终镜像只烧录一次，随后完成 20 次 J-Link nRST：

- 20/20 完整 connectivity contract 通过；
- 20/20 `auth_rsp2_timeouts=0`；
- 每轮 `path_caps=0x1b`；
- vendor TX submission 为 13--14，DMAC TX completion callback 为 100--113；
- DMAC RX prepare 为 333--513，最终 vendor/Rust RX 为 13--19；
- `tx_failed=0`，event drop、runner error 和 100 ms runner budget violation 均为 0；
- 在线分类与离线复算均为 `{"pass": 20}`。

这证明新增 DMAC 观测边界在当前 WPA2 profile 上低扰动、持续有活动，并把未来失败的
定位范围推进到 MAC/空口之前。由于本轮没有复现先前 ping/connect 反例，它不能反向证明
旧失败根因，也不恢复 MAC statistics capability；A5B 的反例归因项继续保持 active。
本机原始证据位于
`/private/tmp/ws63-a5b-data-path-20260729-r2`。

## 发布与依赖传播

- `hisi-rf-ws63 0.1.0-alpha.50` 已发布；main CI
  `30456386488` 和 publish workflow `30456912965` 均通过。
- `hisi-rf 0.1.0-alpha.61` 已精确选择该 backend；main CI
  `30457654110` 覆盖 host/RV32、公开 API、package 和
  macOS/Linux/Windows crates.io-only consumer，publish workflow
  `30458080639` 通过。
- `ws63-examples` commit `1238b4e` 已将公开 connectivity example
  更新到 facade alpha.61，fmt workflow `30458266768` 通过。

这些发布证据只证明已验证的诊断实现可由公开依赖复现，不改变上文对旧反例根因和
pure-WPA3 gate 的保守结论。

## PAC/HAL WLMAC 只读观测

后续没有恢复会阻塞的 ROM statistics helper，而是在 SVD 中建模 WLMAC statistics
窗口，并由 PAC/HAL 暴露一次无副作用的只读快照。新增六个计数：

- successful/failed non-AMPDU；
- successful/failed MPDU in AMPDU；
- RX AMPDU；
- filtered RX。

这使 capability mask 从 `0x1b` 增至 `0x1f`。HAL 快照只在短 critical section 中读取
六个 RO 寄存器，不轮询、不等待、不调用 vendor 代码；RF 仍只在明确诊断 feature 下
消费该能力。

同一 release-train 镜像执行了两轮 unchanged-image 20-reset 矩阵：

| 矩阵 | Pass | Connect failure | Ping timeout |
| --- | ---: | ---: | ---: |
| WLMAC r2 | 18 | 0 | 2 |
| WLMAC r3 | 15 | 3 | 2 |

r2 的两个 ping failure 都完成 scan/connect/WPA/DHCP。WLMAC successful/failed/filtered、
DMAC RX prepare 和 IRQ45 持续增长，RX queue drop 为 0，但只有一个 ICMP reply 跨过
vendor RX boundary。r3 又证明失败不是首轮启动特例，并同时保留了 association/control
尾部失败这一独立类别。由此可以排除“整个 MAC/DMAC RX engine 停止”和“Rust RX queue
溢出”，但不能从 target-only counters 判断 AP 是否发送 reply，也不能区分关联后 PM、
过滤、密钥或 vendor frame classification。

相关 release closure：

- `ws63-svd f4e2d5b`；
- `ws63-pac 0.4.1`；
- `hisi-hal 0.7.0-alpha.4`；
- `hisi-rf-ws63 0.1.0-alpha.51`；
- `hisi-rf 0.1.0-alpha.62`。

## STA power-save 诊断 A/B

为检验关联后的 STA 省电是否导致 reply 接收窗口丢失，增加了一个隐藏且显式 opt-in
的诊断 feature。它在 `Connected` event 后取得当前 STA VAP，并以 SDK 定义的
`HOST/OFF` 参数关闭 station power save。调用成功 marker 必须出现，否则该轮停止，
不进入网络统计。正常 profile 不启用该 feature，也不改变 vendor 默认 PM 策略。

同一诊断镜像只烧录一次，随后执行 20 次 J-Link nRST：

- 17 次完整 connectivity contract 通过；
- 2 次在 connect/control 尾部失败；
- 1 次完成 DHCP 后 gateway/public 均 `0/5`；
- 20 次 `auth_rsp2_timeouts=0`；
- ping failure 中 `RFDBG_STA_PM_DIAG mode=off status=ok` 已出现；
- ping failure 中 WLMAC/DMAC/IRQ 继续增长，vendor RX 为 6，ICMP RX 为 0。

结果没有超出前两轮 baseline 波动，也没有消除数据面反例。因此：

- STA power save 不是已经证实的唯一根因；
- 不把 PM-off 升格为默认行为或稳定配置；
- 该 feature 只保留为可复现的诊断 A/B；
- A5B reliability gate 继续保持 open。

本轮第一次以 3 MHz 下载在 `0x00250000` page program 超时；硬件 nRST 后目标镜像只
完成了部分写入，随后用官方完整 FWPKG 恢复并确认 RF calibration 正常。第二次以
1 MHz 下载和完整 verify 成功，耗时 140.53 s。该事件单列为 probe-rs transport
可靠性，不计入 Wi-Fi 20-reset 矩阵。

## WLMAC 过滤与地址语义修正

后续自底向上审计确认，原 PAC 将 packed RX filter-control 寄存器误写成了 command
语义，VAP0 station/BSSID 的 32-bit word 也被诊断层按主机整数顺序解释。修正链保持
SVD 为寄存器事实源，HAL 只提供无副作用快照，RF 层只输出不泄露地址值的布尔状态：

- `ws63-svd 604a159`：修正 WLMAC filter-control access/命名；
- `ws63-pac 0.4.3`：重新生成 PAC；
- `hisi-hal 0.7.0-alpha.6`：按网络字节序解码 station/BSSID；
- `hisi-rf-ws63 0.1.0-alpha.54`：传播 filter/identity 诊断，
  `0.1.0-alpha.55` 仅补 minimal profile 的 warning-free release closure；
- `hisi-rf 0.1.0-alpha.64`：发布公开 facade closure；
- `ws63-examples 39b7464`：公开 connectivity example 输出新的无秘密 marker。

修正后的最终真机 smoke 得到：

- `path_caps=0x3f`；
- STA address match 与 BSSID programmed 均为 true；
- packed RX filter-control 可观测且非零；
- gateway 和 public ICMP 均为 `5/5`。

这组结果证明诊断链不再因 packed register 语义或地址字节序制造假反例，并关闭该
寄存器解释子问题。它只有一轮 connectivity smoke，不能覆盖上面的 `18/20`、
`15/20` 和 PM-off 矩阵，因此 A5B 20-reset reliability gate 继续保持 open；它也不
构成 pure-WPA3 证据。

原始矩阵证据保存在
`/private/tmp/ws63-a5b-pm-off-20260729-r2/contract`。本地凭据文件是外部秘密输入，
不属于证据或仓库内容，并按用户要求继续保留供后续 HIL 使用。
