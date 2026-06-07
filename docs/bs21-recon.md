# BS21 / BS2X 侦察事实表（fbb_bs2x 实证，WS63→BS21 泛化的真值源）

来源:`/root/fbb_bs2x`(HiSpark `fbb_bs2x`,部分稀疏检出)。本文是后续所有阶段的 ground truth,角色等同 `fbb_ws63` 之于 WS63。

> **里程碑 M1 — 已达成（2026-06-07）。** BS21 的 `blinky` + `uart_hello`(`bs21-examples/`,
> `--features chip-bs21` 的 HAL + BS21 `memory.x`)在 `qemu-system-riscv32 -M bs21` 上**端到端启动**:
> uart_hello 打印横幅(UART0 @ 0x52081000),blinky 翻转 GPIO0(@ 0x57010000,0 条非法指令陷阱)。
> `ws63-qemu/scripts/bs21-smoke-test.sh` 全绿;WS63 不回归(`-M ws63` + 5/5 qtest 仍绿)。
> 落地:`bs21-pac`、`hisi-riscv-hal`(`chip-ws63` 默认 / `chip-bs21`)、`hisi-riscv-rt`(芯片门控)、
> `bs21-examples/`(独立 workspace)、`ws63-qemu` 的 `hw/riscv/{hisi_riscv31.h,bs21.c}` + `-M bs21`。
> **推后(随连接性):** linx131 自定义 ISA 解码、ROM 拦截、全外设对齐、共享模型拆到
> `hisi_riscv31.c`(`CONFIG_HISI_RISCV31`)、CPU 改名 `hisi-riscv31`、BLE/SLE 厂商 blob。

## 芯片身份

- `fbb_bs2x` = **星闪(SparkLink/NearLink)BS20 / BS21E / BS22** 方案(LiteOS)。`bs2x` 是公共驱动目录,`bs20/bs21e/bs22` 是 SKU。用户的「BS21」≈ **BS21E**。
- 规格(README):CPU **64 MHz**、FLASH **1M**、RAM **128K(BS20)/160K(BS21E·BS22)**、SLE 1K/2K/4K、**USB 支持**、**无 Wi-Fi**(仅 BT/BLE 5.4 + SLE/GLE)。
- 主控:HiSilicon RISC-V32 + 硬浮点(编译器 `cc_riscv32_musl_fp` → `riscv32-linux-musl-gcc`),自定义核 **"linx131"**(LiteOS `arch/riscv/src/linx131`)——
  WS63 的 `xlinx`/`riscv31` 的同族但**不同变体**。有一个 **BT 从核**(`SLAVE_CPU_BT`);我们目标是**应用主核**(同 WS63 单应用核思路)。

## 与 WS63 的关键差异(几乎全不同)

| 维度 | WS63 | BS21(BS2X) |
|---|---|---|
| 主频 | 240 MHz | **64 MHz** |
| RAM | 576K @0xA00000 | **160K @0x100000** |
| Flash XIP | 8M @0x200000 | **@0x10000000**(QSPI),SFC @0x90000000 |
| ROM | 0x109000–0x14C000 | **0x0 起**(MPU ROM 窗 0x0/32K,但符号到 ~0x40000+) |
| ITCM | 0x14C000 | **0x80000 / 512K** |
| 外设空间 | 0x44000000 | **0x52000000(M_CTL)+ 0x57000000(GLB/PMU/GPIO/RTC)** |
| 自定义 ISA | xlinx | **linx131**(编码是否与 xlinx 兼容**未知**,需 xcc 编译器或 BS21 ELF 反汇编确认) |
| 连接性 | Wi-Fi6+BLE+SLE | **BLE5.4 + SLE/星闪,无 Wi-Fi** |

## 内存图(platform_core.h 的 MPU 窗)

