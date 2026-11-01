# WS53 SPI DMA 模型夹具

仅用于 QEMU，禁止加载到实板。应用临时接管两条 DMA 通道、SPI/QSPI、缓冲区和
定时器；虚拟对端提供 XOR 数据。退出前的外部排空检查不能作为真实总线/缓存证明。

`poll-smoke` 轮询实际驱动，`async-smoke` 使用 HAL 的执行器无关 Future 和 TIMER0
IRQ；`width32` 选择 32 位元素。两种等待 profile 分别构建，勿合并 feature 验收。
运行 `scripts/test-ws53-spi-dma.py --qemu <binary> --elf <elf>` 检查两实例、八通道
及读写错误、超时、取消、遗忘和不可启动状态，共 50 场景。

async 的两个合法完成路径分别检查：

- 普通/故障回归保留所有数据、地址、请求关闭和缓冲区归属断言；阻塞超时场景必须
  有真实定时器中断。发生 Pending 的等待必须观察到 timer IRQ。
- 添加 `--complete-before-wait` 运行 16 个正常路由。主机在发送 G 前先读取 DMA
  剩余计数、posted write、串行帧数及 FIFO，确认模型中的传输已经完成。随后必须
  首次 poll 返回、Pending 次数为零、timer IRQ 为零。此参数拒绝非 async ELF。

`WS53_SPI_DMA_WAIT_OBSERVATION` 仅记录实际 poll/Pending/IRQ 计数，主机在 CPU
暂停后读取；它既不触发也不伪造完成。G 是测试协调信号，不是产品 DMA API。
主机处理 armed 日志的速度不能决定“正常完成必须触发 IRQ”。旧夹具错误地要求该
条件，可能在首轮已经完成时 panic，外部表现为等不到 retained 的超时。

这些检查不证明物理 CS、精确时序、缓存一致性、停止后的总线排空或实板恢复。
