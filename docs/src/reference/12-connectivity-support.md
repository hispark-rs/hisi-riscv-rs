# Connectivity 支持矩阵与资源边界

本页汇总当前 `hisi-rf` alpha release train 的**可构建能力、真机证据和资源契约**。
它不是稳定性承诺：U8 评审结论仍为 no-go，所有连接性 API 仍可能在后续 alpha 中调整。

## 当前发布闭包

| 组件 | 版本或修订 |
|---|---|
| 用户 facade | `hisi-rf 0.1.0-alpha.114` |
| WS63 backend | `hisi-rf-ws63 0.1.0-alpha.100` |
| target archive 包 | `ws63-radio-blob 0.1.0-alpha.25` |
| Wi-Fi 资源报告 | `hisi-rf-resource-report/v13` / `ws63-radio-2026-09-01-r13` |
| BLE/SLE 资源报告 | `hisi-rf-radio-resource-report/v1` |

当前只有 WS63 backend。`bs20`、`bs21` 和其他芯片没有可选择的 `hisi-rf` 实现，
因此不能从“HAL 支持该芯片”推导出“RF 支持该芯片”。

## Profile 矩阵

| Cargo profile | 能力边界 | Host 构建 | WS63 HIL | 当前等级 |
|---|---|---|---|---|
| `profile-wifi-wpa2-smoltcp` | STA、WPA2-Personal、Ethernet L2、smoltcp | macOS/Linux/Windows | connect、DHCP、ARP、DNS、lease renew、本地 UDP | alpha |
| `profile-wifi-wpa3-smoltcp` | STA、WPA3-SAE+PMF、Ethernet L2、smoltcp | macOS/Linux/Windows | pure WPA3 和 transition，含重复复位 | alpha |
| `profile-wifi-wpa2-softap` | WPA2-Personal SoftAP、DHCP、本地 UDP echo | macOS/Linux/Windows | AP/STA 配对 20-reset | alpha |
| `profile-wifi-wpa3-softap` | WPA3-Personal SoftAP | macOS/Linux/Windows | pure WPA3 AP/STA 配对 | alpha |
| `profile-ble-peripheral` | advertising、连接、GATT server、安全生命周期 | macOS/Linux/Windows | 双板 peripheral/central | alpha |
| `profile-ble-central` | scan、连接、GATT client、安全生命周期 | macOS/Linux/Windows | 双板 peripheral/central | alpha |
| `profile-ble-dual-role` | BLE 双角色静态资源组合 | macOS/Linux/Windows | facade/资源与迁移 fixture | alpha |
| `profile-sle-announce` | SLE announce | macOS/Linux/Windows | 双板 announce/seek | alpha |
| `profile-sle-seek` | SLE seek | macOS/Linux/Windows | 双板 announce/seek | alpha |
| `profile-sle-ssap` | connect、SSAP discover/read/notification | macOS/Linux/Windows | 双板 S3 20-reset | alpha |

`profile-wifi-wpa2-softap-ble-coexistence` 和
`profile-wifi-wpa2-softap-sle-coexistence` 是 doc-hidden maintainer HIL profile，
用于证明共享初始化和并发流量；它们尚不是普通应用的公开组合入口。

Host 构建表示 crates.io-only consumer 可以在对应桌面系统完成普通 Cargo 构建，
不表示这些桌面系统都连接了真机烧录 rig。真机证据当前来自 macOS 上的两块 WS63 EVB。

## 资源预算

资源数字由 facade 的 `resource_report()` / `Storage::report()` 生成；应用不应另建一套
常量。Wi-Fi 固件还会在 UART 输出 `RFDBG_RESOURCE`，HIL 将它与实际 heap/arena 状态核对。

| Profile 族 | RF arena | Runtime arena | 动态任务 | 任务栈总量 | 最小任务栈 | 事件容量 |
|---|---:|---:|---:|---:|---:|---:|
| Wi-Fi WPA2/WPA3 STA | 101,824 B | 197,120 B | 8（7 vendor + 1 worker） | 180,224 B | 8,192 B | 8 public |
| BLE peripheral/central/dual-role | 303,104 B | 由应用 RTOS storage 提供 | 4 | 10,240 B | 512 B | 32 backend / 8 public |
| SLE announce/seek/SSAP | 303,104 B | 由应用 RTOS storage 提供 | 4 | 10,240 B | 512 B | 32 backend / 8 public |

