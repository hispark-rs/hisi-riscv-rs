# 组件评审快照（2026-05）

> 本文件归档原先散落在 `docs/src/explanation/components/` 下的组件评审表。
> 它是历史审计快照，不代表当前路线图优先级；当前计划以根目录 `ROADMAP.md` 为准。

这些内容从 explanation 页面迁出，是为了让组件深入文档专注解释组件职责、边界和设计原因；
评审发现、严重度表、整改排期这类维护台账统一放在 `docs/review/`。

## ws63-svd

来源：`docs/src/explanation/components/02-ws63-svd.md`。

## 评审发现

### 优点

- 建模覆盖广：36 外设 / 497 寄存器，`enumeratedValues`、`derivedFrom`、`writeConstraint`、`addressBlock` 一应俱全，是一份结构完整、可被 svd2rust 直接消费的 1.3 版 SVD。
- 通过 `derivedFrom` 对同构外设（GPIO/UART/I2C/SPI/SDMA）做了正确复用，降低了维护面。
- 提供了针对官方 CMSIS XSD 的格式校验脚本，建模本身有质量门可依。
- UART/GPIO/KM 等关键外设建模到字段+枚举级，下游 HAL 可直接获得类型安全的位域访问。

### 问题

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
| --- | --- | --- | --- | --- |
| 高 | 维护性 | 手补代码曾被手工补进**已格式化的 PAC 生成代码**，而非回填 SVD 后重生成，clean regen 会丢失或冲突。 | 历史提交 df35d69「add missing KM keyslot registers」；该批字段在 `WS63.svd` KM 外设中存在但生成链曾未联动 | ✅ 已修(2026-05-31)：建立 `regen.sh`、停止手补；重生成时 PAC 反而**恢复**了手补遗漏的 KM keyslot 字段（`flush_hmac_kslot_ind`/`tscipher_ind`/`lock_cmd`/`key_slot_num`） |
| 中 | 维护性 | 无可复现生成流水线：`main.py` 是 `print(...)` 桩，无 svd2rust 调用；`ws63-settings.yaml` 在 `base_isa: rv32i` 截断；CI 中无 SVD 引用 | `main.py:1-6`；`ws63-settings.yaml`；`.github/workflows/` 无 SVD 引用 | ✅ 已修(2026-05-31)：`regen.sh`+`postprocess.py` 幂等可复现、build+clippy 门禁；CI 接入（“重生成并 diff”）为剩余小项 |
| 高 | 正确性 | eFuse/LSADC 外设建模错误：eFuse 控制寄存器偏移错位（0x00 段）、`wr_rd` 建成单 bit 而非 16 位魔数、缺 0x800 数据窗口；LSADC 寄存器整块错位（使能/启停/FIFO 寄存器选错） | 评审台账 + 本轮对照 `hal_efuse_v151`/`hal_adc_v154` | ✅ 已修(2026-05-31)：eFuse 控制块移到 base+0x30、16 位魔数、加 0x800 窗口；LSADC 重写为连续 `adc_regs_t`（CTRL_8/9/11、CFG_* @0xDC..0xEC）。偏移已在生成 PAC 中逐一核验 |
| 中 | 正确性 | 覆盖不全：KM 的 `*_FLUSH_BUSY` 状态寄存器（偏移 0xB10–0xB1C）缺失，KM 偏移从 `0x1B0C` 直接跳到 `0x1B30`，存在转录静默缺口 | `WS63.svd` KM 外设；addressOffset 序列断档；`grep FLUSH_BUSY` 无命中 | 历史整改项遗留：`flush_hmac_kslot_ind` 字段已建模，但 BUSY 查询寄存器本身仍未补；不阻塞当前连接性 C1-C5 |
| 低 | 文档 | `README.md` 为空文件（0 字节），组件无任何使用/维护说明 | `README.md`（0 bytes） | ✅ 已修：README 已补写（含 `regen.sh` 用法、流水线步骤、校验命令、维护约定） |

