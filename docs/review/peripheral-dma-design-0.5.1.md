# hisi-riscv-hal 0.5.1 外设 DMA 接入设计（SPI / UART；I2C descope）

> 评审日期：2026-06-16 · 对象：把 0.5.0 的 owned-buffer mem-to-mem DMA `Transfer` guard 接进 SPI/I2C/UART（hisi-riscv-hal#6 / Area C 延后项）· 方法：10-agent 多视角工作流（Opus 4.8，7 并行调研 src/dma.rs+三驱动+vendor fbb_ws63+esp-hal+async 基建 → 1 综合 → 2 对抗性评审[安全/完整性]）。约定见 [类型化配置](../src/explanation/typed-config.md)；DMA 基础见 [components/overview](../src/explanation/components/overview.md)。**两个评审均判 `needs-changes`，其修正已并入本文（§4 列出）。** 规则：vendor C SDK + 硅片为真，非 QEMU。

## 0. 范围结论

| 外设 | 结论 | 依据（file:line） |
|---|---|---|
| **SPI0 / SPI1**（SSI v151）| ✅ 接入 | PAC 已有 `spi_dcr`(tdmae bit0/rdmae bit1)、`spi_dtdl`@0x20(TX 水位)/`spi_drdl`@0x1C(RX 水位)；data 寄存器绝对地址 `0x4402_0060`/`0x4402_1060`（PAC 偏移 **0x60**）。握手 `Spi0Tx=7/Rx=8`；**SPI1 需修**（见 §4.7） |
| **UART0/1/2**（DW16550 v151）| ✅ 接入 | DATA@0x04、FIFO_CTL@0x24 触发位；握手 `Uart{0,1,2}{Tx,Rx}=1..6`（dma.rs:479-489，已对硅片）。**真机验证只能用 UART1**（§4.8） |
| **I2C（WS63 v150）**| ❌ **硬件不支持，descope** | v150 **无任何 `IC_DMA_*` 寄存器**（WS63.svd/ws63-pac 为空）；vendor `I2C_CTRL_GET_DMA_DATA_ADDR`=`hal_i2c_v150_ctrl_check_default_false`（hal_i2c_v150_master.c:193）；I2C 握手 ID ≥29 属 SDMA 组，超 4-bit `src_per` 字段。强行做=凭空捏造，违反 vendor-truth 规则 |
| **I2C（BS2X v151）**| ⏸ 0.5.2+ 未来项 | bs2x-pac 有 `IC_DMA_CR`@0xA0/`IC_DMA_TDLR`@0xA4/`IC_DMA_RDLR`@0xA8，但 BS2X 握手表在 fbb_bs2x（本机无）；且 DW RX 是双通道(写 read-command 流 0x100/WIDTH_16 + 读 dat[7:0]) + CPU 补 STOP + `IC_TX_ABRT_SOURCE`/NACK 检查 |

**结论：0.5.1 = SPI + UART 的 DMA 接入。** I2C-DMA 在发版说明里写明"WS63 硬件不支持、BS2X 待 fbb_bs2x 握手表"。

## 1. 复用既有 0.5.0 基建（不重写，只扩展）

- `DmaDriver::configure_channel`（dma.rs:239-326）——唯一寄存器编程器，写 CONTROL+CONFIG 再 `dmac_en_chns` 起飞。**WS63 无独立握手-mux**，握手纯靠 CONFIG 的 `src_per[1:4]`/`dst_per[5:8]`（与 vendor `dma_cfg_data` union 逐位一致，hal_dmac_v151_regs_def.h:168）。
- `DmaChannelConfig::mem_to_peripheral(peri)`/`peripheral_to_mem(peri)`（dma.rs:526-543）——已置 `FlowControl`、握手 ID、并 pin 外设地址（inc=false）。**直接做外设传输的 config 载体。**
- `DmaPeripheral` + `request_id()`（dma.rs:472-511）——`HAL_DMA_HANDSHAKING_*` 真值表。
- `cache::clean_range`/`invalidate_range` + `DMA_ALIGN=32`（cache.rs:60-99）——单边 cache 维护。
- async：`IrqSignal`（asynch.rs:67）、`DMA_INT`(IRQ59) 钩子 + `wait_transfer_done`（dma.rs:1029-1083）、rt direct-mode 具名路由。

