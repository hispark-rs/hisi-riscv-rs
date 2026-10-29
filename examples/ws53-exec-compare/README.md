# WS53 Flash / ITCM 相同机器码对照

独立实验 workspace。通过厂商 FlashBoot 自主启动，等待30秒后，在Flash和ITCM
执行完全相同、32字节对齐的12字节循环，以AB/BA顺序测三种长度。TIMER1、64位
mcycle和MGPIO17/D5分别记录时间；UART2输出256字状态。本程序不读不支持的
CSR0x320，不改变缓存、PMP、CPU时钟或Flash控制器，不启动控制核。

ITCM使用链接器已保留的bootstrap栈最底部12字节。只允许main已切换DTCM栈、
正常向量已安装、cache=6/0、控制核保持复位、MDMA无活动及固定MCU模式时借用。
保存原字节，复制后逐字核对并fence.i，实验结束逐字恢复并fence.i。这是受控
测试的unsafe局部借用，不是通用可执行RAM分配API，也不授予DMA/缓存维护资格。

正常结束释放GPIO、恢复pad、停止TIMER、还原ITCM并flush UART。故障有界停止，
不承诺自动清理；需要通过已经验证的复位/原厂恢复流程处理。只能与其自身构建、
模型、烧录计划和恢复材料配套，不能继承其他Flash镜像的验收结论。

普通Cargo构建使用生成好的PAC，不需要SDK/Python。程序能编译或在模型中运行
不等于实板、Flash冷启动或精确定时验收，实际结果见主工程WS53实施记录。
