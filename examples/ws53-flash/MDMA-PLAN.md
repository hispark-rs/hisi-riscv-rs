# Flash MDMA 组合验收：接入设计与当前阻碍

状态（2026-10-24）：**软件准备，尚未提供可烧录的 MDMA Flash fixture**。
当前 `capture-delay` / 候选 07 不包含 DMA。下述初稿属于源码审查；此后已按实际
分支完成受限 ROM 及回调槽只读取证，新增证据见文末链接。仍未启动 Flash DMA，
也未把 RAM 测试通过标记成 Flash 验收。

最新SFC专项审计补充：[实际初始化与状态修复](../../docs/plan/evidence/ws53-flash-sfc-handoff-audit-2026-10-24/README.md)。
所选初始化不提交RAM bus-DMA，但忽略末次busy等待错误，主函数遇错仍继续；紧邻
`fix_sr`在特定ID/状态条件下请求非易失状态写`1C 02`。不能以“已跳入Rust”证明
SFC独占/排空，也不能把Flash array备份当作独立状态寄存器NV备份。条件分支是否
实际触发、保持及恢复尚未观测；本轮没有发送该命令或改变硬件。

后续[SFC命令合同与模型](../../docs/plan/evidence/ws53-sfc-command-model-2026-10-24/README.md)
已补齐原SDK busy轮询与硬件更新元数据，并在隔离PAC/QEMU分支通过测试。
合成NOR明确分开CMD完成和WIP提交，含停滞/复位/掉电场景；它未实现bus-DMA、
真实NOR或posted-write drain，不关闭上面的Flash MDMA前置条件。主工程冻结PAC与
候选07保持不变，下一步仍须完成实际资源交接，不能从命令模型通过推导DMA安全。

## 已有证据与不能继承的前提

| 项目 | 已有事实 | Flash 接入仍须证明 |
| --- | --- | --- |
| 四个 MDMA 通道 | RAM `split_four()`：通道 0–3，每路 17 个 `u32`，源/目的两端各有哨兵；IRQ58/async 已有具名实板与 QEMU 记录 | Flash 上的实例、时钟、内存属性和交接条件；不能暴露 4–7 槽作为已验证通道 |
| RAM 所有权 | Flash runtime 校验控制核保持复位，建立 MCU MODE 1，链接器把数据/栈放在 `0x20050000..0x2005ff00` | 所有可能访问此 RAM 的旧总线主设备已停止，未留下随后提交的事务 |
| 数据缓存 | `flash-cache-baseline-02/read.log`：`DCCTL=0`、`ICCTL=6`、`MEMATTRL=0x33333333`；SDK `cache.h` 规定 `DCCTL[0]` 为数据缓存使能 | 每次自主启动的前置检查；启动链未遗留脏缓存别名的证据。一次停止后的寄存器快照不等于完整历史证明 |
| PMP | RAM MDMA 的 region5 是锁定 TOR、非缓存 DTCM；Flash 的 `pmpcfg0/1=0` | Flash 不能复用 RAM 的 PMP 常量并把不匹配忽略掉，也不能随意改写 PMP/cache 配置 |
| 旧 MDMA 状态 | Flash 基线记录 `ENABLED=0`、`RAW=0` | 所有通道的配置、请求/LLI 状态和来源交接。EN/ACTIVE/TC 为零与等待一段时间均不是总线 drain 证明 |
| 其他总线主设备 | RAM HIL 关闭 trace/SDIO，尝试关闭 SDMA 时明确记录 `UNQUALIFIED_SDMA_GATE` | Flash 尚无等价的 trace/SDMA 交接证明；控制核复位不能代替这些设备的状态检查 |
| 目的数据 | RAM 在 CPU 暂停后由 AP0 两次读取源/目的/哨兵，缓冲区一直保留到复位 | Flash 仍需明确的外部只读观察流程，或独立证明 drain 后才能由 CPU 读取目的数据。UART 成功行本身不证明复制的字节正确 |

RAM 的 `UNQUALIFIED_SDMA_GATE` 实际记录为：normal 请求 `0xeb`、读回 `0xfb`；
sleep 请求 `0x3e4`、读回 `0x3f4`。它是一次受限观察的已声明缺口，不是关闭成功。
当前 PAC 有 `MCrg::msub_normal/msub_sleep` 的 SDMA gate 位，但没有 SDMA
控制/状态外设实例。SDK `platform_core.h` 的 `SDMA_BASE_ADDR=0x520a0000`
只能证明地址来源，不能据此假定它与 MDMA 共用寄存器布局或语义。

