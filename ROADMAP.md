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

---

## 阶段总览

| 阶段 | 主题 | 状态 |
|------|------|------|
| 0 | 构建完整性 + 文档 + flashboot 实验化 | ✅ 本轮已完成 |
| 1 | 硬件在环（HIL）bring-up + 链接脚本集成 | 🟡 链接脚本集成已完成（blinky 可链接）；上板冒烟待硬件 |
| 2 | 死代码清理 + 正确性修复 | 计划 |
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
3. **统一 trap/栈布局**（剩余）：把 `.stacks`（NOLOAD）与 `memory.x` 里的栈顶 fallback 合并为单一真值；
   在 `layout.ld` 显式放置 `.trap`/`.trap.*` 段（`KEEP` + `ALIGN(64)`）；`startup.S` 设 Vectored 模式
   或改 Direct + 软件分发（与中断重写一并，见阶段 2）。
4. **上板冒烟**（剩余，需硬件）：真机烧 blinky → UART hello-world，验证 `clock_init`/linker/startup 在硅片上正确。
   用 `readelf` 核实 ELF 内存布局已确实采用 WS63 的 layout（非 lld 默认）。

---

## 阶段 2 — 死代码清理 + 正确性修复

基调：**删无用、留哨兵**。

**死代码清理（删）**
- `clock.rs`：`ClockControl` / `PeripheralGuard` / `REF_COUNTS`（RAII 时钟守卫，零消费者）。
- `private.rs`：`DriverMode` / `Blocking` / `Async`（关联类型恒等，零引用）。
- `dma.rs`：`DmaEligible` / `DmaChannelFor`（无 impl、无驱动接线）。
- `safety.rs`：9 条恒真断言（行 56,67-68,72-75,89-91）。
- **保留哨兵**：`Peripheral` enum + `PERIPHERAL_COUNT`、真正的跨模块漂移断言（safety.rs 37-44,79-85）。

**正确性修复**
- **中断子系统**（严重）：按 riscv31 自定义 CSR（`LOCIPRI`/`LOCIEN`/`LOCIPD`）重写 `interrupt.rs`，或诚实标注为桩
  并撤出 `prelude`；删除"PLIC"措辞。
- **SPI**（严重）：`spi.rs:76` `ctra trsm` 由 `3`(EEPROM-Read) 改 `0`(TX&RX)。
- **I2C/SPI 超时**（高）：所有 `while !…` 加有界计数，超时返回既有的 `I2cError::Timeout`/`BusError`/`SpiError::Overflow`。
- **system reset**（高）：`software_reset` 用 `GLB_CTL_M(0x40002110)`、`reset_reason` 解码 `SYS_RST_RECORD(0x400000A0)`，
  未实现前用 `todo!()` 而非返回似是而非的假值。
- **GPIO pull**（中）：`InputConfig.pull` 经 IO_CONFIG 落地，或移除字段；中断加 trigger 类型。
- **efuse/lsadc**（高）：按 `hal_efuse_v151.c` 区地址方案重写 efuse 读路径；按 SDK 解决 lsadc ctrl_7 位段歧义。
- **safety.rs**：删恒真断言并去掉"formal verification"措辞。
- **host 单测**：把 `ws63-hal` 的 RISC-V 内联汇编（如 `system.rs` 的 `ebreak`）用 `#[cfg(target_arch=…)]`
  门控，使库能为 host 编译，从而真正运行单测（现状：库含 riscv asm，host 根本编不过，旧 CI 用 `|| echo` 掩盖）。
- **flashboot**（若继续维护）：`CodeInfo`/`KeyArea` 按 `secure_verify_boot.h` 重生成；A/B 用分区表/升级配置
  而非误用 `0x40000024`。否则保持实验性、不投入。

**PAC/SVD 流水线**
- 提交可复现的 svd2rust 生成脚本（pin 版本 + form/fmt）；CI 增"从 SVD 重生成并 diff 校验"；停止手补 lib.rs。
- 补齐 KM `*_FLUSH_BUSY` 等缺失寄存器；逐外设对照 fbb_ws63 `*_reg.h` 核覆盖。

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
| 严重 | 中断子系统建在不存在的 PLIC 模型上 | 阶段 2 |
| 严重 | SPI 传输模式位写成 EEPROM-Read | 阶段 2 |
| 严重 | flashboot 无真实性验签（≠ secure boot） | 实验化（阶段 0），整改阶段 2 |
| 严重 | flashboot 镜像头布局对不上真实镜像 | 实验化（阶段 0），整改阶段 2 |
| 严重 | 连接性交付 0%（chip 价值未触及） | 阶段 3-5 |
| 高 | 默认 target ISA 含原子扩展（硅片无 A） | ✅ 阶段 0 已修 |
| 高 | crates.io 发布链路坏 + 失败被静默吞 | ✅ 阶段 0 已修（结构） |
| 高 | release 不挂固件产物 | ✅ 阶段 0 已修（blinky 链接后生效） |
| 高 | porting 层 + HCC IPC 完全未实现 | 阶段 4 |
| 高 | I2C/SPI 无超时死循环 | 阶段 2 |
| 高 | 从未上板验证 / 测试恒真 | 阶段 1 |
| 中 | 死代码（RAII 时钟守卫 / DMA 安全层 / async marker） | 阶段 2 |
| 中 | safety.rs 恒真断言剧场 | 阶段 2 |
| 中 | SVD→PAC 生成流水线不可复现 | 阶段 2 |
| — | 示例无法链接（链接脚本不传播） | ✅ 阶段 1 已修（blinky 可链接） |
