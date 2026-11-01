# UART 共用接收生命周期（2026-11-01）

[报告](report.json)记录两批 HAL 提交、独立主机/目标检查、远端消费者及 WS53 本地模型回放。
共用实现不是独立队列或 executor；Embassy 时间职责继续属于 `hisi-rtos`。

外部消费者在 `consumer/`，可脱离父仓复制该目录，依次运行：

```sh
cargo +nightly-2026-07-09 build --locked -Zbuild-std=core,alloc --target riscv32imfc-unknown-none-elf --features chip-ws63
cargo +nightly-2026-07-09 build --locked -Zbuild-std=core,alloc --target riscv32imfc-unknown-none-elf --features chip-ws53
cargo +nightly-2026-07-09 build --locked -Zbuild-std=core,alloc --target riscv32imfc-unknown-none-elf --features chip-bs21
```

这是远端开发提交及 registry PAC 的库消费者检查；未宣称发布版本、固件链接或实板通过。
HAL 自身的独立测试在其子仓。公共等待取消及竞态测试不依赖 PAC/模型。

WS53 模型覆盖正常两轮、路由阻断两轮后恢复一轮、取消输入超时两轮后恢复一轮。
使用已有 UART 模型 `fd0258f…`，未改模型以使新驱动通过。初次选用的 TIMER/GPIO
模型不含 UART 路由设备，失败结果仍在原始目录中。UART 模型模拟数字 pad 8N1，
不证明电气精度、真正 Flash 启动链或断电恢复。

**复现缺口：**本次复用的旧构建/运行辅助脚本仍为父仓未跟踪成果，报告记录其哈希。
因此 WS53 结果只计本地回放，不计干净检出 CI 验收；后续需单独整理其传递依赖，
不能将本轮外部消费者成功扩大为整套模型流程远端可复现。WS63 接收模型缺少 FIFO
计数及 IRQ 语义，尚未验收。本轮没有连接、复位、烧录或采集实板。