因此本轮不添加调用 `Buffer::from_coherent_static` 的 Flash feature。给 unsafe
调用加上 `DCCTL==0 && MDMA_ENABLED==0` 断言仍不足以满足它的独占、无脏别名和
无其他主设备访问的合同。也不把缺项改成裸地址读取或默认全零。

## 资源条件解决后的代码组织

采用三个显式、独立构建的 Flash 配置构成一组测试，而非在一次启动里复用已交出的
四组缓冲区：

| 计划 feature | 复用文件 | 验证内容 |
| --- | --- | --- |
| `mdma-smoke` | `../ws53/ram_smoke/src/mdma_probe.rs` | 四通道 polling、有限等待、源/目的/哨兵外部读取 |
| `mdma-irq-probe`（依赖 `mdma-smoke`） | 上述文件及 `mdma_irq.rs` | IRQ58 四次通知、屏蔽/解除屏蔽、选择性确认、原始状态保留 |
| `mdma-async-smoke`（依赖 `mdma-smoke`、HAL `async`） | 上述文件及 `mdma_async.rs` | 三次 wake、两个并行等待者、两种已完成事务的取消、12 项状态保护 |

IRQ 与 async 配置互斥，不开放 `mdma-direct-probe`、8/16 位实验或需要主机
UART 输入的 `mdma-async-fault`。默认 feature 与 `capture-delay` 均不隐式开启 DMA；
未来只有显式组合两个 feature 才增加 30 秒观察窗口。候选 07 冻结 ELF、原有资格
profile、UART 文本与波形预期继续保留。

每个配置只构造一次 `Controller::split_four()`，每路使用独立静态缓冲区。完成后
继续 `forget(transfers)`，不重新获取 token，不调用 `release_after_external_quiescence`，
不因得到 TC、EN=0 或取消 future 而把内存返还给 CPU。组合测试通过不同镜像/复位
执行，避免为了在一次启动中跑完三类测试而绕过 retained 所有权限制。

共用 RAM fixture 的失败应传播成明确失败，不能打印 `WS53 FAIL` 后继续输出普通
Flash PASS。共享代码如需返回 `bool`，RAM 侧保持当前输出与行为，Flash 侧才据此
拒绝后续验收。所有等待保持有限：现有 TIMER1 进度检查、poll 预算、IRQ/wake 预算
继续生效，不引入 UART RX 握手。panic 后保留只读诊断，恢复仍由已验证复位路径执行。

## 解除阻碍所需的独立证据

1. 固定实际 FlashBoot 镜像及对应源码/构建配置，核对其缓存、MDMA、SDMA、trace
   和 SDIO 使用。若能证明某设备从复位起没有启动，记录完整路径；不能仅根据某文件
   未出现名称作结论。任何被启动的设备均需有停止/无后续访问的交接协议。
2. 补齐需要的 SDMA 寄存器描述和副作用，按 RDL → SVD → PAC 顺序实现；或者提供
   可审核的“不可能访问应用 RAM”的硬件隔离证明。本轮不猜测 SDMA IP 版本和通道数。
3. 每次镜像运行前在应用自身检查 cache 关闭、MCU mode、RAM ownership、控制核复位、
   trace/SDIO/MDMA 状态及获准时钟配置；禁止调试器写寄存器替程序建立条件。初次
   只读不匹配必须拒绝启动 DMA，不主动停止未知所有者。
4. 在独立 ELF 检查中验证所有 DMA 缓冲区、哨兵及栈都在应用私有 RAM 内，区间不重叠，
   宽度/大小精确。缓冲区初始化后加入适当 CPU/bus fence，检查 fixture 不改变缓存。
5. 确定目的数据检查方案：保留缓冲区并通过 AP0/QEMU 内存读取对照，或先取得可独立
   审核的 drain 协议。前者可验证 Flash 自主运行后的数据，但仍不宣称安全内存回收。

## 后续测试及报告边界

