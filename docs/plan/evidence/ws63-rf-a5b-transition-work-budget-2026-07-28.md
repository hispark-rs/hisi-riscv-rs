# WS63 A5B Transition Work-Budget Evidence

## Scope

This evidence covers the opt-in `hisi-rf-ws63` incremental connect fixture on
real WS63 silicon. It uses the WPA2-Personal profile against an AP advertising
WPA2/WPA3 transition mode. It does **not** establish pure-WPA3 support.

The tested backend commit is `d7ba835` (`test: bound incremental WPA transition
work`). The image was built with the pinned official Rust nightly, linked
through the normalized radio archives, downloaded at 3 MHz with full readback
verification, and then reused unchanged for every reset.

## Result

- 20/20 J-Link nRST boots reached `RFDBG_A5B_CONNECT_PROFILE_OK`.
- All 20 boots reported zero Authentication-response-2 timeouts.
- All 20 boots observed vendor status `8030`, normalized it to IEEE status 30,
  and recovered through the bounded native event-loop path.
- The per-step `WorkBudget` was 100 ms. No step exceeded the budget.
- The longest observed runner step was 38 ms.
- The longest observed initial association ioctl was 32 ms.
- Event and control queues reported no drop or backend error.

The raw captures were kept outside the repository during diagnosis. This
committed summary intentionally excludes credentials, BSS identifiers, and
frame contents; none of those values are part of the evidence contract.

## Interpretation

The earlier multi-second runner slices are not an inherent latency of
`IOCTL_ASSOCIATE`: the instrumented WAL boundary completed in at most 32 ms in
this matrix. The current transition profile therefore has evidence for a
100 ms fail-closed step budget on this board/AP combination.

This does not prove every hostap callback is incrementally preemptible, and it
does not close the pure-WPA3 gate. The 100 ms budget remains a migration
measurement, not a public latency guarantee. The legacy blocking backend stays
the default until the remaining A5B state-machine and parity gates close.
