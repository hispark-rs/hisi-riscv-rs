# WS53 RAM 实验

当前提供 PAC/运行时及 UART/TIMER、基本 GPIO HAL 联调示例，不代表完整 HAL 或稳定 WS53 支持。
独立 workspace 防止与 WS63 示例合并芯片 feature。

独立的仿真专用镜像还有 [NMI 上下文](nmi_context/README.md)、
[看门狗控制](watchdog_control/README.md)、[TCXO 取样](tcxo_control/README.md)和
[RTC 控制与中断](rtc_control/README.md)、[I2C 主机事务与超时](i2c_control/README.md)、
[SPI/QSPI 单线传输与停钟恢复](spi_control/README.md)、
[温度轮询、原始码与异常配置](temperature_control/README.md)、
[I2S 立体声轮询流和断钟恢复](i2s_control/README.md)、
[ADC 校准完成、错误与断钟](adc_calibration/README.md)。
这些包不属于默认构建目标，不能直接加载到当前实板；每个目录提供具名测试器
和具体的未验收条件。

从父仓库根目录执行：

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml --release --locked
uv run scripts/check-ws53-ram-elf.py examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke
uv run scripts/test-ws53-ram-elf.py examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke
```

仿真需要包含 WS53 机器的 `hisi-riscv-qemu` 构建：

```sh
qemu-system-riscv32 -M ws53 -display none -monitor none \
  -serial null -serial null -serial stdio \
  -kernel examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke
```

预期两行：`WS53 RAM data/bss OK`、`WS53 PASS timer_irq=53 count=3`。
通过后程序停止定时器并等待，不自动退出 QEMU。

接收路径可另行构建为 `--features uart-rx-smoke`。用 QEMU 仓库的
`scripts/test-ws53.py --qemu ... --elf-uart-rx <ELF>` 运行；测试器通过 UART2
输入 `00 ff 57 53 35 33 2d 52 58 0a`，预期新增
`WS53 PASS uart2_rx=10 empty=1`。这验证 HAL 逐字节读取及空 FIFO，不验证
物理波特率、接线或收包错误处理。默认构建不要求输入这些字节。

`uart-error-smoke` 是另一个 **仅用于 QEMU** 的显式错误注入变体。它接管三路
UART，清空接收 FIFO 并使用 SDK 的 FIFO 测试入口注入帧/校验/溢出错误；
不能当作默认实板镜像使用。用 `scripts/test-ws53.py --elf-uart-errors <ELF>`
运行，预期 `WS53 PASS uart_errors=3 partial=1 recovery=1`。两种 feature
一起启用时使用 `--elf-uart-combined <ELF>`，验证普通收包和错误恢复能共存。

WS53 `Uart::read_byte(&mut self)` 现在返回 `nb::Result<u8, ReceiveError>`：空
FIFO 为 `WouldBlock`，错误保留同时出现的标志。帧/校验/break 错误丢弃当前
坏字节；单独溢出会保留排队数据。`embedded_io::Read` 对非空缓冲区等待数据，
部分读取后发生的错误延迟至下次读取报告，零长度读取不会消耗该错误。
需要调用方自己控制超时的场景使用非阻塞接口。旧芯片接口不受此改变影响。

`uart-irq-smoke` 是 **仅用于 QEMU** 的中断变体。用
`scripts/test-ws53.py --qemu ... --elf-uart-irq <ELF>` 运行，测试器显式设置
模拟字符超时为 100000 ns；这不是实芯片超时参数。三路 UART 的 IRQ
39/41/42 各验证六次真实处理程序进出、屏蔽、重新使能、错误保留和恢复，预期
`WS53 PASS uart_irq=39,41,42 notifications=6 masks=1 recovery=1`。
`--features uart-rx-smoke,uart-error-smoke,uart-irq-smoke` 构建配合
`--elf-uart-all <ELF>` 验证三种 UART 测试共存。

`gpio-smoke` 是 **仅用于 QEMU** 的 pad/GPIO 变体。先配置全部28个非SWD、非复位
pad，逐项经UART握手核对SDK映射的mux/pad地址及读回，检查输入缓冲关闭时拒绝
GPIO交接、开启后允许交接。该全pad循环禁止用于实板，包含已知敏感测点。
随后数字 GPIO 测试使用 MGPIO2、AGPIO3、
SGPIO2（GPIO0 bit2、GPIO1 bit3、GPIO2 bit2）。测试器通过 qtest 注入数字电平，
通过 UART2 的66个阶段握手，验证 embedded-hal 输入/输出/翻转、三组真实IRQ
34/33/29各6次处理、上升/下降/双边沿/高低电平触发、挂起后取消与Drop释放。
不接管SWD两脚。不要把这些测试引脚直接用于A板，电气用途尚未经交接验证。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml --release --locked --features gpio-smoke
# 在 QEMU 仓库执行，ELF 使用上一步的绝对路径：
uv run --script scripts/test-ws53.py --qemu /path/to/qemu-system-riscv32 --elf-gpio /path/to/ws53_ram_smoke
```

QEMU组合镜像使用 `--features uart-rx-smoke,uart-error-smoke,uart-irq-smoke,gpio-smoke`，
配合 `--elf-all <ELF>`；测试器同时处理UART接收、错误注入和GPIO握手。
不要使用 `--all-features`，实板专用fixture与全bank故障fixture互斥。
默认无feature镜像继续只运行UART/TIMER。
GPIO HAL 需要单独取得 bank 和 pad 的所有权；示例通过 `PadControl` 配置pad与
GPIO复用后消费token交接，时钟、软件控制来源及资源排他性仍需预先满足。
每组同时借用一个Input/Output，避免配置寄存器的并发读改写；不同bank可并行。
`listen` 清旧状态、关闭去抖并保留本地IRQ为关闭，调用者装好处理程序后显式开启。
ISR调用 `GpioDriver::on_interrupt`，只读状态、写EOI并关闭该bank本地IRQ；
服务完输入后显式重新使能。`unlisten/free/Drop` 清理外设和控制器挂起状态，
输出释放时关闭数字驱动，不恢复原厂旧上下文或改变pad电阻。

WS53 接收中断采用一次通知后屏蔽的方式：安装相应 `UART_0/1/2` 处理程序，
调用 `set_receive_interrupts(true)`，再显式启用 `UartPort::interrupt()`。
处理程序调用 unsafe `Uart::<T>::on_receive_interrupt()`，只捕获原因、
屏蔽 RX/错误位和本路控制器中断，并确认控制器挂起位；不读取 RX 或会清除
错误的 LSR。主循环读取数据及错误后，重新开启外设和控制器两级使能。
ISR 只记录元数据，不能持有主循环 `Uart` 的可变引用。其他原因必须单独
处理，不可直接反复重新开启。慢消费者仍可能溢出，此接口不是 async 驱动。
取消已经挂起的通知时，两级都要关闭：仅清外设使能位不保证控制器锁存已清除。

