# RTOS 时间 profile 边界复核

本批不创建 `hisi-embassy`，不改变时间驱动生产行为，不操作硬件。
职责比较、迁移实施和历史模型结果仍由[迁移清单](../../embassy-extraction.md)维护。

RTOS `dad311b` 保留共享模式的启动与存储 API 正例，并为纯模式分别验证
`start_cooperative`、`start_with_port`、`SchedulerStorage` 不可导入（E0432）。
原测试在共享模式主动触发 `compile_error!`，不能验证共享入口仍兼容；现已替换。
三个独立反例避免一个入口仍缺失时掩盖另一个入口被误公开。

开发配置和子仓锁文件同步 HAL `f39aef7`，没有更改版本或 registry 发布要求。
子仓在父仓外独立检出后应用本批差异，以固定远端依赖运行完整时间 profile 脚本；
提交推送后，外部消费者再从远端获取精确 RTOS 提交。共享、共享 WS63、纯
WS53/WS63/BS21 五组 host 测试和 RISC-V 检查通过，使用已有下载缓存。
最终文档测试为共享正例 1 项、每芯片纯模式反例 3 项。

HAL 的三芯片 blocking/async 依赖边界检查通过。首次未带开发配置的检查因
沙箱 DNS 失败未完成；随后显式指定固定远端开发配置离线通过，未把前次失败隐藏。
完整命令输出、来源与校验值见 [report.json](report.json)。

共享模式仍是微秒 tick ABI、毫秒分辨率。纯模式不启动调度器，其微秒粒度是
配置契约，不能当作物理精度证明。初始化仍要求平台显式交接 TIMER/TCXO；
本批没有放宽 unsafe 前提，也没有增加第二个硬件所有者。

消费者复现：在 `consumer` 目录按锁文件执行 `cargo test --locked --target
x86_64-unknown-linux-gnu --features shared`，然后分别选择 `shared,chip-ws63`
或 `standalone,chip-ws53|chip-ws63|chip-bs21`（每次只选一个芯片）；目标检查用
`cargo check --locked -Zbuild-std=core,alloc --target riscv32imfc-unknown-none-elf`。
这些是开发集成验证，不能替代尚未发布依赖的 registry-only 发布验收。

HAL IRQ→WFI 竞态、迁移后的实板 RAM/Flash/冷启动复验及依赖发布仍未关闭。
本批没有重跑 QEMU；此前模型证据不自动升级为实板或物理时序结论。
