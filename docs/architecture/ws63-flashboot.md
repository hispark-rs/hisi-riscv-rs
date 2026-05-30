# ws63-flashboot 架构与评审

> 本文是 ws63-rs 架构文档的一部分。完整评审台账见 [架构评审 2026-05](../review/architecture-review-2026-05.md)，整改排期见 [ROADMAP](../../ROADMAP.md)。

## 职责与边界

`ws63-flashboot` 是一个**实验性 / 学习用途**的 Rust 二级引导（second-stage bootloader），对标 fbb_ws63 原厂 `flashboot_ws63/startup/main.c`。它本轮（2026-05-31）已被明确标注为**实验性、非安全启动、不可用于生产**（`src/main.rs:1-22`、`README.md:3`、`Cargo.toml:5`）。

**负责**（最小化的引导流程）：

- 汇编启动：PMP 清零、`mtvec` 向量模式、关中断、开 FPU、初始化 `gp`/`sp`、清 BSS、跳 `flashboot_main()`（`asm/startup.S`）。
- 时钟切换：Flash/UART 从 TCXO 切到 PLL（`src/main.rs:262-278`）；TCXO 频率检测（24/40 MHz，`src/main.rs:65-69`）。
- SFC（SPI Flash Controller）四线读初始化与按块读取（`src/sfc.rs:83-171`）。
- 镜像头边界校验（`src/image.rs:9-19`）与软件 SHA256 完整性校验（`src/sha256.rs`、`src/main.rs:235-258`）。
- 看门狗、eFuse 时钟周期初始化、FAMA 重映射，最后跳转到 app 入口（`src/main.rs:280-296`、`192-202`、`100-107`、`135-167`）。
- 独立的只写 UART0 调试输出（`src/uart.rs`）。

**不负责 / 当前不具备**：

- **真实性验签（secure boot）** —— 没有基于 efuse 根密钥的 ECC-bp256 / SM2 签名校验。
- 分区表解析、A/B app 槽选择、FOTA / 升级、镜像解压、flash 在线加密 —— 这些在原厂 flashboot 中存在，本 crate 为桩或缺失（`src/main.rs:206-231`）。
- 不依赖 `ws63-pac` / `ws63-hal`：有意用裸 MMIO 保持独立、避免第二份 PAC 在链接期与 `ws63-hal` 的 `DEVICE_PERIPHERALS` 冲突（`Cargo.toml:17-19`）。

生产正确做法：复用 fbb_ws63 原厂 flashboot，把本仓库构建的 Rust 应用按原厂打包/签名流程烧到原厂 flashboot 加载的 APP 分区（`README.md:22-26`）。

## 在依赖链中的位置

`ws63-flashboot` **不在** 主依赖链（SVD → PAC → HAL → examples）上，是一条独立的二进制旁支：

```
SVD → ws63-pac → ws63-hal → ws63-examples/*   （主链，ws63-rt 提供启动）

ws63-flashboot （独立 bin，自带 startup.S / uart / sfc / sha256，裸 MMIO，
                不依赖 pac/hal/rt；被排除在默认构建之外）
```

- 它是一个 `[[bin]]`（`Cargo.toml:13-15`，产物名 `flashboot`），仅依赖 `riscv` 与 `critical-section`（`Cargo.toml:20-22`）。
- 在工作区中它是 `members` 之一（`cargo check --workspace` 仍覆盖），但**不在 `default-members`** 中，默认 `cargo build` 不构建它（根 `Cargo.toml` `default-members` 仅含 `ws63-pac`/`ws63-hal`/`ws63-rt`）。
- 它逻辑上位于 PAC/HAL 之"下"：在硬件上电后、Rust 应用（用 `ws63-rt` 启动 + `ws63-hal` 驱动）运行之"前"运行，但在代码上与三者完全解耦。

## 关键设计

