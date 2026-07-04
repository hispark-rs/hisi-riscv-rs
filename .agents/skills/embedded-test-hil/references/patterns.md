# Embedded-Test HIL Patterns

## Harness Facts

- `embedded-test` provides the test dispatcher `main` and panic handler.
- `hisi-riscv-rt` still supplies reset vector, linker scripts, critical-section implementation, and WS63 `boot-header`.
- `hil/embedded-test-runner.sh` first runs `hisi-fwpkg patch-hash <elf>`, then `probe-rs run --chip WS63 ... <elf>`.
- The HAL harness must be invoked with `--test hil`; otherwise Cargo tries to build the lib test target for bare metal.

## Good HAL Test Shape

```rust
// crates/hisi-riscv-hal/tests/hil/wdt.rs
#[cfg(feature = "chip-ws63")]
pub(crate) fn wdt_leak_keeps_watchdog_armed() {
    use hal::wdt::{ResetPulseLength, Watchdog, WdtMode, WdtTimeout};

    let r = unsafe { &*pac::Wdt::PTR };
    // SAFETY: sequential single-hart HIL run; WDT singleton is not otherwise held.
    let mut wdt = Watchdog::new(unsafe { hal::peripherals::Wdt::steal() });
    wdt.configure(
        WdtTimeout::from_ms(1_000).unwrap(),
        WdtMode::SingleInterrupt,
        false,
        ResetPulseLength::Cycles2,
    )
    .expect("configure on live silicon");
    let _armed = wdt.leak();

    assert_eq!(r.wdt_cr().read().bits() & 0x1, 0x1);

    Watchdog::new(unsafe { hal::peripherals::Wdt::steal() }).disable();
}
```

```rust
// crates/hisi-riscv-hal/tests/hil.rs
#[cfg(feature = "chip-ws63")]
#[test]
fn wdt_leak_keeps_watchdog_armed() {
    crate::wdt::wdt_leak_keeps_watchdog_armed();
}
```

## What Counts As Good Evidence

- Register/poll facts: configuration bits latch, counters advance, status changes, invalid values are rejected.
- Ownership facts: PAC/HAL singleton claims, Drop/armed token behavior, critical-section backed take path.
- Typed-config facts: constructor rejects impossible values, configured values read back as expected.

## What Needs A Gate Or A Different Test

- `hil-loopback`: GPIO/SPI/UART tests requiring external jumpers.
- `hil-rtc`: RTC tests requiring a populated 32.768 kHz crystal.
- `unstable`: tests using unstable public API, DMA/cache-invariant experiments, interrupt/waker async helpers, BS2X target.
- Example-level behavior: put in `hil-smoke`, not HAL HIL.
- Irreversible operations: eFuse writes and destructive reset/program operations should stay out of the default suite.

## Failure Triage

- Link failure mentioning `test`/`std`: command missed `--test hil`.
- No semihosting output: check `PROBE_YAML`, patched `probe-rs`, and that runner is `hil/embedded-test-runner.sh`.
- Boot verify failure: check `hisi-fwpkg patch-hash` ran on the built test ELF.
- Test hangs: inspect for unbounded MMIO polls, waiting inside critical sections, missing clock gates, or board-population assumptions.
- Test passes in QEMU but not silicon: suspect clock source, real latch width, IRQ routing, or physical wiring first.
