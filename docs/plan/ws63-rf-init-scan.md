# WS63 RF 推进计划：Init → Scan 优先

## Summary

WS63 RF 的第一阶段目标是 **真实 WS63 硅片上的 Wi-Fi init + scan**。不把 RF 阻塞
`hisi-riscv-hal 0.6.0`，也不在第一阶段做 WPA2、supplicant、connect 或 ping。

RF 作为独立 connectivity track 推进；HAL 能力优先，但新补的通用硬件能力默认先进
`unstable`。RF 专用 glue、blob ABI、NV item id、porting contract 保持在 `ws63-rf-rs`。

## Key Milestones

### RF0 -- Baseline And Truth Sources

- 固定事实源：vendor 行为以 `fbb_ws63` 为准；blob delivery 以
  `chips/ws63/rf/ws63-RF` 为准；Rust porting layer 以 `ws63-rf-rs` 为实现边界。
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
- 当前实现状态：`wifi_init_smoke` 默认作为 RF1 image smoke 构建通过；开启
  `--features full-init` 会拉入完整 vendor init closure，并稳定暴露 RF3 linker blocker：
  stock `rust-lld` 在 vendor Wi-Fi 对象上报 unknown relocation 58。经
  `tools/rf-reloc58-diagnose.sh` 对照，原厂 binutils 将该 relocation 解释为
  HiSilicon `R_RISCV_48_LLUI`，并能 final-link；LLVM/upstream `lld` 按
  `R_RISCV_IRELATIVE`/unknown path 处理，不能生成最终 executable。
- 当前 workaround 保护：`tools/rf-build-full-init-oracle-patch.sh` 可用原厂 linker
  生成 oracle ELF/map，再把 `R_RISCV_48_LLUI` patch 到 RF archive 后交给
  stock `rust-lld` final-link；但该路径必须通过
  `tools/rf-verify-oracle-layout.py` 校验 oracle/final section VMA 完全一致。当前
  evidence 是 verifier 能识别真实 layout drift 并 fail closed，失败时删除不可信
  final ELF，禁止进入烧录路径。
- 当前 evidence：`hisi-fwpkg plan --chip ws63 --image-output` 可为默认
  `wifi_init_smoke` 生成 8.8 KiB image；`.wifi_pkt_ram` 是 `SHT_NOBITS`，未进入
  flash body。
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

### RF3 -- Wi-Fi Init On Silicon

- 调用最小 `uapi_wifi_init` / vendor init path。
- 先解决 RF1 暴露的 HiSilicon `R_RISCV_48_LLUI` relocation 58：可选路径是
  upstream `lld` 支持该 relocation、构建期使用原厂 linker，或实现受控 post-link /
  object conversion；修复前不能宣称 stock `lld` 可直接生成完整 RF init executable。
- oracle-patch 路径只作为受控 bring-up lane：固定输入、生成 patch manifest、
  final-link 后强制校验布局；布局漂移时必须失败并清理 final ELF。下一步优先消除
  oracle/final layout drift，或改成从 final `rust-lld` layout 解析 relocation 目标值，
  避免依赖 vendor/final linker section 地址相同这一脆弱前提。
- 成功输出 `RF2_INIT_OK`；失败输出分类错误：
  ROM symbol fault、custom relocation fault、missing NV/eFuse、RF clock fault、
  IRQ not delivered、memory layout fault、blob panic。
- trap diagnostics 输出 `mcause`、`mepc`、`mtval` 和当前 RF phase marker。
- HIL 验收：`wifi_init_smoke` 通过 `hisi-fwpkg plan` 生成 image，烧录后 UART 必须出现
  `RF2_INIT_OK` 或结构化 `RF2_INIT_ERR:<class>`，不能无声挂死。

### RF4 -- Scan MVP

- 新增 `wifi_scan` example，只支持 STA scan。
- `ws63-rf-rs` 暴露窄而不稳定的 API：
  - `Wifi::init() -> Result<Wifi, RfError>`
  - `Wifi::scan(&mut self, sink: impl FnMut(ScanRecord)) -> Result<(), RfError>`
- 注册 vendor scan callback，将结果转换为最小
  `ScanRecord { ssid, bssid, channel, rssi, auth_hint }`。
- UART 最多输出前 N 个 AP，避免长日志阻塞。
- HIL 验收：可控 AP 环境下输出 `RF3_SCAN_RESULT count=N`；若失败，输出分类失败码。

### RF5 -- Post-Scan Preparation For Ping

- 只在 scan 成功后进入，不阻塞 init+scan MVP。
- 从 vendor `lwipopts.h` / C headers 抽取 pbuf layout，在 Rust 中加 offset/size 静态断言或
  build-time check。
- 找到真实 TX symbol，将 `netif_smoltcp` TX sink 从测试 sink 替换为 blob transmit adapter。
- RX path 从 blob callback/IRQ 输入 `driverif_input`。
- 开放 AP connect/ping 另立 milestone，不和 scan MVP 混在一个 PR/issue 里。

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
  - `cargo build -p wifi_init_smoke --release --features full-init` 预期失败并报
    `unknown relocation (58)`，直到 RF3 linker blocker 被解决。
  - `chips/ws63/rf/tools/mac-link-residual.sh`
  - `chips/ws63/rf/tools/rf-reloc58-diagnose.sh`
  - QEMU smoke：`rf_port_demo` 输出 `RF PORT DEMO: PASS`
  - `netif_smoltcp_selftest` 保持 ARP/selftest 通过。
- Link/image:
  - `readelf` 验证 `.wifi_pkt_ram` 是 `NOBITS/NOLOAD`，地址 `0x00A00000`，大小 `0xC000`。
  - `hisi-fwpkg plan --image-output` 生成完整 image，hash/body range 正确。
- HIL:
  - `wifi_init_smoke` 必须输出 init OK 或分类错误。
  - `wifi_scan` 必须在可控 AP 环境下输出 scan count 或分类错误。
  - RF HIL 不进入普通 PR gate，放 self-hosted/manual workflow；每个 RF milestone 合并前必须留 UART log 证据。

## Assumptions

- RF 推进不阻塞 `hisi-riscv-hal 0.6.0`。
- HAL 能力优先，但新补的 RF 相关通用能力默认先 `unstable`，不扩大 HAL stable 面。
- 第一阶段只做 init + scan；WPA2、supplicant、connect、ping 推迟。
- `libwpa_supplicant.a` 暂不 vendored；开放 AP / scan MVP 不需要它。
- 如果真实 blob custom relocation 无法被 stock `lld` 产出可执行 image，优先定位并记录
  relocation 类型，再决定 linker workaround、post-link patch 或专用转换工具。