> 说明：本组件已从“几乎全部已排期”转为**四项中三项已修**（仅 KM `*_FLUSH_BUSY` 转录缺口待补）。这些都是静态对照 fbb_ws63 C SDK 的修复；下游 `ws63-pac` 也已随 `regen.sh` 重生成。当前真机证据边界以 [Stable API 清单](../../reference/10-stable-api.md) 为准。

## 改进项与排期

ws63-svd 的整改核心是把 SVD 重新确立为唯一真值。本轮（2026-05-31）已落地大部分：

1. ✅ **建立可复现生成流水线**（已完成）：`regen.sh`（svd2rust 0.37.1 + `postprocess.py` 后处理 + cargo fix/fmt）替代 `main.py` 桩，幂等、build+clippy 门禁。**剩余**：把"从 SVD 重生成并 diff"接入 CI；并加 `validate.py` XSD 校验门（脚本已就绪）。
2. ✅ **以 SVD 为源重生成 PAC**（已完成）：`regen.sh` 即唯一生成路径，手补 lib.rs 被 PreToolUse hook 拦截；重生成恢复了历史手补遗漏的 KM keyslot 字段，消除 SVD↔PAC 漂移。
3. ✅ **eFuse/LSADC 寄存器修复**（已完成）：对照 `hal_efuse_v151`/`hal_adc_v154` 改 SVD 并重生成（详见上表）。**剩余**：KM `*_FLUSH_BUSY`（0xB10–0xB1C）转录缺口仍待补；其它外设逐个对照 fbb_ws63 `*_reg.h` 核覆盖。
4. ✅ **补写 README**（已完成）：含 `regen.sh` 用法、五步流水线、校验命令与"勿手改 lib.rs"约定。

旧阶段编号和完整整改历史见 [归档 roadmap](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/archive/roadmap-2026-05-2026-07-remediation.md)。

## ws63-pac

来源：`docs/src/explanation/components/03-ws63-pac.md`。

## 评审发现

### 优点

- svd2rust **0.37.1** 现代 const-fn 访问器，generic 层零开销（`src/lib.rs:14-51`）。
- 编译快（约 6s 过编译），单文件无复杂构建依赖。
- 工程化完备：`device.x` 中断向量、`critical-section`/`rt` feature（`Cargo.toml:16-18`）、crates.io 元数据齐全（`Cargo.toml:1-9`）。
- 单例语义正确：`take()`/`steal()` 配合 `DEVICE_PERIPHERALS` 标志在临界区内做一次性保护（`src/lib.rs:31760-31775`）。

### 问题

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|--------|------|------|-----------------|------|
| 中 | 维护性 | 单文件 `lib.rs` 体积约 1.8MB / 31814 行，难以审阅与定位 | `src/lib.rs`（1797361 字节、31814 行） | 暂不修（svd2rust 生成产物，按惯例不拆分；通过 CHANGELOG + grep 定位缓解） |
| 高 | 维护性 | 寄存器手补进生成代码：KM keyslot 寄存器（`KC_REECPU_LOCK_CMD` 等）在生成后人工添加，下次重生成会被覆盖 | `src/lib.rs:28415`、`28569`；`CHANGELOG.md:13-21` | 已随 SVD/PAC 流水线整改：应回填到 ws63-svd 源头由生成器产出，避免手补 PAC |
| 中 | 依赖 | 版本曾停在 `0.1.0` 而 tag 后又追加了公开寄存器，违反 SemVer | `Cargo.toml:3`、`CHANGELOG.md` | 已修：bump `0.1.0` →（经 0.1.1/0.1.2）现 `0.1.3`，由 ws63-pac 自有仓库流水线发布 |
| 中 | 依赖 | 曾被 `hisi-riscv-hal` 以 git 依赖引入，导致工作区出现双 PAC 实例 | `crates/hisi-riscv-hal/Cargo.toml:12`、`Cargo.toml:45-51` | 本轮已修：改 registry 版本依赖 + 根 `[patch.crates-io]` 指向本地，`cargo tree` 仅单一 `ws63-pac` 实例 |

## 改进项与排期

2026-05-31 构建完整性整改中，与本 crate 直接相关：

