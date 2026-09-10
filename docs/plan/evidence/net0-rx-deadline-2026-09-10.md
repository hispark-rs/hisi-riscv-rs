# NET0 RX Receipt Deadline (2026-09-10)

Status: **deadline enforcement implemented; normal 3-reset preflight passed,
UDP 30/30; exact-source CI 23/23 and three-platform download verification passed.**
NET0 remains active; this is not a native DMA fence or reconnect release.

## Defect And Fix

Backend `8a6908c193fc6756d5f68b9e919c09903189a5e2` fixes a gap after the
[descriptor rebuild experiment](net0-rx-rebuild-2026-09-10.md). Previously the
waiter inspected a successful completion before sampling elapsed time. If native
code prevented the waiter from running, a receipt returned after the deadline
could still be accepted. The native callback also depended on the waiter having
already noticed expiry before starting destructive work.

The transaction now owns one monotonic start sample and a 1,000 ms deadline.
Callback entry, the final pre-teardown check, pre-rebuild admission, native
completion and waiter result consumption all check that same budget. Exact
expiry, missing time, subtraction underflow and previously recorded failure
fail closed. Late native observations remain available, but zero native status
cannot replace a timeout with success. No second timer or cancellation worker
was added. Clock samples and native operations stay outside Rust critical
sections; metadata changes remain short.

The [source-bound contract](https://github.com/hispark-rs/hisi-rf-ws63/blob/8a6908c193fc6756d5f68b9e919c09903189a5e2/docs/net0-rx-stop.md)
retains the important limit: this does not interrupt an executing C/ROM call.
The monotonic source must advance independently of this waiter. It is a receipt
acceptance deadline, not proof of a hard bound on IRQ-masked native execution.

## Verification

The independent published-dependency build passed 240 host tests, host and
RV32 Clippy with warnings denied, format checks, and 19 RX transaction/rebuild
Miri tests. Five added cases cover overdue callback entry, late native return
without a waiter poll, completed receipts consumed after expiry, teardown
exhausting the rebuild budget, and invalid clock samples.

A controlled local mutation removed the deadline check from `result_at` only.
`completed_receipt_does_not_bypass_end_to_end_deadline` then failed with
`Ok(true)` instead of `Err(-4129)`; restoring the production check passed.
This is a host regression negative, not a physical late-callback injection.

[Exact-source CI](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34430064193)
passed all 23 jobs. The [download verifier](net0-rx-deadline-2026-09-10/verify-downloads.py)
then downloaded all three external-consumer ZIPs, checked their GitHub SHA-256
digests and 24 member hashes, and compared the actual native call/veneer reports
across Linux, macOS and Windows. The [receipt](net0-rx-deadline-2026-09-10/download-verification.json)
records artifact identities and expiry. These are downloaded report ZIPs, not
permanent firmware archives or acceptance of a new registry release.

The final ELF gate resolves 18 native calls and seven ROM veneers and rejects
all 25 call/address mutations. Its v3 report checks the actual 80-byte receipt
storage. Direct-RX mode rejects eight mutations, host-TX eleven and physical
storage eight. L2 packet storage and the existing RF/runtime arenas are unchanged.

## Silicon Preflight

STA ELF SHA-256:
`4229b1c78a55f449a88ad6a2d18052e912155287a47aefdbe3bc4c44a22c99ab`.
The fixed AP ELF remains
`d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
Only the committed public paired configuration was used. The AP was reset
before the STA in each round; no other AP credentials were read.

The normal image passed 3/3 resets with UDP 30/30 and no invalid/duplicate
payloads. Each terminal receipt matched device task `0x105`, rebuilt
normal/high/small queues to 4/4/8 and cleaned them to 0/0/0, with zero native
and cleanup status. MAC was already zero at the pre-stop sample. The 3 MHz
download retained full verification and took 97.977 seconds.

The [collector](net0-rx-deadline-2026-09-10/collect-evidence.py) checks the
independent build inputs against the exact committed tree, recomputes the
public marker results from original UART captures and binds both ELF hashes,
the flash image/plan and the download receipt in the
[summary](net0-rx-deadline-2026-09-10/summary.json).
The retained files are covered by [SHA256SUMS](net0-rx-deadline-2026-09-10/SHA256SUMS).

This validates ordinary integration of the new deadline checks. No physical
late-native fault was injected, no repeated live reconnection was exercised,
and three boot samples are not a statistical reliability claim. Earlier
one-shot association/recovery failures remain in their original evidence.