实板必须先验证 MCU 模式 1、应用 TCM 所有权和 SWD 健康，暂停应用核后
清除并强制读回 `mstatus.MIE=0`，再加载 ELF（入口第一条指令也可能被旧 IRQ
抢占）。该 ELF 会覆盖原厂应用 RAM，必须复位恢复原固件；不能保存旧
寄存器后恢复旧上下文。UART2 使用厂商已配置的引脚和时钟，测试程序
默认无feature镜像不切换 pinmux；包含gpio-smoke的镜像禁止用于实板。
禁止 Flash 擦写。详见 `docs/plan/ws53-adaptation.md`。

HAL 的 `Unclaimed<T>` 只保留 Rust 单例；调用者需先证明控制核/DMA/旧中断
不再使用该资源，才能通过 unsafe 交接取得 UART/TIMER token。
TIMER 的 32 MHz 来自 SDK 应用入口 `dev_work_clock_init()`，不是 menuconfig
初始的 30 MHz，也不是这块板上已测量的频率。当前 SDMA 只读预检有未关闭项，
不能据其门控名称推断可停止。`hil/ws53-ram-smoke.py` 提供 A 板受控实验入口：
控制核保持复位，停止 WDT/trace、关闭 SDIO 时钟，逐字节检查镜像并整机复位恢复。
`--bounded-observation` 明确保留未验证的 SDMA 条件，不声称生产资源交接安全。
该条件下基础镜像已连续 20 轮通过，并与同一 ELF 的 QEMU 串口输出逐字节一致；
其他示例、GPIO 测试 feature 和 DMA 场景不能据此自动用于实板。

在主工程根目录执行（工具路径由当前机器提供）：

```bash
cc -shared -fPIC -Wall -Wextra -Werror -o /tmp/ws53-jlink-guard.so hil/jlink-no-update-guard.c -ldl
uv run --script hil/ws53-ram-smoke.py \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke \
  --openocd "$WS53_OPENOCD" --scripts "$WS53_OPENOCD_SCRIPTS" \
  --jlink "$WS53_JLINK" --jlink-no-update-guard /tmp/ws53-jlink-guard.so \
  --qemu "$WS53_QEMU" --output /tmp/ws53-hil-result --bounded-observation
```

当前入口固定 A 板身份：J-Link `23121302`、串口 `5CB7019921`。输出目录必须
不存在；包含准确 ELF、实际 Tcl、串口原始字节、调试/恢复日志和 JSON 报告。

在上述命令添加 `--gdb "$WS53_GDB"` 可验证 ELF 源码行定位、入口 `stepi` 和
`TIMER_0` 函数入口硬件断点。SDK GDB 12 已在 A 板实测 IRQ53 命中及继续返回；
调试端口只监听 `127.0.0.1`，可用 `--gdb-port` 指定空闲端口。

UART2 主机输入测试各自构建，避免混用 feature：

- `--features uart-rx-smoke`：运行时给 HIL 命令添加 `--uart-rx`，成功标记后
  主机发送 `00 ff 57 53 35 33 2d 52 58 0a`；核对10字节及读完后的空FIFO。
- `--features uart2-irq-smoke`：运行时添加 `--uart2-irq`。只接管UART2，测试
  IRQ42两次通知及显式重新使能，取消外设/控制器两级通知后仍能读取新字节。
  ISR只记录原因/计数；主线程读数据。实际FIFO阈值继承厂商配置，允许
  `DataAvailable` 或 `CharacterTimeout`，不声称已测量超时周期。

两种镜像已在实板与QEMU上完成相同主机输入流程、得到相同串口字节。IRQ场景
不同于原有QEMU专用 `uart-irq-smoke`，没有FIFO/错误注入，也不操作UART0/1。
两种IRQ fixture不能同时编译；HIL也拒绝原QEMU专用故障测试feature。
GDB与多阶段IRQ/GPIO主机交互目前分别运行。

`--features gpio17-smoke` 配合 HIL 的 `--gpio17`，仅操作已经定位的
A板 MIO17/MGPIO17/SDK17→D5；要求VDDIO已供电、只有分析仪输入连接、
原GPIO驱动已关闭、两核旧用户已停止。测试GPIO模式、驱动码0、六段高低输出、
上下拉输入读回，并恢复原pad配置。输出保持输入态，latch/IRQ状态不恢复，
最后仍必须整板复位恢复厂商固件。该镜像与QEMU使用相同八阶段UART握手；
主机每阶段等待40ms，实测波形用于证明物理输出，不把QEMU数字电平当作电压测量。
`--mode gpio17` 可单独运行对应QEMU场景。不能与全bank `gpio-smoke` 混编。
此单pad测试不证明GPIO IRQ、所有bank、输出电流、上升沿或电气时序。

`--features timer1-gpio17-smoke` 配合 HIL `--timer1-gpio17`，使用相同MIO17/D5
接线、所有权与电源条件，测试TIMER1的IRQ54：5ms和10ms两档各16次通知，
主线程每次翻转GPIO；两次单次模式各等待主机至少40ms，确认没有重复触发；
周期模式屏蔽期间保留raw pending，解除屏蔽后处理一次，再停止并清除。
成功标记为 `timer1_irq=54 notifications=35 periods=2 oneshot=2 mask=1`。
运行窗口2秒，仍整板复位恢复原固件。不要与其他GPIO fixture混编。

QEMU `--mode timer1-gpio17` 检查相同UART协议，并通过本地qtest套接字观察
MGPIO17真实模型输出：两组各16个交替边沿，完成后GPIO输出关闭。qtest的
主机接收时间不用作虚拟时间证明。实板需另用DSLogic在D5采样；当前1MS/s
三轮测量见能力矩阵，结果支持此运行配置的32MHz换算，不代表晶振精度或
所有时钟/低功耗配置都固定32MHz。

`--features tcxo-snapshot-smoke` 配合 HIL `--tcxo`，只接管应用TCXO取样窗口和
TIMER1，不修改pad、不发送计数清零命令。TIMER1提供10/20/40ms单次间隔，
每段前后调用TcxoDriver读取计数，保存六个64位样本到ELF符号
`WS53_TCXO_SAMPLES`；完成后再检查32次连续快照单调且整体前进。
HIL在CPU暂停后通过AP0读回样本；10/20/40ms的预期增量分别为
320000/640000/1280000 ticks（32MHz配置），容差5%包含软件处理开销。
运行脚本以准确ELF符号和48字节对象边界确定RAM地址，拒绝缺样本/倒退/停滞。

