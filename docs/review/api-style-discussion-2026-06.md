# hisi-riscv-hal — API Style Discussion (beyond typed-config)

> 评审日期：2026-06-15 · 对象：`hisi-riscv-hal` 的 API 风格（typed-config 约定之外）· 方法：12 维度多 agent 工作流（Opus 4.8 全程，13 个 agent / ~767K tokens），每维度并行「研究业界实践 + 审计当前源码 + 评估」，再综合。业界来源 esp-hal / embassy / embedded-hal 1.0 / Rust API Guidelines / Embedded Rust Book / defmt（链接见末节 Sources）；源码 `file:line` 见正文。typed-config 约定本身见 [类型化配置](../explanation/typed-config.md)。


**Scope:** the owner asked "beyond the typed-config convention, what *other* HAL API-style points are worth discussing, referencing industry practice?" This synthesizes 11 per-dimension audits (esp-hal, embassy, embedded-hal 1.0, the Rust API Guidelines, the Embedded Rust Book, defmt) against the current source. Typed config ("if it compiles, it runs on silicon") is **settled** and only referenced where another dimension collides with it.

---

## 1. What this HAL already does well

A strong, esp-hal-aligned base. These are already nailed — don't re-open them:

- **Lifetime'd peripheral singletons match esp-hal 1.0** — `$name<'d>` ZSTs via `PhantomData`, `unsafe steal()`, `Peripherals::take()`; drivers consume the token by value. (`peripherals.rs:14`, `wdt.rs:76`, `spi.rs:76`)
- **Per-driver error enums, not a god-enum** — exactly the embedded-hal idiom; `I2cError`/`SpiError`/`SfcError`/… each local. (`i2c.rs:290`, `spi.rs:217`)
- **embedded-hal `Error`/`kind()` wiring exists** for I2C and SPI; GPIO/UART correctly use `Infallible`. (`i2c.rs:296`, `spi.rs:222`, `gpio.rs:215`, `uart.rs:201`)
- **Bounded busy-waits return `Timeout`, never hang the CPU** — a genuine robustness strength. (`i2c.rs:62`, `spi.rs:58`)
- **Correct embedded-hal-bus boundary** — implements `SpiBus`, deliberately *not* `SpiDevice` (WS63 CS is software-driven), no `embedded-hal-bus` dep. This is exactly what embedded-hal 1.0 prescribes. (`spi.rs:230`)
- **Sealed traits done by the book** — `private::Sealed` with a doc comment, sealing `DmaWord` and the GPIO signal traits. (`private.rs:7`)
- **Modern GPIO driver model present** — generic-free `AnyPin`, plus `Input`/`Output`/`Flex` driver types (esp-hal post-#1188 shape). (`gpio.rs:90`, `159`, `230`, `295`)
- **Single-type async, no `Driver<Blocking>`/`Driver<Async>` split** — async traits impl'd on the same blocking types; the empty `DriverMode` markers were correctly removed. (`asynch.rs`, per-driver `asynch_impl`)
- **`const fn` config surface is complete and idiomatic** — 39 `const fn`; typed-config newtypes fold at compile time. (`pwm.rs:44`, `time.rs:57`, `gpio.rs:53`)
- **Clean diagnostics posture** — pervasive `Debug` derives, zero `log!`/`println!` in hot paths. (`#[derive(Debug)]` ×75)
- **Edition 2024 + resolver 3**, and chip mutual-exclusion enforced the Cargo-sanctioned way via `compile_error!`. (`lib.rs:32`)

---

## 2. Discussion points, by priority

### Priority overview

| Dimension | Assessment | Priority | One-line decision |
|---|---|---|---|
| Naming, docs & API-guideline conformance | partial | **HIGH** | Turn on `deny(missing_docs)` + fix the 2 concrete violations? |
| Error handling gaps (`non_exhaustive`, `defmt`, `Display`) | partial | MED | Mechanical future-proofing — how much, how soon? |
| Diagnostics: optional `defmt` feature | partial | MED | Add `defmt` feature; blanket vs errors-first? |
| Traits, sealing & `non_exhaustive` | partial | MED | Per-enum `non_exhaustive` policy before 1.0 |
| Construction & config ergonomics | partial | MED | Builder-lite + I2S two-step outlier |
| Resource ownership / RAII (reborrow, Drop) | partial | MED | Add safe `reborrow()`; Drop-to-disable policy |
| Typestate GPIO (kill the dual API) | partial | MED | Remove legacy `GpioPin<MODE>`; explicit `Flex` transitions |
| Interrupt / ISR binding model | partial | MED | Wire RT named-handler routing (mostly an RT task) |
| Const / MSRV / edition | follows-well | MED | Just declare `rust-version` |
| DMA buffer-ownership safety | **gap** | MED | Owned-buffer transfer guard *when DMA is first exposed* |
| Async vs blocking duality | follows-well | LOW | Leave as-is; document the non-IRQ-parking SPI/I2C |

---

### HIGH — Naming, docs & API-guideline conformance

