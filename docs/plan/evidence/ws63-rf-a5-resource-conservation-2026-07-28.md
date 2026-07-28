# WS63 A5 Incremental Resource-Conservation Evidence

## Scope

This evidence closes the host-side A5 acceptance gate for cancellation resource
conservation in the non-default incremental backend. It exercises the production
`IncrementalSupplicantBackend` through the production
`IncrementalBackendDriver`, rather than testing an isolated model of either
state machine.

The deterministic adversarial sequence is:

1. start a connect operation and acquire a simulated link-key lease;
2. retain one replacement command in the bounded pending slot;
3. request cancellation and require exactly one driver disconnect;
4. deliver a late `AUTHORIZED` event after cancellation;
5. deliver the final `DISCONNECTED` event;
6. start the retained replacement in the reused operation slot.

The assertions prove that:

- disconnect releases the key lease before cancellation becomes terminal;
- a late successful event cannot escape as controller success;
- cancellation is submitted exactly once;
- the active timer deadline disappears when the operation becomes terminal;
- the pending command remains owned and starts after the cancelled operation;
- slot reuse receives a new generation, so the old completion identity cannot
  identify the replacement.

Existing `hisi-rf-core` lifecycle and facade tests independently cover stale
reports, duplicate cancellation, queued cancellation, start/poll/cancel errors,
full pending-channel backpressure, dropped control futures and fair wake-source
selection. The bounded design has one active operation and one retained
replacement, so this production-path adversarial sequence plus the existing
transition tests was selected instead of adding a separate Kani or TLA+ model
that would duplicate the same small state machine.

## Verification

Commit `26757c2949b5e223dff9ac5623acbe94c97f5266` in `hisi-rf-ws63`
adds `cancellation_conserves_operation_queue_timer_and_key_resources`.

Local verification:

- WPA2 incremental host profile: 86 tests passed;
- WPA3 incremental host profile: 91 tests passed;
- WPA2 incremental clippy passed with warnings denied;
- the `riscv32imfc-unknown-none-elf` WPA2 incremental library check passed with
  `-Zbuild-std=core,alloc`.

GitHub Actions run
[`30341395954`](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/30341395954)
checks both incremental security profiles, standalone packaging, the RV32
target graph and final links on Linux, macOS and Windows.

## Evidence Boundary

The key lease is a deterministic `SupplicantPort` observation of the production
disconnect contract. This does not claim that WS63 hardware key tables can be
read back from host tests. Existing real-silicon connect/disconnect HIL remains
the target evidence for the driver path.

This gate does not prove pure-WPA3 stability, does not remove the vendor oracle,
and does not make the incremental backend the default. The SAE-only AP gate
remains external-blocked.
