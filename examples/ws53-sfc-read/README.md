# WS53 SFC CPU 只读 RAM 示例

这是 `chip-ws53,unstable` 下的实验接口，支持 JEDEC `9f`、具名状态
`05/35/15` 和普通 `03` 读取。当前只有主机及合成 QEMU 验证，**未通过实板
SFC HAL 验收**；不提供 Flash 编程、擦除、状态写入或任意 opcode。

此目录是独立 Cargo workspace，拥有自己的 `Cargo.lock`，依赖均使用仓库内
相对路径。普通 Cargo 构建使用已生成的 PAC，不读取厂商 SDK，也不运行生成器。

## 构建

在此目录执行，仅构建，不会连接板卡或烧录：

```sh
cargo build --locked --release -Zbuild-std=core,alloc --lib \
  --target riscv32imfc-unknown-none-elf
cargo build --locked --release -Zbuild-std=core,alloc \
  --features model-handoff --bin sfc-read-model \
  --target riscv32imfc-unknown-none-elf
```

使用主仓库固定的 `nightly-2026-07-09`。本地已有依赖缓存时可加 `--offline`。
链接入口仍为 `hisi-riscv-link.x`，使用 WS53 RAM 布局，不生成 Flash 镜像头。
不要直接使用主仓库的通用 `cargo run` runner 把模型程序当作实板程序。

## 如何集成

库入口 `unsafe observe(...)` 位于 `src/lib.rs`，接收应用已预留的
`Unclaimed<SfcCfg>` 和 `Unclaimed<Tcxo>`、经过验证的 `ReadRegion`、
`FlashAddress` 与非零 tick deadline，返回三字节 JEDEC、三个原始状态字节
和 64 字节数据。它不偷偷执行复位、改时钟或接管其他核心。

HAL 的 `ReadOnly::from_configured` 先只读检查配置：命令和 DMA 的 START 清零、
SPI mode0、三字节地址、时钟 enable 置位及 source 编码为 0 或 6。
这两个 source 值只是 SDK 使用的配置编码，不是频率或时钟 ready ACK。
配置不符合即拒绝；驱动不会尝试自动修复。

进入 unsafe 附着前，调用者必须建立并持续维持：

- 应用核与控制核之间的资源交接，其他 CPU、DMA、IRQ/NMI 和调试器访问已停止，
  之前的请求及预取已排空。`START=0` 或 `DMA_START=0` 只表示所观察位为空闲，
  不能作为总线排空证明。
- 完整执行闭包位于独立可用 RAM，包括调用者、驱动、时间源、编译器辅助函数、
  字面量、中断/异常处理、栈和缓冲区。所有核心/主设备均不得并发访问 Flash/XIP；
  单独关闭 cache 不等于维护完成或请求排空。
- CS1 单线引脚和电气连接、持续有效的共享时钟、已释放的复位与路由；期间
  不发生休眠或其他服务修改这些配置。
- 实际 NOR 支持 `9f/05/35/15/03`、三字节地址以及 `05 bit0=WIP`；
  允许读取的区域位于已经验证的真实容量内。没有默认 4 MiB 容量或页/扇区大小。
- TCXO 已启用，且以已知固定速率独立于 SFC 前进；其 snapshot 接口由本例独占。
  或直接使用 HAL 并提供满足有界调用契约的其他 `TickSource`。

所有非 `05` 操作先取得新发起且已完成的 `05`，等待 WIP 清零，再执行目标命令；
完整过程共用一次 deadline 和有限 START 轮询预算。不会把旧 DATABUF 当作状态，
也不会将控制器完成等同于 NOR 就绪。状态 2/3 按原始字节返回，不解释保护位。

`FlashAddress` 接受 24 位地址，`ReadLength` 接受 1..64，`ReadRegion` 检查完整范围。
读取数据先存内部 64 字节缓冲，全部后验检查通过后才写调用者缓冲。
事务错误会永久 poison；后续调用返回 `Error::Poisoned` 且不再访问寄存器或时间源。
Drop 不清 START、不复位，没有 safe recover/free。超时不等于操作已经取消；
再次附着必须重新建立外部交接条件。

## 显式模型程序

