# 外设清单与覆盖情况

`crates/hisi-riscv-hal/src/*.rs` 下的全部驱动模块。基地址取自各模块的 doc 注释/常量或 `safety.rs` 断言；标 "—" 者源文件头未列。"芯片"列：默认构建为 `chip-ws63`，标注 BS21 的模块仅在 `chip-bs21` 下编译。

完整 API 见 [HAL API 总览](hal-api.md)；驱动如何新增见 [新增一个外设驱动](../how-to/add-driver.md)。

## 驱动模块表

| 模块 | 外设 / IP | 芯片 | 基地址 | 主驱动类型 | 示例覆盖 | 裸板自检 |
|------|-----------|:----:|--------|-----------|----------|:--------:|
| `gpio` | GPIO（19 引脚，3 块） | 两者 | `0x4402_8000`/`9000`/`A000` | `Input`/`Output`/`Flex`/`AnyPin`/`Io` | `blinky`、`gpio_irq` | ✅ |
| `ulp_gpio` | ULP GPIO（8 引脚 GPIO107-114） | 两者 | `0x5703_0000` | `UlpGpioPin<MODE>` | 无 | ✅ |
| `uart` | UART0/1/2（16C550） | 两者 | `0x4401_0000`/`1000`/`2000` | `Uart<T>` | `uart_hello` | ✅ |
| `i2c` | I2C 主机 v150（I2C0/1） | WS63 | — (SCL 24 MHz) | `I2c<T>` | `i2c_scan`、`async_bus` | ⚠️ 需从机 |
| `i2c_v151` | I2C DesignWare SSI v151 | BS21 | `0x5208_3000 + idx*0x1000` | `I2c<T>`（`Speed`） | 无 | ⚠️ 需从机 |
| `spi` | SPI 主机 DesignWare SSI v151（SPI0/1） | 两者 | — (SSI_CLK 160 MHz) | `Spi<T>`（`Config`） | `spi_loopback`、`async_bus` | ✅（QEMU 环回） |
| `dma` | DMA(MDMA) + SDMA v151 | 两者 | MDMA `0x4A00_0000`、SDMA `0x520A_0000` | `DmaDriver<T>` | `dma_loopback` | ✅ |
| `pwm` | PWM（8 通道，32 位） | 两者 | — | `PwmChannel` | 无 | ✅ |
| `timer` | 定时器（3× 32 位） | 两者 | — (24 MHz) | `TimerDriver`（`TimerMode`） | `timer_irq`、`async_delay` | ✅ |
| `tcxo` | TCXO 64 位自由计数 | 两者 | `0x4400_04C0` | `TcxoDriver` | （embassy-time 间接） | ✅ |
| `time` | 计时（`Instant`/`Duration`/`Rate`，基于 TCXO） | 两者 | TCXO `0x4400_04C0` | `Instant`/`Duration` | 多处间接 | ✅ |
| `wdt` | 看门狗（24 位降计数） | 两者 | `0x4000_6000`（lock magic `0x5A5A5A5A`） | `Watchdog` | `reset_demo` 间接 | ✅ |
| `rtc` | RTC v100（48 位） | WS63 | — (32768 Hz) | `RtcDriver` | 无 | ✅ |
| `rtc_v150` | RTC v150（64 位） | BS21 | RTC0 `0x5702_4100` | `Rtc`（`Mode`） | 无 | ✅ |
| `lsadc` | 低速 ADC v154 | WS63 | `0x4400_C000` | `LsAdc`（`AdcConfig`） | `async_bus` | ✅ |
| `gadc` | 13 位 GADC v153 | BS21 | digital `0x5703_6000` 等 | `Gadc` | 无 | ⚠️ |
| `tsensor` | 温度传感器（10 位） | WS63 | — (code 114..896) | `TempSensor` | 无 | ✅ |
| `i2s` | I2S / PCM 音频 | WS63 | — | `I2sDriver`（`I2sConfig`） | 无 | ⚠️ 需外设 |
| `pdm` | PDM 麦克风前端 v150 | BS21 | `0x5208_E000` | `Pdm` | 无 | ⚠️ 需外设 |
| `keyscan` | 键矩阵扫描器 v150 | BS21 | `0x5208_D000` | `Keyscan`（`KeyEvent`） | 无 | ⚠️ 需接线 |
| `qdec` | 正交解码器 v150 | BS21 | `0x5200_0200` | `Qdec` | 无 | ⚠️ 需接线 |
| `usb` | USB 2.0 OTG（DWC2 device） | BS21 | `0x5800_0000` | `Usb`（`Speed`/`UsbError`） | 无 | ⚠️ 需主机 |
| `sfc` | SPI Flash 控制器 | WS63 | `0x4800_0000`（safety.rs） | `SfcDriver`（`BusConfig`） | 无 | ✅ |
| `efuse` | eFuse / OTP v151 | WS63 | STS+0x2C / CTL+0x30 / data+0x800 | `EfuseDriver` | 无 | ✅（只读） |
| `km` | 密钥管理 KLAD/RKP | WS63 | —（KEYSLOT_COUNT=8） | `KmDriver` | 无 | ✅ |
| `pke` | 公钥引擎（RSA/ECC/SM2） | WS63 | — | `PkeDriver` | 无 | ✅ |
| `spacc` | 安全加速器（AES/SM4/HASH） | WS63 | Crypto `0x4410_0000`（safety.rs） | `SpaccDriver` | 无 | ✅ |
| `trng` | TRNG（FRO 熵源） | WS63 | — | `TrngDriver`（`TrngError`） | 无 | ✅ |
| `trng_v1` | TRNG v1 | BS21 | `0x5200_9000` | `Trng`（`TrngError`） | 无 | ✅ |
| `system` | 系统控制（时钟/复位/电源） | WS63 | CHIP_RESET `0x4000_2110` 等 | `System`、`ResetReason` | `reset_demo` | ✅ |
| `clock` | 外设时钟门控参考（CLDO_CRG） | WS63 | — | `Peripheral` 枚举 | （多处间接） | n/a |
| `clock_init` | 时钟初始化 / PLL 切换 | WS63 | HW_CTL `0x4000_0014` 等 | `SystemClocks`、`TcxoFreq` | （多处间接） | ✅ |
| `delay` | 忙等阻塞延时 | 两者 | — | `Delay` | `blinky` 间接 | ✅ |
| `interrupt` | 自定义 local 中断控制器（无 PLIC） | 两者 | CSR LOCIEN0..2 `0xBE0..2` 等 | `Priority` | `gpio_irq`、`timer_irq` | ✅ |
| `io_config` | 引脚复用配置 | WS63 | — | — | （多处间接） | ✅ |
| `safety` | 编译期 MMIO/timer 断言（无外设） | WS63 | MMIO `0x4000_0000`..`0x5704_0000` | `PeripheralIndex`/`GpioPinIndex` | n/a | n/a |
| `asynch` | async 胶水（`block_on`/`IrqSignal`） | WS63 (`async`) | — | `IrqSignal` | `async_*` | n/a |
| `embassy` | embassy-time `Driver` | 两者 (`embassy`) | TCXO + TIMER | — | `embassy_*` | ✅ |