QEMU `--mode tcxo` 使用同一ELF和相同样本规则，另从非零高位计数起点开始，
检查64位拼接及未清零。虚拟时间采用 `-icount shift=3,align=off,sleep=on`，
避免宿主调度延迟变成被测间隔；此指令时间不是芯片周期模型。实板与QEMU
比较的是通过同一范围约束的计数增量和相同完成标记，不要求绝对时间戳相同。
共享计数所有权仍仅在控制核保持复位的受控实验中成立；睡眠补偿、时钟切换、
清零/禁用及跨核并发不在本fixture的实板证据范围内。

只验证和生成实际Tcl、而不打开任何设备或动态库：

```bash
uv run --script hil/ws53-ram-smoke.py --elf "$WS53_UART2_IRQ_ELF" \
  --uart2-irq --bounded-observation --prepare-only --output /tmp/ws53-prepared
uv run --script scripts/test-ws53-ram-readback.py --elf "$WS53_UART2_IRQ_ELF" \
  --tcl /tmp/ws53-prepared/run.tcl
uv run --script scripts/test-ws53-uart-host.py --elf "$WS53_UART2_IRQ_ELF" \
  --qemu "$WS53_QEMU" --mode irq --output /tmp/ws53-irq-qemu
```

加载前比较所有文件加载字节；运行后比较只读段。`.data/.bss`允许程序合法更新，
由fixture的初始化、数据、通知次数和取消断言检查，不与初始ELF盲目比较。

A 板当前返回 GW `1.0.20` / 提交 `1308525`，不能假定与参考 SDK 示例配置
一致。`hil/ws53-trace-handoff.py` 已验证当前 trace 配置可停止并恢复，但
没有覆盖 RAM，也不证明共享缓冲区可回收。固件身份、动态 DMA 和接管
剩余条件见 [RAM 接管记录](../../docs/plan/ws53-ram-takeover.md)。

`dma_memory` 是独立的 **QEMU 专用** 八通道 MDMA 示例，不属于默认 RAM 接管
镜像，禁止直接用于实板。它覆盖同宽 u8/u16/u32、八通道并发、总线读写错误、
超时、转换停滞、Drop、forget 和显式取消。每通道17个元素并带首尾哨兵，
主机独立检查全部源/目的内存、地址进度和在途写入，再通过串口允许程序取回
仍持有的资源。丢弃或forget的资源不会重新构造。

```sh
# 从父仓库执行。默认u32；另两种镜像分别加 --features width8 / width16。
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_dma_memory --release --locked
uv run --script scripts/test-ws53-dma.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_dma_memory
```

`Controller` 仅接受已交接的 MDMA，随后拆为八个不可复制的通道。`Buffer`
仅接收经过 unsafe 一致性/归属确认的静态内存；长度1–4095、范围和对齐仍由
HAL检查。`wait` 返回的是完成状态观察，不是安全内存归还。`request_stop`
和Drop不会关闭全局控制器，也不保证已发起总线事务已经落地。
`release_after_external_quiescence` 要求独立排空证明，不能把TC/ACTIVE/ENABLED
或延时当作充分条件。示例的主机握手仅在明确假设的QEMU模型成立，不提供实板
排空、缓存维护或控制核协作协议。当前未实现非UART/SPI外设请求、LLI、SDMA和IRQ/async。

`uart_dma` 是另一份 **QEMU 专用** 示例，使用真正的Rust/PAC驱动访问UART0/1，
UART2作为测试握手控制台。76个场景覆盖两端口、八通道、收发、总线错误、
FIFO满、停滞、取消、Drop/forget、初始状态拒绝，以及DMA完成后发送器仍忙。
主机通过虚拟串口提供/接收数据，独立核对RAM哨兵、地址、剩余计数及在途事务。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_uart_dma --release --locked
uv run --script scripts/test-ws53-uart-dma.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_uart_dma
```

`dma::uart::UartDma::from_configured` 要求unsafe交接已配置UART、两条请求线、
本地IRQ及稳定时钟/引脚；无MMIO，不会猜测DMA_EXTRA写使能。每个传输占有整个
UART，只接受静态字节缓冲区。`wait_transmitter_idle` 是有限的发送器状态观察，
不代替独立总线排空证明。停止、Drop和forget不归还资源、不停止对端，也不撤销
已发出的事务。接收轮询可能漏掉被DMA提前消费的字符错误，不能作为无损、逐字节
校验完备的收流接口。UART2、双向拆分、IRQ/async及实板验证尚未提供。

`spi_dma` 是 **QEMU 专用** SPI0/QSPI1双向DMA所有权示例，默认u8，另有
`width32`变体。两种位宽各56个场景，覆盖八组TX/RX相邻通道、每侧独立故障、
超时、取消、Drop/forget和初始配置/队列/错误拒绝，以及启动前和 DMA 运行中仅状态碰撞、状态与中断同时碰撞。主机配置的是明确的模型
环境，不是实际板卡接线；示例中引脚占用不得用于推断物理复用。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_spi_dma --release --locked
uv run --script scripts/test-ws53-spi-dma-collision.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_spi_dma
# 32位变体：同一构建命令增加 --features width32，然后运行相同检查器。
```

`SpiDma::from_configured`需要unsafe交接已启用且空闲的单线全双工master、
软件CS协议、引脚、IRQ及稳定时钟；本接口不完成板级初始化或自动切换阻塞驱动。
`transfer`只接受两个独占通道和同长度、不重叠的静态一致性缓冲区。`wait`
在同一有限预算内检查双通道进度、SPI错误、残余RX和发送器空闲；错误与取消
先关闭外设请求，再请求停止两通道，不复位共享控制器或刷新FIFO。已读事务
可能晚到；主机独立验证内存/哨兵、计数/地址、raw事件、在途事务及FIFO后，
才允许unsafe取回仍持有的资源。forget的对象不重构。该握手仅证明模型排空。

