# WS63 A5B Rust SoftAP 本地连接证据（2026-08-03）

## 范围

本证据使用两块独立 WS63：一块运行仓库内 `wifi_softap`，另一块运行公开
`wifi_connectivity` facade。测试网络由仓库内双板 HIL fixture 提供，不读取本机
credential env 文件，也不依赖外部 AP。

该 fixture 是隔离的 `192.168.4.0/24` 网络：AP 为 `192.168.4.1`，向 STA 租约
`192.168.4.2`，提供 UDP port 9 echo，并有意不下发默认路由。因此本轮验证本地
L2/L3/L4 数据面，不宣称公网或 pure-WPA3 能力。

## 构建与烧录契约

- AP：`wifi_softap` release build。
- STA：`wifi_connectivity` release build，features 为
  `wpa2,dual-board-hil`。
- 两块板均使用 probe-rs 3 MHz 和完整 verify 烧录；本轮约为 AP 82 秒、STA 102 秒。
- probe selector、J-Link serial 和 UART 按板卡角色显式绑定，不依赖 USB 枚举顺序。

## 真机结果

稳定测试顺序是先启动并保持 AP 常驻，再复位 STA。STA 完成：

- `W2D_WPA2_CONNECT_OK`；
- DHCP 获得 `192.168.4.2/24`，无默认路由；
- 直接 ARP reply；
- sequence-checked UDP echo，在第三次有界尝试内成功；
- 本地数据面 gate 通过，RX queue drop 为 0；
- DHCP lease renewal 通过；
- 公网 DNS 按 `reason=no-default-route` 跳过。

对应 AP 侧完成完整 upstream authenticator 与数据面闭环：4 个 EAPOL RX/feed、
8 个 EAPOL TX、4 次 key install，DHCP offer/ack 与 UDP echo RX/TX 均非零；最终
echo 帧目的地址为已关联 STA，IPv4 目的地址为 `192.168.4.2`，UDP 源端口为 9，
vendor TX failure 为 0。

## 复位语义

同时复位 AP 与 STA 会把 AP 启动、关联安全状态恢复和 STA 连接重试混在同一时间窗，
首轮曾出现 AP 已发送 echo 而 STA 未收到。保持 AP 常驻后仅复位 STA，同一镜像完整
通过。因此双板回归的稳定契约应为：

1. AP 启动一次并等待 `RFDBG_SOFTAP_NET_READY`；
2. STA 烧录一次；
3. 重复 STA nRST 和 marker capture，AP 不随每轮复位；
4. 只有 AP 自身健康检查失败时才重启 AP，并重新开始统计窗口。

当前证据是单轮完整 vertical slice，不替代后续 STA 多次 reset 统计矩阵。它关闭了
“Rust SoftAP 是否能作为受控 DHCP/ARP/UDP echo 对端”的能力问题，但不关闭
pure-WPA3、路由公网 DNS、SoftAP 多客户端或长期压力门槛。