## 2. 架构

- **把 mem-to-mem `Transfer` 推广成单缓冲区 `PeripheralTransfer<'d, BUF>`**：拥有 driver + **一个**内存缓冲 + 固定外设地址 + `Direction` + `PeriDmaCtl{base,kind,dir}`（关外设 DMA-enable 的 POD，避免 guard 对驱动泛型化）。与 mem-to-mem 区别：①一缓冲一固定地址；②按帧宽走 beat（UART=Width8；SPI 按 DataBits 选 Width8/16），**丢掉硬编码 Word=u32**；③**单边 cache**（TX 只 `clean_range(src)`，RX 只 `invalidate_range(dst)`，**绝不碰外设 MMIO**）；④Drop 还要关外设 DMA-enable。
- **`with_dma(self, dma, ch…)` 消费 blocking 驱动 → `SpiDma`/`UartDma`**（esp-hal 模式）。blocking 与 DMA API **互斥**——头号 "if it compiles it runs" 安全属性；blocking `Spi`/`Uart` 的 embedded-hal/io impl 不动；DMA 包装类型另 impl `embedded-hal-async` 替换现有假 async（spi.rs:466 只是回调 blocking）。
- **厂商握手顺序载入相关**（spi.c:691-712）：configure_channel(启通道) → 写水位线(DMA-enable 仍关) → clean cache(TX) → **最后**置 `tdmae/rdmae`。构造器写成直线代码。
- **无原子**：新 static 全走 `portable-atomic`+`critical-section`；async 等在 `DMA_INT`(IRQ59)，用 **per-channel `[IrqSignal;4]`** demux（全双工 SPI 同时占 TX+RX 两通道，单一全局 signal 会误唤醒）。
- **通道所有权**：`configure_channel` 收裸 `u8`、无所有权保护（两驱动可程同一物理通道）。新增类型化 `DmaChannel` token + `split_channels()`（运行时 claim，critical-section）。

## 3. API 草图（全部 `#[cfg(feature = "chip-ws63")]`）

```rust
// src/dma.rs — 新增
pub enum DmaFrame { Byte, HalfWord }          // → Width8 / Width16
pub struct DmaChannel { logical: u8 }          // 0..=3，运行时 claim 的 token
pub struct DmaChannels { pub ch0: DmaChannel, /* … ch3 */ }
impl DmaDriver<'_, Dma0> { pub fn split_channels(&self) -> DmaChannels; }

pub struct PeriDmaCtl { base: usize, kind: PeriKind, dir: DmaDirection }  // Spi|Uart
pub struct PeripheralTransfer<'d, BUF> { /* driver, ch, buf, dir, mem_addr, bytes, peri_dis */ }
impl<'d, BUF> PeripheralTransfer<'d, BUF> {
    pub fn is_done(&self) -> bool;
    pub fn wait(self) -> Result<(DmaDriver<'d, Dma0>, BUF), DmaError>;   // ★可失败：区分 timeout/完成
    #[cfg(feature = "async")] pub async fn wait_async(self) -> (DmaDriver<'d, Dma0>, BUF);
}
impl<'d> DmaDriver<'d, Dma0> {
    pub fn start_mem_to_peripheral<S: ReadBuffer>(self, ch: DmaChannel, src: S,
        peri_data_addr: usize, peri: DmaPeripheral, frame: DmaFrame, dis: PeriDmaCtl) -> PeripheralTransfer<'d, S>;
    pub fn start_peripheral_to_mem<D: WriteBuffer>(self, ch: DmaChannel, dst: D,
        peri_data_addr: usize, peri: DmaPeripheral, frame: DmaFrame, dis: PeriDmaCtl) -> PeripheralTransfer<'d, D>;
}

// src/spi.rs
impl<'d, T> Spi<'d, T> { pub fn with_dma(self, dma, tx_ch, rx_ch) -> SpiDma<'d, T>; }
impl<'d, T> SpiDma<'d, T> {
    pub fn write_dma<B: ReadBuffer<Word=u8>>(&mut self, buf: B) -> Result<SpiTxXfer<'_,B>, SpiError>;
    pub fn read_dma<B: WriteBuffer<Word=u8>>(&mut self, buf: B) -> Result<SpiRxXfer<'_,B>, SpiError>;  // §4.10 待硅片定
    pub fn transfer_dma<RB, TB>(&mut self, read: RB, write: TB) -> Result<SpiXfer<'_,RB,TB>, SpiError>;
    pub fn release(self) -> (Spi<'d,T>, DmaDriver<'d,Dma0>);
}
// src/uart.rs — UartDma::{write_dma, read_dma(定长), release}；data() @base+0x04
```

