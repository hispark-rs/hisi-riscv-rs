# Welcome to WS63-RS

## How We Use Claude

Based on sanchuanhehe's usage over the last 30 days:

Work Type Breakdown:
  Build Feature  ████████████████████  50%
  Plan Design    ███████████████░░░░░  38%
  Write Docs     █████░░░░░░░░░░░░░░░░  12%

Top Skills & Commands:
  /effort        ████████████████████  18x/month
  /model         █████████████░░░░░░░  12x/month
  /code-review   ████████░░░░░░░░░░░░   7x/month
  /goal          ███░░░░░░░░░░░░░░░░░░   3x/month
  /config        ███░░░░░░░░░░░░░░░░░░   3x/month
  /sandbox       ██░░░░░░░░░░░░░░░░░░░   2x/month

Top MCP Servers:
  None configured — this team works without MCP servers.

## Your Setup Checklist

### Codebases
- [ ] ws63-rs — https://github.com/sanchuanhehe/ws63-rs (main monorepo; clone with `git submodule update --init --recursive` — ws63-pac, ws63-hal, ws63-rt, ws63-examples are submodules, and ws63-svd / ws63-RF are nested submodules)
- [ ] ws63-qemu — sister QEMU fork for software-in-the-loop validation (no silicon needed)
- [ ] ws63-rust-toolchain — custom `ws63` rustc with the `riscv32imfc-unknown-none-elf` target baked in (install + `rustup toolchain link ws63`)
- [ ] esp-hal — reference HAL the WS63 driver patterns are modeled on (read-only reference)
- [ ] fbb_ws63 — official HiSilicon C SDK; the **ground-truth** for register/peripheral behavior (read-only reference)

### MCP Servers to Activate
- [ ] None — the team currently uses no MCP servers. Nothing to set up here.

### Skills to Know About
- [ ] /effort — tune how much reasoning effort Claude spends on a task (the team's most-used command — dial it up for architecture/driver work, down for quick edits)
- [ ] /model — switch the active model (Opus/Sonnet/Haiku) to match the task
- [ ] /code-review — review the current branch or a GitHub PR; `/code-review ultra` launches the multi-agent cloud review
- [ ] /goal — set a persistent goal for the session so Claude keeps it in view across turns
- [ ] /config — adjust Claude Code settings (theme, model, permissions, etc.)
- [ ] /sandbox — run work in a sandboxed environment

## Team Tips

Straight from `CLAUDE.md` — the conventions that keep this monorepo sane:

- **Submodules are everything.** `ws63-pac`, `ws63-hal`, `ws63-rt`, `ws63-examples` are standalone repos linked as submodules; `ws63-svd` is nested under `ws63-pac` and `ws63-RF` under `ws63-rf-rs`. Always clone/update with `git submodule update --init --recursive`.
- **Submodule-first, then bump the pointer.** When you edit a file inside a submodule, commit *inside the submodule* first, then update and commit the parent repo's submodule pointer. Don't commit the parent pointer to an unpushed submodule commit.
- **Build with the custom `ws63` toolchain.** The workspace default target is `riscv32imfc-unknown-none-elf` (hard-float ilp32f, no atomics), baked into the `ws63` toolchain as a builtin — install it first (see `rust-toolchain.toml`), no `-Z build-std` needed. Core loop: `cargo build` (libs + blinky), `cargo check --workspace`, `cargo clippy`, `cargo fmt --all -- --check`.
- **fbb_ws63 is the single source of truth.** The WS63 chip is undocumented — the official C SDK is the ground-truth for register offsets, bit fields, and init sequences. Before trusting or writing a driver, grep `fbb_ws63` for the registers you're touching. `esp-hal` is the reference for *Rust HAL patterns* (GPIO type-state, sealed traits), not register behavior.
- **Read the docs before large changes.** `docs/architecture/overview.md` for the whole picture, the review ledger in `docs/review/architecture-review-2026-05.md`, and `ROADMAP.md` for the remediation plan and known defects. Connectivity (Wi-Fi/BLE/SLE) is the north star.
- **No `std`, and register access is `unsafe`.** `#![no_std]` throughout, no heap — use fixed arrays. Raw PAC writes are `unsafe`; encapsulate them inside driver methods, never leak them to callers.

## Get Started

First task for a new teammate — get a clean build going end to end:

1. Clone the monorepo with submodules: `git clone --recurse-submodules https://github.com/sanchuanhehe/ws63-rs` (or `git submodule update --init --recursive` if you already cloned).
2. Install the custom `ws63` toolchain per `rust-toolchain.toml` and link it: `rustup toolchain link ws63 "$PWD/stage2"`.
3. Build the libraries + blinky: `cargo build`, then sanity-check the whole tree: `cargo check --workspace`.
4. Read `docs/architecture/overview.md` and `ROADMAP.md` to see where the project is headed before picking up real work.

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
