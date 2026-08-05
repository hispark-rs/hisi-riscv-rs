# WS63 双板 pure-WPA3 可靠性证据（2026-08-04）

## 结论

仓库内 Rust WPA3 SoftAP 与 upstream-native WPA3 STA 已完成真实硅片
SAE-only 能力证明，但发布可靠性门槛仍未关闭。三个 unchanged-image 20-reset
矩阵累计得到 `58 pass + 2 local_data_path_failure`；60 轮均完成 pure-WPA3
scan、SAE、required PMF、association 和 DHCP，`WLAN_AUTH_RSP2_TIMEOUT=0`。

后两轮各为 20/20，只说明后续观测窗口未复现，不能覆盖第一轮的两个反例。
随后两组针对性矩阵再次复现本地回包失败，因此该门槛仍明确打开：关闭 STA power
save 的 A/B 为 `15 pass + 5 local_data_path_failure`；加入 late-disconnect 消费和
双向 IRQ/TX 诊断后的提交态矩阵为
`16 pass + 3 local_data_path_failure + 1 scan_error`。这两组均为
`auth_rsp2_timeouts=0`，证明省电不是根因，也不能用连接生命周期修复解释全部 TX/RX
反例。

2026-08-05 又完成一轮带两板 OSAL wait/wakeup 终态快照的提交态矩阵，结果为
`18 pass + 2 local_data_path_failure`。两次失败仍分别收到 `4/10` 和 `3/10`
sequence-checked UDP reply；认证、DHCP、ARP、IRQ45 和 vendor worker 都持续运行。
因此该轮证伪了“FRW worker 永久漏唤醒/睡死”这一候选，但没有关闭本地数据路径门槛。

随后完成的 DMAC 队列诊断与门槛修正矩阵再次得到 `18 pass + 2
local_data_path_failure`。两次失败均为真实 `0/10`，而不是公网 ICMP 丢包或部分回包；
AP 已生成并提交全部 reply，但软件描述符队列仍有积压、硬件数据队列为空且 TX completion
保持为 0。当前风险因此进一步收窄到 AP 软件队列向硬件调度/出队的边界，A5B 发布门槛
仍未关闭。

## 版本边界

- `hisi-rf-ws63`: `0ae1f80b7c193c626c71cdda857953ee6da84b02`
  (`diag: snapshot WS63 MAC transmit counters`)
- `ws63-examples`: `9999f1ab8262b92b29f1e6a5ccf15df20a59a6e6`
  (`diag: report WLMAC transmit counters`)
- `hisi-rtos`: `b5af7c887be2f2f82bea026fa5da8211d5a49d87`
  (`chore: prepare 0.1.0-alpha.22`)
- AP/STA 通过显式 probe、J-Link serial 与 UART 绑定；矩阵每轮先 nRST AP，
  等待 AP 恢复，再 nRST STA。固件不读取开发主机 Wi-Fi 凭据。

本轮矩阵没有生成 artifact-identity manifest，因此这些记录是诊断证据，不替代
最终 release gate 所需的提交态 ELF hash 与 immutable identity。

后续 link-lifecycle 诊断矩阵已经补齐 artifact identity：

- STA `ws63-examples` commit `3e598552ffcfda4ecc02f8becc575e7e4d7720c8`，
  ELF SHA-256 `248ba24e421159610f0f31ccc4db2eb1f8efdc77c33e68a8af72a7fc2efbb0fa`；
- AP `ws63-examples` commit `3fc9cbbe335c25f02486b33e4e958c5006437865`，
  ELF SHA-256 `55ce2016b1523c00ff15dfdc44cf5baa961c79af07a99c30ce9ca100e996e6a5`；
- 父仓 closure commit `4c7f59a299d20358f653ecbd885927419df219f6`；
- 两块板均通过 probe-rs 3 MHz 完整 readback verify，分别耗时 91.92 s 与
  99.61 s，随后只使用 J-Link nRST 重跑同一镜像。

OSAL receive-wait 诊断矩阵同样绑定不可变产物：

- AP `ws63-examples` commit `9126643a7674f969e8e78668dce18adc8fee1423`，
  ELF SHA-256 `f5fffb13f7f227aee51894499378948ae79ca5c7069e5c9479fe19f809284908`；
- STA `ws63-examples` commit `2295978cb5ba43a5b3cbaae05778bfc91b899ae6`，
  ELF SHA-256 `8b3095bba31ec08f064089dc62e7a6920503134d3f91bfed34d3c6eb91dd3f8b`；
- facade diagnostic re-export 为 `hisi-rf` commit
  `1a401fafc3d2d1c9a1be9d8e29856394c3f3b49a`，父仓 closure commit 为
  `02a0ed36f`；
- AP/STA 分别以 3 MHz 完整 readback verify 烧录一次，耗时 91.27 s 与
  99.63 s；之后 20 轮只执行配对 J-Link nRST。

## 矩阵结果

| 矩阵 | 结果 | 认证超时 | 新增观测 |
|---|---:|---:|---|
| safe TX status | 18/20 | 0 | STA DMAC completion status histogram |
| AP TX status observation | 20/20 | 0 | AP vendor TX 与 bounded echo 计数 |
| ROM WLMAC TX counters | 20/20 | 0 | AP high/normal MPDU 与 TX-complete interrupt |
| STA PM-off A/B | 15/20 | 0 | 关闭 STA power save 未消除本地反例 |
| link lifecycle + IRQ45 | 16/20 | 0 | 3 次本地回包不足、1 次 scan timeout |
| OSAL receive-wait terminal snapshot | 18/20 | 0 | 两次部分回包；FRW wait/wakeup 与 task dispatch 持续前进 |

