# HiSilicon NVS 镜像工具链计划

## Status

**Deferred / triggered.** 当前只读 `hisi-nvs` baseline 已可服务 connectivity；N0-N3
只在要求完全脱离原厂 NV generator 时启动，N4-N5 仍是更远期工作。本计划保留 NVS
格式、oracle 和验收事实，但不属于当前 A3/A4 WIP。

## Summary

本计划补齐主机端 NVS image 生成、检查和迁移能力，目标是最终不依赖
`fbb_ws63` 的完整 C build system 生成 `ws63_all_nv.bin`。它不阻塞当前 A2
只读 parser、Wi-Fi init/scan/connect/ping，也不改变已发布 `hisi-nvs` reader API。

格式事实边界固定为：

- `hisi-nvs`：`no_std` format primitives、reader 和结构化错误；
- `hisi-nvs-image`：`std` host builder/verifier；
- `hisi-nvs-cli`：`build`、`inspect`、`verify`、`diff`；
- `hisi-fwpkg`：只把已生成 NV image 当 partition payload，不理解 page/key/CRC/GC。

Canonical manifest 第一阶段只接受已经序列化的 value bytes（hex/file 等）。RF key ID
和 typed value serialization 属于 `hisi-rf`；KV format 属于 `hisi-nvs`；image layout
属于 `hisi-nvs-image`。不把原厂 `.etypes`、C struct parser 或 `app.json` 变成新事实源。

## Current Baseline

`hisi-nvs 0.1.0-alpha.1` 已提供 backend-neutral `ReadStorage` 只读 ACPU KV reader：

- WS63 ACPU store `0x254D`、4 KiB page；
- page details/sequence complement 校验；
- duplicate logical page 选择最大 sequence，同序列后物理页胜出；
- 16-byte page/key header、magic/state/length/key/`enc_key` 解析；
- plaintext 4-byte padding、encrypted record 16-byte alignment/integrity length 识别；
- big-endian stored CRC32；
- 完整性验证先于 `BufferTooSmall`；
- `Encrypted`、`CorruptRecord`、`NotFound` 等结构化错误。

它不支持 enumerate/attributes、解密、写入、GC、恢复、升级，也不生成 NV image。

## Oracle Sources

行为事实以当前 pinned `fbb_ws63` 的以下文件为准：

- `src/build/script/nv/nv_binary.py`
- `src/build/config/target_config/ws63/build_nvbin.py`
- `middleware/chips/ws63/nv/nv_config/cfg/nv_target.json`
- `middleware/utils/nv/nv_storage_lib/*`

必须固定的 oracle 事实包括：ACPU store `0x254D`、factory/backup store `0x34B2`、
flash size `0x4000`、page size `0x1000`、4 pages、16-byte page/key headers、plaintext
4-byte padding、header+padded value 的 big-endian CRC32、unused bytes `0xFF`，以及
`ws63_all_nv.bin` / 可选 `ws63_all_nv_factory.bin` 的生成行为。

## Milestones

### N0 -- Oracle Fixtures

- 用 pinned `fbb_ws63` 生成最小 plaintext、跨页、attributes、factory/backup 和真实镜像
  fixtures；保存 canonical input、原厂工具 revision、构建参数与输出 SHA-256。
- 当前 `NvReader` 必须能 inspect/读取每个适用官方 fixture；不支持的 encrypted/attribute
  行为必须返回明确状态，不能误读为 plaintext。
- fixture license/provenance 必须允许进入测试仓；不能提交敏感设备数据或真实密钥。

**Gate:** fixtures 可由一条 hermetic host 命令重建，hash 稳定，reader 对损坏变体的错误
分类有对抗测试。

### N1 -- Plaintext Image Builder

- 新建独立 release unit `hisi-nvs-image`，提供 deterministic `NvImageBuilder`。
- 生成 page header、key header、padding、CRC、page allocation 和 `0xFF` tail。
- duplicate key、capacity exhaustion、oversized record、无效 store/page 参数明确报错。
- 第一阶段不支持 encrypted record、runtime write 或 GC。

