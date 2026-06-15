# hisi-riscv-hal 0.5.0 配置收紧设计（per-driver）

> 评审日期：2026-06-15 · 对象：`hisi-riscv-hal` 全部 config 面 typed 收紧 · 方法：16 驱动多 agent 审计+设计（Opus 4.8），从工作流 journal 救回的结构化设计汇编（自动综合步因连接错误中断，本文档由设计结果生成 + 人工框架）。约定见 [类型化配置](../explanation/typed-config.md) + `.claude/skills/typed-config/`；PWM 已落地（`src/pwm.rs`）。**每一面完整类型草图见附录 [`config-tightening-designs.json`](config-tightening-designs.json)**；API 风格清债见 [api-style 台账](api-style-discussion-2026-06.md)。

## 1. 设计原则

**两层**：配置面用校验 newtype / type-state / 自起时钟收紧（越界构造即 `None`，不静默 clamp/截断）；操作面（embedded-hal）保持 `u16`/`&[u8]` + `Result`，签名不动。**类型编码实测硅片现实**（PWM `freq_h` 不 latch → `PwmPeriod` 为 `u16`）。

**缺陷分类**（共 116 面）：A 溢出 **15** · B 死组合 **34** · **C 未强制前提 49（最多）** · D 静默 clamp **18**。C 类最多 → 头号是『驱动该自起的时钟门 / 板级前提没强制』（见 §3）。

## 2. 逐驱动设计

### SPI

_The SPI Config struct (spi.rs:18-29) has three fields; SpiMode is already a closed 4-variant enum that maps only to valid CTRA scph/scpol combos and needs no change. The two high-severity defects are both on `data_bits: u8`: (A) any value >=33 overflows the 5-bit dfs32 field (bits 13:17) and spills bit18 into the adjacent CTRA.trsm field, silently corrupting transfer mode (e.g. 33 -> DFS=0 + trsm=1/TxOnly so reads return zeros); (B) even within the field only 8 and 32 are vendor-supported, and the driver's data path is byte-only (`write[i] as u32` push / `rx as u8` read at spi.rs:169/173/175/185/207/211), so anything but 8 silently mis-transmits on the wire. I replace `data_bits: u8` with a `DataBits` enum that, for the shipped byte-only data path, exposes only `DataBits::Eight` (an explicit `from_u8`/`new` returning `Option` accepts only 8 and rejects everything else) — this makes both A and B unrepresentable in one move. `frequency: u32` becomes a `Frequency` newtype whose fallible constructor returns None instead of silently clamping (defect D). The two clock-tree preconditions are class C: WS63 already self-enables its source clock + gate in configure_spi_source_clock (keep, harden the PLL-locked precondition as doc-and-guard since "PLL up" is a board/clock_init fact a type cannot express); BS2X has NO source-clock/gate setup ported and SPI_CLOCK_HZ is an unconfirmed TODO value, so BS2X SPI config must be gated off (compile-error / cfg-disabled) until silicon bring-up rather than silently programming a wrong/un-gated clock. OPEN QUESTION needing on-board measurement: whether 32-bit frames are usable at all given the byte-only FIFO loops — they are NOT today (each FIFO slot gets 1 byte), so DataBits deliberately does NOT expose 32 until both a frame-width-aware data path AND a HIL check exist; do not widen to 16/24 (vendor marks them "Not supported now"). Also unconfirmed: the BS2X SPI_CLOCK_HZ=32MHz value and whether BS2X needs its own gate sequence — both require the fbb_bs2x port + board._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| Config.data_bits: u8 (new_spi0 / new_spi1 / configure_spi) -> SPI_CTRA.dfs32 field overflow into trsm (defect A) | A | enum | Y | ✅ |
| Config.data_bits: u8 valid-silicon-values (only 8 and 32 vendor-supported; driver byte-only so only 8 runs) (defect B) | A | enum | Y | ◐ |
| Config.frequency: u32 -> SPI_BRS.clk_div / SCKDV silent clamp [2,0xFFFE] (defect D) | A | newtype | Y | ✅ |
| configure_spi BS2X build: SPI source clock + CKEN gate never set up; SPI_CLOCK_HZ is an unconfirmed TODO value (defect C) | A | doc-and-guard | Y | ✗ |
| configure_spi_source_clock (WS63): CLDO_CRG div [1,0x1F] + TCXO->PLL switch assumes 480MHz FNPLL already locked (defect C) | C | doc-and-guard |  | ◐ |

**头部草图 — Config.frequency: u32 -> SPI_BRS.clk_div / SCKDV silent clamp [2,0xFFFE] (defect D)**（ACCEPT iff 2 <= SPI_CLOCK_HZ/hz <= 0xFFFE. Bounds from PAC clk_div = FieldWriter<_,16,u16> bits 0:15 (lib.rs:19209-19211) and vendor caps HAL_SPI_MINUMUM_CLK_DI）：

```rust
/// A validated SPI bus frequency: one whose SCKDV divisor lands in the
/// hardware-legal [2, 0xFFFE] (even) range for the current SPI_CLOCK_HZ, instead
/// of being silently clamped. Mirrors PWM's PwmPeriod::try_from_hz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frequency { sckdv: u16 }

impl Frequency {
    /// SSI_CLK feeding the SCKDV divider (chip-specific).
    const SRC: u32 = crate::soc::chip::SPI_CLOCK_HZ;
    /// Derive from a target Hz. `None` when the required divisor would fall
    /// outside [2, 0xFFFE] — i.e. freq too high (>= SRC/2, divisor would be < 2)
    /// or too low (divisor > 0xFFFE) — rather than silently clamping to the rail.
    pub const fn from_hz(hz: u32) -> Option<Self> {
        if hz == 0 { return None; }
// …（完整见附录 config-tightening-designs.json）
```

### I2C

_The WS63 path is the dangerous one: `configure_i2c(idx, freq: u32)` writes `half = I2C_CLOCK_HZ/(4*freq)` as a full u32 into a register field that is only 16 bits wide on both the PAC (`SclHW = FieldWriter<...,16,u16>`, ws63-pac/src/lib.rs:8659/8700) and the vendor regs_def (`scl_h:16`), so freq <= 91 Hz silently truncates (class A) and freq >= 6_000_001 Hz produces half==0 / a dead bus (class B), while the freq==0 clamp to 1 Hz (class D) just steers a caller bug straight into the worst truncation case. The fix mirrors PWM exactly: replace the bare `freq: u32` with a construct-time-validated `I2cFreq(u16)` newtype whose `try_from_hz(pclk, hz) -> Option` returns `None` unless the resulting half-period is in 1..=0xFFFF AND hz is in the vendor band 1..=3_400_000 -- once an `I2cFreq` exists it is guaranteed programmable. The new constructors take `I2cFreq` (breaking). BS2X already enums Speed (safe by construction) so its only debt is the class-C wrong-rate magic SCL counts; I keep the enum and add an internal derivation from I2C_CLOCK_HZ plus a doc note. All embedded-hal `I2c`/`ErrorType`/async trait impls are untouched (operational surface stays u8 + &[u8] + Result). OPEN QUESTION needing on-board measurement: the true WS63 I2C source clock is unresolved (24 MHz driver const vs 40 MHz vendor porting default vs 80 MHz HAL doc); the newtype takes pclk as a parameter so the validation predicate is correct for whichever value silicon confirms, but the accept/reject boundary (91 Hz vs ~152 Hz vs ~305 Hz low edge; 6 MHz vs 10 vs 20 MHz dead edge) cannot be pinned until the clock is scoped. The HIL suite can validate the truncation register-level today (write I2cFreq, read back scl_h/scl_l), but spec-correct SCL timing needs a scope._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| WS63 new_i2c0/new_i2c1 -> configure_i2c: replace `freq: u32` with a validated `I2cFreq` newtype (covers the class-A low-end truncation surface) | A | newtype | Y | ◐ |
| WS63 configure_i2c high-end half==0 dead-bus (freq >= 6_000_001) -- subsumed by the I2cFreq upper-bound + vendor 3.4 MHz cap | A | newtype | Y | ◐ |
| WS63 configure_i2c freq==0 silent clamp to 1 Hz (`let freq = if freq == 0 {1} else {freq}`, i2c.rs:43) | A | newtype | Y | ✗ |
| WS63 new_i2c0/new_i2c1 infallible `-> Self` contract (no speed-mode/range validation on a bare u32) | A | newtype | Y | ◐ |
| WS63 configure_i2c I2C_CLOCK_HZ source-clock constant (24 vs 40 vs 80 MHz, unverified) | A | doc-and-guard |  | ✗ |
| WS63 configure_i2c SCL formula fidelity (omits vendor `-1` and the scl_h/(scl_h+scl_l) weighting) | A | doc-and-guard |  | ◐ |
| BS2X configure: hardcoded magic SCL counts (1,160,190)/(2,40,50) not derived from I2C_CLOCK_HZ (class-C wrong-rate) | A | doc-and-guard |  | ◐ |
| BS2X Speed enum exposes only Standard/Fast (no High-Speed, no arbitrary rate) | B | no-change |  | ✗ |

**头部草图 — WS63 new_i2c0/new_i2c1 -> configure_i2c: replace `freq: u32` with a validated `I2cFreq` newtype (covers the class-A low-end truncation surface)**（Accept iff hz != 0 AND hz <= 3_400_000 AND 1 <= pclk/(4*hz) <= 0xFFFF (65535). For pclk=24_000_000: low edge half>0xFFFF at hz<=91 (24e6/(4*65535)=91.55) -> rej）：

```rust
/// A validated I2C SCL frequency for the WS63 custom v150 controller.
///
/// Holds the *programmable* half-period count directly: `half = pclk/(4*hz)`,
/// which is written into BOTH the 16-bit `i2c_scl_h.scl_h` and `i2c_scl_l.scl_l`
/// fields (ws63-pac SclHW/SclLW = FieldWriter<..,16,u16>). Because a value of this
/// type only exists when `1 <= half <= 0xFFFF`, an out-of-range frequency is
/// rejected at construction instead of being silently truncated into the 16-bit
/// field (the old `w.bits(half)` wrote a full u32 and the HW kept only bits 0:15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I2cFreq(u16);

impl I2cFreq {
    /// Vendor hard cap: baudrate must be <= 3.4 MHz HS limit and != 0
    /// (fbb_ws63 i2c.c:933, I2C_HS_MODE_BAUDRATE_HIGH_LIMIT = 3400*1000).
// …（完整见附录 config-tightening-designs.json）
```