第一轮 run 6 与 run 11 的共同路径：

1. STA 已输出 `W2E_WPA3_CONNECT_OK pmf=required` 和 DHCP 成功；
2. STA 发送 10 次本地 UDP echo，相关 DMAC completion status 均为成功；
3. AP 收到全部 10 个 echo request，并把 10 个 reply 交给 vendor TX；
4. STA 没有收到任何 echo reply，上层最后 RX 仍是 DHCP；
5. STA 的 CCMP/TKIP replay、MIC、key-search failure 均为 0，RX queue 无 drop。

因此当前剩余风险位于 AP reply submission 之后、STA Rust-visible L2 RX 之前。
它不是 SAE、DHCP、应用 socket、STA request TX 或 AP bounded queue overflow。

后续使用 pbuf ownership 诊断镜像又复现了一种更靠下层的失败形态。该轮在
sequence 2 之后，STA 的 MAC accepted/filter、DMAC RX、HMAC RX、vendor RX、Rust L2 RX
与 IRQ45 dispatch 计数同时停止增长；后续 TX 仍全部成功完成，但 AP 没有观察到对应
echo request。最终快照中的 BSSID 寄存器已清零，随后出现 DHCP lease 丢失。这说明
`bssid_programmed=0` 更可能是收不到 beacon 后进入 link-loss 清理的结果，不能在没有
更早时间线证据时当作触发原因。该轮 14 次 TX 均由 vendor `drv_send` 同步取得自己的
pbuf reference，排除了调用方过早释放 pbuf 这一候选原因。

这两种形态必须分别保留：一种是 AP 已提交 reply、STA lower RX 不可见；另一种是 STA
整个 MAC/RX/IRQ 进度冻结、AP 收不到 request。当前新增的 IRQ45 lifecycle 快照会记录
enable/disable/clear/dispatch 计数及最终 enabled/pending 状态，用来在下一轮真机矩阵中
区分 vendor teardown、控制器 pending 卡住和 MAC source 不再产生中断。

link-lifecycle 矩阵中的 3 次本地失败均已完成 pure-WPA3、DHCP 和 direct ARP，STA
BSSID 仍 programmed，IRQ45 enabled 且无 pending；10 次 sequence-checked echo 中分别
只收到 4、2、4 次。该矩阵没有出现此前“BSSID 清零后继续使用旧 lease”的形态，说明
网络 runner 消费 `Disconnected` 并停止旧 IP 生命周期的修复是必要的，但不是 AP reply
可靠性的充分修复。另一次失败是两轮 scan 均 operation timeout，必须继续单独保留 scan
可靠性风险。

SoftAP 调度快照同时发现：应用主线程仍为 Cooperative 时，优先级 0 的 vendor task
曾处于 Ready 最长约 340 ms，应用主线程单次连续运行也达到约 341 ms。后续只把 adopted
SoftAP application thread 改为 5 ms `Preemptive`，不改变 vendor task profile；该 A/B 的
新 AP ELF SHA-256 为
`2f67af71c311caa444c3459cdce88a13f6b31294b651d66c6baaeeb65ec66ceb`，父仓 commit 为
`8eeb631b7c66acd0bd6727f06de983008c67e677`。后续 OSAL 诊断矩阵继续使用该
Preemptive application-thread 方向仍只有 18/20，因此 adopted main 的旧长运行窗口不是
充分根因。

## 2026-08-05 OSAL 与逐序列归因

run 6 收到 sequence `0,1,4,5`，AP 观察并提交 `0,1,2,4,5,6`；run 19 收到
`0,2,4`，AP 则观察并提交全部 `0..9`。两轮 STA 的 IRQ45 均保持 enabled、无 pending，
DMAC/HMAC/vendor/Rust RX 计数在探测期仍增长。AP 与 STA 的 `frw_task_thread` wait slot
也持续表现为 `blocks = wakeups + 1` 的正常阻塞终态，task dispatch 数在失败前后继续增长，
没有永久 pending、queue drop 或 worker 消失。

这把当前风险进一步拆成两类：run 6 中有 4 个 request 在 AP Rust echo 层不可见；run 19
中 AP 已提交全部 reply，但只有 3 个进入 STA Rust-visible RX。AP 每个 reply 后 5 ms
窗口内的 TX-complete 增量与收到的 sequence 有较强相关性，但该窗口不能证明缺少增量的
packet 永久未完成：异步 completion 可能落入下一 sequence 的窗口。下一轮必须给 AP
RX、reply submission、completion packet number 和 STA RX 使用同一 sequence/timestamp
时间线，才能区分 MAC completion 延迟、空口丢失和 STA lower-RX 丢失。

## 2026-08-05 DMAC 软件/硬件队列归因

第一轮正确配置的 pure-WPA3 DMAC 队列矩阵使用：

- AP ELF SHA-256
  `2cd1454a429e458ac051ec2cbd914c59985f5e0aa8c579a9f640023fba665537`，
  identity profile 为 `pure-wpa3-softap-dmac-queue-snapshot`；
- AP 以 probe-rs 3 MHz、完整 readback verify 烧录，耗时 91.81 s；
- 20 轮保持镜像不变，只执行配对 J-Link nRST。