裸板自检：✅ 可在裸板上自验（无需外接器件）；⚠️ 需外接器件/接线/从机；n/a 非外设驱动。

## DMA 控制器

| 控制器 | 标记类型 | 基地址 | 通道 |
|--------|----------|--------|------|
| 主 DMA / MDMA | `Dma0` | `0x4A00_0000` | 0–3 |
| 安全 DMA / SDMA | `Sdma0` | `0x520A_0000` | 0–3（逻辑 8–11） |

## `Peripherals` 实例（WS63，`chip-ws63`，35 个）

`SYS_CTL0`、`SYS_CTL1`、`GLB_CTL_M`、`CLDO_CRG`、`IO_CONFIG`、`GPIO0`、`GPIO1`、`GPIO2`、`ULP_GPIO`、`UART0`、`UART1`、`UART2`、`I2C0`、`I2C1`、`SPI0`、`SPI1`、`PWM`、`I2S`、`LSADC`、`DMA`、`SDMA`、`SFC_CFG`、`TIMER`、`WDT`、`RTC`、`TCXO`、`TSENSOR`、`EFUSE`、`SPACC`、`PKE`、`KM`、`TRNG`、`RF_WB_CTL`、`SHARE_MEM_CTL`、`FAMA_REMAP`。

## `Peripherals` 实例（BS21，`chip-bs21`，28 个）

`GLB_CTL_M`、`GPIO0`–`GPIO4`、`ULP_GPIO`、`UART0`–`UART2`、`I2C0`/`I2C1`、`SPI0`–`SPI2`、`PWM`、`DMA`、`SDMA`、`TIMER`、`WDT`、`RTC`、`TCXO`、`TRNG`、`GADC`、`KEYSCAN`、`QDEC`、`PDM`、`USB`。

> 全部 PAC 外设均有 HAL 封装。寄存器行为以 fbb_ws63 / fbb_bs2x C SDK 为 ground-truth。