`model-handoff` feature 才会构建 `src/model.rs`；默认只构建集成库。
这个程序假设外部合成模型已提供独占 SFC 配置，它自主启用 TCXO，只加载 RAM，
不依赖主机预写 SFC/时钟寄存器或 UART 握手。它不建立真实板卡的上述交接证明。

模型需要显式启用 `ws53-sfc-command.assume-profile`、`configured-handoff`，
并提供 `snapshot-global=0`、`snapshot-dma=0`、`snapshot-clock=0xd`、
`snapshot-status3=0xa7`；source0 对照使用 clock=1。这些值是测试种子，
不是实物复位值、实际 NOR 身份或电源/总线资格。

`WS53_SFC_READ_MODEL` 是 96 字节的 RAM 诊断对象：phase、error、JEDEC[4]、
status[4]、data[64]、first_error、followup_error、followup_read_error、
followup_buffer_intact。正常 phase=2；错误 phase=3。发生事务错误后程序真实
再次调用 Status2 与 READ，两者必须返回 Poisoned(1)，READ 哨兵保持。
不能用任意 panic 或非零 error 代替预期故障检查。

## 来源与验证边界

- SDK：`163d9150ab94a02d09a05f623351f9facee3025e`。
- RDL：`hisi-registers@92a3d202e70e305426c18af07158731e7e95e921`。
- PAC：`ws53-pac@00f97a337790aa298112dddf8f7fc9344eb85143`。
- `hal_sfc_v150_regs_op.c:21–44`：CMD_INS 读改写保留高位，命令长度减一，
  CS1 和完整标准 SPI 配置。HAL 使用 PAC 具名字段，将配置与 START 合并成一次
  完整写入；这不是任意并发环境下的时序等价证明。
- 同文件 `:47–66`：START 与 DMA_START 的重复轮询，属于源码级访问语义。
- `hal_sfc_v150.c:48–71,248–275`：新 05/WIP0 检查、完成后读取
  `ceil(n/4)` 个 word 并仅复制请求字节；本实现另外提供独立 deadline、poison
  和失败时用户缓冲不变的约束。
- `hal_sfc_v150_regs_op.h:156–158`：DATABUF 普通 word 访问。
- SFC 时钟为 `M_CRG+0x570`，与 QSPI 的 `0x574` 不同。本驱动只读观察，
  不猜测未知高位、复位值、频率或所有权。

开发副本已通过 15 项新增主机测试（完整库 312 项）、6 项 API 编译拒绝检查、
单 PAC 的 async/Embassy 外部消费者和旧芯片编译回归；冻结开发 ELF
`8064c566…fa5ec7` 在模型中完成 17 轮（5 正常含复位恢复、12 预期拒绝/故障）。
另有独立一轮 GDB 控制的模型跟踪，证明两个 poison 后继之间零 MMIO。
主机变化字节模式和所有长度测试补充检查 packing；模型正常数据全 FF 本身
不能证明字节排列。这些是软件证据，不是 Flash/冷启动或电气验收。

本目录移植后的 ELF `a0122a28…beaa7` 已另外通过 8 场景、17 轮模型测试：
5 正常、10 配置拒绝、2 deadline 后真实 poison 后继检查，包含解除故障后新实例
恢复。它不继承 `8064` 的动态零 MMIO 跟踪，也不能替代候选 08/09 的烧录资格。
重新构建仍需按实际 ELF 哈希绑定结果。
当前 RDL/PAC 提交只在本地集成，尚未推送、发布或更新父仓库提交指针。
旧分支仍保留，回切必须同时考虑 HAL 对新 PAC 时钟 accessor 的依赖。

软件证据分别保存在[时钟来源与 PAC 归档](../../docs/plan/evidence/ws53-sfc-clock-source-pac-2026-10-25/README.md)、
[configured 模型归档](../../docs/plan/evidence/ws53-sfc-configured-model-2026-10-25/README.md)及
[只读 HAL 开发验证归档](../../docs/plan/evidence/ws53-sfc-read-hal-model-2026-10-25/README.md)、
[本目录集成模型归档](../../docs/plan/evidence/ws53-sfc-integrated-hal-model-2026-10-25/README.md)。
归档中的原执行目录和 ELF 哈希属于当时受测输入，不会因本目录重建而自动更新。