该轮按旧的“至少 5/10 reply”分类得到 `15 pass + 5 local_data_path_failure`。其中 run 2
与 run 6 实际分别收到 `4/10` 和 `2/10`，已经证明 DHCP、ARP 和双向本地数据路径可达，
应作为 packet-loss observation，而不是本地路径完全失败；run 8、18、20 才是真实
`0/10`。这促使 HIL contract 把硬门槛收敛为“DHCP 成功、neighbor/ARP 成功且至少收到
一个 sequence-checked 本地 reply”，同时继续完整记录 attempts/replies/lost。该调整不把
`0/10` 降级为成功，也不以公网 ICMP 作为本地数据面门槛。

修正门槛后的提交态矩阵绑定以下 closure：

- 父仓 commit `6948d266b`；
- STA ELF SHA-256
  `ecd0e4c7d779f4a32c052285bf3b08eb5900b2631d3339d3f88ba097752de93d`，
  identity profile 为 `pure-wpa3-sta-pm-off-local-reachability-v2`；
- AP 继续使用上述 pure-WPA3 DMAC queue snapshot ELF；
- STA 以 probe-rs 3 MHz、完整 readback verify 烧录，耗时 99.52 s；随后 20 轮只执行
  配对 J-Link nRST。

最终结果为 `18 pass + 2 local_data_path_failure`，20 轮
`WLAN_AUTH_RSP2_TIMEOUT=0`。失败 run 4 与 run 7 都满足：

1. pure-WPA3 SAE、required PMF、association、DHCP 和 ARP 已完成；
2. STA 执行 10 次本地 echo，结果均为 `attempts=10 replies=0 lost=10`；
3. AP 已收到并生成 10 个 reply，FRW send 与 DMAC event 调用持续成功；
4. AP 软件队列 q0 终态为 `0x80000707`，仍持有 7 个 PPDU/MPDU；硬件 data queue
   为空，`data_tx_completion_total=0`；
5. 没有 auth response timeout、应用有界队列 drop 或 worker 永久 pending。

因此当前高置信边界是 AP DMAC 软件描述符队列到硬件调度队列之间，而不是 SAE、DHCP、
ARP、echo 应用层或 STA 发包。下一步应沿 dequeue eligibility、调度触发、queue ownership、
credit/flow-control 和 completion IRQ 继续诊断；不得用增加 echo 次数或继续放宽门槛代替
修复。

另有一轮误用 `wifi_softap` 默认 WPA2 feature、但 identity 文本写成 pure-WPA3 的 r13
实验。该轮配置与身份不一致，已整体排除，不计入任何 pure-WPA3 能力或可靠性统计。
本节原始证据保存在
`/private/tmp/ws63-tx-timeline-20260805-r14/` 与
`/private/tmp/ws63-tx-timeline-20260805-r15/`。

## 2026-08-05 scan 线性化修复与第二轮 TX completion 矩阵

新增 scan timing 诊断最初在 vendor callback 中读取 ROM 单调时钟，违反 callback
只发布有界状态的约束，并使 scan 路径停滞。时间读取移到普通 runner 后，真机又捕获到
一个独立 TOCTOU：vendor completion 已发布，但 callback 可能在 backend 最后一次只读
检查之后、timeout commit 之前到达，导致已完成 scan 被错误清理为 timeout。
`hisi-rf-ws63` commit `5daed542888b82fa1eaae2c3ad459db7a6a45bbe`
将 completion 与 timeout 改为对同一 `NATIVE_SCAN_ACTIVE` transaction 的原子认领，
并增加直接覆盖该窗口的 host 回归；106 项 host test 和完整 RV32 release link 通过。

修复后的不可变产物为：

- AP ELF SHA-256
  `ee232a5ec7065e3d606dacd982e84cfcd9dee3041620caf5a08756ac72bb1b9d`，
  identity profile `pure-wpa3-softap-data-completion-queue-v4`；
- STA ELF SHA-256
  `d742dffede81b45972f5165f9da5499f4f50254b0ad38597bff4598e4642a905`，
  identity profile `pure-wpa3-sta-atomic-scan-timeout-v6`；
- 父仓 closure commit `617bb5d1d`；
- AP/STA 均以 probe-rs 3 MHz 完整 readback verify 烧录一次，耗时分别为
  91.46 s 和 106.82 s；随后 20 轮只执行配对 J-Link nRST。

第二轮矩阵结果为 `18 capture_timeout + 2 local_data_path_failure`。这里的
`capture_timeout` 不是 scan、SAE 或 association timeout：20/20 均输出 `RF3_SCAN_OK`、
`W2E_WPA3_CONNECT_OK` 和 `RF5A_DHCP_OK`，且
`WLAN_AUTH_RSP2_TIMEOUT=0`。两种分类只表示固件是否在 120 s 采集窗内完成本地探针
总结；20 轮都没有达到 local-data-path pass gate。

跨 20 轮的 AP 终态一致：

1. AP 每轮向 vendor data TX 提交 3--5 个帧，`data_tx_submit_total` 与 `data_tx`
   一致；
2. 过滤后的 queue-0 completion 始终为
   `data_tx_completion_total=0`；该计数器当时错误地只接受 BE queue 0，不能代表全部
   单播数据 completion；
3. 每轮至少一个 hardware data queue snapshot 保持非空，19 轮为
   `0x80010101`，1 轮为 `0x80010202`；
