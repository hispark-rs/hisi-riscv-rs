# ws63-rs 架构评审台账（2026-05）

> 评审日期：2026-05-31 · 评审对象：ws63-rs 单仓多 submodule 工作区（ws63-pac / ws63-hal / ws63-rt / ws63-flashboot / ws63-examples / ws63-RF / ws63-svd / ws63-guide）

## 方法

本次评审采用 **47-agent 多 agent 工作流 + 对抗式验证** 的方式进行。评审被切分为 6 个维度（PAC+SVD 生成层、HAL 核心架构、HAL 外设驱动、运行时与二级 bootloader、构建/依赖/CI、战略方向），由多个独立 agent 并行取证。每一条候选发现都经过一次对抗式 refute（逐条尝试反驳：寻找反例、核对 fbb_ws63 C SDK 与 PAC/SVD/源码行号、复核 `cargo tree`/`cargo build` 实测输出），只有抵住反驳的发现才保留。

- 共产出 **41 条已确认发现**，对抗式验证 **0 条被驳回**（refutedCount = 0），另有若干条由 `confirmed` 降级为 `partial`（结论部分成立、证据有保留）。
- 严重度经验证修正后（以 `corrected_severity` 为准，部分较 agent 初判 `claimed_severity` 下调）：**严重 6 条 / 高 16 条 / 中 15 条 / 低 4 条**。
- 取证方法以 fbb_ws63 官方 C SDK 与 esp-hal 为地面真值（ground truth），所有寄存器/偏移/序列均与 C SDK `*_reg.h` / `*_porting.*` 对照。

## 总体结论

发现可归为三类问题：

1. **方向问题（最致命）**：WS63 是 Wi-Fi 6 + BLE + SLE 连接 SoC，但仓库实现了 **0% 连接性**。ws63-RF 只有厂商 `.a` blob、C 头文件与未实现的 porting 桩（`port_*.h`），没有任何 Rust 绑定、build.rs、bindgen、链接，也没有实现约 70 个 porting 函数或到 Wi-Fi/BT device-MAC 的 HCC IPC（host↔device 传输；WS63 单核，DMAC 是同核软件库，非第二颗核）。当前能点灯，但发不出一个数据包。精力集中在容易的 20%（外设驱动），承重的 80%（IPC / OAL / FRW / 链接 blob）尚未开始。
2. **构建完整性问题**：工作区曾因双 PAC（git 源 + path 源）实例并存而 **blinky 无法链接**（`DEVICE_PERIPHERALS` 符号重复）；默认 target `riscv32imafc` 启用了芯片并不具备的原子扩展；release 流水线发布与产物 glob 均不成立；CI 用 `|| true` 屏蔽失败。本类问题本轮（2026-05-31）已大部完成修复，详见各条"状态"。
3. **正确性地雷 / 过度设计**：存在确定性寄存器 bug（SPI trsm 误设为 EEPROM-Read）、伪装成正确的桩（`software_reset` 执行 `ebreak`、`reset_reason` 恒返回 PowerOn、GPIO pull 被静默忽略、interrupt.rs 建在不存在的 PLIC 模型上）、以及大量从 esp-hal 照搬但无任何调用方的死代码（RAII 时钟守卫、DriverMode Async 标记、DmaChannelFor、safety.rs 的同义反复 const_assert）。

被评审者**没有任何在真实硅片上运行过的证据**：唯一示例 blinky 用手写忙等绕过了 HAL 自己的 timer/delay；git 历史由"code review fixed N bugs"式的自我评审主导；测试是 host 侧 proptest，复述代码自身的常量（含错误的 DMA ID）而非硬件真值。

---

## 进展更新（截至 2026-06-03）

> 本台账是 **2026-05-31 的时点快照**；上文结论按当时事实保留。以下汇总此后已解决项（各维度表内的"状态"列亦已逐条标注）：

- **正确性地雷（阶段 2，大部已修）**：interrupt.rs 重写为 WS63 自定义 `LOCIPRI`/`LOCIEN`/`LOCIPD` CSR 模型（+ 优先级/阈值）；I2C/SPI 加超时并返回错误；`software_reset`/`reset_reason` 真实化；GPIO pull + 中断触发接通；`safety.rs` 恒真断言删除；死代码（async marker、RAII 时钟守卫）删除。全部在 ws63-qemu 端到端验证。SPI `trsm`、eFuse/LSADC 逐寄存器复核仍在推进。
- **连接性（最致命方向，已大幅推进）**：in-tree crate **`ws63-rf-rs`** 实现了 porting 契约（`osal_adapt_*` 33 符号 + oal/log/uapi + 协作调度器 + FRW 工作线程 + HCC 传输 + 软件计时器 + netif→smoltcp 桥）；**Wi-Fi-init 符号闭合达成**（`--gc-sections` rooted `uapi_wifi_init` 残留 2）。不再是"0% 连接性"。剩余是真机 HIL（ROM 地址 + 厂商自定义重定位只在硅片上成立）。
- **异步（超出原评审范围，已实现）**：ws63-hal 新增 `async`/`embassy` —— `embedded-hal-async`/`embedded-io-async` 全套（DelayNs/Wait/SpiBus/I2c/Read/Write）+ embassy-time `Driver` + embassy-executor 多任务，全部 ws63-qemu 验证（见 [async-embassy.md](../architecture/async-embassy.md)）。
- **构建 / CI / 发布**：各 crate 改为**自有仓库流水线**发布到 crates.io（pac 0.1.3 / rt 0.1.1 / hal 0.2.1）；监仓 tag 只出固件 Release。默认 target = builtin `riscv32imfc`。
- **软件在环验证**：姊妹仓 **ws63-qemu**（`-M ws63` + `-cpu ws63` + xlinx 自定义 ISA）建模全部 35 外设，`smoke-test.sh` 跑真实 ws63-rs 固件 + C SDK 样例 + 寄存器级 qtest —— 大幅替代了原"恒真式测试"。真机 HIL 仍待补（阶段 1 尾）。
- **目录重构**：ws63-svd 收为 ws63-pac 的嵌套子模块；ws63-RF 收为 ws63-rf-rs 的嵌套子模块（防依赖横向扩展）。

## 已确认的优点

按维度汇总，作为修复时不应破坏的资产：