## 4. 评审强制修正（这些是方案关键，已并入上文）

1. **`wait()` 必须可失败（安全评审：最严重 UAF）。** 现 `Transfer::wait` 超时后**不停通道就把缓冲 `ptr::read` 交回**（dma.rs:642-666）→ DMA 还活着、缓冲已可释放。外设路径**结构性易超时**（SPI1 握手错 / SPI 纯 RX 无时钟 / UART RX 短帧）。→ `wait()` 返回 `Result`，超时分支必须先 quiesce 通道**再**交回。
2. **abort/Drop 缺 `active` 位 settle（高）。** 现 Drop halt 后**立刻** disable（dma.rs:672），无轮询 `cfg.active(bit15)==0`，半拍总线写可能落在已释放缓冲。→ **cancel-then-quiesce**：关外设 DMA-enable → set halt(bit16) → 有界轮询 active==0 → 清 ch_enable(bit0)。**回灌到 0.5.0 mem-to-mem guard（同样潜伏窗口）。**
3. **Drop 顺序反（中）。** 应**先**关外设 DMA-enable（断请求源）再停通道，否则请求线留驻（esp-hal cancel-then-quiesce, spi/master/dma.rs:1608）。
4. **async ISR 必须清 `dmac_int_clr`（中）。** 现 `on_interrupt` 只清 ECLIC pending（dma.rs:1042），不写 `dmac_int_clr`（PAC lib.rs:15016）→ level-triggered storm。且外设 CONFIG 必须置 `transfer_int`(int_en/int_tc，dma.rs:307) 否则 `dmac_int_st` 不反映完成。统一用 `interrupt_status`(int_st) 还是 `raw_interrupt_status`(ori_int_st) 要在 demux 与 arm-check 间对齐。
5. **"水位线 dtdl/drdl 命名反转"是伪命题。** vendor regs_def(hal_spi_v151_regs_def.h:828) + PAC + setter 三方一致 dtdl=TX/drdl=RX，设计接线本就对。→ 从硬门**降级为一行 sanity**。
6. **`peri_data_addr` 文档错误。** SPI data 是 PAC 偏移 **0x60**（绝对 0x4402_0060/0x4402_1060），**不是 +0x2C**（那是 vendor 数组偏移）。→ 一律用 `spi_dr()`/`data()` 访问器取址，不手算。
7. **SPI1 握手必须修（高）。** HAL 现 `Spi1Tx=13/Rx=14`，但 PAC 的 Spi1@0x4402_1000 对应 vendor **QSPI0_2CS=9/10**（三方印证 platform_core.h:64 / spi_porting.h:38-39 / PAC lib.rs:19977）。→ 加 `Qspi02csTx=9/Rx=10`，改 dma.rs:698-699 测试。错的 13/14 选错请求线 → 通道挂死但 wait 静默"完成"。
8. **UART 真机驱动必须是 UART1 不是 UART0。** UART0 是 semihosting 控制台、未跳线；唯一跳线回环 = **UART1 TXD(GPIO15)→RXD(GPIO16)**（tests/hil.rs:931），握手 `Uart1Tx=3/Rx=4`。**且 UART1 RX FIFO 当前坏**（[[ws63-uart1-rx-fifo-stuck]]，#5）→ P3 前置修 #5，或非退化多字节 pattern + 反码预填 + **独立 liveness 断言**（通道 enable 清零 / DMA_INT 触发 / rx_fifo 排空），防 stuck-FIFO 假阳性。
9. **加"with_dma 后再调 blocking 编译失败"trybuild 测试**——头号属性，原方案漏。
10. **SPI `read_dma`（纯 RX）存疑：** 纯 RX 是否需 dummy TX 喂时钟产生 SCK？→ 要么加 `HIL-SPI-DMA-RX-ONLY` 硬门，要么 0.5.1 只发 `write_dma`+`transfer_dma`，`read_dma` 推 0.5.2。
11. **UART `dma_mode`(uart_parameter bit11) RO/WO 歧义：** vendor 写它但 PAC 标 RO（regs_def.h:456）。DW UART 很可能纯 FIFO-触发驱动 DMA 请求、该写是 no-op。→ 先证"只用 FIFO 触发就能回环"，别盲写。
12. **`release()` 必须验证**：清 tdmae/rdmae、释放通道、恢复 FIFO_CTL 触发位，blocking 再用要正常。
13. **`split_channels()` 二次调用 / token drop** 要有运行时 claim 测试。
14. **BUILD 矩阵**覆盖 `{chip-ws63 / +async / +async+hil-loopback / chip-bs21 / host-test}`，不只 chip-bs21。