4. `mac_tx_norm` 和 `mac_tx_irq` 均增长，证明 MAC/IRQ 并非整体停止；
5. 20 轮均无 scan failure、认证超时或 DHCP failure。

原厂 `hal_tx_queue_type_enum` 明确 BE/BK/VI/VO 分别为 queue 0--3，HI/MC 才是
queue 4/5。恢复预检中，三个 echo reply 提交后 raw completion 从 15 增到 21，
`mac_tx_norm` 从 2 增到 8，hardware snapshot 则落在 queue 3。因此第二轮矩阵能够证明
描述符进入 hardware data queue 且本地 echo 未闭合，但不能再据 queue-0-only 计数器断言
“硬件没有 data completion”。下一步必须先区分 queue 0--3 的真实 completion、STA RX 和
echo correlation，再归因 queue ownership/credit；不得把延长 capture timeout 当作修复。

后续 ROM callback table 只读核对又修正了一项诊断误差。当前 build profile 中
`FRD_ROM_TX_SCH` 的实际 ID 是 239，对应 callback `dmac_tx_schedule_cb`；ID 238 为空。
因此早先 `dmac_schedule_hook=0` 只是读取了错误 slot，不能作为“scheduler hook 未安装”
的证据。硅片 callback table 的 ID 239 为非零，且与原厂最终 ELF 中的 callback symbol
一致。

尝试用 linker `--wrap` 观测 `dmac_tx_need_schedule` / `dmac_tx_schedule` 时，AP 连续停在
`RFDBG_SOFTAP_INIT_BEGIN`。二分到仅保留一个 wrap、再完全移除 wrap 后发现：只有完整撤销
该诊断增量、恢复已验证的 RF/example 树内容，AP 才重新输出 `RFDBG_SOFTAP_READY`，STA
也重新完成 scan、pure-WPA3 association 和 DHCP。故这组侵入式诊断已由
`hisi-rf-ws63` commit `9e7b872` 和父仓 commit `3b11c7e83` 移除；它不进入发布路径。
后续调度归因必须采用不改变 ROM symbol resolution 和最终布局的只读手段，并继续绑定
ELF hash 与 artifact identity。

进一步把 `is_normal_data_queue()` 从只接受 queue 0 修正为接受 queue 0--3，即使没有新增
静态状态或 linker wrap，新 AP 仍停在 `RFDBG_SOFTAP_INIT_BEGIN`。真机反例由
`hisi-rf-ws63` commit `ea4cff0` 产生，随后以 commit `32ece62` 撤销；父仓对应为
`ba0829646` 与 `6b217662c`。已验证 r18 AP 镜像重新以 probe-rs 3 MHz 完整 verify 烧录，
耗时 99.46 s，恢复预检再次达到 AP ready、STA pure-WPA3 association 和 DHCP 基线。
这说明当前风险不只是 wrapper 改变 ROM 符号解析，而是最终 RF ELF 对普通 text layout
变化也敏感。在 normalized relocation/布局契约闭合前，目标端诊断不得通过增加或调整
默认镜像代码来“顺手修正”。

2026-08-05 又做了一次更窄的可证伪实验：新镜像只增加 8 个原子计数桶，在 DMAC
completion callback 中按 queue 编号自增；queue 1--3 不读取时钟、不匹配 submission，
也不读取 PAC queue snapshot。该 ELF 通过 plain-Cargo 检查，包含 37 项 ROM patch、零
vendor relocation，但真机仍在任何新增 completion callback 可能执行之前停于
`RFDBG_SOFTAP_INIT_BEGIN`。实验 ELF SHA-256 为
`092d0a8f2755c6877199f035668b60c7a320fda4fc136e560a552341374f3bfc`。这否定了
“只有 callback 内重诊断导致 init 卡住”的解释，并进一步支持当前 normalized RF ELF
存在布局敏感缺口。失败实验未提交；AP 随后恢复到 r18 已验证 ELF，并再次以 3 MHz
完整 readback verify 烧录，耗时 99.13 s。

同日使用恢复后的 r18 AP 与不变的 r17 STA 执行了另一轮 20 次配对 nRST。产物身份为：

- AP `85df680138aef81799d7c0e3baa7c79400bf6313557927bfe1cd383cc17d11b7`，
  profile `pure-wpa3-softap-correct-tx-scheduler-callback-v6`；
- STA `d742dffede81b45972f5165f9da5499f4f50254b0ad38597bff4598e4642a905`，
  profile `pure-wpa3-sta-atomic-scan-timeout-v6`。

结果为 `20 capture_timeout`，但分类不能解释为 scan、SAE、association 或 DHCP 失败：
20/20 AP ready，20/20 scan、pure-WPA3 association 与 DHCP 完成，DHCP marker 出现在
3.251--3.473 s，`WLAN_AUTH_RSP2_TIMEOUT=0`。A5B event/control queue 均无 drop，runner
errors 为 0，最大 runner step 为 95--98 ms。每轮发送两个本地 echo，汇总为 STA 发送
40、AP 观察 40、AP 提交 40、STA 收到 0；`submitted_without_sta_receive=40`。因此这轮
不是偶发复现，而是稳定证明问题位于 AP 已接收请求并向 vendor TX 提交回复之后、STA
Rust RX 看到回复之前。它仍未达到 connectivity pass gate，不能作为 WPA3 stable 证据。