- **双 PAC 消除**：`hisi-riscv-hal`/`ws63-flashboot` 改为 registry 版本依赖，根 `Cargo.toml` 用 `[patch.crates-io]` 统一指向本地（`Cargo.toml:50-51`），全工作区单实例。
- **版本对齐**：`0.1.x` 后随 SPI_WSR / TIMER 修复发布到 **`0.2.0`**；下游 HAL/RT 通过 registry 版本依赖消费，父仓开发用 `[patch.crates-io]` 指向本地 submodule。
- **ISA 协同**：`rt` feature 导出 `RISCV_RT_BASE_ISA=rv32i`（`build.rs:16`），配合默认 target = builtin、无原子的 **`riscv32imfc-unknown-none-elf`**（硬件单精度浮点 ilp32f，原子由 portable-atomic critical-section polyfill 提供）。

仍需后续处理（指向 ROADMAP 对应阶段）：

- **手补寄存器回源**：把 KM keyslot 等人工添加的寄存器回填到 `ws63-svd`，使其由 svd2rust 重生成产出，消除"生成产物被手改"的维护风险；同一轮整改也补齐了 efuse / lsadc 等外设寄存器的正确性。
- **单文件体积**：作为生成产物，按 svd2rust 惯例暂不拆分；若后续 SVD 重构，可评估按外设分模块生成。

## hisi-riscv-hal

来源：`docs/src/explanation/components/04-hisi-riscv-hal.md`。

## 评审发现

### 优点

- **`clock_init.rs` 是全仓标杆**：逐寄存器、逐位对照 fbb_ws63 C SDK 核实，地址与位含义均注明出处（`clock_init.rs:36-74`、`197-253`）。
- **外设单例 + `'d` 生命周期健全**：宏生成统一、`take()` 经 PAC 单例校验，生命周期防 use-after-drop（`peripherals.rs:10-87`）。
- **embedded-hal/embedded-io/nb trait 选型正确**：`SpiBus`（非 `SpiDevice`）、I2C repeated-START、ACK→`NoAcknowledge` 均符合各 trait 契约（`spi.rs:135`、`i2c.rs:215-280`）。
- **GPIO block 映射正确**：`pin/8` 分 block、`pin%8` 取位，与 3 block × 8 位的硬件布局一致（`gpio.rs:86-88`）。

### 问题

> 下表为 **2026-05 评审快照**；其后多数正确性整改项已修（见各行状态），权威进度以 [评审台账](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/review/architecture-review-2026-05.md) 为准。全部修复在姊妹仓 `ws63-qemu` 软件在环验证。

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|--------|------|------|-----------------|------|
| 严重 | 正确性 | 中断子系统曾建在不存在的 PLIC 模型上。WS63 用自定义 CSR（`LOCIPRI`=0xBC0 / `LOCIEN`=0xBE0 / `LOCIPD`=0xBE8） | `interrupt.rs` | ✅ 阶段2已修：重写为 LOCIPRI/LOCIEN/LOCIPD CSR 模型 + 优先级/阈值；ws63-qemu `timer_irq`/`gpio_irq`(IRQ≥32) 端到端验证 |
| 严重 | 正确性 | SPI `ctra` 写入 `trsm=3`（bits 19:18），该值是 EEPROM-Read 模式；全双工 TX+RX 应为 `0`。注释误写"TX+RX mode"导致 `transfer`/`SpiBus` 全双工语义不成立 | `spi.rs:76` | ✅ 阶段2已修：TRSM 改为全双工模式，`spi_loopback`/`SpiBus` 语义经 QEMU smoke 验证；真机外部回环仍属示例 smoke/台架增量 |
| 高 | 正确性 | I2C/SPI 多处无超时死循环；错误码定义却从不返回 | `spi.rs`、`i2c.rs` | ✅ 阶段2已修：I2C/SPI 加 bounded 超时并真正返回 `Timeout` 等错误 |
| 高 | 正确性 | `software_reset` 执行 `ebreak`（非系统复位）；`reset_reason` 恒返回 `PowerOn` | `system.rs` | ✅ 阶段2已修：`software_reset` 置 GLB_CTL_M 复位位，`reset_reason` 解析 SYS_RST_RECORD；ws63-qemu `reset_demo` 往返验证 |
| 中 | 正确性 | GPIO `InputConfig.pull` 被静默忽略：`init_input` 只设 OEN | `gpio.rs` | ✅ 阶段2已修：`init_input` 经 IO_CONFIG pad 寄存器应用上下拉 + 中断触发模式 |
| 高 | 正确性 | eFuse / LSADC 寄存器布局为猜测，与 SDK 矛盾 | `efuse.rs`、`lsadc.rs` | 🟡 已对照 fbb_ws63 + ws63-qemu(eFuse 写=按位或、LSADC 转换 IRQ72) 验证读写序列；逐寄存器复核仍作为维护项推进 |
| 中 | 维护性 | `safety.rs` 多条 `const_assert!` 为恒真断言；模块头措辞夸大 | `safety.rs` | ✅ 阶段2已修：删除恒真断言 + 夸大措辞 |
| 中 | 架构 | 零消费者死代码：RAII 时钟守卫、DMA 安全 trait、async marker | `clock.rs`/`dma.rs`/`private.rs` | ✅ 已清：async marker(`Blocking`/`Async`)、vestigial `DmaWord`、RAII 时钟守卫、`DmaEligible`/`DmaChannelFor` 均已删除；真正的异步层按 `async`/`unstable` 分层暴露 |
| 高 | 维护性 | 测试为恒真式（重抄被测公式再断言），从未上板验证 | `spi.rs`/`i2c.rs`/`clock.rs`/`safety.rs` | ✅ 已破：ws63-qemu `smoke-test.sh` 用真实固件端到端验证；WS63 HAL embedded-test 套件已在真机通过，精确覆盖与 stable 边界见参考页 |

