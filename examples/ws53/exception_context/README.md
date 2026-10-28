# WS53 同步异常模型与受控 RAM 夹具

在完整RAM镜像中执行真实非法指令和ECALL，验证WS53异常入口、可返回的类型化
处理函数、原有零参数hook覆盖与默认停止路径。默认构建使用`qemu-only`；独立的
`hil-controlled`只允许typed/IRQ场景，必须通过受控RAM协调器进入，两者互斥。
它不是通用烧录镜像，不声明物理NMI、坏栈、Flash或冷启动已验收。

```bash
uv run --script scripts/ws53_exception_build.py --profile irq --output target/exception-irq
uv run --script scripts/test-ws53-exception-context.py \
  --qemu /path/to/qemu-system-riscv32 --model-source /path/to/hisi-riscv-qemu \
  --elf target/exception-irq/firmware.elf --profile typed --output target/exception-irq-run.json
```

构建工具的profile为`typed`、`irq`、`legacy-hooks`、`named-stop`、`default-stop`和
`default-halt`。运行工具对`irq`产物仍选择`typed`，通过镜像报告识别IRQ子场景。
模型二进制及其传输源码沿用固定、已经验证的QEMU校验值；工具不打开物理设备。

typed场景执行16位非法指令、向只读CYCLE写入的32位非法指令与M-mode ECALL，
覆盖FS Off/Initial/Clean/Dirty和原中断使能0/1的24种组合。每次验证全部整数、浮点
位模式、fcsr、FS/MIE和mscratch的保存/恢复，再验证处理函数中触发同步异常的嵌套。
irq场景进一步由真实模型TIMER0 IRQ处理函数触发同步异常，验证IRQ的mscratch帧指针、
栈和主程序返回；不会软件改写mcause/mepc来伪造异常进入。

legacy-hooks覆盖`do_ecall_m()`并返回，检查默认PC加4；named-stop/default-stop分别
覆盖`do_insn_illegal()`与`default_exc_handler()`并停止。default-halt不覆盖，检查默认
汇编停止入口。release构建含LTO，避免只验证无LTO便误认为弱符号冲突已关闭。

异常处理栈必须16字节对齐。没有栈帧的叶hook可以使SP恰好等于栈顶，这是合法的
空栈位置；不能把它误判为越界。嵌套/IRQ测试另验证有界的帧生长和实际返回。
寄存器播种和mscratch哨兵仅为本夹具自有测试协议，不是普通应用允许借用mscratch。

## 受控 RAM 实板

```bash
uv run --script scripts/ws53_exception_build.py --execution hil --profile irq \
  --output target/exception-hil
```

同一ELF先用上面的QEMU运行工具检查。实板使用`hil/ws53-ram-smoke.py`的
`--exception-context --bounded-observation --exception-receipt ... --health-report ...`，
另传ELF、OpenOCD、scripts、J-Link及禁止自动更新的guard路径。需固定A板身份、
600次DPIDR/CPU健康、来源清单和工具校验一致；prepare-only可先生成不访问设备的
Tcl。不能把纯模型ELF、过期健康报告或混用的源码当成硬件准入。

协调器暂停应用核并屏蔽继承MIE，保持控制核复位，停止WDT/trace与两路TIMER，
核对MCU_MODE_1及DMA/NMI观测，再加载/两次逐字节校验。main在BSS清零后停在
ready阶段；宿主观察后写nonce才放行。结束验证邮箱和只读段、TIMER/NMI清理，
经物理nRESET恢复原厂AT，不能继续被覆盖的原程序上下文。

RISC-V允许非法指令的mtval报告零或指令位模式。HIL按此预定规则接收并保留实际
值；模型profile仍检查固定值。2026-10-27的A板实测恰与QEMU一致，typed一次及
IRQ一次加连续20轮通过，各128状态字均与同ELF模型一致。SDMA不可清门控和共享
总线排空仍未证明，仅为有边界实验，不是生产交接。测试不驱动外部IO；UART只
检查原厂恢复，不证明UART电气、精确定时、低功耗或全部外设。

更多帧/继续点安全要求见运行时`docs/ws53-exceptions.md`。RAM实测不毕业整个WS53
为稳定API；具名证据和限制见`docs/plan/ws53-adaptation.md`。