随后对同一 r18 AP / r17 STA 产物执行了第二轮独立 20 次配对 nRST。AP 在测试前以
probe-rs 3 MHz 完整 readback verify 恢复，耗时 99.15 s；STA 镜像保持不变。artifact
identity 再次校验为 AP
`85df680138aef81799d7c0e3baa7c79400bf6313557927bfe1cd383cc17d11b7`、STA
`d742dffede81b45972f5165f9da5499f4f50254b0ad38597bff4598e4642a905`。

该轮仍为 `20 capture_timeout`：20/20 AP ready，20/20 scan、pure-WPA3 association
和 DHCP 成功，`WLAN_AUTH_RSP2_TIMEOUT=0`，无 panic，event/control queue drop 和
runner error 均为 0。STA 共发送 40 个 sequence-checked 本地 echo；AP 只观察并提交
其中 20 个，另 20 个在到达 AP echo 层前不可见；AP 已提交的 20 个 reply 在 STA 侧仍为
0 个可见。第二轮因此把风险拆成两个必须分别关闭的边界：STA 到 AP 的请求路径存在
`20/40` 丢失，AP 到 STA 的已提交回复路径为 `0/20`。原始证据位于
`/private/tmp/ws63-dual-board-20reset-round2-repeat-20260805/`；临时目录只作为本机原始
采集位置，本页记录可长期复核的产物身份和聚合结果。

### AP init 回归修复与提交态 20-reset 复验

后续逐指令差分发现，`32ece62` 并未完全恢复 r18 的已验证热路径：
`snapshot_dmac_tx_queues()` 仍在 enqueue/init 路径读取 ROM callback table。该读取使
最终 `.text` 相对 r18 增长 32 bytes，并在 vendor `dmac_tx_process_data_event` 路径稳定
触发 store-access trap。将 callback ID 239 的只读采样推迟到 TX completion 后，最终
布局恢复到 r18 的 section 边界；唯一预期指令差异是使用已核实的 callback ID 239，
而不是旧诊断中的 242。修复由 `hisi-rf-ws63` commit `261a132` 提交。

提交态 AP ELF profile 为
`pure-wpa3-softap-deferred-callback-sample-v7`，SHA-256 为
`c8eb3cc17f8d0d905ed46d90a5baaf35cece3665904f17034b0e841f996c5bbf`。
该镜像以 probe-rs 3 MHz、完整 readback verify 烧录，耗时 91.22 s；单轮 nRST
预检恢复 `RFDBG_SOFTAP_READY` 与 `RFDBG_SOFTAP_NET_READY`，没有 init trap。

随后保持 STA r17 镜像不变，对提交态 AP 执行 20 次 unchanged-image 配对 nRST：

- `20/20` AP ready、`RF1_IMAGE_OK`、`RF2_INIT_OK`、`RF3_SCAN_OK`、
  `W2E_WPA3_CONNECT_OK` 与 `RF5A_DHCP_OK`；
- `WLAN_AUTH_RSP2_TIMEOUT=0`，两侧均无 panic；event queue drop 与 runner error
  均为 0，最大 runner step 为 95--99 ms；
- 顶层分类为 `19 capture_timeout + 1 local_data_path_failure`，因此 connectivity
  gate 仍未通过；
- 19 个带 echo marker 的样本中，STA 发送 38，AP 观察并提交 38，STA 接收 0；
  `sent_missing_at_ap=0`，`submitted_without_sta_receive=38`。

这轮关闭的是 AP init 回归，不是本地数据面问题。相较前一轮 `20/40` 请求在 AP 前
不可见，本轮请求方向为 `38/38`，剩余失败更集中在 AP 已提交 reply 到 STA Rust RX
之间。原始证据位于
`/private/tmp/ws63-dual-board-20reset-commit-fix-matrix-20260805/`。

### 全 WMM data completion 分类与提交态 20-reset

受控 queue histogram 证明，同一轮 normal-TX completion 分布为 queue 0 两项、
queue 3 八项，management queue 4 另有 32 项；因此旧
`is_normal_data_queue()` 只接受 queue 0 会漏掉 VO queue 3。直接放宽旧条件又会同时在
queue 1--3 completion 后执行 vendor-private queue/VAP snapshot，并连续三轮破坏
association。跳过该 snapshot 后 association 恢复，说明 timeline 分类与私有布局读取
必须解耦。

`hisi-rf-ws63` commit `0e679e00721251f2577cacd957a10226c9d4e292` 因此完成两项窄修复：

1. completion timeline 接受 BE/BK/VI/VO queue 0--3，继续排除 HI/MC queue 4/5；
2. 删除 completion handler 返回后的 vendor-private queue snapshot，保留 data-event
   enqueue 阶段既有的只读 snapshot。

34 项 host test、`clippy -D warnings` 与普通 Cargo RV32 release link 均通过。源码构建
AP 以 probe-rs 3 MHz、完整 readback verify 烧录成功，耗时 91.10 s，并恢复
`RFDBG_SOFTAP_READY`。单轮配对预检已捕获 queue 3 与 queue 0 completion，且没有再次
破坏 pure-WPA3 association。

父仓 closure commit 为 `6cca7d227`。不可变产物为：

- AP ELF SHA-256
  `6f9c7b0a3ae32e8d617a136cbbff0b060dfebe5ff23f8ad5688b1e2f8c7cd9f7`，
  profile `pure-wpa3-softap-unicast-completion-v12`；
