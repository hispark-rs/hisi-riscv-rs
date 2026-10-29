# WS53 RAM 接管证据与未关闭条件

**状态：** 配套证据记录；最新验收状态以 [实施记录](ws53-adaptation.md) 为准。

状态：2026-10-27，在原基础RAM/IRQ的20轮之外，新同步异常运行时另连续20轮通过。
厂商双核并行运行的生产交接、SDMA 隔离和其他外设验收仍未完成。
本文件区分寄存器事实、已观测状态和接管条件，不能用某一项通过自动授权接管。

## 2026-10-23 VDDIO 修复后的实板推进

下列门控失败记录是本轮早期证据；后续采用有明确限制的受控实验，见下一节。

用户确认 VDDIO 已接板上 3V3，并授权所有 IO 定位及真实烧录。
此前禁止 Flash 写入的用户限制已解除；eFuse、安全配置及永久保护仍不写。
这一授权不等于镜像格式、资源交接或恢复流程已经验证。

- GPIO 标记确认 23 路分析仪接线，28 脚复用/上下拉/驱动强度/方向检查一致。
  MIO_06 主动标记在 GPIO 读回中正常，分析仪无匹配通道，保持未定位。
- 定位前后各 600 次 DPIDR 读取全部正确；OpenOCD 暂停、单步、恢复通过。
- 完整 4 MiB XIP 映射备份读了两遍且相同；尚未证明它是可直接回写的原始
  Flash 镜像，不能标为已验证恢复包。
- AP2 的 haltreq 没有得到 allhalted 应答。随后依据 SDK `main.c::copy_to_ccore`
  及 `platform_core.h`，将 `0x5703096c` 从 7 改为 3，读回为 3；这是控制核
  保持复位的实验，不能称作 AP2 调试暂停。整机复位后原固件 AT 正常。
- 看门狗 CR 原为 `0xbd`。按 SDK 解锁、消费 EOI、清 enable 并读回成功；
  trace 两个使能控制清零并读回成功，SDIO 的两个时钟门控检查成功。
- **RAM 接管尚未通过**：MSUB_NORMAL `0x520003e0` 的 SDK 标记为 SDMA 的
  bit4，写入 `0xeb` 后仍读回 `0xfb`。16 位、32 位两种访问均如此，不能
  归因为单纯访问宽度问题，也不能据此认定 SDMA 不存在或处于停止状态。
  两轮均在 `RAM_LOAD_BEGIN` 之前停止，未覆盖应用 RAM；独立硬复位后 AT 正常。
- 同一基础 ELF 在 QEMU 输出 `WS53 RAM data/bss OK` 和
  `WS53 PASS timer_irq=53 count=3`；实板还没有运行该 ELF，故不能填 parity PASS。
  ELF SHA256 为 `b5e78998cd3604ea1afa686311f5369333a7ed47864f498aa6dbfed3b3115074`。

工作区原始记录：`../hardware/ws53/vddio-restored/`（相对主工程根目录），
其中 `ram-probe-01/`、`ram-probe-02-word-msub/` 保存实际 Tcl、ELF、恢复日志和报告。
这些一次性诊断脚本使用明确的 SDK 地址证据，不是新 HAL API，也不替代
后续 RDL → PAC → HAL 的寄存器修复顺序。控制核复位和时钟门控实验尚未形成
生产可用的共享资源交接协议。

## 受控 RAM 实验：20 轮通过及实际发现

在 MCU mode 1 的应用 ITCM/DTCM 范围内执行基础 `ws53_ram_smoke`，没有覆盖
控制核代码或共享缓冲区。控制核在整个实验中保持 CPU reset，WDT 和 trace
停止，SDIO 时钟关闭；MDMA enabled channels 在加载前为零。SDMA 门控位仍
不可清除，本实验没有将它当作已停止：`production_handoff_verified=false`。
实验通过只能证明这组条件下的执行结果，不能证明任意共享内存或 DMA 安全。

早期实际失败定位到两个不同问题：

1. 未屏蔽继承 MIE 时，曾在入口 `0x28000` 被 IRQ53 抢占（`mepc=0x28000`、
   `mcause=0x80000035`），没有串口成功标记。RAM 跳转不是硬件复位；入口
   第一条指令也可能被抢占。因此加载前必须在 CPU 暂停状态清 `mstatus.MIE`，
   并强制读回检查，不能只依赖运行时入口中的 CSR 清除。
