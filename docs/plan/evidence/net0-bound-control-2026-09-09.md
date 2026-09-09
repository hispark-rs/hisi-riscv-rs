# NET0 Bound/Closed Control HIL (2026-09-09)

Status: **3/3 preflight plus 20/20 nRST control samples passed** on one fixed
STA ELF. The new caller-owned L2 queue is now bound to its device and native
worker, but remains closed. This is not NET0 traffic, producer-quiescence,
reconnect, Embassy Net, HTTPS or release acceptance.

## Exact Inputs

- Backend source: `654e90c66d350c75a1b00114d02f68657b0f3eea`, still unversioned
  work after `0.1.0-alpha.101`.
- STA ELF SHA-256:
  `35fbdf1ceed30d1b82c105892ff953dd813e23717885e290516bdfdaaea6df5e`.
- AP ELF SHA-256:
  `d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
  The AP and its public non-production paired configuration are unchanged
  from the [previous control evidence](net0-control-2026-09-09.md).
- The standalone backend uses its committed lockfile, public dependencies and
  offline build, not parent workspace patches. The only configuration input
  is the committed paired fixture; no private credential file was read.
- Exact-source [CI 23/23](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34310132814)
  passed, including Linux/macOS/Windows contracts and final links, Miri,
  feature gates, public API review and packaging.

The [machine record](net0-bound-control-2026-09-09/summary.json) retains both
lockfiles, toolchain, capture tool, image plan, three downloaded CI storage
reports and per-round whitelisted marker excerpts with hashes. The original
UART hashes are preserved separately; excerpts are explicitly not raw captures.
Raw logs contain ambient scan data and remain local, as do the ELF/image files.
The final downloadable NET0 release bundle is still an open gate.

## Observations

| Check | Observed result | Boundary |
|---|---|---|
| Flash | STA app-only, 3 MHz, full verify, 91.05 seconds | No flashboot/NV rewrite or AP reflash. |
| Fixed-image reset preflight | 3/3 passed | Every round has bound/closed, connect, disconnect and final profile markers. |
| Same-image nRST matrix | 20/20 passed, no reflashing | AP remains running; this is not cold-boot or network-load coverage. |
| Firmware control latency | Connect 1,794–3,111 ms; disconnect 108–110 ms over all 23 samples | These are firmware markers, not a native-drain acknowledgement or scheduling bound. |
| Resource layout | Control 21,440 bytes; L2 object 12,560 bytes; payload 12,112 and metadata 448 bytes | CI derives and compares final target layout; no peak-usage claim. |
| Existing allocations | Shared arena 299,008; main stack 32,768; packet RAM 49,152 bytes | No RF stack/arena reduction to fit the new handles. |
| Boot warning | `Flash Init Fail! ret = 0x80001341` in all 23 captures | Retained as a known warning, not waived or described as fixed. |

Local standalone validation passed 173 NET0 host tests, 27 L2/pbuf Miri tests,
119 legacy incremental and 83 legacy blocking tests, host/RV32 Clippy with
warnings denied, formatting and the reviewed WPA2 API snapshot. Host tests
cover missing MAC/duplicate storage claims, opaque device capacity, the same
queue's smoltcp adapter, close-before-subscription and coalesced-close wakeups.

## Remaining Gate

`RFDBG_NET0_BOUND_CLOSED` checks the actual MAC, Down state and absent TX token
after composition. The worker receives the unique native link but does not
open it. Successful Wi-Fi control operations therefore do not prove data can
pass through these queues. Native authorization and a producer fence must
precede open; delayed-frame and traffic/reconnect HIL must follow.

The [native-fence audit](net0-native-fence-audit-2026-09-09.md) now distinguishes
the two FRW threads' synchronous control, RX message and TX/EAPOL queues.
WAL return, user-reference zero and a Rust callback counter each cover only
part of that chain. None has been promoted to a producer-quiescence proof.
The active milestone remains [NET0](../hisi-connectivity-stack.md#net0-net5-https).
