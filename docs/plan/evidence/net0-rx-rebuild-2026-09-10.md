# NET0 Native Descriptor Rebuild (2026-09-10)

Status: **closed-admission allocation round-trip implemented; 3/3 positive
preflight, expected negative rejection, and 1/1 restore passed. Exact-source
CI passed 23/23; all three downloaded consumer report archives were verified.**
NET0 remains active; this is neither a DMA fence nor a reconnect release.

## Exact Implementation

Backend `5e7b179feadf9600aebc356f40d49748dd1ed5cd` extends the existing terminal
RX-stop experiment, without a new feature or production profile. The same
generation-checked device worker performs stop, native descriptor initialization,
per-queue verification, and final cleanup. Application RX and host TX remain
sealed. The operation never enables MAC or grants a reopen capability.

The ROM initializer's inner allocation routine is void and logs partial
allocation without an aggregate error. Its outer handler may return zero with
undersized queues. The Rust check therefore compares normal/high/small counts
individually against unchanged configured counts; nonempty or equal totals are
insufficient. Any attempted initialization proceeds to checked cleanup, retaining
the first failure and separate cleanup status. It does not free while the MAC
enable getter is nonzero.

The read-only RAM prefix is independently aligned with SDK structure dimensions
and ROM's actual load offsets. Compile-time assertions fix the layout; six
volatile u16 fields are sampled, with no queue pointer traversal or MMIO guess.
The [source-bound contract](https://github.com/hispark-rs/hisi-rf-ws63/blob/5e7b179feadf9600aebc356f40d49748dd1ed5cd/docs/net0-rx-stop.md)
states the exact ABI and limitations.

## Local Validation

- Independent published-dependency build: 235 host tests, host and RV32 Clippy,
  and format checks passed.
- Seven production-decision Miri tests passed: partial/zero allocation,
  configuration drift, ordering, precondition failure, cleanup failure and
  native re-enable. These are mock native observations, not physical OOM HIL.
- Final ELF has 18 resolved stop/rebuild/cleanup calls and seven 12-byte ROM
  veneers. All 25 call/address mutations are rejected; receipt storage is
  physically 68 bytes, with no additional packet queue.
- Unchanged direct-RX ownership gate rejects eight mutations; host-TX rejects
  eleven; physical storage gate rejects eight. All reports bind the new ELF.

## Silicon Attempts

STA ELF SHA-256:
`f60cf55a4cadc9c3bce563ac5297db2f4989f13b89c6296802338e64f0ebadc1`.
The AP remains the earlier fixed
`d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
Only the committed public paired configuration was used.

The positive 3-reset completed 3/3 with UDP 30/30. Each receipt records
configured/actual normal/high/small **4/4/8**, final counts **0/0/0**, init and
cleanup status zero, MAC zero after both operations, and matching device task
identity `0x105`. The full-verify 3 MHz download took 98.155 seconds.

The equal-layout negative replaces only the resolved initialization call with
`addi a0,zero,0; nop`. Its purpose is to prove rejection of zero status with no
allocation, not to emulate a real partially failing allocator. Source, symbols,
layout and other bytes remain unchanged; the mutation manifest fixes the exact
site and both digests. Negative results and restoration are recorded separately.

| Attempt | Traffic | Descriptor Outcome | Download With Full Verify |
|---|---|---|---|
| Normal, three resets | 30/30 UDP | 3/3 exact 4/4/8 rebuild, then 0/0/0 | 98.155 s |
| Equal-layout no-op init, one reset | 10/10 UDP before terminal operation | Expected failure: status 0, actual 0/0/0, error -0x1026; cleanup 0/0/0 | 97.084 s |
| Restored normal ELF, one reset | 10/10 UDP | 1/1 exact rebuild and cleanup | 98.113 s |

The negative ELF is
`9d9528b04ecd789566ef62431282d792db39cd363ad7e1aeda7d22d29b3b58ce`.
Its ordinary HIL outcome remains **false**: neither successful Disconnect nor
the final profile marker is emitted. This is an expected negative, not an
additional successful connectivity round. The board was restored to the normal
ELF; no serial, flash or reset process remains active.

## Evidence Boundaries

The [source CI](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34427450569)
passed 23/23. Its Linux x86_64, macOS ARM64 and Windows x86_64 external consumers
completed clean/offline, incremental, missing-link-metadata negative and restored
builds. Three actual ZIP downloads matched GitHub's digests; their 24 members and
resolved-call semantics were checked, not just artifact names.

[Recomputed evidence](net0-rx-rebuild-2026-09-10/summary.json) binds source inputs,
lock/toolchain, original UART digests, complete public markers, image plans and
mutation bytes. The [download receipt](net0-rx-rebuild-2026-09-10/download-verification.json)
records exact job/source identity and archive/member hashes; the
[collector](net0-rx-rebuild-2026-09-10/collect-evidence.py) rejects inconsistent
stored outcomes. Actions artifacts have finite retention; this is not NET5
durable release-artifact acceptance or a new package release.

One reconstruction per boot does not prove repeated allocation is leak-free.
The tested MAC was already disabled before stop; active DMA cessation, queued
native work visibility, autonomous re-enable, and a bounded reusable lifecycle
remain open. A native C call cannot be cancelled merely because the outer wait
expires. Timeout is sticky and late completion cannot report success. No
statistical result here closes those gaps or graduates NET1.