> **Industry practice.** esp-hal mechanically enforces docs: `#![deny(missing_docs)]` + `deny(rustdoc::all)` in `lib.rs`. The API Guidelines require `# Safety` on every `pub unsafe fn` (C-SAFETY-DOC), `# Errors`/`# Panics` sections (C-FAILURE), getters without `get_` (C-GETTER), and runnable examples — on no_std the universal convention is ` ```no_run ` (compiles + type-checks) not ` ```ignore ` (never compiled). [API Guidelines: naming; esp-hal lib.rs]

**Current state.** No doc-coverage enforcement in `lib.rs` (only `allow(...)` lints) → large undocumented surface (`uart.rs:89-198`, `spi.rs:76-190`, `i2c.rs:27-157` all bare). One C-GETTER violation: `io_config::get_gpio_mux` (`io_config.rs:169`) — the *only* offender. **Zero** `# Errors`/`# Panics` sections crate-wide despite many fallible/panicking methods. Every doc example is ` ```ignore ` (4 + lib.rs) so examples can rot silently. *Good:* `# Safety` coverage on the public unsafe surface is effectively complete (13 sections / 9 `pub unsafe fn`); `as_`/`to_`/`into_` cost conventions used correctly.

**Decision.** *Do we adopt esp-hal's mechanical doc enforcement now, accepting a backfill pass?*
- **(a)** `#![warn(missing_docs)]` now → backfill → flip to `deny` (esp-hal's staged `cfg_attr` approach; doesn't block other work).
- **(b)** `#![deny(missing_docs)]` immediately (hard build break today across uart/spi/i2c).
- **(c)** Leave unenforced (the gap silently regrows).

**Recommendation.** **(a)**, plus the two cheap concrete fixes now while pre-1.0: rename `get_gpio_mux → gpio_mux` (breaking later, free now), add `# Errors`/`# Panics` to the Result-returning and asserting methods, and convert ` ```ignore ` → ` ```no_run ` so examples at least type-check (the HIL suite already covers runtime, so `no_run` is the right cost/benefit). Note for the owner: `write_byte(idx)/read_byte(idx)` taking a runtime `idx` on a type that already encodes the instance in `PhantomData<Uart0>` is a latent footgun worth an API-shape look, not just a doc.

---

### MEDIUM (1) — Error handling: `non_exhaustive`, `defmt`, `Display`

> **Industry practice.** embedded-hal's own `ErrorKind` enums are `#[non_exhaustive]`, and the convention is to mark *your* driver error enums the same (adding a variant becomes non-breaking). API Guidelines C-GOOD-ERR wants `Debug` + `Display`/`core::error::Error`. esp-hal's hard rule: every `Debug`-deriving type also derives a `defmt`-gated `defmt::Format`. [embedded-hal i2c docs; esp-hal DEVELOPER-GUIDELINES; API Guidelines future-proofing]

**Current state.** Per-driver enums + `ErrorType`/`kind()` skeleton is right, but: **zero `non_exhaustive`**, **zero `must_use`**, **zero `defmt`** (no feature, no derives), **no `Display`/`core::error::Error`** anywhere. `SpiError` collapses both `Overflow` *and* `Timeout` → `ErrorKind::Other`, discarding the free `Overrun` mapping (`spi.rs:223`). BS2X `i2c_v151::I2cError` is **not** wired to `embedded_hal::i2c::Error` (`i2c_v151.rs:26`), so BS2X I2C isn't portable like WS63. Production-path panics: `PwmChannel::new` asserts `channel < 8` (`pwm.rs:158`), `km.rs:43/62`, `ulp_gpio.rs:144/152`, `clock.rs:116`.

**Decision.** *Which mechanical fixes ride in the config pass, and where is the boundary with typed-config?*

**Recommendation (all low-risk, mostly mechanical):**
1. `#[non_exhaustive]` on every public error enum (×8) — cheap, now that semver-checks gates CI.
2. Map `SpiError::Overflow → ErrorKind::Overrun` (free, correct); confirm `Timeout → Other` is intentional (no portable kind exists).
3. Wire `i2c_v151::I2cError` into `embedded_hal::i2c::Error` so BS2X I2C is portable.
4. `impl Display + core::error::Error` (stable in `core` since 1.81) — optional; see the diagnostics point below, the ecosystem increasingly drops `Display` in favor of `Debug + defmt`.
5. **The `assert!`-in-constructor cases are the explicit boundary with typed-config.** `assert!(channel < 8)` is the canonical example typed-config exists to eliminate — route `PwmChannel`/`Km`/`UlpGpio` index validation through the **typed-config skill** (validated index newtype / fallible constructor), *not* as fresh error work. It must not fall through the cracks. Leave the `unreachable!` dispatch arms (sound, internal) and `Infallible` GPIO/UART as-is. **Do not** introduce a crate-wide `HalError` — per-driver enums are correct.

---

### MEDIUM (2) — Diagnostics: an optional `defmt` feature