`i2c_dma` 是 **QEMU专用** 命令/接收DMA示例，166个场景覆盖两路I2C、八组
相邻DMA通道、7/10位地址、单/四拍、写/读/写后重复START读，以及1/3/4字节短读。
纯写结果通过虚拟对端读回校验；同时检查内存哨兵、命令/地址进度、错误和STOP。
模型故障涵盖每路DMA读写错误、超时、取消、Drop/forget、NACK和初始交接拒绝。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_i2c_dma --release --locked
uv run --script scripts/test-ws53-i2c-dma.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_i2c_dma
```

`I2cDma::from_configured`需要unsafe交接已启用、空闲、目标地址已配置的master、
SCL/SDA路由、IRQ、请求和稳定时钟。`Commands`在静态u16缓冲区构建合法命令，
纯写需要相同长度数据；读需要等长u8目的缓冲区；组合操作的命令长度等于写长度
加读长度。四拍收尾由HAL在剩余1..3字节时降低RX水位，仍使用同一有限轮询预算。
完成还要求FIFO无残留、线路空闲和STOP，成功不代表可以安全交还资源。

故障后主机独立停止模型DMA、等待在途事务、关闭I2C并清理FIFO/历史，再允许
unsafe交还仍持有的对象；忘记的对象不会重构。即使有STOP也不能将这个仿真
恢复步骤视作物理排空证明。示例中的引脚占用不代表实板接线，不得加载到实板。

`i2s_dma` 是 **QEMU专用** merge DMA示例。669个场景执行实际Rust/PAC代码：
480组主模式覆盖八个DMA通道、16/18/20/24/32位采样、1/4/8/16拍、独立收发
及双向；60组从模式；12组短接收尾部；18组单向故障、停止、Drop/forget和交接拒绝，另加99组双向故障。
双向故障覆盖通道0/1及7/0、16/32位、主从模式、发送/接收侧读写错误、
超时和取消；另有双向断钟、错误配置及初始占用。
收发长度独立，双向测试明确提供比接收窗口更长的发送流，不隐式补零。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_i2s_dma --release --locked
uv run --script scripts/test-ws53-i2s-dma.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_i2s_dma
```

执行器使用明确假设的32位单采样、左右交替merge模型，检查数据、哨兵、地址及
计数进度、请求路由和在途事务。正常双向数据经虚拟对端变换后逐采样校验；纯发送
验证搬运进度，不证明线路输出完毕。停止仅halt DMA，独立主机恢复先停序列化器、
放行在途事务，再复位FIFO，才允许unsafe取回保留资源。forget对象不重构。
物理打包、左右声道、握手、缓存及核心归属仍待实板核实；IRQ/async未开放。

`trng_control` 是 **QEMU专用** 共享TRNG交接/读取示例。执行器明确启用固定种子
测试源，58项场景涵盖两种种子、0–17字节正常填充、总预算耗尽的部分填充、
ring关闭、停钟、源停顿、READY/DONE缺失、就绪竞态、零/全一/相等候选，以及
恢复后重新接管、单字读取和Drop保持ring。主机和Rust共同核对数据、前缀及哨兵。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_trng_control --release --locked
uv run --script scripts/test-ws53-trng.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_trng_control
```

主机提供合成时钟、ring、FIFO及服务交接，错误后显式恢复模型再继续读取。
测试源不是密码学随机数，不得用于生成密钥；示例不得加载实板。实际FIFO深度、
两字准入、ring/时钟行为、跨核服务协调和熵质量仍待验证，不因仿真通过毕业为稳定。

`efuse_control` 是 **QEMU专用** 合成eFuse只读服务示例。111项实际Rust/PAC场景
覆盖三组完整256字节内容、奇偶/末地址和跨槽读取，每组20个读后失效位置，
boot/时钟/时序/模式及命令回读故障、空缓冲区和越界拒绝；每项再恢复并单字节读取。
主机和Rust核对前缀及边界哨兵，并用模型计数检查构造、释放、Drop及空读无MMIO。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_efuse_control --release --locked
uv run --script scripts/test-ws53-efuse.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_efuse_control
```

合成内容不是芯片身份。主机配置模型的读模式及时序，不建立真实boot/安全/控制核
服务交接。HAL仅发送受限的易失性读模式命令，无OTP数据或编程供电写接口。
不得加载实板；真实读时序、源时钟、跨核归属及身份schema仍未验收。

`ram_smoke` 的 `pwm0-smoke` 是已做单pad实板测试的实验fixture：
MIO21/SGPIO2/SDK47，mode1/PWM0P，对应当前A板D11。六阶段为25/50/75%、
常低、常高、停止低；保存并恢复pad配置，不更改GPIO方向。运行前须按
`hil/ws53-ram-smoke.py --pwm0` 的检查确认其他PWM0P别名未启用及整个PWM空闲。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_ram_smoke --features pwm0-smoke --release --locked
uv run --script scripts/test-ws53-uart-host.py \
  --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke \
  --mode pwm0 --output /tmp/ws53-pwm0-result
```

执行器为QEMU显式选择2MHz计数档：本板分频16、period200实测约100us。
SDK报告32MHz无法直接用于当前继承时钟状态，上游来源尚未确定。该档位让
仿真反映测量，不能作为通用时钟配置修复。实板使用`--bounded-observation`
受控RAM接管，仍不证明生产跨核/SDMA交接安全。证据见
`../../docs/plan/evidence/ws53-pwm0-hil-2026-10-23/summary.json`。

`pwm0-clock-probe` 在同一MIO21/D11上验证本地分频，依次使用
`ClockDivider::Div16/Div8/Div4/Div2/Div1`。每次先停止全部组，调用
`set_clock_divider`，读回PAC字段后启动；运行中再次改频必须返回GroupRunning。
结束恢复Div16及pad、停止输出，由HIL执行整板复位。三个重复实验和最终格式化
源码重建后的复测通过；它不包含上游PLL/XO切换或在线无毛刺改频。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_ram_smoke --features pwm0-clock-probe --release --locked
uv run --script scripts/test-ws53-uart-host.py \
  --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke \
  --mode pwm-clock --output /tmp/ws53-pwm-clock-result
```

实板仍使用相同受控接管执行器，但选择`--pwm-clock`。周期计数200时，
五档测得100/50/25/12.5/6.25us，说明当前分频前等效32MHz。原SDK返回的固定
32MHz与其默认÷16配置不一致；不得据此直接计算最终输出频率。此前的上游
疑问已缩小为“源身份和其他电源/时钟状态未知”，本地分频比例已有实测证据。

### 有限 PWM 实板/QEMU 共用程序

`pwm0-finite-probe`在既有MIO21/SDK47/D11上依次输出8、16个100us/50%脉冲。
它用TCXO独立等待、轮询配置加载事件、确认清除与pad恢复；尚不启用PWM IRQ。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_ram_smoke --features pwm0-finite-probe --release --locked
uv run --script scripts/test-ws53-uart-host.py --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke \
  --mode pwm-finite --output /tmp/ws53-pwm-finite
