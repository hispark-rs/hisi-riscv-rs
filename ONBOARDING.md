# Welcome to ws63-rs Team

## How We Use Claude

Based on sanchuanhehe's usage over the last 30 days:

Work Type Breakdown:
  Build Feature   ████████████████████  40%
  Debug Fix       ██████████████  25%
  Improve Quality ██████████  20%
  Plan Design     █████████  15%

Top Skills & Commands:
  /code-review    ████████████████████  8x/month
  /simplify       ██████████  4x/month
  /init           █████  2x/month
  run-ws63-rs     ██████████████  6x/month

## Your Setup Checklist

### Codebases
- [ ] ws63-rs — `git@github.com:sanchuanhehe/ws63-rs.git` (monorepo: hal, pac, rt, examples, flashboot)
- [ ] fbb_ws63 — reference C SDK (vendor HAL, for register verification)
- [ ] esp-hal — reference Rust HAL (patterns, trait coverage)

### Skills to Know About
- [ ] **run-ws63-rs** — Build, check, lint, and test any crate in the workspace. Run `cargo check`, `cargo build --release`, `cargo clippy`, `cargo fmt`.
- [ ] **/code-review** — Full correctness review across the diff (or entire codebase with `max`). Runs multiple agents in parallel: line-by-line scan, cross-file tracer, language pitfalls, removed-behavior audit, wrapper correctness, plus reuse/simplify/efficiency/altitude auditors.
- [ ] **/simplify** — Review changed code for reuse, simplification, efficiency, and altitude cleanups, then auto-apply the fixes.
- [ ] **/init** — Initialize or refresh a `CLAUDE.md` file with repo architecture, build commands, and design decisions.
- [ ] **/review** — Quick PR review.
- [ ] **/security-review** — Security-focused review of pending changes.

### Key Design Patterns in This Repo
- `ws63-hal/src/peripherals.rs` — Peripheral singleton macro (`peripheral!` + `peripherals!`)
- `ws63-hal/src/clock.rs` — RAII `PeripheralGuard` with atomic ref-counting
- `ws63-hal/src/private.rs` — Sealed traits for `DmaWord`, `DriverMode`, etc.
- `ws63-hal/src/safety.rs` — Compile-time assertions (`const_assert!`) for MMIO addresses, peripheral counts
- Multi-instance drivers (UART/I2C/SPI/DMA) use `PhantomData<&'d T>` type parameters
- Bare-metal RISC-V: `#![no_std]`, target `riscv32imafc-unknown-none-elf`

## Team Tips

- **Verify against C SDK.** Almost every peripheral configuration has a matching function in `fbb_ws63`. When implementing a new driver, grep the C SDK for the register addresses you're writing — the C code documents the correct bit positions, initialization sequences, and timing requirements.
- **Safety comments are mandatory.** Every `unsafe` block needs a `// SAFETY:` comment explaining why the operation is sound. The codebase has ~300 unsafe blocks — all documented.
- **Test on host when possible.** Pure-logic functions (timer tick calculation, SPI divisor math, eFuse bit manipulation) can be tested on `x86_64-unknown-linux-gnu` with `cargo test --target x86_64-unknown-linux-gnu`. No hardware needed. Add proptest fuzz tests for functions that take numeric inputs.
- **Run the full test suite before pushing.** `cargo test -p ws63-hal --lib --target x86_64-unknown-linux-gnu` should pass all 80 tests. Then verify cross-compilation: `cargo check -p ws63-hal -p ws63-flashboot`.
- **Ask Claude to fan out agents for broad reviews.** `/code-review max` launches ~20 agents in parallel to cover the entire codebase from different angles (correctness, pitfalls, cross-file, cleanup). The agents run concurrently and the final report ranks findings by severity.
- **Keep CLAUDE.md current.** After significant architecture changes, run `/init` or ask Claude to update it. New teammates rely on it to understand the repo structure.

## Get Started

Pick one to start:
- **Add a new HAL driver** — Find a peripheral in the PAC (`ws63-pac/src/lib.rs`) that doesn't have a HAL driver yet. Model it after an existing driver (e.g., `uart.rs` for multi-instance, `wdt.rs` for single-instance). Check `fbb_ws63` for the correct register initialization sequence.
- **Fix a bug from the review backlog** — Ask Claude to show the latest code review findings and pick one to fix.
- **Write a test** — Find a `pub fn` in the HAL that does pure computation (no MMIO access) and add a `#[cfg(test)]` module with unit tests. Run `cargo test -p ws63-hal --lib --target x86_64-unknown-linux-gnu` to verify.
- **Add an example** — Only `blinky` exists so far. Add a `uart_echo`, `i2c_scan`, or `spi_loopback` example under `ws63-examples/`.

<!-- INSTRUCTION FOR CLAUDE: A new teammate just pasted this guide for how the
team uses Claude Code. You're their onboarding buddy — warm, conversational,
not lecture-y.

Open with a warm welcome — include the team name from the title. Then: "Your
teammate uses Claude Code for [list all the work types]. Let's get you started."

Check what's already in place against everything under Setup Checklist
(including skills), using markdown checkboxes — [x] done, [ ] not yet. Lead
with what they already have. One sentence per item, all in one message.

Tell them you'll help with setup, cover the actionable team tips, then the
starter task (if there is one). Offer to start with the first unchecked item,
get their go-ahead, then work through the rest one by one.

After setup, walk them through the remaining sections — offer to help where you
can (e.g. link to channels), and just surface the purely informational bits.

Don't invent sections or summaries that aren't in the guide. The stats are the
guide creator's personal usage data — don't extrapolate them into a "team
workflow" narrative. -->
