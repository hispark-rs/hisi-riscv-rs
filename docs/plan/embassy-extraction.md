# Embassy 职责整改（2026-11-01）

**状态：** HAL 跨芯片整改的配套工作；实现、仿真与实板验收分别记录。

用户撤回独立 `hisi-embassy` 仓库方案；未创建远端仓库，也未推送该组件。
未发布原型保存在工作区 `validation/embassy-extraction/superseded-hisi-embassy/`，用于
迁移对照，不参与构建。此次不操作实板。遵循 `hisi-rtos-future-architecture.md`，
先在现有 RTOS 仓库内整合；没有提前拆出 core/port/adapter 新仓库。

## 职责与依赖

```text
应用/平台 → hisi-rtos 门面 → 所选时间 profile → HAL → PAC
应用/平台 → hisi-riscv-rt / Embassy executor（独立选择）
```

HAL 保留 timer/counter/IRQ、标准 async trait 和外设等待/取消；不保留 Embassy 队列、
注册或调度策略。RTOS 门面负责 profile 选择，平台负责时钟、跨核交接和中断路由。
日后抽取 `hisi-rtos-embassy` 必须依赖抽出的 core 契约，由 facade 组合，不能反依赖 facade。

| 比较项 | 已有 RTOS `embassy` | 从 HAL 承接的 `embassy-only` |
| --- | --- | --- |
| 时间来源 | 注入 `monotonic_ms` | 显式交出的 TCXO |
| ABI / 分辨率 | 1 MHz tick / 1 ms | 1 MHz tick / 配置上的 1 µs，非物理精度证明 |
| 计数范围 | u64 毫秒转 tick，乘法饱和 | 完整 u64 原始计数除以 24/32，倒退/完整回绕报告 epoch 故障 |
| 截止时间 | 与线程睡眠/时间片/预算/锁期限取最早；向上舍入毫秒 | 保留绝对微秒截止时间，32 位硬件闹钟分段 |
| 硬件所有者 | SchedulerPort，仅一个共享定时器 | HAL TIMER/TCXO，永久交接，无线程调度器 |
| 唤醒 | 改为先解除队列借用再回调；重编程 generation 覆盖回调重入 | 保留 busy/dirty 及编程期间重入检测 |
| 初始化 | 原 start / WS63 port 合同保留 | unsafe init 返回失败资源；成功无 deinit；禁止运行中换频/停钟 |

只保留一处 `time_driver_impl!`；同一编译 profile 只有一个队列实例和一个硬件定时器所有者。
两种 profile 共用有界队列实现及其测试。`embassy-only` 不编译 scheduler/context/storage，
不提供 scheduler start API。普通 HAL async 完全不需要 RTOS 或 Embassy。

## 行为与迁移

- `hisi_hal::embassy` 改为 `hisi_rtos::embassy`，纯 Embassy 消费者显式选择
  `embassy-only,chip-*,unstable`；已有 RTOS 消费者继续使用 `embassy`。
- 原 HAL Embassy 接口均为实验接口；普通 HAL 已发布 blocking/async API 保留。
- 队列沿用 WS53 16 项实现。queue-utils 会在借用期间调用 Waker，与已有重入合同不符；
  新共用队列在借用外 clone/drop/wake。共享 RTOS 路径也改用同一实现，不留下第二套队列。
- RTOS 唤醒回调移出队列锁后，重编程 ticket 必须在回调前领取，避免旧计划覆盖新计划。
  这项行为变化单独测试，不能继承原 A3 实板结论。
- Cargo 会合并 feature；纯 Embassy 示例与 RTOS/RF 示例分开构建。纯 profile 隐藏
  scheduler API，误合并不会静默启动第二个定时器所有者。
- 硬件操作差异仍在小型 backend 中；不得为每个芯片复制完整调度器。

## 实施与验收

增量复核：RTOS `dad311b` 将纯 profile 的启动/存储不可见性改为三个独立 E0432
反例，共享 profile 增加原 API 正例；开发依赖与子仓锁文件同步 HAL `f39aef7`。
独立时间矩阵、HAL 无 Embassy 依赖检查及五组远端消费者通过；只改门控测试和
开发依赖，未改变时间精度、调度行为或新增实板结论。
见[复核证据](evidence/rtos-profile-contracts-2026-11-01/README.md)。