```

实板执行器选择`--pwm-finite`，其余身份、接管和恢复参数与PWM0相同。
`--prepare-only`可先生成并检查加载脚本。QEMU执行器明确选择2MHz、1us异步START
假定档及icount；不得删去诊断检查或用默认同步模型代替这项回归。
`pwm0-finite-delay-probe`仅保留为直接PAC状态调查用例，带额外串口采样；
不属于正常通过路径，严格UART oracle应拒绝其额外输出。

三轮实板证据在`docs/plan/evidence/ws53-pwm-finite-hil-2026-10-24/`。
加载事件不等于整组输出结束，任意短块、抢占、实板IRQ及计数器全部阶段仍未验收。

### 有限 PWM 计数器对照

用`pwm0-finite-counter-probe`构建、`--mode pwm-counter`运行QEMU，实板执行器
使用`--pwm-counter`。该程序仍只操作MIO21/SDK47/D11，收集18个TCXO时间戳、
配置加载标志和计数值，采样完成后才打印。执行器检查完整日志、逐点计数/状态
和20us采样窗口；不以不同平台的原始时间戳字节相等作为标准。
`uv run --script scripts/test-ws53-pwm-counter-samples.py`验证这些接受/拒绝条件。
测试证据在`docs/plan/evidence/ws53-pwm-counter-hil-2026-10-24/`。
实测有限输出的计数为从1开始的当前脉冲序号，停止后0；不能据此判断未测的
连续/重复/溢出模式，也不能用一次计数读取证明软件没有错过整个重复块。

### PWM IRQ72 实板/QEMU 共用程序

构建`pwm0-finite-irq-probe`，QEMU执行器用`--mode pwm-irq`，实板执行器用
`--pwm-irq`。仍只在MIO21/SDK47/D11输出8/16个脉冲；不得同时启用直接PAC
诊断feature。程序显式开启/恢复全局中断，检查外设屏蔽、IRQ72投递、ACK及重启。

预期两次有效通知、三次ISR进入，其中一次的外设事件快照为空：清除外设标志
不等于清除已经锁存的CPU pending。`pwm::acknowledge_interrupt()`允许返回空
快照，业务应检查事件位再计数/唤醒。不能为减少ISR次数而无条件清共享pending，
否则可能丢失其他通道的事件。严格日志包含初始pending及外设状态读数，能发现
模型错误地清pending或丢失真实完成通知。IRQ71异常路径仍未实板验证。
证据见`docs/plan/evidence/ws53-pwm-irq-hil-2026-10-24/`。

`ram_smoke --features tsensor-smoke` 是温度模块的共享RAM实板/QEMU测试，独立于
只供仿真的`temperature_control`。从父仓库构建与预检查：

```sh
cargo build --locked -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml -p ws53_ram_smoke --features tsensor-smoke --release
uv run --script hil/ws53-ram-smoke.py --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke --tsensor --bounded-observation --prepare-only --output /tmp/ws53-temperature-prepared
uv run --script scripts/test-ws53-uart-host.py --qemu "$QEMU_BIN" --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke --mode tsensor --output /tmp/ws53-temperature-qemu
```

实板流程沿用`hil/ws53-ram-smoke.py`的设备身份、RAM归属检查和复位恢复，需提供
OpenOCD、脚本目录、J-Link及禁止更新/烧录的guard路径；选择`--tsensor`，不可和
其他外设场景同时运行。它要求CONTROL/状态初始为0，暂停原应用和控制核后初始化，
记录12次原始码、名义换算及重新接管状态。没有stop/清READY/校准/共有时钟写入。
首次READY实板约4.63ms，当前QEMU约4.10ms，精确时序尚不匹配；报告中的功能通过
不表示温度标定准确、每次读均是新转换或生产跨核交接已安全。最终整板复位恢复原固件。

温度重新启动诊断使用 `--features tsensor-restart-probe`，HIL 选择
`--tsensor-restart`，QEMU 主机工具选择 `--mode tsensor-restart`。它在正常
初始化/读取后执行两次 SDK 定义的 START1，并在立即、约32us、约500us及
下一次 READY 时记录状态。START 后旧 CODE 保留，READY=0 时 HAL 必须返回
`None`；不得把旧码作为新的测量结果。三轮实板与修正模型通过上述状态契约，
重新启动到 READY 的耗时仍存在差异，未证明精确采样周期或启动内部流程。
证据见 `docs/plan/evidence/ws53-tsensor-restart-hil-2026-10-24/summary.json`。

`ram_smoke --features trng-smoke` 是 TRNG 的共享 RAM 测试；HIL 选 `--trng`，
QEMU 主机工具选 `--mode trng`。实板预检查要求厂商已启用 ring，测试只读取状态和
消耗 FIFO，不改 ring/FRO/时钟。它检查16组SDK式两字读取、32次HAL读取、
0/1/3/4/5/17字节填充的边界及两次重新接管后读取，结束仍保持原ring设置。
同ELF的QEMU由主机设置合成测试源并核对固定种子序列；实板输出不要求与它相等。
这是停止厂商CPU客户端后的有界观察，不是IPC服务交接或随机数熵质量认证。
沿用主机工具的设备身份、RAM归属和整板复位恢复流程，不能直接加载
只为模型错误注入设计的 `trng_control` 到实板。

WDT诊断分为 `wdt-smoke`（HIL `--wdt`、主机 `--mode wdt`）和
`wdt-settle-probe`（HIL `--wdt-settle`、主机 `--mode wdt-settle`）。前者早期
实板发现立即快照超时/旧计数，现 HAL 请求前加入至少200us及两个输入时钟周期的
等待，32000Hz配置下三轮同一镜像实板/QEMU对照通过；后者在启动、喂狗、
停止后显式等待200us，再记录8组TIMER1时间戳/看门狗快照。采用1024秒名义超时，
只观察约30ms计数，不主动触发NMI或复位。主机先停止原看门狗、核对无pending，
结束独立确认WDT关闭且无NMI，最终整板复位恢复。等待是实测配置下的经验保护，
其他时钟、低功耗、生产交接和真正超时复位仍待验证；原QEMU专用`watchdog_control`仍不可直接上板。
证据见 `docs/plan/evidence/ws53-wdt-hil-2026-10-24/summary.json`。

`wdt-handshake-probe`（HIL `--wdt-handshake`）进一步观察BUSY/CCVR握手，
包含上一次CNT读取后等待0/200us、四类命令/无命令对照、五个请求间隔，共40组。
日志完整仅标为“观察完成”，不代表HAL可正常连续取样；主机拒绝对此模式启用QEMU，
因为BUSY和再次接受请求的时间窗口仍未建模。三轮观察确认CNT读取清有效位，
且纯快照连续请求也受影响；这修正了仅归因于控制命令的早期判断。
RDL/PAC及模型已补读副作用；模型有可配置再次请求窗口，但仍无BUSY和命令旧计数模型。
HAL保护修复证据见 `docs/plan/evidence/ws53-wdt-guard-hil-2026-10-24/summary.json`。

`uart-line-smoke`（HIL `--uart-line`、主机 `--mode uart-line`）在继承时钟/引脚
配置下独占UART2，先等待移位寄存器空闲，再测试72组线路配置及144字节内部回环，
最后恢复1152008N1控制台。三轮实板与同ELF QEMU判据一致。整数分频读回先打开
DIV_EN，关闭时UART2 DIV_L实测返回0；QEMU采用显式严格读回配置复现此差异。
RAM中的`WS53_UART_LINE_STATE`保留进度，供UART不可用时诊断。只读回和内部回环
不证明物理波特率/停止位宽度；不修改共享门控或pad，不代替UART0/1及冷启动验收。
使用本夹具的主机保持静默，不得同时启用其他ram_smoke场景。

该场景现另检查发送完成：48个115200波特率帧的首次HAL/nb trait flush必须
返回WouldBlock，并至少观察一次“FIFO空、移位器仍忙”；完成后144个回环字节
全部保留。主机还要求`WS53 PASS uart2_flush ...`标记，旧镜像不属于这个版本的
场景。QEMU明确启用FIFO64/定时发送器假设，字符时间100us不是实板校准值。
三路接收错误在`uart-error-smoke`模型专用场景中覆盖：连续错误合并、正常队首
保护、部分读取的延后错误、溢出后64字节保留。该错误注入镜像仍禁止直接上板。

`uart-all-lines-smoke` 在上述公共测试上增加 UART0/1，使用 HIL `--uart-all-lines`
和主机 `--mode uart-all-lines`。三路各72组线路配置、144字节回环、48次首次flush
等待；UART0/1一直保持内部回环到整板复位，只有UART2回到已知控制台引脚。
该模式先核对三路能力寄存器、复位释放、普通数据访问模式及H0已知门控位；不写
时钟或pad。`WS53_UART_ALL_LINE_STATE`分别保存三路结果，主机拒绝漏测、端口
错位、计数不足或没有观察到FIFO空但发送器忙的结果。三轮实板及同ELF QEMU通过。
H1/L0具体门控、物理频率/引脚、RX真实错误和跨核生产交接仍未验收。

```bash
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_ram_smoke --release --locked --features uart-all-lines-smoke
WS53_ALL_UART_ELF=examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke
uv run --script hil/ws53-ram-smoke.py --elf "$WS53_ALL_UART_ELF" \
  --uart-all-lines --bounded-observation --prepare-only --output target/uart-all-lines-prepared
