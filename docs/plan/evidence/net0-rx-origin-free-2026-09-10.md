# NET0 Native Free-Attempt Census (2026-09-10)

Status: **observation implemented; coverage remains failed. The requested
three-round preflight stopped after its first failed round.** UDP was 10/10,
but terminal RX-stop timed out. No 20-reset matrix, reconnect acceptance or
NET1 advancement follows this result.

## Change And Oracle

Backend `f04b701bc0b859e1370c14d65c429152a87b2fcc` extends the existing
default-off descriptor-origin experiment with native free-attempt observation.
Read-only ROM and pool snapshots show why sixteen RX descriptors were not
sufficient evidence for a sixteen-entry live buffer bound: both fixed rigs
had 35 native pool controls. The pool is not necessarily the only allocator.

The ROM free path consults callbacks 250 and 249. With 250 null, callback 249
sees both RAM-backed and pool-backed buffers. Its return of 2 tells ROM to
continue pool freeing. The callback's native function is a **local** archive
symbol; a direct `--wrap=oal_mem_netbuf_free_from_ram` prototype failed linking
with an unresolved `__real_*` symbol. No fixed image address was substituted.

The implemented experiment instead wraps the public callback-registration
entry. It captures the original pointer and publishes the observer atomically,
then forwards native frees exactly once outside Rust critical sections.
Native ownership, status values, packet admission and the reconnect guard are
unchanged. Runtime validation rejects a missing original or a non-null callback
250 that could bypass observation. The child repository records the bounded
[ABI and ROM/pool oracle](../../../crates/chips/ws63/hisi-rf-ws63/docs/net0-native-free-observation.md).

This retires **observations before free attempts**, not proven successful frees.
The applicable conservation equation is now:

```text
bindings = matched + replaced + retired_on_free_attempt + occupied
matched = closed_origin + current_origin + stale_origin
```

## Local Checks

- Independent pinned-dependency build: 249 host tests, host/RV32 Clippy and fmt.
- Miri: nine origin tests, including undelivered free/reuse and missing or
  bypassing callback rejection. This does not execute vendor code.
- Final ELF: six call edges, original callback argument, ROM thunk forwarding,
  three descriptor patch destinations; 16 address/call/table mutations rejected.
- Physical storage gate: eight mutations rejected. Census metadata is 384
  bytes plus a four-byte original callback. Existing RF/runtime arenas and
  the 32 KiB main stack were not reduced; the linker still leaves 64 bytes
  between shared arenas and the main stack.

Removing the registration-link metadata can let LTO/GC remove the unused
wrapper and its `__real_*` reference. The initial negative-test expectation
of a linker error was falsified locally. The committed test instead requires
the final-ELF checker to reject missing registration, just as for the earlier
descriptor edge. A successful Cargo link alone is not observation coverage.

## Exact-Source CI And Downloads