1. HAL 硬件接口 `e73a4b6`，职责迁移 `2b6cfce`，共用 TCXO 字访问器 `5895b95` 已推送。
2. RTOS 共用队列、唯一注册、纯 profile `9b83efa` 已推送；子仓有自有测试与显式远端开发配置。
3. WS63 示例、模板和父仓示例已迁移；STA/SoftAP 分开检查通过，父仓只解析一个 HAL。
   WS63 加密/RF 两处 alpha.9 精确依赖同步到 alpha.10，独立回归通过。
4. WS63/BS21 纯 Embassy、WS53 正常/时钟停顿恢复 QEMU 通过。旧 TCXO 模型固定返回
   VALID、不反映 EN 的问题由 QEMU `e60f95c` 修复；双芯片 qtest、11 项旧 qtest 和
   WS53 TCXO 原回归通过。原始模型和失败报告完整保留，没有绕过初始化检查。
5. RTOS 共存场景的 QEMU 阻塞已定位并修复：旧模型遗漏 SYS_CTL1 软件中断，迁移前
   `d17beda` 与迁移后均为 native=7、embassy=7、timer IRQ=14、switch=14。
   QEMU `d61c4a3` 补上 cause36–39 后，同一未修改 ELF 达到 native=17、embassy=10，
   通过原 native>=8 等门槛；新增示例还要求 software IRQ>0。没有改变生产 RTOS 调度策略。
   这只关闭该模型缺陷，不能关闭 `block_on` 软件唤醒/WFI 竞态或继承旧 HIL 结论。
   见[共存定位证据](evidence/rtos-coexist-2026-11-01/README.md)。
6. 不执行实板；所有新行为的 RAM / Flash / 冷启动结果保持未验收。

测试与来源索引：[精简报告](evidence/embassy-rtos-migration-2026-11-01/report.json)。
HAL 主机 WS63/BS21/WS53 为 336/220/427；RTOS 基础/共享为 86/91，每芯片纯 profile 14。
远端依赖的独立测试、文档/错误 feature 检查及三芯片 Clippy 通过；父仓外的 WS63/BS21
生成模板完整链接通过，WS53 外部消费者主机与目标检查通过。开发覆盖不依赖本机绝对路径。

`ws53-pac 0.1.0-alpha.1` 已按原标签发布；registry 包与空缓存外部消费者主机/RISC-V
检查通过。仍未发布：HAL alpha.10、含 `chip-ws53` 的 runtime、RF runtime driver alpha.20、
新 RTOS profile。子仓 registry-only 发布检查保持阻塞；固定远端的开发 CI 与发布 CI 分开。
本轮没有创建新仓库，也没有发布其他 crate。各分支与 PR 见[多仓状态](ws53-workspace-status.md)。

## 复现入口

- RTOS 独立检出：开发模式将 `.ci/development-dependencies.toml` 复制到 `.cargo/config.toml`，
  执行 `bash scripts/check-time-profiles.sh`；不复制则为 registry 模式，当前有上述发布阻塞。
- HAL 独立检出：`bash scripts/check-ws53-integration.sh`，随后运行依赖边界检查。
- 父仓 QEMU：`uv run --script scripts/test-embassy-time-profile.py --qemu <binary> --elf <elf>
  --case ws53|ws63|bs21|coexist --output <new-report.json>`。coexist 必须使用含软件通知源的
  固定模型；带 `model-timeline` 构建另加 `--require-timeline`，缺事件或溢出均不通过。
- 本轮未整理其他历史未提交证据，没有删除工作树、原始波形、备份或已有成果。

远端 RTOS [CI 41281349605](https://github.com/hispark-rs/hisi-rtos/actions/runs/41281349605)：
固定远端依赖的 time-development 与原 TLA 作业通过；registry check 和 Kani 在解析
未发布 HAL alpha.10 时失败，Kani 未进入证明执行阶段。不能称全套 CI 或 Kani 已通过。
文档已在可写 uv 缓存下重新构建，预处理器及 HTML 输出均成功。
