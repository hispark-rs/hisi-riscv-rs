# HAL 最小执行器与平台休眠交接审查

状态：已复现软件唤醒失败，尚未修复。不操作实板，不新增执行器或时间组件。

HAL `c907bed` 的 `asynch::block_on` 使用无操作 Waker。Future 在 poll 内发出
唤醒并返回 Pending 后，执行器仍进入 WFI。独立 WS63 QEMU 反例只输出
WAKE ISSUED，没有 COMPLETE；同一 Future 两次显式 poll 的诊断对照能够完成。
没有启动 TIMER，因此不能依靠周期中断使这个错误偶然恢复。源码、远端锁文件、
观察命令和模型哈希见 [证据](evidence/block-on-wake-2026-11-01/README.md)。

另一个源码窗口是：完成 IRQ 在 poll 返回后、WFI 前已经执行并清除，随后 WFI
可能等待一个不会再来的中断。本轮没有对这个窗口做确定性 IRQ 注入，不能说
两个问题都已由本反例验证。翻译日志末尾包含 WFI，也不等于采样得到停机 PC。

## 职责与后续实现约束

- `IrqSignal` 与外设 Future 继续属于 HAL；它们不能负责执行器何时休眠。
- 应用/平台选择执行器。已有 `hisi-rtos::embassy` 提供时间契约，不因此再加一套
  时间队列、硬件所有者或新的 `hisi-embassy` 组件；纯 async 也不强制启动 RTOS。
- 不能在 `critical_section::with` 内 poll Future、执行 Waker 或等待。当前项目约定
  同样禁止在关中断区间等待，因此不能直接用“关中断包住 WFI”充当例行修复。
- 标准 RISC-V 的 WFI 恢复规则与全局 MIE 独立，但平台还须核对这些芯片的自定义
  本地 IRQ、单核/跨核唤醒和实际 port 行为，不能直接把标准规则当作芯片证明。
  依据：[RISC-V Machine ISA，WFI](https://docs.riscv.org/reference/isa/priv/machine.html)。
- 后续先明确平台级原子休眠交接与中断路由的所有者，再选择可复用机制；若需调整
  现有禁等待约定，应明确设计理由和适用边界，不暗中绕过。不能以无条件忙轮询、
  偷占周期定时器或另一套 HAL 全局调度器代替修复。
- 验证须包括 poll 内软件唤醒、poll 后 IRQ、空/重复唤醒、嵌套调用或显式拒绝、
  保留 Waker 的生命周期、原 MIE 状态，以及各芯片实际 IRQ/停止条件。

既有五次 TIMER QEMU 成功只说明常规路径完成，不能关闭这个反例。HAL async
驱动可由符合 Waker 契约的外部执行器驱动；本审查不把该最小执行器的问题推断为
全部外设 Future 有同样缺陷，也不自动给其他执行器的休眠策略背书。