2. OpenOCD `get_reg` 默认可返回缓存。单步后用缓存 PC 做断言产生假失败，
   改用 `get_reg -force`。另将单步验证放在已知入口指令，避免以 WFI 空闲
   循环判断进度。AP0 不支持 `checksum_memory`，加载校验改为逐字节读取比较。

修正后的 `ram-entry-fresh-01..20` 连续 20 轮全部通过：

- 加载后立即和等待 100 ms 后各比较所有文件加载字节；运行后再次比较。
- 入口硬件单步，实际 PC 从 `0x28000` 到 `0x28002`，随后继续运行。
- `.data/.bss` 检查、UART2 输出、TIMER0 IRQ53 累计三次、ISR 返回。
- 运行后暂停、继续、再次暂停；整机复位后原固件 AT 正常。
- 同一 ELF SHA256 `b5e78998cd3604ea1afa686311f5369333a7ed47864f498aa6dbfed3b3115074`。

独立的可配置入口 `hil/ws53-ram-smoke.py` 已额外实测，并用同一冻结 ELF
逐字节比较实板和 QEMU 的两行串口输出，结果相同。新增本地动态库保护器
在打开探针前禁用 SEGGER 自动固件更新、Flash download 和 Flash breakpoints；
使用前验证库可加载。没有写 Flash、eFuse 或永久配置。

这不是全部 MCU 外设或完整启动的验收。实际定时器输入频率和外部波形没有
测量，UART2 继承厂商配置；NMI、FPU 运算、缓存维护、控制核并发协作、Flash
冷启动和源码级 GDB 断点尚不能由这 20 轮推导。相关接口保持实验状态。

## GDB 与 UART2 接收/IRQ 补充实测

后续三个具名场景已在A板通过，见
`evidence/ws53-gdb-uart-hil-2026-10-23/summary.json`：

- `gdb-hil-final`：SDK GDB12加载当前基础ELF，入口 `stepi` 从0x28000到0x28002；
  函数入口硬件断点命中 `ws53_ram_smoke::TIMER_0`，显示 `ram_smoke/src/main.rs`
  源码行，读到 `mcause=0x80000035`。删除断点后继续，完成IRQ返回、串口标记和
  只读段校验。GDB正常detach，主机结束OpenOCD后整机复位恢复。尚未测试任意
  源码行断点、优化后局部变量或源码级step的所有行为。
- `uart-rx-hil-01`：UART2接收主机发送的10字节（含00/ff），逐字节核对后FIFO为空。
- `uart2-irq-hil-02`：专用 `uart2-irq-smoke` 只访问UART2，不注入FIFO或错误状态。
  两轮主机输入各触发一次IRQ42；ISR屏蔽通知、记录原因/计数，主线程取走数据并
  显式重新使能。两级取消后输入的新字节仍可读取、通知数保持2。数据阈值与字符
  超时原因都被允许，不能据此认为实板和QEMU的FIFO阈值或超时周期相同。

三项均与同一冻结ELF的QEMU串口字节一致，QEMU开启guest_errors且无未实现访问
诊断，结束后原固件AT正常；最终600次DPIDR无错。共享主机协议进入CI仿真任务。
新功能不提高UART0/1、DMA、错误注入、时钟配置、控制核并行或SDMA的实板状态。

IRQ场景也暴露了HIL检查器的错误假设：`EVENT`位于可变`.data`，ISR正常写入后
不应再与ELF初值比较。现在加载前仍检查所有文件加载字节，运行后只检查只读段，
可变数据由程序断言验证。7项无设备Tcl故障注入证明：初始化数据损坏、只读段
损坏、短读和控制核意外释放会被拒绝，合法运行时数据变化不会被误报。

## 当前固件身份

A 板串口 `5CB7019921` 的只读 `AT+CSV` 返回：

- SDK 版本：`1.10.103`
- GW 版本：`1.0.20`
- GW 源码提交：`1308525`
- GW 源码分支：`feature/hi3853`
- GW 构建时间：`2025-08-13 17:10:52`

这是定制网关固件。适配所用 `fbb_ws53` 提交
`163d9150ab94a02d09a05f623351f9facee3025e` 的 `ws53_liteos_app`
提供寄存器与 SDK 流程证据，不是这份已安装固件的完整构建身份。
版本字符串也不是二进制匹配证明。应取得对应 ELF/map、源码或可校验固件包，
核对配置、符号、内存分配及控制核映像；没有这些时不能默认继承其资源所有权。

## 分项核对

