# 发布与证据可信化验收

日期：2026-09-07。性质：独立复验，不是新增实施计划或产品发布。

**结论：未通过完整验收。** 原有整改已经修复发布镜像旁路、旧 consumer 版本、公告
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