Wi-Fi 另外需要 49,152 B `.wifi_pkt_ram` 和至少 32,768 B main stack。当前 Wi-Fi
报告仍是 `runtime_resources_calibrated=false`：固定 HIL 已证明显式 arena、零分配失败和
可观测 headroom，但这些数字尚未毕业为跨 workload 的稳定容量承诺。

SoftAP 暂无与 STA v13 等价的公开机器资源报告。当前只承诺固定 HIL composition 的
链接器 guard、启动 admission 和零分配失败；统一报告是 stable graduation 前的已知缺口。

U8R-E3c 固定闭包中，`hisi-fwpkg plan` 得到的完整 flash image 为：

| 角色 | ELF SHA-256 | image 长度 |
|---|---|---:|
| WPA2 STA | `7fefd18bc0c2d59eb3592e41c8ff81c50c62d67e05953ef1d50bf2d85e032c3d` | 774,024 B |
| WPA2 SoftAP | `f1ccf42b169df42cc996e5af3a19dc78860d5a051267ebaddaa681e583728c38` | 675,672 B |

这些是该固定 source closure 的测量值，不是未来版本的 flash 上限。

## Blob 与 ROM 身份

target archive 的逐文件输入/输出 hash、normalization revision、hostap source tag/commit
和构建器身份以 `ws63-radio-blob/artifacts/manifest.json` 为事实源。当前关键身份为：

- base profile：`ws63-wifi-base-v1`；
- upstream hostap：`hostap_2_11`，commit
  `f735d907a3fdc423747db1e7fa4aaf61676baa82`，release archive SHA-256
  `912ea06f74e30a8e36fbb68064d6cdff218d8d591db0fc5d75dee6c81ac7fc0a`；
- BLE archive ABI：`ws63-ble-b0-archive-abi-v2`；
- SLE archive ABI：`ws63-sle-s0-archive-abi-v1`。

ROM 符号、callback 和 patch metadata 的输入 hash 由
`hisi-rom-sys-ws63/assets/ws63/manifest.txt` 固定：

- ROM linker facts：`fa8f0071c07374d443db10dd2d5569a48e1e07486538b2eae701252e2188edbc`；
- callback facts：`66934b3acfac013106f19ae40753eeb561cfbe21bc5e347b50c3b52e409ea92e`；
- Wi-Fi patch facts：`993e3a3007c0a660eae7f006172cfdee32ce5c1c5fedafdc780c1005487f8d22`。

## 已知边界

- `hisi-rf` 仍是 alpha；[U8 stable graduation review](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/plan/evidence/hisi-rf-u8-stable-graduation-review-2026-09-01.md) 明确维持 no-go。
- Wi-Fi 的正式 IP adapter 当前是 smoltcp；Embassy wait 不等于 Embassy Net driver。
- BLE/SLE 使用 vendor host；不能把它描述为通用 BLE HCI 或标准 SLE DLI backend。
- coexistence profile 仍是 maintainer fixture；公开 `coex` 需要单独产品/API 评审。
- `ws63-rf-rs` legacy facade 要保留到至少一个迁移 release，并且不能早于父仓 `v0.8.0` 退役。
- HIL 的事件/队列守恒只证明内部路径；不能证明 RF 环境或外部网络永不丢包。

## 证据入口

- [U8R-E3c opaque facade fixed-image HIL](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/plan/evidence/hisi-rf-u8r-e3c-facade-hil-2026-09-03.md)
- [WPA2/WPA3 release closure](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/plan/evidence/ws63-rf-release-closure-wpa2-wpa3-2026-08-06.md)
- [BLE/SLE U7 integration acceptance](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/plan/evidence/hisi-rf-u8-stable-graduation-review-2026-09-01.md)
- [RF 错误与诊断契约](11-rf-diagnostics.md)

若文档与构建输出冲突，以当前 crate metadata、机器资源报告、artifact manifest 和最新
CI/HIL evidence 为准，并请通过仓库 issue 报告漂移。
