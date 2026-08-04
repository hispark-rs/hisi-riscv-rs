# WS63 双板 pure-WPA3 可靠性证据（2026-08-04）

## 结论

仓库内 Rust WPA3 SoftAP 与 upstream-native WPA3 STA 已完成真实硅片
SAE-only 能力证明，但发布可靠性门槛仍未关闭。三个 unchanged-image 20-reset
矩阵累计得到 `58 pass + 2 local_data_path_failure`；60 轮均完成 pure-WPA3
scan、SAE、required PMF、association 和 DHCP，`WLAN_AUTH_RSP2_TIMEOUT=0`。

后两轮各为 20/20，只说明后续观测窗口未复现，不能覆盖第一轮的两个反例。

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

## 矩阵结果

| 矩阵 | 结果 | 认证超时 | 新增观测 |
|---|---:|---:|---|
| safe TX status | 18/20 | 0 | STA DMAC completion status histogram |
| AP TX status observation | 20/20 | 0 | AP vendor TX 与 bounded echo 计数 |
| ROM WLMAC TX counters | 20/20 | 0 | AP high/normal MPDU 与 TX-complete interrupt |

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

## 未关闭门槛

下一诊断镜像须把 AP echo reply 区间与 ROM WLMAC normal-MPDU/TX-complete
增量逐轮关联，并同时保留 STA/AP IRQ45 lifecycle。若失败轮的 ROM 计数未覆盖 10 个
reply，继续定位 AP vendor enqueue 到 MAC；若计数覆盖，则转向空口 ACK、STA lower
RX/filter/IRQ。若 STA MAC/RX/IRQ 计数整体冻结，则先用 enabled/pending 与 lifecycle
调用计数裁决控制器状态，再检查 MAC source 与 link-loss 时间线。
STA 同时按每次 echo 窗口记录 MAC、DMAC、HMAC、vendor RX 与 Rust L2 的累计快照，
由 matrix 计算相邻窗口增量。配对复位仍是发布 gate；可另跑 AP 常驻、只复位 STA 的
差分矩阵，用于识别 AP 启动时序或残留状态，但不能替代发布 gate。

最终验收必须使用提交态同一镜像、写入 artifact identity，并至少完成 20 次
unchanged-image nRST：无本地数据面失败、无永久 pending、无 queue drop、无
scheduler invariant failure。达到该门槛前，不删除 migration oracle、不切唯一默认
backend，也不宣称 WPA3 stable。
