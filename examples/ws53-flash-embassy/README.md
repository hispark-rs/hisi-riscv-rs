# WS53 Flash Embassy 时间驱动夹具

独立 workspace，沿用限定的厂商 FlashBoot 32 MHz、模式1内存、控制CPU保持复位的
交接配置。复用已具名RAM测试的Timer/Ticker、三个初始化拒绝、取消、16槽溢出和
重入waker场景，再通过真实Embassy Timer在MGPIO17/DSLogic D5输出
1/2/4/5/10/20/8ms脉冲，逐段记录实际单调时间。只连接分析仪输入。

上电后自主等待30秒，不等待宿主命令，不加载RAM或修改Flash/eFuse。
TIMER/TCXO由时间驱动永久持有；正常收尾队列为空、TIMER1本地IRQ关闭且无pending，
GPIO17释放并恢复pad配置。普通Timer取消后保留的waker按期限自然到期，不伪称
Timer::drop会立即注销。测试不涉及低功耗、切换时钟、生产资源协商或完整BootROM模型。

当前为开发夹具，尚未取得此ELF的模型或实板资格；实际进度见主工程实施记录。
普通编译只依赖已生成PAC，不读取SDK或在线生成寄存器。