| 项目 | 已有证据 | 尚需关闭的条件 |
|---|---|---|
| 探针/串口 | J-Link `23121302`；串口 `5CB7019921`；暂停/恢复、AT 正常 | 每轮重新核对，读错立即停止 |
| 内存模式 | BRAM_SHARE=`0x3f8`，DTCM_SHARE=`0x3c`，MCU mode bits=0 | 与所选 mode 1 相符；不是空闲内存证明 |
| Rust 链接范围 | ITCM `0x28000..0x30000`，DTCM `0x20050000..0x2005ff00` | 原应用占用会被覆盖，禁止恢复旧 PC/SP |
| ROM 数据/共享区 | 当前 ELF 避开 ROM 数据与控制核头部声明区 | 检查所有加载段、栈和缓存维护范围 |
| IPC 消息队列 | 控制核头部给出 `0x1e300`，应用视图加 `0x10000000` | 不能拿旧 port 默认 `0x10080000` 当实际地址 |
| HCC 载荷 | SDK app HCC 根据消息携带的地址调用 DMA 单块/链表搬运 | 队列区不重叠不能证明动态载荷缓冲区不重叠；需核对定制固件 |
| TRNG IPC 服务 | SDK main 注册随机数补充回调，使用消息中的控制核地址加 `0x10000000` 写整个或半个缓冲区 | 当前固件中的消费者、动态地址/长度及停用/交接协议未知；不能默认为 Rust 独占 TRNG 或可覆盖对应 RAM |
| MDMA | `ENABLED_CHANNELS=0`，已核对 8 通道与寄存器布局 | 确定其他核心不会再次启动，不能只凭一次读零断言独占 |
| SDMA | 地址宏/门控位存在，CPU/AP0 读取均失败 | 未证实可用，也未证实停止或不存在；由实际固件与总线事实解决 |
| SDIO ADMA | 只读 ROM 核对独立表指针入口 `0x52060040`，与 MDMA 不同 | 当前活动/所有权、停止及排空均未确认；ROM soft-reset 函数为空，不能作为停机证据 |
| MCU_DIAG/trace | 启用状态 `CPU_TRACE=3`、`SAMPLE_MODE=0x301`、`SAMPLE_SELECT=0` | 共用 ADC/trace 资源；必须停用或证明其写入隔离 |
| trace 缓冲区 | `0x200223ec..0x200227d0`，与当前 ELF 不重叠 | 地址配置不等于所有总线写入已排空 |
| UART2/TIMER | SDK app 路由、当前串口可用；QEMU 已闭环 | 实际固件的使用者、时钟及跨核归属要核对 |
| NMI/WDT | 汇聚器半字采样已执行；受控实验按SDK停止WDT并读回，未触发WDT/NMI | 普通 MIE/LOCI 屏蔽不能证明 NMI 停用；源外设、交接与复位路径仍需明确，零快照不作为放行条件 |

SDK 应用 `main.c::ipc_init` 会覆盖 ROM port 中的默认队列配置。当前头部起点
配合四个长度 `0x500,0x100,0x500,0x100` 得到连续 `0xc00` 消息区。
`rom/app/middleware/utils/hcc/acore/master/hcc_ipc_host.c` 使用消息内的源/目标
地址并调用 `uapi_dma_transfer_memory_single/lli`；控制核地址映射偏移为
`0x10000000`。这些源码说明为什么静态消息区检查不足，但还不是定制固件
所有 DMA 发起者和缓冲区的清单。

SDK 候选 `libriscv.a` 的 NMI hook 有 `mret` 返回路径，但默认 handler 不返回。
其中 `OsClearNMI`/`OsTriggerNMI` 写地址 `0x20c18` 与该配置的应用 ITCM 重叠，
不将它们当作可用 MMIO helper，也不在实板试写。离线复现与证据见
`evidence/ws53-nmi-entry-2026-10-23/`。运行时现已补齐专用 direct NMI 软件入口，
并通过12次软件上下文测试，但硬件投递、任意时刻重入和源清除仍未验收，不能将
`evidence/ws53-nmi-runtime-2026-10-23/` 的软件结果作为完整 NMI 验收。

SDK 的 `trng_ipc_acore_buffer.c` 还处理
`IPC_MSG_UPDATE_SHARE_RANDOM_NUMBER`：应用核读取消息中的类型、地址、长度，
映射控制核地址并填充随机字节。该路径是 CPU 写缓冲区，不是额外的 DMA
控制器；应用核暂停时回调不会执行，但 Rust 接管后原有控制核请求仍需要
明确处理或停用。原源码未校验 payload 长度、地址范围或加法溢出；Rust
实现不能照搬，也不能根据静态 IPC 队列地址推断随机缓冲区地址。