- **PAC + SVD**：已建模外设质量高——UART0 有 24 个寄存器、完整 enumeratedValues、读写访问标注；SVD 自洽（497 寄存器 / 36 外设，使用 derivedFrom、addressBlock、writeConstraint、resetValue，并有按 ARM CMSIS-SVD XSD 校验的 validate.py）；svd2rust 0.37.1 为当前版本，约 6s 编译完成。KM keyslot bitfield 与 C SDK `hal_keyslot_reg.h` 逐位一致。
- **HAL 核心**：`clock_init.rs` 是亮点，TCXO 检测、flash→PLL 切换序列、UART/SPI 时钟门控序列与寄存器地址（0x44001134/0x44001104/0x400034A4）均与 fbb_ws63 逐寄存器一致——这是唯一被真正对照地面真值验证过的一层。peripheral 单例 + `'d` 生命周期 ZST 设计正确，委托给 PAC 真实的 critical-section 单例。GPIO block/group 映射、DMA/SDMA 基址（0x4a00_0000 / 0x520a_0000）、cken 位表均与 C SDK 一致。
- **HAL 驱动**：embedded-hal 1.0 / embedded-io / embedded-hal-nb trait 实现到位且 error 类型合理（SpiBus 而非 SpiDevice、I2c transaction、SetDutyCycle、DelayNs 选型正确）；I2C transaction 正确使用 repeated-START、仅在末尾 STOP；通过具名 PAC 访问器的位定义与 C SDK 一致；算术溢出处理稳健（saturating clamp / u64 中间量）。
- **运行时与 flashboot**：两个 startup.S 均实现了规范的 RV32 启动序列（PMP 清除、mtvec、关中断、FPU FS=11、gp `.option norelax`、sp/BSS）；硬件地址与 fbb_ws63 SDK 交叉验证一致（FLASHBOOT_RAM 0xA28000、FLASH_BOOT_TYPE 0x40000024、MAIN/BKUP 魔数）；sha256.rs 是正确、无分配的 no_std 实现；ws63-rt 异常/IRQ 汇编经过认真设计（独立异常/IRQ/NMI 栈、mscratch 交换、完整 save_all/restore_all 含 ccause 0xFC2）。
- **构建**：工作区通过 `[workspace.package]`/`[workspace.dependencies]` 集中版本；release profile 适合嵌入式（opt-level="s"、lto、codegen-units=1）；HAL 正确引入 portable-atomic；CI 覆盖 per-crate check / clippy -D / fmt / cargo-audit。
- **方向**：底层 HAL 广而架构自洽（31 驱动覆盖 35 外设）；坚持对照 fbb_ws63 C SDK 验证寄存器行为；ws63-RF 复用厂商 `.a` 协议栈而非重写协议栈是正确战略；ws63-guide 逆向文档是稀缺 IP。

---

## 发现台账

严重度图例：**严重** / **高** / **中** / **低**。"状态"依据本轮（2026-05-31）实际进展标注：**本轮已修** / **已排期（阶段 N）** / **暂不修**。验证结论 `partial` 在标题旁标注。

### 维度一：PAC + SVD 生成层

PAC 是手工维护的文件伪装成生成代码：SVD 为手写 CMSIS-SVD（非厂商抽取），lib.rs 经 cargo fmt 后续被手工补丁直接打入，无再生脚本、无 CI 再生校验、环境中未安装 svd2rust。

