# 发布与证据可信化验收

初次验收：2026-09-07；闭环复验完成：2026-09-08。

**最终结论：发布与证据可信化的 A1-A5 及本轮端到端补验已通过。** 修复、真实 CI/
发布演练、下载字节复验和失败注入的依据见文末；不扩大为全生态功能稳定声明。

**初次验收结论：未通过完整验收。** 后续修复与最终复验见文末，以下保留初次发现。
原有整改已经修复发布镜像旁路、旧 consumer 版本、公告
适用性缺失和部分文本漂移，但几个会产生假阳性结果的门禁仍可被反例穿透。
不能继续用“F1-F6 全部关闭”代表原评审的关闭条件已全部满足。

本报告补充并收窄[同日生态评审的 L0 关闭声明](ecosystem-architecture-quality-roadmap-2026-09-07.md)。
这不是认定现有固件损坏、历史 HIL 无效或本次 CI 没有执行证明；问题是验收工具尚不能
可靠拒绝缺失、损坏或未执行的证据。

## 发现

### A1 / P1：损坏镜像头和不完整写入计划仍通过格式验收

定位：`scripts/check-flash-plan.py:58`、`:74`、`:83`。

检查器只比较 JSON 指定范围内的 body hash，不解析实际镜像头；对 write chunks
只检查每块不越界，没有检查其并集覆盖完整 image。两项独立 mutation 已本地复现：

- 对真实 blinky FlashPlan 产物，将 image 的全部 768-byte header 清零，保持 body
  和 plan 不变，检查器仍接受。
- 保持原 image/plan 其他字段，将 `write_chunks` 改成仅覆盖 base 地址的 1 byte，
  检查器仍接受。

原始未修改产物为 9,640 bytes、body 8,872 bytes，原始 SHA256SUMS 本轮复验通过。
因此这是验证器的漏检，不是声称原始产物已经损坏。release 在此检查之后生成
SHA256SUMS，故若生成阶段出现头损坏，随后计算文件 checksum 也不能补回语义检查。

**关闭条件：**使用 `hisi-fwpkg` 自身的解析/验证语义重新读取产物，核对真实
header/code-area/hash 与 plan；不要在 Python 重写一份芯片头格式。检查写入并集
完整性及非法重叠/间隙。新增 header 损坏、截断、只写一字节、gap/headered/large
ELF 的负测试，并在隔离目录复验实际下载产物。

### A2 / P1：父仓仍在完成发布验收前公开 Release

定位：`.github/workflows/release.yml:59`、`:95`、`:121`、`:138`、`:163`。

`Create Release` 没有 `draft: true`，排在 mdBook/rustdoc 构建、版本站点校验及在线
检查之前。后续任一步失败，已公开的 release 仍存在；同 SHA 的普通 CI 成功不能替代
尚未执行的 tag/version 路径。固件 build 也仍缺少 `--locked`，未提供原评审要求的
release train manifest。

**关闭条件：**准备阶段生成不可变产物和来源清单；package/docs/contract 全通过后再
执行唯一公开发布步骤，或使用 draft 并在验收后显式 promote。注入 tag-docs 失败，
验证不存在非 draft release。build 使用 `--locked`；来源清单绑定 parent/submodule
revision、lock digest、toolchain/tool version 和每个资产 hash。

### A3 / P2：proof manifest 可以把未执行证明记为 pass

定位：`crates/hisi-rtos/scripts/check-requirements.py:90`、`:121`；
`crates/hisi-rtos/scripts/proof-evidence.py:222`。

`validate_kani` 只要求 workflow 文本出现 `--harness <name>`；`record_kani` 不读取
每个 harness 的实际结果或 step outcome，直接为 requirements 中的清单写入
`completed-proof-run` / `result=pass`。临时 fixture 中加入 `if: false` 跳过一个 Kani
步骤后，映射检查仍返回 0；未执行任何 Kani 的合成 fixture 也能生成宣称 13 个
harness 完成的 manifest。合成 fixture 使用既有测试环境变量，不是真实 CI 证据。

TLA 同样有缺口：从 `ReadyOwnership.cfg` 移除全部 `INVARIANT` 行后，映射检查仍通过。
它只查模型文件里有名称，没有查配置实际启用的性质；记录工具只认 TLC 成功字符串，
无法区分“遍历状态成功”和“所需不变量已检查成功”。

