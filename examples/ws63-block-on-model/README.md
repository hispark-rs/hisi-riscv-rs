# HAL 单 Future 等待诊断

默认 profile 在普通与全局 IRQ 禁用状态各验证三次自唤醒、重复/消费唤醒、返回后
保留 Waker、后续调用中的伪唤醒及 MIE 保持，不使用 TIMER/SOFT IRQ。禁用 IRQ
时不会进入 WFI，不测试关中断休眠。
`irq-race` 是独立 QEMU 诊断：显式消费 SYS_CTL1 并路由 SOFT_INT0，Future 使用真实 HAL
`IrqSignal` 和 `block_on`；没有复制等待实现或加测试 hook 到 HAL。

构建 `--features irq-race` 后，用父仓 `scripts/test-block-on-irq-window.py` 分别选择
`--timing before-wfi` / `after-wfi`。脚本从 ELF 验证真实 WFI 指令和观察变量，用
执行断点与模型软件中断进行定点注入；不修改客体代码或 RAM，唯一写入是 IRQ SET。

before-wfi 当前能复现丢失休眠交接：ISR 已记唤醒并清源，随后却执行 WFI，必须再来
一次中断才返回。报告 `correctness_passed=false`，默认退出 1；显式 `--expect-race`
只让成功复现的诊断退出 0，不改变正确性结论。after-wfi 是一次中断正常完成的对照。
代码重排后 ELF 验证可能拒绝运行，需重新审查实际指令，不能随意替换固定地址。

这不是修复、真实芯片证明或精确时间测量。不能在主验收中把“复现成功”当作驱动正确。
该夹具不用于实板；不增加周期定时器或新的时间管理组件。