## 改进项与排期

本页保留 2026-05 评审后的整改现状；当前优先级以 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md) 为准。

- **Bring-up + 链接脚本集成**：✅ 链接脚本集成已打通（`hisi-riscv-rt` 经 `cargo:rustc-link-search` + `hisi-riscv-link.x`，示例正常链接）；✅ 恒真式测试已由 **ws63-qemu 软件在环**和真机 HAL embedded-test HIL 大幅替代。精确 HIL 覆盖见 [Stable API 清单](../../reference/10-stable-api.md)；示例级 smoke 与连接性 HIL 继续分轨推进。
- **死代码清理 + 正确性修复**：✅ 中断子系统已重写到 `LOCIPRI`/`LOCIEN`/`LOCIPD` CSR 模型；✅ I2C/SPI 超时并返回错误；✅ SPI `trsm` 全双工模式修复；✅ `software_reset`/`reset_reason`；✅ GPIO pull + 中断触发；✅ `safety.rs` 恒真断言 + 夸大措辞已删；✅ async marker / RAII 时钟守卫 / vestigial DMA marker 死代码已删。🟡 eFuse/LSADC 逐寄存器复核仍在推进。
- **新增（超出原评审）**：✅ **异步 HAL**（`async`/`embassy` feature，见 [async-embassy.md](06-async-embassy.md)）已实现；0.6.0 起按 HIL/soundness 证据分层，SPI/I2C blocking-backed async 默认可用，interrupt/waker async 与 embassy 需 `unstable`。
- **连接性支撑**：`ws63-rf-rs` 已承接 RF porting/HCC/netif 数据通路，HAL 的边界是继续提供可组合的底层外设；剩余风险在 blob 链接、pbuf/TX-sink pin 与真机连通。
- **async 后续**：连接性专属的异步包装待 blob 上板后再做；不恢复旧的空壳 `Blocking`/`Async` 类型状态。

## ws63-examples

来源：`docs/src/explanation/components/07-ws63-examples.md`。

## 评审发现

### 优点

- 入口形态正确：`#[entry]` + `#[panic_handler]` 的裸机骨架完整，`blinky` 可作后续示例的模板。
- 覆盖面已大幅扩展：GPIO / UART / Timer / DMA + 中断 + 复位 + semihosting + 自定义内存 + async/embassy + RF porting 等示例均在参考页登记。
- 链接已打通且诚实标注：WS63 示例集合纳入 `default-members`，`cargo build` 默认即构建；`ws63-flashboot` 的排除附了原因注释。

