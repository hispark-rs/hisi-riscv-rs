# ws63-rs 路线图（ROADMAP）

> 本路线图源于 2026-05 的一次深度架构评审（多 agent 工作流 + 对抗式验证，41 条发现 0 条被驳回）。
> 完整发现台账见 [`docs/review/architecture-review-2026-05.md`](docs/review/architecture-review-2026-05.md)，
> 各组件架构与评审见 [`docs/architecture/`](docs/architecture/)。

## 北极星

**在真实 EVB 上"连上 AP 并 ping 通"。** WS63 的全部价值是连接性（Wi-Fi 6 / BLE / SLE/星闪）；
HAL 是手段，不是终点。一切排序以"离能联网更近"为准绳。

评审揭示的三类核心问题：
1. **方向**：连接性交付 0%（ws63-RF 只有闭源 blob + 未实现的 porting 桩），精力却集中在底层外设打磨。
2. **构建完整性**：双 PAC 致示例链接失败、默认 target ISA 与硅片不符、发布链路坏。← 本轮（阶段 0）已修。
3. **正确性地雷 / 过度设计**：中断模型错误、SPI/复位/超时缺陷、大量零消费者死代码、从未上板验证。
   ← SPI 位段+超时、eFuse/LSADC 寄存器映射、可复现 SVD→PAC 流水线已修（阶段 2 部分，见下）。

---

## 阶段总览

| 阶段 | 主题 | 状态 |
|------|------|------|
| 0 | 构建完整性 + 文档 + flashboot 实验化 | ✅ 已完成（2026-05） |
| 1 | 硬件在环（HIL）bring-up + 链接脚本集成 | 🟡 链接脚本 ✅；QEMU 软件在环 ✅（blinky/uart/中断/async 全绿）；**板子短期到位 → 先备烧录脚本 + HIL 冒烟框架**（见下「阶段 1 准备」）|
| 2 | 死代码清理 + 正确性修复 | ✅ 已完成（中断 LOCI* / SPI / I2C/SPI 超时 / 复位 / GPIO pull / eFuse / LSADC / 死代码 / host 单测 / DMA + **本会话：timer/WDT/UART/SPI/I2C 时钟真实化、cken 位逐一对照 SDK 审计**）|
| 3 | 链接/blob 尖刺 | ✅ 已完成（2026-06-02：`libwifi_rom_data.a` 全量链接 + 重定位，QEMU 验证 13/13） |
| 4 | porting 层 + HCC IPC | 🟡 数据通路已实现 + standalone 自测（`ws63-rf-rs`：FRW/HCC/OSAL/netif→smoltcp）；剩 blob 链接 + pbuf/TX-sink pin + 上板（依赖阶段 1）|
| 5 | 连接性示例（scan → connect → ping） | 🔴 待真机（HIL，阶段 1/4 之后）|
| 6 | async（embassy） | ✅ 已完成（async HAL + embassy 时间驱动 + 6 示例，见 [docs/architecture/async-embassy.md](docs/architecture/async-embassy.md)）|
| **7** | **HAL 收尾 + 发布（← 当前焦点）** | 🟢 进行中（见下「阶段 7」）|

> **当前焦点（2026-06）**：阶段 0/2/3/6 已收口、QEMU 软件在环成熟、文档已是「官方重建 + ch8 实证补充」双层。
> 主攻 **阶段 7（HAL 收尾 + 发布）**，同时因**真机短期到位**而并行**阶段 1 准备**（烧录脚本 + HIL 框架），
> 让板子一到位即可 blinky→uart→连接性 bring-up。阶段 4/5（连接性上板）随之解锁。

---

## 阶段 0 — 构建完整性 + 文档（✅ 本轮已完成，2026-05-31）

- **消除双 PAC**：`ws63-hal`/`ws63-flashboot` 改 registry 版本依赖，根 `Cargo.toml` 加
  `[patch.crates-io] ws63-pac = { path = "ws63-pac" }`；`cargo tree` 单一 `ws63-pac` 实例；
  `ws63-pac` 版本 bump `0.1.0 → 0.1.1`（tag 后加了 KM 寄存器，SemVer 要求）。
- **ISA 修正**：先用 builtin 无原子 `riscv32imc`（软浮点、stable）做过渡，随后**切到自定义 `ws63` 工具链**——
  把硬浮点 `riscv32imfc-unknown-none-elf`（`ilp32f`，无原子）烤进 stable rustc 作 builtin，仓库默认即用之、
  **无需 `-Z build-std`**。`portable-atomic` 用 `critical-section` polyfill，`ws63-rt` 的 `riscv` 开
  `critical-section-single-hart`。**实测产物零原子指令（lr/sc/amo）、single-float ABI**，不再会在无 A 核上触发陷阱。
  - 工具链仓库：<https://github.com/sanchuanhehe/ws63-rust-toolchain>（release v1.96.0 提供预编译 sysroot tarball）。
    硬浮点提前到位，为阶段 3 链接 ilp32f vendor blob 的 ABI 一致性做好准备。