### UART

_The uart driver's whole config surface is a bare struct of raw numbers (`baudrate: u32`, `clock_hz: Option<u32>`) whose validity is purely runtime and clock-dependent, and `configure_uart` only clamps the low end while leaving the high end (the dominant defect) totally unguarded, never enables the UART clock gate / pinmux, and mis-handles the 5-data-bit + 2-stop frame. The tightening folds the three coupled numeric defects (high-end div=0, low-end silent clamp, clock_hz/base mismatch) into ONE construct-time-validated newtype `BaudRate` whose fallible constructor `try_new(baud, pclk)` accepts iff `1 <= pclk/(16*baud) <= 65535` (i.e. `pclk/(16*65535)+1 <= baud <= pclk/16`), exactly mirroring the PWM `PwmPeriod::try_from_hz` pattern. Because the constructor must be given the *actual* pclk, the clock_hz mismatch becomes structurally hard to make: `UartClock` is a small enum (Pll160M / BootTcxo24M / BootTcxo40M / runtime probe) that yields the pclk fed to `BaudRate`, replacing the foot-gun `Option<u32>`. The class-C clock-gate/pinmux precondition is fixed at the driver level by an `enable_uart_clock(idx)` self-enable in `configure_uart` (CKEN_CTL1 bits 18/19/20 from clock.rs, plus the per-instance TXD/RXD pinmux), exactly the PWM "construct -> clocked" lesson. The 5-bit/2-stop frame collision is fixed by making `configure_uart` honor the v151 rule (stp=1 means 1.5 stops when dlen=0). All embedded-io / embedded-hal-nb / embedded-io-async trait impls are byte-for-byte untouched (they only call `write_byte`/`read_byte`, not config). OPEN QUESTION needing on-board measurement: (1) whether silicon actually runs at div=1 with a large div_fra or needs div>=2 (would raise the high-end ceiling predicate from `<= pclk/16` to `<= pclk/32`); (2) the BS21/BS2X UART_CLOCK_HZ is TODO/assumed 32 MHz (soc/bs21.rs:34-35) so every BS2X bound is provisional until confirmed; (3) whether UART1/UART2 gate+pinmux are truly off at app entry (flashboot only provisions the UART0 console), which sets how load-bearing the self-enable is._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| Config.baudrate HIGH end (integer divider underflow to div=0 when baud > pclk/16) — the dominant defect | B | newtype | Y | ◐ |
| Config.clock_hz: Option<u32> (None silently selects 160 MHz PLL even when running on the 24/40 MHz flashboot TCXO base -> ~4x-wrong baud; also shifts the unguarded high-end ceiling) | B | enum | Y | ✅ |
| UART clock gate CKEN_CTL1[20:18] + TXD/RXD pinmux — unenforced precondition for new_uart1 / new_uart2 (class C) | B | doc-and-guard |  | ◐ |
| Config.baudrate LOW end (silent clamp to min_baud instead of signalling) — uart.rs:122-123 | B | newtype | Y | ◐ |
| Config { data_bits: Five, stop_bits: Two } — stp=1 with dlen=0 yields 1.5 stop bits, not 2 (v151 frame mismatch) | B | doc-and-guard |  | ✗ |
| div_fra (6-bit fractional divider) and DataBits/Parity/StopBits encodings | B | no-change |  | ✅ |
| Operational write_byte/read_byte/write taking a runtime idx: u8 independent of the type-state instance (SCOPE-NOTE foot-gun, not a Config-value defect) | B | no-change |  | ✗ |

**头部草图 — Config.baudrate HIGH end (integer divider underflow to div=0 when baud > pclk/16) — the dominant defect**（ACCEPT iff 1 <= floor(pclk/(16*baud)) <= 65535, equivalently pclk/(16*65535)+1 <= baud <= pclk/16. Verified numerically: at 160 MHz accept [153, 10_000_000]; at）：

```rust
/// A baud rate validated against the ACTUAL UART base clock at construction.
/// Once a `BaudRate` exists, `1 <= pclk/(16*baud) <= 65535`, so the 16-bit
/// integer divider DIV_H:DIV_L is guaranteed non-zero and non-overflowing —
/// a div=0 dead line (or a >0xFFFF overflow) is unrepresentable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaudRate {
    baud: u32,
    pclk: u32,
}

impl BaudRate {
    /// Build a baud rate for a specific base clock `pclk` (Hz). Returns `None`
    /// unless the integer divider `div = pclk/(16*baud)` is in `1..=65535`.
    pub const fn try_new(baud: u32, pclk: u32) -> Option<Self> {
// …（完整见附录 config-tightening-designs.json）
```

### I2S