### 问题

| 严重度 | 类别 | 问题 | 状态 |
|--------|------|------|------|
| 高 | 构建 | （曾）`blinky` 无法链接：lib 依赖的 `cargo:rustc-link-arg` 不传播到下游二进制 | ✅ 已修：`hisi-riscv-rt` 导出 `hisi-riscv-link.x` + 各 `build.rs` 用 `-Thisi-riscv-link.x`，WS63 示例集合已回到 `default-members` |
| 高 | 方向 | （曾）唯一示例（blinky）+ 手写忙等，无法证明其余驱动可用 | ✅ 大部已破：现有 UART/Timer/GPIO/DMA + async SPI/I2C 等 13 个额外示例 |
| 中 | 演示覆盖 | `blinky` 曾用 legacy `create_output_pin`，未直接演示 `OutputConfig`/`InputConfig` | ✅ 已修：`blinky` 走现代 `OutputConfig` 输出路径；`gpio_irq` 继续覆盖输入/中断 |
| 中 | 文档 | 旧构建指引曾指向自定义 JSON target | ✅ 已统一为 builtin `riscv32imfc-unknown-none-elf`（硬浮点 ilp32f、无原子；2026-05-31 曾过渡用 stable `riscv32imc`） |
| 低 | 依赖 | `blinky/Cargo.toml` 曾多声明 `ws63-pac` 直接依赖，源码未用 | ✅ 已随示例依赖整理清理 |
| — | 连接性 | 缺真实 Wi-Fi/BLE/SLE 链路示例 | 🔴 待 connectivity milestones C2-C5 上板 HIL |

## 改进项与排期

- **已完成的示例底座**：链接脚本传播已修、示例覆盖面已扩；`blinky` 已切到现代 `OutputConfig` 输出路径并完成真机点灯验证。
- **连接性示例** 🔴：按当前 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md) C2-C5 推进，在 blob 上板 HIL 后新增 Wi-Fi scan/connect/ping 真实链路示例，使示例集覆盖 SoC 核心能力。
- **async 示例** ✅：`async_delay` / `async_bus` / `embassy_multitask` / `embassy_async_io` 已落地（依赖 HAL 的 `async`/`embassy` 支持，见 [async-embassy.md](06-async-embassy.md)）。

## ws63-flashboot

来源：`docs/src/explanation/components/08-ws63-flashboot.md`。

## 评审发现

> 已对照 fbb_ws63 与 esp-hal、按 file:line 验证，0 条被驳回。

### 优点

- SHA256 软件实现正确，常量与填充无误，含已知向量单测（`src/sha256.rs:14-141`、`:148-175`）。
- `startup.S` 对照原厂 `riscv_init.S`，PMP/FPU/BSS/boot flag 处理到位（`asm/startup.S`）。
- 关键地址（SFC/UART/WDT/FAMA/efuse 寄存器、`FLASHBOOT_RAM` 语义）与镜像头 magic/版本对照 SDK 一致；整改后镜像头布局对齐 `secure_verify_boot.h`。
- 镜像头边界校验有较完整的拒绝/接受边界单测（`src/image.rs:52-135`）。
- 本轮已正确自我定级为实验性：banner、`publish=false`、移出默认构建、README 说明（`src/main.rs:1-22`、`Cargo.toml:11`、`README.md`）。