- **裸 MMIO 而非 PAC**：所有外设地址硬编码为 `*mut u32`/`*const u32` 常量（`src/main.rs:40-48`，`src/sfc.rs:9-27`，`src/uart.rs:8-15`），刻意不引入 `ws63-pac`，避免双份 PAC 链接冲突（`Cargo.toml:17-19`）。代价是与 HAL 重复造 UART/SFC/SHA256/startup（见评审）。
- **汇编启动对照原厂**：`asm/startup.S` 注释声明基于 fbb_ws63 `flashboot_ws63/startup/riscv_init.S`，做 PMP 清零、清自定义 CSR `0x7d9`、从 `a0` 保存 boot flag 到 `__flash_boot_flag`、`mtvec` 向量模式（+1）、开 FPU（`mstatus.FS=0b11`）、清 BSS、`tail flashboot_main`。
- **地址与 magic 对照 SDK**：`FLASH_BOOT_TYPE = 0x4000_0024`、`BOOT_MAIN = 0xA5A5_A5A5`、`FLASH_START = 0x0020_0000` 与原厂一致（vendor `main.c:50-52`：`FLASH_BOOT_TYPE_REG 0x40000024` / `_MAIN 0xA5A5A5A5` / `_BKUP 0x5A5A5A5A`）。
- **镜像头数据结构**：`ImageHeader = KeyArea(0x100) + CodeInfo(0x200) = 0x300`（`src/sfc.rs:32-61`）。`CodeInfo` 自注偏移：`image_length` 在 +0x114、`image_hash` 在 +0x11C（`src/sfc.rs:51-52`）。**这些偏移与真实 WS63 镜像不符**（见评审，对照 vendor `secure_verify_boot.h:156-178` 的 `image_code_info_t`）。
- **校验流程**：`validate()` 做边界检查（`image_id`、`structure_length∈{0x200,0x400}`、`image_length∈(0,8MB)`、`signature_length∈(0,512]`、`structure_version==0x0001_0000`，`src/image.rs:9-19`），随后 `verify_sha256()` 分 256 字节块读 app body、软件 SHA256、与头里的 `image_hash` 比对（`src/main.rs:235-258`）。SHA256 实现完整且正确（标准 H/K 常量、压缩函数、大端长度填充，`src/sha256.rs`，含 `""`/`"abc"`/长输入测试 `:148-175`）。
- **SFC**：`sfc_init()` 配置四线快读（rd_ins=0xEB Quad I/O，`src/sfc.rs:99-104`）；`sfc_read_data()` 以 16 字（64 字节）为硬件上限分块、轮询 `SFC_INT_STATUS` 完成位（`src/sfc.rs:137-171`）。
- **跳转**：清 `mie`、喂狗后将 `addr + 0x300` `transmute` 为 `extern "C" fn() -> !` 并调用（`src/main.rs:159-166`），SAFETY 注释声明 app 入口同 ABI（RV32IMFC ilp32f）。
- **本轮构建完整性修复（针对该 crate）**：banner 重写为"非安全启动"警告（`src/main.rs:1-22`）、`publish = false`（`Cargo.toml:11`）、移出 `default-members`、删除未用的 `ws63-pac` 依赖、新增 `README.md`。

## 评审发现

> 已对照 fbb_ws63 与 esp-hal、按 file:line 验证，0 条被驳回。

### 优点

- SHA256 软件实现正确，常量与填充无误，含已知向量单测（`src/sha256.rs:14-141`、`:148-175`）。
- `startup.S` 对照原厂 `riscv_init.S`，PMP/FPU/BSS/boot flag 处理到位（`asm/startup.S`）。
- 关键地址与 magic（`FLASHBOOT_RAM` 语义、`FLASH_BOOT_TYPE=0x40000024`、`BOOT_MAIN=0xA5A5A5A5`）对照 SDK 一致（`src/main.rs:45,57` vs vendor `main.c:50-52`）。
- 镜像头边界校验有较完整的拒绝/接受边界单测（`src/image.rs:52-135`）。
- 本轮已正确自我定级为实验性：banner、`publish=false`、移出默认构建、README 说明（`src/main.rs:1-22`、`Cargo.toml:11`、`README.md`）。