_The I2S config surface is the worst-affected driver in the audit: the ws63-pac SVD encodes a fabricated textbook layout for the `mode` and `i2s_crg` registers that contradicts the authoritative vendor `hal_sio_v151_regs_def.h`, so essentially NO structurally-valid `Master` config is runnable today (the master/slave-select bit7 is never written, and the crg clock-enables at bit0/bit1 are never set — `configure` writes 0x100/bit8, a reserved bit). The tightening therefore has two halves that must land together: (1) regenerate the PAC `mode`/`i2s_crg` from the vendor regs_def, then drive the correct bits, and (2) wrap every divider/threshold field in a construct-time-validated newtype and make a zero-divider Master unrepresentable via a type-state `Master`/`Slave` split (mirroring PWM's `PwmPeriod`/`Duty` and the role-as-type pattern). The driver must also self-enable its own clock tree in `configure` (CMU_NEW_CFG0 bit0 + CKEN_CTL0 bit11 bus + bit12 clk), exactly as PWM self-enables CLK_SEL/CKEN/DIV — that is the class-C gate. ChannelCount and DataWidth enums must be re-tabled to the silicon enums (no 6ch; data widths are 16/18/20/24/32-bit with code0 reserved, not 8/10/12/14/16/18/20/24). Newtype range checks, the clock-gate self-enable, and post-configure register read-back are validatable on the connected board at register/poll level (the existing `i2s_version_live` HIL test already proves the window is live and can be extended to read back `mode`/`i2s_crg`/dividers). OPEN QUESTIONS needing on-board measurement (cannot be settled from headers): (a) whether DataWidth code5 (32-bit) actually latches on this part and which of 16/18/20/24 the codec path accepts — parallel to PWM's pwm_freq-high-half lesson; (b) whether BOTH CKEN bit11+bit12 AND CMU_NEW_CFG0 bit0 are required for the SIO block to respond (sio_porting sets all three); (c) effective FIFO depth for threshold bounding (vendor SIO_FIFO_SIZE=8); (d) whether i2s_fs_sel/i2s_bclk_sel (crg b2/b4) must be explicitly cleared. Producing an actual BCLK/FS waveform or audio frame is NOT board-validatable here (no scope/codec/jumper for I2S)._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| I2sConfig.role + bclk_div/fs_div_num/fs_div_ratio (Master with all dividers=0) | B | type-state | Y | ◐ |
| I2sConfig (Default impl that ships role=Master, all dividers=0) | B | no-change | Y | ✗ |
| configure(): i2s_crg clock-enable write (currently 0x100 = bit8, a reserved bit) | B | result |  | ◐ |
| configure(): mode register bit packing (channels@0:1, clk_edge@2, master@3, pcm@4) | B | result |  | ◐ |
| ChannelCount enum (Two=0,Four=1,Six=2,Eight=3) | B | enum | Y | ◐ |
| DataWidth enum (Bits8=0,Bits10,Bits12,Bits14,Bits16=4,Bits18,Bits20,Bits24=7) | B | enum | Y | ◐ |
| bclk_div: u8 written as `bits & 0x7F` (silent truncation of 128..=255) | B | newtype | Y | ◐ |
| fs_div_num: u16 written as `bits & 0x3FF` (silent truncation of 1024..=65535) | B | newtype | Y | ◐ |
| fs_div_ratio: u16 written as `bits & 0x7FF` (silent truncation of 2048..=65535) | B | newtype | Y | ◐ |
| I2S clock gate: CKEN_CTL0 bit11(bus)+bit12(clk) and CMU_NEW_CFG0 bit0 (class C — no self-enable) | B | doc-and-guard |  | ✅ |
| tx_fifo_threshold / rx_fifo_threshold: u8 (0..=255) vs 8-entry FIFO | D | newtype | Y | ◐ |
| loopback: bool (blind write of 1<<8 to the version register, no RMW, no txrx-enable) | B | doc-and-guard |  | ✗ |

**头部草图 — I2sConfig.role + bclk_div/fs_div_num/fs_div_ratio (Master with all dividers=0)**（ACCEPT: role=Master only when constructed with BclkDiv/FsDivNum/FsDivRatio newtypes (each of which independently rejects an all-zero/out-of-range divider at con）：

```rust
// Replace the flat `role: I2sRole` + three free divider fields with a role-as-type
// payload so a zero-divider Master is UNREPRESENTABLE (mirrors PWM 'construct -> runnable').
pub struct Master {
    pub bclk_div: BclkDiv,        // validated newtypes below (all guaranteed non-zero-usable)
    pub fs_div_num: FsDivNum,
    pub fs_div_ratio: FsDivRatio,
}
pub struct Slave;                // no dividers: clocks come from outside

// Sealed trait so only these two roles exist.
pub trait Role: crate::private::Sealed { fn is_master(&self) -> bool; fn dividers(&self) -> Option<(u8,u16,u16)>; }
impl Role for Master { fn is_master(&self)->bool{true} fn dividers(&self)->Option<(u8,u16,u16)>{Some((self.bclk_div.get(), self.fs_div_num.get(), self.fs_div_ratio.get()))} }
impl Role for Slave  { fn is_master(&self)->bool{false} fn dividers(&self)->Option<(u8,u16,u16)>{None} }

// …（完整见附录 config-tightening-designs.json）
```

### WDT

_The headline tightening is a construct-time-validated `WdtTimeout` newtype that replaces the raw `timeout_ms: u32` argument to `configure`, exactly mirroring how `PwmPeriod`/`Duty` make `PwmChannel::configure` infallible. Its `from_ms(u32) -> Option<Self>` rejects (a) `timeout_ms == 0` — which today programs WDT_LOAD=0 / instant-reset (class B) — and (b) any value whose load field would exceed the 24-bit WDT_LOAD max, replacing the dangerous silent clamp to ~179 s (WS63) / ~134 s (BS2X) that fires the watchdog SOONER than asked (class D, the high-severity defect). Because the constructor pre-validates, `configure` becomes a total function that always programs the requested window or never compiles, and the misleading `zero_timeout_yields_zero_load` / `large_timeout_clamps_to_max_field` tests get replaced by constructor-rejection tests. The per-chip max bound is derived from the clock: max_ms = (0xFFFFFF<<8)*1000/WDT_CLOCK_HZ = 178956 @24 MHz, 134217 @32 MHz. The class C clock-gate gap (AON_CRG_CKEN_CTL.wdt_gate[4] / wdt_soft_rst[1]) and the class B BS2X clock-rate assumption are doc-and-guard, not types: the AON WDT gate is reset-default-on and the vendor turnon hook is a no-op, so unlike PWM there is nothing to self-enable on the normal path — but I add an optional `#[cfg(feature=\"chip-ws63\")]` `enable_wdt_clock()` guard plus a documented precondition. OPEN QUESTION needing on-board measurement: the BS2X WDT counting clock is assumed = 32 MHz TCXO but is UNVERIFIED (bs21.rs literally carries `TODO(bs21): confirm`, and no fbb_bs2x watchdog porting exists in-tree); if BS2X actually counts at a divided/24 MHz rate, every timeout_ms AND the 134217 ms max bound are mis-scaled. The WS63 HIL suite can validate the WS63 bound and rejection logic at register-poll level today; the BS2X bound needs a BS2X board._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| configure(timeout_ms: u32, ...) — the ms->WDT_LOAD 24-bit field conversion that silently clamps to WDT_MAX_LOAD (line 119 .min(WDT_MAX_LOAD)), firing the watchdog sooner than requested (class D, high) | D | newtype | Y | ✅ |
| configure timeout_ms == 0 -> WDT_LOAD = 0 (instant/immediate-reset watchdog; codified by the `zero_timeout_yields_zero_load` test) (class B, medium) | D | newtype | Y | ✅ |
| new()/configure() never enable AON_CRG_CKEN_CTL.wdt_gate[4] or release AON_SOFT_RST_CTL.wdt_soft_rst[1] -- 'construct -> clocked' is an unenforced precondition (class C, low) | D | doc-and-guard |  | ◐ |
| WDT_CLOCK_HZ = soc::chip::TCXO_HZ assumed = 32 MHz on BS2X/BS21 (unverified vs vendor) -- mis-scales every timeout_ms and the MAX_MS bound if wrong (class B, medium) | D | doc-and-guard |  | ◐ |

**头部草图 — configure(timeout_ms: u32, ...) — the ms->WDT_LOAD 24-bit field conversion that silently clamps to WDT_MAX_LOAD (line 119 .min(WDT_MAX_LOAD)), firing the watchdog sooner than requested (class D, high)**（ACCEPT iff ms != 0 AND 1 <= (ms*WDT_CLOCK_HZ/1000)>>8 <= 0xFFFFFF. Equivalently ms in 1..=MAX_MS where MAX_MS = (0xFFFFFF<<8)*1000/WDT_CLOCK_HZ = 178_956 @24MHz）：

```rust
/// A validated watchdog timeout: a millisecond value guaranteed to fit the
/// 24-bit WDT_LOAD field at the current chip's WDT_CLOCK_HZ. Once constructed,
/// programming it is exact -- never clamped, never wrapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WdtTimeout {
    /// The 24-bit WDT_LOAD field value (already shifted right by WDT_LOAD_RESEV).
    load: u32,
}

impl WdtTimeout {
    /// Largest representable timeout in ms at WDT_CLOCK_HZ:
    /// (WDT_MAX_LOAD<<RESEV)*1000/clk = 178_956 @24MHz, 134_217 @32MHz.
    pub const MAX_MS: u32 =
        (((WDT_MAX_LOAD as u64) << WDT_LOAD_RESEV) * 1000 / WDT_CLOCK_HZ as u64) as u32;
// …（完整见附录 config-tightening-designs.json）
```

### Timer

_The timer driver's config surface is tightened with one construct-time-validated newtype, TimerTicks(u32), mirroring PWM's Duty/PwmPeriod pattern: it has from_ticks (rejects 0), try_from_micros / try_from_millis / try_from_nanos (reject 0 and >max, returning Option instead of the current silent clamp/truncate), and a max_micros() bound derived from TIMER_CLOCK_HZ. configure()/start()/start_micros()/start_millis() are retyped to take TimerTicks, so the class-B zero-period and class-D over-range/sub-tick defects become unrepresentable rather than silently wrong. The control-register write in configure() is changed from a blind full w.bits() to a read-modify-write that preserves int_mask (PAC bit 3), fixing the class-C implicit-unmask side-effect using named PAC fields. The class-C wrong-crystal-frequency defect is the highest-severity item but CANNOT be fixed by a type alone: TIMER_CLOCK_HZ is a compile-time constant while the WS63 timer counts at a runtime-selected 24-or-40 MHz crystal; the design adds a runtime TimerClock read of the HW_CTL TCXO strap (reusing the proven uart_boot_clock_hz() pattern) so the us/ms conversions use the measured crystal, plus a doc-and-guard board precondition. The class-B HW-periodic-mode item stays no-change in code but is flagged doc-and-guard pending on-board confirmation (the vendor never programs HW mode=1). Open questions needing on-board measurement: (a) does HW mode=1 actually auto-reload on WS63 silicon, since the vendor forces one-shot + software reload; (b) the exact target-board WS63 crystal (24 vs 40 MHz) — the HW_CTL strap read must be scope-confirmed against produced delays; (c) BS21's actual TCXO (32 MHz is self-flagged TODO). The HIL suite can validate the ticks/zero/over-range typing and the int_mask-preserving RMW at the register/poll level; the crystal-frequency correctness needs a scope on the timer output._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| OneShotTimer::start_micros/start_millis, PeriodicTimer::start_micros (us/ms -> ticks; silent clamp to u32::MAX past ~178.96 s @24 MHz) | D | newtype | Y | ✅ |
| TimerDriver::configure(load_value) / OneShotTimer::start(count) / PeriodicTimer::start(period) — load_value == 0 | D | newtype | Y | ✅ |
| OneShotTimer::DelayNs::delay_ns and AsyncDelay::delay_ns (ns -> ticks; sub-tick truncates to 0 -> blocking no-op vs async max(1)) | D | newtype |  | ◐ |
| TimerDriver::configure / all start_*/delay_* — TIMER_CLOCK_HZ assumed 24 MHz (WS63) / 32 MHz (BS21), but WS63 crystal is runtime 24-or-40 MHz | D | doc-and-guard |  | ✗ |
| TimerDriver::configure(mode, load_value) — blind full control write zeroes int_mask (bit 3), silently UNMASKING the IRQ each configure | B | result |  | ◐ |
| TimerDriver::configure(mode: TimerMode::Periodic) / PeriodicTimer::start — HW mode=1 auto-reload unverified on silicon (vendor never programs it) | D | doc-and-guard |  | ✅ |

**头部草图 — OneShotTimer::start_micros/start_millis, PeriodicTimer::start_micros (us/ms -> ticks; silent clamp to u32::MAX past ~178.96 s @24 MHz)**（Accept us in 1..=max_micros() where max_micros() = u32::MAX/(TIMER_CLOCK_HZ/1e6) = 178_956_970 us (~178.96 s) at 24 MHz, 107_374_182 us (~107.37 s) at 40 MHz. R）：

```rust
/// A validated timer load count: a non-zero count of TIMER_CLOCK_HZ ticks that
/// fits the full 32-bit TIMER%s_LOAD_COUNT register. Construction rejects both
/// the zero-period (class B: no working output) and over-range (class D: the old
/// silent clamp to u32::MAX) cases, so configure()/start() can never be handed a
/// load the hardware cannot run for the requested time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimerTicks(u32);

impl TimerTicks {
    /// Wrap a raw down-counter load. `None` for 0 (a 0 load does NOT yield a
    /// zero-length delay on the V150 down-counter: it fires immediately or wraps
    /// 2^32 — no usable output; vendor rejects it, timer.c:425).
    pub const fn from_ticks(ticks: u32) -> Option<Self> {
        if ticks == 0 { None } else { Some(TimerTicks(ticks)) }
// …（完整见附录 config-tightening-designs.json）
```

### GPIO

_The gpio driver's tightening centers on making "a pin value exists" imply "this pin is physically present, correctly muxed, on the right chip" — none of which the current bare-u8 + assert!/unreachable! surface guarantees. The highest-leverage change is a validated `GpioPin` newtype (Option ctor over the exact WS63 map: GPIO0 0..=7, GPIO1 8..=15, GPIO2 16..=18, i.e. block 2 only bits 0..=2) that replaces every bare `u8` taken by `AnyPin::steal`, the legacy free functions, and `IoConfigDriver`'s pin args; this kills both the silent no-op (pins 19..=23 → GPIO2 phantom pads) and the `unreachable!()` panic (pin ≥ 24 → block ≥ 3) in one type. `set_uart_mux` is a clean class-A overflow (0x07 mask into a 2-bit field) fixed by a `UartSel` enum/`u8`-newtype bounded 0..=3. `configure_*_pad`'s full `.write()` zeroing of IE/mode/ST bits is a class-D silent clamp fixed by switching to RMW (`.modify`) — no type change, but it must be paired with the apply_pull lesson. Two class-C preconditions cannot be expressed by value-types and need type-state / chip-gating instead: (1) the pin-mux-must-be-GPIO precondition, addressed by routing `init_*` through a `GpioFunction` capability obtained from `IoConfigDriver::into_gpio(pin)` so a GPIO driver is unconstructable without having set the mux; and (2) the chip-gating — `gpio` and `ulp_gpio` must become `#[cfg(feature = "chip-ws63")]` (or have apply_pull/ULP-clock paths gated) because the inline 0x4400_D000 IO_CONFIG write and the 32K ULP clock provisioning are WS63-only and silently wrong on BS2X. OPEN QUESTIONS needing on-board measurement: (a) does the WS63 reset-default mux already select GPIO (sel=0) for the blinky pads — if yes the mux-capability type is a correctness-hardening, not a bug-fix, and the silicon-validated blinky is setting mux out-of-band; (b) whether GPIO2 pads 19..=23 truly no-op or alias something on silicon; (c) whether ROM leaves the ULP 32K clock running (gpio_ulp_int_en is a WS63 stub) or enable_interrupt silently never fires — determines if enable_interrupt must self-enable a clock gate (class C) or just gain a doc-and-guard._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| AnyPin::steal(pin: u8) + the legacy gpio::create_input_pin/create_output_pin(pin: u8) free functions (gpio.rs:101/549/557) | B | newtype | Y | ✅ |
| AnyPin::init_input / init_output / init_flex — the unenforced precondition that the pad mux must select the GPIO function before GPIO data/oen writes take effect (gpio.rs:126/133/145) | B | type-state | Y | ◐ |
| gpio module chip-gating + apply_pull's inline 0x4400_D000 IO_CONFIG write reachable on BS2X (gpio.rs:404/423, lib.rs:42) | B | doc-and-guard |  | ✗ |
| IoConfigDriver::set_uart_mux(function: u8) — masks function & 0x07 into a 2-bit hardware field (io_config.rs:193) | C | enum | Y | ✅ |
| IoConfigDriver::configure_gpio_pad / configure_uart_pad / configure_sfc_pad — full .write() zeroes every pad-ctrl bit not in build_pad_ctrl, clearing IE/mode/ST for that pad (io_config.rs:218/280/308) | C | result |  | ◐ |
| ulp_gpio module gating + UlpGpioPin::enable_interrupt — ULP interrupt needs the 32K clock provisioned (driver never enables it) and the module is ungated (ulp_gpio.rs:80, lib.rs:156) | C | doc-and-guard |  | ◐ |
| ulp_gpio::create_input_pin / create_output_pin(pin: u8) — computes bit = pin - 107 (u8 underflow/wrap) before assert!(bit < 8); panics on bad input instead of returning Option (ulp_gpio.rs:142/150) | B | newtype | Y | ✅ |
| Pull enum cannot express vendor value 2 = PIN_PULL_TYPE_STRONG_UP (missing-feature noted in audit; gpio.rs Pull / io_config.rs PullResistor) | B | enum | Y | ◐ |

**头部草图 — AnyPin::steal(pin: u8) + the legacy gpio::create_input_pin/create_output_pin(pin: u8) free functions (gpio.rs:101/549/557)**（ACCEPT pin in 0..=18 (block in {0,1,2}; for block==2, bit in 0..=2). REJECT pin in 19..=23 (GpioId::from_pin returns None — these are GPIO2 bits 3..=7 with no p）：

```rust
/// A construct-time-validated WS63 GPIO pin id. Once one exists it maps to a
/// physically-present pad and a reachable register block (never hits regs()'s
/// unreachable!()).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpioId { block: u8, bit: u8 }

impl GpioId {
    /// Valid WS63 pins: 0..=18. GPIO0=0..=7, GPIO1=8..=15, GPIO2=16..=18.
    /// Returns None for 19..=23 (GPIO2 phantom pads: block2 only has 3 pads) and
    /// >=24 (block>=3, which regs() cannot map).
    pub const fn from_pin(pin: u8) -> Option<Self> {
        if pin > 18 { return None; }
        Some(Self { block: pin / 8, bit: pin % 8 })
    }
// …（完整见附录 config-tightening-designs.json）
```

### DMA

_The DMA driver needs four real tightenings plus one no-change. (A, breaking) transfer_size: replace the raw u16 + `& 0xFFF` truncation (dma.rs:272) with a TransferCount(u16) newtype, from_count -> Option valid only for 1..=4095 (12-bit field per PAC DmacTransSize0W lib.rs:15745, vendor reject at 4096); existence of the value means it programs without wrapping, and the mask is dropped. (A, breaking) src/dst_peripheral: drop the public raw u8 fields (dma.rs:133-136) in favour of the existing DmaPeripheral enum (Option<DmaPeripheral>, None=>Tie0), whose discriminants {0..14} all fit the 4-bit field, eliminating the `& 0x0F` wrap; higher vendor IDs (16..28) are genuinely unreachable so the enum rightly omits them. (C, non-breaking) cache coherence: make configure_channel self-service like PWM self-enabling its clock — clean_range the incrementing RAM source before the kick (mirroring vendor osal_dcache_flush_all before ch_en) and document that the caller must invalidate_range the destination after completion; addresses stay raw u32 because a type cannot tell a RAM buffer from a peripheral FIFO. (C, non-breaking) CLK_AUTO_CTRL: gate the 0x4400_0244/bit19 constant behind chip-ws63 and set None on chip-bs2x rather than poking a WS63-specific glue register on BS2X. (D, no-change) channel u8: the assert already signals (panics, not truncates), so keep it; optionally add an additive try_channel(u8)->Option backstop. The WS63 HIL suite (dma_mem_to_mem, hil.rs:250) can validate TransferCount, the cache self-service, the channel guard, and the WS63 clock path at register/poll level (it already clean/invalidates and polls channel_enabled). OPEN, needs on-board measurement: the BS2X CLK_AUTO_CTRL register offset/mask is unconfirmed (fbb_bs2x not in tree) — until a BS2X board measurement provides it, BS2X DMA clocking stays None/unverified, and the WS63 HIL rig cannot reach a BS2X part to close this. The peripheral-select enum swap is also not WS63-HIL-observable beyond compile because there is no peripheral-paced DMA jumper in the loopback suite._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| configure_channel(transfer_size: u16) — the 12-bit trans_size field (dma.rs:272, control /= (transfer_size as u32) & 0xFFF) | A | newtype | Y | ✅ |
| configure_channel(src_addr / dst_addr) cache-coherence precondition — driver kicks transfer (dma.rs:322-325) without any D-cache maintenance on a non-coherent cache | A | doc-and-guard |  | ✅ |
| DmaChannelConfig.src_peripheral / dst_peripheral: public raw u8 (dma.rs:133-136); masked & 0x0F into the 4-bit src_per/dest_per field (dma.rs:297-298) | A | enum | Y | ✗ |
| Dma0::CLK_AUTO_CTRL = Some((0x4400_0244, 0x0008_0000)) hardcoded WS63 glue register, not chip-gated (dma.rs:60-63) | C | doc-and-guard |  | ◐ |
| configure_channel / enable_channel(channel: u8) — physical_channel_index assert!()-panics for channel outside [base, base+4) (dma.rs:78-81) | A | no-change |  | ✅ |

**头部草图 — configure_channel(transfer_size: u16) — the 12-bit trans_size field (dma.rs:272, control |= (transfer_size as u32) & 0xFFF)**（Accept beats in 1..=4095; reject 0 (programs a zero-beat no-op that completes instantly copying nothing) and reject >=4096 (wraps: 4096->0, 5000->904). Bound 40）：

```rust
/// A validated DMA transfer length in source-width beats.
/// Existence of a value guarantees it fits the 12-bit `trans_size` field
/// (1..=4095) — there is no representable count that the hardware truncates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferCount(u16);

impl TransferCount {
    /// Max programmable beats: the trans_size field is 12 bits, and the
    /// vendor rejects `transfer_num >= HAL_DMA_CH_MAX_TRANSFER_NUM (4096)`.
    pub const MAX: u16 = 4095;

    /// Construct from a beat count; `None` if 0 (no-op) or > 4095 (would wrap).
    pub const fn from_count(beats: u16) -> Option<Self> {
        if beats >= 1 && beats <= Self::MAX {
// …（完整见附录 config-tightening-designs.json）
```

### Clock

_The clock driver is probe/reference code, not a numeric Config-struct surface, so there are no class-A register-field-overflow defects to fix with bound-checked newtypes; the tightening is mostly fixing a polarity bug, replacing the assumed()/init_clocks "boot ROM did it" preconditions with a fallible read+verify Result API, and encoding the XIP/SRAM and PLL-up preconditions into type-state and ownership so an unrunnable init is unrepresentable. The single highest-value change is fixing TcxoFreq::detect's inverted strap polarity (bit0==1 -> MHz24) AND making it a fallible reader that returns a strap-derived value rather than guessing, then deleting the dishonest SystemClocks::assumed() const fabricator in favor of probe_clocks() as the only constructor. init_clocks is split into a typed sequencer that (a) takes ownership of CldoCrg by value, (b) is gated behind an ExecFromSram type-state token the caller can only mint after relocating out of XIP, (c) verifies PLL lock BEFORE switching any source mux (returning Err and switching nothing if unlocked), and (d) applies the per-TCXO TRNG/TSENSOR/I2C/Timer/WDT dividers the vendor switch_clock does. OPEN ON-BOARD QUESTION (HIL register-poll only confirms part of it): the polarity fix is a source-contradiction provable statically, but it still wants a known-24MHz board to confirm corrected detect() now reads MHz24 on bit0==1; and whether init_clocks actually hangs under XIP (the ExecFromSram guard's necessity) cannot be HIL-validated at register level — it needs a scope/board-population observation of a fetch-clock glitch, so that guard is justified by vendor evidence + prose, not the HIL suite._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| TcxoFreq::detect (clock_init.rs:103-107) — inverted HW_CTL bit0 strap polarity (defect D, source contradiction) | D | result |  | ✅ |
| init_clocks (clock_init.rs:197-253) — switch-then-verify ordering (defect B); switches flash/UART/SPI muxes to PLL then polls lock | D | result | Y | ✅ |
| init_clocks (clock_init.rs:197-214) — XIP-unsafe flash-clock manipulation precondition (defect C), unenforced | D | type-state | Y | ✗ |
| init_clocks (clock_init.rs:197-253) — ignores &SysCtl0/&CldoCrg (_sys_ctl0/_cldo_crg), no exclusive CRG ownership; other live CldoCrg refs can race (defect C) | D | type-state | Y | ◐ |
| init_clocks PLL-up precondition (clock_init.rs:197-253, doc 166-169) — PLL assumed-up, never powered/configured (defect C) | D | doc-and-guard |  | ◐ |
| init_clocks (clock_init.rs:216-253) — missing per-TCXO TRNG/TSENSOR/I2C/Timer/WDT divider setup vendor switch_clock does (defect B) | D | result |  | ◐ |
| Peripheral::cken_info (clock.rs:59-81) — None for 11 default-on peripherals, incl. Spi1 (defect C, reference-table precondition) | D | enum | Y | ✗ |
| SystemClocks::assumed (clock_init.rs:165-170) — const fabricator hard-codes tcxo_freq=MHz40, pll_locked=true (defect D, silent guess) | D | no-change | Y | ✗ |

**头部草图 — init_clocks (clock_init.rs:197-214) — XIP-unsafe flash-clock manipulation precondition (defect C), unenforced**（Not a numeric bound — a presence predicate: init_clocks is callable iff the caller can produce an ExecFromSram, which only `unsafe fn assume()` mints. Evidence ）：

```rust
/// Capability token proving the caller is executing from SRAM, not XIP from
/// SPI flash. init_clocks reconfigures the live SFC controller (CMU_NEW_CFG1
/// 0x1->0x3, CLK_SEL bit18); doing that while fetching instructions from that
/// same flash glitches the fetch clock and hangs the core.
///
/// There is no register bit that proves 'PC is in SRAM', so this is a
/// doc-and-guard-backed *capability*: only an unsafe constructor mints it, and
/// the safe init_clocks signature REQUIRES it, moving the precondition from
/// prose (soc/ws63.rs:66) into the type. A pure-XIP example simply cannot call
/// init_clocks because it cannot honestly produce this token.
pub struct ExecFromSram(());

impl ExecFromSram {
    /// # Safety
// …（完整见附录 config-tightening-designs.json）
```

### SFC

_The sfc driver has two highest-confidence silicon bugs (bus_dma_len off-by-one, and rd/wr_enable never set) plus a skewed/reserved SpiIfType encoding, all of which make the documented operation silently produce wrong/no result and are directly contradicted by the vendor regs_op. The fix is a SpiIfType enum re-encoded to the 5 real hardware values {0,1,2,5,6} (so reserved 3/4 are unrepresentable), a DmaLen newtype that stores n=len-1 and rejects 0/over-window, a BusConfig that cannot be built without rd/wr_enable and that RMW-preserves the write/read half, validated-newtype timing fields (Tshsl in 3..=15, Tcss/Tcsh in 0..=7), a CmdAddr newtype rejecting >=0x4000_0000, and a per-command IfType/dummy plumbed through send_command/command_with_data. No class-C clock-gate self-enable applies: SFC is the boot/XIP controller and is clocked by reset default (clock.rs returns cken_info=None for Sfc, correct). The class-C XIP-running and addr-mode write-ordering preconditions are genuinely not type-expressible (the type system cannot know whether the CPU is currently fetching from the flash window) and stay doc-and-guard. OPEN QUESTION needing on-board measurement: whether reserved iftype encodings 3/4 actually hang vs silently degrade to standard on silicon (only needed to confirm the enum's reject behaviour is conservative-correct), and whether the SFC bus window upper bound is 0x9FFFFF (vendor SFC_MCPU_END) vs the PAC mem_saddr 0xBFFFFF doc — these two windows disagree and the tighter 0x9FFFFF should be used until a board read confirms which the silicon enforces._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| configure_bus: SpiIfType enum (read_if_type / write_if_type) discriminants cast into 3-bit rd_mem_if_type[0:2]/wr_mem_if_type[16:18] | B | enum | Y | ✅ |
| configure_bus: rd_enable[bit31]/wr_enable[bit30] never set + blind full write of BUS_CONFIG1 instead of RMW; missing BUS_CONFIG2 | B | newtype | Y | ◐ |
| configure_bus / bus reconfigure while executing from flash (XIP-running precondition) | B | doc-and-guard |  | ✗ |
| bus_dma_start: length -> bus_dma_len (off-by-one, no -1; length=0 wraps to 0x3FFF_FFFF) | B | newtype | Y | ✅ |
| bus_dma_start: mem_addr (and flash op range) unchecked against SFC bus window 0x200000..0x9FFFFF | B | newtype | Y | ◐ |
| bus_dma_start: bus_dma_ahb_ctrl never written (bursts off) + dma_sel_cs left at 0 (CS0) instead of vendor 1 (CS1) | B | result | Y | ✅ |
| send_command / command_with_data: address -> cmd_addr (full u32 into 30-bit field, top 2 bits truncated) | C | newtype | Y | ✅ |
| configure_timing: tcss / tcsh (3-bit, silent & 0x07 wrap) | D | newtype | Y | ◐ |
| configure_timing: tshsl (4-bit, silent mask-then-floor clamp to MIN_TSHSL-2) | D | newtype | Y | ◐ |
| send_command / command_with_data: mem_if_type hardcoded Standard (0<<17), dummy_byte_cnt never set | B | enum | Y | ✅ |
| configure_global: addr_mode (flash_addr_mode[bit2]) write-ordering precondition (no effect while CMD_CONFIG[start]=1) | B | result | Y | ✅ |
| Clock gating for Peripheral::Sfc (audit notes NO clock-gate defect) |  | no-change |  | ✗ |

**头部草图 — configure_bus: rd_enable[bit31]/wr_enable[bit30] never set + blind full write of BUS_CONFIG1 instead of RMW; missing BUS_CONFIG2**（rd_enable=bit31, wr_enable=bit30 always programmed to 1 (PAC lib.rs:17258-17265; vendor hal_sfc_v150_regs_op.c:72,85). dummy fields 0..=7 (3-bit), prefetch 0..=）：

```rust
// A BusConfig that cannot be constructed without committing to enabling the bus halves,
// and a configure_bus that does the vendor's read-modify-write so the two halves and the
// enable bits coexist instead of clobbering each other.
pub struct BusConfig { /* existing fields unchanged */ }
impl BusConfig {
    /// Build a read+write bus config. Both bus halves are ENABLED unconditionally
    /// (vendor RD_ENABLE/WR_ENABLE), matching a working XIP/mem-mapped setup.
    pub const fn new(/* read_if_type, read_dummy_bytes, ... write_instruction */) -> Option<Self> {
        // reject dummy>7 (3-bit), prefetch>3 (2-bit) here so the later pack needs no mask
        // ... returns None on any out-of-field input ...
    }
}
pub fn configure_bus(&mut self, config: &BusConfig) {
    let r = self.regs();
// …（完整见附录 config-tightening-designs.json）
```

### LSADC

_The lsadc (v154, base 0x4400_C000) tightening splits cleanly: three class-A silent-truncation fields (sample_cnt 5b, cast_cnt 7b, rxintsize 3b) become construct-time-validated newtypes returning Option, mirroring PwmPeriod/Duty — once constructed they are guaranteed programmable, so the driver drops its `& 0x1F`/`& 0x7F`/`& 0x07` masks entirely (those masks are redundant anyway: the PAC FieldWriter::bits already masks `& mask::<WI>()`, confirmed at lib.rs:384-387, so the existing code truncates twice and signals never). The load-bearing class-C gap is that LsAdc, unlike PWM, never brings up its own gate: it must self-enable the CLDO bus clock + deassert soft_rst_lsadc_n(bit5)/soft_rst_lsadc_bus_n(bit7) at cldo_crg_rst_soft_cfg_1 @ 0x4400_1138 in new(), using exactly the raw-MMIO rmw pattern the sibling gadc driver already uses (gadc.rs:57-85) and PWM uses (pwm.rs:120-149); without it no config write latches. The AFE staged power-up (da_lsadc_en |= 0x7000 -> 0xE7F -> 0x100 -> 0x80 with 500us/300us tcxo delays, plus simu_cfg writes to da_lsadc_rwreg_1..3 which are NOT in the PAC RegisterBlock — confirmed absent) plus offset/gain/cap calibration must move into a single staged power_up() method; the raw set_analog_enable overwrite is removed (class B). CIC/offset/gain stay no-change-but-deprecated (vendor never writes them; dead in the supported raw path). channel pinmux (class C) and the unbounded conversion wait (class C, vendor caps at COUNT_THRESHOLD=1000) get a guard method + a Result-returning timeout. OPEN QUESTION needing on-board measurement: per the PWM high-half-doesn't-latch precedent, whether the full PAC widths (cic_osr 8b, da_lsadc_en 16b) and even the staged AFE ramp actually produce a live conversion on silicon is unconfirmed — the current HIL rig (tests/hil.rs) has no lsadc coverage and no analog source, so only the newtype constructors and register-latch (post clock-gate) are board-validatable at register-poll level; an actual conversion-correct test needs an analog stimulus part._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| configure_scan — AdcConfig.sample_count (sample_cnt, 5-bit) | A | newtype | Y | ◐ |
| configure_scan — AdcConfig.cast_count (cast_cnt, 7-bit) | A | newtype | Y | ◐ |
| set_fifo_waterline — level (rxintsize, 3-bit) | A | newtype | Y | ◐ |
| new / enable / configure_scan / start_scan — CLDO bus clock + soft-reset gate (cldo_crg_rst_soft_cfg_1 @ 0x4400_1138, soft_rst_lsadc_n bit5 / soft_rst_lsadc_bus_n bit7) | A | doc-and-guard |  | ✅ |
| configure_scan / start_scan / read_sample / read_async — AFE analog power-up (da_lsadc_rwreg_1..3 simu writes + staged da_lsadc_en ramp + tcxo delays + offset/cap/gain cal) | A | doc-and-guard |  | ◐ |
| set_analog_enable — bits (da_lsadc_en, 16-bit, overwrite semantics) | C | no-change | Y | ✗ |
| enable_cic_filter — oversampling_ratio (cic_osr 8-bit) + cic_filter_en | B | no-change | Y | ✗ |
| set_offset / set_gain — offset (cfg_offset 16-bit) / gain (cfg_gain 16-bit) | C | no-change |  | ✗ |
| configure_scan — channel enable bitmask (implied GPIO pad provisioning, GPIO_07..GPIO_12 for ch0..5) | A | doc-and-guard |  | ◐ |
| read_async / data_ready — conversion-complete wait (no timeout) | C | result | Y | ✅ |

**头部草图 — configure_scan — AdcConfig.sample_count (sample_cnt, 5-bit)**（accept count <= 31, reject count >= 32. Bound = (1<<5)-1 = 31, from PAC SampleCntW=FieldWriter<5> (lib.rs:21283) and vendor adc_ctrl_data.sample_cnt:5 (regs_def）：

```rust
/// Scan sample count (`sample_cnt`, 5-bit hardware field). Construct-time
/// validated: an out-of-range value is rejected, never silently masked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleCount(u8);
impl SampleCount {
    /// Vendor `hal_adc_auto_scan_mode_set` default.
    pub const DEFAULT: SampleCount = SampleCount(0x8);
    /// `count` must be `0..=31` (5-bit field). Returns `None` otherwise.
    pub const fn from_count(count: u8) -> Option<Self> {
        if count <= 31 { Some(SampleCount(count)) } else { None }
    }
    pub const fn count(self) -> u8 { self.0 }
}
// AdcConfig.sample_count: u8  ->  SampleCount
// …（完整见附录 config-tightening-designs.json）
```

### TCXO

_The tcxo driver has NO numeric Config struct: new() takes only the Tcxo<'d> token and the count registers are read-only, so there is no caller-supplies-an-overflowing-value newtype surface like PWM's Duty/PwmPeriod. The defects are (1+2) class-C unenforced init preconditions — new() neither self-enables the free-running counter (tcxo_status.enable, bit 2) nor clears it (bit 1), so 'construct -> running, zero-based counter' does NOT hold the way PWM's configure self-enables its clock tree; (3) a class-A read-path width hazard — read_counter/read_counter32 use raw .bits() (full u32) on each count register instead of the PAC's 16-bit-masked .countN() accessor, so if silicon leaves bits[31:16] non-zero the upper half of one lane bleeds into the next; (4+5) two class-C handshake gaps — a 100-iteration poll budget justified by a wrong 32kHz figure (TCXO actually runs 24/40MHz) and a refresh/valid handshake that never re-arms the valid flag, risking a sticky stale read. The dominant, type-expressible fix is to make new() self-enable+clear the counter (the actual run-gate, since TCXO has no separate CKEN register) so the counter is guaranteed running by construction; the read-path width fix is a one-line switch to .countN(); the timeout/race fixes are doc-and-guard plus a saner budget. OPEN QUESTIONS needing on-board measurement: (a) do count bits[31:16] read as zero on silicon (decides whether the .countN() mask is cosmetic or load-bearing); (b) does tcxo_status.valid auto-clear on a new refresh request or is it sticky (decides whether a re-arm is mandatory); (c) the true latch latency at 24/40MHz to size the poll budget. The connected WS63 HIL suite can drive register-level checks for all of these (read raw tcxo_countN bits[31:16] post-refresh; observe valid before/after a refresh; time the latch) so the chosen bounds can be validated and the doc-and-guard items upgraded to hard guarantees once measured._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| new() — counter-enable precondition (tcxo_status.enable, bit 2 / TCXO_EN_BIT) | C | type-state | Y | ✅ |
| new() — counter-clear precondition (tcxo_status.clear, bit 1 / TCXO_CLEAR_BIT) | C | type-state | Y | ✅ |
| read_counter / read_counter32 — count0..count3 read width (.bits() u32 vs 16-bit count field) | C | result |  | ✅ |
| refresh — valid-flag poll timeout (100 iterations; comment claims 32kHz, real TCXO is 24/40MHz) | C | newtype |  | ✅ |
| refresh — request bit not self-cleared / valid flag not re-armed before poll (stale-read race) | A | doc-and-guard |  | ◐ |

**头部草图 — new() — counter-enable precondition (tcxo_status.enable, bit 2 / TCXO_EN_BIT)**（Self-enable is unconditional: new() writes (status | TCXO_EN_BIT) where TCXO_EN_BIT = 1<<2 (tcxo_status.enable bit 2, regs_def.h:53), matching vendor hal_tcxo_v）：

```rust
// Make 'construct -> running counter' hold the way PWM's configure self-enables
// its clock tree. TCXO has NO separate CKEN register (notes: gated solely by
// tcxo_status.enable), so self-enabling in new() IS the clock-gate change.
//
// Phantom type-state encodes whether the run-gate is up, so only a Running driver
// exposes read_counter(). The default constructor self-enables (matches vendor
// hal_tcxo_init order: set_enable THEN set_clear).
pub struct Enabled;   // counter free-running (enable bit set)
pub struct Disabled;  // explicit low-power / parked state

pub struct TcxoDriver<'d, S = Enabled> {
    _tcxo: Tcxo<'d>,
    _state: PhantomData<S>,
}
// …（完整见附录 config-tightening-designs.json）
```

### RTC

_The RTC's deploy-blocking defect is class-C but UNLIKE PWM/I2S it is NOT a clock-gate the driver can flip itself: the v100 RTC at 0x5702_4000 is an AON block with no CKEN bit anywhere (absent from clock.rs Peripheral enum, rtc_porting.c has no enable sequence), driven directly by the external 32.768 kHz crystal. So "self-enable its own clock gate" does NOT apply here — there is nothing to gate; the only honest fix for the missing clock is to move construction behind a precondition the BSP must vouch for. The tightening therefore is: (1) a doc-and-guard fallible constructor `RtcDriver::new(rtc) -> Option<Self>` returning Some only after a liveness probe (two current_value reads with a bounded settle), plus an `unsafe new_assume_clocked` escape hatch for BSPs that already know the crystal is populated — types cannot express "a 32 kHz crystal is soldered on", so a guard + Option is the correct tool; (2) collapse RtcMode to drop the unattested Periodic/mode=1 variant down to a single FreeRunning (enum-with-one-value / or remove the mode arg) since the vendor never programs mode=1; (3) a validated `RtcReload(u32)` newtype that is non-zero (and a `try_from_duration`/`try_from_ms` Option ctor) so load_value=0 is unrepresentable, mirroring PwmPeriod; (4) make `set_load` stop->write->restart internally (bracket the reload like every vendor path) so the live-reload glitch precondition is enforced, not documented. The doc-fiction RTC_COUNTER_WIDTH=48 / module doc "48-bit" stays a comment fix only — the registers and the u32 API width are correct. OPEN QUESTION needing on-board measurement: every behavioral claim (does mode=1 do anything; does a live set_load glitch; does the bus actually stall on the unpopulated EVB; what counter width silicon really exposes) is inferred from the vendor SDK + bring-up notes — none can be confirmed until an EVB with the 32.768 kHz crystal populated is available, because on THIS board any RTC register access hangs the core, so even the liveness probe in (1) can itself hang and may need a watchdog/timeout backstop at the BSP level._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| RtcDriver::new(rtc) — construction / clock-source (32.768 kHz crystal) precondition (defect class C) | C | doc-and-guard | Y | ◐ |
| RtcDriver::configure(mode, ..) — mode = RtcMode::Periodic (control bit 1 = RTC_MODE_USER_DEF, unattested) (defect class B) | C | enum | Y | ◐ |
| RtcDriver::configure / set_load — load_value = 0 silently accepted (defect class D) | C | newtype | Y | ✅ |
| RtcDriver::set_load — reload written to a live (enabled) counter without stop/reload bracketing (defect class C) | C | doc-and-guard |  | ◐ |
| Module doc + RTC_COUNTER_WIDTH=48 (soc/ws63.rs:150, rtc.rs module doc) — '48-bit counter' doc fiction | C | doc-and-guard |  | ◐ |

**头部草图 — RtcDriver::configure / set_load — load_value = 0 silently accepted (defect class D)**（ACCEPT count in 1..=u32::MAX (RTC_LOAD_COUNT is a real 32-bit reg). REJECT count == 0. For try_from_ms: ACCEPT iff 1 <= ms*32768/1000 <= u32::MAX, else None (su）：

```rust
// Mirror PwmPeriod: a validated non-zero reload count, so a 0 reload (degenerate
// immediate-expiry counter the vendor rejects with ERRCODE_INVALID_PARAM) is
// UNREPRESENTABLE rather than silently programmed. RTC_LOAD_COUNT is a genuine
// full-width 32-bit reg (PAC Ux=u32), so the inner type is u32 (no truncation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RtcReload(u32);

impl RtcReload {
    /// Wrap a raw reload count in 32.768 kHz ticks. `None` for 0.
    pub const fn from_count(count: u32) -> Option<Self> {
        if count == 0 { None } else { Some(RtcReload(count)) }
    }

    /// Derive the reload from a millisecond interval. `None` when the interval is
// …（完整见附录 config-tightening-designs.json）
```

### GADC（BS2X）

_The gadc driver's danger is concentrated in two unenforced-precondition (class C) surfaces and one fragile-default (class B) surface, plus a measured-width (class B) question on the result layout. Top priority is the UNBOUNDED done-poll in convert_once (gadc.rs:199-201): a fully-valid read(Ain0) can deadlock the core forever, and the vendor v154 pattern (`COUNT_THRESHOLD 1000` bounded loops, hal_adc_v154.c:33) is the exact fix — bound the loop and return Result<i32, GadcError>, making read() and a new try_read() fallible. Second, new() advances on bare delay_us() spins and never reads the RO ack bits it documents (pwr_ack PMU_AFE_DIG_PWR_EN[2], done PMU_AFE_GADC_CFG[4], gadc.rs:36-37); it should become `try_new() -> Result<Self, GadcError>` that polls those bits with the same bounded loop and self-enables/acks its own AFE bring-up (the class-C "construct -> live" guarantee, mirroring PWM's enable_pwm_clock). The delay_us 64 MHz assumption (gadc.rs:81) should route through the existing tcxo-based delay like the vendor's uapi_tcxo_delay_us instead of a CPU-cycle spin. cfg_clk_div_0=0x27 (prechg_div=0) becomes a validated GadcClkDiv newtype with a non-zero-prechg predicate. The 18-bit-signed-at-bit-17 result width and the exact PMU ack-bit registers are NOT verifiable in this checkout (fbb_bs2x v153 absent) and CANNOT be validated on the connected WS63 HIL rig — gadc is `#[cfg(feature="chip-bs21")]` (lib.rs:101) and the WS63 board runs chip-ws63, so every on-silicon claim here needs a BS2X board; host proptests can only validate the pure newtype/sign-extend logic. OPEN QUESTION needing on-board (BS2X) measurement: (a) does rpt_gadc_data_3 bit0 actually assert within COUNT_THRESHOLD iterations under a correct bring-up, (b) the true accumulator width / sign-bit position of rpt_gadc_data_2, (c) the COMMON_DEFAULT cfg_clk_div_0 value and whether prechg_div really must be non-zero, (d) the precise PMU ack-bit register offsets._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| convert_once done-poll on rpt_gadc_data_3 bit0 (reached via read()) | C | result | Y | ✗ |
| new() power-up: poll PMU_AFE_DIG_PWR_EN pwr_ack (RO bit2) and PMU_AFE_GADC_CFG done (RO bit4) instead of bare delays; make construction fallible + self-enabling | C | result | Y | ✗ |
| delay_us core-frequency assumption (us * 64) used for all analog settling + the convert trigger delays | C | doc-and-guard |  | ◐ |
| cfg_clk_div_0 = 0x27 magic (ana_div_th[11:0] / prechg_div_th[23:12], 12-bit each) | C | newtype |  | ✗ |
| rpt_gadc_data_2 result: 18-bit sign-extend-at-bit-17 vs measured accumulator width | C | doc-and-guard |  | ✗ |
| absolute base addresses GADC_BASE / PMU_AFE_BASE / AON_AFE_ISO + chip gating of the whole module | C | doc-and-guard |  | ✗ |

**头部草图 — cfg_clk_div_0 = 0x27 magic (ana_div_th[11:0] / prechg_div_th[23:12], 12-bit each)**（Accept iff ana_div <= 0x0FFF AND prechg_div <= 0x0FFF AND prechg_div != 0. Field widths 0:11 and 12:23 (12-bit each) are confirmed from the PAC (cfg_clk_div ana）：

```rust
/// Validated GADC common clock divider for cfg_clk_div_0. Two 12-bit fields:
/// ana_div_th[11:0] and prechg_div_th[23:12] (PAC widths confirmed: both 12-bit).
/// A zero prechg divider produces no usable conversion clock (feeds the bounded
/// done-poll), so the constructor rejects prechg_div == 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GadcClkDiv(u32);

impl GadcClkDiv {
    /// UNVERIFIED vendor default placeholder (was raw 0x27 = ana_div 0x27,
    /// prechg_div 0). Marked needs-board-confirmation: COMMON_DEFAULT not in this
    /// checkout. Provided as a *validated* value, not a bare magic literal.
    pub const COMMON_DEFAULT: GadcClkDiv = GadcClkDiv(0x27);

    /// Build from the two 12-bit dividers. Returns None if either exceeds 12 bits
// …（完整见附录 config-tightening-designs.json）
```

### BS2X HID

_All three drivers are BS2X-only, inherent-method drivers with ZERO embedded-hal trait impls (verified: no `embedded_hal`/`embedded_io` reference in keyscan.rs/pdm.rs/qdec.rs), so every config tightening is free to type and provably cannot break any trait surface. The ONE true register-field overflow (defect A) is Keyscan::new(rows, cols): `rows.saturating_sub(1)` -> 5-bit row_pin_en (PAC RowPinEnW=FieldWriter<5>, lib.rs:24244) and `cols.saturating_sub(1)` -> 3-bit clo_pin_en (CloPinEnW=FieldWriter<3>, lib.rs:24240), both masked silently by FieldWriter::bits(); fix with two validated newtypes RowCount(1..=18) and ColCount(1..=8) whose `from_count` returns None, so the silent wrap becomes an un-constructable value — directly mirroring PwmPeriod/Duty. The dominant theme is the broken "construct -> clocked" contract that PWM honors: ALL THREE fail to self-enable their clock tree (defect C) and ALL THREE skip the vendor scan/sample/filter config (defect B). The faithful fix folds the vendor port functions into new(): keyscan pinmux+scan-timing, pdm_port_clock_enable (CRG glb gate 0x52000548 bit10 + clk-src mux 0x5200004c bit1 + 7 datapath sub-clocks), qdec_clk_sel_set (M_CTL 0x520004A0 clk-enable + sampleper) + qdec pinmux — implemented as private `enable_*_clock()`/`configure_*()` helpers exactly like pwm.rs `enable_pwm_clock()`. PDM/QDEC take no caller numerics today (gains/modes/thresholds hardcoded and in-range), so they need no newtype — only the clock/config self-enable. OPEN HARDWARE QUESTIONS for the WS63 HIL suite (it is register/poll-level, and these are BS2X parts so full HIL is partial at best): (1) confirm the silicon row maximum is truly 18 and column max truly 8 before freezing the newtype bounds — the PAC width (5-bit/3-bit) exceeds the vendor "valid 0-17 / 0-7", and per the PWM pwm_freq_h lesson the type must encode MEASURED reality not the regs_def; (2) confirm CRG bits (0x52000548 bit10, 0x5200004c bit1) and M_CTL 0x520004A0 clk-enable actually latch and yield a non-empty UP-FIFO / moving qdec_acc — needs a BS2X board + DMIC/encoder, which the current WS63 HIL rig cannot fully cover._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| Keyscan::new(rows, cols) — cols argument -> clo_pin_en (3-bit field) | A | newtype | Y | ◐ |
| Keyscan::new(rows, cols) — rows argument -> row_pin_en (5-bit field) | A | newtype | Y | ◐ |
| Keyscan::new — GPIO pin mux (HAL_PIO_KEY_SCAN) + row pull-down/col pull-up + KEYSCAN_PIN_SEL_ROW/COL routing (defect C: dead output path) | A | doc-and-guard |  | ◐ |
| Keyscan::new — scan timing: pulse_time / scan_mode / wait_time / idle_time / defence / io_de (defect B: under-configured, no working scan) | A | doc-and-guard |  | ◐ |
| Pdm::new — PDM clock-source mux (0x5200004c bit1, XO sel) + CRG global gate (0x52000548 bit10, glb_clken_pdm) (defect C: whole block unclocked) | C | doc-and-guard |  | ◐ |
| Pdm::new — per-stage datapath clocks: cic_clken_0 / srcdn_clken_0 / hpf_clken_0 / compd_clken_0 / ckdiv_6144k_clken / saradc_clken / clk_freq_sel (defect B: no decimation / no DMIC bit clock) | C | doc-and-guard |  | ◐ |
| Pdm::new — AFE/DMIC pad provisioning; cic_gain (8-bit, const 0x14) / srcdn_mode (2-bit, const 0) hardcoded, no caller config surface | C | no-change |  | ✗ |
| Qdec::new — QDEC clock select+enable (M_CTL_QDEC_CLK_CTL 0x520004A0: set QDEC_CLK_EN_BIT, freq divider) + sample period (M_CTL_QDEC_SAMPLEPER) (defect C: decoder never samples, read_count() stuck at 0) | C | doc-and-guard |  | ◐ |
| Qdec::new — A/B GPIO pin mux (HAL_PIO_QDEC_A/B) + input dir + pull-up (defect C: dead input, no gray-code transitions) | C | doc-and-guard |  | ◐ |
| Qdec::new — sample period / debounce (defen_en/defen_num) / report period / LED config (hal_qdec_v150_init) (defect B: unreliable decode, bounce double-counts) | C | doc-and-guard |  | ◐ |

**头部草图 — Keyscan::new(rows, cols) — cols argument -> clo_pin_en (3-bit field)**（ACCEPT cols in 1..=8 (vendor KEYSCAN_MAX_COLUMN=8 / regs_def clo_pin_en valid 0-7); REJECT (None) for 0 and for >=9. field() = count-1 is always in 0..=7 so the）：

```rust
/// Number of key-matrix COLUMNS, validated 1..=8 at construction.
/// The IP programs `count - 1` into the 3-bit `clo_pin_en` field; a count > 8
/// would silently mask (cols=9 -> 8=0b1000 & 0b111 = 0 -> 1 column). This newtype
/// makes that wrap UNREPRESENTABLE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColCount(u8);
impl ColCount {
    /// KEYSCAN_MAX_COLUMN = 8 (vendor keyscan.c:54). Accepts 1..=8.
    pub const fn from_count(n: u8) -> Option<Self> {
        if n >= 1 && n <= 8 { Some(ColCount(n)) } else { None }
    }
    /// The value written to the 3-bit clo_pin_en field, always 0..=7.
    pub const fn field(self) -> u8 { self.0 - 1 }
    pub const fn count(self) -> u8 { self.0 }
// …（完整见附录 config-tightening-designs.json）
```

### 杂项 sweep

_Eight surfaces need tightening; the unifying defect is "structurally valid but unrunnable on silicon" config that compiles. The two highest-leverage fixes are (1) make Delay carry the live clock instead of the const SYSTEM_CLOCK_HZ (the same Delay times an IRREVERSIBLE eFuse program pulse, so a wrong clock burns the OTP wrong), and (2) replace the free-u8 calibration setters (EfuseClkPeriod, FroDivCnt) with TcxoFreq-derived / range-checked newtypes — exactly the PWM precedent, reusing the EXISTING clock_init::TcxoFreq enum and SystemClocks struct rather than inventing new ones. system.rs must be #[cfg(chip-...)]-gated (its WS63 absolute MMIO is silently wrong on BS2X). TRNG gets a vendor-equivalent value/duplicate quality gate plus an enum for the sample-clock source. usb.rs/pke.rs/spacc.rs/km.rs are no-change (no writable-but-unrunnable surface today; the pke/spacc empty clock-enable is a latent class-C only once registers are actually written). OPEN MEASUREMENTS the types cannot settle and that gate the exact bounds: (a) the 3-cycles/loop Delay calibration and PLL-locked base must be scoped on 240 MHz WS63, 64 MHz BS21, and 40 MHz-crystal boards; (b) whether efuse access truly fails at clk_period=0 vs merely degrades; (c) the BS2X chip-reset / reset-record register addresses; (d) the minimum usable fro_div_cnt and whether an external TRNG sample clock is ever physically routed. The HIL suite (tests/hil.rs + hil/hil-smoke.sh) can validate every register-level fix (clk_period readback, fro_div behavior, reset-reason record, address-span rejection) but cannot by itself prove Delay wall-clock accuracy without a scope/GPIO-toggle measurement, and cannot prove the eFuse burn (irreversible — must stay experimental)._

| 字段/面 | 类 | 方案 | brk | 上板 |
|---|---|---|---|---|
| Delay::new (delay.rs:14) — const ZST with no clock argument; busy-loop derived from const SYSTEM_CLOCK_HZ assuming PLL locked | C | newtype | Y | ◐ |
| Delay::delay_micros (delay.rs:19-27) — cycles_per_loop=3 hardcoded, body=spin_loop() with no guaranteed cycle cost | C | doc-and-guard |  | ✗ |
| EfuseDriver::set_clock_period (efuse.rs:85-89) — period: u8 accepts any value incl. reset 0; must match live TCXO (0x29@24MHz / 0x19@40MHz) | C | newtype | Y | ✅ |
| EfuseDriver::read_buffer (efuse.rs:119-124) — address = start_byte + i as u16 wraps/panics; only per-byte <256 guard, no up-front span check | C | result |  | ✅ |
| EfuseDriver::write_byte (efuse.rs:133-149) — AVDD settle via CPU busy-loop Delay::new(); wrong clock under/over-burns an IRREVERSIBLE fuse; no verify | C | doc-and-guard | Y | ✗ |
| TrngDriver::set_divider (trng.rs:119-123) — div: u8 accepts 0/1 (FRO sampled too fast => no entropy => permanent Timeout); working value 0x1b | B | newtype | Y | ◐ |
| TrngDriver::set_sample_clock (trng.rs:109-113) — external: bool; selecting external with no board-routed clock => FRO never sampled => permanent Timeout | C | enum | Y | ◐ |
| TrngDriver::read / read_blocking / fill_bytes / fill_words (trng.rs:54-104) — raw FIFO passthrough; no vendor quality gate (rejects 0x0/0xFFFFFFFF/duplicate) | D | result |  | ◐ |
| System::software_reset / software_reset_cpu / reset_reason (system.rs:42-97) — hardcoded WS63 absolute MMIO (0x4000_2110/_00A0/_00A4), NO chip gating; wrong/harmful on BS2X | C | doc-and-guard |  | ✅ |
| usb.rs Usb::new / device_enumerate (no caller-writable config; devspd=0/devaddr=0 hardcoded valid; speed_from_enumspd total) | D | no-change |  | ✗ |
| pke.rs / spacc.rs enable/disable/is_busy (stubs: empty/return-false no-ops, no register writes, no config fields) |  | no-change |  | ✗ |
| km.rs is_keyslot_locked / lock_keyslot (slot: u8 guarded by assert!(slot < KEYSLOT_COUNT=8); 10-bit slot fields) |  | no-change |  | ✅ |

**头部草图 — Delay::new (delay.rs:14) — const ZST with no clock argument; busy-loop derived from const SYSTEM_CLOCK_HZ assuming PLL locked**（No reject predicate (any Hz is a legal clock); the tightening is that the clock is an explicit construction input, not an implicit const. from_clocks selects c.）：

```rust
// Carry the live clock at construction instead of reading the const.
// Reuse the EXISTING clock_init::{TcxoFreq, SystemClocks} — do NOT invent a new clock type.
pub struct Delay { hz: u32 }
impl Delay {
    /// Build from a measured/resolved clock (probe_clocks() / init_clocks() result).
    /// pll_locked==false => CPU runs from TCXO, so use tcxo_freq, not SYSTEM_CLOCK_HZ.
    pub const fn from_clocks(c: &crate::clock_init::SystemClocks) -> Self {
        let hz = if c.pll_locked { c.cpu_clk } else { c.tcxo_freq.hz() };
        Self { hz }
    }
    /// Escape hatch with an explicit, caller-asserted Hz (40 MHz-crystal / XIP boot).
    pub const fn from_hz(hz: u32) -> Self { Self { hz } }
    /// Back-compat: assume the datasheet PLL clock. Deprecated, name the risk.
    #[deprecated = "assumes PLL locked at SYSTEM_CLOCK_HZ; use from_clocks(&probe_clocks())"]
// …（完整见附录 config-tightening-designs.json）
```

## 3. 横切主题

- **驱动自起时钟门（C 类，49 面里的主力）** — `configure`/`new` 内部照搬 vendor `*_porting` 的 CKEN(+DIV_CTL 分频 + LOAD_DIV)序列，「构造即供时钟」。PWM 已做；需补的（有 C 类时钟面）：spi, i2c, uart, i2s, wdt, timer, gpio, dma, clock, sfc, lsadc, tcxo, rtc, gadc, bs2x-hid, misc-sweep。SPI 已自起（CKEN_CTL1 bit25），可作第二参考。WS63 时钟复位默认开，但 PWM/I2S 等仍需显式 latch。

- **I2S type-state Master/Slave** — `I2s::new_master(cfg_with_dividers)` 强制非零 BCLK/FS 分频，`new_slave()` 不要；零分频 Master 类型上不可表达（治 B）。把现状两步 `new()`+`configure()` 合并进构造。

- **频率 newtype：各驱动独立**（`SpiHz`/`I2cHz`/`BaudRate`/`PwmPeriod`），不做统一 `Hertz` —— 各自时钟源(SPI 160M PLL、I2C 24M TCXO、UART 160M、PWM clk/6)和可达分频范围不同，校验谓词不同。底层可共用一个 `u32` 单位 newtype，但校验各自实现。

## 4. 破坏性与迁移（一次性 0.5.0 minor，semver-checks 把关）

```rust
// SPI / UART：Config 字段换 newtype
- Config { frequency: 1_000_000, mode, data_bits: 8 }
+ Config { frequency: SpiHz::try_from_hz(1_000_000).unwrap(), mode, data_bits: DataBits::Eight }
// I2C：构造参数换 newtype（WS63 对齐 BS2X 的 Speed）
- I2c::new_i2c0(p, 400_000)
+ I2c::new_i2c0(p, I2cHz::try_from_hz(400_000).unwrap())
// I2S：两步合一 + 角色 type-state
- let mut i2s = I2sDriver::new(p); i2s.configure(&cfg);
+ let i2s = I2s::new_master(p, MasterConfig{ bclk_div, fs_div, .. });
```

## 5. 上板验证计划

全部面：✅ 可上板 **35** · ◐ 部分(寄存器级) **59** · ✗ 不可 **29**。寄存器/轮询级(SPI/I2C/UART/I2S/WDT/Timer/PWM 的分频/位段 latch + 越界拒绝)走 HIL 套件确认；**✗ 的主要是 RTC(板上无 32k 晶振) / GADC(AFE 上电) / 纯模拟**，类型+文档兜底,不上板。每驱动收紧后跑 `tests/hil.rs`。

## 6. 待上板测量的开放点

PWM `freq_h` 那一类「手册宽于硅片」的字段：定 newtype 位宽**前**要先上板量。逐驱动审计里凡 evidence 标注位宽/范围存疑的(尤见 SPI `data_bits` 实际 DFS 上限、I2C SCL 计数位宽、各分频 LOAD 行为)都列为开放点 —— 实现该字段前先量一次,别只信 PAC/SDK。完整逐条见附录 JSON 的 audits。

## 7. 实现顺序（docs-first：每驱动先改 handbook 组件页，再写码+测试+上板）

1. **PWM ✅** 参考。2. **频率三连 SPI→I2C→UART**（同 newtype 范式）。3. **I2S type-state**（Master/Slave + 两步合一）。4. **WDT/Timer**（饱和→Result、`start_us` 溢出）。5. **GPIO 清债**（删 `GpioPin<MODE>` + Flex 显式转换 + `degrade()`）。6. **构造清债**（bus builder-lite + 裸标量→newtype）。7. **DMA 完整**（owned-buffer `Transfer` guard + 缓存进类型 + **接进 SPI/I2C/UART**）。8. **中断路由**（hisi-riscv-rt 具名 handler + HAL `on_interrupt` 钩子，干掉硬编码 `mcause==26`）。9. **RTC/GADC/LSADC**（doc+守护 + 有界轮询）。10. **机械批**（docs.rs metadata / rust-version / `non_exhaustive` / defmt / 命名 / `reborrow()` / 删残留 marker）。11. **drop-to-disable（方案 B）**：危险态驱动 scoped `Drop` + 逃生口（见 §8）。

## 8. drop-to-disable（方案 B，已定 2026-06-15）

只给「句柄丢了还在干危险事」的驱动实现 `Drop` 让其安静化，**不全量 RAII**（时钟复位默认开，Drop 不省电，只为危险态兜底）：

- **`Wdt`**：drop 默认停表（否则到点复位芯片）；给**显式逃生口** `into_armed(self)` / `leak()` 声明「我要它 drop 后继续 armed」——「继续跑」是显式选择，不是默契。
- **`PwmChannel`**（在驱动）：drop 默认 disable 输出；给 `into_running()` 逃生口（背光/蜂鸣器等故意常驻）。
- **`Output`**（拉高/拉低）：drop 回安全默认（高阻/输入），避免一直拉着某条 reset/enable 线。
- **`Timer`**：作中断源常驻是常见正当用法，**不**纳入默认 Drop，保持显式 `disable()`。

实现注意：no-atomic 单 hart，`Drop` 里**只关本外设 en 位、不碰共享时钟门**（避免误关到别的驱动）；逃生口用「消费 `self` 返回一个无 `Drop` 的 marker 类型」表达。
