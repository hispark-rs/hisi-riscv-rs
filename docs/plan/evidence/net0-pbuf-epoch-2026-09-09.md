# NET0 Native Pbuf Allocation Epoch (2026-09-09)

Status: **allocation-provenance regression fixed; complete native lifecycle
and NET0 acceptance remain open**. The new ELF passed a paired 3-reset
preflight (30/30 UDP replies). The subsequent requested 20-reset matrix stopped
on its first round when an association retry triggered the existing one-shot
guard. This is 3/3 followed by 0/1, not 20/20 or reconnect acceptance.

## Defect And Implementation

Backend `2ce83fdffacf8b9aad459d03a830c5c46f20d092` fixes a production-boundary
regression: allocate a real native pbuf in an open connection, close/reopen the
route, then deliver that pbuf through `driverif_input`. Before the fix, the
callback captured the new generation and queued the old frame (`pending=1`
instead of zero). This is distinct from a callback ticket already captured
before close, which the earlier tests covered.

The allocator now captures a non-reused close revision before allocation can
be preempted. An immutable private prefix follows that native buffer through
host queues. Delivery rejects old/closed allocation stamps, including across
route re-registration and revision exhaustion. Admission and payload copies
retain their existing short-critical-section and outside-copy separation.

The prefix costs 16 bytes per live native pbuf inside the existing RF heap.
It is not native headroom: RV32 pbuf remains 32 bytes with unchanged offsets,
80-byte headroom and four-byte tailroom. `pbuf_header` cannot expose the prefix;
the last `pbuf_free` reference releases the complete allocation. Native
`malloc_len` remains relative to pbuf and oversized lengths now return null
instead of truncating. The v3 resource descriptor reports the extra per-buffer
cost separately; the physical RF/RTOS arenas and existing stacks are unchanged.

The explicit SDK final-ELF oracle confirms `oal_pbuf_netbuf_alloc` at 0x268922
stores the pbuf pointer and `oal_netbuf_free` at 0x2688e4 releases it through
`pbuf_free`. The oracle SHA-256 is
`5aedd0fb8e916efc27325fc5190822e05a0cf84f0b4d602e12c6fa06b242bf2e`.
These addresses identify that oracle only; none became runtime constants.

## Verification Boundary

Local checks passed 193 integration host tests, 39 NET0/netif tests under Miri,
host NET0 Clippy including tests, RV32 NET0 Clippy, format and final RF link.
The supported legacy-blocking profile also passes Clippy. A bare
`wpa2-personal,smoltcp` library without either backend still reports unused
implementation items with `-D warnings`; it was not promoted to a supported
feature combination or silenced by this change.

Miri reports existing integer-to-pointer casts in the published `hisi-alloc`
dependency, so this is not a strict-provenance proof of that allocator. The
modified `pbuf_header` preserves pointer provenance. Tests cover allocation
preemption, replacement routes, post-admission close, exhaustion, native ABI,
headroom moves, reference release and callback conservation.

The target descriptor checker passes three unit tests and rejects eight real
ELF mutations, including hiding the native prefix cost. The control object is
21,440 bytes; L2 is 12,560 bytes within it. RF arena is 101,888 bytes, RTOS arena
197,120 bytes, main stack 32,768 bytes and packet RAM 49,152 bytes. The 16-byte
native prefix is charged to live RF-heap allocations, not added to static L2.

[Exact-source CI](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34328292710)
passed 23/23 jobs. The [download verification](net0-pbuf-epoch-2026-09-09/download-verification.json)
compares all 103 tracked files with the independent build copy and all nine
downloaded target reports across Linux, macOS and Windows. Each bootstrap,
closed-incremental and initial-payload variant agrees across hosts, including
its physical memory addresses. These are downloaded reports, not downloaded
CI ELF binaries; Actions retention is finite and no release was made.

## Silicon Attempts

STA ELF SHA-256:
`793bffe53c7688e6edb9f6014e78c67c58082f52a920ab1853f7b841a44874eb`.
Unchanged AP ELF SHA-256:
`d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
STA app-only flashing at 3 MHz with full verify completed in 98.32 seconds
(91.83 seconds reported by probe-rs). No flashboot/NV write occurred.

Every round reset AP, waited for its ready marker, then reset STA. Only the
committed public paired-board configuration was used; no private credentials
or persistent runner/service were used.

- Preflight: 3/3; each round ten unique, content-checked UDP replies from ten
  requests, zero invalid/duplicate replies, followed by disconnect/L2 closure.
- Follow-up: requested 20, executed one, passed zero. Control connected in
  about 13 seconds, but `initial-rejected` kept L2 closed; no payload was sent.
  The original 40-second capture and failure remain retained. The firmware
  was not reflashed or rerun to replace that result.

The [machine record](net0-pbuf-epoch-2026-09-09/summary.json) binds both matrices,
raw UART hashes/lengths, full-line whitelisted excerpts, lock/toolchain,
FlashPlan, resource report and exact CI. The
[checksums](net0-pbuf-epoch-2026-09-09/SHA256SUMS) cover those public files.
The collector rechecks complete markers and packet counts rather than trusting
substring matches. Original captures and ELF/image bytes remain local; this
does not close the publicly downloadable firmware delivery gate.

This demonstrates the new pbuf layout can carry initial-session traffic on
silicon; it does not attribute the pre-existing association retry to pbuf
provenance or prove the complete reconnect path. Hardware work **before**
allocation, native copying into a fresh pbuf, TX/EAPOL queues and bounded
teardown still require the separate [native-fence contract](net0-native-fence-audit-2026-09-09.md).
The [NET plan](../hisi-connectivity-stack.md#net0-net5-https) stays at NET0;
NET1 Embassy Net and later HTTPS stages remain queued. No crate release or
new support claim was made for this fix.