主机测试覆盖 handoff 错配、缓存开启、越界/重叠、旧通道活动、共享 IRQ 冲突和
禁止 feature 组合。QEMU 必须显式建模所需 CSR/clock/旧主设备状态；当前有限
Flash handoff 模型没有证明真实缓存或 SDMA 的行为，未实现 MMIO 返回零不可作为通过条件。

每个正常配置两轮，对照完整 UART、四路数据/哨兵、IRQ/wake 及 retained 状态。
故障配置分别注入无进展、总线错误、屏蔽/缺失 IRQ 和保留的 posted write，报告为
“检测到预期故障”，不能作为正常启动资格。先验证模型，再为新 ELF 单独冻结烧录计划。

具备条件后实板依次进行 hisiflash 回读、Flash 自主运行、物理复位、真正断电启动、
恢复与 SWD 健康检查。逻辑分析仪可记录应用阶段/IRQ 观察标记和 UART 时间顺序；
它不能直接看到片内 MDMA RAM 数据，不能替代字节与哨兵校验。报告分别记录 QEMU、
RAM 实板、Flash 实板、冷启动、physical drain 五项状态。

现有证据入口：

- `docs/plan/evidence/ws53-mdma-hil-2026-10-24/summary.json`
- `docs/plan/evidence/ws53-mdma-irq-hil-2026-10-24/summary.json`
- `docs/plan/evidence/ws53-mdma-async-hil-2026-10-24/summary.json`
- 本地只读基线：`../hardware/ws53/vddio-restored/flash-cache-baseline-02/read.log`、
  `flash-dma-rtc-baseline-01/read.log`、`mdma-async-hil-06/openocd.log`

这里的相对路径均以主仓库根目录为准。私有板卡数据不随本文发布。

## SDK SDMA 寄存器证据审查

以下路径以 SDK 根目录为准，本地为
`/home/sanchuanhehe/Documents/hispark/fbb_ws53`，提交
`163d9150ab94a02d09a05f623351f9facee3025e`。本节只读审查源码，没有执行 SDK 程序。

| 证据位置 | 可确认的内容 | 不能推导的内容 |
| --- | --- | --- |
| `src/drivers/chips/ws53/include/acore/platform_core.h:48–49`；control_core 同名文件 `:47–48` | M_DMA 基址 `0x52070000`、S_DMA 声明基址 `0x520a0000` | 第二个基址的可访问性、寄存器布局、通道数、reset value |
| acore `platform_core.h:468–472`；`rom/app/drivers/drivers/hal/dma/romable/hal_dma.h:301–309` | 源码称 SMDMA 为 **Small DMA** | “Secure DMA”属性、由哪个核心独占；`DMA_WITH_MDMA=NO` 的旧宏也不能否定已实测的 MDMA |
| `src/drivers/chips/ws53/rom/app/drivers/chips/ws53/porting/dma/dma_porting.c:19–29` | 实际移植层只把 `DMA_BASE_ADDR` 绑定至 v151 HAL | SDMA 使用同一 v151 HAL |
| `src/drivers/chips/ws53/rom/app/drivers/drivers/hal/dma/Kconfig:17–21`；同目录 `hal_dma.c:12–22,46–59` | SDMA 分支依赖 `DMA_USING_V100`，引用 `g_sdma_base_addr`；整个 SDK 文本检索只找到其引用，没有定义 | 不能把条件编译残留当成有效第二实例的驱动 |
| 同目录 `romable/v151/hal_dmac_v151_regs_def.h:213–239`；porting `include/dma_porting.h:28–29` | MDMA 槽间距 `0x20`，槽内 LLI/dest/cfg/src/ctrl 顺序；八槽数组注释明确写 M_DMA | SDMA 寄存器偏移；实物 MDMA 也只验收四槽，不能据数组长度声明八槽均可用 |
| porting `include/dma_porting.h:49–83`；`dma_porting.c:49–68` | MDMA request 编码 0–15；16 及以后注释为不存在；mux/busy helpers 是返回原值/恒 idle 的占位实现 | SDMA 请求源、物理 mux 切换、外设请求已静止的证明 |
| `src/drivers/chips/ws53/include/hal_clocks.h:52–75`；`include/clock_recover.h:94–95` | MSUB normal `0x520003e0` / sleep `0x520003e4` 的 SDMA clock 位索引 4 | 强制停机、读回成功、posted write drain、复位位或复位时序 |
| `src/drivers/chips/ws53/clock_recover/clock_recover.c:18,150–175` | generic DMA auto-CG 寄存器地址 `0x52001100`；SDK 写 32 位零以开启自动门控 | 零不是“关闭 DMA”；字段布局、适用 MDMA/SDMA 的拆分及 reset value 未给出 |
| `src/kernel/non_os/non_os/non_os.h:108–129`；`non_os_nmi.c:19–21,26–46,59–62` | NMI enable/raw/masked 寄存器 `0x52000700/704/708`，SMDMA 为 bit5，16 位访问 | NMI clear 副作用、DMA 完成 IRQ、bus idle 或 master ownership |
| `src/drivers/chips/ws53/include/acore/chip_core_irq.h:45–47` 对照 `interrupt/interrupt_adapter.c:69` | 正式 +31 为 `SDIO_INTR1_FROM_CSUB`，+32 为 M_DMA；旧优先级表把 +31 注释成 M_SDMA | 不得从旧注释给 SDMA 创建 IRQ57；不能假定借用 M_DMA 的 IRQ58 |
| `src/drivers/chips/ws53/rom_config/acore/acore.sym:70–102` | ROM 导出 v151 MDMA 函数/单基址 setter；本次未找到 SDMA 专用 symbol | 函数名 `hal_dma_reset` 不能解释为 SDMA 总线/系统复位；其实现未在当前源码中提供 |

