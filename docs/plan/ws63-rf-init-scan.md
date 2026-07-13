# WS63 RF 推进计划：Init → Scan 优先

## Summary

WS63 RF 的第一阶段目标是 **真实 WS63 硅片上的 Wi-Fi init + scan**；该目标现已
通过 HIL。RF5 的 TX/RX、open-AP connect、WPA2-Personal oracle 和 ping 也已在
2026-07-12 完成；WPA2-only 裁剪和 crypto closure 也已完成，当前工作转入
[Connectivity 全栈重构计划](hisi-connectivity-stack.md) 拆分独立组件。

RF 作为独立 connectivity track 推进；HAL 能力优先，但新补的通用硬件能力默认先进
`unstable`。RF 专用 glue、blob ABI、NV item id、porting contract 保持在 `ws63-rf-rs`。

## Key Milestones

### RF0 -- Baseline And Truth Sources

- 固定事实源：vendor 行为以 `fbb_ws63` 为准；blob delivery 以
  `crates/chips/ws63/ws63-radio-sys/ws63-RF` 为准；archive/ABI/post-link 以
  `ws63-radio-sys`/`hisi-rf-link` 为边界，Rust porting layer 暂由 `ws63-rf-rs` 实现。
- 更新 RF link residual 工具到官方 nightly 工具链路径，不再引用旧 `+hisi-riscv`。
- 建立 RF bring-up 日志规范：固定 UART marker
  `RF0_LINK_OK`、`RF1_IMAGE_OK`、`RF2_INIT_BEGIN`、`RF2_INIT_OK/ERR:<code>`、
  `RF3_SCAN_RESULT/ERR:<code>`。

### RF1 -- Real RF Runtime Image

- 在 `hisi-riscv-rt` WS63 linker 中提供真实 `.wifi_pkt_ram (NOLOAD)`：
  `0x00A00000..0x00A0C000`。
- `__wifi_pkt_ram_begin__` / `__wifi_pkt_ram_end__` 由 linker section 定义，不再由
  RF examples `--defsym` 定义。
- 新增最小真机 example：`wifi_init_smoke`。它依赖 `ws63-rf-rs`，链接完整 Wi-Fi init
  需要的 blob set、ROM symbol table 和 runtime porting layer。
- 当前实现状态：`wifi_init_smoke --features full-init` 已可通过
  `tools/rf-build-full-init-lld-layout-patch.sh` 生成完整 ELF。脚本先让 stock
  `rust-lld` 决定最终 section layout，再按该 layout patch vendor custom relocation，
  最后重新链接并逐 section 校验；布局漂移或 unresolved relocation 会 fail closed。
- 原厂 linker/map 仍是诊断 oracle，但最终镜像不依赖原厂 linker。构建脚本为
  patch manifest 和最终 RF section layout 做 fail-closed 校验；具体计数随 blob
  closure 和诊断 feature 变化，不作为外部契约。
- 当前 evidence：`hisi-fwpkg plan --chip ws63 --image-output` 已为完整
  `wifi_init_smoke --features full-init` 产出从 `0x230000` 开始的 app image；
  `.wifi_pkt_ram` 是 `SHT_NOBITS`，未进入 flash body。
- 保持 `rf_port_demo` / `netif_smoltcp_selftest` 作为 QEMU/host selftest，不把它们称为
  真实 Wi-Fi。

### RF2 -- HAL-Backed Platform Services

- HAL 提供通用硬件能力，RF 保持组合层：
  - eFuse read buffer / bit read：用于 MAC、RF trim、校准数据读取；不可逆 write 不启用。
  - TRNG byte source：替换 RF 里的 xorshift scaffold。
  - TSENSOR basic read：替换固定 25°C scaffold；完整校准未闭合时返回带状态的 conservative value。
  - IRQ line registration：HAL/RT 提供通用 local IRQ enable/dispatch 能力，`ws63-rf-rs`
    维护 blob handler table。