### 问题

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|--------|------|------|-----------------|------|
| 严重 | 安全 | 无真实性验签：`verify_sha256()` 只把算出的哈希与**同一份未签名头里的** `image_hash` 比对。能写 flash 的攻击者改镜像后重算 SHA256 写回头部即可以 M 态特权跳进任意代码，≠ secure boot（原厂用 efuse 根密钥 ECC-bp256/SM2 签名验签） | `src/main.rs:150,235-258`；对照 vendor `secure_verify_boot.c`、`upg_verify.c:671-723` | 已排期(ROADMAP 阶段 2) |
| 严重 | 正确性 | `ImageHeader`/`CodeInfo` 布局对不上真实 WS63 镜像：`image_length`(+0x114)/`image_hash`(+0x11C) 偏移读错。原厂 `image_code_info_t` 在这些字段前还有 `version_ext`/`mask_version_ext`/`msid_ext`/`mask_msid_ext`、随后是 `code_area_addr`/`code_area_len`/`code_area_hash` 及签名区，布局不同 → 会拒绝真镜像 | `src/sfc.rs:44-54`；对照 vendor `secure_verify_boot.h:156-178` | 已排期(ROADMAP 阶段 2) |
| 高 | 正确性 | A/B 误用 `0x4000_0024`：该寄存器是 flashboot **自身的备份恢复标志**（main flashboot vs backup flashboot），并非 app 槽选择器。代码却用它选 app 区 A/B | `src/main.rs:45,118-127`；对照 vendor `main.c:131-135`（`flashboot_need_recovery`） | 已排期(ROADMAP 阶段 2) |
| 高 | 方向 | 重写原厂安全关键件（验签/启动链）属误导努力。生产应复用原厂 flashboot，本 crate 仅供学习 | `src/main.rs:5-8`、`README.md:22-26` | 暂不修(定级实验性；定位为学习件，整体方向走复用原厂) |
| 高 | 正确性 | 关键子流程是桩：`boot_clock_adapt()` 为 TODO 空操作；`read_partition_app_addr()` 恒返回 `FLASH_START`；`check_upgrade_mode()` 恒 false | `src/main.rs:171-188,206-215,219-224` | 已排期(ROADMAP 阶段 2) |
| 中 | 维护性 | 重复造轮子：UART/SFC/SHA256/startup 与 `ws63-hal`/`ws63-rt` 重复（因刻意不依赖 PAC/HAL） | `src/uart.rs`、`src/sfc.rs`、`src/sha256.rs`、`asm/startup.S`、`Cargo.toml:17-19` | 暂不修(为保持独立、规避双份 PAC 链接冲突的有意取舍) |
| 中 | 工程化 | 删除未用的 `ws63-pac` 依赖、`publish=false`、移出默认构建、banner 改为实验性警告 | `Cargo.toml:11,17-19`、根 `Cargo.toml` `default-members`、`src/main.rs:1-22` | 本轮已修 |

## 改进项与排期

- 生产层面的结论是**复用 fbb_ws63 原厂 flashboot**（已做签名验签 / A/B / 升级 / 解压 / flash 加密），Rust 应用以 app 镜像形式由原厂 flashboot 加载（`README.md:22-26`）。本 crate 维持实验/学习定位。
- 若继续维护本 crate，正确性整改集中在 **ROADMAP 阶段 2（死代码清理 + 正确性修复）**：efuse/lsadc 寄存器、flashboot 镜像头布局 / 验签 / A/B 语义修正。
- 本轮（阶段 0）已完成的构建完整性修复已落地：双份 PAC 消除（registry 版本依赖 + 根 `[patch.crates-io]` 指向本地）、ISA 改为无原子 `riscv32imc-unknown-none-elf` + `portable-atomic` critical-section polyfill、CI/release gating 与发布顺序修复、`ws63-rt` MIE 中断宏 typo 与栈顶符号 GC fallback 修复。
- 尚未解决并已排期：示例链接（`ws63-rt` 链接脚本不传播到下游 bin）见 **阶段 1**；中断模型（PLIC vs LOCIPRI/LOCIEN）、SPI/I2C/SPI 超时、system reset、GPIO pull、死代码清理见 **阶段 2**；porting 层 + HCC IPC + blob 链接的连接性见 **阶段 3–5**；async 见 **阶段 6**。详见 [ROADMAP](../../ROADMAP.md)。