定向全文检索 `g_sdma_base_addr`、`SDMA_BASE_ADDR`、`HAL_DMA_TYPE_SDMA`、
`CONFIG_DMA_SUPPORT_SMDMA` 及 DMA/reset/clock 组合后，仍未找到可绑定至
`0x520a0000` 的寄存器结构、活动/排空状态、独立复位或请求路由实现。
这个结论仅适用于当前 SDK 源码树，**不证明芯片没有 SDMA**。可用于 RDL 的是上述
具名系统 gate/NMI 字段（当前 PAC 已有）及实例来源元数据；不能据此新增可读写 SDMA
register block，更不能复制 WS63 或 MDMA 的 block。

Flash 启动链的源级证据比应用 RAM 接管更有利，但尚须与实际固件绑定：

- `src/build/config/target_config/ws53/menuconfig/acore/ws53_flashboot.config:136,257,308`
  禁用 DMA、SFC DMA、UART DMA；`:261` 禁用 SFC 数据缓存支持。
- `src/build/config/target_config/ws53/config.py:79–104` 的 FlashBoot 组件表不含 DMA，
  同时定义 `BUILD_APPLICATION_ROM`；上述 config `:41` 开启 ROM API。
- `src/build/config/target_config/ws53/menuconfig/acore/ws53_romboot.config:170` 也禁用
  DMA 驱动，但板上 BootROM 内容不能仅由这份配置证明。
- `src/bootloader/flashboot_ws53/startup/riscv_init.S:14–19` 清 PMP 配置，未在此文件
  写 DCCTL；`startup/main.c:506–521` 经 Flash/SFC、升级/安全相关调用后跳到应用，
  没有显式 MDMA/SDMA drain。需审查实际启用的下层调用及保留镜像身份，不能仅凭
  `start_fastboot` 无 DMA 字样证明所有旧 bus master 从未运行。
- `src/kernel/liteos/liteos_v208.5.0/Huawei_LiteOS/arch/riscv/include/arch/cache.h:70–76`
  确认 `DCCTL=0x7c1`、enable bit0；`src/drivers/chips/ws53/porting/pmp/acore_pmp_cfg.c:48–60`
  描述应用 RAM 配置。Flash 的 `PMP=0` 与实测 `DCCTL=0` 是不同前提，不需要也不能
  偷改成 RAM fixture 的锁定配置。

下一步应先核对实际保留 FlashBoot/ROM 的无旧 DMA/无脏别名路径，再决定是否已有足够
证据建立保守 Flash handoff；若依赖 SDMA 状态，必须取得 WS53 专属布局来源再补 RDL。
不能用重复读取某未知寄存器总是零来代替布局来源，也不能把 NMI 状态零等同于总线静止。

补充交叉审查发现两个必须单独处理的问题：