## 5. 分阶段实施（每阶段独立可验，P4 已拆分）

| 阶段 | 内容 | 交付物 | 依赖 |
|---|---|---|---|
| **P0** core 推广 | `start_mem_to_peripheral`/`start_peripheral_to_mem` + `PeripheralTransfer`(★可失败 wait + cancel-then-quiesce Drop) + `DmaFrame`/`PeriDmaCtl` + 类型化 `DmaChannel`(运行时 claim) + **SPI1 握手修** + 4095-beat 分块 + **回灌 mem-to-mem Drop 的 active-settle** | core 编译(双芯片)+host 测过；无驱动改动 | 无 |
| **P1** SPI0 TX（证明） | `Spi::with_dma`+`SpiDma::write_dma`，vendor 顺序写 dtdl=4/tdmae、clean cache、`wait_idle`(spi.rs:281) 收尾、teardown 清 tdmae | **单驱动单方向真机过**，架构端到端验 | P0 |
| **P2** SPI0 RX+全双工 | `read_dma`/`transfer_dma`，wait 时 invalidate dst，查 `spi_wsr.rxff`→`Overflow`；`SpiBus` blocking impl | 全 SPI0 blocking DMA，embedded-hal 兼容 | P1 |
| **P3** UART1 TX+RX | `Uart::with_dma`+`write_dma`/`read_dma`(定长)，FIFO_CTL 触发位作**最后**一次写；解 `dma_mode` RO/WO；**前置修 #5 或非退化测试** | UART blocking DMA 真机过 + dma_mode 有答案 | P2 |
| **P4a** async demux | `DMA_SIGNAL`→`[IrqSignal;4]`，ISR 内清 `dmac_int_clr`，单通道 async(复用 P1) | per-channel demux 正确 | P3 |
| **P4b** async 全双工/UART | 双通道 await + 替换 spi.rs:466 假 async + UART async | async `.await` DMA 全覆盖 | P4a |
| **P5** 文档+发版 | 改 CLAUDE.md DMA 段(还在讲已删的 DmaEligible/DmaChannelFor，dma.rs:546)+handbook(**docs-first**)；I2C-DMA 限制写进发版说明；**semver 0.5.1**(additive) | 0.5.1 发布，文档对齐代码 | P4b |

## 6. 验收标准

