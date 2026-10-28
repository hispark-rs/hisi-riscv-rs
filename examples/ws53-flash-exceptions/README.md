# WS53 自主 Flash 异常/IRQ53 测试候选

独立workspace，使用应用核MCU_MODE_1和厂商FlashBoot交接，运行时包含
`ws53-flash-start`。Flash只保存应用镜像，不提供控制核/BootROM模拟或烧录命令。
当前是待验收候选，不继承RAM夹具或旧Flash镜像的实板资格。

与RAM `exception_context`共用`exercise.rs`和`probe.S`，执行24种FS/MIE状态、
16/32位真实非法指令、ECALL、同步嵌套及IRQ53中异常并返回。Flash环境不等待
主机nonce，`WS53_EXCEPTION_HIL_GATE`必须一直为零；不允许混用纯模型/HIL/旧hook
feature。IRQ驱动消费调用方已经交接的TIMER，不再次获取Peripherals单例。

启动检查data/bss、控制核复位状态，停止WDT，用TIMER1自主等待30秒采集窗口。
MGPIO17/D5仅连接分析仪输入，以最低驱动标记三个阶段：1ms前导、完整异常/IRQ
执行、2ms完成。中间脉冲是执行范围，不能当作精确IRQ延迟。随后串口输出128字
原始状态，关闭两路TIMER、恢复pad并flush。UART2未引出不计作电气验收。

```bash
uv run --script scripts/ws53_flash_exception_build.py --output target/flash-exception-build
uv run --script scripts/test-ws53-flash-exceptions-qemu.py \
  --qemu /path/to/qemu-system-riscv32 \
  --model-source-root /path/to/hisi-riscv-qemu \
  --elf target/flash-exception-build/firmware.elf \
  --build-receipt target/flash-exception-build/receipt.json \
  --output target/flash-exception-normal
```

模型仅接受固定的已验证二进制。正常场景两轮从毒化RAM/FlashBoot合成交接开始，
不加载调试器RAM镜像、不写寄存器结果。`--scenario irq-stall`模拟TIMER0停滞，要求
触发有界IRQ等待失败且不输出PASS；复位释放故障后同一镜像应重新完成。

源清单固定所有依赖、共享测试源码、锁文件、配置及rust-src。Flash实测仍须独立
完成镜像/分区规划、现场备份、hisiflash写入/回读、原厂恢复和断电冷启动；不能
直接拿ELF当完整芯片镜像，也不能覆盖控制核、校准、安全配置或永久保护位。