- STA ELF SHA-256
  `d742dffede81b45972f5165f9da5499f4f50254b0ad38597bff4598e4642a905`，
  profile `pure-wpa3-sta-atomic-scan-timeout-v6`。

保持两侧镜像不变执行 20 次配对 nRST，结果为：

- `20/20` AP ready、scan、pure-WPA3 association 和 DHCP；
- `auth_rsp2_timeouts=0`，A5B event/control queue drop 与 runner error 为 0；
- 顶层分类为 `20 capture_timeout`，没有一轮达到 local-data-path success marker；
- `20/20` AP 均收到并提交两个 sequence-checked echo reply；
- `20/20` AP 均记录 `data_tx_submit_total=5`、`data_tx_completion_total=10`；
- 每轮 bounded completion timeline 均恰好包含 queue 3 八项和 queue 0 两项；
- STA 对已提交 reply 仍为 0 个 Rust-visible receive。

该矩阵否定了“AP hardware data completion 缺失”作为当前根因。现有证据只允许把问题
放在 completion 之后、STA Rust-visible L2 RX 之前；下一步须关联空口 TX、STA MAC/DMAC
RX/filter、CCMP/key search 与 lower-RX callback，不能继续围绕 queue credit 或增加
capture timeout。原始证据位于
`/private/tmp/ws63-dual-board-20reset-unicast-completion-commit-20260805/`。

### STA PM profile 与 smoltcp burst 容量 A/B

后续证据推翻了上一节“已越过 AP completion”的当前归因。此前所有 `20 capture_timeout`
矩阵均复用 STA r17 ELF；该 profile 在 association 前调用
`uapi_wifi_set_pm_switch(false)`。原厂实现只用无响应 WAL 消息投递配置，返回 0 不证明
HMAC/DMAC 已应用。旧 STA ELF 在当前 AP 上先达到 `3/3`；当前源码保留
`data-path-diagnostics`、只移除 `diagnostic-disable-sta-pm` 后也达到 `3/3`，随后 20-reset
为 `19 pass + 1 local_data_path_failure`。不过该 feature 同时改变最终布局：在失败 ELF
原地址用等长指令旁路 UAPI 调用、保留 feature 和所有地址后仍为 `0/3 capture_timeout`。
因此当前证据只足以将这个 PM 诊断 profile 移出默认 HIL，不能把 `0/20` 单独归因于运行时
PM 调用。

无 PM profile 的前三轮又给出独立的应用层事实：STA L2 每轮收到 12 个 IPv4 packet，
其中 3 个是 DHCP、9 个是 AP echo reply，但应用仅收到 `7/6/6`。原因是 STA 复用的
smoltcp UDP socket 只有一个 RX metadata 槽；`Interface::poll()` 在应用 `recv` 前连续处理
burst，后续 datagram 因 packet buffer full 被丢弃。SoftAP echo socket 同样只有两个槽。
ws63-examples commit `ca5c978698ea1d721e3ee5df8b61afd1f360a9dd` 将 STA metadata
容量绑定到十次 bounded local probe，并把 AP echo metadata 扩为 16；父仓 closure 为
`ef96628bf`。由于 WS63 SRAM 接近链接边界，STA DNS payload storage同时收窄为 128 bytes，
没有扩大 shared arena 或越过固定 stacks。

提交前 3-reset 的不可变产物为：

- AP ELF SHA-256
  `0b9b2e563cd4a078bdce6ce7f278682cc0a1617beac0a66cfcf3371af92044b8`，
  profile `pure-wpa3-softap-udp-rx-capacity-v15`；
- STA ELF SHA-256
  `8cf50198770ed016642afe233e43f3aa277d7d2a871ff75060466236577ef217`，
  profile `pure-wpa3-sta-no-pm-udp-rx-capacity-v15`。

两侧均以 probe-rs 3 MHz 完整 readback verify 烧录。3-reset 为 `3/3 pass`，STA
`30 sent / 30 received`，每轮 L2 `rx_ipv4=13`（3 DHCP + 10 echo）；AP 最终计数每轮
`echo_rx=10 echo_tx=10`。summary 中 AP per-echo marker 少计三项是 UART 行交错造成的采集
下计；AP 最终计数与 STA sequence 去重结果一致。

同一产物不重烧执行提交态 20-reset，结果为 `18 pass + 2 local_data_path_failure`，
`auth_rsp2_timeouts=0`。STA 共发送 200、收到 177；17 轮为 `10/10`，一轮为 `7/10`，
run 4 与 run 10 为 `0/10`。这两个反例不是 socket overflow：失败 AP 的 queue-0 software
queue 非空、hardware queue 空闲，completion timeline 只有 queue-3 项而没有 queue-0；
成功轮则明确出现十个 `0x180...` queue-0 completion 并清空 software queue。一次在
DMAC event callback 返回后执行 `need_schedule -> schedule` 的低侵入实验为 `0/3`，且
`dmac_schedule_hook=0`，说明检查时 queue 0 尚为空；该未提交实验同时再次触发 text-layout
敏感性，已经撤销并恢复上述提交态 AP 镜像。原始证据分别位于
`/private/tmp/ws63-udp-rx-capacity-3reset-20260805/`、
`/private/tmp/ws63-udp-rx-capacity-20reset-commit-20260805/` 和
`/private/tmp/ws63-q0-reschedule-ab-3reset-20260805/`。

