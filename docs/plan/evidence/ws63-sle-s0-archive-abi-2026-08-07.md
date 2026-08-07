# WS63 SLE S0 archive/ABI closure evidence

## Scope

This evidence closes S0 only: hash-bound archive selection, target ABI,
external-capability ownership, conservative memory envelope, normalized
relocations, Cargo delivery, and CI drift gates. It does not claim SLE
controller/host initialization, announce/seek, connection, SSAP, coexistence,
or silicon behavior.

## Artifact identity

- `ws63-radio-sys` inventory commit: `e2fb7aa`
- `ws63-radio-sys` artifact commit: `0091705`
- SLE consumer CI commit: `4b7e4f2`
- profile revision: `ws63-sle-s0-archive-abi-v1`
- normalization revision: `ws63-sle-s0-normalized-v1`
- normalized `libbth_gle.a` SHA-256:
  `dd87f79e276daf05f3996e33157d808612c9571fd44ec40df30d448db909d627`

The profile binds `libbth_gle.a`, `libbt_host.a`, `libbt_app.a`,
`libbth_sdk.a`, and `libbg_common.a` to their source SHA-256 values. The
generated report records 267 archive members, 3,663 defined globals, 2,056
undefined globals, and 124 required external symbols.

## ABI and ownership gate

All five inputs are ELF32 little-endian RISC-V objects with `e_flags = 3`
(`ilp32f` + RVC). Every external symbol has an explicit owner. The six SLE-only
integration hooks are classified as bounded application/product hooks:

- GLP callback registration/unregistration;
- CHBA AT registration;
- low-latency mouse/dongle enablement.

`osal_printk` is owned by platform diagnostics. Unknown external symbols remain
fail-closed.

## Relocation and memory evidence

The source archives contain 8,900 declared vendor relocations:

- `R_RISCV_48_LLUI`: 2,201;
- `R_RISCV_BRANCHI`: 1,756, all same-section;
- `R_RISCV_LLUI_REP`: 4,943;
- cross-section `R_RISCV_BRANCHI`: 0.

`hisi-rf-link normalize` converts the selected archives to stock `rust-lld`
semantics. The normalized `libbth_gle.a` contains no vendor relocations and is
reproduced byte-for-byte by the release-artifact check.

The report also records a conservative sum of every member's allocatable
sections: 333,522 bytes text, 23,723 bytes read-only data, 2,686 bytes writable
data, and 4,272 bytes BSS, for a 364,203-byte archive envelope. This is an upper
bound before final-link reachability and is not a runtime admission result.

## Verification

- `cargo test --workspace --target aarch64-apple-darwin --locked`
- `cargo clippy -p hisi-rf-link -p ws63-radio-blob --all-targets --target aarch64-apple-darwin --locked -- -D warnings`
- `cargo check -p ws63-radio-sys --no-default-features --features sle -Zbuild-std=core,alloc --target riscv32imfc-unknown-none-elf`
- `uv run scripts/check-release-artifacts.py`
- `cargo package -p ws63-radio-blob --locked --allow-dirty --no-verify`

The package contains 30 files and remains below the crates.io compressed-size
limit at 8.4 MiB. CI rebuilds the profile/report and normalized payload and
builds the `sle` Cargo consumer contract on Linux, macOS, and Windows.

## Remaining gate

S1 must establish a rooted, reachable initialization and announce/seek closure,
then produce dual-board markers. S0 alone does not prove that the archive's
full memory envelope is linked or that any SLE protocol state machine runs.