**关闭条件：**每个必需 harness 必须有对应执行记录、成功 outcome、工具版本、参数/
bounds 和日志摘要；缺失/skipped/失败任一项均不得生成完成态证据。TLA 需校验
requirement 对应性质确实在所用 config 中启用，并记录 constants/fairness/bounds。
保留当前真实 Kani/TLA jobs 和 legacy counterexamples，不用新的纯文本声明替代它们。

### A4 / P2：HIL 的 exact 标记仍是元数据声明，不是产物交叉校验

定位：`crates/hisi-rtos/scripts/check-requirements.py:153`、`:235`、`:347`。

检查器校验 SHA-256 的语法，但不读取 ELF，也不校对 immutable evidence 文档或
manifest 中记载的值。将一条 exact firmware SHA 换成 64 个 `0`，检查仍成功且仍报告
2/7 marker records bind exact firmware。

目前报告已明确是 `contract-map`，把另外 5 条历史记录降级为 legacy 也是正确改进。
但原 F5 的“ELF 与 evidence 不符会失败”关闭条件仍未满足。该反例不意味着当前两条
历史 hash 已错误，只说明工具不能发现一个格式合法的错误绑定。

**关闭条件：**区分 declared identity 与 artifact-verified identity；为可验证等级提供
可定位的 immutable bundle，包含 ELF/hash、summary、raw-log digest、runtime/parent
revision 和 profile。验收读取实际文件并交叉比对；缺失文件、错 hash、错 runtime
revision 都失败。无法获取原文件的历史记录只能保留为受限历史声明，不能重新认证。

### A5 / P1：release-train 在没有发布物时仍报告已发布

定位：`.agents/skills/release-train/train.sh:117`、`:131`、`:139`；
`.agents/skills/release-train/SKILL.md:98`。

Release 查询失败时，脚本猜测该仓库可能只发布 crates.io，并将 `STATUS_ASSET=0`。
只要选中的 workflow conclusion 为 success，最后就输出 PASS/released。它没有区分
404、网络错误、权限错误，也没有验证 crates.io 的 exact version/checksum。

已使用完全离线的 mock git/gh 复现：workflow success、Release 不存在、命令显式要求
4 个资产，脚本仍以 0 退出并报告 released。mock 不调用真实 Git/GitHub，不创建 tag。

即使 Release 存在，脚本也只数资产、提示是否有 checksum 文件，不下载并核验内容。
Skill 仍列父仓旧的 `blinky.bin + blinky.elf + SHA256SUMS` 三资产契约，与新四资产
FlashPlan 发布不一致。这是原 F6 三处指定修复之外、本轮验收新增发现的发布工具缺口。

**关闭条件：**明确 release kind（GitHub assets / crates.io / both）和 required asset
清单，不做猜测。绑定 tag SHA、workflow ID/run attempt；所有必要 job 成功后，下载
并核验实际资产；crates.io 路径验证 exact version 与 checksum。缺失或查询错误一律
失败或输出明确的未验收状态，不得输出 released。同步 Skill 和负测试。

## 端到端覆盖缺口