### RTOS switch ownership 收窄与 20-reset closure

旧失败轮的 SoftAP RTOS snapshot 显示 priority-0 timer worker 已成为 `Ready`，但
dispatch count 停止增长，同时低优先级 adopted main 和全局 switch count 仍持续增长。
这使调查从 DMAC timer 本身上移到 RTOS ready ownership。代码审计确认两个独立边界：

1. block、sleep、mutex/semaphore wait 与 task exit 过去先把 source 标为 non-running，
   再在后续 scheduler 临界区 detach target 并提交 switch ticket。当前实现把这三个动作
   合并为一个明确的线性化点；这是契约与形式化模型边界的收窄。
2. `set_effective_priority` 和 `set_run_policy` 曾把所有 `State::Ready` 都当作 ready-queue
   成员。pending switch 合法拥有的 detached target 同样是 `Ready`；旧代码会把它重新
   入队，使 ready queue 与 pending ticket 双重拥有同一 task。修正后只有实际
   `ready_contains(task)` 的 task 才会 remove/reinsert，并有优先级、policy 两条回归。

`hisi-rtos` commit `fac6dd4` 包含上述修复与 `ready_queued` / `pending_switch_target`
诊断；`ws63-examples` commit `dec215e` 将诊断输出接入 SoftAP。host test 为 `81/81`，
UI compile-fail test、host clippy `-D warnings` 和 RV32 check 均通过。

最终不可变产物为：

- AP ELF SHA-256
  `0cc232ce341906da617735c363f03189e25657ffeb6305e5800b4244ba459618`，
  profile `pure-wpa3-softap-rtos-ownership-v18`；
- STA ELF SHA-256
  `b30d5176705096165817a96d9ca82a90b3c618459aeae64345bca59d1ce66587`，
  profile `pure-wpa3-sta-rtos-ownership-v18`。

两侧均通过 probe-rs 3 MHz 完整 readback verify；AP 下载约 90 s，STA 约 106 s。
同一镜像先完成 `3/3` 预检，再执行 20 次配对 nRST，不重复烧录：

- `20/20` 完整 connectivity contract，`auth_rsp2_timeouts=0`；
- STA sequence-checked echo 为 `200 sent / 200 received`，每轮 `10/10`；
- AP 最终 echo counter 每轮达到 `10/10`（个别逐包 UART marker 因并发行交错下计，
  不作为无线丢包计数）；
- timer worker 在采样中保持 sleeping/running 循环且 dispatch count 持续增长；没有再次
  出现 `Ready` 冻结、panic、永久 pending 或本地数据面失败。

原始脱敏证据位于
`/private/tmp/ws63-rtos-ownership-v18-3reset-20260805/` 和
`/private/tmp/ws63-rtos-ownership-v18-20reset-20260805/`。这组证据证明当前补丁组合关闭了
现有复现门槛，但不能唯一证明旧 run 4/run 10 由上述任一单点触发：旧两段临界区已有
resume check，而 detached priority/policy mutation 的生产 call graph 触发频率仍需单独
证明。后续形式化工作必须将 ticket creation decision 与 ticket lifetime 分开建模，并
增加“非 idle Ready task 的 owner 恰为 ready queue xor pending target”的 invariant；
不得把这次 20/20 改写成未经证实的单一根因叙述。

变量比较必须选择正确基线。相对直接前驱 `ef96628bf` 的
`18 pass + 2 local_data_path_failure` 固定镜像，STA PM override 已经关闭，十槽 UDP
metadata 已经生效，AP completion 后的 vendor-private snapshot 也早已移除；v18 的生产
逻辑变化只有 `hisi-rtos fac6dd4`，另有 `dec215e` 增加只读 ownership marker。因此这轮
支持“RTOS 变更与反例消失相关”，但 `fac6dd4` 内仍同时包含 atomic switch-away 契约收窄
和 detached priority/policy ownership 修复，尚不能在二者之间唯一归因。相对更早的
`20/20 capture_timeout` 固定 ELF，v18 还跨越 PM profile、UDP capacity 和多轮 RF 诊断
清理，是多变量对照，不能用于声称 RTOS 单点因果。

### Detached mutation 因果探针

为验证旧实现的 detached target 缺陷是否在当前 upstream-native SoftAP 负载中真实触发，
`hisi-rtos` 在 priority/policy mutation 入口增加两个只读饱和计数器。计数条件严格限定为：
task 为 `Ready`、不在 ready queue，且恰为当前 pending switch target。host 回归分别构造
priority inheritance 与 policy mutation 路径，并确认计数和 detached ownership 同时保持。

诊断 AP ELF SHA-256 为
`5cb1a67f72efd0b8875dc5e751ae044868e838d3ec86a2e744bd49ea67d79fc7`，profile 为
`pure-wpa3-softap-detached-mutation-diag-v19`；STA 继续使用上述 v18 ELF 和 profile，避免
重烧或修改 STA。先执行 3-reset，再用同一产物执行 20-reset：

- 两组分别为 `3/3` 与 `20/20`，`auth_rsp2_timeouts=0`；
- 20-reset 的 STA sequence-checked echo 为 `200 sent / 200 received`，每轮 `10/10`；
- 所有 AP scheduler snapshot 中 `detached_prio_mut=0`、`detached_policy_mut=0`；
- 没有 panic、永久 pending 或本地数据面失败。