| 严重度 | 类别 | 标题 | 证据 | 影响 | 建议 | 状态 |
|---|---|---|---|---|---|---|
| 高 | maintainability | PAC 寄存器是手工补丁打入生成代码，而非再生——任何 clean 再生会静默丢失 | SVD `587b65a` 与 PAC `df35d69` 相隔 96s，df35d69 仅改 src/lib.rs 231 行（加 4 个 KM 锁寄存器）；`3a1c5ec` 曾 cargo fmt 重写 42976 行；main.py 仅 `print("Hello...")`；CI 无 svd2rust | PAC 是伪生成文件，下次从 WS63.svd 再生 + fmt 会产出迥异文件，手工补丁丢失；SVD→PAC 链接只靠约定、CI 不可验证 | 提交固定 svd2rust 版本的再生脚本，CI 增加 diff-check，禁止手改 lib.rs | 暂不修（再生流水线非本轮范围；本轮已 bump 0.1.0→0.1.1） |
| 中 | dependency | ws63-pac 停留 0.1.0 但 tag 后有 API 新增，发布不遵守 SemVer（partial） | tag 仅 v0.1.0，`v0.1.0..HEAD` 含 df35d69 公有 API 新增；Cargo.toml 仍 0.1.0；release.yml 对任意 `v*` tag 直接 publish 无版本校验 | 下游无法区分含/不含 KM 寄存器的 0.1.0；重发 0.1.0 违反 SemVer 且会被 crates.io 拒 | 升 0.1.1/0.2.0 并打 tag，CI 增加版本不同于已发布版的守卫 | **本轮已修**（ws63-pac 已 bump 0.1.0→0.1.1） |
| 高 | dependency | ws63-hal 经未固定 git 源依赖 ws63-pac，Cargo.lock 含两份分叉 PAC | ws63-hal/Cargo.toml:12 git 无 rev/tag；根 Cargo.toml:19 用 path；Cargo.lock 同时有 path 0.1.0（745 行）与 git#df35d69（754 行） | HAL git 依赖会浮动到默认分支 HEAD，消费者可能拿到与 submodule pin 不同的 PAC，寄存器布局错配（硬件 UB）且无编译错误 | 固定 git dep 到精确 tag/rev，或统一用 workspace path | **本轮已修**（ws63-hal 改 registry 版本依赖 + 根 `[patch.crates-io]` 指向本地，`cargo tree` 单一 ws63-pac 实例） |
| 中 | correctness | 被"修复"的 KM 块内仍有覆盖缺口：4 个 `*_FLUSH_BUSY` 寄存器缺失 | hal_keyslot_reg.h:21-24 定义 0xB10/0xB14/0xB18/0xB1c；SVD 与 lib.rs 中 `flush_busy` 计数为 0；df35d69 把 0xB10-0xB2c 折叠为 `_reserved16` | keyslot flush 完成无法经 PAC 轮询，HAL 须裸指针穿透 reserved，违背 PAC 类型安全初衷；暗示其它块也有静默缺口 | 在 WS63.svd KM 外设补 4 个 FLUSH_BUSY 并再生；逐块对照 fbb_ws63 `*_reg.h` 审计 | 已排期（阶段 2，efuse/lsadc/keyslot 寄存器修复） |
| 中 | build | 无自动化/有文档/环境可用的生成流水线；ws63-svd 工具链实质为空 | main.py 是桩；README 0 行；svd2rust 未安装（`which svd2rust` 退出 1）；无 Makefile/justfile/*.sh 再生脚本 | "SVD→svd2rust→PAC"流水线只存于口口相传，新贡献者无法复现 PAC，高 bus-factor | 文档化并脚本化完整流水线（固定 svd2rust + form + flags），填充 README，删除或实现 main.py | 暂不修（与"手工补丁"条共属再生流水线，非本轮范围） |

### 维度二：HAL 核心架构

一层真正扎实（clock_init.rs）与大量从 esp-hal 照搬、无调用方、且部分硬件模型错误的脚手架并存。

| 严重度 | 类别 | 标题 | 证据 | 影响 | 建议 | 状态 |
|---|---|---|---|---|---|---|
| 严重 | correctness | interrupt.rs 建在错误的中断控制器上（WS63 用自定义 CSR 本地 INTC，非 PLIC）且为非功能桩 | interrupt.rs:1 文档"RISC-V PLIC-based"；enable/bind_handler 忽略参数仅调全局 `riscv::interrupt::enable()`，disable() 为空；fbb_ws63 interrupt_handler.c:125-129 用 `write_custom_csr_val(LOCIPRI0..4)`，本 SoC 无 PLIC | 整个中断 API 失效：无 per-source enable、无优先级、无 handler 绑定；任何依赖它的 GPIO/UART/DMA 中断都无法工作，且模型方向性错误 | 围绕 riscv31 自定义 CSR（LOCIPRI/LOCIEN/LOCIPD）+ chip_core_irq.h 编号重写，或移入 ws63-rt；移除 PLIC 措辞；未实现前显式标桩 | ✅ 阶段 2 已修（2026-06-01）：`interrupt.rs` 按 fbb_ws63 `arch/riscv/riscv31/interrupt.c` 重写——IRQ 26–31 走 `mie` 位、≥32 走 `LOCIEN0-2`（base 32）、4-bit 优先级 `LOCIPRI0-15`（base 26）、`LOCIPCLR` 清挂起、`PRITHD` 阈值、默认优先级 `0x11111111`；提供 `enable/disable/set_priority/threshold/clear_pending/is_pending/init/enable_global/free`；移除 PLIC 措辞与零消费者的 `InterruptConfigurable`/`InterruptHandler`。`timer_irq`(mie)/`gpio_irq`(LOCIEN+LOCIPCLR) 改用此 API，经 ws63-qemu 端到端验证 |
| 中 | architecture | RAII 时钟守卫系统（ClockControl/PeripheralGuard/refcount）是完全死代码，零消费者 | grep 仅在 clock.rs（定义）/prelude.rs（re-export）/lib.rs（doc）命中；无驱动构造器、无示例使用；blinky 用 create_output_pin(0) 且从不设时钟 | 大量维护面（unsafe Send/Sync、裸指针、原子、double-drop 断言）守护无人调用的行为；实际时钟路径是 clock_init.rs 的裸寄存器写 | 要么让驱动构造器获取 PeripheralGuard（esp-hal 的价值兑现），要么删除 refcount/guard 机制保留简单 enable_* | ✅ 阶段 2 已修（2026-06-01）：删除 `ClockControl`/`PeripheralGuard`/`REF_COUNTS`（驱动依赖复位默认开的时钟）；保留 `Peripheral` enum + `cken_info` 门控图 + `PERIPHERAL_COUNT` 作哨兵 |
| 低 | architecture | DriverMode Blocking/Async 标记过早，GAT 设计是 no-op | private.rs:32-51 `type Async<D>` 对 Blocking 与 Async 都解析为 `D`（恒等）；零驱动使用；HAL 无 async runtime | 照搬 esp-hal 标记但没有它所守护的机制；恒等 GAT 误导维护者以为存在 mode 转换系统 | 在出现 async executor 与 async 驱动前删 GAT/整个 trait，或退化为 esp-hal 的空 marker trait 并注明 async 未实现 | ✅ 阶段 2 已修（2026-06-01）：删除 `DriverMode`/`Blocking`/`Async` 空 marker；**真实异步随后已实现**（2026-06，ws63-hal `async`/`embassy`，见「进展更新」）|
| 低 | maintainability | safety.rs "编译期形式化验证"基本是同义反复式表演 | safety.rs:37-44 const_assert 比较硬编码字面量与字面量；60-63 断言 `[(); 17] == [(); PERIPHERAL_COUNT]` 而 PERIPHERAL_COUNT 即 17；文档自称"形式化安全契约" | 制造严谨假象；真正有用的跨模块断言被字面量自比埋没；"形式化验证"措辞误导 | 仅保留绑定两个独立可变值的跨模块一致性断言，删字面量自比，去掉"formal verification"措辞 | ✅ 阶段 2 已修（2026-06-01）：删除 10 条 `const X == 字面量` 恒真计数断言 + `verify_peripheral_count`，模块文档去掉"formal/verification"措辞；保留 MMIO 地址范围断言（37-44）与定时器算术溢出断言（79-85） |
| 中 | correctness | GPIO Input/Output 驱动从不应用 pull 配置或中断触发类型——InputConfig.pull 被静默忽略（partial） | gpio.rs:107-109 init_input 仅 set_oen(true)，InputConfig.pull 从不写；pull 在 IO_CONFIG pad 寄存器（pe/ps 位 9/10），驱动从不碰；enable_interrupt 仅 set int_en，未设 int_type/polarity/dedge | `with_pull(Pull::Up)` 编译通过却无效——浮空输入暗坑；GPIO 中断只按复位默认触发，无 API 选 edge/level | init 时经 IO_CONFIG 路由 pull，或移除该字段；enable_interrupt 接收触发类型并按 C SDK 编程 int_type/polarity/dedge | ✅ 阶段 2 已修（2026-06-01）：`init_input` 经 IO_CONFIG pad 寄存器（`pad_gpio_NN_ctrl` PE/PS 位 9/10，pads 0..=14）读-改-写落地 `pull`；新增 `InterruptTrigger`（边沿/电平 + 极性）+ `Input::set_interrupt_trigger` 写 `GPIO_INT_TYPE`/`GPIO_INT_POLARITY`。上拉电阻在 QEMU 数字引脚网不可观测，静态修复 |
| 高 | correctness | System::software_reset 与 reset_reason 是欺骗性桩（ebreak 而非复位；硬编码 PowerOn） | system.rs:53-65 software_reset 执行 `ebreak` 后自旋（ebreak 是调试 trap 非复位）；reset_reason 恒返回 PowerOn；真实机制在 SYS_RST_RECORD 0x40000098 / GLB_CTL_RB | 故障恢复路径调 software_reset 会挂起/陷入调试器而非重启（看门狗路径危险）；reset_reason 恒 PowerOn 破坏 boot-reason 逻辑 | 经 WDT/GLB_CTL 复位路径实现，按 SYS_RST_RECORD 解码原因；未实现前用 unimplemented!()/todo!() | ✅ 阶段 2 已修（2026-06-01）：`software_reset` 置 `GLB_CTL_M(0x40002110)` bit2 触发全芯片复位（`reboot_port_reboot_chip`）；`reset_reason` 读 `SYS_RST_RECORD_0(0x400000A0)` 解码 WDT/软件/上电、经 `SYS_DIAG_CLR_1(0x400000A4)` 清位（`reboot_port_get_rst_reason`）。ws63-qemu 新增复位模型，`reset_demo` 端到端验证往返 |
| 中 | architecture | DMA 控制器泛型设计是未测试脚手架；DmaChannelFor/DmaEligible 安全层未完成 | dma.rs:439 DmaChannelFor 无 impl（注释承认 blanket impl 省略）；DmaEligible 仅 impl 给 Spi0/Spi1；SDMA 逻辑 8-11 映射被忽略（仍按 0-3 索引）；无驱动构造 DmaDriver | 看似可用的安全绑定系统其绑定 trait 为空、通道映射部分且未验证；存在编译通过却错误编程传输的风险 | 完成 impl 并对照 fbb_ws63 dma porting 验证通道/请求映射，或降级为有文档的底层寄存器驱动并删半成品 trait | 🟡 部分（2026-06-01）：删除空的 `DmaChannelFor` 与未被调用的 `DmaEligible` 半成品安全层；保留 `DmaDriver`（内存搬运）+ `DmaPeripheral` 请求-ID 枚举。请求-ID 正确性已修（见下「DMA 请求 ID」行，2026-06-01：对齐 `dma_porting.h` + 接入 flow-control）；SDMA 逻辑通道 8–11 物理映射**已做**（2026-06-02：`DmaInstance::CHANNEL_BASE` Dma0=0/Sdma0=8，逻辑 8–11→物理 0–3，全通道方法+位操作统一物理索引，6 个映射单测；外设 DMA（mem↔SPI0 握手/flow-control）+ SDMA 通道经 ws63-qemu `dma_loopback` 例子端到端验证）→ ✅ |

### 维度三：HAL 外设驱动

统一的阻塞式寄存器 poker + 薄 embedded-hal 外壳；含至少一个确定性寄存器 bug，多个未验证/猜测的布局，以及系统性鲁棒性缺口。

| 严重度 | 类别 | 标题 | 证据 | 影响 | 建议 | 状态 |
|---|---|---|---|---|---|---|
| 严重 | correctness | SPI 传输模式字段误设为 EEPROM-Read（0b11）而非 TX&RX（0b00） | spi.rs:76 `ctra \|= 3 << 18; // TX+RX mode`；trsm 字段在 bit18-19，hal_spi.h:174 `HAL_SPI_TRANS_MODE_TXRX = 0`，11=EEPROM-Read | 全双工 transfer/transfer_in_place 在硬件上行为错误；EEPROM-Read 模式下读返回垃圾且时序错——驱动头号操作在寄存器级损坏 | trsm 设 0（删 `3<<18`）或显式暴露传输模式；对照 hal_spi_v151.c:241 | 已排期（阶段 2，SPI trsm 修复） |
| 高 | correctness | 所有 I2C/SPI 阻塞等待是无界忙等、无超时；Timeout/BusError 变体是死代码 | i2c.rs 9 个无上限 `while !...`，spi.rs 9 个同类；I2cError::Timeout/BusError 已定义但从不返回；C SDK hal_i2c_v150_wait 用 timeout_us 计数返回 ERRCODE_I2C_TIMEOUT | 缺失/掉电/卡死的从机或不置 txfnf/rxfne 的外设会让整个 MCU 永久挂起、无法恢复——对真实可能故障的总线不安全 | 加有界自旋计数返回 Timeout/SpiError 变体，照 C SDK timeout_us 模式接入既有错误变体 | ✅ 阶段 2 已修（SPI 2026-05-31，I2C 2026-06-01）：两侧全部无界 `while !..{}` 改为有界 `wait_until` → `SpiError::Timeout` / `I2cError::Timeout`（`SPI_WAIT_LOOPS` / `I2C_WAIT_LOOPS`） |
| 中 | correctness | I2C 完成轮询 int_tx/int_rx 而非 C SDK 用的 int_done | i2c.rs:80 等 int_tx（SR bit4）、:153 等 int_rx（bit3）；C SDK hal_i2c_v150_wait 轮询 I2C_INT_TYPE_DONE（bit0）作完成信号，DONE 后才查 ACK_ERR | 轮询与厂商不同的状态位在真实硅片上有竞态/早退风险（如字节完全锁存前读 RXR）；至少偏离唯一已知正确序列且从未硬件验证 | 照 C SDK：等 int_done 完成→查 int_ack_err→一起清 done\|tx\|rx；从 hal_i2c_v150_comm.c 重新推导序列 | 已排期（阶段 2，I2C 正确性） |
| 中 | architecture | DMA 基础设施完全未接线；外设请求 ID 是杜撰且与硬件不符（partial） | 无驱动 import crate::dma；DmaChannelFor 零 impl；proptest 断言的 ID（Spi0Tx=0/Uart0Tx=4/I2sTx=10）与 dma_porting.h:56-93（index0=HANDSHAKING_TIE0/UART_L_TX=1/SPI_MS0_TX=7/I2S_TX=11）矛盾 | CLAUDE.md 宣称的"编译期通道安全 DMA 层"是非功能表演：无法搬数据、安全 trait 无实例、测试"验证"的常量会在硬件上编程错误握手源 | 至少接 DMA 进一个驱动（如 SPI write）并用 dma_porting.h 正确 ID，或删死脚手架；任何人依赖前先修 ID | ✅ 阶段 2 已修（2026-06-01）：`DmaPeripheral` 请求 ID 由杜撰的 0..11 改为 `dma_porting.h` 的 `HAL_DMA_HANDSHAKING_*`（UART0=UART_L 1/2、UART1=UART_H0 3/4、UART2=UART_H1 5/6、SPI0=SPI_MS0 7/8、I2S 11/12、SPI1=SPI_MS1 13/14，均合 4-bit 字段；UART 映射据 `platform_core.h`）；经 `request_id()` + `DmaChannelConfig::mem_to_peripheral`/`peripheral_to_mem` 接入 `configure_channel` 的 flow-control/握手字段。死的 `DmaEligible`/`DmaChannelFor` 已删（见上）。proptest/单测改断真值（host 78 passed）。SDMA 逻辑通道 8–11 物理映射**已做**（2026-06-02，见上行；host 82 passed；`dma_loopback` 例子接 DMA 进真实外设路径 mem↔SPI0）|
| 高 | maintainability | 无任何受审驱动在硬件运行的证据；测试断言自定义常量 | 唯一示例 blinky 仅 GPIO，用手写忙等 delay_ms 绕过 HAL timer；无 UART/SPI/I2C/I2S/timer 示例；host 测试是 proptest 复述代码自身公式与同义反复；git log 由"fix: code review — N bugs"主导 | 结合上述具体寄存器 bug，强烈表明驱动编译干净、host 测过但从未对硅验证；断言错误 DMA ID 的 proptest 主动误导 | 视 HAL 为未验证；优先 UART/SPI 的硬件（或 QEMU/寄存器 trace）bring-up；用对照 C 驱动的序列替换同义反复 host 测试 | 🟡 部分（2026-06-01）：host 单测不再被 `\|\| echo` 掩盖——cfg 门控 ws63-hal/ws63-pac 的 riscv 耦合后，`cargo test --target x86_64` 真正编译并跑通 77 个单测，CI 有 `host-test` job；外设 bring-up 已用 ws63-qemu `smoke-test`（blinky/uart_hello/timer_irq/gpio_irq/reset_demo）+ C SDK 样例覆盖。仍缺 (a) 真硅验证（阶段 1），(b) 用对照 C 序列替换恒真/自比测试（阶段 5 示例） |
| 高 | correctness | efuse 与 lsadc 寄存器访问是自认的猜测，非来自 SDK | efuse.rs:103-118 注释"PAC 只暴露 efuse_ctl_data...内部锁存"，自旋 100 次回读同一寄存器；C SDK 按区域计算的 MMIO 地址读 eFuse（hal_efuse_v151.c:76-92）；lsadc.rs 注释承认位域不确定（12 vs 14 bit 矛盾） | 这些驱动几乎肯定读不到正确值；eFuse 尤其门控安全/校准数据，错误读路径危险——以自信 API 出货诱发静默数据损坏 | 按 hal_efuse_v151.c 区域地址方案重新实现 efuse；对照 lsadc C 驱动/SVD 解决位域歧义；验证前标实验性 | 已排期（阶段 2，efuse/lsadc 寄存器） |
| 低 | correctness | UART 硬编码 16x 过采样与零分数分频器，不编程 baud_ctl（partial） | uart.rs:102 `div = pclk/(16*baud)` 假定固定 16x，:107 写 div_fra=0；C SDK baud_ctl.baud_div 选采样率（0x7=8x/0xF=16x）从不被写，6 位 div_fra 被硬置零 | 若 baud_ctl 复位默认非 16x，所有配置波特率按比例偏移、UART 通信全坏；零 div_fra 在整除不尽时降低波特精度 | 显式编程 baud_ctl 到假定过采样因子，计算 div_fra=(remainder*64)/(16*baud) 用分数分频器 | 已排期（阶段 2，正确性修复） |

### 维度四：运行时（ws63-rt）+ 二级 bootloader（ws63-flashboot）

启动骨架真实可信，但 flashboot 三大承重宣称中两项为假（无验签、镜像头布局错），且双 PAC 曾破坏整个工作区构建。

| 严重度 | 类别 | 标题 | 证据 | 影响 | 建议 | 状态 |
|---|---|---|---|---|---|---|
| 严重 | security | flashboot 不做任何真实性验证——"verify"是攻击者可轻易重算的完整性哈希 | main.rs:225-248 verify_sha256 计算 body SHA256 与同一未签名头里的 image_hash 比对（注释自承认 fbb 用 ROM ECC/SM2）；厂商 secure_verify_boot.c:281/319 做真实签名验证，公钥链根植 efuse | 无安全启动：能写 SPI flash 的攻击者放任意镜像 + 自己的正确 SHA256，哈希匹配后 flashboot 以 M 模式跳入攻击者代码——完全绕过其宣称属性，且 banner/注释暗示在验证，比无宣称更糟 | 要么不宣传为"verification"（改称 CRC/完整性自检并注明非安全启动），要么调用 WS63 ROM cipher 用 efuse 根密钥做真实验签 | **已修（标实验性 + 如实化）**：banner/`publish=false`/移出 default-members/README（2026-05-31）；2026-06-01 取前一路径——`verify_sha256`→`verify_image_integrity`，函数+模块文档明确"仅完整性自检、非真实性验签"。真实 ECC/SM2 验签按 ROADMAP 冻结项复用原厂，不在实验件投入 |
| 严重 | correctness | ImageHeader/CodeInfo 结构布局与真实 WS63 镜像头不符——hash 与 length 从错误偏移读取 | sfc.rs:43-54 CodeInfo 把 image_length 放 +0x14、image_hash 放 +0x1C；真实 image_code_info_t 中 +0x14=mask_version_ext、+0x1C=mask_msid_ext，真实 code_area_len 在 +0x24、code_area_hash 在 +0x28 | 即便作为纯完整性检查也对真实 WS63 签名镜像失效：哈希错误字节范围、比对错误 32 字节、会拒绝每个真实镜像；image.rs 边界测试通过仅因用自洽伪造头 | 直接从 fbb_ws63 secure_verify_boot.h 重新生成 CodeInfo/KeyArea，加一个解析 SDK 真实镜像头的测试 | ✅ 已修（2026-06-01）：sfc.rs `KeyArea`/`CodeInfo` 按 `image_key_area_t`/`image_code_info_t`(ECC256) 逐字段重排，`code_area_len`@+0x24、`code_area_hash`@+0x28，`const` 断言锁定 0x100/0x200/0x300；image.rs validate()/main.rs 改读正确字段；对抗式评审(layout) ok |
| 严重 | build | 工作区拉入两份不同 ws63-pac（git vs path）；blinky 构建已坏（DEVICE_PERIPHERALS 重复） | ws63-rt path、ws63-hal/ws63-flashboot git；Cargo.lock 两份 PAC（745 path / 754 git）；`cargo tree -p blinky` 两份并列；`cargo build -p blinky --release` 失败："Linking globals named DEVICE_PERIPHERALS: symbol multiply defined!" | 验证 rt+hal+pac 集成的示例根本不链接；svd2rust 的 `#[no_mangle] DEVICE_PERIPHERALS` 在两份 PAC 间冲突，两个类型宇宙不兼容 | 全工作区选一个 PAC 源（优先 workspace path 让 submodule pointer 权威），CI 加 `cargo tree` >1 ws63-pac 则失败的 gate | **本轮已修**（hal/flashboot 改 registry 版本依赖 + 根 `[patch.crates-io]` 指向本地；flashboot 另删除未用的 ws63-pac 依赖；`cargo tree` 单一实例） |
| 中 | correctness | ws63-rt trap/exception/NMI handler 段被 layout.ld 孤立，mtvec 以 Direct 模式设置却配 Vectored 表（partial） | startup.S 声明 `.trap`/`.trap.exception`/`.trap.nmi`/`.trap.mie`/`.trap.local`，layout.ld 无对应输出段（grep trap 仅 EXTERN）；startup.S:40-41 `csrw mtvec` 无 +1 模式位（Direct）却 trap_vector 是 Vectored 跳表；flashboot 正确做 `addi t0,t0,1` 并显式放 `.trap` | 依赖 lld 孤儿放置脆弱（可能无 64 字节对齐）；Direct 模式下每个 trap 跳 trap_vector+0，per-interrupt 入口死代码，向量化设计静默失效 | layout.ld 显式 `.trap : ALIGN(64) { KEEP(*(.trap)) KEEP(*(.trap.*)) }`，startup.S 设向量模式；readelf 验证对齐与 mtvec[1:0]=01 | ✅ 已修（2026-06-01）：startup.S 以 Vectored 模式设 mtvec（`ori t0,t0,1`→0x230441）；layout.ld 显式放置 `.trap`(ALIGN 64) + `.trap.exception/.nmi/.mieN/.local`(KEEP)；栈顶 `__irq/exc/nmi_stack_top__` 统一在 `.stacks`(单一真值，被 KEEP 的 .trap 处理器引用故 GC 安全)，删除 memory.x 里指向 .heap 的错误 fallback（修潜在 trap 栈/堆重叠）。readelf/反汇编验证：.trap 64 对齐、表项 0→trap_entry/26→mie_interrupt0_handler/32-91→local_interrupt_handler 均解析且在范围内、无未定义符号；smoke 5/5。运行期分发受 fat-LTO + 跨 crate 弱符号覆盖限制未做示例（同 timer_irq/gpio_irq），IRQ 投递已由二者在 QEMU 验证 |
| 高 | correctness | AB 回退逻辑误用 FLASH_BOOT_TYPE_REG（flashboot 恢复选择器）作为 app A/B 区选择器 | main.rs:108-117 读 0x40000024 设 region；fbb_ws63 中 0x40000024 是 flashboot 自恢复标志（flashboot_need_recovery）非 app A/B 区；真实 app 区来自持久升级配置 upg_get_region_addr(upg_get_run_region())，验证失败时 SDK 调 ws63_try_fix_app()+reset() | Rust bootloader 选错槽语义并加了厂商刻意不做的进程内 fall-through；结合 read_partition_app_addr 是恒返回 FLASH_START 的桩、REGION_SIZE 硬编码，A/B 寻址实质是杜撰 | 实现真实分区表解析 + upg run-region 选择，或彻底去掉 A/B 改单镜像启动；勿把恢复寄存器当 app 槽选择器 | ✅ 已修（2026-06-01）：去掉 A/B 改单镜像启动；删除 `0x40000024` 误用，并如实注明真实 A/B = upg run-region(magic `0x70746C6C`)+分区表(`@0x200380`)、`0x40000024`=bootloader 自恢复标志（`flashboot_need_recovery`）；`read_partition_app_addr` 桩如实标注 |
| 高 | direction | 手写 Rust flashboot 相对成熟厂商 flashboot 是错误方向的投入 | 厂商 flashboot 提供 ECC/SM2 安全启动、FOTA 验证、A/B + dmmu remap、flash 在线加密、解压、看门狗/恢复；Rust 重写把分区解析/升级模式/时钟适配桩掉，验签换成未认证哈希；banner 称"All P0/P1 gaps addressed"不准确 | 大量工程投入一个处于安全关键路径、且严格弱于所替代厂商件的组件（无安全启动/FOTA/头布局错），并强制重复维护 uart/sfc/sha256/startup/linker | 降级或限定 ws63-flashboot 范围；生产用厂商 flashboot 启动 Rust app；若作学习目标则明确标实验性/不安全并排除出 release | **本轮已修（部分）**（已标实验性 + publish=false + 移出 default-members + README；战略性降级与"用厂商 bootloader"长期决策见阶段 3） |
| 中 | maintainability | flashboot 重复 HAL/RT 代码（uart/sfc/sha256/startup/linker）造成分叉风险 | flashboot 重实现 ws63-hal/ws63-rt 已提供的功能；仓库现有两份 startup.S/layout.ld/memory.x，寄存器偏移假设不同；flashboot 仍链接其宣称不用的 PAC | 寄存器级知识在两个不同步实现间分叉；HAL 的 UART/SFC 修复不会传播到 flashboot | 把真正共享的寄存器定义抽到无依赖支撑 crate（或复用单一 PAC）；至少删除 flashboot 未用的 ws63-pac 依赖 | **本轮已修**（已删除 flashboot 未用的 ws63-pac 依赖；共享 crate 抽取见阶段 2） |

### 维度五：构建系统、依赖图、submodule 策略、CI、release

工作区曾"恰好能编译"靠的是 HAL ZST 隔离与 git pin 偶然等于 submodule commit；release 流水线发布与产物两线皆断；默认 ISA 与硅片不符。

| 严重度 | 类别 | 标题 | 证据 | 影响 | 建议 | 状态 |
|---|---|---|---|---|---|---|
| 高 | build | 分裂的 ws63-pac 依赖源（git vs path）在一次构建中产出两份不兼容 PAC | hal/flashboot git、根/rt/blinky path；Cargo.lock 两份 PAC；HAL 公有 API 泄漏 git PAC 类型（spi.rs:108 `&'static ws63_pac::spi0::RegisterBlock` 等） | 每次构建编译两份同名异身 crate；消费者把 path/registry PAC 与暴露 RegisterBlock 的 HAL API 配对时得到诡异"expected ws63_pac::X, found ws63_pac::X"；并使 HAL 实际未对在编辑的 in-repo PAC 构建 | 全 crate 统一 `ws63-pac = { workspace = true }`，需覆盖 registry 用根 `[patch.crates-io]`；确认 lock 恰好一个 ws63-pac | **本轮已修**（registry 版本依赖 + 根 `[patch.crates-io]` → 本地；`cargo tree` 单一实例） |
| 高 | build | ws63-hal 的 crates.io 发布失败：git 依赖 + 不兼容的已发布 PAC | release.yml 运行 `cargo publish`，cargo 禁止 git 依赖；`--dry-run` 失败 `E0599: no method kc_rd_slot_num`（crates.io PAC 旧于 HAL 期望）；且报 `ws63-hal@0.1.0 already exists` | publish 任务会失败（continue-on-error 静默吞掉，tag 出来却无 crate 发布）；即便修了 git dep，HAL 也无法对已发布 PAC 编译；0.1.0 已被占 | 发布前移除 git 依赖；按依赖序 pac→rt→hal 串行发布；每 tag bump 版本；去掉 continue-on-error；PR 加 `cargo publish --dry-run` gate | **本轮已修**（去除 `\|\| true`/continue-on-error；clippy 改 gating 并排除实验性 flashboot；发布改依赖序 pac→rt→hal） |
| 高 | correctness | 默认 target riscv32imafc 启用 WS63 核不具备的原子（应为 rv32imfc） | .cargo/config.toml:6 `riscv32imafc`（'a'=atomic）；C SDK target_config.py:61 `-mabi=ilp32f -march=rv32imfc`（无原子）；仓库已带正确 spec `riscv32imfc-...json`（`-a`、`max-atomic-width:0`）却被注释为 nightly-only；HAL 依赖 portable-atomic 佐证核无 LR/SC | imafc 可能发出核未实现的 lr.w/sc.w，运行时非法指令 trap——一类过 CI（只 check/link 从不在硬件运行）只在设备显现的 bug；六个 workflow 都硬编码 imafc | 确认 ISA 后把 imfc spec 设为默认，portable-atomic 配 critical-section/single-core polyfill，全 CI 切到该 target | **本轮已修**（默认 target 改 builtin 无原子 `riscv32imc-unknown-none-elf`（stable）；portable-atomic 用 critical-section polyfill；ws63-rt 的 riscv 开 critical-section-single-hart；实测产物零原子指令 lr/sc/amo；自定义 ilp32f rv32imfc JSON 保留供阶段 3 链接 blob 启用） |
| 高 | build | Release 工作流附加零固件产物（glob 不匹配、无 objcopy） | release.yml:45-47 附加 `*.bin`/`*.elf`，但 cargo 产无扩展名 ELF（`flashboot` 84256B、`blinky`），无 .bin，无 objcopy 步骤 | 每次 tag release 不附任何固件，glob 静默匹配空——嵌入式项目主交付物为空 | 加 `objcopy -O binary` 每二进制产 .bin，重命名 ELF 或改 glob；WS63 还需打包/签名镜像才可刷 | **本轮已修**（release 加 objcopy 产 .bin、修正 artifact glob、删除 fake host-test 任务） |
| 中 | build | 各 submodule 的 Cargo.lock 与工作区 lock 分叉 | ws63-examples/Cargo.lock 是独立锁文件，同样有双 PAC 且 pin 不同 commit（185 行 `#c6ae2f3`，父锁 756 行 `#df35d69`）；ws63-examples/ws63-rt 检出在 detached origin/HEAD | 独立构建 ws63-examples 解析出不同 PAC，"示例仓能编译"不蕴含"monorepo 能编译"；detached pin 易丢工作；双锁必然周期性漂移 | 选一个真值源：要么合并为单 workspace 单 lock，要么保留 submodule 但删冗余内层 lock 由父工作区拥有解析；pin 到分支 tip/tag 而非 detached HEAD | 已排期（阶段 1，链接脚本/submodule 集成时一并处理） |
| 低 | architecture | 组件布局不一致：flashboot 在树内而其它都是 submodule，且它重实现 ws63-rt（partial） | submodule status 列 7 个 submodule 但 ws63-flashboot 直接在父仓跟踪；flashboot 重复 ws63-rt 的 startup.S/layout.ld/memory.x；main.rs:28 故意绕过 HAL/PAC | 两套并行 startup/linker 维护，bootloader 自带副本而非复用；submodule-vs-树内分裂无说明，抬高贡献门槛 | 明确决定：flashboot 若核心则同样做 submodule 或全树内；抽出共享 startup/linker；在 CLAUDE.md 文档化布局理由 | 已排期（阶段 2-3，结构决策与共享 rt-core 抽取） |
| 中 | build | CI 从不链接库 crate 或在 target 运行；release 是 hal/pac 唯一真实链接 | ci.yml build/workspace 仅 `cargo check` 无 codegen/link；"Check examples"与 release"Build all examples"都 `\|\| true` 屏蔽失败；host-test `\|\| echo`；blinky/flashboot 是 `[[bin]]` 非 `examples/`，`--examples` 检查不到任何东西 | codegen/link/ISA 问题（如原子）到 release 或上设备才暴露；`\|\| true` 让坏示例/失败发布永不红；完整固件路径只在 nightly size-report 与 release 跑 | 加 PR job 在正确 target 上 `cargo build`（非 check）blinky/flashboot 不带 `\|\| true`；`--examples` 换成显式 `-p blinky -p ws63-flashboot`；移除 gating 步骤的失败屏蔽 | **本轮已修（部分）**（已去除 `\|\| true`/continue-on-error、删除 fake host-test；链接脚本传播已修，CI 现真实 `cargo build` 链接 blinky；release 经 objcopy 产 `.bin`） |

### 维度六：战略方向与生态优先级

建了能干的底层外设 HAL，但瞄准方向偏离了芯片存在的全部理由（连接性）。

| 严重度 | 类别 | 标题 | 证据 | 影响 | 建议 | 状态 |
|---|---|---|---|---|---|---|
| 严重 | direction | 精力瞄准底层外设而非芯片真正价值（连接性） | ws63-RF 只有 lib/*.a(7) + include/port/port_*.h(8 桩) + README，无任何 .rs/build.rs/Cargo.toml；全仓 grep `libwifi\|wifi_init\|extern "C"` 零连接性链接；待替换的 C porting 层约 16300 LOC | 买家为 Wi-Fi6/BLE/SLE 选 WS63；现状能点灯但发不出一个包，交付芯片差异化价值约 0%；无连接性它只是通用 RISC-V GPIO/UART 库 | 把连接性 bring-up 作为首要交付物重构路线图；以"连上 AP 并 ping 通"为北极星里程碑 | 已排期（阶段 3-5，连接性：blob 链接 → porting+HCC → 连接性示例） |
| 高 | direction | 无产品路径：porting 层 + HCC IPC 桥完全未实现，这才是真正难点 | ws63-RF/README 列 8 个 porting 桩（约 70 函数）需用户实现，无一在 Rust 实现；难点是到 Wi-Fi/BT device-MAC 的 HCC IPC（host↔device 传输；WS63 单核，DMAC 是同核软件库，非第二颗 RISC-V 核）；HAL 无 DMA/共享内存/IPC 框架接这些库；48KB 包 RAM + 约 500KB RAM 区不在任何链接脚本 | "HAL 存在"到"Wi-Fi 工作"的鸿沟是全项目最大最险且未开始的部分；当前打磨容易 20%，承重 80%（IPC/OAL/FRW/链接集成/blob 符号解析）未动；无 .a blob 能对 Rust 二进制链接的证据 | 先尖刺最大未知：写最小 Rust crate 链接一个 blob（libwifi_rom_data.a 仅 3KB）并解析外部符号，证明工具链/链接路径可行；再实现 port_log/osal/oal + HCC | 🟡 阶段 3 尖刺**已完成**（2026-06-02）：`wifi_blob_link` 用 `--whole-archive` 全量链接 `libwifi_rom_data.a`（rv32imfc/ilp32f ABI 一致），13/13 配置全局进镜像、数据符号 + 链接器符号 `__wifi_pkt_ram_begin__` 重定位全解析，ws63-qemu 验证——「无 blob 能对 Rust 链接的证据」已证伪。**仍未做**（阶段 4）：大型代码 blob（dmac 629KB/bt 1.1MB）链接 + 真实 porting/HCC + 真实 `.wifi_pkt_ram` NOLOAD 区 |
| 中 | direction | git 历史由 AI 自我评审打磨主导，而非硬件验证或功能进展（partial） | 35 commit 中大量是评审/审计表演（"12 bugs fixed"/"agent review"/"safety.rs"/"add SAFETY comments"/"TOCTOU"）；ONBOARDING.md 强调"/code-review max 启动约 20 agents"；零 commit/doc 提及硬件/硅/EVB 测试 | 反复评审从未执行的代码产生虚假信心、烧掉稀缺单人预算在表面正确性上；refcount 的 TOCTOU 修复若 clock_init 寄存器序列在真实硅上错则毫无意义 | 任何后续评审前先建硬件在环信号：刷 blinky、再 UART hello-world；以"在硅上跑"为门、而非"过 N 次 agent 评审"；大幅削减评审频率 | 已排期（阶段 1，HIL bring-up） |
| 高 | direction | 宣称的 async 架构是 no-op；无 Embassy/RTOS/networking 路径 | private.rs:39-51 Blocking/Async 都实现恒等 `type Async<D>=D`，无法区分；仅 delay.rs 引用（纯忙等）；全仓 grep `embassy\|async fn\|.await\|rtic\|smoltcp\|defmt` 零命中；CLAUDE.md 自承认 async 未实现 | sealed DriverMode 是死脚手架，发出不存在的 async 信号；真实连接性需 executor + 中断驱动 I/O，ws63-rs 无 networking 所需运行时基底 | 删 Async 标记直到有真实 async 驱动支撑，或承诺接 embassy-executor + critical-section 并让一个驱动真正中断驱动 async 作证据点；勿出货空类型态 | ✅ 已实现（2026-06）：空 marker 已删；ws63-hal `async`/`embassy` 真实落地 —— embedded-hal-async/embedded-io-async 全套 + embassy-time `Driver` + embassy-executor 多任务 + 中断驱动 I/O，ws63-qemu 验证（见「进展更新」/ async-embassy.md）|
| 高 | direction | 范围铺张：单人作者的独立子项目过多，多个重造 C SDK 或 crates.io | 7 submodule + 手写 bootloader；flashboot/sha256.rs 从零 SHA256（已有 crypto crate，且芯片有硬件 SHA/PKE）；WS63.svd 10744 行手写；ws63-flashboot 865 LOC 重复 SDK loaderboot/flashboot；外加完整 Sphinx 文档书 + 7 个 CI workflow | 每个子项目都是维护税与对连接性的分心；bootloader 尤其高投入、安全敏感且不在通往可用 HAL 的关键路径；手写 SHA256 是无收益的风险 | 砍或冻结：降级 ws63-flashboot（用厂商 bootloader 刷 app）、弃手写 SHA256 改用 vetted crate 或片上 crypto、冻结 7-workflow CI/文档书打磨；集中预算于 HAL→RF 桥；保留 SVD 与 guide 但停止扩张 | **本轮已修（部分）**（flashboot 已标实验性 + 移出 default-members；战略性冻结与 SHA256 替换见阶段 3+） |
| 中 | direction | 单一示例（blinky）无代表性外设或连接性演示（partial） | `find ws63-examples -name '*.rs'` 仅 blinky/main.rs，且用开环忙等 delay_ms（硬编码 240_000 迭代）而非 HAL 自己的 Delay/timer——连它出货的 HAL 都不用；esp-hal 出货 54 示例 | 采用需可复制粘贴、证明驱动可用的示例；一个绕过 HAL 计时层的点灯几乎什么都不证明；新用户无 on-ramp | 加一小组 HIL 验证示例（uart_echo/i2c_scan/spi_loopback/timer 驱动闪烁）真正用 HAL 驱动，至少一个在真实硬件跑；以示例作驱动正确性验收 | 已排期（阶段 5，连接性示例；外设示例可随阶段 1-2 HIL bring-up 增补） |
| 中 | direction | safety.rs "编译期形式化验证"夸大价值并制造脆弱的双重维护 | safety.rs:37-91 多为对硬编码字面量的 const_assert，重复 soc/ws63.rs 常量（PERIPHERAL_COUNT==17、SYSTEM_CLOCK_HZ==240MHz）；自称"formal safety contracts" | 读起来严谨却验证同义反复——SYSTEM_CLOCK_HZ==240MHz 的 const_assert 抓不到硅上错误时钟；新增第二处需更新每个计数并招致 churn | 仅保留抓真实跨模块漂移的 const_assert（数组界 vs 枚举数），删同义反复，停用"formal verification"措辞；把精力转向 HIL 烟雾测试验常量 | 已排期（阶段 2，死代码清理，与 HAL 核心维度同条） |
| 高 | direction | 最高杠杆的下一步未被推进 | 轨迹对比：esp-hal 有 esp-radio/esp-rtos/embassy 集成/54 示例/3435 commit；ws63-rs 31 驱动/1 示例/0 连接性/35 commit 半数是评审；战略排序倒置（打磨先于验证、驱动先于桥） | 不重排优先级则项目止步于"某冷门 RISC-V 芯片的好 GPIO/UART 库"，永不达到能吸引用户/贡献者的连接性 | 排序后续 3-5 步：(1) HIL bring-up 验 clock_init/linker/startup；(2) 链接 blob 尖刺；(3) 实现最小 porting 桩 + HCC IPC；(4) 出"Wi-Fi 扫描"再"连接 + ping"示例；(5) 然后才加 embassy async；在 (4) 落地前冻结 bootloader/SHA256/CI/docs 扩张 | 已排期（阶段 1→3→4→5→6，即 ROADMAP 主干） |

---

## 本轮（2026-05-31）修复小结与 ROADMAP

### ROADMAP 阶段编号

- **阶段 0（本轮已完成）**：构建完整性修复——双 PAC 消除、ISA 修正、flashboot 标实验性、CI/release 修复、ws63-rt 中断宏 typo + 栈顶符号 GC fallback。
- **阶段 1**：硬件在环（HIL）bring-up + 链接脚本集成。✅ 链接脚本集成**已打通**：ws63-rt 改用 `cargo:rustc-link-search` + 生成 `ws63-link.x`（取代不传播的 `rustc-link-arg`），blinky 等全部示例正常链接。✅ 恒真式测试已由 ws63-qemu 软件在环大幅替代。🟡 真机 HIL 冒烟仍待补。
- **阶段 2**：死代码清理 + 正确性修复——中断子系统模型重写（PLIC→LOCIPRI/LOCIEN）、SPI trsm、I2C/SPI 超时、system reset、GPIO pull、efuse/lsadc 寄存器、flashboot 镜像头/验签/AB、死代码（RAII 守卫/DriverMode/DmaChannelFor/safety.rs 同义反复）。
- **阶段 3**：链接 blob 尖刺（证明 Rust 二进制能链接 libwifi_rom_data.a 并解析外部符号；自定义 ilp32f rv32imfc JSON target 在此启用）。
- **阶段 4**：porting 层（port_log/osal/oal）+ HCC 共享内存 IPC。
- **阶段 5**：连接性示例（Wi-Fi 扫描 → 连接 + ping）。
- **阶段 6**：async ✅ **已实现**（embassy-executor + embassy-time `Driver` + critical-section + 中断驱动 I/O；ws63-hal `async`/`embassy` feature，见 async-embassy.md）。

### 本轮已完成的构建完整性修复

- **双 PAC 已消除**：ws63-hal / ws63-flashboot 改为 registry 版本依赖 + 根 Cargo.toml `[patch.crates-io]` 指向本地，`cargo tree` 单一 ws63-pac 实例；ws63-pac 版本 bump 0.1.0→0.1.1。
- **ISA 已修**：默认 target 改为 builtin 无原子 `riscv32imc-unknown-none-elf`（stable），portable-atomic 用 critical-section polyfill，ws63-rt 的 riscv 开 critical-section-single-hart；实测编译产物零原子指令（lr/sc/amo）。自定义 ilp32f rv32imfc JSON 保留供阶段 3 链接 blob 时启用。
- **flashboot 已标实验性**：banner 重写为"非安全启动"警告、Cargo.toml `publish=false`、移出 default-members、删除未用的 ws63-pac 依赖、新增 README.md。
- **CI/release 已修**：去除失败屏蔽（`|| true` / continue-on-error），clippy 改为 gating（排除实验性 flashboot），发布改依赖序顺序 pac→rt→hal，release 加 objcopy 产 .bin、修正 artifact glob、删除 fake host-test 任务。
- **ws63-rt 已修**：MIE 中断宏 typo（`call mie\()_interrupt_handler`）、栈顶符号 GC fallback。

### 仍未解决（已排期）

- 示例无法链接：ws63-rt 链接脚本不传播到下游二进制，blinky 因 `__exc/nmi/irq_stack_top__` 未定义而链接失败 → 阶段 1。
- 中断子系统模型错误（PLIC vs LOCIPRI/LOCIEN）、SPI trsm、I2C/SPI 超时、system reset、GPIO pull、死代码清理 → 阶段 2。
- efuse/lsadc 寄存器、flashboot 镜像头/验签/AB → 阶段 2。
- 连接性（porting 层 + HCC IPC + blob 链接）→ 阶段 3-5。

---

## 附：与组件架构文档的对应

各维度的组件级架构说明见 `../architecture/` 目录：

- 总览：[../architecture/overview.md](../architecture/overview.md)
- PAC + SVD 生成层：[../architecture/ws63-pac.md](../architecture/ws63-pac.md) · [../architecture/ws63-svd.md](../architecture/ws63-svd.md)
- HAL 核心与驱动：[../architecture/ws63-hal.md](../architecture/ws63-hal.md)
- 运行时与 bootloader：[../architecture/ws63-rt.md](../architecture/ws63-rt.md) · [../architecture/ws63-flashboot.md](../architecture/ws63-flashboot.md)
- 示例：[../architecture/ws63-examples.md](../architecture/ws63-examples.md)
- 连接性（RF / Wi-Fi / BT / SLE）：[../architecture/ws63-RF.md](../architecture/ws63-RF.md)
- 逆向文档书：[../architecture/ws63-guide.md](../architecture/ws63-guide.md)