### 问题

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|--------|------|------|-----------------|------|
| 严重 | 安全 | 无真实性验签：只把算出的哈希与**同一份未签名头里的**哈希比对。能写 flash 的攻击者改镜像后重算 SHA256 写回头部即可以 M 态特权跳进任意代码，≠ secure boot（原厂用 efuse 根密钥 ECC-bp256/SM2 签名验签） | `src/main.rs`、`verify_image_integrity()`；对照 vendor `secure_verify_boot.c` | ✅ 已如实标注(2026-06-01)：函数改名 `verify_image_integrity`、文档明确"仅完整性、非真实性"；真实 ECC/SM2 验签属 ROADMAP 冻结项（复用原厂，不在本实验件投入） |
| 严重 | 正确性 | `ImageHeader`/`CodeInfo` 布局对不上真实 WS63 镜像：`image_length`(+0x114)/`image_hash`(+0x11C) 偏移读错 → 会拒绝真镜像 | `src/sfc.rs`；对照 vendor `secure_verify_boot.h:156-178` | ✅ 已修(2026-06-01)：`sfc.rs` `KeyArea`/`CodeInfo` 按 `image_key_area_t`/`image_code_info_t`(ECC256) 逐字段重排，`code_area_len`@+0x24、`code_area_hash`@+0x28，`const` 断言锁定 0x100/0x200/0x300；评审(layout) ok |
| 高 | 正确性 | A/B 误用 `0x4000_0024`：该寄存器是 flashboot **自身的备份恢复标志**，并非 app 槽选择器。代码却用它选 app 区 A/B | `src/main.rs`；对照 vendor `main.c:131-135`（`flashboot_need_recovery`） | ✅ 已修(2026-06-01)：删除该误用，改单镜像启动 + 如实注明真实 A/B = upg run-region(magic `0x70746C6C`)+分区表(`@0x200380`)、`0x40000024`=bootloader 自恢复 |
| 高 | 方向 | 重写原厂安全关键件（验签/启动链）属误导努力。生产应复用原厂 flashboot，本 crate 仅供学习 | `src/main.rs:5-8`、`README.md:22-26` | 暂不修(定级实验性；定位为学习件，整体方向走复用原厂) |
| 高 | 正确性 | 关键子流程是桩：`boot_clock_adapt()` 为 TODO 空操作；`read_partition_app_addr()` 恒返回 `FLASH_START`；`check_upgrade_mode()` 恒 false | `src/main.rs` | 🟡 部分(2026-06-01)：`read_partition_app_addr()` 改为**如实标注**的桩（注明不解析分区表、真实查表在 `@0x200380` magic `0x4b87a54b`）；`boot_clock_adapt`/`check_upgrade_mode` 仍为桩（实验定位，生产复用原厂） |
| 中 | 维护性 | 重复造轮子：UART/SFC/SHA256/startup 与 `hisi-riscv-hal`/`hisi-riscv-rt` 重复（因刻意不依赖 PAC/HAL） | `src/uart.rs`、`src/sfc.rs`、`src/sha256.rs`、`asm/startup.S`、`Cargo.toml:17-19` | 暂不修(为保持独立、规避双份 PAC 链接冲突的有意取舍) |
| 中 | 工程化 | 删除未用的 `ws63-pac` 依赖、`publish=false`、移出默认构建、banner 改为实验性警告 | `Cargo.toml:11,17-19`、根 `Cargo.toml` `default-members`、`src/main.rs:1-22` | 本轮已修 |

## 改进项与排期

- 生产层面的结论是**复用 fbb_ws63 原厂 flashboot**（已做签名验签 / A/B / 升级 / 解压 / flash 加密），Rust 应用以 app 镜像形式由原厂 flashboot 加载（`README.md:22-26`）。本 crate 维持实验/学习定位。
- **整改已落地（2026-06-01）**：镜像头布局对齐 `secure_verify_boot.h`（`code_area_len`/`code_area_hash` 偏移修正 + const 尺寸断言）、删除 `0x40000024` 的 A/B 误用改单镜像启动并如实注明真实 A/B 机制、`verify_sha256`→`verify_image_integrity` 如实标注"仅完整性非真实性"、`read_partition_app_addr` 桩如实标注。flashboot 现已纳入 CI clippy 门禁（不再 `--exclude`）。**真实 ECC/SM2 验签**仍按冻结项复用原厂、不在本实验件投入。
- 阶段 0 的构建完整性修复已落地：双份 PAC 消除（registry 版本依赖 + 根 `[patch.crates-io]` 指向本地）、无原子 ISA + `portable-atomic` critical-section polyfill（默认 target 为官方 rustc builtin 的 `riscv32imfc-unknown-none-elf`，当前用 `rust-src` + `-Zbuild-std=core,alloc` 构建；2026-05-31 曾过渡用 stable `riscv32imc`）、CI/release gating 与发布顺序修复、`hisi-riscv-rt` MIE 中断宏 typo 与栈顶符号 GC fallback 修复。
- 历史整改中的示例链接、中断模型、SPI/I2C 超时、system reset、GPIO pull、死代码清理和 async 底座均已收口；连接性仍按当前 [ROADMAP C1-C5](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md) 推进。旧阶段编号保存在 [归档 roadmap](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/docs/archive/roadmap-2026-05-2026-07-remediation.md)。