- ROM:`0x0` len `0x8000`(32K MPU 窗;ROM 符号实际延伸到 ~0x40000)
- ITCM:`0x80000` len `0x80000`(512K)
- L2RAM:`0x100000`(实际 RAM ~160K;MPU 窗 `0x100000`/1M + `0x200000`/1M;`L2RAM_MEMORY` 0x100000–0x35FFFF)
- XIP PSRAM:RO `0x08000000`,RW `0x0C000000`
- XIP NorFlash:`0x10000000` len `0x10000000`;QSPI XIP 0x08000000–0x1FFFFFFF
- ShareMem:`0x87000000`;SFC 寄存器 `0x90000000`(flash1 @0x90100000)
- MPU 寄存器区:`0x50000000`/256M、`0xA3000000`/16M(XIP cache @0xA3006000)

## 外设基址(platform_core.h)——全部不同于 WS63

- GLB_CTL_M/A/D:`0x57000000`/`57000400`/`57000800`;PMU1 `0x57004000`、PMU2_CMU `0x57008000`;ULP_AON `0x5702C000`;FUSE `0x57028000`
- **UART**:UART0(L0)`0x52081000`、UART1(H0)`0x52080000`、UART2(L1)`0x52082000`(默认 3 路;ROMBOOT 时 1 路)
- **GPIO**:GPIO0-4 `0x57010000`/`14000`/`18000`/`1C000`/`20000`(**5 bank**),ULP_GPIO `0x57030000`;引脚 `S_MGPIO0..31`(32 脚);`GPIO_MAX_NUMBER=3`(默认用 3)
- **TIMER**:`0x52002000`,TIMER0-3 = base+0x100..0x400(**4 个**,dw_apb 风格 +0x100/通道,同 WS63 的 IP 家族);TICK=TIMER3;SYSTICK `0x5702C330`
- **I2C**:`0x52083000`/`52084000`(最多 2);**SPI**:`0x52087000`/`88000`/`89000`(最多 3)
- **PWM**:`0x52090000`(12 通道 PWM_0..11);INTR 寄存器 `0x52000900/904/908`
- **DMA**:M_DMA `0x52070000`、SDMA `0x520A0000`(SMDMA 4 通道 + MDMA 8 通道)
- **I2S/SIO**:`0x5203003C`;**RTC**:`0x57024000`(4 个);**TCXO_COUNT**:`0x57000200`;**WDT**:`0x52003000`/CHIP_WDT `0x57034000`
- **SEC/TRNG**:`0x52009000`;**QSPI0/1**:`0xA3000000`/`A3002000`;**USB/NFC/PDM/QDEC/KEYSCAN**:新增(WS63 无)
- 复位偏移 `CHIP_RESET_OFF=0x600`;NMI_CTL `0x52000700`

## 中断(chip_core_irq.h)——`LOCAL_INTERRUPT0=26`(同 WS63 基),但**映射全不同**

BS21 的 core_irq 从 26 起(共 ~64 个本地中断,26..~89,`BUTT_IRQN` 收尾):
- 26 BT_INT0,27 BT_INT1,28 GADC_DONE,29 GADC_ALARM,32 MCU_PCLR_LOCK,**33 ULP_GPIO,34 GPIO_0,35 GPIO_1**,36/37 BT_TOGGLE,38 KEYSCAN_LP,
  **39 UART_0,41 UART_1,42 UART_2**,43 QSPI0_2CS,44 PDM,46 KEYSCAN,47/48 M_WAKEUP/SLEEP,49-52 RTC_0-3,**53-56 TIMER_0-3**,57 M_SDMA,
  59-61 SPI_M_S_0/1·SPI_M,**62 I2C_0,63 I2C_1**,64-66 BT_BB(BT/BLE/GLE),67 I2S,68 RF_PRT,69 NFC,70 SEC,**71 PWM_0,72 PWM_1**,
  78 PMU_CMU_ERR,79 ULP_INT,85 PMU2_CLK_32K,86 ULP_WKUP,**87 TSENSOR,88 QDEC,89 USB**。
- `BCPU_INT0_ID=26`、`LOCAL_INTERRUPT0=26` 与 WS63 一致 → **同一套 LOCI 本地中断核架构**(mie 类 26-31 + LOCI ≥32 的切分**待 BS21 arch 代码确认**,但同族极可能相同)。
- **注意**:BS21 的 26-31 是 BT/ADC(非 WS63 的 TIMER);TIMER 落在 53-56(LOCI 区)。

## ROM 符号(acore.sym,172KB)——ROM 拦截可用