- `uapi_nv_read` 第一版从官方 fwpkg/NV 分区或已知 flash/NV item 读取真实 MAC/RF
  calibration；找不到时返回明确 `RF_ERR_NV_MISSING`，不静默使用全零校准。
- 当前 evidence：SVD/PAC 已建模共享 RAM bank 与 BT exchange-memory gate；HAL
  `SharedMemory`（unstable）实现原厂默认 `dyn_mem_cfg` 序列。`uapi_nv_read` 已对齐
  4 参数 SDK ABI，并实现只读 ACPU KV page/key/CRC 解析；host parser tests 3/3 通过。

### RF3 -- Wi-Fi Init On Silicon

- 调用最小 `uapi_wifi_init` / vendor init path。
- custom relocation 已采用 stock `rust-lld` layout 驱动的受控 patch lane；原厂
  linker 不再参与最终地址决定。长期仍应推动 LLVM/binutils 正式识别 vendor
  relocation，当前 lane 保留 manifest 与布局 fail-closed 保护。
- 当前真机路径已越过 ROM veneer、cache、task ABI、PMP/shared-memory 和 NV 初始化，
  UART 稳定输出 `RF2_INIT_OK ifname=wlan0`。任务切换按原厂 LiteOS 契约保存
  `tp`、`mstatus` 和 `fcsr`，ROM FRW/HMAC 实现保持复用。
- C ABI 对抗审计还修正了两个会静默破坏 init 的问题：tsensor 改回
  `int8_t *` 输出参数 + status 返回；删除一参数 Rust `frw_rom_cb_register` stub，最终
  ELF 已校验该符号解析到官方 mask-ROM `0x128d4a`。完整构建脚本会持续检查该地址。
- 第一次修复后 HIL 已越过旧非对齐 trap，但定位到 `fe_rf_dev_attach.c:92` 的
  `error rf cfg ops 35`。反汇编进一步证明 blob 自身按当前 feature 配置注册
  `RX_NETBUF=0x105`、`RX_MSG=0x106`；示例原先手写 267/268 可能越界破坏 callback
  table 相邻数据，现已删除这组重复注册，交由 vendor init 唯一负责。
- 成功输出 `RF2_INIT_OK`；失败输出分类错误：
  ROM symbol fault、custom relocation fault、missing NV/eFuse、RF clock fault、
  IRQ not delivered、memory layout fault、blob panic。
- trap diagnostics 输出 `mcause`、`mepc`、`mtval` 和当前 RF phase marker。
- HIL 验收：`wifi_init_smoke` 通过 `hisi-fwpkg plan` 生成 image，烧录后 UART 必须出现
  `RF2_INIT_OK` 或结构化 `RF2_INIT_ERR:<class>`，不能无声挂死。

### RF4 -- Scan MVP

- `wifi_init_smoke` 通过窄 `Wifi::init` / `Wifi::scan` API 执行 STA scan，并把
  vendor 结果转换为固定容量 `ScanResult` 数组。
- 原厂 `PBUF_ZERO_COPY_RESERVE=80` 已落实为
  `[pbuf header][80-byte headroom][payload]`；`oal_pbuf_netbuf_alloc` 的
  `payload - 0x50` 因而保持在分配内，不再覆盖通用 heap。
- HIL 已验收：无高流量诊断的 `full-init` 镜像输出
  `RF3_SCAN_OK count=0x20` 和真实 AP 的 SSID/frequency/RSSI；WLMAC IRQ 也在扫描期间
  到达。输出仍限制为固定容量，避免无界日志。

### RF5 -- Data Path, Connect And Ping

- **RF5A TX/RX closure**：从 vendor headers 生成 pbuf layout checks；真实 TX symbol
  替换测试 sink，RX `driverif_input` 进入有界 L2 queue，先以 ARP round-trip 验收。
