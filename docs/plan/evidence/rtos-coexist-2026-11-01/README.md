# RTOS/Embassy 共存：WS63 软件通知模型

本轮不改生产 HAL/RTOS，只修复 QEMU 软件通知源并增强诊断。原程序在旧模型为
native=7、embassy=7、timer IRQ=14、switch=14；固定 icount 参数也未使原门槛通过。
模型将 0x44000150..15c 当普通 shadow 数值，未投递 SOFT_INT0；线程等待切换时
只能靠后续 TIMER IRQ 推进。时间线与新增寄存器反例均支持此因果链。

QEMU `d61c4a3` 将四路软件源接到 cause36..39。同一个原 ELF（SHA 见
`fixed-original.json`）变为 native=17、embassy=10，通过原门槛；增强示例进一步要求
软件中断非零。默认及时间线分开构建和运行，时间线必须有全部事件且 `lost=0`。
最初 192 项诊断缓冲区溢出 4 条，保留该报告；扩至有界 256 项并重新验证，没有
把截断时间线当完整证据。计数变化不是物理吞吐量或时钟精度证明。

## 边界

- 地址、位和访问形式来自固定 WS63 PAC `6324bcf` 的 SYS_CTL1；IRQ 映射对照 SDK
  `drivers/chips/ws63/include/chip_core_irq.h`。SDK 文件哈希记录于报告，无可用 SDK Git
  元数据时不编造提交。屏蔽保留 pending 是模型明确语义，实板仍待验证。
- 共享 profile 是微秒 tick ABI、毫秒分辨率；纯 Embassy profile 的硬件时间契约未改变。
- 不修复或掩盖尚未关闭的 WFI/software-wake 竞态，不把软件 IRQ 计数当其证明。
- 无烧录、复位、调试探针、串口或逻辑分析仪操作；新模型结果不继承历史实板结论。
- 两个子仓已先推送，父仓再固定源码和子模块；没有新 crate 发布或锁文件依赖变更。

## 构建与材料保护

原始报告、日志和构建材料位于工作区同级
`validation/rtos-coexist-2026-11-01/`，公开精简报告与校验索引在本目录。
最初误用旧 `validation/ws63-uart-2026-11-01/merged-fresh/qemu` 构建目录尝试增量
构建，因上游 GCC 警告失败；旧 binary 未被替换，校验后复制为本轮 `pre-softint-qemu`。
旧 `hw/riscv/ws63.c` 已还原到 `7c88dac`，但 configure 状态曾重建，**不再声称旧构建树
能通过原回执验证**。原日志/回执完整保留，没有重封存或伪造回执。

随后使用全新 `model/qemu` 编译；首次联网子依赖获取失败后再继续，两个日志均保留。
它用于初步定位，不冒充标准构建回执。最终另外使用父仓固定源码构建器在全新
`pinned-build/` 生成并校验回执，来源为已推送的模型提交。
Rust `target` 是可重建输出：原 ELF 的两模型对比完成后，该路径被更新示例重建；
各次 JSON 保留当时哈希，不能将当前同路径文件当作旧 ELF。最终产物另外保存并索引。

复现：父仓固定模型构建器与 `WS53 model contracts` workflow 的
“Check RTOS and Embassy sharing one timer”步骤；寄存器测试属于 QEMU 子仓，
跨仓 Rust 执行结果属于父仓。不要把一次本地通过表述为整个远端 CI 已通过。
