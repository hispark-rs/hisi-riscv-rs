# WS53 control-core cooperation prototype

This experimental, unpublished crate contains vendor-firmware message framing,
mailbox layout, SPSC storage and IPC18 random-buffer transaction logic. It does
not own or initialize the hardware doorbell, negotiate ownership, map shared
memory, maintain caches or prove peer quiescence. A platform adapter must map
the protocol channel to a HAL doorbell and establish those contracts explicitly.

The entropy callback accepts a platform-controlled source without coupling this
protocol crate to a specific HAL or random-number driver. All destination updates
remain transactional on an entropy failure. The existing HAL implementation and
its consumers remain in place while migration is reviewed; this prototype does
not imply new on-silicon validation.

Run independent host tests with `cargo test --locked --target
x86_64-unknown-linux-gnu --features defmt`; the RISC-V check uses the project's
pinned nightly and `-Zbuild-std=core,alloc`.
