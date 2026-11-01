# SPI DMA 首轮完成与定时等待

此前模型 CI 在 `1/0/2/t/N` 输出 armed 后超时。审查发现 async 夹具把正常成功与
“至少一次 TIMER0 IRQ”绑定；但驱动在第一次 poll 前已经完成时应直接返回。
主机读取 armed、检查寄存器再发 G 的延迟会决定是否经过等待，不能成为通过条件。

保留原断言、仅增加诊断的夹具在普通调度下通过 50 场景；延迟 G 后失败。
确定性反例进一步在发 G 前观察到 TX/RX remaining=0、pending writes=0、33 帧、
TX/RX FIFO=0，随后旧夹具仍打印 zero timer IRQ 并 panic。见
[原断言反例](controlled-negative.log)。这证实测试假设错误，可解释 CI 的同类症状；
原 CI 没有 panic 源码位置，不能仅据外部超时判定全部失败均为同一原因。

修正保留故障超时必须触发真实 IRQ 的断言，并增加 poll/Pending/IRQ 观察。
正常首轮完成不要求 IRQ；新增确定性场景反而严格要求一次 poll、零 Pending、零 IRQ，
不得为测试人为启动无用定时器。内存 guard、数据、请求路由、停止、取消与归属检查不变。
观察数组是 CPU 写入的诊断数据，主机只读，不是模拟完成寄存器。

本地结果：8/32 位 × poll/async 的原 200 场景通过；两种宽度各 16 个首轮完成场景
通过，共 232。HAL 子仓新增两个共享 timed_wait 契约测试，WS53 444 项主机测试与
测试 Clippy 通过；WS63 UART 公共测试八项通过。Clippy 同时指出旧 UART 测试的
手写 noop waker，现改为收到取消后唤醒即失败的 waker，仍验证 Arc 引用释放。
HAL 改动全部位于 cfg(test)，产品行为、公共 API、依赖版本与 QEMU 模型未变。

CI 新增两组首轮完成回归；远端整条流程另行观察，不以本地通过替代。
原始数据、ELF、诊断源和构建日志位于 `validation/spi-dma-progress-2026-11-01/`；
哈希及场景索引见 [report.json](report.json)。正常构建无需该路径。

本轮没有实板操作，不证明真实 SPI CS、时钟、FIFO 深度、DMA 排空或缓存一致性。
Flash、冷启动和既有硬件限制不因本次夹具修正而关闭。实验驱动仍未毕业为稳定 API。
