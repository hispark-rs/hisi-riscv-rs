# WS53 多仓库维护状态

## 2026-11-01 UART 回归工具链与干净检出

父仓 `29e02117a` 已推送：UART 构建/运行器纳入 Git，并脱离旧温度/PWM/RTC
脚本导入链；复用已跟踪的构建来源清单，将 QMP/qtest 与完整 DWARF 路径绑定分别
收敛为工具模块。CI 加入正常、路由阻断/恢复及取消超时/恢复场景。

远端干净稀疏检出、五个精确子模块和全新 Cargo/uv 缓存构建通过；CI 固定 QEMU
`27def194` 从固定上游重新编译，八轮模型回放通过。协议/清理八项和记录/ELF 来源
反例九项通过，检出及 overlay 均保持干净。因此下节“本地辅助脚本未跟踪”的复现
缺口由本批关闭；整条远端 workflow 结果仍单独验收。HAL 保持 `3ff019d`，无行为
改动。WS63 RX 模型、实板、TX 与 WFI 问题未关闭。
见 [报告](evidence/uart-model-repro-2026-11-01/report.json) 和
[可复现入口](uart-model-reproducibility.md)。旧脚本/记录已保全，未重写旧验收证据。

## 2026-11-01 UART 共用接收增量

HAL `refactor/uart-async-receive` / `3ff019d` 已推送，
[草稿 PR #22](https://github.com/hispark-rs/hisi-hal/pull/22) 叠加 GPIO #21。
`0052bd3` 先保持行为抽取 WS53 接收轮询；`3ff019d` 将 WS63 接入共用等待/取消守卫，
删除 ISR 单字节暂存槽，保留 FIFO 数据并关闭取消后的接收通知。每芯片寄存器、错误
及所有权契约保留后端；WS63 async TX 仍是阻塞实现，本批没有闭合该问题。

独立主机测试 WS63/WS53/BS21 为 366/442/220，三芯片目标 Clippy、文档测试及六组
无 Embassy 依赖检查通过。全新 Cargo 缓存从远端精确提交构建三芯片外部库消费者
通过；该开发图仍含显式 runtime override，不是 registry 发布验收。
父仓仅更新 UART Flash 示例锁文件中的 HAL alpha.9→alpha.10，其他锁定版本不变。

WS53 匹配既有 UART 模型的八轮本地回放通过，含正常、路由阻断/恢复、取消超时/恢复。
所用旧辅助脚本仍未跟踪，**本地回放不计入干净检出 CI 验收**；哈希与整理缺口已记录。
WS63 UART 模型缺少 FIFO 计数和中断屏蔽语义，模型验收未关闭。全部实板验收保持原状态，
本轮未操作硬件。见 [证据](evidence/uart-async-lifecycle-2026-11-01/report.json)。

## 2026-11-01 GPIO 生命周期增量

HAL `refactor/gpio-async-lifecycle` / `454ab40`，先抽取 WS53（`08a4120`）、修复
WS63 每引脚隔离（`737599f`），再保留临时屏蔽时的 pending；三批已推送，
[草稿 PR #21](https://github.com/hispark-rs/hisi-hal/pull/21)
叠加 #20。父仓同步可从远端取得的精确指针、新模型夹具及独立锁文件、CI 与证据。
既有依赖版本没有变化，其他组件开发图保持各自验收提交。

三芯片主机/目标检查、WS53/WS63 文档测试和无 Embassy 依赖检查通过。WS63 模型
验证同组引脚相互隔离及取消，WS53 GPIO async 增加 ISR 前重 poll 并通过；
相同夹具的旧版及中间版本 pending-peer 失败均保留。
全新 Cargo 缓存的父仓外消费者从远端构建并通过 WS63 模型。新增 CI 远端结果尚未
验收，发布与实板阻塞保留。完整范围见 [报告](evidence/gpio-async-lifecycle-2026-11-01/report.json)。

## 2026-11-01 BS20 独立回归与 WFI 反例

父仓 `18dab2e18` 已推送：同步 BS20 独立锁文件中的 HAL/runtime/PAC 版本，
不改变已锁定第三方版本；新增独立 CI 构建九个 BS20 示例。远端干净稀疏检出、
四个精确子模块和全新 Cargo 缓存下，九个 release ELF 全部链接成功且检出仍干净。
下方“BS20 锁文件阻塞”已由本批关闭；远端新增 CI 结果尚未验收。
见 [报告](evidence/bs20-regression-2026-11-01/report.json)。

未修改 HAL/runtime 行为。另建立 `block_on` 软件唤醒失败的独立 QEMU 反例，
明确记录为未修复，不以诊断用两次 poll 代替产品修复；平台休眠交接仍待设计。
见 [审查及证据](hal-block-on-idle.md)。没有硬件操作或稳定性升级。

## 2026-11-01 DMA 控制器生命周期增量

HAL 当前 `fix/dma-controller-lifetime` / `c907bed`，已推送；
[草稿 PR #20](https://github.com/hispark-rs/hisi-hal/pull/20) 叠加在定时器 #19 上。
父仓同步子模块精确提交，无依赖版本变化，不改已有锁文件。编译期要求旧 DMA
guard 持有静态控制器，防止遗忘短借用 guard 后重新取得原控制器；没有运行时改动。

主机 WS63/BS21/WS53 351/220/432、三芯片目标 Clippy、旧芯片文档测试通过。
全新 Cargo 缓存下远端开发图的正例主机/RISC-V 构建通过，反例按预期拒绝。
WS63/BS21 现有 DMA 示例通过；BS20 的 `--locked` 检查被旧锁文件阻塞，未更新或
掩盖这一问题。QEMU 未重跑，实板未操作，发布及总线排空阻塞继续保留。
见 [报告](evidence/dma-controller-lifetime-2026-11-01/report.json)。

## 2026-11-01 定时器生命周期增量

| 仓库 | 分支 / 提交 | 状态 |
| --- | --- | --- |
| hisi-hal | `refactor/timer-async-lifecycle` / `6a6ff72` | 两批提交已推送，[草稿 PR #19](https://github.com/hispark-rs/hisi-hal/pull/19)；共用生命周期，分别保留芯片后端和所有权入口 |
| ws63-examples | `test/timer-async-lifecycle` / `20bac94` | 已推送，[草稿 PR #8](https://github.com/hispark-rs/ws63-examples/pull/8)；三通道取消/复用夹具 |

父仓更新两个子模块指针、模型 CI 与报告上传、精简证据。没有版本/依赖清单变化，
原集成锁文件无需修改；其他组件的固定开发图保留各自原验收提交。主机、目标与
远端消费者通过，WS63 三通道和 WS53 两通道 QEMU 通过，SPI/I2C DMA 组合
50/166 场景通过。远端新增 CI 和实板未验收，发布阻塞未关闭。
完整结果见 [报告](evidence/timer-async-lifecycle-2026-11-01/report.json)。

## 2026-11-01 HAL IRQ 等待增量

HAL 当前为 `fix/irq-signal-rendezvous` / `da254f8`，已推送并核对远端精确提交；
[草稿 PR #18](https://github.com/hispark-rs/hisi-hal/pull/18) 叠加在 #17 上，只包含
软件登记竞态修复、五驱动共用等待入口、子仓测试及契约文档。父仓同步此指针，
版本和依赖清单未变，无需改 Cargo.lock；其他组件的固定开发依赖仍保持各自原验收提交。

独立主机 WS63/BS21/WS53 为 343/220/427，三芯片目标 Clippy、无 Embassy 依赖检查、
远端 Git 外部消费者主机及 RISC-V 构建通过。父仓新增 QEMU 回归入口并接入模型 CI；
本地五次 async 定时器等待通过，远端此新增 CI 尚未验收。主机测试 Clippy 的 16 项
旧告警保持开放。见 [报告](evidence/hal-irq-rendezvous-2026-11-01/report.json)。
无硬件操作；本批不解决取消、DMA 停机、WFI 或发布阻塞，不覆盖下方已有未验收项。

## 2026-11-01 时间职责整改增量（优先于下方历史盘点）

| 仓库 | 当前分支 / 提交 | 推送与验证 |
| --- | --- | --- |
| hisi-hal | `refactor/embassy-extraction` / `5895b95` | 已推送，[PR #17](https://github.com/hispark-rs/hisi-hal/pull/17)；三芯片独立集成及无 Embassy 依赖检查通过 |
| hisi-rtos | `refactor/embassy-time-profiles` / `9b83efa` | 已推送，[PR #1](https://github.com/hispark-rs/hisi-rtos/pull/1)；基础 86、共享 91、纯模式每芯片 14 测试及三芯片 Clippy 通过 |
| hisi-riscv-qemu | `fix/tcxo-time-contract` / `e60f95c` | 已推送，[PR #25](https://github.com/hispark-rs/hisi-riscv-qemu/pull/25)；TCXO 双芯片测试、11 项旧 qtest、WS53 TCXO 回归通过 |
| ws63-examples | `refactor/rtos-embassy-profile` / `f52900d` | 已推送，[PR #7](https://github.com/hispark-rs/ws63-examples/pull/7)；两示例链接，纯计时 QEMU 通过 |
| hisi-rs-template | `refactor/rtos-embassy-profile` / `a843ec2` | 已推送，[PR #6](https://github.com/hispark-rs/hisi-rs-template/pull/6)；WS63/BS21 生成项目在父仓外完整构建，显式固定远端开发图 |
| hisi-crypto-ws63 | `chore/hal-time-integration` / `560b713` | 已推送，[PR #1](https://github.com/hispark-rs/hisi-crypto-ws63/pull/1)；统一 HAL 精确依赖，19 项独立主机测试与目标检查通过 |
| hisi-rf-ws63 | `chore/hal-time-integration` / `b7f742b` | 已推送，[PR #1](https://github.com/hispark-rs/hisi-rf-ws63/pull/1)；统一 HAL 精确依赖，26 项独立主机测试与目标检查通过 |

父仓同步上述子模块与锁文件，STA 工作区和 SoftAP 检查通过，只有一份 HAL。
纯 Embassy 与线程 profile 分开构建，没有新增 `hisi-embassy` 仓库或子模块。
原型保留在父工程外 `validation/embassy-extraction/superseded-hisi-embassy/`。

`ws53-pac` 发布已关闭 403 阻塞，原标签第 3 次执行成功；其他 crate 无发布授权。
新 HAL、runtime WS53、RF driver 及新 RTOS profile 仍未发布；开发覆盖不是 registry 发布验收。
实板未操作。共享共存原 HIL 阈值在迁移前后 QEMU 均失败，保持未关闭；
完整边界和报告见 [Embassy 迁移清单](embassy-extraction.md)。

## 先前盘点（保留历史，不代表上述仓库的当前 HEAD）


**状态：** WS53 适配的配套工作，持续维护。

状态记录：2026-11-01。本清单记录整合状态，不替代具体功能的验收证据。

| 仓库 | 本地状态与集成目标 | 远端与待办 |
| --- | --- | --- |
| hisi-registers | `feat/ws53-agpio-clock`，`bc02ca0`；包含 IPC、SPI 及 AGPIO 时钟等分项历史 | 同名远端分支，[草稿 PR #1](https://github.com/hispark-rs/hisi-registers/pull/1)；独立 SDIO、SFC 证据已分别推送并建 [PR #2](https://github.com/hispark-rs/hisi-registers/pull/2)、[PR #3](https://github.com/hispark-rs/hisi-registers/pull/3)，尚未合入 |
| ws63-svd / ws63-pac | `fix/ws63-tcxo-two-word`：SVD `9f11980`、PAC `6324bcf`；新增完整 TCXO 字段并保留旧访问器 | [SVD 草稿 PR #2](https://github.com/hispark-rs/ws63-svd/pull/2)、[PAC 草稿 PR #2](https://github.com/hispark-rs/ws63-pac/pull/2)；均尚未合并或发布，待实板验证 |
| ws53-pac | `feat/ws53-agpio-clock`，`cd5db21`；生成来源固定到 `bc02ca0` | [项目远端](https://github.com/hispark-rs/ws53-pac)同名分支；独立打包、远端 CI 和发布 dry run 通过。用户授权发布 `v0.1.0-alpha.1`，2026-11-01 原标签发布任务第 3 次运行成功；registry 包校验及全新缓存主机/RISC-V 外部消费者通过，见 evidence/ws53-pac-registry-2026-11-01/report.json |
| hisi-riscv-rt | `codex/ws53-exceptions`，`eb3e2d5`；RAM 函数审计绑定快照和实际加载字节 | 同名远端分支，[草稿 PR #7](https://github.com/hispark-rs/hisi-riscv-rt/pull/7)；独立依赖 PAC 的外部构建待验证 |
| hisi-ws53-coex | `feat/ws53-control-core-protocol`，`5a8971a`；协议、传输、服务分层，队列损坏锁定会话失效 | [项目远端](https://github.com/hispark-rs/hisi-ws53-coex)同名分支；空缓存外部消费者、独立 CI 及轮询端点 QEMU 通过，未发布，真实对端与实板尚未验收 |
| hisi-hal | `feat/ws53-mcu-integration`，`f14aabc`；共用 I2C 事务、SPI 帧和分频计算 | 同名远端分支，[草稿 PR #16](https://github.com/hispark-rs/hisi-hal/pull/16)；`ws53-pac 0.1.0-alpha.1` 已发布；HAL 新版本尚未发布，旧芯片 SPI 改动的实板验证未完成 |
| hisi-riscv-qemu | `ws53-agpio-clock`，`756662c`；包含多个模型集成分支 | 同名远端分支，[草稿 PR #23](https://github.com/hispark-rs/hisi-riscv-qemu/pull/23)；继续核查各模型的实板边界 |
| hisi-fwpkg | `feat/ws53-application-images`，`217447f` | 同名远端分支，[草稿 PR #4](https://github.com/hispark-rs/hisi-fwpkg/pull/4)；为 hisiflash 的固定 Git 依赖 |
| hisiflash | `feat/ws53-serial-flash`，`13a2bb2`；含串口回读文件头重传和恢复保护范围检查 | 同名远端分支，[草稿 PR #26](https://github.com/hispark-rs/hisiflash/pull/26)；本地工作区测试及格式/Clippy 通过，新增路径尚未实板验证 |
| hisi-riscv-rs | `feat/ws53-integration`；PAC、HAL、运行时指针、示例及首轮 SPI Flash 恢复证据索引已分项提交 | 同名远端分支，[草稿 PR #58](https://github.com/hispark-rs/hisi-riscv-rs/pull/58)；余下脚本与证据待分项整理 |

寄存器来源 → PAC → 运行时/HAL → 示例与测试；QEMU 模型独立版本化并由测试
固定其来源。`hisiflash` 固定远端 `hisi-fwpkg` 的 `217447f`。目前尚不能声称
整个适配已完成：脚本与证据尚未全部提交，外部消费者构建仍在检查。
WS53 PAC 已于 2026-11-01 发布到 crates.io；HAL 与 Embassy 新版本仍为开发集成。干净检出已成功初始化全部递归子模块；
空 Cargo 缓存下 WS53 HAL 主机测试、WS53/WS63/BS21 RISC-V 检查和 WS53 示例
构建通过。两个诊断示例分别显式启用 `qemu-only`，不能把它们视为硬件验收。

## 材料保护

- 完整本地盘点位于本工程构建目录 `target/ws53-workspace-audit-20261029/`：
  仓库状态、分支和未推送历史、递归子模块状态及工作树列表；它是本地快照，
  后续变更需重新采集。部分证据目录含可重建的 QEMU 构建文件，不能将整个
  618 MB 目录不加审查地作为精简证据提交，也不能删除已封存的原始证据。
- 原始波形、厂商固件和完整 Flash 备份存于工作区同级的 `hardware/ws53/`，
  当前约 67 GB，不纳入 Git。首次 SPI23 Flash 前 4 MiB 备份为
  `hardware/ws53/vddio-restored/spi23-cli-flash01/transfer/before-full.bin`，
  SHA-256 `969ab8499de4afd155fca6a61055b78d8b1398abaf6193de161418e6e423219d`；
  用于恢复原先镜像。其他私有文件路径和校验值见
  `evidence/ws53-spi23-first-flash-2026-10-28/private-files.json`。
- 构建缓存和临时日志位于 `target/`（约 125 GB）；其中仍有可能是唯一副本的
  测试脚本和快照，清理前必须逐项核对并转存必要源码。当前未删除工作树或数据。
- 原父仓内的协议组件及其本地构建目录完整转存到
  `target/ws53-coex-pre-submodule-20261029/`；与已推送的新组件源码逐文件对照
  一致后才切换子模块。保留该副本直至确认不再被任务或证据引用。
- 独立示例目录的 `target/` 已由项目 `.gitignore` 排除；锁文件与源码没有被
  排除。`hisiflash` 的三个已跟踪 `.vscode` 配置受本机全局忽略规则匹配，但
  仍处于版本管理中，未删除或通过忽略规则改变它们。

## 硬件与验收边界

上次 SPI23 Flash 首轮测试失败后，原始镜像与 NV 已回读校验恢复；后续 AGPIO
时钟修正已完成 RAM 实板与 QEMU 测试，但新的 Flash 实板测试仍未完成。真实断电
冷启动仍未验收。2026-10-29 盘点时串口设备未出现，未发起新的硬件操作。
历史记录中仅能证明复位的结果不能标记为冷启动通过。

### UART 回归后续：模型 CI 的旧锁文件

远端 [run 41288759118](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/41288759118)
在 UART 步骤前因 HASH 示例的旧锁失败。已检查 workflow 的十三个独立解析范围，
七个 WS53 示例仅同步 HAL alpha.9→alpha.10，第三方锁定版本不变；全部重新通过
`cargo metadata --locked --offline`。继续保留严格锁定，整条远端 workflow 单独验收。
[审查](evidence/uart-model-repro-2026-11-01/lock-audit.json)。