**HOST（`cfg(test, not riscv32)`）**：`HOST-CFGWORD-TX/RX`(hard，flow_ctl/per/inc 位) · `HOST-WIDTH-FROM-DATABITS`(hard) · `HOST-SPI1-HANDSHAKE`(hard，9/10 非 13/14) · `HOST-HANDSHAKE/ADDR-PER-INSTANCE`(hard，逐实例) · `HOST-CHUNK-4095`(hard，不静默截断) · `HOST-DROP-CLEARS-PERI`+`HOST-DROP-ORDER`+`HOST-ABORT-ACTIVE-SETTLE`(hard，mock MMIO：先关外设→halt→轮询 active→清 enable) · `HOST-PERI-CONFIG-INT-ENABLED`(hard) · `WAIT-TIMEOUT-QUIESCED`(hard) · `HOST-DOUBLE-SPLIT`(soft)。

**COMPILE-FAIL（trybuild）**：`COMPILE-FAIL-UAF`(hard，活缓冲不可动/释) · `COMPILE-FAIL-BLOCKING-AFTER-DMA`(hard，with_dma 后 blocking=use-after-move，头号属性) · `SDMA0-REJECTED`(soft，API 限定 Dma0)。

**HIL（`hil-loopback`）**：`HIL-SPI-DMA-TX`(hard，MOSI→MISO >64B) · `HIL-SPI-DMA-FULLDUPLEX`(hard) · `HIL-SPI-DMA-RX-ONLY`(hard **或 descope read_dma**) · `HIL-UART-DMA-LIVENESS`(hard，UART1 非退化+反码预填+独立 liveness，防 #5 假阳性) · `HIL-DMA-INT-FIRES-PERIPHERAL`(hard，证 IRQ59 对外设单块完成触发) · `HIL-CACHE-RX`(hard，目的预脏) · `HIL-DMA-CHUNK-OVER-4095`(hard，~5000B) · `HIL-DMA-RELEASE-ROUNDTRIP`(hard) · `HIL-ASYNC-LOOPBACK`(hard，看门狗兜底) · `BUILD-MATRIX`(hard)。

## 7. 需在硅片上先答的开放问题（gating）

1. **(P3)** UART `dma_mode` 真可写还是 RO/no-op？（先证"只用 FIFO 触发就能回环"）
2. **(P2)** SPI 纯 RX 是否需 dummy TX 喂时钟？（决定 `read_dma` 是否独立成立）
3. **(P4a)** IRQ59 对外设单块完成是否像 mem-to-mem 一样触发？（不触发则 async 退化为轮询）
4. ~~水位线命名反转~~（已查实为伪命题，降级 sanity）。

## 8. 关键引用（file:line）

- HAL：`src/dma.rs`{239 configure_channel, 472-543 DmaPeripheral/mem_to_peripheral, 590-676 start_mem_to_mem+Transfer, 698-699 SPI1 测试待改, 1029-1083 async}、`src/cache.rs:60-99`、`src/spi.rs`{218 configure, 249/266 transfer/write, 281 wait_idle, 466 假 async}、`src/uart.rs`{218/221 FIFO_CTL, 233-242 read_byte/rx_fifo_cnt, 431-442 UARTn_INT}
- PAC：SPI `spi_dcr`(lib.rs:19258)/`spi_dtdl`@0x20/`spi_drdl`@0x1C、data@0x60；UART data@0x04/fifo_ctl@0x24/uart_parameter@0x60(dma_mode RO)；`dmac_int_clr`(lib.rs:15016)
- vendor fbb_ws63：`spi.c:691-712`(握手顺序)、`hal_spi_v151.c:634`(水位线值 TX=4/RX=0)、`spi_porting.h:38-39`(SPI1=QSPI0_2CS 9/10)、`hal_uart_v151.c:127/133/781`(触发默认/data 地址)、`hal_i2c_v150_master.c:193`(I2C 无 DMA)、`dma_porting.h:57`(握手表)
- esp-hal：`spi/master/dma.rs`{77 with_dma 消费, 1540 wait→(driver,buf), 1608 cancel-then-quiesce}