## 已实测的 trace 可逆操作

工具 `hil/ws53-trace-handoff.py` 从 PAC 的 SVD 读取寄存器位置并检查关键位，
仅接受当前 `CPU_TRACE=3`、`SAMPLE_MODE=0x301` 且非 ADC source=6 的配置。
其他配置不写寄存器。它的操作为：

1. 核对串口身份及 AT 健康，连接固定 J-Link，暂停应用核。
2. 保存 trace 控制、采样范围和完成地址。
3. 依据 SDK `CORE==APPS` disable 流程，以 16 位访问将 SAMPLE_MODE、CPU_TRACE 清零并读回。
4. 等待 20 ms 后读取完成地址，再隔 20 ms 读取一次；仅报告两次读数是否相同。
5. 独立恢复原模式，按 sync/enable 顺序重新启用，恢复 CPU_TRACE 并检查读回。
6. 仅在恢复成功时继续原 CPU 上下文，检查 AT。恢复失败则保持暂停并报告需恢复处理。

实测两个控制寄存器读回零，两个完成地址均为 `0x200224ec`，原配置恢复后
AT 正常。完成地址可能是停止时锁存值；其稳定性**不是**释放缓冲区或宣称
总线排空的充分证据。工具不写采样缓冲区、Flash、eFuse 或安全配置，不替换固件。

主机上的六项 Tcl 故障注入测试执行同一份操作序列，覆盖正常路径、两个停止
写入位置出错、恢复写入出错、读失败、ADC 配置拒绝及完成地址变化。恢复失败
不继续 CPU；停止过程部分失败仍尝试恢复。模拟测试不代替实板验证。

## RAM 接管后的恢复规则

上述 trace 工具能继续原上下文，是因为它没有覆盖应用 RAM。实际加载 Rust
镜像后，即使测试失败，也不能用原 PC/SP 继续旧应用。须使用已验证的整机
复位路径恢复厂商应用，再确认串口/SWD 健康；恢复过程不写 Flash。

后续 RAM runner 应在同一调试会话中完成身份/模式/加载段/共享资源检查、
暂停和接管，并把“RAM 已覆盖”记录为不可回退的会话状态。当前工具没有
加载功能，不是隐含放行开关。后续新增的受控 RAM 工具已经完成上述20轮实验；
这不改变只读预检、trace可逆操作各自的证明范围，也没有关闭生产交接条件。

## 可复测命令

```bash
uv run --script hil/ws53-readonly-preflight.py --uart-version --output /tmp/ws53-version
uv run --script scripts/test-ws53-trace-handoff.py
uv run --script hil/ws53-trace-handoff.py \
  --openocd /path/to/openocd --scripts /path/to/openocd/tcl \
  --output /tmp/ws53-trace-handoff
```

原始记录见本目录 `evidence/ws53-installed-version-2026-10-23.json`、
`evidence/ws53-trace-handoff-2026-10-23/`。这些是 RAM 接管的准备证据，
没有新增实板支持或稳定性结论。

## GPIO17 单pad RAM验证（2026-10-23）

在已通过的受控接管路径加入 `gpio17-smoke`；仅MIO17/MGPIO17/SDK17接D5，
驱动码0、八阶段串口握手，运行窗口1.5秒。接管前确认GPIO0 OEN bit17为1，
记录SW_CTL（连续三轮为0），不更改来源编码。六段主动高低、两段输入上下拉
及pad配置恢复全部通过；32通道波形只有D5逐段匹配。相同ELF的QEMU输出与
实板逐字节一致，连续三轮复位后AT恢复；前后SWD各600/600及CPU单步正常。
证据见 `evidence/ws53-gpio17-hil-2026-10-23/summary.json`。

输出free后保持输入，恢复pad字段，但不回放旧GPIO latch/IRQ及旧程序上下文。
控制核仍保持复位、SDMA交接未闭合；这些结果不扩大生产接管安全保证。

## TIMER1 IRQ54与物理周期（2026-10-23）

使用相同受控RAM接管，新增 `--timer1-gpio17`、2秒运行窗口。TIMER0初始三次
IRQ53完成并停止后，TIMER1进行32次周期、2次单次、1次解屏蔽通知；ISR只应答
并记录，主线程翻转已定位的MIO17/D5。三轮实板与QEMU串口一致，mcause为
`0x80000036`；5ms/10ms翻转间隔分别为5.000–5.001ms和10.000–10.001ms。
两次单次模式的40ms以上观察期均保持IRQ开放，避免通过mask隐藏重复事件。
最后停计数、清中断并恢复pad，整板复位后三轮AT正常。测试前后各600次DPIDR
正确且CPU单步正常。证据 `evidence/ws53-timer1-hil-2026-10-23/summary.json`。