ROM 驻留 secure-libc/printf:`memset_s=0x3d1dc`、`memcpy_s=0x3da4e`、`sprintf_s=0x3e8e8`、`snprintf_s=0x3e930`、`vsnprintf_s=0x3e962`、`bt_util_sprintf=0x2c3fa`…
→ ROM-call 拦截框架适用,BS21 ROM 表从此表提取(地址区间约 `[0x0, 0x80000)`)。

## 8 个门禁问题 — 现状

1. **ISA**:riscv32 + 硬浮点已确认;自定义核 **linx131**。**exact `-march` 未取**(在未检出的 `compiler_xcc_cpu.cmake`);**linx131 压缩编码是否==xlinx 未知**(需 xcc 或 BS21 ELF)。
   → **M1 影响为零**:我们的 Rust blinky/uart_hello **只发标准 RV32IMFC 指令**,现有 `ws63` 工具链可用;linx131 解码器**只在跑厂商 C 固件时才需要**(随连接性推后)。
2. **LOCI CSR**:✅ **确认与 WS63 完全相同**(`vectors.h`:同一 HiSilicon「HimiDeer」核)——`RISCV_SYS_VECTOR_CNT=26`、**mie 类 6 个(IRQ 26-31,「enabled by CSR mie 26-31 bit」)**、
   **custom 类 60 个(IRQ≥32,「enabled by custom CSR locie0~2」)**、`LOCIEN_IRQ_NUM=32`、`LOCIPRI_IRQ_NUM=8`、`LOCIPRI_IRQ_BITS=4`、`LOCIPRI_DEFAULT_VAL=0x11111111`、
   异常上下文含 `ccause`(WS63 的 0xFC2 自定义 CSR)——与 `hisi-riscv-hal/interrupt.rs` 常量逐一吻合。**→ HAL `interrupt.rs` + QEMU LOCI intc + target/riscv LOCI 投递补丁完全复用**(CSR 原始地址 0xBC0/0xBE0/0xBF0/0xBFE 极高概率相同,待 linx131 `trap.S`/`arch_encoding.h` 末确认)。
3. **内存图**:✅ 已取(见上)。
4. **ROM 符号**:✅ acore.sym 已取。
5. **UART/timer/GPIO IP 是否寄存器级相同**:✅ **确认相同**——BS21 用 **UART v151 / timer v150 / GPIO v150**,与 WS63 是**同版本 IP 块**(`hal_uart_v151_regs_def.h`:`intr_id`@0x00、`fifo_status` = tx_full[0]/tx_empty[1]/rx_full[2]/rx_empty[3],与 WS63 QEMU 模型逐位吻合)。
   **→ QEMU 设备模型(ws63-uart/timer/gpio)与 HAL 驱动逻辑(uart.rs/gpio.rs)原样复用,仅基址 + IRQ 号不同。** 这是最好结果:per-chip 面只剩「内存图 + 基址 + IRQ + 实例数 + 时钟常量」。
6. **GPIO/UART/IO_CONFIG 基址 + IRQ 号**:✅ 已取(见上)。
7. **启动路径**:`-kernel` 直载 + 标准 RV32IMFC 的 Rust 固件,M1 多半只需 UART+GPIO+吸收器;ROM/TCXO 桩按需。**待确认 BS21 startup 是否读 TCXO/sysctl**。
8. **PAC 来源**:无 SVD(同 WS63),从 platform_core.h + 各 `*_regs.h` 手工派生 `bs21-pac`。

## M1 路径(关键结论)

BS21 的 **Rust blinky + uart_hello 只用标准 RV32IMFC** → **不需要 linx131 解码器,也不需要 ROM 拦截**(那些是厂商 C 固件的事,随连接性推后)。
故 M1 = 现有 `ws63` 工具链 + 一个 `-M bs21` QEMU 机器(标准 riscv + LOCI 中断模型 + BS21 内存图/UART/GPIO)+ `chip-bs21` 的 HAL(GPIO/UART)+ BS21 `memory.x`。
**剩余 M1 门禁**:问题 5(UART/GPIO 寄存器是否同 WS63 IP)+ 问题 2(LOCI CSR/切分)——下一步读 BS21 porting/arch 确认。
