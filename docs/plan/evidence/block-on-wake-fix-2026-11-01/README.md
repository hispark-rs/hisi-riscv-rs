# HAL `block_on` 软件唤醒：部分修复

修复前 HAL `3a1288d`，修复后 `cc1be08`；父仓指向后续仅文档澄清的 `d5ca0ba`。
变化只针对 WS63 实验入口的无操作 Waker：
记录/合并唤醒，消费通知后再选择原 idle。没有增加硬件所有者、队列或忙轮询。
**通知检查之后、WFI 之前的 IRQ 窗口仍未解决**，本证据不是完整执行器验收。

| 验证层 | 范围 |
| --- | --- |
| HAL 子仓 | 六个新生命周期测试；独立副本 WS53/WS63/BS21 的 445/372/220 项主机测试及文档测试；三芯片目标 Clippy |
| 父仓 | 无 TIMER/SOFT IRQ 的实际 Rust 代码，重复软件唤醒、遗留 Waker、MIE 打开/关闭时保持不变；三路 TIMER 原生命周期回归 |
| 外部消费者 | 相同 source/build.rs，分别固定修复前后 HAL Git 提交与同一 runtime Git 提交，registry PAC；脱离父仓构建和运行 |
| 实板 | 本轮未连接或操作；不继承旧 RAM/Flash/冷启动证据 |

示例关闭 MIE 时每次 Pending 都有软件通知，**没有**测试或允许关中断 WFI。
Waker 不包含栈指针，完成后仍有效；后续调用容忍伪唤醒。嵌套/并发被拒绝，
展开释放声明；这不声称单核 critical-section 能同步多个控制核。

目标 Clippy 和示例 Clippy 通过。额外 WS63 `cargo clippy --tests -D warnings` 在旧、
新源码均报 16 个既有问题（GPIO/DMA/I2S/KM/PWM/safety/tsensor）；原日志保留，
未放宽全局 lint 或把这些测试判为通过。新增测试故意分别覆盖 consuming wake 与
wake_by_ref，只在这条测试上说明 `waker_clone_wake` 的例外。

公开消费者配置与锁文件在 `consumer/`；无本机绝对依赖路径。使用已有 Cargo
下载缓存，**不宣称空缓存验证**。固定远端开发依赖不等同 registry-only 发布验证；
HAL 新版本与 WS53 runtime 等发布阻塞继续保留，没有自动发布任何其他 crate。

原始报告、构建日志与二进制存于工作区同级
`validation/block-on-wake-fix-2026-11-01/`，哈希索引见 `report.json`。
保留此前 `block-on-wake-2026-11-01` 反例，未改写历史失败报告。
