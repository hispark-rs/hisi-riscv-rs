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

The original local verification covered:

- WPA2 incremental host profile: 86 tests passed;
- WPA3 incremental host profile: 91 tests passed;
- WPA2 incremental clippy passed with warnings denied;
- the `riscv32imfc-unknown-none-elf` WPA2 incremental library check passed with
  `-Zbuild-std=core,alloc`.

GitHub Actions run
[`30341395954`](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/30341395954)
checks both incremental security profiles, standalone packaging, the RV32
target graph and final links on Linux, macOS and Windows.

Two later changes bind the simulated key lease to the real interfaces on both
sides of the native supplicant boundary:

- `ws63-radio-sys` commit `8e7774729c2325052248e89a6002d0f45b8975cb`
  calls the actual `wpa_driver_ws63_ops.set_key` entry for installation and
  removal. It checks key activity/counts and proves that duplicate removal
  propagates the driver callback error instead of recreating state. CI run
  [`30352153340`](https://github.com/hispark-rs/ws63-radio-sys/actions/runs/30352153340)
  passed both native WPA2 and WPA3 supplicant port profiles.
- `hisi-rf-ws63 0.1.0-alpha.35`, commit
  `3ce931a91eb2422e634f9a66a9d389f8a284b956`, tests the production
  `install_key_via` and `remove_key_via` helpers with typed WAL requests. The
  tests verify the exact `IOCTL_NEW_KEY`, optional `IOCTL_SET_KEY` and
  `IOCTL_DEL_KEY` sequence, deletion payload, and rollback when selecting the
  default TX key fails. WPA2/WPA3 incremental host profiles now pass 88/93
  tests. CI run
  [`30352373508`](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/30352373508)
  and publish run
  [`30352636258`](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/30352636258)
  passed.
- `hisi-rf 0.1.0-alpha.45`, commit
  `07eb8b1e8fa1d96e1aad4520ac03e3380d42aae7`, carries that backend through the
  facade. CI run
  [`30353092794`](https://github.com/hispark-rs/hisi-rf/actions/runs/30353092794)
  passed the facade suite and crates.io-only WPA2/WPA3 external consumer builds
  on Linux, macOS and Windows; publish run
  [`30353500687`](https://github.com/hispark-rs/hisi-rf/actions/runs/30353500687)
  passed.
- `ws63-radio-sys` commit
  `aef315a6c8f99961ef5f1dae37c23fe77895f190` links the complete pinned hostap
  source closure into `native_supplicant_lifecycle.c`. The test initializes the
  production `hisi_wpa_context`, installs a pairwise CCMP key through the real
  `wpa_driver_ws63_ops.set_key` hook, calls the public production
  `hisi_wpa_disconnect`, and proves upstream `wpa_clear_keys` reaches the real
  remove-key hook exactly once for that pairwise key. A second disconnect is
  idempotent. CI run
  [`30355215033`](https://github.com/hispark-rs/ws63-radio-sys/actions/runs/30355215033)
  passed the native WPA2/WPA3 profiles, both RV32 source closures, package
  verification, and Linux/macOS/Windows consumer builds.

## Evidence Boundary

The host acceptance is compositional across exact production seams rather than
one synthetic mega-binary:

1. the incremental adversarial test proves cancellation calls the production
   `SupplicantPort::disconnect` once and conserves owner, queue, timer and
   generation state;
2. the production port directly calls `NativeSupplicant::disconnect`, which
   directly calls public `hisi_wpa_disconnect`;
3. the complete hostap lifecycle test proves `hisi_wpa_disconnect` invokes
   upstream `wpa_clear_keys` and the real remove-key hook;
4. the WS63 backend test proves that remove-key hook is encoded as
   `IOCTL_DEL_KEY`, while failed install selection rolls back.

No seam is substituted with an error builder or a model-only callback. This
closes the host-side key-conservation gate. It does not claim that a host
process can read back WS63 hardware key tables. Existing real-silicon
connect/disconnect HIL remains the target evidence for the driver path.

This gate does not prove pure-WPA3 stability, does not remove the vendor oracle,
and does not make the incremental backend the default. The SAE-only AP gate
remains external-blocked.