**Gate:** 同一 canonical manifest 重复构建逐字节一致；Rust writer -> `NvReader`
roundtrip 覆盖边界长度、跨页、空值、满页和错误输入。

### N2 -- Cross-Oracle Verification

- 同一输入的 Rust builder 与 `fbb_ws63` 输出逐字节一致；若原厂包含非确定字段，则先证明
  该字段语义，再做逐字段规范化比较，不能直接放宽为“reader 能读”。
- 官方 firmware/真实 WS63 接受 Rust 生成镜像，并完成 NV-backed Wi-Fi init/scan。
- 保存 image hash、partition plan、UART marker 和差异报告。

**Gate:** 至少一个真实官方 fixture byte parity + 一个 Rust image 真机 HIL。仅“自己的
reader 能读自己的 writer”不足以通过 N2。

### N3 -- CLI And Packaging

- 新建 `hisi-nvs-cli`，提供 `build`、`inspect`、`verify`、`diff`。
- `inspect` 输出 store/pages/keys/attributes/encrypted/corrupt；`diff` 按 key 语义比较，
  同时保留 raw offset 诊断。
- `hisi-fwpkg` 接收生成的 `ws63_all_nv.bin` 作为 partition payload；partition
  address/capacity 继续由 fwpkg/chip partition manifest 管理。
- 独立 Rust firmware/release image happy path 在 N3 之前不能宣称摆脱原厂 NV generator。

**Gate:** CLI golden tests、malformed image fuzz/property tests，以及 `hisi-fwpkg` package
roundtrip 不改变 payload bytes。

### N4 -- Factory, Backup And Upgrade

- 支持 `0x34B2` factory/backup image、normal/permanent/non-upgrade attributes。
- 对齐原厂 upgrade merge、restore、version 和 duplicate-page rollover 规则。
- 与原厂 `ws63_all_nv_factory.bin` oracle 对齐。

**Gate:** factory/backup cross-oracle fixtures、升级前后 semantic diff，以及真实设备恢复
演练；N4 不阻塞 Wi-Fi connectivity baseline。

### N5 -- Encrypted And Write Lifecycle

- encrypted record 依赖成熟的 `hisi-keystore`/`hisi-crypto`，只使用不可导出 key handle；
  `hisi-nvs` 不拥有 key policy。
- runtime write、invalid/delete、GC、power-loss recovery、sequence rollover 和 endurance
  全部保持 unstable。
- 不允许硬件 crypto 失败后静默软件回退，不允许在 XIP/critical section 中等待 erase/write。

**Gate:** 掉电注入、损坏恢复、rollover、erase endurance 和真机 HIL 完成前，不进入
stable API。N5 不阻塞 scan/connect/ping。

## Dependency And Scheduling

- 当前 connectivity A2 只读 reader 迁移独立完成，不等待 N0-N5。
- N0-N2 必须先于“完全脱离原厂 NV 生成流程”的声明。
- N3 必须先于独立 Rust firmware/release image happy path。
- N4/N5 不阻塞 Wi-Fi scan/connect/ping、A3 RTOS 或 A4 radio API migration。
- encrypted NVS 必须等待 `hisi-keystore`、hardware crypto resource/error contract 和 HIL。

## Test Plan

- Host: official fixtures、roundtrip、malformed bounds/CRC/complements、duplicate sequence、
  deterministic build、semantic diff。
- Cross-oracle: pinned vendor generator output hash 与逐字节/逐字段报告。
- Packaging: NV payload 经过 `hisi-fwpkg` pack/unpack 后逐字节一致。
- HIL: official firmware 和 Rust connectivity firmware 分别接受生成镜像；记录 NV marker、
  init/scan，以及适用阶段的 connect/ping。

## Assumptions

- KV format 的唯一 Rust 事实源是 `hisi-nvs` 体系，不是 `hisi-fwpkg`。
- 第一阶段不重做 `.etypes`/C struct parser；可后续提供 `import-fbb` 兼容工具。
- fixtures 不包含生产凭据、设备秘密或不可再分发内容。
