# WS53 Flash GPIO 异步实验

本例使用厂商 FlashBoot 启动应用核，复用已审核的 mode1 内存运行时。
UART2 和 TIMER1 的时钟来自对应 FlashBoot 交接；控制 CPU 保持复位。
MGPIO17（板端 MIO17、DSLogic D5）只连接分析仪输入，VDDIO 接板上 3V3。

启动后检查 data/bss，停止继承的 WDT，独立计时等待 30 秒，再执行：

- 低电平 2 ms 后输出 1/2/4 ms 三个前导脉冲，随后关闭强输出。
- 用弱上下拉产生 35 个具名输入步骤，包含 32 次电平变化；每步保持至少 1 ms。
- 检查全部五类异步等待、8 次真实 IRQ34/唤醒、2 次立即完成、2 次取消及 2 次接管拒绝。
- 保持其他引脚的 OEN/MASK/EN/TYPE/POLARITY/DEDGE/DEBOUNCE 位和完整 SW_CTL 原值。
- 清理本线通知，恢复原 pad 配置并停止 TIMER1。

`pad_config_restore=1` 只表示 mux/pull/drive/input/schmitt 配置恢复。
本线 GPIO 方向保持输入，latch 保持低，触发和中断优先级不恢复旧值；例程独占 GPIO0。
SW_CTL 选择编码未知，只观察完整值及前后相等，不写入或解释零值。

```sh
cargo build --offline --locked --release -Zbuild-std=core,alloc \
  --manifest-path examples/ws53-flash-gpio/Cargo.toml \
  --target riscv32imfc-unknown-none-elf
```

该命令只构建，不能证明 Flash 实板运行。新镜像必须单独完成模型、分区规划、备份、
hisiflash 写入/回读、同段 UART/DSLogic、复位及恢复验收。不可沿用温度候选11的镜像身份。
逻辑分析仪检查数字电平与顺序；模拟电压、精确 IRQ 延迟及真实断电冷启动另行验收。
