# QEMU 模型

这一篇讲 [hisi-riscv-qemu](https://github.com/hispark-rs/hisi-riscv-qemu)（WS63/BS2X 的
QEMU 仿真）**为什么存在、它的能力边界在哪**。一句话先说结论：它让你在**没有真硅片、没有
RF 的情况下**把绝大多数开发——内存布局、启动、外设逻辑、中断投递——都验证掉，
但它**模拟不了的那部分**恰恰是 [HIL](07-hil-framework.md) 必须上板验的部分。把这两件事的边界
看清楚，就理解了整套验证策略。

## 为什么要造一个 QEMU 板卡

最朴素的理由：**真硅片稀缺、RF 难搞、迭代慢**。如果每改一行 HAL 都得打包、烧片、接串口看
输出，开发会被硬件可用性卡死。QEMU 给的是一个**软件在环**的快速反馈环——`cargo build` 完
直接 `-kernel firmware.elf` 就能看它跑、能 GDB 调、能跑确定性回归。

为什么不用现成的 `-M virt`？因为**固件是按 WS63 的真实地址链接的**——外设在 `0x4400_xxxx`、
flash 在 `0x200000`、SRAM 在特定高地址。在 `virt` 上，固件第一次访问 WS63 外设就会
fault。所以必须有一个**地址布局和 WS63 一致**的板卡。

为什么不做"树外插件"？因为 **QEMU 没有稳定的树外板卡 ABI**。自定义 SoC 的标准做法
（Espressif 的 esp-qemu 也是如此）是 **fork 一个固定版本的 QEMU、加一个 in-tree 板卡文件**。
hisi-riscv-qemu 正是这么做的：加 `hw/riscv/ws63.c`，只构建 `riscv32-softmmu`。

## 它建模了什么

这个模型的覆盖面相当完整，远不止"能跑起来"：

- **CPU**：命名核 `-cpu ws63` = **RV32IMFC**（I/M/F/C + Zicsr/Zcf，关 A/D、无 MMU），
  和真硅片的 ISA 一致——包括"没有原子"这一点。
- **xlinx 自定义 ISA**：HiSilicon riscv31 的一批私有指令。这是为了能跑**厂商 gcc 编的
  C SDK 固件**——有了它，Rust 固件（标准 RV32IMFC）和厂商 C SDK 固件可以**对照交叉验证**
  内存映射、启动、外设时序。
- **全部 35 个 SVD 外设**：不是"catch-all 黑洞"，而是逐个建模。其中很多是**行为完整**的——
  DMA 真的搬内存、Timer/RTC/WDT 真的计时和触发中断、I2C/SPI/I2S 真的回环 FIFO、
  LSADC 真的出采样、EFUSE 真的走 OTP 按位或、GPIO 是真实信号网（bank 内回环 + 跨 bank
  板级连线 + 可外部驱动）。少数配置类寄存器（晶振/RF/PHY 相关）是读回影子。
- **时钟树**：时钟门控生效（清门会冻结定时器、置位恢复）、源路由（TCXO/PLL 选择）建模为状态。
- **中断**：两类都端到端投递——IRQ 26–31 走标准 `mie`，IRQ ≥32 走 HiSilicon 自定义
  `LOCIxx` CSR（经 target/riscv 补丁实现），并强制 `LOCIPRI` 优先级 + `PRITHD` 阈值。
- **`-icount` 确定性指令计时**：开了之后虚拟时间绑定指令数，**同一固件每次运行结果完全
  一致**（实测 1e6 循环三次都是同一个 tick 数）。这让 CI 回归可重现。

这是一个**真值驱动**的模型：外设基址/寄存器对着 `WS63.svd`，内存布局对着
`hisi-riscv-rt` 的 `memory.x`/`layout.ld`，UART 行为对着 HAL 的 `uart.rs` + SDK 头文件。
也就是说它和真硅片共享同一批真值来源——这是它能当"软件在环替身"的前提。

## 它模拟不了什么

诚实地划出边界，比夸大覆盖面重要得多。QEMU **本质上**模拟不了这几类东西：

- **RF / PHY / 模拟量**：射频前端、PHY 事件、真实无线收发——这是物理边界，仿真器里没有可
  观测行为。（连接性因此走"合成 MAC 在 netif 缝合点"的软件在环底座，而**不**仿 RF；
  BS2X 的 BLE/SLE 在 radio-MMIO 层模拟已被论证是死胡同。）
- **真实时钟频率与时序**：TCG 不模拟流水线/cache/逐指令周期。`-icount` 给的是 IPC=1 的
  确定性近似，**不是**真实微架构周期精确。更要命的是——**QEMU 的 chardev 不限速**，
  所以 UART 哪怕波特率算错了，它照样把字节原样吐出来，**根本不会暴露波特错误**。
- **真实 flash 内容**：flash XIP 窗口是 RAM 背靠的，默认空白。分区表、NV、出厂标定
  （`xo_trim` 这类逐芯片烧录的键）QEMU 里**天然没有**——只能用 `-device loader` 回填一份
  构建出来的，而出厂标定值任何构建产物都不含。
- **掩膜 ROM / app ROM 的真实内容**：那些是从硅片读出的专有 dump，不在仓库。厂商 blob 里
  会跳进这些 ROM 地址，所以连接性的 blob 难以脱离真硅片——这也是 QEMU 的天花板之一。

## QEMU 跑 vs 真硅片跑：根本差别

这是最该记住的一张对照表——它直接解释了为什么"QEMU 过了"不等于"硅片能跑"：

| | QEMU（`-kernel ELF`） | 真硅片 |
|---|---|---|
| 镜像 | **裸 ELF**，`load_elf()` 解析并按物理地址落段 | 带 **0x300 头部**的镜像，烧到 app 分区 |
| 引导链 | **没有** flashboot；复位向量直接设成 ELF entry | mask ROM → loaderboot → flashboot 跳 `+0x300` |
| 时钟 | 取**标称值**（240 MHz PLL / 24 MHz TCXO），chardev 不限速 | **真实**频率，会暴露分频/波特/PLL 锁定的真相 |
| RF | **不仿** | 真实射频 |

换句话说，QEMU 替你做了 flashboot **不做**的事（理解 ELF、跳对入口），又**省略**了
flashboot 做的事（镜像格式、引导链）——所以"裸 ELF 在 QEMU 里能跑、在硅片上不能跑"
不是矛盾，而是这张表的直接推论（详见 [启动流程](02-boot-flow.md)）。

## QEMU↔HIL 的 parity 思路

把上面拼起来，整套验证策略就清楚了：**QEMU 负责证明逻辑对——内存布局、启动序列、
外设行为、中断投递、DMA 搬运；HIL 负责证明 QEMU 证明不了的物理现实对——真实时钟、真实波特、
真实外设时序、引导链、RF。** 两者验的是**不同的东西**，不是冗余。

理想形态是 **parity**：同一个固件（比如 `uart_hello`），QEMU 里看到什么标记串、硅片上就该
看到同样的标记串。QEMU 先把逻辑钉死，硅片再把物理现实钉死，两边比对。一旦硅片上的输出和
QEMU baseline 分歧，那个分歧**几乎必然落在 QEMU 模拟不了的那几类**——波特、时钟 10× 偏差、
引导挂死、IRQ 投递、外设接线。这正是 [HIL 框架](07-hil-framework.md) 那套 triage 的出发点。
模型的逐外设建模矩阵、xlinx ISA 细节、NV 回填等见 hisi-riscv-qemu 仓库的
`docs/design.md`。