- 当前 RF5A evidence：以原厂 `lwip/lwipopts_default.h`（`NO_SYS=0`）和 delivered
  archive DWARF 为 oracle，确认 `pbuf=32 bytes`、`netif.drv_send@244`、80-byte
  headroom、4-byte tailroom 和 RX `ETH_PAD_SIZE=2`。smoltcp token 的 MTU scratch
  已移到静态单占用缓冲，`dhcp_probe` 栈帧从超过主栈的规模降到 1136 bytes；真机
  获得 `192.168.155.2/24`、router `192.168.155.1`，随后 ARP request/reply 通过
  Rust-visible L2 path，UART 输出 `RF5A_DHCP_OK`、`RF5A_ARP_OK`。
- **RF5B Open-AP connect**：增加 typed station config 和 deferred connection events；
  受控实验室 open AP 只用于数据面证明，不作为生产安全承诺。
- 当前 RF5B evidence：`OpenNetwork::from_scan` 只能从真实 scan result 构造，
  `Wifi::connect_open` 直接使用 SDK `ext_associate_params_stru` ABI；vendor event 6/7
  只复制连接/断开状态，用户逻辑在普通任务上下文轮询。2026-07-12 真机扫描并关联
  无密码测试 AP `HUAWEI-HLJ_Guest`，UART 输出
  `RF5B_CONNECT_OK freq=0x0000096c`（2412 MHz）。这证明 802.11 open-system
  auth/association 已闭合；同日 RF5A Rust-visible TX/RX 与 RF5C ping 也已通过。
- 原厂 app 变体的 `libwpa_supplicant.a` 已作为可选 archive 收入 `ws63-RF`，用于后续
  WPA bring-up；开放网络路径默认不链接它。
- **RF5B WPA2-Personal oracle**：`wpa` 实验 feature 已按原厂 `--short-enums` ABI
  对齐 scan/association/event 结构，并按原厂顺序初始化 unified cipher driver 与
  mbedTLS harden provider。2026-07-12 真机连接受保护的 `HUAWEI-HLJ_Guest` 后依次输出
  `RF5B_WPA_CONNECT_OK`、DHCP、ARP 和 ping marker。该路径仍链接完整原厂 supplicant
  与 mbedTLS，只作为裁剪前行为 oracle；不据此承诺 WPA3、SoftAP 或 Enterprise。
- **RF5C Ping**：静态 IPv4 先行、DHCP 后补；ICMP 必须经过 Rust-visible L2 path。
  UART、ELF layout、patch manifest、image plan 和资源占用形成后续拆分的 A0 baseline。
- 当前 RF5C evidence：2026-07-12 真机通过。修复 ICMP frame buffer 只有 42 bytes、
  但 IPv4 `total_length` 声明 60 bytes 的长度不一致后，`HUAWEI-HLJ_Guest` 路径依次输出
  `RF5B_CONNECT_OK`、`RF5A_DHCP_OK`、`RF5A_ARP_OK` 和 `RF5C_PING_OK rx=0x00000004`。
  Echo Request/Reply 均经过 Rust-visible L2 path；A0 证据冻结在
  [WS63 RF A0 baseline](evidence/ws63-rf-a0-2026-07-12.md)。
- WPA2-Personal oracle 的独立真机记录见
  [WS63 WPA2-Personal evidence](evidence/ws63-wpa2-personal-2026-07-12.md)。
- WPA2-only supplicant 已从原厂 Ninja 编译图独立重建，并在删除 SAE/AP/EAP-TLS/WPS/WAPI
  源码及 supplicant mbedTLS archives 后复现同一组真机 marker。PBKDF2/TRNG 使用
  WS63 unified-cipher，SHA/HMAC/AES 使用 RustCrypto，并在关联前执行真机 KAT；见
  [WS63 cropped WPA2 evidence](evidence/ws63-wpa2-cropped-2026-07-12.md)。
- RF5 完整 API、crate 边界、RTOS/NVS/BLE/SLE 后续见
  [Connectivity 全栈重构计划](hisi-connectivity-stack.md)。

## Issue Mapping

