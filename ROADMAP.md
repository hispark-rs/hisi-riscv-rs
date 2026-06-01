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
| 0 | 构建完整性 + 文档 + flashboot 实验化 | ✅ 本轮已完成 |
| 1 | 硬件在环（HIL）bring-up + 链接脚本集成 | 🟡 链接脚本已完成；**软件在环（QEMU）已跑通 blinky + uart_hello**（[ws63-qemu](https://github.com/sanchuanhehe/ws63-qemu)）；上板冒烟待硬件 |
| 2 | 死代码清理 + 正确性修复 | 🟡 部分完成（SPI/eFuse/LSADC 寄存器 + **中断子系统重写** + 可复现 SVD→PAC 流水线已修；死代码/I2C 超时/复位等待做） |
| 3 | 链接/blob 尖刺 | 计划 |
| 4 | porting 层 + HCC IPC | 计划 |
| 5 | 连接性示例（scan → connect → ping） | 计划 |
| 6 | async（embassy） | 计划 |

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
4. **统一 trap/栈布局**（剩余）：把 `.stacks`（NOLOAD）与 `memory.x` 里的栈顶 fallback 合并为单一真值；
   在 `layout.ld` 显式放置 `.trap`/`.trap.*` 段（`KEEP` + `ALIGN(64)`）；`startup.S` 设 Vectored 模式
   或改 Direct + 软件分发（与中断重写一并，见阶段 2）。
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
>
> 注：1–3 为静态对照 SDK 的修复，**仍未上板验证**（属阶段 1 门禁）；4 已在 QEMU 验证投递闭环但仍非真机。
> 其余阶段 2 项目（死代码、I2C 超时、reset、GPIO pull、safety.rs、host 单测、flashboot）仍待做。

**死代码清理（删）**
- `clock.rs`：`ClockControl` / `PeripheralGuard` / `REF_COUNTS`（RAII 时钟守卫，零消费者）。
- `private.rs`：`DriverMode` / `Blocking` / `Async`（关联类型恒等，零引用）。
- `dma.rs`：`DmaEligible` / `DmaChannelFor`（无 impl、无驱动接线）。
- `safety.rs`：9 条恒真断言（行 56,67-68,72-75,89-91）。
- **保留哨兵**：`Peripheral` enum + `PERIPHERAL_COUNT`、真正的跨模块漂移断言（safety.rs 37-44,79-85）。

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
- **I2C 超时**（高，剩余）：`i2c.rs` 仍有多处无界 `while !…{}`（行 59/80/130/153…）；加有界计数并接入既有
  `I2cError::Timeout`。（SPI 侧已完成，见上。）
- **system reset**（高）：`software_reset` 用 `GLB_CTL_M(0x40002110)`、`reset_reason` 解码 `SYS_RST_RECORD(0x400000A0)`，
  未实现前用 `todo!()` 而非返回似是而非的假值。
- **GPIO pull**（中）：`InputConfig.pull` 经 IO_CONFIG 落地，或移除字段；中断加 trigger 类型。
- ✅ **efuse/lsadc**（高，已修 2026-05-31）：通过改 SVD + 重生成 PAC + 重写 HAL 落地——
  - eFuse：控制块 base+0x30、16 位模式魔数（`0x5A5A` 读 / `0xA5A5` 写）、0x800 数据窗口（128 字），
    `efuse.rs` 改为字节读写（窗口索引 `byte/2`、奇偶字节抽取），对齐 `hal_efuse_v151.c`。
  - LSADC：重写为连续 `adc_regs_t`——使能/复位 `CTRL_11`（`da_lsadc_en[15:0]`/`rstn`@16）、扫描 `CTRL_0`、
    启停 `CTRL_8`、FIFO 读 `CTRL_9`、空判定 `CTRL_1.rne`、`CFG_*` @ 0xDC..0xEC，对齐 `hal_adc_v154`。
  - 偏移已在生成的 PAC 中逐一核验；纯解析逻辑有 proptest。**未上板**。
- **safety.rs**：删恒真断言并去掉"formal verification"措辞。
- **host 单测**：把 `ws63-hal` 的 RISC-V 内联汇编（如 `system.rs` 的 `ebreak`）用 `#[cfg(target_arch=…)]`
  门控，使库能为 host 编译，从而真正运行单测（现状：库含 riscv asm，host 根本编不过，旧 CI 用 `|| echo` 掩盖）。
- **flashboot**（若继续维护）：`CodeInfo`/`KeyArea` 按 `secure_verify_boot.h` 重生成；A/B 用分区表/升级配置
  而非误用 `0x40000024`。否则保持实验性、不投入。

**PAC/SVD 流水线**
- ✅ **可复现生成脚本（已完成 2026-05-31）**：`ws63-svd/regen.sh` + `postprocess.py`——pin `svd2rust@0.37.1`/`form@0.13.0`，
  补齐 svd2rust→edition 2024 的三处确定性落差（删 5 个 dim 重复 TIMER 访问器、`#[no_mangle]`→`#[unsafe(no_mangle)]`、
  `cargo fix` 套 `unsafe_op_in_unsafe_fn`），**幂等**（同 SVD→字节一致 lib.rs）、build+clippy 内建门禁。
  **停止手补 lib.rs**（主仓 PreToolUse hook 已拦截手改）。再生成同时恢复了手补漏掉的 KM keyslot 字段。
- 剩余：CI 增"从 SVD 重生成并 diff 校验"步骤（脚本已就绪，接 CI 即可）；逐外设对照 fbb_ws63 `*_reg.h` 核覆盖
  （本轮已核 SPI/eFuse/LSADC，其余待逐个过）。

---

## 阶段 3 — 链接/blob 尖刺

先消除最大未知：写最小 crate 链接 `ws63-RF/lib/libwifi_rom_data.a`（仅 3KB）并解析其外部符号，
**证明工具链/链接路径走得通**。ABI 已就绪：仓库已用 `ws63` 硬浮点工具链（`riscv32imfc`/`ilp32f`），与 blob ABI 一致。

---

## 阶段 4 — porting 层 + HCC IPC

实现 `ws63-RF/include/port/` 的最小桩（`port_log`/`port_osal`/`port_oal`）+ 与 Wi-Fi/BT 协处理器的
共享内存 HCC IPC，架在现有 DMA/UART 驱动之上。这是到产品**最大、最高风险**的一块。

---

## 阶段 5 — 连接性示例

交付 `Wi-Fi scan → connect → ping`——决定能否被采用的 demo。补 `uart_echo`/`i2c_scan`/`spi_loopback`
等上板验证过的外设示例，用示例作为驱动正确性的验收，替代 host 端恒真单测。

---

## 阶段 6 — async

接 `embassy-executor` + 中断驱动 I/O，让一个驱动（UART 或 timer）真正异步，作为概念验证。
在此之前不要保留空的 async 类型状态（阶段 2 已删 `Blocking`/`Async` marker）。

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
| 高 | crates.io 发布链路坏 + 失败被静默吞 | ✅ 阶段 0 已修（结构） |
| 高 | release 不挂固件产物 | ✅ 阶段 0 已修（blinky 链接后生效） |
| 高 | porting 层 + HCC IPC 完全未实现 | 阶段 4 |
| 高 | I2C/SPI 无超时死循环 | 🟡 SPI 已修（有界超时）；I2C 待做（阶段 2） |
| 高 | eFuse 读路径/控制偏移错误 | ✅ 阶段 2 已修（2026-05-31） |
| 高 | LSADC 寄存器映射整块错位（ctrl_7/fifo 等） | ✅ 阶段 2 已修（2026-05-31） |
| 高 | 从未上板验证 / 测试恒真 | 阶段 1 |
| 中 | 死代码（RAII 时钟守卫 / DMA 安全层 / async marker） | 阶段 2 |
| 中 | safety.rs 恒真断言剧场 | 阶段 2 |
| 中 | SVD→PAC 生成流水线不可复现 | ✅ 阶段 2 已修（regen.sh 幂等可复现，2026-05-31） |
| — | 示例无法链接（链接脚本不传播） | ✅ 阶段 1 已修（blinky 可链接） |
