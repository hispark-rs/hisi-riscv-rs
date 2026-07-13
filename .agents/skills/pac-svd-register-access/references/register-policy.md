# Register Access Policy

## Table of Contents

- Source of truth hierarchy
- SVD/PAC modeling rules
- HAL register access rules
- BS2X SVD generation policy
- CI gate interpretation
- Validation commands

## Source of truth hierarchy

Use the lowest available evidence, in this order:

1. Measured silicon behavior or HIL evidence.
2. Vendor SDK porting/register code under `fbb_ws63`, `fbb_bs2x`, or other chip SDK.
3. Generated SVD/PAC checked against SDK headers.
4. HAL code.

Do not let HAL invent register facts that can be represented in SVD/PAC. If HAL
needs a field name, access type, base address, instance, interrupt, or register
window, add that to SVD or the SVD generator and regenerate PAC.

## SVD/PAC Modeling Rules

- Model a register block in SVD when code needs MMIO at a stable address.
- Model field names when SDK headers or helper functions give a bit's meaning.
- Model access accurately:
  - `write-only` for command/clear/FIFO control registers where reads are invalid
    or meaningless.
  - `read-only` for status/counter/sample registers.
  - `read-write` only when read and write are both meaningful.
- Do not use `derivedFrom` only because peripheral names match. It is allowed only
  after register layout compatibility is checked.
- Keep generated PAC output deterministic. Re-run `regen.sh`; do not edit
  `src/lib.rs` by hand.
- If an SDK header is contradictory, encode the conservative model and document the
  exception next to the generator/supplement code.
- If a field is only known from SDK helper functions, add it as an audited
  supplement with a comment naming that helper.

## HAL Register Access Rules

Allowed patterns:

```rust
r.reg().write(|w| w.field().set_bit().other().clear_bit());
r.status().read().ready().bit_is_set();
r.ctrl().modify(|_, w| w.enable().set_bit());
```

Allowed with care:

```rust
// Full-register write when the register is naturally unfielded or all fields are
// intentionally programmed together.
r.data().write(|w| unsafe { w.bits(byte as u16) });

// DMA address export. This is an address value, not a CPU MMIO read/write.
let data_addr = r.data() as *const _ as u32;
```

Forbidden in production HAL:

```rust
core::ptr::read_volatile(0x5200_9000 as *const u32);
core::ptr::write_volatile(0x4000_0000 as *mut u32, value);
r.write_only_reg().modify(|_, w| ...);
r.reg().modify(|r, w| unsafe { w.bits(r.bits() | mask) });
```

The last form is allowed only for dynamic bit-position registers where the active
bit is a runtime pin/channel number and svd2rust cannot expose one useful field
per legal bit. Add an allowlist entry in
`crates/hisi-hal/scripts/check-register-access.py` with a rationale.

## BS2X SVD Generation Policy

Do not convert `BS2X.svd` to a fully handwritten XML file by default. Prefer the
hybrid model:

- Reuse WS63 SVD blocks only for verified register-compatible IP.
- Generate BS2X variant blocks from SDK headers when a same-name peripheral has a
  different layout, such as I2C v151, RTC v150, and TRNG v1.
- Generate BS2X-only blocks from SDK headers for GADC, KEYSCAN, PDM, QDEC, USB,
  and future chip-specific IP.
- Keep manual knowledge in generator supplements or data overrides, not scattered
  by hand inside the final generated XML.

If the supplement layer becomes hard to review, split it into explicit override
data files or `tools/manual_*.py` helpers. Do not jump straight to a fully manual
SVD unless generator evidence is no longer recoverable.

## CI Gate Interpretation

`crates/hisi-hal/scripts/check-register-access.py` currently catches:

- raw volatile access in HAL production code,
- numeric MMIO pointer casts,
- read/modify on known write-only registers,
- whole-register dynamic RMW without allowlist.

When it fails:

1. Prefer adding/fixing SVD fields/access types.
2. Regenerate PAC.
3. Change HAL to use field accessors or full writes.
4. Add an allowlist entry only for unavoidable dynamic masks.
5. Keep the gate wired in both HAL CI and parent CI.

## Validation Commands

From the parent repo:

```bash
bash .agents/skills/pac-svd-register-access/scripts/run-register-audit.sh
```

For SVD/PAC changes:

```bash
cd crates/chips/ws63/ws63-pac/ws63-svd && uv run validate.py && bash regen.sh
cd crates/chips/bs2x/bs2x-pac/bs2x-svd && uv run validate.py && bash regen.sh
```

For HAL register behavior:

```bash
cargo check -Zbuild-std=core,alloc -p hisi-hal --no-default-features --features chip-ws63,rt,unstable --target riscv32imfc-unknown-none-elf
cargo clippy -Zbuild-std=core,alloc -p hisi-hal --no-default-features --features chip-ws63,rt,unstable --target riscv32imfc-unknown-none-elf -- -D warnings
cargo check -Zbuild-std=core,alloc -p hisi-hal --no-default-features --features chip-bs21,rt,unstable --target riscv32imfc-unknown-none-elf
cargo clippy -Zbuild-std=core,alloc -p hisi-hal --no-default-features --features chip-bs21,rt,unstable --target riscv32imfc-unknown-none-elf -- -D warnings
```

For public API or stable behavior changes, also use `stable-unstable`,
`typed-config`, and `embedded-test-hil` as appropriate.
