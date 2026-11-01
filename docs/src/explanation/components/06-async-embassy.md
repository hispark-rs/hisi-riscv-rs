# 异步外设与 Embassy 适配

HAL 的 `async` feature 提供执行器无关的 `embedded-hal-async` / `embedded-io-async`
及外设 Future。原有稳定性门控继续生效：SPI/I2C 的 blocking-backed async trait
可使用 `async`；中断唤醒、取消等实验能力仍需 `unstable`。选择 async 不会拉入 Embassy。

## 组件边界

- `hisi-hal`：计数器采样、定时器配置、IRQ 状态/确认以及硬件独占资源。
- `hisi-rtos`：`embassy-time-driver` 全局实现，公共调度核心、有界队列、
  单调时间缓存、截止时间分段、闹钟重编程及唤醒。
- 应用/平台：建立时钟与资源归属，调用 `unsafe hisi_rtos::embassy::init(timer, tcxo)`，
  将该芯片 `hisi_rtos::embassy::ALARM_IRQ` 路由到 `hisi_rtos::embassy::on_alarm_interrupt()`，配置全局中断。
- `hisi-riscv-rt`：入口和中断基础设施；不包含 Embassy 调度策略。
- 默认 RTOS profile 的 `embassy`：协调线程睡眠、时间片及预算，共享 SchedulerPort。
- `embassy-only`：明确的纯 Embassy profile，编译时不包含线程 scheduler/context/storage。
  两种 profile 共用队列实现和唯一驱动注册；在不同 Cargo invocation 中构建。

纯 profile 无直接 PAC 依赖，不接受地址、IRQ 数字或 IP 版本作为用户输入。WS53 使用
TIMER1/IRQ54；WS63 使用 TIMER0/IRQ26；BS2X 使用 TIMER0/IRQ53。驱动永久占用整个
TIMER 资源和 TCXO 采样窗口，不能再把同一块资源交给 DMA 等待、普通 delay 或 RTOS。

## 分辨率与模式

RTOS 共享模式继续使用注入的毫秒时钟：即使 tick ABI 为 1 MHz，也只有 1 ms 分辨率，
亚毫秒截止时间向上舍入。纯 profile 的 TCXO 路径保留配置上的微秒粒度，不能用它为
RTOS 模式宣称微秒精度。两者 `embassy::RESOLUTION_US` 分别为 1000 和 1，均不是
物理计时精度证明。共享模式在唤醒回调前领取重编程 generation，回调外置后仍检测旧计划。

## 纯 profile 的调度和安全语义

纯 profile 的三种芯片共用 `embassy/standalone.rs`、`embassy/math.rs`；`embassy/backend.rs` 仅适配 HAL 的采样
结果和硬件配置。初始化之前 now=0，不访问硬件；失败返回两个资源，采样超时可能保留
未完成请求。成功后不提供 deinit，不自动换频、复位计数器或进入停钟睡眠。

配置频率是平台前提，不是测量结果。平台须保证单应用 hart、无控制核/DMA/NMI 争用、
稳定时钟和 epoch。TCXO 未就绪时返回最后时间并缩短重试闹钟；读数倒退（包括无法区分
的计数器完整回绕/复位）报告 epoch 错误，不伪造时间前进。32 位闹钟可分段覆盖远期
64 位截止时间。实际编程延迟可能使唤醒晚于截止时间，不能保证硬实时精度。

暂保留 WS53 已有 16 项队列：上游 queue-utils 在队列借用期间调用 Waker，不能直接
满足已有重入契约。克隆、丢弃、唤醒回调均在状态借用和临界区外执行。容量满时唤醒
被替换任务重新轮询；取消等待可能留下有界的旧唤醒，绝不表示撤回已提交操作。

## 迁移

删除 HAL 的实验 `embassy` feature 与 `hisi_hal::embassy` 路径；加入独立依赖：

```toml
hisi-hal = { version = "0.7.0-alpha.10", features = ["chip-ws63", "unstable"] }
hisi-rtos = { version = "0.1.0-alpha.25", features = ["chip-ws63", "embassy-only", "unstable"] }
embassy-time = { version = "0.5", features = ["tick-hz-1_000_000"] }
```

以上 API 属于开发分支，当前标注的 RTOS 版本尚未包含已发布的 `embassy-only` feature；
正式发布需要单独版本规划和授权，不能将开发集成通过当作 registry 可用。
原 WS53 `init/status/on_alarm_interrupt` 改为 `hisi_rtos::embassy`；WS63/BS2X 新增
显式 init，不再通过 HAL feature 隐式访问裸寄存器。旧芯片普通已发布 HAL API 保留。

## 测试归属

组件子仓维护主机调度、重入、边界、错误返回、feature/单提供者冲突和独立消费者测试。
HAL 子仓维护不依赖 Embassy 的硬件及 async 测试。父仓维护真实 runtime/HAL/适配层
组合的 QEMU 场景和实板测试。迁移状态、发布阻塞与本次验收结果统一见
[迁移清单](https://github.com/hispark-rs/hisi-riscv-rs/blob/feat/ws53-integration/docs/plan/embassy-extraction.md)。历史实板证据不自动适用于新行为。
