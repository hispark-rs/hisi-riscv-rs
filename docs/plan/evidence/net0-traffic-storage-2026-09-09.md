# NET0 Traffic-fixture Physical Storage (2026-09-09)

Status: **physical storage checks passed; NET0 lifecycle/HIL gate remains open**.
The 3-round preflight stopped on its second round after the one-association
experiment rejected a retry. This is not 3/3, reconnect acceptance or a release.

## Implementation And Checks

Backend `97840727c2da80812ada30459aaaaad08c5dff37` shares one fixture storage
declaration between bootstrap and incremental traffic. Its target-sized v2
descriptor is checked against actual control, separate RF/runtime arena,
packet-RAM and stack objects. The checker rejects mismatched child budgets,
nonphysical/aliased symbols, section escape, overlap and stack-size drift.
The existing non-NET0 declarations and radio algorithms are unchanged.

[Exact-source CI](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34326046377)
passed 23/23 jobs. Linux, macOS and Windows each final-link and inspect the
bootstrap, closed-route incremental and initial-payload ELF, including seven
real ELF tamper cases per fixture. Local checks passed 189 host tests, format,
NET0 and non-NET0 fixture Clippy, and RV32 final links. Three checker unit tests
also cover descriptor fields, child-budget swaps and physical-range ownership.

The [download verification](net0-traffic-storage-2026-09-09/download-verification.json)
binds exact CI source, all nine downloaded report hashes/metadata, and all 103
tracked files in the independent local build copy. Physical reports agree
across host OSes for each variant; dimensions also match the local traffic ELF.
These downloads are **reports, not CI ELF binaries**, and their Actions
artifacts have finite retention. No registry release was made.

| Physical allocation | Bytes |
| --- | ---: |
| Control storage | 21,440 |
| L2 within control | 12,560 (12,112 payload + 448 metadata) |
| RF arena | 101,888 |
| RTOS arena | 197,120 |
| Shared arena total | 299,008 |
| Main stack | 32,768 |
| Wi-Fi packet RAM | 49,152 |

These are target allocations, not free-RAM/peak-use estimates. Existing arena
and task-stack capacities were not reduced. The shared declaration changes
linked object ordering/addresses; earlier ELF HIL results do not transfer.

## Physical Preflight

The fixed STA ELF has SHA-256
`2a1f7969e5db7df93608e0316ae32eb61be40e6431a6eafef187ca2db34a272b`.
The unchanged AP ELF has SHA-256
`d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
STA app-only download at 3 MHz completed with full verify in 97.69 seconds
(including the flash wrapper/reset). No flashboot/NV write occurred.

Only the committed public paired-board configuration was used. Each round
reset AP, waited for its ready marker, then reset STA. Of three requested
rounds, two ran before the first failure:

- Round 1: connected, ten unique/content-checked replies from ten requests,
  zero invalid/duplicate replies, then disconnect and L2 closure.
- Round 2: control connected after two association ioctls. The one-association
  guard returned `initial-rejected`; no L2 payload was admitted. Capture stopped
  after its 40-second observation limit. This is not a resource admission error,
  nor evidence that safe retry/reconnect has been implemented.

The [machine record](net0-traffic-storage-2026-09-09/summary.json) retains both
rounds, original UART hashes/lengths, reset policy, image plan and whitelisted
markers. [Checksums](net0-traffic-storage-2026-09-09/SHA256SUMS) cover the public
files. The prior collector rechecks the raw captures rather than promoting
substring matches or a rerun to a pass. Ambient scan output is not published;
raw UART and ELF/image bytes remain local, not a downloadable firmware bundle.

The [previous failure matrix](net0-initial-session-2026-09-09.md) is unchanged.
Next remains actual native producer drainage and retry/reconnect ownership,
followed by payload/reconnection HIL. This resource improvement does not remove
the one-shot guard or advance the [execution plan](../hisi-connectivity-stack.md#net0-net5-https)
to NET1.
