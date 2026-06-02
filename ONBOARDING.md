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

_TODO_

## Get Started

_TODO_

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