uv run --script scripts/test-ws53-uart-line-samples.py --elf "$WS53_ALL_UART_ELF"
uv run --script scripts/test-ws53-uart-host.py --elf "$WS53_ALL_UART_ELF" \
  --mode uart-all-lines --qemu "$QEMU" --output target/uart-all-lines-qemu
```

实板执行沿用上文 RAM runner 的工具路径参数，移除 `--prepare-only`；必须使用
新的输出目录。仅准备脚本或仿真通过不代表完成实板测试。

`uart-reset-probe` 是三端口复位观察夹具变体：HIL使用
`--uart-reset-probe`。在UART0/1分别留下一个回环接收字节，以非默认7位/奇校验/
2停止位、8倍采样配置及scratch=0x5a建立对照，再经PAC脉冲各自的半字复位位。
UART2不参与复位。每路记录复位前后14个值，再重新配置并收发一个字节验证恢复。
主机仅验收观察完整、基线有效及恢复成功，不把任意快照自动判成正确复位值。
QEMU现有显式启用的H1/H0复位模型，可单独运行主机 `--mode uart-reset-probe`；
单独运行成功只表示观察及恢复流程完整。HIL现允许与`--qemu`组合，且额外要求
原始日志逐字节一致；此前DIV_L和FIFO_STATUS差异已由三路RX访问矩阵定位并修正。

记录顺序为 UART_CTL、DIV_L（打开DIV_EN后等待10us）、DIV_H、DIV_FRA、BAUD_CTL、
MODEM_CTL、INTR_EN、FIFO_STATUS、TX_FIFO_CNT、RX_FIFO_CNT、UART_GP_REG、
UART_PARAMETER、LINE_STATUS、INTR_ID。读LINE_STATUS会消费错误，故不能用此
探测证明错误通知的保留。复位释放后另等待10us；这些等待都是观察条件，不是
最短复位脉宽或硬件规格。该模式既不改引脚，也不改变时钟，结束恢复原固件。

分别采集实板和QEMU报告后，以 `uv run --script scripts/compare-ws53-uart-reset.py
--hardware-report <实板report.json> --model-report <QEMU report.json>` 比较。
工具要求同一ELF及实板恢复成功，保留全部14项原始差异；任一差异均以非零退出。
当前测试的完整快照已一致。模型中的整数分频复位默认值、在途传输取消和保持
复位时的访问行为仍未获独立实测证明，不能因日志一致就扩大验收范围。

`uart-access-probe`（HIL `--uart-access-probe`、主机 `--mode uart-access-probe`）
增加三路各六种RX占用状态：空、1字节、2字节、读空、64字节、再次读空；每种分别
尝试DIV_EN=0/1并读回，保留36组九字段快照。64字节必须全部按序读回，UART2最后
恢复控制台。实测FIFO_STATUS空为0x0a、1/2字节为0x12、64字节为0x16；RX未读空
时DIV_EN置位不生效，读空后置位生效并读回整数低字节17。主机严格检查该已观察
配置，拒绝仅日志完整但寄存器值错误的结果。三轮实板一致，另一次联合RAM验证
与QEMU原始日志完全相同。示例不测试TX忙时改配置、非零DIV_H、字节模式或外部波形。

`uart-hal-reset-probe`（HIL `--uart-hal-reset-probe`、主机同名mode）进一步调用
`Uart<Uart0/Uart1>::reset_and_configure(LineConfig, SignalPath, delay)`，验证中断
使能和发送忙时拒绝、旧RX丢弃、重新配置后收发、共享复位字保持，以及另一UART
和UART2的scratch/帧格式/回环状态隔离。UART2使用独立L0复位字，见下方场景。成功标记包含
`ports=2 rejected=4 recovery=2 peers=4`，缺项或错误镜像会被主机拒绝。

接口仍是unsafe的继承资源操作：调用者负责共享半字RMW互斥、停止并排空DMA、
关闭本地IRQ、保持稳定时钟、取得pad和控制核交接、允许丢弃RX。外设IRQ必须先
关闭；API不等待忙发送结束而是返回TransmitBusy。复位读回失败仍尝试释放，
配置读回失败后需保持流量静止并恢复，不能直接继续旧上下文。实板只验证内部
回环路径及正常读回，读回失败通过主机故障模型验证；TX精确帧时序未验收；L0后续证据见下方。

`uart-l0-reset-probe` 通过GLB_CTL独立半字bit7观察UART2的14项复位前后状态，
并检查UART0/1非零scratch及帧/回环配置保持，最后恢复1152008N1控制台。
`uart-l0-hal-reset-probe`进一步测试UART2的HAL IRQ/TX忙拒绝、回环恢复，再经
`reset_and_configure(..., SignalPath::Pins, ...)`回到已配置的串口引脚。两者均使用
同名HIL参数和主机mode；每种均完成三轮实板，HAL版每轮与同ELF QEMU原始日志
一致。HIL组合模式要求逐字节一致。不能与H1/H0观察feature同时启用。

L0模型通过`ws53-uart-reset.assume-l0-profile=on`单独启用，H1/H0模型开关不替代它。
HAL调用者须互斥整条选定复位字的RMW（L0为GLB_CTL），时钟/引脚及控制核交接
仍须预先完成。UART2已有Pins输出恢复证据，不等同于实现自动pinmux/时钟初始化。

`uart-pad-probe` 使用已有 `PadControl` 配置 SDK 的四组 UART 引脚，HIL 参数为
`--uart-pad-probe`。UART0/MGPIO12(mode2)、UART1/MGPIO17(mode2)、
UART1/MGPIO8(mode1) 各发送带路由编号的264字节；分析仪对应 D10/D5/D15，
独立解码工具为 `scripts/analyze-ws53-uart-pads.py --raw <LA_CROSS_DATA>
--rate 4000000 --output <结果.json>`。UART2 在 AGPIO1/2(mode1) 重新配置后
输出专用控制台标记。八个 pad 配置及 GPIO 方向均恢复并读回，最后整板复位。

该模式须在外部线路允许驱动、厂商两核客户端已停止的继承时钟环境中运行。
HIL 的 passed 只涵盖固件断言与恢复，UART0/1 必须另有成功的分析仪解码结果。
三轮实测发现 UART1 的 MGPIO17 与 MGPIO8 可以同时输出同一数据，不能认为
配置新 pad 会自动释放旧 pad。RX 输入、自动时钟、跨核所有权尚未验收；
TX 模型已接入 `--qemu`：严格检查62项pad字节统计、18项配置恢复及原始控制台
日志一致。该模式显式启用TX/继承console配置，不启用RX模型。不能与复位观察
feature同时启用，也没有新增稳定UART引脚API。

以 `scripts/compare-ws53-uart-pads.py --hardware-report <实板report.json>
--waveform <解码.json> --model-report <QEMU report.json> --fault-test` 检查同一ELF
的路由标记、双pad同时输出和控制台。只比较完整标记包，不把复用切换期间
额外捕获的边沿当成UART数据，也不宣称模拟时序/电压或RX一致。


UART2 的 `uart2-irq-smoke` 镜像新增数字RX对端对照模式，无需改固件：

```sh
uv run --script scripts/test-ws53-uart-host.py --qemu <qemu-system-riscv32> \
  --elf <uart2-irq-smoke.elf> --mode irq-pad --output <新目录>
