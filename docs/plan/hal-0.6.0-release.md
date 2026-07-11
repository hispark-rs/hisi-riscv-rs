# hisi-riscv-hal v0.6.0 正式版里程碑计划

## Summary

`hisi-riscv-hal v0.6.0` 是 HAL stable API stabilization release。它的发布门槛是
默认 stable API、HIL 证据、safe/unsafe 边界、CI/release/docs 全部闭合；WS63 RF / Wi-Fi
init / scan / connect / ping 属于下游 connectivity track，不阻塞 HAL 0.6.0。

`0.6.0` 也是 `hisi-riscv-hal` 名称下最后一个主 release。正式版发布后，后续
重命名为 `hisi-hal` 的 H0 迁移按
[Connectivity 全栈计划](hisi-connectivity-stack.md) 单独执行；重命名不得反向扩大
本计划的 release gate，也不得夹带 API 重构。

RF 推进中如果暴露 HAL 已稳定 API 的 bug，可以阻塞 0.6.0；如果只是需要新增能力，默认先进
`unstable`，等 HIL 与 soundness 证据闭合后再毕业。

## Milestones

### M0 -- Blocking Bug Closure

- 修复 `#16`：WS63 I2C completion 轮询必须对齐 SDK `int_done` 语义，而不是把
  `int_tx` / `int_rx` 当作事务完成信号。
- 复核 `#17` 的边界：0.6.0 只稳定当前已经 HIL 的 eFuse / LSADC 子集，不要求完整
  eFuse write 或 LSADC analog/data path 毕业。
- 若 `#17` 标题继续误导，将其改为 post-0.6 full feature validation，并在 issue 中说明
  0.6.0 的 stable 边界。

### M1 -- Stable API Freeze

- 默认 `chip-ws63,rt` 暴露项必须全部出现在
  `docs/src/reference/10-stable-api.md`。
- 没有 WS63 HIL 证据、或 soundness 尚未闭合的 public item 必须继续 gated behind
  `unstable`。
- 不扩大 0.6.0 stable 面：DMA、embassy、BS2X、SFC/PKE/SPACC/KM、RTC、UartDma、
  GPIO wait/IRQ async 继续实验性。
- `CHANGELOG.md` 的 `[Unreleased]` 只记录 alpha.2 之后的 bug fix、docs、CI、gate
  cleanup，不重复宣传 alpha.1 已有内容。

### M2 -- Evidence Gate

- 本地与 CI 必须通过：
  - `cargo fmt --all -- --check`
  - `python3 scripts/check-register-access.py`
  - `cargo clippy --locked --no-default-features --features chip-ws63,rt,async,embassy -- -D warnings`
  - `cargo clippy --locked --no-default-features --features chip-ws63,rt,async,embassy,defmt -- -D warnings`
  - `cargo clippy --locked --no-default-features --features chip-bs21,rt,unstable -- -D warnings`
  - `cargo check --locked --no-default-features --features chip-ws63,rt,async,embassy --release`
  - `cargo check --locked --no-default-features --features chip-bs21,rt,unstable --release`
  - `cargo doc --locked --no-deps --document-private-items --no-default-features --features chip-ws63,rt,async,embassy` with `RUSTDOCFLAGS=-D warnings`
- BS2X negative gate：`chip-bs21,rt` without `unstable` 必须失败并包含实验性提示。
- HIL gate：
  - 跑默认 WS63 HAL embedded-test suite：`chip-ws63,rt`。
  - 跑一组 `unstable` smoke HIL，至少证明当前已有 unstable HIL 未回归。
  - `hil-loopback`、`hil-rtc`、示例级 `hil-smoke.sh` 不作为 crates.io 0.6.0 blocker。

### M3 -- Release Candidate

- 发布 `0.6.0-rc.1`，rc 后只接受编译、文档、CI、release、stable API 行为 bug 修复。
- rc 后冻结 stable API、默认 feature 语义和 breaking rename。
- 父仓同步：
  - 更新 submodule pointer / patch 状态。
  - 用 rc 验证 happy path docs 和 template。
  - 父仓 changelog 标记 anchor 为 `hisi-riscv-hal 0.6.0-rc.1`。

### M4 -- Final v0.6.0 Release

- 将 HAL 版本提升到 `0.6.0`，更新 changelog 日期。
- `cargo generate-lockfile --locked` 后确认 `Cargo.lock` 无意外漂移。
- `cargo package --locked --no-verify` 通过。
- 打 tag `v0.6.0`，观察 publish workflow，确认 crates.io 发布成功。
- 父仓更新 HAL submodule pointer，并发布 ecosystem release train：
  `hisi-riscv-rs v0.6.0`，anchor 为 `hisi-riscv-hal 0.6.0`。

## Test Cases And Scenarios

- Stable default consumer：只启用 `chip-ws63,rt` 时，可使用 stable API 清单中的
  GPIO、UART、SPI0、I2C0、Timer、TCXO、PWM0、WDT、TRNG、eFuse read、LSADC subset、
  TSENSOR subset、I2S liveness 等默认面。
- Experimental consumer：加 `unstable` 后可继续使用实验面，但文档明确 minor 版本可能破坏。
- BS2X consumer：`chip-bs21,rt` without `unstable` 必须失败；`chip-bs21,rt,unstable`
  必须构建通过，但没有 stable/HIL 承诺。
- Hardware evidence：默认 HAL embedded-test suite 在真实 WS63 通过；`#16` I2C 修复必须有
  HIL 或 register-level HIL 证明。
- Docs evidence：stable API reference、HAL changelog、release guide、parent happy path
  不能互相矛盾，且不能暗示 DMA/embassy/BS2X/RF connectivity 已经 stable。

## Assumptions

- HAL 0.6.0 不等待 WS63 Wi-Fi init、scan、connect 或 ping。
- HAL 0.6.0 不等待 PAC/SVD 全覆盖审计完成；只要求 stable HAL 面不依赖缺失 PAC 字段。
- eFuse 写入、完整 LSADC analog/data path、DMA stable graduation、embassy stable graduation
  全部推迟到 0.6.x 或 0.7。
- 如果 rc 阶段发现 stable API 需要 breaking 修正，重新发 `0.6.0-rc.2`，不直接发 final。
