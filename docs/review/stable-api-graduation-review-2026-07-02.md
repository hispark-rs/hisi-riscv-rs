# hisi-riscv-hal 0.6.0 stable API graduation review

> Date: 2026-07-02
> Scope: documented STABLE public API in `AGENTS.md` and `docs/src/explanation/policies/02-stable-unstable.md` after the P0 typed-identity/gating pass.
> Method: adversarial multi-agent review across HIL coverage, gating/export hygiene, typed-config, safe/unsafe encapsulation, DMA, peripheral groups, miscellaneous drivers, and docs/report readiness.
> Rule: an API graduates only when it has named WS63 HIL evidence, typed/sound safe inputs, no unstable leakage, and no known safe->unsafe soundness blocker.

## 0. Executive Decision

**Do not graduate the currently documented stable surface as a blanket public API.**

The review found that the docs now list a broader stable surface than the evidence and soundness state justify. Several narrowly-tested sub-surfaces can graduate, but many modules need one of these actions before release:

- Gate the unproven or unsound public items behind `unstable`.
- Add named WS63 HIL tests and move those tests out from `feature = "unstable"` when graduating.
- Tighten typed config / operational `Result` handling.
- Fix safe/unsafe blockers, especially DMA lifetime/cache/cancellation issues.

Immediate hygiene fixes made during this review:

- `prelude` no longer exposes `delay::Delay` or `rtc::RtcDriver` unless `unstable` is enabled.
- `UartDmaError` is now unstable-gated with the rest of `UartDma`.
- `private` is crate-internal again, so sealed traits are not externally implementable.
- The unused vestigial `DmaWord` marker was removed.
- WS63-only async IRQ/DMA glue is now cfg-gated as `chip-ws63 + async`, so `chip-bs21 + async` no longer compiles WS63 interrupt names.

## 0.1. Remediation Status After The Narrow-Stable Pass

The release posture was changed from "graduate the broad documented surface" to
"publish only the scoped default-stable subset; keep the rest behind `unstable`".
Implemented after this review:

- Public `dma` is now module-gated behind `unstable` as a whole, including mem-to-mem, typed channel tokens, peripheral-paced DMA, and async DMA hooks. This avoids default exposure while cache-line ownership/alignment, timeout quiescence, async cancellation, and SPI1/UART DMA evidence remain open.
- `asynch` and `embassy` are hard-gated behind `unstable`; GPIO wait, timer async delay, UART async I/O, DMA async, and LSADC async require `async + unstable`. SPI/I2C blocking-backed async trait impls remain available with `async` alone.
- UART raw `Config::clock_hz` was replaced by typed `Config::clock: UartClock` with `Pll`/`Boot` choices.
- GPIO `OutputConfig::open_drain` and `with_open_drain` were removed instead of preserving a no-op stable knob.
- PWM `SetDutyCycle` now returns `PwmError::DutyOutOfRange` when `duty > max_duty_cycle()`.
- I2C operations reject `addr > 0x7f` with `I2cError::InvalidAddress`.
- eFuse default-stable surface was narrowed to automatic clock setup plus `read_byte`; manual clock period, `read_buffer`, and `write_byte` are unstable.
- `System::software_reset*`, `Instant::now`/`elapsed`, interrupt priority/threshold getters/setters, SFC pad config, broad I2S data/FIFO/IRQ methods, broad LSADC analog/conversion/filter/calibration/data-path methods, broad TSENSOR config/interrupt/blocking-read methods, and TRNG manual clock/divider/status controls are unstable-gated.
- `prelude` re-exports now follow the underlying gates, including DMA and untested drivers.

This remediation does **not** claim the blocked APIs are sound or graduated. It
reduces the default stable API to the scoped surfaces that can be defended for
0.6.0 and leaves the blockers below as the graduation backlog.

## 1. Review Inputs

Agents/workflows used:

- HIL coverage audit: mapped each documented stable group to `tests/hil.rs` evidence.
- Gating/export audit: checked `lib.rs`, `prelude.rs`, module gates, and unstable leaks.
- Typed-config audit: searched for raw identity/config inputs, silent clamp/truncate/wrap, and dead combos.
- Safe/unsafe audit: searched stable safe APIs for violated unsafe preconditions.
- DMA adversarial audit: focused on owned buffers, cache, timeout/drop, and peripheral-DMA.
- Peripheral group audit: GPIO/IO_CONFIG, Timer/Interrupt/Async, UART, PWM/eFuse.
- Misc driver audit: clock/system/peripherals/time/wdt/trng/tcxo/cache/i2c/i2s/lsadc/tsensor.
- Docs/report audit: checked policy consistency and report requirements.

Local commands run:

```bash
bash .agents/skills/safe-unsafe-verify/verify.sh --audit-only
rg -n '#\[instability::unstable\]|unstable_module!|unstable_driver!' crates/hisi-riscv-hal/src
cargo check -p hisi-riscv-hal --no-default-features --features chip-ws63 --target riscv32imfc-unknown-none-elf
cargo check -p hisi-riscv-hal --no-default-features --features chip-ws63,unstable --target riscv32imfc-unknown-none-elf
cargo check -p hisi-riscv-hal --no-default-features --features chip-ws63,async --target riscv32imfc-unknown-none-elf
cargo check -p hisi-riscv-hal --no-default-features --features chip-ws63,async,unstable --target riscv32imfc-unknown-none-elf
cargo check -p hisi-riscv-hal --no-default-features --features chip-ws63,async,embassy --target riscv32imfc-unknown-none-elf
cargo check -p hisi-riscv-hal --no-default-features --features chip-ws63,async,embassy,unstable --target riscv32imfc-unknown-none-elf
cargo check -p hisi-riscv-hal --no-default-features --features chip-bs21 --target riscv32imfc-unknown-none-elf
cargo check -p hisi-riscv-hal --no-default-features --features chip-bs21,async --target riscv32imfc-unknown-none-elf
```

The unsafe-readiness baseline was written to `docs/review/unsafe-audit-2026-07-02.md`.

## 2. Release-Blocking Findings

| Severity | Finding | Evidence | Required action |
|---|---|---|---|
| Critical | `SpiDma::transfer_dma` can return timeout while DMA channels remain active, dropping buffers/tokens while hardware may still own addresses. | `crates/hisi-riscv-hal/src/spi.rs` timeout path around `transfer_dma`; previous design warned this exact class in `docs/review/peripheral-dma-design-0.5.1.md`. | Keep peripheral-DMA blocking transfer unstable or implement cancel-then-quiesce before returning. |
| Critical | `SpiDma::write_dma_async` is cancellation-unsafe; dropping the future can drop buffer/channel while DMA is live. | `crates/hisi-riscv-hal/src/spi.rs` async path. | Keep async DMA unstable or replace with owned guard/future whose `Drop` quiesces DMA. |
| High | Safe DMA RX paths invalidate arbitrary cache lines without enforcing cache-line aligned owned buffers. | `crates/hisi-riscv-hal/src/cache.rs` warns partial-line invalidation danger; stable DMA APIs accept arbitrary `WriteBuffer`. | Enforce alignment with typed DMA buffers or runtime rejection, or keep these APIs unstable. |
| High | Stable `SpiDma` overclaims SPI1, whose DMA request mapping is explicitly unverified. | `crates/hisi-riscv-hal/src/dma.rs` comments on `Spi1Tx/Rx`; HIL covers SPI0 only. | Gate SPI1 DMA or add SPI1 HIL and mapping proof. |
| High | `asynch::block_on` / `IrqSignal` have lost-wake races. | `crates/hisi-riscv-hal/src/asynch.rs` no-op waker and `take_fired()` then `register()` pattern. | Fix wake registration/critical section before graduating async infrastructure. |
| High | UART `Config::clock_hz` is raw and can produce divider 0 or silently truncate incompatible clock/baud pairs. | `crates/hisi-riscv-hal/src/uart.rs` `Config::clock_hz` and divider programming. | Replace with validated clock/baud pair or reject invalid config. |
| High | GPIO `OutputConfig::open_drain` is accepted but not programmed. | `crates/hisi-riscv-hal/src/gpio.rs` config and `init_output`/`init_flex`. | Implement open-drain or remove/gate field. |
| High | PWM public surface exposes Ch0..Ch7, but clock/divider bring-up and HIL cover Ch0 only. | `crates/hisi-riscv-hal/src/pwm.rs`; HIL `pwm_configure_and_enable` uses Ch0. | Gate unproven channels or add HIL/clock proof per channel group. |
| High | PWM `SetDutyCycle` accepts out-of-range duty with `Infallible`. | `crates/hisi-riscv-hal/src/pwm.rs` trait impl. | Return an error for `duty > max_duty_cycle()` per embedded-hal semantics. |
| High | I2C transaction APIs accept invalid raw 7-bit addresses and lack transaction HIL. | `crates/hisi-riscv-hal/src/i2c.rs`; HIL is config-only. | Reject `addr > 0x7f` and add transaction/NACK HIL, or gate operations. |
| High | TSENSOR masks raw config values and exposes broad un-HIL config. | `crates/hisi-riscv-hal/src/tsensor.rs` mode/threshold setters. | Type thresholds/modes or gate config methods until HIL-covered. |
| High | LSADC docs/code still expose raw analog/filter/calibration knobs with only config HIL. | `crates/hisi-riscv-hal/src/lsadc.rs`; HIL only scan config. | Gate analog/conversion/config subsets or add full power/conversion HIL. |
| High | `Peripherals` stable tokens expose raw register access for unstable peripherals. | `crates/hisi-riscv-hal/src/peripherals.rs` token fields and `register_block()`. | Decide explicit policy exception, gate unstable tokens, or make raw access unsafe/unstable. |
| High | `Instant::now()` uses global TCXO MMIO and can spin forever; no direct HIL. | `crates/hisi-riscv-hal/src/time.rs`. | Add bounded/fallible API and HIL, or keep hardware instant unstable. |

