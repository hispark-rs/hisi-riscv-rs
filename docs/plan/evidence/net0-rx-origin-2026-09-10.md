# NET0 Native RX Origin Census (2026-09-10)

Status: **prototype implemented; provenance coverage failed in all three
rounds; one-shot traffic preflight 2/3.** NET0 remains active. This experiment
does not authorize reconnect or advance NET1.

## Hypothesis And Boundary

The current host-pbuf prefix records an epoch only when the native receiver
allocates its host copy. Work already queued in DMAC before that allocation
can therefore outlive the close boundary. An earlier origin stamp might allow
late work to be identified without mistaking an empty software list for a DMA
fence. This experiment tests the proposed interception point first; it does not
yet propagate its observation into packet admission.

The pinned allocator patch calls `hh503_rx_set_ctrl_dscr(descriptor, netbuf)`
before publishing the descriptor to the native list. The declaration is in
the SDK `hal_dscr_rom.h`; the original SDK assembly calls it at `0x14ed34`.
The existing 37-entry ROM patch table replaces the allocator and both RX
descriptor-add functions. No new ROM patch or MMIO access was introduced.

Backend `e3005eb74fe216f695038e2b997eb50b638c8570` adds the default-off
`standard-l2-rx-origin-experiment`. A transitive linker wrapper captures the
close revision, forwards the native binding exactly once outside Rust critical
sections, and records successful opaque identities. Callback 261 looks up and
retires that observation before forwarding native delivery unchanged, including
management and EAPOL. No native pointer is dereferenced or retained as an owner.

Replacement, missing identities, closed/current/stale origins, capacity
failure and counter exhaustion are explicit. The table cannot prove native
free completion, and a pointer match alone is not a lifetime proof. It must not
be used as an admission capability in its present form.

## Resource And Static Checks

The first 64-slot representation exceeded available SRAM by 1,216 bytes; the
linker rejected it. No RF stack, packet storage or shared arena was reduced.
The tested representation uses compact columns and a 16-slot, 368-byte metadata
table. Sixteen comes from the already-observed 4/4/8 descriptor configuration,
not a proven bound on all retained buffer identities. Silicon falsified its
sufficiency below.

The independent published-dependency build passed 246 host tests, six Miri
origin tests and host/RV32 Clippy with warnings denied. Tests cover identity
reuse, duplicate/unknown delivery, failed binding, closed/stale epochs,
capacity exhaustion and counter saturation.

The final-ELF check resolves both binding/forwarding calls, checks the original
ROM veneer and verifies the three actual descriptor patch destinations at their
ROM execution PCs. Nine missing-call/address/table mutations are rejected.
The physical storage gate also rejects eight mutations. These checks establish
the selected static call path, not the absence of other ROM producers.

Exact-source CI and three-platform artifact verification are recorded in the
[download receipt](net0-rx-origin-2026-09-10/download-verification.json).
The external consumer checks plain offline Cargo and rejects removal of the
transitive descriptor-link metadata. Reports are finite-retention Actions
artifacts, not a new registry release or permanent firmware archive.

The first consumer negative incorrectly required an undefined-symbol linker
error. With wrap metadata removed, GC can discard the unused wrapper and its
ROM veneer, so linking alone still succeeds. Verification-only follow-up
`70efda369aa9b8914b6d05659783a8a24b3d9e63` instead requires the actual final-ELF
call checker to reject that missing edge. Runtime/build inputs are unchanged;
the receipt binds CI to this follow-up rather than accepting the earlier run.

## Silicon Result

STA ELF: `9f8918bf623781ace088000ac03810b065c9fae12bb6616c329762d5999076a4`.
Unchanged AP ELF: `d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091`.
Only the public paired fixture was used. Three MHz download with full verify
took 99.725 seconds. The AP was reset before the STA each round.

- Rounds 1 and 2 each completed 10/10 sequence-checked UDP echoes and the
  terminal stop/rebuild/cleanup receipt.
- Every round exhausted the 16-slot observation table and had unmatched native
  delivery identities. This is failed provenance coverage, even when traffic
  passed; missing identities must not be assigned the current epoch.
- Round 3 reached native connection success after two association ioctl calls,
  but the existing initial-session guard rejected opening. No UDP phase or
  disconnect receipt completed. The 150-second capture timeout is retained;
  this was not ten lost UDP replies, nor evidence of an AP transient cause.

The [collector](net0-rx-origin-2026-09-10/collect-evidence.py) compares all
build-affecting inputs with the committed tree, verifies original capture
hashes and recomputes traffic, cleanup/rebuild receipts and origin conservation
counters. Its [negative test](net0-rx-origin-2026-09-10/test-collector.py)
rejects five mutations to capture bytes, pass flags, received count and the
descriptor receipt, including an attempt to mark failed round 3 successful.
It preserves the
pre-commit capture label rather than rewriting it. Retained public markers,
per-round failures, ELF/image hashes and source inputs are in the
[summary](net0-rx-origin-2026-09-10/summary.json), covered by
[SHA256SUMS](net0-rx-origin-2026-09-10/SHA256SUMS).

## Next Decision

The [fixed-ELF copy-path audit](net0-rx-origin-2026-09-10/native-copy-paths.json)
checks three resolved call edges and rejects their three removed-call mutations.
In this ELF, both `frw_alloc_pbuf` (called by `frw_rx_netbuf`) and
`hmac_rx_parse_amsdu_etc` call `oal_pbuf_netbuf_alloc`. The latter is a second
allocation boundary after native-to-host copying: a future admission path must
inherit the source epoch there rather than tag the new allocation with the
current connection. This is static reachability, not evidence that these three
HIL rounds exercised AMSDU. The audit covers normalized direct call pairs, not
indirect calls or ROM-internal producers.

The SDK `oal_net_pkt_rom.h` also distinguishes pool-backed and RAM-backed
native buffers. `wlan_spec_rom.h` gives `WLAN_MEM_NETBUF_CNT1` different values
under OS configuration branches and mentions cloned AMSDU buffers. Neither a
header branch nor the descriptor count establishes this image's live native
buffer bound. Do not repurpose reserved native bytes, assume a header layout,
or treat a flash-only free wrapper as coverage of ROM-internal frees.

The interception point is active on silicon, but descriptor count does not
bound observations that survive native frees without host delivery. Audit the
native buffer recycling/free paths and establish an ownership-bounded origin
representation before propagating epochs into host copies. A larger table
alone is not proof. Keep native delivery, one-shot rejection and the existing
DMA/reconnect gate intact while resolving this gap.