> **Industry practice.** Every public `Debug`-deriving type also derives `#[cfg_attr(feature = "defmt", derive(defmt::Format))]`. embedded-hal *requires* error types be `Debug` but **not** `Display`; the ecosystem largely dropped `Display` for HAL errors (binary bloat, defmt does it better on-target). HALs must not call `log!`/`info!` in hot paths. [esp-hal DEVELOPER-GUIDELINES; defmt book; embedded-hal migration guide]

**Current state.** `Debug` half is done well; `defmt` completely absent. `Display`/hot-path-logging correctly absent already. Minor consistency gap: `SpiError`/`SfcError`/`I2cError`(v150) derive `Debug` only, while `EfuseError`/`TrngError` derive `Debug, Clone, Copy, PartialEq, Eq` — error types should be `Copy + Eq`.

**Decision.** *(a) feature name `defmt` (esp-hal/embassy) vs `defmt-03` (embedded-hal, multi-major-proof)? (b) blanket all ~30 Debug types vs errors+configs first?*

**Recommendation.** Add `defmt = { version = "0.3", optional = true }` + `defmt = ["dep:defmt"]`; use plain **`defmt`** (single-major HAL). Roll out **errors + user-facing config enums first** (smaller reviewable diff; note `Format` can't derive on a type whose field lacks `Format`). Normalize the error-enum derives to `Copy + Eq` while there. **No `Display`, no internal logging.** Add a `cargo check --features defmt` CI job so the `cfg_attr` derives can't rot.

---

### MEDIUM (3) — Traits, sealing & `non_exhaustive`

> **Industry practice.** Sealing via `private::Sealed` (C-SEALED) — done well here. `#[non_exhaustive]` on essentially every public enum and config struct (C-STRUCT-PRIVATE: private fields + `with_*` builder + getters). `SpiBus`-only is the correct HAL boundary. [API Guidelines future-proofing; esp-hal uart::Config]

**Current state.** Sealing follows the guideline (`private.rs:7`). embedded-hal coverage strong and correctly bounded (`SpiBus` yes, `SpiDevice` no). **`non_exhaustive`: zero across all 51 public enums and every `Config` struct** (all fields `pub`, struct-literal + `..Default::default()`). Minor: vestigial duplicate empty `PeripheralInput`/`PeripheralOutput` markers (`private.rs:24-27`) shadowed by the real signal traits (`gpio.rs:575-582`).

**Decision.** *`non_exhaustive` collides head-on with the settled typed-config public-field convention — you cannot have both public struct-literals AND `non_exhaustive` on a struct.*
- **Enums:** mark error enums + growth-prone mode enums `non_exhaustive` (costs downstream one `_ =>`). Leave hardware-closed enums (`DataBits`, `StopBits`, `SpiMode`, `Parity`) exhaustive — fixed state sets, no upside.
- **Config structs:** **(a)** keep public fields, accept field-add = breaking, lean on cargo-semver-checks (already in CI) to turn the silent hazard into a visible minor-bump decision; **(b)** migrate to private fields + `with_*` builders + `non_exhaustive` (esp-hal gold standard, but a break + boilerplate now).

**Recommendation.** Enums: mark the 8 `*Error` + growth-prone mode enums **before 1.0** (silicon bring-up keeps finding fault modes). Configs: **(a)** for now — semver-checks is the legitimate guardrail; reconsider **(b)** only for a Config expected to grow post-1.0. Leave the `SpiBus`/`embedded-hal-bus` boundary as-is (correct); add a rustdoc note pointing multi-device users to `embedded_hal_bus::spi::ExclusiveDevice`. Delete the vestigial markers.

---

### MEDIUM (4) — Construction & configuration ergonomics

> **Industry practice.** esp-hal: per-driver `Config` taken by value, Builder-Lite (private fields + `with_x`), `apply_config(&mut self, &Config)` for reconfigure, one call to a working peripheral. [esp-hal DEVELOPER-GUIDELINES; API Guidelines C-BUILDER]

**Current state.** Split story. Bus peripherals: public-field `Config` + `Default`, infallible `new` (`spi.rs:18`, `uart.rs:35`). GPIO/DMA already do builder-lite (`gpio.rs:52` `with_pull`/`with_open_drain`). **Outliers:** I2S is two-step `new()` then `configure()` (`i2s.rs:133/143`) — the one real wart; WS63 I2C takes raw `freq: u32` (`i2c.rs:27`) while BS2X takes a typed `Speed` enum (`i2c_v151.rs:58`). **Zero `with_*` on bus configs, zero `apply_config`/reconfigure anywhere, zero fallible constructors.** SPI `data_bits: u8` is a raw scalar clamped inside `configure` rather than a typed newtype.

**Decision.** *Standardize on builder-lite for the bus family now (churns struct-literal call sites) or defer to the next breaking release? And where's the `Result`-constructor boundary vs typed-config?*

**Recommendation.** (1) Give bus configs the same `const fn new()` + `with_*` GPIO/DMA already have; make fields private or `non_exhaustive`. (2) Replace raw scalar args with typed newtypes per the settled convention (SPI `data_bits → DataBits`, WS63 I2C `freq → Speed`, sharing the BS2X surface). (3) Fold I2S `configure()` into construction — uniform "one call to a working peripheral." (4) Add `apply_config` **only** where reconfigure is real (UART baud, SPI mode/freq), not blanket. (5) **Keep `new` infallible over typed newtypes** (consistent with "if it compiles, it runs"); reserve `Result`/`ConfigError` for residual *runtime-only* failures (clock can't hit requested baud).

---

### MEDIUM (5) — Resource ownership, lifetimes & RAII

> **Industry practice.** esp-hal 1.0: lifetime'd singletons with `steal()`/`clone_unchecked()` (unsafe) **and a safe `reborrow()`**; "after dropping the driver, the peripheral should be accessible again." Drop-to-disable for hardware quiescence is idiomatic but a deliberate trade-off — many HALs choose explicit `free()`/`disable()` instead. [esp-hal Peripheral docs / 1.0 release; embassy `Peri<'d>`]

**Current state.** Singletons follow esp-hal closely. **Gaps:** (1) zero `Drop` impls — a dropped driver leaves the IP enabled (explicit `disable()` exists but nothing runs on drop); (2) **no safe `reborrow()`** — reuse only via `unsafe steal()`; (3) `PwmChannel::new(&Pwm)` takes a *shared ref* (`pwm.rs:157`) so the token is never consumed and channels alias with no guard; (4) `into_input`/`into_output` re-stamp lifetime to `'static` (`gpio.rs:481`), silently widening the borrow past `'d`; (5) GPIO pins are made by `unsafe AnyPin::steal`, not `Peripherals`-owned singletons. Note: the ref-counted clock layer was *deliberately* removed (clocks reset-default-on, `clock.rs:20`), so Drop-to-disable buys nothing for power.

**Decision.** *Adopt the esp-hal "drivers borrow, reuse via reborrow" mental model, or keep "one driver owns it forever, steal() to reuse"? And Drop-to-disable vs explicit teardown?*

**Recommendation.** (A) **Add safe `reborrow(&mut self) -> Self<'_>` to the `peripheral!` macro** — highest-value, purely additive, SemVer-safe. (B) **Prefer explicit teardown** (keep `disable()`, add `free(self) -> Token` where it matters) over surprise Drop — clocks are already on, so the only Drop value is leaving hazardous state quiescent (Wdt running, PWM driving, Output high); if you do add Drop, scope it to those and beware ordering on the no-atomic core. (C) Replace the `PwmChannel::new(&Pwm)` outlier with a `split()`-returns-array API (esp-hal/embassy idiom, tracks the 1:N fan-out). (D) Thread `'d` through `into_*` (soundness-adjacent). (E) Promoting pins to singletons is the largest change — defer; given 19 pins, `steal()` may be acceptable. Encode the choice **once in the macros** so all ~35 peripherals stay consistent.

---

### MEDIUM (6) — Typestate GPIO (the dual API)

> **Industry practice.** The ecosystem moved *away* from per-pin `GpioPin<MODE>` toward runtime-typed `Input`/`Output`/`Flex` drivers + generic-free `AnyPin` (esp-hal #1188 "massively simplify GPIO"). `Flex` mode transitions are explicit in-place `&mut self` (`set_as_input`/`set_as_output`), not consume-self; `AnyPin` via `degrade()`. [esp-hal #1188; embassy-stm32 gpio.rs]

**Current state.** Both models coexist in one file. Modern `Input`/`Output`/`Flex`/`AnyPin` matches the reference (`gpio.rs:90+`). **But legacy `GpioPin<'d, MODE>` + `InputMode`/`OutputMode` + `create_input/output_pin` is still present and exported from the prelude** (`gpio.rs:450-562`, `prelude.rs:15`) with **duplicate** embedded-hal impls — and has effectively zero live users (one stale CHANGELOG mention). `Flex` has no explicit transitions: every method silently flips OEN, and `Flex::is_high` does a hidden save-force-read-restore RMW (`gpio.rs:327-341`). `AnyPin` only constructible via `unsafe steal` (no safe `degrade()`). `ulp_gpio.rs` independently rebinds `Input`/`Output` as *mode markers* — a third GPIO shape, name collision.

**Decision.** *Remove the legacy `GpioPin<MODE>` (deprecate-then-delete across two minors) or keep it as a compat shim?*

**Recommendation.** **Remove it** — it's the exact wart esp-hal #1188 deleted, and keeping two conventions is what the references rejected. Use `#[deprecated]` for one minor, drop on the next breaking bump (clean semver-checks story). Then: add explicit `Flex::set_as_input(&mut self, Pull)`/`set_as_output(&mut self)` (kill the hidden RMW; keep `&mut self`, *not* consume-self); add a safe `degrade()` + a safe typed pin source so `AnyPin` erasure is actually reachable; reconcile `ulp_gpio.rs` naming so the crate presents **one** GPIO convention.

---

### MEDIUM (7) — Interrupt / ISR binding model

> **Industry practice.** Two complementary conventions, and the best HALs offer both: (1) named-handler vectoring at the runtime layer — app writes `#[interrupt] fn GPIO_INT0()`, never hand-decodes mcause or touches mtvec (riscv-rt/svd2rust `device.x`); (2) compile-proven handler ownership at the HAL layer (esp-hal `set_interrupt_handler`, embassy `bind_interrupts!`). [riscv-rt interrupts; embassy bind_interrupts!; esp-hal interrupt]

**Current state.** Deliberately controller-only; ISR routing pushed to the app (`interrupt.rs:18`). Drivers expose free `on_interrupt(...)` the app must call (`gpio.rs:634`, `timer.rs:422`). **Every async/embassy example hand-rolls** a `csrr mcause` decode + `csrw mtvec` (e.g. `embassy_multitask` hard-codes `== 26`, which silently breaks on BS2X where `ALARM_IRQ` is 53 — the HAL *exposes* `embassy::ALARM_IRQ` but the example ignores it). A named-handler path *exists but is vestigial*: `device.x` PROVIDEs the symbols and `startup.S` has a vectored table, but `local_isr_dispatch` is a weak no-op stub — so the convention is half-built and examples override mtvec to bypass it.

**Decision.** *Is this in-scope for the HAL review or an `hisi-riscv-rt` task? (They're coupled — the `on_interrupt` hooks only become ergonomic once RT routing exists.) And how much compile-time proof is worth it on a chip with a fixed IRQ↔peripheral map?*

**Recommendation.** **(1)** Finish the cheap idiomatic win **in `hisi-riscv-rt`**: wire `local_isr_dispatch` to the `device.x` named symbols (or adopt riscv-rt 0.14's `__EXTERNAL_INTERRUPTS`/`_dispatch_*`) so apps write `#[interrupt] fn GPIO_INT0() { gpio::on_interrupt(0); }`. This deletes the most copy-pasted, footgun-prone code in every example and eliminates the hard-coded-`== 26` bug class. **(2)** *Then* consider a thin `macro_rules!` `bind_interrupts!`-style shim — **no `Binding<I,H>` witness plumbing** (embassy needs it for instance-generic drivers; WS63's map is fixed, so a shim captures ~90% of the value at ~10% of the cost). Pick **one** of vectored/direct mode (startup.S sets vectored but examples override to direct — delete the unused half). Leave the controller API as-is. This is independent of typed-config (operational surface → `unsafe fn`/Result idioms, not newtypes).

---

### MEDIUM (8) — Const, compile-time & MSRV/edition

> **Industry practice.** Declare MSRV via `rust-version` in `Cargo.toml` (the MSRV-aware resolver uses it); pick a policy and write it down. esp-hal: "latest stable at release, no patch guarantee"; embedded-hal: explicit `rust-version` + changelog discipline. `const fn` validating constructors; `#[inline]` on thin register wrappers (LTO softens it). [Cargo Book rust-version; embedded-hal msrv.md; esp-hal crates.io]

**Current state.** `follows-well`. Edition 2024 + resolver 3 uniform; `const fn` coverage idiomatic and complete (39); zero `inline(always)`. **One real gap: no `rust-version` anywhere** — MSRV is the implicit pinned custom `hisi-riscv` 1.96.0 toolchain, undocumented. Minor: ~32 hardcoded MMIO base-address `const`s scattered file-locally (only 4 covered by `safety.rs` const_asserts); per-driver `regs()` helpers not `#[inline]` (rely on LTO).

**Decision.** *Should `rust-version` reflect the pinned 1.96.0 or a tested-lower floor?*

**Recommendation.** **Add `rust-version` to `[workspace.package]`** + a one-line policy in README (esp-hal-style: "compiles on the pinned toolchain; MSRV may bump in minor, not patch"). Because the toolchain is *non-standard*, downstreams benefit **more** from an explicit floor, not less. Run `cargo +<msrv> check` before committing the number (it interacts with resolver 3). Optional/low: centralize the MMIO bases into a `soc::addr` module so all are const_assert-guarded; `#[inline]` on `regs()` is a no-op under `lto=true` (style only). **Leave the runtime `channel: u8` dispatch** — zero const generics is the correct call (keeps trait objects usable, no monomorphization bloat); confirm it stays a settled non-goal.

---

### MEDIUM (9) — DMA buffer-ownership safety **(the one `gap`)**

> **Industry practice.** Make use-after-free *unrepresentable* by moving buffer ownership into an in-flight transfer guard, and make cache coherence part of the type/API. `embedded-dma` `ReadBuffer`/`WriteBuffer` are the standard bound; esp-hal's `DmaTxBuf`/`DmaRxBuf` *move* the buffer into the transfer, bake `cache_writeback`/`cache_invalidate` + cache-line alignment into `prepare()`, and the compiler rejects touching the buffer until `.wait()`. [embedded-dma; Japaric "safe DMA"; esp-hal dma buffers]

**Current state.** **The only `gap`.** `configure_channel(&mut self, channel, src_addr: u32, dst_addr: u32, size, &config)` (`dma.rs:239`) takes **bare `u32` addresses — and is not even `unsafe`**. No `ReadBuffer`/`WriteBuffer` bound, no `embedded-dma` dep. The buffer can be dropped/moved/reused mid-transfer → use-after-free / data race **fully representable in safe code**. Cache coherence is a doc note + standalone `unsafe fn`s (`cache::clean_range`/`invalidate_range`, `cache.rs:69`) the caller must sequence and 32-byte-align by hand; the only correct end-to-end use is the HIL test. Async `wait_transfer_done` returns no buffer and does no invalidate → stale-cache reads. DMA is **not yet wired into any peripheral driver**, so the raw API is currently contained.

**Decision.** *Build the safe layer now (cheap, sets the pattern) or when the first DMA-backed peripheral lands? `&'static mut` (esp-hal, no stack buffers) vs borrowed-with-blocking-Drop (sound only if you accept `mem::forget` as the documented hole)? Cache alignment as a type invariant vs a runtime check?*

**Recommendation.** Adopt the ownership model **when DMA graduates to a user-facing API** (it's most valuable at first exposure; cheap to set the pattern now). Keep `configure_channel` as a private/**`unsafe`** core (its safe signature understates the contract today). Layer over it: `start_transfer(self, buf) -> Transfer<'d, BUF>` consuming driver + buffer, returning `(driver, buf)` on `wait()`/`.await`; bound on `embedded_dma::{ReadBuffer,WriteBuffer}`. **Fold cache maintenance into the transfer** (`clean_range(src)` on launch, `invalidate_range(dst)` on wait) — exactly esp-hal's `prepare()`, and squarely the project's "if it compiles, it runs on silicon" rule. Enforce 32-byte alignment in the type (aligned-buffer newtype) so a neighbour-clobbering invalidate is unrepresentable. Cover the **async path in the same change** (don't leave a second un-coherent surface). Note: `embedded-dma` is necessary-but-not-sufficient — it doesn't model non-coherent caches/alignment, those stay your glue.

---

### LOW — Async vs blocking duality

> **Industry practice.** Two valid models: esp-hal `DriverMode` generic (`into_async()` installs the ISR) vs embassy one-type/both-trait-sets + the blessed `BlockingAsync` adapter (wrap blocking in an async fn). Both agree: don't duplicate the driver type per mode; provide full async-trait coverage. [esp-hal DEVELOPER-GUIDELINES; embassy BlockingAsync]

**Current state.** `follows-well`. Single-type, cargo-`async`-gated; async traits on the exact blocking types; full standard-trait coverage (DelayNs, Wait, SpiBus, I2c, embedded-io-async). **Load-bearing invariant:** enabling `async` never installs a vector — the app routes the trap. `embassy` feature deliberately decoupled from `async` (time-driver works on BS2X; async drivers still WS63-only). SPI/I2C async just await-wrap the blocking path (hand-rolled `BlockingAsync`).

**Decision.** *Keep cargo-feature-gated app-routed async, or move to esp-hal `into_async()` auto-install?*

**Recommendation.** **Leave the core design.** The feature-gate is the *right* call precisely because the HAL refuses to auto-install ISRs (no `into_async` moment to hang installation on; the app owns the trap). **Do not** adopt `into_async()` — it conflicts with the invariant that lets `async`/`embassy` unify safely across a workspace. Two small adapts: (1) put `--features async` and `async,embassy` in the CI matrix so gated blocks don't bit-rot; (2) document the non-IRQ-parking SPI/I2C async in **rustdoc** (not just inline) — an embassy user awaiting a long `SpiBus::transfer` busy-blocks the executor; surface the decoupled-`embassy`-feature story in crate-level rustdoc too (currently only in Cargo.toml + Chinese docs).

---

## 3. The chip-selection wart

`default = ["chip-ws63", "rt", "dep:critical-section"]`; `chip-ws63` **XOR** `chip-bs21`, each pulling exactly one PAC. Mutual exclusion is enforced correctly by `compile_error!` (`lib.rs:32`). But mutually-exclusive features violate Cargo's additivity rule, with three concrete consequences:

- **`--all-features` cannot resolve** (pulls both PACs) — acknowledged in `Cargo.toml:150`; CI replaces it with a matrix `["chip-ws63,rt,async,embassy", "chip-bs21,rt"]`.
- **docs.rs renders wrong** — **no `[package.metadata.docs.rs]`** at all, so docs.rs builds default features on its default *host* target (no riscv target, may not render the BS21 surface). This is esp-hal's single most important multi-chip-docs move, and it's absent.
- **semver-checks default ambiguity** — the default build is WS63-only; the BS2X surface isn't checked by the default invocation.

> **What other multi-chip HALs do.** **(1) One crate, chip = mutually-exclusive feature** (esp-hal: `esp32`/`esp32c3`/…). esp-hal mitigates with three moves: **no chip in `default`** (forces an explicit pick), `assert_unique_features!`, and **docs.rs pinned to one chip + the riscv target**. **(2) Per-chip wrapper crates** (nrf-rs `nrf52840-hal`, stm32-rs split families) — fully eliminates the `--all-features` wart, each chip docs/publishes independently, at the cost of duplicated boilerplate and a harder shared-driver story. [Cargo Book features; esp-hal Cargo.toml/lib.rs; docs.rs metadata]

**Decisions & recommendations:**

| Move | Decision | Recommendation |
|---|---|---|
| `[package.metadata.docs.rs]` | which chip to pin; pin the riscv target? | **Do now.** `default-target = "riscv32imfc-unknown-none-elf"`, `features = ["chip-ws63","rt","async","embassy"]`. Near-zero-risk, unblocks correct docs. Mirrors esp-hal (pins esp32c6). |
| Keep `chip-ws63` in `default`? | esp-hal ships **no** default chip (a BS21 user who forgets `--no-default-features` gets a *loud* `compile_error`, not a silent WS63 build) vs ergonomics for the dominant WS63 audience (`cargo build`/`doc` "just works") | ~~**Keep `chip-ws63` default for now**~~ → **决定 2026-06-15：采纳 esp-hal 式，`default` 不放芯片**（`default = ["rt","dep:critical-section"]`），强制显式选芯片。最正确、最对齐 esp-hal。代价：每个消费者（examples/* / tests-hil / hisi-rs-template）须在 hal 依赖上声明 `features=["chip-ws63"]`；`cargo check --workspace` 裸跑会因 hal 无默认芯片而报错 → 改用 per-chip 命令；`compile_error!` 要为「零芯片」也给清晰提示；CLAUDE.md build 命令同步。纳入 0.5.0。 |
| Split into per-chip crates? | eliminates the wart entirely vs duplicated boilerplate; most driver code here is already chip-neutral via `soc::pac` aliases | **Stay single-crate-feature** — the better fit at 2 chips. Revisit only past 2-3 chips or if cfg density becomes unmanageable. |

*Publishing caveat:* the `[patch.crates-io]` for `bs2x-pac` (`Cargo.toml:157`) exists because Cargo resolves *all* optional deps when locking — even a WS63-only default build must resolve `bs2x-pac`. If it's ever yanked/lags, published default-feature builds could fail to resolve even though they never compile it.

---

## 4. Suggested sequencing

### Fold into the 0.5.0 batch (cheap, mechanical, aligned with the config pass / pre-1.0 window)

1. **`[package.metadata.docs.rs]`** — near-zero-risk, unblocks docs immediately. *(chip wart)*
2. **`rust-version` in `[workspace.package]`** + one-line MSRV policy. *(const-msrv)*
3. **`#[non_exhaustive]` on the 8 `*Error` enums** + growth-prone mode enums (skip hardware-closed ones). *(errors / traits)*
4. **`defmt` feature** + `Format` derives on errors & user-facing config enums; normalize error-enum derives to `Copy + Eq`; add the `--features defmt` CI job. *(diagnostics)*
5. **Free error-mapping fixes:** `SpiError::Overflow → Overrun`; wire `i2c_v151::I2cError` into `embedded_hal::i2c::Error`. *(errors)*
6. **`#![warn(missing_docs)]`** now (flip to `deny` after backfill); rename `get_gpio_mux → gpio_mux`; ` ```ignore ``` → ``` ```no_run `. *(naming-docs — the HIGH item, but the *enforcement switch* is cheap; the backfill is the work)*
7. **Safe `reborrow()` in the `peripheral!` macro** — purely additive, SemVer-safe. *(ownership)*
8. **Route the `assert!`-constructor cases** (`PwmChannel`/`Km`/`UlpGpio`) **through the typed-config skill** — the boundary item; don't let it slip. *(errors ↔ typed-config)*
9. **Rustdoc notes:** non-IRQ-parking SPI/I2C async; `embedded-hal-bus` pointer for SPI multi-device; decoupled `embassy` feature. *(async / traits)*
10. **Add `async`/`embassy` to the CI build matrix** so gated blocks don't rot. *(async)*
11. **Delete vestigial `private.rs:24-27` markers.** *(traits)*

### Defer (breaking, larger, or better-timed elsewhere)

- **Remove legacy `GpioPin<MODE>`** + explicit `Flex` transitions + `degrade()` — deprecate-in-0.5.x, delete next breaking. *(typestate-gpio)*
- **Builder-lite on bus configs + private fields / `non_exhaustive`** + fold I2S two-step into construction + raw-scalar→newtype args — next breaking release (semver-checks flags the field-visibility change). *(construction)*
- **DMA owned-buffer transfer guard + cache-in-type + `embedded-dma`** — build **when DMA is first exposed** to peripheral/app code (cover the async path in the same change). *(dma)*
- **Interrupt named-handler routing** — file as an **`hisi-riscv-rt`** task (coupled to the HAL but lives in RT); the `bind_interrupts!` shim is a later HAL follow-up. *(interrupts)*
- **`PwmChannel::new(&Pwm)` → `split()`** + thread `'d` through `into_*` + promote pins to singletons — ownership cleanups, breaking, batch with the GPIO/construction breaks. *(ownership)*
- **Drop-to-disable policy** — decide deliberately; if adopted, scope to hazardous-state drivers and encode in the macro. *(ownership)*

---

## 5. Sources

**esp-hal**
- DEVELOPER-GUIDELINES.md (Config/BuilderLite/apply_config, DriverMode, fallible-over-panic, defmt::Format) — https://github.com/esp-rs/esp-hal/blob/main/documentation/DEVELOPER-GUIDELINES.md
- lib.rs (deny(missing_docs), assert_unique_features) — https://github.com/esp-rs/esp-hal/blob/main/esp-hal/src/lib.rs
- Cargo.toml (chip features, default set, docs.rs metadata) — https://github.com/esp-rs/esp-hal/blob/main/esp-hal/Cargo.toml
- #1188 GpioPin typestate removal — https://github.com/esp-rs/esp-hal/issues/1188 ; #740 Flex — https://github.com/esp-rs/esp-hal/issues/740
- Peripheral/ownership (reborrow) — https://docs.espressif.com/projects/rust/esp-hal/1.0.0-beta.0/esp32/esp_hal/peripheral/trait.Peripheral.html ; 1.0 release — https://developer.espressif.com/blog/2025/10/esp-hal-1/
- DMA buffers (prepare() cache + alignment) — https://docs.espressif.com/projects/rust/esp-hal/1.1.1/esp32h2/src/esp_hal/dma/buffers.rs.html
- interrupt (set_interrupt_handler, #[handler]) — https://docs.espressif.com/projects/rust/esp-hal/1.0.0-beta.1/esp32c3/esp_hal/interrupt/index.html
- uart::Config (non_exhaustive + with_* builders) — https://docs.espressif.com/projects/rust/esp-hal/1.0.0-rc.0/esp32c3/esp_hal/uart/struct.Config.html
- API guidelines (HackMD) — https://hackmd.io/@esp-rs/Hy8RR5FkC ; MSRV — https://crates.io/crates/esp-hal

**embassy**
- bind_interrupts! (Binding<I,H> witness) — https://docs.embassy.dev/embassy-rp/git/rp2040/macro.bind_interrupts.html
- BlockingAsync adapter — https://docs.embassy.dev/embassy-embedded-hal/git/default/adapter/struct.BlockingAsync.html
- embassy-stm32 gpio.rs (Flex set_as_input/output &mut self) — https://github.com/embassy-rs/embassy/blob/main/embassy-stm32/src/gpio.rs

**embedded-hal / embedded-dma / embedded-hal-bus**
- i2c module (Error/ErrorKind/ErrorType) — https://docs.rs/embedded-hal/latest/embedded_hal/i2c/index.html
- migration 0.2→1.0 (error: Debug required) — https://github.com/rust-embedded/embedded-hal/blob/master/docs/migrating-from-0.2-to-1.0.md
- digital.rs (cfg_attr defmt-03 ErrorKind) — https://github.com/rust-embedded/embedded-hal/blob/master/embedded-hal/src/digital.rs
- MSRV policy (msrv.md) — https://github.com/rust-embedded/embedded-hal/blob/master/docs/msrv.md
- embedded-hal-bus SPI sharing — https://docs.rs/embedded-hal-bus/latest/embedded_hal_bus/spi/index.html
- embedded-dma ReadBuffer/WriteBuffer — https://docs.rs/embedded-dma/latest/embedded_dma/ ; WriteBuffer contract — https://docs.rs/embedded-dma/latest/embedded_dma/trait.WriteBuffer.html

**Rust API Guidelines / Cargo / Embedded Book / defmt / riscv-rt**
- Naming (C-GETTER/C-CONV/C-WORD-ORDER) — https://rust-lang.github.io/api-guidelines/naming.html
- Future-proofing (C-SEALED, non_exhaustive, C-STRUCT-PRIVATE) — https://rust-lang.github.io/api-guidelines/future-proofing.html
- Interoperability (C-GOOD-ERR) — https://rust-lang.github.io/api-guidelines/interoperability.html ; Checklist (C-SAFETY-DOC/C-FAILURE/C-EXAMPLE) — https://rust-lang.github.io/api-guidelines/checklist.html
- Cargo Book — Features (additive) — https://doc.rust-lang.org/cargo/reference/features.html ; rust-version — https://doc.rust-lang.org/cargo/reference/rust-version.html ; docs.rs metadata — https://docs.rs/about/metadata
- Embedded Rust Book — HAL design patterns — https://doc.rust-lang.org/stable/embedded-book/design-patterns/hal/index.html
- defmt book — Format (cfg_attr gating) — https://defmt.ferrous-systems.com/format
- riscv-rt interrupts (direct vs vectored, device.x) — https://docs.rs/riscv-rt/latest/riscv_rt/interrupts/index.html ; svd2rust #404 — https://github.com/rust-embedded/svd2rust/issues/404
- Japaric "Memory-safe DMA transfers" — https://blog.japaric.io/safe-dma/
- therealprof "Revamping Rust Embedded Error Handling" — https://therealprof.github.io/blog/revamping-rust-embedded-error-handling/