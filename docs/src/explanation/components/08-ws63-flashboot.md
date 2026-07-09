# ws63-flashboot 架构

> 本文是 ws63-rs 组件深入文档的一部分，聚焦当前架构、职责边界和设计原因。历史评审快照见 [组件评审快照](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/review/component-review-snapshots-2026-05.md)，当前优先级见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

## 职责与边界

`ws63-flashboot` 是一个**实验性 / 学习用途**的 Rust 二级引导（second-stage bootloader），对标 fbb_ws63 原厂 `flashboot_ws63/startup/main.c`。它本轮（2026-05-31）已被明确标注为**实验性、非安全启动、不可用于生产**（`src/main.rs:1-22`、`README.md:3`、`Cargo.toml:5`）。当前专为 **WS63 设计**；BS2X 系列（BS21/BS22/BS20）的引导加载另行开发（见下），复用原厂 flashboot 是生产推荐。

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
- 不依赖 `ws63-pac` / `hisi-riscv-hal`：有意用裸 MMIO 保持独立、避免第二份 PAC 在链接期与 `hisi-riscv-hal` 的 `DEVICE_PERIPHERALS` 冲突（`Cargo.toml:17-19`）。

生产正确做法：复用 fbb_ws63 原厂 flashboot，把本仓库构建的 Rust 应用按原厂打包/签名流程烧到原厂 flashboot 加载的 APP 分区（`README.md:22-26`）。

## 在依赖链中的位置

`ws63-flashboot` **不在** 主依赖链（SVD → PAC → HAL → examples）上，是一条独立的二进制旁支：

```console
SVD → ws63-pac → hisi-riscv-hal → examples/ws63/*   （主链，hisi-riscv-rt 提供启动）

ws63-flashboot （独立 bin，自带 startup.S / uart / sfc / sha256，裸 MMIO，
                不依赖 pac/hal/rt；被排除在默认构建之外）
```

- 它是一个 `[[bin]]`（`Cargo.toml:13-15`，产物名 `flashboot`），仅依赖 `riscv` 与 `critical-section`（`Cargo.toml:20-22`）。
- 在工作区中它是 `members` 之一（`cargo check --workspace` 仍覆盖），但**不在 `default-members`** 中，默认 `cargo build` 不构建它（根 `Cargo.toml` `default-members` 是库 + WS63 示例集合，不含 `chips/ws63/flashboot`）。
- 它逻辑上位于 PAC/HAL 之"下"：在硬件上电后、Rust 应用（用 `hisi-riscv-rt` 启动 + `hisi-riscv-hal` 驱动）运行之"前"运行，但在代码上与三者完全解耦。

## 关键设计

- **裸 MMIO 而非 PAC**：所有外设地址硬编码为 `*mut u32`/`*const u32` 常量（`src/main.rs:40-48`，`src/sfc.rs:9-27`，`src/uart.rs:8-15`），刻意不引入 `ws63-pac`，避免双份 PAC 链接冲突（`Cargo.toml:17-19`）。代价是与 HAL 重复造 UART/SFC/SHA256/startup（见评审）。
- **汇编启动对照原厂**：`asm/startup.S` 注释声明基于 fbb_ws63 `flashboot_ws63/startup/riscv_init.S`，做 PMP 清零、清自定义 CSR `0x7d9`、从 `a0` 保存 boot flag 到 `__flash_boot_flag`、`mtvec` 向量模式（+1）、开 FPU（`mstatus.FS=0b11`）、清 BSS、`tail flashboot_main`。
- **单镜像启动（2026-06-01 整改）**：删除了对 `0x4000_0024` 的 A/B 误用。该寄存器是 flashboot **自身**的备份恢复标志（`0x5A5A5A5A` ⇒ 从备份分区恢复 bootloader；vendor `main.c:131-135` `flashboot_need_recovery`），**不是** app 槽选择器。真实 app A/B 由 upg run-region 配置（`PARTITION_FOTA_DATA` 末尾 magic `0x70746C6C`、`run_region` 0=A/1=B）+ 分区表（`@0x200380`）决定 —— 本实验 loader 不解析这些，仅启动单一 app 镜像，A/B/恢复/FOTA 交给原厂 flashboot（`src/main.rs:110-131`）。
- **镜像头数据结构（整改：对齐 secure_verify_boot.h）**：`ImageHeader = KeyArea(0x100) + CodeInfo(0x200) = 0x300`，按 vendor `image_key_area_t`/`image_code_info_t`（ECC256/SM2 构建）逐字段重排（`src/sfc.rs`）。`CodeInfo` 的关键字段现在正确：`code_area_len` 在 +0x24（旧代码错读 `mask_version_ext`@+0x14 当长度）、`code_area_hash` 在 +0x28（旧代码错读 +0x1C）。`const` 断言锁定 `size_of` = 0x100/0x200/0x300。
- **校验流程**：`validate()` 做结构边界检查（`image_id`、`structure_version==0x0001_0000`、`structure_length∈{0x200,0x400}`、`signature_length∈(0,512]`、`code_area_len∈(0,8MB)`，`src/image.rs`），随后 `verify_image_integrity()`（原 `verify_sha256`）分 256 字节块读 app body、软件 SHA256、与头里的 `code_area_hash` 比对（`src/main.rs`）。**这是完整性校验、非真实性验签**：哈希在同一份未签名头里，能写 flash 的攻击者可重算 —— 函数名/文档已如实标注。SHA256 软件实现（`src/sha256.rs`，含 `""`/`"abc"`/长输入测试）未经审计、仅作完整性用途。
- **SFC**：`sfc_init()` 配置四线快读（rd_ins=0xEB Quad I/O，`src/sfc.rs:99-104`）；`sfc_read_data()` 以 16 字（64 字节）为硬件上限分块、轮询 `SFC_INT_STATUS` 完成位（`src/sfc.rs:137-171`）。
- **跳转**：清 `mie`、喂狗后将 `addr + 0x300` `transmute` 为 `extern "C" fn() -> !` 并调用（`src/main.rs:159-166`），SAFETY 注释声明 app 入口同 ABI（RV32IMFC ilp32f）。
- **本轮构建完整性修复（针对该 crate）**：banner 重写为"非安全启动"警告（`src/main.rs:1-22`）、`publish = false`（`Cargo.toml:11`）、移出 `default-members`、删除未用的 `ws63-pac` 依赖、新增 `README.md`。

## 历史评审

本文只保留当前架构解释。2026-05 的逐项评审快照已归档到 [组件评审快照](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/review/component-review-snapshots-2026-05.md)，当前优先级以根目录 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md) 和对应 reference 页面为准。

## 注记：BS2X 引导加载（BS21/BS22/BS20）

WS63-flashboot 当前**专为 WS63 SoC 实现**。BS2X 系列（BS21/BS22/BS20）作为独立芯片系列，有自己的：

- **ROM 代码**：不同的掩膜 ROM 版本与启动流程（相似但非完全兼容）。
- **原厂 flashboot**：fbb_bs2x 中的 flashboot_bs2x（结构类似但地址/配置寄存器有差异）。
- **推荐方案**：复用 fbb_bs2x 的原厂 flashboot 加载 Rust 应用镜像；若需自研，按 WS63-flashboot 模式（对照 secure_verify_boot.h 等）另行实现。

QEMU 验证侧，`-M bs21/bs22/bs20` 已支持硬件仿真；vendor 的 LiteOS 栈由 hisi-riscv-qemu 虚拟；BS2X 真机引导加载与连接性由 BS2X 团队后续跟进（见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)）。
