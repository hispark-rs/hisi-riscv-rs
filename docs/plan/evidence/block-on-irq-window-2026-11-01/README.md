# HAL IRQ→WFI 窗口定点复现（2026-11-01）

**结论：问题已确定性复现，尚未修复。** 真实 HAL `f39aef7` 在最后一次
`BLOCK_ON_NOTIFIED.swap(false)` 之后收到并处理 IRQ，仍会执行后续 WFI。
已记录的软件通知不能唤醒已经睡眠的 CPU；再来一次中断才能继续 poll。

没有新增 HAL 测试 hook 或复制生产等待循环。父仓现有最小示例新增 `irq-race`
profile，显式持有 `SoftwareInterrupt0` / SYS_CTL1，ISR 通过生产 HAL 清源并
调用 `IrqSignal::signal`。不使用周期 TIMER、RTOS 调度器或 Embassy。

## 定点观察

同一 ELF、同一固定 QEMU，GDB 执行断点只控制暂停点；唯一 host MMIO 写入是模型
`SOFT_INT0 SET`。加载后及完成时核对执行函数字节哈希不变。执行器 WFI 地址从
具名函数的实际 RISC-V 指令边界解析，不硬编码指令地址；默认 profile 被验证器拒绝。

| 时点 | 轮询次数 | IRQ 次数 | 通知位 | 观察 |
| --- | ---: | ---: | ---: | --- |
| 首次 Pending、实际 WFI 前 | 1 | 0 | 0 | MIE 开启 |
| 注入 IRQ、ISR 返回同一 WFI 前 | 1 | 1 | 1 | IRQ 源已清零 |
| 恢复执行并观察 100 ms | 1 | 1 | 1 | PC 为 WFI 后继地址，没有返回 |
| 再注入一次 IRQ | 2 | 2 | 1 | 到达 block_on 返回后的具名函数入口 |

对照：先允许执行 WFI，再注入第一次 IRQ，只需一次 IRQ、两次 poll 就返回。
因此失败不是 IRQ 路由未建立或 Future 无法 Ready。观察期限不是物理时延证明，
PC 是调试观察，不把它扩展为所有芯片的硬件停机状态证明。完成断点在回调入口，
此时回调中的 done 标志尚未执行；报告明确区别“block_on 已返回”和“回调已执行”。

原默认软件唤醒场景回归通过；新 profile Clippy 通过。竞态场景默认退出 1，
`--expect-race` 只承认诊断完成，始终保留 `correctness_passed=false`，不加入
“所有驱动正确”的通过门槛。脚本、模型、ELF 与原始日志见 [report.json](report.json)。

## 边界与后续

- `IrqSignal` 记录通知正确；问题在执行器的通知检查到休眠交接，不推断所有外设
  Future 都有同一缺陷。
- 平台/执行器负责原子休眠交接，HAL 不增加时间队列、周期中断或隐藏硬件所有者。
- 不能不加论证地违反项目现有“关中断区间不得等待”约定；若采用专门架构休眠原语，
  应先明确例外范围、MIE/本地 IRQ 恢复语义、唯一所有者及跨核限制。
- QEMU 对 LOCIEN/LOCIPD 的 WFI 唤醒是显式模型行为，不是 HiSilicon 芯片证明。
- 未操作实板，未烧录、复位或改动调试连接；RAM/Flash/冷启动验收不变。
