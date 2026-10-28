# WS53 多仓库维护状态

状态记录：2026-10-29。本清单记录整合状态，不替代具体功能的验收证据。

| 仓库 | 本地状态与集成目标 | 远端与待办 |
| --- | --- | --- |
| hisi-registers | `feat/ws53-agpio-clock`，`bc02ca0`；包含 IPC、SPI 及 AGPIO 时钟等分项历史 | 同名远端分支，[草稿 PR #1](https://github.com/hispark-rs/hisi-registers/pull/1)；独立 SDIO、SFC 证据已分别推送并建 [PR #2](https://github.com/hispark-rs/hisi-registers/pull/2)、[PR #3](https://github.com/hispark-rs/hisi-registers/pull/3)，尚未合入 |
| ws53-pac | `feat/ws53-agpio-clock`，`cd5db21`；生成来源固定到 `bc02ca0` | [项目远端](https://github.com/hispark-rs/ws53-pac)同名分支；独立打包、远端 CI 和发布 dry run 通过。用户授权发布 `v0.1.0-alpha.1`，正式上传因 crates.io 凭据返回 403，尚未发布 |
| hisi-riscv-rt | `codex/ws53-exceptions`，`03fbef8` | 同名远端分支，[草稿 PR #7](https://github.com/hispark-rs/hisi-riscv-rt/pull/7)；独立依赖 PAC 的外部构建待验证 |
| hisi-ws53-coex | `feat/ws53-control-core-protocol`，`615fcf5`；协议、传输、服务已分层，独立锁文件与 CI | [项目远端](https://github.com/hispark-rs/hisi-ws53-coex)同名分支；空缓存独立测试通过，未发布，真实对端及新轮询端点尚未验收 |
| hisi-hal | `feat/ws53-mcu-integration`，`6fa9738`；IPC 仅保留硬件驱动，三芯片共用定时器换算 | 同名远端分支，[草稿 PR #16](https://github.com/hispark-rs/hisi-hal/pull/16)；子仓 CI 增加固定远端依赖的预发布测试，正式 registry 与外部消费者验收仍受未发布依赖阻塞 |
| hisi-riscv-qemu | `ws53-agpio-clock`，`756662c`；包含多个模型集成分支 | 同名远端分支，[草稿 PR #23](https://github.com/hispark-rs/hisi-riscv-qemu/pull/23)；继续核查各模型的实板边界 |
| hisi-fwpkg | `feat/ws53-application-images`，`217447f` | 同名远端分支，[草稿 PR #4](https://github.com/hispark-rs/hisi-fwpkg/pull/4)；为 hisiflash 的固定 Git 依赖 |
| hisiflash | `feat/ws53-serial-flash`，`916cf06`；库和 CLI 分两次提交 | 同名远端分支，[草稿 PR #26](https://github.com/hispark-rs/hisiflash/pull/26)；空缓存干净检出 394 项测试及格式/Clippy 通过 |
| hisi-riscv-rs | `feat/ws53-integration`；PAC、HAL、运行时指针、示例及首轮 SPI Flash 恢复证据索引已分项提交 | 同名远端分支，[草稿 PR #58](https://github.com/hispark-rs/hisi-riscv-rs/pull/58)；余下脚本与证据待分项整理 |

寄存器来源 → PAC → 运行时/HAL → 示例与测试；QEMU 模型独立版本化并由测试
固定其来源。`hisiflash` 固定远端 `hisi-fwpkg` 的 `217447f`。目前尚不能声称
整个适配已完成：脚本与证据尚未全部提交，外部消费者构建仍在检查。HAL/运行时
依赖的 WS53 PAC 尚未发布到 crates.io。干净检出已成功初始化全部递归子模块；
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
