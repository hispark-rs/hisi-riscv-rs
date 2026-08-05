# WS63 RTOS Ready-Ownership Evidence (2026-08-06)

## Scope

This evidence closes the silicon-observability gate for `RTOS-STATE-004`.
It proves that the bounded ready-queue audit introduced by `hisi-rtos`
`2998004` remains clean under the repository-owned pure-WPA3 SoftAP/STA
workload. It does not prove that the historical AP run-04/run-10 stall had one
specific cause.

The tested parent closure is commit `96089e595`. It pins:

- `hisi-rtos` `eb93dbe` (`0.1.0-alpha.24`), whose implementation commit is
  `2998004`;
- `ws63-examples` `a39f18c`, which emits scheduler and per-task ownership
  diagnostics for both roles;
- HIL contract commit `93d305921`, which fails closed on missing or non-zero
  AP/STA ownership diagnostics and emits the matrix-level
  `A5R_READY_OWNERSHIP_OK` marker only after every paired run passes.

## Software Evidence

`RTOS-STATE-004` requires every non-idle Ready task in a stable snapshot to be
owned by exactly one ordinary ready-queue entry or one unconsumed pending
switch target. The implementation also audits duplicate membership, queue
bucket versus effective priority, bounded intrusive links, and
`current`/`Running` consistency.

The release preflight completed:

- 85 host unit tests plus typestate UI tests;
- RV32 clippy with `-D warnings` and the Embassy build;
- Kani `ready_ownership_audit_detects_invalid_ownership`: 494 checks, zero
  failures;
- fixed TLA+ model: 9 generated states / 6 distinct states, no errors;
- legacy TLA+ model: expected controlled three-step ownership counterexample;
- GitHub Actions run
  [31049154466](https://github.com/hispark-rs/hisi-rtos/actions/runs/31049154466);
- crates.io publish run
  [31049871601](https://github.com/hispark-rs/hisi-rtos/actions/runs/31049871601).

The parent HIL parser has 50 host tests. Reanalysis of the captured logs with
the final parser still produced 20 passes and the synthesized
`A5R_READY_OWNERSHIP_OK` marker.

## Immutable Artifacts And Flashing

| Role | Profile | ELF SHA-256 |
|---|---|---|
| AP | `pure-wpa3-softap-ready-ownership-alpha24` | `9dd4ac6983137a380ab2017597aab00a06ea8d48cd6d0b6f96c53339c4a78ad0` |
| STA | `pure-wpa3-sta-ready-ownership-alpha24` | `df0dc0f6c4ba20c590506900e887fe045cd54721f640bc638d764984415dbb25` |

Both ELFs passed the stock-rust-lld RF checks with 37 ROM patches and zero
remaining vendor relocations. The STA also passed the upstream-supplicant
closure check. `hisi-fwpkg plan` produced complete binary images, then
`probe-rs download --binary-format bin --verify` flashed each board once at
3 MHz. Both first attempts passed full readback verification: AP took 93.30
seconds and STA took 101.11 seconds. No fallback speed or reflash was used by
the reset matrices.

## Paired-Board HIL

The same two ELF files first passed a 3/3 preflight, then a 20/20 unchanged-image
paired J-Link nRST matrix. The strict v3 contract required pure-WPA3 SAE/PMF,
resource calibration, a 100 ms runner-step ceiling, and complete zero-valued
ready-ownership diagnostics on both roles.

Results:

- 20/20 runs passed; authentication-response-2 timeouts were zero;
- STA local echo was 200/200, with 10/10 unique replies in every run;
- AP and STA maxima were zero for ownership violations, duplicate memberships,
  wrong priority buckets, invalid links, detached priority mutation, and
  detached policy mutation;
- runner errors, RTOS allocation failures, and RF allocation failures were
  zero;
- maximum runner step was 97 ms;
- every strict contract violation list was empty.

Per-packet AP UART markers undercounted because long diagnostic lines can
interleave, but every STA sequence was received and each AP final network
counter reached 10 RX / 10 TX. The acceptance gate uses final counters and
sequence de-duplication rather than treating UART marker count as packet loss.

## Evidence Boundary

The result proves executable detection, fail-closed HIL integration, and
pure-WPA3 workload parity for `RTOS-STATE-004`. It does not establish that
`fac6dd4`, `7cbcd48`, or the detached-priority fix was the sole cause of older
run-04/run-10 failures; the diagnostic mutation counters were zero throughout
this matrix. Existing recovery defenses remain in place.

The bounded snapshot audit also does not replace continuous trace, 100-reset
ticket-age evidence, long soak, or future SMP ownership proofs. Those remain
separate triggered gates.