```

实板流程使用 `hil/ws53-ram-smoke.py --uart2-irq-pads --qemu <qemu>`，其余
OpenOCD/J-Link/ELF/输出路径和 `--bounded-observation` 参数同上。每轮发送
`00 ff 51`、`a5 5a 53`、`3c` 三组输入。QEMU中输入必须先由独立1152008N1
对端驱动AGPIO2，再经复用/输入使能和RX采样进入FIFO；不能直接注入字节。
检查7帧发送/接收、0帧取消、IRQ42两次通知、IRQ关闭后读取及串口逐字节一致。

同一ELF已完成三轮实板/QEMU对照，结束均整板复位并恢复AT响应；测试前后
600+600次DPIDR无错误。这里只验收继承引脚/时钟配置下的UART2正常接收，
不涵盖UART0/1外部RX、帧错误、流控、电气时序或自主时钟/引脚所有权。
证据见 `docs/plan/evidence/ws53-uart-rx-peer-hil-2026-10-24/summary.json`。


`uart2-async-smoke` 在相同继承时钟/引脚条件下验证 `embedded_io_async::Read`：

```sh
cargo build --locked -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_ram_smoke --features uart2-async-smoke --release
uv run --script scripts/test-ws53-uart-host.py --qemu <qemu> \
  --elf <uart2-async-smoke.elf> --mode async-pad --output <新目录>
```

实板脚本使用 `--uart2-async --bounded-observation --qemu <qemu>`，其余参数与
既有RAM流程相同；不能同时启用 `uart2-irq-smoke` 或 `uart-irq-smoke`。
固件先拒绝已有外设或控制器中断使能的接管，再经两组3字节输入验证ISR唤醒。
测试执行器在Pending后只等待waker，收到唤醒才再次poll；使用有界忙等待以免
未经验证的WFI睡眠策略掩盖丢唤醒。此执行器仅用于测试，不是生产调度器。
随后丢弃一个实际Pending的future，确认缓冲区未改变、IRQ关闭，最后让主机
再发1字节，确认没有中断回调但数据保留，下一次async read立即完成。

驱动的 `AsyncReceiver` 独占UART可变借用，每次非空读取返回1字节；空读取返回0。
ISR须绑定 `Uart::<T>::on_async_receive_interrupt`，不能再使用旧通知处理函数。
UART当前仅实现异步RX，发送和DMA future仍待做；TIMER/GPIO异步及Embassy时间驱动见下文。
三轮实板与数字对端QEMU原始日志一致，验证记录在
`docs/plan/evidence/ws53-uart-async-hil-2026-10-24/summary.json`。


`timer-async-smoke` 验证两路 `embedded_hal_async::delay::DelayNs`：

```sh
cargo build --locked -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_ram_smoke --features timer-async-smoke --release
uv run --script scripts/test-ws53-uart-host.py --qemu <qemu> \
  --elf <timer-async-smoke.elf> --mode timer-async --output <新目录>
```

实板使用 `--timer-async --bounded-observation --qemu <qemu>`，其余RAM加载/恢复
参数同上。该模式独占两路TIMER及应用TCXO快照窗口，不改引脚、不清TCXO epoch。
不能与其他TIMER1/TCXO例程混用。每路完成8次ISR唤醒、到期前和挂起后两次取消、
运行中/IRQ已使能/有挂起状态三类接管拒绝，并复查另一通道未改变。
执行器仅在wake后重新poll；TCXO检查请求延时下限，不承诺精确中断延迟或功耗。

HIL同时保存 `WS53_TIMER_ASYNC_PANIC` 行号及LOAD配置/运行/停止/到期观察。
两路各16种写入模式的读回应为 `written | 0xF`；该规律来自实测，不能推断为
低4位不参与实际计数。QEMU核对53项寄存器/RAM状态，HIL核对相同的44项LOAD
观察及原始UART日志。最初“LOAD原值保留”断言失败已留存，修正后的证据见
`docs/plan/evidence/ws53-timer-async-hil-2026-10-24/summary.json`。


### GPIO 异步输入（单脚 RAM/HIL）

```bash
cargo build --locked -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_ram_smoke --release --features gpio-async-smoke
uv run --script scripts/test-ws53-uart-host.py --qemu <qemu-system-riscv32> \
  --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_ram_smoke \
  --mode gpio-async --output <new-qemu-output-directory>