- `#9`：RF1 必须关闭，`.wifi_pkt_ram` NOLOAD 是 image 前置。
- `#7`：RF2/RF3 分批关闭；IRQ、uapi、libc/sched stub 不要求一次全部清零，但 init 所需路径必须真实。
- `#6`：保留到 RF5；不阻塞 init+scan。
- `#8`：拆成 scan/connect/ping 子任务；本计划只把 scan 作为第一阶段目标。
- `#17`：HAL 的 eFuse/LSADC 完整验证不阻塞 RF，但 RF 需要的 eFuse/TRNG/TSENSOR 最小读路径要补
  HIL 或保持 unstable。
- `#16`：I2C 修复属于 HAL 0.6.0 blocker；不直接阻塞 RF，除非 RF/NV 路径实际依赖 I2C 外设。

## Test Plan

- No-hardware:
  - `cargo build -p ws63-rf-rs --release`
  - `cargo build -p rf_port_demo --release`
  - `cargo build -p wifi_init_smoke --release`
  - `chips/ws63/rf/tools/rf-build-full-init-lld-layout-patch.sh` 生成完整
    `wifi_init_smoke` ELF，并验证 patch manifest 与 final layout。
  - `cargo test -p ws63-rf-rs --target <host> --lib` 验证 NV parser。
  - `chips/ws63/rf/tools/mac-link-residual.sh`
  - `chips/ws63/rf/tools/rf-reloc58-diagnose.sh`
  - QEMU smoke：`rf_port_demo` 输出 `RF PORT DEMO: PASS`
  - `netif_smoltcp_selftest` 保持 ARP/selftest 通过。
- Link/image:
  - `readelf` 验证 `.wifi_pkt_ram` 是 `NOBITS/NOLOAD`，地址 `0x00A00000`，大小 `0xC000`。
  - `hisi-fwpkg plan --image-output` 生成完整 image，hash/body range 正确。
  - `hil/pack.sh` 只把上述 canonical `.img` 封装成 app-only FWPKG，不再从 ELF
    重复推导另一份 partition image。
- HIL:
  - `wifi_init_smoke --features full-init` 必须输出 `RF2_INIT_OK`，随后输出
    `RF3_SCAN_OK count=N` 或分类错误。
  - RF HIL 不进入普通 PR gate，放 self-hosted/manual workflow；每个 RF milestone 合并前必须留 UART log 证据。
  - RF5A HIL 依次要求 `RF5B_CONNECT_OK`、`RF5A_DHCP_OK`、`RF5A_ARP_OK`，并禁止
    `RFDBG_EXCEPTION` / `RFDBG_FRW_QUEUE_BOUNDARY`。
  - RF5C 只有出现 `RF5C_PING_OK` 才通过；该 marker 已于 2026-07-12 在 WS63 真机获得。
    `RF5C_PING_TIMEOUT` 仍只表示 request TX 已运行，不是 connectivity pass。
  - WPA2-Personal HIL 从 secret 注入 passphrase，必须依次出现
    `RF5B_WPA_CONNECT_OK`、`RF5A_DHCP_OK`、`RF5A_ARP_OK`、`RF5C_PING_OK`；日志和
    artifact 不得记录 passphrase。

## Assumptions

- RF 推进不阻塞 `hisi-riscv-hal 0.6.0`。
- HAL 能力优先，但新补的 RF 相关通用能力默认先 `unstable`，不扩大 HAL stable 面。
- Init、scan、RF5A-C 与 WPA2-PSK/CCMP W0-W1 已完成；当前优先级是按
  [Connectivity 全栈重构计划](hisi-connectivity-stack.md) 抽取 `hisi-crypto` 等独立组件，
  然后推进 WPA3/SAE、SoftAP 和 Enterprise，各阶段保持连接性 marker 不回归。
- `libwpa_supplicant.a` 暂不 vendored；开放 AP / scan MVP 不需要它。
- 如果真实 blob custom relocation 无法被 stock `lld` 产出可执行 image，优先定位并记录
  relocation 类型，再决定 linker workaround、post-link patch 或专用转换工具。
