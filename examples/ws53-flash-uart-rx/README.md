# WS53 Flash UART2 异步接收开发夹具

本目录为开发夹具，不提供稳定 API 或实板验收承诺；当前证据与未关闭项目见
[WS53 实施记录](../../docs/plan/ws53-adaptation.md)。
独立 workspace 保留旧候选镜像及证据，不与其他芯片合并 feature。

依赖已观察的厂商 FlashBoot MCU_MODE_1 内存和 32 MHz UART/TIMER 时钟交接，
控制 CPU 保持复位。它不是任意复位状态下的完整时钟初始化驱动。
程序自主等待 30 秒后，在 L0 reset 期间配置 AGPIO1/2 的 mode1 和上拉，
保留原驱动/Schmitt 设置，释放后经 HAL 重置 FIFO/115200 8N1，并发出 SDK
已证实的完整字路由命令1。没有调试器 RAM 加载或寄存器预初始化。

串口 peer 在以下标记后发送原始字节（不添加换行）：

| 标记 | 字节 |
| --- | --- |
| `WS53 UART2 ASYNC ARM 0` | `00 ff 51` |
| `WS53 UART2 ASYNC ARM 1` | `a5 5a 53` |
| `WS53 UART2 ASYNC CANCEL` | `3c` |

独立实现已有七字节真实 IRQ42/waker、取消及排队字节保留场景；等待缺失 IRQ 或
peer 使用持续运行的 TIMER1 两秒期限，时钟停止/计数跳变亦拒绝。正常结束清理通知并停止
TIMER1，UART pad 保持本夹具配置，不宣称恢复了厂商上下文。Flash 写入前仍须
完成同 ELF 模型资格、独立镜像规划、现场备份及恢复检查；禁止直接使用未经
检查的地址烧录。逻辑分析仪的串口测点尚须重新定位，不能以日志替代电气验收。