```

在本文已核对的A板RAM加载命令上选择 `--gpio-async --bounded-observation`
并加 `--qemu <qemu-system-riscv32>`，即可对同一冻结ELF做实板/模型对照。
MGPIO17/MIO17/D5 必须只有分析仪输入，VDDIO已供电；测试只改变该pad的弱
上下拉产生真实输入，不驱动输出。覆盖五种Wait、8次IRQ34唤醒、2次立即满足、
2次取消和重新等待、2项拒绝、邻脚状态及pad恢复，结束后由runner复位回厂商固件。
本模式无需宿主发送测试字节，也不能与其他GPIO17/bank使用模式混用。

GPIO异步接口需要unsafe ISR/资源交接，并保持已准备好的输入、供电和时钟。
每bank只能借用一个等待器；不是任意引脚并发框架。三轮A板结果只覆盖MGPIO17，
其他bank、外部脉冲宽度、去抖、睡眠及停钟恢复尚未实板验收。完整证据见
`docs/plan/evidence/ws53-gpio-async-hil-2026-10-24/summary.json`。


### TCXO 非阻塞快照与取消重用

将RAM示例构建feature设为 `tcxo-nonblocking-smoke`，HIL参数使用
`--tcxo-nonblocking --bounded-observation --qemu <qemu>`；独立QEMU runner
使用 `--mode tcxo-nonblocking`。它包含原TCXO/TIMER1参考检查，再验证40个
非阻塞样本、32次缓存读取、8次取消/回收/重用以及16次状态冲突拒绝。
不改变IO、不关闭TCXO时钟、不清零继承计数器；结束后使用现有复位恢复流程。

`tcxo_control --features transaction-smoke` 是QEMU专用故障固件，需通过
`test-ws53-tcxo.py --transaction-elf <elf>` 提供分阶段时钟/完成标志故障与恢复。
不能把该QEMU专用镜像作为实板HIL镜像加载。

WS53 `tcxo.enable()` 现在是可失败操作，调用改为 `tcxo.enable()?` 或在已审查
夹具中 `.unwrap()`。非阻塞事务Drop后，先用 `try_discard_snapshot` 等待旧请求
完成；新请求也会检查并丢弃已完成的旧快照。不要绕过Busy重发refresh或清计数。


`embassy-smoke` 验证真正的 `embassy_time::Timer`、`Ticker` 和 `with_timeout`：

```sh
cargo build --locked -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_ram_smoke --features embassy-smoke --release
uv run --script scripts/test-ws53-uart-host.py --qemu <qemu> \
  --elf <embassy-smoke.elf> --mode embassy --output <新目录>
```

实板使用 `--embassy --bounded-observation --qemu <qemu>`，其余参数同RAM流程。
只接管TIMER/TCXO与既有UART2，不改引脚或清零TCXO，不写Flash。
先验证TIMER运行、IRQ已使能及TCXO忙时拒绝接管，再显式安装全局时间驱动。
测试执行器只在waker通知后重试Pending future；覆盖5个期限、3次Ticker、超时、
提前截止时间重排、取消后重用、16槽满载、`u64::MAX`告警分段及回调重入。
`Timer`析构无注销接口，取消后的waker保留至截止/驱逐，数量有界；不宣称立即停钟。
三轮同ELF原始日志完全一致，结束后均复位恢复厂商固件。

`embassy-fault-probe` **仅限QEMU**：按阶段关闭TCXO时钟或卡住快照，确认时间不
倒退、不提前唤醒，恢复后截止任务继续。普通HIL的ELF检查明确拒绝这个镜像。
用 `scripts/test-ws53-embassy.py --elf <普通镜像> --fault-elf <故障镜像> --qemu <qemu>`
运行故障和协议负例。需要独立构建两份ELF，不能用后一次构建覆盖普通镜像。

驱动需要单hart、已交接的32MHz时钟及IRQ54专用处理函数，未实现睡眠补偿、
自主时钟配置或生产级控制核协商。时间故障只在QEMU注入，真实多分钟期限、
任意抢占交错与低功耗未验收，WS53仍整体要求 `unstable`。
证据：`docs/plan/evidence/ws53-embassy-hil-2026-10-24/summary.json`。

### UART2 TX IRQ 与异步发送

`--features uart-tx-irq-probe` 观察两种PTIM模式的IIR读清、重使能、FIFO满、
阈值通知和移位器完成；与 `hil/ws53-ram-smoke.py --uart-tx-irq-probe` 及
`scripts/test-ws53-uart-host.py --mode tx-irq-probe` 配套。
`--features uart2-async-tx-smoke` 使用真实异步Write/flush，验证257字节、
一次待发送取消和两次flush取消、三项绑定拒绝、TX期间保留RX三字节。
对应HIL参数 `--uart2-async-tx` 和QEMU模式 `tx-async-pad`；宿主按RX ARM
标记注入00 ff 53。两种fixture不能与其他UART拥有者feature同时启用。
保持继承时钟/控制台pad，有限RAM接管后必须整板复位；不直接恢复旧PC。
取消flush不会丢弃已入队数据。接口依赖独占UART与正确ISR交接；UART0/1、
流控/时钟故障及并发RX/TX split尚未验收。详细结果见WS53实施记录。

### MDMA 异步等待与取消

RAM示例 `--features mdma-async-smoke` 对应HIL `--mdma-async` 和QEMU
`--mode mdma-async`。使用四个已观察通道：真实IRQ58唤醒、两路同时等待、
一次通知后取消、一次屏蔽投递时取消、未poll取消及12项禁止重启检查。
四份源/目标及边界由暂停后的调试器完整比对，程序始终保留内存，不从
DMA所有者取回引用。继承时钟、非缓存DTCM与有限接管约束仍需满足。

`--features mdma-async-fault` 仅供QEMU，实板加载器拒绝该ELF。运行
`scripts/test-ws53-mdma-async.py --elf NORMAL --fault-elf FAULT --qemu QEMU`
可注入读错误、写错误、在途写阻塞和通道阻塞；在释放总线前确认取消后
仍有一笔pending write，随后核对恰好一个目标字改变。不能用模型结果
声称实板总线已排空。两种镜像均未做Flash烧录或生产控制核交接。