## ws63-RF

来源：`docs/src/explanation/components/09-ws63-rf.md`。

## 评审发现

### 优点
- **战略方向正确**：复用经过现场验证的闭源协议栈、只移植 ~70 个 OS/IPC 抽象函数，而非用 Rust 重写 Wi-Fi MAC / BLE host，是务实且正确的判断。
- **依赖面识别准确**：README 准确指出"硬件寄存器访问自包含于 blob，外部符号都是 OS 抽象 / IPC / 缓冲管理"，并准确量化为约 70 个 porting 函数。（注：README 同时把 HMAC/DMAC 描述为「ACORE/DCORE 双核」——此点不准确，WS63 单核，见「在依赖链中的位置」；但「依赖面 = OS/IPC/缓冲抽象」这一核心判断方向正确。）
- **接口文档化完整**：8 个 `port_*.h` 每个函数都有 doc 注释、返回语义与移植难度评级，`port_linker.h` 给出了内存布局与区段符号清单，为后续移植提供了清晰契约。
- **文档与代码一致**：README 的库目录表、依赖计数表与磁盘实际 `.a` 大小、头文件函数数逐项吻合，无夸大。

### 问题
| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|---|---|---|---|---|
| 严重 | 方向 | （曾）纯 blob + C 头，无 Rust/链接配置，连接性 0% | — | ✅ 已修：in-tree crate **`ws63-rf-rs`** 提供完整 Rust porting + `build.rs` + 链接搜索；blob 经它链入镜像（`wifi_blob_link`/`rf_port_demo` 在 ws63-qemu 验证） |
| 高 | 方向 | （曾）porting 层完全未实现：`osal`/`oal`/`log`/HCC 无一行实现 | `chips/ws63/rf/src/*` | ✅ 已实现：`osal_adapt_*`(33 符号) + `oal`/`log`/`uapi` + 协作调度器 + FRW 工作线程 + HCC 传输 + 软件计时器 + netif→smoltcp 桥；`frw_hcc_selftest`/`sched_selftest`/`netif_smoltcp_selftest` 自测通过 |
| 高 | 链接 | （曾）blob 数千未定义符号无一被满足 | `mac-link-residual.sh` | ✅ **Wi-Fi-init 符号闭合达成**：whole-archive 0 重复符号；`--gc-sections` rooted at `uapi_wifi_init` 残留仅 **2** 个（`__wifi_pkt_ram_begin__/end__` defsym）。早先"~3126/~96 missing"是 whole-archive 上界，被 off-path BT/alt-OS 代码主导（可达路径 0 BT 符号） |
| 高 | 工具链 | 链接 blob 需 ilp32f rv32imfc 目标 | `.cargo/config.toml` | ✅ 已就位：默认 target **就是** builtin 的 `riscv32imfc-unknown-none-elf`（硬浮点 ilp32f），原子由 portable-atomic critical-section 垫片提供（之前文档误写 imc） |
| 中 | 集成 | `port_linker.h` 的 `extern` 符号与 hisi-riscv-rt 链接脚本的衔接 | `hisi-riscv-rt`/`ws63-rf-rs` | 🟡 hisi-riscv-rt 提供 `__wifi_pkt_ram_*` 的 scaffold `--defsym`；真机前需把 netif pbuf 布局 pin 到 WiFi 构建的 `lwipopts.h`、TX sink 指向 blob 真实发送符号（见 ws63-rf-rs README）|

## 改进项与排期

本组件是 ws63-rs 通往"可用产品"的最大缺口。多数前置已完成，现状如下：