- 既有 `ram-probe-01` 的 16 位 MSUB 写与 `ram-probe-02-word-msub` 的 32 位写都
  在 `0x520003e0` 请求 `0xeb`、读回 `0xfb`。因此不能把 gate 失败直接归因访问宽度。
  SDK 当前可见 MSUB 操作使用 32 位；PAC 的 16 位视图需要来源核对，但这些结果本身
  不证明 16 位访问非法，本次未改寄存器描述。
- 当前 QEMU `hw/riscv/ws53.c:199` 复用 `ws63_register_custom_csrs()`；
  `hw/riscv/ws63.c:239–250,349–364` 把 `0x7c0..0x7ff` 全部设为读零/写忽略，
  包括 ICCTL、DCCTL、MEMATTRL。`hw/riscv/ws53_mdma.c.inc:423,431` 的 MDMA clock
  默认开启，虽有可注入 clock 输入，但机器未把 MSUB 门控接到该输入。不能让
  `DCCTL==0` 或模型默认时钟成为新的 handoff 资格；必须先实现明确的状态与反例模型。

模型源码路径以独立的 `hisi-riscv-qemu/src` 仓库为准。RAM async 的有限轮询不使用
WFI；任何后续改为 WFI 的 Flash async 方案还需验证睡眠门控，不能继承忙轮询结果。

## 保留启动链的实际镜像审计

磁盘审计见
[`ws53-flash-dma-boot-audit-2026-10-24`](../../docs/plan/evidence/ws53-flash-dma-boot-audit-2026-10-24/README.md)。
本轮没有连接设备或烧录；Flash DMA handoff 仍未关闭，但范围已缩小：

- 原厂串口回读中的主、备 FlashBoot 正文相同（41984 字节，SHA-256
  `8b54ff160782411db66cd5db80389978563d3bdba64242213740d10e5ce31e0d`）；
  07 的 before/expected 镜像完整保留两份启动分区。实际 SSB 与 SDK 同长度正文有
  9315 字节不同，因此不能声称完整启动链与 SDK 提交一致。
- 实际 FlashBoot 入口只清 `pmpcfg0/1/2`，并非全部 PMP；其已核对入口没有写 cache CSR。
  旧 stack/data/BSS 位于 `0x20054000..0x20058d20` 内，与 Rust RAM 重叠。
  必须在覆盖之前核实旧主设备交接；内存重叠本身不能证明在途访问。
- 正常正文验证调用 HASH ch1。消息来自 Flash，padding 在栈 RAM；候选 HAL 只有
  输入描述符，摘要由 CPU 从寄存器读到 RAM，再由 CPU 复制。这里证实需要 RAM
  **读取**，没有证实 HASH 输出 DMA。实际 FlashBoot 对两次 ROM wait 返回值和最终
  摘要都检查成功才跳应用，不能仅因使用 HASH 就认定正常返回还有在途写。
- 六个实际 ROM 跳板与 SDK 地址一致，但当前没有覆盖这些入口的 ROM dump。
  SDK 候选 unlock 只清 ownership，不能取代对实际 ROM 完成语义的核对。
  下一轮最小只读取证是 `0x14126..0x14618`（右开，1266 字节），两次读取比对，
  跟踪 LAST 完成、摘要读取和解锁；是否再读范围外代码以实际分支为依据。
- boot 专用 SYMC 确实能写 RAM，不能被通用 DMA 配置关闭所排除；其超时后 deinit
  只解锁的候选路径没有 drain 证明。不过原备份和 07 的 `0x3f2000` FOTA flag
  扇区一致，head/end/complete 均为 `0xffffffff`，按已核对 SDK 条件无升级请求。
  这是当前保存镜像的条件事实，不等于 SYMC 普通启动必然执行，也不覆盖未知 SSB。

如需应用入口状态观察，证据目录给出了源自 SDK 的 SPACC ch1 候选只读地址；
必须先审访问副作用并补 RDL → SVD → PAC，不在固件散写裸地址。单次 idle/ownership
快照不等于物理 drain。优先核对实际 ROM 正常完成语义，再决定必要快照与交接协议，
不主动停钟、清请求或复位未知所有者。此次审计没有新增 Flash DMA 稳定能力标记。

## 实际ROM及当前回调取证增补

上述只读步骤现已执行，原始镜像留在私有硬件目录；公开证据记录哈希和解码结果：

- [HASH主体](../../docs/plan/evidence/ws53-hash-rom-read-2026-10-24/README.md)：实际LAST
  等待检查并应答完成位，随后CPU读取摘要；unlock本身只释放ownership。
