# Welcome to HISPARK-RS

## How We Use Claude

Based on sanchuanhehe's usage over the last 30 days:

Work Type Breakdown:
  Build Feature     █████████████░░░░░░░  63%
  Plan Design       █████░░░░░░░░░░░░░░░░  25%
  Improve Quality   ██░░░░░░░░░░░░░░░░░░░  12%

Top Skills & Commands:
  /exit                            ████████████████████  16x/month
  /effort                          ███████████████████░  15x/month
  /model                           ███████████████░░░░░  12x/month
  /code-review                     █████████░░░░░░░░░░░   7x/month
  /config                          ████░░░░░░░░░░░░░░░░   3x/month
  /claude-automation-recommender   ████░░░░░░░░░░░░░░░░   3x/month
  /sandbox                         ███░░░░░░░░░░░░░░░░░   2x/month

Top MCP Servers:
  None configured — this team works without MCP servers.

## Your Setup Checklist

### Codebases
- [ ] hisi-riscv-rs — https://github.com/hispark-rs/hisi-riscv-rs (the monorepo; clone with `git submodule update --init --recursive` — crates, examples, guides, and SVD repos are all submodules)
- [ ] hisi-riscv-rust-toolchain — https://github.com/hispark-rs/hisi-riscv-rust-toolchain (custom rustc with the `riscv32imfc-unknown-none-elf` target baked in; **required to build** — install + `rustup toolchain link hisi-riscv`)
- [ ] hisi-riscv-qemu — https://github.com/hispark-rs/hisi-riscv-qemu (QEMU fork with `-M ws63/bs21/bs21e/bs22/bs20`; software-in-the-loop, no silicon needed — used by the `qemu-smoke` skill)
- [ ] fbb_ws63 — https://gitcode.com/HiSpark/fbb_ws63 (official HiSilicon C SDK; the **ground-truth** for register/peripheral behavior, read-only reference)
- [ ] fbb_bs2x — https://gitcode.com/HiSpark/fbb_bs2x (official C SDK for BS2X; register/peripheral ground-truth for BS21/BS20/BS22)
- [ ] hisiflash — https://github.com/hispark-rs/hisiflash (serial flash CLI; needed for hardware-in-the-loop — the `hil-smoke` skill)
- [ ] esp-hal — https://github.com/esp-rs/esp-hal (reference HAL the WS63 driver patterns are modeled on; read-only reference)

### MCP Servers to Activate
- [ ] None — the team currently uses no MCP servers. Nothing to set up here.

### Skills to Know About
- [ ] /run-ws63-rs — build / check / clippy / fmt the workspace with the `hisi-riscv` toolchain (start here to confirm your toolchain works)
- [ ] /submodule-commit — land changes spanning submodules in dependency order, then bump the parent pointer (the monorepo is all submodules)
- [ ] /qemu-smoke — build an example for a chip and boot it in the QEMU fork, asserting UART banner / GPIO toggle / IRQ delivery
- [ ] /hil-smoke — the silicon twin of qemu-smoke: flash a real board and assert UART (run with `--preflight` to check the rig with no board attached)
- [ ] /qemu-vs-hil — run an example through both QEMU and silicon and diff the markers (the emulator↔hardware parity check)
- [ ] /release-train — drive a repo's tag-triggered release: push the tag, watch the CI matrix, verify the published assets
- [ ] /code-review — review the current branch or a GitHub PR; `/code-review ultra` launches the multi-agent cloud review (~7x/month)
- [ ] /claude-automation-recommender — analyze the codebase and suggest Claude Code automations (skills / hooks / agents)
- [ ] /effort — tune how much reasoning effort Claude spends (the team's most-used control — dial up for architecture/driver work, down for quick edits)
- [ ] /model — switch the active model (Opus/Sonnet/Haiku) to match the task
- [ ] /config — adjust Claude Code settings (theme, model, permissions, etc.)
- [ ] /sandbox — run work in a sandboxed environment

## Team Tips

Straight from `CLAUDE.md` — the conventions that keep this monorepo sane:

- **Submodules are everything.** Core submodules: `crates/pac/{ws63-pac,bs2x-pac}` (PACs), `crates/{hisi-riscv-hal,hisi-riscv-rt}` (HAL + runtime), `examples/{ws63,bs21,bs20}` (chip-specific examples), `chips/{ws63,bs2x}/guide` (hardware guides), `chips/ws63/rf/ws63-RF` (Wi-Fi blob). SVD repos (`ws63-svd`, `bs2x-svd`) are separate root-level submodules. Always clone/update with `git submodule update --init --recursive`.
- **Submodule-first, then bump the pointer.** When you edit a file inside a submodule, commit *inside the submodule* first, push it, then update and commit the parent repo's submodule pointer. Don't commit the parent pointer to an unpushed submodule commit. (The `/submodule-commit` skill does this for you.)
- **Build with the custom `hisi-riscv` toolchain.** The workspace default target is `riscv32imfc-unknown-none-elf` (hard-float ilp32f, no atomics), baked into the `hisi-riscv` toolchain as a builtin — install it first (see `rust-toolchain.toml`), no `-Z build-std` needed. Core loop: `cargo build` (libs + blinky), `cargo check --workspace`, `cargo clippy`, `cargo fmt --all -- --check`.
- **Official C SDK is the single source of truth.** The WS63 and BS2X chips are undocumented — the official HiSilicon C SDKs (`fbb_ws63`, `fbb_bs2x`) are ground-truth for register offsets, bit fields, and init sequences. Before trusting or writing a driver, grep the SDK for the registers you're touching. The `register-auditor` subagent automates this cross-check. `esp-hal` is the reference for *Rust HAL patterns*, not register behavior.
- **Validate in QEMU, then on silicon.** `/qemu-smoke` boots firmware in the QEMU fork (no hardware); `/hil-smoke` flashes a real board; `/qemu-vs-hil` diffs the two — the timing-sensitive checks (160 MHz UART baud, 24 MHz TCXO timer) are exactly what QEMU can't prove. HIL is scaffolding until a board is wired up (`hil/README.md`).
- **Read the docs before large changes.** `docs/architecture/overview.md` for the whole picture, the review ledger in `docs/review/architecture-review-2026-05.md`, and `ROADMAP.md` for the remediation plan and known defects. **Connectivity (Wi-Fi/BLE/SLE) is the north star.**
- **No `std`, and register access is `unsafe`.** `#![no_std]` throughout, no heap — use fixed arrays. Raw PAC writes are `unsafe`; encapsulate them inside driver methods, never leak them to callers.

## Get Started

First task for a new teammate — get a clean build going end to end:

1. Clone the monorepo with submodules: `git clone --recurse-submodules https://github.com/hispark-rs/hisi-riscv-rs` (or `git submodule update --init --recursive` if you already cloned).
   - Note: BS21/BS20 examples are in isolated workspaces (`examples/bs21/`, `examples/bs20/`) because `hisi-riscv-hal` builds for exactly one chip at a time (enforced by `compile_error!`). WS63 examples are in the root workspace.
2. Install the custom `hisi-riscv` toolchain per `rust-toolchain.toml` and link it: `rustup toolchain link hisi-riscv "$PWD/stage2"`.
3. Build the libraries + blinky: `cargo build`, then sanity-check the whole tree: `cargo check --workspace` (the `/run-ws63-rs` skill wraps this).
4. Boot blinky in the emulator to see it run end to end: `/qemu-smoke ws63 blinky`.
5. Read `docs/architecture/overview.md` and `ROADMAP.md` to see where the project is headed before picking up real work.

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