- **Baseline 已完成**：消除双 PAC；默认 target = builtin **`riscv32imfc`**（硬浮点 ilp32f，blob 所需）+ critical-section polyfill。工具链、HAL/RT、HIL、QEMU、image plan 前置已清。
- **C1 RF runtime image** 🟡：`chips/ws63/rf/build.rs` + 链接搜索把 `lib/*.a` 喂给链接器；**Wi-Fi-init 符号闭合达成**（残留 2）。下一步是补真实 `.wifi_pkt_ram` NOLOAD，并把可烧录镜像带到真机。
- **C2-C5 真机连接性** 🔴：`osal_adapt_*`(33) + `oal`/`log`/`uapi` + 协作调度器 + FRW 工作线程 + HCC 传输 + 软件计时器 + netif→smoltcp 桥均已在 ws63-qemu 自测；剩余是把 pbuf 布局/TX sink pin 到真实 blob，并在真实硅片上依次证明 Wi-Fi init、scan、connect、ping。
- **连接性专属 async** deferred：hisi-riscv-hal 的通用 `async`/`embassy`（见 [async-embassy.md](06-async-embassy.md)）已实现并验证；连接性专属的异步包装待 blob 上板后再做。

详见 [ROADMAP](https://github.com/hispark-rs/hisi-riscv-rs/blob/main/ROADMAP.md)。

## ws63-guide

来源：`docs/src/explanation/components/10-ws63-guide.md`。

## 评审发现

### 优点

- **独特的逆向 IP**：RF/外设/安全寄存器描述、存储器地址映射、中断编号表，对一颗 undocumented 的芯片极具价值，是 PAC/HAL 核对硬件语义的权威中文参照（如中断模型、内存图）。
- **与代码文档清晰分工**：硬件手册（本组件）与 Rust 架构文档（`docs/`）受众不同、内容零重叠，互补关系明确（见 `ws63-guide/ARCHITECTURE.md:5-7`）。
- **工程化完善**：`uv.lock` 锁定依赖、`-c source` 配置隔离、HTML/PDF/linkcheck 三类构建、构建后拷贝 Markdown 源供机器读取，自带 CI/CD。
- **覆盖完整**：9 章 + 附录覆盖系统/QSPI/Wi-Fi&BLE&SLE/安全/外设/JTAG，子目录拆分粒度合理。

### 问题

| 严重度 | 类别 | 问题 | 证据(file:line) | 状态 |
|--------|------|------|-----------------|------|
| 低 | 方向 | 与 Rust 代码架构文档（`docs/`）零重叠、受众不同；独立 Sphinx 构建链与 workspace 分离，维护面双倍 | `ws63-guide/source/conf.py:30`(`-c source`)、`ws63-guide/pyproject.toml`(独立工程)、`ws63-guide/ARCHITECTURE.md:5-7` | 暂不修（这是互补关系而非缺陷，刻意分离） |
| 低（方向） | 范围 | 手册应**冻结扩张、聚焦连接性**：当前价值已确立，继续扩章节会分散到连接性里程碑的精力 | `ROADMAP.md:138`(CI/文档/SVD 持续扩张冻结)、`ROADMAP.md:140`(保留 ws63-guide 独特逆向 IP 但停止扩张) | 已排期（ROADMAP "冻结/降优先级"：保留、停止扩张） |
| 低 | 文档一致性 | README 技术栈列 `sphinx-rtd-theme`，实际 `conf.py` 用 `sphinx_book_theme`，记述过时 | `ws63-guide/README.md:118` vs `ws63-guide/source/conf.py:55` | 暂不修（不影响构建，留作小修；非本轮整改范围） |

说明：本组件无被驳回项，评审要点已对照 fbb_ws63 / esp-hal 与 file:line 验证。手册内容本身（中断模型、内存图、寄存器位）经核实与真实硅片一致，恰好是 HAL 侧若干正确性问题（中断 PLIC 误建模等）的纠偏依据。

## 改进项与排期

本组件**无本轮（阶段 0）整改项**——阶段 0 的构建完整性修复（双 PAC 消除、默认 target ISA 改 `riscv32imc`、flashboot 实验化、CI/release 修复、hisi-riscv-rt MIE 宏 typo）均落在 Rust crate 侧，不涉及本手册。