- [helpers与跳表](../../docs/plan/evidence/ws53-hash-rom-helpers-read-2026-10-24/README.md)：
  cache helper实际为空返回，不能把名称当成已执行cache维护；SHA-256分支已核对。
- [当前回调与最终健康](../../docs/plan/evidence/ws53-hash-rom-callbacks-read-2026-10-24/README.md)：
  当前地址转换回调指向单条ret，CPU类型走ACPU默认分支。双读一致，最终600次SWD
  读取零错误，CPU暂停/单步/继续正常；未烧录、复位、调用HASH或启动DMA。

这些结果缩小了HASH正常完成路径的不确定性，不能外推回调表的整个启动历史、
物理地址无别名、cache属性、SSB或其他master的交接。因此上述Flash DMA接入条件
仍未全部满足，暂无可烧录的MDMA Flash配置，也没有新增安全内存回收能力。

## cache状态、SSB调用闭包与仿真异常修复增补

本节是2026-10-24的离线增量，没有设备访问或Flash DMA执行。公开证据见
[交接审计与模型修复](../../docs/plan/evidence/ws53-flash-dma-cache-audit-2026-10-24/README.md)。

SDK实际`libriscv.a/cache.c.obj`补充了头文件无法保证的边界：enable仅接受size编码
0/2/4/6，头枚举中的64KiB编码8会被该库拒绝；维护指令写CSR并执行部分CPU fence，
没有已证实的完成轮询。`MEMATTRL`属性3明确为不可缓存、**可缓冲**，不能作为没有
posted write的证据。当前runtime在RAM所有权/门控检查之后写入data/BSS，但还没有
cache/PMP前置检查；未来检查若要保护覆盖过程，须放在这些RAM写入之前。

实际SSB应按`0x20300`解码，旧私有反汇编误沿用FlashBoot的`0x28300`基址；旧公开
HASH审计没有依赖该错误SSB地址。正确解码后，FlashBoot头和正文由CPU memcpy搬到
`0x28000/0x28300`，校验后跳应用加载入口。初始化表中具名UART/pinctrl回调及
pre-FOTA的CPU分配、SFC命令读/CPU拷贝、释放和progress注册已逐边收窄。保存的
108字节全ff标志不走已核对的条件写/升级执行分支；实际SYMC描述符仍证明它具有
RAM写能力，PKE条件验签另有ROM路径。这些条件事实不覆盖全部早期init、IRQ hook、
其他间接边、BootROM或cache历史，不宣称整个启动链从未使用其他bus master。

独立QEMU开发分支`feat/ws53-cache-state`的本地提交`85b2e1f`补充严格CSR模型。
默认`cache-state-profile=unknown`，不再把WS53 `0x7c0..0x7ff`静默读成零；显式
`observed-flash-baseline`只提供ICCTL6、DCCTL0、MEMATTRL33333333的历史观测状态及
同值写假设。启用/改尺寸、改变属性、维护/VA/预取和未知项均有明确诊断与异常，
不清除其他master工作，不授予缓存一致性或DMA安全资格。该拒绝表示模型能力不足，
不表示芯片真的会拒绝相同指令。

真实TCG反例进一步发现共享CPU缺陷：旧BS21 ROM stub范围`0x10000..0x40000`
覆盖WS53 ITCM，使`0x2802c`的非法CSR异常被误当成ROM调用、返回到ra=0，再发生
指令访问异常。新增每CPU的板级配置让WS53禁用两条旧ROM拦截，配置跨CPU复位保留；
旧WS63/BS2X保持原行为。修复后2186次CSR操作、450次预期异常、18次ITCM异常/mret、
30次旧ROM调用、完整41组WS53聚合、36项优先级、11项qtest及旧ROM烟测通过。
此前失败、测试诊断前缀预期错误及工具名缺失的跳过记录均保留。

新开发二进制`5a885547…4d0b5`不替换冻结`d40673cb…230e`或已验证CI构建
`b1e35004…ed584e`；原正式CI helper仍固定先前的源码政策，没有借新模型报告
授予候选09硬件资格。新版仅本机v10.0.0验证，未发布或运行远端CI。上述软件修复
不能关闭Flash DMA、物理排空、缓存历史或冷启动验收。