- **flashboot 标记实验性**：banner 改为"非安全启动"警告、`publish = false`、移出 `default-members`、
  删除未使用的 `ws63-pac` 依赖、新增 `README.md`。生产启动复用 fbb_ws63 原厂 flashboot。
- **CI/release 修复**：去除失败屏蔽（`|| true` / `continue-on-error` / `| head` 吞掉退出码）；
  clippy 改为 gating（排除实验性 flashboot）；发布改依赖序顺序 `pac → rt → hal`、删并行 matrix 竞态、
  删 `continue-on-error`；release 加 `objcopy` 产 `.bin`、修正 artifact glob、删除从未真正运行的 host-test 任务。
- **ws63-rt 旁修**：修了 MIE 中断宏 typo、栈顶符号 GC fallback；为 publish 给 ws63-pac 依赖补 version。
- **文档**：新增 `docs/`（中文）总体架构 + 8 个组件架构与评审 + 完整评审台账；各子模块加薄链接 `ARCHITECTURE.md`。

验证：`cargo build`（默认 = 库）与 `cargo check --workspace` 均绿；clippy（排除 flashboot）零告警；
`cargo fmt --all -- --check` 干净；产物反汇编零原子指令。

---

## 阶段 1 — 硬件在环 bring-up + 链接脚本集成

**门禁理念：把"在硅片上跑起来"当作验收标准，而不是"过了 N 次 agent review"。** 评审显示底座大概率从未上板。

1. ✅ **修复链接脚本传播（已完成 2026-05-31）**：`ws63-rt` 的 `cargo:rustc-link-arg=-T…` 来自 *库* 依赖、不传播到下游二进制
   （blinky 曾因此用 lld 默认布局链接、trap 栈符号 `__exc/nmi/irq_stack_top__` 未定义而失败）。
   已改为：`ws63-rt/build.rs` 用 `cargo:rustc-link-search` 导出脚本目录（link-search 可传播）+ 生成 `ws63-link.x`
   包装脚本（按 memory→layout→device→symbols 顺序 `INCLUDE`），blinky 的 `build.rs` 以 `-Tws63-link.x` 引入。
   **blinky 现已可链接**（已加回 default-members，CI/release 构建并 objcopy 产 `.bin`）。