- RF 新 Publish workflow 已有同 SHA source-CI、不可变 candidate、12 个 candidate
  consumer job、repackage hash 比对和 published-consumer。其结构有实质改善。
  但截至本轮查询，新流程未运行过；最新 Publish 仍是 2026-09-03 的旧提交
  `c6812fc8`。[旧 Publish 记录](https://github.com/hispark-rs/hisi-rf/actions/runs/33704796687)。
- 当前普通 RF CI 的三平台 external fixture 已 exact-pin `0.1.0-alpha.114` 并通过，
  不是旧 alpha.110。当前源码 candidate 另在 Ubuntu/WPA2 路径实际构建；不能将这
  两条证据合并成“新 12 路 candidate Publish 流程已经演练”。
- 父仓最新 Release 是 2026-08-05 的 `46e7672f`，早于本轮 FlashPlan 整改。
  [最近 Release run](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/31053993876)。
  本地生成并校验 blinky 不能代替“下载新发布资产再复验”。
- RF consumer 报告目前只保存 facade version/candidate hash/profile，不保存实际
  backend/sys 版本、解析图和 consumer Cargo.lock digest；临时目录退出即删除。
  原 F1 要求的可重建解析证据尚未齐全，定位为
  `crates/hisi-rf/.github/scripts/check-release-consumer.py:225`。

下一次补验应先运行不会发布新版本的 candidate/dry-run 流程并注入失败场景；修复 A1/A2/A5
之后，再以受控 draft 和实际下载验证发布链。不要为了得到绿色记录随意创建产品 tag。

## 已确认通过的部分

| 项目 | 本轮确认的范围 | 结果 |
| --- | --- | --- |
| 父仓 source CI | 精确 HEAD `5490586cc` | [CI 成功](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34098366681) |
| 文档 CI | 同 HEAD；包括 Check Links，非只看 workflow 汇总 | [全部 job 成功](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34098366661) |
| RF source/registry consumer | `c120de33`，三平台四 profile；alpha.114 fixture | [CI 成功](https://github.com/hispark-rs/hisi-rf/actions/runs/34092745250) |
| hostap 安全雷达 | `06504cca`，在线官方索引和 2026-4/2026-5 disposition | [2026-09-07 定时 run 成功](https://github.com/hispark-rs/ws63-radio-sys/actions/runs/34099418632) |
| 计划注册表 | 11 份计划，0 个 active implementation，connectivity 为 decision-pending | 本地正向和 7 项 mutation/解析负场景通过 |
| connectivity 状态 | 84 evidence links、2 conditional items | 本地检查通过 |
| RTOS 真实证明 | `5991896f`，13 个 Kani 步骤、8 个正常 TLA 模型与 2 个 legacy 反例步骤 | [实际 job 均成功](https://github.com/hispark-rs/hisi-rtos/actions/runs/34096444025) |
| RTOS 下载产物完整性 | 3 份 proof-contract 相同；run manifest 绑定同 SHA/run；10 个 TLC log digest 匹配 | 本轮下载并复验通过 |
| F6 原指定文档 | NVS alpha.3 append/GC 与成熟度区分；IRQ typed Binding 优先；run-ws63-rs host-test 指南 | 文本/实现边界已修正；不等于全部 Skills 均验收 |

本轮重跑的既有测试：FlashPlan 4 项、image-truth 3 项、registry 正向 + 7 负场景、
RTOS evidence 5 项、RF consumer 5 项、hostap security 5 项，均通过。
新增独立反例则暴露上述漏检，说明既有测试覆盖不足，不能用其绿色否定反例。

## 范围与处置

验收快照：parent `5490586ccf512941da02fd1ebade0515185333b4`；
RF `c120de33aaf42005f785c5abf8500c9699a6e6fe`；
RTOS `5991896f01daadd4ab2d1e62d767c3f94de9f187`；
radio-sys `06504ccac57cb21c7cfa5442256cf17b178e14e0`。

本轮不改生产代码、不重新发布、不烧录、不创建 runner。扩展 mutation 仅在临时副本中
执行；原始镜像和真实 CI artifacts 均未修改。临时探针、JSON 结果和下载文件保留在
`/private/tmp/hisi-release-acceptance-20260907`，不是可长期依赖的在线发布证据。

建议处理顺序：先关闭 A1/A2/A5 的发布假阳性，再关闭 A3/A4 的证据假阳性；补齐
candidate/draft/download 和故障注入端到端验收后，再更新 L0 关闭声明。
不扩大 connectivity 功能 WIP，不要求把 liveness/timing 远期规划提前实施。

## 2026-09-07 至 09-08 闭环复验

复验状态：**通过本轮完整验收**。本节覆盖初次拒绝结论，但不删除反例历史；关闭以
实际执行、下载复验及下列范围为准。

### 新增实际反例与修复

1. **公开工具与本机同版本号语义不同。** parent `9005b34b4` 的第一次云端演练使用
   crates.io `hisi-fwpkg-cli 0.3.2`，同一个 blinky ELF 展开为 941,176-byte image；本机
   从未发布 main 重建、仍报 0.3.2 的工具输出 9,640 bytes。独立下载复验明确拒绝该
   差异，不能将“生产器自校验通过”解释成 PT_LOAD 语义正确。
   原因是 `2c139bb` 的 physical load-address 修复晚于 v0.3.2 tag。
   已以 `366c2cf` 正式发布 **hisi-fwpkg / hisi-fwpkg-cli 0.3.3**，而非重写旧 tag。
   [Publish 34108710617](https://github.com/hispark-rs/hisi-fwpkg/actions/runs/34108710617)
   执行 fmt、locked clippy/tests、锁文件检查和库/CLI 顺序发布，全部成功；本机重新从
   registry 安装并下载两份 `.crate` 校验 source SHA 与 registry checksum。
2. **同工具往返不是独立 oracle。** `test-release-evidence.py` 的通用 RV32 ELF fixture
   现在独立断言 LMA span=0x38、段间 gap=0xFF。隔离安装的公开 0.3.2 在该断言失败；
   正式 0.3.3 通过。测试不复制 HiSilicon header 常量或算法。
3. **Windows candidate 路径转义。**第一次 RF 新发布演练
   [34106579180](https://github.com/hispark-rs/hisi-rf/actions/runs/34106579180)
   在 Windows TOML 路径失败；`re.subn` replacement 对反斜线的二次解释已由 callable
   replacement 修复，补 Windows/空格/非 ASCII 路径回归。
4. **下载包污染发布 checkout。**第二次 RF 演练
   [34108425524](https://github.com/hispark-rs/hisi-rf/actions/runs/34108425524)
   的 12 路 consumer 全通过，但重打包因源码树内 `candidate/*.crate` 未跟踪而失败。
   `173deae` 将下载目录改为 runner temp，保留干净工作树要求，未加 `--allow-dirty`。
5. **模板消费者仍引用私有 API。** BLE/SLE starter 直接调用已私有化的 allocation
   函数。模板 `a877d88` 改用安装后 `runtime_allocator()` capability；独立生成的
   BLE/SLE 项目使用公开 alpha.114/alpha.25、官方 pinned nightly + build-std 完整
   链接、`just image` 和 FlashPlan 语义复验通过，BLE 离线重建通过。没有改变 RTOS
   算法、RF profile 或任何硬件结论。
6. **模板 CI 复制了旧资源尺寸。** `a877d88` 的 Wi-Fi firmware/image 都已生成，但
   CI 仍要求 v10/r9 schema 与旧 control-storage 字节数，实际公开 facade 输出 v13/r13。
   `a0a3519` 改为解析 JSON、验证已审核 schema/profile 和 owner subtotals/arena accounting；
   不再复制 profile revision 或固定总字节数。6 项负测试拒绝未知 schema、错 profile、
   缺字段、bool 冒充计数、错总量和容量不足；三个 host 的报告与锁文件上传保留。
   这只证明资源报告契约，不代表完成资源硅片校准。

### 逐项关闭依据

| 项目 | 实现与复验 | 状态 |
| --- | --- | --- |
| A1 镜像语义与写入覆盖 | fwpkg 重读真实头；完整 write-chunk 并集；独立 LMA/gap oracle；6 个 FlashPlan + 6 个 release 负测试；下载 0.3.3 产物再生成比对 | 通过 |
| A2 发布时序 | locked build；7 资产 train bundle；draft、文档、下载复验全部在唯一 promote 之前；正常演练及 pre-publication 故障注入/清理 | 通过，未创建产品版本 |
| A3 证明执行 | RTOS `d17beda`；每项 receipt 绑定命令、exit、工具、source/model/config、日志；实际下载后重新 record | 通过 |
| A4 HIL 证据等级 | 2 条改为 declared-firmware/not-reverified，5 条仍为 legacy；新的 bundle 验证器读取 ELF、summary、逐轮 raw capture 和外部 pin 的 manifest hash | 通过本轮可信边界；无新增 HIL |
| A5 发布验收工具 | release kind/required assets 明确；错误不回退 registry；实际下载 registry checksum 与 clean tag SHA；mock missing/draft/skipped/source mismatch 均拒绝 | 通过 |
| RF candidate 到交付 | 三系统四 profile、同一 candidate SHA、独立解析图/lock/ELF；重打包 hash 相等后才 publish/dry-run；下载全部 12 份复验 | 通过 dry-run；没有新 RF 发布 |
| 模板完整消费 | 公开 allocator capability + fwpkg 0.3.3；生成/构建/image 与跨平台资源报告；下载 16 份报告及 lock 复验 | 通过，无硬件声明 |

RTOS [CI 34107500673](https://github.com/hispark-rs/hisi-rtos/actions/runs/34107500673)
的 check/Kani/TLA 全部成功。独立复验实际下载的 **13 个 Kani harness、8 个正常 TLA
模型、2 个旧设计反例**；contract SHA-256 为
`ae968c7d0e7c1ddc47440b064b33713477eaee0531fdb1eee3222a1dcfd9b268`。
9 项 evidence negative tests 同时覆盖 disabled invariant、skipped/missing receipt、
错 source/log/hash、缺 raw UART、重复 run、summary 计数及固件身份错配。

父仓源提交 `4ffd06bb9987b168a1c2603789b3589e86c1661b` 的
[CI 34109784872](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34109784872)
与[完整文档 34109784883](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34109784883)
均成功。基于该精确来源：

- [正常演练 34110566113](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34110566113)
  完成 draft 上传、mdBook/rustdoc/site 校验、下载重验、草稿清理。再次从 Actions 下载
  7 件包在本机隔离目录复验，ELF/image/plan 完全一致，image 为 9,640 bytes。
- [故障演练 34110573399](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34110573399)
  只在预设的 `Inject rehearsal failure before publication` 失败，promote 跳过，cleanup
  成功。2026-09-08 分页查询 release API，两个 rehearsal tag 均无残留 release。
  此红灯是受控反例通过，不是待重跑的产品 CI 失败。
- RF `173deae319fe01531d06af96800c5534150abc12` 的
  [CI 34109164597](https://github.com/hispark-rs/hisi-rf/actions/runs/34109164597) 和
  [Publish dry-run 34109641559](https://github.com/hispark-rs/hisi-rf/actions/runs/34109641559)
  全部必需 job 成功。12 个 downloaded consumers 绑定同一 candidate SHA-256
  `ae180ba191ad1dcad2b0215f95829dcda77dee90975e6a7fb3229f0fef1d64dd`；分别重算 lock、
  ELF、report hash，校验 source/run、完整图边和 facade/core/backend/sys/blob 唯一解析。
  发布后消费阶段在 dispatch 中按设计跳过；既有 registry alpha.114 另行下载验证。
- 模板 `a0a3519862fa6c04684dbe11bcb22a5681c76f45` 的
  [CI 34174872035](https://github.com/hispark-rs/hisi-rs-template/actions/runs/34174872035)
  完成 10 组生成/检查/完整构建、3 个 host 资源报告 job、1 个负测试 job，共 14 个
  必需 job 成功；tag-only Release 按设计跳过。下载复验 7 个 Wi-Fi/BLE/SLE consumer
  与 3 host × 3 profile 的报告和 Cargo.lock，共 16 份。无板 image 覆盖 WS63
  blinky/uart_hello/Wi-Fi/BLE peripheral/SLE announce，不把其余 radio profile 的构建
  说成 image/HIL 验收。本机 Wi-Fi/BLE/SLE 均完整链接和 image 复验；Wi-Fi/BLE
  另完成 offline locked 重建。
- 最终父仓实现/指针快照 `b8870b0a6fbe433752579f90a2d6eb6d876c0a8d` 的
  [CI 34175080802](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34175080802)
  16 个 job 与[Documentation 34175080823](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/34175080823)
  6 个 job 全部通过，包括 happy path、Check Links、mdBook link check 和 Pages。
  相对发布演练的 `4ffd06bb9` 仅新增模板 pointer；没有改动发布算法/契约。

实际复验 JSON 收据与原始 CI artifact 定位已提交到
[release-trust-2026-09-08](evidence/release-trust-2026-09-08/README.md)。临时目录不再是
本轮结论的唯一定位入口；收据仍不代替重新认证时必须取得的原始字节。

### 明确边界

- 本轮关闭的是发布/证据工具的假阳性与跨仓消费漂移，不是宣布全部生态功能稳定。
- 没有新增父仓产品版本、RF 产品 tag、板卡 HIL 或本机 runner；仅 fwpkg 必须发布
  补丁以使公开工具与已验证语义一致。RF workflow_dispatch 只 dry-run。
- historical declared/legacy HIL 不因修改元数据变成 artifact-verified，也不等于无效；
  新验证器保证 bytes/identity/summary 一致性，不等于对物理实验真实性的密码学证明。
- Kani/TLA 结论只覆盖契约中列出的 bounds/assumptions；不宣称完整 liveness/timing，
  更不能把 20-reset 等价于形式证明或外部网络永不丢包。
- GitHub Actions 原始日志与产物有保留期限；run URL/hash 是审计定位，不是永久存储
  承诺。未来重新认证必须重新下载可用 bundle；文件已过期时只能报告不可复验。
- 模板 CI 的既有 Action Node 版本提示及 uv cache 配置提示不影响本次验收；没有把
  warning-free、全生态 CI 性能优化或模板产品发新版列作已完成项。