## 3. Graduation Matrix

Legend:

- **Graduate**: can be stable with current evidence, subject to normal docs/build checks.
- **Scoped**: only the named subset can graduate; the module-level stable claim is too broad.
- **Block**: do not expose as stable until blockers are fixed.
- **Infra**: compile-time or re-export infrastructure; silicon HIL is indirect.

| API surface | Verdict | Graduation reason | Blocking / follow-up |
|---|---|---|---|
| `gpio` core input/output/readback/IRQ, `GpioBank` | Scoped | HIL covers GPIO output readback, loopback, and named IRQ routing; `GpioBank` removes raw IRQ bank index. | Full module blocked by `open_drain` no-op and pull config no-op on GPIO15..18 unless documented/gated. |
| `io_config` GPIO/UART mux, `GpioPad`, `UartPad`, `MuxFunction` | Scoped | Pad and mux identities are typed; GPIO/UART/SPI loopback tests exercise representative mux paths. | `SfcPad` / `configure_sfc_pad` should be unstable while SFC is unstable; pad/function validity is not fully encoded. |
| Blocking `spi` | Graduate | HIL SPI0 loopback exercises real blocking transfer; typed `SpiHz`/`DataBits` avoid raw config identity. | Frequency rounding to even divider should be documented or tightened; non-SPI0 coverage is representative, not per-instance. |
| `SpiDma` stable subset | Block | SPI0 HIL exists for TX/full-duplex/IRQ/async paths. | Critical timeout/cancellation/cache blockers; tests still `unstable`-gated; SPI1 mapping unverified. |
| Blocking `uart`, `UartPort`, sealed `UartInstance` | Scoped | `UartPort`/`UartInstance` encode port identity; HIL covers UART0 divider config and UART1 loopback path. | Raw `clock_hz` can invalidate divider math; constructors do not self-enable clocks in all tested paths; trait paths need broader HIL. |
| `UartDma` | Block / unstable | No graduation reason; UART DMA remains unproven. | `UartDmaError` now correctly gated unstable. |
| `timer`, `TimerChannel` | Scoped | HIL covers counter advance and named timer IRQ routing; `TimerChannel` removes raw channel index. | One-shot/periodic wrappers, zero tick semantics, `AsyncDelay`, and `TimerChannel` variants beyond Channel0 need more HIL. |
| `interrupt`, `Priority`, `Threshold` | Scoped | Typed priority/threshold constructors reject invalid levels; routing HIL exercises interrupt enable path. | `set_priority`/`set_threshold` lack direct HIL and may enable pending delivery from safe code; `interrupt::free` needs ordering review. |
| `asynch::block_on` / `IrqSignal` | Block | Async examples/tests exercise the path under feature builds. | Lost-wake races block graduation. |
| `tcxo` | Scoped | HIL covers status/counter monotonicity through `read_counter32`. | `read_counter64`, enable/disable/clear, and timeout behavior need coverage/SAFETY cleanup. |
| `pwm`, `PwmPeriod`, `Duty`, `PwmChannelId` | Block for full module; scoped Ch0 config only | HIL covers Ch0 configure/enable with typed period/duty. | Ch1..Ch7 clock/divider not proven; duplicate `PwmChannel` ownership; trait duty validation bug; frequency clock note unconfirmed. |
| `wdt` | Scoped / mostly graduate | HIL covers typed timeout config, rejection of over-range load, drop-disable, and armed escape. | Feed/counter/interrupt APIs need HIL before claiming full module stable. |
| DMA mem-to-mem: `Dma0`, `DmaDriver`, `DmaChannel`, `DmaChannels`, `DmaTransferSize`, `DmaSyncMask`, `Transfer`, `start_mem_to_mem` | Block for safe stable; scoped API design is close | HIL covers mem-to-mem and owned transfer guard; typed channel and beat count remove raw channel/size. | Cache-line alignment soundness unresolved; `DmaSyncMask`/`set_sync` lack HIL; zero-beat/min-length semantics need decision. |
| `trng` default read/fill path | Scoped | HIL `trng_produces_entropy` exercises blocking entropy generation. | `set_sample_clock`, `set_divider`, and status/config helpers are raw/uncovered; gate or type them. |
| eFuse read-only: `EfuseDriver`, `EfuseByteAddress`, `read_byte` | Scoped | HIL reads byte 0; address newtype prevents OOB data-window access; `write_byte` is unstable. | `read_buffer` lacks HIL; `set_clock_period(u8)` is raw timing config and should be typed/gated. |
| `clock` | Scoped | HIL checks UART0 gate metadata; `Peripheral` avoids raw gate index in stable APIs. | Broad enum/gate coverage is not proven; docs still mention removed RAII guard. |
| `system` | Scoped | HIL covers reset reason. | `software_reset*` needs opt-in reset HIL or unstable gate. |
| `peripherals` | Block unless policy exception | `Peripherals::take`/tokens are foundational and used by all HIL tests. | Public tokens/register blocks expose unstable/un-HIL peripherals. Needs explicit policy exception or gating. |
| `i2c` WS63 v150 | Block for full module | HIL covers SCL config/register path only. | Transaction APIs lack real bus HIL and invalid address rejection. |
| `i2s` | Block for full module; scoped config/liveness only | HIL constructs master config and reads version. | TX/RX, FIFO, slave mode, interrupts, waveform/data path unproven. |
| `lsadc` | Block | HIL covers scan register config only. | File docs say incomplete silicon validation; analog/conversion/filter/calibration APIs raw/uncovered. |
| `tsensor` | Block for full module; scoped basic read only | HIL covers basic enable/start/read_raw in range. | Mode/threshold/interrupt/auto-refresh/calibration are raw or uncovered; blocking read unbounded. |
| `cache` | Infra / unsafe-only graduate | Low-level cache APIs are `unsafe`; DMA HIL indirectly exercises clean/invalidate. | Safe DMA callers must enforce cache preconditions before DMA graduates. Add overflow precondition docs. |
| `time` | Block for hardware instant; pure newtypes need tightening | Pure `Duration`/`Rate` are infrastructure. | `Duration`/`Rate` arithmetic can overflow/wrap; `Instant::now` can hang and lacks HIL. |
| `prelude` | Infra, fixed | Re-exports now no longer leak `delay`/`rtc` by default. | Keep prelude re-exports tied to underlying gates. |
| `macros` | Infra | Compile-time infrastructure; no silicon behavior. | Add compile tests if treated as stable user API. |
| `soc` | Infra / WS63 scoped | Constants/PAC aliases are indirectly exercised by HIL. | BS21 constants are not silicon-graduated; docs should avoid implying BS21 stable silicon proof. |
| `private` | Not public API | Module is now crate-private; sealing no longer leaks. | Remove from public stable lists, already updated in policy docs. |