2. ✅ **MIE 中断宏 typo + 栈顶符号 GC fallback（已完成）**：见 ws63-rt 评审。
3. ✅ **软件在环（QEMU）bring-up（已完成 2026-05-31）**：硬件不便时的替代验证信号——
   [`ws63-qemu`](https://github.com/sanchuanhehe/ws63-qemu) 仿照 esp-qemu，fork 固定版 QEMU v9.2.4 加
   in-tree `hw/riscv/ws63.c`（rv32imfc hart、按 `memory.x` 的内存映射、自定义 HiSilicon UART、
   自定义 CSR RAZ/WI、其余外设 MMIO 吸收）。**已实测**：`blinky` 启动并跑到 GPIO 翻转循环（0 非法指令陷阱）、
   新增的 `uart_hello` 在 QEMU 串口打印。这验证了内存布局 / startup（PMP/FPU/cache/数据重定位/栈）/ 链接脚本
   在一个 WS63 地址空间模型上能正确运行——但**不等于真机验证**（QEMU 未建模时钟/中断/RF，时序也不保真）。
4. ✅ **统一 trap/栈布局（已完成 2026-06-01）**：`startup.S` 现以 **Vectored 模式**设 mtvec（`ori t0,t0,1`，
   mtvec[1:0]=01）——此前是 Direct，每个中断都落到 `trap_vector+0`(异常入口)、26-91 表项是死代码。`layout.ld`
   显式放置 `.trap`(64 字节对齐) + `.trap.exception/.nmi/.mieN/.local`(`KEEP`)，不再依赖 lld 孤儿放置。栈顶
   `__irq/exc/nmi_stack_top__` 统一定义在 `.stacks`(单一真值,被 KEEP 的 .trap 处理器引用故 GC 安全)，删除
   `memory.x` 里指向 `.heap` 区的错误 fallback(修了一个潜在的 trap 栈/堆重叠)。静态验证(readelf/反汇编):
   mtvec=0x230441、`.trap` 64 对齐、表项 0→trap_entry / 26→mie_interrupt0_handler / 32-91→local_interrupt_handler
   均解析且在跳转范围内、无未定义栈/trap 符号；smoke 5/5 无回归。**运行期分发**(经 ws63-rt 弱处理器覆盖)受
   工作区 fat-LTO + 跨 crate 弱符号覆盖限制(同 `timer_irq`/`gpio_irq` 用自带 mtvec 的原因)，未做运行期示例;
   底层 IRQ 投递已由 `timer_irq`(26)/`gpio_irq`(≥32) 在 QEMU 验证。
5. **上板冒烟**（剩余，需硬件）：真机烧 blinky → UART hello-world，验证 `clock_init`/linker/startup 在硅片上正确。
   用 `readelf` 核实 ELF 内存布局已确实采用 WS63 的 layout（非 lld 默认）。

---

## 阶段 2 — 死代码清理 + 正确性修复

基调：**删无用、留哨兵**。

> **✅ 本轮已完成**——以下已落地（详见下方对应条目）：
> 1. **SPI**（2026-05-31）：`ctra.trsm` 改 0（TX&RX）+ 所有忙等加有界超时 `SpiError::Timeout`。
> 2. **eFuse / LSADC**（2026-05-31）：寄存器映射按 fbb_ws63 C SDK（`hal_efuse_v151`/`hal_adc_v154`）整体重写。
> 3. **PAC/SVD 流水线**（2026-05-31）：`ws63-svd/regen.sh` 可复现生成 + 停止手补 lib.rs（幂等、build+clippy 门禁）。
> 4. **中断子系统重写**（2026-06-01）：`interrupt.rs` 由 PLIC 虚构改为 WS63 真实模型（见下「正确性修复」首条），
>    并经 ws63-qemu 的 `timer_irq`/`gpio_irq` 端到端验证——这是首个**有软件在环验证信号**的正确性修复。
> 5. **I2C 超时**（2026-06-01）：`i2c.rs` 全部无界忙等改为有界 `wait_until` → `I2cError::Timeout`（对齐 SPI）。
> 6. **system reset**（2026-06-01）：`software_reset` 触发 `GLB_CTL_M(0x40002110)` bit2 全芯片复位、`reset_reason`
>    解码 `SYS_RST_RECORD_0(0x400000A0)`（WDT/软件/上电），经 ws63-qemu 新增复位模型 + `reset_demo` 往返验证。
> 7. **死代码清理**（2026-06-01）：删除 `ClockControl`/`PeripheralGuard`/`REF_COUNTS` RAII 时钟守卫、
>    `DriverMode`/`Blocking`/`Async` marker、`DmaEligible`/`DmaChannelFor` 绑定 trait、safety.rs 的恒真计数断言；
>    保留 `Peripheral` enum + `cken_info` 门控图 + `PERIPHERAL_COUNT` 及 MMIO 地址范围/算术溢出断言。
> 8. **GPIO pull**（2026-06-01）：`init_input` 经 IO_CONFIG pad 寄存器落地 `InputConfig.pull`；新增 `InterruptTrigger`。
> 9. **host 单测真正跑起来**（2026-06-01）：cfg 门控 `ws63-hal` 与 `ws63-pac` 的 riscv 耦合，使库能为 x86 编译；
>    `cargo test --target x86_64` 跑通 **77 个单测**，CI 新增 `host-test` job。
> 10. **DMA 请求 ID + 接线**（2026-06-01）：`DmaPeripheral` 请求 ID 由杜撰的 0..11 改为 `dma_porting.h` 的
>     `HAL_DMA_HANDSHAKING_*`（UART_L/H0/H1=UART0/1/2、SPI_MS0/1=SPI0/1、I2S），经 `request_id()` +
>     `DmaChannelConfig::mem_to_peripheral`/`peripheral_to_mem` 接入 `configure_channel` 的 flow-control/握手字段。
> 11. **SDMA 8–11 通道映射 + 外设 DMA 端到端验证**（2026-06-02）：`DmaInstance` 加 `CHANNEL_BASE`
>     （`Dma0`=0 / `Sdma0`=8），`DmaDriver<Sdma0>` 接受逻辑通道 8–11 并内部映射到物理 0–3（对齐
>     C SDK `hal_dma_ch_get`/`hal_dma_type_get`），全通道方法 + `en_chns`/`burst`/`single`/`int_clr`
>     位操作统一用物理索引；+6 个映射单测（host 82 passed）。ws63-qemu DMA 模型补外设 DMA：
>     `ws63_dma_run` 解析 `fc_tt`/`src_per`/`dest_per`，修正 TC 中断门控 `(ctrl.tc_int_en)&&(cfg.tc_int_mask, bit13)`
>     （原 `!(cfg&bit2)` 位号+极性皆错）、修完成清位 `bit0+bit15(active)`（原误清 fc_tt 内的 bit10）。
>     新增 `ws63-examples/dma_loopback`：mem↔SPI0 外设 DMA 环回（MDMA ch0，fc=1/2，握手 7/8）+ SDMA
>     逻辑通道 8 mem2mem，**在 ws63-qemu 上跑通**（接入 smoke-test）；C SDK `dma.elf` 回归无回归。
>
> 注：1–3、5、7、8 为静态对照 SDK 的修复，**仍未上板验证**（属阶段 1 门禁；GPIO pull 是上拉电阻、QEMU 数字引脚网不建模）；
> 4、6 已在 QEMU 验证（投递闭环 / 复位往返）但仍非真机；9 是 host 逻辑单测（非硬件）。**阶段 2 正确性项已全部落地**
> （死代码、I2C 超时、reset、GPIO pull、host 单测、trap/向量布局、DMA 请求 ID、flashboot 整改）；唯一显式不做的是
> flashboot 真实 secure-boot 验签（冻结项，复用原厂）。下一步是连接性北极星（阶段 3 blob 链接尖刺 → 4 porting/HCC → 5 demo）。

**死代码清理（删）✅ 已完成（2026-06-01）**
- ✅ `clock.rs`：删除 `ClockControl` / `PeripheralGuard` / `REF_COUNTS`（RAII 时钟守卫，零消费者；驱动依赖复位默认开）。
- ✅ `private.rs`：删除 `DriverMode` / `Blocking` / `Async`（关联类型恒等，零引用）。
- ✅ `dma.rs`：删除 `DmaEligible` / `DmaChannelFor`（前者仅 impl 未调用、后者无 impl 无接线）。
- ✅ `safety.rs`：删除 10 条 `const X == 字面量` 恒真计数断言 + `verify_peripheral_count`，去掉"formal verification"措辞。
- ✅ **保留哨兵**：`Peripheral` enum + `cken_info` 门控图 + `PERIPHERAL_COUNT`、MMIO 地址范围断言（37-44）与定时器算术溢出断言（79-85）、`PeripheralIndex`/`GpioPinIndex`。

**正确性修复**
- ✅ **中断子系统**（严重，已修 2026-06-01）：`interrupt.rs` 删除"PLIC"虚构，按 fbb_ws63
  `arch/riscv/riscv31/interrupt.c` 重写为 WS63 真实双层模型——**IRQ 26–31** 走标准 `mie` 位，**IRQ ≥32**
  走自定义 `LOCIEN0-2`（0xBE0–2，base 32）使能、`LOCIPRI0-15`（0xBC0，base 26，4-bit/IRQ）优先级、
  `LOCIPCLR`（0xBF0）按号清挂起、`PRITHD`（0xBFE）阈值；默认优先级 `0x11111111`。API：
  `enable`/`disable`/`set_priority`/`priority`/`set_threshold`/`threshold`/`clear_pending`/`is_pending`/
  `is_enabled`/`init`/`enable_global`/`disable_global`/`free`。移除零消费者的 `InterruptConfigurable`/
  `InterruptHandler`（`prelude` 改导出 `Interrupt` 枚举 + `Priority`）。`timer_irq`(mie)/`gpio_irq`(LOCIEN+LOCIPCLR)
  改用此 API，经 ws63-qemu `smoke-test.sh` 端到端验证投递闭环。**仍未上板**（属阶段 1 门禁）。
- ✅ **SPI**（严重，已修 2026-05-31）：`ctra.trsm` 由 `3`(EEPROM-Read) 改 `0`(TX&RX)；SCKDV 分频去掉
  多余的 `/2`/`-1`（曾产出 ~2× SCK）；全部忙等改有界 `wait_until` → `SpiError::Timeout`。
- ✅ **I2C 超时**（高，已修 2026-06-01）：`i2c.rs` 的全部无界 `while !…{}`（busy / tx-ack / rx / stop 等待）改为
  有界 `wait_until` → `I2cError::Timeout`（与 SPI 同一模式，`I2C_WAIT_LOOPS`）。卡死 / 缺席的从设备不再死锁内核。
- ✅ **system reset**（高，已修 2026-06-01）：`system.rs` 删除 `software_reset` 的占位 `ebreak` 与 `reset_reason`
  恒返回 `PowerOn` 的假值，按 fbb_ws63 `reboot_porting.c` 实现——`software_reset` 置 `GLB_CTL_M(0x40002110)` bit2
  触发全芯片复位；`reset_reason` 读 `SYS_RST_RECORD_0(0x400000A0)` 解码 WDT(bit0)/软件(bit1)/上电(bit3) 并经
  `SYS_DIAG_CLR_1(0x400000A4)` 清位。ws63-qemu 新增对应复位模型，`reset_demo` 示例端到端验证往返
  （冷启动 → `software_reset` → 重启 → `reset_reason`=Software）。
- ✅ **GPIO pull**（中，已修 2026-06-01）：`init_input` 不再静默忽略 `InputConfig.pull`——经 IO_CONFIG pad 控制寄存器
  （`pad_gpio_NN_ctrl` @ IO_CONFIG+0x800+N*4，PE=bit9/PS=bit10）读-改-写落地，保留 drive/Schmitt/IE 位（pad 仅 0..=14 有，
  对齐 `io_config::build_pad_ctrl` 编码）。新增 `InterruptTrigger`（上升/下降沿、高/低电平）+ `Input::set_interrupt_trigger`
  写 `GPIO_INT_TYPE`/`GPIO_INT_POLARITY`。上拉电阻在 QEMU 数字引脚网不可观测，故为静态修复。
- ✅ **efuse/lsadc**（高，已修 2026-05-31）：通过改 SVD + 重生成 PAC + 重写 HAL 落地——
  - eFuse：控制块 base+0x30、16 位模式魔数（`0x5A5A` 读 / `0xA5A5` 写）、0x800 数据窗口（128 字），
    `efuse.rs` 改为字节读写（窗口索引 `byte/2`、奇偶字节抽取），对齐 `hal_efuse_v151.c`。
  - LSADC：重写为连续 `adc_regs_t`——使能/复位 `CTRL_11`（`da_lsadc_en[15:0]`/`rstn`@16）、扫描 `CTRL_0`、
    启停 `CTRL_8`、FIFO 读 `CTRL_9`、空判定 `CTRL_1.rne`、`CFG_*` @ 0xDC..0xEC，对齐 `hal_adc_v154`。
  - 偏移已在生成的 PAC 中逐一核验；纯解析逻辑有 proptest。**未上板**。
- **safety.rs**：删恒真断言并去掉"formal verification"措辞。
- ✅ **host 单测**（已修 2026-06-01）：`ws63-hal` 的 riscv CSR 内联汇编（`interrupt.rs`）用
  `#[cfg(target_arch="riscv32")]` 门控（host 得 no-op 桩）。更深的阻塞在 `ws63-pac`——它无条件
  `pub use riscv::interrupt::*` 且对 `ExternalInterrupt` 打 `#[riscv::pac_enum]`,而 riscv 0.13 不能为 x86 构建。
  经 `ws63-svd/postprocess.py` 把这些 riscv 耦合 cfg 门控到 riscv32（host 得纯 enum,HAL 仅用 `as u16`）,
  `riscv` 改 riscv32-only 依赖;`lib.rs` 改 `#![cfg_attr(not(test), no_std)]` + dev-dep `critical-section/std`。
  `cargo test --target x86_64` 现真正编译并跑通 **77 个单测**;CI 新增 `host-test` job（stable + x86_64）。
  注:这是 host 逻辑单测,非硅上验证（仍属阶段 1 门禁;用对照 C 驱动序列替换恒真单测见阶段 5 示例）。
- ✅ **flashboot 整改（已修 2026-06-01）**：`CodeInfo`/`KeyArea` 按 `secure_verify_boot.h`(ECC256) 重排
  （`code_area_len`@+0x24 / `code_area_hash`@+0x28 偏移修正 + `const` 尺寸断言锁定 0x100/0x200/0x300）；
  删除 `0x40000024` 的 A/B 误用 → 单镜像启动，并如实注明真实 A/B = upg run-region(magic `0x70746C6C`)+分区表
  （`@0x200380`）、`0x40000024`=bootloader 自恢复标志；`verify_sha256`→`verify_image_integrity` 如实标注
  "仅完整性、非真实性"；flashboot 纳入 CI clippy 门禁（不再 `--exclude`）。对抗式评审：layout ok、honesty
  ok（修了 README 一处过时声明）。**真实 ECC/SM2 签名验签**保持冻结、生产复用原厂 flashboot（不在实验件投入）。

**PAC/SVD 流水线**
- ✅ **可复现生成脚本（已完成 2026-05-31）**：`ws63-svd/regen.sh` + `postprocess.py`——pin `svd2rust@0.37.1`/`form@0.13.0`，
  补齐 svd2rust→edition 2024 的三处确定性落差（删 5 个 dim 重复 TIMER 访问器、`#[no_mangle]`→`#[unsafe(no_mangle)]`、
  `cargo fix` 套 `unsafe_op_in_unsafe_fn`），**幂等**（同 SVD→字节一致 lib.rs）、build+clippy 内建门禁。
  **停止手补 lib.rs**（主仓 PreToolUse hook 已拦截手改）。再生成同时恢复了手补漏掉的 KM keyslot 字段。
- 剩余：CI 增"从 SVD 重生成并 diff 校验"步骤（脚本已就绪，接 CI 即可）；逐外设对照 fbb_ws63 `*_reg.h` 核覆盖
  （本轮已核 SPI/eFuse/LSADC，其余待逐个过）。

---

## 阶段 3 — 链接/blob 尖刺 ✅ 已完成（2026-06-02）

先消除最大未知：写最小 crate 链接 `ws63-rf-rs/ws63-RF/lib/libwifi_rom_data.a`（仅 3KB）并解析其外部符号，
**证明工具链/链接路径走得通**。ABI 已就绪：仓库已用 `ws63` 硬浮点工具链（`riscv32imfc`/`ilp32f`），与 blob ABI 一致。

**已交付**：`ws63-examples/wifi_blob_link` 用 `--whole-archive` **全量**静态链接该 blob（确认其 `rv32imfc`/`ilp32f`
与工具链一致），13 个配置全局全部进入镜像（配置 blob 须整体在位——厂商 ROM 按地址读全部），并解析其全部 3 个
外部符号：2 个数据符号在 Rust 里打桩（`g_dmac_alg_main`/`g_mac_res_etc`），链接器符号 `__wifi_pkt_ram_begin__`
经 build.rs `--defsym=0xA00000`（C SDK `.wifi_pkt_ram` 基址）提供。**ws63-qemu 实跑 13/13 通过**：所有配置全局
== 厂商初值、`g_mem_start_addr_cfg[2]` == `__wifi_pkt_ram_begin__+4`（链接器符号重定位）、`g_dmac_algorithm_main`/
`g_mac_res` == 打桩地址（数据符号重定位）；接入 `smoke-test`。

**已证伪的未知**：ABI 匹配、厂商静态归档可链入 Rust 镜像、`.data` 重定位（数据符号 + 链接器符号两类）可解析、
`--whole-archive` 可把整个配置 blob 纳入。**本尖刺不涉及**（留阶段 4）：大型**代码** blob（`libwifi_driver_dmac.a`
~629KB 带真实 `.text`、`libbt_host.a` ~1.1MB）的链接与符号闭合；用真实驱动库（而非桩）满足 `g_dmac_alg_main`/
`g_mac_res_etc` 的 ABI；`ws63-rt` 真实的 `.wifi_pkt_ram` NOLOAD 区（此处 `__wifi_pkt_ram_begin__` 仅是裸 `--defsym`）。
**Wi-Fi 栈未运行**——这是链接/重定位路径证明，不是连接性。

---

## 阶段 4 — porting 层 + HCC IPC 🟡 已起步（2026-06-02）

实现 `ws63-rf-rs/ws63-RF/include/port/` 的最小桩（`port_log`/`port_osal`/`port_oal`）+ 与 Wi-Fi/BT 协处理器的
共享内存 HCC IPC，架在现有 DMA/UART 驱动之上。这是到产品**最大、最高风险**的一块。

**已交付**：`ws63-rf-rs`（in-tree crate，仿 esp-radio 的 os-adapter）——把 ws63-RF 的 77 函数运行时无关
移植契约（`include/port/*.h`）用 Rust 实现为 `#[no_mangle] extern "C"`：**已真正实现** `osal_kmalloc`/`kfree`
（`linked_list_allocator` 真实堆）、log/`memset_s`/`memcpy_s`、`uapi_systick_get_ms`/`osal_udelay`、
`osal_irq_lock`/`restore`（mstatus）、OAL 池配置、2 个 ROM 全局 `g_dmac_alg_main`/`g_mac_res_etc`（任何 blob 都不定义）；
线程/wait/`frw_*`/`hcc_*`/`wlan_*` 为**有类型有 TODO 的桩**（需调度器 + IPC 框架）。`rf_port_demo` 例子在 ws63-qemu
验证：实现的契约函数真能用，且厂商 ROM blob **经 ws63-rf-rs 全量链接**。

**符号闭合 ✅（已用真实链接器达成，2026-06-02）**：完整 MAC blob 集（`libwifi_driver_{hmac,dmac,tcm}.a`、
`libbg_common.a`、`libwifi_alg_*.a`、`libwifi_rom_data.a`）经 `rust-lld` 与 ws63-rf-rs + WS63 掩膜 ROM 符号表
（`ws63-rf-rs/ws63-RF/rom/ws63_acore_rom.lds`，3752 符号）+ compiler-rt **可重定位链接成单一对象，0 个重复符号**；从
`uapi_wifi_init` 做 `--gc-sections` 可达性链接后**残留仅 2 个符号**——`__wifi_pkt_ram_begin__`/`__wifi_pkt_ram_end__`，
二者皆为链接器 `--defsym` 区界符号（已由 `wifi_blob_link` 例子提供）。即**整个 Wi-Fi init 闭包对 ws63-rf-rs + ROM + rt
完成符号解析**。可复现：`ws63-rf-rs/tools/mac-link-residual.sh`。

之前"~96 缺失"是 `--whole-archive`（强制纳入每个 obj）的上界，其中**绝大多数是 Wi-Fi init 不可达**的 BT 共存 + 备用 OS
适配器代码（可达路径上 BT 符号数 = 0）。ws63-rf-rs 现导出 220+ 个契约符号：已**真正实现**堆/调度器（`osal_kthread_*`/
信号量/消息队列/事件组/**带超时阻塞**）/自旋锁/原子/log/securec/时间/字符串/`osal_adapt_*`(33)/libc/LiteOS-arch 兼容。

**数据通路（运行时半边）已实现并在 ws63-qemu 自验**（无 blob，用 mock 验证）：
- **FRW 工作线程 + HCC 传输**（`frw.rs`/`hcc.rs`）：真实消息节点池（`frw_msg_node` 精确 40B 布局）、worker 线程（跑在
  `sched` 上）、host↔device 消息 FIFO。`frw_hcc_selftest`：5 条消息经 HCC→worker 全投递、校验和正确。
- **软件定时器服务**（`timer.rs`）：`osal_adapt_timer_*`/`frw_dmac_timer_*`，ms deadline，由 worker 循环驱动。
  `timer_selftest`：one-shot 触发一次、不重触、re-arm 再触发。
- **netif→smoltcp 桥**（`netif_smoltcp.rs`，feature `net`）：真实 `smoltcp::phy::Device`，`driverif_input` 喂 RX 队列、
  `TxToken` 走 TX sink。`netif_smoltcp_selftest`：ARP 请求经 驱动→smoltcp→驱动 往返、回 ARP reply。

承接阶段 3/4，**仍需**完成：
- ✅ ~~补齐漏抽的 wifi `.a` 库~~（hmac/tcm/alg/bg_common 已纳入 `ws63-rf-rs/ws63-RF/lib`；wpa 按 MVP 决策剔除；ROM 表已补）。
- ✅ ~~任务调度器~~（`crate::sched` cooperative 调度器已实现并在 ws63-qemu 验证）。
- ✅ ~~数据通路运行时半边~~（FRW worker + HCC 传输 + 软件定时器 + netif→smoltcp 桥，均已实现 + QEMU 自验）。
- **接真实 blob（硬件在环）**：把 netif `pbuf` 布局按 wifi 构建的 `lwipopts.h` 对齐；把 smoltcp TX sink 指向 blob 的真实
  发帧符号；用真实 `frw_event_process_all_event_etc` 等 blob 协议半边联调。
- **真实 `.wifi_pkt_ram` NOLOAD 区**：把裸 `--defsym=__wifi_pkt_ram_*` 升级为 `ws63-rt` 链接脚本里**保留的** NOLOAD 段
  （C SDK `linker.lds`：0xA00000、0xC000=48KB），否则 Wi-Fi ROM 初始化运行期会写入未保留区域。
- **真机验证（仍属硬件在环）**：ROM 符号是真实硅地址（QEMU 未填充掩膜 ROM）；且厂商 blob 携带 stock `lld` 无法定位的
  HiSilicon 自定义重定位（残留探针用可重定位链接来推迟它们），故可运行镜像需原厂链接器 + 真机。

---

## 阶段 5 — 连接性示例

交付 `Wi-Fi scan → connect → ping`——决定能否被采用的 demo。补 `uart_echo`/`i2c_scan`/`spi_loopback`
等上板验证过的外设示例，用示例作为驱动正确性的验收，替代 host 端恒真单测。

---

## 阶段 6 — async

接 `embassy-executor` + 中断驱动 I/O，让一个驱动（UART 或 timer）真正异步，作为概念验证。
在此之前不要保留空的 async 类型状态（阶段 2 已删 `Blocking`/`Async` marker）。

---

## 阶段 7 — HAL 收尾 + 发布（← 当前焦点，2026-06）

把现有成果固化为「可用、可发布、文档齐全」的产品级 crate 集。

1. **SPI 两级时钟建模**：HAL 当前按固定 160 MHz SSI_CLK 写 SCKDV，但未配 CLDO_CRG 的 SPI 分频
   （480 MHz PLL → `DIV_CTL3[9:5]`，见 ch8 时钟树）。补全 CRG 分频配置，使 SCK 在真机上准确；
   或显式记录「依赖 boot 默认 SSI_CLK」的边界。
2. **cken 位复核收口**：已逐一对照 SDK 审计——I2S 修正为 `CKEN0` bit11/12，其余
   I2C/Timer/LSADC/Tsensor/TRNG/Security/DMA/SDMA/SFC/SPI1 标为「SDK 不门控、占位未证实」。
   决定保留占位（已标注）还是删除仅留已证实位；并使 `safety.rs` 的 cken 漂移检查与之一致。
3. **补示例**：`blinky` 升级到 `OutputConfig`/`InputConfig`；按需加 `i2c_scan` / `spi_loopback` 覆盖更多驱动 API。
4. **发布到 crates.io**：`ws63-pac` / `ws63-hal` / `ws63-rt` 各自仓自治发布（`release.yml` 已就位），
   `ws63-rf-rs` / `ws63-flashboot` 维持 `publish=false`；校验 docs.rs 构建（features 门控）。
5. **ws63-guide 上线**：Pages 部署目前一直红（仓库未把 Pages 源设为 GitHub Actions）——需 owner 在
   Settings→Pages 启用；之后 ch1–8 全量上线。可选：补 ch6 外设寄存器深度（UART/QSPI/I2S 全寄存器图）。
6. **版本 + CHANGELOG**：各 crate bump/tag，记录本会话的时钟修复与 cken 审计。

## 阶段 1 准备 — 真机 bring-up 框架（板子短期到位）

板子一到位即可上手，不临时现搭：

1. **烧录脚本**：封装 BurnTool / 串口 ymodem（参考 fbb_ws63 烧录流程），`flash.sh <bin> <port>`。
2. **HIL 冒烟框架**：镜像 ws63-qemu 的 `smoke-test.sh`，跑在真实串口上——blinky（LED/逻辑分析仪）、
   uart_hello（读 banner）、timer_irq/gpio_irq（读中断计数）、reset_demo。
3. **bring-up 清单**：上电 → flashboot → blinky → uart → 中断 →（实测核对时钟 240/160/24）→ DMA → 连接性，
   每步附预期 + 失败诊断。
4. **首板验证目标**：确认本会话的时钟修复（timer 24 MHz、UART 160 MHz 波特、SPI/I2C）在真硅片上准确
   ——这正是 QEMU 无法证明的部分。一旦通过，阶段 4/5（连接性上板）即可推进。

---

## 冻结 / 降优先级

- **flashboot**：已转实验性，不再投入；生产复用原厂 flashboot。
- **手写 SHA256**：换 vetted crate 或片上 SPACC/PKE。
- **CI / 文档书 / SVD 的持续扩张**：冻结，直到连接性里程碑落地。
- **AI self-review 节奏**：先建立"上板"信号，再谈再 review；停止反复审计未上板的代码。
- **保留** SVD（基础）与 ws63-guide（独特逆向 IP），但停止扩张。

---

## 发现台账（摘要）

严重度（验证修正后）：**严重 6 / 高 16 / 中 15 / 低 4**，共 41 条，0 条被驳回。完整列表见
[`docs/review/architecture-review-2026-05.md`](docs/review/architecture-review-2026-05.md)。

| 严重度 | 代表问题 | 状态 |
|--------|----------|------|
| 严重 | 双 PAC 致示例链接失败（DEVICE_PERIPHERALS 重复） | ✅ 阶段 0 已修 |
| 严重 | 中断子系统建在不存在的 PLIC 模型上 | ✅ 阶段 2 已修（2026-06-01，重写为 LOCIEN/LOCIPRI/LOCIPCLR/PRITHD，QEMU 验证） |
| 严重 | SPI 传输模式位写成 EEPROM-Read | ✅ 阶段 2 已修（2026-05-31） |
| 严重 | flashboot 无真实性验签（≠ secure boot） | 实验化（阶段 0），整改阶段 2 |
| 严重 | flashboot 镜像头布局对不上真实镜像 | 实验化（阶段 0），整改阶段 2 |
| 严重 | 连接性交付 0%（chip 价值未触及） | 阶段 3-5 |
| 高 | 默认 target ISA 含原子扩展（硅片无 A） | ✅ 阶段 0 已修 |
| 高 | crates.io 发布链路坏 + 失败被静默吞 | ✅ 阶段 0 已修（结构）；2026-06 起改为各子仓自治发布，monorepo 的 `release.yml` 不再 publish（见 `release.yml:67`） |
| 高 | release 不挂固件产物 | ✅ 阶段 0 已修（blinky 链接后生效） |
| 高 | porting 层 + HCC IPC 完全未实现 | 🟡 数据通路已实现 + 自测：`ws63-rf-rs` 落地 FRW（`frw.rs`）/ HCC IPC（`hcc.rs`：`hcc_wifi_msg_send`/`_register`/`hcc_msg_open_wlan`）/ OSAL / netif→smoltcp，`frw_hcc_selftest` 无 blob standalone 验证。剩 blob 链接 + 上板连通（阶段 3-4） |
| 高 | I2C/SPI 无超时死循环 | ✅ 阶段 2 已修：SPI 2026-05-31、I2C 2026-06-01，无界 `while !..{}` 改有界 `wait_until`→`Timeout`（`SPI_WAIT_LOOPS`/`I2C_WAIT_LOOPS`） |
| 高 | eFuse 读路径/控制偏移错误 | ✅ 阶段 2 已修（2026-05-31） |
| 高 | LSADC 寄存器映射整块错位（ctrl_7/fifo 等） | ✅ 阶段 2 已修（2026-05-31） |
| 高 | 从未上板验证 / 测试恒真 | 🟡 上板（真硅片）验证仍待（阶段 1）；但「测试恒真」已破——`host-test` job 跑 77 个真单测（`ci.yml:123`）+ ws63-qemu 软件在环冒烟 |
| 中 | 死代码（RAII 时钟守卫 / DMA 安全层 / async marker） | ✅ 阶段 2 已修（2026-06-01）：`ClockControl`/`PeripheralGuard`/`REF_COUNTS` RAII 守卫、`DmaEligible`/`DmaChannelFor` 绑定 trait、`DriverMode`/`Blocking`/`Async` marker 全删（`clock.rs:10`、`dma.rs:498` 注明） |
| 中 | safety.rs 恒真断言剧场 | ✅ 阶段 2 已修（2026-06-01）：恒真 `const X == <literal>` 计数断言删除，仅保留真实 MMIO 范围/对齐编译期断言（`safety.rs:8`） |
| 中 | SVD→PAC 生成流水线不可复现 | ✅ 阶段 2 已修（regen.sh 幂等可复现，2026-05-31） |
| — | 示例无法链接（链接脚本不传播） | ✅ 阶段 1 已修（blinky 可链接） |