此结果扩大TIMER1功能证据，不改变控制核复位/SDMA未闭合的接管限制，也不
证明所有时钟配置或晶振精度；未进行Flash/永久配置写入。

## TCXO应用窗口取样（2026-10-23）

同一受控RAM接管增加 `--tcxo`，继承状态0x14、enable不发清零命令，TIMER1
10/20/40ms单次间隔前后取样。三轮六样本/32次连续读取全部通过，增量分别
320215/640215/1280215；暂停后AP0按ELF对象读取样本并独立检查。原计数起点
在整板复位之间变化，不作为跨轮时间连续性保证。前后SWD各600/600及CPU单步
正常，三轮复位后AT恢复。证据 `evidence/ws53-tcxo-hil-2026-10-23/summary.json`。
本实验不建立跨核时间服务、低功耗补偿或生产资源交接，不关闭SDMA未验证项。

## PWM0 单pad输出与时钟限定（2026-10-23）

`--pwm0`在RAM加载前拒绝已路由的其他PWM0P别名、MIO21原输出、非空组/
通道和非零高计数半字。fixture用PAC读回分频16、enable/load及门控，TCXO
提供停止等待；仅切换SDK47至mode1，六段握手后停止、恢复pad并关闭PWM时钟。
连续三轮同ELF输出和物理25/50/75%、常低/高/停止通过，复位后AT恢复。
前后SWD各600/600，CPU暂停/单步/继续正常。

首轮失败揭示时钟假设：period200实际100us，而非SDK32MHz预期6.25us。
后续QEMU以显式2MHz档与实测对照；未修改上游共享时钟，也未确认其来源。
证据`evidence/ws53-pwm0-hil-2026-10-23/summary.json`同时保存失败及成功，
不把采用实测档视为完成时钟树适配。跨核/SDMA交接、其他PWM通道、有限/IRQ
及停止延迟仍未验收。本轮没有Flash或永久配置写入。

## PWM 本地分频闭环（2026-10-23，后续验证）

新增`--pwm-clock`复用PWM接管检查及MIO21/D11，初次PAC诊断后改由正式实验HAL
`ClockDivider`接口完成。每次全组停止→设置分频→读回→启动；五次运行中改频
请求均应拒绝。16/8/4/2/1在200计数下实测100/50/25/12.5/6.25us，各25%占空比，
停止段低。三轮同ELF与QEMU一致，最终格式化源码重建另复测一轮；原固件AT恢复。
证据`evidence/ws53-pwm-divider-hil-2026-10-23/summary.json`。

这确认当前接管下32MHz等效输入经PWM本地÷16成为2MHz，定位了SDK固定时钟
返回值导致的16倍误差；未确认输入来自哪一全局时钟路径。未更改上游时钟、
控制核/SDMA交接限制仍存在，未扩展为无毛刺改频或其他通道的实板验收。

## 2026-10-27：新同步异常运行时的受控 RAM 验收

运行时代码 `daa33a3` 的类型化异常帧、LTO覆盖修复和IRQ所属mscratch恢复，现以
独立 `hil-controlled` 夹具完成具名实板测试。typed一次、IRQ一次及连续20轮通过；
每轮25次真实异常返回，IRQ版本27次hook进入，FS/MIE的24种组合、同步嵌套及
TIMER0 IRQ53中的异常后返回均检查。128状态字与同一ELF的QEMU结果逐字相同。

协调器在BSS初始化后的ready/nonce握手才放行；内存/源码/工具/新鲜SWD健康检查、
继承MIE屏蔽、控制核保持复位、WDT/trace/TIMER停用、加载及只读段校验、TIMER/NMI
清理和每轮物理复位恢复均有记录。前/系列/后各600次DPIDR正确，CPU暂停/单步/
恢复通过。新增路径不驱动引脚，不借UART输出证明异常；UART只检查原厂AT恢复。

这是对该固定RAM镜像的CPU异常/IRQ证明。SDMA不可清门控、其他master排空和生产
双核交接仍未证明；Flash/冷启动、外部波形/精确定时、坏栈和物理NMI仍未验收。
来源、逐轮状态和恢复记录见 `evidence/ws53-exceptions-ram-2026-10-27/`。