## 4. HIL Evidence Map

Named HIL evidence found in `crates/hisi-riscv-hal/tests/hil.rs`:

| HIL test | Supports | Notes |
|---|---|---|
| `gpio_output_readback` | GPIO output/readback | Self-contained. |
| `gpio_loopback_0_to_3` | GPIO input/output loopback | `hil-loopback`. |
| `gpio_int0_named_routing` | GPIO IRQ, `GpioBank`, async IRQ hook | `async` + `hil-loopback`. |
| `spi0_loopback_mosi_to_miso` | Blocking SPI0 | `hil-loopback`. |
| `uart0_divider_config` | UART0 config | Register/config HIL. |
| `uart1_loopback_tx_to_rx` | UART1 blocking RX/TX path | `hil-loopback`; previous board/pad caveats still apply. |
| `timer_counter_advances` | Timer counter | Channel0. |
| `timer_int0_named_routing` | Timer IRQ routing | Excludes embassy. |
| `tcxo_counter_monotonic` / status test | TCXO | Counter32/status. |
| `pwm_configure_and_enable` | PWM Ch0 config/enable | Ch0 only. |
| `wdt_configure_saturates_load`, `wdt_drop_disables_unless_armed` | WDT typed config/drop | Good graduation evidence for those paths. |
| `dma_mem_to_mem`, `dma_transfer_guard` | DMA mem-to-mem and owned guard | Does not prove arbitrary cache-line safe buffers. |
| `trng_produces_entropy` | TRNG read path | Default read path only. |
| `efuse_read_byte0_ok` | eFuse `read_byte` | Does not cover `read_buffer`. |
| `clock_gate_uart0_enabled` | Clock gate metadata | UART0 only. |
| `system_reset_reason_valid` | System reset reason | Not software reset. |
| `i2c0_scl_config` | I2C config | No transaction. |
| `i2s_version_live` | I2S liveness/config | No data path. |
| `lsadc_scan_config` | LSADC scan config | No analog conversion. |
| `tsensor_reads_in_range` | TSENSOR basic conversion | No config/interrupt paths. |
| `spi_dma_*` tests | SPI0 peripheral-DMA | Currently feature-gated with `unstable`; not sufficient for default stable HIL reproduction. |

