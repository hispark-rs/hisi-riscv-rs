# UART 非阻塞发送模型夹具

独立 workspace，选择 `chip-ws63` 或 `chip-bs21`。UART0 输出阶段标记，UART1
连接虚拟对端。只用于 QEMU：测试 host 在启动前暂停 UART1，检测 FIFO 满时公共
`embedded_hal_nb::serial::Write` 是否立即返回 `WouldBlock`，再释放并检查顺序。
没有定时器、Embassy、RTOS 或 IRQ 调度依赖。

构建使用锁文件及 `-Zbuild-std=core,alloc --target riscv32imfc-unknown-none-elf
--release --features chip-ws63`；运行入口为父仓 `scripts/test-uart-nb-qemu.py`。
切换芯片后保存各自 ELF，避免用后一次构建覆盖前一次结果。

容量 1/16 是模型测试配置，不是物理 FIFO 深度。未验证真实波特率、硬件 TX IRQ、
旧芯片 async Write 取消或 flush 的芯片间等价性。本夹具未经实板验收，不作为硬件示例。