原始证据位于
`/private/tmp/ws63-rtos-detached-mutation-diag-3reset-20260805/` 和
`/private/tmp/ws63-rtos-detached-mutation-diag-20reset-20260805/`。这项负证据说明：修复的
ownership 缺陷真实且可由 host test 构造，但该 20-reset 工作负载没有观测到其生产触发
条件。因而 v18/v19 的稳定结果不能唯一归因于 priority inheritance 或 policy mutation；
若历史形态再次出现，应扩展 ready bucket、membership multiplicity 与有界链诊断，而不是
把零计数解释成“缺陷不存在”。

## 剩余因果与发布门槛

下一诊断必须继续保留两板 sequence/timestamp、IRQ45 lifecycle 和 OSAL wait 终态，
但先聚焦 AP queue-0 enqueue、`dmac_tx_need_schedule`、`dmac_tx_schedule` 与
completion-reschedule 的真实线性化点。最新失败轮已经重新证明 queue-0 completion 缺失，
不能沿用早先“所有 completion 稳定存在”的结论，也不应先扩大 STA RX 日志。
由于仅增加 event-return 诊断代码就能改变行为，观测方式必须保持最终布局，或先关闭
normalized ELF 的布局契约。
当前首要可证伪假设是 delayed reschedule timer 的 liveness：原厂 q0 dequeue/聚合路径通过
`frw_dmac_timer_create_timer` 创建延迟调度，运行时链路为 OSAL base timer -> FRW message
55 -> DMAC timeout list -> `dmac_tx_sched_timer_handler` -> `dmac_tx_schedule`。此前
event-return 检查时 q0 尚为空，因此 `dmac_schedule_hook=0` 不能否证这条延迟链。
下一次复现 stalled 终态时，应先用 debugger 只读检查 `0x181434` 起的 base timer 与
`0x181450` timeout-list head，并只跟随有限节点，确认是否存在指向 q0 且已 overdue 的
timer。只有得到该证据后，才受控触发一次现有 timeout/message 55 链或
`dmac_tx_schedule(device, 0)`，观察 software queue、completion 与 STA reply 是否恢复。
这些地址来自当前 pinned ROM/SDK oracle，不是跨 archive 的稳定 ABI；使用前必须再次绑定
ELF/archive hash。

对本轮 AP ELF `/private/tmp/ws63-udp-rx-capacity-20260805/ap.elf`，最小只读取证集合为
`g_tx_sched_timer` 36 bytes、`g_timer` 16 bytes、对应 Rust timer slot 24 bytes、DMAC
timer-list head 8 bytes，以及 `g_dmac_frw_stat` 56 bytes。下列符号地址仅是该 ELF 的
已知值，抓取前必须用该次 ELF 的 `nm` 重新确认：`g_tx_sched_timer=0x00180f18`、
`g_timer=0x00181434`、list head `0x00181450`、`g_dmac_frw_stat=0x00180fd0`、Rust
`timer::TIMERS=0x00a15b78`（32 个 24-byte slot）、timer-worker started flag
`0x00a152a2`。`g_timer + 0` 是 `(slot_index + 1)`，slot 布局为 `used/active/periodic`
三个 byte、`timer_ptr +4`、`deadline u64 +8`、`interval u64 +16`。

`g_tx_sched_timer` 按 36-byte `frw_timeout_stru` 解码：argument `+0`、callback `+4`
（本 ELF 预期 `0x14cab8`）、deadline `+8`、timeout `+12`、registered/periodic/enabled
`+16..+18`、callback-private `+24`、list next/prev `+28/+32`。argument 非零时再只读
`argument + 78` 的 queue（预期 0）与 `+80` 的 retry interval。stall 保持数秒后的判定为：

| 终态 | 判定 | 下一步 |
|---|---|---|
| list 非空，base slot inactive | timer 已触发，但 message 55 未被 FRW 处理或重臂 | 查 callback 投递与 FRW wake |
| list 非空，slot active，deadline 已过 | Rust timer worker 或时间比较未执行 | 查 worker started、wake 与 deadline poll |
| list 非空，slot active，deadline 异常远 | 重臂 deadline 可能被 stale overwrite | 查 arm/rearm 线性化 |
| list 为空，q0 仍 stranded | 该 frame 没有建立 deferred timer | 查 completion-reschedule ownership；此时单次 `dmac_tx_schedule(device, 0)` 才是有效 A/B |
| timer object callback/queue 匹配且已 overdue | q0 delayed retry liveness 基本成立 | 沿 message 55 和 timer worker 闭环根因 |

读取 list 时最多跟随少量节点；`frw_timeout_stru` 的 list entry 位于 object `+28`，不得
对未知指针做无界遍历。成功与失败终态各抓一份相同集合，先做强差分，再决定是否执行
一次受控调度调用。
不能再用 5 ms 的即时 delta 代替异步 completion 归属，
也不能靠增加 echo 次数或观察时间掩盖 `0/10` 反例。配对复位仍是发布 gate；可另跑 AP
常驻、只复位 STA 的差分矩阵，用于识别 AP 启动时序或残留状态，但不能替代发布 gate。

最终验收必须使用提交态同一镜像、写入 artifact identity，并至少完成 20 次
unchanged-image nRST：无本地数据面失败、无永久 pending、无 queue drop、无
scheduler invariant failure。达到该门槛前，不删除 migration oracle、不切唯一默认
backend，也不宣称 WPA3 stable。