[CI 34438207077](https://github.com/hispark-rs/hisi-rf-ws63/actions/runs/34438207077)
completed with **23/23 successful jobs** for the exact backend commit above.
Linux x86_64, macOS ARM64 and Windows x86_64 each built the packaged external
consumer with clean/offline and incremental/restored-build checks, without a
consumer build script or duplicate native wrap flags. Each rejected missing
descriptor-hook and free-registration metadata in its final ELF gate.

The [download verifier](net0-rx-origin-free-2026-09-10/verify-downloads.py) fetched
all three actual consumer ZIPs, checked their GitHub SHA-256 digests and all
27 member digests, and compared the resolved six-edge/three-patch contract.
The [download receipt](net0-rx-origin-free-2026-09-10/download-verification.json)
records artifact IDs, expiry, lock digest and each origin ELF digest. The
directory [checksum list](net0-rx-origin-free-2026-09-10/SHA256SUMS) binds the
retained reports and tools. These are report artifacts with finite Actions
retention, not downloadable HIL firmware or a registry release. CI success
does not change the failed silicon coverage and deadline result below.

## Fixed-Image HIL

STA ELF: `21aabd008af78cf3094f1498ab15f788ff7cc37b25dc8bae46935e3a46ed3ccf`.
Unchanged AP ELF: `d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
The build used only the public paired fixture, not the private credentials
file. Three MHz download with full verify and the flash-script reset took
98.110 seconds. The paired AP-then-STA reset policy was unchanged.

The first round connected and completed ten sequence-checked UDP echoes with
zero invalid or duplicate replies. At the payload census snapshot:

| Observation | Count |
| --- | ---: |
| Successful bindings | 218 |
| Matched delivery | 168 |
| Retired before native free attempt | 35 |
| Remaining occupied / peak | 15 / 16 |
| Capacity failures | 23 |
| Unmatched delivery | 21 |
| Native free callback calls | 355 |

The snapshot conserves its observations, and the free hook is exercised, but
coverage fails. Free observation reduces a known blind spot; it does **not**
make the 16-slot table sufficient or establish pointer lifetime identity.

Terminal stop/rebuild observed the expected worker, disabled MAC, empty
descriptor lists, and a successful 4/4/8 allocate/cleanup round-trip. However,
the encompassing 1,000 ms stop transaction returned `-0x1021` (`0xffffefdf`),
which remains a failure even when the native result is zero. No successful
disconnect/closed-session marker was emitted. The harness retained its
150-second capture and stopped on this first failure; rounds 2/3 and a 20-reset
matrix were not run. This is neither UDP loss nor proof of an AP cause.

A subsequent [read-only postmortem](net0-rx-origin-free-2026-09-10/terminal-origin-state.json)
found 235 bindings, 169 matched, 66 retired and **zero occupied** observations;
capacity/unmatched counters remained 23/21. The captured original callback
equals this ELF's native free function. Thus permanent retention is not a
sufficient explanation of this sample's peak/capacity failures. The decoder
uses actual DWARF member offsets and requires explicit addresses matching the
ELF symbols. An initial incorrect assumption that the tracker and saved
pointer were adjacent was rejected; that diagnostic read was discarded and
the two objects were read separately at their verified addresses. No firmware
state was written. These sequential reads are not an atomic snapshot or a
replacement for the failed UART terminal receipt.

The [buffer-owner postmortem](net0-rx-origin-free-2026-09-10/terminal-buffer-owner.json)
separately verifies the current ELF's RAM-budget symbols and reads the live
pool pointer before following its bounded header/subpool arrays. It reports
all 35 pool controls free and zero current RAM-fallback bytes. The nonempty
subpools have 9/10/16 blocks and individual historical usage peaks of 9/5/8;
those peaks need not have occurred together. A zero configured RAM limit uses
the SDK's default budget, not a disabled RAM path. The
[decoder](net0-rx-origin-free-2026-09-10/decode-buffer-owner.py) rejects another
ELF, inconsistent addresses, sizes, index ranges and subpool counts. It does
not turn these later sequential observations into a timely stop receipt.

The [verified summary](net0-rx-origin-free-2026-09-10/verified-summary.json)
preserves the original pre-commit capture label, source/build byte comparison,
both ELF hashes, flash image/plan hashes, raw-capture receipts, public markers
and failed stop result. The [collector](net0-rx-origin-free-2026-09-10/collect-evidence.py)
recomputes traffic and origin results; missing final phases cannot become
coverage success. Its synthetic test rejects seven counter/phase mutations.
Raw captures remain local; this is not a permanent downloadable firmware
release or a complete NET0 acceptance bundle.

## Next Decision

Do not enlarge the table and call the problem solved. Establish a
representation bounded by the actual native buffer owner, including RAM
fallback and delayed host delivery while hardware descriptors are replenished.
The saved ROM gives a useful bound on the hypothesis: `oal_netbuf_dscr`
(`0x132596`) calls `oal_dmac_netbuf_end` (`0x132544`) and subtracts `0x5c` on
its non-null path. The patched allocator obtains that descriptor from the
same netbuf before binding it. Thus independently reused descriptor addresses
are not an established cause; multiple generations of in-flight buffers can
outnumber the configured hardware list even when each descriptor belongs to
its buffer. The SDK RAM fallback also enforces a byte budget (default `0x6c00`,
configurable), not a fixed number of buffers. Do not infer a complete owner
count from the 35 pool controls or the 4/4/8 RX list alone. These are bounded
local assembly observations, not a complete clone/free census.
Preserve the free-attempt versus free-completion distinction.
The terminal-stop timeout is an independent remaining gate; do not extend or
remove its deadline to hide this result. AMSDU copy inheritance and reusable
native DMA/host-RX closure remain unproven. NET0 stays active; NET1 stays queued.