## 5. Required Graduation Actions

Before publishing 0.6.0 stable API claims, perform these in order:

1. Decide whether stability is module-wide or item/subset-wide. The current module-wide wording is too broad.
2. Gate or remove public stable items that have no HIL and no soundness proof.
3. Move any graduated HIL tests out from `feature = "unstable"` so the default stable suite reproduces the decision.
4. Fix DMA soundness before any safe DMA API graduates: timeout quiesce, async cancellation guard, cache-line alignment/ownership, SPI1 gating.
5. Fix async lost-wake before graduating `asynch` as stable infrastructure.
6. Tighten typed-config blockers: UART clock/baud pair, GPIO open-drain, PWM duty/channel ownership, I2C address validation, TSENSOR/LSADC/TRNG raw config.
7. Add missing HIL: `EfuseDriver::read_buffer`, `Priority`/`Threshold`, I2C transaction, I2S data path or narrower docs, LSADC conversion, TSENSOR config/interrupt, WDT feed/counter/interrupt, `Instant::now` or its replacement.
8. Update docs to state scoped graduation decisions with file/test evidence, not broad module labels.

## 6. Current Graduation Verdict

As of this review, the safe release posture is:

- **Graduate only the scoped sub-surfaces explicitly covered above.**
- **Do not claim the full documented stable API list has graduated.**
- **Treat the blockers in section 2 as release-blocking for any API that would expose those paths by default.**

This report is intentionally conservative: QEMU examples and compile-only evidence are useful, but they are not substitutes for named WS63 HIL or a closed safe/unsafe invariant.
